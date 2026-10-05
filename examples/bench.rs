//! How often the suggested offset is right, and how long a big folder takes.
//!
//!     CARGO_PROFILE_RELEASE_STRIP=false cargo run --release --example bench
//!
//! Accuracy runs on 1,000 simulated trips (times only, no files). Timing
//! writes a folder of about 2,000 test photos to a temporary directory,
//! then reads it, suggests an offset and writes corrected copies.

mod common;

use std::time::Instant;

use chrono::{NaiveDateTime, TimeDelta};
use common::{Rng, jpeg, simulate};
use shotclock::suggest::{confidence_word, for_camera, suggest};
use shotclock::write::{Tz, changes, write};
use shotclock::{offset, scan};

const KINDS: [&str; 4] = [
    "any offset up to 20 h",
    "whole hours (time zone)",
    "one hour (daylight saving)",
    "a few minutes (drift)",
];

fn truth(rng: &mut Rng, kind: usize) -> i64 {
    let sign = if rng.chance(0.5) { 1 } else { -1 };
    sign * match kind {
        0 => rng.range(60, 20 * 3600),
        1 => rng.range(2, 12) * 3600,
        2 => 3600,
        _ => rng.range(60, 15 * 60),
    }
}

fn accuracy() {
    let trips = 1000;
    let mut hits = [[0usize; 2]; 4];
    let mut by_word: Vec<(&str, usize, usize)> =
        vec![("high", 0, 0), ("medium", 0, 0), ("low", 0, 0)];
    let mut sizes = Vec::new();
    let mut misses = Vec::new();
    for seed in 0..trips {
        let mut rng = Rng::new(1000 + seed);
        let kind = seed as usize % 4;
        let days = rng.range(1, 7) as u32;
        let trip = simulate(&mut rng, days);
        let t = truth(&mut rng, kind);
        let phone: Vec<NaiveDateTime> = trip.phone.iter().map(|s| s.at).collect();
        let camera: Vec<NaiveDateTime> = trip
            .camera
            .iter()
            .map(|s| s.at - TimeDelta::seconds(t))
            .collect();
        sizes.push(phone.len() + camera.len());
        let s = suggest(&phone, &camera, &[]);
        let hit = s.as_ref().is_some_and(|s| (s.offset - t).abs() <= 60);
        hits[kind][0] += hit as usize;
        hits[kind][1] += 1;
        if let Some(s) = &s {
            let w = by_word
                .iter_mut()
                .find(|w| w.0 == confidence_word(s.confidence))
                .unwrap();
            w.1 += hit as usize;
            w.2 += 1;
            if !hit && misses.len() < 8 {
                misses.push(format!(
                    "seed {}: truth {}, got {} ({} days, {} photos, confidence {:.2})",
                    1000 + seed,
                    offset::format(t),
                    offset::format(s.offset),
                    days,
                    phone.len() + camera.len(),
                    s.confidence
                ));
            }
        }
    }
    sizes.sort_unstable();
    println!(
        "{trips} simulated trips of 1 to 7 days, {} to {} photos each (median {}).",
        sizes[0],
        sizes[sizes.len() - 1],
        sizes[sizes.len() / 2]
    );
    println!("Suggested offset within 1 minute of the truth:");
    let all: usize = hits.iter().map(|h| h[0]).sum();
    for (k, h) in hits.iter().enumerate() {
        println!(
            "  {:28} {:>4} of {:>4}  {:.1}%",
            KINDS[k],
            h[0],
            h[1],
            100.0 * h[0] as f64 / h[1] as f64
        );
    }
    println!(
        "  {:28} {all:>4} of {trips:>4}  {:.1}%",
        "all",
        100.0 * all as f64 / trips as f64
    );
    println!("By the confidence shown:");
    for (w, hit, n) in &by_word {
        let pct = if *n > 0 {
            100.0 * *hit as f64 / *n as f64
        } else {
            0.0
        };
        println!("  {w:7} {n:>4} trips, {pct:.1}% right");
    }
    for m in &misses {
        println!("  miss: {m}");
    }
}

fn timing() -> std::io::Result<()> {
    let tmp = tempfile::tempdir()?;
    let dir = tmp.path().join("trip");
    std::fs::create_dir(&dir)?;
    let mut rng = Rng::new(42);
    let mut trip = simulate(&mut rng, 10);
    while trip.phone.len() + trip.camera.len() < 2000 {
        trip = simulate(&mut rng, 11);
    }
    let shift = TimeDelta::minutes(5 * 60 + 37);
    let mut bytes = 0;
    for (i, s) in trip.phone.iter().enumerate() {
        let b = jpeg(s, s.at, "Apple", "iPhone 15", Some(540), false);
        bytes += b.len();
        std::fs::write(dir.join(format!("IMG_{i:05}.JPG")), b)?;
    }
    for (i, s) in trip.camera.iter().enumerate() {
        let b = jpeg(s, s.at - shift, "FUJIFILM", "X-T30", None, true);
        bytes += b.len();
        std::fs::write(dir.join(format!("DSCF{i:05}.JPG")), b)?;
    }
    let n = trip.phone.len() + trip.camera.len();
    println!(
        "\n{n} photos ({} phone, {} camera, {:.1} MB) in a temporary folder.",
        trip.phone.len(),
        trip.camera.len(),
        bytes as f64 / 1e6
    );
    for run in 1..=3 {
        let t = Instant::now();
        let folder = scan::scan(&dir)?;
        let t_scan = t.elapsed();
        let t = Instant::now();
        let s = for_camera(&folder, 1).expect("a suggestion");
        let t_suggest = t.elapsed();
        let t = Instant::now();
        let out = tmp.path().join(format!("out{run}"));
        let ch = changes(&folder, 1, s.offset, Tz::Phone);
        let w = write(&ch, &out).expect("writing copies");
        let t_write = t.elapsed();
        println!(
            "  run {run}: read EXIF {:.0} ms, suggest {:.0} ms ({}), write and check {} copies {:.0} ms",
            t_scan.as_secs_f64() * 1e3,
            t_suggest.as_secs_f64() * 1e3,
            offset::format(s.offset),
            w.len(),
            t_write.as_secs_f64() * 1e3
        );
    }
    Ok(())
}

fn main() -> std::io::Result<()> {
    accuracy();
    timing()
}
