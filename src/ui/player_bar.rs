// SPDX-License-Identifier: GPL-3.0-only

//! Bottom transport bar: current track, controls, seek bar and volume.

use cosmic::iced::{Alignment, Length};
use cosmic::prelude::*;
use cosmic::widget;

use super::{artist_label, cover, dim_text};
use crate::app::{App, Message, Page};
use crate::fl;
use crate::library::format_duration;
use crate::queue::Repeat;

fn toggle_button<'a>(
    icon: &'static str,
    active: bool,
    tooltip: String,
    msg: Message,
) -> Element<'a, Message> {
    let mut b = widget::button::icon(widget::icon::from_name(icon))
        .tooltip(tooltip)
        .on_press(msg);
    if active {
        b = b.class(cosmic::theme::Button::Suggested);
    }
    b.into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let current = app.queue.current();

    // --- Left: cover + titles --------------------------------------------------------
    let info: Element<_> = match current {
        Some(t) => {
            let titles = widget::column::with_capacity(2)
                .push(
                    widget::button::custom(
                        widget::text::body(t.title.as_str())
                            .font(cosmic::font::semibold())
                            .wrapping(cosmic::iced::widget::text::Wrapping::None),
                    )
                    .class(cosmic::theme::Button::Text)
                    .padding(0)
                    .on_press(Message::Navigate(Page::NowPlaying)),
                )
                .push(
                    widget::button::custom(
                        widget::text::caption(artist_label(t))
                            .class(cosmic::theme::Text::Custom(dim_text))
                            .wrapping(cosmic::iced::widget::text::Wrapping::None),
                    )
                    .class(cosmic::theme::Button::Text)
                    .padding(0)
                    .on_press(Message::Navigate(Page::Artist(t.grouping_artist().to_string()))),
                )
                .spacing(2)
                .clip(true);
            let mut row = widget::row::with_capacity(3)
                .push(
                    widget::button::custom(cover(t.cover.as_deref(), 56.0, 8.0, false))
                        .padding(0)
                        .class(cosmic::theme::Button::Image)
                        .on_press(Message::Navigate(Page::NowPlaying)),
                )
                .push(titles)
                .spacing(12)
                .align_y(Alignment::Center);
            if t.id > 0 {
                let fav = app.library.get(t.id).is_some_and(|t| t.favorite);
                row = row.push(
                    widget::button::icon(widget::icon::from_name(if fav {
                        "starred-symbolic"
                    } else {
                        "non-starred-symbolic"
                    }))
                    .tooltip(if fav { fl!("unfavorite") } else { fl!("favorite") })
                    .on_press(Message::ToggleFavorite(t.id)),
                );
            }
            row.into()
        }
        None => widget::row::with_capacity(2)
            .push(cover(None, 56.0, 8.0, false))
            .push(widget::text::body(fl!("nothing-playing")).class(cosmic::theme::Text::Custom(dim_text)))
            .spacing(12)
            .align_y(Alignment::Center)
            .into(),
    };

    // --- Center: transport + seek bar ------------------------------------------------
    let play_icon = if app.playing {
        "media-playback-pause-symbolic"
    } else {
        "media-playback-start-symbolic"
    };
    let repeat_icon = match app.queue.repeat {
        Repeat::One => "media-playlist-repeat-song-symbolic",
        _ => "media-playlist-repeat-symbolic",
    };
    let repeat_tip = match app.queue.repeat {
        Repeat::Off => fl!("repeat-off"),
        Repeat::All => fl!("repeat-all"),
        Repeat::One => fl!("repeat-one"),
    };
    let controls = widget::row::with_capacity(5)
        .push(toggle_button(
            "media-playlist-shuffle-symbolic",
            app.config.shuffle,
            fl!("shuffle"),
            Message::ToggleShuffle,
        ))
        .push(
            widget::button::icon(widget::icon::from_name("media-skip-backward-symbolic"))
                .tooltip(fl!("previous"))
                .on_press(Message::Previous),
        )
        .push(
            widget::button::icon(widget::icon::from_name(play_icon))
                .large()
                .class(cosmic::theme::Button::Suggested)
                .tooltip(if app.playing { fl!("pause") } else { fl!("play") })
                .on_press(Message::TogglePlay),
        )
        .push(
            widget::button::icon(widget::icon::from_name("media-skip-forward-symbolic"))
                .tooltip(fl!("next"))
                .on_press(Message::Next),
        )
        .push(toggle_button(
            repeat_icon,
            app.queue.repeat != Repeat::Off,
            repeat_tip,
            Message::CycleRepeat,
        ))
        .spacing(8)
        .align_y(Alignment::Center);

    let duration = app.duration.as_secs_f32().max(0.0);
    let position = app
        .seeking
        .unwrap_or(app.position.as_secs_f32())
        .min(duration.max(0.001));
    let seek = widget::row::with_capacity(3)
        .push(
            widget::text::caption(format_duration(std::time::Duration::from_secs_f32(position)))
                .class(cosmic::theme::Text::Custom(dim_text))
                .width(Length::Fixed(48.0))
                .align_x(cosmic::iced::alignment::Horizontal::Right),
        )
        .push(
            widget::slider(0.0..=duration.max(0.001), position, Message::SeekPreview)
                .on_release(Message::SeekCommit)
                .step(0.5_f32)
                .width(Length::Fill),
        )
        .push(
            widget::text::caption(format_duration(app.duration))
                .class(cosmic::theme::Text::Custom(dim_text))
                .width(Length::Fixed(48.0)),
        )
        .spacing(10)
        .align_y(Alignment::Center);

    let center = widget::column::with_capacity(2)
        .push(controls)
        .push(seek)
        .spacing(2)
        .align_x(Alignment::Center)
        .max_width(640.0);

    // --- Right: lyrics, queue, volume --------------------------------------------------
    let volume_icon = if app.config.muted || app.config.volume <= 0.0 {
        "audio-volume-muted-symbolic"
    } else if app.config.volume < 0.34 {
        "audio-volume-low-symbolic"
    } else if app.config.volume < 0.67 {
        "audio-volume-medium-symbolic"
    } else {
        "audio-volume-high-symbolic"
    };
    let right = widget::row::with_capacity(4)
        .push(toggle_button(
            "format-justify-left-symbolic",
            app.page == Page::NowPlaying,
            fl!("lyrics"),
            Message::Navigate(Page::NowPlaying),
        ))
        .push(toggle_button(
            "view-list-symbolic",
            app.page == Page::Queue,
            fl!("page-queue"),
            Message::Navigate(Page::Queue),
        ))
        .push(
            widget::button::icon(widget::icon::from_name(volume_icon))
                .tooltip(if app.config.muted {
                    fl!("unmute")
                } else {
                    fl!("mute")
                })
                .on_press(Message::ToggleMute),
        )
        .push(
            widget::slider(
                0.0..=1.0,
                if app.config.muted { 0.0 } else { app.config.volume },
                Message::Volume,
            )
            .step(0.01_f32)
            .width(Length::Fixed(110.0)),
        )
        .spacing(6)
        .align_y(Alignment::Center);

    let bar = widget::row::with_capacity(3)
        .push(widget::container(info).width(Length::FillPortion(3)).clip(true))
        .push(
            widget::container(center)
                .width(Length::FillPortion(4))
                .center_x(Length::FillPortion(4)),
        )
        .push(
            widget::container(right)
                .width(Length::FillPortion(3))
                .align_right(Length::FillPortion(3)),
        )
        .spacing(16)
        .align_y(Alignment::Center);

    widget::container(bar)
        .padding([10, 18])
        .width(Length::Fill)
        .class(cosmic::theme::Container::Card)
        .into()
}
