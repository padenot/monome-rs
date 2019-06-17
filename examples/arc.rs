extern crate env_logger;
extern crate monome;
use std::{thread, time};

use monome::{Monome, MonomeEvent};

fn main() {
    env_logger::init();

    let mut monome = Monome::new("/prefix".to_string()).unwrap();

    println!("{:?}", monome);

    let mut led: [f32; 4]= [0., 0., 0., 0.];
    let mut ii: i32 = 0;
    let mut dii: i32 = 1;

    for i in 0..4 {
        monome.ring_all(i, 0);
    }

    loop {
        let e = monome.poll();
        ii+=dii;
        if ii == 120 || ii == 0 {
            dii = -dii;
        }

         monome.ring_range(0, 0, 32, 15);
         monome.ring_range(1, 32, 64, 15);
         monome.ring_range(2, 16, 48, 15);
         monome.ring_range(3, 48, 16, 15);
        //let mut v: [u8; 64] = [0; 64];

        //for i in 0..64 {
        //    v[i] = (i / 4) as u8;
        //}

        //monome.ring_map(0, &v);
        //monome.ring_map(1, &v);
        //monome.ring_map(2, &v);
        //monome.ring_map(3, &v);

        // for i in 0..4 {
        //     monome.ring_all(i, (ii / 10) as u32);
        //     monome.ring_set(i, led[i] as u32, 15);
        // }
        match e {
            Some(MonomeEvent::EncoderDelta {n, delta} ) => {
                let n = n as usize;
                monome.ring_set(n, led[n] as u32, 0);
                led[n] = led[n] + (delta as f32 / 4.);
               // if led[n] > 64. {
               //     led[n] -= 64.;
               // } else 
                    if led[n] < 0. {
                    led[n] += 64.;
                }
                monome.ring_set(n, led[n] as u32, 15);
            }
            _ => {
            }
        }


        let refresh = time::Duration::from_millis(10);
        thread::sleep(refresh);
    }
}
