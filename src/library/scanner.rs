// SPDX-License-Identifier: GPL-3.0-only

//! Recursive, incremental library scanner.
//!
//! Folders are walked recursively (symbolic links are followed, loops and
//! duplicates reached through several links are detected). Only new or modified
//! files (size / mtime) are parsed, in parallel; files that disappeared are
//! removed from the index.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use lofty::config::ParseOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::PictureType;
use lofty::prelude::{Accessor, ItemKey};
use lofty::probe::Probe;
use lofty::tag::Tag;
use rayon::prelude::*;
use walkdir::WalkDir;

use super::db::{self, Db};
use super::{Track, covers, is_audio_file};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanSummary {
    pub total: usize,
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
}

/// Lists the audio files below `roots`, canonicalised and de-duplicated.
#[must_use]
pub fn collect_files(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    for root in roots {
        let walker = WalkDir::new(root)
            .follow_links(true)
            .into_iter()
            .filter_entry(|e| {
                // Skip hidden folders (.git, .Trash…) but never the root itself.
                e.depth() == 0 || !e.file_name().to_str().is_some_and(|n| n.starts_with('.'))
            });
        for entry in walker.flatten() {
            if !entry.file_type().is_file() || !is_audio_file(entry.path()) {
                continue;
            }
            let path = std::fs::canonicalize(entry.path()).unwrap_or_else(|_| entry.path().to_path_buf());
            if seen.insert(path.clone()) {
                files.push(path);
            }
        }
    }
    files
}

fn file_state(path: &Path) -> Option<(i64, i64)> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64);
    Some((mtime, meta.len() as i64))
}

/// Folder covers are looked up once per directory.
#[derive(Default)]
pub struct FolderCovers(Mutex<HashMap<PathBuf, Option<String>>>);

impl FolderCovers {
    fn get(&self, dir: &Path) -> Option<String> {
        if let Some(hit) = self.0.lock().ok()?.get(dir) {
            return hit.clone();
        }
        let key = covers::find_folder_image(dir).and_then(|p| covers::store_file(&p));
        if let Ok(mut map) = self.0.lock() {
            map.insert(dir.to_path_buf(), key.clone());
        }
        key
    }
}

fn tag_string(tag: &Tag, key: ItemKey) -> Option<String> {
    tag.get_string(key)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn parse_gain(s: &str) -> Option<f32> {
    s.trim()
        .trim_end_matches(|c: char| c.is_alphabetic() || c.is_whitespace())
        .trim()
        .parse()
        .ok()
}

fn parse_year(s: &str) -> Option<i32> {
    let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
    digits.get(..4)?.parse().ok()
}

/// Reads the metadata of one file. Never fails: unreadable tags fall back to the file name.
pub fn read_track(path: &Path, folder_covers: &FolderCovers) -> Option<Track> {
    let (mtime, size) = file_state(path)?;
    let format = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase();
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut track = Track {
        path: path.to_path_buf(),
        title: stem,
        format,
        mtime,
        size,
        ..Track::default()
    };

    let tagged = Probe::open(path)
        .ok()
        .and_then(|p| p.guess_file_type().ok())
        .and_then(|p| p.options(ParseOptions::new().read_cover_art(true)).read().ok());

    let mut cover = None;
    if let Some(tagged) = tagged {
        let props = tagged.properties();
        track.duration_ms = props.duration().as_millis() as u64;
        track.bitrate = props.audio_bitrate().or(props.overall_bitrate());
        track.sample_rate = props.sample_rate();
        track.bit_depth = props.bit_depth();
        track.channels = props.channels();

        if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
            if let Some(t) = tag
                .title()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
            {
                track.title = t;
            }
            let artists: Vec<&str> = tag
                .get_strings(ItemKey::TrackArtist)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            track.artist = if artists.len() > 1 {
                artists.join(", ")
            } else {
                tag.artist().map(|s| s.trim().to_string()).unwrap_or_default()
            };
            track.album = tag.album().map(|s| s.trim().to_string()).unwrap_or_default();
            track.album_artist = tag_string(tag, ItemKey::AlbumArtist).unwrap_or_default();
            let genres: Vec<&str> = tag
                .get_strings(ItemKey::Genre)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            track.genre = genres.join("; ");
            track.year = tag
                .date()
                .map(|d| i32::from(d.year))
                .filter(|y| *y > 0)
                .or_else(|| tag_string(tag, ItemKey::Year).as_deref().and_then(parse_year))
                .or_else(|| {
                    tag_string(tag, ItemKey::RecordingDate)
                        .as_deref()
                        .and_then(parse_year)
                });
            track.track_no = tag.track();
            track.disc_no = tag.disk();
            track.replay_gain = tag_string(tag, ItemKey::ReplayGainTrackGain)
                .as_deref()
                .and_then(parse_gain);
            track.musicbrainz_id = tag_string(tag, ItemKey::MusicBrainzRecordingId);
        }

        // Embedded cover: front cover first, then any picture, from any tag.
        let pictures: Vec<_> = tagged.tags().iter().flat_map(Tag::pictures).collect();
        let picture = pictures
            .iter()
            .find(|p| p.pic_type() == PictureType::CoverFront)
            .or_else(|| pictures.first());
        if let Some(pic) = picture {
            cover = covers::store(pic.data());
        }
    }
    if cover.is_none()
        && let Some(dir) = path.parent()
    {
        cover = folder_covers.get(dir);
    }
    track.cover = cover;
    Some(track)
}

