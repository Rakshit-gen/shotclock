//! Writing a small EXIF block from scratch, in either byte order. The test
//! trips and the tests use it; shotclock itself never rebuilds a photo's
//! EXIF, it only patches what is there.

use chrono::NaiveDateTime;

use crate::exif::{self, DATE_FORMAT};

pub struct Fields<'a> {
    pub make: &'a str,
    pub model: &'a str,
    pub taken: NaiveDateTime,
    pub tz: Option<i32>,
    pub thumbnail: Option<&'a [u8]>,
    pub little: bool,
}

struct Entry {
    tag: u16,
    typ: u16,
    count: u32,
    data: Vec<u8>,
}

fn ascii(tag: u16, s: &str) -> Entry {
    let mut data = s.as_bytes().to_vec();
    data.push(0);
    Entry {
        tag,
        typ: 2,
        count: data.len() as u32,
        data,
    }
}

fn long(tag: u16, v: u32, little: bool) -> Entry {
    Entry {
        tag,
        typ: 4,
        count: 1,
        data: u32b(v, little).to_vec(),
    }
}

fn u16b(v: u16, little: bool) -> [u8; 2] {
    if little { v.to_le_bytes() } else { v.to_be_bytes() }
}

fn u32b(v: u32, little: bool) -> [u8; 4] {
    if little { v.to_le_bytes() } else { v.to_be_bytes() }
}

/// Size of an IFD plus the out-of-line data that follows it.
fn size(entries: &[Entry]) -> usize {
    let data: usize = entries
        .iter()
        .filter(|e| e.data.len() > 4)
        .map(|e| e.data.len().next_multiple_of(2))
        .sum();
    2 + entries.len() * 12 + 4 + data
}

/// An IFD at TIFF offset `at`, its data right after it.
fn ifd(entries: &[Entry], at: usize, next: u32, little: bool) -> Vec<u8> {
    let mut out = u16b(entries.len() as u16, little).to_vec();
    let mut data = Vec::new();
    let data_at = at + 2 + entries.len() * 12 + 4;
    for e in entries {
        out.extend(u16b(e.tag, little));
        out.extend(u16b(e.typ, little));
        out.extend(u32b(e.count, little));
        if e.data.len() <= 4 {
            let mut v = e.data.clone();
            v.resize(4, 0);
            out.extend(v);
        } else {
            out.extend(u32b((data_at + data.len()) as u32, little));
            data.extend(&e.data);
            if data.len() % 2 == 1 {
                data.push(0);
            }
        }
    }
    out.extend(u32b(next, little));
    out.extend(data);
    out
}

/// A complete APP1 segment, marker and length included.
pub fn app1(f: &Fields) -> Vec<u8> {
    let le = f.little;
    let date = f.taken.format(DATE_FORMAT).to_string();
    // Tags must be in ascending order within each IFD.
    let mut ifd0 = vec![
        ascii(exif::MAKE, f.make),
        ascii(exif::MODEL, f.model),
        ascii(exif::MODIFY_DATE, &date),
        long(exif::EXIF_IFD, 0, le),
    ];
    let mut exif_ifd = vec![
        ascii(exif::DATE_ORIGINAL, &date),
        ascii(exif::CREATE_DATE, &date),
    ];
    if let Some(tz) = f.tz {
        let s = exif::format_tz(tz);
        exif_ifd.push(ascii(exif::OFFSET_TIME, &s));
        exif_ifd.push(ascii(exif::OFFSET_ORIGINAL, &s));
        exif_ifd.push(ascii(exif::OFFSET_DIGITIZED, &s));
    }
    let mut ifd1 = vec![long(exif::THUMB_START, 0, le), long(exif::THUMB_LEN, 0, le)];

    let ifd0_at = 8;
    let exif_at = ifd0_at + size(&ifd0);
    let ifd1_at = exif_at + size(&exif_ifd);
    let thumb_at = ifd1_at + size(&ifd1);
    ifd0[3] = long(exif::EXIF_IFD, exif_at as u32, le);

    let mut tiff = if le { b"II*\0".to_vec() } else { b"MM\0*".to_vec() };
    tiff.extend(u32b(ifd0_at as u32, le));
    let next = if f.thumbnail.is_some() { ifd1_at } else { 0 };
    tiff.extend(ifd(&ifd0, ifd0_at, next as u32, le));
    tiff.extend(ifd(&exif_ifd, exif_at, 0, le));
    if let Some(thumb) = f.thumbnail {
        ifd1[0] = long(exif::THUMB_START, thumb_at as u32, le);
        ifd1[1] = long(exif::THUMB_LEN, thumb.len() as u32, le);
        tiff.extend(ifd(&ifd1, ifd1_at, 0, le));
        tiff.extend(thumb);
    }

    let mut out = vec![0xff, 0xe1];
    out.extend(((tiff.len() + 8) as u16).to_be_bytes());
    out.extend(b"Exif\0\0");
    out.extend(tiff);
    out
}

