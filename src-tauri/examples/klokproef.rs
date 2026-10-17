//! Zet de klok via dezelfde code als de app: `cargo run --example klokproef -- 44100 256`,
//! of zonder argumenten terug naar Auto.
use cachymix_lib::klok::{self, Klok};

fn main() {
    let a: Vec<u32> = std::env::args().skip(1).filter_map(|s| s.parse().ok()).collect();
    let k = Klok { rate: a.first().copied(), buffer: a.get(1).copied() };
    klok::zet(&k).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1500));
    println!("{:?}", klok::lees().unwrap());
}
