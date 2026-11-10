use katran::table_fragments::{self as table, Table};

#[test]
fn tsv_preserves_quotes_newlines_unicode_and_empty_cells() {
    let source = "Имя\tЗначение\tПусто\r\n\"Лилия\t🐄\"\t\"строка 1\nстрока \"\"2\"\"\"\t\r\n";
    let parsed = table::parse_tsv(source, false).unwrap();
    assert_eq!(
        parsed.rows,
        vec![
            vec!["Имя", "Значение", "Пусто"],
            vec!["Лилия\t🐄", "строка 1\nстрока \"2\"", ""]
        ]
    );
    assert_eq!(
        table::parse_tsv(&table::to_tsv(&parsed).unwrap(), false).unwrap(),
        parsed
    );
    assert!(table::parse_tsv("строка\nдругая", false).is_err());
    assert_eq!(
        table::parse_tsv("строка\nдругая", true).unwrap().rows.len(),
        2
    );
    assert!(table::parse_tsv("a\tb\nc", false).is_err());
    assert!(table::parse_tsv("\"незакрытый\ta", false).is_err());
    assert!(table::parse_tsv("\"закрыт\"лишнее\tb", false).is_err());
}
#[test]
fn limits_and_dense_rectangular_structure_are_enforced() {
    assert!(Table { rows: vec![] }.validate().is_err());
    assert!(
        Table {
            rows: vec![vec!["a".into()], vec!["b".into(), "c".into()]]
        }
        .validate()
        .is_err()
    );
    assert!(
        Table {
            rows: vec![vec!["x\0y".into()]]
        }
        .validate()
        .is_err()
    );
    assert!(
        Table {
            rows: vec![vec!["x".repeat(table::MAX_CONTENT_BYTES + 1)]]
        }
        .validate()
        .is_err()
    );
    assert!(
        Table {
            rows: vec![vec![String::new(); table::MAX_COLUMNS + 1]]
        }
        .validate()
        .is_err()
    );
    assert!(
        Table {
            rows: vec![vec![String::new()]; table::MAX_ROWS + 1]
        }
        .validate()
        .is_err()
    );
    assert!(
        Table {
            rows: vec![vec![String::new(); 256]; 391]
        }
        .validate()
        .is_err()
    );
}
#[test]
fn csv_html_and_cf_html_preserve_content_and_utf8_offsets() {
    let table = Table {
        rows: vec![
            vec!["🐄,а".into(), "\"<b>\"\n&".into()],
            vec!["".into(), "\rстрока".into()],
        ],
    };
    let csv = table::to_csv_bom(&table).unwrap();
    assert!(csv.starts_with(&[0xef, 0xbb, 0xbf]));
    assert_eq!(
        std::str::from_utf8(&csv[3..]).unwrap(),
        "\"🐄,а\",\"\"\"<b>\"\"\n&\"\r\n,\"\rстрока\"\r\n"
    );
    let html = table::to_html(&table).unwrap();
    assert!(html.contains("&quot;&lt;b&gt;&quot;<br>&amp;"));
    assert!(html.contains("&#13;строка"));
    let encoded = table::encode_cf_html(&html).unwrap();
    assert_eq!(table::extract_cf_html(&encoded).unwrap(), html);
    let mut bad = encoded.clone();
    let marker = b"StartFragment:";
    let pos = bad.windows(marker.len()).position(|w| w == marker).unwrap() + marker.len();
    bad[pos..pos + 10].copy_from_slice(b"9999999999");
    assert!(table::extract_cf_html(&bad).is_err());
}

#[test]
fn html_table_decodes_entities_spans_and_discards_active_content() {
    let html = r#"<table><thead><tr><th rowspan="2">🐄 &amp; чай</th><th colspan="2">Итог</th></tr></thead><tbody><tr><td><p>первая</p><div>вторая<br>третья</div><script>evil()</script><style>hidden</style><img src="https://invalid.example/a"></td><td></td></tr></tbody></table>"#;
    let result = table::parse_html(html).unwrap();
    assert_eq!(
        result.rows,
        vec![
            vec!["🐄 & чай", "Итог", ""],
            vec!["", "первая\nвторая\nтретья", ""]
        ]
    );
    let serialized = table::to_html(&result).unwrap();
    assert_eq!(table::parse_html(&serialized).unwrap(), result);
    assert!(!serialized.contains("evil"));
    assert!(!serialized.contains("invalid.example"));
}
#[test]
fn html_rejects_ambiguous_or_unbounded_shapes() {
    assert!(!table::has_html_table(
        "<script>const example = '<table>'</script><!-- <table> -->"
    ));
    assert!(table::has_html_table("<TABLE><tr><td>x</td></tr></TABLE>"));
    assert!(table::parse_html("<table><tr><td>\0</td></tr></table>").is_err());
    for html in [
        "<p>not a table</p>",
        "<table><tr><td>a</td></tr></table><table><tr><td>b</td></tr></table>",
        "<table><tr><td><table><tr><td>nested</td></tr></table></td></tr></table>",
        "<table><tr><td rowspan='2'>a</td></tr></table>",
        "<table><tr><td colspan='257'>a</td></tr></table>",
    ] {
        assert!(table::parse_html(html).is_err(), "{html}");
    }
}
#[test]
fn csv_roundtrip_preserves_bom_unicode_and_spreadsheet_shaped_content() {
    let source = "\u{feff}имя,заметка,пусто\r\n🐄,\"строка 1\r\nстрока \"\"2\"\"\",\r\n";
    let parsed = table::parse_csv(source).unwrap();
    assert_eq!(parsed.rows[1], vec!["🐄", "строка 1\r\nстрока \"2\"", ""]);
    assert_eq!(
        table::parse_csv(std::str::from_utf8(&table::to_csv_bom(&parsed).unwrap()).unwrap())
            .unwrap(),
        parsed
    );
}

