//! Leest status en meters, zonder iets te schrijven: `cargo run --example peil`.
use cachymix_lib::{ff400, firewire::Node};

fn main() {
    let g = ff400::zoek().expect("geen Fireface 400 gevonden");
    println!("node {:?}, hwdep {:?}", g.fw, g.hwdep);
    let mut node = Node::open(&g.fw).expect("openen mislukt");
    println!("{:#?}", ff400::lees_status(&mut node).unwrap());
    let m = ff400::lees_meters(&mut node).unwrap();
    let db = |v: &Vec<f32>| v.iter().map(|x| if *x > 0.0 { format!("{:.0}", 20.0 * x.log10()) } else { "-".into() }).collect::<Vec<_>>().join(" ");
    println!("in   {}\nplay {}\nuit  {}", db(&m.ingangen), db(&m.playback), db(&m.uitgangen));
}
