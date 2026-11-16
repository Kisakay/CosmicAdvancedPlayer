// SPDX-License-Identifier: GPL-3.0-only

//! Views. Each submodule renders one area of the window.

pub mod albums;
pub mod folders;
pub mod lists;
pub mod now_playing;
pub mod player_bar;
pub mod settings;

use std::collections::HashMap;
use std::sync::LazyLock;

use cosmic::iced::widget::scrollable::{AbsoluteOffset, scroll_to};
use cosmic::iced::{Alignment, Background, Border, Color, Length};
use cosmic::prelude::*;
use cosmic::widget::{self, menu};

use crate::app::{App, ContextPage, MenuAction, Message, Page, TrackAction, TrackMenu};
use crate::fl;
use crate::library::{Track, covers, format_duration};

static SCROLL_ID: LazyLock<widget::Id> = LazyLock::new(|| widget::Id::new("main-scroll"));

/// Height of a track row; rows must all have this height (virtualised lists).
pub const ROW_HEIGHT: f32 = 56.0;

pub fn scroll_to_top<T: Send + 'static>() -> Task<T> {
    scroll_to(
        SCROLL_ID.clone(),
        AbsoluteOffset {
            x: None,
            y: Some(0.0),
        },
    )
}

/// The main scrollable of a page (reports its offset for list virtualisation).
pub fn page_scroll<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    widget::scrollable(content)
        .id(SCROLL_ID.clone())
        .on_scroll(|vp| Message::Scrolled(vp.absolute_offset().y, vp.bounds().height, vp.bounds().width))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Main window content.
pub fn view(app: &App) -> Element<'_, Message> {
    if app.library_loaded && app.library.is_empty() && app.queue.is_empty() && app.page != Page::Search {
        return welcome(app);
    }
    let content = match &app.page {
        Page::NowPlaying => return now_playing::view(app),
        Page::Songs => lists::songs(app),
        Page::Albums => albums::grid(app),
        Page::Artists => albums::artists(app),
        Page::Genres => albums::genres(app),
        Page::Folders => folders::view(app, None),
        Page::Favorites => lists::favorites(app),
        Page::Queue => lists::queue(app),
        Page::Search => lists::search(app),
        Page::Album(key) => albums::album(app, key),
        Page::Artist(name) => albums::artist(app, name),
        Page::Genre(name) => lists::genre(app, name),
        Page::Folder(path) => folders::view(app, Some(path)),
    };
    let mut col = widget::column::with_capacity(2);
    if let Some((done, total)) = app.scanning {
        col = col.push(scan_banner(done, total));
    }
    col.push(content).into()
}

fn scan_banner<'a>(done: usize, total: usize) -> Element<'a, Message> {
    let label = if total == 0 {
        fl!("scanning")
    } else {
        fl!("scanning-progress", done = done, total = total)
    };
    let progress = if total == 0 {
        0.0
    } else {
        done as f32 / total as f32
    };
    widget::container(
        widget::row::with_capacity(2)
            .push(widget::text::caption(label).width(Length::Shrink))
            .push(
                widget::progress_bar::determinate_linear(progress)
                    .width(Length::Fill)
                    .girth(Length::Fixed(4.0)),
            )
            .spacing(12)
            .align_y(Alignment::Center),
    )
    .padding([6, 24])
    .into()
}

fn welcome(app: &App) -> Element<'_, Message> {
    let icon = widget::icon::from_svg_bytes(crate::app::APP_ICON)
        .icon()
        .size(128);
    let mut col = widget::column::with_capacity(6)
        .push(icon)
        .push(widget::text::title1(fl!("welcome-title")))
        .push(widget::text::body(fl!("welcome-body")).class(cosmic::theme::Text::Custom(dim_text)))
        .push(
            widget::row::with_capacity(2)
                .push(
                    widget::button::suggested(fl!("add-library-folder"))
                        .leading_icon(widget::icon::from_name("folder-new-symbolic"))
                        .on_press(Message::AddLibraryFolder),
                )
                .push(
                    widget::button::standard(fl!("open-folder"))
                        .leading_icon(widget::icon::from_name("document-open-symbolic"))
                        .on_press(Message::OpenFolderDialog),
                )
                .spacing(12),
        )
        .spacing(16)
        .align_x(Alignment::Center);
    if let Some((done, total)) = app.scanning {
        col = col.push(widget::text::caption(fl!(
            "scanning-progress",
            done = done,
            total = total
        )));
    }
    widget::container(col).center(Length::Fill).into()
}

