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

/// A patched copy of a JPEG's bytes. With `tz`, the OffsetTime fields are
/// set too, and OffsetTimeOriginal is added if the camera didn't write one.
pub fn patch(bytes: &[u8], shift: TimeDelta, tz: Option<i32>) -> Result<Vec<u8>, Error> {
    let x = exif::read(bytes)?;
    let mut out = bytes.to_vec();
    shift_dates(&mut out, &x, shift);
    let Some(tz) = tz else {
        return Ok(out);
    };
    let value = exif::format_tz(tz);
    for &(_, t) in &x.offsets {
        if t.len >= 6 {
            out[t.at..t.at + 6].copy_from_slice(value.as_bytes());
        }
    }
    if x.offsets
        .iter()
        .any(|(tag, _)| *tag == exif::OFFSET_ORIGINAL)
    {
        return Ok(out);
    }
    add_offset_original(&out, &x, &value)
}

/// Adding a field means a longer Exif IFD, and moving it in place would
/// shift every value after it. Instead the IFD is copied, with the new
/// entry, to the end of the EXIF segment and IFD0's pointer is changed to
/// the copy. Everything else keeps its offset, including maker notes that
/// break if moved. The old IFD stays behind, unused.
fn add_offset_original(bytes: &[u8], x: &Exif, value: &str) -> Result<Vec<u8>, Error> {
    let l = &x.layout;
    let (Some(pointer), Some(ifd)) = (l.exif_pointer, l.exif_ifd) else {
        return Err(Error::Damaged("no Exif IFD to hold a time zone"));
    };
    let r = exif::Reader {
        bytes,
        little: l.little,
        tiff: l.tiff,
        end: l.segment.end,
    };
    let n = r.u16(ifd)? as usize;
    let u16b = |v: u16| {
        if l.little {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
    };
    let u32b = |v: u32| {
        if l.little {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
    };

    // IFDs start on an even offset.
    let pad = (l.segment.end - l.tiff) % 2;
    let new_ifd = l.segment.end + pad - l.tiff;
    let data_at = new_ifd + 2 + (n + 1) * 12 + 4;

    let mut entry = Vec::with_capacity(12);
    entry.extend(u16b(exif::OFFSET_ORIGINAL));
    entry.extend(u16b(2));
    entry.extend(u32b(7));
    entry.extend(u32b(data_at as u32));

    let mut added = vec![0; pad];
    added.extend(u16b(n as u16 + 1));
    let mut placed = false;
    for i in 0..n {
        let e = ifd + 2 + i * 12;
        if !placed && r.u16(e)? > exif::OFFSET_ORIGINAL {
            added.extend(&entry);
            placed = true;
        }
        added.extend(&bytes[e..e + 12]);
    }
    if !placed {
        added.extend(&entry);
    }
    added.extend(&bytes[ifd + 2 + n * 12..ifd + 2 + n * 12 + 4]);
    added.extend(value.as_bytes());
    added.extend([0, 0]);

    let seg_len = l.segment.len() - 2 + added.len();
    if seg_len > u16::MAX as usize {
        return Err(Error::Damaged("no room left in the EXIF segment"));
    }
    let mut out = Vec::with_capacity(bytes.len() + added.len());
    out.extend(&bytes[..l.segment.end]);
    out[l.segment.start + 2..l.segment.start + 4].copy_from_slice(&(seg_len as u16).to_be_bytes());
    out[pointer..pointer + 4].copy_from_slice(&u32b(new_ifd as u32));
    out.extend(added);
    out.extend(&bytes[l.segment.end..]);
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
            let out = patch(&src, shift, None).unwrap();
            assert_eq!(out.len(), src.len());

            let (a, b) = (exif::read(&src).unwrap(), exif::read(&out).unwrap());
            assert_eq!(b.taken.unwrap().to_string(), "2024-07-14 23:40:27");
            assert_eq!(image_hash(&src, &a), image_hash(&out, &b));

            // Only bytes inside the three date strings differ.
            let changed: Vec<usize> = (0..src.len()).filter(|&i| src[i] != out[i]).collect();
            assert!(!changed.is_empty());
            assert!(
                changed
                    .iter()
                    .all(|&i| a.dates.iter().any(|(_, t)| (t.at..t.at + 19).contains(&i)))
            );
        }
    }

    #[test]
    fn crosses_midnight_and_month_ends() {
        let out = patch(&photo(true, None), TimeDelta::days(18), None).unwrap();
        assert_eq!(
            exif::read(&out).unwrap().taken.unwrap().to_string(),
            "2024-08-01 18:03:27"
        );
        let out = patch(&photo(false, None), TimeDelta::hours(-19), None).unwrap();
        assert_eq!(
            exif::read(&out).unwrap().taken.unwrap().to_string(),
            "2024-07-13 23:03:27"
        );
    }

    #[test]
    fn sets_the_time_zone_in_place_when_there_is_one() {
        for little in [true, false] {
            let src = photo(little, Some(60));
            let out = patch(&src, TimeDelta::hours(8), Some(9 * 60)).unwrap();
            assert_eq!(out.len(), src.len());
            let b = exif::read(&out).unwrap();
            assert_eq!(b.tz, Some(540));
            assert_eq!(
                image_hash(&src, &exif::read(&src).unwrap()),
                image_hash(&out, &b)
            );
        }
    }

    #[test]
    fn adds_a_time_zone_when_the_camera_wrote_none() {
        for little in [true, false] {
            let src = photo(little, None);
            let out = patch(&src, TimeDelta::minutes(-30), Some(-210)).unwrap();
            let (a, b) = (exif::read(&src).unwrap(), exif::read(&out).unwrap());
            assert_eq!(b.tz, Some(-210));
            assert_eq!(b.taken.unwrap().to_string(), "2024-07-14 17:33:27");
            assert_eq!(b.make.as_deref(), Some("FUJIFILM"));
            assert_eq!(
                &out[b.thumbnail.clone().unwrap()],
                &src[a.thumbnail.clone().unwrap()]
            );
            assert_eq!(image_hash(&src, &a), image_hash(&out, &b));
            // A second pass finds the added field and changes it in place.
            let again = patch(&out, TimeDelta::zero(), Some(60)).unwrap();
            assert_eq!(again.len(), out.len());
            assert_eq!(exif::read(&again).unwrap().tz, Some(60));
        }
    }
}
