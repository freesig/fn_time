extern crate fn_time as ft;
extern crate rand;

use std::time::Duration;
use std::thread;

fn to_time(col: ft::Collector, mut fnt: ft::FnTime) -> (ft::Collector, ft::FnTime) {
    fnt.capture(line!());
    thread::sleep(Duration::from_millis(200));
    fnt.capture(line!());
    thread::sleep(Duration::from_millis(200));

    let sleep_time: u64 = (rand::random::<f32>() * 400.0) as u64;
    thread::sleep(Duration::from_millis(sleep_time));
    fnt.capture(line!());
    thread::sleep(Duration::from_millis(200));
    fnt.capture(line!());
    
    col.send(&mut fnt);
    (col, fnt)
}

fn main(){
    let (col, fnt, stats) = ft::new(10, 100, ft::Output::JSON("log.json".to_owned()));
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
