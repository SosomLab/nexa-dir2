//! 점검 벤치(docs/29 §P-4): VT 파서 처리량 — `cargo run --release -p nexa-term --example audit_vt`.
//! pwsh `ls` 출력을 흉내 낸 SGR 섞인 텍스트 N MB를 feed하고 MB/s·셀 메모리를 출력한다.
//! 기준: ≥ 50 MB/s(ConPTY 출력 상한을 크게 웃돌아 렌더가 병목이 되지 않는 수준).

use std::time::Instant;

fn main() {
    let cols = 240;
    let rows = 50;
    let mut s = nexa_term::VtScreen::new(cols, rows);
    // 한 줄 = 색 있는 표 행 + 한글 + 트루컬러 + 리셋(실제 pwsh/PSStyle 형태)
    let line = "\x1b[32;1mMode\x1b[0m   \x1b[93mls\x1b[0m  d----  2026-09-04  15:19  \x1b[44;1m.cargo\x1b[0m 한글 경로 \x1b[38;2;12;103;169m트루컬러\x1b[0m\r\n";
    let target_mb = 32usize;
    let reps = target_mb * 1024 * 1024 / line.len();
    let chunk: String = line.repeat(64);
    let t0 = Instant::now();
    let mut fed = 0usize;
    for _ in 0..(reps / 64) {
        s.feed(&chunk);
        fed += chunk.len();
    }
    let dt = t0.elapsed();
    let mbps = fed as f64 / 1_048_576.0 / dt.as_secs_f64();
    let cell = std::mem::size_of::<nexa_term::TermCell>();
    println!(
        "fed {:.1} MB in {:.0} ms = {:.1} MB/s · cell {}B · scrollback {} lines ≈ {:.2} MB",
        fed as f64 / 1_048_576.0,
        dt.as_millis(),
        mbps,
        cell,
        s.scrollback_count(),
        (s.scrollback_count() + rows) as f64 * cols as f64 * cell as f64 / 1_048_576.0
    );
    // 견고성: 비정상 시퀀스(거대 파라미터·미완 CSI·NUL·비BMP)가 패닉 없이 소화되는지
    let t1 = Instant::now();
    let nasty = "\x1b[99999999999;1;2;3;4;5;6;7;8;9;10;11;12;13;14;15;16;17;18;19;20m\x1b[\x1b]0;title\x07\x1b[?9999h\0\u{1F600}\x1b[38;5;999m\x1b[48;2;300;300;300mX\x1b[2147483647C\x1b[-5;-5H\x1b[";
    for _ in 0..10_000 {
        s.feed(nasty);
    }
    println!(
        "robustness: 10k nasty sequences ok in {} ms (cursor {},{})",
        t1.elapsed().as_millis(),
        s.cursor_row(),
        s.cursor_col()
    );
}
