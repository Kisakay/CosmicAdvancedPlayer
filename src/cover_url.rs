// SPDX-License-Identifier: GPL-3.0-only

//! Public URL of a track's cover art, for the Discord presence.
//!
//! Discord can only show images it can download, so the embedded cover of a
//! local file has to be found online or published somewhere:
//!
//! 1. music catalogues, no account needed: Deezer, then iTunes, then Last.fm
//!    (when an API key is set) — results are checked against the artist;
//! 2. as a last resort (optional), the embedded cover itself is uploaded to
//!    a temporary host: litterbox.catbox.moe (deleted after 72 hours), or
//!    uguu.se (deleted after 3 hours) when litterbox refuses the upload.
//!
//! Results are cached on disk so a cover is looked up or uploaded only once.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::lastfm;
use crate::library::covers;
use crate::library::db::now;

const CATALOG_TTL: i64 = 30 * 24 * 3600;
/// Upload hosts delete files after 72 h (litterbox) or 3 h (uguu): reuse them a bit less long.
const LITTERBOX_TTL: i64 = 70 * 3600;
const UGUU_TTL: i64 = 150 * 60;
const MISS_TTL: i64 = 24 * 3600;
const LITTERBOX_URL: &str = "https://litterbox.catbox.moe/resources/internals/api.php";
const UGUU_URL: &str = "https://uguu.se/upload";

/// What we know about the playing track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub artist: String,
    pub album: String,
    pub title: String,
    /// Key of the local cover in the cache (embedded or folder image).
    pub cover: Option<String>,
}

impl Query {
    /// Cache key: the local picture when there is one (same art, same URL).
    #[must_use]
    pub fn key(&self) -> String {
        match &self.cover {
            Some(k) => format!("cover:{k}"),
            None => format!(
                "meta:{}\u{1f}{}\u{1f}{}",
                norm(&self.artist),
                norm(&self.album),
                norm(&self.title)
            ),
        }
    }
}

// --- Matching ---------------------------------------------------------------------------

/// Lower-case letters and digits only (accents and punctuation ignored).
fn norm(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ñ' => 'n',
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ý' | 'ÿ' => 'y',
            c => c,
        })
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Loose comparison: equal, or one contains the other ("Artist" vs "Artist & Friend").
fn same(found: &str, wanted: &str) -> bool {
    let (f, w) = (norm(found), norm(wanted));
    !f.is_empty() && !w.is_empty() && (f == w || f.contains(&w) || w.contains(&f))
}

fn artist_ok(found: &str, wanted: &str) -> bool {
    same(found, wanted) || same(found, &crate::lyrics::main_artist(wanted))
}

// --- Catalogues ---------------------------------------------------------------------------

async fn get_json(client: &reqwest::Client, url: &str, params: &[(&str, String)]) -> Option<Value> {
    client
        .get(url)
        .query(params)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()
}

