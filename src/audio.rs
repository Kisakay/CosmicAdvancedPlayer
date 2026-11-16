// SPDX-License-Identifier: GPL-3.0-only

//! Playback engine.
//!
//! A dedicated thread owns the audio output (rodio + symphonia) and receives
//! [`Command`]s. It reports [`Event`]s back to the UI through a subscription.
//! The next track is appended to the same player a few seconds before the end
//! of the current one, which gives gapless transitions.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use cosmic::iced::Subscription;
use cosmic::iced::futures::SinkExt;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};

const TICK: Duration = Duration::from_millis(200);

#[derive(Debug, Clone)]
pub enum Command {
    /// Starts playing `path` (replaces whatever is playing).
    Load {
        id: i64,
        path: PathBuf,
        gain: f32,
        paused: bool,
        start_at: Duration,
    },
    /// Queues `path` right after the current track (gapless).
    Preload {
        id: i64,
        path: PathBuf,
        gain: f32,
    },
    /// Forgets a preloaded track (the queue changed).
    CancelPreload,
    Play,
    Pause,
    Stop,
    Seek(Duration),
    /// Linear volume 0..=1 (a perceptual curve is applied by the engine).
    Volume(f32),
}

#[derive(Debug, Clone)]
pub enum Event {
    Ready(Handle),
    /// A track started (after `Load`).
    Started {
        id: i64,
        duration: Option<Duration>,
    },
    /// The preloaded track took over without a gap.
    Advanced {
        id: i64,
    },
    Position(Duration),
    /// Nothing left to play.
    Ended,
    /// The track could not be decoded.
    Failed {
        id: i64,
        error: String,
    },
    /// The audio device could not be opened.
    DeviceError(String),
}

/// Cheap clonable handle to the engine thread.
#[derive(Debug, Clone)]
pub struct Handle(mpsc::Sender<Command>);

impl Handle {
    pub fn send(&self, command: Command) {
        let _ = self.0.send(command);
    }
}

/// Perceptual volume curve.
#[must_use]
pub fn amplitude(volume: f32) -> f32 {
    let v = volume.clamp(0.0, 1.0);
    v * v
}

/// Converts a ReplayGain value (dB) to a linear factor, clamped to avoid surprises.
#[must_use]
pub fn gain_factor(db: Option<f32>, preamp_db: f32) -> f32 {
    db.map_or(1.0, |g| 10f32.powf((g + preamp_db).clamp(-24.0, 12.0) / 20.0))
}

fn open(path: &Path) -> Result<Decoder<BufReader<File>>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    let mut builder = Decoder::builder()
        .with_data(BufReader::new(file))
        .with_byte_len(len)
        .with_seekable(true)
        .with_gapless(true);
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        builder = builder.with_hint(ext);
    }
    builder.build().map_err(|e| e.to_string())
}

struct Engine {
    events: tokio::sync::mpsc::UnboundedSender<Event>,
    device: Option<MixerDeviceSink>,
    player: Option<Player>,
    current: Option<i64>,
    next: Option<i64>,
    next_cancelled: bool,
    volume: f32,
    /// What is loaded, to reopen it when the decoder cannot seek.
    source: Option<(PathBuf, f32)>,
}

impl Engine {
    fn emit(&self, event: Event) {
        let _ = self.events.send(event);
    }

    fn device(&mut self) -> Option<&MixerDeviceSink> {
        if self.device.is_none() {
            match DeviceSinkBuilder::open_default_sink() {
                Ok(mut sink) => {
                    sink.log_on_drop(false);
                    self.device = Some(sink);
                }
                Err(err) => {
                    self.emit(Event::DeviceError(err.to_string()));
                    return None;
                }
            }
        }
        self.device.as_ref()
    }

