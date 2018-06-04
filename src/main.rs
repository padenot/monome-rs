#[macro_use]
extern crate futures;
extern crate tokio;
extern crate rosc;
extern crate num;
#[macro_use]
extern crate log;
extern crate env_logger;
extern crate rand;
use rand::prelude::*;

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

/// The port at which serialosc is running.
pub const SERIALOSC_PORT: i32 = 12002;

/// From a x and y position, and a stride, returns the offset at which the elment is in an array.
fn toidx(x: i32, y: i32, width: i32) -> usize {
  (y * width + x) as usize
}

#[derive(Debug)]
struct MonomeInfo {
    port: Option<i32>,
    host: Option<String>,
    prefix: Option<String>,
    id: Option<String>,
    size: Option<(i32,i32)>,
    rotation: Option<i32>
}

impl MonomeInfo {
    fn new() -> MonomeInfo {
        return MonomeInfo {
            port: None,
            host: None,
            prefix: None,
            id: None,
            size: None,
            rotation: None
        }
    }
    fn complete(&self) -> bool {
        self.port.is_some() &&
            self.host.is_some() &&
            self.prefix.is_some() &&
            self.id.is_some() &&
            self.size.is_some() &&
            self.rotation.is_some()
    }
    fn fill(&mut self, packet: OscPacket) {
        match packet {
            OscPacket::Message(message) => {
                if message.addr.starts_with("/sys") {
                    if let Some(args) = message.args {
                        if message.addr.starts_with("/sys/port") {
                            if let OscType::Int(port) = args[0] {
                                self.port = Some(port);
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
                }
            }
            OscPacket::Bundle(_bundle) => {
                error!("Bundle during setup!?");
            }
        }
    }
}


/// `Transport` implements the network input and output to and from serialosc, as well as the setup
/// of the device.
struct Transport {
    /// The port for this device. This is the to the first free port starting at
    /// 10000.
    device_port: i32,
    /// This is the socket with with we send and receive to and from the device.
    socket: UdpSocket,
    /// This is the channel we use to forward the received OSC messages to the client object.
    tx: Sender<Vec<u8>>,
    /// This is where Transport receives the OSX messages to send.
    rx: future_mpsc::Receiver<Vec<u8>>
}

impl Transport {
    pub fn new(device_port: i32,
               socket: UdpSocket,
               tx: Sender<Vec<u8>>,
               rx: future_mpsc::Receiver<Vec<u8>>)
        -> Transport {
      return Transport {
          device_port,
          socket,
          tx,
          rx
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
                            let _amt = try_ready!(self.socket.poll_send_to(&mut b.unwrap(), &addr));
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

/// The client object for a Monome grid device
pub struct Monome {
    /// The name of this device
    name: String,
    /// The type of this device
    device_type: String,
    /// The port at which this device is running at
    port: i32,
    /// The host for this device (usually localhost)
    host: String,
    /// The ID of this device
    id: String,
    /// The prefix set for this device
    prefix: String,
    /// The current rotation for this device. This can be 0, 90, 180 or 270.
    rotation: i32,
    /// THe x and y size for this device.
    size: (i32, i32),
    /// A channel that allows receiving serialized OSC messages from a device.
    rx: Receiver<Vec<u8>>,
    /// A channel that allows sending serialized OSC messages to a device.
    tx: future_mpsc::Sender<Vec<u8>>
}

/// Returns an osc packet from a address and arguments
fn message(addr: &str, args: Vec<OscType>) -> OscPacket {
    let message = OscMessage {
        addr: addr.to_owned(),
        args: Some(args),
    };
    OscPacket::Message(message)
}

/// Whether a key press is going up or down
#[derive(Debug)]
pub enum KeyDirection {
    /// The key has been released.
    Up,
    /// The key has been pressed.
    Down
}

/// An event received from a monome grid. This can be either a key press or release, or a tilt
/// event.
pub enum MonomeEvent {
    /// A key press or release
    GridKey {
        /// The horizontal offset at which the key has been pressed.
        x: i32,
        /// The vertical offset at which the key has been pressed.
        y: i32,
        /// Whether the key has been pressed (`Down`), or released (`Up`).
        direction: KeyDirection
    },
    /// An updated about the tilt of this device
    Tilt {
        /// Which sensor sent this tilt update
        n: i32,
        /// The pitch of this device
        x: i32,
        /// The roll of this device
        y: i32,
        /// The yaw of this device
        z: i32
    }
}

/// Converts an to a Monome method argument to a OSC address fragment and suitble OscType,
/// performing an eventual conversion.
pub trait IntoAddrAndArgs<B> {
    fn into_addr_frag_and_args(self) -> (String, B);
}

/// Used to make a call with an intensity value, adds the `"level/"` portion in the address.
impl IntoAddrAndArgs<OscType> for i32 {
    fn into_addr_frag_and_args(self) -> (String, OscType){
        ("level/".to_string(), OscType::Int(self))
    }
}

/// Used to make an on/off call, converts to 0 or 1.
impl IntoAddrAndArgs<OscType> for bool {
    fn into_addr_frag_and_args(self) -> (String, OscType) {
        ("".to_string(), OscType::Int(if self { 1 } else { 0 }))
    }
}

/// Used to convert vectors of integers for calls with an intensity value, adds the `"level/"`
/// portion in the address.
impl IntoAddrAndArgs<Vec<OscType>> for Vec<u8> {
    fn into_addr_frag_and_args(self) -> (String, Vec<OscType>){
        // TODO: error handling both valid: either 64 intensity values, or 8 masks
        assert!(self.len() == 64 || self.len() == 8);
        let mut osctype_vec = Vec::with_capacity(self.len());
        for item in self.iter().map(|i| OscType::Int(*i as i32)) {
            osctype_vec.push(item);
        }
        ("level/".to_string(), osctype_vec)
    }
}

/// Used to convert vectors of bools for on/off calls, packs into a 8-bit integer mask.
impl IntoAddrAndArgs<Vec<OscType>> for Vec<bool> {
    fn into_addr_frag_and_args(self) -> (String, Vec<OscType>) {
        // TODO: error handling
        assert!(self.len() == 64);
        let mut masks: Vec<u8> = vec![0; 8];
        for i in 0..8 { // for each row
            let mut mask: u8 = 0;
            for j in (0..8).rev() { // create mask
                let idx = toidx(j, i, 8);
                mask = mask.rotate_left(1) | if self[idx] { 1 } else { 0 };
            }
            masks[i as usize] = mask;
        }
        let mut osctype_vec = Vec::with_capacity(8);
        for item in masks.iter().map(|i| OscType::Int(*i as i32)) {
            osctype_vec.push(item);
        }
        ("".to_string(), osctype_vec)
    }
}

impl Monome {
    fn setup(serialosc_port: i32, prefix: &String)
        -> Result<(MonomeInfo, UdpSocket, String, String, i32), String>
    {
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
                    let packet =
                        message("/sys/host",
                                vec![OscType::String(local_addr.to_string())]);

                    let bytes: Vec<u8> = encode(&packet).unwrap();

                    socket.send_dgram(bytes, &addr).and_then(|(socket, _)| {
                        let packet =
                            message("/sys/prefix",
                                    vec![OscType::String(prefix.to_string())]);

                        let bytes: Vec<u8> = encode(&packet).unwrap();
                        socket.send_dgram(bytes, &addr).and_then(|(socket, _)| {
                            let packet = message("/sys/info", vec![]);

                            let bytes: Vec<u8> = encode(&packet).unwrap();
                            socket.send_dgram(bytes, &addr)
                        })
                    })
                }).wait();
                let (socket, _) = rv.unwrap();
                (socket, name, device_type, port)
            }).wait()
        }).wait();


        match rv {
            Ok((mut socket, name, device_type, port)) => {
                let mut info = MonomeInfo::new();

                // Loop until we've received all the /sys/info messages
                let socket = loop {
                    socket = socket.recv_dgram(vec![0u8; 1024])
                                        .and_then(|(socket, data, _, _)| {
                                            let packet = decode(&data).unwrap();
                                            info.fill(packet);
                                            Ok(socket)
                                        }).wait().map(|socket| {
                                            socket
                                        }).unwrap();

                    if info.complete() {
                        break socket
                    }
                };


                Ok((info,
                    socket,
                    name,
                    device_type,
                    port))
            }
            Err(e) => {
                Err(e.to_string())
            }
        }
    }
    /// Sets up a monome, with a particular prefix
    ///
    /// # Arguments
    ///
    /// * `prefix` - the prefix to use for this device and this application
    ///
    /// # Example
    ///
    /// Set up a monome, with a prefix of "/prefix":
    ///
    /// ```
    /// let m = new Monome("/prefix");
    ///
    /// match m {
    ///   Ok(monome) => {
    ///     println!("{:?}", monome);
    ///   }
    ///   Err(s) => {
    ///     println!("Could not setup the monome: {}", s);
    ///   }
    /// }
    /// ```
    pub fn new(prefix: String) -> Result<Monome, String> {
        let (sender, receiver) = futures::sync::mpsc::channel(16);;
        let (tx, rx) = channel();

        let (info,
             socket,
             name,
             device_type,
             device_port) = Monome::setup(SERIALOSC_PORT, &prefix).unwrap();

        let t = Transport::new(device_port, socket, tx, receiver);

        thread::spawn(move || {
            tokio::run(t.map_err(|e| error!("server error = {:?}", e)));
        });

        let monome = Monome {
            tx: sender,
            rx: rx,
            name: name,
            device_type: device_type,
            host: info.host.unwrap(),
            id: info.id.unwrap(),
            port: device_port,
            prefix: prefix,
            rotation: info.rotation.unwrap(),
            size: info.size.unwrap()
        };
        return Ok(monome);
    }

