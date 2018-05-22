#[macro_use]
extern crate futures;
extern crate tokio;
extern crate rosc;
#[macro_use]
extern crate log;
extern crate env_logger;

use std::io::{self, Read};
use std::fmt;
use std::net::SocketAddr;
use std::thread;
use std::sync::mpsc::{self, channel, Receiver, Sender};

use tokio::prelude::*;
use tokio::net::UdpSocket;

use futures::sync::mpsc as fut_mpsc;

use rosc::decoder::decode;
use rosc::encoder::encode;
use rosc::{OscPacket, OscMessage, OscType};

pub const PREFIX: &str = "/prefix";

struct Transport {
    server_port: u16,
    device_port: i32,
    socket: UdpSocket,
    tx: Sender<Vec<u8>>,
    rx: fut_mpsc::Receiver<Vec<u8>>
}

impl Transport {
    pub fn new(rrx: fut_mpsc::Receiver<Vec<u8>>) -> Result<(Transport, Receiver<Vec<u8>>, String, String, i32), String> {
        // serialosc address, pretty safe to hardcode
        let addr = "127.0.0.1:12002".parse().unwrap();

        // find a free port
        let socket = loop {
            let mut port = 10000;
            let server_addr = format!("127.0.0.1:{}", port).parse().unwrap();
            let bind_result = UdpSocket::bind(&server_addr);
            match bind_result {
                Ok(socket) => {
                    break socket
                }
                Err(e) => {
                    port = port + 1;
                }
            }
        };

        let server_port = socket.local_addr().unwrap().port();
        let server_ip = socket.local_addr().unwrap().ip().to_string();

        let packet = message("/serialosc/list",
                             vec![OscType::String(server_ip),
                             OscType::Int(i32::from(server_port))]);

        let bytes: Vec<u8> = encode(&packet).unwrap();

        let rv = socket.send_dgram(bytes, &addr).and_then(|(socket, _)| {
            socket.recv_dgram(vec![0u8; 1024]).map(|(socket, data, len, _)| {
                let packet = decode(&data).unwrap();

                let rv = match packet {
                    OscPacket::Message(message) => {
                        (||{
                            if message.addr.starts_with("/serialosc") {
                                if message.addr == "/serialosc/device" {
                                    if let Some(args) = message.args {
                                        if let OscType::String(ref name) = args[0] {
                                            if let OscType::String(ref device_type) = args[1] {
                                                if let OscType::Int(port) = args[2] {
                                                    return Ok((name.clone(), device_type.clone(), port));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Err("Bad format")
                        }
                        )()
                    }
                    OscPacket::Bundle(bundle) => {
                        Err("Unexpected bundle received during setup")
                    }
                };

                let (name, device_type, port): (String, String, i32) = rv.unwrap();

                let device_address = format!("127.0.0.1:{}", port);
                let add = device_address.parse();
                let addr: SocketAddr = add.unwrap();

                let packet = message("/sys/port",
                                     vec![OscType::Int(i32::from(server_port))]);

                let bytes: Vec<u8> = encode(&packet).unwrap();

                let rv = socket.send_dgram(bytes, &addr).and_then(|(socket, buf)| {
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
                                    debug!("finished init");
                                    socket.send_dgram(bytes, &addr)
                                })
                            })
                        })
                    })
                }).wait();
                let (socket, _) = rv.unwrap();
                (socket, name, device_type, port)
            }).wait()
        }).wait();

        match rv {
            Ok((socket, name, device_type, port)) => {
                let (tx, rx) = channel();
                Ok((Transport {
                    socket,
                    server_port,
                    device_port: port,
                    tx: tx,
                    rx: rrx
                    }, rx, name, device_type, port))
            }
            Err(e) => {
                Err(e.to_string())
            }
        }
    }
}

