extern crate env_logger;
extern crate monome;
use std::{thread, time};

use monome::{Monome, MonomeEvent};

fn main() {
    env_logger::init();

    let mut monome = Monome::new("/prefix".to_string()).unwrap();

    println!("{:?}", monome);

    let mut led = [0.; 4];

    for i in 0..4 {
        monome.ring_all(i, 0);
    }

    loop {
        let e = monome.poll();

        for i in 0..4 {
            monome.ring_set(i, led[i] as u32, 15);
        }
        match e {
            Some(MonomeEvent::EncoderDelta { n, delta }) => {
                let n = n as usize;
                monome.ring_set(n, led[n] as u32, 0);
                led[n] = led[n] + (delta as f32 / 4.);
                if led[n] < 0. {
                    led[n] += 64.;
                }
                monome.ring_set(n, led[n] as u32, 15);
            }
            _ => {}
        }

        let refresh = time::Duration::from_millis(10);
        thread::sleep(refresh);
    }
}