    /// Set a single led on a grid on or off.
    ///
    /// # Arguments
    ///
    /// - `x` - the horizontal position of the led to set.
    /// - `y` - the vertical positino of the led to set.
    /// - `arg` - either a bool, true to set a led On, false to set it Off, or a number between 0
    /// and 16, 0 being led off, 16 being full led brightness.
    ///
    /// # Example
    ///
    /// Set the led on the second row and second column to On, and also the third row and second
    /// column to mid-brightness:
    ///
    /// ```
    /// monome.set(1 /* 2nd, 0-indexed */,
    ///            1 /* 2nd, 0-indexed */,
    ///            true);
    /// monome.set(1 /* 2nd, 0-indexed */,
    ///            2 /* 3nd, 0-indexed */,
    ///            8);
    /// ```
    pub fn set<A>(&mut self, x: i32, y: i32, arg: A)
        where A: IntoAddrAndArgs<OscType> {
        let (frag, arg) = arg.into_addr_frag_and_args();
        self.send(&format!("/grid/led/{}set", frag).to_string(),
                  vec![OscType::Int(x),
                       OscType::Int(y),
                       arg]);
    }

    /// Set all led of the grid to an intensity
    ///
    /// # Arguments
    ///
    /// * `intensity` - either a bool, true for led On or false for led Off, or a number between 0
    /// and 16, 0 being led off, and 16 being full led brightness.
    ///
    /// # Example
    ///
    /// On a grid, set all led to medium brightness, then turn it on:
    ///
    /// ```
    /// monome.all(8);
    /// monome.all(false);
    /// ```
    pub fn all<A>(&mut self, arg: A)
        where A: IntoAddrAndArgs<OscType> {
        let (frag, arg) = arg.into_addr_frag_and_args();
        self.send(&format!("/grid/led/{}all", frag).to_string(), vec![arg]);
    }

