// SPDX-License-Identifier: GPL-3.0-only

//! Cosmic Advanced Player — a native music player for Linux, built with libcosmic.

mod app;
mod audio;
mod config;
mod discord;
mod i18n;
mod lastfm;
mod library;
mod lyrics;
mod mpris;
mod queue;
mod ui;

fn main() -> cosmic::iced::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("cosmic-advanced-player {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "Usage: cosmic-advanced-player [FILE|FOLDER]...\n\nPlays the given files or folders (recursively)."
        );
        return Ok(());
    }

    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();
    i18n::init(&requested_languages);

    // Files / folders / file:// URIs to play, made absolute for the running instance.
    let paths = args
        .into_iter()
        .filter(|a| !a.starts_with('-'))
        .map(|a| {
            let p = a
                .strip_prefix("file://")
                .map(|s| urlencoding::decode(s).map_or_else(|_| s.to_string(), |d| d.into_owned()))
                .unwrap_or(a);
            std::fs::canonicalize(&p).map_or(p, |c| c.to_string_lossy().into_owned())
        })
        .collect();

    let settings = cosmic::app::Settings::default()
        .size(cosmic::iced::Size::new(1280.0, 820.0))
        .size_limits(cosmic::iced::Limits::NONE.min_width(860.0).min_height(560.0));

    cosmic::app::run_single_instance::<app::App>(settings, app::Flags { paths })
}
