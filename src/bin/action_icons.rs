//! Lucide SVG vocabulary matching Afio Actions.
use egui_lucide::Lucide;
pub fn icon(id: &str) -> Lucide {
    match id {
        "copy" => Lucide::Copy,
        "delete" => Lucide::Trash2,
        "open" => Lucide::Image,
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
