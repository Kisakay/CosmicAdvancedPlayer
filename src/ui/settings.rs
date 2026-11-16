// SPDX-License-Identifier: GPL-3.0-only

//! Settings drawer: library, playback, lyrics plugins, Last.fm, Discord, appearance.

use cosmic::iced::{Alignment, Length};
use cosmic::prelude::*;
use cosmic::widget::{self, settings};

use super::dim_text;
use crate::app::{App, LastfmAuth, Message};
use crate::fl;
use crate::lyrics::Provider;

pub fn view(app: &App) -> Element<'_, Message> {
    widget::column::with_capacity(6)
        .push(library(app))
        .push(playback(app))
        .push(lyrics(app))
        .push(lastfm(app))
        .push(discord(app))
        .push(appearance(app))
        .spacing(cosmic::theme::spacing().space_m)
        .into()
}

fn caption<'a>(text: String) -> Element<'a, Message> {
    widget::text::caption(text)
        .class(cosmic::theme::Text::Custom(dim_text))
        .into()
}

fn library(app: &App) -> Element<'_, Message> {
    let mut section = settings::section().title(fl!("settings-library"));
    for (i, folder) in app.config.library_folders.iter().enumerate() {
        section = section.add(
            settings::item_row(vec![
                widget::icon::from_name("folder-music-symbolic")
                    .size(20)
                    .icon()
                    .into(),
                widget::text::body(folder.as_str()).width(Length::Fill).into(),
                widget::button::icon(widget::icon::from_name("list-remove-symbolic"))
                    .tooltip(fl!("remove"))
                    .on_press(Message::RemoveLibraryFolder(i))
                    .into(),
            ])
            .align_y(Alignment::Center),
        );
    }
    let scanning = app.scanning.is_some();
    section
        .add(settings::item_row(vec![
            widget::button::standard(fl!("add-library-folder"))
                .leading_icon(widget::icon::from_name("folder-new-symbolic"))
                .on_press(Message::AddLibraryFolder)
                .into(),
        ]))
        .add(
            settings::item_row(vec![
                widget::button::standard(fl!("rescan"))
                    .leading_icon(widget::icon::from_name("view-refresh-symbolic"))
                    .on_press_maybe((!scanning).then_some(Message::Rescan(false)))
                    .into(),
                widget::button::text(fl!("rescan-full"))
                    .on_press_maybe((!scanning).then_some(Message::Rescan(true)))
                    .into(),
            ])
            .spacing(8),
        )
        .add(settings::item(
            fl!("watch-folders"),
            widget::toggler(app.config.watch_folders).on_toggle(Message::WatchFolders),
        ))
        .add(caption(fl!(
            "library-stats",
            tracks = app.library.len(),
            albums = app.library.albums.len(),
            duration = crate::library::format_total(app.library.total_duration())
        )))
        .into()
}

fn playback(app: &App) -> Element<'_, Message> {
    settings::section()
        .title(fl!("settings-playback"))
        .add(settings::item(
            fl!("replay-gain"),
            widget::toggler(app.config.replay_gain).on_toggle(Message::ReplayGain),
        ))
        .add(settings::item(
            fl!("gapless"),
            widget::toggler(app.config.gapless).on_toggle(Message::Gapless),
        ))
        .into()
}

fn lyrics(app: &App) -> Element<'_, Message> {
    let mut section = settings::section().title(fl!("settings-lyrics"));
    let n = app.config.lyrics_order.len();
    for (i, p) in app.config.lyrics_order.iter().copied().enumerate() {
        let enabled = !app.config.lyrics_disabled.contains(&p);
        let desc = match p {
            Provider::LocalFile => fl!("provider-local"),
            Provider::Embedded => fl!("provider-embedded"),
            Provider::Lrclib => fl!("provider-lrclib"),
            Provider::LyricsOvh => fl!("provider-ovh"),
        };
        section = section.add(
            settings::item_row(vec![
                widget::text::body(format!("{}.", i + 1))
                    .class(cosmic::theme::Text::Custom(dim_text))
                    .into(),
                widget::column::with_capacity(2)
                    .push(widget::text::body(p.name()))
                    .push(widget::text::caption(desc).class(cosmic::theme::Text::Custom(dim_text)))
                    .width(Length::Fill)
                    .into(),
                widget::button::icon(widget::icon::from_name("go-up-symbolic"))
                    .extra_small()
                    .on_press_maybe((i > 0).then_some(Message::LyricsProviderMove(p, -1)))
                    .into(),
                widget::button::icon(widget::icon::from_name("go-down-symbolic"))
                    .extra_small()
                    .on_press_maybe((i + 1 < n).then_some(Message::LyricsProviderMove(p, 1)))
                    .into(),
                widget::toggler(enabled)
                    .on_toggle(move |on| Message::LyricsProviderEnabled(p, on))
                    .into(),
            ])
            .spacing(8)
            .align_y(Alignment::Center),
        );
    }
    section.add(caption(fl!("lyrics-order-hint"))).into()
}

