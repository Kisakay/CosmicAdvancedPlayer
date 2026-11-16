// SPDX-License-Identifier: GPL-3.0-only

//! Albums grid, album / artist pages, artists and genres overviews.

use cosmic::iced::{Alignment, Background, Border, Color, Length};
use cosmic::prelude::*;
use cosmic::widget;

use super::{
    Leading, ListOptions, cover, dim_text, empty_state, page_header, page_scroll, play_buttons, track_list,
    virtual_grid,
};
use crate::app::{App, Message, Page};
use crate::fl;
use crate::library::{Album, AlbumKey, album_title, format_duration};

const CARD: f32 = 176.0;
/// Grid cells (button padding included); every card must fit in its cell.
const ALBUM_CELL: (f32, f32) = (CARD + 12.0, 238.0);
const ARTIST_CELL: (f32, f32) = (162.0, 204.0);
const GENRE_CELL: (f32, f32) = (200.0, 96.0);

pub fn album_card<'a>(album: &'a Album, size: f32) -> Element<'a, Message> {
    let title = album_title(&album.key.title).map_or_else(|| fl!("unknown-album"), str::to_string);
    let artist = if album.key.artist.is_empty() {
        fl!("unknown-artist")
    } else {
        album.key.artist.clone()
    };
    let col = widget::column::with_capacity(3)
        .push(cover(album.cover.as_deref(), size, 10.0, false))
        .push(
            widget::text::body(title)
                .font(cosmic::font::semibold())
                .wrapping(cosmic::iced::widget::text::Wrapping::None),
        )
        .push(
            widget::text::caption(artist)
                .class(cosmic::theme::Text::Custom(dim_text))
                .wrapping(cosmic::iced::widget::text::Wrapping::None),
        )
        .spacing(4)
        .width(Length::Fixed(size))
        .clip(true);
    widget::button::custom(col)
        .class(cosmic::theme::Button::Image)
        .padding(6)
        .on_press(Message::Navigate(Page::Album(album.key.clone())))
        .into()
}

pub fn grid_view(app: &App) -> Element<'_, Message> {
    let albums = &app.library.albums;
    virtual_grid(app, albums.len(), ALBUM_CELL, 100.0, |i| {
        album_card(&albums[i], CARD)
    })
}

pub fn grid(app: &App) -> Element<'_, Message> {
    if app.library.albums.is_empty() {
        return empty_state(
            "media-optical-symbolic",
            fl!("library-empty"),
            fl!("library-empty-body"),
        );
    }
    let header = page_header(
        fl!("page-albums"),
        Some(fl!("album-count", count = app.library.albums.len())),
        Vec::new(),
        false,
    );
    page_scroll(
        widget::column::with_capacity(2)
            .push(header)
            .push(grid_view(app))
            .padding([0, 0, 24, 0]),
    )
}

pub fn album<'a>(app: &'a App, key: &AlbumKey) -> Element<'a, Message> {
    let Some(album) = app.library.album(key) else {
        return empty_state("media-optical-symbolic", fl!("unknown-album"), String::new());
    };
    let title = album_title(&album.key.title).map_or_else(|| fl!("unknown-album"), str::to_string);
    let mut facts = Vec::new();
    if let Some(y) = album.year {
        facts.push(y.to_string());
    }
    facts.push(fl!("track-count", count = album.tracks.len()));
    facts.push(format_duration(std::time::Duration::from_millis(
        album.duration_ms,
    )));
    if !album.genre.is_empty() {
        facts.push(album.genre.clone());
    }

    let mut buttons = widget::row::with_capacity(3).spacing(8);
    for b in play_buttons(&album.tracks) {
        buttons = buttons.push(b);
    }
    let info = widget::column::with_capacity(5)
        .push(widget::text::caption(fl!("album").to_uppercase()).class(cosmic::theme::Text::Custom(dim_text)))
        .push(widget::text::title1(title))
        .push(
            widget::button::link(if album.key.artist.is_empty() {
                fl!("unknown-artist")
            } else {
                album.key.artist.clone()
            })
            .on_press(Message::Navigate(Page::Artist(album.key.artist.clone()))),
        )
        .push(widget::text::body(facts.join(" · ")).class(cosmic::theme::Text::Custom(dim_text)))
        .push(widget::Space::new().height(Length::Fixed(8.0)))
        .push(buttons)
        .spacing(6)
        .width(Length::Fill);

    let header = widget::row::with_capacity(3)
        .push(
            widget::button::icon(widget::icon::from_name("go-previous-symbolic"))
                .tooltip(fl!("back"))
                .on_press_maybe(app.can_go_back().then_some(Message::Back)),
        )
        .push(cover(album.cover.as_deref(), 220.0, 14.0, true))
        .push(info)
        .spacing(24)
        .align_y(Alignment::End)
        .padding([24, 24, 16, 24]);

    page_scroll(
        widget::column::with_capacity(2)
            .push(header)
            .push(track_list(
                app,
                &album.tracks,
                &ListOptions {
                    leading: Leading::TrackNumber,
                    show_album: false,
                    top: 270.0,
                },
            ))
            .padding([0, 0, 24, 0]),
    )
}

