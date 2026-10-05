//! Turning an offset into new dates, and writing corrected copies.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDateTime, TimeDelta, TimeZone};
use serde::Serialize;

use crate::exif;
use crate::patch::{self, image_hash};
use crate::scan::{Camera, Folder};

/// What to do with the time zone fields.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tz {
    /// Leave them as they are.
    Keep,
    /// Minutes east of UTC, for every photo.
    Fixed(i32),
    /// Whatever the nearest reference photo recorded, which follows a trip
    /// across time zones.
    Phone,
}

#[derive(Debug, Clone, Serialize)]
pub struct Change {
    #[serde(skip)]
    pub path: PathBuf,
    pub name: String,
    pub from: NaiveDateTime,
    pub to: NaiveDateTime,
    /// The time zone to write, if any.
    pub tz: Option<i32>,
}

/// The time zone recorded by the reference photo nearest in time to `t`.
pub fn nearest_tz(reference: &Camera, t: NaiveDateTime) -> Option<i32> {
    reference
        .photos
        .iter()
        .filter_map(|p| p.tz.map(|tz| ((p.taken - t).num_seconds().abs(), tz)))
        .min_by_key(|(gap, _)| *gap)
        .map(|(_, tz)| tz)
}

/// The new date of every photo from one camera.
pub fn changes(folder: &Folder, camera: usize, shift: i64, tz: Tz) -> Vec<Change> {
    let reference = folder.cameras.iter().find(|c| c.reference);
    let shift = TimeDelta::seconds(shift);
    folder.cameras[camera]
        .photos
        .iter()
        .map(|p| {
            let to = p.taken + shift;
            Change {
                path: p.path.clone(),
                name: p.name.clone(),
                from: p.taken,
                to,
                tz: match tz {
                    Tz::Keep => None,
                    Tz::Fixed(m) => Some(m),
                    Tz::Phone => reference.and_then(|r| nearest_tz(r, to)),
                },
            }
        })
        .collect()
}

/// A sibling of the photo folder: "Japan 2024" gets "Japan 2024 fixed".
pub fn default_out(dir: &Path) -> PathBuf {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "photos".into());
    dir.with_file_name(format!("{name} fixed"))
}

#[derive(Debug, Clone, Serialize)]
pub struct Written {
    pub name: String,
    /// SHA-256 of the image data, the same in the original and the copy.
    pub image_sha256: String,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the output folder is the photo folder; pick another so the originals stay as they are")]
    SameFolder,
    #[error("{0} already exists in the output folder; pick an empty folder, nothing is ever overwritten")]
    Exists(String),
    #[error("{name}: {source}")]
    Exif { name: String, source: exif::Error },
    #[error("{name}: the image data changed while writing the copy, so it was deleted; please report this")]
    Mismatch { name: String },
    #[error("{0}: {1}")]
    Io(String, std::io::Error),
}

/// Write corrected copies of the photos into `out`, keeping their names.
/// Checks everything it can before writing the first file, then checks
/// each copy on disk against its original.
pub fn write(changes: &[Change], out: &Path) -> Result<Vec<Written>, Error> {
    let io = |what: &Path, e| Error::Io(what.display().to_string(), e);
    fs::create_dir_all(out).map_err(|e| io(out, e))?;
    let out_real = fs::canonicalize(out).map_err(|e| io(out, e))?;
    for c in changes {
        if let Some(dir) = c.path.parent().and_then(|d| fs::canonicalize(d).ok())
            && dir == out_real
        {
            return Err(Error::SameFolder);
        }
        if out.join(&c.name).exists() {
            return Err(Error::Exists(c.name.clone()));
        }
    }

    let mut done = Vec::with_capacity(changes.len());
    for c in changes {
        let exif_err = |source| Error::Exif {
            name: c.name.clone(),
            source,
        };
        let bytes = fs::read(&c.path).map_err(|e| io(&c.path, e))?;
        let before = image_hash(&bytes, &exif::read(&bytes).map_err(exif_err)?);
        let fixed = patch::patch(&bytes, c.to - c.from, c.tz).map_err(exif_err)?;

        let dst = out.join(&c.name);
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dst)
            .map_err(|e| io(&dst, e))?;
        f.write_all(&fixed).map_err(|e| io(&dst, e))?;
        // Photo libraries fall back to the file date when EXIF is missing
        // or ignored, so it should agree with the new capture time.
        let when = match c.tz {
            Some(tz) => Some(c.to.and_utc() - TimeDelta::minutes(tz as i64)),
            None => Local.from_local_datetime(&c.to).earliest().map(|t| t.to_utc()),
        };
        if let Some(when) = when {
            f.set_modified(when.into()).map_err(|e| io(&dst, e))?;
        }
        drop(f);

        let written = fs::read(&dst).map_err(|e| io(&dst, e))?;
        let after = exif::read(&written).map(|x| image_hash(&written, &x));
        if after.as_ref() != Ok(&before) {
            let _ = fs::remove_file(&dst);
            return Err(Error::Mismatch {
                name: c.name.clone(),
            });
        }
        done.push(Written {
            name: c.name.clone(),
            image_sha256: before,
        });
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::tests::photo;
    use crate::scan::scan;

    #[test]
    fn writes_checked_copies_and_never_overwrites() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("trip");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("DSCF0001.JPG"), photo(true, None)).unwrap();
        fs::write(src.join("DSCF0002.JPG"), photo(false, None)).unwrap();
        let folder = scan(&src).unwrap();

        let ch = changes(&folder, 0, 3600, Tz::Fixed(120));
        assert_eq!(ch[0].to.to_string(), "2024-07-14 19:03:27");
        let out = default_out(&src);
        assert!(out.ends_with("trip fixed"));
        let w = write(&ch, &out).unwrap();
        assert_eq!(w.len(), 2);

        let copy = fs::read(out.join("DSCF0001.JPG")).unwrap();
        let x = exif::read(&copy).unwrap();
        assert_eq!(x.taken.unwrap().to_string(), "2024-07-14 19:03:27");
        assert_eq!(x.tz, Some(120));
        // 19:03:27 at +02:00 is 17:03:27 UTC.
        let mtime = fs::metadata(out.join("DSCF0001.JPG")).unwrap().modified().unwrap();
        let mtime: chrono::DateTime<chrono::Utc> = mtime.into();
        assert_eq!(mtime.to_rfc3339(), "2024-07-14T17:03:27+00:00");
        // The originals are untouched.
        assert_eq!(fs::read(src.join("DSCF0001.JPG")).unwrap(), photo(true, None));

        assert!(matches!(write(&ch, &out), Err(Error::Exists(_))));
        assert!(matches!(write(&ch, &src), Err(Error::SameFolder)));
    }
}
