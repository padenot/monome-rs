#[macro_use]
extern crate futures;
extern crate tokio;
extern crate rosc;

use std::io;

use tokio::prelude::*;
use tokio::net::UdpSocket;

use rosc::decoder::decode;
use rosc::encoder::encode;
use rosc::{OscPacket, OscMessage, OscType};

pub const PREFIX: &str = "/prefix";

struct Enumerator {
    server_port: u16,
    buf: Vec<u8>,
    device_port: Option<u16>,
    device_type: Option<String>,
    device_name: Option<String>,
    received_server: Option<(usize, std::net::SocketAddr)>,
    enumerated: bool,
    host_port_set: bool
}

impl Enumerator {
    pub fn new() -> Result<Enumerator, String> {

                let enumerator = Enumerator {
                    received_server: None,
                    device_port: None,
                    device_name: None,
                    device_type: None,
                    server_port: 10000, // FIXME stop hardcode
                    buf: vec![0; 1024],
                    enumerated: false,
                    host_port_set: false
                };
                return Ok(enumerator);
    }

    pub fn setup(&mut self) {
        let addr = "127.0.0.1";
        let packet = message("/serialosc/list",
                             vec![OscType::String(addr.to_string()),
                             OscType::Int(i32::from(self.server_port))]);

        let bytes: Vec<u8> = encode(&packet).unwrap();

        // serialosc address, pretty safe to hardcode
        let addr = "127.0.0.1:12002".parse().unwrap();


        let server_addr = "0.0.0.0:10000".parse().unwrap();
        let bind_result = UdpSocket::bind(&server_addr);

        let socket = bind_result.unwrap();

        let processing = socket.send_dgram(bytes, &addr)
            .and_then(|(socket, _)| {
                let rv = socket.recv_dgram(vec![0u8; 1024])
                      .map(|(socket, data, len, _)| {
                        println!("{:?}", data);
                        let packet = decode(&data).unwrap();
                        println!("OscConnection: <- {:?}", packet);

                        match packet {
                            OscPacket::Message(message) => {
                                if message.addr.starts_with("/serialosc") {
                                    if message.addr == "/serialosc/device" {
                                        if let Some(args) = message.args {
                                            if let OscType::String(ref device_name) = args[0] {
                                                self.device_name = Some(device_name.to_string());
                                                println!("device_name: {}", device_name);
                                            }
                                            if let OscType::String(ref device_type) = args[1] {
                                                self.device_type = Some(device_type.to_string());
                                                println!("device_type: {}", device_type.to_string());
                                            }
                                            if let OscType::Int(device_port) = args[2] {
                                                self.device_port = Some(device_port as u16);
                                                println!("device_port: {}", device_port);
                                            }
                                        }
                                    } else {
                                        panic!("unexp prefix.");
                                    }
                                } else {
                                    panic!("unexp prefix.");
                                }
                            }
                            OscPacket::Bundle(_bundle) => {
                                panic!("wtf.");
                            }
                        }

                        let device_address = format!("127.0.0.1:{}", self.device_port.unwrap());
                        let addr = device_address.parse().unwrap();

                        socket.connect(&addr);
                        let packet = message("/sys/port",
                                             vec![OscType::Int(i32::from(self.server_port))]);

                        let bytes: Vec<u8> = encode(&packet).unwrap();

                        println!("sent: {:?}", packet);
                        Async::Ready((socket, self.device_port.unwrap()))
                    }).and_then(move |fut| {
                        println!("lala {:?}", fut);
                        if let Async::Ready((socket, port)) = fut {
                            let device_address = format!("127.0.0.1:{}", port);
                            let addr = device_address.parse().unwrap();

                            socket.connect(&addr);
                            let packet = message("/sys/host",
                                                 vec![OscType::String("127.0.0.1".to_string())]);

                            let bytes: Vec<u8> = encode(&packet).unwrap();

                            println!("sent: {:?}", packet);
                            Ok(Async::Ready((socket, port)))
                        } else {
                            panic!("??");
                        }
                    }).and_then(move |fut| {
                        println!("lala {:?}", fut);
                        if let Async::Ready((socket, port)) = fut {
                            let device_address = format!("127.0.0.1:{}", port);
                            let addr = device_address.parse().unwrap();

                            socket.connect(&addr);
                            let packet = message("/sys/host",
                                                 vec![OscType::String("127.0.0.1".to_string())]);

                            let bytes: Vec<u8> = encode(&packet).unwrap();

                            println!("sent: {:?}", packet);
                        } else {
                            panic!("??");
                        }
                        Ok(Async::Ready(()))
                    }).wait();
                    Ok(Async::Ready(()))
            }).wait();

            match processing {
                Ok(p) => { println!("{:?}", p);  }
                Err(e) => eprintln!("Encountered an error: {}", e),
            }
    }
}

fn message(addr: &str, args: Vec<OscType>) -> OscPacket {
    let message = OscMessage {
        addr: addr.to_owned(),
        args: Some(args),
    };
    OscPacket::Message(message)
}

impl Future for Enumerator {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<(), io::Error> {
        // Then, continously read the incoming packets
     //   loop {
     //       self.received_server = Some(try_ready!(self.socket.poll_recv_from(&mut self.buf)));
     //       let packet = decode(&self.buf).unwrap();
     //       println!("OscConnection: <- {:?}", packet);

     //       match packet {
     //           OscPacket::Message(message) => {
     //               if message.addr.starts_with("/serialosc") {
     //                   if message.addr == "/serialosc/device" {
     //                       if let Some(args) = message.args {
     //                           if let OscType::String(ref device_name) = args[0] {
     //                               self.device_name = Some(device_name.to_string());
     //                               println!("device_name: {}", device_name);
     //                           }
     //                           if let OscType::String(ref device_type) = args[1] {
     //                               self.device_type = Some(device_type.to_string());
     //                               println!("device_type: {}", device_type.to_string());
     //                           }
     //                           if let OscType::Int(device_port) = args[2] {
     //                               self.device_port = Some(device_port as u16);
     //                               println!("device_port: {}", device_port);
     //                           }
     //                       }
     //                   }
     //               } else if message.addr.starts_with("/sys") {
     //                   println!("/sys message");
     //               } else if message.addr.starts_with(PREFIX) {
     //                   println!("{}", PREFIX);
     //               }
     //           }
     //           OscPacket::Bundle(_bundle) => {
     //               panic!("wtf.");
     //           }
     //       }
     //   }
     Ok(Async::Ready(()))
    }
}

fn main() {
    let mut enumerator = Enumerator::new().unwrap();
    enumerator.setup();
    // tokio::run(enumerator.map_err(|e| println!("server error = {:?}", e)));
}