/// Put an APP1 segment right after the JPEG's start-of-image marker.
pub fn insert(jpeg: &[u8], app1: &[u8]) -> Vec<u8> {
    let mut out = jpeg[..2].to_vec();
    out.extend(app1);
    out.extend(&jpeg[2..]);
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::exif::{Error, read};

    /// A stand-in JPEG: start of image, a fake scan with every byte value
    /// in it, end of image. Enough for the EXIF code, which never decodes
    /// pixels.
    pub fn bare_jpeg() -> Vec<u8> {
        let mut j = vec![0xff, 0xd8, 0xff, 0xda, 0x00, 0x04, 0x01, 0x02];
        j.extend((0..=255u8).cycle().take(2000));
        j.extend([0xff, 0xd9]);
        j
    }

    pub fn photo(little: bool, tz: Option<i32>) -> Vec<u8> {
        let thumb = [0xff, 0xd8, 1, 2, 3, 0xff, 0xd9];
        let f = Fields {
            make: "FUJIFILM",
            model: "X-T30",
            taken: exif::parse_date("2024:07:14 18:03:27").unwrap(),
            tz,
            thumbnail: Some(&thumb),
            little,
        };
        insert(&bare_jpeg(), &app1(&f))
    }

    #[test]
    fn reads_back_in_both_byte_orders() {
        for little in [true, false] {
            let x = read(&photo(little, Some(9 * 60))).unwrap();
            assert_eq!(x.make.as_deref(), Some("FUJIFILM"));
            assert_eq!(x.model.as_deref(), Some("X-T30"));
            assert_eq!(x.taken.unwrap().to_string(), "2024-07-14 18:03:27");
            assert_eq!(x.tz, Some(540));
            assert_eq!(x.dates.len(), 3);
            assert_eq!(x.offsets.len(), 3);
            let bytes = photo(little, None);
            let t = read(&bytes).unwrap().thumbnail.unwrap();
            assert_eq!(&bytes[t], &[0xff, 0xd8, 1, 2, 3, 0xff, 0xd9]);
        }
    }

    #[test]
    fn says_what_is_wrong_with_files_it_cannot_read() {
        assert_eq!(read(b"GIF89a").unwrap_err(), Error::NotJpeg);
        assert_eq!(read(&bare_jpeg()).unwrap_err(), Error::NoExif);
        let mut cut = photo(true, None);
        cut.truncate(40);
        assert!(read(&cut).is_err());
    }

    #[test]
    fn time_zones_round_trip() {
        for (s, m) in [("+09:00", 540), ("-03:30", -210), ("+00:00", 0)] {
            assert_eq!(exif::parse_tz(s), Some(m));
            assert_eq!(exif::format_tz(m), s);
        }
        assert_eq!(exif::parse_tz("   :  "), None);
        assert_eq!(exif::parse_date("0000:00:00 00:00:00"), None);
    }
}
