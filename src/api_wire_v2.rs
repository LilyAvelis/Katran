//! Optional lossless JSON-line protocol for JavaScript clients.
use super::Request;
use serde_json::Value;
const MAX_SAFE: i64 = 9_007_199_254_740_991;

fn id(value: &mut Value) -> Result<(), String> {
    match value {
        Value::Null => Ok(()),
        Value::Number(n) => match n.as_i64() {
            Some(n) if (-MAX_SAFE..=MAX_SAFE).contains(&n) => Ok(()),
            _ => Err("ID numbers must be safe integers; use a decimal string".into()),
        },
        Value::String(s) => {
            let n: i64 = s.parse().map_err(|_| "Invalid decimal ID")?;
            if n.to_string() != *s {
                return Err("ID must be a canonical decimal string".into());
            }
            *value = Value::from(n);
            Ok(())
        }
        _ => Err("Invalid ID".into()),
    }
}
fn unsigned_id(value: &mut Value) -> Result<(), String> {
    match value {
        Value::Number(n) => match n.as_u64() {
            Some(n) if n <= MAX_SAFE as u64 => Ok(()),
            _ => {
                Err("Unsigned identity numbers must be safe integers; use a decimal string".into())
            }
        },
        Value::String(s) => {
            let n: u64 = s.parse().map_err(|_| "Invalid decimal unsigned identity")?;
            if n.to_string() != *s {
                return Err("Unsigned identity must be a canonical decimal string".into());
            }
            *value = Value::from(n);
            Ok(())
        }
        _ => Err("Invalid unsigned identity".into()),
    }
}
fn field(object: &mut serde_json::Map<String, Value>, name: &str) -> Result<(), String> {
    if let Some(value) = object.get_mut(name) {
        id(value)?;
    }
    Ok(())
}
fn command(value: &mut Value) -> Result<(), String> {
    let object = value.as_object_mut().ok_or("Expected command object")?;
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or("Expected command type")?
        .to_owned();
    match kind.as_str() {
        "capture_media_frame" | "toggle_media_pause" => {
            if let Some(value) = object.get_mut("session")
                && !value.is_null()
            {
                unsigned_id(value)?;
            }
        }
        "control_media" | "media_frame" => {
            if let Some(value) = object.get_mut("session") {
                unsigned_id(value)?;
            }
            if let Some(value) = object.get_mut("after_serial") {
                unsigned_id(value)?;
            }
            if let Some(value) = object
                .get_mut("action")
                .and_then(Value::as_object_mut)
                .and_then(|action| action.get_mut("position_ms"))
            {
                unsigned_id(value)?;
            }
        }
        "poll_job" | "cancel_job" => {
            if let Some(value) = object.get_mut("job_id") {
                unsigned_id(value)?;
            }
        }
        "begin_paste_collection_page" => {
            field(object, "id")?;
            if let Some(value) = object.get_mut("owner_window") {
                unsigned_id(value)?;
            }
        }
        "toggle_resident_overlay"
        | "summon_captured_overlay"
        | "summon_tab_overlay"
        | "summon_overlay"
        | "begin_paste_previous"
        | "complete_paste_previous"
        | "run_prepared_drag" => {
            if let Some(value) = object.get_mut("owner_window") {
                unsigned_id(value)?;
            }
            if let Some(previous) = object
                .get_mut("capture")
                .and_then(Value::as_object_mut)
                .and_then(|capture| capture.get_mut("previous"))
                .and_then(Value::as_object_mut)
            {
                for key in ["hwnd", "process_started"] {
                    if let Some(value) = previous.get_mut(key) {
                        unsigned_id(value)?;
                    }
                }
            }
        }
        "describe_pod"
        | "open_media"
        | "get_pinned_properties"
        | "set_pinned_properties"
        | "set_pinned_tags"
        | "get_tag_slot"
        | "get_link_visual"
        | "refresh_link_visual"
        | "get_table"
        | "get_link"
        | "get_text"
        | "get_document_text"
        | "list_open_apps"
        | "open_file"
        | "preview_external"
        | "translation_collection_page"
        | "copy_collection_page"
        | "open_collection_link"
        | "rename_collection_link"
        | "open_bs_entry"
        | "move_pinned"
        | "select"
        | "preview_image"
        | "preview_file"
        | "get" => field(object, "id")?,
        "list_link_ids" | "browse_shelf" | "search_page" => field(object, "after_id")?,
        "add_folder_files" | "toggle_pins" | "assign_tag_slot" | "get_tag_slots"
        | "delete_selection" | "copy_events" | "delete_events" | "set_visible" | "import" => {
            if let Some(Value::Array(items)) = object.get_mut("ids") {
                for item in items {
                    id(item)?;
                }
            }
        }
        "stage_transfer"
        | "copy_tables"
        | "copy_links"
        | "merge_text"
        | "copy_text"
        | "copy_smart"
        | "drag_events_checked"
        | "copy_events_checked" => {
            if let Some(Value::Array(items)) = object.get_mut("items") {
                for item in items {
                    if let Some(item) = item.as_object_mut() {
                        field(item, "id")?;
                    }
                }
            }
        }
        "commit_screenshot_matches" => {
            if let Some(Value::Array(items)) = object.get_mut("pairs") {
                for item in items {
                    if let Some(item) = item.as_object_mut() {
                        field(item, "clipboard_id")?;
                        field(item, "source_id")?;
                    }
                }
            }
        }
        "begin_job" | "ingest_with_origin" => {
            if let Some(value) = object.get_mut("command") {
                command(value)?;
            }
        }
        _ => (),
    }
    Ok(())
}

/// Decode a v2 request into the unchanged in-process v1 API.
pub fn decode(mut value: Value) -> Result<Request, String> {
    if value.get("api_version").and_then(Value::as_u64) != Some(2) {
        return Err("Expected API version 2".into());
    }
    command(value.get_mut("command").ok_or("Missing command")?)?;
    value["api_version"] = Value::from(super::API_VERSION);
    serde_json::from_value(value).map_err(|e| e.to_string())
}

/// Encode unsafe JSON integers as decimal strings, including nested response fields.
pub fn encode(value: &mut Value) {
    match value {
        Value::Number(n) => {
            let unsafe_number = n
                .as_i64()
                .is_some_and(|v| !(-MAX_SAFE..=MAX_SAFE).contains(&v))
                || n.as_u64().is_some_and(|v| v > MAX_SAFE as u64);
            if unsafe_number {
                *value = Value::String(n.to_string());
            }
        }
        Value::Array(items) => items.iter_mut().for_each(encode),
        Value::Object(fields) => fields.values_mut().for_each(encode),
        _ => (),
    }
}
