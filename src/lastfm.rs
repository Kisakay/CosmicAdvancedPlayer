// SPDX-License-Identifier: GPL-3.0-only

//! Last.fm Scrobbling 2.0 API client.
//!
//! Authentication uses the desktop flow: request a token, let the user approve
//! it in the browser, then exchange it for a session key. Every write call is
//! signed with the API secret (`api_sig` = md5 of the sorted parameters).

use std::collections::BTreeMap;
use std::time::Duration;

use md5::{Digest, Md5};
use serde_json::Value;

use crate::library::db::PendingScrobble;

const API: &str = "https://ws.audioscrobbler.com/2.0/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub api_key: String,
    pub secret: String,
}

impl Credentials {
    #[must_use]
    pub fn is_set(&self) -> bool {
        !self.api_key.trim().is_empty() && !self.secret.trim().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The session key is no longer valid: the user must reconnect.
    InvalidSession,
    /// Temporary failure, try again later.
    Retry(String),
    Api(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSession => f.write_str("Last.fm session expired"),
            Self::Retry(m) | Self::Api(m) => f.write_str(m),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Track information sent to Last.fm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackInfo {
    pub artist: String,
    pub title: String,
    pub album: String,
    pub album_artist: String,
    pub duration_secs: u64,
}

/// Last.fm rule: tracks over 30 s, scrobbled after half their length or 4 minutes.
#[must_use]
pub fn scrobble_threshold(duration: Duration) -> Option<Duration> {
    (duration > Duration::from_secs(30)).then(|| (duration / 2).min(Duration::from_secs(240)))
}

#[must_use]
pub fn sign(params: &BTreeMap<String, String>, secret: &str) -> String {
    let mut h = Md5::new();
    for (k, v) in params {
        if k != "format" && k != "callback" {
            h.update(k.as_bytes());
            h.update(v.as_bytes());
        }
    }
    h.update(secret.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

#[must_use]
pub fn auth_url(api_key: &str, token: &str) -> String {
    format!("https://www.last.fm/api/auth/?api_key={api_key}&token={token}")
}

async fn call(
    client: &reqwest::Client,
    creds: &Credentials,
    method: &str,
    mut params: BTreeMap<String, String>,
    signed: bool,
    post: bool,
) -> Result<Value> {
    params.insert("method".into(), method.into());
    params.insert("api_key".into(), creds.api_key.trim().into());
    if signed {
        let sig = sign(&params, creds.secret.trim());
        params.insert("api_sig".into(), sig);
    }
    params.insert("format".into(), "json".into());

    let req = if post {
        client.post(API).form(&params)
    } else {
        client.get(API).query(&params)
    };
    let resp = req
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| Error::Retry(e.to_string()))?;
    let status = resp.status();
    let body: Value = resp
        .json()
        .await
        .map_err(|e| Error::Retry(format!("{status}: {e}")))?;
    if let Some(code) = body.get("error").and_then(Value::as_i64) {
        let message = body
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Last.fm error")
            .to_string();
        return Err(match code {
            9 | 4 | 14 | 15 => Error::InvalidSession,
            8 | 11 | 16 | 29 => Error::Retry(message),
            _ => Error::Api(message),
        });
    }
    if status.is_server_error() {
        return Err(Error::Retry(status.to_string()));
    }
    Ok(body)
}

/// Step 1: get a request token the user has to approve.
pub async fn get_token(client: &reqwest::Client, creds: &Credentials) -> Result<String> {
    let v = call(client, creds, "auth.getToken", BTreeMap::new(), true, false).await?;
    v.get("token")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| Error::Api("no token".into()))
}

/// Step 2: exchange the approved token for `(username, session key)`.
pub async fn get_session(
    client: &reqwest::Client,
    creds: &Credentials,
    token: &str,
) -> Result<(String, String)> {
    let params = BTreeMap::from([("token".to_string(), token.to_string())]);
    let v = call(client, creds, "auth.getSession", params, true, false).await?;
    let session = v.get("session").ok_or_else(|| Error::Api("no session".into()))?;
    let name = session
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let key = session
        .get("key")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Api("no key".into()))?;
    Ok((name, key.to_string()))
}

fn track_params(t: &TrackInfo, suffix: &str, params: &mut BTreeMap<String, String>) {
    params.insert(format!("artist{suffix}"), t.artist.clone());
    params.insert(format!("track{suffix}"), t.title.clone());
    if !t.album.is_empty() {
        params.insert(format!("album{suffix}"), t.album.clone());
    }
    if !t.album_artist.is_empty() && t.album_artist != t.artist {
        params.insert(format!("albumArtist{suffix}"), t.album_artist.clone());
    }
    if t.duration_secs > 0 {
        params.insert(format!("duration{suffix}"), t.duration_secs.to_string());
    }
}

pub async fn now_playing(
    client: &reqwest::Client,
    creds: &Credentials,
    sk: &str,
    t: &TrackInfo,
) -> Result<()> {
    let mut params = BTreeMap::from([("sk".to_string(), sk.to_string())]);
    track_params(t, "", &mut params);
    call(client, creds, "track.updateNowPlaying", params, true, true)
        .await
        .map(|_| ())
}

/// Sends up to 50 scrobbles; returns the ids that Last.fm accepted or ignored for good.
pub async fn scrobble(
    client: &reqwest::Client,
    creds: &Credentials,
    sk: &str,
    batch: &[PendingScrobble],
) -> Result<Vec<i64>> {
    let mut params = BTreeMap::from([("sk".to_string(), sk.to_string())]);
    for (i, s) in batch.iter().take(50).enumerate() {
        let info = TrackInfo {
            artist: s.artist.clone(),
            title: s.title.clone(),
            album: s.album.clone(),
            album_artist: s.album_artist.clone(),
            duration_secs: s.duration_secs,
        };
        let suffix = format!("[{i}]");
        track_params(&info, &suffix, &mut params);
        params.insert(format!("timestamp{suffix}"), s.timestamp.to_string());
    }
    call(client, creds, "track.scrobble", params, true, true).await?;
    Ok(batch.iter().take(50).map(|s| s.id).collect())
}

pub async fn love(
    client: &reqwest::Client,
    creds: &Credentials,
    sk: &str,
    artist: &str,
    title: &str,
    loved: bool,
) -> Result<()> {
    let params = BTreeMap::from([
        ("sk".to_string(), sk.to_string()),
        ("artist".to_string(), artist.to_string()),
        ("track".to_string(), title.to_string()),
    ]);
    let method = if loved { "track.love" } else { "track.unlove" };
    call(client, creds, method, params, true, true).await.map(|_| ())
}

/// Public URL of an album cover (used for the Discord presence).
pub async fn album_art_url(
    client: &reqwest::Client,
    creds: &Credentials,
    artist: &str,
    album: &str,
) -> Option<String> {
    if album.is_empty() {
        return None;
    }
    let params = BTreeMap::from([
        ("artist".to_string(), artist.to_string()),
        ("album".to_string(), album.to_string()),
        ("autocorrect".to_string(), "1".to_string()),
    ]);
    let v = call(client, creds, "album.getInfo", params, false, false)
        .await
        .ok()?;
    v.pointer("/album/image")?
        .as_array()?
        .iter()
        .rev()
        .filter_map(|img| img.get("#text").and_then(Value::as_str))
        .find(|url| !url.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_matches_reference() {
        // Reference: md5("api_keyxxxmethodauth.getSessiontokenyyy" + "secret")
        let params = BTreeMap::from([
            ("api_key".to_string(), "xxx".to_string()),
            ("method".to_string(), "auth.getSession".to_string()),
            ("token".to_string(), "yyy".to_string()),
            ("format".to_string(), "json".to_string()),
        ]);
        let expected: String = Md5::digest(b"api_keyxxxmethodauth.getSessiontokenyyysecret")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(sign(&params, "secret"), expected);
    }

    #[test]
    fn threshold_rules() {
        assert_eq!(scrobble_threshold(Duration::from_secs(20)), None);
        assert_eq!(
            scrobble_threshold(Duration::from_secs(180)),
            Some(Duration::from_secs(90))
        );
        assert_eq!(
            scrobble_threshold(Duration::from_secs(1200)),
            Some(Duration::from_secs(240))
        );
    }
}
