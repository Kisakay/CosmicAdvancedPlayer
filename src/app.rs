// SPDX-License-Identifier: GPL-3.0-only

//! Application model, messages and update logic.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cosmic::Application;
use cosmic::app::context_drawer;
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::keyboard::{self, Key, key::Named};
use cosmic::iced::{Event, Subscription, event};
use cosmic::prelude::*;
use cosmic::widget::menu::key_bind::{KeyBind, Modifier};
use cosmic::widget::{self, about::About, menu, nav_bar};

use crate::config::{AppTheme, Config};
use crate::library::db::{self, Db, PendingScrobble};
use crate::library::scanner::{self, ScanSummary};
use crate::library::{AlbumKey, Library, SongSort, Track, covers, watcher};
use crate::lyrics::{self, Lyrics, Provider};
use crate::queue::{Queue, Repeat};
use crate::{audio, discord, fl, lastfm, mpris};

pub const APP_ICON: &[u8] =
    include_bytes!("../resources/icons/hicolor/scalable/apps/io.github.kisakay.CosmicAdvancedPlayer.svg");
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
/// Preload the next track this long before the end (gapless).
const PRELOAD_BEFORE: Duration = Duration::from_secs(10);

/// Command line / D-Bus activation flags: files and folders to play.
#[derive(Debug, Clone, Default)]
pub struct Flags {
    pub paths: Vec<String>,
}

impl cosmic::app::CosmicFlags for Flags {
    type SubCommand = String;
    type Args = Vec<String>;

    fn action(&self) -> Option<&String> {
        static OPEN: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| "open".to_string());
        (!self.paths.is_empty()).then(|| &*OPEN)
    }

    fn args(&self) -> Vec<&str> {
        self.paths.iter().map(String::as_str).collect()
    }
}

/// Top-level pages (sidebar) and detail pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Page {
    NowPlaying,
    Songs,
    Albums,
    Artists,
    Genres,
    Folders,
    Favorites,
    Queue,
    Search,
    Album(AlbumKey),
    Artist(String),
    Genre(String),
    Folder(PathBuf),
}

impl Page {
    pub const SIDEBAR: [Self; 8] = [
        Self::NowPlaying,
        Self::Songs,
        Self::Albums,
        Self::Artists,
        Self::Genres,
        Self::Folders,
        Self::Favorites,
        Self::Queue,
    ];

    #[must_use]
    pub fn title(&self) -> String {
        match self {
            Self::NowPlaying => fl!("page-now-playing"),
            Self::Songs => fl!("page-songs"),
            Self::Albums => fl!("page-albums"),
            Self::Artists => fl!("page-artists"),
            Self::Genres => fl!("page-genres"),
            Self::Folders => fl!("page-folders"),
            Self::Favorites => fl!("page-favorites"),
            Self::Queue => fl!("page-queue"),
            Self::Search => fl!("page-search"),
            Self::Album(k) => {
                crate::library::album_title(&k.title).map_or_else(|| fl!("unknown-album"), str::to_string)
            }
            Self::Artist(a) => a.clone(),
            Self::Genre(g) => g.clone(),
            Self::Folder(p) => p
                .file_name()
                .map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned()),
        }
    }

    #[must_use]
    pub fn icon(&self) -> &'static str {
        match self {
            Self::NowPlaying => "media-playback-start-symbolic",
            Self::Songs => "audio-x-generic-symbolic",
            Self::Albums => "media-optical-symbolic",
            Self::Artists => "system-users-symbolic",
            Self::Genres => "view-grid-symbolic",
            Self::Folders => "folder-music-symbolic",
            Self::Favorites => "starred-symbolic",
            Self::Queue => "view-list-symbolic",
            _ => "folder-symbolic",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContextPage {
    #[default]
    Settings,
    About,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LyricsState {
    Idle,
    Loading,
    Found(Lyrics),
    NotFound,
}

/// Last.fm connection flow.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LastfmAuth {
    #[default]
    Idle,
    RequestingToken,
    /// Waiting for the user to approve the token in the browser.
    Pending(String),
    Finishing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    OpenFolder,
    AddLibraryFolder,
    Rescan,
    Settings,
    Quit,
    PlayPause,
    Next,
    Previous,
    Stop,
    Shuffle,
    Repeat,
    SeekForward,
    SeekBackward,
    VolumeUp,
    VolumeDown,
    Search,
    Page(usize),
    About,
    Repository,
}

impl menu::action::MenuAction for MenuAction {
    type Message = Message;

    fn message(&self) -> Message {
        Message::Menu(*self)
    }
}

/// Right-click actions on a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackAction {
    PlayNext,
    AddToQueue,
    GoToAlbum,
    GoToArtist,
    ToggleFavorite,
    ShowInFolder,
}

/// A track action bound to a track id (context menus).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackMenu(pub i64, pub TrackAction);

impl menu::action::MenuAction for TrackMenu {
    type Message = Message;

    fn message(&self) -> Message {
        Message::TrackAction(self.0, self.1)
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Surface(cosmic::surface::Action<Message>),
    // Engine & system integration
    Audio(audio::Event),
    Mpris(mpris::Event),
    // Library
    LibraryLoaded(Arc<Library>),
    ScanProgress(usize, usize),
    ScanDone(Result<(ScanSummary, Arc<Library>), String>),
    FilesChanged,
    Rescan(bool),
    // Navigation
    Navigate(Page),
    Back,
    Scrolled(f32, f32),
    SortSongs(SongSort),
    // Playback
    /// Play these library tracks, starting at index.
    PlayIds(Vec<i64>, usize),
    /// Same as `PlayIds`, shared between the rows of a list.
    PlayList(Arc<[i64]>, usize),
    PlayShuffled(Vec<i64>),
    TogglePlay,
    Next,
    Previous,
    Stop,
    SeekPreview(f32),
    SeekCommit,
    SeekBy(i64),
    Volume(f32),
    ToggleMute,
    ToggleShuffle,
    CycleRepeat,
    // Queue
    QueueAdd(Vec<i64>, bool),
    QueueJump(usize),
    QueueRemove(usize),
    QueueMove(usize, usize),
    QueueClear,
    TrackAction(i64, TrackAction),
    ToggleFavorite(i64),
    // Open folders / files outside the library
    OpenFolderDialog,
    OpenPaths(Vec<PathBuf>, bool),
    TransientLoaded(Vec<Track>, bool),
    // Search
    SearchInput(String),
    SearchResults(String, Vec<i64>),
    SearchClear,
    // Lyrics & covers
    LyricsLoaded(i64, Option<Lyrics>),
    LyricsRefresh,
    SeekToLine(usize),
    Ambient(String, Option<PathBuf>, Option<[f32; 3]>),
    DiscordCover((String, String), Option<String>),
    // Settings
    AddLibraryFolder,
    LibraryFolderPicked(Option<PathBuf>),
    RemoveLibraryFolder(usize),
    WatchFolders(bool),
    Theme(usize),
    ReplayGain(bool),
    Gapless(bool),
    LyricsProviderEnabled(Provider, bool),
    LyricsProviderMove(Provider, i32),
    LastfmApiKey(String),
    LastfmSecret(String),
    LastfmConnect,
    LastfmToken(Result<String, String>),
    LastfmFinish,
    LastfmSession(Result<(String, String), String>),
    LastfmDisconnect,
    LastfmScrobble(bool),
    LastfmNowPlaying(bool),
    LastfmLove(bool),
    LastfmFlushed(Result<usize, lastfm::Error>),
    DiscordEnabled(bool),
    DiscordClientId(String),
    DiscordOnlineCovers(bool),
    // Chrome
    Menu(MenuAction),
    ToggleContextPage(ContextPage),
    LaunchUrl(String),
    CloseToast(widget::ToastId),
    UpdateConfig(Config),
    Noop,
}

pub struct App {
    pub core: cosmic::Core,
    pub config: Config,
    config_handler: Option<cosmic_config::Config>,
    pub nav: nav_bar::Model,
    pub key_binds: HashMap<KeyBind, MenuAction>,
    pub toasts: widget::Toasts<Message>,
    pub about: About,
    pub context_page: ContextPage,
    pub theme_names: Vec<String>,
    db: Option<Arc<Mutex<Db>>>,
    db_path: PathBuf,
    http: reqwest::Client,
    // Library
    pub library: Arc<Library>,
    pub library_loaded: bool,
    pub scanning: Option<(usize, usize)>,
    scan_again: Option<bool>,
    next_transient_id: i64,
    // Navigation
    pub page: Page,
    history: Vec<Page>,
    pub scroll: (f32, f32),
    pub songs_sort: SongSort,
    pub songs_desc: bool,
    pub songs: Vec<i64>,
    pub search: String,
    pub search_results: Vec<i64>,
    pub search_id: widget::Id,
    // Playback
    pub queue: Queue,
    engine: Option<audio::Handle>,
    pub playing: bool,
    pub position: Duration,
    pub duration: Duration,
    pub seeking: Option<f32>,
    loaded: bool,
    preloaded: Option<i64>,
    failures: u32,
    resume_at: Option<Duration>,
    // Scrobbling
    listened: Duration,
    counted: bool,
    started_at: i64,
    // Now playing extras
    pub lyrics: LyricsState,
    lyrics_for: Option<i64>,
    pub ambient: Option<(String, Option<PathBuf>, Option<[f32; 3]>)>,
    // Integrations
    mpris: Option<mpris::Handle>,
    discord: discord::Handle,
    discord_covers: HashMap<(String, String), Option<String>>,
    pub lastfm_auth: LastfmAuth,
    pub lastfm_pending: usize,
    pending_open: Vec<PathBuf>,
}

impl cosmic::Application for App {
    type Executor = cosmic::executor::Default;
    type Flags = Flags;
    type Message = Message;

