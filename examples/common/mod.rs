//! Simulated trips: a phone with the right time and a camera whose clock is
//! off by a known amount, shooting at the same moments the way people do.
//! No real photos are involved.

#![allow(dead_code)]

use chrono::{NaiveDate, NaiveDateTime, TimeDelta};
use jpeg_encoder::{ColorType, Encoder};
use shotclock::build::{self, Fields};

/// splitmix64: small, fast and the same on every machine, so a seed always
/// gives the same trip.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in lo..=hi.
    pub fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }

    pub fn chance(&mut self, p: f64) -> bool {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64 <= p
    }
}

#[derive(Clone)]
pub struct Shot {
    /// The true local time on the trip.
    pub at: NaiveDateTime,
    /// Which moment of the trip it belongs to, so test images can look
    /// alike for the same scene.
    pub scene: u32,
}

pub struct Trip {
    pub phone: Vec<Shot>,
    pub camera: Vec<Shot>,
}

/// A trip of `days` days. Each day has 6 to 18 moments worth a photo
/// between 8:00 and 22:00. At about half of them both the phone and the
/// camera come out; at the rest only one does. Cameras shoot in bursts,
/// phones take a few frames. The phone also takes stray photos (a menu, a
/// ticket) that the camera never sees.
pub fn simulate(rng: &mut Rng, days: u32) -> Trip {
    let start = NaiveDate::from_ymd_opt(2024, 10, 3)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let mut trip = Trip {
        phone: Vec::new(),
        camera: Vec::new(),
    };
    let mut scene = 0;
    for day in 0..days as i64 {
        let day_start = start + TimeDelta::days(day);
        let mut moments: Vec<i64> = (0..rng.range(6, 18))
            .map(|_| rng.range(8 * 3600, 22 * 3600))
            .collect();
        moments.sort_unstable();
        for m in moments {
            scene += 1;
            let t = day_start + TimeDelta::seconds(m);
            let roll = rng.range(0, 99);
            let (phone, camera) = (roll < 85, roll >= 30);
            if phone {
                let mut at = t + TimeDelta::seconds(rng.range(-60, 60));
                for _ in 0..rng.range(1, 6) {
                    trip.phone.push(Shot { at, scene });
                    at += TimeDelta::seconds(rng.range(2, 25));
                }
            }
            if camera {
                let mut at = t + TimeDelta::seconds(rng.range(-60, 60));
                for _ in 0..rng.range(2, 25) {
                    trip.camera.push(Shot { at, scene });
                    // Mostly quick frames, with the odd pause to recompose.
                    at += TimeDelta::seconds(if rng.chance(0.15) {
                        rng.range(20, 90)
                    } else {
                        rng.range(0, 4)
                    });
                }
            }
        }
        for _ in 0..rng.range(3, 8) {
            scene += 1;
            let at = day_start + TimeDelta::seconds(rng.range(7 * 3600, 23 * 3600));
            trip.phone.push(Shot { at, scene });
        }
    }
    trip.phone.sort_by_key(|s| s.at);
    trip.camera.sort_by_key(|s| s.at);
    trip
}

/// A small picture: sky colour from the time of day, and a block whose hue
/// comes from the scene, so photos of one moment look alike in a lane.
fn pixels(at: NaiveDateTime, scene: u32, w: usize, h: usize) -> Vec<u8> {
    use chrono::Timelike;
    let hour = at.hour() as f64 + at.minute() as f64 / 60.0;
    // Bright at midday, warm towards dusk, dark at night.
    let day = (1.0 - ((hour - 13.0) / 8.0).powi(2)).clamp(0.08, 1.0);
    let dusk = (1.0 - ((hour - 19.0) / 2.0).powi(2)).clamp(0.0, 1.0);
    let sky = [
        120.0 * day + 120.0 * dusk,
        160.0 * day + 50.0 * dusk,
        220.0 * day + 10.0 * dusk,
    ];
    let hue = (scene.wrapping_mul(2_654_435_761) >> 8) as f64 / (1u32 << 24) as f64 * 6.0;
    let c = |k: f64| {
        let x = ((hue + k) % 6.0 - 3.0).abs() - 1.0;
        (x.clamp(0.0, 1.0) * 170.0 + 40.0) * (0.4 + 0.6 * day)
    };
    let block = [c(0.0), c(4.0), c(2.0)];
    let ground = [70.0 * day + 20.0, 90.0 * day + 25.0, 60.0 * day + 20.0];
    let mut out = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f64 / w as f64, y as f64 / h as f64);
            let px = if fy > 0.68 {
                ground
            } else if (0.3..0.62).contains(&fx) && fy > 0.3 {
                block
            } else {
                let t = 1.0 - fy * 0.4;
                [sky[0] * t, sky[1] * t, sky[2] * t]
            };
            out.extend(px.map(|v| v.clamp(0.0, 255.0) as u8));
        }
    }
    out
}

fn encode(rgb: &[u8], w: u16, h: u16, quality: u8) -> Vec<u8> {
    let mut out = Vec::new();
    Encoder::new(&mut out, quality)
        .encode(rgb, w, h, ColorType::Rgb)
        .expect("encoding a test image");
    out
}

/// A JPEG with EXIF: what the camera's clock said, the camera's name, an
/// optional time zone and a 160 by 120 thumbnail.
pub fn jpeg(
    shot: &Shot,
    clock: NaiveDateTime,
    make: &str,
    model: &str,
    tz: Option<i32>,
    little: bool,
) -> Vec<u8> {
    let thumb = encode(&pixels(shot.at, shot.scene, 160, 120), 160, 120, 70);
    let main = encode(&pixels(shot.at, shot.scene, 480, 360), 480, 360, 75);
    let app1 = build::app1(&Fields {
        make,
        model,
        taken: clock,
        tz,
        thumbnail: Some(&thumb),
        little,
    });
    build::insert(&main, &app1)
}