/// The menu bar shown at the start of the header.
pub fn menu_bar(app: &App) -> Element<'_, Message> {
    let page_items: Vec<_> = Page::SIDEBAR
        .iter()
        .enumerate()
        .map(|(i, p)| menu::Item::Button(p.title(), None, MenuAction::Page(i)))
        .collect();
    let play_label = if app.playing { fl!("pause") } else { fl!("play") };
    menu::bar(vec![
        menu::Tree::with_children(
            menu::root(fl!("menu-file")).apply(Element::from),
            menu::items(
                &app.key_binds,
                vec![
                    menu::Item::Button(fl!("open-folder"), None, MenuAction::OpenFolder),
                    menu::Item::Button(fl!("add-library-folder"), None, MenuAction::AddLibraryFolder),
                    menu::Item::Button(fl!("rescan"), None, MenuAction::Rescan),
                    menu::Item::Divider,
                    menu::Item::Button(fl!("settings"), None, MenuAction::Settings),
                    menu::Item::Divider,
                    menu::Item::Button(fl!("quit"), None, MenuAction::Quit),
                ],
            ),
        ),
        menu::Tree::with_children(
            menu::root(fl!("menu-playback")).apply(Element::from),
            menu::items(
                &app.key_binds,
                vec![
                    menu::Item::Button(play_label, None, MenuAction::PlayPause),
                    menu::Item::Button(fl!("next"), None, MenuAction::Next),
                    menu::Item::Button(fl!("previous"), None, MenuAction::Previous),
                    menu::Item::Button(fl!("stop"), None, MenuAction::Stop),
                    menu::Item::Divider,
                    menu::Item::CheckBox(fl!("shuffle"), None, app.config.shuffle, MenuAction::Shuffle),
                    menu::Item::Button(fl!("repeat"), None, MenuAction::Repeat),
                    menu::Item::Divider,
                    menu::Item::Button(fl!("seek-forward"), None, MenuAction::SeekForward),
                    menu::Item::Button(fl!("seek-backward"), None, MenuAction::SeekBackward),
                    menu::Item::Button(fl!("volume-up"), None, MenuAction::VolumeUp),
                    menu::Item::Button(fl!("volume-down"), None, MenuAction::VolumeDown),
                ],
            ),
        ),
        menu::Tree::with_children(
            menu::root(fl!("menu-view")).apply(Element::from),
            menu::items(&app.key_binds, page_items),
        ),
        menu::Tree::with_children(
            menu::root(fl!("menu-help")).apply(Element::from),
            menu::items(
                &app.key_binds,
                vec![
                    menu::Item::Button(fl!("about"), None, MenuAction::About),
                    menu::Item::Button(fl!("about-repository"), None, MenuAction::Repository),
                ],
            ),
        ),
    ])
    .into()
}

pub fn header_end(app: &App) -> Vec<Element<'_, Message>> {
    let search = widget::search_input(fl!("search-placeholder"), app.search.as_str())
        .id(app.search_id.clone())
        .on_input(Message::SearchInput)
        .on_clear(Message::SearchClear)
        .width(Length::Fixed(280.0));
    let mut items: Vec<Element<_>> = Vec::new();
    if app.scanning.is_some() {
        items.push(
            widget::tooltip(
                widget::icon::from_name("emblem-synchronizing-symbolic")
                    .size(16)
                    .icon(),
                widget::text::caption(fl!("scanning")),
                widget::tooltip::Position::Bottom,
            )
            .into(),
        );
    }
    items.push(search.into());
    items.push(
        widget::button::icon(widget::icon::from_name("emblem-system-symbolic"))
            .tooltip(fl!("settings"))
            .on_press(Message::ToggleContextPage(ContextPage::Settings))
            .into(),
    );
    items
}

