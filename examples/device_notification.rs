extern crate env_logger;
extern crate monome;

use std::io;

use monome::Monome;
use monome::DeviceChangeEvent;

fn main() {
    env_logger::init();

    Monome::register_device_change_callback(|event| {
        match event {
            DeviceChangeEvent::Added(id) => {
                println!("Device {} added", id);
            }
            DeviceChangeEvent::Removed(id) => {
                println!("Device {} removed", id);
            }
        }
    });
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .ok()
        .expect("Failed to read line");
}
