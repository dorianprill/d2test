// Dorian Prill <drn@tuta.io>
//
// Entry point for d2re (Diablo II Reverse[d] Engine).
// Sniffs all network packets sent by:
//  1) Diablo II Game Server (D2GS)
//  2) Battle.net Chat Server (BNCS)
//  3) Realm Server for logon (MCP)
// Then hands them over to the appropriate handling procedures.
// WIP currently, ultimate goal would be enabling clientless botting


mod game;
mod client;
mod engine;

extern crate pnet;
use pnet::datalink::{self, NetworkInterface};
use pnet::packet::ethernet::{EtherTypes, EthernetPacket, MutableEthernetPacket};
use pnet::packet::ip::{IpNextHeaderProtocol, IpNextHeaderProtocols};
use pnet::packet::ipv4::Ipv4Packet;
use pnet::packet::ipv6::Ipv6Packet;
use pnet::packet::tcp::TcpPacket;
use pnet::packet::udp::UdpPacket;
use pnet::packet::Packet;
use pnet::util::MacAddr;

//use std::env;
//use std::io::{self, Write};
use std::net::IpAddr;
use std::process;
use std::str;


const PORTS: [u16; 2] = [6112, 4000];

fn handle_udp_packet(source: IpAddr, destination: IpAddr, packet: &[u8]) {
    let udp = UdpPacket::new(packet);

    if let Some(udp) = udp {
        // filter packet by ports used by d2, will continue for both sent & received
        //if !PORTS.contains(&udp.get_destination()) && !PORTS.contains(&udp.get_source()) {
        //    return
        //}
        match udp.get_destination() {
            // game packet
            4000 => engine::handlers::game_packet::game_packet_handler(&packet),
            // bncs/realm packet -> not implemented yet
            6112 => (),
            _ => return,
        }
        // println!(
        //         "UDP {}:{} > {}:{}  len={:03}  {:x?}  {:?}",
        //         source,
        //         udp.get_source(),
        //         destination,
        //         udp.get_destination(),
        //         udp.payload().len(),
        //         udp.payload(),
        //         String::from_utf8_lossy(udp.payload()).into_owned()
        // );
    } else {
        println!("Malformed UDP Packet");
    }
}


fn handle_tcp_packet(source: IpAddr, destination: IpAddr, packet: &[u8]) {
    let tcp = TcpPacket::new(packet);
    if let Some(tcp) = tcp {
        // filter packet by ports used by d2, will continue for both sent & received
        //if !PORTS.contains(&tcp.get_destination()) {//&& !PORTS.contains(&tcp.get_source()) {
        //    return
        //}
        match tcp.get_destination() {
            // game packet
            4000 => engine::handlers::game_packet::game_packet_handler(&tcp.payload()),
            // bncs/realm packet -> not implemented yet
            6112 => (),
            _ => return,
        }
        // println!(
        //     "TCP {}:{} > {}:{}  len={:03}  {:x?}  {:?}",
        //     source,
        //     tcp.get_source(),
        //     destination,
        //     tcp.get_destination(),
        //     tcp.payload().len(),
        //     tcp.payload(),
        //     String::from_utf8_lossy(tcp.payload()).into_owned()
        // );
    } else {
        println!("Malformed TCP Packet");
    }
}


fn handle_transport_protocol(
    interface_name: &str,
    source:         IpAddr,
    destination:    IpAddr,
    protocol:       IpNextHeaderProtocol,
    packet:         &[u8],
) {
    match protocol {
        IpNextHeaderProtocols::Udp => {
           handle_udp_packet(source, destination, packet)
        }
        IpNextHeaderProtocols::Tcp => {
            handle_tcp_packet(source, destination, packet)
        }
        _ => println!(
            "[{}]: Unhandled {} packet: {} > {}; protocol: {:?} length: {}",
            interface_name,
            match source {
                IpAddr::V4(..) => "IPv4",
                _ => "IPv6",
            },
            source,
            destination,
            protocol,
            packet.len()
        ),
    }
}

