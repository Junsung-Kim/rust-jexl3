//! Times the test JSON reader over a fixture file (a debugging aid).
#[path = "../tests/common/json.rs"]
mod json;

fn main() {
    let path = std::env::args().nth(1).expect("a jsonl file");
    let text = std::fs::read_to_string(path).expect("read");
    for (n, line) in text.lines().enumerate() {
        let t = std::time::Instant::now();
        let _ = json::parse(line).expect("parse");
        let ms = t.elapsed().as_millis();
        if ms > 50 {
            println!("line {} took {} ms ({} bytes)", n, ms, line.len());
        }
        if n % 1000 == 0 {
            println!("at line {}", n);
        }
    }
    println!("done");
}
