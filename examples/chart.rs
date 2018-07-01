extern crate fn_time as ft;

fn main() {
    let mut path = std::env::current_dir().unwrap();
    path.push("assets");
    path.push("performance.log");
    ft::display(path).expect("file doesn't exist");
}
