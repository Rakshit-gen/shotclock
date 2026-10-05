//! Guessing a camera's clock offset by lining its bursts of shooting up
//! with the phone's.
//!
//! People reach for the camera and the phone at the same moments: a view,
//! a meal, a sunset. So the camera's photo counts per minute, slid along
//! in time, should line up with the phone's at one offset much better than
//! at any other.

use chrono::NaiveDateTime;
use serde::Serialize;

use crate::scan::{Camera, Folder};
use crate::write::nearest_tz;

/// How far either way the coarse search looks, in minutes.
const WINDOW: i64 = 24 * 60;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Suggestion {
    /// Seconds to add to the camera's times.
    pub offset: i64,
    /// 0 to 1: how much better this offset fits than the next best one,
    /// times the share of camera photos that land near a phone photo.
    pub confidence: f64,
    /// Share of camera photos within 3 minutes of a phone photo after the
    /// shift.
    pub matched: f64,
    /// The best offset more than 15 minutes away from this one.
    pub runner_up: Option<i64>,
    /// Set when the offset was rounded to a likely value, saying why.
    pub note: Option<String>,
}

fn secs(t: &NaiveDateTime) -> i64 {
    t.and_utc().timestamp()
}

fn median(v: &mut [i64]) -> i64 {
    v.sort_unstable();
    v[v.len() / 2]
}

/// Photos per minute, squashed so one 40-frame burst doesn't outweigh
/// several moments with a few photos each.
fn per_minute(times: &[i64], start: i64, len: usize) -> Vec<f64> {
    let mut counts = vec![0u32; len];
    for t in times {
        let i = (t.div_euclid(60) - start) as usize;
        if let Some(c) = counts.get_mut(i) {
            *c += 1;
        }
    }
    counts.iter().map(|&c| (c as f64).ln_1p()).collect()
}

/// Seconds from `t` to the nearest time in a sorted list.
fn gap(sorted: &[i64], t: i64) -> i64 {
    let i = sorted.partition_point(|&x| x < t);
    let after = sorted.get(i).map(|x| x - t);
    let before = i.checked_sub(1).map(|j| t - sorted[j]);
    after.into_iter().chain(before).min().unwrap_or(i64::MAX)
}

/// How well camera photos shifted by `offset` seconds sit next to phone
/// photos, counting a photo fully when it is within seconds of one and
/// fading out over a couple of minutes.
fn fine_score(phone: &[i64], camera: &[i64], offset: i64) -> f64 {
    camera
        .iter()
        .map(|&t| {
            let g = gap(phone, t + offset) as f64 / 45.0;
            (-g * g).exp()
        })
        .sum()
}

/// `hints` are offsets in seconds worth preferring when they fit nearly as
/// well as the best one, such as the gap between the time zones the two
/// devices recorded.
pub fn suggest(phone: &[NaiveDateTime], camera: &[NaiveDateTime], hints: &[i64]) -> Option<Suggestion> {
    if phone.is_empty() || camera.is_empty() {
        return None;
    }
    let mut p: Vec<i64> = phone.iter().map(secs).collect();
    let mut c: Vec<i64> = camera.iter().map(secs).collect();
    // A camera reset after a battery change can be years out. Centre the
    // search on the gap between the middles of the two sets so that case
    // still lands inside the window.
    let center = {
        let d = median(&mut p) - median(&mut c);
        if d.abs() > WINDOW * 60 { d.div_euclid(60) } else { 0 }
    };

    // Coarse: correlate per-minute activity at every whole-minute offset.
    // The phone side is blurred over five minutes, since the two devices
    // never fire at quite the same second.
    let p_start = p[0].div_euclid(60) - 2;
    let p_len = (p[p.len() - 1].div_euclid(60) - p_start + 3) as usize;
    let pm = per_minute(&p, p_start, p_len);
    let mut blurred = vec![0.0; p_len];
    for (i, v) in pm.iter().enumerate().filter(|(_, v)| **v > 0.0) {
        for (k, w) in [(-2, 0.25), (-1, 0.75), (0, 1.0), (1, 0.75), (2, 0.25)] {
            if let Some(b) = blurred.get_mut((i as i64 + k) as usize) {
                *b += v * w;
            }
        }
    }
    let c_start = c[0].div_euclid(60);
    let c_len = (c[c.len() - 1].div_euclid(60) - c_start + 1) as usize;
    let cm: Vec<(i64, f64)> = per_minute(&c, c_start, c_len)
        .into_iter()
        .enumerate()
        .filter(|(_, v)| *v > 0.0)
        .map(|(i, v)| (c_start + i as i64, v))
        .collect();

    let mut scores = Vec::with_capacity(2 * WINDOW as usize + 1);
    for d in center - WINDOW..=center + WINDOW {
        let mut s = 0.0;
        for &(m, v) in &cm {
            let i = m + d - p_start;
            if i >= 0 && (i as usize) < p_len {
                s += v * blurred[i as usize];
            }
        }
        scores.push((d, s));
    }
    let &(best, top) = scores.iter().max_by(|a, b| a.1.total_cmp(&b.1))?;
    if top <= 0.0 {
        return None;
    }
    let runner = scores
        .iter()
        .filter(|(d, _)| (d - best).abs() > 15)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .filter(|r| r.1 > 0.0);

    // Fine: search seconds around the best minute.
    p.sort_unstable();
    let fine = |o: i64| fine_score(&p, &c, o);
    let mut offset = best * 60;
    let mut fine_top = f64::MIN;
    for o in (best * 60 - 120)..=(best * 60 + 120) {
        let s = fine(o);
        if s > fine_top {
            fine_top = s;
            offset = o;
        }
    }

    // Clocks are most often off by whole hours: a time zone not changed, or
    // daylight saving. A camera and a phone rarely fire within the same
    // half minute, so a whole number of hours (or a hint) a minute or two
    // from the best fit, and scoring nearly as well, is the likelier truth.
    let mut note = None;
    let hour = (offset as f64 / 3600.0).round() as i64 * 3600;
    let near = |o: i64| (o - offset).abs() <= 3 * 60 && fine(o) >= 0.75 * fine_top;
    if let Some(&h) = hints.iter().find(|&&h| near(h)) {
        offset = h;
        note = Some("matches the gap between the time zones the two devices recorded".into());
    } else if hour != offset && near(hour) {
        offset = hour;
        note = Some(if hour.abs() == 3600 {
            "a whole hour, which is usually daylight saving time".into()
        } else {
            "a whole number of hours, which usually means the camera was set to another time zone".into()
        });
    }

    let matched = c.iter().filter(|&&t| gap(&p, t + offset) <= 180).count() as f64 / c.len() as f64;
    let margin = runner.map_or(1.0, |r| 1.0 - r.1 / top);
    Some(Suggestion {
        offset,
        confidence: (margin * matched).clamp(0.0, 1.0),
        matched,
        runner_up: runner.map(|r| r.0 * 60),
        note,
    })
}