    const APP_ID: &'static str = "io.github.kisakay.CosmicAdvancedPlayer";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(core: cosmic::Core, flags: Flags) -> (Self, Task<cosmic::Action<Message>>) {
        let (config_handler, config) = Config::load(Self::APP_ID);
        let db_path = db::default_path();
        let db = match Db::open(&db_path) {
            Ok(db) => Some(Arc::new(Mutex::new(db))),
            Err(err) => {
                eprintln!("cannot open the library database: {err}");
                None
            }
        };

        let about = About::default()
            .name(fl!("app-title"))
            .icon(widget::icon::from_svg_bytes(APP_ICON))
            .version(env!("CARGO_PKG_VERSION"))
            .author("Kisakay")
            .license("GPL-3.0-only")
            .links([
                (fl!("about-repository"), REPOSITORY.to_string()),
                (fl!("about-issues"), format!("{REPOSITORY}/issues")),
                ("LRCLIB".to_string(), "https://lrclib.net".to_string()),
                ("lyrics.ovh".to_string(), "https://lyrics.ovh".to_string()),
            ])
            .comments(fl!("about-comments"));

        let http = reqwest::Client::builder()
            .user_agent(lyrics::USER_AGENT)
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap_or_default();

        let discord = discord::spawn();
        discord.configure(config.discord_enabled, config.discord_app_id());

        let mut queue = Queue::default();
        queue.repeat = config.repeat;

        let mut app = App {
            core,
            config_handler,
            nav: nav_bar::Model::default(),
            key_binds: key_binds(),
            toasts: widget::Toasts::new(Message::CloseToast),
            about,
            context_page: ContextPage::default(),
            theme_names: vec![fl!("theme-system"), fl!("theme-dark"), fl!("theme-light")],
            db,
            db_path,
            http,
            library: Arc::new(Library::default()),
            library_loaded: false,
            scanning: None,
            scan_again: None,
            next_transient_id: -1,
            page: Page::Albums,
            history: Vec::new(),
            scroll: (0.0, 800.0),
            songs_sort: SongSort::Artist,
            songs_desc: false,
            songs: Vec::new(),
            search: String::new(),
            search_results: Vec::new(),
            search_id: widget::Id::unique(),
            queue,
            engine: None,
            playing: false,
            position: Duration::ZERO,
            duration: Duration::ZERO,
            seeking: None,
            loaded: false,
            preloaded: None,
            failures: 0,
            resume_at: None,
            listened: Duration::ZERO,
            counted: false,
            started_at: 0,
            lyrics: LyricsState::Idle,
            lyrics_for: None,
            ambient: None,
            mpris: None,
            discord,
            discord_covers: HashMap::new(),
            lastfm_auth: LastfmAuth::Idle,
            lastfm_pending: 0,
            pending_open: flags.paths.iter().map(PathBuf::from).collect(),
            config,
        };
        app.rebuild_nav();

        let mut tasks = vec![
            cosmic::command::set_theme(app.config.app_theme.theme()),
            app.update_title(),
        ];
        if let Some(db) = app.db.clone() {
            tasks.push(cosmic::task::future(async move {
                let lib = tokio::task::spawn_blocking(move || {
                    let tracks = db
                        .lock()
                        .ok()
                        .and_then(|db| db.load_all().ok())
                        .unwrap_or_default();
                    Library::new(tracks)
                })
                .await
                .unwrap_or_default();
                Message::LibraryLoaded(Arc::new(lib))
            }));
        }
        tasks.push(app.flush_scrobbles());
        (app, Task::batch(tasks))
    }

