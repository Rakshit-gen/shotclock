//! Reading a folder of photos and sorting them by the camera that took them.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use serde::Serialize;

use crate::exif::{self, Exif};

#[derive(Debug, Clone, Serialize)]
pub struct Photo {
    #[serde(skip)]
    pub path: PathBuf,
    pub name: String,
    /// Wall-clock time as the camera wrote it.
    pub taken: NaiveDateTime,
    /// Minutes east of UTC, if the camera recorded its time zone.
    pub tz: Option<i32>,
    pub thumbnail: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Camera {
    pub name: String,
    /// The camera the others are lined up against: a phone if there is one.
    pub reference: bool,
    /// Sorted by time taken.
    pub photos: Vec<Photo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Skipped {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Folder {
    pub dir: PathBuf,
    pub cameras: Vec<Camera>,
    pub skipped: Vec<Skipped>,
}

/// EXIF sits at the start of a JPEG and its segment can't pass 64 KB, so
/// this is nearly always enough, and reading 2,000 whole photos is not.
const HEAD: u64 = 128 * 1024;

/// The EXIF of a file, read from its first bytes, or the whole file if the
/// EXIF turns out to start further in.
pub fn read_head(path: &Path) -> std::io::Result<(Vec<u8>, Result<Exif, exif::Error>)> {
    let mut bytes = Vec::new();
    File::open(path)?.take(HEAD).read_to_end(&mut bytes)?;
    let x = exif::read(&bytes);
    if matches!(x, Err(exif::Error::Damaged(_))) && bytes.len() as u64 == HEAD {
        let bytes = std::fs::read(path)?;
        let x = exif::read(&bytes);
        return Ok((bytes, x));
    }
    Ok((bytes, x))
}

fn camera_name(x: &Exif) -> String {
    let make = x.make.as_deref().unwrap_or("").trim();
    let model = x.model.as_deref().unwrap_or("").trim();
    let mut name = if model.to_lowercase().starts_with(&make.to_lowercase()) {
        model.to_owned()
    } else {
        format!("{make} {model}").trim().to_owned()
    };
    if name.is_empty() {
        name = "Unknown camera".into();
    }
    // Two of the same model on one trip are told apart by serial number.
    if let Some(s) = &x.serial {
        name += &format!(" ({s})");
    }
    name
}

/// Phones set their clocks from the network, so they are the reference.
fn looks_like_phone(name: &str) -> bool {
    let n = name.to_lowercase();
    [
        "apple", "iphone", "pixel", "samsung", "sm-", "xiaomi", "oneplus", "huawei", "oppo",
        "vivo", "motorola", "nothing", "xperia",
    ]
    .iter()
    .any(|k| n.contains(k))
}

const RAW: &[&str] = &[
    "heic", "heif", "cr2", "cr3", "nef", "arw", "dng", "raf", "orf", "rw2", "pef",
];

pub fn scan(dir: &Path) -> std::io::Result<Folder> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    entries.sort();

    let mut cameras: Vec<Camera> = Vec::new();
    let mut skipped = Vec::new();
    for path in entries {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if RAW.contains(&ext.as_str()) {
            skipped.push(Skipped {
                name,
                reason: "only JPEG files can be read and fixed for now".into(),
            });
            continue;
        }
        if ext != "jpg" && ext != "jpeg" {
            continue;
        }
        let x = match read_head(&path) {
            Ok((_, Ok(x))) => x,
            Ok((_, Err(e))) => {
                skipped.push(Skipped {
                    name,
                    reason: e.to_string(),
                });
                continue;
            }
            Err(e) => {
                skipped.push(Skipped {
                    name,
                    reason: format!("can't read it: {e}"),
                });
                continue;
            }
        };
        let Some(taken) = x.taken else {
            skipped.push(Skipped {
                name,
                reason: "it has no capture date".into(),
            });
            continue;
        };
        let photo = Photo {
            path,
            name,
            taken,
            tz: x.tz,
            thumbnail: x.thumbnail.is_some(),
        };
        let cam = camera_name(&x);
        match cameras.iter_mut().find(|c| c.name == cam) {
            Some(c) => c.photos.push(photo),
            None => cameras.push(Camera {
                name: cam,
                reference: false,
                photos: vec![photo],
            }),
        }
    }

    for c in &mut cameras {
        c.photos
            .sort_by(|a, b| a.taken.cmp(&b.taken).then(a.name.cmp(&b.name)));
    }
    // The reference is the most phone-like camera: a known phone maker, and
    // photos that carry a time zone, which phones write and most cameras
    // don't. Ties go to the camera with more photos.
    let score = |c: &Camera| {
        let tz = c.photos.iter().filter(|p| p.tz.is_some()).count() as f64 / c.photos.len() as f64;
        (
            looks_like_phone(&c.name) as u8 as f64 * 2.0 + tz,
            c.photos.len(),
        )
    };
    cameras.sort_by(|a, b| {
        let (sa, sb) = (score(a), score(b));
        sb.0.total_cmp(&sa.0).then(sb.1.cmp(&sa.1))
    });
    if let Some(c) = cameras.first_mut() {
        c.reference = true;
    }
    Ok(Folder {
        dir: dir.to_path_buf(),
        cameras,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{self, Fields, tests::bare_jpeg};

    fn write(dir: &Path, name: &str, make: &str, model: &str, at: &str, tz: Option<i32>) {
        let f = Fields {
            make,
            model,
            taken: exif::parse_date(at).unwrap(),
            tz,
            thumbnail: None,
            little: true,
        };
        std::fs::write(
            dir.join(name),
            build::insert(&bare_jpeg(), &build::app1(&f)),
        )
        .unwrap();
    }

    #[test]
    fn groups_by_camera_and_picks_the_phone() {
        let d = tempfile::tempdir().unwrap();
        write(
            d.path(),
            "DSCF0002.JPG",
            "FUJIFILM",
            "X-T30",
            "2024:07:14 10:00:05",
            None,
        );
        write(
            d.path(),
            "DSCF0001.JPG",
            "FUJIFILM",
            "X-T30",
            "2024:07:14 10:00:00",
            None,
        );
        write(
            d.path(),
            "DSCF0003.JPG",
            "FUJIFILM",
            "X-T30",
            "2024:07:14 11:00:00",
            None,
        );
        write(
            d.path(),
            "IMG_1.jpg",
            "Apple",
            "iPhone 15",
            "2024:07:14 15:37:00",
            Some(540),
        );
        write(
            d.path(),
            "IMG_2.jpg",
            "Canon",
            "Canon EOS R6",
            "2024:07:14 15:37:00",
            None,
        );
        std::fs::write(d.path().join("IMG_3.HEIC"), b"x").unwrap();
        std::fs::write(d.path().join("notes.txt"), b"x").unwrap();
        std::fs::write(d.path().join("broken.jpg"), b"x").unwrap();

        let f = scan(d.path()).unwrap();
        let names: Vec<&str> = f.cameras.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Apple iPhone 15", "FUJIFILM X-T30", "Canon EOS R6"]);
        assert!(f.cameras[0].reference && !f.cameras[1].reference);
        let fuji: Vec<&str> = f.cameras[1]
            .photos
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(fuji, ["DSCF0001.JPG", "DSCF0002.JPG", "DSCF0003.JPG"]);
        let skipped: Vec<&str> = f.skipped.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(skipped, ["IMG_3.HEIC", "broken.jpg"]);
    }
}
