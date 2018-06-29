extern crate serde;
extern crate serde_json;

#[macro_use]
extern crate serde_derive;

use std::sync::mpsc::{Sender, Receiver};
use std::sync::mpsc;
use std::time::{Instant, Duration};
use std::collections::{HashMap, VecDeque};
use std::thread;
use std::fmt;
use std::fs::File;
use std::io::Write;
use std::fs::OpenOptions;

type LineDuration = VecDeque<(u32, Duration)>;


pub struct Spot{
    line: u32,
    when: Instant,
}

pub struct FnTime{
    spots: Vec<Option<Spot>>,
    last_inst: Option<Instant>,
    current_spot: usize,
}

pub struct Collector{
    tx: Sender<LineDuration>,
}

pub struct Stats{
    rx: Receiver<LineDuration>,
    tops: HashMap<u32, Vec<Duration>>,
    visits: u32,
    total_time: Duration,
    count: u64,
    reset_at: u64,
    totals: HashMap<u32, Duration>,
    output: Output
}

pub enum Output {
    Print,
    JSON(String),
}

struct KillMsg;

pub struct StatsHandle{
    tx: Sender<KillMsg>,
    handle: thread::JoinHandle<()>,
}

#[derive(Serialize, Deserialize)]
struct LineTiming {
    line_number: u32,
    top_durations: Vec<(Duration, f64)>,
    average_of_line: Duration,
}

pub fn new(num_spots: usize, reset_at: u64, output: Output) -> (Collector, FnTime, Stats) {
    let (tx, rx) = mpsc::channel();
    if let Output::JSON(ref s) = output {
        wipe_file(&s[..]);
    }
    (Collector{ tx }, 
     FnTime::new(num_spots),
     Stats{ rx , tops: HashMap::new(), 
         visits: 0, total_time: Duration::new(0, 0), 
         count: 0, reset_at,
         totals: HashMap::new(),
         output,
     } )
}

// Convenience function for threading
pub fn run_stats(mut stats: Stats) -> StatsHandle {
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        loop{
            match rx.try_recv() {
                Ok(KillMsg) => break,
                Err(_)=> stats.top(),
            }
        }
    });
    StatsHandle{ tx, handle }
}

impl StatsHandle {
    pub fn stop(self) {
        self.tx.send(KillMsg).expect("Failed to send kill message to stats thread");
        self.handle.join().expect("Failed to join stats thread");
    }
}

impl Spot{
    pub fn capture(line: u32) -> Self{
        let when = Instant::now();
        Spot{ line, when }
    }
}

impl FnTime{
    pub fn new(num_spots: usize) -> Self {
        let mut spots: Vec<Option<Spot>> = Vec::with_capacity(num_spots);
        for _i in 0..num_spots {
            spots.push(None);
        }
        FnTime{ spots, last_inst: None, current_spot: 0 }
    }

    pub fn make_durations(&mut self) -> LineDuration {
        let mut times: LineDuration = VecDeque::new();
        for s in &self.spots {
            match s {
                Some(s) =>{
                    let duration = self.last_inst.map_or_else(
                        || Duration::new(0, 0),
                        |li| s.when.duration_since(li));
                    times.push_back((s.line, duration));
                    self.last_inst = Some(s.when);
                },
                None =>(),
            }
        }
        self.last_inst = None;
        self.current_spot = 0;
        times
    }

    pub fn capture(&mut self, line: u32) {
        self.spots[self.current_spot] = Some(Spot{ line, when: Instant::now() });
        self.current_spot += 1;
    }
}

impl Collector{
    pub fn send(&self, fnt: &mut FnTime){
        let times = fnt.make_durations();
        self.tx.send(times).unwrap_or_else(|e| {
            println!("fn_time failed to send data from timed function because: {}", e) });
    }
}

impl Stats{
    pub fn print(&mut self){
        for t in &self.rx {
            for (k, v) in t.iter() {
                println!("id: {} elapsed: {:?}", k, v);
            }
        }
    }

    pub fn top(&mut self){
        // Create output file if needed
        let mut json_output = match self.output {
            Output::JSON(ref s) => {
                OpenOptions::new().append(true).create(true).open(s).ok()
            },
            Output::Print => None,
        };
        for t in &self.rx {
            // Average duration of function
            let average = self.total_time.checked_div(self.visits);
            self.visits += 1;
            
            for (line, duration) in t.iter() {
                self.total_time += *duration;
                // Insert line in tops if it doesn't exist
                if !self.tops.contains_key(line) {
                    self.tops.insert(*line, vec![Duration::new(0,0); 10]);
                }
                if !self.totals.contains_key(line) {
                    self.totals.insert(*line, Duration::new(0,0));
                }
                let total_of_line = self.totals.get_mut(line).unwrap();
                *total_of_line += *duration;
                let average_of_line = total_of_line.checked_div(self.visits).unwrap_or_else(||Duration::new(0, 0));
                let mut top_durations = self.tops.get_mut(line).expect("Top durations missing");
                add_duration(top_durations, duration);

                if let Some(av) = average {
                    let av = duration_to_nano(&av);
                    let durations = top_durations.iter().map(|&top|{
                        let top_nano = duration_to_nano(&top);
                        if av > 0 {
                            (top, top_nano as f64 / av as f64 * 100.0)
                        }else{ (top, 0.0) }
                    }).collect();
                    let timings = LineTiming{ line_number: *line,
                    top_durations: durations,
                    average_of_line };

                    match self.output {
                        Output::JSON(_) => write_json(&mut json_output,
                        &timings),
                        Output::Print => println!("{}", timings),
                    }
                }
            }
            self.count += 1;
            if self.count > self.reset_at {
                self.tops = HashMap::new();
                self.count = 0;
            }
        }
    }
}

impl fmt::Display for LineTiming {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Line number: {}", self.line_number)?;
        for (top, p) in &self.top_durations {
            writeln!(f, "Duration: {:?}", top)?;
            writeln!(f, "Percentage of top: {:.*}%", 1, p)?;
        }
        write!(f, "Average duration of line: {:?}", self.average_of_line)
    }
}

fn duration_to_nano(dur: &Duration) -> u32 {
    (dur.as_secs() as u32 * 1_000_000_000) + dur.subsec_nanos()
}

fn add_duration(top_durations: &mut Vec<Duration>, duration: &Duration) {
    top_durations.push(*duration);
    top_durations.sort_by(|a,b| b.cmp(a));
    if top_durations.len() > 9 {
        top_durations.pop();
    }
}

fn write_json(output: &mut Option<File>, timing: &LineTiming) {
    match output {
        Some(ref output) => serde_json::to_writer(output, timing).expect("Failed to serialize to json"),
        None => println!("Missing output file for JSON output"),
    }
}

fn wipe_file(output: &str) {
    let mut f = File::create(output).expect("failed to open log file to wipe");
    f.write_all(b"").expect("Failed to wipe file");
}