fn lastfm(app: &App) -> Element<'_, Message> {
    let mut section = settings::section().title("Last.fm");
    if app.config.lastfm_connected() {
        section = section
            .add(
                settings::item_row(vec![
                    widget::icon::from_name("emblem-ok-symbolic")
                        .size(20)
                        .icon()
                        .into(),
                    widget::text::body(fl!("lastfm-connected-as", user = app.config.lastfm_user.as_str()))
                        .width(Length::Fill)
                        .into(),
                    widget::button::destructive(fl!("disconnect"))
                        .on_press(Message::LastfmDisconnect)
                        .into(),
                ])
                .align_y(Alignment::Center),
            )
            .add(settings::item(
                fl!("lastfm-scrobble"),
                widget::toggler(app.config.lastfm_scrobble).on_toggle(Message::LastfmScrobble),
            ))
            .add(settings::item(
                fl!("lastfm-now-playing"),
                widget::toggler(app.config.lastfm_now_playing).on_toggle(Message::LastfmNowPlaying),
            ))
            .add(settings::item(
                fl!("lastfm-love"),
                widget::toggler(app.config.lastfm_love).on_toggle(Message::LastfmLove),
            ));
        if app.lastfm_pending > 0 {
            section = section.add(caption(fl!("lastfm-pending", count = app.lastfm_pending)));
        }
        return section.into();
    }

    section = section
        .add(caption(fl!("lastfm-help")))
        .add(
            widget::button::link(fl!("lastfm-create-key")).on_press(Message::LaunchUrl(
                "https://www.last.fm/api/account/create".into(),
            )),
        )
        .add(
            widget::column::with_capacity(2)
                .push(widget::text::body(fl!("lastfm-api-key")))
                .push(
                    widget::text_input("", app.config.lastfm_api_key.as_str())
                        .on_input(Message::LastfmApiKey),
                )
                .spacing(4),
        )
        .add(
            widget::column::with_capacity(2)
                .push(widget::text::body(fl!("lastfm-secret")))
                .push(
                    widget::secure_input("", app.config.lastfm_secret.as_str(), None, true)
                        .on_input(Message::LastfmSecret),
                )
                .spacing(4),
        );
    let action: Element<_> = match &app.lastfm_auth {
        LastfmAuth::Idle => widget::button::suggested(fl!("lastfm-connect"))
            .on_press(Message::LastfmConnect)
            .into(),
        LastfmAuth::RequestingToken | LastfmAuth::Finishing => {
            widget::button::suggested(fl!("lastfm-connecting")).into()
        }
        LastfmAuth::Pending(_) => widget::column::with_capacity(2)
            .push(caption(fl!("lastfm-approve")))
            .push(widget::button::suggested(fl!("lastfm-finish")).on_press(Message::LastfmFinish))
            .spacing(6)
            .into(),
    };
    section.add(action).into()
}

fn discord(app: &App) -> Element<'_, Message> {
    let mut section = settings::section()
        .title("Discord")
        .add(settings::item(
            fl!("discord-enable"),
            widget::toggler(app.config.discord_enabled).on_toggle(Message::DiscordEnabled),
        ))
        .add(
            widget::column::with_capacity(3)
                .push(widget::text::body(fl!("discord-client-id")))
                .push(
                    widget::text_input(
                        crate::config::DEFAULT_DISCORD_CLIENT_ID,
                        app.config.discord_client_id.as_str(),
                    )
                    .on_input(Message::DiscordClientId),
                )
                .push(caption(fl!("discord-client-id-help")))
                .spacing(4),
        )
        .add(settings::item(
            fl!("discord-covers"),
            widget::toggler(app.config.discord_online_covers).on_toggle(Message::DiscordOnlineCovers),
        ))
        .add(caption(fl!("discord-covers-help")));
    if !app.config.discord_client_id.is_empty() {
        section = section.add(
            widget::button::link(fl!("discord-create-app")).on_press(Message::LaunchUrl(
                "https://discord.com/developers/applications".into(),
            )),
        );
    }
    section.into()
}

fn appearance(app: &App) -> Element<'_, Message> {
    settings::section()
        .title(fl!("settings-appearance"))
        .add(settings::item(
            fl!("theme"),
            widget::dropdown(
                &app.theme_names,
                Some(app.config.app_theme.index()),
                Message::Theme,
            ),
        ))
        .into()
}