    /// Set all the leds of a monome in one call.
    ///
    /// # Arguments
    ///
    /// * `leds` - a vector of 64 elements for a monome 64, 128 elements for a monome 128, and 256
    /// elements for a monome 256, packed in row order.
    ///
    /// # Example
    ///
    /// One a monome 128, do a checkerboard pattern:
    ///
    /// ```
    /// let grid: Vec<u8> = vec!(0; 128);
    /// for i in 0..128 {
    ///   grid[i] = (i + 1) % 2;
    /// }
    /// monome.set_all(grid);
    /// ```
    pub fn set_all(&mut self, leds: &Vec<u8>) {
        let width_in_quad = if leds.len() == 64 { 1 } else { 2 };
        let height_in_quad = if leds.len() == 256 { 2 } else { 1 };
        let width = width_in_quad * 8;

        for a in 0..height_in_quad {
            for b in 0..width_in_quad {
                let mut masks: Vec<u8> = vec![0; 8];
                for i in 0..8 { // for each row
                    let mut mask: u8 = 0;
                    for j in (0..8).rev() { // create mask
                      let idx = toidx(b * 8 + j, a * 8 + i, width);
                      mask = mask.rotate_left(1) | leds[idx];
                    }
                    masks[i as usize] = mask;
                }
                self.map(b * 8, a * 8, masks);
            }
        }
    }

