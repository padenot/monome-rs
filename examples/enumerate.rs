extern crate env_logger;
extern crate monome;

use monome::Monome;

fn main() {
    env_logger::init();

    let enumeration = Monome::enumerate_devices_with_host_and_port("127.0.0.1".parse().unwrap(), 12002);
    match enumeration {
        Ok(devices) => {
            println!("Found {} devices.", devices.len());
            let mut idx = 0;
            for device in &devices {
                println!("Device {}: {}. Setting it up:", idx, device);
                idx+=1;
                match Monome::from_device(device, "prefix") {
                    Ok(m) => {
                        println!("Monome {} setup OK:\n{}", device, m);
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