#[test]
fn cf_html_unknown_context_offsets_cr_headers_and_untrusted_body() {
    let fragment = "<table><tr><td>🐄 &amp; чай</td></tr></table>";
    let template =
        "Version:1.0\rStartHTML:-1\rEndHTML:-1\rStartFragment:0000000000\rEndFragment:0000000000\r";
    let before = "<html><body><!--StartFragment-->";
    let after = "<!--EndFragment--></body></html>";
    let start = template.len() + before.len();
    let end = start + fragment.len();
    let header = format!(
        "Version:1.0\rStartHTML:-1\rEndHTML:-1\rStartFragment:{start:010}\rEndFragment:{end:010}\r"
    );
    let mut data = format!("{header}{before}{fragment}{after}").into_bytes();
    data.extend_from_slice(&[0, 0, 0]);
    assert_eq!(table::extract_cf_html(&data).unwrap(), fragment);
    let fake = "Version:1.0\r\nStartHTML:-1\r\nEndHTML:-1\r\nStartFragment:0000000000\r\n<html>\nEndFragment:0000000000\n</html>";
    assert!(table::extract_cf_html(fake.as_bytes()).is_err());
    let mut embedded = table::encode_cf_html(fragment).unwrap();
    let index = embedded
        .windows(4)
        .position(|w| w == "🐄".as_bytes())
        .unwrap();
    embedded[index] = 0;
    assert!(table::extract_cf_html(&embedded).is_err());
}

#[test]
fn serializers_roundtrip_final_empty_rows_and_columns() {
    for rows in [
        vec![vec![""]],
        vec![vec![""], vec![""]],
        vec![vec!["a"], vec![""]],
        vec![vec!["a", ""], vec!["multi\nline", ""]],
    ] {
        let original = Table {
            rows: rows
                .into_iter()
                .map(|row| row.into_iter().map(str::to_owned).collect())
                .collect(),
        };
        let tsv = table::to_tsv(&original).unwrap();
        assert!(tsv.ends_with("\r\n"));
        assert_eq!(table::parse_tsv(&tsv, true).unwrap(), original);
        let csv = table::to_csv_bom(&original).unwrap();
        assert_eq!(
            table::parse_csv(std::str::from_utf8(&csv).unwrap()).unwrap(),
            original
        );
    }
    assert!(Table { rows: vec![vec![]] }.validate().is_err());
}

#[test]
fn cf_html_accepts_unicode_source_url_with_byte_offsets() {
    let fragment = "<table><tr><td>🐄</td></tr></table>";
    let before = "<html><body><!--StartFragment-->";
    let after = "<!--EndFragment--></body></html>";
    let template = "Version:1.0\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\nSourceURL:https://пример.рф/путь\r\n";
    let start_html = template.len();
    let start_fragment = start_html + before.len();
    let end_fragment = start_fragment + fragment.len();
    let end_html = end_fragment + after.len();
    let header = format!(
        "Version:1.0\r\nStartHTML:{start_html:010}\r\nEndHTML:{end_html:010}\r\nStartFragment:{start_fragment:010}\r\nEndFragment:{end_fragment:010}\r\nSourceURL:https://пример.рф/путь\r\n"
    );
    assert_eq!(header.len(), template.len());
    let data = format!("{header}{before}{fragment}{after}");
    assert_eq!(table::extract_cf_html(data.as_bytes()).unwrap(), fragment);
}

#[test]
fn html_recognition_rejects_prose_outside_table_without_losing_it() {
    let table_markup = "<table><tr><td>🐄</td></tr></table>";
    let mixed = format!("<p>important note</p>{table_markup}");
    assert!(!table::has_html_table(&mixed));
    assert!(table::parse_html(&mixed).is_err());
    let wrapper =
        format!("<meta charset='utf-8'><style>body{{color:red}}</style><div>{table_markup}</div>");
    assert!(table::has_html_table(&wrapper));
    assert_eq!(table::parse_html(&wrapper).unwrap().rows, vec![vec!["🐄"]]);
    let two = format!("{table_markup}{table_markup}");
    assert!(table::has_html_table(&two));
    assert!(table::parse_html(&two).is_err());
    assert!(!table::has_html_table("<script>const x='<table>'</script>"));
    let hidden_prose = format!("<p style='display:none'>note</p>{table_markup}");
    assert!(!table::has_html_table(&hidden_prose));
}
