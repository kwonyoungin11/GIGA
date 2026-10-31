use arch3d_core::strata_tokens::{
    metrics::{SPLASH_DURATION_SEC, SPLASH_HEIGHT, SPLASH_WIDTH},
    tokens::{ACTION, EDGE, GROUND, INK, MUTED, WASH},
};
use eframe::egui;
use image::{Rgba, RgbaImage};

/// 스플래시 화면 상태 (`700 × 400 px`, `3.0초` 지층 등고선 페이드인).
#[derive(Debug, Clone)]
pub struct SplashState {
    pub elapsed_sec: f32,
    pub duration_sec: f32,
    pub checklist: [(&'static str, bool); 4],
}

impl Default for SplashState {
    fn default() -> Self {
        Self {
            elapsed_sec: 0.0,
            duration_sec: SPLASH_DURATION_SEC,
            checklist: [
                ("GPU 16B 정점 파이프라인 및 한지 렌더러 초기화", true),
                ("Stackless LBVH 다중 스케일 곡률 적분 엔진 준비", true),
                ("6대 발굴 유물군(완형·편·구슬·기와·석기·금속) 도법 로드", true),
                ("작업 공간(.darch / .ap2) 및 1mm=5px 정밀 스케일 검증", true),
            ],
        }
    }
}

impl SplashState {
    pub fn is_finished(&self) -> bool {
        self.elapsed_sec >= self.duration_sec
    }

    pub fn progress_ratio(&self) -> f32 {
        (self.elapsed_sec / self.duration_sec).clamp(0.0, 1.0)
    }

    /// `egui` 중앙 모달/오버레이로 `700 × 400 px` 스플래시 카드를 렌더링한다.
    pub fn show_egui(&mut self, ctx: &egui::Context, dt: f32) {
        self.elapsed_sec += dt;
        let ratio = self.progress_ratio();
        let sw = SPLASH_WIDTH as f32;
        let sh = SPLASH_HEIGHT as f32;

        egui::Area::new(egui::Id::new("strata_splash_overlay"))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let rect_size = egui::vec2(sw, sh);
                let (rect, _) = ui.allocate_exact_size(rect_size, egui::Sense::hover());
                let painter = ui.painter_at(rect);

                let bg = egui::Color32::from_rgb(GROUND.rgb[0], GROUND.rgb[1], GROUND.rgb[2]);
                let border = egui::Color32::from_rgb(EDGE.rgb[0], EDGE.rgb[1], EDGE.rgb[2]);
                let ink = egui::Color32::from_rgb(INK.rgb[0], INK.rgb[1], INK.rgb[2]);
                let muted = egui::Color32::from_rgb(MUTED.rgb[0], MUTED.rgb[1], MUTED.rgb[2]);
                let terra = egui::Color32::from_rgb(ACTION.rgb[0], ACTION.rgb[1], ACTION.rgb[2]);
                let wash = egui::Color32::from_rgb(WASH.rgb[0], WASH.rgb[1], WASH.rgb[2]);

                painter.rect_filled(rect, 4.0, bg);
                painter.rect_stroke(
                    rect,
                    4.0,
                    egui::Stroke::new(1.5_f32, border),
                    egui::StrokeKind::Inside,
                );

                // 지층(Stratigraphy) 수평 등고선 5줄 애니메이션
                for layer_idx in 0..5 {
                    let y_off = 68.0 + (layer_idx as f32) * 14.0;
                    let mut pts = Vec::with_capacity(36);
                    for ix in 0..36 {
                        let t = (ix as f32) / 35.0;
                        let x = rect.left() + 36.0 + t * (sw - 72.0);
                        let wave = ((t * std::f32::consts::TAU + layer_idx as f32 * 0.7 + ratio * 2.0).sin()) * 4.5;
                        pts.push(egui::pos2(x, rect.top() + y_off + wave));
                    }
                    let stroke_col = if layer_idx == 2 { terra } else { border };
                    painter.add(egui::Shape::line(pts, egui::Stroke::new(1.2_f32, stroke_col)));
                }

                // 타이틀 & 부제
                painter.text(
                    egui::pos2(rect.left() + 40.0, rect.top() + 165.0),
                    egui::Align2::LEFT_TOP,
                    "STRATA",
                    egui::FontId::proportional(24.0),
                    ink,
                );
                painter.text(
                    egui::pos2(rect.left() + 40.0, rect.top() + 198.0),
                    egui::Align2::LEFT_TOP,
                    "동국유물실측 · 고고학 3D 정밀 실측 · 단면 · 탁본 작업대 (Rust Engine v2.0)",
                    egui::FontId::proportional(13.5),
                    muted,
                );

                // 체크리스트 4항목
                for (i, (label, done)) in self.checklist.iter().enumerate() {
                    let cy = rect.top() + 240.0 + (i as f32) * 24.0;
                    let mark = if *done { "✓" } else { "·" };
                    painter.text(
                        egui::pos2(rect.left() + 44.0, cy),
                        egui::Align2::LEFT_TOP,
                        format!("{mark}  {label}"),
                        egui::FontId::proportional(12.0),
                        ink,
                    );
                }

                // 하단 진행 막대 (4px 테라코타)
                let bar_bg = egui::Rect::from_min_size(
                    egui::pos2(rect.left() + 40.0, rect.bottom() - 32.0),
                    egui::vec2(sw - 80.0, 4.0),
                );
                painter.rect_filled(bar_bg, 2.0, wash);
                let bar_fg = egui::Rect::from_min_size(
                    bar_bg.min,
                    egui::vec2(bar_bg.width() * ratio, 4.0),
                );
                painter.rect_filled(bar_fg, 2.0, terra);
            });
    }

    /// 헤드리스 검증 투어(`--screenshot-tour`)용 `700 × 400 px` 스플래시 PNG 이미지를 생성한다.
    pub fn render_offscreen_image() -> RgbaImage {
        let w = SPLASH_WIDTH;
        let h = SPLASH_HEIGHT;
        let mut img = RgbaImage::from_pixel(w, h, Rgba([GROUND.rgb[0], GROUND.rgb[1], GROUND.rgb[2], 255]));

        for x in 0..w {
            for dy in 0..2 {
                img.put_pixel(x, dy, Rgba([209, 207, 196, 255]));
                img.put_pixel(x, h - 1 - dy, Rgba([209, 207, 196, 255]));
            }
        }
        for y in 0..h {
            for dx in 0..2 {
                img.put_pixel(dx, y, Rgba([209, 207, 196, 255]));
                img.put_pixel(w - 1 - dx, y, Rgba([209, 207, 196, 255]));
            }
        }

        for layer_idx in 0..5 {
            let base_y = 75.0 + (layer_idx as f64) * 18.0;
            let col = if layer_idx == 2 {
                Rgba([ACTION.rgb[0], ACTION.rgb[1], ACTION.rgb[2], 255])
            } else {
                Rgba([185, 182, 170, 255])
            };
            for x in 40..(w - 40) {
                let t = (x as f64) / (w as f64);
                let wave = (t * 12.56 + (layer_idx as f64) * 0.9).sin() * 5.0;
                let py = (base_y + wave).round().clamp(10.0, (h - 10) as f64) as u32;
                img.put_pixel(x, py, col);
                if py + 1 < h {
                    img.put_pixel(x, py + 1, col);
                }
            }
        }

        for x in 40..(w - 40) {
            for y in (h - 36)..(h - 31) {
                img.put_pixel(x, y, Rgba([ACTION.rgb[0], ACTION.rgb[1], ACTION.rgb[2], 255]));
            }
        }

        img
    }
}
