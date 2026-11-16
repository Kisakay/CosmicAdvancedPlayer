// SPDX-License-Identifier: GPL-3.0-only

//! Track lists: songs, favourites, genre, search results and the play queue.

use cosmic::iced::{Alignment, Length};
use cosmic::prelude::*;
use cosmic::widget;

use super::{
    ListOptions, ROW_HEIGHT, accent_text, artist_label, cover, dim_text, empty_state, page_header,
    page_scroll, play_buttons, track_list,
};
use crate::app::{App, Message, Page};
use crate::fl;
use crate::library::{SongSort, format_duration};

fn sort_button<'a>(app: &App, sort: SongSort, label: String) -> Element<'a, Message> {
    let active = app.songs_sort == sort;
    let mut b = widget::button::text(label).on_press(Message::SortSongs(sort));
    if active {
        b = b.trailing_icon(widget::icon::from_name(if app.songs_desc {
            "pan-up-symbolic"
        } else {
            "pan-down-symbolic"
        }));
        b = b.class(cosmic::theme::Button::Suggested);
    }
    b.into()
}

pub fn songs(app: &App) -> Element<'_, Message> {
    let header = page_header(
        fl!("page-songs"),
        Some(fl!(
            "library-stats",
            tracks = app.library.len(),
            albums = app.library.albums.len(),
            duration = crate::library::format_total(app.library.total_duration())
        )),
        play_buttons(&app.songs),
        false,
    );
    let sorts = widget::row::with_capacity(6)
        .push(sort_button(app, SongSort::Artist, fl!("sort-artist")))
        .push(sort_button(app, SongSort::Title, fl!("sort-title")))
        .push(sort_button(app, SongSort::Album, fl!("sort-album")))
        .push(sort_button(app, SongSort::Duration, fl!("sort-duration")))
        .push(sort_button(app, SongSort::Added, fl!("sort-added")))
        .push(sort_button(app, SongSort::Plays, fl!("sort-plays")))
        .spacing(4)
        .padding([0, 24, 8, 24]);
    page_scroll(
        widget::column::with_capacity(3)
            .push(header)
            .push(sorts)
            .push(track_list(
                app,
                &app.songs,
                &ListOptions {
                    top: 150.0,
                    ..ListOptions::default()
                },
            ))
            .padding([0, 0, 24, 0]),
    )
}

pub fn favorites(app: &App) -> Element<'_, Message> {
    let ids = app.library.favorites();
    if ids.is_empty() {
        return empty_state(
            "starred-symbolic",
            fl!("favorites-empty"),
            fl!("favorites-empty-body"),
        );
    }
    simple_list(app, fl!("page-favorites"), None, ids, false)
}

pub fn genre<'a>(app: &'a App, name: &str) -> Element<'a, Message> {
    let ids = app
        .library
        .genre(name)
        .map(|g| g.tracks.clone())
        .unwrap_or_default();
    simple_list(
        app,
        name.to_string(),
        Some(fl!("track-count", count = ids.len())),
        ids,
        true,
    )
}

/// Header + virtualised track list.
fn simple_list<'a>(
    app: &'a App,
    title: String,
    subtitle: Option<String>,
    ids: Vec<i64>,
    back: bool,
) -> Element<'a, Message> {
    let subtitle = subtitle.or_else(|| Some(fl!("track-count", count = ids.len())));
    let header = page_header(title, subtitle, play_buttons(&ids), back);
    page_scroll(
        widget::column::with_capacity(2)
            .push(header)
            .push(track_list(
                app,
                &ids,
                &ListOptions {
                    top: 100.0,
                    ..ListOptions::default()
                },
            ))
            .padding([0, 0, 24, 0]),
    )
}

pub fn search(app: &App) -> Element<'_, Message> {
    let q = app.search.trim().to_lowercase();
    if q.is_empty() {
        return empty_state(
            "system-search-symbolic",
            fl!("search-hint"),
            fl!("search-hint-body"),
        );
    }
    let albums: Vec<_> = app
        .library
        .albums
        .iter()
        .filter(|a| a.key.title.to_lowercase().contains(&q) || a.key.artist.to_lowercase().contains(&q))
        .take(12)
        .collect();
    let artists: Vec<_> = app
        .library
        .artists
        .iter()
        .filter(|a| a.name.to_lowercase().contains(&q))
        .take(12)
        .collect();

    if app.search_results.is_empty() && albums.is_empty() && artists.is_empty() {
        return empty_state(
            "system-search-symbolic",
            fl!("search-none"),
            fl!("search-none-body", query = app.search.as_str()),
        );
    }

    let mut col = widget::column::with_capacity(6).padding([0, 0, 24, 0]);
    col = col.push(page_header(
        fl!("search-results-for", query = app.search.as_str()),
        Some(fl!("track-count", count = app.search_results.len())),
        play_buttons(&app.search_results),
        true,
    ));
    let mut top = 100.0;
    if !artists.is_empty() {
        let mut chips = Vec::with_capacity(artists.len());
        for a in artists {
            chips.push(
                widget::button::custom(
                    widget::row::with_capacity(2)
                        .push(cover(a.cover.as_deref(), 32.0, 16.0, false))
                        .push(widget::text::body(a.name.as_str()))
                        .spacing(8)
                        .align_y(Alignment::Center),
                )
                .class(cosmic::theme::Button::Standard)
                .padding([4, 12, 4, 4])
                .on_press(Message::Navigate(Page::Artist(a.name.clone())))
                .into(),
            );
        }
        col = col.push(section_title(fl!("page-artists"))).push(
            widget::container(widget::flex_row(chips).row_spacing(8).column_spacing(8)).padding([0, 24]),
        );
        top += 100.0;
    }
    if !albums.is_empty() {
        let cards = albums
            .into_iter()
            .map(|a| super::albums::album_card(a, 150.0))
            .collect();
        col = col.push(section_title(fl!("page-albums"))).push(
            widget::container(widget::flex_row(cards).row_spacing(16).column_spacing(16)).padding([0, 24]),
        );
        top += 260.0;
    }
    if !app.search_results.is_empty() {
        col = col.push(section_title(fl!("page-songs"))).push(track_list(
            app,
            &app.search_results,
            &ListOptions {
                top,
                ..ListOptions::default()
            },
        ));
    }
    page_scroll(col)
}

