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

struct Enumerator {
    socket: UdpSocket,
    server_port: u16,
    buf: Vec<u8>,
    received_server: Option<(usize, std::net::SocketAddr)>,
    enumerated: bool
}

impl Enumerator {
    pub fn new() -> Result<Enumerator, String> {

        let server_addr = "0.0.0.0:10000".parse().unwrap();
        let bind_result = UdpSocket::bind(&server_addr);

        match bind_result {
            Ok(socket) => {
                let enumerator = Enumerator {
                    received_server: None,
                    socket: socket,
                    server_port: 10000, // FIXME stop hardcode
                    buf: vec![0; 1024],
                    enumerated: false
                };
                return Ok(enumerator);
            }
            Err(e) => {
                return Err(e.to_string());
            }
        }
    }

    pub fn enumerate_devices(&mut self) -> Result<i16, i16> {
      self.enumerated = true;
      let enumeration_task = EnumerationTask::new().unwrap();
      tokio::spawn(enumeration_task.map_err(|e| {
          println!("enumeration = {:?}", e);
        })
      );
      Ok(12123)
    }
}

struct EnumerationTask {
  socket: UdpSocket,
  server_port: u16
}

impl EnumerationTask {
    pub fn new() -> Result<EnumerationTask, String> {
        let client_addr = "0.0.0.0:0".parse().unwrap();
        let bind_result = UdpSocket::bind(&client_addr);

        match bind_result {
            Ok(socket) => {
                let enumeration_task = EnumerationTask {
                    socket: socket,
                    server_port: 10000, // FIXME stop hardcode
                };
                return Ok(enumeration_task);
            } 
            Err(e) => {
                return Err(e.to_string());
            }
        }
    }
}

impl Future for EnumerationTask {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<(), io::Error> {
        let addr = "127.0.0.1";
        let packet = message("/serialosc/list",
                             vec![OscType::String(addr.to_string()),
                             OscType::Int(i32::from(self.server_port))]);

        let bytes: Vec<u8> = encode(&packet).unwrap();

        // serialosc address
        let addr = "127.0.0.1:12002".parse().unwrap();

        try_ready!(self.socket.poll_send_to(&bytes, &addr));

        println!("sent: {:?}", packet);

        return Ok(Async::Ready(()));
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
        // initially, get the devices
        if !self.enumerated {
            let _op = self.enumerate_devices();
        }

        // Then, continously read the incoming packets
        loop {
            self.received_server = Some(try_ready!(self.socket.poll_recv_from(&mut self.buf)));
            let packet = decode(&self.buf).unwrap();
            println!("OscConnection: <- {:?}", packet);
        }
    }
}

fn main() {
    let enumerator = Enumerator::new().unwrap();
    tokio::run(enumerator.map_err(|e| println!("server error = {:?}", e)));
}