pub fn artist<'a>(app: &'a App, name: &str) -> Element<'a, Message> {
    let Some(artist) = app.library.artist(name) else {
        return empty_state(
            "system-users-symbolic",
            name.to_string(),
            fl!("artist-unknown-body"),
        );
    };
    let albums: Vec<&Album> = artist
        .albums
        .iter()
        .filter_map(|k| app.library.album(k))
        .collect();
    let ids: Vec<i64> = albums.iter().flat_map(|a| a.tracks.iter().copied()).collect();

    let mut buttons = widget::row::with_capacity(3).spacing(8);
    for b in play_buttons(&ids) {
        buttons = buttons.push(b);
    }
    let header = widget::row::with_capacity(3)
        .push(
            widget::button::icon(widget::icon::from_name("go-previous-symbolic"))
                .tooltip(fl!("back"))
                .on_press_maybe(app.can_go_back().then_some(Message::Back)),
        )
        .push(cover(artist.cover.as_deref(), 160.0, 80.0, true))
        .push(
            widget::column::with_capacity(4)
                .push(
                    widget::text::caption(fl!("artist").to_uppercase())
                        .class(cosmic::theme::Text::Custom(dim_text)),
                )
                .push(widget::text::title1(if artist.name.is_empty() {
                    fl!("unknown-artist")
                } else {
                    artist.name.clone()
                }))
                .push(
                    widget::text::body(fl!(
                        "artist-stats",
                        albums = albums.len(),
                        tracks = artist.track_count
                    ))
                    .class(cosmic::theme::Text::Custom(dim_text)),
                )
                .push(buttons)
                .spacing(8),
        )
        .spacing(24)
        .align_y(Alignment::Center)
        .padding([24, 24, 8, 24]);

    page_scroll(
        widget::column::with_capacity(3)
            .push(header)
            .push(super::lists::section_title(fl!("page-albums")))
            .push(virtual_grid(app, albums.len(), ALBUM_CELL, 300.0, |i| {
                album_card(albums[i], CARD)
            }))
            .padding([0, 0, 24, 0]),
    )
}

pub fn artists(app: &App) -> Element<'_, Message> {
    if app.library.artists.is_empty() {
        return empty_state(
            "system-users-symbolic",
            fl!("library-empty"),
            fl!("library-empty-body"),
        );
    }
    let artists = &app.library.artists;
    let card = |i: usize| -> Element<'_, Message> {
        let a = &artists[i];
        {
            let name = if a.name.is_empty() {
                fl!("unknown-artist")
            } else {
                a.name.clone()
            };
            widget::button::custom(
                widget::column::with_capacity(3)
                    .push(cover(a.cover.as_deref(), 140.0, 70.0, false))
                    .push(
                        widget::text::body(name)
                            .font(cosmic::font::semibold())
                            .wrapping(cosmic::iced::widget::text::Wrapping::None),
                    )
                    .push(
                        widget::text::caption(fl!("album-count", count = a.albums.len()))
                            .class(cosmic::theme::Text::Custom(dim_text)),
                    )
                    .spacing(4)
                    .align_x(Alignment::Center)
                    .width(Length::Fixed(150.0))
                    .clip(true),
            )
            .class(cosmic::theme::Button::Image)
            .padding(6)
            .on_press(Message::Navigate(Page::Artist(a.name.clone())))
            .into()
        }
    };
    let header = page_header(
        fl!("page-artists"),
        Some(fl!("artist-count", count = app.library.artists.len())),
        Vec::new(),
        false,
    );
    page_scroll(
        widget::column::with_capacity(2)
            .push(header)
            .push(virtual_grid(app, artists.len(), ARTIST_CELL, 100.0, card))
            .padding([0, 0, 24, 0]),
    )
}

/// Stable, pleasant colour per genre name.
fn genre_color(name: &str) -> Color {
    let h = name
        .bytes()
        .fold(5381u32, |h, b| h.wrapping_mul(33) ^ u32::from(b));
    let hue = (h % 360) as f32;
    // HSL(hue, 55%, 50%) → RGB
    let (s, l) = (0.55f32, 0.5f32);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match hue as u32 / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    Color::from_rgb(r + m, g + m, b + m)
}

pub fn genres(app: &App) -> Element<'_, Message> {
    if app.library.genres.is_empty() {
        return empty_state(
            "view-grid-symbolic",
            fl!("genres-empty"),
            fl!("genres-empty-body"),
        );
    }
    let genres = &app.library.genres;
    let tile = |i: usize| -> Element<'_, Message> {
        let g = &genres[i];
        {
            let color = genre_color(&g.name);
            widget::button::custom(
                widget::container(
                    widget::column::with_capacity(2)
                        .push(
                            widget::text::title4(g.name.as_str())
                                .wrapping(cosmic::iced::widget::text::Wrapping::None),
                        )
                        .push(widget::text::caption(fl!("track-count", count = g.tracks.len())))
                        .spacing(4),
                )
                .padding(16)
                .width(Length::Fixed(200.0))
                .height(Length::Fixed(96.0))
                .clip(true)
                .class(cosmic::theme::Container::custom(move |_| {
                    cosmic::iced::widget::container::Style {
                        background: Some(Background::Gradient(
                            cosmic::iced::gradient::Linear::new(0.8)
                                .add_stop(0.0, color)
                                .add_stop(1.0, Color { a: 0.65, ..color })
                                .into(),
                        )),
                        text_color: Some(Color::WHITE),
                        border: Border {
                            radius: 14.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                })),
            )
            .class(cosmic::theme::Button::Image)
            .padding(0)
            .on_press(Message::Navigate(Page::Genre(g.name.clone())))
            .into()
        }
    };
    let header = page_header(
        fl!("page-genres"),
        Some(fl!("genre-count", count = app.library.genres.len())),
        Vec::new(),
        false,
    );
    page_scroll(
        widget::column::with_capacity(2)
            .push(header)
            .push(virtual_grid(app, genres.len(), GENRE_CELL, 100.0, tile))
            .padding([0, 0, 24, 0]),
    )
}
