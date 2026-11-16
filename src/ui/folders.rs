// SPDX-License-Identifier: GPL-3.0-only

//! Folder browser over the indexed library.

use std::path::{Path, PathBuf};

use cosmic::iced::{Alignment, Length};
use cosmic::prelude::*;
use cosmic::widget;

use super::{ListOptions, dim_text, empty_state, page_header, page_scroll, play_buttons, track_list};
use crate::app::{App, Message, Page};
use crate::fl;

fn folder_row<'a>(path: PathBuf, label: String, count: usize) -> Element<'a, Message> {
    widget::button::custom(
        widget::row::with_capacity(3)
            .push(widget::icon::from_name("folder-music-symbolic").size(24).icon())
            .push(widget::text::body(label).width(Length::Fill))
            .push(
                widget::text::caption(fl!("track-count", count = count))
                    .class(cosmic::theme::Text::Custom(dim_text)),
            )
            .spacing(14)
            .align_y(Alignment::Center),
    )
    .class(cosmic::theme::Button::ListItem([8.0; 4]))
    .padding([10, 12])
    .width(Length::Fill)
    .on_press(Message::Navigate(Page::Folder(path)))
    .into()
}

pub fn view<'a>(app: &'a App, dir: Option<&PathBuf>) -> Element<'a, Message> {
    let Some(dir) = dir else { return roots(app) };
    let (folders, files) = app.library.folder_children(dir);
    let all = app.library.tracks_under(dir);

    let mut col = widget::column::with_capacity(4).padding([0, 0, 24, 0]);
    col = col.push(page_header(
        dir.file_name()
            .map_or_else(|| dir.display().to_string(), |n| n.to_string_lossy().into_owned()),
        Some(dir.display().to_string()),
        play_buttons(&all),
        true,
    ));
    if !folders.is_empty() {
        let mut list = widget::column::with_capacity(folders.len())
            .padding([0, 16])
            .spacing(2);
        for (path, count) in folders {
            let label = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            list = list.push(folder_row(path, label, count));
        }
        col = col.push(list);
    }
    if !files.is_empty() {
        let top = 100.0;
        col = col.push(track_list(
            app,
            &files,
            &ListOptions {
                top,
                ..ListOptions::default()
            },
        ));
    }
    page_scroll(col)
}

fn roots(app: &App) -> Element<'_, Message> {
    if app.config.library_folders.is_empty() {
        return empty_state(
            "folder-music-symbolic",
            fl!("no-library-folders"),
            fl!("no-library-folders-body"),
        );
    }
    let mut list = widget::column::with_capacity(app.config.library_folders.len())
        .padding([0, 16])
        .spacing(2);
    for root in &app.config.library_folders {
        let path = std::fs::canonicalize(root).unwrap_or_else(|_| PathBuf::from(root));
        let count = app
            .library
            .tracks()
            .iter()
            .filter(|t| t.path.starts_with(&path))
            .count();
        list = list.push(folder_row(path, display_root(Path::new(root)), count));
    }
    let header = page_header(
        fl!("page-folders"),
        None,
        vec![
            widget::button::standard(fl!("add-library-folder"))
                .leading_icon(widget::icon::from_name("folder-new-symbolic"))
                .on_press(Message::AddLibraryFolder)
                .into(),
            widget::button::standard(fl!("open-folder"))
                .leading_icon(widget::icon::from_name("document-open-symbolic"))
                .on_press(Message::OpenFolderDialog)
                .into(),
        ],
        false,
    );
    page_scroll(widget::column::with_capacity(2).push(header).push(list))
}

/// `~/Music` instead of `/home/user/Music`.
fn display_root(p: &Path) -> String {
    if let Some(home) = dirs::home_dir()
        && let Ok(rest) = p.strip_prefix(&home)
    {
        return format!("~/{}", rest.display());
    }
    p.display().to_string()
}
