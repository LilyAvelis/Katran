//! Process-local fitted-image zoom geometry; coordinates are canvas-local points.
use crate::{ApiError, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct State {
    pub factor: f32,
    pub offset: Point,
}
impl Default for State {
    fn default() -> Self {
        Self {
            factor: 1.0,
            offset: Point::default(),
        }
    }
}
impl State {
    fn validate(viewport: Size, fitted: Size, point: Point) -> Result<()> {
        if [viewport.width, viewport.height, fitted.width, fitted.height]
            .iter()
            .any(|n| !n.is_finite() || *n <= 0.0 || *n > 1_000_000.0)
            || !point.x.is_finite()
            || !point.y.is_finite()
        {
            return Err(ApiError::new(
                "invalid_argument",
                "Expected finite positive image/canvas sizes and finite coordinates",
            ));
        }
        Ok(())
    }
    pub fn zoom(
        &mut self,
        multiplier: f32,
        anchor: Point,
        viewport: Size,
        fitted: Size,
    ) -> Result<()> {
        Self::validate(viewport, fitted, anchor)?;
        if !multiplier.is_finite() || multiplier <= 0.0 {
            return Err(ApiError::new(
                "invalid_argument",
                "Expected a finite positive zoom multiplier",
            ));
        }
        let old = self.factor;
        self.factor = (old * multiplier).clamp(1.0, 16.0);
        let ratio = self.factor / old;
        self.offset.x = anchor.x
            - viewport.width / 2.0
            - (anchor.x - viewport.width / 2.0 - self.offset.x) * ratio;
        self.offset.y = anchor.y
            - viewport.height / 2.0
            - (anchor.y - viewport.height / 2.0 - self.offset.y) * ratio;
        self.constrain(viewport, fitted);
        Ok(())
    }
    pub fn pan(&mut self, delta: Point, viewport: Size, fitted: Size) -> Result<()> {
        Self::validate(viewport, fitted, delta)?;
        self.offset.x += delta.x;
        self.offset.y += delta.y;
        self.constrain(viewport, fitted);
        Ok(())
    }
    fn constrain(&mut self, viewport: Size, fitted: Size) {
        let x = ((fitted.width * self.factor - viewport.width) / 2.0).max(0.0);
        let y = ((fitted.height * self.factor - viewport.height) / 2.0).max(0.0);
        self.offset.x = self.offset.x.clamp(-x, x);
        self.offset.y = self.offset.y.clamp(-y, y);
    }
    pub fn geometry(&mut self, viewport: Size, fitted: Size) -> Result<serde_json::Value> {
        Self::validate(viewport, fitted, Point::default())?;
        self.constrain(viewport, fitted);
        Ok(
            serde_json::json!({"factor": self.factor, "offset": self.offset,
            "left": (viewport.width - fitted.width * self.factor) / 2.0 + self.offset.x,
            "top": (viewport.height - fitted.height * self.factor) / 2.0 + self.offset.y,
            "width": fitted.width * self.factor, "height": fitted.height * self.factor}),
        )
    }
}