pub fn section_title<'a>(title: String) -> Element<'a, Message> {
    widget::container(widget::text::title4(title))
        .padding([16, 24, 8, 24])
        .into()
}

pub fn queue(app: &App) -> Element<'_, Message> {
    if app.queue.is_empty() {
        return empty_state("view-list-symbolic", fl!("queue-empty"), fl!("queue-empty-body"));
    }
    let remaining = format_duration(std::time::Duration::from_millis(app.queue.remaining_ms()));
    let header = page_header(
        fl!("page-queue"),
        Some(fl!("queue-stats", count = app.queue.len(), remaining = remaining)),
        vec![
            widget::button::standard(fl!("queue-clear"))
                .leading_icon(widget::icon::from_name("edit-clear-all-symbolic"))
                .on_press(Message::QueueClear)
                .into(),
        ],
        false,
    );

    let items = app.queue.items();
    let current = app.queue.position();
    let (offset, height) = app.scroll;
    let top = 100.0;
    let first = (((offset - top) / ROW_HEIGHT).floor().max(0.0) as usize)
        .saturating_sub(10)
        .min(items.len());
    let last = (first + (height / ROW_HEIGHT).ceil() as usize + 20).min(items.len());

    let mut list = widget::column::with_capacity(last - first + 2).padding([0, 16]);
    if first > 0 {
        list = list.push(widget::Space::new().height(Length::Fixed(first as f32 * ROW_HEIGHT)));
    }
    for (i, t) in items.iter().enumerate().take(last).skip(first) {
        let is_current = current == Some(i);
        let title = if is_current {
            widget::text::body(t.title.as_str())
                .class(cosmic::theme::Text::Custom(accent_text))
                .font(cosmic::font::semibold())
        } else if current.is_some_and(|c| i < c) {
            widget::text::body(t.title.as_str()).class(cosmic::theme::Text::Custom(dim_text))
        } else {
            widget::text::body(t.title.as_str())
        };
        let row = widget::row::with_capacity(7)
            .push(
                widget::text::caption((i + 1).to_string())
                    .class(cosmic::theme::Text::Custom(dim_text))
                    .width(Length::Fixed(32.0))
                    .align_x(cosmic::iced::alignment::Horizontal::Right),
            )
            .push(cover(t.cover.as_deref(), 40.0, 6.0, false))
            .push(
                widget::column::with_capacity(2)
                    .push(title.wrapping(cosmic::iced::widget::text::Wrapping::None))
                    .push(
                        widget::text::caption(artist_label(t))
                            .class(cosmic::theme::Text::Custom(dim_text))
                            .wrapping(cosmic::iced::widget::text::Wrapping::None),
                    )
                    .width(Length::Fill)
                    .clip(true),
            )
            .push(
                widget::text::body(format_duration(t.duration()))
                    .class(cosmic::theme::Text::Custom(dim_text))
                    .width(Length::Fixed(56.0)),
            )
            .push(
                widget::button::icon(widget::icon::from_name("go-up-symbolic"))
                    .extra_small()
                    .on_press_maybe((i > 0).then_some(Message::QueueMove(i, i.saturating_sub(1)))),
            )
            .push(
                widget::button::icon(widget::icon::from_name("go-down-symbolic"))
                    .extra_small()
                    .on_press_maybe((i + 1 < items.len()).then_some(Message::QueueMove(i, i + 1))),
            )
            .push(
                widget::button::icon(widget::icon::from_name("list-remove-symbolic"))
                    .extra_small()
                    .on_press(Message::QueueRemove(i)),
            )
            .spacing(12)
            .align_y(Alignment::Center);
        list = list.push(
            widget::container(
                widget::button::custom(row)
                    .class(cosmic::theme::Button::ListItem([8.0; 4]))
                    .selected(is_current)
                    .padding([6, 10])
                    .width(Length::Fill)
                    .height(Length::Fixed(ROW_HEIGHT - 4.0))
                    .on_press(Message::QueueJump(i)),
            )
            .height(Length::Fixed(ROW_HEIGHT))
            .center_y(Length::Fixed(ROW_HEIGHT)),
        );
    }
    if last < items.len() {
        list =
            list.push(widget::Space::new().height(Length::Fixed((items.len() - last) as f32 * ROW_HEIGHT)));
    }
    page_scroll(
        widget::column::with_capacity(2)
            .push(header)
            .push(list)
            .padding([0, 0, 24, 0]),
    )
}