// --- Shared building blocks ------------------------------------------------------------

pub fn dim_text(theme: &cosmic::Theme) -> cosmic::iced::widget::text::Style {
    let mut c: Color = theme.cosmic().on_bg_color().into();
    c.a = 0.62;
    cosmic::iced::widget::text::Style {
        color: Some(c),
        ..Default::default()
    }
}

pub fn accent_text(theme: &cosmic::Theme) -> cosmic::iced::widget::text::Style {
    cosmic::iced::widget::text::Style {
        color: Some(theme.cosmic().accent_text_color().into()),
        ..Default::default()
    }
}

/// Album art (or a placeholder) of a fixed size.
pub fn cover<'a>(key: Option<&str>, size: f32, radius: f32, large: bool) -> Element<'a, Message> {
    match key {
        Some(key) => {
            let path = covers::path(
                key,
                if large {
                    covers::Size::Large
                } else {
                    covers::Size::Thumb
                },
            );
            widget::image(widget::image::Handle::from_path(path))
                .width(Length::Fixed(size))
                .height(Length::Fixed(size))
                .content_fit(cosmic::iced::ContentFit::Cover)
                .border_radius([radius; 4])
                .into()
        }
        None => widget::container(
            widget::icon::from_name("audio-x-generic-symbolic")
                .size((size * 0.4).max(16.0) as u16)
                .icon(),
        )
        .center(Length::Fixed(size))
        .class(cosmic::theme::Container::custom(move |theme| {
            let c = theme.cosmic();
            let mut bg: Color = c.accent_color().into();
            bg.a = 0.16;
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(bg)),
                border: Border {
                    radius: radius.into(),
                    ..Default::default()
                },
                icon_color: Some(c.accent_color().into()),
                ..Default::default()
            }
        }))
        .into(),
    }
}

/// Big page title with an optional subtitle and actions.
pub fn page_header<'a>(
    title: impl Into<String>,
    subtitle: Option<String>,
    actions: Vec<Element<'a, Message>>,
    back: bool,
) -> Element<'a, Message> {
    let mut title_col = widget::column::with_capacity(2).push(widget::text::title2(title.into()));
    if let Some(sub) = subtitle {
        title_col = title_col.push(widget::text::body(sub).class(cosmic::theme::Text::Custom(dim_text)));
    }
    let mut row = widget::row::with_capacity(3)
        .spacing(12)
        .align_y(Alignment::Center);
    if back {
        row = row.push(
            widget::button::icon(widget::icon::from_name("go-previous-symbolic"))
                .tooltip(fl!("back"))
                .on_press(Message::Back),
        );
    }
    row = row.push(title_col.width(Length::Fill));
    for a in actions {
        row = row.push(a);
    }
    widget::container(row).padding([20, 24, 8, 24]).into()
}

/// "Play" / "Shuffle" buttons for a list of tracks.
pub fn play_buttons<'a>(ids: &[i64]) -> Vec<Element<'a, Message>> {
    if ids.is_empty() {
        return Vec::new();
    }
    vec![
        widget::button::suggested(fl!("play"))
            .leading_icon(widget::icon::from_name("media-playback-start-symbolic"))
            .on_press(Message::PlayIds(ids.to_vec(), 0))
            .into(),
        widget::button::standard(fl!("shuffle"))
            .leading_icon(widget::icon::from_name("media-playlist-shuffle-symbolic"))
            .on_press(Message::PlayShuffled(ids.to_vec()))
            .into(),
        widget::button::icon(widget::icon::from_name("list-add-symbolic"))
            .tooltip(fl!("add-to-queue"))
            .on_press(Message::QueueAdd(ids.to_vec(), false))
            .into(),
    ]
}

pub fn artist_label(t: &Track) -> String {
    if t.artist.is_empty() {
        fl!("unknown-artist")
    } else {
        t.artist.clone()
    }
}

/// How the first column of a track row looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leading {
    Cover,
    TrackNumber,
}

/// Options for [`track_list`].
pub struct ListOptions {
    pub leading: Leading,
    pub show_album: bool,
    /// Approximate height of the content above the list inside the page scrollable.
    pub top: f32,
}

