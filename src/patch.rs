//! Changing the dates in a copy of a JPEG without touching anything else.
//!
//! An EXIF date is always 19 characters, so a shifted date fits exactly
//! where the old one was. Nothing moves, no offsets change, and the image
//! data is copied byte for byte.

use chrono::TimeDelta;
use sha2::{Digest, Sha256};

use crate::exif::{self, DATE_FORMAT, Error, Exif};

/// SHA-256 of every byte outside the EXIF segment: the picture itself and
/// all other metadata. A correct patch leaves it unchanged.
pub fn image_hash(bytes: &[u8], x: &Exif) -> String {
    let seg = &x.layout.segment;
    let mut h = Sha256::new();
    h.update(&bytes[..seg.start]);
    h.update(&bytes[seg.end..]);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Shift every readable date field by `shift`. Fields that hold no valid
/// date (an unset clock writes blanks) are left alone.
pub fn shift_dates(bytes: &mut [u8], x: &Exif, shift: TimeDelta) {
    for &(_, t) in &x.dates {
        if t.len < 19 {
            continue;
        }
        let old = String::from_utf8_lossy(&bytes[t.at..t.at + 19]).into_owned();
        let Some(d) = exif::parse_date(&old) else {
            continue;
        };
        let Some(new) = d.checked_add_signed(shift) else {
            continue;
        };
        let new = new.format(DATE_FORMAT).to_string();
        // Years past 9999 would not fit; the caller's offsets never get there.
        if new.len() == 19 {
            bytes[t.at..t.at + 19].copy_from_slice(new.as_bytes());
        }
    }
}

/// A patched copy of a JPEG's bytes.
pub fn patch(bytes: &[u8], shift: TimeDelta) -> Result<Vec<u8>, Error> {
    let x = exif::read(bytes)?;
    let mut out = bytes.to_vec();
    shift_dates(&mut out, &x, shift);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::tests::photo;

    #[test]
    fn moves_every_date_and_nothing_else() {
        for little in [true, false] {
            let src = photo(little, None);
            let shift = TimeDelta::minutes(5 * 60 + 37);
            let out = patch(&src, shift).unwrap();
            assert_eq!(out.len(), src.len());

            let (a, b) = (exif::read(&src).unwrap(), exif::read(&out).unwrap());
            assert_eq!(b.taken.unwrap().to_string(), "2024-07-14 23:40:27");
            assert_eq!(image_hash(&src, &a), image_hash(&out, &b));

            // Only bytes inside the three date strings differ.
            let changed: Vec<usize> = (0..src.len()).filter(|&i| src[i] != out[i]).collect();
            assert!(!changed.is_empty());
            assert!(changed.iter().all(|&i| a.dates.iter().any(|(_, t)| (t.at..t.at + 19).contains(&i))));
        }
    }

    #[test]
    fn crosses_midnight_and_month_ends() {
        let out = patch(&photo(true, None), TimeDelta::days(18)).unwrap();
        assert_eq!(exif::read(&out).unwrap().taken.unwrap().to_string(), "2024-08-01 18:03:27");
        let out = patch(&photo(false, None), TimeDelta::hours(-19)).unwrap();
        assert_eq!(exif::read(&out).unwrap().taken.unwrap().to_string(), "2024-07-13 23:03:27");
    }
}
