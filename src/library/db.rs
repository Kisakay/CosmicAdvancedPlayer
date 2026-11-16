// SPDX-License-Identifier: MIT

//! Persistent index: SQLite database with an FTS5 full-text index for search.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use super::Track;

const SCHEMA_VERSION: i32 = 1;

const SCHEMA: &str = r"
CREATE TABLE IF NOT EXISTS tracks (
    id           INTEGER PRIMARY KEY,
    path         TEXT NOT NULL UNIQUE,
    mtime        INTEGER NOT NULL,
    size         INTEGER NOT NULL,
    title        TEXT NOT NULL,
    artist       TEXT NOT NULL,
    album        TEXT NOT NULL,
    album_artist TEXT NOT NULL,
    genre        TEXT NOT NULL,
    year         INTEGER,
    track_no     INTEGER,
    disc_no      INTEGER,
    duration_ms  INTEGER NOT NULL,
    bitrate      INTEGER,
    sample_rate  INTEGER,
    bit_depth    INTEGER,
    channels     INTEGER,
    format       TEXT NOT NULL,
    cover        TEXT,
    replay_gain  REAL,
    mbid         TEXT,
    added_at     INTEGER NOT NULL,
    play_count   INTEGER NOT NULL DEFAULT 0,
    last_played  INTEGER,
    favorite     INTEGER NOT NULL DEFAULT 0
);

CREATE VIRTUAL TABLE IF NOT EXISTS tracks_fts USING fts5(
    title, artist, album, album_artist, genre, path,
    content='tracks', content_rowid='id',
    tokenize='unicode61 remove_diacritics 2'
);

CREATE TRIGGER IF NOT EXISTS tracks_ai AFTER INSERT ON tracks BEGIN
    INSERT INTO tracks_fts(rowid, title, artist, album, album_artist, genre, path)
    VALUES (new.id, new.title, new.artist, new.album, new.album_artist, new.genre, new.path);
END;
CREATE TRIGGER IF NOT EXISTS tracks_ad AFTER DELETE ON tracks BEGIN
    INSERT INTO tracks_fts(tracks_fts, rowid, title, artist, album, album_artist, genre, path)
    VALUES ('delete', old.id, old.title, old.artist, old.album, old.album_artist, old.genre, old.path);
END;
CREATE TRIGGER IF NOT EXISTS tracks_au AFTER UPDATE OF title, artist, album, album_artist, genre, path ON tracks BEGIN
    INSERT INTO tracks_fts(tracks_fts, rowid, title, artist, album, album_artist, genre, path)
    VALUES ('delete', old.id, old.title, old.artist, old.album, old.album_artist, old.genre, old.path);
    INSERT INTO tracks_fts(rowid, title, artist, album, album_artist, genre, path)
    VALUES (new.id, new.title, new.artist, new.album, new.album_artist, new.genre, new.path);
END;

CREATE TABLE IF NOT EXISTS scrobbles (
    id        INTEGER PRIMARY KEY,
    artist    TEXT NOT NULL,
    title     TEXT NOT NULL,
    album     TEXT NOT NULL,
    album_artist TEXT NOT NULL,
    duration  INTEGER NOT NULL,
    timestamp INTEGER NOT NULL
);
";

/// A pending Last.fm scrobble (kept until the API accepts it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingScrobble {
    pub id: i64,
    pub artist: String,
    pub title: String,
    pub album: String,
    pub album_artist: String,
    pub duration_secs: u64,
    pub timestamp: i64,
}

