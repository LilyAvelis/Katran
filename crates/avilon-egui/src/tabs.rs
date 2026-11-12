//! Neutral measured pill geometry and retargetable selection motion.
use egui::{self, Color32, Rect, Shape, Stroke};

/// Follows measured rectangles and retargets from its current position without a queue.
/// A missing target clears the indicator; the host decides when this is appropriate.
#[derive(Clone, Default)]
pub struct Indicator {
    pub rect: Option<Rect>,
    pub active: String,
}
impl Indicator {
    pub fn update(&mut self, active: &str, target: Option<Rect>, dt: f32, duration: f32) -> bool {
        let Some(target) = target else {
            self.rect = None;
            self.active = active.to_owned();
            return false;
        };
        let Some(current) = self.rect else {
            self.rect = Some(target);
            self.active = active.to_owned();
            return false;
        };
        let factor = if duration <= 0.0 {
            1.0
        } else {
            1.0 - (-dt.max(0.0) * 5.0 / duration).exp()
        };
        let next = Rect::from_min_max(
            current.min + (target.min - current.min) * factor,
            current.max + (target.max - current.max) * factor,
        );
        let moving = (next.min - target.min).length_sq() > 0.25
            || (next.max - target.max).length_sq() > 0.25;
        self.rect = Some(if moving { next } else { target });
        self.active = active.to_owned();
        moving
    }
}
/// Geometry derived from the actual widget rectangle; rendering/optics are host-owned.
pub struct Pill {
    pub rect: Rect,
    pub radius: f32,
    pub highlight: Rect,
}
impl Pill {
    pub fn minimum_size(label_width: f32, horizontal_padding: f32, height: f32) -> egui::Vec2 {
        egui::vec2(label_width + 2.0 * horizontal_padding, height)
    }
    pub fn new(widget_rect: Rect) -> Self {
        let rect = widget_rect.shrink(0.5);
        let radius = rect.height() / 2.0;
        let highlight = Rect::from_min_max(
            rect.min + egui::vec2(radius * 0.55, 2.0),
            egui::pos2(rect.max.x - radius * 0.55, rect.min.y + 3.0),
        );
        Self {
            rect,
            radius,
            highlight,
        }
    }
    pub fn body(&self, color: Color32) -> Shape {
        Shape::rect_filled(self.rect, self.radius, color)
    }
    pub fn rim(&self, color: Color32) -> Shape {
        Shape::rect_stroke(
            self.rect,
            self.radius,
            Stroke::new(1.0, color),
            egui::StrokeKind::Inside,
        )
    }
    pub fn shine(&self, color: Color32) -> Shape {
        Shape::rect_filled(self.highlight, 1.0, color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn indicator_redirects_and_clears() {
        let r = |x| Rect::from_min_size(egui::pos2(x, 0.0), egui::vec2(40.0, 24.0));
        let mut i = Indicator::default();
        assert!(!i.update("a", Some(r(0.0)), 0.016, 0.2));
        assert!(i.update("b", Some(r(200.0)), 0.016, 0.2));
        let previous = i.rect.unwrap().left();
        assert!(i.update("c", Some(r(0.0)), 0.016, 0.2));
        assert!(i.rect.unwrap().left() < previous);
        assert!(!i.update("other", None, 0.016, 0.2));
        assert!(!i.update("c", Some(r(70.0)), 0.016, 0.2));
        assert_eq!(i.rect, Some(r(70.0)));
    }
    #[test]
    fn pill_uses_measured_widget_and_supplied_width() {
        assert_eq!(Pill::minimum_size(56.0, 12.0, 32.0), egui::vec2(80.0, 32.0));
        let p = Pill::new(Rect::from_min_size(
            egui::pos2(10.0, 20.0),
            egui::vec2(80.0, 32.0),
        ));
        assert_eq!(
            p.rect,
            Rect::from_min_max(egui::pos2(10.5, 20.5), egui::pos2(89.5, 51.5))
        );
        assert_eq!(p.radius, 15.5);
        assert!(p.rect.contains_rect(p.highlight));
    }
}
