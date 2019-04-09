extern crate env_logger;
extern crate monome;

use monome::Monome;

fn main() {
    env_logger::init();

    let enumeration = Monome::enumerate_devices();
    match enumeration {
        Ok(devices) => {
            for device in &devices {
                println!("{}", device);
                match Monome::from_device(device, "prefix") {
                    Ok(m) => {
                        println!("Monome setup:\n{}", m);
                    }
                    Err(e) => {
                        println!("Error setting up {} ({})", device, e);
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }
}
