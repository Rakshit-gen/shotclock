//! shotclock on the command line: say what the dates would become, and
//! write corrected copies only when asked.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use clap::Parser;
use serde::Serialize;
use shotclock::exif;
use shotclock::offset;
use shotclock::scan::{self, Folder};
use shotclock::suggest::{self, Suggestion};
use shotclock::write::{self, Tz};

#[derive(Parser)]
#[command(
    version,
    about = "Fix the dates on photos from a camera whose clock was wrong",
    after_help = "Without --offset, shotclock suggests one by lining up the camera's photos with the phone's. Nothing is written without --write."
)]
struct Cli {
    /// Folder with the photos from the phone and the camera.
    dir: PathBuf,
    /// What to add to the camera's times, like +1h23m, -45m or -0:30.
    #[arg(long, allow_hyphen_values = true)]
    offset: Option<String>,
    /// Which camera to fix, by name or part of it, when there are several.
    #[arg(long)]
    camera: Option<String>,
    /// A camera photo and a phone photo of the same moment, by file name.
    /// The offset is the gap between their times.
    #[arg(long, num_args = 2, value_names = ["CAMERA_PHOTO", "PHONE_PHOTO"], conflicts_with = "offset")]
    pair: Option<Vec<String>>,
    /// Also set the time zone: "phone" copies it from the nearest phone
    /// photo, or give one like +09:00.
    #[arg(long, allow_hyphen_values = true)]
    tz: Option<String>,
    /// Folder for the corrected copies. Defaults to "<dir> fixed" next to it.
    #[arg(long)]
    out: Option<PathBuf>,
    /// Write the corrected copies. Without this, nothing is written.
    #[arg(long)]
    write: bool,
    /// Print the result as JSON instead of text.
    #[arg(long)]
    json: bool,
}

#[derive(Serialize)]
struct Fix {
    camera: String,
    /// "given", "pair" or "suggested".
    source: &'static str,
    offset: Option<i64>,
    suggestion: Option<Suggestion>,
    changes: Vec<write::Change>,
    written: Option<Vec<write::Written>>,
}

fn parse_tz(s: Option<&str>) -> Result<Tz, String> {
    match s {
        None => Ok(Tz::Keep),
        Some("phone") => Ok(Tz::Phone),
        Some(v) => exif::parse_tz(v).map(Tz::Fixed).ok_or_else(|| {
            format!(
                "can't read \"{v}\" as a time zone; write it like +09:00 or -03:30, or use phone"
            )
        }),
    }
}

/// The offset from a camera photo and a phone photo of the same moment.
fn pair_offset(
    folder: &Folder,
    camera_photo: &str,
    phone_photo: &str,
) -> Result<(usize, i64), String> {
    let find = |name: &str, reference: bool| {
        folder
            .cameras
            .iter()
            .enumerate()
            .filter(|(_, c)| c.reference == reference)
            .find_map(|(i, c)| {
                c.photos
                    .iter()
                    .find(|p| p.name.eq_ignore_ascii_case(name))
                    .map(|p| (i, p.taken))
            })
    };
    let (cam, cam_t) = find(camera_photo, false)
        .ok_or_else(|| format!("{camera_photo} isn't one of the camera photos in the folder"))?;
    let (_, phone_t) = find(phone_photo, true)
        .ok_or_else(|| format!("{phone_photo} isn't one of the phone photos in the folder"))?;
    Ok((cam, (phone_t - cam_t).num_seconds()))
}

fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn pick_camera(folder: &Folder, name: Option<&str>) -> Result<Vec<usize>, String> {
    let others: Vec<usize> = (0..folder.cameras.len())
        .filter(|&i| !folder.cameras[i].reference)
        .collect();
    let Some(name) = name else {
        return Ok(others);
    };
    let n = name.to_lowercase();
    let hits: Vec<usize> = others
        .into_iter()
        .filter(|&i| folder.cameras[i].name.to_lowercase().contains(&n))
        .collect();
    match hits.len() {
        1 => Ok(hits),
        0 => Err(format!(
            "no camera other than the phone matches \"{name}\"; the list above has their names"
        )),
        _ => Err(format!(
            "\"{name}\" matches more than one camera; use more of the name"
        )),
    }
}

fn describe(s: &Suggestion) -> String {
    let mut out = format!(
        "suggested offset {} ({} confidence, {:.2})\n  {:.0}% of its photos land within 3 minutes of a phone photo.",
        offset::format(s.offset),
        suggest::confidence_word(s.confidence),
        s.confidence,
        s.matched * 100.0
    );
    if let Some(note) = &s.note {
        out += &format!("\n  Rounded: {note}.");
    }
    if let Some(r) = s.runner_up {
        out += &format!("\n  Next best: {}.", offset::format(r));
    }
    out
}

