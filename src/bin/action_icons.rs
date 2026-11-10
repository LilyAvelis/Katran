//! Lucide SVG vocabulary matching Afio Actions.
use egui_lucide::Lucide;
pub fn icon(id: &str) -> Lucide {
    match id {
        "quicklook" | "preview" => Lucide::Eye,
        "density_more" => Lucide::ZoomOut,
        "density_less" => Lucide::ZoomIn,
        "all" => Lucide::LayoutGrid,
        "images" => Lucide::Image,
        "screenshots" => Lucide::Camera,
        "downloads" => Lucide::Download,
        "tab_toggle" => Lucide::ArrowLeftRight,
        "cleanup" => Lucide::Trash2,
        id if id.starts_with("tab_position_") => Lucide::PanelsTopLeft,
        "copy" => Lucide::Copy,
        "delete" => Lucide::Trash2,
        "open" => Lucide::Eye,
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
