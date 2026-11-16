// SPDX-License-Identifier: GPL-3.0-only

//! Music library: track model, in-memory views (albums, artists, folders),
//! persistent index (SQLite + FTS5), recursive scanner and cover cache.

pub mod covers;
pub mod db;
pub mod scanner;
pub mod watcher;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// File extensions the player can decode.
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "flac", "mp3", "ogg", "oga", "opus", "m4a", "m4b", "mp4", "aac", "wav", "wave", "aif", "aiff", "aifc",
    "wv", "ape", "mka", "webm", "caf", "alac",
];

#[must_use]
pub fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

/// One indexed audio file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Track {
    pub id: i64,
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub genre: String,
    pub year: Option<i32>,
    pub track_no: Option<u32>,
    pub disc_no: Option<u32>,
    pub duration_ms: u64,
    pub bitrate: Option<u32>,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u8>,
    pub channels: Option<u8>,
    pub format: String,
    /// Key of the cached cover art (see [`covers`]).
    pub cover: Option<String>,
    /// ReplayGain track gain in dB.
    pub replay_gain: Option<f32>,
    pub musicbrainz_id: Option<String>,
    pub mtime: i64,
    pub size: i64,
    pub added_at: i64,
    pub play_count: u32,
    pub last_played: Option<i64>,
    pub favorite: bool,
}

impl Track {
    #[must_use]
    pub fn duration(&self) -> Duration {
        Duration::from_millis(self.duration_ms)
    }

    /// Artist used to group albums: album artist, then track artist.
    #[must_use]
    pub fn grouping_artist(&self) -> &str {
        if self.album_artist.is_empty() {
            &self.artist
        } else {
            &self.album_artist
        }
    }

    /// Short technical description, e.g. `FLAC · 24 bit · 96 kHz`.
    #[must_use]
    pub fn quality(&self) -> String {
        let mut parts = vec![self.format.to_uppercase()];
        if let Some(depth) = self.bit_depth.filter(|d| *d > 0) {
            parts.push(format!("{depth} bit"));
        }
        if let Some(rate) = self.sample_rate.filter(|r| *r > 0) {
            let khz = f64::from(rate) / 1000.0;
            parts.push(if (khz - khz.round()).abs() < 0.05 {
                format!("{khz:.0} kHz")
            } else {
                format!("{khz:.1} kHz")
            });
        }
        if self.bit_depth.is_none()
            && let Some(kbps) = self.bitrate.filter(|b| *b > 0)
        {
            parts.push(format!("{kbps} kbps"));
        }
        parts.join(" · ")
    }
}