    fn header_start(&self) -> Vec<Element<'_, Message>> {
        vec![crate::ui::menu_bar(self)]
    }

    fn header_end(&self) -> Vec<Element<'_, Message>> {
        crate::ui::header_end(self)
    }

    fn nav_model(&self) -> Option<&nav_bar::Model> {
        Some(&self.nav)
    }

    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<cosmic::Action<Message>> {
        if let Some(page) = self.nav.data::<Page>(id).cloned() {
            self.history.clear();
            return self.navigate(page, false);
        }
        Task::none()
    }

    fn context_drawer(&self) -> Option<context_drawer::ContextDrawer<'_, Message>> {
        if !self.core.window.show_context {
            return None;
        }
        Some(match self.context_page {
            ContextPage::About => context_drawer::about(
                &self.about,
                |url| Message::LaunchUrl(url.to_string()),
                Message::ToggleContextPage(ContextPage::About),
            ),
            ContextPage::Settings => context_drawer::context_drawer(
                crate::ui::settings::view(self),
                Message::ToggleContextPage(ContextPage::Settings),
            )
            .title(fl!("settings")),
        })
    }

    fn footer(&self) -> Option<Element<'_, Message>> {
        Some(crate::ui::player_bar::view(self))
    }

    fn on_escape(&mut self) -> Task<cosmic::Action<Message>> {
        if self.core.window.show_context {
            self.core.window.show_context = false;
            return Task::none();
        }
        if !self.history.is_empty() {
            return self.update(Message::Back);
        }
        if !self.search.is_empty() {
            return self.update(Message::SearchClear);
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        widget::toaster(&self.toasts, crate::ui::view(self))
    }

    fn dbus_activation(&mut self, msg: cosmic::dbus_activation::Message) -> Task<cosmic::Action<Message>> {
        if let cosmic::dbus_activation::Details::ActivateAction { args, .. } = msg.msg {
            let paths = args.into_iter().map(PathBuf::from).collect();
            return self.update(Message::OpenPaths(paths, true));
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let keys = event::listen_with(|event, status, _id| match event {
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modifiers,
                physical_key,
                ..
            }) => {
                if let Some(action) = key_binds()
                    .into_iter()
                    .find(|(bind, _)| bind.matches(modifiers, &key, Some(&physical_key)))
                    .map(|(_, action)| action)
                {
                    // Plain keys (space, arrows) only when no text field has the focus.
                    if status == event::Status::Captured && !modifiers.control() {
                        return None;
                    }
                    return Some(Message::Menu(action));
                }
                match key {
                    Key::Named(Named::MediaPlayPause) => Some(Message::TogglePlay),
                    Key::Named(Named::MediaTrackNext) => Some(Message::Next),
                    Key::Named(Named::MediaTrackPrevious) => Some(Message::Previous),
                    Key::Named(Named::MediaStop) => Some(Message::Stop),
                    _ => None,
                }
            }
            _ => None,
        });

        let mut subs = vec![
            keys,
            audio::subscription().map(Message::Audio),
            mpris::subscription().map(Message::Mpris),
            self.core()
                .watch_config::<Config>(Self::APP_ID)
                .map(|update| Message::UpdateConfig(update.config)),
        ];
        if self.config.watch_folders && self.library_loaded && !self.config.library_folders.is_empty() {
            let roots = self.config.library_folders.iter().map(PathBuf::from).collect();
            subs.push(watcher::subscription(roots).map(|()| Message::FilesChanged));
        }
        Subscription::batch(subs)
    }

    #[allow(clippy::too_many_lines)]
    fn update(&mut self, message: Message) -> Task<cosmic::Action<Message>> {
        match message {
            Message::Surface(action) => {
                return cosmic::task::message(cosmic::Action::Surface(action));
            }
            Message::Audio(event) => return self.on_audio(event),
            Message::Mpris(event) => return self.on_mpris(event),

            // --- Library -------------------------------------------------------------
            Message::LibraryLoaded(lib) => {
                self.set_library(lib);
                self.library_loaded = true;
                if self.library.is_empty() {
                    self.page = Page::Albums;
                }
                let mut tasks = vec![self.start_scan(false)];
                let open = std::mem::take(&mut self.pending_open);
                if open.is_empty() {
                    tasks.push(self.restore_session());
                } else {
                    tasks.push(self.update(Message::OpenPaths(open, true)));
                }
                return Task::batch(tasks);
            }
            Message::ScanProgress(done, total) => {
                if self.scanning.is_some() {
                    self.scanning = Some((done, total));
                }
            }
            Message::ScanDone(result) => {
                self.scanning = None;
                let mut tasks = Vec::new();
                match result {
                    Ok((summary, lib)) => {
                        self.set_library(lib);
                        if summary.added + summary.removed > 0 {
                            tasks.push(self.toast(fl!(
                                "scan-done",
                                total = summary.total,
                                added = summary.added,
                                removed = summary.removed
                            )));
                        }
                    }
                    Err(err) => tasks.push(self.toast(fl!("scan-failed", error = err))),
                }
                if let Some(force) = self.scan_again.take() {
                    tasks.push(self.start_scan(force));
                }
                return Task::batch(tasks);
            }
            Message::FilesChanged => return self.start_scan(false),
            Message::Rescan(force) => return self.start_scan(force),

            // --- Navigation ----------------------------------------------------------
            Message::Navigate(page) => return self.navigate(page, true),
            Message::Back => {
                if let Some(page) = self.history.pop() {
                    return self.navigate(page, false);
                }
            }
            Message::Scrolled(offset, height) => self.scroll = (offset, height),
            Message::SortSongs(sort) => {
                if self.songs_sort == sort {
                    self.songs_desc = !self.songs_desc;
                } else {
                    self.songs_sort = sort;
                    self.songs_desc = false;
                }
                self.songs = self.library.sorted_ids(self.songs_sort, self.songs_desc);
            }

            // --- Playback ------------------------------------------------------------
            Message::PlayList(ids, start) => return self.update(Message::PlayIds(ids.to_vec(), start)),
            Message::PlayIds(ids, start) => {
                let tracks = self.tracks_for(&ids);
                if tracks.is_empty() {
                    return Task::none();
                }
                self.queue.set(tracks, start, self.config.shuffle);
                return self.load_current(Duration::ZERO, false);
            }
            Message::PlayShuffled(ids) => {
                let tracks = self.tracks_for(&ids);
                if tracks.is_empty() {
                    return Task::none();
                }
                let start = rand::random_range(0..tracks.len());
                self.config.shuffle = true;
                self.save_config();
                self.queue.set(tracks, start, true);
                return self.load_current(Duration::ZERO, false);
            }
            Message::TogglePlay => {
                if self.queue.current().is_none() {
                    // Nothing queued: play the whole library.
                    let ids = self.library.sorted_ids(SongSort::Artist, false);
                    return self.update(if self.config.shuffle {
                        Message::PlayShuffled(ids)
                    } else {
                        Message::PlayIds(ids, 0)
                    });
                }
                if self.engine_idle() {
                    let at = self.resume_at.take().unwrap_or(self.position);
                    return self.load_current(at, false);
                }
                self.playing = !self.playing;
                self.send(if self.playing {
                    audio::Command::Play
                } else {
                    audio::Command::Pause
                });
                self.publish_state();
                if !self.playing {
                    self.save_session();
                }
            }
            Message::Next => {
                if self.queue.advance(true).is_some() {
                    return self.load_current(Duration::ZERO, false);
                }
            }
            Message::Previous => {
                if self.position > Duration::from_secs(4) {
                    return self.seek(Duration::ZERO);
                }
                if self.queue.back().is_some() {
                    return self.load_current(Duration::ZERO, false);
                }
            }
            Message::Stop => {
                self.send(audio::Command::Stop);
                self.playing = false;
                self.loaded = false;
                self.position = Duration::ZERO;
                self.preloaded = None;
                self.publish_state();
            }
            Message::SeekPreview(secs) => self.seeking = Some(secs),
            Message::SeekCommit => {
                if let Some(secs) = self.seeking.take() {
                    return self.seek(Duration::from_secs_f32(secs.max(0.0)));
                }
            }
            Message::SeekBy(delta) => {
                let pos = self.position.as_secs() as i64 + delta;
                let pos = pos.clamp(0, self.duration.as_secs() as i64);
                return self.seek(Duration::from_secs(pos as u64));
            }
            Message::Volume(v) => {
                self.config.volume = v.clamp(0.0, 1.0);
                self.config.muted = false;
                self.apply_volume();
                self.save_config();
            }
            Message::ToggleMute => {
                self.config.muted = !self.config.muted;
                self.apply_volume();
                self.save_config();
            }
            Message::ToggleShuffle => {
                self.config.shuffle = !self.config.shuffle;
                self.queue.set_shuffle(self.config.shuffle);
                self.save_config();
                self.sync_preload();
                self.publish_state();
            }
            Message::CycleRepeat => {
                self.queue.repeat = self.queue.repeat.cycle();
                self.config.repeat = self.queue.repeat;
                self.save_config();
                self.sync_preload();
                self.publish_state();
            }

            // --- Queue ---------------------------------------------------------------
            Message::QueueAdd(ids, next) => {
                let tracks = self.tracks_for(&ids);
                let was_empty = self.queue.current().is_none();
                if next {
                    self.queue.play_next(&tracks);
                } else {
                    self.queue.append(&tracks);
                }
                self.sync_preload();
                let msg = if next {
                    fl!("queued-next", count = tracks.len())
                } else {
                    fl!("queued", count = tracks.len())
                };
                let toast = self.toast(msg);
                if was_empty {
                    return Task::batch([toast, self.load_current(Duration::ZERO, false)]);
                }
                self.publish_state();
                return toast;
            }
            Message::QueueJump(i) => {
                if self.queue.jump(i).is_some() {
                    return self.load_current(Duration::ZERO, false);
                }
            }
            Message::QueueRemove(i) => {
                if self.queue.remove(i) {
                    if self.queue.current().is_some() {
                        return self.load_current(Duration::ZERO, !self.playing);
                    }
                    return self.update(Message::Stop);
                }
                self.sync_preload();
                self.publish_state();
            }
            Message::QueueMove(from, to) => {
                self.queue.move_item(from, to);
                self.sync_preload();
                self.publish_state();
            }
            Message::QueueClear => {
                self.queue.clear_upcoming();
                self.sync_preload();
                self.publish_state();
            }
            Message::TrackAction(id, action) => return self.track_action(id, action),
            Message::ToggleFavorite(id) => return self.toggle_favorite(id),

            // --- Opening folders / files ----------------------------------------------
            Message::OpenFolderDialog => {
                return cosmic::task::future(async {
                    let picked = rfd::AsyncFileDialog::new()
                        .set_title(fl!("open-folder"))
                        .pick_folder()
                        .await
                        .map(|h| h.path().to_path_buf());
                    match picked {
                        Some(p) => Message::OpenPaths(vec![p], true),
                        None => Message::Noop,
                    }
                });
            }
            Message::OpenPaths(paths, play) => {
                // Files inside the library are played from the index.
                let first = self.next_transient_id;
                return cosmic::task::future(async move {
                    let tracks = tokio::task::spawn_blocking(move || scanner::read_transient(&paths, first))
                        .await
                        .unwrap_or_default();
                    Message::TransientLoaded(tracks, play)
                });
            }
            Message::TransientLoaded(tracks, play) => {
                if tracks.is_empty() {
                    return self.toast(fl!("nothing-to-play"));
                }
                self.next_transient_id -= tracks.len() as i64;
                // Prefer the indexed version (statistics, favourites) when known.
                let by_path: HashMap<&PathBuf, &Arc<Track>> =
                    self.library.tracks().iter().map(|t| (&t.path, t)).collect();
                let tracks: Vec<Arc<Track>> = tracks
                    .into_iter()
                    .map(|t| {
                        by_path
                            .get(&t.path)
                            .map_or_else(|| Arc::new(t), |a| Arc::clone(a))
                    })
                    .collect();
                let count = tracks.len();
                if play {
                    self.queue.set(tracks, 0, self.config.shuffle);
                    self.history.clear();
                    let nav = self.navigate(Page::NowPlaying, false);
                    return Task::batch([
                        nav,
                        self.load_current(Duration::ZERO, false),
                        self.toast(fl!("opened", count = count)),
                    ]);
                }
                self.queue.append(&tracks);
                self.sync_preload();
            }

            // --- Search ----------------------------------------------------------------
            Message::SearchInput(text) => {
                self.search = text;
                if self.search.trim().is_empty() {
                    self.search_results.clear();
                    if self.page == Page::Search {
                        return self.update(Message::Back);
                    }
                    return Task::none();
                }
                let mut task = Task::none();
                if self.page != Page::Search {
                    task = self.navigate(Page::Search, true);
                }
                let Some(db) = self.db.clone() else { return task };
                let query = self.search.clone();
                return Task::batch([
                    task,
                    cosmic::task::future(async move {
                        let q = query.clone();
                        let ids = tokio::task::spawn_blocking(move || {
                            db.lock()
                                .ok()
                                .and_then(|db| db.search(&q, 300).ok())
                                .unwrap_or_default()
                        })
                        .await
                        .unwrap_or_default();
                        Message::SearchResults(query, ids)
                    }),
                ]);
            }
            Message::SearchResults(query, ids) => {
                if query == self.search {
                    self.search_results = ids;
                }
            }
            Message::SearchClear => {
                self.search.clear();
                self.search_results.clear();
                if self.page == Page::Search {
                    return self.update(Message::Back);
                }
            }

            // --- Lyrics, covers ----------------------------------------------------------
            Message::LyricsLoaded(id, lyrics) => {
                if self.lyrics_for == Some(id) {
                    self.lyrics = lyrics.map_or(LyricsState::NotFound, LyricsState::Found);
                    self.publish_state();
                }
            }
            Message::LyricsRefresh => {
                if let Some(track) = self.queue.current().cloned() {
                    lyrics::forget(&lyrics_query(&track));
                    self.lyrics_for = None;
                    return self.load_lyrics(&track, false);
                }
            }
            Message::SeekToLine(i) => {
                if let LyricsState::Found(l) = &self.lyrics
                    && let Some(t) = l.lines.get(i).and_then(|l| l.time)
                {
                    return self.seek(t);
                }
            }
            Message::Ambient(key, blur, color) => {
                if self.queue.current().and_then(|t| t.cover.as_ref()) == Some(&key) {
                    self.ambient = Some((key, blur, color));
                }
            }
            Message::DiscordCover(key, url) => {
                self.discord_covers.insert(key, url);
                self.publish_discord();
            }

            // --- Settings ------------------------------------------------------------------
            Message::AddLibraryFolder => {
                return cosmic::task::future(async {
                    let picked = rfd::AsyncFileDialog::new()
                        .set_title(fl!("add-library-folder"))
                        .pick_folder()
                        .await
                        .map(|h| h.path().to_path_buf());
                    Message::LibraryFolderPicked(picked)
                });
            }
            Message::LibraryFolderPicked(Some(path)) => {
                let path = path.to_string_lossy().into_owned();
                if !self.config.library_folders.contains(&path) {
                    self.config.library_folders.push(path);
                    self.save_config();
                    return self.start_scan(false);
                }
            }
            Message::LibraryFolderPicked(None) | Message::Noop => {}
            Message::RemoveLibraryFolder(i) => {
                if i < self.config.library_folders.len() {
                    self.config.library_folders.remove(i);
                    self.save_config();
                    return self.start_scan(false);
                }
            }
            Message::WatchFolders(on) => {
                self.config.watch_folders = on;
                self.save_config();
            }
            Message::Theme(i) => {
                self.config.app_theme = AppTheme::ALL.get(i).copied().unwrap_or_default();
                self.save_config();
                return cosmic::command::set_theme(self.config.app_theme.theme());
            }
            Message::ReplayGain(on) => {
                self.config.replay_gain = on;
                self.save_config();
            }
            Message::Gapless(on) => {
                self.config.gapless = on;
                self.save_config();
                self.sync_preload();
            }
            Message::LyricsProviderEnabled(p, on) => {
                self.config.lyrics_disabled.retain(|x| *x != p);
                if !on {
                    self.config.lyrics_disabled.push(p);
                }
                self.save_config();
            }
            Message::LyricsProviderMove(p, dir) => {
                if let Some(i) = self.config.lyrics_order.iter().position(|x| *x == p) {
                    let j = i as i32 + dir;
                    if j >= 0 && (j as usize) < self.config.lyrics_order.len() {
                        self.config.lyrics_order.swap(i, j as usize);
                        self.save_config();
                    }
                }
            }
            Message::LastfmApiKey(v) => {
                self.config.lastfm_api_key = v;
                self.save_config();
            }
            Message::LastfmSecret(v) => {
                self.config.lastfm_secret = v;
                self.save_config();
            }
            Message::LastfmConnect => {
                let creds = self.lastfm_credentials();
                if !creds.is_set() {
                    return self.toast(fl!("lastfm-need-key"));
                }
                self.lastfm_auth = LastfmAuth::RequestingToken;
                let http = self.http.clone();
                return cosmic::task::future(async move {
                    Message::LastfmToken(lastfm::get_token(&http, &creds).await.map_err(|e| e.to_string()))
                });
            }
            Message::LastfmToken(Ok(token)) => {
                let url = lastfm::auth_url(self.config.lastfm_api_key.trim(), &token);
                self.lastfm_auth = LastfmAuth::Pending(token);
                let _ = open::that_detached(url);
            }
            Message::LastfmToken(Err(err)) | Message::LastfmSession(Err(err)) => {
                self.lastfm_auth = LastfmAuth::Idle;
                return self.toast(fl!("lastfm-error", error = err));
            }
            Message::LastfmFinish => {
                let LastfmAuth::Pending(token) = self.lastfm_auth.clone() else {
                    return Task::none();
                };
                self.lastfm_auth = LastfmAuth::Finishing;
                let creds = self.lastfm_credentials();
                let http = self.http.clone();
                return cosmic::task::future(async move {
                    Message::LastfmSession(
                        lastfm::get_session(&http, &creds, &token)
                            .await
                            .map_err(|e| e.to_string()),
                    )
                });
            }
            Message::LastfmSession(Ok((user, key))) => {
                self.lastfm_auth = LastfmAuth::Idle;
                self.config.lastfm_user = user.clone();
                self.config.lastfm_session = key;
                self.save_config();
                return Task::batch([
                    self.toast(fl!("lastfm-connected", user = user)),
                    self.flush_scrobbles(),
                ]);
            }
            Message::LastfmDisconnect => {
                self.config.lastfm_session.clear();
                self.config.lastfm_user.clear();
                self.lastfm_auth = LastfmAuth::Idle;
                self.save_config();
            }
            Message::LastfmScrobble(on) => {
                self.config.lastfm_scrobble = on;
                self.save_config();
            }
            Message::LastfmNowPlaying(on) => {
                self.config.lastfm_now_playing = on;
                self.save_config();
            }
            Message::LastfmLove(on) => {
                self.config.lastfm_love = on;
                self.save_config();
            }
            Message::LastfmFlushed(result) => {
                self.lastfm_pending = self
                    .db
                    .as_ref()
                    .and_then(|d| d.lock().ok())
                    .map_or(0, |d| d.pending_scrobble_count());
                match result {
                    Ok(n) if n > 0 && self.lastfm_pending > 0 => return self.flush_scrobbles(),
                    Err(lastfm::Error::InvalidSession) => {
                        self.config.lastfm_session.clear();
                        self.save_config();
                        return self.toast(fl!("lastfm-expired"));
                    }
                    _ => {}
                }
            }
            Message::DiscordEnabled(on) => {
                self.config.discord_enabled = on;
                self.save_config();
                self.discord.configure(on, self.config.discord_app_id());
                return self.publish_discord_task();
            }
            Message::DiscordClientId(id) => {
                self.config.discord_client_id = id.chars().filter(char::is_ascii_digit).collect();
                self.save_config();
                self.discord
                    .configure(self.config.discord_enabled, self.config.discord_app_id());
                self.publish_discord();
            }
            Message::DiscordOnlineCovers(on) => {
                self.config.discord_online_covers = on;
                self.save_config();
                return self.publish_discord_task();
            }

            // --- Chrome --------------------------------------------------------------------
            Message::Menu(action) => return self.menu_action(action),
            Message::ToggleContextPage(page) => {
                if self.context_page == page {
                    self.core.window.show_context = !self.core.window.show_context;
                } else {
                    self.context_page = page;
                    self.core.window.show_context = true;
                }
            }
            Message::LaunchUrl(url) => {
                let _ = open::that_detached(url);
            }
            Message::CloseToast(id) => self.toasts.remove(id),
            Message::UpdateConfig(config) => {
                if config != self.config {
                    let theme_changed = config.app_theme != self.config.app_theme;
                    self.config = config;
                    self.config.normalize();
                    if theme_changed {
                        return cosmic::command::set_theme(self.config.app_theme.theme());
                    }
                }
            }
        }
        Task::none()
    }
}

