extern crate env_logger;
extern crate monome;
extern crate num;
extern crate rand;
use rand::prelude::*;
use std::{thread, time};

use monome::Monome;

fn main() {
    env_logger::init();

    let mut monome = Monome::new("/prefix".to_string()).unwrap();

    println!("{:?}", monome);

    let mut v = [0; 64];
    let mut v2 = [false; 64];

    loop {
        for i in 0..64 {
            v[i] = (random::<u8>() % 16) as u8;
            v2[i] = if random::<u8>() % 2 == 0 { false } else { true };
        }
        // random intensity from 0 to 15
        monome.map(0, 0, &v);
        // On/Off
        monome.map(8, 0, &v2);

        let refresh = time::Duration::from_millis(33);
        thread::sleep(refresh);
    }
}
