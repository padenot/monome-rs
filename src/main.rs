#[macro_use]
extern crate futures;
extern crate tokio;
extern crate rosc;
#[macro_use]
extern crate log;
extern crate env_logger;

use std::io;
use std::net::SocketAddr;

use tokio::prelude::*;
use tokio::net::UdpSocket;

use rosc::decoder::decode;
use rosc::encoder::encode;
use rosc::{OscPacket, OscMessage, OscType};

pub const PREFIX: &str = "/prefix";

struct Enumerator {
    server_port: u16,
    socket: Option<UdpSocket>,
    device_port: Option<i32>,
    device_type: Option<String>,
    device_name: Option<String>,
    device_host: Option<String>,
    device_id: Option<String>,
    device_prefix: Option<String>,
    device_rotation: Option<i32>,
    device_size: Option<(i32, i32)>,
    received_server: Option<(usize, std::net::SocketAddr)>,
    enumerated: bool,
    host_port_set: bool
}

impl Enumerator {
    pub fn new() -> Result<Enumerator, String> {

                let enumerator = Enumerator {
                    received_server: None,
                    device_port: None,
                    socket: None,
                    device_name: None,
                    device_type: None,
                    device_host: None,
                    device_id: None,
                    device_prefix: None,
                    device_rotation: None,
                    device_size: None,
                    server_port: 10001, // FIXME stop hardcode
                    enumerated: false,
                    host_port_set: false
                };
                return Ok(enumerator);
    }

    pub fn setup(&mut self) -> Result<UdpSocket, &'static str> {
        let addr = "127.0.0.1";
        let packet = message("/serialosc/list",
                             vec![OscType::String(addr.to_string()),
                             OscType::Int(i32::from(self.server_port))]);

        let bytes: Vec<u8> = encode(&packet).unwrap();

        // serialosc address, pretty safe to hardcode
        let addr = "127.0.0.1:12002".parse().unwrap();


        let server_addr = "127.0.0.1:10001".parse().unwrap();
        let bind_result = UdpSocket::bind(&server_addr);

        let socket = bind_result.unwrap();

        let rv = socket.send_dgram(bytes, &addr).and_then(|(socket, _)| {
            socket.recv_dgram(vec![0u8; 1024]).map(|(socket, data, len, _)| {
                let packet = decode(&data).unwrap();

                match packet {
                    OscPacket::Message(message) => {
                        if message.addr.starts_with("/serialosc") {
                            if message.addr == "/serialosc/device" {
                                if let Some(args) = message.args {
                                    if let OscType::String(ref device_name) = args[0] {
                                        self.device_name = Some(device_name.to_string());
                                        info!("device_name: {}", device_name);
                                    }
                                    if let OscType::String(ref device_type) = args[1] {
                                        self.device_type = Some(device_type.to_string());
                                        info!("device_type: {}", device_type.to_string());
                                    }
                                    if let OscType::Int(device_port) = args[2] {
                                        self.device_port = Some(device_port);
                                        info!("device_port: {}", device_port);
                                    }
                                }
                            } else {
                                error!("Unexp prefix during setup: {}", message.addr);
                            }
                        } else {
                            error!("Unexp prefix during setup: {}", message.addr);
                        }
                    }
                    OscPacket::Bundle(bundle) => {
                        error!("Unexpected bundle received during setup {:?}", bundle);
                    }
                }

                let device_address = format!("127.0.0.1:{}", self.device_port.unwrap());
                let add = device_address.parse();
                let addr: SocketAddr = add.unwrap();

                let packet = message("/sys/port",
                                     vec![OscType::Int(i32::from(10001))]);

                let bytes: Vec<u8> = encode(&packet).unwrap();

                socket.send_dgram(bytes, &addr).and_then(|(socket, buf)| {
                    let local_addr = socket.local_addr().unwrap().ip();
                    let packet = message("/sys/host",
                                         vec![OscType::String(local_addr.to_string())]);

                    let bytes: Vec<u8> = encode(&packet).unwrap();

                    socket.send_dgram(bytes, &addr).and_then(|(socket, buf)| {
                        let packet = message("/sys/prefix", vec![OscType::String(PREFIX.to_string())]);

                        let bytes: Vec<u8> = encode(&packet).unwrap();
                        socket.send_dgram(bytes, &addr).and_then(|(socket, buf)| {
                            let packet = message("/sys/info", vec![]);

                            let bytes: Vec<u8> = encode(&packet).unwrap();
                            socket.send_dgram(bytes, &addr).and_then(|(socket, buf)| {
                                // led flash
                                let packet = message("/prefix/grid/led/all", vec![OscType::Int(1)]);
                                let bytes: Vec<u8> = encode(&packet).unwrap();
                                socket.send_dgram(bytes, &addr).and_then(|(socket, buf)| {
                                    let packet = message("/prefix/grid/led/all", vec![OscType::Int(0)]);
                                    let bytes: Vec<u8> = encode(&packet).unwrap();
                                    socket.send_dgram(bytes, &addr)
                                })
                            })
                        })
                    })
                }).wait()
            }).wait()
        }).wait();