async fn deezer(client: &reqwest::Client, q: &Query) -> Option<String> {
    if !q.album.is_empty() {
        let v = get_json(
            client,
            "https://api.deezer.com/search/album",
            &[
                (
                    "q",
                    format!("{} {}", q.artist, crate::lyrics::clean_title(&q.album)),
                ),
                ("limit", "10".into()),
            ],
        )
        .await?;
        let hit = v.get("data")?.as_array()?.iter().find(|a| {
            a.pointer("/artist/name")
                .and_then(Value::as_str)
                .is_some_and(|n| artist_ok(n, &q.artist))
                && a.get("title")
                    .and_then(Value::as_str)
                    .is_some_and(|t| same(t, &q.album))
        });
        if let Some(url) = hit.and_then(|a| a.get("cover_xl")).and_then(Value::as_str) {
            return Some(url.to_string());
        }
    }
    let v = get_json(
        client,
        "https://api.deezer.com/search",
        &[
            (
                "q",
                format!("{} {}", q.artist, crate::lyrics::clean_title(&q.title)),
            ),
            ("limit", "10".into()),
        ],
    )
    .await?;
    v.get("data")?
        .as_array()?
        .iter()
        .find(|t| {
            t.pointer("/artist/name")
                .and_then(Value::as_str)
                .is_some_and(|n| artist_ok(n, &q.artist))
                && t.get("title")
                    .and_then(Value::as_str)
                    .is_some_and(|t| same(t, &q.title))
        })
        .and_then(|t| t.pointer("/album/cover_xl"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

async fn itunes(client: &reqwest::Client, q: &Query) -> Option<String> {
    let (entity, name_field, wanted) = if q.album.is_empty() {
        ("song", "trackName", &q.title)
    } else {
        ("album", "collectionName", &q.album)
    };
    let v = get_json(
        client,
        "https://itunes.apple.com/search",
        &[
            ("media", "music".into()),
            ("entity", entity.into()),
            ("limit", "10".into()),
            ("term", format!("{} {}", q.artist, wanted)),
        ],
    )
    .await?;
    let art = v
        .get("results")?
        .as_array()?
        .iter()
        .find(|r| {
            r.get("artistName")
                .and_then(Value::as_str)
                .is_some_and(|n| artist_ok(n, &q.artist))
                && r.get(name_field)
                    .and_then(Value::as_str)
                    .is_some_and(|n| same(n, wanted))
        })?
        .get("artworkUrl100")?
        .as_str()?;
    Some(art.replace("100x100bb", "600x600bb"))
}

async fn post_cover(
    client: &reqwest::Client,
    url: &str,
    bytes: Vec<u8>,
    field: &str,
    extra: &[(&'static str, &'static str)],
) -> Option<String> {
    let part = reqwest::multipart::Part::bytes(bytes)
        .file_name("cover.jpg")
        .mime_str("image/jpeg")
        .ok()?;
    let mut form = reqwest::multipart::Form::new();
    for (k, v) in extra {
        form = form.text(*k, *v);
    }
    let resp = client
        .post(url)
        .multipart(form.part(field.to_string(), part))
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.text().await.ok()
}

/// Publishes the local cover on a temporary host; returns its URL and how long
/// it can be reused. litterbox keeps files 72 h but may refuse uploads (anti-abuse),
/// uguu.se keeps them 3 h.
async fn upload(client: &reqwest::Client, cover_key: &str) -> Option<(String, i64)> {
    let bytes = tokio::fs::read(covers::path(cover_key, covers::Size::Large))
        .await
        .ok()?;
    if let Some(body) = post_cover(
        client,
        LITTERBOX_URL,
        bytes.clone(),
        "fileToUpload",
        &[("reqtype", "fileupload"), ("time", "72h")],
    )
    .await
    {
        let url = body.trim();
        if url.starts_with("https://") {
            return Some((url.to_string(), LITTERBOX_TTL));
        }
    }
    let body = post_cover(client, UGUU_URL, bytes, "files[]", &[]).await?;
    let v: Value = serde_json::from_str(&body).ok()?;
    let url = v.pointer("/files/0/url")?.as_str()?;
    url.starts_with("https://").then(|| (url.to_string(), UGUU_TTL))
}

/// Finds a public URL for the cover of `q` (cached).
pub async fn resolve(
    client: &reqwest::Client,
    q: &Query,
    lastfm: Option<lastfm::Credentials>,
    allow_upload: bool,
) -> Option<String> {
    let key = q.key();
    if let Some(cached) = cache_get(&key, allow_upload) {
        return cached;
    }
    let mut url = deezer(client, q).await;
    if url.is_none() {
        url = itunes(client, q).await;
    }
    if url.is_none()
        && let Some(creds) = lastfm.filter(lastfm::Credentials::is_set)
    {
        url = lastfm::album_art_url(client, &creds, &q.artist, &q.album).await;
    }
    let mut ttl = CATALOG_TTL;
    if url.is_none()
        && allow_upload
        && let Some(cover) = &q.cover
        && let Some((uploaded, keep)) = upload(client, cover).await
    {
        url = Some(uploaded);
        ttl = keep;
    }
    cache_put(
        key,
        url.clone(),
        if url.is_some() { ttl } else { MISS_TTL },
        allow_upload,
    );
    url
}

// --- Disk cache -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    url: Option<String>,
    expires: i64,
    /// A miss recorded while uploads were disabled must not hide an upload later.
    #[serde(default)]
    upload_tried: bool,
}

fn cache_file() -> PathBuf {
    covers::cache_dir()
        .parent()
        .map_or_else(std::env::temp_dir, std::path::Path::to_path_buf)
        .join("discord-covers.json")
}

static CACHE: LazyLock<Mutex<HashMap<String, Entry>>> = LazyLock::new(|| {
    let map = std::fs::read(cache_file())
        .ok()
        .and_then(|b| serde_json::from_slice::<HashMap<String, Entry>>(&b).ok())
        .unwrap_or_default();
    Mutex::new(map)
});

/// `Some(url or none)` when a fresh answer is cached.
fn cache_get(key: &str, allow_upload: bool) -> Option<Option<String>> {
    let map = CACHE.lock().ok()?;
    let e = map.get(key)?;
    if e.expires <= now() || (e.url.is_none() && allow_upload && !e.upload_tried) {
        return None;
    }
    Some(e.url.clone())
}

fn cache_put(key: String, url: Option<String>, ttl: i64, upload_tried: bool) {
    let Ok(mut map) = CACHE.lock() else { return };
    let t = now();
    map.retain(|_, e| e.expires > t);
    map.insert(
        key,
        Entry {
            url,
            expires: t + ttl,
            upload_tried,
        },
    );
    if let Ok(json) = serde_json::to_vec(&*map) {
        let path = cache_file();
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_is_loose_but_safe() {
        assert!(same("Discovery", "discovery"));
        assert!(same("Daft Punk", "Daft Punk"));
        assert!(!same("Data Punk", "Daft Punk"));
        assert!(same("Bohémienne", "Bohemienne"));
        assert!(same("Papaoutai", "Papaoutai (Radio Edit)"));
        assert!(artist_ok("Daft Punk", "Daft Punk feat. Pharrell"));
        assert!(!same("", "x"));
    }

    /// Real network round-trip: `cargo test -- --ignored cover_url` (set XDG_CACHE_HOME).
    #[test]
    #[ignore = "uses the network"]
    fn resolves_real_covers() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let client = reqwest::Client::builder()
            .user_agent(crate::lyrics::USER_AGENT)
            .build()
            .unwrap();
        rt.block_on(async {
            let album = Query {
                artist: "Daft Punk".into(),
                album: "Discovery".into(),
                title: "One More Time".into(),
                cover: None,
            };
            let url = resolve(&client, &album, None, false).await;
            println!("album  -> {url:?}");
            assert!(url.is_some_and(|u| u.starts_with("https://")));

            let single = Query {
                artist: "Stromae".into(),
                album: String::new(),
                title: "Papaoutai".into(),
                cover: None,
            };
            let url = resolve(&client, &single, None, false).await;
            println!("single -> {url:?}");
            assert!(url.is_some());

            // Unknown to every catalogue: only the upload can provide a cover.
            let png = include_bytes!("../resources/icons/io.github.kisakay.CosmicAdvancedPlayer-1024.png");
            let key = covers::store(png).unwrap();
            let unknown = Query {
                artist: "Zzqx Imaginary Band 4242".into(),
                album: "Nothing Here 4242".into(),
                title: "Silence 4242".into(),
                cover: Some(key),
            };
            assert!(resolve(&client, &unknown, None, false).await.is_none());
            let url = resolve(&client, &unknown, None, true).await;
            println!("upload -> {url:?}");
            assert!(url.is_some_and(|u| u.starts_with("https://")));
        });
    }

    #[test]
    fn keys_prefer_the_local_picture() {
        let mut q = Query {
            artist: "A".into(),
            album: "B".into(),
            title: "C".into(),
            cover: Some("abc".into()),
        };
        assert_eq!(q.key(), "cover:abc");
        q.cover = None;
        assert_eq!(q.key(), "meta:a\u{1f}b\u{1f}c");
    }
}
