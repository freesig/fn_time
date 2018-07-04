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

type Times = Vec<(u32, Duration)>;
type Counts = Vec<Counter>;

struct TimeMsg {
    times: Times,
    counts: Counts,
}

pub struct Settings {
    pub max_timers: usize, 
    pub max_counters: usize, 
    pub average_length: u64, 
    pub num_top: usize,
    pub output: Output,
}

pub struct Spot{
    line: u32,
    when: Instant,
}


pub struct FnTimer{
    spots: Vec<Option<Spot>>,
    counters: Vec<Option<Counter>>,
    last_inst: Option<Instant>,
    current_spot: usize,
    current_counter: usize,
}

pub struct Collector{
    tx: Sender<TimeMsg>,
}

pub struct Stats{
    rx: Receiver<TimeMsg>,
    kill_rx: Option<Receiver<KillMsg>>,
    tops: HashMap<u32, Vec<Duration>>,
    visits: u32,
    total_time: Duration,
    count: u64,
    reset_at: u64,
    totals: HashMap<u32, Duration>,
    output: Output,
    num_top: usize,
}

pub enum Output {
    Print,
    JSON(String),
}

struct KillMsg;

pub struct StatsHandle{
    tx: Sender<KillMsg>,
    loop_tx: Sender<KillMsg>,
    handle: thread::JoinHandle<()>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LineTiming {
    pub line_number: u32,
    pub top_durations: Vec<(Duration, f64)>,
    pub average_of_line: (Duration, f64),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Counter {
    pub line: u32,
    pub n: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StreamData{
    pub capture: Vec<LineTiming>,
    pub count: Vec<Counter>,
}

pub fn new(settings: Settings) -> (Collector, FnTimer, Stats) {
    let Settings {
        max_timers,
        max_counters,
        average_length,
        num_top,
        output } = settings;
    let (tx, rx) = mpsc::channel();
    if let Output::JSON(ref s) = output {
        wipe_file(&s[..]);
    }
    (Collector{ tx }, 
     FnTimer::new(max_timers, max_counters),
     Stats{ rx , kill_rx: None,
     tops: HashMap::new(), 
         visits: 0, total_time: Duration::new(0, 0), 
         count: 0, reset_at: average_length,
         totals: HashMap::new(),
         output,
         num_top,
     } )
}

// Convenience function for threading
pub fn run_stats(mut stats: Stats) -> StatsHandle {
    let (tx, rx) = mpsc::channel();
    let (loop_tx, loop_rx) = mpsc::channel();
    stats.kill_rx = Some(loop_rx);
    let handle = thread::spawn(move || {
        loop{
            match rx.try_recv() {
                Ok(KillMsg) => break,
                Err(_)=> stats.top(),
            }
        }
    });
    StatsHandle{ tx, loop_tx, handle }
}

impl StatsHandle {
    pub fn stop(self) {
        self.tx.send(KillMsg).expect("Failed to send kill message to stats thread");
        self.loop_tx.send(KillMsg).expect("Failed to send kill message to stats loop thread");
        self.handle.join().expect("Failed to join stats thread");
    }
}

impl Spot{
    pub fn capture(line: u32) -> Self{
        let when = Instant::now();
        Spot{ line, when }
    }
}

impl FnTimer{
    pub fn new(num_spots: usize, num_counters: usize) -> Self {
        let mut spots: Vec<Option<Spot>> = Vec::with_capacity(num_spots);
        let mut counters: Vec<Option<Counter>> = Vec::with_capacity(num_counters);
        for _i in 0..num_spots {
            spots.push(None);
        }
        for _i in 0..num_counters{
            counters.push(None);
        }
        FnTimer{ spots, counters, last_inst: None, current_spot: 0, current_counter: 0 }
    }

    pub fn make_durations(&mut self) -> Times {
        let mut times: Times = Vec::new();
        for s in &self.spots {
            match s {
                &Some(ref s) =>{
                    let duration = self.last_inst.map_or_else(
                        || Duration::new(0, 0),
                        |li| s.when.duration_since(li));
                    times.push((s.line, duration));
                    self.last_inst = Some(s.when);
                },
                &None =>(),
            }
        }
        self.last_inst = None;
        self.current_spot = 0;
        times
    }

    pub fn make_counts(&mut self) -> Counts {
        let mut counts: Counts = Vec::new();
        for c in &self.counters {
            if let &Some(c) = c {
                counts.push(c);
            }
        }
        self.current_counter = 0;
        counts
    }

    pub fn capture(&mut self, line: u32) {
        self.spots[self.current_spot] = Some(Spot{ line, when: Instant::now() });
        self.current_spot += 1;
    }

    pub fn counter(&mut self, line: u32, n: usize){
        self.counters[self.current_counter] = Some(Counter{ line, n });
        self.current_counter += 1;
    }
}

impl Collector{
    pub fn send(&self, fnt: &mut FnTimer){
        let times = fnt.make_durations();
        let counts = fnt.make_counts();
        let msg = TimeMsg{ times, counts };
        self.tx.send(msg).unwrap_or_else(|e| {
            println!("fn_time failed to send data from timed function because: {}", e) });
    }
}

impl Stats{
    pub fn top(&mut self){
        let &mut Stats {
            ref rx,
            ref kill_rx,
            ref mut tops,
            ref mut visits,
            ref mut total_time,
            ref mut count,
            ref reset_at,
            ref mut totals,
            ref output,
            ref num_top,
        } = self;
        let mut output_queue: VecDeque<StreamData> = VecDeque::new();
        let mut json_output = setup_json(output);


        for TimeMsg{ times, counts } in rx.iter() {
            match *kill_rx {
                Some(ref m) => {
                    if let Ok(KillMsg) = m.try_recv() { break; }
                },
                _ => (),
            }

            let line_timings = create_line_timing(
                times,
                total_time,
                visits,
                tops,
                totals,
                num_top);

            output_queue.push_back(StreamData{ capture: line_timings, count: counts });
            *count += 1;
            if *count > *reset_at {
                *tops = HashMap::new();
                *count = 0;

                // Only output at reset
                loop {
                    match output_queue.pop_front() {
                        Some(s) => {
                            match *output {
                                Output::JSON(_) => write_json(&mut json_output, s),
                                Output::Print => println!("{}", s),
                            }
                        },
                        None => break,
                    }
                }
            }
        }
    }


}
fn create_line_timing(
    times: Times, 
    total_time: &mut Duration, 
    visits: &mut u32, 
    tops: &mut HashMap<u32, Vec<Duration>>,
    totals: &mut HashMap<u32, Duration>,
    num_top: &usize 
    ) -> Vec<LineTiming> {
    // Average duration of function
    let average = total_time.checked_div(*visits);
    *visits += 1;

    let mut timings = Vec::<LineTiming>::new();

    for &(line, duration) in times.iter() {
        *total_time += duration;
        // Insert line in tops if it doesn't exist
        if !tops.contains_key(&line) {
            tops.insert(line, vec![Duration::new(0,0); 10]);
        }
        if !totals.contains_key(&line) {
            totals.insert(line, Duration::new(0,0));
        }
        let total_of_line = totals.get_mut(&line).unwrap();
        *total_of_line += duration;
        let average_of_line = total_of_line.checked_div(*visits).unwrap_or_else(||Duration::new(0, 0));
        let mut top_durations = tops.get_mut(&line).expect("Top durations missing");
        add_duration(top_durations, &duration, *num_top);

        if let Some(av) = average {
            let av = duration_to_nano(&av);
            let durations = top_durations.iter().map(|&top|{
                let top_nano = duration_to_nano(&top);
                if av > 0 {
                    (top, top_nano as f64 / av as f64 * 100.0)
                }else{ (top, 0.0) }
            }).collect();
            let average_of_line = if av > 0 {
                let av_nano = duration_to_nano(&average_of_line);
                (average_of_line, av_nano as f64 / av as f64 * 100.0)
            }else{ (average_of_line, 0.0) };

            let timing = LineTiming{ 
                line_number: line,
                top_durations: durations,
                average_of_line,
            };
            timings.push(timing);

        }
    }
    timings
}
fn setup_json(output: &Output) -> Option<File> {
    // Create output file if needed
    match *output {
        Output::JSON(ref s) => {
            let mut path = std::env::current_dir().unwrap();
            path.push(s);
            match OpenOptions::new().append(true).create(true).open(path) {
                Ok(o) => Some(o),
                Err(e) => {
                    println!("Couldn't open JSON file {}", e);
                    None
                },
            }
        },
        Output::Print => None,
    }
}

impl fmt::Display for LineTiming {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Line number: {}", self.line_number)?;
        for &(top, p) in &self.top_durations {
            writeln!(f, "Duration: {:?}", top)?;
            writeln!(f, "Percentage of top: {:.*}%", 1, p)?;
        }
        write!(f, "Average duration of line: {:?}", self.average_of_line)
    }
}

impl fmt::Display for Counter {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Line number: {}", self.line)?;
        write!(f, "Count: {}", self.n)
    }
}

impl fmt::Display for StreamData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for line in &self.capture {
            writeln!(f, "{}", line)?;
        }
        for c in &self.count {
            writeln!(f, "{}", c)?;
        }
        Ok(())
    }
}

fn duration_to_nano(dur: &Duration) -> u32 {
    (dur.as_secs() as u32 * 1_000_000_000) + dur.subsec_nanos()
}

fn add_duration(top_durations: &mut Vec<Duration>, duration: &Duration, max_durations: usize) {
    top_durations.push(*duration);
    top_durations.sort_by(|a,b| b.cmp(a));
    if top_durations.len() >= max_durations {
        top_durations.pop();
    }
}

fn write_json(output: &mut Option<File>, stream_data: StreamData) {
    match output {
        &mut Some(ref output) => serde_json::to_writer(output, &stream_data).expect("Failed to serialize to json"),
        &mut None => println!("Missing output file for JSON output"),
    }
}

fn wipe_file(output: &str) {
    let mut f = File::create(output).expect("failed to open log file to wipe");
    f.write_all(b"").expect("Failed to wipe file");
}
