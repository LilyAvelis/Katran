//! Feature switches and navigation are domain policy, shared by every client.
use crate::{ApiError, Command, Result, Service, interaction::ShelfTab};
use serde_json::{Value, json};

pub fn label(tab: ShelfTab) -> &'static str {
    match tab {
        ShelfTab::All => "Folder",
        ShelfTab::Images => "Images",
        ShelfTab::Screenshots => "Screenshots",
        ShelfTab::Downloads => "Downloads",
        ShelfTab::Text => "Text",
        ShelfTab::Links => "Links",
        ShelfTab::Tables => "Tables",
        ShelfTab::Pods => "Pods",
        ShelfTab::Files => "Files",
        ShelfTab::Desktop => "Desktop",
        ShelfTab::Translate => "Translate",
        ShelfTab::Pages => "Pages",
    }
}

#[derive(Clone, Copy, Default)]
pub(crate) struct BackgroundActions {
    image: bool,
    video: bool,
    image_batch: bool,
    pack_images: bool,
    pack_links: bool,
    pack_text: bool,
    unpack_bull: bool,
    merge_bull: bool,
    pack_bs: bool,
    unpack_bs: bool,
    merge_bs: bool,
}

// The Actions catalogue is presentation metadata. Resolve at most one event;
// destructive or external effects still validate bytes at their action boundary.
fn background_actions(
    selected: &[i64],
    cursor: Option<i64>,
    resolve: impl FnOnce(i64) -> Result<crate::Event>,
) -> BackgroundActions {
    let id = match selected {
        [] => cursor,
        [id] => Some(*id),
        _ => None,
    };
    let Some(event) = id.and_then(|id| resolve(id).ok()) else {
        return BackgroundActions::default();
    };
    if crate::collection::packable_link(&event) {
        return BackgroundActions {
            pack_links: true,
            ..Default::default()
        };
    }
    if crate::collection::packable_text(&event) {
        return BackgroundActions {
            pack_text: true,
            ..Default::default()
        };
    }
    if matches!(
        event.kind,
        crate::EventKind::TextFragment
            | crate::EventKind::LinkFragment
            | crate::EventKind::TableFragment
    ) {
        return BackgroundActions::default();
    }
    let image = event.kind == crate::EventKind::ManagedImage
        || crate::file_types::is_native_image(&event.path);
    let extension = event
        .path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let video = (event.revision.starts_with("ref:")
        || event.revision.starts_with("blob:")
        || (event.id < 0 && event.revision.starts_with("file:")))
        && ["mp4", "m4v", "mov", "mkv", "webm"]
            .iter()
            .any(|candidate| extension.eq_ignore_ascii_case(candidate));
    BackgroundActions {
        image,
        video,
        image_batch: image || crate::image_translation_batch::is_bull(&event),
        pack_images: image,
        unpack_bull: crate::collection::is_managed(&event),
        merge_bull: false,
        pack_links: false,
        pack_text: false,
        pack_bs: crate::bs_collection::is_packable_file(&event),
        unpack_bs: crate::bs_collection::is_managed(&event),
        merge_bs: false,
    }
}

impl Service {
    fn current_archive_selection(&self) -> bool {
        let ids = if self.interaction.selected.is_empty() {
            self.interaction.cursor.as_slice()
        } else {
            self.interaction.selected.as_slice()
        };
        (1..=baloh_collection::bs::MAX_FILES).contains(&ids.len())
            && ids.iter().all(|&id| {
                self.get(id).is_ok_and(|event| {
                    crate::collection_kind::external(&event)
                        || crate::collection::is_managed(&event)
                })
            })
    }

    pub(crate) fn navigate_tab(&mut self, delta: i8) -> Result<Value> {
        if !matches!(delta, -1 | 1) {
            return Err(ApiError::new(
                "invalid_tab_direction",
                "Tab direction must be -1 or 1",
            ));
        }
        let visible = self.settings.visible_tabs();
        if self.interaction.tab == ShelfTab::All || visible.is_empty() {
            return Ok(json!(self.interaction));
        }
        if let Some(index) = visible.iter().position(|tab| *tab == self.interaction.tab) {
            let target =
                (index as isize + isize::from(delta)).clamp(0, visible.len() as isize - 1) as usize;
            self.interaction.set_tab(visible[target]);
        }
        Ok(json!(self.interaction))
    }

