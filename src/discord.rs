// SPDX-License-Identifier: GPL-3.0-only

//! Discord Rich Presence over the local IPC socket.
//!
//! A background thread keeps the connection to the Discord client, reconnects
//! when Discord is (re)started and publishes a "Listening to" activity with a
//! progress bar. Covers must be public URLs: they come from Last.fm or from an
//! optional iTunes Search lookup.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use discord_rich_presence::activity::{
    Activity, ActivityType, Assets, Button, StatusDisplayType, Timestamps,
};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};
use serde_json::Value;

const RETRY: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq)]
pub struct Presence {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub cover_url: Option<String>,
    pub position: Duration,
    pub duration: Duration,
    pub paused: bool,
}

#[derive(Debug, Clone)]
enum Command {
    Configure { enabled: bool, client_id: String },
    Set(Option<Presence>),
}

#[derive(Debug, Clone)]
pub struct Handle(mpsc::Sender<Command>);

impl Handle {
    pub fn configure(&self, enabled: bool, client_id: &str) {
        let _ = self.0.send(Command::Configure {
            enabled,
            client_id: client_id.trim().to_string(),
        });
    }

    pub fn set(&self, presence: Option<Presence>) {
        let _ = self.0.send(Command::Set(presence));
    }
}

/// Starts the presence thread.
#[must_use]
pub fn spawn() -> Handle {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("discord-rpc".into())
        .spawn(move || run(&rx))
        .expect("spawn discord thread");
    Handle(tx)
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn truncate(s: &str, max: usize) -> String {
    // Discord rejects fields shorter than 2 or longer than 128 characters.
    let mut out: String = s.chars().take(max).collect();
    while out.chars().count() < 2 {
        out.push(' ');
    }
    out
}

fn lastfm_track_url(p: &Presence) -> String {
    format!(
        "https://www.last.fm/music/{}/_/{}",
        urlencoding::encode(&p.artist).replace("%20", "+"),
        urlencoding::encode(&p.title).replace("%20", "+")
    )
}

fn publish(client: &mut DiscordIpcClient, p: &Presence) -> Result<(), discord_rich_presence::error::Error> {
    let title = truncate(&p.title, 128);
    let artist = truncate(
        if p.artist.is_empty() {
            "Unknown artist"
        } else {
            &p.artist
        },
        128,
    );
    let album = truncate(if p.album.is_empty() { &p.title } else { &p.album }, 128);
    let url = lastfm_track_url(p);

    let mut assets = Assets::new().large_text(&album);
    if let Some(cover) = &p.cover_url {
        assets = assets.large_image(cover);
    }
    if p.paused {
        assets = assets.small_text("Paused");
    }
    let mut activity = Activity::new()
        .activity_type(ActivityType::Listening)
        .status_display_type(StatusDisplayType::State)
        .details(&title)
        .state(&artist)
        .assets(assets)
        .buttons(vec![Button::new("Last.fm", &url)]);
    if !p.paused && !p.duration.is_zero() {
        let start = unix_now() - p.position.as_secs() as i64;
        activity = activity.timestamps(
            Timestamps::new()
                .start(start)
                .end(start + p.duration.as_secs() as i64),
        );
    }
    client.set_activity(activity)
}

fn run(rx: &mpsc::Receiver<Command>) {
    let mut enabled = false;
    let mut client_id = String::new();
    let mut client: Option<DiscordIpcClient> = None;
    let mut wanted: Option<Presence> = None;
    let mut dirty = false;
    let mut last_attempt: Option<Instant> = None;

    loop {
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Command::Configure {
                enabled: e,
                client_id: id,
            }) => {
                if e != enabled || id != client_id {
                    if let Some(mut c) = client.take() {
                        let _ = c.clear_activity();
                        let _ = c.close();
                    }
                    enabled = e;
                    client_id = id;
                    last_attempt = None;
                    dirty = true;
                }
            }
            Ok(Command::Set(p)) => {
                if p != wanted {
                    wanted = p;
                    dirty = true;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        // Coalesce bursts (seeking…).
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                Command::Set(p) => {
                    if p != wanted {
                        wanted = p;
                        dirty = true;
                    }
                }
                Command::Configure {
                    enabled: e,
                    client_id: id,
                } => {
                    enabled = e;
                    client_id = id;
                    client = None;
                    dirty = true;
                }
            }
        }

        if !enabled || client_id.is_empty() {
            continue;
        }
        if client.is_none() {
            if wanted.is_none() || last_attempt.is_some_and(|t| t.elapsed() < RETRY) {
                continue;
            }
            last_attempt = Some(Instant::now());
            let mut c = DiscordIpcClient::new(&client_id);
            if c.connect().is_err() {
                continue; // Discord not running: try again later.
            }
            client = Some(c);
            dirty = true;
        }
        if dirty && let Some(c) = client.as_mut() {
            let result = match &wanted {
                Some(p) => publish(c, p),
                None => c.clear_activity(),
            };
            if result.is_err() {
                client = None; // Discord closed: reconnect later.
            } else {
                dirty = false;
            }
        }
    }
}

/// Finds a public album cover URL with the iTunes Search API (no account needed).
pub async fn itunes_cover(client: &reqwest::Client, artist: &str, album: &str) -> Option<String> {
    let term = format!("{artist} {album}");
    let url = format!(
        "https://itunes.apple.com/search?media=music&entity=album&limit=5&term={}",
        urlencoding::encode(term.trim())
    );
    let v: Value = client
        .get(&url)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let results = v.get("results")?.as_array()?;
    let album_lower = album.to_lowercase();
    let best = results
        .iter()
        .find(|r| {
            r.get("collectionName")
                .and_then(Value::as_str)
                .is_some_and(|n| n.to_lowercase().contains(&album_lower))
        })
        .or_else(|| results.first())?;
    let art = best.get("artworkUrl100")?.as_str()?;
    Some(art.replace("100x100bb", "512x512bb"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_fields_for_discord() {
        assert_eq!(truncate("a", 128), "a ");
        assert_eq!(truncate(&"x".repeat(200), 128).len(), 128);
    }

    #[test]
    fn builds_lastfm_url() {
        let p = Presence {
            title: "Get Lucky".into(),
            artist: "Daft Punk".into(),
            album: String::new(),
            cover_url: None,
            position: Duration::ZERO,
            duration: Duration::ZERO,
            paused: false,
        };
        assert_eq!(
            lastfm_track_url(&p),
            "https://www.last.fm/music/Daft+Punk/_/Get+Lucky"
        );
    }
}