/// Keyboard shortcuts.
pub fn key_binds() -> HashMap<KeyBind, MenuAction> {
    let mut map = HashMap::new();
    let mut bind = |modifiers: Vec<Modifier>, key: Key, action| {
        map.insert(KeyBind { modifiers, key }, action);
    };
    bind(vec![], Key::Character(" ".into()), MenuAction::PlayPause);
    bind(
        vec![Modifier::Ctrl],
        Key::Named(Named::ArrowRight),
        MenuAction::Next,
    );
    bind(
        vec![Modifier::Ctrl],
        Key::Named(Named::ArrowLeft),
        MenuAction::Previous,
    );
    bind(vec![], Key::Named(Named::ArrowRight), MenuAction::SeekForward);
    bind(vec![], Key::Named(Named::ArrowLeft), MenuAction::SeekBackward);
    bind(
        vec![Modifier::Ctrl],
        Key::Named(Named::ArrowUp),
        MenuAction::VolumeUp,
    );
    bind(
        vec![Modifier::Ctrl],
        Key::Named(Named::ArrowDown),
        MenuAction::VolumeDown,
    );
    bind(
        vec![Modifier::Ctrl],
        Key::Character("o".into()),
        MenuAction::OpenFolder,
    );
    bind(
        vec![Modifier::Ctrl],
        Key::Character("f".into()),
        MenuAction::Search,
    );
    bind(
        vec![Modifier::Ctrl],
        Key::Character("s".into()),
        MenuAction::Shuffle,
    );
    bind(
        vec![Modifier::Ctrl],
        Key::Character("r".into()),
        MenuAction::Repeat,
    );
    bind(
        vec![Modifier::Ctrl],
        Key::Character(",".into()),
        MenuAction::Settings,
    );
    bind(vec![Modifier::Ctrl], Key::Character("q".into()), MenuAction::Quit);
    for (i, k) in ["1", "2", "3", "4", "5", "6", "7", "8"].into_iter().enumerate() {
        bind(
            vec![Modifier::Ctrl],
            Key::Character(k.into()),
            MenuAction::Page(i),
        );
    }
    map
}