impl Default for ListOptions {
    fn default() -> Self {
        Self {
            leading: Leading::Cover,
            show_album: true,
            top: 120.0,
        }
    }
}

/// A virtualised grid of fixed-size cells: only the rows near the viewport are
/// built, so a library of thousands of albums costs as much as a screenful.
/// `top` is the approximate height of the content above the grid.
pub fn virtual_grid<'a>(
    app: &App,
    count: usize,
    (cell_w, cell_h): (f32, f32),
    top: f32,
    mut build: impl FnMut(usize) -> Element<'a, Message>,
) -> Element<'a, Message> {
    const GAP: f32 = 12.0;
    const PAD_X: f32 = 18.0;
    let width = (app.content_width() - 2.0 * PAD_X).max(cell_w);
    let cols = (((width + GAP) / (cell_w + GAP)).floor() as usize).max(1);
    let rows = count.div_ceil(cols);
    let row_h = cell_h + GAP;
    let (offset, height) = app.scroll;
    let first = (((offset - top) / row_h).floor().max(0.0) as usize)
        .saturating_sub(2)
        .min(rows);
    let last = (first + (height / row_h).ceil() as usize + 4).min(rows);

    let mut col = widget::column::with_capacity(last - first + 2).padding([8.0, PAD_X]);
    if first > 0 {
        col = col.push(widget::Space::new().height(Length::Fixed(first as f32 * row_h)));
    }
    for r in first..last {
        let mut row = widget::row::with_capacity(cols)
            .spacing(GAP)
            .height(Length::Fixed(row_h));
        for i in r * cols..((r + 1) * cols).min(count) {
            row = row.push(
                // No clip here: every clip is a separate render layer.
                widget::container(build(i))
                    .width(Length::Fixed(cell_w))
                    .height(Length::Fixed(cell_h)),
            );
        }
        col = col.push(row);
    }
    if last < rows {
        col = col.push(widget::Space::new().height(Length::Fixed((rows - last) as f32 * row_h)));
    }
    col.into()
}

/// A virtualised list of tracks: only the visible rows are built.
/// Clicking a row plays `ids` starting at that row.
pub fn track_list<'a>(app: &'a App, ids: &[i64], opts: &ListOptions) -> Element<'a, Message> {
    let (offset, height) = app.scroll;
    let first = (((offset - opts.top) / ROW_HEIGHT).floor().max(0.0) as usize)
        .saturating_sub(10)
        .min(ids.len());
    let visible = ((height / ROW_HEIGHT).ceil() as usize) + 20;
    let last = (first + visible).min(ids.len());

    let mut col = widget::column::with_capacity(last - first + 2).padding([0, 16]);
    if first > 0 {
        col = col.push(widget::Space::new().height(Length::Fixed(first as f32 * ROW_HEIGHT)));
    }
    let current = app.queue.current().map(|t| t.id);
    // One shared copy of the list for every row message.
    let shared: std::sync::Arc<[i64]> = std::sync::Arc::from(ids);
    for (i, id) in ids.iter().enumerate().take(last).skip(first) {
        if let Some(t) = app.library.get(*id) {
            col = col.push(track_row(app, t, i, &shared, current == Some(t.id), opts));
        }
    }
    if last < ids.len() {
        col = col.push(widget::Space::new().height(Length::Fixed((ids.len() - last) as f32 * ROW_HEIGHT)));
    }
    col.into()
}

