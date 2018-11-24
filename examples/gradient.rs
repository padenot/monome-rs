extern crate env_logger;
extern crate monome;
use std::{thread, time};

use monome::Monome;

fn main() {
    env_logger::init();

    let mut monome = Monome::new("/prefix".to_string()).unwrap();

    println!("{:?}", monome);

    let mut v = [0; 64];

    let mut sp : isize = 1;
    let mut dir : isize = 1;

    loop {
        for i in 0..8 {
            for j in 0..8 {
                v[i * 8 + j] = (sp / ((i + 1) as isize)) as u8;
            }
        }

        let mut grid: Vec<u8> = vec!(0; 128);
        for i in 0..8 {
            for j in 0..16 {
                grid[i * 16 + j] =  (sp / ((i + 1) as isize)) as u8;
            }
        }

        // both methods are equivalent
        monome.set_all_intensity(&grid);

        monome.map(0, 0, &v);
        monome.map(8, 0, &v);

        sp += dir;
        if sp == 15 {
            dir = -1;
        }
        if sp == 1 {
            dir = 1;
        }

        let refresh = time::Duration::from_millis(100);
        thread::sleep(refresh);
    }
}