    /// Set the value an 8x8 quad of led on a monome grid
    ///
    /// # Arguments
    ///
    /// * `x_offset` - at which offset, that must be a multiple of 8, to set the quad.
    /// * `y_offset` - at which offset, that must be a multiple of 8, to set the quad.
    /// * `masks` - a vector of 8 unsigned 8-bit integers that is a mask representing the leds to
    /// light up, or a vector of 64 bools, true for led On, false for led Off, packed in row order,
    /// or a vector of 64 integers between 0 and 15, for the brightness of each led, packed in
    /// row order.
    ///
    /// # Example
    ///
    /// On a monome 128, draw a triangle in the lower left half of the rightmost half, and a
    /// gradient on the leftmost half.
    /// ```
    /// let mut v: Vec<u8> = vec![0; 64];
    /// for i in 0..64 {
    ///     v[i] = i / 4;
    /// }
    /// monome.map(0, 0, v);
    /// monome.map(8, 0, vec![1, 3, 7, 15, 32, 63, 127, 0b11111111]);
    /// ```
    pub fn map<A>(&mut self, x_offset: i32, y_offset: i32, masks: A)
        where A: IntoAddrAndArgs<Vec<OscType>> {
        let mut args = Vec::with_capacity(10);

        let (frag, mut arg) = masks.into_addr_frag_and_args();

        args.push(OscType::Int(x_offset));
        args.push(OscType::Int(y_offset));
        args.append(&mut arg);

        self.send(&format!("/grid/led/{}map", frag), args);
    }

    /// Set a full row of a grid, using one or more 8-bit mask(s).
    ///
    /// # Arguments
    ///
    /// * `x_offset` - at which 8 button offset to start setting the leds. This is always 0 for a
    /// 64, and can be 8 for a 128 or 256.
    /// * `y` - which row to set, 0-indexed. This must be lower than the number of rows of the
    /// device.
    /// * `masks` - the list of masks that determine the pattern to light on for a particular 8 led
    /// long row. This vector MUST be one element long for monome 64, and for a monome 128 and 256
    /// if `x_offset` is 8, two elements long for monome 128 and 256 if `y_offset` is 0.
    ///
    /// # Example
    ///
    /// On a monome 128, light up every other led of the right half of the 3rd  row:
    ///
    /// ```
    ///   monome.col(8 /* rightmost half */
    ///              2 /* 3rd row, 0 indexed */,
    ///              vec![0b01010101u8] /* every other led, 85 in decimal */);
    /// ```
    pub fn row(&mut self, x_offset: i32, y: i32, masks: Vec<u8>) {
        let mut args = Vec::with_capacity(3);

        args.push(OscType::Int(x_offset));
        args.push(OscType::Int(y));

        for mask in masks.iter().map(|m| OscType::Int(*m as i32)) {
            args.push(mask);
        }

        self.send("/grid/led/row", args);
    }