    pub(crate) fn background_action_availability(&self) -> BackgroundActions {
        let selected = &self.interaction.selected;
        if (2..=crate::image_translation_batch::MAX_PAGES).contains(&selected.len()) {
            let events = selected
                .iter()
                .map(|id| self.get(*id))
                .collect::<Result<Vec<_>>>();
            let images = events
                .as_ref()
                .is_ok_and(|events| events.iter().all(crate::image_collection::is_image));
            return BackgroundActions {
                image_batch: images,
                pack_images: images,
                merge_bull: events
                    .as_ref()
                    .is_ok_and(|events| events.iter().all(crate::collection::is_managed)),
                pack_links: events
                    .as_ref()
                    .is_ok_and(|events| events.iter().all(crate::collection::packable_link)),
                pack_text: events
                    .as_ref()
                    .is_ok_and(|events| events.iter().all(crate::collection::packable_text)),
                pack_bs: events
                    .as_ref()
                    .is_ok_and(|events| events.iter().all(crate::bs_collection::is_packable_file)),
                merge_bs: events
                    .as_ref()
                    .is_ok_and(|events| events.iter().all(crate::bs_collection::is_managed)),
                ..Default::default()
            };
        }
        background_actions(&self.interaction.selected, self.interaction.cursor, |id| {
            self.get(id)
        })
    }

    fn text_card_for_obsidian(&self) -> bool {
        self.interaction.tab == ShelfTab::Text
            && self
                .interaction
                .cursor
                .or_else(|| self.interaction.selected.first().copied())
                .is_some_and(|id| {
                    self.get(id)
                        .is_ok_and(|event| event.kind == crate::EventKind::TextFragment)
                })
    }

    pub(crate) fn action_available(&self, id: &str, background: BackgroundActions) -> bool {
        match id {
            "rename" if self.interaction.tab == ShelfTab::Desktop => false,
            "rename" => self
                .interaction
                .selected
                .first()
                .copied()
                .or(self.interaction.cursor)
                .and_then(|id| self.get(id).ok().map(|event| (id, event)))
                .is_some_and(|(id, event)| {
                    crate::collection_rename::can_rename(&event)
                        || id > 0
                            && crate::pinned::properties(&self.db, id)
                                .is_ok_and(|properties| properties.pinned)
                }),
            "toggle_pin" => {
                self.interaction.tab != ShelfTab::All
                    && !self.interaction.emoji_catalogue
                    && (!self.interaction.selected.is_empty() || self.interaction.cursor.is_some())
            }
            "tag_menu" => {
                !self.interaction.emoji_catalogue
                    && crate::tag_service::scope(self.interaction.tab).is_ok()
            }
            "apply_text_draft" => self.text_card_for_obsidian(),
            "show_text_vault" => self.interaction.tab == ShelfTab::Text,
            "move_to_pages" => self.can_move_to_pages(),
            "open_pages_vault" => self.interaction.tab == ShelfTab::Pages,
            "video_pause" | "capture_video_frame" | "extract_video_audio" => background.video,
            "folder_favorites" | "folder_history" | "folder_standard" => {
                self.interaction.tab == ShelfTab::All
            }
            "upscale_remacri_x2"
            | "upscale_remacri_x4"
            | "upscale_realesrgan_x2"
            | "upscale_realesrgan_x4" => background.image_batch,
            "translate_image_ja"
            | "translate_image_zh"
            | "translate_image_zh_hant"
            | "translate_image_ko" => background.image_batch,
            "search_image_google_lens" => background.image,
            "create_image_collection" => {
                let supported = match self.interaction.tab {
                    ShelfTab::Images | ShelfTab::Translate => background.pack_images,
                    ShelfTab::Links => background.pack_links,
                    ShelfTab::Text => background.pack_text,
                    ShelfTab::Files => {
                        return background.pack_bs
                            || background.unpack_bs
                            || background.merge_bs
                            || background.unpack_bull
                            || background.merge_bull;
                    }
                    ShelfTab::Pages => return self.pages_collection_available(),
                    ShelfTab::All => return background.pack_bs || self.current_archive_selection(),
                    _ => return false,
                };
                supported || background.unpack_bull || background.merge_bull
            }
            "use_as_background" => background.image,
            "use_as_desktop_wallpaper" => background.image,
            "use_as_video_background" => background.video,
            "use_as_video_desktop_wallpaper" => background.video,
            "use_as_video_desktop_wallpaper_silent" => background.video,
            "add_to_katran" => self.interaction.tab == ShelfTab::All,
            "images" => self.settings.tab_enabled(ShelfTab::Images),
            "screenshots" => self.settings.tab_enabled(ShelfTab::Screenshots),
            "downloads" => self.settings.tab_enabled(ShelfTab::Downloads),
            "text" => self.settings.tab_enabled(ShelfTab::Text),
            "links" => self.settings.tab_enabled(ShelfTab::Links),
            "tables" => self.settings.tab_enabled(ShelfTab::Tables),
            "pods" => self.settings.tab_enabled(ShelfTab::Pods),
            "files" => self.settings.tab_enabled(ShelfTab::Files),
            "desktop" => self.settings.tab_enabled(ShelfTab::Desktop),
            "translate" => self.settings.tab_enabled(ShelfTab::Translate),
            "pages" => self.settings.tab_enabled(ShelfTab::Pages),
            _ => true,
        }
    }