#[must_use]
pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// Default database location.
#[must_use]
pub fn default_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("cosmic-advanced-player")
        .join("library.db")
}

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Self> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version != SCHEMA_VERSION {
            conn.execute_batch(SCHEMA)?;
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(Self { conn })
    }

    /// `path -> (id, mtime, size)` for every indexed file.
    pub fn file_states(&self) -> rusqlite::Result<HashMap<PathBuf, (i64, i64, i64)>> {
        let mut stmt = self.conn.prepare_cached("SELECT id, path, mtime, size FROM tracks")?;
        let rows = stmt.query_map([], |r| {
            Ok((PathBuf::from(r.get::<_, String>(1)?), (r.get(0)?, r.get(2)?, r.get(3)?)))
        })?;
        rows.collect()
    }

    pub fn load_all(&self) -> rusqlite::Result<Vec<Track>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, path, mtime, size, title, artist, album, album_artist, genre, year,
                    track_no, disc_no, duration_ms, bitrate, sample_rate, bit_depth, channels,
                    format, cover, replay_gain, mbid, added_at, play_count, last_played, favorite
             FROM tracks",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Track {
                id: r.get(0)?,
                path: PathBuf::from(r.get::<_, String>(1)?),
                mtime: r.get(2)?,
                size: r.get(3)?,
                title: r.get(4)?,
                artist: r.get(5)?,
                album: r.get(6)?,
                album_artist: r.get(7)?,
                genre: r.get(8)?,
                year: r.get(9)?,
                track_no: r.get(10)?,
                disc_no: r.get(11)?,
                duration_ms: r.get::<_, i64>(12)?.max(0) as u64,
                bitrate: r.get(13)?,
                sample_rate: r.get(14)?,
                bit_depth: r.get(15)?,
                channels: r.get(16)?,
                format: r.get(17)?,
                cover: r.get(18)?,
                replay_gain: r.get(19)?,
                musicbrainz_id: r.get(20)?,
                added_at: r.get(21)?,
                play_count: r.get(22)?,
                last_played: r.get(23)?,
                favorite: r.get(24)?,
            })
        })?;
        rows.collect()
    }

    /// Inserts or refreshes the metadata of scanned tracks (keeps play statistics).
    pub fn upsert(&mut self, tracks: &[Track]) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO tracks (path, mtime, size, title, artist, album, album_artist, genre,
                    year, track_no, disc_no, duration_ms, bitrate, sample_rate, bit_depth, channels,
                    format, cover, replay_gain, mbid, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                    ?17, ?18, ?19, ?20, ?21)
                 ON CONFLICT(path) DO UPDATE SET
                    mtime = excluded.mtime, size = excluded.size, title = excluded.title,
                    artist = excluded.artist, album = excluded.album,
                    album_artist = excluded.album_artist, genre = excluded.genre,
                    year = excluded.year, track_no = excluded.track_no,
                    disc_no = excluded.disc_no, duration_ms = excluded.duration_ms,
                    bitrate = excluded.bitrate, sample_rate = excluded.sample_rate,
                    bit_depth = excluded.bit_depth, channels = excluded.channels,
                    format = excluded.format, cover = excluded.cover,
                    replay_gain = excluded.replay_gain, mbid = excluded.mbid",
            )?;
            for t in tracks {
                stmt.execute(params![
                    t.path.to_string_lossy(),
                    t.mtime,
                    t.size,
                    t.title,
                    t.artist,
                    t.album,
                    t.album_artist,
                    t.genre,
                    t.year,
                    t.track_no,
                    t.disc_no,
                    t.duration_ms as i64,
                    t.bitrate,
                    t.sample_rate,
                    t.bit_depth,
                    t.channels,
                    t.format,
                    t.cover,
                    t.replay_gain,
                    t.musicbrainz_id,
                    if t.added_at > 0 { t.added_at } else { now() },
                ])?;
            }
        }
        tx.commit()
    }

    pub fn remove_ids(&mut self, ids: &[i64]) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached("DELETE FROM tracks WHERE id = ?1")?;
            for id in ids {
                stmt.execute([id])?;
            }
        }
        tx.commit()
    }

    pub fn remove_paths(&mut self, paths: &[PathBuf]) -> rusqlite::Result<usize> {
        let tx = self.conn.transaction()?;
        let mut n = 0;
        {
            let mut stmt = tx.prepare_cached("DELETE FROM tracks WHERE path = ?1")?;
            for p in paths {
                n += stmt.execute([p.to_string_lossy()])?;
            }
        }
        tx.commit()?;
        Ok(n)
    }

    /// Full-text search. Every word is matched as a prefix, accents ignored.
    pub fn search(&self, query: &str, limit: usize) -> rusqlite::Result<Vec<i64>> {
        let Some(fts) = fts_query(query) else { return Ok(Vec::new()) };
        let mut stmt = self.conn.prepare_cached(
            "SELECT rowid FROM tracks_fts WHERE tracks_fts MATCH ?1
             ORDER BY bm25(tracks_fts, 10.0, 6.0, 4.0, 3.0, 1.0, 0.2) LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![fts, limit as i64], |r| r.get(0))?;
        rows.collect()
    }

    pub fn record_play(&self, id: i64, when: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE tracks SET play_count = play_count + 1, last_played = ?2 WHERE id = ?1",
            params![id, when],
        )?;
        Ok(())
    }

    pub fn set_favorite(&self, id: i64, favorite: bool) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE tracks SET favorite = ?2 WHERE id = ?1", params![id, favorite])?;
        Ok(())
    }

    pub fn clear(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch("DELETE FROM tracks; INSERT INTO tracks_fts(tracks_fts) VALUES('rebuild');")
    }

    // --- Last.fm offline queue -------------------------------------------------

    pub fn queue_scrobble(&self, s: &PendingScrobble) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO scrobbles (artist, title, album, album_artist, duration, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![s.artist, s.title, s.album, s.album_artist, s.duration_secs as i64, s.timestamp],
        )?;
        Ok(())
    }

    pub fn pending_scrobbles(&self, limit: usize) -> rusqlite::Result<Vec<PendingScrobble>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, artist, title, album, album_artist, duration, timestamp
             FROM scrobbles ORDER BY timestamp LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |r| {
            Ok(PendingScrobble {
                id: r.get(0)?,
                artist: r.get(1)?,
                title: r.get(2)?,
                album: r.get(3)?,
                album_artist: r.get(4)?,
                duration_secs: r.get::<_, i64>(5)?.max(0) as u64,
                timestamp: r.get(6)?,
            })
        })?;
        rows.collect()
    }

    pub fn delete_scrobbles(&self, ids: &[i64]) -> rusqlite::Result<()> {
        let mut stmt = self.conn.prepare_cached("DELETE FROM scrobbles WHERE id = ?1")?;
        for id in ids {
            stmt.execute([id])?;
        }
        Ok(())
    }

    pub fn pending_scrobble_count(&self) -> usize {
        self.conn
            .query_row("SELECT COUNT(*) FROM scrobbles", [], |r| r.get::<_, i64>(0))
            .optional()
            .ok()
            .flatten()
            .unwrap_or(0) as usize
    }
}