    /// Set a full column of a grid, using one or more 8-bit mask(s).
    ///
    /// # Arguments
    ///
    /// * `x` - which column to set 0-indexed. This must be lower than the number of columns of the
    /// device.
    /// * `y_offset` - at which 8 button offset to start setting the leds. This is always 0 for a
    /// 64 and 128, can be 8 for a 256.
    /// * `masks` - the list of masks that determine the pattern to light on for a particular 8 led
    /// long column. This vector MUST be one element long for monome 64 and 128, and 256 if `y_offset`
    /// is 0, two elements long for monome 256 if `y_offset` is 8.
    ///
    /// # Example
    ///
    /// On a monome 256, light up every other led of the bottom half of the 3rd column from the
    /// right:
    ///
    /// ```
    ///   monome.col(2 /* 3rd column, 0-indexed */,
    ///              8 /* bottom half */,
    ///              vec![0b01010101u8] /* every other led, 85 in decimal */);
    /// ```
    pub fn col(&mut self, x: i32, y_offset: i32, masks: Vec<u8>) {
        let mut args = Vec::with_capacity(4);

        args.push(OscType::Int(x));
        args.push(OscType::Int(y_offset));

        for mask in masks.iter().map(|m| OscType::Int(*m as i32)) {
            args.push(mask);
        }

        self.send("/grid/led/col", args);
    }

    /// Enable or disable all tilt sensors (usually, there is only one), which allows receiving the
    /// `/<prefix>/tilt/` events, with the n,x,y,z coordinates as parameters.
    pub fn tilt_all(&mut self, on: bool) {
        self.send("/tilt/set", vec![OscType::Int(if on { 1 } else { 0 })]);
    }

    /// Set the rotation for this device. This is either 0, 90, 180 or 270
    pub fn set_rotation(&mut self, rotation: i32) {
        self.send_no_prefix("/sys/rotation", vec![OscType::Int(rotation)]);
        self.rotation = rotation;
    }

    /// Set the prefix for this device.
    pub fn set_prefix(&mut self, prefix: String) {
        self.send_no_prefix("/sys/prefix", vec![OscType::String(prefix.clone())]);
        self.prefix = prefix;
    }

    /// Get the name of this device.
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// Get the type for this device (for example `"monome 128"`).
    pub fn device_type(&self) -> String {
        self.device_type.clone()
    }

    /// Get the port for this device.
    pub fn port(&self) -> i32 {
        self.port
    }

    /// Get the host for this device is at.
    pub fn host(&self) -> String {
        self.host.clone()
    }

    /// Get the id of this device.
    pub fn id(&self) -> String {
        self.id.clone()
    }

    /// Get the current prefix of this device.
    pub fn prefix(&self) -> String {
        self.prefix.clone()
    }

    /// Get the current rotation of this device.
    pub fn rotation(&self) -> i32 {
        self.rotation
    }

    /// Get the size of this device, as a `(width, height)`.
    pub fn size(&self) -> (i32, i32) {
        self.size
    }

    /// Adds the prefix, packs the OSC message into an u8 vector and sends it to the transport.
    fn send(&mut self, addr: &str, args: Vec<OscType>) {
        let with_prefix = format!("{}{}", self.prefix, addr);
        self.send_no_prefix(&with_prefix, args);
    }

