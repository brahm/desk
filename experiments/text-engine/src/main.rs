// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Lays out one justified, hyphenated paragraph around a circle with
//! Parley and with a minimal engine of our own, checks both, times them
//! and draws them side by side.
//!
//! Run with `cargo run --release -- out.png`.

mod common;
mod own_path;
mod parley_path;
mod render;

use std::time::Instant;

use common::{Laid, check, hyphenate};

fn report(name: &str, laid: &Laid) {
    let hyphens = laid.lines.iter().filter(|l| l.hyphenated).count();
    let beside = laid
        .lines
        .iter()
        .filter(|l| l.width < common::COLUMN_WIDTH)
        .count();
    println!(
        "{name}: {} lines ({beside} beside the circle), {hyphens} hyphenated, height {:.1}pt",
        laid.lines.len(),
        laid.lines.last().map_or(0.0, |l| l.bottom)
    );
    let problems = check(laid);
    if problems.is_empty() {
        println!("  every line fits its piece and stays clear of the circle");
    }
    for problem in problems {
        println!("  problem: {problem}");
    }
}

fn time<T>(runs: u32, mut f: impl FnMut() -> T) -> f64 {
    let start = Instant::now();
    for _ in 0..runs {
        std::hint::black_box(f());
    }
    start.elapsed().as_secs_f64() * 1e6 / f64::from(runs)
}

fn main() {
    let output = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "text-engine.png".into());
    let text = hyphenate(common::TEXT);

    let mut parley = parley_path::Engine::new(parley_path::Breaking::Parley);
    let (parley_laid, passes) = parley.layout(&text);
    let mut hybrid = parley_path::Engine::new(parley_path::Breaking::Ours);
    let (hybrid_laid, _) = hybrid.layout(&text);
    let own = own_path::Engine::new(parley_laid.font.clone());
    let own_laid = own.layout(&text);

    report("Parley, its own breaker", &parley_laid);
    println!("  line breaking ran {passes} times, to make room for hyphens");
    report("Parley, our breaking decision", &hybrid_laid);
    report("Own engine", &own_laid);
    for (name, laid) in [
        ("Parley, its own breaker", &parley_laid),
        ("Parley, our breaking decision", &hybrid_laid),
    ] {
        let narrowed: Vec<_> = laid
            .lines
            .iter()
            .zip(&own_laid.lines)
            .filter(|(a, b)| (a.width - b.width).abs() > 0.5)
            .map(|(a, _)| a.text.trim_end().to_string())
            .collect();
        if !narrowed.is_empty() {
            println!("{name}: lines set narrower than their piece: {narrowed:?}");
        }
    }

    let differing = hybrid_laid
        .lines
        .iter()
        .zip(&own_laid.lines)
        .filter(|(a, b)| a.text.trim_end() != b.text.trim_end())
        .count();
    println!(
        "lines with different text: {differing} of {}",
        hybrid_laid.lines.len().max(own_laid.lines.len())
    );
    for (a, b) in hybrid_laid.lines.iter().zip(&own_laid.lines) {
        let mark = if a.text.trim_end() == b.text.trim_end() {
            ' '
        } else {
            '*'
        };
        println!(
            "{mark} {:>6.1} {:>6.1}  {:<60} | {}",
            a.x,
            a.width,
            a.text.trim_end(),
            b.text.trim_end()
        );
    }

    let runs = 300;
    let parley_us = time(runs, || parley.layout(&text).1);
    let hybrid_us = time(runs, || hybrid.layout(&text).1);
    let own_us = time(runs, || own.layout(&text).lines.len());
    println!(
        "time per paragraph, shaping included: Parley {parley_us:.0} µs, \
         Parley with our breaking {hybrid_us:.0} µs, own engine {own_us:.0} µs"
    );

    render::write_png(
        &output,
        &[
            ("Parley 0.12, its own breaker", &parley_laid),
            ("Parley 0.12, our breaking decision", &hybrid_laid),
            ("Own engine", &own_laid),
        ],
    );
    println!("wrote {output}");
}
