extern crate fn_time as ft;
extern crate rand;

use std::time::Duration;
use std::thread;

fn to_time(col: ft::Collector, mut fnt: ft::FnTimer) -> (ft::Collector, ft::FnTimer) {
    let mut counter = 0;
    fnt.capture(line!());
    fnt.counter(line!(), counter);
    thread::sleep(Duration::from_millis(200));
    fnt.capture(line!());
    thread::sleep(Duration::from_millis(200));
    counter += 1;

    fnt.counter(line!(), counter);
    let sleep_time: u64 = (rand::random::<f32>() * 400.0) as u64;
    thread::sleep(Duration::from_millis(sleep_time));
    fnt.capture(line!());
    thread::sleep(Duration::from_millis(200));
    counter += 1;
    fnt.capture(line!());
    fnt.counter(line!(), counter);

    col.send(&mut fnt);
    (col, fnt)
}

fn main(){
    let settings = ft::Settings{ 
        max_timers: 10, 
        max_counters: 10,
        average_length: 2, 
        num_top: 4,
        output: ft::Output::JSON("performance.log".to_owned()) };
    let (col, fnt, stats) = ft::new(settings);
    let test_handle = thread::spawn(move || {
        let mut model = (col, fnt);
        for _i in 0..5 {
            model = to_time(model.0, model.1);
        }
    });

    let stats_handle = ft::run_stats(stats);

    test_handle.join().expect("Failed to join testing thread");

    stats_handle.stop();
}