fn run(cli: Cli) -> Result<(), String> {
    let started = Instant::now();
    let folder =
        scan::scan(&cli.dir).map_err(|e| format!("can't read {}: {e}", cli.dir.display()))?;
    let read_secs = started.elapsed().as_secs_f64();
    let total: usize = folder.cameras.iter().map(|c| c.photos.len()).sum();
    let tz = parse_tz(cli.tz.as_deref())?;
    let out = cli
        .out
        .clone()
        .unwrap_or_else(|| write::default_out(&cli.dir));
    let say = |s: String| {
        if !cli.json {
            println!("{s}");
        }
    };

    say(format!(
        "Read {} photos from {} in {read_secs:.2} s.",
        thousands(total),
        cli.dir.display()
    ));
    let width = folder
        .cameras
        .iter()
        .map(|c| c.name.len())
        .max()
        .unwrap_or(0);
    for c in &folder.cameras {
        let role = if c.reference { "  reference" } else { "" };
        say(format!(
            "  {:width$}  {:>6} photos{role}",
            c.name,
            thousands(c.photos.len())
        ));
    }
    for s in &folder.skipped {
        say(format!("  Skipped {}: {}.", s.name, s.reason));
    }
    if total == 0 {
        return Err(format!(
            "found no JPEG photos with a capture date in {}; point it at the folder that holds them",
            cli.dir.display()
        ));
    }

    let (targets, given, source) = match (&cli.pair, &cli.offset) {
        (Some(p), _) => {
            let (cam, off) = pair_offset(&folder, &p[0], &p[1])?;
            (vec![cam], Some(off), "pair")
        }
        (None, Some(o)) => {
            let off = offset::parse(o)?;
            let targets = pick_camera(&folder, cli.camera.as_deref())?;
            if targets.len() > 1 {
                return Err("there is more than one camera to fix; say which one the offset is for with --camera".into());
            }
            (targets, Some(off), "given")
        }
        (None, None) => (
            pick_camera(&folder, cli.camera.as_deref())?,
            None,
            "suggested",
        ),
    };
    if targets.is_empty() {
        return Err("all the photos are from one camera, so there is nothing to line up; add the phone's photos from the same days to the folder".into());
    }

    let mut fixes = Vec::new();
    for &i in &targets {
        let cam = &folder.cameras[i];
        say(String::new());
        let suggestion = if given.is_none() {
            suggest::for_camera(&folder, i)
        } else {
            None
        };
        let shift = match (given, &suggestion) {
            (Some(s), _) => {
                let how = if source == "pair" {
                    "from the pair of photos"
                } else {
                    "as given"
                };
                say(format!("{}: offset {} {how}.", cam.name, offset::format(s)));
                Some(s)
            }
            (None, Some(s)) => {
                say(format!("{}: {}", cam.name, describe(s)));
                Some(s.offset)
            }
            (None, None) => {
                say(format!(
                    "{}: no phone photos near enough in time to suggest an offset. Give one with --offset or --pair.",
                    cam.name
                ));
                None
            }
        };
        let changes = shift
            .map(|s| write::changes(&folder, i, s, tz))
            .unwrap_or_default();
        for c in changes.iter().take(5) {
            let zone =
                c.tz.map(|z| format!(" {}", exif::format_tz(z)))
                    .unwrap_or_default();
            say(format!("  {}  {}  becomes  {}{zone}", c.name, c.from, c.to));
        }
        if changes.len() > 5 {
            say(format!("  and {} more", thousands(changes.len() - 5)));
        }
        fixes.push(Fix {
            camera: cam.name.clone(),
            source,
            offset: shift,
            suggestion,
            changes,
            written: None,
        });
    }

    if cli.write {
        for f in &mut fixes {
            if f.changes.is_empty() {
                continue;
            }
            let w = write::write(&f.changes, &out).map_err(|e| e.to_string())?;
            say(format!(
                "\nWrote {} copies from the {} to {}. The image data in each matches its original.",
                thousands(w.len()),
                f.camera,
                out.display()
            ));
            f.written = Some(w);
        }
    } else {
        say(format!(
            "\nDry run: nothing was written. Add --write to save corrected copies to {}.",
            out.display()
        ));
    }

    if cli.json {
        let v = serde_json::json!({
            "dir": cli.dir,
            "out": out,
            "cameras": folder.cameras.iter().map(|c| serde_json::json!({
                "name": c.name,
                "reference": c.reference,
                "photos": c.photos.len(),
            })).collect::<Vec<_>>(),
            "skipped": folder.skipped,
            "fixes": fixes,
            "written": cli.write,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?
        );
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("shotclock: {e}");
            ExitCode::FAILURE
        }
    }
}
