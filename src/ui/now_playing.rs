// SPDX-License-Identifier: GPL-3.0-only

//! "Now playing": large cover over an ambient blurred background, and lyrics.

use cosmic::iced::{Alignment, Background, Border, Color, Length};
use cosmic::prelude::*;
use cosmic::widget;

use super::{accent_text, artist_label, cover, dim_text, empty_state};
use crate::app::{App, LyricsState, Message, Page};
use crate::fl;
use crate::library::{Track, album_title};

/// Synced lyrics: lines shown before / after the current one.
const BEFORE: usize = 3;
const AFTER: usize = 7;

pub fn view(app: &App) -> Element<'_, Message> {
    let Some(track) = app.queue.current() else {
        return empty_state(
            "media-playback-start-symbolic",
            fl!("nothing-playing"),
            fl!("nothing-playing-body"),
        );
    };

    let content = widget::row::with_capacity(2)
        .push(
            widget::container(info(app, track))
                .width(Length::FillPortion(2))
                .center_x(Length::FillPortion(2)),
        )
        .push(
            widget::container(lyrics(app))
                .width(Length::FillPortion(3))
                .height(Length::Fill),
        )
        .spacing(32)
        .padding([24, 32])
        .align_y(Alignment::Center)
        .height(Length::Fill);

    // Ambient background: tiny blurred cover stretched over the page, under a tinted veil.
    let tint = app.ambient.as_ref().and_then(|(_, _, c)| *c);
    let veil = widget::container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .class(cosmic::theme::Container::custom(move |theme| {
            let bg: Color = theme.cosmic().bg_color().into();
            let mut c = match tint {
                Some([r, g, b]) => Color::from_rgb(
                    bg.r * 0.72 + r * 0.28,
                    bg.g * 0.72 + g * 0.28,
                    bg.b * 0.72 + b * 0.28,
                ),
                None => bg,
            };
            c.a = 0.78;
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(c)),
                ..Default::default()
            }
        }));

    match app.ambient.as_ref().and_then(|(_, blur, _)| blur.clone()) {
        Some(blur) => cosmic::iced::widget::stack![
            widget::image(widget::image::Handle::from_path(blur))
                .width(Length::Fill)
                .height(Length::Fill)
                .content_fit(cosmic::iced::ContentFit::Cover),
            veil,
        ]
        .into(),
        None => veil.into(),
    }
}

