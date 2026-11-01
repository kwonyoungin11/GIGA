use super::config::*;
use eframe::egui;
use strata_core::strata_tokens::tokens::{ACTION, INK, PROFILE_RED, SELECT_BLUE};

/// 탁본 이미지 바로 밑에 가로형 정밀 미세조정 휠 위젯을 그리고 드래그/클릭 스텝을 반환한다 (`Image 14`, `Image 3: 감도 +30% 향상`).
pub(crate) fn draw_rubbing_fine_wheel(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    wheel_rect: egui::Rect,
    id_str: &str,
    title_kr: &str,
    value_str: &str,
    phase_0_to_1: f32,
) -> i32 {
    let resp = ui.interact(
        wheel_rect,
        egui::Id::new(id_str),
        egui::Sense::click_and_drag(),
    );
    let mut steps = 0i32;
    if resp.clicked() {
        steps = 1;
    } else if resp.dragged() {
        // Image 3 ("감도를 30%올려라"): 기존 (1 / 2.2) 대비 +30% 향상된 (1.30 / 2.2) 감도 적용!
        steps = (resp.drag_delta().x * (1.30 / 2.2)).round() as i32;
    }

    let dial_fill = egui::Color32::from_rgba_unmultiplied(236, 233, 224, 242);
    let dial_stroke = egui::Color32::from_rgb(174, 170, 158);
    let terra = egui::Color32::from_rgb(ACTION.rgb[0], ACTION.rgb[1], ACTION.rgb[2]);
    let ink = egui::Color32::from_rgb(INK.rgb[0], INK.rgb[1], INK.rgb[2]);
    let tick_col = egui::Color32::from_rgb(154, 150, 140);
    let tick_major = egui::Color32::from_rgb(98, 94, 86);

    painter.rect_filled(wheel_rect, 8.0, dial_fill);
    painter.rect_stroke(
        wheel_rect,
        8.0,
        egui::Stroke::new(
            if resp.hovered() { 1.4_f32 } else { 1.0_f32 },
            if resp.hovered() { terra } else { dial_stroke },
        ),
        egui::StrokeKind::Inside,
    );

    let n_ticks = 22usize;
    for ti in 0..=n_ticks {
        let frac = ((ti as f32 + phase_0_to_1).rem_euclid(n_ticks as f32) / (n_ticks as f32))
            .clamp(0.0, 1.0);
        let tx = wheel_rect.left() + 7.0 + frac * (wheel_rect.width() - 14.0);
        let is_mid = ti == n_ticks / 2;
        let th = if is_mid {
            10.0
        } else if ti % 4 == 0 {
            7.5
        } else {
            5.0
        };
        painter.line_segment(
            [
                egui::pos2(tx, wheel_rect.center().y - th * 0.5),
                egui::pos2(tx, wheel_rect.center().y + th * 0.5),
            ],
            egui::Stroke::new(
                if is_mid { 1.2_f32 } else { 0.8_f32 },
                if is_mid { tick_major } else { tick_col },
            ),
        );
    }

    let lbl_rect = egui::Rect::from_min_size(
        egui::pos2(wheel_rect.left(), wheel_rect.bottom() + 1.5),
        egui::vec2(wheel_rect.width(), 13.0),
    );
    painter.rect_filled(
        lbl_rect,
        2.0,
        egui::Color32::from_rgba_unmultiplied(250, 249, 245, 225),
    );
    painter.text(
        lbl_rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("{} {}", title_kr, value_str),
        egui::FontId::proportional(8.8),
        ink,
    );

    steps
}

