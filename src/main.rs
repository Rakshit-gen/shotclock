//! shotclock on the command line: say what the dates would become, and
//! write corrected copies only when asked.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use clap::Parser;
use shotclock::scan::{self, Folder};
use shotclock::suggest::{self, Suggestion};
use shotclock::write::{self, Tz};
use shotclock::offset;

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
    /// Folder for the corrected copies. Defaults to "<dir> fixed" next to it.
    #[arg(long)]
    out: Option<PathBuf>,
}

fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn pick_camera(folder: &Folder, name: Option<&str>) -> Result<Vec<usize>, String> {
    let others: Vec<usize> = (0..folder.cameras.len()).filter(|&i| !folder.cameras[i].reference).collect();
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
        0 => Err(format!("no camera other than the phone matches \"{name}\"; the list above has their names")),
        _ => Err(format!("\"{name}\" matches more than one camera; use more of the name")),
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
    let folder = scan::scan(&cli.dir).map_err(|e| format!("can't read {}: {e}", cli.dir.display()))?;
    let total: usize = folder.cameras.iter().map(|c| c.photos.len()).sum();
    println!(
        "Read {} photos from {} in {:.2} s.",
        thousands(total),
        cli.dir.display(),
        started.elapsed().as_secs_f64()
    );
    let width = folder.cameras.iter().map(|c| c.name.len()).max().unwrap_or(0);
    for c in &folder.cameras {
        let role = if c.reference { "  reference" } else { "" };
        println!("  {:width$}  {:>6} photos{role}", c.name, thousands(c.photos.len()));
    }
    for s in &folder.skipped {
        println!("  Skipped {}: {}.", s.name, s.reason);
    }
    let targets = pick_camera(&folder, cli.camera.as_deref())?;
    if targets.is_empty() {
        return Err("all the photos are from one camera, so there is nothing to line up; add the phone's photos from the same days to the folder".into());
    }
    let typed = cli.offset.as_deref().map(offset::parse).transpose()?;
    if typed.is_some() && targets.len() > 1 {
        return Err("there is more than one camera to fix; say which one the offset is for with --camera".into());
    }

    let out = cli.out.clone().unwrap_or_else(|| write::default_out(&cli.dir));
    for &i in &targets {
        let cam = &folder.cameras[i];
        println!();
        let shift = match typed {
            Some(s) => {
                println!("{}: offset {} as given.", cam.name, offset::format(s));
                s
            }
            None => match suggest::for_camera(&folder, i) {
                Some(s) => {
                    println!("{}: {}", cam.name, describe(&s));
                    s.offset
                }
                None => {
                    println!("{}: no phone photos near enough in time to suggest an offset. Give one with --offset.", cam.name);
                    continue;
                }
            },
        };
        let changes = write::changes(&folder, i, shift, Tz::Keep);
        for c in changes.iter().take(5) {
            println!("  {}  {}  becomes  {}", c.name, c.from, c.to);
        }
        if changes.len() > 5 {
            println!("  and {} more", thousands(changes.len() - 5));
        }
    }
    println!(
        "\nDry run: nothing was written. Add --write to save corrected copies to {}.",
        out.display()
    );
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