        match rv {
            Ok(Ok((socket, _))) => {
                Ok(socket)
            }
            Ok(Err(_)) => {
                Err("setup failed")
            }
            Err(e) => {
                error!("Setup failed: {}", e);
                Err("setup failed")
            }
        }
    }

    fn set_socket(&mut self, socket: UdpSocket) {
      self.socket = Some(socket);
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
        let mut s = self.socket.take().unwrap();
        loop {
            let mut buf = vec![0; 1000];
            match s.poll_recv(&mut buf) {
                Ok(fut) => {
                    match fut {
                        Async::Ready(ready) => {
                            println!("ready: {:?}", ready);
                        }
                        Async::NotReady => {
                            futures::task::park();
                            self.set_socket(s);
                            return Ok(Async::NotReady);
                        }
                    }
                }
                Err(e) => {
                    return Err(e);
                }
            }
            let packet = decode(&buf).unwrap();
            debug!("⇐  {:?}", packet);

            match packet {
                OscPacket::Message(message) => {
                    if message.addr.starts_with("/serialosc") {
                        if message.addr == "/serialosc/device" {
                            if let Some(args) = message.args {
                                if let OscType::String(ref device_name) = args[0] {
                                    self.device_name = Some(device_name.to_string());
                                }
                                if let OscType::String(ref device_type) = args[1] {
                                    self.device_type = Some(device_type.to_string());
                                }
                                if let OscType::Int(device_port) = args[2] {
                                    self.device_port = Some(device_port);
                                }
                            }
                        } else if message.addr == "/serialosc/add" {
                            if let Some(args) = message.args {
                                if let OscType::String(ref device_name) = args[0] {
                                    info!("device added: {}", device_name);
                                } else {
                                    warn!("unexpected message for prefix {}", message.addr);
                                }
                            } else if message.addr == "/serialosc/remove" {
                                if let Some(args) = message.args {
                                    if let OscType::String(ref device_name) = args[0] {
                                        info!("device removed: {}", device_name);
                                    } else {
                                        warn!("unexpected message for prefix {}", message.addr);
                                    }
                                }
                            }
                        }
                    } else if message.addr.starts_with("/sys") {
                        if let Some(args) = message.args {
                            if message.addr.starts_with("/sys/port") {
                                if let OscType::Int(port) = args[0] {
                                    self.device_port = Some(port);
                                }
                            } else if message.addr.starts_with("/sys/host") {
                                if let OscType::String(ref name) = args[0] {
                                    self.device_name = Some(name.to_string());
                                }
                            } else if message.addr.starts_with("/sys/host") {
                                if let OscType::String(ref host) = args[0] {
                                    self.device_host = Some(host.to_string());
                                }
                            } else if message.addr.starts_with("/sys/id") {
                                if let OscType::String(ref id) = args[0] {
                                    self.device_id = Some(id.to_string());
                                }
                            } else if message.addr.starts_with("/sys/prefix") {
                                if let OscType::String(ref prefix) = args[0] {
                                    self.device_prefix = Some(prefix.to_string());
                                }
                            } else if message.addr.starts_with("/sys/rotation") {
                                if let OscType::Int(rotation) = args[0] {
                                    self.device_rotation = Some(rotation);
                                }
                            } else if message.addr.starts_with("/sys/size") {
                                if let OscType::Int(x) = args[0] {
                                    if let OscType::Int(y) = args[1] {
                                        self.device_size = Some((x, y));
                                    }
                                }
                            }
                        }
                    } else if message.addr.starts_with(PREFIX) {
                        if let Some(args) = message.args {
                            if message.addr.starts_with(&format!("{}/grid/key", PREFIX)) {
                                if let OscType::Int(x)  = args[0] {
                                    if let OscType::Int(y) = args[1] {
                                        if let OscType::Int(v) = args[2] {
                                            info!("Key: {}:{} {}", x, y, v);
                                        }
                                    }
                                }
                            } else if message.addr.starts_with(&format!("{}/grid/tilt", PREFIX)) {
                                if let OscType::Int(n)  = args[0] {
                                    if let OscType::Int(x) = args[1] {
                                        if let OscType::Int(y) = args[2] {
                                            if let OscType::Int(z) = args[2] {
                                                info!("Tilt {} {},{},{}", n, x, y, z);
                                            }
                                        }
                                    }
                                }
                            } else {
                                println!("{:?}", message.addr);
                            }
                        }
                    }
                }
                OscPacket::Bundle(_bundle) => {
                    panic!("wtf.");
                }
            }
        }
        println!("ciao");
    }
}

fn main() {
    env_logger::init();

    let mut enumerator = Enumerator::new().unwrap();
    let s = enumerator.setup();
    enumerator.set_socket(s.unwrap());
    tokio::run(enumerator.map_err(|e| println!("server error = {:?}", e)));
}
