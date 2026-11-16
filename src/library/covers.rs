// SPDX-License-Identifier: MIT

//! Cover art cache.
//!
//! Covers come from the pictures embedded in the audio files (FLAC
//! `METADATA_BLOCK_PICTURE`, ID3 `APIC`, MP4 `covr`, Vorbis comments…) or from an
//! image next to the files (`cover.jpg`, `folder.png`…). Each distinct image is
//! stored once, keyed by a hash of its bytes, in three sizes:
//!
//! - `<key>.jpg`   — large (now playing, MPRIS art)
//! - `<key>_t.jpg` — thumbnail (lists and grids)
//! - `<key>_b.jpg` — tiny blurred version used as an ambient background

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView};
use md5::{Digest, Md5};

pub const LARGE: u32 = 900;
pub const THUMB: u32 = 256;
const BLUR: u32 = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    Large,
    Thumb,
    Blur,
}

/// `~/.cache/cosmic-advanced-player/covers`.
pub fn cache_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = dirs::cache_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("cosmic-advanced-player")
            .join("covers");
        let _ = std::fs::create_dir_all(&dir);
        dir
    })
}

#[must_use]
pub fn path(key: &str, size: Size) -> PathBuf {
    let suffix = match size {
        Size::Large => "",
        Size::Thumb => "_t",
        Size::Blur => "_b",
    };
    cache_dir().join(format!("{key}{suffix}.jpg"))
}

#[must_use]
pub fn key_for(bytes: &[u8]) -> String {
    let digest = Md5::digest(bytes);
    digest.iter().take(10).map(|b| format!("{b:02x}")).collect()
}

/// Stores `bytes` (any common image format) in the cache and returns its key.
/// Existing entries are reused: only the first track of an album pays the cost.
pub fn store(bytes: &[u8]) -> Option<String> {
    let key = key_for(bytes);
    if path(&key, Size::Thumb).exists() && path(&key, Size::Large).exists() {
        return Some(key);
    }
    let img = image::load_from_memory(bytes).ok()?;
    if img.width() < 8 || img.height() < 8 {
        return None;
    }
    write_jpeg(&fit(&img, LARGE), &path(&key, Size::Large), 90)?;
    write_jpeg(&fit(&img, THUMB), &path(&key, Size::Thumb), 86)?;
    Some(key)
}

/// Stores an image file found next to the music.
pub fn store_file(file: &Path) -> Option<String> {
    let bytes = std::fs::read(file).ok()?;
    store(&bytes)
}

/// Returns the blurred background for a cover, creating it on first use.
pub fn blurred(key: &str) -> Option<PathBuf> {
    let out = path(key, Size::Blur);
    if out.exists() {
        return Some(out);
    }
    let img = image::open(path(key, Size::Thumb)).ok()?;
    let small = img.resize_to_fill(BLUR, BLUR, FilterType::Triangle);
    let blurred = image::imageops::fast_blur(&small.to_rgb8(), 3.0);
    write_jpeg(&DynamicImage::ImageRgb8(blurred), &out, 80)?;
    Some(out)
}

/// Average colour of a cover (r, g, b in 0..=1), used to tint the now-playing view.
pub fn dominant_color(key: &str) -> Option<[f32; 3]> {
    let img = image::open(path(key, Size::Thumb)).ok()?;
    let small = img.resize_exact(16, 16, FilterType::Triangle).to_rgb8();
    let (mut r, mut g, mut b, mut w) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for p in small.pixels() {
        let [pr, pg, pb] = p.0.map(|c| f32::from(c) / 255.0);
        let max = pr.max(pg).max(pb);
        let min = pr.min(pg).min(pb);
        // Favour saturated, mid-bright pixels over greys.
        let weight = 0.15 + (max - min) * (1.0 - (max - 0.6).abs());
        r += pr * weight;
        g += pg * weight;
        b += pb * weight;
        w += weight;
    }
    (w > 0.0).then(|| [r / w, g / w, b / w])
}

fn fit(img: &DynamicImage, max: u32) -> DynamicImage {
    let (w, h) = img.dimensions();
    if w <= max && h <= max {
        img.clone()
    } else {
        img.resize(max, max, FilterType::Lanczos3)
    }
}

fn write_jpeg(img: &DynamicImage, out: &Path, quality: u8) -> Option<()> {
    let mut buf = Vec::new();
    let rgb = img.to_rgb8();
    JpegEncoder::new_with_quality(Cursor::new(&mut buf), quality).encode_image(&rgb).ok()?;
    // Write atomically: several scanner threads may store the same album cover.
    let tmp = out.with_extension(format!("tmp{}", std::process::id() ^ thread_tag()));
    std::fs::write(&tmp, &buf).ok()?;
    std::fs::rename(&tmp, out).ok()
}

fn thread_tag() -> u32 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::thread::current().id().hash(&mut h);
    h.finish() as u32
}

const FOLDER_NAMES: &[&str] = &["cover", "folder", "front", "album", "albumart", "albumartsmall", "thumb"];
const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp"];

/// Finds a cover image next to the music files (`cover.jpg`, `Folder.png`…).
#[must_use]
pub fn find_folder_image(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(usize, PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        let Some(ext) = p.extension().and_then(|e| e.to_str()) else { continue };
        if !IMAGE_EXTS.iter().any(|x| x.eq_ignore_ascii_case(ext)) {
            continue;
        }
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_lowercase();
        let rank = FOLDER_NAMES
            .iter()
            .position(|n| stem == *n || stem.starts_with(&format!("{n}_")) || stem.starts_with(n))
            .unwrap_or(usize::MAX);
        if best.as_ref().is_none_or(|(r, _)| rank < *r) {
            best = Some((rank, p));
        }
    }
    // Any image will do if it is the only one in the folder.
    best.map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_cover_named_files() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["scan.png", "Folder.jpg", "cover.webp"] {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        let found = find_folder_image(dir.path()).unwrap();
        assert_eq!(found.file_name().unwrap(), "cover.webp");
    }

    #[test]
    fn keys_are_stable() {
        assert_eq!(key_for(b"abc"), key_for(b"abc"));
        assert_ne!(key_for(b"abc"), key_for(b"abd"));
        assert_eq!(key_for(b"abc").len(), 20);
    }
}
