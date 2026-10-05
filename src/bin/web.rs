//! shotclock-web: the same fixer as a local page. Open a folder, see each
//! camera's photos on a timeline next to the phone's, line them up, and
//! write corrected copies.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use axum::extract::{Path as UrlPath, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use clap::Parser;
use serde::Deserialize;
use serde_json::json;
use shotclock::scan::{self, Folder};
use shotclock::{suggest, write};
use tower_http::services::{ServeDir, ServeFile};

#[derive(Parser)]
#[command(version, about = "Fix the dates on photos from a camera whose clock was wrong, in the browser")]
struct Cli {
    /// A folder of photos to open right away. You can also type one on the page.
    dir: Option<PathBuf>,
    /// Port on 127.0.0.1 to listen on.
    #[arg(long, default_value_t = 7777)]
    port: u16,
    /// Serve the UI from this directory instead of the copy built into the
    /// binary. Handy while working on the UI.
    #[arg(long)]
    static_dir: Option<PathBuf>,
}

struct Opened {
    /// Goes up by one per folder opened, so thumbnail URLs from an earlier
    /// folder never hit the browser cache for this one.
    generation: u64,
    folder: Folder,
}

struct App {
    opened: RwLock<Option<Arc<Opened>>>,
}

struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

fn bad(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.into())
}

type Api<T> = Result<T, ApiError>;

fn describe(o: Option<&Opened>) -> serde_json::Value {
    let Some(o) = o else {
        return json!({ "dir": null });
    };
    let f = &o.folder;
    let cameras: Vec<_> = f
        .cameras
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let s = suggest::for_camera(f, i);
            json!({
                "name": c.name,
                "reference": c.reference,
                "photos": c.photos,
                "suggestion": s.as_ref().map(|s| json!({
                    "offset": s.offset,
                    "confidence": s.confidence,
                    "word": suggest::confidence_word(s.confidence),
                    "matched": s.matched,
                    "runner_up": s.runner_up,
                    "note": s.note,
                })),
            })
        })
        .collect();
    json!({
        "dir": f.dir,
        "out": write::default_out(&f.dir),
        "generation": o.generation,
        "cameras": cameras,
        "skipped": f.skipped,
    })
}

async fn get_folder(State(state): State<Arc<App>>) -> Json<serde_json::Value> {
    Json(describe(state.opened.read().unwrap().as_deref()))
}

/// "~/Pictures/Japan" as people type it.
fn expand(path: &str) -> PathBuf {
    let path = path.trim();
    match (path.strip_prefix("~"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with('/') => {
            PathBuf::from(home).join(rest.trim_start_matches('/'))
        }
        _ => PathBuf::from(path),
    }
}

fn open(state: &App, dir: &Path) -> Result<Arc<Opened>, String> {
    let dir = std::fs::canonicalize(dir).map_err(|e| {
        format!("can't open {}: {e}. Check the path and try again.", dir.display())
    })?;
    if !dir.is_dir() {
        return Err(format!("{} is a file; give the folder that holds the photos", dir.display()));
    }
    let folder = scan::scan(&dir).map_err(|e| format!("can't read {}: {e}", dir.display()))?;
    let mut slot = state.opened.write().unwrap();
    let generation = slot.as_ref().map_or(1, |o| o.generation + 1);
    let opened = Arc::new(Opened { generation, folder });
    *slot = Some(opened.clone());
    Ok(opened)
}

#[derive(Deserialize)]
struct OpenRequest {
    path: String,
}

async fn open_folder(
    State(state): State<Arc<App>>,
    Json(req): Json<OpenRequest>,
) -> Api<Json<serde_json::Value>> {
    if req.path.trim().is_empty() {
        return Err(bad("type the path of a folder with photos in it"));
    }
    let dir = expand(&req.path);
    let st = state.clone();
    let opened = tokio::task::spawn_blocking(move || open(&st, &dir))
        .await
        .map_err(|e| bad(e.to_string()))?
        .map_err(bad)?;
    Ok(Json(describe(Some(&opened))))
}

/// The preview JPEG the camera embedded in the photo's EXIF. Far smaller
/// than decoding the photo, and already the right size for a timeline.
async fn thumbnail(
    State(state): State<Arc<App>>,
    UrlPath((generation, camera, index)): UrlPath<(u64, usize, usize)>,
) -> Response {
    let Some(o) = state.opened.read().unwrap().clone() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let photo = o
        .folder
        .cameras
        .get(camera)
        .and_then(|c| c.photos.get(index));
    let (Some(photo), true) = (photo, o.generation == generation) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let path = photo.path.clone();
    let thumb = tokio::task::spawn_blocking(move || {
        let (bytes, x) = scan::read_head(&path).ok()?;
        let range = x.ok()?.thumbnail?;
        Some(bytes[range].to_vec())
    })
    .await
    .ok()
    .flatten();
    match thumb {
        Some(t) => (
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, "private, max-age=86400, immutable"),
            ],
            t,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(rust_embed::RustEmbed)]
#[folder = "web/dist"]
#[allow_missing = true]
struct Ui;

async fn embedded_ui(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let (file, path) = match Ui::get(path) {
        Some(f) if !path.is_empty() => (f, path),
        _ => match Ui::get("index.html") {
            Some(f) => (f, "index.html"),
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    "This build has no UI. Run `pnpm --dir web build` and rebuild, or pass --static-dir.",
                )
                    .into_response();
            }
        },
    };
    let cache = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    (
        [
            (header::CONTENT_TYPE, file.metadata.mimetype().to_string()),
            (header::CACHE_CONTROL, cache.to_string()),
        ],
        file.data,
    )
        .into_response()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let state = Arc::new(App {
        opened: RwLock::new(None),
    });
    if let Some(dir) = &cli.dir {
        open(&state, dir).map_err(anyhow::Error::msg)?;
    }
    let app = Router::new()
        .route("/api/folder", get(get_folder).post(open_folder))
        .route("/api/thumb/{generation}/{camera}/{index}", get(thumbnail))
        .with_state(state);
    let app = match &cli.static_dir {
        Some(dir) => app.fallback_service(
            ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html"))),
        ),
        None => app.fallback(embedded_ui),
    };

    let addr = SocketAddr::from(([127, 0, 0, 1], cli.port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("shotclock is running at http://{addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_a_home_folder() {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        assert_eq!(expand(" ~/Pictures/Japan "), home.join("Pictures/Japan"));
        assert_eq!(expand("~"), home);
        assert_eq!(expand("~bob/x"), PathBuf::from("~bob/x"));
        assert_eq!(expand("/tmp/x"), PathBuf::from("/tmp/x"));
    }
}