impl Future for Transport {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<(), io::Error> {
        loop {
            match self.rx.poll() {
                Ok(fut) => {
                    match fut {
                        Async::Ready(b) => {
                            let device_address = format!("127.0.0.1:{}", self.device_port);
                            let addr: SocketAddr = device_address.parse().unwrap();
                            match self.socket.poll_send_to(&mut b.unwrap(), &addr) {
                                Ok(Async::Ready(count)) => {
                                }
                                Ok(Async::NotReady) => {
                                }
                                Err(_) => {
                                    println!("Error sending");
                                }
                            }
                        }
                        Async::NotReady => {
                        }

                    }
                }
                Err(e) => {
                    println!("{:?}", e);
                }
            }

            let mut buf = vec![0; 1000];
            match self.socket.poll_recv(&mut buf) {
                Ok(fut) => {
                    match fut {
                        Async::Ready(ready) => {
                            self.tx.send(buf);
                        }
                        Async::NotReady => {
                            futures::task::park();
                            return Ok(Async::NotReady);
                        }
                    }
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
        println!("ciao");
    }
}

struct Monome {
    name: String,
    device_type: String,
    port: i32,
    host: Option<String>,
    id: Option<String>,
    prefix: Option<String>,
    rotation: Option<i32>,
    size: Option<(i32, i32)>,
    rx: Receiver<Vec<u8>>,
    tx: fut_mpsc::Sender<Vec<u8>>
}

impl Monome {
    pub fn new() -> Result<Monome, String> {
        let (sender, receiver) = futures::sync::mpsc::channel(16);
        let (transport, rx, name, device_type, port) = Transport::new(receiver).unwrap();

        thread::spawn(move || {
            tokio::run(transport.map_err(|e| println!("server error = {:?}", e)));
        });

        let mut monome = Monome {
            tx: sender,
            rx: rx,
            name: name,
            device_type: device_type,
            host: None,
            id: None,
            port: port,
            prefix: None,
            rotation: None,
            size: None
        };
        return Ok(monome);
    }
    fn send(&mut self, a: i32) {
        let packet = message("/prefix/grid/led/all", vec![OscType::Int(a)]);
        let bytes: Vec<u8> = encode(&packet).unwrap();
        self.tx.try_send(bytes).unwrap();
    }
    fn poll(&mut self) {
            let buf = self.rx.recv().unwrap();
            let packet = decode(&buf).unwrap();
            debug!("⇐  {:?}", packet);

            match packet {
                OscPacket::Message(message) => {
                    if message.addr.starts_with("/serialosc") {
                        if message.addr == "/serialosc/device" {
                          info!("/serialosc/device");
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
                                    info!("/sys/port {}", port);
                                }
                            } else if message.addr.starts_with("/sys/host") {
                                if let OscType::String(ref host) = args[0] {
                                    self.host = Some(host.to_string());
                                }
                            } else if message.addr.starts_with("/sys/id") {
                                if let OscType::String(ref id) = args[0] {
                                    self.id = Some(id.to_string());
                                }
                            } else if message.addr.starts_with("/sys/prefix") {
                                if let OscType::String(ref prefix) = args[0] {
                                    self.prefix = Some(prefix.to_string());
                                }
                            } else if message.addr.starts_with("/sys/rotation") {
                                if let OscType::Int(rotation) = args[0] {
                                    self.rotation = Some(rotation);
                                }
                            } else if message.addr.starts_with("/sys/size") {
                                if let OscType::Int(x) = args[0] {
                                    if let OscType::Int(y) = args[1] {
                                        self.size = Some((x, y));
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
}

impl fmt::Debug for Monome {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Monome {}\ntype: {}\nport: {}\nhost: {}\nid: {}\nprefix: {}\nrotation: {}\nsize: {}:{}",
         self.name,
         self.device_type,
         self.port,
         self.host.as_ref().unwrap_or(&String::from("?")),
         self.id.as_ref().unwrap_or(&String::from("?")),
         self.prefix.as_ref().unwrap_or(&String::from("?")),
         self.rotation.unwrap_or(0).to_string(),
         self.size.unwrap_or((0,0)).0,
         self.size.unwrap_or((0,0)).1)
    }
}

fn message(addr: &str, args: Vec<OscType>) -> OscPacket {
    let message = OscMessage {
        addr: addr.to_owned(),
        args: Some(args),
    };
    OscPacket::Message(message)
}

fn main() {
    env_logger::init();

    let mut monome = Monome::new().unwrap();

    let mut a = 0;

    loop {
        monome.poll();
        monome.send(a);
        a = (a + 1) % 2;
    }
}
