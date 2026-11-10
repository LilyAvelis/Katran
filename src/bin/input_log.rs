//! Only events delivered to Katran's own egui viewports, never global hooks.
pub fn record(ctx: &eframe::egui::Context, logger: Option<&katran::diagnostics::Logger>) {
    let Some(logger) = logger else {
        return;
    };
    ctx.input(|input|{
  for event in &input.events {
   use eframe::egui::Event;
   if matches!(event,Event::PointerMoved(..)|Event::MouseMoved(..)) { continue; }
   let data=match event {
    Event::Screenshot{..}=>serde_json::json!({"event":"screenshot_result","payload":"omitted"}),
    Event::Text(text)=>serde_json::json!({"type":"text","text":text.chars().take(2048).collect::<String>(),"length":text.chars().count()}),
    Event::Paste(text)=>serde_json::json!({"type":"paste_text","text":text.chars().take(2048).collect::<String>(),"length":text.chars().count()}),
    _=>serde_json::json!({"event":format!("{event:?}")}),
   };logger.record("input",data);
  }
  for file in &input.raw.dropped_files{logger.record("drop",serde_json::json!({"path":file.path().display().to_string()}));}
 });
}
