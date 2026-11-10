//! Two-phase drag: prepare immutable files off-thread, run OLE on the window thread.
use crate::{ApiError, Result, SelectionRef, Service, copy_out, drag_out};
use base64::Engine;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);
pub(crate) struct Prepared {
    token: String,
    plan: copy_out::Plan,
    preview: Option<drag_out::Preview>,
}

pub(crate) struct DragPreparation {
    pub(crate) plan: copy_out::Plan,
    preview: Option<drag_out::Preview>,
}
pub(crate) fn prepare_owned(
    root: &std::path::Path,
    events: &[crate::Event],
    quota: u64,
    references: &[PathBuf],
) -> Result<DragPreparation> {
    let mut plan = copy_out::prepare_with_budget(&root.join("drag-cache"), events, quota)
        .map_err(|e| ApiError::new("drag_error", e))?;
    plan.protect_references(references)
        .map_err(|e| ApiError::new("drag_error", e))?;
    if plan.paths.is_empty() {
        return Err(ApiError::new(
            "drag_error",
            format!(
                "No available files: {}",
                serde_json::to_string(&plan.failures).unwrap_or_default()
            ),
        ));
    }
    // Reuse the bounded persistent image cache. Preview failure is cosmetic.
    let preview = events
        .iter()
        .find(|event| !plan.failures.iter().any(|failure| failure.id == event.id))
        .and_then(|event| {
            let rendered = crate::preview::render(root, event, 160).ok()?;
            Some(drag_out::Preview {
                width: rendered["width"].as_u64()? as u32,
                height: rendered["height"].as_u64()? as u32,
                rgba: base64::engine::general_purpose::STANDARD
                    .decode(rendered["rgba_base64"].as_str()?)
                    .ok()?,
            })
        });
    Ok(DragPreparation { plan, preview })
}

impl Service {
    /// Native boundary injection for embedded clients and synthetic API tests.
    pub fn set_drag_runner(&mut self, runner: Box<dyn drag_out::Runner>) {
        self.drag_runner = runner;
    }

    pub(crate) fn prepare_drag(&mut self, items: Vec<SelectionRef>) -> Result<Value> {
        if items.is_empty() || items.len() > 1000 {
            return Err(ApiError::new(
                "invalid_argument",
                "Expected 1..1000 objects",
            ));
        }
        let mut events = Vec::new();
        let mut unique = std::collections::HashSet::new();
        for item in items {
            let event = self.get(item.id)?;
            if event.revision != item.revision {
                return Err(ApiError::new("stale_selection", "Drag selection changed"));
            }
            if unique.insert(item.id) {
                events.push(event);
            }
        }
        // Retire an abandoned unpublished gesture before budgeting a new one.
        // A previously successful drop is independent and remains protected.
        self.pending_drag = None;
        let references = self.reference_paths()?;
        let prepared = prepare_owned(&self.root, &events, self.quota, &references)?;
        self.commit_drag_preparation(prepared)
    }

    pub(crate) fn commit_drag_preparation(&mut self, prepared: DragPreparation) -> Result<Value> {
        let DragPreparation { plan, preview } = prepared;
        let token = format!(
            "{}-{}",
            std::process::id(),
            NEXT_TOKEN.fetch_add(1, Ordering::Relaxed)
        );
        let result = json!({"prepared": true, "drag_token": token, "count": plan.paths.len(), "failures": plan.failures});
        // Replacing a preparation drops only its unpublished owned files.
        self.pending_drag = Some(Prepared {
            token,
            plan,
            preview,
        });
        Ok(result)
    }

    fn take_drag(&mut self, token: &str) -> Result<Prepared> {
        if self.pending_drag.as_ref().is_none_or(|p| p.token != token) {
            return Err(ApiError::new(
                "stale_drag",
                "Drag preparation expired or was already consumed",
            ));
        }
        Ok(self.pending_drag.take().unwrap())
    }

    pub(crate) fn cancel_prepared_drag(&mut self, token: &str) -> Result<Value> {
        drop(self.take_drag(token)?);
        Ok(json!({"dropped": false, "cancelled": true}))
    }

    pub(crate) fn run_prepared_drag(&mut self, token: &str, owner_window: u64) -> Result<Value> {
        let prepared = self.take_drag(token)?;
        if let Some(log) = &self.diagnostics {
            log.record(
                "drag_native_begin",
                json!({"count": prepared.plan.paths.len(), "owner_window": owner_window}),
            );
        }
        let mut sink = drag_out::RunnerSink {
            runner: self.drag_runner.as_mut(),
            preview: prepared.preview.as_ref(),
            owner_window,
        };
        let result = copy_out::commit(&prepared.plan, &mut sink);
        if let Some(log) = &self.diagnostics {
            log.record(
                "drag_native_end",
                json!({"success": result.is_ok(), "error": result.as_ref().err()}),
            );
        }
        match result {
            Ok(receipt) => Ok(json!({"dropped": true, "dragged": receipt.count,
                "failures": receipt.failures, "warnings": receipt.warnings})),
            Err(e) if e == "drag_cancelled" => Ok(json!({"dropped": false, "cancelled": true})),
            Err(e) => Err(ApiError::new("drag_error", e)),
        }
    }
}
