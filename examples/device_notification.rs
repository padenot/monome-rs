extern crate env_logger;
extern crate monome;

use std::io;

use monome::DeviceChangeNotifier;

fn main() {
    env_logger::init();

    DeviceChangeNotifier::new(|event| {
        println!("{:?}", event);
    });
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .ok()
        .expect("Failed to read line");
}
