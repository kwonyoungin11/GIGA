use arch3d_core::strata_tokens::tokens::{ACTION, EDGE, GROUND, INK, MUTED, WASH};
use arch3d_core::{SurfaceRegion, TriMesh};
use arch3d_orient::RimGapChart360;
use eframe::egui;

/// 3D 뷰포트 표시 옵션 (`DESIGN_SPEC.md` 3.3).
#[derive(Debug, Clone)]
pub struct View3DPaneState {
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub zoom: f32,
    pub show_cutting_frame: bool,
    pub show_concentric_scope: bool,
    pub show_3way_segmentation: bool,
    pub xray_inner_mode: bool,
    pub active_section_angle_deg: f64,
}

impl Default for View3DPaneState {
    fn default() -> Self {
        Self {
            yaw_deg: 24.0,
            pitch_deg: 16.0,
            zoom: 1.0,
            show_cutting_frame: true,
            show_concentric_scope: true,
            show_3way_segmentation: false,
            xray_inner_mode: false,
            active_section_angle_deg: 0.0,
        }
    }
}

/// 3D 뷰포트(`27%`) 및 하단 `0° ~ 360°` 구연 결손 띠(`28px`)를 렌더링한다.
pub fn render_view3d_pane_egui(
    ui: &mut egui::Ui,
    mesh: &TriMesh,
    region_labels: Option<&[SurfaceRegion]>,
    rim_gap_chart: Option<&RimGapChart360>,
    best_angle_deg: f64,
    state: &mut View3DPaneState,
) {
    let avail = ui.available_size();
    let gap_bar_h = 28.0;
    let canvas_h = (avail.y - gap_bar_h - 4.0).max(120.0);

    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(avail.x, canvas_h), egui::Sense::click_and_drag());
    if response.dragged() {
        let delta = response.drag_delta();
        state.yaw_deg = (state.yaw_deg + delta.x * 0.6).rem_euclid(360.0);
        state.pitch_deg = (state.pitch_deg + delta.y * 0.4).clamp(-85.0, 85.0);
    }

    let painter = ui.painter_at(rect);
    let wash = egui::Color32::from_rgb(WASH.rgb[0], WASH.rgb[1], WASH.rgb[2]);
    let border = egui::Color32::from_rgb(EDGE.rgb[0], EDGE.rgb[1], EDGE.rgb[2]);
    let ink = egui::Color32::from_rgb(INK.rgb[0], INK.rgb[1], INK.rgb[2]);
    let muted = egui::Color32::from_rgb(MUTED.rgb[0], MUTED.rgb[1], MUTED.rgb[2]);
    let terra = egui::Color32::from_rgb(ACTION.rgb[0], ACTION.rgb[1], ACTION.rgb[2]);

    painter.rect_filled(rect, 2.0, wash);
    painter.rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(1.0_f32, border),
        egui::StrokeKind::Inside,
    );

    let center = rect.center();
    let ext = mesh.bbox.extents();
    let max_span = ext[0].max(ext[1]).max(ext[2]).max(1.0);
    let scale = ((rect.width().min(rect.height()) * 0.72) as f64 / max_span) * (state.zoom as f64);

    let yaw_rad = (state.yaw_deg as f64).to_radians();
    let pitch_rad = (state.pitch_deg as f64).to_radians();
    let (sy, cy) = yaw_rad.sin_cos();
    let (sp, cp) = pitch_rad.sin_cos();

    let m_cx = (mesh.bbox.min[0] + mesh.bbox.max[0]) * 0.5;
    let m_cy = (mesh.bbox.min[1] + mesh.bbox.max[1]) * 0.5;
    let m_cz = (mesh.bbox.min[2] + mesh.bbox.max[2]) * 0.5;

    let stride = (mesh.triangles.len() / 1600).max(1);
    for (t_idx, tri) in mesh.triangles.iter().enumerate().step_by(stride) {
        let p0 = mesh.positions[tri[0] as usize];
        let p1 = mesh.positions[tri[1] as usize];
        let p2 = mesh.positions[tri[2] as usize];

        let proj = |p: [f64; 3]| -> egui::Pos2 {
            let dx = p[0] - m_cx;
            let dy = p[1] - m_cy;
            let dz = p[2] - m_cz;
            let rx = dx * cy - dy * sy;
            let ry = dx * sy + dy * cy;
            let rz = dz * cp - ry * sp;
            egui::pos2(
                center.x + (rx * scale) as f32,
                center.y - (rz * scale) as f32,
            )
        };

        let s0 = proj(p0);
        let s1 = proj(p1);
        let s2 = proj(p2);

        let col = if state.show_3way_segmentation {
            if let Some(labels) = region_labels {
                match labels.get(tri[0] as usize).copied().unwrap_or(SurfaceRegion::OuterWall) {
                    SurfaceRegion::OuterWall => egui::Color32::from_rgb(47, 107, 72),
                    SurfaceRegion::InnerWall => egui::Color32::from_rgb(56, 98, 138),
                    SurfaceRegion::FractureZone => terra,
                }
            } else {
                ink
            }
        } else if state.xray_inner_mode {
            egui::Color32::from_rgba_unmultiplied(181, 87, 58, 135)
        } else if t_idx % 3 == 0 {
            ink
        } else {
            muted
        };

        painter.line_segment([s0, s1], egui::Stroke::new(0.7_f32, col));
        painter.line_segment([s1, s2], egui::Stroke::new(0.7_f32, col));
    }

    if state.show_cutting_frame {
        let frame_w = rect.width() * 0.68;
        let frame_h = rect.height() * 0.78;
        let frame_rect = egui::Rect::from_center_size(center, egui::vec2(frame_w, frame_h));
        painter.rect_stroke(
            frame_rect,
            1.0,
            egui::Stroke::new(1.3_f32, terra),
            egui::StrokeKind::Inside,
        );
        painter.line_segment(
            [
                egui::pos2(center.x, frame_rect.top()),
                egui::pos2(center.x, frame_rect.bottom()),
            ],
            egui::Stroke::new(1.2_f32, terra),
        );
    }

    if state.show_concentric_scope {
        for r_factor in [0.16_f32, 0.28_f32] {
            painter.circle_stroke(
                center,
                rect.width().min(rect.height()) * r_factor,
                egui::Stroke::new(0.8_f32, border),
            );
        }
    }

    painter.text(
        egui::pos2(rect.left() + 8.0, rect.top() + 6.0),
        egui::Align2::LEFT_TOP,
        format!(
            "3D 정위 뷰포트 (고정 절단틀 ψ={:.0}° · 추천 최적각 ★{:.0}°)",
            state.active_section_angle_deg, best_angle_deg
        ),
        egui::FontId::proportional(11.5),
        ink,
    );

    ui.add_space(2.0);
    let (bar_rect, bar_resp) =
        ui.allocate_exact_size(egui::vec2(avail.x, gap_bar_h), egui::Sense::click());
    if bar_resp.clicked() {
        state.active_section_angle_deg = best_angle_deg;
    }
    let bar_painter = ui.painter_at(bar_rect);
    let hanji = egui::Color32::from_rgb(GROUND.rgb[0], GROUND.rgb[1], GROUND.rgb[2]);
    bar_painter.rect_filled(bar_rect, 2.0, hanji);
    bar_painter.rect_stroke(
        bar_rect,
        2.0,
        egui::Stroke::new(1.0_f32, border),
        egui::StrokeKind::Inside,
    );

    if let Some(chart) = rim_gap_chart {
        let n_bins = chart.gap_mm_per_bin.len().max(1);
        let bin_w = bar_rect.width() / (n_bins as f32);
        for (i, &gap_mm) in chart.gap_mm_per_bin.iter().enumerate() {
            let x0 = bar_rect.left() + (i as f32) * bin_w;
            let seg_rect = egui::Rect::from_min_size(
                egui::pos2(x0, bar_rect.top() + 2.0),
                egui::vec2(bin_w.max(1.0), bar_rect.height() - 4.0),
            );
            let c = if gap_mm.is_none_or(|g| g < 5.0) {
                egui::Color32::from_rgb(218, 226, 216)
            } else {
                egui::Color32::from_rgb(235, 205, 198)
            };
            bar_painter.rect_filled(seg_rect, 0.0, c);
        }
    }

    bar_painter.text(
        egui::pos2(bar_rect.left() + 8.0, bar_rect.center().y),
        egui::Align2::LEFT_CENTER,
        format!(
            "0°~360° 구연 잔존·결손 띠  [★ 최적 단면각 {:.0}° 즉시 이동]",
            best_angle_deg
        ),
        egui::FontId::proportional(11.0),
        ink,
    );
}
