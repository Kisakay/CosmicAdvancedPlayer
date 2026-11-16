// SPDX-License-Identifier: MIT

//! Watches the library folders and asks for an incremental re-scan when files
//! are added, removed or modified (debounced).

use std::path::PathBuf;
use std::time::Duration;

use cosmic::iced::Subscription;
use cosmic::iced::futures::SinkExt;
use notify::{EventKind, RecursiveMode, Watcher};

use super::is_audio_file;

const QUIET: Duration = Duration::from_secs(3);

/// Emits `()` each time the content of `roots` changed and settled down.
pub fn subscription(roots: Vec<PathBuf>) -> Subscription<()> {
    Subscription::run_with(roots, |roots| {
        let roots = roots.clone();
        cosmic::iced::stream::channel(4, async move |mut output| {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();
            let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                let Ok(event) = res else { return };
                let relevant = matches!(
                    event.kind,
                    EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_)
                ) && event.paths.iter().any(|p| is_audio_file(p) || p.extension().is_none());
                if relevant {
                    let _ = tx.send(());
                }
            });
            let mut watcher = match watcher {
                Ok(w) => w,
                Err(err) => {
                    eprintln!("library watcher unavailable: {err}");
                    return std::future::pending().await;
                }
            };
            for root in &roots {
                if let Err(err) = watcher.watch(root, RecursiveMode::Recursive) {
                    eprintln!("cannot watch {}: {err}", root.display());
                }
            }
            while rx.recv().await.is_some() {
                // Debounce: wait until nothing happened for a while (copies in progress…).
                while let Ok(Some(())) = tokio::time::timeout(QUIET, rx.recv()).await {}
                if output.send(()).await.is_err() {
                    break;
                }
            }
            drop(watcher);
        })
    })
}