    /// Adds the prefix, packs the OSC message into an u8 vector and sends it to the transport.
    fn send_no_prefix(&mut self, addr: &str, args: Vec<OscType>) {
        let message = OscMessage {
            addr: addr.to_owned(),
            args: Some(args),
        };
        let packet = OscPacket::Message(message);
        debug!("⇨ {:?}", packet);
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

    /// Receives a MonomeEvent, from a connected monome, which can be a grid key press, or an event
    /// from the tilt sensor. Only the events from the set `prefix` will be received.
    ///
    /// # Example
    ///
    /// ```
    /// loop {
    ///     match monome.poll() {
    ///         Some(MonomeEvent::GridKey{x, y, direction}) => {
    ///             match direction {
    ///                 KeyDirection::Down => {
    ///                     println!("Key pressed: {}x{}", x, y);
    ///                 }
    ///                 KeyDirection::Up => {
    ///                     println!("Key released: {}x{}", x, y);
    ///                 }
    ///             }
    ///         }
    ///         Some(MonomeEvent::Tilt{n: _n, x, y, z: _z}) => {
    ///           println!("tilt update: pitch: {}, roll {}", x, y);
    ///         }
    ///         None => {
    ///             break;
    ///         }
    ///     }
    /// }
    /// ```
    pub fn poll(&mut self) -> Option<MonomeEvent> {
        match self.rx.try_recv() {
            Ok(buf) => {
                self.parse(&buf)
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                return None;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                error!("error tryrecv discon");
                return None;
            }
        }
    }

    fn parse(&self, buf: &Vec<u8>) -> Option<MonomeEvent> {
        let packet = decode(buf).unwrap();
        debug!("⇦ {:?}", packet);

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
                        };
                    }
                    None
                } else if message.addr.starts_with("/sys") {
                    debug!("/sys received: {:?}", message);
                    None
                } else if message.addr.starts_with(&self.prefix) {
                    if let Some(args) = message.args {
                        if message.addr.starts_with(&format!("{}/grid/key", self.prefix)) {
                            if let OscType::Int(x)  = args[0] {
                                if let OscType::Int(y) = args[1] {
                                    if let OscType::Int(v) = args[2] {
                                        info!("Key: {}:{} {}", x, y, v);
                                        return Some(MonomeEvent::GridKey {
                                            x, y, direction: if v == 1 { KeyDirection::Down } else { KeyDirection::Up }
                                        });
                                    } else { None }
                                } else  { None }
                            } else { None }
                        } else if message.addr.starts_with(&format!("{}/tilt", self.prefix)) {
                            if let OscType::Int(n)  = args[0] {
                                if let OscType::Int(x) = args[1] {
                                    if let OscType::Int(y) = args[2] {
                                        if let OscType::Int(z) = args[2] {
                                            info!("Tilt {} {},{},{}", n, x, y, z);
                                            return Some(MonomeEvent::Tilt {
                                                n, x, y, z
                                            });
                                        } else { None }
                                    }  else { None }
                                } else { None }
                            } else { None }
                        } else {
                            error!("not handled: {:?}", message.addr);
                            return None;
                        }
                    } else { None }
                } else { None }
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
         self.host,
         self.id,
         self.prefix,
         self.rotation,
         self.size.0,
         self.size.1)
    }
}

fn main() {
    env_logger::init();

    let mut monome = Monome::new("/prefix".to_string()).unwrap();

    monome.tilt_all(true);

    let mut grid: Vec<u8> = vec!(0; 128);

    fn moveall(grid: &mut Vec<u8>, dx: i32, dy: i32) {
        let mut grid2: Vec<u8> = vec!(0; 128);
        for x in 0..16 {
            for y in 0..8 {
                grid2[toidx(num::clamp(x + dx, 0, 15),
                            num::clamp(y + dy, 0, 7), 16)] = grid[toidx(x, y, 16)];
            }
        }

        for x in 0..128 {
            grid[x] = grid2[x];
        }
    }

    let mut i = 0;

    let mut x = 0;
    let mut y= 0;

    println!("{:?}", monome);

    loop {
        loop {
            match monome.poll() {
                Some(MonomeEvent::GridKey{x, y, direction}) => {
                    match direction {
                        KeyDirection::Down => {
                            let idx = toidx(x, y, 16);
                            grid[idx] = if grid[idx] == 1 { 0 } else { 1 }
                        }
                        _ => {}
                    }
                }
                Some(MonomeEvent::Tilt{n: _n, x, y, z: _z}) => {
                    if i % 10 == 0{
                        moveall(&mut grid, (-x as f32 / 64.) as i32, (-y as f32/ 64.) as i32);
                    }
                    i+=1;
                }
                None => {
                    break;
                }
            }
        }


        // monome.set(x, y, x);
        // x += 1;
        // if (x == 16) {
        //     x = 0;
        //     y += 1;
        //     if (y == 8) {
        //         y = 0;
        //     }
        // }
        let mut v: Vec<u8> = vec![0; 64];
        let mut v2 = vec![false; 64];
        for i in 0..64 {
            v[i] = (random::<u8>() % 16) as u8;
            v2[i] = random();
        }
        monome.map(0, 0, v);
        monome.map(8, 0, v2);

        let refresh = time::Duration::from_millis(33);

        thread::sleep(refresh);
    }
}
