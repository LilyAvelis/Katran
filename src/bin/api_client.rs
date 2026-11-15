//! Single-writer dispatcher jobs and UI result handling.
use super::i18n::tr;
use super::*;

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod folder_page_tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn drain(app: &mut Shelf) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.folder_page_job.is_some() {
            app.finish_folder_page(&egui::Context::default());
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn transfer_result_exposes_recovery_path() {
        let root = tempfile::tempdir().unwrap();
        let mut app = super::super::tests::test_shelf(root.path());
        app.direct_transfer_result(&serde_json::json!({
            "direction":"to_folder", "items":[{
                "status":"recovery_required", "error":{"message":"synthetic failure"},
                "recovery_path":"C:/synthetic/recovery.bin"
            }]
        }));
        assert!(app.message.contains("synthetic failure"));
        assert!(app.message.contains("C:/synthetic/recovery.bin"));
        app.workspace_transfer_result(
            &serde_json::json!({
                "items":[{"status":"recovery_required", "error":{"message":"synthetic failure"},
                    "recovery_path":"C:/synthetic/recovery.bin"}]
            }),
            false,
        );
        assert!(app.message.contains("synthetic failure"));
        assert!(app.message.contains("C:/synthetic/recovery.bin"));
    }

    #[test]
    fn large_transfer_paints_before_begin_job_even_for_unloaded_selection() {
        use std::io::Write;
        let root = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let path = source.path().join("synthetic.bin");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(b"synthetic").unwrap();
        file.set_len(16 * 1024 * 1024).unwrap();
        drop(file);
        let mut app = super::super::tests::test_shelf(root.path());
        let event = app.call(Command::IngestPath {
            path,
            source: "synthetic".into(),
        });
        let id = event["id"].as_i64().unwrap();
        app.call(Command::ChooseFolder {
            path: target.path().into(),
        });
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        app.call(Command::SetVisible { ids: vec![id] });
        app.state = app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        app.events.clear(); // The selected item is outside the loaded page.
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        assert!(app.start_job(Command::CopySelection, false, ctx.clone()));
        assert!(matches!(app.job, Some(UiJob::TransferPrepaint { .. })));
        app.finish_job();
        assert!(matches!(app.job, Some(UiJob::TransferPrepaint { .. })));
        let mut repeated_move = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: Some(egui::Key::Tab),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::SHIFT,
            }],
            ..Default::default()
        };
        eframe::App::raw_input_hook(&mut app, &ctx, &mut repeated_move);
        assert!(repeated_move.events.is_empty());
        assert!(app.pending_tab_action.is_none());
        assert!(matches!(app.job, Some(UiJob::TransferPrepaint { .. })));
        // Ctrl+Tab must not become a copy when the shortcut is not bound.
        assert!(app.shortcuts.is_empty());
        let mut unbound_copy = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: Some(egui::Key::Tab),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL,
            }],
            ..Default::default()
        };
        eframe::App::raw_input_hook(&mut app, &ctx, &mut unbound_copy);
        assert!(app.pending_transfer_copies.is_empty());
        app.pending_density.push("density_more".into());
        app.pending_sort = Some("sort_name".into());
        app.pending_tab_action = Some("tab_toggle".into());
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.render(ui));
        output.textures_delta.clear();
        assert_eq!(app.pending_density.len(), 1);
        assert_eq!(app.pending_sort.as_deref(), Some("sort_name"));
        assert_eq!(app.pending_tab_action.as_deref(), Some("tab_toggle"));
        assert_eq!(app.transfer_begin_count, 0);
        fn has_status(shape: &egui::Shape) -> bool {
            match shape {
                egui::Shape::Text(text) => {
                    text.galley.text().contains("Копирование")
                        || text.galley.text().contains("Copying")
                }
                egui::Shape::Vec(shapes) => shapes.iter().any(has_status),
                _ => false,
            }
        }
        fn collect_text(shape: &egui::Shape, output: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => output.push(text.galley.text().to_owned()),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect_text(shape, output);
                    }
                }
                _ => {}
            }
        }
        let mut painted_text = Vec::new();
        for shape in &output.shapes {
            collect_text(&shape.shape, &mut painted_text);
        }
        assert!(
            output.shapes.iter().any(|shape| has_status(&shape.shape)),
            "text: {painted_text:?}"
        );
        assert!(app.transfer_busy.as_ref().unwrap().painted);
        assert!(matches!(app.job, Some(UiJob::TransferPrepaint { .. })));
        // A second move cannot replace or start beside the accepted transfer.
        app.run("transfer_selection".into(), &ctx);
        assert!(matches!(app.job, Some(UiJob::TransferPrepaint { .. })));
        assert!(app.pending_transfer_copies.is_empty());
        app.finish_job();
        assert!(matches!(app.job, Some(UiJob::Domain { .. })));
        assert_eq!(app.transfer_begin_count, 1);
        app.run("transfer_selection".into(), &ctx);
        assert_eq!(app.transfer_begin_count, 1);
        let mut repeated_copy = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: Some(egui::Key::Tab),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL,
            }],
            ..Default::default()
        };
        eframe::App::raw_input_hook(&mut app, &ctx, &mut repeated_copy);
        assert!(repeated_copy.events.is_empty());
        assert_eq!(app.pending_transfer_copies.len(), 1);
        assert_eq!(app.pending_tab_action.as_deref(), Some("tab_toggle"));
        let deadline = Instant::now() + Duration::from_secs(20);
        while app.job.is_some() || !app.pending_transfer_copies.is_empty() {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.render(ui));
            output.textures_delta.clear();
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(target.path().join("synthetic.bin").exists());
        assert_eq!(std::fs::read_dir(target.path()).unwrap().count(), 2);
        assert_eq!(app.transfer_begin_count, 2);
    }

    #[test]
    fn small_transfer_and_unknown_paste_have_no_busy_overlay() {
        let root = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let mut app = super::super::tests::test_shelf(root.path());
        let event = app.call(Command::IngestBytes {
            name: "tiny.png".into(),
            bytes: vec![1, 2, 3],
            source: "synthetic".into(),
        });
        let id = event["id"].as_i64().unwrap();
        app.call(Command::ChooseFolder {
            path: target.path().into(),
        });
        app.call(Command::SetTab {
            tab: katran::interaction::ShelfTab::Images,
        });
        app.call(Command::SetVisible { ids: vec![id] });
        app.state = app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        app.events = vec![event];
        assert_eq!(app.transfer_size_risk(&Command::PasteToFolder), Some(None));
        assert_eq!(
            app.transfer_size_risk(&Command::AddFolderFiles {
                ids: vec![-999_999]
            }),
            Some(None)
        );
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        assert!(app.start_job(Command::CopySelection, false, ctx.clone()));
        assert!(matches!(app.job, Some(UiJob::Domain { .. })));
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.render(ui));
        output.textures_delta.clear();
        assert!(
            app.transfer_busy
                .as_ref()
                .is_none_or(|busy| busy.visible_since.is_none())
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.job.is_some() {
            app.finish_job();
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
    }

    #[test]
    fn busy_overlay_labels_match_transfer_operation() {
        fn contains(shape: &egui::Shape, ru: &str, en: &str) -> bool {
            match shape {
                egui::Shape::Text(text) => text.galley.text() == ru || text.galley.text() == en,
                egui::Shape::Vec(shapes) => shapes.iter().any(|shape| contains(shape, ru, en)),
                _ => false,
            }
        }
        let cases = [
            (
                Command::CopySelection,
                TransferKind::Copy,
                "Копирование…",
                "Copying…",
            ),
            (
                Command::TransferSelection,
                TransferKind::Move,
                "Перемещение…",
                "Moving…",
            ),
            (
                Command::AddFolderFiles { ids: vec![] },
                TransferKind::Import,
                "Добавление в Катран…",
                "Adding to Katran…",
            ),
            (
                Command::PasteToFolder,
                TransferKind::Paste,
                "Вставка…",
                "Pasting…",
            ),
        ];
        let root = tempfile::tempdir().unwrap();
        let mut app = super::super::tests::test_shelf(root.path());
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        for (command, expected, ru, en) in &cases {
            let kind = TransferKind::for_command(command).unwrap();
            assert_eq!(kind, *expected);
            for (language, expected_text) in [
                (katran::settings::Language::Russian, *ru),
                (katran::settings::Language::English, *en),
            ] {
                i18n::set(language);
                app.transfer_busy = Some(TransferBusy {
                    kind,
                    visible_since: None,
                    painted: false,
                    active: true,
                    prepaint: true,
                });
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    app.paint_transfer_busy(ui.ctx())
                });
                output.textures_delta.clear();
                assert!(
                    output.shapes.iter().any(|shape| contains(
                        &shape.shape,
                        expected_text,
                        expected_text
                    )),
                    "missing label {expected_text}"
                );
            }
        }
        i18n::set(katran::settings::Language::Russian);
        assert_eq!(
            TransferKind::for_command(&Command::RunAction {
                id: "add_to_katran".into()
            }),
            Some(TransferKind::Import)
        );
    }

    #[test]
    fn delete_and_transfer_risks_use_physical_files_and_confirmation() {
        let root = tempfile::tempdir().unwrap();
        let mut app = super::super::tests::test_shelf(root.path());
        let ids: Vec<i64> = (1..=20).map(|index| -index).collect();
        app.events = ids
            .iter()
            .map(|id| {
                serde_json::json!({
                    "id":id, "size":1, "kind":"file_reference", "source":"work_folder",
                    "storage":"reference", "path":format!("C:/synthetic/{id}.txt"),
                    "delete_policy":{"kind":"shelf_only"}
                })
            })
            .collect();
        assert_eq!(
            app.transfer_size_risk(&Command::AddFolderFiles { ids: ids.clone() }),
            Some(Some(true))
        );
        assert_eq!(
            app.delete_size_risk(&Command::DeleteSelection {
                ids: ids.clone(),
                confirmed: false
            }),
            None
        );
        assert_eq!(
            app.delete_size_risk(&Command::DeleteSelection {
                ids: ids.clone(),
                confirmed: true
            }),
            Some((TransferKind::Recycle, Some(true)))
        );
        assert_eq!(
            app.delete_size_risk(&Command::DeleteSelection {
                ids: vec![ids[0]],
                confirmed: false
            }),
            Some((TransferKind::Recycle, Some(false)))
        );
        app.events[0]["size"] = serde_json::json!(16 * 1024 * 1024);
        assert_eq!(
            app.delete_size_risk(&Command::DeleteSelection {
                ids: vec![ids[0]],
                confirmed: false
            }),
            Some((TransferKind::Recycle, Some(true)))
        );
        app.events[0]["source"] = serde_json::json!("desktop");
        assert_eq!(
            app.delete_size_risk(&Command::DeleteSelection {
                ids: vec![ids[0]],
                confirmed: true
            }),
            Some((TransferKind::Delete, Some(true)))
        );
        app.events[0]["source"] = serde_json::json!("downloads");
        app.events[0]["delete_policy"] =
            serde_json::json!({"kind":"live_original","mode":"recycle"});
        assert_eq!(
            app.delete_size_risk(&Command::DeleteSelection {
                ids: vec![ids[0]],
                confirmed: true
            }),
            Some((TransferKind::Recycle, Some(true)))
        );
        app.events[0]["source"] = serde_json::json!("sharex");
        app.events[0]["delete_policy"] =
            serde_json::json!({"kind":"live_original","mode":"permanent"});
        assert_eq!(
            app.delete_size_risk(&Command::DeleteSelection {
                ids: vec![ids[0]],
                confirmed: true
            }),
            Some((TransferKind::Recycle, Some(true)))
        );
        app.events[0]["delete_policy"] = serde_json::json!({"kind":"shelf_only"});
        assert_eq!(
            app.delete_size_risk(&Command::DeleteSelection {
                ids: vec![ids[0]],
                confirmed: true
            }),
            None
        );
        for event in &mut app.events {
            event["kind"] = serde_json::json!("text_fragment");
            event["source"] = serde_json::json!("emoji_catalogue");
            event["storage"] = serde_json::json!("virtual");
            event["size"] = serde_json::json!(1);
        }
        assert_eq!(
            app.transfer_size_risk(&Command::AddFolderFiles { ids: ids.clone() }),
            Some(Some(false))
        );
        for event in &mut app.events {
            event["kind"] = serde_json::json!("file_reference");
            event.as_object_mut().unwrap().remove("storage");
        }
        assert_eq!(
            app.transfer_size_risk(&Command::AddFolderFiles { ids }),
            Some(None)
        );
    }

    #[test]
    fn delete_overlay_labels_are_painted_in_both_languages() {
        fn contains(shape: &egui::Shape, expected: &str) -> bool {
            match shape {
                egui::Shape::Text(text) => text.galley.text() == expected,
                egui::Shape::Vec(shapes) => shapes.iter().any(|shape| contains(shape, expected)),
                _ => false,
            }
        }
        let root = tempfile::tempdir().unwrap();
        let mut app = super::super::tests::test_shelf(root.path());
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        for (kind, ru, en) in [
            (TransferKind::Delete, "Удаление…", "Deleting…"),
            (
                TransferKind::Recycle,
                "В корзину…",
                "Moving to Recycle Bin…",
            ),
        ] {
            for (language, expected) in [
                (katran::settings::Language::Russian, ru),
                (katran::settings::Language::English, en),
            ] {
                i18n::set(language);
                app.transfer_busy = Some(TransferBusy {
                    kind,
                    visible_since: None,
                    painted: false,
                    active: true,
                    prepaint: true,
                });
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    app.paint_transfer_busy(ui.ctx())
                });
                output.textures_delta.clear();
                assert!(
                    output
                        .shapes
                        .iter()
                        .any(|shape| contains(&shape.shape, expected)),
                    "missing {expected}"
                );
            }
        }
        i18n::set(katran::settings::Language::Russian);
    }

    #[test]
    fn single_tiny_folder_delete_has_no_overlay() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        struct FakeTrash(Arc<AtomicUsize>);
        impl katran::downloads::Trash for FakeTrash {
            fn recycle(&self, staged: &std::path::Path) -> katran::Result<()> {
                self.0.fetch_add(1, Ordering::SeqCst);
                std::fs::remove_file(staged)?;
                Ok(())
            }
        }
        let root = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("tiny.txt"), b"x").unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let mut app = super::super::tests::test_shelf(root.path());
        app.api
            .as_mut()
            .unwrap()
            .set_trash(Box::new(FakeTrash(calls.clone())));
        app.state = app.call(Command::ChooseFolder {
            path: folder.path().into(),
        });
        let page = app.call(Command::BrowseShelfSorted {
            query: "".into(),
            limit: 10,
            cursor: None,
        });
        app.events = page["events"].as_array().unwrap().clone();
        let id = app.events[0]["id"].as_i64().unwrap();
        app.call(Command::SetVisible { ids: vec![id] });
        app.state = app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        let ctx = egui::Context::default();
        app.run("delete".into(), &ctx);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(app.transfer_begin_count, 1);
        assert!(
            app.transfer_busy
                .as_ref()
                .is_none_or(|busy| busy.visible_since.is_none() && !busy.prepaint)
        );
        assert!(app.job.is_none());
    }

    #[test]
    fn single_large_folder_delete_paints_recycle_before_begin() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        struct FakeTrash(Arc<AtomicUsize>);
        impl katran::downloads::Trash for FakeTrash {
            fn recycle(&self, staged: &std::path::Path) -> katran::Result<()> {
                self.0.fetch_add(1, Ordering::SeqCst);
                std::fs::remove_file(staged)?;
                Ok(())
            }
        }
        let root = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("synthetic.txt"), b"x").unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let mut app = super::super::tests::test_shelf(root.path());
        app.api
            .as_mut()
            .unwrap()
            .set_trash(Box::new(FakeTrash(calls.clone())));
        app.state = app.call(Command::ChooseFolder {
            path: folder.path().into(),
        });
        let page = app.call(Command::BrowseShelfSorted {
            query: "".into(),
            limit: 10,
            cursor: None,
        });
        let mut event = page["events"][0].clone();
        let id = event["id"].as_i64().unwrap();
        app.call(Command::SetVisible { ids: vec![id] });
        app.state = app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        event["size"] = serde_json::json!(16 * 1024 * 1024);
        app.events = vec![event];
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        app.run("delete".into(), &ctx);
        assert!(matches!(app.job, Some(UiJob::TransferPrepaint { .. })));
        assert_eq!(
            app.transfer_busy.as_ref().unwrap().kind,
            TransferKind::Recycle
        );
        assert_eq!(app.transfer_begin_count, 0);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.render(ui));
        output.textures_delta.clear();
        assert!(app.transfer_busy.as_ref().unwrap().painted);
        assert!(matches!(app.job, Some(UiJob::TransferPrepaint { .. })));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        app.finish_job();
        assert_eq!(app.transfer_begin_count, 1);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(app.job.is_none());
    }

    #[test]
    fn many_tiny_folder_deletes_confirm_then_paint_before_begin() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        struct FakeTrash(Arc<AtomicUsize>);
        impl katran::downloads::Trash for FakeTrash {
            fn recycle(&self, staged: &std::path::Path) -> katran::Result<()> {
                self.0.fetch_add(1, Ordering::SeqCst);
                std::fs::remove_file(staged)?;
                Ok(())
            }
        }
        let root = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        for index in 0..20 {
            std::fs::write(folder.path().join(format!("item-{index:02}.txt")), b"x").unwrap();
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let mut app = super::super::tests::test_shelf(root.path());
        app.api
            .as_mut()
            .unwrap()
            .set_trash(Box::new(FakeTrash(calls.clone())));
        app.state = app.call(Command::ChooseFolder {
            path: folder.path().into(),
        });
        let page = app.call(Command::BrowseShelfSorted {
            query: "".into(),
            limit: 30,
            cursor: None,
        });
        app.events = page["events"].as_array().unwrap().clone();
        let ids: Vec<i64> = app
            .events
            .iter()
            .map(|event| event["id"].as_i64().unwrap())
            .collect();
        assert_eq!(ids.len(), 20);
        app.call(Command::SetVisible { ids: ids.clone() });
        for (index, id) in ids.iter().enumerate() {
            app.state = app.call(Command::Select {
                id: *id,
                mode: if index == 0 {
                    SelectionMode::Replace
                } else {
                    SelectionMode::Toggle
                },
            });
        }
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        app.run("delete".into(), &ctx);
        assert_eq!(app.pending_delete.len(), 20);
        assert!(app.transfer_busy.is_none());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(app.start_job(
            Command::DeleteSelection {
                ids: app.pending_delete.clone(),
                confirmed: true
            },
            false,
            ctx.clone()
        ));
        app.pending_delete.clear();
        assert!(matches!(app.job, Some(UiJob::TransferPrepaint { .. })));
        assert_eq!(app.transfer_begin_count, 0);
        app.run("delete".into(), &ctx);
        app.queue_transfer_copy();
        assert!(app.pending_transfer_copies.is_empty());
        assert_eq!(app.transfer_begin_count, 0);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.render(ui));
        output.textures_delta.clear();
        assert!(matches!(app.job, Some(UiJob::TransferPrepaint { .. })));
        assert!(app.transfer_busy.as_ref().unwrap().painted);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        app.finish_job();
        assert_eq!(app.transfer_begin_count, 1);
        assert_eq!(calls.load(Ordering::SeqCst), 20);
        assert!(app.job.is_none());
    }

    #[test]
    fn completed_busy_overlay_dwell_does_not_hold_job() {
        let root = tempfile::tempdir().unwrap();
        let mut app = super::super::tests::test_shelf(root.path());
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        app.transfer_busy = Some(TransferBusy {
            kind: TransferKind::Copy,
            visible_since: Some(Instant::now()),
            painted: true,
            active: false,
            prepaint: true,
        });
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.paint_transfer_busy(ui.ctx())
        });
        output.textures_delta.clear();
        assert!(app.transfer_busy.is_some());
        assert!(!app.transfer_input_reserved());
        app.transfer_busy.as_mut().unwrap().visible_since =
            Some(Instant::now() - Duration::from_millis(201));
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.paint_transfer_busy(ui.ctx())
        });
        output.textures_delta.clear();
        assert!(app.transfer_busy.is_none());
    }

    #[test]
    fn ctrl_shift_c_folder_import_uses_begin_job_after_overlay_frame() {
        let root = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("synthetic.txt"), b"hello").unwrap();
        let mut app = super::super::tests::test_shelf(root.path());
        app.call(Command::SetTabEnabled {
            tab: katran::interaction::ShelfTab::Text,
            enabled: true,
        });
        app.state = app.call(Command::ChooseFolder {
            path: folder.path().into(),
        });
        let page = app.call(Command::BrowseShelfSorted {
            query: "".into(),
            limit: 10,
            cursor: None,
        });
        let mut event = page["events"][0].clone();
        let id = event["id"].as_i64().unwrap();
        app.call(Command::SetVisible { ids: vec![id] });
        app.state = app.call(Command::Select {
            id,
            mode: SelectionMode::Replace,
        });
        event["size"] = serde_json::json!(16 * 1024 * 1024);
        app.events = vec![event];
        let ctx = egui::Context::default();
        ui_style::install(&ctx);
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::C,
                physical_key: Some(egui::Key::C),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            }],
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| app.render(ui));
        output.textures_delta.clear();
        assert!(matches!(app.job, Some(UiJob::TransferPrepaint { .. })));
        assert!(app.transfer_busy.as_ref().unwrap().painted);
        app.finish_job();
        assert!(matches!(app.job, Some(UiJob::Domain { .. })));
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.job.is_some() {
            app.finish_job();
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(folder.path().join("synthetic.txt").exists());
    }

    #[test]
    fn folder_refresh_discards_old_query_and_loads_more_asynchronously() {
        let root = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        for n in 0..1002 {
            std::fs::write(folder.path().join(format!("item-{n:04}")), b"x").unwrap();
        }
        let mut app = super::super::tests::test_shelf(root.path());
        app.state = app.call(Command::ChooseFolder {
            path: folder.path().into(),
        });
        app.refresh();
        assert!(app.folder_page_job.is_some());
        app.query = "item-100".into();
        app.refresh();
        drain(&mut app);
        assert_eq!(app.events.len(), 2);
        assert!(
            app.events
                .iter()
                .all(|event| event["name"].as_str().unwrap().contains("item-100"))
        );
        app.query.clear();
        app.refresh();
        drain(&mut app);
        assert_eq!(app.events.len(), 1000);
        app.load_more();
        assert!(app.folder_page_job.is_some());
        drain(&mut app);
        assert_eq!(app.events.len(), 1002);
    }

    #[test]
    fn select_all_runs_as_a_job_and_updates_selection() {
        let root = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("a"), b"x").unwrap();
        std::fs::write(folder.path().join("b"), b"x").unwrap();
        let mut app = super::super::tests::test_shelf(root.path());
        app.state = app.call(Command::ChooseFolder {
            path: folder.path().into(),
        });
        app.refresh();
        drain(&mut app);
        app.run("select_all".into(), &egui::Context::default());
        assert!(app.job.is_some());
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.job.is_some() {
            app.finish_job();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(app.state["selected"].as_array().unwrap().len(), 2);

        app.query = "a".into();
        app.refresh();
        app.run("select_all".into(), &egui::Context::default());
        assert!(app.pending_folder_select_all);
        drain(&mut app);
        while app.job.is_some() {
            app.finish_job();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(app.state["selected"].as_array().unwrap().len(), 1);
    }
}

