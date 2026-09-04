//! 점검 벤치(docs/29 §P-1): 폴더 열거 시간 — `cargo run --release -p nexa-vfs --example audit_enum -- <dir> [반복]`.
//! `read_dir_entries`(스트리밍 열거 + 메타데이터)로 전 엔트리를 수집해 건수·소요·건당 µs를 출력한다.
//! 기준(docs/18 P1): 100k 첫 렌더 <150ms 중 열거 몫 — 10k ≤ 40ms · 100k ≤ 400ms(SSD 로컬, 캐시 온).

use std::time::Instant;

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let reps: usize = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(3);
    let mut times = Vec::new();
    let mut count = 0usize;
    let mut errors = 0usize;
    for _ in 0..reps {
        let t0 = Instant::now();
        count = 0;
        errors = 0;
        match nexa_vfs::read_dir_entries(&dir) {
            Ok(it) => {
                for e in it {
                    match e {
                        Ok(_) => count += 1,
                        Err(_) => errors += 1,
                    }
                }
            }
            Err(e) => {
                eprintln!("read_dir failed: {e}");
                std::process::exit(2);
            }
        }
        times.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let med = times[times.len() / 2];
    println!(
        "{dir}: {count} entries ({errors} errors) · median {med:.1} ms · {:.2} µs/entry · runs {:?}",
        med * 1000.0 / count.max(1) as f64,
        times.iter().map(|t| format!("{t:.1}")).collect::<Vec<_>>()
    );
}