fn info<'a>(app: &'a App, t: &'a Track) -> Element<'a, Message> {
    let art = widget::container(cover(t.cover.as_deref(), 340.0, 18.0, true)).class(
        cosmic::theme::Container::custom(|_| cosmic::iced::widget::container::Style {
            shadow: cosmic::iced::Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.45),
                offset: cosmic::iced::Vector::new(0.0, 14.0),
                blur_radius: 36.0,
            },
            border: Border {
                radius: 18.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }),
    );

    let mut col = widget::column::with_capacity(8)
        .push(art)
        .push(widget::Space::new().height(Length::Fixed(12.0)))
        .push(
            widget::text::title2(t.title.as_str())
                .align_x(cosmic::iced::alignment::Horizontal::Center)
                .width(Length::Fill),
        )
        .push(
            widget::button::link(artist_label(t))
                .on_press(Message::Navigate(Page::Artist(t.grouping_artist().to_string()))),
        );

    if let Some(album) = album_title(&t.album) {
        let label = match t.year {
            Some(y) => format!("{album} · {y}"),
            None => album.to_string(),
        };
        let mut btn = widget::button::link(label);
        if let Some(a) = app.library.album_of(t) {
            btn = btn.on_press(Message::Navigate(Page::Album(a.key.clone())));
        }
        col = col.push(btn);
    }

    // Technical badge: FLAC · 24 bit · 96 kHz
    col = col.push(
        widget::container(widget::text::caption(t.quality()).class(cosmic::theme::Text::Custom(dim_text)))
            .padding([3, 10])
            .class(cosmic::theme::Container::custom(|theme| {
                let mut c: Color = theme.cosmic().on_bg_color().into();
                c.a = 0.08;
                cosmic::iced::widget::container::Style {
                    background: Some(Background::Color(c)),
                    border: Border {
                        radius: 12.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })),
    );

    if let Some(next) = app.queue.peek_next().filter(|n| n.id != t.id) {
        col = col.push(widget::Space::new().height(Length::Fixed(8.0))).push(
            widget::text::caption(fl!(
                "up-next",
                title = next.title.as_str(),
                artist = artist_label(next)
            ))
            .class(cosmic::theme::Text::Custom(dim_text)),
        );
    }

    col.spacing(6).align_x(Alignment::Center).max_width(420.0).into()
}

fn lyrics(app: &App) -> Element<'_, Message> {
    let header = widget::row::with_capacity(3)
        .push(widget::text::heading(fl!("lyrics")).width(Length::Fill))
        .push_maybe(match &app.lyrics {
            LyricsState::Found(l) => Some(
                widget::text::caption(fl!(
                    "lyrics-source",
                    source = l.provider.name(),
                    kind = if l.synced {
                        fl!("lyrics-synced")
                    } else {
                        fl!("lyrics-plain")
                    }
                ))
                .class(cosmic::theme::Text::Custom(dim_text)),
            ),
            _ => None,
        })
        .push(
            widget::button::icon(widget::icon::from_name("view-refresh-symbolic"))
                .tooltip(fl!("lyrics-refresh"))
                .on_press(Message::LyricsRefresh),
        )
        .spacing(8)
        .align_y(Alignment::Center);

    let body: Element<_> = match &app.lyrics {
        LyricsState::Idle | LyricsState::Loading => widget::container(
            widget::text::body(fl!("lyrics-loading")).class(cosmic::theme::Text::Custom(dim_text)),
        )
        .center(Length::Fill)
        .into(),
        LyricsState::NotFound => widget::container(
            widget::column::with_capacity(2)
                .push(
                    widget::icon::from_name("format-justify-left-symbolic")
                        .size(48)
                        .icon(),
                )
                .push(widget::text::body(fl!("lyrics-none")).class(cosmic::theme::Text::Custom(dim_text)))
                .spacing(12)
                .align_x(Alignment::Center),
        )
        .center(Length::Fill)
        .into(),
        LyricsState::Found(l) if l.synced => {
            let pos = app
                .seeking
                .map_or(app.position, std::time::Duration::from_secs_f32);
            let current = l.current_line(pos);
            let center = current.unwrap_or(0);
            let start = center.saturating_sub(BEFORE);
            let end = (center + AFTER + 1).min(l.lines.len());
            let mut col = widget::column::with_capacity(end - start)
                .spacing(10)
                .width(Length::Fill);
            for (i, line) in l.lines.iter().enumerate().take(end).skip(start) {
                let text = if line.text.is_empty() {
                    "♪".to_string()
                } else {
                    line.text.clone()
                };
                let label: Element<_> = if Some(i) == current {
                    widget::text::title3(text)
                        .class(cosmic::theme::Text::Custom(accent_text))
                        .into()
                } else if current.is_some_and(|c| i < c) {
                    widget::text::body(text)
                        .class(cosmic::theme::Text::Custom(faded_text))
                        .into()
                } else {
                    widget::text::title4(text)
                        .class(cosmic::theme::Text::Custom(dim_text))
                        .into()
                };
                col = col.push(
                    widget::button::custom(label)
                        .class(cosmic::theme::Button::Text)
                        .padding([2, 4])
                        .on_press(Message::SeekToLine(i)),
                );
            }
            widget::container(col).center_y(Length::Fill).into()
        }
        LyricsState::Found(l) => {
            let mut col = widget::column::with_capacity(l.lines.len())
                .spacing(6)
                .width(Length::Fill);
            for line in &l.lines {
                col = col.push(widget::text::body(line.text.as_str()));
            }
            widget::scrollable(col).height(Length::Fill).into()
        }
    };

    widget::container(
        widget::column::with_capacity(2)
            .push(header)
            .push(body)
            .spacing(12)
            .height(Length::Fill),
    )
    .padding(20)
    .height(Length::Fill)
    .class(cosmic::theme::Container::custom(|theme| {
        let mut c: Color = theme.cosmic().bg_color().into();
        c.a = 0.35;
        cosmic::iced::widget::container::Style {
            background: Some(Background::Color(c)),
            border: Border {
                radius: 18.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }))
    .into()
}

fn faded_text(theme: &cosmic::Theme) -> cosmic::iced::widget::text::Style {
    let mut c: Color = theme.cosmic().on_bg_color().into();
    c.a = 0.35;
    cosmic::iced::widget::text::Style {
        color: Some(c),
        ..Default::default()
    }
}
