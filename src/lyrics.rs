// SPDX-License-Identifier: MIT

//! Lyrics: LRC parsing and a small plugin system of providers.
//!
//! Providers are tried in the order chosen by the user. A time-synced result
//! wins immediately; otherwise the first plain-text result is kept.

use std::path::{Path, PathBuf};
use std::time::Duration;

use lofty::file::TaggedFileExt;
use lofty::prelude::ItemKey;
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

pub const USER_AGENT: &str = concat!(
    "CosmicAdvancedPlayer/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/Kisakay/CosmicAdvancedPlayer)"
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub time: Option<Duration>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lyrics {
    pub lines: Vec<Line>,
    pub synced: bool,
    pub provider: Provider,
}

impl Lyrics {
    /// Parses LRC (`[mm:ss.xx] text`) or plain text.
    #[must_use]
    pub fn parse(text: &str, provider: Provider) -> Option<Self> {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let mut offset_ms: i64 = 0;
        let mut timed: Vec<Line> = Vec::new();
        let mut plain: Vec<Line> = Vec::new();
        for raw in text.lines() {
            let mut rest = raw.trim();
            let mut times = Vec::new();
            while let Some(stripped) = rest.strip_prefix('[') {
                let Some(end) = stripped.find(']') else { break };
                let tag = &stripped[..end];
                if let Some(t) = parse_timestamp(tag) {
                    times.push(t);
                } else if let Some(v) = tag.strip_prefix("offset:") {
                    offset_ms = v.trim().parse().unwrap_or(0);
                } else if times.is_empty() && tag.contains(':') {
                    // Metadata tag ([ar:], [ti:]…): ignore the whole line.
                    rest = "";
                    break;
                } else {
                    break;
                }
                rest = stripped[end + 1..].trim_start();
            }
            // Word-level (enhanced LRC) timestamps: <mm:ss.xx>
            let text = strip_word_timestamps(rest).trim().to_string();
            if times.is_empty() {
                if !raw.trim().starts_with('[') {
                    plain.push(Line { time: None, text });
                }
            } else {
                for t in times {
                    timed.push(Line { time: Some(t), text: text.clone() });
                }
            }
        }
        if timed.len() >= 2 {
            timed.sort_by_key(|l| l.time);
            if offset_ms != 0 {
                for l in &mut timed {
                    l.time = l.time.map(|t| {
                        let ms = t.as_millis() as i64 - offset_ms;
                        Duration::from_millis(ms.max(0) as u64)
                    });
                }
            }
            return Some(Self { lines: timed, synced: true, provider });
        }
        // Trim leading / trailing blank lines.
        while plain.first().is_some_and(|l| l.text.is_empty()) {
            plain.remove(0);
        }
        while plain.last().is_some_and(|l| l.text.is_empty()) {
            plain.pop();
        }
        (!plain.is_empty()).then_some(Self { lines: plain, synced: false, provider })
    }

    /// Index of the line being sung at `pos`.
    #[must_use]
    pub fn current_line(&self, pos: Duration) -> Option<usize> {
        if !self.synced {
            return None;
        }
        let idx = self.lines.partition_point(|l| l.time.is_some_and(|t| t <= pos));
        idx.checked_sub(1)
    }

    /// Back to LRC / plain text (for the cache).
    #[must_use]
    pub fn to_text(&self) -> String {
        self.lines
            .iter()
            .map(|l| match l.time {
                Some(t) => {
                    let cs = t.as_millis() / 10;
                    format!("[{:02}:{:02}.{:02}]{}", cs / 6000, (cs / 100) % 60, cs % 100, l.text)
                }
                None => l.text.clone(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn parse_timestamp(tag: &str) -> Option<Duration> {
    let (min, rest) = tag.split_once(':')?;
    let min: u64 = min.trim().parse().ok()?;
    let (sec, frac) = match rest.split_once(['.', ':']) {
        Some((s, f)) => (s, f),
        None => (rest, "0"),
    };
    let sec: u64 = sec.trim().parse().ok()?;
    if sec >= 60 || !frac.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let frac_ms = match frac.len() {
        0 => 0,
        1 => frac.parse::<u64>().ok()? * 100,
        2 => frac.parse::<u64>().ok()? * 10,
        _ => frac[..3].parse::<u64>().ok()?,
    };
    Some(Duration::from_millis(min * 60_000 + sec * 1000 + frac_ms))
}

fn strip_word_timestamps(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        match rest[start..].find('>') {
            Some(end) if parse_timestamp(&rest[start + 1..start + end]).is_some() => {
                rest = &rest[start + end + 1..];
            }
            _ => {
                out.push('<');
                rest = &rest[start + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Lyrics providers ("plugins"), enabled and ordered in the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Provider {
    /// `.lrc` / `.txt` file next to the audio file.
    LocalFile,
    /// Lyrics stored in the file tags (USLT, LYRICS…).
    Embedded,
    /// <https://lrclib.net> — community database of time-synced lyrics.
    Lrclib,
    /// <https://lyrics.ovh> — plain lyrics.
    LyricsOvh,
}

impl Provider {
    pub const ALL: [Self; 4] = [Self::LocalFile, Self::Embedded, Self::Lrclib, Self::LyricsOvh];

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::LocalFile => "Local .lrc",
            Self::Embedded => "Embedded tags",
            Self::Lrclib => "LRCLIB",
            Self::LyricsOvh => "lyrics.ovh",
        }
    }

    #[must_use]
    pub fn is_online(self) -> bool {
        matches!(self, Self::Lrclib | Self::LyricsOvh)
    }

    #[must_use]
    pub fn url(self) -> Option<&'static str> {
        match self {
            Self::Lrclib => Some("https://lrclib.net"),
            Self::LyricsOvh => Some("https://lyrics.ovh"),
            _ => None,
        }
    }
}

/// What we know about the track we want lyrics for.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Query {
    pub path: PathBuf,
    pub artist: String,
    pub title: String,
    pub album: String,
    pub duration_secs: u64,
}

impl Query {
    fn cache_file(&self) -> PathBuf {
        let mut h = Md5::new();
        h.update(self.artist.to_lowercase().as_bytes());
        h.update([0]);
        h.update(self.title.to_lowercase().as_bytes());
        let key: String = h.finalize().iter().take(10).map(|b| format!("{b:02x}")).collect();
        cache_dir().join(format!("{key}.json"))
    }
}

fn cache_dir() -> PathBuf {
    let dir = dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("cosmic-advanced-player")
        .join("lyrics");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[derive(Serialize, Deserialize)]
struct CacheEntry {
    provider: Provider,
    text: String,
}

/// "Song (feat. X) - Remastered 2011" → "Song".
#[must_use]
pub fn clean_title(title: &str) -> String {
    let mut t = title.to_string();
    for open in ['(', '['] {
        if let Some(i) = t.find(open) {
            let inner = t[i..].to_lowercase();
            if ["feat", "ft.", "with ", "remaster", "live", "version", "edit", "mix", "mono", "stereo", "explicit"]
                .iter()
                .any(|k| inner.contains(k))
            {
                t.truncate(i);
            }
        }
    }
    if let Some(i) = t.find(" - ") {
        let tail = t[i..].to_lowercase();
        if ["remaster", "live", "version", "edit", "mix", "mono", "stereo"].iter().any(|k| tail.contains(k)) {
            t.truncate(i);
        }
    }
    t.trim().to_string()
}

/// First artist of "A, B & C" / "A feat. B".
#[must_use]
pub fn main_artist(artist: &str) -> String {
    let lower = artist.to_lowercase();
    let mut end = artist.len();
    for sep in [", ", " & ", " feat. ", " feat ", " ft. ", " x ", "; ", " / "] {
        if let Some(i) = lower.find(sep) {
            end = end.min(i);
        }
    }
    artist[..end].trim().to_string()
}

/// Looks for lyrics with the given providers, in order.
pub async fn fetch(client: &reqwest::Client, query: &Query, providers: &[Provider], use_cache: bool) -> Option<Lyrics> {
    let cache = query.cache_file();
    if use_cache
        && let Ok(bytes) = tokio::fs::read(&cache).await
        && let Ok(entry) = serde_json::from_slice::<CacheEntry>(&bytes)
        && providers.contains(&entry.provider)
        && let Some(l) = Lyrics::parse(&entry.text, entry.provider)
    {
        return Some(l);
    }

    let mut fallback: Option<Lyrics> = None;
    for provider in providers {
        let result = match provider {
            Provider::LocalFile => local_file(&query.path).await,
            Provider::Embedded => embedded(query.path.clone()).await,
            Provider::Lrclib => lrclib(client, query).await,
            Provider::LyricsOvh => lyrics_ovh(client, query).await,
        };
        match result {
            Some(l) if l.synced => {
                store(&cache, &l, *provider).await;
                return Some(l);
            }
            Some(l) if fallback.is_none() => fallback = Some(l),
            _ => {}
        }
    }
    if let Some(l) = &fallback {
        store(&cache, l, l.provider).await;
    }
    fallback
}

async fn store(cache: &Path, lyrics: &Lyrics, provider: Provider) {
    // Local sources are always re-read (the user may edit them).
    if provider.is_online()
        && let Ok(json) = serde_json::to_vec(&CacheEntry { provider, text: lyrics.to_text() })
    {
        let _ = tokio::fs::write(cache, json).await;
    }
}

/// Forgets the cached lyrics of a track (manual refresh).
pub fn forget(query: &Query) {
    let _ = std::fs::remove_file(query.cache_file());
}

async fn local_file(path: &Path) -> Option<Lyrics> {
    for ext in ["lrc", "LRC", "txt"] {
        if let Ok(text) = tokio::fs::read_to_string(path.with_extension(ext)).await
            && let Some(l) = Lyrics::parse(&text, Provider::LocalFile)
        {
            return Some(l);
        }
    }
    None
}

async fn embedded(path: PathBuf) -> Option<Lyrics> {
    tokio::task::spawn_blocking(move || {
        let tagged = lofty::read_from_path(&path).ok()?;
        tagged.tags().iter().find_map(|tag| {
            [ItemKey::Lyrics, ItemKey::UnsyncLyrics]
                .into_iter()
                .filter_map(|k| tag.get_string(k))
                .find_map(|text| Lyrics::parse(text, Provider::Embedded))
        })
    })
    .await
    .ok()
    .flatten()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrclibRecord {
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    instrumental: bool,
    #[serde(default)]
    plain_lyrics: Option<String>,
    #[serde(default)]
    synced_lyrics: Option<String>,
}

impl LrclibRecord {
    fn into_lyrics(self) -> Option<Lyrics> {
        if self.instrumental {
            return None;
        }
        self.synced_lyrics
            .as_deref()
            .and_then(|s| Lyrics::parse(s, Provider::Lrclib))
            .or_else(|| self.plain_lyrics.as_deref().and_then(|s| Lyrics::parse(s, Provider::Lrclib)))
    }
}

/// Full artist first, then the main artist of a collaboration.
fn artist_variants(artist: &str) -> Vec<String> {
    let full = artist.trim().to_string();
    let main = main_artist(artist);
    let mut v = vec![full];
    if !main.is_empty() && main != v[0] {
        v.push(main);
    }
    v.retain(|a| !a.is_empty());
    v
}

async fn lrclib(client: &reqwest::Client, q: &Query) -> Option<Lyrics> {
    for artist in artist_variants(&q.artist) {
        if let Some(l) = lrclib_for(client, q, artist).await {
            return Some(l);
        }
    }
    None
}

async fn lrclib_for(client: &reqwest::Client, q: &Query, artist: String) -> Option<Lyrics> {
    let title = clean_title(&q.title);
    if title.is_empty() {
        return None;
    }
    // 1. Exact signature match.
    let mut params = vec![("artist_name", artist.clone()), ("track_name", title.clone())];
    if !q.album.is_empty() {
        params.push(("album_name", q.album.clone()));
    }
    if q.duration_secs > 0 {
        params.push(("duration", q.duration_secs.to_string()));
    }
    let url = format!("https://lrclib.net/api/get?{}", encode(&params));
    if let Ok(resp) = client.get(&url).send().await
        && resp.status().is_success()
        && let Ok(rec) = resp.json::<LrclibRecord>().await
        && let Some(l) = rec.into_lyrics()
    {
        return Some(l);
    }
    // 2. Search, keeping the closest duration and preferring synced lyrics.
    let url = format!(
        "https://lrclib.net/api/search?{}",
        encode(&[("artist_name", artist), ("track_name", title)])
    );
    let records: Vec<LrclibRecord> = client.get(&url).send().await.ok()?.json().await.ok()?;
    let target = q.duration_secs as f64;
    records
        .into_iter()
        .filter(|r| !r.instrumental)
        .filter(|r| target <= 0.0 || r.duration.is_none_or(|d| (d - target).abs() <= 8.0))
        .min_by(|a, b| {
            let score = |r: &LrclibRecord| {
                let d = r.duration.map_or(5.0, |d| (d - target).abs());
                d + if r.synced_lyrics.is_some() { 0.0 } else { 30.0 }
            };
            score(a).total_cmp(&score(b))
        })
        .and_then(LrclibRecord::into_lyrics)
}

#[derive(Deserialize)]
struct OvhResponse {
    #[serde(default)]
    lyrics: Option<String>,
}

async fn lyrics_ovh(client: &reqwest::Client, q: &Query) -> Option<Lyrics> {
    for artist in artist_variants(&q.artist) {
        if let Some(l) = lyrics_ovh_for(client, q, &artist).await {
            return Some(l);
        }
    }
    None
}

async fn lyrics_ovh_for(client: &reqwest::Client, q: &Query, artist: &str) -> Option<Lyrics> {
    let title = clean_title(&q.title);
    if title.is_empty() {
        return None;
    }
    let url = format!(
        "https://api.lyrics.ovh/v1/{}/{}",
        urlencoding::encode(artist),
        urlencoding::encode(&title)
    );
    let resp = client.get(&url).timeout(Duration::from_secs(12)).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let text = resp.json::<OvhResponse>().await.ok()?.lyrics?;
    // lyrics.ovh sometimes prepends "Paroles de la chanson X par Y".
    let text = match text.split_once('\n') {
        Some((first, rest)) if first.starts_with("Paroles de la chanson") => rest.to_string(),
        _ => text,
    };
    Lyrics::parse(&text, Provider::LyricsOvh)
}

fn encode(params: &[(&str, String)]) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{k}={}", urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_synced_lrc() {
        let lrc = "[ar:Someone]\n[offset:+500]\n[00:01.00]First\n[00:03.50][00:10.00]Chorus\n\n[00:05.123]<00:05.20>Word <00:05.80>by word";
        let l = Lyrics::parse(lrc, Provider::Lrclib).unwrap();
        assert!(l.synced);
        let texts: Vec<_> = l.lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, vec!["First", "Chorus", "Word by word", "Chorus"]);
        // Offset is subtracted.
        assert_eq!(l.lines[0].time, Some(Duration::from_millis(500)));
        assert_eq!(l.current_line(Duration::from_millis(400)), None);
        assert_eq!(l.current_line(Duration::from_secs(4)), Some(1));
        assert_eq!(l.current_line(Duration::from_secs(60)), Some(3));
    }

    #[test]
    fn parses_plain_text() {
        let l = Lyrics::parse("\n\nHello\r\n\r\nWorld\n", Provider::LyricsOvh).unwrap();
        assert!(!l.synced);
        assert_eq!(l.lines.len(), 3);
        assert_eq!(l.current_line(Duration::from_secs(1)), None);
        assert!(Lyrics::parse("  \n ", Provider::LyricsOvh).is_none());
    }

    #[test]
    fn lrc_roundtrip() {
        let l = Lyrics::parse("[01:02.30]a\n[01:05.00]b", Provider::Lrclib).unwrap();
        assert_eq!(l.to_text(), "[01:02.30]a\n[01:05.00]b");
    }

    #[test]
    fn cleans_titles_and_artists() {
        assert_eq!(clean_title("Song (feat. Someone)"), "Song");
        assert_eq!(clean_title("Song - Remastered 2011"), "Song");
        assert_eq!(clean_title("Love (Is All)"), "Love (Is All)");
        assert_eq!(main_artist("A, B & C"), "A");
        assert_eq!(main_artist("Daft Punk feat. Pharrell"), "Daft Punk");
        assert_eq!(artist_variants("Simon & Garfunkel"), vec!["Simon & Garfunkel", "Simon"]);
        assert_eq!(artist_variants("Solo"), vec!["Solo"]);
    }
}
