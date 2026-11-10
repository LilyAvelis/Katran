//! Process-local table canvas geometry in screen points; no UI or storage effects.
use crate::{
    ApiError, Result,
    preview_view::{Point, Size},
};
use serde::Serialize;

pub const MIN_FACTOR: f32 = 0.001;
pub const MAX_FACTOR: f32 = 4.0;
#[derive(Clone, Copy, Debug, Serialize)]
pub struct State {
    pub factor: f32,
    pub scroll: Point,
}
impl Default for State {
    fn default() -> Self {
        Self {
            factor: 1.0,
            scroll: Point::default(),
        }
    }
}
impl State {
    fn validate(viewport: Size, content: Size, point: Point) -> Result<()> {
        if [
            viewport.width,
            viewport.height,
            content.width,
            content.height,
        ]
        .iter()
        .any(|v| !v.is_finite() || *v <= 0.0 || *v > 1_000_000.0)
            || [point.x, point.y]
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 1_000_000.0)
        {
            return Err(ApiError::new(
                "invalid_argument",
                "Expected finite positive table/canvas sizes and bounded coordinates",
            ));
        }
        Ok(())
    }
    fn constrain(&mut self, viewport: Size, content: Size) {
        self.scroll.x = self
            .scroll
            .x
            .clamp(0.0, (content.width * self.factor - viewport.width).max(0.0));
        self.scroll.y = self.scroll.y.clamp(
            0.0,
            (content.height * self.factor - viewport.height).max(0.0),
        );
    }
    pub fn zoom(
        &mut self,
        multiplier: f32,
        anchor: Point,
        viewport: Size,
        content: Size,
    ) -> Result<()> {
        Self::validate(viewport, content, anchor)?;
        if !multiplier.is_finite() || multiplier <= 0.0 {
            return Err(ApiError::new(
                "invalid_argument",
                "Expected finite positive table zoom multiplier",
            ));
        }
        let next = (self.factor * multiplier).clamp(MIN_FACTOR, MAX_FACTOR);
        let ratio = next / self.factor;
        self.scroll.x = (self.scroll.x + anchor.x) * ratio - anchor.x;
        self.scroll.y = (self.scroll.y + anchor.y) * ratio - anchor.y;
        self.factor = next;
        self.constrain(viewport, content);
        Ok(())
    }
    pub fn pan(&mut self, delta: Point, viewport: Size, content: Size) -> Result<()> {
        Self::validate(viewport, content, delta)?;
        self.scroll.x -= delta.x;
        self.scroll.y -= delta.y;
        self.constrain(viewport, content);
        Ok(())
    }
    pub fn fit(&mut self, viewport: Size, content: Size) -> Result<()> {
        Self::validate(viewport, content, Point::default())?;
        self.factor = (viewport.width / content.width)
            .min(viewport.height / content.height)
            .clamp(MIN_FACTOR, MAX_FACTOR);
        self.scroll = Point::default();
        Ok(())
    }
    pub fn geometry(&mut self, viewport: Size, content: Size) -> Result<serde_json::Value> {
        Self::validate(viewport, content, Point::default())?;
        self.constrain(viewport, content);
        Ok(
            serde_json::json!({"factor":self.factor,"scroll":self.scroll,"width":content.width*self.factor,"height":content.height*self.factor}),
        )
    }
}

impl crate::Service {
    pub(crate) fn require_table_view(&self) -> Result<()> {
        let id = self
            .interaction
            .preview
            .ok_or_else(|| ApiError::new("preview_closed", "Open a table preview first"))?;
        let table: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM table_fragments WHERE event_id=?1)",
            [id],
            |row| row.get(0),
        )?;
        if !table {
            return Err(ApiError::new(
                "not_table_preview",
                "The current preview is not a table",
            ));
        }
        Ok(())
    }
}
