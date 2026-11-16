// SPDX-License-Identifier: GPL-3.0-only

//! Playback session (queue + position) restored on the next start.
//!
//! A queue can hold the whole library: it is kept out of `cosmic-config`
//! (rewritten and re-parsed on every settings change) and written by a
//! background thread to `~/.local/share/cosmic-advanced-player/session.json`.

use std::path::PathBuf;
use std::sync::mpsc;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub paths: Vec<String>,
    pub index: usize,
    pub position_ms: u64,
}

#[must_use]
pub fn file() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("cosmic-advanced-player")
        .join("session.json")
}

#[must_use]
pub fn load() -> Option<Session> {
    let bytes = std::fs::read(file()).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Writes atomically (temporary file + rename).
pub fn write(session: &Session) {
    let path = file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let Ok(json) = serde_json::to_vec(session) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// Background writer: only the latest pending session is written.
#[derive(Debug, Clone)]
pub struct Saver(mpsc::Sender<Session>);

impl Saver {
    #[must_use]
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<Session>();
        std::thread::Builder::new()
            .name("session-writer".into())
            .spawn(move || {
                while let Ok(mut session) = rx.recv() {
                    while let Ok(newer) = rx.try_recv() {
                        session = newer;
                    }
                    write(&session);
                }
            })
            .expect("spawn session writer");
        Self(tx)
    }

    pub fn save(&self, session: Session) {
        let _ = self.0.send(session);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let s = Session {
            paths: vec!["/m/a.flac".into()],
            index: 0,
            position_ms: 1200,
        };
        let json = serde_json::to_vec(&s).unwrap();
        assert_eq!(serde_json::from_slice::<Session>(&json).unwrap(), s);
    }
}