/// "high", "medium" or "low", for people.
pub fn confidence_word(c: f64) -> &'static str {
    if c >= 0.5 {
        "high"
    } else if c >= 0.25 {
        "medium"
    } else {
        "low"
    }
}

/// Suggest an offset for one camera of a scanned folder against its
/// reference camera, using any time zones both recorded as hints.
pub fn for_camera(folder: &Folder, camera: usize) -> Option<Suggestion> {
    let reference = folder.cameras.iter().find(|c| c.reference)?;
    let cam = &folder.cameras[camera];
    if cam.reference {
        return None;
    }
    let mut hints: Vec<i64> = cam
        .photos
        .iter()
        .filter_map(|p| Some((nearest_tz(reference, p.taken)? - p.tz?) as i64 * 60))
        .collect();
    hints.sort_unstable();
    hints.dedup();
    let times = |c: &Camera| c.photos.iter().map(|p| p.taken).collect::<Vec<_>>();
    suggest(&times(reference), &times(cam), &hints)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    fn at(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    /// Ten moments over a day; both devices shoot at each, a little apart.
    fn trip(offset: TimeDelta) -> (Vec<NaiveDateTime>, Vec<NaiveDateTime>) {
        let start = at("2024-07-14 08:00:00");
        let moments = [0, 47, 95, 180, 222, 300, 371, 460, 545, 610];
        let (mut phone, mut camera) = (Vec::new(), Vec::new());
        for (i, m) in moments.iter().enumerate() {
            let t = start + TimeDelta::minutes(*m);
            for k in 0..(2 + i % 3) {
                phone.push(t + TimeDelta::seconds(20 * k as i64));
            }
            for k in 0..(3 + i % 4) {
                camera.push(t + TimeDelta::seconds(35 + 4 * k as i64) - offset);
            }
        }
        (phone, camera)
    }

    #[test]
    fn finds_an_odd_offset_to_the_minute() {
        let truth = TimeDelta::minutes(5 * 60 + 37);
        let (phone, camera) = trip(truth);
        let s = suggest(&phone, &camera, &[]).unwrap();
        assert!((s.offset - truth.num_seconds()).abs() <= 60, "{s:?}");
        assert!(s.confidence > 0.5, "{s:?}");
        assert_eq!(s.matched, 1.0);
    }

    #[test]
    fn finds_offsets_either_way_and_years_out() {
        for truth in [TimeDelta::hours(-9), TimeDelta::minutes(3), TimeDelta::days(3000)] {
            let (phone, camera) = trip(truth);
            let s = suggest(&phone, &camera, &[]).unwrap();
            assert!((s.offset - truth.num_seconds()).abs() <= 60, "{truth} {s:?}");
        }
    }

    #[test]
    fn gives_up_without_overlap() {
        assert_eq!(suggest(&[], &[at("2024-07-14 08:00:00")], &[]), None);
    }

    #[test]
    fn rounds_to_whole_hours_and_hints_only_when_they_fit() {
        // Camera shots land 35 s after the phone's, so the raw fit is off
        // the hour by about that much.
        let (phone, camera) = trip(TimeDelta::hours(-1));
        let s = suggest(&phone, &camera, &[]).unwrap();
        assert_eq!(s.offset, -3600);
        assert!(s.note.unwrap().contains("daylight"));

        let (phone, camera) = trip(TimeDelta::hours(8));
        let s = suggest(&phone, &camera, &[8 * 3600]).unwrap();
        assert_eq!(s.offset, 8 * 3600);
        assert!(s.note.unwrap().contains("time zones"));

        // Seven minutes off the hour is a real drift, not a time zone.
        let truth = TimeDelta::minutes(2 * 60 + 7);
        let (phone, camera) = trip(truth);
        let s = suggest(&phone, &camera, &[2 * 3600]).unwrap();
        assert!((s.offset - truth.num_seconds()).abs() <= 60, "{s:?}");
        assert_eq!(s.note, None);
    }
}
