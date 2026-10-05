//! Write a folder of made-up trip photos to try shotclock on.
//!
//!     cargo run --release --example make_trip -- /tmp/trip [days] [seed]
//!
//! An iPhone with the right time and its time zone (+09:00), a Fujifilm
//! whose clock is 5 h 37 min behind (set by hand after a battery change),
//! and a Sony that is an hour behind (never moved to daylight saving).

mod common;

use std::path::PathBuf;

use chrono::TimeDelta;
use common::{Rng, jpeg, simulate};

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(dir) = args.first().map(PathBuf::from) else {
        eprintln!("usage: make_trip OUT_DIR [days] [seed]");
        std::process::exit(2);
    };
    let days = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(4);
    let seed = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(7);
    std::fs::create_dir_all(&dir)?;

    let mut rng = Rng::new(seed);
    let trip = simulate(&mut rng, days);
    for (i, s) in trip.phone.iter().enumerate() {
        let bytes = jpeg(s, s.at, "Apple", "iPhone 15", Some(9 * 60), false);
        std::fs::write(dir.join(format!("IMG_{:04}.JPG", 4000 + i)), bytes)?;
    }

    // The two cameras split the camera shots between them: the Sony is the
    // pocket camera for the first half of the trip.
    let fuji = -TimeDelta::minutes(5 * 60 + 37);
    let sony = -TimeDelta::hours(1);
    let half = trip.camera.len() / 2;
    let (mut f, mut s) = (0, 0);
    for (i, shot) in trip.camera.iter().enumerate() {
        if i < half && i % 3 == 0 {
            s += 1;
            let bytes = jpeg(shot, shot.at + sony, "SONY", "DSC-RX100M5A", None, true);
            std::fs::write(dir.join(format!("DSC{:05}.JPG", 1200 + s)), bytes)?;
        } else {
            f += 1;
            let bytes = jpeg(shot, shot.at + fuji, "FUJIFILM", "X-T30", None, true);
            std::fs::write(dir.join(format!("DSCF{:04}.JPG", 3100 + f)), bytes)?;
        }
    }
    println!(
        "Wrote {} phone, {f} Fujifilm and {s} Sony photos to {}.",
        trip.phone.len(),
        dir.display()
    );
    println!("The Fujifilm needs +5h37m and the Sony +1h.");
    Ok(())
}
