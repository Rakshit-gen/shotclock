//! Reading the few EXIF fields shotclock cares about from a JPEG, and where
//! they sit in the file so they can be patched in place later.

use std::ops::Range;

use chrono::NaiveDateTime;

pub const MAKE: u16 = 0x010f;
pub const MODEL: u16 = 0x0110;
pub const MODIFY_DATE: u16 = 0x0132;
pub const EXIF_IFD: u16 = 0x8769;
pub const THUMB_START: u16 = 0x0201;
pub const THUMB_LEN: u16 = 0x0202;
pub const DATE_ORIGINAL: u16 = 0x9003;
pub const CREATE_DATE: u16 = 0x9004;
pub const OFFSET_TIME: u16 = 0x9010;
pub const OFFSET_ORIGINAL: u16 = 0x9011;
pub const OFFSET_DIGITIZED: u16 = 0x9012;
pub const SERIAL: u16 = 0xa431;

pub const DATE_FORMAT: &str = "%Y:%m:%d %H:%M:%S";

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum Error {
    #[error("not a JPEG file")]
    NotJpeg,
    #[error("the file has no EXIF data")]
    NoExif,
    #[error("the EXIF data is damaged ({0})")]
    Damaged(&'static str),
}

/// An ASCII value inside the file: where its bytes start and how many there
/// are, including the trailing NUL.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Text {
    pub at: usize,
    pub len: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Exif {
    pub make: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    /// DateTimeOriginal, falling back to CreateDate and then the IFD0 date,
    /// because some cameras only fill one of them.
    pub taken: Option<NaiveDateTime>,
    /// OffsetTimeOriginal in minutes east of UTC.
    pub tz: Option<i32>,
    /// The embedded preview JPEG, as a byte range of the file.
    pub thumbnail: Option<Range<usize>>,
    /// Every date field found, by tag, for patching.
    pub dates: Vec<(u16, Text)>,
    /// Every OffsetTime* field found, by tag.
    pub offsets: Vec<(u16, Text)>,
    pub layout: Layout,
}

/// Where the EXIF block is, which the writer needs to add a field.
#[derive(Debug, Clone, Default)]
pub struct Layout {
    /// The APP1 segment, from its 0xFF marker to its last byte.
    pub segment: Range<usize>,
    /// Start of the TIFF header; every EXIF offset counts from here.
    pub tiff: usize,
    pub little: bool,
    /// File offset of the IFD0 entry value that points at the Exif IFD.
    pub exif_pointer: Option<usize>,
    /// File offset of the Exif IFD itself.
    pub exif_ifd: Option<usize>,
}

/// Byte-order aware reads over the whole file, bounded to the EXIF segment.
pub(crate) struct Reader<'a> {
    pub bytes: &'a [u8],
    pub little: bool,
    pub tiff: usize,
    pub end: usize,
}

impl Reader<'_> {
    pub fn u16(&self, at: usize) -> Result<u16, Error> {
        let b = self.get(at, 2)?;
        Ok(if self.little {
            u16::from_le_bytes([b[0], b[1]])
        } else {
            u16::from_be_bytes([b[0], b[1]])
        })
    }

    pub fn u32(&self, at: usize) -> Result<u32, Error> {
        let b = self.get(at, 4)?;
        let b = [b[0], b[1], b[2], b[3]];
        Ok(if self.little {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        })
    }

    fn get(&self, at: usize, len: usize) -> Result<&[u8], Error> {
        if at < self.tiff || at.saturating_add(len) > self.end {
            return Err(Error::Damaged("an offset points outside the EXIF block"));
        }
        Ok(&self.bytes[at..at + len])
    }

    /// The entries of the IFD at a file offset: (tag, type, count, file
    /// offset of the 4-byte value field).
    pub fn entries(&self, ifd: usize) -> Result<Vec<(u16, u16, u32, usize)>, Error> {
        let n = self.u16(ifd)? as usize;
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let e = ifd + 2 + i * 12;
            out.push((self.u16(e)?, self.u16(e + 2)?, self.u32(e + 4)?, e + 8));
        }
        Ok(out)
    }

    /// Where an entry's data lives: inline in the value field when it fits
    /// in four bytes, otherwise at the offset stored there.
    fn data(&self, typ: u16, count: u32, field: usize) -> Result<Text, Error> {
        let size = match typ {
            1 | 2 | 6 | 7 => 1,
            3 | 8 => 2,
            4 | 9 | 11 => 4,
            5 | 10 | 12 => 8,
            _ => return Err(Error::Damaged("unknown field type")),
        };
        let len = (count as usize)
            .checked_mul(size)
            .ok_or(Error::Damaged("field too long"))?;
        let at = if len <= 4 {
            field
        } else {
            self.tiff + self.u32(field)? as usize
        };
        self.get(at, len)?;
        Ok(Text { at, len })
    }

    fn ascii(&self, t: Text) -> String {
        let b = &self.bytes[t.at..t.at + t.len];
        let b = b.split(|&c| c == 0).next().unwrap_or(b);
        String::from_utf8_lossy(b).trim().to_owned()
    }
}