/// Turns user input into a safe FTS5 query: `foo bar` → `"foo"* "bar"*`.
#[must_use]
pub fn fts_query(input: &str) -> Option<String> {
    let terms: Vec<String> = input
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .map(|w| w.trim_matches('\''))
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{}\"*", w.replace('"', "")))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(path: &str, title: &str, artist: &str) -> Track {
        Track {
            path: PathBuf::from(path),
            title: title.into(),
            artist: artist.into(),
            album: "Album".into(),
            format: "flac".into(),
            mtime: 1,
            size: 2,
            duration_ms: 1000,
            ..Track::default()
        }
    }

    #[test]
    fn upsert_search_and_stats() {
        let mut db = Db::open_in_memory().unwrap();
        db.upsert(&[
            sample("/m/a.flac", "Bohémienne", "Ana"),
            sample("/m/b.flac", "Other song", "Kisakay"),
        ])
        .unwrap();
        let all = db.load_all().unwrap();
        assert_eq!(all.len(), 2);

        // Accent-insensitive prefix search.
        let hits = db.search("bohem", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(db.search("kisa", 10).unwrap().len(), 1);
        assert!(db.search("   ", 10).unwrap().is_empty());

        // Re-scanning updates metadata and keeps statistics.
        let id = hits[0];
        db.record_play(id, 42).unwrap();
        db.set_favorite(id, true).unwrap();
        db.upsert(&[sample("/m/a.flac", "Renamed", "Ana")]).unwrap();
        let t = db.load_all().unwrap().into_iter().find(|t| t.id == id).unwrap();
        assert_eq!(t.title, "Renamed");
        assert_eq!(t.play_count, 1);
        assert!(t.favorite);
        assert!(db.search("bohem", 10).unwrap().is_empty());
        assert_eq!(db.search("renam", 10).unwrap(), vec![id]);

        db.remove_paths(&[PathBuf::from("/m/a.flac")]).unwrap();
        assert_eq!(db.load_all().unwrap().len(), 1);
        assert!(db.search("renam", 10).unwrap().is_empty());
    }

    #[test]
    fn scrobble_queue_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        let s = PendingScrobble {
            id: 0,
            artist: "A".into(),
            title: "T".into(),
            album: String::new(),
            album_artist: String::new(),
            duration_secs: 200,
            timestamp: 1_700_000_000,
        };
        db.queue_scrobble(&s).unwrap();
        let pending = db.pending_scrobbles(50).unwrap();
        assert_eq!(pending.len(), 1);
        db.delete_scrobbles(&[pending[0].id]).unwrap();
        assert_eq!(db.pending_scrobble_count(), 0);
    }

    #[test]
    fn fts_query_is_sanitised() {
        assert_eq!(fts_query("foo \"bar"), Some("\"foo\"* \"bar\"*".into()));
        assert_eq!(fts_query("AND OR -"), Some("\"AND\"* \"OR\"*".into()));
        assert_eq!(fts_query(" ** "), None);
    }
}
