//! `wisp://` hands pictures from Wisp's data folder to the window: screenshots (whole, or as
//! thumbnails made the first time they're asked for), world icons and skins. Only PNG and JPEG
//! files inside the data folder are served, and thumbnails are made off the main thread.
//!
//!   wisp://localhost/file/<path in the data folder>
//!   wisp://localhost/thumb/<width>/<path in the data folder>
//!   wisp://localhost/skin/<skin id>
//!
//! (Windows spells it http://wisp.localhost/…; the path is the same.)

use std::path::{Component, Path, PathBuf};

use tauri::http::{Request, Response, StatusCode, header};
use tauri::{Manager, Runtime, UriSchemeContext, UriSchemeResponder};
use wisp_core::screenshots;

use crate::Paths;

pub const SCHEME: &str = "wisp";
const IMAGE_EXTENSIONS: [&str; 3] = ["png", "jpg", "jpeg"];
const THUMB_WIDTHS: (u32, u32) = (64, 1920);

pub fn handle<R: Runtime>(context: UriSchemeContext<'_, R>, request: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let app = context.app_handle().clone();
    let path = request.uri().path().to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let response = match app.try_state::<Paths>() {
            Some(paths) => serve(&paths, &path),
            None => Err(StatusCode::SERVICE_UNAVAILABLE),
        };
        responder.respond(response.unwrap_or_else(|status| {
            Response::builder()
                .status(status)
                .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                .body(Vec::new())
                .expect("a plain status response")
        }));
    });
}

fn serve(paths: &Paths, path: &str) -> Result<Response<Vec<u8>>, StatusCode> {
    let (kind, rest) = path.trim_start_matches('/').split_once('/').ok_or(StatusCode::NOT_FOUND)?;
    match kind {
        "file" => send_file(&resolve(&paths.data, rest)?),
        "thumb" => {
            let (width, rest) = rest.split_once('/').ok_or(StatusCode::NOT_FOUND)?;
            let width: u32 = width.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
            let source = resolve(&paths.data, rest)?;
            let thumb = screenshots::thumbnail(&source, &paths.thumbs(), width.clamp(THUMB_WIDTHS.0, THUMB_WIDTHS.1))
                .map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
            send_file(&thumb)
        }
        "skin" => {
            let id = percent_decode(rest).ok_or(StatusCode::BAD_REQUEST)?;
            let file = paths.skins().png_path(&id);
            if !file.is_file() {
                return Err(StatusCode::NOT_FOUND);
            }
            send_file(&file)
        }
        _ => Err(StatusCode::NOT_FOUND),
    }
}

/// A data-folder path from the URL, refusing anything that isn't a picture inside the folder.
fn resolve(data: &Path, encoded: &str) -> Result<PathBuf, StatusCode> {
    let relative = percent_decode(encoded).ok_or(StatusCode::BAD_REQUEST)?;
    let path = Path::new(&relative);
    if relative.is_empty() || !path.components().all(|part| matches!(part, Component::Normal(_))) {
        return Err(StatusCode::FORBIDDEN);
    }
    let picture = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| IMAGE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()));
    if !picture {
        return Err(StatusCode::FORBIDDEN);
    }
    let full = data.join(path).canonicalize().map_err(|_| StatusCode::NOT_FOUND)?;
    let root = data.canonicalize().map_err(|_| StatusCode::NOT_FOUND)?;
    if !full.starts_with(&root) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(full)
}

fn send_file(path: &Path) -> Result<Response<Vec<u8>>, StatusCode> {
    let bytes = std::fs::read(path).map_err(|_| StatusCode::NOT_FOUND)?;
    let jpeg = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("jpg") || extension.eq_ignore_ascii_case("jpeg"));
    Response::builder()
        .header(header::CONTENT_TYPE, if jpeg { "image/jpeg" } else { "image/png" })
        // URLs carry the file's time (?v=…), so a changed file gets a new URL.
        .header(header::CACHE_CONTROL, "max-age=31536000, immutable")
        // The skin viewer and editor read pixels, which needs this.
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(bytes)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// Decodes %XX escapes; None for broken ones or text that isn't UTF-8.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let pair = std::str::from_utf8(bytes.get(index + 1..index + 3)?).ok()?;
            out.push(u8::from_str_radix(pair, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_and_refuses_what_it_should() {
        assert_eq!(percent_decode("New%20World/icon.png").as_deref(), Some("New World/icon.png"));
        assert_eq!(percent_decode("caf%C3%A9").as_deref(), Some("café"));
        assert!(percent_decode("bad%2").is_none());
        let data = std::env::temp_dir().join(format!("wisp-protocol-{}", std::process::id()));
        std::fs::create_dir_all(data.join("instances/a")).unwrap();
        std::fs::write(data.join("instances/a/shot.png"), b"png").unwrap();
        std::fs::write(data.join("accounts.json"), b"{}").unwrap();
        assert!(resolve(&data, "instances/a/shot.png").is_ok());
        assert_eq!(resolve(&data, "accounts.json"), Err(StatusCode::FORBIDDEN));
        assert_eq!(resolve(&data, "..%2F..%2Fetc%2Fpasswd.png"), Err(StatusCode::FORBIDDEN));
        assert_eq!(resolve(&data, "instances/a/missing.png"), Err(StatusCode::NOT_FOUND));
        std::fs::remove_dir_all(data).unwrap();
    }
}
