#[macro_use]
extern crate futures;
extern crate tokio;
extern crate rosc;
#[macro_use]
extern crate log;
extern crate env_logger;

use std::io;
use std::fmt;
use std::net::SocketAddr;
use std::{thread, time};
use std::sync::mpsc::{channel, Receiver, Sender};

use tokio::prelude::*;
use tokio::net::UdpSocket;

use futures::sync::mpsc as future_mpsc;

use rosc::decoder::decode;
use rosc::encoder::encode;
use rosc::{OscPacket, OscMessage, OscType};

pub const PREFIX: &str = "/prefix";
pub const SERIALOSC_PORT: i32 = 12002;

struct Transport {
    device_port: i32,
    socket: UdpSocket,
    tx: Sender<Vec<u8>>,
    rx: future_mpsc::Receiver<Vec<u8>>
}

impl Transport {
    pub fn new(serialosc_port: i32, rrx: future_mpsc::Receiver<Vec<u8>>)
        -> Result<(Transport, Receiver<Vec<u8>>, String, String, i32), String> {
        // serialosc address, pretty safe to hardcode
        let addr = format!("127.0.0.1:{}", serialosc_port).parse().unwrap();

        // find a free port
        let mut port = 10000;
        let socket = loop {
            let server_addr = format!("127.0.0.1:{}", port).parse().unwrap();
            let bind_result = UdpSocket::bind(&server_addr);
            match bind_result {
                Ok(socket) => {
                    break socket
                }
                Err(e) => {
                    error!("bind error: {}", e.to_string());
                    if port > 65536 {
                        panic!("Could not bind socket: port exhausted");
                    }
                }
            }
            port += 1;
        };

        let server_port = socket.local_addr().unwrap().port();
        let server_ip = socket.local_addr().unwrap().ip().to_string();

        let packet = message("/serialosc/list",
                             vec![OscType::String(server_ip),
                             OscType::Int(i32::from(server_port))]);

        let bytes: Vec<u8> = encode(&packet).unwrap();

        let rv = socket.send_dgram(bytes, &addr).and_then(|(socket, _)| {
            socket.recv_dgram(vec![0u8; 1024]).map(|(socket, data, _, _)| {
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
                                                    return Ok((name.clone(),
                                                               device_type.clone(),
                                                               port));
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
                    OscPacket::Bundle(_bundle) => {
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

                let rv = socket.send_dgram(bytes, &addr).and_then(|(socket, _)| {
                    let local_addr = socket.local_addr().unwrap().ip();
                    let packet = message("/sys/host",
                                         vec![OscType::String(local_addr.to_string())]);

                    let bytes: Vec<u8> = encode(&packet).unwrap();

                    socket.send_dgram(bytes, &addr).and_then(|(socket, _)| {
                        let packet = message("/sys/prefix", vec![OscType::String(PREFIX.to_string())]);

                        let bytes: Vec<u8> = encode(&packet).unwrap();
                        socket.send_dgram(bytes, &addr).and_then(|(socket, _)| {
                            let packet = message("/sys/info", vec![]);

                            let bytes: Vec<u8> = encode(&packet).unwrap();
                            socket.send_dgram(bytes, &addr).and_then(|(socket, _)| {
                                // led flash
                                let packet = message("/prefix/grid/led/all", vec![OscType::Int(1)]);
                                let bytes: Vec<u8> = encode(&packet).unwrap();
                                socket.send_dgram(bytes, &addr).and_then(|(socket, _)| {
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
                            let amt = try_ready!(self.socket.poll_send_to(&mut b.unwrap(), &addr));
                        }
                        Async::NotReady => {
                            break;
                        }
                    }
                }
                Err(e) => {
                    error!("Error on future::mpsc {:?}", e);
                }
            }
        }

        loop {
            let mut buf = vec![0; 1000];
            match self.socket.poll_recv(&mut buf) {
                Ok(fut) => {
                    match fut {
                        Async::Ready(_ready) => {
                            match self.tx.send(buf) {
                                Ok(()) => {
                                    continue;
                                }
                                Err(e) => {
                                    error!("receive from monome, {}", e);
                                }
                            }
                        }
                        Async::NotReady => {
                            return Ok(Async::NotReady);
                        }
                    }
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
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
    tx: future_mpsc::Sender<Vec<u8>>
}

fn message(addr: &str, args: Vec<OscType>) -> OscPacket {
    let message = OscMessage {
        addr: addr.to_owned(),
        args: Some(args),
    };
    OscPacket::Message(message)
}

impl Monome {
    pub fn new() -> Result<Monome, String> {
        let (sender, receiver) = futures::sync::mpsc::channel(16);
        let (transport, rx, name, device_type, port) = Transport::new(SERIALOSC_PORT, receiver).unwrap();

        thread::spawn(move || {
            tokio::run(transport.map_err(|e| error!("server error = {:?}", e)));
        });

        let monome = Monome {
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
    fn set(&mut self, x: i32, y: i32, on: bool) {
        self.send("/prefix/grid/led/set",
             vec![OscType::Int(x),
             OscType::Int(y),
             OscType::Int(if on { 1 } else { 0 })]);
    }
    //fn all(&mut self, on: bool) {
    //    self.send("/prefix/grid/led/all",
    //              vec![OscType::Int(if on { 1 } else { 0 })]);
    //}
    fn all(&mut self, intensity: i32) {
        self.send("/prefix/grid/led/level/all",
                  vec![OscType::Int(intensity)]);
    }
    fn map(&mut self, x: i32, y: i32, masks: Vec<i8>) {
        let mut args = Vec::with_capacity(10);

        args.push(OscType::Int(x));
        args.push(OscType::Int(y));

        for mask in masks.iter().map(|m| OscType::Int(*m as i32)) {
            args.push(mask);
        }
        self.send("/prefix/grid/led/map", args);
    }
    fn row(&mut self, x_offset: i32, y: i32, mask: i8) {
        let mut args = Vec::with_capacity(3);

        args.push(OscType::Int(x_offset));
        args.push(OscType::Int(y));

        // twice for a 128 for ex (implement dynamic check)
        args.push(OscType::Int(i32::from(mask)));
        args.push(OscType::Int(i32::from(mask)));

        self.send("/prefix/grid/led/row", args);
    }
    fn col(&mut self, x: i32, y_offset: i32, mask: i8) {
        let mut args = Vec::with_capacity(4);

        args.push(OscType::Int(x));
        args.push(OscType::Int(y_offset));

        args.push(OscType::Int(i32::from(mask)));

        self.send("/prefix/grid/led/col", args);
    }
    fn tilt_all(&mut self, on: bool) {

        for i in vec![0,1,2] {
            let mut args = Vec::with_capacity(2);
            args.push(OscType::Int(i));
            args.push(OscType::Int(if on { 1 } else { 0 }));

            self.send("/prefix/tilt/set", args.clone());
        }
    }
    fn send(&mut self, addr: &str, args: Vec<OscType>) {
        let message = OscMessage {
            addr: addr.to_owned(),
            args: Some(args),
        };
        let packet = OscPacket::Message(message);
        debug!("sending {:?}", packet);
        let bytes: Vec<u8> = encode(&packet).unwrap();
        match self.tx.try_send(bytes) {
            Ok(()) => {
            }
            Err(b) => {
                let full = b.is_full();
                let disconnected = b.is_disconnected();
                error!("full: {:?}, disconnected: {:?}", full, disconnected);
            }
        }
    }
    fn poll(&mut self) {
        match self.rx.try_recv() {
            Ok(buf) => {
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
                                    error!("not handled: {:?}", message.addr);
                                }
                            }
                        }
                    }
                    OscPacket::Bundle(_bundle) => {
                        panic!("wtf.");
                    }
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => { }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                error!("error tryrecv discon");
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

fn main() {
    env_logger::init();

    let mut monome = Monome::new().unwrap();

    let mut i = 0;

    monome.tilt_all(true);

    loop {
        monome.poll();
        monome.all(i);

        i+=1;

        if i > 16 {
            i = 0;
        }

        let refresh = time::Duration::from_millis(32);
        thread::sleep(refresh);
    }
}