fn track_row<'a>(
    app: &'a App,
    t: &'a Track,
    index: usize,
    ids: &std::sync::Arc<[i64]>,
    is_current: bool,
    opts: &ListOptions,
) -> Element<'a, Message> {
    let leading: Element<_> = match opts.leading {
        Leading::Cover => cover(t.cover.as_deref(), 40.0, 6.0, false),
        Leading::TrackNumber => {
            let label = if is_current && app.playing {
                widget::icon::from_name("audio-volume-high-symbolic")
                    .size(16)
                    .icon()
                    .into()
            } else {
                Element::from(
                    widget::text::body(t.track_no.map_or_else(|| "·".to_string(), |n| n.to_string()))
                        .class(cosmic::theme::Text::Custom(dim_text)),
                )
            };
            widget::container(label).center(Length::Fixed(40.0)).into()
        }
    };
    let title = if is_current {
        widget::text::body(t.title.as_str())
            .class(cosmic::theme::Text::Custom(accent_text))
            .font(cosmic::font::semibold())
    } else {
        widget::text::body(t.title.as_str())
    };
    let titles = widget::column::with_capacity(2)
        .push(title.wrapping(cosmic::iced::widget::text::Wrapping::None))
        .push(
            widget::text::caption(artist_label(t))
                .class(cosmic::theme::Text::Custom(dim_text))
                .wrapping(cosmic::iced::widget::text::Wrapping::None),
        )
        .width(Length::FillPortion(5))
        .clip(true);

    let mut row = widget::row::with_capacity(6)
        .push(leading)
        .push(titles)
        .spacing(14)
        .align_y(Alignment::Center);
    if opts.show_album {
        row = row.push(
            widget::container(
                widget::text::body(crate::library::album_title(&t.album).unwrap_or("").to_string())
                    .class(cosmic::theme::Text::Custom(dim_text))
                    .wrapping(cosmic::iced::widget::text::Wrapping::None),
            )
            .width(Length::FillPortion(4))
            .clip(true),
        );
    }
    let heart = if t.favorite {
        "starred-symbolic"
    } else {
        "non-starred-symbolic"
    };
    row = row
        .push(
            widget::button::icon(widget::icon::from_name(heart))
                .extra_small()
                .on_press(Message::ToggleFavorite(t.id)),
        )
        .push(
            widget::text::body(format_duration(t.duration()))
                .class(cosmic::theme::Text::Custom(dim_text))
                .width(Length::Fixed(56.0))
                .align_x(cosmic::iced::alignment::Horizontal::Right),
        );

    let button = widget::button::custom(row)
        .class(cosmic::theme::Button::ListItem([8.0; 4]))
        .selected(is_current)
        .padding([6, 10])
        .width(Length::Fill)
        .height(Length::Fixed(ROW_HEIGHT - 4.0))
        .on_press(Message::PlayList(ids.clone(), index));

    widget::container(
        widget::context_menu(button, Some(menu::items(&HashMap::new(), track_menu(t))))
            .on_surface_action(Message::Surface),
    )
    .height(Length::Fixed(ROW_HEIGHT))
    .center_y(Length::Fixed(ROW_HEIGHT))
    .into()
}

pub fn track_menu(t: &Track) -> Vec<menu::Item<TrackMenu, String>> {
    let id = t.id;
    vec![
        menu::Item::Button(fl!("play-next"), None, TrackMenu(id, TrackAction::PlayNext)),
        menu::Item::Button(fl!("add-to-queue"), None, TrackMenu(id, TrackAction::AddToQueue)),
        menu::Item::Divider,
        menu::Item::Button(fl!("go-to-album"), None, TrackMenu(id, TrackAction::GoToAlbum)),
        menu::Item::Button(fl!("go-to-artist"), None, TrackMenu(id, TrackAction::GoToArtist)),
        menu::Item::Divider,
        menu::Item::Button(
            if t.favorite {
                fl!("unfavorite")
            } else {
                fl!("favorite")
            },
            None,
            TrackMenu(id, TrackAction::ToggleFavorite),
        ),
        menu::Item::Button(
            fl!("show-in-folder"),
            None,
            TrackMenu(id, TrackAction::ShowInFolder),
        ),
    ]
}

/// Centered placeholder for empty pages.
pub fn empty_state<'a>(icon: &'static str, title: String, body: String) -> Element<'a, Message> {
    widget::container(
        widget::column::with_capacity(3)
            .push(widget::icon::from_name(icon).size(64).icon())
            .push(widget::text::title3(title))
            .push(widget::text::body(body).class(cosmic::theme::Text::Custom(dim_text)))
            .spacing(10)
            .align_x(Alignment::Center),
    )
    .center(Length::Fill)
    .padding(48)
    .into()
}