/// Albums are identified by (album artist, album title).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AlbumKey {
    pub artist: String,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct Album {
    pub key: AlbumKey,
    pub year: Option<i32>,
    pub cover: Option<String>,
    pub genre: String,
    /// Track ids sorted by disc / track number.
    pub tracks: Vec<i64>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
pub struct Artist {
    pub name: String,
    pub albums: Vec<AlbumKey>,
    pub track_count: usize,
    pub cover: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Genre {
    pub name: String,
    pub tracks: Vec<i64>,
}

/// Sort orders for the songs list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SongSort {
    #[default]
    Artist,
    Title,
    Album,
    Duration,
    Added,
    Plays,
}

/// In-memory snapshot of the index, rebuilt after each scan.
#[derive(Debug, Default, Clone)]
pub struct Library {
    tracks: Vec<Arc<Track>>,
    by_id: HashMap<i64, usize>,
    pub albums: Vec<Album>,
    album_index: HashMap<AlbumKey, usize>,
    pub artists: Vec<Artist>,
    pub genres: Vec<Genre>,
}

const UNKNOWN: &str = "\u{0}";

fn sort_key(s: &str) -> String {
    let lower = s.to_lowercase();
    let trimmed = lower.strip_prefix("the ").unwrap_or(&lower);
    trimmed.trim().to_string()
}

fn album_key_of(t: &Track) -> AlbumKey {
    AlbumKey {
        artist: t.grouping_artist().to_string(),
        title: if t.album.is_empty() {
            UNKNOWN.to_string()
        } else {
            t.album.clone()
        },
    }
}

impl Library {
    #[must_use]
    pub fn new(mut tracks: Vec<Track>) -> Self {
        tracks.sort_by_cached_key(|t| {
            (
                sort_key(t.grouping_artist()),
                sort_key(&t.album),
                t.disc_no.unwrap_or(0),
                t.track_no.unwrap_or(0),
                sort_key(&t.title),
            )
        });
        let tracks: Vec<Arc<Track>> = tracks.into_iter().map(Arc::new).collect();
        let by_id = tracks.iter().enumerate().map(|(i, t)| (t.id, i)).collect();

        // Albums, keyed by normalised (artist, title) for stable ordering.
        let mut albums_map: BTreeMap<(String, String), Album> = BTreeMap::new();
        for t in &tracks {
            let key = album_key_of(t);
            let sort = (sort_key(&key.artist), sort_key(&key.title));
            let album = albums_map.entry(sort).or_insert_with(|| Album {
                key,
                year: None,
                cover: None,
                genre: String::new(),
                tracks: Vec::new(),
                duration_ms: 0,
            });
            album.tracks.push(t.id);
            album.duration_ms += t.duration_ms;
            if album.year.is_none() {
                album.year = t.year;
            }
            if album.cover.is_none() {
                album.cover.clone_from(&t.cover);
            }
            if album.genre.is_empty() {
                album.genre.clone_from(&t.genre);
            }
        }
        let albums: Vec<Album> = albums_map.into_values().collect();
        let album_index = albums
            .iter()
            .enumerate()
            .map(|(i, a)| (a.key.clone(), i))
            .collect();

        let mut artists_map: BTreeMap<String, Artist> = BTreeMap::new();
        for album in &albums {
            let artist = artists_map
                .entry(sort_key(&album.key.artist))
                .or_insert_with(|| Artist {
                    name: album.key.artist.clone(),
                    albums: Vec::new(),
                    track_count: 0,
                    cover: None,
                });
            artist.albums.push(album.key.clone());
            artist.track_count += album.tracks.len();
            if artist.cover.is_none() {
                artist.cover.clone_from(&album.cover);
            }
        }
        let artists = artists_map.into_values().collect();

        let mut genres_map: BTreeMap<String, Genre> = BTreeMap::new();
        for t in &tracks {
            for g in split_genres(&t.genre) {
                genres_map
                    .entry(sort_key(g))
                    .or_insert_with(|| Genre {
                        name: g.to_string(),
                        tracks: Vec::new(),
                    })
                    .tracks
                    .push(t.id);
            }
        }
        let genres = genres_map.into_values().collect();

        Self {
            tracks,
            by_id,
            albums,
            album_index,
            artists,
            genres,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    #[must_use]
    pub fn tracks(&self) -> &[Arc<Track>] {
        &self.tracks
    }

    #[must_use]
    pub fn get(&self, id: i64) -> Option<&Arc<Track>> {
        self.by_id.get(&id).map(|i| &self.tracks[*i])
    }

    #[must_use]
    pub fn album(&self, key: &AlbumKey) -> Option<&Album> {
        self.album_index.get(key).map(|i| &self.albums[*i])
    }

    #[must_use]
    pub fn album_of(&self, track: &Track) -> Option<&Album> {
        self.album(&album_key_of(track))
    }

    #[must_use]
    pub fn artist(&self, name: &str) -> Option<&Artist> {
        self.artists.iter().find(|a| a.name == name)
    }

    #[must_use]
    pub fn genre(&self, name: &str) -> Option<&Genre> {
        self.genres.iter().find(|g| g.name == name)
    }

    /// Updates a track in place (play count, favourite…).
    pub fn update<F: FnOnce(&mut Track)>(&mut self, id: i64, f: F) {
        if let Some(i) = self.by_id.get(&id) {
            f(Arc::make_mut(&mut self.tracks[*i]));
        }
    }

    /// Track ids sorted for the songs view.
    #[must_use]
    pub fn sorted_ids(&self, sort: SongSort, descending: bool) -> Vec<i64> {
        let mut ids: Vec<&Arc<Track>> = self.tracks.iter().collect();
        match sort {
            SongSort::Artist => {} // natural order
            SongSort::Title => ids.sort_by_cached_key(|t| sort_key(&t.title)),
            SongSort::Album => ids.sort_by_cached_key(|t| {
                (
                    sort_key(&t.album),
                    t.disc_no.unwrap_or(0),
                    t.track_no.unwrap_or(0),
                )
            }),
            SongSort::Duration => ids.sort_by_key(|t| t.duration_ms),
            SongSort::Added => ids.sort_by_key(|t| std::cmp::Reverse(t.added_at)),
            SongSort::Plays => ids.sort_by_key(|t| std::cmp::Reverse(t.play_count)),
        }
        if descending {
            ids.reverse();
        }
        ids.into_iter().map(|t| t.id).collect()
    }

    /// Direct sub-folders (with their recursive track count) and audio files of `dir`.
    #[must_use]
    pub fn folder_children(&self, dir: &Path) -> (Vec<(PathBuf, usize)>, Vec<i64>) {
        let mut folders: BTreeMap<PathBuf, usize> = BTreeMap::new();
        let mut files: Vec<&Arc<Track>> = Vec::new();
        for t in &self.tracks {
            let Ok(rest) = t.path.strip_prefix(dir) else {
                continue;
            };
            let mut comps = rest.components();
            let first = comps.next();
            if comps.next().is_none() {
                files.push(t);
            } else if let Some(first) = first {
                *folders.entry(dir.join(first)).or_default() += 1;
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        (
            folders.into_iter().collect(),
            files.into_iter().map(|t| t.id).collect(),
        )
    }

    /// All tracks under `dir` (recursively), in path order.
    #[must_use]
    pub fn tracks_under(&self, dir: &Path) -> Vec<i64> {
        let mut v: Vec<&Arc<Track>> = self.tracks.iter().filter(|t| t.path.starts_with(dir)).collect();
        v.sort_by(|a, b| a.path.cmp(&b.path));
        v.into_iter().map(|t| t.id).collect()
    }

    #[must_use]
    pub fn favorites(&self) -> Vec<i64> {
        self.tracks.iter().filter(|t| t.favorite).map(|t| t.id).collect()
    }

    #[must_use]
    pub fn total_duration(&self) -> Duration {
        Duration::from_millis(self.tracks.iter().map(|t| t.duration_ms).sum())
    }
}

/// Display name for an album title (unknown albums are stored as `\0`).
#[must_use]
pub fn album_title(title: &str) -> Option<&str> {
    (title != UNKNOWN).then_some(title)
}

/// Splits multi-valued genre strings (`Rock; Pop`, `Rock/Pop`).
pub fn split_genres(s: &str) -> impl Iterator<Item = &str> {
    s.split([';', '/', ',', '\u{0}'])
        .map(str::trim)
        .filter(|g| !g.is_empty())
}

/// `3:07` or `1:02:03`.
#[must_use]
pub fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let (h, m, s) = (secs / 3600, (secs / 60) % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// `12 min` or `3 h 05 min` (library totals).
#[must_use]
pub fn format_total(d: Duration) -> String {
    let mins = d.as_secs() / 60;
    if mins >= 60 {
        format!("{} h {:02} min", mins / 60, mins % 60)
    } else {
        format!("{mins} min")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_totals() {
        assert_eq!(format_total(Duration::from_secs(190)), "3 min");
        assert_eq!(format_total(Duration::from_secs(3 * 3600 + 5 * 60)), "3 h 05 min");
    }

    fn track(id: i64, artist: &str, album: &str, n: u32) -> Track {
        Track {
            id,
            path: PathBuf::from(format!("/music/{artist}/{album}/{n:02}.flac")),
            title: format!("Song {n}"),
            artist: artist.into(),
            album: album.into(),
            track_no: Some(n),
            duration_ms: 60_000,
            genre: "Rock; Pop".into(),
            ..Track::default()
        }
    }

    #[test]
    fn groups_albums_and_artists() {
        let lib = Library::new(vec![
            track(1, "B", "Two", 2),
            track(2, "A", "One", 1),
            track(3, "B", "Two", 1),
            track(4, "The C", "Three", 1),
        ]);
        assert_eq!(lib.albums.len(), 3);
        assert_eq!(lib.artists.len(), 3);
        let two = lib
            .album(&AlbumKey {
                artist: "B".into(),
                title: "Two".into(),
            })
            .unwrap();
        assert_eq!(two.tracks, vec![3, 1]);
        assert_eq!(two.duration_ms, 120_000);
        // "The C" sorts as "c", after "b".
        assert_eq!(lib.artists[2].name, "The C");
        assert_eq!(lib.genres.len(), 2);
    }

    #[test]
    fn folder_children_lists_direct_entries() {
        let lib = Library::new(vec![track(1, "A", "One", 1), track(2, "A", "Two", 1)]);
        let (folders, files) = lib.folder_children(Path::new("/music/A"));
        assert_eq!(folders.len(), 2);
        assert!(files.is_empty());
        let (folders, files) = lib.folder_children(Path::new("/music/A/One"));
        assert!(folders.is_empty());
        assert_eq!(files, vec![1]);
    }

    #[test]
    fn formats_durations() {
        assert_eq!(format_duration(Duration::from_secs(187)), "3:07");
        assert_eq!(format_duration(Duration::from_secs(3723)), "1:02:03");
    }

    #[test]
    fn audio_extension_detection() {
        assert!(is_audio_file(Path::new("/a/b.FLAC")));
        assert!(!is_audio_file(Path::new("/a/cover.jpg")));
    }
}