fn lyrics_query(t: &Track) -> lyrics::Query {
    lyrics::Query {
        path: t.path.clone(),
        artist: t.artist.clone(),
        title: t.title.clone(),
        album: t.album.clone(),
        duration_secs: t.duration_ms / 1000,
    }
}

fn track_info(t: &Track) -> lastfm::TrackInfo {
    lastfm::TrackInfo {
        artist: t.artist.clone(),
        title: t.title.clone(),
        album: t.album.clone(),
        album_artist: t.album_artist.clone(),
        duration_secs: t.duration_ms / 1000,
    }
}

impl App {
    fn send(&self, command: audio::Command) {
        if let Some(engine) = &self.engine {
            engine.send(command);
        }
    }

    fn engine_idle(&self) -> bool {
        !self.loaded
    }

    pub fn toast(&mut self, message: impl Into<String>) -> Task<cosmic::Action<Message>> {
        self.toasts
            .push(widget::Toast::new(message.into()))
            .map(cosmic::Action::App)
    }

    fn save_config(&self) {
        if let Some(handler) = &self.config_handler
            && let Err(err) = self.config.write_entry(handler)
        {
            eprintln!("failed to save config: {err}");
        }
    }

    fn lastfm_credentials(&self) -> lastfm::Credentials {
        lastfm::Credentials {
            api_key: self.config.lastfm_api_key.trim().to_string(),
            secret: self.config.lastfm_secret.trim().to_string(),
        }
    }

    pub fn update_title(&mut self) -> Task<cosmic::Action<Message>> {
        let title = match self.queue.current() {
            Some(t) if self.playing => format!("{} — {}", t.title, fl!("app-title")),
            _ => fl!("app-title"),
        };
        self.set_header_title(String::new());
        match self.core.main_window_id() {
            Some(id) => self.set_window_title(title, id),
            None => Task::none(),
        }
    }

    fn rebuild_nav(&mut self) {
        self.nav.clear();
        for page in Page::SIDEBAR {
            let active = page == self.page;
            let item = self
                .nav
                .insert()
                .text(page.title())
                .icon(widget::icon::from_name(page.icon()))
                .data::<Page>(page);
            if active {
                item.activate();
            }
        }
    }

    fn set_library(&mut self, lib: Arc<Library>) {
        self.library = lib;
        self.songs = self.library.sorted_ids(self.songs_sort, self.songs_desc);
        // Refresh queued tracks with their new metadata.
        let fresh: HashMap<&PathBuf, &Arc<Track>> =
            self.library.tracks().iter().map(|t| (&t.path, t)).collect();
        let items: Vec<Arc<Track>> = self
            .queue
            .items()
            .iter()
            .map(|t| fresh.get(&t.path).map_or_else(|| t.clone(), |a| Arc::clone(a)))
            .collect();
        if items
            .iter()
            .zip(self.queue.items())
            .any(|(a, b)| !Arc::ptr_eq(a, b))
        {
            let pos = self.queue.position().unwrap_or(0);
            let shuffled = self.queue.shuffled();
            let repeat = self.queue.repeat;
            self.queue.set(items, pos, false);
            self.queue.repeat = repeat;
            // Keep the play order: shuffling again would reorder the queue.
            let _ = shuffled;
        }
    }

    fn tracks_for(&self, ids: &[i64]) -> Vec<Arc<Track>> {
        ids.iter()
            .filter_map(|id| {
                self.library
                    .get(*id)
                    .cloned()
                    .or_else(|| self.queue.items().iter().find(|t| t.id == *id).cloned())
            })
            .collect()
    }