/// Image 4 ("아이콘형태 ui로 잘보이게하라"):
/// 단면선(0) · 외곽선(1) · 횡단면(2) · 치수선(3) 카드 좌측에 직관적인 고시인성 벡터 아이콘을 제도한다.
pub(crate) fn draw_line_style_vector_icon(
    painter: &egui::Painter,
    icon_rect: egui::Rect,
    idx: usize,
    stroke_col: egui::Color32,
) {
    let bg =
        egui::Color32::from_rgba_unmultiplied(stroke_col.r(), stroke_col.g(), stroke_col.b(), 28);
    painter.rect_filled(icon_rect, 3.5, bg);
    painter.rect_stroke(
        icon_rect,
        3.5,
        egui::Stroke::new(1.0_f32, stroke_col),
        egui::StrokeKind::Inside,
    );
    let c = icon_rect.center();
    let r = (icon_rect.width().min(icon_rect.height()) * 0.36).max(4.5);
    match idx {
        0 => {
            // 단면선 아이콘: 중심축 + 우측 단면 두께 프로파일
            painter.line_segment(
                [egui::pos2(c.x, c.y - r), egui::pos2(c.x, c.y + r)],
                egui::Stroke::new(1.0_f32, stroke_col),
            );
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(c.x + r * 0.85, c.y - r * 0.85),
                    egui::pos2(c.x + r * 0.65, c.y + r * 0.25),
                    egui::pos2(c.x + r * 0.25, c.y + r * 0.85),
                    egui::pos2(c.x, c.y + r * 0.85),
                ],
                egui::Stroke::new(2.0_f32, stroke_col),
            ));
        }
        1 => {
            // 외곽선 아이콘: 토기 좌우 대칭 외곽 입면 윤곽
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(c.x - r * 0.85, c.y - r * 0.80),
                    egui::pos2(c.x - r * 0.65, c.y + r * 0.25),
                    egui::pos2(c.x - r * 0.35, c.y + r * 0.80),
                    egui::pos2(c.x + r * 0.35, c.y + r * 0.80),
                    egui::pos2(c.x + r * 0.65, c.y + r * 0.25),
                    egui::pos2(c.x + r * 0.85, c.y - r * 0.80),
                ],
                egui::Stroke::new(1.7_f32, stroke_col),
            ));
        }
        2 => {
            // 횡단면 아이콘: 동심원 + 수평 절단선
            painter.circle_stroke(c, r * 0.85, egui::Stroke::new(1.4_f32, stroke_col));
            painter.circle_stroke(c, r * 0.52, egui::Stroke::new(1.0_f32, stroke_col));
            painter.line_segment(
                [egui::pos2(c.x - r, c.y), egui::pos2(c.x + r, c.y)],
                egui::Stroke::new(1.3_f32, stroke_col),
            );
        }
        _ => {
            // 치수선 아이콘: 양방향 치수 화살표 + 양끝 보조선
            painter.line_segment(
                [
                    egui::pos2(c.x - r * 0.85, c.y - r * 0.65),
                    egui::pos2(c.x - r * 0.85, c.y + r * 0.65),
                ],
                egui::Stroke::new(1.1_f32, stroke_col),
            );
            painter.line_segment(
                [
                    egui::pos2(c.x + r * 0.85, c.y - r * 0.65),
                    egui::pos2(c.x + r * 0.85, c.y + r * 0.65),
                ],
                egui::Stroke::new(1.1_f32, stroke_col),
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - r * 0.80, c.y),
                    egui::pos2(c.x + r * 0.80, c.y),
                ],
                egui::Stroke::new(1.4_f32, stroke_col),
            );
        }
    }
}

/// 액션바는 **선택된 레이어 또는 마우스가 호버된 레이어 하나에만** 표시한다(도면 외곽선 가림 방지).
pub fn active_action_bar_layer(
    selected: Option<usize>,
    hovered: Option<usize>,
    visible_layers: &[usize],
) -> Option<usize> {
    if let Some(s) = selected {
        if visible_layers.contains(&s) {
            return Some(s);
        }
    }
    hovered.filter(|h| visible_layers.contains(h))
}

