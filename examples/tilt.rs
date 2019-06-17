extern crate env_logger;
extern crate monome;
extern crate num;
extern crate rand;
use std::{thread, time};

use monome::{KeyDirection, Monome, MonomeEvent};

fn toidx(x: i32, y: i32, width: i32) -> usize {
    (y * width + x) as usize
}

fn main() {
    env_logger::init();

    let mut monome = Monome::new("/prefix".to_string()).unwrap();

    monome.tilt_all(true);

    let mut grid: Vec<bool> = vec![false; 128];

    fn moveall(grid: &mut Vec<bool>, dx: i32, dy: i32) {
        let mut grid2: Vec<bool> = vec![false; 128];
        for x in 0..16 {
            for y in 0..8 {
                grid2[toidx(num::clamp(x + dx, 0, 15), num::clamp(y + dy, 0, 7), 16)] =
                    grid[toidx(x, y, 16)];
            }
        }

        for x in 0..128 {
            grid[x] = grid2[x];
        }
    }

    let mut i = 0;

    println!("{:?}", monome);

    loop {
        loop {
            match monome.poll() {
                Some(MonomeEvent::GridKey { x, y, direction }) => match direction {
                    KeyDirection::Down => {
                        let idx = toidx(x, y, 16);
                        grid[idx] = !grid[idx];
                    }
                    _ => {}
                },
                Some(MonomeEvent::Tilt { n: _n, x, y, z: _z }) => {
                    if i % 10 == 0 {
                        moveall(
                            &mut grid,
                            (-x as f32 / 64.) as i32,
                            (-y as f32 / 64.) as i32,
                        );
                    }
                    i += 1;
                }
                _ => {
                    break;
                }
            }
        }

        monome.set_all(&grid);

        let refresh = time::Duration::from_millis(33);
        thread::sleep(refresh);
    }
}
