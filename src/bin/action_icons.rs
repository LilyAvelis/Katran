//! Lucide SVG vocabulary matching Afio Actions.
use egui_lucide::Lucide;
pub fn icon(id: &str) -> Lucide {
    match id {
        id if id.starts_with("sort_") => Lucide::ArrowUpDown,
        "quicklook" | "preview" => Lucide::Eye,
        "density_more" => Lucide::ZoomOut,
        "density_less" => Lucide::ZoomIn,
        "all" => Lucide::LayoutGrid,
        "use_as_background" => Lucide::Image,
        "images" => Lucide::Image,
        "screenshots" => Lucide::Camera,
        "downloads" => Lucide::Download,
        "tab_toggle" | "tab_previous" | "tab_next" => Lucide::ArrowLeftRight,
        "cleanup" => Lucide::Trash2,
        id if id.starts_with("tab_position_") => Lucide::PanelsTopLeft,
        "copy" => Lucide::Copy,
        "add_to_katran" => Lucide::ClipboardPaste,
        "transfer_selection" => Lucide::ArrowLeftRight,
        "delete" => Lucide::Trash2,
        "open" => Lucide::ExternalLink,
        "reveal" => Lucide::FolderOpen,
        "open_with" => Lucide::AppWindow,
        "tabs_settings" => Lucide::PanelsTopLeft,
        "toggle_images" | "toggle_screenshots" | "toggle_downloads" | "toggle_text"
        | "toggle_links" | "toggle_tables" => Lucide::Eye,
        "tables" => Lucide::Table,
        "pods" | "toggle_pods" => Lucide::PanelsTopLeft,
        "paste_previous" => Lucide::ClipboardPaste,
        "quit" => Lucide::ExternalLink,
        "links" => Lucide::Link,
        "text" => Lucide::FileText,
        "copy_text" => Lucide::Copy,
        "save" => Lucide::Save,
        "gallery" => Lucide::Images,
        "list" => Lucide::List,
        "capture" => Lucide::ClipboardPaste,
        "refresh" => Lucide::RefreshCw,
        "settings" => Lucide::Settings,
        "commands" => Lucide::Search,
        _ => Lucide::CircleQuestionMark,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalogue_action_has_a_specific_icon() {
        for definition in katran::actions::DEFS {
            let id = definition.id;
            assert_ne!(
                icon(id),
                Lucide::CircleQuestionMark,
                "missing icon for {id}"
            );
        }
    }
}