    pub(crate) fn action_label(
        &self,
        id: &str,
        fallback: &str,
        background: BackgroundActions,
    ) -> String {
        let english = self.settings.language == crate::settings::Language::English;
        if id == "open"
            && (self.text_card_for_obsidian()
                || (self.interaction.tab == ShelfTab::Pages
                    && self.interaction.cursor.is_some_and(|id| {
                        self.get(id)
                            .is_ok_and(|event| crate::file_types::is_markdown(&event.path))
                    })))
        {
            return if english {
                "Edit in Obsidian"
            } else {
                "Редактировать в Obsidian"
            }
            .into();
        }
        if id == "create_image_collection"
            && matches!(
                self.interaction.tab,
                ShelfTab::Files | ShelfTab::All | ShelfTab::Pages
            )
        {
            return match (background.unpack_bs, background.merge_bs, english) {
                (true, _, true) => "Unpack collection",
                (true, _, false) => "Разобрать коллекцию",
                (_, true, true) => "Merge collections",
                (_, true, false) => "Объединить коллекции",
                (_, _, true) => "Pack / unpack collection",
                (_, _, false) => "Собрать / разобрать коллекцию",
            }
            .into();
        }
        if id == "create_image_collection" {
            if background.merge_bull {
                return if english {
                    "Merge BULL collections"
                } else {
                    "Объединить BULL"
                }
                .into();
            }
            if background.pack_links {
                return if english {
                    "Pack links into BULL"
                } else {
                    "Собрать ссылки в BULL"
                }
                .into();
            }
            if background.pack_text {
                return if english {
                    "Pack texts into BULL"
                } else {
                    "Собрать тексты в BULL"
                }
                .into();
            }
            return match (background.unpack_bull, english) {
                (true, true) => "Unpack BULL",
                (true, false) => "Разобрать BULL",
                (false, true) => "Pack images into BULL",
                (false, false) => "Собрать изображения в BULL",
            }
            .into();
        }
        let tab = match id {
            "toggle_images" => Some(ShelfTab::Images),
            "toggle_screenshots" => Some(ShelfTab::Screenshots),
            "toggle_downloads" => Some(ShelfTab::Downloads),
            "toggle_text" => Some(ShelfTab::Text),
            "toggle_links" => Some(ShelfTab::Links),
            "toggle_tables" => Some(ShelfTab::Tables),
            "toggle_pods" => Some(ShelfTab::Pods),
            "toggle_files" => Some(ShelfTab::Files),
            "toggle_desktop" => Some(ShelfTab::Desktop),
            _ => None,
        };
        if let Some(tab) = tab {
            format!(
                "{} {}",
                if self.settings.tab_enabled(tab) {
                    if english {
                        "Disable"
                    } else {
                        "Отключить"
                    }
                } else {
                    if english {
                        "Enable"
                    } else {
                        "Включить"
                    }
                },
                label(tab)
            )
        } else if id == "capture" {
            if english && self.capture_enabled {
                "Pause clipboard capture"
            } else if english {
                "Enable clipboard capture"
            } else if self.capture_enabled {
                "Приостановить захват буфера"
            } else {
                "Включить захват буфера"
            }
            .into()
        } else if id == "tab_toggle" {
            if self.interaction.tab == ShelfTab::All {
                if english {
                    format!("Return to {}", label(self.interaction.last_tab))
                } else {
                    format!("Вернуться в {}", label(self.interaction.last_tab))
                }
            } else {
                if english {
                    "Go to Folder"
                } else {
                    "Перейти в Folder"
                }
                .into()
            }
        } else if id == "toggle_pinned_view" {
            if self.interaction.pinned_only {
                if english {
                    "Show all in category"
                } else {
                    "Показать все в категории"
                }
                .into()
            } else {
                if english {
                    "Show pinned items"
                } else {
                    "Показать закреплённые"
                }
                .into()
            }
        } else if id == "transfer_copy" {
            if self.interaction.tab == ShelfTab::All {
                if english {
                    "Copy from Folder to Katran"
                } else {
                    "Копировать из Folder в Катран"
                }
                .into()
            } else {
                if english {
                    "Copy to Folder"
                } else {
                    "Копировать в Folder"
                }
                .into()
            }
        } else if id == "transfer_selection" {
            if self.interaction.tab == ShelfTab::All {
                if english {
                    "Move from Folder to Katran"
                } else {
                    "Перенести из Folder в Катран"
                }
                .into()
            } else {
                if english {
                    "Move to Folder"
                } else {
                    "Перенести в Folder"
                }
                .into()
            }
        } else {
            fallback.into()
        }
    }