    fn navigate(&mut self, page: Page, push: bool) -> Task<cosmic::Action<Message>> {
        if page == self.page {
            return Task::none();
        }
        if push {
            self.history.push(std::mem::replace(&mut self.page, page));
            if self.history.len() > 30 {
                self.history.remove(0);
            }
        } else {
            self.page = page;
        }
        // Highlight the matching sidebar entry.
        let root = match &self.page {
            Page::Album(_) | Page::Artist(_) => Some(Page::Albums),
            Page::Genre(_) => Some(Page::Genres),
            Page::Folder(_) => Some(Page::Folders),
            Page::Search => None,
            p => Some(p.clone()),
        };
        let target = root.and_then(|root| {
            self.nav
                .iter()
                .find(|id| self.nav.data::<Page>(*id) == Some(&root))
        });
        if let Some(id) = target {
            self.nav.activate(id);
        }
        self.scroll.0 = 0.0;
        crate::ui::scroll_to_top()
    }

    pub fn can_go_back(&self) -> bool {
        !self.history.is_empty()
    }

    fn start_scan(&mut self, force: bool) -> Task<cosmic::Action<Message>> {
        if self.scanning.is_some() {
            self.scan_again = Some(self.scan_again.unwrap_or(false) || force);
            return Task::none();
        }
        self.scanning = Some((0, 0));
        let roots: Vec<PathBuf> = self.config.library_folders.iter().map(PathBuf::from).collect();
        let db_path = self.db_path.clone();
        let stream = cosmic::iced::stream::channel(32, async move |mut out| {
            use cosmic::iced::futures::SinkExt;
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let job = tokio::task::spawn_blocking(move || {
                let mut db = Db::open(&db_path).map_err(|e| e.to_string())?;
                let summary = scanner::scan(&mut db, &roots, force, &|d, t| {
                    let _ = tx.send((d, t));
                })
                .map_err(|e| e.to_string())?;
                let lib = Library::new(db.load_all().map_err(|e| e.to_string())?);
                Ok((summary, Arc::new(lib)))
            });
            let mut last = std::time::Instant::now();
            while let Some((d, t)) = rx.recv().await {
                if last.elapsed() > Duration::from_millis(120) || d == t {
                    last = std::time::Instant::now();
                    let _ = out.send(Message::ScanProgress(d, t)).await;
                }
            }
            let result = job.await.map_err(|e| e.to_string()).and_then(|r| r);
            let _ = out.send(Message::ScanDone(result)).await;
        });
        Task::run(stream, cosmic::Action::App)
    }

    // --- Playback ------------------------------------------------------------------------

    fn apply_volume(&self) {
        let v = if self.config.muted {
            0.0
        } else {
            self.config.volume
        };
        self.send(audio::Command::Volume(v));
        self.publish_state();
    }

    fn gain_for(&self, t: &Track) -> f32 {
        if self.config.replay_gain {
            audio::gain_factor(t.replay_gain, 0.0)
        } else {
            1.0
        }
    }

    fn seek(&mut self, pos: Duration) -> Task<cosmic::Action<Message>> {
        if self.engine_idle() {
            self.resume_at = Some(pos);
            self.position = pos;
            return Task::none();
        }
        self.send(audio::Command::Seek(pos));
        self.position = pos;
        if let Some(m) = &self.mpris {
            m.seeked(pos.as_micros() as i64);
        }
        self.publish_discord();
        Task::none()
    }

    /// Starts the queue's current track.
    fn load_current(&mut self, start_at: Duration, paused: bool) -> Task<cosmic::Action<Message>> {
        let Some(track) = self.queue.current().cloned() else {
            return Task::none();
        };
        self.resume_at = None;
        self.send(audio::Command::Volume(if self.config.muted {
            0.0
        } else {
            self.config.volume
        }));
        self.send(audio::Command::Load {
            id: track.id,
            path: track.path.clone(),
            gain: self.gain_for(&track),
            paused,
            start_at,
        });
        self.playing = !paused;
        self.loaded = true;
        self.position = start_at;
        self.on_track_changed(&track)
    }

    /// Bookkeeping when a new track becomes current (loaded or gapless).
    fn on_track_changed(&mut self, track: &Arc<Track>) -> Task<cosmic::Action<Message>> {
        self.preloaded = None;
        self.duration = track.duration();
        self.listened = Duration::ZERO;
        self.counted = false;
        self.started_at = db::now();
        self.save_session();

        let mut tasks = vec![
            self.load_lyrics(track, true),
            self.load_ambient(track),
            self.update_title(),
        ];
        if self.config.lastfm_connected() && self.config.lastfm_now_playing {
            let (http, creds, sk, info) = (
                self.http.clone(),
                self.lastfm_credentials(),
                self.config.lastfm_session.clone(),
                track_info(track),
            );
            tasks.push(cosmic::task::future(async move {
                if let Err(err) = lastfm::now_playing(&http, &creds, &sk, &info).await {
                    eprintln!("last.fm now playing: {err}");
                }
                Message::Noop
            }));
        }
        tasks.push(self.publish_discord_task());
        self.publish_state();
        Task::batch(tasks)
    }

    fn on_audio(&mut self, event: audio::Event) -> Task<cosmic::Action<Message>> {
        match event {
            audio::Event::Ready(handle) => {
                handle.send(audio::Command::Volume(if self.config.muted {
                    0.0
                } else {
                    self.config.volume
                }));
                self.engine = Some(handle);
            }
            audio::Event::Started { id, duration } => {
                self.failures = 0;
                // A (re)load drops whatever the engine had queued.
                self.preloaded = None;
                if let Some(d) = duration.filter(|d| !d.is_zero())
                    && self.queue.current().is_some_and(|t| t.id == id)
                {
                    self.duration = d;
                }
            }
            audio::Event::Position(pos) => {
                let delta = pos.saturating_sub(self.position);
                if pos > self.position && delta < Duration::from_secs(2) {
                    self.listened += delta;
                }
                self.position = pos;
                if let Some(m) = &self.mpris {
                    m.set_position(pos.as_micros() as i64);
                }
                let mut tasks = vec![self.check_scrobble()];
                if self.config.gapless
                    && self.preloaded.is_none()
                    && !self.duration.is_zero()
                    && self.duration.saturating_sub(pos) < PRELOAD_BEFORE
                    && let Some(next) = self.queue.peek_next().cloned()
                {
                    self.preloaded = Some(next.id);
                    self.send(audio::Command::Preload {
                        id: next.id,
                        path: next.path.clone(),
                        gain: self.gain_for(&next),
                    });
                }
                return Task::batch(tasks.drain(..));
            }
            audio::Event::Advanced { id } => {
                let expected = self.queue.peek_next().map(|t| t.id);
                if expected == Some(id) {
                    self.queue.advance(false);
                    self.position = Duration::ZERO;
                    if let Some(track) = self.queue.current().cloned() {
                        return self.on_track_changed(&track);
                    }
                } else {
                    // Queue changed in the meantime: load the right track.
                    if self.queue.advance(false).is_some() {
                        return self.load_current(Duration::ZERO, false);
                    }
                }
            }
            audio::Event::Ended => {
                self.preloaded = None;
                if self.queue.advance(false).is_some() {
                    return self.load_current(Duration::ZERO, false);
                }
                self.playing = false;
                self.loaded = false;
                self.position = Duration::ZERO;
                self.publish_state();
                return self.update_title();
            }
            audio::Event::Failed { id, error } => {
                eprintln!("cannot play track {id}: {error}");
                self.failures += 1;
                let name = self.queue.current().map(|t| t.title.clone()).unwrap_or_default();
                let toast = self.toast(fl!("cannot-play", title = name));
                if self.failures < 5 && self.queue.advance(true).is_some() {
                    return Task::batch([toast, self.load_current(Duration::ZERO, false)]);
                }
                self.playing = false;
                self.loaded = false;
                self.publish_state();
                return toast;
            }
            audio::Event::DeviceError(err) => {
                self.playing = false;
                self.loaded = false;
                self.publish_state();
                return self.toast(fl!("audio-device-error", error = err));
            }
        }
        Task::none()
    }

    /// Keeps the engine's preloaded track consistent with the queue.
    fn sync_preload(&mut self) {
        let next = self.queue.peek_next().map(|t| t.id);
        if self.preloaded.is_some() && (self.preloaded != next || !self.config.gapless) {
            self.send(audio::Command::CancelPreload);
            // Not cleared: the engine will report `Ended` at the transition.
            self.preloaded = Some(i64::MIN);
        }
    }