/// Scans `roots` and updates the index. `progress(done, total)` is called regularly.
pub fn scan(
    db: &mut Db,
    roots: &[PathBuf],
    force: bool,
    progress: &(dyn Fn(usize, usize) + Sync),
) -> rusqlite::Result<ScanSummary> {
    let roots: Vec<PathBuf> = roots
        .iter()
        .map(|r| std::fs::canonicalize(r).unwrap_or_else(|_| r.clone()))
        .collect();
    let known = db.file_states()?;
    let files = collect_files(&roots);
    let found: HashSet<&PathBuf> = files.iter().collect();

    let changed: Vec<&PathBuf> = files
        .iter()
        .filter(|p| {
            force
                || match (known.get(*p), file_state(p)) {
                    (Some((_, mtime, size)), Some((m, s))) => *mtime != m || *size != s,
                    _ => true,
                }
        })
        .collect();

    let total = changed.len();
    progress(0, total);
    let done = AtomicUsize::new(0);
    let folder_covers = FolderCovers::default();
    let now = db::now();
    let tracks: Vec<Track> = changed
        .par_iter()
        .filter_map(|p| {
            let t = read_track(p, &folder_covers);
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n.is_multiple_of(16) || n == total {
                progress(n, total);
            }
            t
        })
        .map(|mut t| {
            t.added_at = now;
            t
        })
        .collect();

    let added = tracks.iter().filter(|t| !known.contains_key(&t.path)).count();
    let updated = tracks.len() - added;
    db.upsert(&tracks)?;

    // Remove files that vanished or are no longer below a library folder.
    let removed: Vec<i64> = known
        .iter()
        .filter(|(p, _)| !found.contains(p))
        .map(|(_, (id, _, _))| *id)
        .collect();
    db.remove_ids(&removed)?;

    Ok(ScanSummary {
        total: files.len(),
        added,
        updated,
        removed: removed.len(),
    })
}

/// Reads files that are not part of the library (opened folder, command line).
/// They get negative ids so they never collide with indexed tracks.
#[must_use]
pub fn read_transient(paths: &[PathBuf], first_id: i64) -> Vec<Track> {
    let mut files = Vec::new();
    for p in paths {
        if p.is_dir() {
            files.extend(collect_files(std::slice::from_ref(p)));
        } else if is_audio_file(p) {
            files.push(std::fs::canonicalize(p).unwrap_or_else(|_| p.clone()));
        }
    }
    let folder_covers = FolderCovers::default();
    let mut tracks: Vec<Track> = files
        .par_iter()
        .filter_map(|p| read_track(p, &folder_covers))
        .collect();
    // Album order within each folder, folders in path order.
    tracks.sort_by(|a, b| {
        (a.path.parent(), a.disc_no, a.track_no, &a.path).cmp(&(
            b.path.parent(),
            b.disc_no,
            b.track_no,
            &b.path,
        ))
    });
    for (i, t) in tracks.iter_mut().enumerate() {
        t.id = first_id - i as i64;
    }
    tracks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gain_and_year() {
        assert_eq!(parse_gain("-6.48 dB"), Some(-6.48));
        assert_eq!(parse_gain("+1.20 dB"), Some(1.2));
        assert_eq!(parse_year("2019-04-12"), Some(2019));
        assert_eq!(parse_year("19"), None);
    }

    #[test]
    fn collects_recursively_through_symlinks_without_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a/b/c");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::write(a.join("song.flac"), b"").unwrap();
        std::fs::write(a.join("notes.txt"), b"").unwrap();
        std::fs::create_dir_all(dir.path().join(".hidden")).unwrap();
        std::fs::write(dir.path().join(".hidden/x.mp3"), b"").unwrap();
        // A link back to an ancestor (loop) and a second link to the same folder.
        std::os::unix::fs::symlink(dir.path().join("a"), a.join("loop")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("a/b"), dir.path().join("alias")).unwrap();

        let files = collect_files(&[dir.path().to_path_buf()]);
        assert_eq!(files.len(), 1, "{files:?}");
    }

    #[test]
    fn unreadable_file_falls_back_to_file_name() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("My Song.mp3");
        std::fs::write(&p, b"not really audio").unwrap();
        let t = read_track(&p, &FolderCovers::default()).unwrap();
        assert_eq!(t.title, "My Song");
        assert_eq!(t.format, "mp3");
    }
}