/// Image 7 & 레이어 관리: 개별 도면 레이어(`0..5`) 상단에
/// `[✓/□] [↻90°] [↕180°] [↻앞으로] [🗑Del]` 소형 아이콘 컨트롤 바를 렌더링하고 드래그 이동을 지원한다.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_individual_view_flip_controls(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    anchor_top_center: egui::Pos2,
    view_id_prefix: &str,
    layer_idx: usize,
    is_selected: bool,
    scale_px_per_mm: f32,
    transform: &mut IndividualViewTransform2D,
) -> (bool, bool) {
    let total_w = 184.0_f32;
    let bar_h = 18.0_f32;
    let bar_rect = egui::Rect::from_center_size(
        egui::pos2(anchor_top_center.x, anchor_top_center.y - 12.0),
        egui::vec2(total_w, bar_h),
    );
    let terra = egui::Color32::from_rgb(ACTION.rgb[0], ACTION.rgb[1], ACTION.rgb[2]);
    let ink = egui::Color32::from_rgb(INK.rgb[0], INK.rgb[1], INK.rgb[2]);
    let select_blue =
        egui::Color32::from_rgb(SELECT_BLUE.rgb[0], SELECT_BLUE.rgb[1], SELECT_BLUE.rgb[2]);
    let profile_red =
        egui::Color32::from_rgb(PROFILE_RED.rgb[0], PROFILE_RED.rgb[1], PROFILE_RED.rgb[2]);
    painter.rect_filled(
        bar_rect,
        3.5,
        egui::Color32::from_rgba_unmultiplied(250, 249, 245, 244),
    );
    painter.rect_stroke(
        bar_rect,
        3.5,
        egui::Stroke::new(
            if is_selected { 1.5_f32 } else { 1.0_f32 },
            if is_selected { select_blue } else { terra },
        ),
        egui::StrokeKind::Inside,
    );

    let mut clicked_select = false;
    let mut clicked_delete = false;

    // 좌측 그립(`✥` 드래그 시 해당 레이어만 자유 이동, 억지로 붙이지 않고 원하는 간격으로 배치 가능!)
    let grip_rect = egui::Rect::from_min_size(
        egui::pos2(bar_rect.left() + 2.0, bar_rect.top() + 1.5),
        egui::vec2(20.0, bar_h - 3.0),
    );
    let grip_resp = ui.interact(
        grip_rect,
        egui::Id::new((view_id_prefix, "drag_grip", layer_idx)),
        egui::Sense::click_and_drag(),
    );
    if grip_resp.clicked() || grip_resp.dragged() {
        clicked_select = true;
    }
    if grip_resp.dragged() {
        let sc = scale_px_per_mm.max(0.5);
        let d = grip_resp.drag_delta();
        transform.translate_mm(f64::from(d.x / sc), f64::from(-d.y / sc));
    }
    painter.rect_filled(
        grip_rect,
        2.5,
        if is_selected {
            select_blue
        } else {
            egui::Color32::from_rgb(236, 232, 222)
        },
    );
    painter.text(
        grip_rect.center(),
        egui::Align2::CENTER_CENTER,
        "✥",
        egui::FontId::proportional(9.0),
        if is_selected {
            egui::Color32::WHITE
        } else {
            ink
        },
    );

    let specs = [
        ("rot90", "↻90°", 34.0_f32, 0u8),
        ("flip180", "↕180°", 38.0_f32, 1u8),
        ("flipfwd", "↻앞으로", 48.0_f32, 2u8),
        ("del", "🗑Del", 34.0_f32, 3u8),
    ];
    let mut bx = grip_rect.right() + 2.0;
    for (suffix, lbl, bw, kind) in specs {
        let brect = egui::Rect::from_min_size(
            egui::pos2(bx, bar_rect.top() + 1.5),
            egui::vec2(bw, bar_h - 3.0),
        );
        let resp = ui.interact(
            brect,
            egui::Id::new((view_id_prefix, suffix, layer_idx)),
            egui::Sense::click(),
        );
        if resp.clicked() {
            clicked_select = true;
            match kind {
                0 => transform.rotate_cw_90(),
                1 => transform.flip_180(),
                2 => transform.flip_forward_continuous_90(),
                _ => clicked_delete = true,
            }
        }
        let active = (kind == 0 && transform.rot_90_steps != 0)
            || ((kind == 1 || kind == 2) && transform.flip_forward_steps != 0);
        painter.rect_filled(
            brect,
            2.5,
            if active {
                profile_red
            } else if kind == 3 && resp.hovered() {
                egui::Color32::from_rgb(252, 224, 224)
            } else if resp.hovered() {
                egui::Color32::from_rgb(240, 232, 220)
            } else {
                egui::Color32::from_rgb(246, 243, 236)
            },
        );
        painter.text(
            brect.center(),
            egui::Align2::CENTER_CENTER,
            lbl,
            egui::FontId::proportional(8.2),
            if active {
                egui::Color32::WHITE
            } else if kind == 3 {
                profile_red
            } else {
                ink
            },
        );
        bx += bw + 1.5;
    }

    (clicked_select, clicked_delete)
}