fn handle_ipv4_packet(interface_name: &str, ethernet: &EthernetPacket) {
    let header = Ipv4Packet::new(ethernet.payload());
    if let Some(header) = header {
        handle_transport_protocol(
            interface_name,
            IpAddr::V4(header.get_source()),
            IpAddr::V4(header.get_destination()),
            header.get_next_level_protocol(),
            header.payload(),
        );
    } else {
        println!("[{}]: Malformed IPv4 Packet", interface_name);
    }
}

fn handle_ipv6_packet(interface_name: &str, ethernet: &EthernetPacket) {
    let header = Ipv6Packet::new(ethernet.payload());
    if let Some(header) = header {
        handle_transport_protocol(
            interface_name,
            IpAddr::V6(header.get_source()),
            IpAddr::V6(header.get_destination()),
            header.get_next_header(),
            header.payload(),
        );
    } else {
        println!("[{}]: Malformed IPv6 Packet", interface_name);
    }
}


fn handle_ethernet_frame(interface: &NetworkInterface, ethernet: &EthernetPacket) {
    let interface_name = &interface.name[..];
    match ethernet.get_ethertype() {
        EtherTypes::Ipv4 => handle_ipv4_packet(interface_name, ethernet),
        EtherTypes::Ipv6 => handle_ipv6_packet(interface_name, ethernet),
        _ => ()
            // TODO make debug only print
            // println!(
            // "[{}]: Unknown packet: {} > {}; ethertype: {:?} length: {}",
            // interface_name,
            // ethernet.get_source(),
            // ethernet.get_destination(),
            // ethernet.get_ethertype(),
            // ethernet.packet().len()
        //),
    }
}


fn main() {
    use pnet::datalink::Channel::Ethernet;

    // Find the first network interface connected to the internet
    let interfaces = datalink::interfaces();
    let some_if: Option<NetworkInterface> = Some(interfaces
        .into_iter()
        .filter(|ref ifx| ifx.is_up() && !ifx.is_loopback() && ifx.ips.len() > 0)
        .next()
        .unwrap());

    let interface: NetworkInterface;

    if some_if != None {
    	interface = some_if.unwrap();
	    println!("Identified network interface {}", interface);
    } else {
	    println!("No active network adapter found, aborting...");
	    process::exit(1);
    }


    // Create a channel to receive on
    let (_, mut rx) = match datalink::channel(&interface, Default::default()) {
        Ok(Ethernet(tx, rx)) => (tx, rx),
        Ok(_) => panic!("unhandled channel type: {}"),
        Err(e) => panic!("unable to create channel: {}", e),
    };

    loop {
        let mut buf: [u8; 1600] = [0u8; 1600];
        let mut fake_ethernet_frame = MutableEthernetPacket::new(&mut buf[..]).unwrap();
        match rx.next() {
            Ok(packet) => {
                if cfg!(target_os = "macos")  && !interface.is_broadcast()
                     && interface.is_point_to_point() {
                    // Maybe is TUN interface
                    let version = Ipv4Packet::new(&packet).unwrap().get_version();

                    fake_ethernet_frame.set_destination(MacAddr(0, 0, 0, 0, 0, 0));
                    fake_ethernet_frame.set_source(MacAddr(0, 0, 0, 0, 0, 0));
                    if version == 4 {
                        fake_ethernet_frame.set_ethertype(EtherTypes::Ipv4);
                        continue;
                    } else if version == 6 {
                        fake_ethernet_frame.set_ethertype(EtherTypes::Ipv6);
                        continue;
                    }
                    fake_ethernet_frame.set_payload(&packet);
                    handle_ethernet_frame(&interface, &fake_ethernet_frame.to_immutable());
                }
                handle_ethernet_frame(&interface, &EthernetPacket::new(packet).unwrap());
            }
            Err(e) => panic!("unable to receive packet: {}", e),
        }
    }
}
