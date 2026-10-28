//! Local Codex images omitted by `CodexAdapterV2.ts`'s item projection.
//! Protocol 2 exposes the native thread reference, so only that thread's
//! `generated_images/<native thread>` directory is read. Directory scans and
//! image decoding stay off the UI thread; remote environments never use this.
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use crate::Repaint;

use crate::model::Detail;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedImage {
    pub path: PathBuf,
    pub modified: SystemTime,
    pub bytes: u64,
}

impl GeneratedImage {
    /// A completed write retries a file first seen while it was still saving.
    pub fn key(&self) -> String {
        format!(
            "generated:{}:{:?}:{}",
            self.path.display(),
            self.modified,
            self.bytes
        )
    }
}

fn directories(detail: &Detail, home: &Path) -> Vec<PathBuf> {
    detail
        .provider_threads
        .iter()
        .filter(|t| t["driver"] == "codex" && t["appThreadId"] == detail.id())
        .filter_map(|t| t["nativeThreadRef"]["nativeId"].as_str())
        .filter(|id| {
            let mut parts = Path::new(id).components();
            matches!(parts.next(), Some(Component::Normal(_))) && parts.next().is_none()
        })
        .map(|id| home.join("generated_images").join(id))
        .collect()
}

fn scan(directory: &Path) -> Vec<GeneratedImage> {
    std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension()? != "png" {
                return None;
            }
            let meta = entry.metadata().ok()?;
            (meta.is_file() && meta.len() > 0).then(|| GeneratedImage {
                path,
                modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                bytes: meta.len(),
            })
        })
        .collect()
}

#[derive(Default)]
struct Directory {
    checked: Option<Instant>,
    scanning: bool,
    images: Vec<GeneratedImage>,
}

/// Poll only this conversation's native Codex directories, including when a
/// native image generation produced no server event to invalidate the timeline.
pub fn images(detail: &Detail, repaint: &Repaint) -> Vec<GeneratedImage> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Directory>>> = OnceLock::new();
    const INTERVAL: Duration = Duration::from_secs(1);
    let cache = CACHE.get_or_init(Default::default);
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|h| h.join(".codex")));
    let Some(home) = home else { return Vec::new() };
    let mut out = Vec::new();
    for path in directories(detail, &home) {
        repaint.after(INTERVAL);
        let Ok(mut cache_guard) = cache.lock() else {
            continue;
        };
        let entry = cache_guard.entry(path.clone()).or_default();
        out.extend(entry.images.clone());
        if entry.scanning || entry.checked.is_some_and(|at| at.elapsed() < INTERVAL) {
            continue;
        }
        entry.scanning = true;
        let repaint = repaint.clone();
        std::thread::spawn(move || {
            let images = scan(&path);
            if let Ok(mut cache) = cache.lock() {
                cache.insert(
                    path,
                    Directory {
                        checked: Some(Instant::now()),
                        scanning: false,
                        images,
                    },
                );
            }
            repaint.now();
        });
    }
    out.sort_by(|a, b| (&a.modified, &a.path).cmp(&(&b.modified, &b.path)));
    out.dedup_by(|a, b| a.path == b.path);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn directories_use_only_this_threads_native_codex_identity() {
        let detail = Detail {
            thread: json!({"id":"t"}),
            provider_threads: vec![
                json!({"driver":"codex", "appThreadId":"t", "providerSessionId":"session",
                    "nativeThreadRef":{"nativeId":"native-thread"}}),
                json!({"driver":"codex", "appThreadId":"other",
                    "nativeThreadRef":{"nativeId":"unrelated"}}),
                json!({"driver":"kimiCode", "appThreadId":"t",
                    "nativeThreadRef":{"nativeId":"kimi"}}),
                json!({"driver":"codex", "appThreadId":"t",
                    "nativeThreadRef":{"nativeId":"../elsewhere"}}),
            ],
            ..Default::default()
        };
        assert_eq!(
            directories(&detail, Path::new("/fixture/codex")),
            vec![PathBuf::from(
                "/fixture/codex/generated_images/native-thread"
            )]
        );
    }

    #[test]
    fn scans_find_existing_and_new_images_and_retry_changed_files() {
        let root = std::env::temp_dir().join(format!("pluto-generated-{}", uuid::Uuid::new_v4()));
        let directory = root.join("native-thread");
        assert!(scan(&directory).is_empty());
        std::fs::create_dir_all(&directory).unwrap();
        let file = directory.join("exec-image.png");
        std::fs::write(&file, b"partial").unwrap();
        std::fs::write(directory.join("notes.txt"), b"not an image").unwrap();
        let before = scan(&directory);
        assert_eq!(before.len(), 1);
        image::RgbaImage::from_pixel(4, 4, image::Rgba([10, 20, 30, 255]))
            .save(&file)
            .unwrap();
        let after = scan(&directory);
        assert_eq!(after.len(), 1);
        assert_ne!(before[0].key(), after[0].key());
        assert_eq!(after[0].path, file);
        std::fs::remove_dir_all(root).unwrap();
    }
}
