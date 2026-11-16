// SPDX-License-Identifier: GPL-3.0-only

//! MPRIS2 D-Bus interface: media keys, the COSMIC panel media applet,
//! `playerctl`, KDE Connect…

use std::sync::{Arc, Mutex};

use cosmic::iced::Subscription;
use cosmic::iced::futures::SinkExt;
use mpris_server::zbus::{Result, fdo};
use mpris_server::{
    LoopStatus, Metadata, PlaybackRate, PlaybackStatus, PlayerInterface, Property, RootInterface, Server,
    Signal, Time, TrackId, Volume,
};
use tokio::sync::mpsc;

/// Requests coming from other applications.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Raise,
    Quit,
    Next,
    Previous,
    Play,
    Pause,
    PlayPause,
    Stop,
    /// Relative seek in microseconds.
    SeekBy(i64),
    /// Absolute position in microseconds.
    SetPosition(i64),
    Volume(f64),
    Shuffle(bool),
    Loop(LoopStatus),
}

#[derive(Debug, Clone)]
pub enum Event {
    Ready(Handle),
    Action(Action),
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub status: PlaybackStatus,
    pub metadata: Metadata,
    pub position_us: i64,
    pub volume: f64,
    pub shuffle: bool,
    pub loop_status: LoopStatus,
    pub can_next: bool,
    pub can_previous: bool,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            status: PlaybackStatus::Stopped,
            metadata: Metadata::new(),
            position_us: 0,
            volume: 1.0,
            shuffle: false,
            loop_status: LoopStatus::None,
            can_next: false,
            can_previous: false,
        }
    }
}

#[derive(Debug)]
enum Update {
    State(Box<Snapshot>),
    Seeked(i64),
}

/// Lets the app publish its state.
#[derive(Debug, Clone)]
pub struct Handle {
    state: Arc<Mutex<Snapshot>>,
    updates: mpsc::UnboundedSender<Update>,
}

impl Handle {
    /// Publishes a new state; only changed properties are signalled.
    pub fn update(&self, snapshot: Snapshot) {
        let _ = self.updates.send(Update::State(Box::new(snapshot)));
    }

    /// Updates the position silently (clients poll it).
    pub fn set_position(&self, position_us: i64) {
        if let Ok(mut s) = self.state.lock() {
            s.position_us = position_us;
        }
    }

    /// The position jumped (user seek): clients must be told.
    pub fn seeked(&self, position_us: i64) {
        self.set_position(position_us);
        let _ = self.updates.send(Update::Seeked(position_us));
    }
}

#[must_use]
pub fn track_id(id: i64) -> TrackId {
    let path = format!(
        "/io/github/kisakay/CosmicAdvancedPlayer/track/{}",
        id.unsigned_abs()
    );
    TrackId::try_from(path).unwrap_or(TrackId::NO_TRACK)
}

struct Imp {
    state: Arc<Mutex<Snapshot>>,
    actions: mpsc::UnboundedSender<Action>,
}

impl Imp {
    fn snapshot(&self) -> Snapshot {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }

    fn act(&self, action: Action) -> fdo::Result<()> {
        self.actions
            .send(action)
            .map_err(|e| fdo::Error::Failed(e.to_string()))
    }
}

impl RootInterface for Imp {
    async fn raise(&self) -> fdo::Result<()> {
        self.act(Action::Raise)
    }
    async fn quit(&self) -> fdo::Result<()> {
        self.act(Action::Quit)
    }
    async fn can_quit(&self) -> fdo::Result<bool> {
        Ok(true)
    }
    async fn fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }
    async fn set_fullscreen(&self, _fullscreen: bool) -> Result<()> {
        Ok(())
    }
    async fn can_set_fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }
    async fn can_raise(&self) -> fdo::Result<bool> {
        Ok(true)
    }
    async fn has_track_list(&self) -> fdo::Result<bool> {
        Ok(false)
    }
    async fn identity(&self) -> fdo::Result<String> {
        Ok("Cosmic Advanced Player".into())
    }
    async fn desktop_entry(&self) -> fdo::Result<String> {
        Ok("io.github.kisakay.CosmicAdvancedPlayer".into())
    }
    async fn supported_uri_schemes(&self) -> fdo::Result<Vec<String>> {
        Ok(vec!["file".into()])
    }
    async fn supported_mime_types(&self) -> fdo::Result<Vec<String>> {
        Ok([
            "audio/flac",
            "audio/mpeg",
            "audio/ogg",
            "audio/opus",
            "audio/wav",
            "audio/mp4",
            "audio/aac",
        ]
        .into_iter()
        .map(String::from)
        .collect())
    }
}