    fn check_scrobble(&mut self) -> Task<cosmic::Action<Message>> {
        let Some(track) = self.queue.current().cloned() else {
            return Task::none();
        };
        if self.counted {
            return Task::none();
        }
        let threshold = lastfm::scrobble_threshold(self.duration).unwrap_or(self.duration / 2);
        if self.listened < threshold || threshold.is_zero() {
            return Task::none();
        }
        self.counted = true;
        let when = self.started_at;
        if track.id > 0 {
            self.library_update(track.id, |t| {
                t.play_count += 1;
                t.last_played = Some(when);
            });
        }
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        let scrobble = self.config.lastfm_connected()
            && self.config.lastfm_scrobble
            && lastfm::scrobble_threshold(self.duration).is_some()
            && !track.artist.is_empty();
        let pending = PendingScrobble {
            id: 0,
            artist: track.artist.clone(),
            title: track.title.clone(),
            album: track.album.clone(),
            album_artist: track.album_artist.clone(),
            duration_secs: track.duration_ms / 1000,
            timestamp: when,
        };
        let id = track.id;
        let save = cosmic::task::future(async move {
            let _ = tokio::task::spawn_blocking(move || {
                if let Ok(db) = db.lock() {
                    if id > 0 {
                        let _ = db.record_play(id, when);
                    }
                    if scrobble {
                        let _ = db.queue_scrobble(&pending);
                    }
                }
            })
            .await;
            Message::Noop
        });
        if scrobble {
            save.chain(self.flush_scrobbles())
        } else {
            save
        }
    }

    fn flush_scrobbles(&self) -> Task<cosmic::Action<Message>> {
        if !self.config.lastfm_connected() {
            return Task::none();
        }
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        let (http, creds, sk) = (
            self.http.clone(),
            self.lastfm_credentials(),
            self.config.lastfm_session.clone(),
        );
        cosmic::task::future(async move {
            let db2 = db.clone();
            let batch = tokio::task::spawn_blocking(move || {
                db2.lock()
                    .ok()
                    .and_then(|d| d.pending_scrobbles(50).ok())
                    .unwrap_or_default()
            })
            .await
            .unwrap_or_default();
            if batch.is_empty() {
                return Message::LastfmFlushed(Ok(0));
            }
            match lastfm::scrobble(&http, &creds, &sk, &batch).await {
                Ok(ids) => {
                    let n = ids.len();
                    let _ = tokio::task::spawn_blocking(move || {
                        if let Ok(d) = db.lock() {
                            let _ = d.delete_scrobbles(&ids);
                        }
                    })
                    .await;
                    Message::LastfmFlushed(Ok(n))
                }
                Err(err) => Message::LastfmFlushed(Err(err)),
            }
        })
    }

    fn library_update(&mut self, id: i64, f: impl FnOnce(&mut Track)) {
        Arc::make_mut(&mut self.library).update(id, f);
    }

    fn toggle_favorite(&mut self, id: i64) -> Task<cosmic::Action<Message>> {
        let Some(track) = self.library.get(id).cloned() else {
            return Task::none();
        };
        let fav = !track.favorite;
        self.library_update(id, |t| t.favorite = fav);
        let mut tasks = Vec::new();
        if let Some(db) = self.db.clone() {
            tasks.push(cosmic::task::future(async move {
                let _ = tokio::task::spawn_blocking(move || {
                    db.lock().ok().map(|d| d.set_favorite(id, fav).is_ok())
                })
                .await;
                Message::Noop
            }));
        }
        if self.config.lastfm_connected() && self.config.lastfm_love {
            let (http, creds, sk) = (
                self.http.clone(),
                self.lastfm_credentials(),
                self.config.lastfm_session.clone(),
            );
            tasks.push(cosmic::task::future(async move {
                if let Err(err) = lastfm::love(&http, &creds, &sk, &track.artist, &track.title, fav).await {
                    eprintln!("last.fm love: {err}");
                }
                Message::Noop
            }));
        }
        Task::batch(tasks)
    }

    fn track_action(&mut self, id: i64, action: TrackAction) -> Task<cosmic::Action<Message>> {
        let Some(track) = self.library.get(id).cloned() else {
            return Task::none();
        };
        match action {
            TrackAction::PlayNext => self.update(Message::QueueAdd(vec![id], true)),
            TrackAction::AddToQueue => self.update(Message::QueueAdd(vec![id], false)),
            TrackAction::GoToAlbum => match self.library.album_of(&track) {
                Some(album) => self.navigate(Page::Album(album.key.clone()), true),
                None => Task::none(),
            },
            TrackAction::GoToArtist => self.navigate(Page::Artist(track.grouping_artist().to_string()), true),
            TrackAction::ToggleFavorite => self.toggle_favorite(id),
            TrackAction::ShowInFolder => {
                if let Some(dir) = track.path.parent() {
                    let _ = open::that_detached(dir);
                }
                Task::none()
            }
        }
    }

    fn load_lyrics(&mut self, track: &Arc<Track>, use_cache: bool) -> Task<cosmic::Action<Message>> {
        if self.lyrics_for == Some(track.id) {
            return Task::none();
        }
        self.lyrics_for = Some(track.id);
        let providers = self.config.lyrics_providers();
        if providers.is_empty() {
            self.lyrics = LyricsState::NotFound;
            return Task::none();
        }
        self.lyrics = LyricsState::Loading;
        let (http, query, id) = (self.http.clone(), lyrics_query(track), track.id);
        cosmic::task::future(async move {
            Message::LyricsLoaded(id, lyrics::fetch(&http, &query, &providers, use_cache).await)
        })
    }

    fn load_ambient(&mut self, track: &Arc<Track>) -> Task<cosmic::Action<Message>> {
        let Some(key) = track.cover.clone() else {
            self.ambient = None;
            return Task::none();
        };
        if self.ambient.as_ref().is_some_and(|(k, _, _)| *k == key) {
            return Task::none();
        }
        cosmic::task::future(async move {
            let k = key.clone();
            let (blur, color) =
                tokio::task::spawn_blocking(move || (covers::blurred(&k), covers::dominant_color(&k)))
                    .await
                    .unwrap_or_default();
            Message::Ambient(key, blur, color)
        })
    }

    // --- System integration ----------------------------------------------------------------

    fn on_mpris(&mut self, event: mpris::Event) -> Task<cosmic::Action<Message>> {
        match event {
            mpris::Event::Ready(handle) => {
                self.mpris = Some(handle);
                self.publish_state();
                Task::none()
            }
            mpris::Event::Action(action) => match action {
                mpris::Action::Raise => match self.core.main_window_id() {
                    Some(id) => cosmic::iced::window::gain_focus(id),
                    None => Task::none(),
                },
                mpris::Action::Quit => self.menu_action(MenuAction::Quit),
                mpris::Action::Next => self.update(Message::Next),
                mpris::Action::Previous => self.update(Message::Previous),
                mpris::Action::PlayPause => self.update(Message::TogglePlay),
                mpris::Action::Play if !self.playing => self.update(Message::TogglePlay),
                mpris::Action::Pause if self.playing => self.update(Message::TogglePlay),
                mpris::Action::Play | mpris::Action::Pause => Task::none(),
                mpris::Action::Stop => self.update(Message::Stop),
                mpris::Action::SeekBy(us) => {
                    let pos = (self.position.as_micros() as i64 + us).max(0) as u64;
                    self.seek(Duration::from_micros(pos).min(self.duration))
                }
                mpris::Action::SetPosition(us) => self.seek(Duration::from_micros(us.max(0) as u64)),
                mpris::Action::Volume(v) => self.update(Message::Volume(v.sqrt() as f32)),
                mpris::Action::Shuffle(on) if on != self.config.shuffle => {
                    self.update(Message::ToggleShuffle)
                }
                mpris::Action::Shuffle(_) => Task::none(),
                mpris::Action::Loop(status) => {
                    let repeat = match status {
                        mpris_server::LoopStatus::None => Repeat::Off,
                        mpris_server::LoopStatus::Playlist => Repeat::All,
                        mpris_server::LoopStatus::Track => Repeat::One,
                    };
                    self.queue.repeat = repeat;
                    self.config.repeat = repeat;
                    self.save_config();
                    self.sync_preload();
                    self.publish_state();
                    Task::none()
                }
            },
        }
    }