    pub(crate) fn tabs(&self) -> Value {
        let visible = self.settings.visible_tabs();
        let tabs: Vec<_> = std::iter::once(ShelfTab::All)
            .chain(self.settings.tab_order.iter().copied())
            .map(|tab| json!({
                "tab":tab,"label":label(tab),"enabled":self.settings.tab_enabled(tab),
                "can_toggle":tab!=ShelfTab::All,
                "position":if tab==ShelfTab::All {Some(0)} else {visible.iter().position(|t|*t==tab).map(|i|i+1)}
            }))
            .collect();
        json!({"tabs":tabs,"visible_tabs":visible})
    }

    pub(crate) fn set_tab_enabled(&mut self, tab: ShelfTab, enabled: bool) -> Result<Value> {
        if tab == ShelfTab::All {
            return Err(ApiError::new("fixed_tab", "Folder is always enabled"));
        }
        if enabled
            && !self.settings.tab_enabled(tab)
            && self.settings.visible_tabs().len() >= crate::tab_layout::MAX_NUMBERED_TABS
        {
            return Err(ApiError::new(
                "tab_limit",
                "Disable a numbered tab before enabling another. Katran supports positions 1–9.",
            ));
        }
        let mut settings = self.settings.clone();
        settings.hidden_tabs.retain(|t| *t != tab);
        if !enabled {
            settings.hidden_tabs.push(tab);
        }
        if tab == ShelfTab::Downloads {
            settings.downloads_enabled = enabled;
        }
        if tab == ShelfTab::Text {
            settings.text_enabled = enabled;
        }
        if tab == ShelfTab::Tables {
            settings.tables_enabled = enabled;
        }
        if tab == ShelfTab::Links {
            settings.links_enabled = enabled;
        }
        if tab == ShelfTab::Pods {
            settings.pods_enabled = enabled;
        }
        self.dispatch(Command::SetSettings { settings })?;
        Ok(self.tabs())
    }

    pub(crate) fn select_tab(&mut self, tab: ShelfTab) -> Result<Value> {
        if !self.settings.tab_enabled(tab) {
            return Err(ApiError::new(
                "tab_disabled",
                "Enable this tab in Settings → Tabs first",
            ));
        }
        self.interaction.set_tab(tab);
        Ok(json!(self.interaction))
    }

    pub(crate) fn normalize_tabs(&mut self) {
        if !self.settings.tab_enabled(self.interaction.tab) {
            self.interaction.set_tab(ShelfTab::All);
            self.interaction.visible(Vec::new());
        }
        if self.interaction.last_tab == ShelfTab::All
            || !self.settings.tab_enabled(self.interaction.last_tab)
        {
            self.interaction.last_tab = self
                .settings
                .visible_tabs()
                .first()
                .copied()
                .unwrap_or(ShelfTab::All);
        }
    }

    pub(crate) fn check_ingest_enabled(&self, name: &str) -> Result<()> {
        if !self.settings.tab_enabled(ShelfTab::Images)
            && crate::file_types::is_native_image(std::path::Path::new(name))
        {
            return Err(ApiError::new(
                "tab_disabled",
                "Images is disabled; image ingestion is stopped",
            ));
        }
        Ok(())
    }
    pub(crate) fn check_links_enabled(&self) -> Result<()> {
        if !self.settings.tab_enabled(ShelfTab::Links) {
            return Err(ApiError::new(
                "tab_disabled",
                "Links is disabled; enable its eye in Settings → Tabs",
            ));
        }
        Ok(())
    }
    pub(crate) fn check_text_enabled(&self) -> Result<()> {
        if !self.settings.tab_enabled(ShelfTab::Text) {
            return Err(ApiError::new(
                "tab_disabled",
                "Text is disabled; enable its eye in Settings → Tabs",
            ));
        }
        Ok(())
    }
    pub(crate) fn check_pods_enabled(&self) -> Result<()> {
        if !self.settings.tab_enabled(ShelfTab::Pods) {
            return Err(ApiError::new(
                "tab_disabled",
                "Pods is disabled; enable its eye in Settings → Tabs",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod background_action_tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn catalogue_classification_resolves_only_one_selected_event() {
        let calls = Cell::new(0);
        let missing = |_: i64| {
            calls.set(calls.get() + 1);
            Err(ApiError::new("synthetic", "missing"))
        };
        let none = background_actions(&[], None, missing);
        assert!(!none.image && !none.video);
        assert_eq!(calls.get(), 0);
        let multiple = background_actions(&[1, 2], Some(1), missing);
        assert!(!multiple.image && !multiple.video);
        assert_eq!(calls.get(), 0);
        let _ = background_actions(&[1], Some(2), missing);
        assert_eq!(calls.get(), 1);
        let _ = background_actions(&[], Some(2), missing);
        assert_eq!(calls.get(), 2);
    }
}