impl Shelf {
    pub(super) fn queue_transfer_copy(&mut self) {
        if self.transfer_busy.as_ref().is_some_and(|busy| {
            busy.active && matches!(busy.kind, TransferKind::Delete | TransferKind::Recycle)
        }) {
            self.message = tr(
                "Дождитесь завершения удаления",
                "Wait for deletion to finish",
            )
            .into();
            return;
        }
        if self.pending_transfer_copies.len() >= 64 {
            self.message = tr("Очередь копирования заполнена", "Copy queue is full").into();
        } else if let Some(items) = self.selection_snapshot() {
            self.pending_transfer_copies.push_back((
                items,
                self.state["tab"].clone(),
                self.state["folder_path"].clone(),
            ));
            self.message = tr("Копирование добавлено в очередь", "Copy added to queue").into();
        } else {
            self.message = tr("Выбор устарел", "Selection is out of date").into();
        }
    }
    pub(super) fn direct_transfer_input(&mut self, ctx: &egui::Context) {
        if self.job.is_none()
            && self.api.is_some()
            && let Some((expected, tab, folder)) = self.pending_transfer_copies.pop_front()
        {
            let valid = self.state["tab"] == tab
                && self.state["folder_path"] == folder
                && self.selection_snapshot().is_some_and(|actual| {
                    actual.len() == expected.len()
                        && actual
                            .iter()
                            .zip(&expected)
                            .all(|(a, b)| a.id == b.id && a.revision == b.revision)
                });
            if valid {
                self.start_job(Command::CopySelection, false, ctx.clone());
            } else {
                self.message = tr(
                    "Копия из очереди отменена: выбор или папка изменились",
                    "Queued copy cancelled: selection or folder changed",
                )
                .into();
            }
        }
        if self.api.is_none()
            || self.modal_owns_input()
            || ctx
                .memory(|m| m.focused())
                .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some())
        {
            return;
        }
        let catalogue: Vec<_> = self
            .shortcuts
            .iter()
            .filter(|shortcut| {
                matches!(shortcut.id.as_str(), "transfer_selection" | "transfer_copy")
            })
            .cloned()
            .collect();
        if let Some(action) = shortcuts_ui::take_action_with_editing(ctx, &catalogue, false) {
            if self.job.is_some() {
                if action == "transfer_copy" {
                    self.queue_transfer_copy();
                } else {
                    self.message = tr(
                        "Дождитесь завершения текущей операции",
                        "Wait for the current operation to finish",
                    )
                    .into();
                }
                return;
            }
            if let Some(id) = ctx.memory(|memory| memory.focused()) {
                ctx.memory_mut(|memory| memory.surrender_focus(id));
            }
            self.run(action, ctx);
        }
    }
    pub(super) fn start_drag(&mut self, ids: Vec<i64>, ctx: &egui::Context) {
        if self.native_drag.is_some() || self.job.is_some() {
            return;
        }
        let items = self.snapshot_ids(ids);
        let Some(items) = items else {
            self.message = tr(
                "Выбор устарел; обновите полку",
                "Selection is out of date; refresh the shelf",
            )
            .into();
            return;
        };
        if self.start_job(Command::DragEventsChecked { items }, false, ctx.clone()) {
            self.message = tr("Перетаскивание файлов…", "Dragging files…").into();
        }
    }
    pub(super) fn call(&mut self, command: Command) -> Value {
        let Some(api) = self.api.as_mut() else {
            return match command {
                Command::Interaction | Command::SetVisible { .. } => self.state.clone(),
                _ => Value::Null,
            };
        };
        let r = api.execute(Request {
            api_version: 1,
            request_id: "ui".into(),
            command,
        });
        if r["ok"] == false {
            self.message = r["error"]["message"]
                .as_str()
                .unwrap_or(tr("Ошибка", "Error"))
                .into();
        }
        r["result"].clone()
    }
    fn risk_event(&mut self, id: i64) -> Value {
        match self
            .events
            .iter()
            .find(|event| event["id"].as_i64() == Some(id))
        {
            Some(event)
                if event["size"].is_u64()
                    && event["kind"].is_string()
                    && event["source"].is_string()
                    && event["storage"].is_string()
                    && event["delete_policy"].is_object() =>
            {
                event.clone()
            }
            _ => self.call(Command::Get { id }),
        }
    }
    fn is_real_file(event: &Value) -> bool {
        matches!(
            event["kind"].as_str(),
            Some("file_reference" | "managed_image")
        ) && matches!(event["storage"].as_str(), Some("cached" | "reference"))
            && event["path"].as_str().is_some_and(|path| !path.is_empty())
    }
    fn unknown_file_identity(event: &Value) -> bool {
        match event["kind"].as_str() {
            Some("file_reference" | "managed_image") => {
                if event["storage"] == "virtual" {
                    return false;
                }
                !matches!(event["storage"].as_str(), Some("cached" | "reference"))
                    || event["path"].as_str().is_none_or(|path| path.is_empty())
            }
            Some("text_fragment" | "link_fragment" | "table_fragment") => false,
            _ => true,
        }
    }
    fn transfer_size_risk(&mut self, command: &Command) -> Option<Option<bool>> {
        let ids = match command {
            Command::CopySelection | Command::TransferSelection => self.selected_ids(),
            Command::AddFolderFiles { ids } => ids.clone(),
            Command::RunAction { id } if id == "add_to_katran" => self.selected_ids(),
            Command::PasteToFolder => return Some(None), // Clipboard size is not in the UI state.
            _ => return None,
        };
        let mut total = 0u64;
        let mut unknown = false;
        let mut files = 0usize;
        let mut seen = HashSet::new();
        for id in ids {
            if !seen.insert(id) {
                continue;
            }
            let event = self.risk_event(id);
            if let Some(size) = event["size"].as_u64() {
                total = total.saturating_add(size);
            } else {
                unknown = true;
            }
            if Self::is_real_file(&event) {
                files += 1;
            } else if Self::unknown_file_identity(&event) {
                unknown = true;
            }
        }
        Some(if total >= 16 * 1024 * 1024 || files >= 20 {
            Some(true)
        } else if unknown {
            None
        } else {
            Some(false)
        })
    }
    fn delete_size_risk(&mut self, command: &Command) -> Option<(TransferKind, Option<bool>)> {
        let (ids, confirmed) = match command {
            Command::DeleteSelection { ids, confirmed } => (ids.clone(), *confirmed),
            Command::RunAction { id } if id == "delete" => (self.selected_ids(), false),
            _ => return None,
        };
        let mut seen = HashSet::new();
        let ids: Vec<_> = ids.into_iter().filter(|id| seen.insert(*id)).collect();
        if ids.is_empty() || (!confirmed && ids.len() > 1) {
            return None;
        }
        let mut total = 0u64;
        let mut count = 0usize;
        let mut unknown = false;
        let mut recycle_only = true;
        for id in ids {
            let event = self.risk_event(id);
            let source = event["source"].as_str();
            let policy = event["delete_policy"]["kind"].as_str();
            let physical = if !Self::is_real_file(&event) {
                if Self::unknown_file_identity(&event) {
                    unknown = true;
                }
                false
            } else if source == Some("work_folder") {
                true
            } else if source == Some("desktop") {
                recycle_only = false;
                true
            } else if policy == Some("live_original") {
                // The dispatcher recycles even legacy ShareX records marked permanent.
                true
            } else {
                if policy.is_none() || source.is_none() {
                    unknown = true;
                }
                false
            };
            if physical {
                count += 1;
                if let Some(size) = event["size"].as_u64() {
                    total = total.saturating_add(size);
                } else {
                    unknown = true;
                }
            }
        }
        if count == 0 && !unknown {
            return None;
        } // Shelf-only removal is metadata-only.
        let kind = if recycle_only && count > 0 {
            TransferKind::Recycle
        } else {
            TransferKind::Delete
        };
        let risk = if total >= 16 * 1024 * 1024 || count >= 20 {
            Some(true)
        } else if unknown {
            None
        } else {
            Some(false)
        };
        Some((kind, risk))
    }
    pub(super) fn start_job(
        &mut self,
        command: Command,
        capture: bool,
        ctx: egui::Context,
    ) -> bool {
        if self.api.is_none()
            || (capture && self.capture_job.is_some())
            || (!capture && self.job.is_some())
        {
            return false;
        }
        self.busy_text_editing = ctx.egui_wants_keyboard_input();
        let operation = if capture {
            None
        } else {
            self.delete_size_risk(&command).or_else(|| {
                self.transfer_size_risk(&command).map(|risk| {
                    (
                        TransferKind::for_command(&command).expect("Folder transfer kind"),
                        risk,
                    )
                })
            })
        };
        if let Some((kind, risk)) = operation {
            let prepaint = risk == Some(true);
            self.transfer_busy = Some(TransferBusy {
                kind,
                visible_since: None,
                painted: false,
                active: true,
                prepaint,
            });
            if prepaint {
                self.job = Some(UiJob::TransferPrepaint {
                    command: Box::new(command),
                    ctx: ctx.clone(),
                });
                ctx.request_repaint();
                return true;
            }
        }
        self.submit_job(command, capture, ctx)
    }
    fn submit_job(&mut self, command: Command, capture: bool, ctx: egui::Context) -> bool {
        #[cfg(test)]
        if self.transfer_busy.as_ref().is_some_and(|busy| busy.active) {
            self.transfer_begin_count += 1;
        }
        let result = self.call(Command::BeginJob {
            command: Box::new(command),
        });
        if result["state"] == "succeeded" {
            self.apply_job_reply(
                serde_json::json!({"ok":true,"result":result["result"]}),
                capture,
            );
            if !capture && let Some(busy) = &mut self.transfer_busy {
                busy.active = false;
            }
            return true;
        }
        let Some(id) = result["job_id"].as_u64() else {
            if !capture && let Some(busy) = &mut self.transfer_busy {
                busy.active = false;
            }
            return false;
        };
        if capture {
            self.capture_job = Some(katran::domain_jobs::JobId(id));
        } else {
            self.job = Some(UiJob::Domain {
                job_id: katran::domain_jobs::JobId(id),
                capture: false,
            });
        }
        ctx.request_repaint();
        true
    }
    fn preview_request_context(&mut self) -> Value {
        let state = self.call(Command::Interaction);
        serde_json::json!({"selected":state["selected"],"cursor":state["cursor"],"tab":state["tab"],"pinned_only":state["pinned_only"],"folder_path":state["folder_path"]})
    }
    pub(super) fn preview_warmup_tick(&mut self, ctx: &egui::Context) {
        if let Some(job_id) = self.preview_warmup {
            let status = self.call(Command::PollJob { job_id });
            if !matches!(
                status["state"].as_str(),
                Some("succeeded" | "failed" | "cancelled")
            ) {
                return;
            }
            self.preview_warmup = None;
            if let Some(log) = &self.logger {
                log.record(
                    "preview_backend_startup",
                    serde_json::json!({"state":status["state"],"error":status["error"]}),
                );
            }
            if status["state"] != "succeeded" {
                self.pending_preview = None;
                self.message = format!(
                    "QuickLook: {}",
                    status["error"]["message"].as_str().unwrap_or(tr(
                        "Не удалось подготовить просмотр",
                        "Could not prepare preview"
                    ))
                );
                return;
            }
        }
        if self.job.is_none()
            && let Some((id, context)) = self.pending_preview.take()
            && self.preview_request_context() == context
        {
            self.start_job(Command::RunAction { id }, false, ctx.clone());
        }
    }
    fn finish_capture_job(&mut self) {
        let Some(job_id) = self.capture_job else {
            return;
        };
        let status = self.call(Command::PollJob { job_id });
        let reply = match status["state"].as_str() {
            Some("queued" | "running" | "ready") => return,
            Some("succeeded") => serde_json::json!({"ok":true,"result":status["result"]}),
            _ => serde_json::json!({"ok":false,"error":status["error"]}),
        };
        self.capture_job = None;
        self.apply_job_reply(reply, true);
    }
    pub(super) fn finish_job(&mut self) {
        if self
            .transfer_busy
            .as_ref()
            .is_some_and(|busy| busy.active && busy.show_due() && !busy.painted)
        {
            return;
        }
        if matches!(self.job, Some(UiJob::TransferPrepaint { .. })) {
            if let Some(UiJob::TransferPrepaint { command, ctx }) = self.job.take() {
                self.submit_job(*command, false, ctx);
            }
            return;
        }
        self.finish_capture_job();
        let Some(job) = self.job.as_ref() else {
            return;
        };
        let (reply, capture) = match job {
            UiJob::TransferPrepaint { .. } => unreachable!("handled before polling"),
            UiJob::Domain { job_id, capture } => {
                let job_id = *job_id;
                let capture = *capture;
                let status = self.call(Command::PollJob { job_id });
                match status["state"].as_str() {
                    Some("queued" | "running" | "ready") => return,
                    Some("succeeded") => (
                        serde_json::json!({"ok":true,"result":status["result"]}),
                        capture,
                    ),
                    Some("failed" | "cancelled") => (
                        serde_json::json!({"ok":false,"error":status["error"]}),
                        capture,
                    ),
                    _ => (
                        serde_json::json!({"ok":false,"error":{"message":tr("Потерян статус фоновой операции", "Background operation status lost")}}),
                        capture,
                    ),
                }
            }
            #[cfg(test)]
            UiJob::UnavailableTransport(rx) => match rx.try_recv() {
                Ok((api, reply, capture)) => {
                    self.api = Some(api);
                    (reply, capture)
                }
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => (
                    serde_json::json!({"ok":false,"error":{"message":"Тестовый транспорт отключён"}}),
                    false,
                ),
            },
        };
        self.job = None;
        self.apply_job_reply(reply, capture);
        if !capture && let Some(busy) = &mut self.transfer_busy {
            busy.active = false;
        }
    }
    fn apply_job_reply(&mut self, reply: Value, capture: bool) {
        let wallpaper = !capture && std::mem::take(&mut self.wallpaper_job);
        if reply["ok"] == false {
            self.message = reply["error"]["message"]
                .as_str()
                .unwrap_or(tr("Ошибка", "Error"))
                .into();
            if wallpaper {
                self.wallpaper_notice = Some((self.message.clone(), Instant::now()));
            }
            return;
        }
        let result = &reply["result"];
        if result["backend"] == "quicklook" {
            self.action_result(result.clone());
            self.state = self.call(Command::Interaction);
            return;
        }
        if result.get("preview").is_some() && result.get("view").is_some() {
            self.state = result.clone();
            return;
        }
        if result.get("classification_changed").is_some() {
            if result["classification_changed"] == true {
                self.refresh();
            }
            return;
        }
        #[cfg(feature = "clipboard-events")]
        if capture {
            self.clipboard_events.completed(result, Instant::now());
            if result["capture_schedule"]["retry_after_ms"].is_null() {
                if let Some(error) = result["capture_error"]["message"].as_str() {
                    self.message = format!(
                        "{}: {error}",
                        tr(
                            "Не удалось захватить Clipboard",
                            "Could not capture Clipboard"
                        )
                    );
                } else if result["capture_schedule"]["exhausted"] == true {
                    self.message = tr("Clipboard оставался занят или менялся; повторы исчерпаны. Скопируйте объект ещё раз.", "Clipboard stayed busy or changed; retries exhausted. Copy the item again.").into();
                }
            }
        }
        if let Some(token) = result["drag_token"].as_str() {
            self.native_drag = Some(token.to_owned());
            self.message = tr("Файлы готовы к перетаскиванию…", "Files are ready to drag…").into();
            return;
        }
        if result["scanned"] == true {
            let errors = item_failures(&result["items"]);
            if !errors.is_empty() {
                self.message = format!("{}: {errors}", tr("Папки", "Folders"));
            }
            if result["items"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item["ok"] == true))
            {
                self.sources.match_dirty |= result["items"].as_array().is_some_and(|items| {
                    items.iter().any(|item| item["event"]["source"] == "sharex")
                });
                self.refresh();
            }
            return;
        }
        if !capture && result["folder_paste"] == true {
            self.workspace_transfer_result(result, false);
            return;
        }
        if !capture && result["folder_import"] == true {
            let items = result["items"].as_array().map(Vec::as_slice).unwrap_or(&[]);
            let failed = items.iter().filter(|item| item["ok"] != true).count();
            self.message = format!(
                "{}: {}. {}: {failed}. {}",
                tr("Добавлено в Катран", "Added to Katran"),
                items.len() - failed,
                tr("Ошибок", "Failures"),
                item_failures(&result["items"])
            );
            self.refresh();
            return;
        }
        if !capture
            && matches!(
                result["direction"].as_str(),
                Some("from_folder" | "to_folder")
            )
        {
            self.direct_transfer_result(result);
            self.refresh();
            return;
        }
        if capture {
            let errors = item_failures(&result["items"]);
            if !errors.is_empty() {
                self.message = format!(
                    "{}: {errors}",
                    tr("Не удалось захватить", "Could not capture")
                );
            }
            if clipboard_changed(result) {
                self.sources.match_dirty = true;
                self.refresh();
            }
        } else {
            self.action_result(result.clone());
            if wallpaper {
                self.wallpaper_notice = Some((self.message.clone(), Instant::now()));
            }
            self.refresh();
        }
    }
    fn direct_transfer_result(&mut self, result: &Value) {
        if result["interaction"].is_object() {
            self.state = result["interaction"].clone();
        }
        let items = result["items"].as_array().cloned().unwrap_or_default();
        let from_folder = result["direction"] == "from_folder";
        let failures = items
            .iter()
            .filter(|item| {
                if from_folder {
                    item["ok"] != true
                } else {
                    !matches!(
                        item["status"].as_str(),
                        Some("moved" | "copied" | "pinned_copy" | "same_folder")
                    )
                }
            })
            .count();
        let pinned = items
            .iter()
            .filter(|item| item["status"] == "pinned_copy")
            .count();
        let moved = items
            .iter()
            .filter(|item| item["status"] == "moved")
            .count();
        let copied = items
            .iter()
            .filter(|item| item["status"] == "copied")
            .count();
        let details = items
            .iter()
            .filter_map(|item| {
                let error = item["error"]["message"].as_str()?;
                Some(match item["recovery_path"].as_str() {
                    Some(path) => {
                        format!("{error} · {}: {path}", tr("Файл сохранён", "File saved"))
                    }
                    None => error.to_owned(),
                })
            })
            .collect::<Vec<_>>()
            .join("; ");
        self.message = if from_folder {
            format!(
                "{}: {} · {}: {failures}. {details}",
                tr("Добавлено в Катран", "Added to Katran"),
                items.len() - failures,
                tr("Ошибок", "Failures")
            )
        } else {
            if super::i18n::is_english() {
                format!(
                    "To Folder: moved {moved} · copied {copied} · pinned copied {pinned} · failures: {failures}. {details}"
                )
            } else {
                format!(
                    "В Folder: перемещено {moved} · скопировано {copied} · Закреплённых скопировано {pinned} · Ошибок: {failures}. {details}"
                )
            }
        };
        if let Some(error) = result["interaction_error"]["message"].as_str() {
            self.message.push_str(" · ");
            self.message.push_str(error);
        }
    }
    fn folder_page_context(&self) -> Value {
        serde_json::json!({"tab":self.state["tab"], "path":self.state["folder_path"], "sort":self.state["sort"], "query":self.query})
    }
    fn cancel_folder_page(&mut self) {
        if let Some(job) = self.folder_page_job.take()
            && let Some(api) = self.api.as_mut()
        {
            api.execute(Request {
                api_version: 1,
                request_id: "ui-folder-cancel".into(),
                command: Command::CancelJob { job_id: job.id },
            });
        }
    }
    fn start_folder_page(&mut self, cursor: Option<String>, target: usize) {
        self.workspace.last_refresh = Some(Instant::now());
        let append = cursor.is_some();
        let context = self.folder_page_context();
        let response = self.call(Command::BeginJob {
            command: Box::new(Command::BrowseShelfSorted {
                query: self.query.clone(),
                limit: 1000,
                cursor,
            }),
        });
        if let Ok(id) = serde_json::from_value(response["job_id"].clone()) {
            self.folder_page_job = Some(FolderPageJob {
                id,
                context,
                append,
                target,
            });
        }
    }
    pub(super) fn finish_folder_page(&mut self, ctx: &egui::Context) {
        let Some(job) = self.folder_page_job.take() else {
            return;
        };
        if self.folder_page_context() != job.context {
            self.pending_folder_select_all = false;
            self.folder_page_job = Some(job);
            self.cancel_folder_page();
            return;
        }
        let status = self.call(Command::PollJob { job_id: job.id });
        if matches!(
            status["state"].as_str(),
            Some("succeeded" | "failed" | "cancelled")
        ) {
            self.workspace.last_refresh = Some(Instant::now());
        }
        match status["state"].as_str() {
            Some("queued" | "running" | "ready") => self.folder_page_job = Some(job),
            Some("succeeded") => {
                let result = &status["result"];
                if let Some(events) = result["events"].as_array() {
                    if !job.append {
                        self.events.clear();
                    }
                    self.events.extend(events.iter().cloned());
                    self.next_cursor = result["next_cursor"].as_str().map(str::to_owned);
                    self.loaded_query = self.query.clone();
                    let ids = self
                        .events
                        .iter()
                        .filter_map(|event| event["id"].as_i64())
                        .collect();
                    self.state = self.call(Command::SetVisible { ids });
                    if self.events.len() < job.target
                        && let Some(cursor) = self.next_cursor.clone()
                    {
                        self.start_folder_page(Some(cursor), job.target);
                    } else if self.pending_folder_select_all {
                        self.pending_folder_select_all = false;
                        self.start_job(Command::SelectAll, false, ctx.clone());
                    }
                }
            }
            Some("failed") => {
                self.pending_folder_select_all = false;
                if status["error"]["code"] == "stale_cursor" {
                    self.next_cursor = None;
                    self.message = tr(
                        "Папка изменилась; обновляем список",
                        "Folder changed; refreshing the list",
                    )
                    .into();
                } else if status["error"]["code"] != "stale_folder" {
                    self.message = status["error"]["message"]
                        .as_str()
                        .unwrap_or("Folder browse failed")
                        .into();
                }
            }
            _ => self.pending_folder_select_all = false,
        }
    }
    pub(super) fn refresh(&mut self) {
        self.pending_folder_select_all = false;
        self.preloaded_has_more = false;
        self.tab_preload.waiting = None;
        if self.emoji_ai.enabled
            && self.state["tab"] == "pods"
            && self.state["emoji_catalogue"] == true
        {
            self.emoji_ai_refresh();
            return;
        }
        let target = if self.query == self.loaded_query {
            self.events.len().max(1000)
        } else {
            1000
        };
        self.cancel_folder_page();
        if self.state["tab"] == "folder" {
            if self.state["folder_path"].is_null() {
                self.events.clear();
                self.next_cursor = None;
                return;
            }
            if self.query != self.loaded_query {
                self.events.clear();
                self.next_cursor = None;
            }
            self.start_folder_page(None, target);
            return;
        }
        let r = self.call(Command::BrowseShelfSorted {
            query: self.query.clone(),
            limit: 1000,
            cursor: None,
        });
        if let Some(events) = r["events"].as_array() {
            self.pods_ui.category_total = if self.state["emoji_catalogue"] == true {
                r["total"].as_u64()
            } else {
                None
            };
            self.events = events.clone();
            self.next_cursor = r["next_cursor"].as_str().map(str::to_owned);
            self.loaded_query = self.query.clone();
            while self.events.len() < target && self.next_cursor.is_some() {
                let previous = self.events.len();
                self.load_more();
                if self.events.len() == previous {
                    break;
                }
            }
        } else {
            self.events.clear();
            self.next_cursor = None;
        }
        self.capture = self.call(Command::Status)["capture_enabled"] == true;
    }
    pub(super) fn load_more(&mut self) {
        if self.emoji_ai.enabled {
            self.emoji_ai_load_more();
            return;
        }
        if self.state["tab"] == "folder" && self.folder_page_job.is_some() {
            return;
        }
        let Some(cursor) = self.next_cursor.clone() else {
            return;
        };
        if self.state["tab"] == "folder" {
            self.start_folder_page(Some(cursor), self.events.len().saturating_add(1000));
            return;
        }
        let r = self.call(Command::BrowseShelfSorted {
            query: self.query.clone(),
            limit: 1000,
            cursor: Some(cursor),
        });
        if let Some(events) = r["events"].as_array() {
            self.events.extend(events.iter().cloned());
            self.next_cursor = r["next_cursor"].as_str().map(str::to_owned);
        }
    }
    pub(super) fn selected_ids(&self) -> Vec<i64> {
        let ids: Vec<i64> = self.state["selected"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_i64)
            .collect();
        if ids.is_empty() {
            self.state["cursor"].as_i64().into_iter().collect()
        } else {
            ids
        }
    }
    fn snapshot_ids(&mut self, ids: Vec<i64>) -> Option<Vec<katran::SelectionRef>> {
        ids.into_iter()
            .map(|id| {
                let event = self
                    .events
                    .iter()
                    .find(|event| event["id"].as_i64() == Some(id))
                    .cloned()
                    .unwrap_or_else(|| self.call(Command::Get { id }));
                Some(katran::SelectionRef {
                    id,
                    revision: event["revision"].as_str()?.to_owned(),
                })
            })
            .collect()
    }
    pub(super) fn selection_snapshot(&mut self) -> Option<Vec<katran::SelectionRef>> {
        self.snapshot_ids(self.selected_ids())
    }
    pub(super) fn copy_request(&mut self, ctx: &egui::Context) {
        if self.state["tab"] == "pods" && self.state["emoji_catalogue"] == true {
            if self.job.is_some() {
                self.message = tr(
                    "Дождитесь завершения текущей операции",
                    "Wait for the current operation to finish",
                )
                .into();
                return;
            }
            let result = self.call(Command::CopySelectionAction);
            if !result.is_null() {
                self.action_result(result);
            }
            return;
        }
        if self.pending_copy.is_some() || self.pending_copy_ids.is_some() {
            self.message = tr("Копирование уже в очереди", "Copy is already queued").into();
            return;
        }
        if self.api.is_none()
            && self.job.is_some()
            && self.selected_ids().iter().any(|id| {
                !self
                    .events
                    .iter()
                    .any(|event| event["id"].as_i64() == Some(*id))
            })
        {
            self.pending_copy_ids = Some(self.selected_ids());
            self.message = tr("Копирование в очереди", "Copy queued").into();
            return;
        }
        let Some(items) = self.selection_snapshot() else {
            self.message = tr(
                "Выбор устарел. Обновите полку и выберите объекты заново.",
                "Selection is out of date. Refresh the shelf and select the items again.",
            )
            .into();
            return;
        };
        if self.api.is_some() {
            let staged = self.call(Command::StageTransfer {
                items: items.clone(),
                cut: false,
            });
            if staged.is_null() {
                return;
            }
        }
        if self.job.is_some() {
            self.pending_copy = Some(items);
            self.message = tr("Копирование в очереди", "Copy queued").into();
        } else if self.api.is_some() {
            if self.start_job(Command::CopySelectedObjects, false, ctx.clone()) {
                self.actions = false;
            }
        } else if self.job.is_some() {
            self.pending_copy = Some(items);
            self.message = tr("Копирование в очереди", "Copy queued").into();
        } else {
            self.message = tr(
                "Не скопировано. Перезапустите Katran.",
                "Not copied. Restart Katran.",
            )
            .into();
        }
    }
    pub(super) fn take_queued_copy(&mut self) -> Option<Command> {
        let items = if let Some(ids) = self.pending_copy_ids.take() {
            match self.snapshot_ids(ids) {
                Some(items) => items,
                None => {
                    self.message = tr(
                        "Выбор устарел; копирование не выполнено",
                        "Selection is out of date; nothing was copied",
                    )
                    .into();
                    return None;
                }
            }
        } else {
            self.pending_copy.take()?
        };
        if self
            .call(Command::StageTransfer {
                items: items.clone(),
                cut: false,
            })
            .is_null()
        {
            return None;
        }
        Some(Command::CopySmart { items })
    }
    pub(super) fn run(&mut self, id: String, ctx: &egui::Context) {
        if id == "search_with_ai" {
            if self.state["tab"] == "pods"
                && self.state["emoji_catalogue"] == true
                && self.state["preview"].is_null()
            {
                self.emoji_ai_toggle(ctx);
            } else {
                self.message = tr(
                    "Поиск с ИИ доступен в каталоге эмодзи Pods",
                    "AI search is available in the Pods emoji catalogue",
                )
                .into();
            }
            return;
        }
        if id == "shelf_search" {
            if self.emoji_ai.enabled {
                self.emoji_ai_leave();
                self.query.clear();
                self.loaded_query.clear();
                self.refresh();
                ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("shelf-search")));
                self.actions = false;
                ctx.request_repaint();
                return;
            }
            self.toggle_search_focus(ctx);
            return;
        }
        if id == "restart" {
            self.restart_pending = true;
            self.actions = false;
            ctx.request_repaint();
            return;
        }
        if id == "quit" {
            self.quit_application(ctx);
            return;
        }
        if id == "paste_previous" {
            self.paste_previous(ctx);
            return;
        }
        if id == "select_all"
            && self.state["tab"] == "folder"
            && !self.state["folder_path"].is_null()
        {
            self.actions = false;
            if self.folder_page_job.is_some() {
                if self
                    .folder_page_job
                    .as_ref()
                    .is_some_and(|job| job.context != self.folder_page_context())
                {
                    self.refresh();
                }
                self.pending_folder_select_all = self.folder_page_job.is_some();
                return;
            }
            if self.loaded_query != self.query {
                self.refresh();
                self.pending_folder_select_all = self.folder_page_job.is_some();
                return;
            }
            self.start_job(Command::RunAction { id }, false, ctx.clone());
            return;
        }
        if matches!(id.as_str(), "transfer_selection" | "transfer_copy") {
            if self.job.is_some() {
                if id == "transfer_copy" {
                    self.queue_transfer_copy();
                } else {
                    self.message = tr(
                        "Дождитесь завершения текущей операции",
                        "Wait for the current operation to finish",
                    )
                    .into();
                }
                return;
            }
            let command = if id == "transfer_copy" {
                Command::CopySelection
            } else {
                Command::TransferSelection
            };
            if self.start_job(command, false, ctx.clone()) {
                self.actions = false;
                self.message = tr("Перенос объектов…", "Transferring items…").into();
            }
            return;
        }
        if id == "add_to_katran" {
            if self.start_job(Command::RunAction { id }, false, ctx.clone()) {
                self.actions = false;
                self.message = tr("Добавление в Катран…", "Adding to Katran…").into();
            }
            return;
        }
        if id == "open_with" {
            self.open_apps_request(ctx);
            return;
        }
        if id == "copy" {
            self.copy_request(ctx);
            return;
        }
        if matches!(
            id.as_str(),
            "cleanup"
                | "delete"
                | "copy_text"
                | "use_as_background"
                | "use_as_desktop_wallpaper"
                | "use_as_video_background"
                | "use_as_video_desktop_wallpaper"
                | "use_as_video_desktop_wallpaper_silent"
        ) {
            let wallpaper = matches!(
                id.as_str(),
                "use_as_desktop_wallpaper"
                    | "use_as_video_desktop_wallpaper"
                    | "use_as_video_desktop_wallpaper_silent"
            );
            if wallpaper {
                self.wallpaper_job = true;
                self.message.clear();
            }
            if self.start_job(Command::RunAction { id }, false, ctx.clone()) {
                self.actions = false;
            } else if wallpaper {
                self.wallpaper_job = false;
                if self.message.is_empty() {
                    self.message = tr(
                        "Другая операция ещё выполняется",
                        "Another operation is still running",
                    )
                    .into();
                }
                self.wallpaper_notice = Some((self.message.clone(), Instant::now()));
            }
            return;
        }
        if matches!(
            id.as_str(),
            "open" | "preview" | "quicklook" | "reveal" | "open_with"
        ) && let Some(focus) = ctx.memory(|m| m.focused())
        {
            ctx.memory_mut(|m| m.surrender_focus(focus));
        }
        let preview_action = matches!(
            id.as_str(),
            "open" | "preview" | "quicklook" | "reveal" | "open_with"
        );
        if preview_action {
            if self.preview_warmup.is_some() && matches!(id.as_str(), "open" | "quicklook") {
                let context = self.preview_request_context();
                self.pending_preview = Some((id, context));
                self.actions = false;
                ctx.request_repaint();
                return;
            }
            if self.start_job(Command::RunAction { id }, false, ctx.clone()) {
                self.actions = false;
            }
            self.state = self.call(Command::Interaction);
            return;
        }
        let sort_action = id.starts_with("sort_");
        let tab_action = id.starts_with("tab_position_")
            || matches!(
                id.as_str(),
                "tab_toggle"
                    | "tab_previous"
                    | "tab_next"
                    | "images"
                    | "screenshots"
                    | "downloads"
                    | "text"
                    | "links"
                    | "tables"
                    | "pods"
                    | "files"
            );
        let r = self.call(Command::RunAction { id });
        if sort_action && r["sort"].is_object() {
            let enabled = self.call(Command::GetSettings)["sort_notifications"] != false;
            self.sort_notice =
                enabled.then(|| (sorting_ui::confirmation(&r["sort"]), Instant::now()));
        }
        self.action_result(r);
        if preview_action {
            self.state = self.call(Command::Interaction);
        } else {
            self.state = self.call(Command::Interaction);
            if !tab_action || !self.show_preloaded_tab(ctx) {
                self.refresh();
            }
        }
    }
    pub(super) fn action_result(&mut self, r: Value) {
        if let Some(copied) = r["table_copied"].as_bool() {
            self.message = if copied {
                format!(
                    "{}: {} · {} × {}",
                    tr("Скопировано таблиц", "Tables copied"),
                    r["table_count"].as_u64().unwrap_or(1),
                    r["row_count"].as_u64().unwrap_or(0),
                    r["column_count"].as_u64().unwrap_or(0)
                )
            } else if r["text_written"] == true {
                tr(
                    "Скопированы значения с табуляциями; HTML таблицы записать не удалось",
                    "Tab-separated values copied; table HTML could not be written",
                )
                .into()
            } else if r["html_written"] == true {
                tr(
                    "Скопирована HTML-таблица; текстовый формат записать не удалось",
                    "HTML table copied; text format could not be written",
                )
                .into()
            } else if r["merged"] == false {
                format!(
                    "{}: {}",
                    tr("Таблицы не скопированы", "Tables not copied"),
                    item_failures(&r["items"])
                )
            } else {
                tr(
                    "Не удалось записать таблицы в буфер обмена",
                    "Could not write tables to the clipboard",
                )
                .into()
            };
            if let Some(warnings) = r["warnings"].as_array() {
                for warning in warnings.iter().filter_map(Value::as_str) {
                    self.message.push_str(" · ");
                    self.message.push_str(warning);
                }
            }
        }
        if r["background_applied"] == true {
            self.message = tr(
                "Изображение сохранено и установлено на фон",
                "Image saved and set as background",
            )
            .into();
        }
        if r["desktop_wallpaper_applied"] == true {
            self.message = tr(
                "Windows приняла запрос на смену обоев",
                "Windows accepted the wallpaper request",
            )
            .into();
        }
        if r["video_desktop_wallpaper_requested"] == true {
            self.message = if r["muted"] == true {
                tr(
                    "Видео без звука передано Wallpaper Engine; проверьте рабочий стол",
                    "Silent video sent to Wallpaper Engine; check the desktop",
                )
            } else {
                tr(
                    "Запрос передан Wallpaper Engine; проверьте рабочий стол",
                    "Request sent to Wallpaper Engine; check the desktop",
                )
            }
            .into();
            if let Some(error) = r["desktop_asset_error"]["message"].as_str() {
                self.message.push_str(" · ");
                self.message
                    .push_str(tr("Не сохранено в Desktop: ", "Not saved in Desktop: "));
                self.message.push_str(error);
            }
        }
        if r["sort"].is_object() {
            self.message = sorting_ui::confirmation(&r["sort"]);
        }
        if r["emoji_copy"] == true {
            let items = r["items"].as_array().map(Vec::as_slice).unwrap_or(&[]);
            let copied = items.iter().filter(|item| item["ok"] == true).count();
            let failed = items.len() - copied;
            self.message = if super::i18n::is_english() {
                format!("Added to Pods: {copied}; failed: {failed}")
            } else {
                format!("Добавлено в Pods: {copied}; ошибок: {failed}")
            };
            let clipboard = &r["clipboard"];
            match clipboard["status"].as_str() {
                Some("copied") => self.message.push_str(tr(" · Скопировано", " · Copied")),
                Some("failed") => {
                    self.message
                        .push_str(tr(" · Буфер не скопирован", " · Clipboard copy failed"));
                    if let Some(error) = clipboard["error"]["message"].as_str() {
                        self.message.push_str(": ");
                        self.message.push_str(error);
                    }
                }
                _ => self
                    .message
                    .push_str(tr(" · Буфер не изменён", " · Clipboard unchanged")),
            }
            let errors = item_failures(&r["items"]);
            if !errors.is_empty() {
                self.message.push_str(" · ");
                self.message.push_str(&errors);
            }
            return;
        }
        if let Some(copied) = r["text_copied"].as_bool() {
            self.message = if copied && r["kind"] == "link_fragment" {
                tr("Ссылки скопированы текстом", "Links copied as text").into()
            } else if copied {
                tr(
                    "Текстовые фрагменты скопированы одним текстом",
                    "Text fragments copied as one text",
                )
                .into()
            } else {
                format!(
                    "{}: {}",
                    tr("Текст не скопирован", "Text not copied"),
                    item_failures(&r["items"])
                )
            };
        }
        if r["requested"] == true {
            match r["operation"].as_str() {
                Some("open") => {
                    self.message = if r["link"] == true {
                        tr(
                            "Ссылка передана браузеру по умолчанию",
                            "Link sent to the default browser",
                        )
                        .into()
                    } else {
                        tr(
                            "Файл передан приложению по умолчанию",
                            "File sent to the default app",
                        )
                        .into()
                    }
                }
                Some("reveal") => {
                    self.message = tr("Файл показан в Проводнике", "File shown in Explorer").into()
                }
                Some("open_with") => {
                    self.message = if r["application_id"].is_string() {
                        tr(
                            "Файл передан выбранному приложению",
                            "File sent to the selected app",
                        )
                        .into()
                    } else {
                        tr(
                            "Запрошен выбор приложения Windows",
                            "Windows app picker requested",
                        )
                        .into()
                    }
                }
                _ => {}
            }
        }
        if r["cancelled"] == true {
            self.message = tr("Перетаскивание отменено", "Drag cancelled").into();
        }
        if let Some(count) = r["dragged"].as_u64() {
            self.message = format!(
                "{}: {count}",
                tr("Перетаскивание завершено", "Drag completed")
            );
            let errors = item_failures(&r["failures"]);
            if !errors.is_empty() {
                self.message.push_str(&format!(
                    " · {}: {errors}",
                    tr("не передано", "not transferred")
                ));
            }
            if let Some(warnings) = r["warnings"].as_array() {
                for warning in warnings.iter().filter_map(Value::as_str) {
                    self.message.push_str(&format!(" · {warning}"));
                }
            }
        }
        if let Some(bytes) = r["reclaimed_blob_bytes"].as_u64() {
            self.message = format!(
                "{}: {bytes} {}",
                tr("Очищено временных данных", "Temporary data cleared"),
                tr("байт", "bytes")
            );
        }
        if r["confirmation_required"] == true {
            self.pending_delete_originals = r["original_files"] == true;
            self.pending_delete_permanent = r["permanent_files"] == true;
            self.pending_delete_recycle = r["recycle_bin"] == true;
            self.pending_delete = r["ids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_i64)
                .collect();
        }
        if let Some(count) = r["deleted"].as_u64()
            && r["confirmation_required"] != true
        {
            self.message = if r["original_files"] == true {
                format!(
                    "{}: {count}{}",
                    tr("Удалено объектов", "Items deleted"),
                    cleanup_message(&r)
                )
            } else {
                format!(
                    "{}: {count}{}",
                    tr("Убрано с полки", "Removed from shelf"),
                    cleanup_message(&r)
                )
            };
            let errors = item_failures(&r["items"]);
            if !errors.is_empty() {
                self.message
                    .push_str(&format!(" · {}: {errors}", tr("не удалено", "not deleted")));
            }
        }
        if let Some(count) = r["copied"].as_u64() {
            self.message = format!("{}: {count}", tr("Скопировано", "Copied"));
            if let Some(failures) = r["failures"].as_array()
                && !failures.is_empty()
            {
                self.message.push_str(&format!(
                    " · {}: {}",
                    tr("не скопировано", "not copied"),
                    item_failures(&Value::Array(failures.clone()))
                ));
            }
            if let Some(warnings) = r["warnings"].as_array()
                && !warnings.is_empty()
            {
                self.message.push_str(&format!(
                    " · {}",
                    warnings
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("; ")
                ));
            }
        }
        if let Some(items) = r["items"].as_array()
            && items.iter().any(|i| i.get("path").is_some())
            && r.get("atomic").is_some()
        {
            self.message = format!(
                "{} {}/{}",
                tr("Сохранено", "Saved"),
                items.iter().filter(|i| i["ok"] == true).count(),
                items.len()
            );
            let errors = item_failures(&r["items"]);
            if !errors.is_empty() {
                self.message
                    .push_str(&format!(" · {}: {errors}", tr("Ошибки", "Errors")));
            }
        }
        if r["intent"] == "choose_destination" {
            self.save = true;
        }
        self.actions = false;
        if r["intent"] == "show_commands" {
            self.actions = true;
            self.focus_actions = true;
            self.action_query.clear();
            self.action_selected = 0;
        }
    }
}