    /// Pushes the playback state to MPRIS and Discord.
    fn publish_state(&self) {
        if let Some(m) = &self.mpris {
            let mut metadata = mpris_server::Metadata::new();
            if let Some(t) = self.queue.current() {
                metadata.set_trackid(Some(mpris::track_id(t.id)));
                metadata.set_title(Some(t.title.clone()));
                if !t.artist.is_empty() {
                    metadata.set_artist(Some(vec![t.artist.clone()]));
                }
                if !t.album.is_empty() {
                    metadata.set_album(Some(t.album.clone()));
                }
                if !t.album_artist.is_empty() {
                    metadata.set_album_artist(Some(vec![t.album_artist.clone()]));
                }
                metadata.set_length(Some(mpris_server::Time::from_millis(
                    self.duration.as_millis() as i64
                )));
                if let Some(key) = &t.cover {
                    let path = covers::path(key, covers::Size::Large);
                    metadata.set_art_url(Some(format!("file://{}", path.display())));
                }
                metadata.set_url(Some(format!("file://{}", t.path.display())));
                if let Some(n) = t.track_no {
                    metadata.set_track_number(Some(n as i32));
                }
                if !t.genre.is_empty() {
                    metadata.set_genre(Some(
                        crate::library::split_genres(&t.genre)
                            .map(str::to_string)
                            .collect::<Vec<_>>(),
                    ));
                }
                if let LyricsState::Found(l) = &self.lyrics {
                    metadata.set_lyrics(Some(
                        l.lines
                            .iter()
                            .map(|l| l.text.as_str())
                            .collect::<Vec<_>>()
                            .join("\n"),
                    ));
                }
            }
            let status = if self.queue.current().is_none() || (!self.loaded && self.resume_at.is_none()) {
                mpris_server::PlaybackStatus::Stopped
            } else if self.playing {
                mpris_server::PlaybackStatus::Playing
            } else {
                mpris_server::PlaybackStatus::Paused
            };
            let volume = if self.config.muted {
                0.0
            } else {
                f64::from(audio::amplitude(self.config.volume))
            };
            m.update(mpris::Snapshot {
                status,
                metadata,
                position_us: self.position.as_micros() as i64,
                volume,
                shuffle: self.config.shuffle,
                loop_status: match self.queue.repeat {
                    Repeat::Off => mpris_server::LoopStatus::None,
                    Repeat::All => mpris_server::LoopStatus::Playlist,
                    Repeat::One => mpris_server::LoopStatus::Track,
                },
                can_next: self.queue.next_index(true).is_some(),
                can_previous: self.queue.current().is_some(),
            });
        }
        self.publish_discord();
    }

    fn discord_key(t: &Track) -> (String, String) {
        (t.grouping_artist().to_string(), t.album.clone())
    }

    fn publish_discord(&self) {
        if !self.config.discord_enabled {
            return;
        }
        let presence = self
            .queue
            .current()
            .filter(|_| self.playing || !self.position.is_zero())
            .map(|t| discord::Presence {
                title: t.title.clone(),
                artist: t.artist.clone(),
                album: t.album.clone(),
                cover_url: self.discord_covers.get(&Self::discord_key(t)).cloned().flatten(),
                position: self.position,
                duration: self.duration,
                paused: !self.playing,
            });
        self.discord.set(presence);
    }

    /// Publishes the presence and looks the cover URL up if needed.
    fn publish_discord_task(&mut self) -> Task<cosmic::Action<Message>> {
        self.publish_discord();
        if !self.config.discord_enabled || !self.config.discord_online_covers {
            return Task::none();
        }
        let Some(track) = self.queue.current().cloned() else {
            return Task::none();
        };
        let key = Self::discord_key(&track);
        if key.1.is_empty() || self.discord_covers.contains_key(&key) {
            return Task::none();
        }
        self.discord_covers.insert(key.clone(), None);
        let http = self.http.clone();
        let creds = self.lastfm_credentials();
        cosmic::task::future(async move {
            let mut url = None;
            if !creds.api_key.is_empty() {
                url = lastfm::album_art_url(&http, &creds, &key.0, &key.1).await;
            }
            if url.is_none() {
                url = discord::itunes_cover(&http, &key.0, &key.1).await;
            }
            Message::DiscordCover(key, url)
        })
    }

    // --- Session -----------------------------------------------------------------------------

    fn save_session(&mut self) {
        let paths: Vec<String> = self
            .queue
            .items()
            .iter()
            .map(|t| t.path.to_string_lossy().into_owned())
            .collect();
        let index = self.queue.position().unwrap_or(0);
        let pos = self.position.as_millis() as u64;
        if paths != self.config.resume_paths
            || index != self.config.resume_index
            || pos != self.config.resume_position_ms
        {
            self.config.resume_paths = paths;
            self.config.resume_index = index;
            self.config.resume_position_ms = pos;
            self.save_config();
        }
    }

    /// Restores the previous queue (paused) on startup.
    fn restore_session(&mut self) -> Task<cosmic::Action<Message>> {
        if self.config.resume_paths.is_empty() {
            return Task::none();
        }
        let by_path: HashMap<String, Arc<Track>> = self
            .library
            .tracks()
            .iter()
            .map(|t| (t.path.to_string_lossy().into_owned(), t.clone()))
            .collect();
        let tracks: Vec<Arc<Track>> = self
            .config
            .resume_paths
            .iter()
            .filter_map(|p| by_path.get(p).cloned())
            .collect();
        if tracks.is_empty() {
            return Task::none();
        }
        let index = self.config.resume_index.min(tracks.len() - 1);
        self.queue.set(tracks, index, false);
        if self.config.shuffle {
            // The saved queue is already in play order.
            self.queue.set_shuffle(false);
        }
        let Some(track) = self.queue.current().cloned() else {
            return Task::none();
        };
        let at = Duration::from_millis(self.config.resume_position_ms);
        self.resume_at = Some(at);
        self.position = at;
        self.duration = track.duration();
        self.page = Page::NowPlaying;
        self.rebuild_nav();
        let lyrics = self.load_lyrics(&track, true);
        let ambient = self.load_ambient(&track);
        self.publish_state();
        Task::batch([lyrics, ambient])
    }

    fn menu_action(&mut self, action: MenuAction) -> Task<cosmic::Action<Message>> {
        match action {
            MenuAction::OpenFolder => self.update(Message::OpenFolderDialog),
            MenuAction::AddLibraryFolder => self.update(Message::AddLibraryFolder),
            MenuAction::Rescan => self.start_scan(false),
            MenuAction::Settings => self.update(Message::ToggleContextPage(ContextPage::Settings)),
            MenuAction::About => self.update(Message::ToggleContextPage(ContextPage::About)),
            MenuAction::Repository => self.update(Message::LaunchUrl(REPOSITORY.to_string())),
            MenuAction::Quit => {
                self.save_session();
                self.discord.set(None);
                std::process::exit(0);
            }
            MenuAction::PlayPause => self.update(Message::TogglePlay),
            MenuAction::Next => self.update(Message::Next),
            MenuAction::Previous => self.update(Message::Previous),
            MenuAction::Stop => self.update(Message::Stop),
            MenuAction::Shuffle => self.update(Message::ToggleShuffle),
            MenuAction::Repeat => self.update(Message::CycleRepeat),
            MenuAction::SeekForward => self.update(Message::SeekBy(10)),
            MenuAction::SeekBackward => self.update(Message::SeekBy(-10)),
            MenuAction::VolumeUp => self.update(Message::Volume(self.config.volume + 0.05)),
            MenuAction::VolumeDown => self.update(Message::Volume(self.config.volume - 0.05)),
            MenuAction::Search => widget::text_input::focus(self.search_id.clone()),
            MenuAction::Page(i) => match Page::SIDEBAR.get(i) {
                Some(page) => {
                    self.history.clear();
                    self.navigate(page.clone(), false)
                }
                None => Task::none(),
            },
        }
    }
}