    fn reset(&mut self) {
        self.player = None;
        self.current = None;
        self.next = None;
        self.next_cancelled = false;
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::Load {
                id,
                path,
                gain,
                paused,
                start_at,
            } => {
                self.reset();
                self.source = Some((path.clone(), gain));
                let decoder = match open(&path) {
                    Ok(d) => d,
                    Err(error) => return self.emit(Event::Failed { id, error }),
                };
                let duration = decoder.total_duration();
                let volume = amplitude(self.volume);
                let Some(device) = self.device() else { return };
                let player = Player::connect_new(device.mixer());
                player.set_volume(volume);
                if paused {
                    player.pause();
                }
                player.append(decoder.amplify(gain));
                if !start_at.is_zero() {
                    let _ = player.try_seek(start_at);
                }
                self.player = Some(player);
                self.current = Some(id);
                self.emit(Event::Started { id, duration });
            }
            Command::Preload { id, path, gain } => {
                let Some(player) = &self.player else { return };
                if self.next.is_some() || player.len() != 1 {
                    return;
                }
                match open(&path) {
                    Ok(decoder) => {
                        player.append(decoder.amplify(gain));
                        self.next = Some(id);
                        self.next_cancelled = false;
                    }
                    // Not fatal: the UI falls back to a regular Load at the end.
                    Err(err) => eprintln!("preload failed for {}: {err}", path.display()),
                }
            }
            Command::CancelPreload => {
                // rodio cannot remove a queued source; remember to stop when it starts.
                if self.next.is_some() {
                    self.next_cancelled = true;
                }
            }
            Command::Play => {
                if let Some(p) = &self.player {
                    p.play();
                }
            }
            Command::Pause => {
                if let Some(p) = &self.player {
                    p.pause();
                }
            }
            Command::Stop => self.reset(),
            Command::Seek(pos) => {
                let Some(p) = &self.player else { return };
                match p.try_seek(pos) {
                    Ok(()) => self.emit(Event::Position(p.get_pos())),
                    Err(err) => {
                        // Some decoders (Vorbis…) refuse to seek backwards: reopen the
                        // file and start again from the requested position.
                        eprintln!("seek failed ({err}), reopening the track");
                        let paused = p.is_paused();
                        if let (Some(id), Some((path, gain))) = (self.current, self.source.clone()) {
                            self.handle(Command::Load {
                                id,
                                path,
                                gain,
                                paused,
                                start_at: pos,
                            });
                        }
                    }
                }
            }
            Command::Volume(v) => {
                self.volume = v;
                if let Some(p) = &self.player {
                    p.set_volume(amplitude(v));
                }
            }
        }
    }

    fn tick(&mut self) {
        let Some(player) = &self.player else { return };
        let len = player.len();
        if len == 1
            && let Some(next) = self.next.take()
        {
            // The current source finished and the queued one took over.
            if std::mem::take(&mut self.next_cancelled) {
                self.reset();
                self.emit(Event::Ended);
            } else {
                self.current = Some(next);
                self.emit(Event::Advanced { id: next });
            }
            return;
        }
        if len == 0 {
            if self.current.is_some() {
                self.reset();
                self.emit(Event::Ended);
            }
            return;
        }
        if !player.is_paused() {
            self.emit(Event::Position(player.get_pos()));
        }
    }
}

fn run(commands: mpsc::Receiver<Command>, events: tokio::sync::mpsc::UnboundedSender<Event>) {
    let mut engine = Engine {
        events,
        device: None,
        player: None,
        current: None,
        next: None,
        next_cancelled: false,
        volume: 1.0,
        source: None,
    };
    loop {
        match commands.recv_timeout(TICK) {
            Ok(command) => {
                engine.handle(command);
                // Apply bursts of commands (seek drags) before ticking.
                while let Ok(command) = commands.try_recv() {
                    engine.handle(command);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        engine.tick();
    }
}

/// Starts the engine; the first event is [`Event::Ready`] with its handle.
pub fn subscription() -> Subscription<Event> {
    Subscription::run(|| {
        cosmic::iced::stream::channel(64, async move |mut output| {
            let (cmd_tx, cmd_rx) = mpsc::channel();
            let (ev_tx, mut ev_rx) = tokio::sync::mpsc::unbounded_channel();
            std::thread::Builder::new()
                .name("audio-engine".into())
                .spawn(move || run(cmd_rx, ev_tx))
                .expect("spawn audio thread");
            let _ = output.send(Event::Ready(Handle(cmd_tx))).await;
            while let Some(event) = ev_rx.recv().await {
                if output.send(event).await.is_err() {
                    break;
                }
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_curve() {
        assert!((amplitude(1.0) - 1.0).abs() < f32::EPSILON);
        assert!((amplitude(0.5) - 0.25).abs() < f32::EPSILON);
        assert!(amplitude(2.0) <= 1.0);
    }

    #[test]
    fn replay_gain() {
        assert!((gain_factor(None, 0.0) - 1.0).abs() < f32::EPSILON);
        assert!((gain_factor(Some(-6.0), 0.0) - 0.501).abs() < 0.01);
        assert!(gain_factor(Some(40.0), 0.0) < 4.0);
    }
}