impl PlayerInterface for Imp {
    async fn next(&self) -> fdo::Result<()> {
        self.act(Action::Next)
    }
    async fn previous(&self) -> fdo::Result<()> {
        self.act(Action::Previous)
    }
    async fn pause(&self) -> fdo::Result<()> {
        self.act(Action::Pause)
    }
    async fn play_pause(&self) -> fdo::Result<()> {
        self.act(Action::PlayPause)
    }
    async fn stop(&self) -> fdo::Result<()> {
        self.act(Action::Stop)
    }
    async fn play(&self) -> fdo::Result<()> {
        self.act(Action::Play)
    }
    async fn seek(&self, offset: Time) -> fdo::Result<()> {
        self.act(Action::SeekBy(offset.as_micros()))
    }
    async fn set_position(&self, _track_id: TrackId, position: Time) -> fdo::Result<()> {
        self.act(Action::SetPosition(position.as_micros()))
    }
    async fn open_uri(&self, _uri: String) -> fdo::Result<()> {
        Err(fdo::Error::NotSupported("OpenUri".into()))
    }
    async fn playback_status(&self) -> fdo::Result<PlaybackStatus> {
        Ok(self.snapshot().status)
    }
    async fn loop_status(&self) -> fdo::Result<LoopStatus> {
        Ok(self.snapshot().loop_status)
    }
    async fn set_loop_status(&self, loop_status: LoopStatus) -> Result<()> {
        let _ = self.actions.send(Action::Loop(loop_status));
        Ok(())
    }
    async fn rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }
    async fn set_rate(&self, _rate: PlaybackRate) -> Result<()> {
        Ok(())
    }
    async fn shuffle(&self) -> fdo::Result<bool> {
        Ok(self.snapshot().shuffle)
    }
    async fn set_shuffle(&self, shuffle: bool) -> Result<()> {
        let _ = self.actions.send(Action::Shuffle(shuffle));
        Ok(())
    }
    async fn metadata(&self) -> fdo::Result<Metadata> {
        Ok(self.snapshot().metadata)
    }
    async fn volume(&self) -> fdo::Result<Volume> {
        Ok(self.snapshot().volume)
    }
    async fn set_volume(&self, volume: Volume) -> Result<()> {
        let _ = self.actions.send(Action::Volume(volume.clamp(0.0, 1.0)));
        Ok(())
    }
    async fn position(&self) -> fdo::Result<Time> {
        Ok(Time::from_micros(self.snapshot().position_us))
    }
    async fn minimum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }
    async fn maximum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }
    async fn can_go_next(&self) -> fdo::Result<bool> {
        Ok(self.snapshot().can_next)
    }
    async fn can_go_previous(&self) -> fdo::Result<bool> {
        Ok(self.snapshot().can_previous)
    }
    async fn can_play(&self) -> fdo::Result<bool> {
        Ok(true)
    }
    async fn can_pause(&self) -> fdo::Result<bool> {
        Ok(true)
    }
    async fn can_seek(&self) -> fdo::Result<bool> {
        Ok(self.snapshot().status != PlaybackStatus::Stopped)
    }
    async fn can_control(&self) -> fdo::Result<bool> {
        Ok(true)
    }
}

fn changed(old: &Snapshot, new: &Snapshot) -> Vec<Property> {
    let mut props = Vec::new();
    if old.status != new.status {
        props.push(Property::PlaybackStatus(new.status));
        props.push(Property::CanSeek(new.status != PlaybackStatus::Stopped));
    }
    if old.metadata != new.metadata {
        props.push(Property::Metadata(new.metadata.clone()));
    }
    if (old.volume - new.volume).abs() > f64::EPSILON {
        props.push(Property::Volume(new.volume));
    }
    if old.shuffle != new.shuffle {
        props.push(Property::Shuffle(new.shuffle));
    }
    if old.loop_status != new.loop_status {
        props.push(Property::LoopStatus(new.loop_status));
    }
    if old.can_next != new.can_next {
        props.push(Property::CanGoNext(new.can_next));
    }
    if old.can_previous != new.can_previous {
        props.push(Property::CanGoPrevious(new.can_previous));
    }
    props
}

/// Registers `org.mpris.MediaPlayer2.CosmicAdvancedPlayer` on the session bus.
pub fn subscription() -> Subscription<Event> {
    Subscription::run(|| {
        cosmic::iced::stream::channel(16, async move |mut output| {
            let state = Arc::new(Mutex::new(Snapshot::default()));
            let (action_tx, mut action_rx) = mpsc::unbounded_channel();
            let (update_tx, mut update_rx) = mpsc::unbounded_channel();
            let imp = Imp {
                state: state.clone(),
                actions: action_tx,
            };
            let server = match Server::new("CosmicAdvancedPlayer", imp).await {
                Ok(s) => s,
                Err(err) => {
                    eprintln!("MPRIS unavailable: {err}");
                    return std::future::pending().await;
                }
            };
            let _ = output
                .send(Event::Ready(Handle {
                    state: state.clone(),
                    updates: update_tx,
                }))
                .await;
            loop {
                tokio::select! {
                    Some(action) = action_rx.recv() => {
                        if output.send(Event::Action(action)).await.is_err() {
                            break;
                        }
                    }
                    Some(update) = update_rx.recv() => match update {
                        Update::State(new) => {
                            let props = {
                                let Ok(mut old) = state.lock() else { continue };
                                let props = changed(&old, &new);
                                *old = *new;
                                props
                            };
                            if !props.is_empty() {
                                let _ = server.properties_changed(props).await;
                            }
                        }
                        Update::Seeked(us) => {
                            let _ = server.emit(Signal::Seeked { position: Time::from_micros(us) }).await;
                        }
                    },
                    else => break,
                }
            }
        })
    })
}
