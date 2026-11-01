use strata_core::strata_tokens::tokens::{
    ACTION, CLAY_FOCUS, CLAY_INK, CLAY_WASH, EDGE, GROUND, INK, MUTED, PROFILE_RED, SELECT_BLUE,
    SURFACE,
};
use strata_core::{SurfaceRegion, TriMesh};
use strata_orient::RimGapChart360;
use eframe::egui;

use super::geometry::*;
use super::raster::*;
use super::state::*;

/// 3D 뷰포트(`27%`) — 상단 `직상방 동심원 방안지 절단 뷰 (중앙 고정 단면선 + 유물 수평이동·회전)` +
/// 하단 `수평 정면 입면 3D 스캐너 뷰 (전체 방안선 + 상하 점선 연결 + 5휠 미세조정 시스템)`를 렌더링한다.
pub fn render_view3d_pane_egui(
    ui: &mut egui::Ui,
    mesh: &TriMesh,
    region_labels: Option<&[SurfaceRegion]>,
    _rim_gap_chart: Option<&RimGapChart360>,
    best_angle_deg: f64,
    state: &mut View3DPaneState,
) {
    let ext = mesh.bbox.extents();
    let height_mm = ext[2].max(1.0);
    let radius_mm = (0.5 * ext[0].max(ext[1])).max(5.0);
    let grid_r_mm = compute_concentric_grid_radius_mm(radius_mm);

    // 새 유물을 열면 수평횡단면 기본 높이를 잡는다(높이 2mm 미만 유물에서도 패닉 없음).
    state.initialize_top_cut_for_mesh(mesh);

    let avail = ui.available_size();
    let top_plan_h = (avail.y * 0.37).clamp(180.0, 340.0);
    let front_elev_h = (avail.y - top_plan_h - 4.0).max(200.0);

    let hanji = egui::Color32::from_rgb(GROUND.rgb[0], GROUND.rgb[1], GROUND.rgb[2]);
    let surface = egui::Color32::from_rgb(SURFACE.rgb[0], SURFACE.rgb[1], SURFACE.rgb[2]);
    let clay_wash = egui::Color32::from_rgb(CLAY_WASH.rgb[0], CLAY_WASH.rgb[1], CLAY_WASH.rgb[2]);
    let clay_ink = egui::Color32::from_rgb(CLAY_INK.rgb[0], CLAY_INK.rgb[1], CLAY_INK.rgb[2]);
    let border = egui::Color32::from_rgb(EDGE.rgb[0], EDGE.rgb[1], EDGE.rgb[2]);
    let ink = egui::Color32::from_rgb(INK.rgb[0], INK.rgb[1], INK.rgb[2]);
    let muted = egui::Color32::from_rgb(MUTED.rgb[0], MUTED.rgb[1], MUTED.rgb[2]);
    let terra = egui::Color32::from_rgb(ACTION.rgb[0], ACTION.rgb[1], ACTION.rgb[2]);
    let clay_focus =
        egui::Color32::from_rgb(CLAY_FOCUS.rgb[0], CLAY_FOCUS.rgb[1], CLAY_FOCUS.rgb[2]);
    let profile_red = egui::Color32::from_rgb(
        PROFILE_RED.rgb[0],
        PROFILE_RED.rgb[1],
        PROFILE_RED.rgb[2],
    );
    let select_blue = egui::Color32::from_rgb(
        SELECT_BLUE.rgb[0],
        SELECT_BLUE.rgb[1],
        SELECT_BLUE.rgb[2],
    );

    // 방향키(←/→/↑/↓)로도 유물 이동 및 수평횡단면(Alt+↑/↓) 미세이동 가능 (<USER_REQUEST>)
    ui.input(|i| {
        let shift_fast = i.modifiers.shift;
        let alt_horiz_z = i.modifiers.alt;
        let mut dx = 0i32;
        let mut dy = 0i32;
        if i.key_pressed(egui::Key::ArrowLeft) {
            dx -= 1;
        }
        if i.key_pressed(egui::Key::ArrowRight) {
            dx += 1;
        }
        if i.key_pressed(egui::Key::ArrowUp) {
            dy += 1;
        }
        if i.key_pressed(egui::Key::ArrowDown) {
            dy -= 1;
        }
        if dx != 0 || dy != 0 {
            state.apply_arrow_key_nudge(dx, dy, shift_fast, alt_horiz_z, height_mm);
        }
    });

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;

        // =========================================================================================
        // [상단 37% 뷰] 직상방(Top-Down) 동심원 방안지 절단 뷰 (`반경 = 유물 반경 × 1.20 = +20%`)
        // - 좌측 상단: `[+ 단면추가]` 버튼 (`Image 5`) -> 끝지점 독립 이동 가능한 추가 단면선 `A-A'`
        // - 우측 옆: `수평횡단면 고정 유물` 뷰 (`Image 4` -> 유물 고정 + 수평절단선만 상하 이동)
        // =========================================================================================
        let (top_rect, top_resp) =
            ui.allocate_exact_size(egui::vec2(avail.x, top_plan_h), egui::Sense::click_and_drag());

        let td_center = top_rect.center();
        let td_scale = compute_shared_ortho_scale(
            f64::from(top_rect.width()),
            grid_r_mm,
            height_mm,
            state.zoom,
        );
        let r_120_px = (grid_r_mm * td_scale) as f32;
        let r_100_px = (radius_mm * td_scale) as f32;

        // Image 5: 도면 안 좌측 위에 `[+ 단면추가]` 버튼 배치
        let add_sec_btn_rect = egui::Rect::from_min_size(
            egui::pos2(top_rect.left() + 8.0, top_rect.top() + 28.0),
            egui::vec2(92.0, 22.0),
        );
        let add_sec_btn_resp = ui.interact(
            add_sec_btn_rect,
            egui::Id::new("v3d_btn_add_custom_section"),
            egui::Sense::click(),
        );
        if add_sec_btn_resp.clicked() {
            if state.custom_section_line_mm.is_none() {
                state.add_default_custom_section_line(radius_mm);
            } else {
                state.custom_section_draw_mode = !state.custom_section_draw_mode;
            }
        }

        // Image 5: 추가 단면선(A-A')의 양 끝점(0번 또는 1번) 중 한쪽 끝지점을 누르고 이동하면 그 끝점만 이동!
        let mut custom_ep_dragged = false;
        if let Some([p0_mm, p1_mm]) = state.custom_section_line_mm {
            let ep0_px = egui::pos2(
                td_center.x + (p0_mm[0] * td_scale) as f32,
                td_center.y - (p0_mm[1] * td_scale) as f32,
            );
            let ep1_px = egui::pos2(
                td_center.x + (p1_mm[0] * td_scale) as f32,
                td_center.y - (p1_mm[1] * td_scale) as f32,
            );
            let ep0_rect = egui::Rect::from_center_size(ep0_px, egui::vec2(20.0, 20.0));
            let ep1_rect = egui::Rect::from_center_size(ep1_px, egui::vec2(20.0, 20.0));
            let ep0_resp = ui.interact(
                ep0_rect,
                egui::Id::new("v3d_custom_sec_ep0"),
                egui::Sense::click_and_drag(),
            );
            let ep1_resp = ui.interact(
                ep1_rect,
                egui::Id::new("v3d_custom_sec_ep1"),
                egui::Sense::click_and_drag(),
            );
            if ep0_resp.dragged() {
                custom_ep_dragged = true;
                state.custom_section_dragging_endpoint = Some(0);
                if let Some(pos) = ep0_resp.interact_pointer_pos() {
                    let nx_mm = f64::from(pos.x - td_center.x) / td_scale.max(0.5);
                    let ny_mm = f64::from(td_center.y - pos.y) / td_scale.max(0.5);
                    let max_r = grid_r_mm * 1.05;
                    state.move_custom_section_endpoint(
                        0,
                        [nx_mm.clamp(-max_r, max_r), ny_mm.clamp(-max_r, max_r)],
                    );
                }
            } else if ep1_resp.dragged() {
                custom_ep_dragged = true;
                state.custom_section_dragging_endpoint = Some(1);
                if let Some(pos) = ep1_resp.interact_pointer_pos() {
                    let nx_mm = f64::from(pos.x - td_center.x) / td_scale.max(0.5);
                    let ny_mm = f64::from(td_center.y - pos.y) / td_scale.max(0.5);
                    let max_r = grid_r_mm * 1.05;
                    state.move_custom_section_endpoint(
                        1,
                        [nx_mm.clamp(-max_r, max_r), ny_mm.clamp(-max_r, max_r)],
                    );
                }
            } else {
                state.custom_section_dragging_endpoint = None;
            }
        }

        if top_resp.dragged()
            && !custom_ep_dragged
            && !add_sec_btn_resp.hovered()
        {
            let d = top_resp.drag_delta();
            let shift_or_right = ui.input(|i| i.modifiers.shift || i.pointer.secondary_down());
            if shift_or_right {
                // 우클릭 또는 Shift+드래그: 유물 시계·반시계 회전 (ψ)
                let next_psi = state.active_section_angle_deg + f64::from(d.x) * 0.5;
                state.active_section_angle_deg =
                    ((next_psi * 10.0).round() / 10.0).rem_euclid(360.0);
            } else {
                // 기본 좌클릭 드래그: 중앙 단면선은 고정하고 유물만 좌우(ΔX)·앞뒤(ΔY) 수평 이동!
                let max_off = (radius_mm * 1.10).max(15.0);
                let mm_per_px = 1.0 / td_scale.max(0.5);
                state.artifact_offset_x_mm = (((state.artifact_offset_x_mm
                    + f64::from(d.x) * mm_per_px)
                    .clamp(-max_off, max_off)
                    * 100.0)
                    .round())
                    / 100.0;
                state.cut_y_mm = (((state.cut_y_mm + f64::from(d.y) * mm_per_px)
                    .clamp(-max_off, max_off)
                    * 100.0)
                    .round())
                    / 100.0;
            }
        }
        if top_resp.double_clicked() {
            state.artifact_offset_x_mm = 0.0;
            state.cut_y_mm = 0.0;
        }

        let tp = ui.painter_at(top_rect);
        tp.rect_filled(top_rect, 3.0, surface);

        let top_w = (top_rect.width().round() as u32).max(160);
        let top_h = (top_rect.height().round() as u32).max(140);
        let _ = state.ensure_topdown_cached(mesh, top_w, top_h);
        if state.cached_topdown_tex.is_none() {
            if let Some(ref td_img) = state.cached_topdown_rgba {
                let cimg = egui::ColorImage::from_rgba_unmultiplied(
                    [td_img.width() as usize, td_img.height() as usize],
                    td_img.as_raw(),
                );
                state.cached_topdown_tex = Some(ui.ctx().load_texture(
                    "strata_view3d_topdown_plan",
                    cimg,
                    egui::TextureOptions::LINEAR,
                ));
            }
        }
        if let Some(ref td_tex) = state.cached_topdown_tex {
            tp.image(
                td_tex.id(),
                top_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }

        // 동심원은 항상 1:1 정원(둥근 원)이며 중앙에 고정됨
        for rk in 1..=5 {
            let r_px = r_120_px * (rk as f32 / 5.0);
            tp.circle_stroke(
                td_center,
                r_px,
                egui::Stroke::new(
                    if rk == 5 { 1.3_f32 } else { 0.8_f32 },
                    egui::Color32::from_rgba_unmultiplied(
                        156,
                        74,
                        47,
                        if rk == 5 { 170 } else { 95 },
                    ),
                ),
            );
        }
        tp.circle_stroke(
            td_center,
            r_100_px,
            egui::Stroke::new(1.1_f32, egui::Color32::from_rgba_unmultiplied(42, 120, 214, 150)),
        );

        // 직상방 십자 기준선 (0°·90°·180°·270°)
        tp.line_segment(
            [
                egui::pos2(td_center.x - r_120_px - 8.0, td_center.y),
                egui::pos2(td_center.x + r_120_px + 8.0, td_center.y),
            ],
            egui::Stroke::new(0.9_f32, egui::Color32::from_rgba_unmultiplied(94, 93, 89, 130)),
        );
        tp.line_segment(
            [
                egui::pos2(td_center.x, td_center.y - r_120_px - 8.0),
                egui::pos2(td_center.x, td_center.y + r_120_px + 8.0),
            ],
            egui::Stroke::new(0.9_f32, egui::Color32::from_rgba_unmultiplied(94, 93, 89, 130)),
        );

        // Image 1 ("단면선이 유물에가려진다"): 단면선이 3D 유물에 절대 가려지지 않도록 밝은 외곽 후광(Halo) + 최상단 선으로 렌더링!
        let fixed_cut_sy = td_center.y;
        tp.line_segment(
            [
                egui::pos2(td_center.x - r_120_px - 10.0, fixed_cut_sy),
                egui::pos2(td_center.x + r_120_px + 10.0, fixed_cut_sy),
            ],
            egui::Stroke::new(
                4.2_f32,
                egui::Color32::from_rgba_unmultiplied(255, 252, 235, 220),
            ),
        );
        tp.line_segment(
            [
                egui::pos2(td_center.x - r_120_px - 10.0, fixed_cut_sy),
                egui::pos2(td_center.x + r_120_px + 10.0, fixed_cut_sy),
            ],
            egui::Stroke::new(2.0_f32, select_blue),
        );

        state.ensure_3d_curves_cached(mesh);
        let mut cut_min_wx = f64::INFINITY;
        let mut cut_max_wx = f64::NEG_INFINITY;
        for seg in &state.cached_v_segs {
            cut_min_wx = cut_min_wx.min(seg[0][0]).min(seg[1][0]);
            cut_max_wx = cut_max_wx.max(seg[0][0]).max(seg[1][0]);
            let p0 = egui::pos2(
                td_center.x + (seg[0][0] * td_scale) as f32,
                fixed_cut_sy,
            );
            let p1 = egui::pos2(
                td_center.x + (seg[1][0] * td_scale) as f32,
                fixed_cut_sy,
            );
            tp.line_segment(
                [p0, p1],
                egui::Stroke::new(
                    4.6_f32,
                    egui::Color32::from_rgba_unmultiplied(255, 250, 210, 235),
                ),
            );
            tp.line_segment([p0, p1], egui::Stroke::new(2.8_f32, profile_red));
        }

        // Image 5: 사용자가 추가한 단면선(`A — A'`)과 한쪽 끝점만 독립 이동 가능한 핸들(`A`, `A'`) 렌더링
        if let Some([p0_mm, p1_mm]) = state.custom_section_line_mm {
            let ep0_px = egui::pos2(
                td_center.x + (p0_mm[0] * td_scale) as f32,
                td_center.y - (p0_mm[1] * td_scale) as f32,
            );
            let ep1_px = egui::pos2(
                td_center.x + (p1_mm[0] * td_scale) as f32,
                td_center.y - (p1_mm[1] * td_scale) as f32,
            );
            tp.line_segment(
                [ep0_px, ep1_px],
                egui::Stroke::new(
                    4.4_f32,
                    egui::Color32::from_rgba_unmultiplied(255, 252, 230, 235),
                ),
            );
            tp.line_segment([ep0_px, ep1_px], egui::Stroke::new(2.4_f32, profile_red));
            for (ep_px, lbl) in [(ep0_px, "A"), (ep1_px, "A'")] {
                tp.circle_filled(ep_px, 6.2, egui::Color32::WHITE);
                tp.circle_stroke(ep_px, 6.2, egui::Stroke::new(2.0_f32, profile_red));
                tp.text(
                    egui::pos2(ep_px.x, ep_px.y - 11.0),
                    egui::Align2::CENTER_CENTER,
                    lbl,
                    egui::FontId::proportional(9.5),
                    profile_red,
                );
            }
        }

        // Image 5: 좌측 상단 `[+ 단면추가]` 버튼 렌더링
        let btn_active = state.custom_section_line_mm.is_some();
        tp.rect_filled(
            add_sec_btn_rect,
            4.0,
            if btn_active { profile_red } else { clay_wash },
        );
        tp.rect_stroke(
            add_sec_btn_rect,
            4.0,
            egui::Stroke::new(1.2_f32, profile_red),
            egui::StrokeKind::Inside,
        );
        tp.text(
            add_sec_btn_rect.center(),
            egui::Align2::CENTER_CENTER,
            if btn_active {
                "✂ 단면선 A-A' 활성"
            } else {
                "+ 단면추가"
            },
            egui::FontId::proportional(9.5),
            if btn_active { egui::Color32::WHITE } else { ink },
        );
        if btn_active {
            let hint_rect = egui::Rect::from_min_size(
                egui::pos2(add_sec_btn_rect.left(), add_sec_btn_rect.bottom() + 3.0),
                egui::vec2(102.0, 28.0),
            );
            tp.rect_filled(
                hint_rect,
                3.0,
                egui::Color32::from_rgba_unmultiplied(250, 249, 245, 232),
            );
            tp.text(
                egui::pos2(hint_rect.left() + 4.0, hint_rect.top() + 8.0),
                egui::Align2::LEFT_CENTER,
                "끝점(A/A') 드래그:",
                egui::FontId::proportional(8.2),
                ink,
            );
            tp.text(
                egui::pos2(hint_rect.left() + 4.0, hint_rect.top() + 19.0),
                egui::Align2::LEFT_CENTER,
                "한쪽 끝만 단독 이동",
                egui::FontId::proportional(8.2),
                profile_red,
            );
        }

        // 유물 현재 중심점 및 회전 방위각(ψ) 바늘 표시
        let _ = region_labels;
        let art_center_px = egui::pos2(
            td_center.x + (state.artifact_offset_x_mm * td_scale) as f32,
            td_center.y + (state.cut_y_mm * td_scale) as f32,
        );
        let psi_rad = state.active_section_angle_deg.to_radians() as f32;
        let needle_pt = egui::pos2(
            art_center_px.x + r_100_px * 0.85 * psi_rad.cos(),
            art_center_px.y - r_100_px * 0.85 * psi_rad.sin(),
        );
        tp.line_segment([art_center_px, needle_pt], egui::Stroke::new(1.3_f32, terra));
        tp.circle_filled(td_center, 3.2, profile_red);

        // 직상방 뷰 상단 미니 배지
        let td_hdr = egui::Rect::from_min_size(
            egui::pos2(top_rect.left() + 6.0, top_rect.top() + 4.0),
            egui::vec2(top_rect.width() - 12.0, 20.0),
        );
        tp.rect_filled(
            td_hdr,
            3.0,
            egui::Color32::from_rgba_unmultiplied(250, 249, 245, 225),
        );
        tp.text(
            egui::pos2(td_hdr.left() + 6.0, td_hdr.center().y),
            egui::Align2::LEFT_CENTER,
            format!(
                "직상방 동심원 (중앙단면 고정 · 유물이동 ΔX={:+.1} ΔY={:+.2}mm · 드래그/방향키=유물이동)",
                state.artifact_offset_x_mm, state.cut_y_mm
            ),
            egui::FontId::proportional(9.8),
            ink,
        );
        tp.rect_stroke(
            top_rect,
            3.0,
            egui::Stroke::new(1.0_f32, border),
            egui::StrokeKind::Inside,
        );

        // =========================================================================================
        // [하단 63% 뷰] 수평 정면 입면 3D 스캐너 뷰 (전체 방안선 + 상하 점선 연결 + 5휠 미세조정 시스템)
        // =========================================================================================
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(avail.x, front_elev_h),
            egui::Sense::click_and_drag(),
        );

        let dial_horiz_w = (rect.width() * 0.44).clamp(148.0, 196.0);
        let dial_horiz_h = 20.0_f32;
        // Image 1: 상단 가로 휠 (유물이 시계방향·반시계방향으로 회전하는 휠, 휠 아래 설명)
        // Image 6 유물 뒤집기(90°/180°) 버튼 행 아래쪽에 여유 있게 배치!
        let top_dial_rect = egui::Rect::from_center_size(
            egui::pos2(rect.center().x, rect.top() + 82.0),
            egui::vec2(dial_horiz_w, dial_horiz_h),
        );
        // <USER_REQUEST>: 상하좌우 휠 안쪽에 수평·단면 미세정합 제5휠 추가!
        let inner_level_dial_rect = egui::Rect::from_center_size(
            egui::pos2(rect.center().x, rect.bottom() - 68.0),
            egui::vec2((dial_horiz_w * 0.86).max(128.0), 17.0),
        );
        // Image 2: 하단 가로 휠 (유물·단면선 앞뒤로 미세조정 휠, 휠 아래 설명)
        let bot_dial_rect = egui::Rect::from_center_size(
            egui::pos2(rect.center().x, rect.bottom() - 28.0),
            egui::vec2(dial_horiz_w, dial_horiz_h),
        );
        let dial_vert_w = 21.0_f32;
        let dial_vert_h = (rect.height() * 0.36).clamp(104.0, 168.0);
        let dial_mid_y = (top_dial_rect.bottom() + inner_level_dial_rect.top()) * 0.5 + 4.0;
        // Image 4: 좌측 세로 휠 (유물의 아래위를 회전하는 휠 — 올리면 위로 회전, 내리면 아래로 회전, 옆에 설명)
        let left_dial_rect = egui::Rect::from_center_size(
            egui::pos2(rect.left() + 16.0, dial_mid_y),
            egui::vec2(dial_vert_w, dial_vert_h),
        );
        // Image 3: 우측 세로 휠 (유물의 좌우 기울기를 미세조정하는 휠, 옆에 설명)
        let right_dial_rect = egui::Rect::from_center_size(
            egui::pos2(rect.right() - 16.0, dial_mid_y),
            egui::vec2(dial_vert_w, dial_vert_h),
        );

        let top_dial_resp = ui.interact(
            top_dial_rect,
            egui::Id::new("v3d_wheel_top_turn_psi"),
            egui::Sense::click_and_drag(),
        );
        let bot_dial_resp = ui.interact(
            bot_dial_rect,
            egui::Id::new("v3d_wheel_bot_section_dy"),
            egui::Sense::click_and_drag(),
        );
        let left_dial_resp = ui.interact(
            left_dial_rect,
            egui::Id::new("v3d_wheel_left_pitch_updown"),
            egui::Sense::click_and_drag(),
        );
        let right_dial_resp = ui.interact(
            right_dial_rect,
            egui::Id::new("v3d_wheel_right_tilt_lr"),
            egui::Sense::click_and_drag(),
        );
        let inner_dial_resp = ui.interact(
            inner_level_dial_rect,
            egui::Id::new("v3d_wheel_inner_level_sync"),
            egui::Sense::click_and_drag(),
        );

        // Image 1: 상단 가로 휠 -> 유물 시계(+)/반시계(-) 회전 (0.1°, Image 3 감도 +30% 향상: * 0.65)
        if top_dial_resp.clicked() {
            state.apply_precision_wheel(PrecisionWheelKind::TopTurnCwCcw, 1);
        } else if top_dial_resp.dragged() {
            let steps = (top_dial_resp.drag_delta().x * 0.65).round() as i32;
            if steps != 0 {
                state.apply_precision_wheel(PrecisionWheelKind::TopTurnCwCcw, steps);
            }
        }
        // Image 2: 하단 가로 휠 -> 유물·단면선 앞뒤 미세조정 (0.01mm, Image 3 감도 +30% 향상: * 0.65)
        if bot_dial_resp.clicked() {
            state.apply_precision_wheel(PrecisionWheelKind::BottomSectionFrontBack, 1);
        } else if bot_dial_resp.dragged() {
            let steps = (bot_dial_resp.drag_delta().x * 0.65).round() as i32;
            if steps != 0 {
                state.apply_precision_wheel(PrecisionWheelKind::BottomSectionFrontBack, steps);
            }
        }
        // Image 4: 좌측 세로 휠 -> 올리면(drag_delta.y < 0) 위로 회전(+steps), 내리면(drag_delta.y > 0) 아래로 회전(-steps) (+30% 감도)
        if left_dial_resp.clicked() {
            state.apply_precision_wheel(PrecisionWheelKind::LeftPitchUpDown, 1);
        } else if left_dial_resp.dragged() {
            let steps = (-left_dial_resp.drag_delta().y * 0.65).round() as i32;
            if steps != 0 {
                state.apply_precision_wheel(PrecisionWheelKind::LeftPitchUpDown, steps);
            }
        }
        // Image 3: 우측 세로 휠 -> 유물 좌우 기울기 미세조정 (0.1°, +30% 감도)
        if right_dial_resp.clicked() {
            state.apply_precision_wheel(PrecisionWheelKind::RightTiltLeftRight, 1);
        } else if right_dial_resp.dragged() {
            let steps = (-right_dial_resp.drag_delta().y * 0.65).round() as i32;
            if steps != 0 {
                state.apply_precision_wheel(PrecisionWheelKind::RightTiltLeftRight, steps);
            }
        }
        // Image 0: 안쪽 제5휠 -> 수평횡단면을 그리는 휠 (선만 상하이동, 0.1mm 단위, +30% 감도)
        if inner_dial_resp.clicked() {
            state.apply_precision_wheel(PrecisionWheelKind::InnerHorizSectionZ, 5);
        } else if inner_dial_resp.dragged() {
            let d = inner_dial_resp.drag_delta();
            let raw = if d.x.abs() >= d.y.abs() { d.x } else { -d.y };
            let steps = (raw * 0.65).round() as i32;
            if steps != 0 {
                state.apply_precision_wheel(PrecisionWheelKind::InnerHorizSectionZ, steps);
                state.top_cut_z_mm = state
                    .top_cut_z_mm
                    .clamp(1.0, (height_mm - 1.0).max(2.0));
            }
        }
        if inner_dial_resp.double_clicked() {
            state.top_cut_z_mm = ((height_mm * 0.50) * 10.0).round() / 10.0;
        }

        let any_fine_dial_active = top_dial_resp.dragged()
            || bot_dial_resp.dragged()
            || left_dial_resp.dragged()
            || right_dial_resp.dragged()
            || inner_dial_resp.dragged()
            || top_dial_resp.hovered()
            || bot_dial_resp.hovered()
            || left_dial_resp.hovered()
            || right_dial_resp.hovered()
            || inner_dial_resp.hovered();

        if response.dragged() && !any_fine_dial_active {
            let delta = response.drag_delta();
            match state.drag_mode {
                DragMode3D::TurntableArtifact => {
                    let next_psi = state.active_section_angle_deg + f64::from(delta.x) * 0.6;
                    state.active_section_angle_deg =
                        ((next_psi * 10.0).round() / 10.0).rem_euclid(360.0);
                    state.yaw_deg = 0.0;
                    state.pitch_deg = 0.0;
                }
                DragMode3D::CameraOrbit => {
                    state.yaw_deg = (state.yaw_deg + delta.x * 0.6).rem_euclid(360.0);
                    state.pitch_deg = (state.pitch_deg + delta.y * 0.4).clamp(-45.0, 45.0);
                }
                DragMode3D::AxisTilt => {
                    state.tilt_deg =
                        ((state.tilt_deg - f64::from(delta.y) * 0.1).clamp(-30.0, 30.0) * 10.0)
                            .round()
                            / 10.0;
                }
            }
        }
        if response.hovered() || top_resp.hovered() {
            let scroll_y = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll_y.abs() > 0.1 {
                state.zoom = (state.zoom * (1.0 + scroll_y * 0.0015)).clamp(0.45, 3.2);
            }
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 3.0, surface);

        let is_dragging = response.dragged()
            || top_resp.dragged()
            || top_dial_resp.dragged()
            || bot_dial_resp.dragged()
            || left_dial_resp.dragged()
            || right_dial_resp.dragged()
            || inner_dial_resp.dragged();
        state.is_interacting = is_dragging;

        let (render_w, render_h) =
            compute_isotropic_render_dimensions(rect.width(), rect.height(), is_dragging);
        let _ = state.ensure_zbuffer_cached(mesh, region_labels, render_w, render_h);

        if state.cached_zbuffer_tex.is_none() {
            if let Some(ref zbuf_img) = state.cached_zbuffer_rgba {
                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                    [zbuf_img.width() as usize, zbuf_img.height() as usize],
                    zbuf_img.as_raw(),
                );
                state.cached_zbuffer_tex = Some(ui.ctx().load_texture(
                    "strata_view3d_zbuffer",
                    color_image,
                    egui::TextureOptions::LINEAR,
                ));
            }
        }
        if let Some(ref tex_handle) = state.cached_zbuffer_tex {
            painter.image(
                tex_handle.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        if is_dragging {
            ui.ctx().request_repaint();
        }

        // 정면 입면 투영 변환 도우미 (상단 직상방 동심원 평면도와 100% 1:1 동일한 scale 사용!)
        let scale = compute_shared_ortho_scale(
            f64::from(rect.width()),
            grid_r_mm,
            height_mm,
            state.zoom,
        );
        let half_w = f64::from(rect.width()) * 0.5;
        let half_h = f64::from(rect.height()) * 0.54;
        let (sin_cy, cos_cy) = f64::from(state.yaw_deg).to_radians().sin_cos();
        let (sin_cp, cos_cp) = f64::from(state.pitch_deg).to_radians().sin_cos();

        let proj_pos2 = |w: [f64; 3]| -> egui::Pos2 {
            let s = project_world_to_screen(
                w, height_mm, half_w, half_h, scale, cos_cy, sin_cy, cos_cp, sin_cp,
            );
            egui::pos2(rect.left() + s[0] as f32, rect.top() + s[1] as f32)
        };

        // 수평 바닥 동심원 기준선 (`Z = 0` 평면)
        if state.show_concentric_scope {
            let b0 = proj_pos2([-grid_r_mm, state.cut_y_mm, 0.0]);
            let b1 = proj_pos2([grid_r_mm, state.cut_y_mm, 0.0]);
            painter.line_segment(
                [b0, b1],
                egui::Stroke::new(1.2_f32, egui::Color32::from_rgba_unmultiplied(156, 74, 47, 165)),
            );
        }

        // 월드 고정 수평 정면 절단틀 (`P_cut: Y = cut_y_mm`) 및 수직 중심축선 + 실시간 단면 곡선
        let mut elev_min_sx = rect.center().x;
        let mut elev_max_sx = rect.center().x;
        let mut elev_top_sy = rect.center().y;
        if state.show_cutting_frame {
            let hw = grid_r_mm;
            let hh = height_mm + 4.0;
            let y_cut = state.cut_y_mm;
            let f0 = proj_pos2([-hw, y_cut, -2.0]);
            let f1 = proj_pos2([hw, y_cut, -2.0]);
            let f2 = proj_pos2([hw, y_cut, hh]);
            let f3 = proj_pos2([-hw, y_cut, hh]);
            elev_top_sy = f2.y.min(f3.y);

            painter.add(egui::Shape::closed_line(
                vec![f0, f1, f2, f3],
                egui::Stroke::new(1.1_f32, select_blue),
            ));

            let ax_bot = proj_pos2([state.artifact_offset_x_mm, y_cut, -4.0]);
            let ax_top = proj_pos2([state.artifact_offset_x_mm, y_cut, hh + 2.0]);
            painter.line_segment([ax_bot, ax_top], egui::Stroke::new(1.3_f32, terra));

            for seg in &state.cached_v_segs {
                let a = proj_pos2(seg[0]);
                let b = proj_pos2(seg[1]);
                elev_min_sx = elev_min_sx.min(a.x).min(b.x);
                elev_max_sx = elev_max_sx.max(a.x).max(b.x);
                painter.line_segment(
                    [a, b],
                    egui::Stroke::new(
                        4.2_f32,
                        egui::Color32::from_rgba_unmultiplied(255, 250, 210, 230),
                    ),
                );
                painter.line_segment([a, b], egui::Stroke::new(2.2_f32, profile_red));
            }

            // Image 0 & Image 1: 수평횡단면 절단선이 유물에 절대 가려지지 않도록 전체 가로지르는 기준선 + 교차선 최상단 렌더링
            let hz = clamp_ordered(state.top_cut_z_mm, 0.5, height_mm);
            let h_left = proj_pos2([-hw * 0.96, y_cut, hz]);
            let h_right = proj_pos2([hw * 0.96, y_cut, hz]);
            painter.line_segment(
                [h_left, h_right],
                egui::Stroke::new(
                    4.0_f32,
                    egui::Color32::from_rgba_unmultiplied(255, 252, 232, 225),
                ),
            );
            painter.line_segment([h_left, h_right], egui::Stroke::new(2.0_f32, profile_red));

            for seg in &state.cached_h_segs {
                let a = proj_pos2(seg[0]);
                let b = proj_pos2(seg[1]);
                painter.line_segment(
                    [a, b],
                    egui::Stroke::new(
                        3.6_f32,
                        egui::Color32::from_rgba_unmultiplied(255, 250, 215, 225),
                    ),
                );
                painter.line_segment([a, b], egui::Stroke::new(2.0_f32, profile_red));
            }
        }

        // =========================================================================================
        // Image 7 & v6_vessel_roi_1to1 Image 0:
        // 상단 직상방 동심원 평면도와 하단 수평 정면 입면도가 100% 1:1 동일한 스케일(td_scale == scale)을 공유하므로
        // 좌측 절단점·중심축·우측 절단점이 기울어지지 않고 90° 수직 점선으로 1:1 직결된다!
        // =========================================================================================
        let link_stroke = egui::Stroke::new(
            1.1_f32,
            egui::Color32::from_rgba_unmultiplied(200, 48, 36, 175),
        );
        let center_x = td_center.x + (state.artifact_offset_x_mm * td_scale) as f32;
        draw_vertical_dashed_line(&tp, center_x, fixed_cut_sy, top_rect.bottom(), link_stroke);
        draw_vertical_dashed_line(&painter, center_x, rect.top(), elev_top_sy + 14.0, link_stroke);

        if cut_min_wx.is_finite() && cut_max_wx.is_finite() {
            let left_x = td_center.x + (cut_min_wx * td_scale) as f32;
            let right_x = td_center.x + (cut_max_wx * td_scale) as f32;

            draw_vertical_dashed_line(&tp, left_x, fixed_cut_sy, top_rect.bottom(), link_stroke);
            draw_vertical_dashed_line(&tp, right_x, fixed_cut_sy, top_rect.bottom(), link_stroke);
            draw_vertical_dashed_line(&painter, left_x, rect.top(), elev_top_sy + 14.0, link_stroke);
            draw_vertical_dashed_line(&painter, right_x, rect.top(), elev_top_sy + 14.0, link_stroke);
        }

        // =========================================================================================
        // 상·하·좌·우 4방향 휠 + 안쪽 제5 수평정합 휠 렌더링 (`Image 1 ~ 4` & `<USER_REQUEST>`)
        // =========================================================================================
        let dial_fill = egui::Color32::from_rgba_unmultiplied(236, 233, 224, 238);
        let dial_stroke = egui::Color32::from_rgb(178, 174, 162);
        let tick_col = egui::Color32::from_rgb(156, 152, 142);
        let tick_major_col = egui::Color32::from_rgb(104, 100, 92);
        let label_bg = egui::Color32::from_rgba_unmultiplied(250, 249, 245, 225);

        // 1. Image 1 (상단 가로 휠): 유물이 시계방향·반시계방향으로 회전을 하는 휠 + 휠 아래 설명!
        painter.rect_filled(top_dial_rect, 10.0, dial_fill);
        painter.rect_stroke(
            top_dial_rect,
            10.0,
            egui::Stroke::new(
                if top_dial_resp.hovered() { 1.4_f32 } else { 1.0_f32 },
                if top_dial_resp.hovered() { terra } else { dial_stroke },
            ),
            egui::StrokeKind::Inside,
        );
        let n_ticks_h = 32usize;
        let phase_psi = ((state.active_section_angle_deg * 10.0).rem_euclid(5.0) / 5.0) as f32;
        for ti in 0..=n_ticks_h {
            let frac = ((ti as f32 + phase_psi) / (n_ticks_h as f32)).clamp(0.0, 1.0);
            let tx = top_dial_rect.left() + 9.0 + frac * (top_dial_rect.width() - 18.0);
            let is_mid = ti == n_ticks_h / 2;
            let th = if is_mid { 12.0 } else if ti % 4 == 0 { 9.0 } else { 6.0 };
            painter.line_segment(
                [
                    egui::pos2(tx, top_dial_rect.center().y - th * 0.5),
                    egui::pos2(tx, top_dial_rect.center().y + th * 0.5),
                ],
                egui::Stroke::new(
                    if is_mid { 1.3_f32 } else { 0.8_f32 },
                    if is_mid { tick_major_col } else { tick_col },
                ),
            );
        }
        // Image 1: 휠 아래 설명 ("유물이 시계방향 반시계방향으로 회전을 하는 휠 휠아래설명")
        let top_lbl_rect = egui::Rect::from_center_size(
            egui::pos2(top_dial_rect.center().x, top_dial_rect.bottom() + 9.5),
            egui::vec2(216.0, 15.0),
        );
        painter.rect_filled(top_lbl_rect, 3.0, label_bg);
        painter.text(
            top_lbl_rect.center(),
            egui::Align2::CENTER_CENTER,
            format!(
                "↻ 유물 시계·반시계 회전 휠 (ψ={:.1}°)",
                state.active_section_angle_deg
            ),
            egui::FontId::proportional(9.5),
            ink,
        );

        // 2. Image 0 (안쪽 제5휠): 수평횡단면을 그리는 휠 (선만 아래위로 이동 + 수평횡단면 도면 실시간 갱신)
        painter.rect_filled(inner_level_dial_rect, 8.5, clay_wash);
        painter.rect_stroke(
            inner_level_dial_rect,
            8.5,
            egui::Stroke::new(
                if inner_dial_resp.hovered() { 1.4_f32 } else { 1.0_f32 },
                if inner_dial_resp.hovered() { profile_red } else { clay_focus },
            ),
            egui::StrokeKind::Inside,
        );
        let phase_hz = ((state.top_cut_z_mm * 10.0).rem_euclid(5.0) / 5.0) as f32;
        for ti in 0..=24usize {
            let frac = ((ti as f32 + phase_hz) / 24.0).clamp(0.0, 1.0);
            let tx =
                inner_level_dial_rect.left() + 8.0 + frac * (inner_level_dial_rect.width() - 16.0);
            let is_mid = ti == 12;
            let th = if is_mid { 10.5 } else if ti % 4 == 0 { 8.0 } else { 5.0 };
            painter.line_segment(
                [
                    egui::pos2(tx, inner_level_dial_rect.center().y - th * 0.5),
                    egui::pos2(tx, inner_level_dial_rect.center().y + th * 0.5),
                ],
                egui::Stroke::new(
                    if is_mid { 1.3_f32 } else { 0.8_f32 },
                    if is_mid { profile_red } else { terra },
                ),
            );
        }
        let inner_lbl_rect = egui::Rect::from_center_size(
            egui::pos2(
                inner_level_dial_rect.center().x,
                inner_level_dial_rect.bottom() + 8.5,
            ),
            egui::vec2(262.0, 14.0),
        );
        painter.rect_filled(inner_lbl_rect, 3.0, label_bg);
        painter.text(
            inner_lbl_rect.center(),
            egui::Align2::CENTER_CENTER,
            format!(
                "↕ 수평횡단면 그리기 휠 (Z={:.1}mm · 선만 상하이동)",
                state.top_cut_z_mm
            ),
            egui::FontId::proportional(9.0),
            profile_red,
        );

        // 3. Image 2 (하단 가로 휠): 유물·단면선 앞뒤로 미세조정 휠 + 휠 아래 설명!
        painter.rect_filled(bot_dial_rect, 10.0, dial_fill);
        painter.rect_stroke(
            bot_dial_rect,
            10.0,
            egui::Stroke::new(
                if bot_dial_resp.hovered() { 1.4_f32 } else { 1.0_f32 },
                if bot_dial_resp.hovered() { terra } else { dial_stroke },
            ),
            egui::StrokeKind::Inside,
        );
        let phase_dy = ((state.cut_y_mm * 100.0).rem_euclid(5.0) / 5.0) as f32;
        for ti in 0..=n_ticks_h {
            let frac = ((ti as f32 + phase_dy).rem_euclid(n_ticks_h as f32) / (n_ticks_h as f32))
                .clamp(0.0, 1.0);
            let tx = bot_dial_rect.left() + 9.0 + frac * (bot_dial_rect.width() - 18.0);
            let is_mid = ti == n_ticks_h / 2;
            let th = if is_mid { 12.0 } else if ti % 4 == 0 { 9.0 } else { 6.0 };
            painter.line_segment(
                [
                    egui::pos2(tx, bot_dial_rect.center().y - th * 0.5),
                    egui::pos2(tx, bot_dial_rect.center().y + th * 0.5),
                ],
                egui::Stroke::new(
                    if is_mid { 1.3_f32 } else { 0.8_f32 },
                    if is_mid { tick_major_col } else { tick_col },
                ),
            );
        }
        // Image 2: 휠 아래 설명 ("이것은 유물이단면선 앞뒤로 미세조정휠 휠아래설명")
        let bot_lbl_rect = egui::Rect::from_center_size(
            egui::pos2(bot_dial_rect.center().x, bot_dial_rect.bottom() + 9.0),
            egui::vec2(236.0, 15.0),
        );
        painter.rect_filled(bot_lbl_rect, 3.0, label_bg);
        painter.text(
            bot_lbl_rect.center(),
            egui::Align2::CENTER_CENTER,
            format!(
                "▲▼ 유물·단면선 앞뒤 미세조정 휠 (ΔY={:+.2}mm)",
                state.cut_y_mm
            ),
            egui::FontId::proportional(9.5),
            ink,
        );

        // 4. Image 4 (좌측 세로 휠): 유물의 아래위를 회전하는 휠 (올리면 위로, 내리면 아래로) + 휠 옆에 설명!
        painter.rect_filled(left_dial_rect, 10.5, dial_fill);
        painter.rect_stroke(
            left_dial_rect,
            10.5,
            egui::Stroke::new(
                if left_dial_resp.hovered() { 1.4_f32 } else { 1.0_f32 },
                if left_dial_resp.hovered() { terra } else { dial_stroke },
            ),
            egui::StrokeKind::Inside,
        );
        let n_ticks_v = 26usize;
        let phase_tilt = ((state.tilt_deg * 10.0).rem_euclid(5.0) / 5.0) as f32;
        for ti in 0..=n_ticks_v {
            let frac = ((ti as f32 - phase_tilt).rem_euclid(n_ticks_v as f32) / (n_ticks_v as f32))
                .clamp(0.0, 1.0);
            let ty = left_dial_rect.top() + 9.0 + frac * (left_dial_rect.height() - 18.0);
            let is_mid = ti == n_ticks_v / 2;
            let tw = if is_mid { 15.0 } else if ti % 4 == 0 { 10.5 } else { 7.0 };
            painter.line_segment(
                [
                    egui::pos2(left_dial_rect.center().x - tw * 0.5, ty),
                    egui::pos2(left_dial_rect.center().x + tw * 0.5, ty),
                ],
                egui::Stroke::new(
                    if is_mid { 1.3_f32 } else { 0.8_f32 },
                    if is_mid { tick_major_col } else { tick_col },
                ),
            );
        }
        // Image 4: 좌측 세로 휠 바로 위쪽에 설명 표시 (유물 및 파란색 바운딩박스와 절대 겹치지 않도록 배치!)
        let left_side_lbl = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 4.0, (left_dial_rect.top() - 40.0).max(rect.top() + 56.0)),
            egui::vec2(88.0, 36.0),
        );
        painter.rect_filled(left_side_lbl, 3.0, label_bg);
        painter.text(
            egui::pos2(left_side_lbl.left() + 3.0, left_side_lbl.top() + 9.0),
            egui::Align2::LEFT_CENTER,
            "↑올리면 위로회전",
            egui::FontId::proportional(8.6),
            ink,
        );
        painter.text(
            egui::pos2(left_side_lbl.left() + 3.0, left_side_lbl.top() + 19.0),
            egui::Align2::LEFT_CENTER,
            "↓내리면 아래회전",
            egui::FontId::proportional(8.6),
            ink,
        );
        painter.text(
            egui::pos2(left_side_lbl.left() + 3.0, left_side_lbl.top() + 29.0),
            egui::Align2::LEFT_CENTER,
            format!("상하 {:+.1}°", state.tilt_deg),
            egui::FontId::monospace(8.5),
            terra,
        );

        // 5. Image 3 (우측 세로 휠): 유물의 좌우 기울기를 미세조정하는 휠 + 휠 위쪽에 설명!
        painter.rect_filled(right_dial_rect, 10.5, dial_fill);
        painter.rect_stroke(
            right_dial_rect,
            10.5,
            egui::Stroke::new(
                if right_dial_resp.hovered() { 1.4_f32 } else { 1.0_f32 },
                if right_dial_resp.hovered() { terra } else { dial_stroke },
            ),
            egui::StrokeKind::Inside,
        );
        let phase_roll = ((state.roll_deg * 10.0).rem_euclid(5.0) / 5.0) as f32;
        for ti in 0..=n_ticks_v {
            let frac = ((ti as f32 - phase_roll).rem_euclid(n_ticks_v as f32) / (n_ticks_v as f32))
                .clamp(0.0, 1.0);
            let ty = right_dial_rect.top() + 9.0 + frac * (right_dial_rect.height() - 18.0);
            let is_mid = ti == n_ticks_v / 2;
            let tw = if is_mid { 15.0 } else if ti % 4 == 0 { 10.5 } else { 7.0 };
            painter.line_segment(
                [
                    egui::pos2(right_dial_rect.center().x - tw * 0.5, ty),
                    egui::pos2(right_dial_rect.center().x + tw * 0.5, ty),
                ],
                egui::Stroke::new(
                    if is_mid { 1.3_f32 } else { 0.8_f32 },
                    if is_mid { tick_major_col } else { tick_col },
                ),
            );
        }
        // Image 3: 우측 세로 휠 바로 위쪽에 설명 표시 (유물 및 파란색 바운딩박스와 절대 겹치지 않도록 배치!)
        let right_side_lbl = egui::Rect::from_min_size(
            egui::pos2(rect.right() - 88.0, (right_dial_rect.top() - 40.0).max(rect.top() + 56.0)),
            egui::vec2(84.0, 36.0),
        );
        painter.rect_filled(right_side_lbl, 3.0, label_bg);
        painter.text(
            egui::pos2(right_side_lbl.right() - 3.0, right_side_lbl.top() + 9.0),
            egui::Align2::RIGHT_CENTER,
            "유물 좌우기울기",
            egui::FontId::proportional(8.8),
            ink,
        );
        painter.text(
            egui::pos2(right_side_lbl.right() - 3.0, right_side_lbl.top() + 19.0),
            egui::Align2::RIGHT_CENTER,
            "미세조정 휠",
            egui::FontId::proportional(8.8),
            ink,
        );
        painter.text(
            egui::pos2(right_side_lbl.right() - 3.0, right_side_lbl.top() + 29.0),
            egui::Align2::RIGHT_CENTER,
            format!("기울기 {:+.1}°", state.roll_deg),
            egui::FontId::monospace(8.5),
            terra,
        );

        // 상단 1줄 상태 및 셰이더/정위치 칩 바
        let top_bar_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 6.0, rect.top() + 4.0),
            egui::vec2(rect.width() - 12.0, 19.0),
        );
        painter.rect_filled(top_bar_rect, 3.0, label_bg);
        painter.text(
            egui::pos2(top_bar_rect.left() + 6.0, top_bar_rect.center().y),
            egui::Align2::LEFT_CENTER,
            format!(
                "수평 정면 입면 (ψ={:.1}° · 상하={:+.1}° · 좌우={:+.1}° · 수평보정={:+.2}°)",
                state.active_section_angle_deg,
                state.tilt_deg,
                state.roll_deg,
                state.inner_level_deg
            ),
            egui::FontId::proportional(10.0),
            ink,
        );

        let modes = SHADER_CHIP_MODES;
        let mut chip_x = rect.left() + 6.0;
        let chip_y = rect.top() + 26.0;
        for m in modes {
            let label = m.label_kr();
            let cw = (label.chars().count() as f32) * 10.0 + 10.0;
            let c_rect =
                egui::Rect::from_min_size(egui::pos2(chip_x, chip_y), egui::vec2(cw, 19.0));
            let c_resp = ui.interact(
                c_rect,
                egui::Id::new(("v3d_shader_chip", label)),
                egui::Sense::click(),
            );
            let is_on = match m {
                View3DShaderMode::Segmentation3Way => state.show_3way_segmentation,
                View3DShaderMode::XRayInner => state.xray_inner_mode,
                _ => {
                    !state.show_3way_segmentation
                        && !state.xray_inner_mode
                        && state.shader_mode == m
                }
            };
            if c_resp.clicked() {
                match m {
                    View3DShaderMode::Segmentation3Way => {
                        state.show_3way_segmentation = true;
                        state.xray_inner_mode = false;
                    }
                    View3DShaderMode::XRayInner => {
                        state.xray_inner_mode = true;
                        state.show_3way_segmentation = false;
                    }
                    _ => {
                        state.show_3way_segmentation = false;
                        state.xray_inner_mode = false;
                        state.shader_mode = m;
                    }
                }
            }
            painter.rect_filled(c_rect, 3.0, if is_on { ink } else { surface });
            painter.rect_stroke(
                c_rect,
                3.0,
                egui::Stroke::new(1.0_f32, if is_on { ink } else { border }),
                egui::StrokeKind::Inside,
            );
            painter.text(
                c_rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(10.0),
                if is_on { hanji } else { ink },
            );
            chip_x += cw + 3.0;
        }

        for (btn_id, btn_label, is_accent) in [
            ("v3d_reset_front_chip", "정위치·수평 0°".to_string(), false),
            (
                "v3d_best_angle_chip",
                format!("★최적 {:.0}°", best_angle_deg),
                true,
            ),
        ] {
            let cw = (btn_label.chars().count() as f32) * 9.2 + 10.0;
            let c_rect =
                egui::Rect::from_min_size(egui::pos2(chip_x, chip_y), egui::vec2(cw, 19.0));
            let c_resp = ui.interact(c_rect, egui::Id::new(btn_id), egui::Sense::click());
            if c_resp.clicked() {
                if is_accent {
                    state.active_section_angle_deg = best_angle_deg;
                    state.cut_y_mm = 0.0;
                    state.yaw_deg = 0.0;
                    state.pitch_deg = 0.0;
                } else {
                    state.reset_hand_orientation();
                }
            }
            painter.rect_filled(c_rect, 3.0, if is_accent { clay_wash } else { surface });
            painter.rect_stroke(
                c_rect,
                3.0,
                egui::Stroke::new(1.0_f32, if is_accent { terra } else { border }),
                egui::StrokeKind::Inside,
            );
            painter.text(
                c_rect.center(),
                egui::Align2::CENTER_CENTER,
                &btn_label,
                egui::FontId::proportional(10.0),
                if is_accent { clay_ink } else { ink },
            );
            chip_x += cw + 3.0;
        }

        // Image 2 ("광원을 조정할수있는 작은창"): [★최적 ...] 버튼 바로 우측 자리에 광원 토글 칩 및 광원 조정 작은창 배치!
        let light_chip_lbl = format!(
            "💡광원 {:.0}°/{:.0}°",
            state.light_azimuth_deg, state.light_elevation_deg
        );
        let lcw = 86.0_f32;
        let lc_rect = egui::Rect::from_min_size(egui::pos2(chip_x, chip_y), egui::vec2(lcw, 19.0));
        let lc_resp = ui.interact(
            lc_rect,
            egui::Id::new("v3d_light_window_toggle_chip"),
            egui::Sense::click(),
        );
        if lc_resp.clicked() {
            state.show_light_window = !state.show_light_window;
        }
        painter.rect_filled(
            lc_rect,
            3.0,
            if state.show_light_window {
                egui::Color32::from_rgb(252, 243, 215)
            } else {
                surface
            },
        );
        painter.rect_stroke(
            lc_rect,
            3.0,
            egui::Stroke::new(
                1.1_f32,
                if state.show_light_window {
                    terra
                } else {
                    border
                },
            ),
            egui::StrokeKind::Inside,
        );
        painter.text(
            lc_rect.center(),
            egui::Align2::CENTER_CENTER,
            &light_chip_lbl,
            egui::FontId::proportional(9.2),
            ink,
        );

        if state.show_light_window {
            let lw_w = 172.0_f32;
            let lw_h = 64.0_f32;
            let lw_x = (rect.center().x - lw_w * 0.5).clamp(rect.left() + 8.0, rect.right() - lw_w - 8.0);
            let lw_y = rect.top() + 114.0;
            let lw_rect = egui::Rect::from_min_size(egui::pos2(lw_x, lw_y), egui::vec2(lw_w, lw_h));
            painter.rect_filled(
                lw_rect,
                4.5,
                egui::Color32::from_rgba_unmultiplied(250, 249, 245, 244),
            );
            painter.rect_stroke(
                lw_rect,
                4.5,
                egui::Stroke::new(1.2_f32, terra),
                egui::StrokeKind::Inside,
            );

            // 좌측: 2D 광원 구면 패드 (드래그 시 방위각·고도각 즉시 변경)
            let pad_r = 19.0_f32;
            let pad_center = egui::pos2(lw_rect.left() + 24.0, lw_rect.center().y + 4.0);
            let pad_rect =
                egui::Rect::from_center_size(pad_center, egui::vec2(pad_r * 2.0, pad_r * 2.0));
            let pad_resp = ui.interact(
                pad_rect,
                egui::Id::new("v3d_light_sphere_pad"),
                egui::Sense::click_and_drag(),
            );
            if pad_resp.clicked() || pad_resp.dragged() {
                if let Some(p) = pad_resp.interact_pointer_pos() {
                    let dx = (p.x - pad_center.x) / pad_r;
                    let dy = (p.y - pad_center.y) / pad_r;
                    let r_norm = dx.hypot(dy).clamp(0.0, 1.0);
                    let az = dx.atan2(-dy).to_degrees().rem_euclid(360.0);
                    let el = ((1.0 - r_norm) * 80.0 + 5.0).clamp(5.0, 85.0);
                    state.light_azimuth_deg = (az * 2.0).round() / 2.0;
                    state.light_elevation_deg = (el * 2.0).round() / 2.0;
                }
            }
            painter.circle_filled(
                pad_center,
                pad_r,
                egui::Color32::from_rgb(236, 232, 222),
            );
            painter.circle_stroke(pad_center, pad_r, egui::Stroke::new(1.0_f32, border));
            painter.line_segment(
                [
                    egui::pos2(pad_center.x - pad_r, pad_center.y),
                    egui::pos2(pad_center.x + pad_r, pad_center.y),
                ],
                egui::Stroke::new(0.6_f32, tick_col),
            );
            painter.line_segment(
                [
                    egui::pos2(pad_center.x, pad_center.y - pad_r),
                    egui::pos2(pad_center.x, pad_center.y + pad_r),
                ],
                egui::Stroke::new(0.6_f32, tick_col),
            );
            let az_r = state.light_azimuth_deg.to_radians();
            let rad_frac = ((85.0 - state.light_elevation_deg) / 80.0).clamp(0.0, 1.0);
            let sun_pos = egui::pos2(
                pad_center.x + az_r.sin() * pad_r * rad_frac,
                pad_center.y - az_r.cos() * pad_r * rad_frac,
            );
            painter.circle_filled(sun_pos, 4.5, egui::Color32::from_rgb(235, 152, 28));
            painter.circle_stroke(sun_pos, 4.5, egui::Stroke::new(1.0_f32, ink));
            painter.text(
                egui::pos2(lw_rect.left() + 6.0, lw_rect.top() + 8.0),
                egui::Align2::LEFT_CENTER,
                "💡 광원 조정",
                egui::FontId::proportional(8.8),
                terra,
            );

            // 우측: 방위각 · 고도각 · 밝기 강도 및 [좌상45°] [측면광] 프리셋 버튼
            let ctrl_rect = egui::Rect::from_min_max(
                egui::pos2(lw_rect.left() + 47.0, lw_rect.top() + 3.0),
                egui::pos2(lw_rect.right() - 3.0, lw_rect.bottom() - 3.0),
            );
            let mut l_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(ctrl_rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            l_ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
            l_ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("좌상45°").size(8.2).color(ink))
                            .min_size(egui::vec2(44.0, 15.0)),
                    )
                    .clicked()
                {
                    state.light_azimuth_deg = 315.0;
                    state.light_elevation_deg = 45.0;
                    state.light_intensity = 1.0;
                }
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("측면광").size(8.2).color(ink))
                            .min_size(egui::vec2(40.0, 15.0)),
                    )
                    .clicked()
                {
                    state.light_azimuth_deg = 275.0;
                    state.light_elevation_deg = 18.0;
                    state.light_intensity = 1.18;
                }
            });
            l_ui.horizontal(|ui| {
                ui.label(egui::RichText::new("방위").size(8.2).color(ink));
                let _ = ui.add(
                    egui::DragValue::new(&mut state.light_azimuth_deg)
                        .speed(1.0)
                        .range(0.0..=360.0)
                        .suffix("°")
                        .fixed_decimals(0),
                );
                ui.label(egui::RichText::new("고도").size(8.2).color(ink));
                let _ = ui.add(
                    egui::DragValue::new(&mut state.light_elevation_deg)
                        .speed(0.5)
                        .range(5.0..=85.0)
                        .suffix("°")
                        .fixed_decimals(0),
                );
            });
            l_ui.horizontal(|ui| {
                ui.label(egui::RichText::new("광량").size(8.2).color(ink));
                let _ = ui.add(
                    egui::DragValue::new(&mut state.light_intensity)
                        .speed(0.02)
                        .range(0.40..=1.80)
                        .suffix("×")
                        .fixed_decimals(2),
                );
            });
        }

        // =========================================================================================
        // Image 6 ("여기 이자리에 유물 뒤집기 90도 180도 회전을 할수있는 버튼을 만들라 뒤집횐 유물이있을수있다.")
        // 직상방 동심원 뷰 바로 아래 / 수평 정면 입면 상단 자리에 유물 90° 앞으로뒤집기 · 180° 상하뒤집기 · 90° 수평회전 · 측면뒤집기 버튼 배치!
        // =========================================================================================
        let flip_row_y = rect.top() + 48.0;
        let mut flip_x = rect.left() + 6.0;
        let flip_buttons = [
            ("v3d_flip_fwd_90", "↻ 유물 90° 앞으로뒤집기", 116.0_f32, 0u8),
            ("v3d_flip_ud_180", "↕ 유물 180° 상하뒤집기", 114.0_f32, 1u8),
            ("v3d_rot_yaw_90", "↻ 90° 수평회전", 82.0_f32, 2u8),
            ("v3d_flip_roll_90", "↔ 90° 측면뒤집기", 92.0_f32, 3u8),
        ];
        for (fid, flabel, fw, fkind) in flip_buttons {
            let f_rect =
                egui::Rect::from_min_size(egui::pos2(flip_x, flip_row_y), egui::vec2(fw, 20.0));
            let f_resp = ui.interact(f_rect, egui::Id::new(fid), egui::Sense::click());
            if f_resp.clicked() {
                match fkind {
                    0 => state.flip_artifact_forward_90(),
                    1 => state.flip_artifact_upside_down_180(),
                    2 => state.rotate_artifact_yaw_90(),
                    _ => state.flip_artifact_roll_90(),
                }
            }
            let is_flipped_active = (fkind == 0 || fkind == 1) && state.tilt_deg.abs() >= 45.0
                || (fkind == 3 && state.roll_deg.abs() >= 45.0);
            painter.rect_filled(
                f_rect,
                3.5,
                if is_flipped_active {
                    profile_red
                } else if f_resp.hovered() {
                    clay_wash
                } else {
                    egui::Color32::from_rgb(244, 238, 228)
                },
            );
            painter.rect_stroke(
                f_rect,
                3.5,
                egui::Stroke::new(
                    1.1_f32,
                    if is_flipped_active || f_resp.hovered() {
                        profile_red
                    } else {
                        terra
                    },
                ),
                egui::StrokeKind::Inside,
            );
            painter.text(
                f_rect.center(),
                egui::Align2::CENTER_CENTER,
                flabel,
                egui::FontId::proportional(8.8),
                if is_flipped_active {
                    egui::Color32::WHITE
                } else {
                    clay_ink
                },
            );
            flip_x += fw + 3.0;
        }

        let _ = muted;
        painter.rect_stroke(
            rect,
            3.0,
            egui::Stroke::new(1.0_f32, border),
            egui::StrokeKind::Inside,
        );
    });
}
