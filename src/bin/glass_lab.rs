//! Opt-in visual lab: no shelf, clipboard listener, hotkeys or profile are opened.
use super::i18n::tr;
use avilon_egui::optics::{LensOptions, render_lens};
use eframe::egui;

fn source() -> egui::ColorImage {
    let rgba = image::load_from_memory(include_bytes!("../../assets/themes/lilies/lily-1.png"))
        .expect("bundled lab image")
        .resize_exact(512, 288, image::imageops::FilterType::Triangle)
        .into_rgba8();
    egui::ColorImage::from_rgba_unmultiplied([512, 288], rgba.as_raw())
}
pub(super) fn export() -> Result<(), String> {
    let result =
        render_lens(&source(), [512, 288], LensOptions::default()).map_err(|e| e.to_string())?;
    let bytes: Vec<u8> = result
        .pixels
        .iter()
        .flat_map(|p| p.to_srgba_unmultiplied())
        .collect();
    std::fs::create_dir_all("target").map_err(|e| e.to_string())?;
    image::save_buffer(
        "target/avilon-glass-lab.png",
        &bytes,
        512,
        288,
        image::ColorType::Rgba8,
    )
    .map_err(|e| e.to_string())
}
struct Lab {
    source: egui::ColorImage,
    original: egui::TextureHandle,
    result: egui::TextureHandle,
    options: LensOptions,
    elapsed: std::time::Duration,
    dirty: bool,
}
impl Lab {
    fn new(ctx: &egui::Context) -> Self {
        let source = source();
        let original = ctx.load_texture("lab-source", source.clone(), Default::default());
        let result = ctx.load_texture("lab-result", source.clone(), Default::default());
        let mut lab = Self {
            source,
            original,
            result,
            options: LensOptions::default(),
            elapsed: Default::default(),
            dirty: true,
        };
        lab.refresh();
        lab
    }
    fn refresh(&mut self) {
        let start = std::time::Instant::now();
        if let Ok(result) = render_lens(&self.source, [512, 288], self.options) {
            self.result.set(result, egui::TextureOptions::LINEAR);
            self.dirty = false;
        }
        self.elapsed = start.elapsed();
    }
}
impl eframe::App for Lab {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        ui.heading("Avilon · Optical glass lab");
        ui.label(tr(
            "Эксперимент над лилиями внутри окна. CPU reference; без захвата рабочего стола.",
            "An experiment with lilies inside the window. CPU reference; no desktop capture.",
        ));
        self.dirty |= ui
            .add(
                egui::Slider::new(&mut self.options.refraction, 0.0..=64.0)
                    .text(tr("Преломление · px", "Refraction · px")),
            )
            .changed();
        self.dirty |= ui
            .add(
                egui::Slider::new(&mut self.options.blur, 0.0..=8.0)
                    .text(tr("Размытие · px", "Blur · px")),
            )
            .changed();
        self.dirty |= ui
            .add(
                egui::Slider::new(&mut self.options.chromatic, 0.0..=1.0)
                    .text(tr("Разделение цветов", "Chromatic separation")),
            )
            .changed();
        self.dirty |= ui
            .add(
                egui::Slider::new(&mut self.options.highlight, 0.0..=1.0)
                    .text(tr("Краевой свет", "Edge highlight")),
            )
            .changed();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.dirty,
                    egui::Button::new(tr("Применить параметры", "Apply settings")),
                )
                .clicked()
            {
                self.refresh();
            }
            if ui.button(tr("Сбросить", "Reset")).clicked() {
                self.options = LensOptions::default();
                self.refresh();
            }
            ui.label(format!(
                "{}: {:.1} ms · {}",
                tr("Расчёт", "Render"),
                self.elapsed.as_secs_f64() * 1000.0,
                tr("кэш до изменения", "cached until changed")
            ));
        });
        ui.label(if self.dirty {
            tr(
                "Изменения ещё не применены",
                "Changes have not been applied",
            )
        } else {
            tr(
                "Исходник слева · оптическая линза справа",
                "Source on the left · optical lens on the right",
            )
        });
        ui.columns(2, |columns| {
            columns[0]
                .add(egui::Image::new(&self.original).fit_to_exact_size(egui::vec2(420.0, 236.25)));
            columns[1]
                .add(egui::Image::new(&self.result).fit_to_exact_size(egui::vec2(420.0, 236.25)));
        });
    }
}
pub(super) fn run() -> eframe::Result {
    eframe::run_native(
        "Avilon glass lab",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([940.0, 520.0]),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(Lab::new(&cc.egui_ctx)))),
    )
}