/// Find the first EXIF APP1 segment: (segment range, TIFF header offset).
fn find_app1(bytes: &[u8]) -> Result<(Range<usize>, usize), Error> {
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return Err(Error::NotJpeg);
    }
    let mut i = 2;
    loop {
        // Markers may be padded with any number of 0xFF fill bytes.
        if bytes.get(i) != Some(&0xff) {
            return Err(Error::Damaged("expected a JPEG marker"));
        }
        while bytes.get(i + 1) == Some(&0xff) {
            i += 1;
        }
        let marker = *bytes.get(i + 1).ok_or(Error::NoExif)?;
        match marker {
            // Start of scan or end of image: metadata comes before both.
            0xda | 0xd9 => return Err(Error::NoExif),
            0x01 | 0xd0..=0xd7 => {
                i += 2;
                continue;
            }
            _ => {}
        }
        let len = match bytes.get(i + 2..i + 4) {
            Some(b) => u16::from_be_bytes([b[0], b[1]]) as usize,
            None => return Err(Error::NoExif),
        };
        let end = i + 2 + len;
        if len < 2 || end > bytes.len() {
            return Err(Error::Damaged("a segment runs past the end of the file"));
        }
        if marker == 0xe1 && bytes[i + 4..end].starts_with(b"Exif\0\0") {
            return Ok((i..end, i + 10));
        }
        i = end;
    }
}

pub fn read(bytes: &[u8]) -> Result<Exif, Error> {
    let (segment, tiff) = find_app1(bytes)?;
    let little = match bytes.get(tiff..tiff + 4) {
        Some(b"II*\0") => true,
        Some(b"MM\0*") => false,
        _ => return Err(Error::Damaged("bad TIFF header")),
    };
    let r = Reader {
        bytes,
        little,
        tiff,
        end: segment.end,
    };
    let mut x = Exif {
        layout: Layout {
            segment,
            tiff,
            little,
            ..Layout::default()
        },
        ..Exif::default()
    };

    let ifd0 = tiff + r.u32(tiff + 4)? as usize;
    let mut modify = None;
    for (tag, typ, count, field) in r.entries(ifd0)? {
        match tag {
            MAKE | MODEL if typ == 2 => {
                let s = r.ascii(r.data(typ, count, field)?);
                let s = (!s.is_empty()).then_some(s);
                if tag == MAKE {
                    x.make = s;
                } else {
                    x.model = s;
                }
            }
            MODIFY_DATE if typ == 2 => {
                let t = r.data(typ, count, field)?;
                modify = parse_date(&r.ascii(t));
                x.dates.push((tag, t));
            }
            EXIF_IFD => {
                x.layout.exif_pointer = Some(field);
                x.layout.exif_ifd = Some(tiff + r.u32(field)? as usize);
            }
            _ => {}
        }
    }

    // IFD1 holds the thumbnail. Its pointer follows IFD0's entries.
    let n = r.u16(ifd0)? as usize;
    let next = r.u32(ifd0 + 2 + n * 12)? as usize;
    if next != 0 {
        let (mut start, mut len) = (None, None);
        for (tag, _, _, field) in r.entries(tiff + next)? {
            match tag {
                THUMB_START => start = Some(r.u32(field)? as usize),
                THUMB_LEN => len = Some(r.u32(field)? as usize),
                _ => {}
            }
        }
        if let (Some(s), Some(l)) = (start, len) {
            let at = tiff + s;
            if l > 0 && at + l <= r.end {
                x.thumbnail = Some(at..at + l);
            }
        }
    }

    let (mut original, mut created) = (None, None);
    if let Some(ifd) = x.layout.exif_ifd {
        for (tag, typ, count, field) in r.entries(ifd)? {
            if typ != 2 {
                continue;
            }
            match tag {
                DATE_ORIGINAL | CREATE_DATE => {
                    let t = r.data(typ, count, field)?;
                    let d = parse_date(&r.ascii(t));
                    if tag == DATE_ORIGINAL {
                        original = d;
                    } else {
                        created = d;
                    }
                    x.dates.push((tag, t));
                }
                OFFSET_TIME | OFFSET_ORIGINAL | OFFSET_DIGITIZED => {
                    let t = r.data(typ, count, field)?;
                    if tag == OFFSET_ORIGINAL {
                        x.tz = parse_tz(&r.ascii(t));
                    }
                    x.offsets.push((tag, t));
                }
                SERIAL => {
                    let s = r.ascii(r.data(typ, count, field)?);
                    x.serial = (!s.is_empty()).then_some(s);
                }
                _ => {}
            }
        }
    }
    x.taken = original.or(created).or(modify);
    Ok(x)
}

/// "2024:07:14 18:03:27". Cameras with an unset clock write zeros or
/// spaces, which come back as None.
pub fn parse_date(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s.trim(), DATE_FORMAT).ok()
}

/// "+09:00" or "-03:30" to minutes east of UTC.
pub fn parse_tz(s: &str) -> Option<i32> {
    let s = s.trim();
    let sign = match s.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let (h, m) = s[1..].split_once(':')?;
    // Unsigned, so "+-5:00" or "+05:-30" don't come out as a time zone.
    let (h, m): (u8, u8) = (h.parse().ok()?, m.parse().ok()?);
    (h <= 14 && m < 60).then_some(sign * (i32::from(h) * 60 + i32::from(m)))
}

/// The inverse of `parse_tz`: always seven bytes with the NUL, as EXIF wants.
pub fn format_tz(minutes: i32) -> String {
    let sign = if minutes < 0 { '-' } else { '+' };
    let m = minutes.abs();
    format!("{sign}{:02}:{:02}", m / 60, m % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_zones() {
        assert_eq!(parse_tz("+09:00"), Some(540));
        assert_eq!(parse_tz("-03:30"), Some(-210));
        assert_eq!(parse_tz(" +05:45 "), Some(345));
        for bad in ["+-5:00", "+05:-30", "+15:00", "+05:60", "05:00", "+5", ""] {
            assert_eq!(parse_tz(bad), None, "{bad}");
        }
    }
}
