//! Read-only sort presentation. Domain keys and file metadata arrive through API.
use serde_json::Value;

pub fn sequence_hint(id: &str) -> Option<&'static str> {
    Some(match id {
        "sort_arrival" => ", t",
        "sort_arrival_reverse" => ", Shift+T",
        "sort_pinned" => ", y",
        "sort_extension" => ", e",
        "sort_extension_reverse" => ", Shift+E",
        "sort_modified" => ", m",
        "sort_modified_reverse" => ", Shift+M",
        "sort_shuffle" => ", r",
        "sort_natural" => ", n",
        "sort_natural_reverse" => ", Shift+N",
        "sort_size" => ", s",
        "sort_size_reverse" => ", Shift+S",
        "sort_name" => ", a",
        "sort_name_reverse" => ", Shift+A",
        "sort_origin" => ", o",
        "sort_origin_reverse" => ", Shift+O",
        _ => return None,
    })
}
pub fn confirmation(sort: &Value) -> String {
    let reverse = sort["reverse"] == true;
    let (name, direction) = match sort["mode"].as_str().unwrap_or("arrival") {
        "size" => (
            "размеру",
            if reverse {
                "большие → маленькие"
            } else {
                "маленькие → большие"
            },
        ),
        "modified" => (
            "дате изменения",
            if reverse {
                "новые → старые"
            } else {
                "старые → новые"
            },
        ),
        "arrival" => (
            "поступлению",
            if reverse {
                "старые → новые"
            } else {
                "новые → старые"
            },
        ),
        "pinned" => return "Сортировка: закреплённые в своём порядке, затем по поступлению".into(),
        "origin" => ("источнику", if reverse { "Я → А" } else { "А → Я" }),
        "extension" => ("расширению", if reverse { "Я → А" } else { "А → Я" }),
        "natural" => (
            "естественному имени",
            if reverse {
                "убывание"
            } else {
                "возрастание"
            },
        ),
        "name" => ("имени", if reverse { "Я → А" } else { "А → Я" }),
        "shuffle" => return "Сортировка: случайный порядок".into(),
        _ => return "Сортировка изменена".into(),
    };
    format!("Сортировка по {name} · {direction}")
}
pub fn criterion(event: &Value, sort: &Value) -> Option<String> {
    Some(match sort["mode"].as_str()? {
        "size" => event["size"]
            .as_u64()
            .map(size)
            .unwrap_or_else(|| "Размер неизвестен".into()),
        "modified" => event["modified_at_ms"]
            .as_i64()
            .map(date)
            .map(|date| format!("Изменён: {date}"))
            .unwrap_or_else(|| "Дата изменения неизвестна".into()),
        "extension" => {
            let extension = katran::sorting::extension_key(event["name"].as_str().unwrap_or(""));
            if extension.is_empty() {
                "Без расширения".into()
            } else {
                format!(".{extension}")
            }
        }
        "origin" => katran::sorting::origin_key(
            event["origin"]["source_domain"].as_str(),
            event["origin"]["source_app"].as_str(),
            event["source"].as_str().unwrap_or(""),
        )
        .map(|value| format!("Источник: {value}"))
        .unwrap_or_else(|| "Источник неизвестен".into()),
        _ => return None,
    })
}
fn size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["Б", "КиБ", "МиБ", "ГиБ", "ТиБ"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024. && unit < 4 {
        value /= 1024.;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} Б")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
// Gregorian civil date from epoch days. UTC is explicit; no filesystem/local
// timezone query is performed while painting a card.
fn date(ms: i64) -> String {
    let seconds = ms.div_euclid(1000);
    let z = seconds.div_euclid(86400) + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let time = seconds.rem_euclid(86400);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        time / 3600,
        (time % 3600) / 60
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn presentation_respects_direction_and_never_fabricates_metadata() {
        assert!(
            confirmation(&json!({"mode":"size","reverse":true})).contains("большие → маленькие")
        );
        assert!(
            confirmation(&json!({"mode":"modified","reverse":false})).contains("старые → новые")
        );
        assert_eq!(
            criterion(&json!({"detected_at_ms":1}), &json!({"mode":"modified"})),
            Some("Дата изменения неизвестна".into())
        );
        assert_eq!(
            criterion(&json!({"size":1536}), &json!({"mode":"size"})),
            Some("1.5 КиБ".into())
        );
        assert_eq!(date(0), "1970-01-01 00:00 UTC");
        assert_eq!(date(-60000), "1969-12-31 23:59 UTC");
        assert_eq!(date(951782400000), "2000-02-29 00:00 UTC");
    }
    #[test]
    fn sequence_hints_are_not_bindings() {
        for def in katran::actions::DEFS
            .iter()
            .filter(|d| d.id.starts_with("sort_"))
        {
            let hint = sequence_hint(def.id).unwrap();
            assert!(hint.starts_with(", "));
            assert!(def.default_binding.is_empty());
        }
        assert_eq!(sequence_hint("sort_origin_reverse"), Some(", Shift+O"));
    }
}
