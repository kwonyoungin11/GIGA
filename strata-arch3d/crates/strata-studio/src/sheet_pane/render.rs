use super::config::*;
use super::offscreen::*;
use super::rubbing::*;
use super::state::*;
use super::widgets::*;
use eframe::egui;
use strata_core::strata_tokens::tokens::{
    ACTION, DONE_BG, DONE_INK, EDGE, GRID_1MM, GROUND, INK, LINE, MUTED, PROFILE_RED, SELECT_BLUE,
    SURFACE, WASH,
};
use strata_core::{ScalePreset, TriMesh};
use strata_export::{triangulate_polygon_ear_clipping, ComposedPlate2D, PlateRasterKind};
use strata_measure::ArtifactMeasurementReport;

/// 우측 `73%` (또는 `Tab` 키 `100%`) 2D 실측 도판 작업대(`SheetPane`)를 렌더링한다.
#[allow(clippy::too_many_arguments)]
pub fn render_sheet_pane_egui(
    ui: &mut egui::Ui,
    plate: &ComposedPlate2D,
    report: &ArtifactMeasurementReport,
    mesh: &TriMesh,
    psi_deg: f64,
    ink_density_pct: f64,
    show_mm_grid: bool,
    state: &mut SheetPaneState,
) -> (egui::Rect, egui::Rect, egui::Rect) {
    let avail = ui.available_size();
    let canvas_h = avail.y.max(160.0);

    let _ground = egui::Color32::from_rgb(GROUND.rgb[0], GROUND.rgb[1], GROUND.rgb[2]);
    let _wash = egui::Color32::from_rgb(WASH.rgb[0], WASH.rgb[1], WASH.rgb[2]);
    let paper = egui::Color32::from_rgb(SURFACE.rgb[0], SURFACE.rgb[1], SURFACE.rgb[2]);
    let border = egui::Color32::from_rgb(EDGE.rgb[0], EDGE.rgb[1], EDGE.rgb[2]);
    let _line_col = egui::Color32::from_rgb(LINE.rgb[0], LINE.rgb[1], LINE.rgb[2]);
    let grid_10 = egui::Color32::from_rgb(198, 192, 174);
    let grid_5 = egui::Color32::from_rgb(220, 215, 200);
    let grid_1 = egui::Color32::from_rgb(GRID_1MM.rgb[0], GRID_1MM.rgb[1], GRID_1MM.rgb[2]);
    let ink = egui::Color32::from_rgb(INK.rgb[0], INK.rgb[1], INK.rgb[2]);
    let muted = egui::Color32::from_rgb(MUTED.rgb[0], MUTED.rgb[1], MUTED.rgb[2]);
    let terra = egui::Color32::from_rgb(ACTION.rgb[0], ACTION.rgb[1], ACTION.rgb[2]);
    let profile_red =
        egui::Color32::from_rgb(PROFILE_RED.rgb[0], PROFILE_RED.rgb[1], PROFILE_RED.rgb[2]);
    let select_blue =
        egui::Color32::from_rgb(SELECT_BLUE.rgb[0], SELECT_BLUE.rgb[1], SELECT_BLUE.rgb[2]);
    let _done_bg = egui::Color32::from_rgb(DONE_BG.rgb[0], DONE_BG.rgb[1], DONE_BG.rgb[2]);
    let _done_fg = egui::Color32::from_rgb(DONE_INK.rgb[0], DONE_INK.rgb[1], DONE_INK.rgb[2]);

    if (state.rubbing_params.ink_density_pct - ink_density_pct).abs() > 0.5
        && !state.show_roi_rubbing_window
    {
        // Keep user wheel adjustments primary
    }

    let rub_view_tf = state.view_transforms[2];
    let key: SheetRubbingCacheKey = (
        (
            mesh.name.clone(),
            psi_deg.round() as i32,
            (state.rubbing_params.pose_tilt_deg * 10.0).round() as i32,
            (state.rubbing_params.pose_roll_deg * 10.0).round() as i32,
            state.rubbing_params.light_azimuth_deg.round() as i32,
            state.rubbing_params.light_elevation_deg.round() as i32,
            (state.rubbing_params.light_intensity * 100.0).round() as i32,
        ),
        (
            state.rubbing_params.ink_density_pct.round() as i32,
            (state.rubbing_params.msii_detail_boost * 100.0).round() as i32,
            (state.rubbing_params.sigmoid_contrast * 10.0).round() as i32,
            (state.rubbing_params.contact_threshold * 100.0).round() as i32,
            state.rubbing_params.style,
            plate.flip_exterior_right,
            None,
            rub_view_tf,
            state.rubbing_params.unroll_mode,
        ),
    );
    // Rayon 3-뷰 동시 병렬 탁본 버퍼 생성 (560x560 고선명 + 멀티코어 병렬화로 렉 제로 달성!)
    // Image 7: 우측 상단 1:1 정밀 탁본(bottom_img)은 개별 뷰의 [🔁앞으로], [🔃180°], [↻90°] 조작에 따라
    // 연속으로 앞으로 뒤집힌 면(외저면 -> 정면외면 -> 상하반전 -> 내면/후면) 및 90°/180° 회전이 즉시 반영됨!
    if state.cached_key.as_ref() != Some(&key) {
        let params_copy = state.rubbing_params.clone();
        // 곡면 전개 시 실제 물리 크기(둘레 × 높이)와 그에 맞는 텍스처 해상도를 쓴다.
        state.unrolled_size_mm = params_copy.unroll_mode.map(|mode| {
            let u = strata_shade::unroll_mesh_surface(mesh, mode);
            [u.unrolled_width_mm, u.unrolled_height_mm]
        });
        let (bw, bh) = match state.unrolled_size_mm {
            Some(size) => unrolled_texture_dims(size, 520),
            None => (520, 520),
        };
        let (front_img, (back_img, mut bottom_img)) = rayon::join(
            || {
                render_orthographic_rubbing_rgba_with_params(
                    mesh,
                    PlateRasterKind::FrontExteriorRubbing,
                    psi_deg,
                    &params_copy,
                    560,
                    560,
                )
            },
            || {
                rayon::join(
                    || {
                        render_orthographic_rubbing_rgba_with_params(
                            mesh,
                            PlateRasterKind::BackInteriorRubbing,
                            psi_deg,
                            &params_copy,
                            520,
                            520,
                        )
                    },
                    || {
                        let flipped_kind = match rub_view_tf.flip_forward_steps % 4 {
                            1 => PlateRasterKind::FrontExteriorRubbing,
                            3 => PlateRasterKind::BackInteriorRubbing,
                            _ => PlateRasterKind::BottomPlanRubbing,
                        };
                        render_orthographic_rubbing_rgba_with_params(
                            mesh,
                            flipped_kind,
                            psi_deg,
                            &params_copy,
                            bw,
                            bh,
                        )
                    },
                )
            },
        );
        if rub_view_tf.flip_forward_steps % 4 == 2 {
            bottom_img = image::imageops::flip_vertical(&bottom_img);
        }
        bottom_img = match rub_view_tf.rot_90_steps % 4 {
            1 => image::imageops::rotate90(&bottom_img),
            2 => image::imageops::rotate180(&bottom_img),
            3 => image::imageops::rotate270(&bottom_img),
            _ => bottom_img,
        };
        state.cached_front_img = Some(front_img);
        state.cached_back_img = Some(back_img);
        state.cached_bottom_img = Some(bottom_img);
        state.cached_front_tex = None;
        state.cached_back_tex = None;
        state.cached_bottom_tex = None;
        state.cached_key = Some(key);
    }

    let mut out_header_rect = egui::Rect::NOTHING;
    let mut out_strip_rect = egui::Rect::NOTHING;
    let mut out_sheet_rect = egui::Rect::NOTHING;

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;

        let (sheet_rect, sheet_resp) = ui.allocate_exact_size(
            egui::vec2(avail.x, canvas_h),
            egui::Sense::click_and_drag(),
        );
        out_header_rect =
            egui::Rect::from_min_size(sheet_rect.min, egui::vec2(sheet_rect.width(), 0.0));
        out_strip_rect =
            egui::Rect::from_min_size(sheet_rect.min, egui::vec2(sheet_rect.width(), 0.0));
        out_sheet_rect = sheet_rect;

        // Image 3: 조판(Publication Page Layout) 용지 규격 및 용지 내부 전용 방안선 + 5cm(50mm) 빨간색선 명세
        let spec = compute_paper_sheet_graph_spec(
            sheet_rect,
            state.paper_preset,
            state.zoom,
            state.pan_px,
        );
        let p_rect = spec.paper_bounds_rect;
        let paper_scale = spec.scale_px_per_mm;
        // 대형 유물: 1:1 도판이 용지보다 크면 자동으로 축척을 낮춘다(사용자가 직접 선택하면 고정).
        if !state.scale_locked {
            let (pw, ph) = state.paper_preset.dimensions_mm();
            state.active_scale_preset = fit_scale_preset(
                plate.canvas_width_mm,
                plate.canvas_height_mm,
                pw,
                ph,
                12.0,
            );
        }
        let scale_factor = (state.active_scale_preset.ratio() as f32).clamp(0.2, 5.0);
        let scale = paper_scale * scale_factor;
        let cw_px = (plate.canvas_width_mm as f32) * scale;
        let ch_px = (plate.canvas_height_mm as f32) * scale;
        let origin_x = p_rect.center().x - cw_px * 0.48;
        let origin_y = p_rect.center().y + ch_px * 0.48;

        // 상단 용지설정·부분탁본 바, 선조정 바, 하단 탁본휠·축척설정 바 영역과의 충돌 방지
        let top_style_bar_rect = egui::Rect::from_min_size(
            egui::pos2(sheet_rect.left() + 12.0, sheet_rect.top() + 6.0),
            egui::vec2((sheet_rect.width() - 276.0).clamp(320.0, 620.0), 26.0),
        );
        let bot_scale_bar_rect = egui::Rect::from_min_size(
            egui::pos2((sheet_rect.right() - 328.0).max(sheet_rect.left() + 12.0), sheet_rect.bottom() - 34.0),
            egui::vec2(318.0, 26.0),
        );
        let pointer_in_bars = sheet_resp.interact_pointer_pos().is_some_and(|p| {
            p.y < sheet_rect.top() + 62.0 || p.y > sheet_rect.bottom() - 68.0
        });

        if sheet_resp.hovered() && !pointer_in_bars {
            let scroll_y = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll_y.abs() > 0.1 {
                let factor = (1.0 + scroll_y * 0.0025).clamp(0.85, 1.18);
                state.zoom = (state.zoom * factor).clamp(0.45, 4.0);
            }
        }

        let shift_held = ui.input(|i| i.modifiers.shift);
        if !pointer_in_bars && (state.roi_drag_mode || shift_held) {
            if sheet_resp.drag_started() {
                if let Some(pos) = sheet_resp.interact_pointer_pos() {
                    let mm_x = f64::from((pos.x - origin_x) / scale);
                    let mm_y = f64::from((origin_y - pos.y) / scale);
                    state.roi_drag_start_mm = Some([mm_x, mm_y]);
                }
            }
            if sheet_resp.dragged() {
                if let (Some(start_mm), Some(pos)) =
                    (state.roi_drag_start_mm, sheet_resp.interact_pointer_pos())
                {
                    let cur_x = f64::from((pos.x - origin_x) / scale);
                    let cur_y = f64::from((origin_y - pos.y) / scale);
                    state.set_rubbing_roi_mm(Some([start_mm[0], start_mm[1], cur_x, cur_y]));
                }
            }
            if sheet_resp.drag_stopped() {
                state.roi_drag_start_mm = None;
            }
        } else if !pointer_in_bars {
            if sheet_resp.dragged() {
                state.pan_px += sheet_resp.drag_delta();
            }
            if sheet_resp.double_clicked() {
                state.zoom = 1.0;
                state.pan_px = egui::Vec2::ZERO;
            }
        }

        let paper_painter = ui.painter_at(sheet_rect);
        // "방안지 여백은 무한대로 배경을 하고 용지만 붉은선으로 표시하게 하자":
        // sheet_rect 전체를 한지 방안지 배경색(paper)으로 채우고, 방안선을 화면 끝에서 끝까지 무한대로 그린다!
        paper_painter.rect_filled(sheet_rect, 0.0, paper);

        if show_mm_grid {
            for &gx in &spec.lines_1mm_x {
                paper_painter.line_segment(
                    [egui::pos2(gx, sheet_rect.top()), egui::pos2(gx, sheet_rect.bottom())],
                    egui::Stroke::new(0.45_f32, grid_1),
                );
            }
            for &gy in &spec.lines_1mm_y {
                paper_painter.line_segment(
                    [egui::pos2(sheet_rect.left(), gy), egui::pos2(sheet_rect.right(), gy)],
                    egui::Stroke::new(0.45_f32, grid_1),
                );
            }
            for &gx in &spec.lines_5mm_x {
                paper_painter.line_segment(
                    [egui::pos2(gx, sheet_rect.top()), egui::pos2(gx, sheet_rect.bottom())],
                    egui::Stroke::new(0.70_f32, grid_5),
                );
            }
            for &gy in &spec.lines_5mm_y {
                paper_painter.line_segment(
                    [egui::pos2(sheet_rect.left(), gy), egui::pos2(sheet_rect.right(), gy)],
                    egui::Stroke::new(0.70_f32, grid_5),
                );
            }
            for &gx in &spec.lines_10mm_x {
                paper_painter.line_segment(
                    [egui::pos2(gx, sheet_rect.top()), egui::pos2(gx, sheet_rect.bottom())],
                    egui::Stroke::new(0.95_f32, grid_10),
                );
            }
            for &gy in &spec.lines_10mm_y {
                paper_painter.line_segment(
                    [egui::pos2(sheet_rect.left(), gy), egui::pos2(sheet_rect.right(), gy)],
                    egui::Stroke::new(0.95_f32, grid_10),
                );
            }
            // 5cm(50mm)마다 얇은 빨간색선!
            let red_5cm_col = egui::Color32::from_rgba_unmultiplied(218, 46, 36, 175);
            for &rx in &spec.lines_50mm_red_x {
                paper_painter.line_segment(
                    [egui::pos2(rx, sheet_rect.top()), egui::pos2(rx, sheet_rect.bottom())],
                    egui::Stroke::new(1.00_f32, red_5cm_col),
                );
            }
            for &ry in &spec.lines_50mm_red_y {
                paper_painter.line_segment(
                    [egui::pos2(sheet_rect.left(), ry), egui::pos2(sheet_rect.right(), ry)],
                    egui::Stroke::new(1.00_f32, red_5cm_col),
                );
            }

            for (gx, lbl) in &spec.cm_labels_top {
                if *gx >= p_rect.left() + 4.0 && *gx <= p_rect.right() - 22.0 {
                    let is_5cm = lbl == "0cm" || lbl == "5cm" || lbl == "10cm" || lbl == "15cm" || lbl == "20cm" || lbl == "25cm" || lbl == "30cm" || lbl == "35cm" || lbl == "40cm";
                    paper_painter.text(
                        egui::pos2(*gx + 2.0, p_rect.top() + 7.0),
                        egui::Align2::LEFT_CENTER,
                        lbl,
                        egui::FontId::monospace(8.5),
                        if is_5cm { red_5cm_col } else { muted },
                    );
                }
            }
            for (gy, lbl) in &spec.cm_labels_left {
                if *gy >= p_rect.top() + 14.0 && *gy <= p_rect.bottom() - 8.0 {
                    let is_5cm = lbl == "0cm" || lbl == "5cm" || lbl == "10cm" || lbl == "15cm" || lbl == "20cm" || lbl == "25cm" || lbl == "30cm";
                    paper_painter.text(
                        egui::pos2(p_rect.left() + 3.0, *gy - 6.0),
                        egui::Align2::LEFT_CENTER,
                        lbl,
                        egui::FontId::monospace(8.5),
                        if is_5cm { red_5cm_col } else { muted },
                    );
                }
            }
        }

        // "용지만 붉은선으로 표시하게 하자": 출판 용지 경계(p_rect)를 선명한 붉은색 실선 테두리로 표시!
        let paper_red_border = egui::Color32::from_rgb(218, 34, 34);
        paper_painter.rect_stroke(
            p_rect,
            1.5,
            egui::Stroke::new(2.2_f32, paper_red_border),
            egui::StrokeKind::Inside,
        );
        let (pw_mm_lbl, ph_mm_lbl) = state.paper_preset.dimensions_mm();
        paper_painter.text(
            egui::pos2(p_rect.left() + 6.0, (p_rect.top() - 8.0).max(sheet_rect.top() + 38.0)),
            egui::Align2::LEFT_CENTER,
            format!(
                "📄 용지 경계 (붉은선): {} ({:.0}×{:.0}mm · 무한대 방안지 배경)",
                state.paper_preset.label_kr(),
                pw_mm_lbl,
                ph_mm_lbl
            ),
            egui::FontId::proportional(9.2),
            paper_red_border,
        );
        paper_painter.rect_stroke(
            sheet_rect,
            0.0,
            egui::Stroke::new(1.0_f32, border),
            egui::StrokeKind::Inside,
        );

        // `Delete` / `Backspace` 키 입력 시 현재 선택된 레이어 즉시 삭제 ("delete키로 지우기")
        if ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
            let _ = state.delete_selected_layer();
        }
        state.has_custom_section_layer = plate
            .paths
            .iter()
            .any(|p| p.layer_id.starts_with("추가단면_"));
        if state.has_custom_section_layer && !state.request_clear_custom_section {
            // 추가단면선 생성 시 레이어 목록에 활성 표시
            if state.selected_layer_idx.is_none() {
                state.selected_layer_idx = Some(4);
            }
        }

        // 2. 다중 뷰 정밀 탁본(`Rubbing`) 래스터 영역 합성 및 Image 0 / Image 1 반영:
        //    - Image 0 ("평단면을 그려야한다 이렇게나오는게 맞나 ? 이것은 일러스트에서 그려야한다. 내가."):
        //      좌측 1:1 평면도 + 1:1 정면 입·단면도에는 기본적으로 회색 래스터 탁본을 덮어씌우지 않고 순수 일러스트 벡터 실측 선화로 표시한다!
        //    - Image 1 ("탁본은 여기에 1:1로 여기에 나타나게하라."):
        //      우측 상단(`ri > 0` 또는 `BottomPlanRubbing`) 자리에 1:1 정밀 탁본이 선명하게 나타나게 한다!
        let mut right_rubbing_box_px: Option<egui::Rect> = None;

        for (ri, region) in plate.raster_regions.iter().enumerate() {
            // 곡면 전개 모드에서는 전개 띠 하나만 도판에 표시한다(다른 판과 겹치지 않도록).
            if state.unrolled_size_mm.is_some()
                && region.kind != PlateRasterKind::BottomPlanRubbing
            {
                continue;
            }
            let raster_layer_idx = if ri == 0 && region.kind == PlateRasterKind::FrontExteriorRubbing {
                1usize
            } else {
                2usize
            };
            if !state.view_transforms[raster_layer_idx].visible {
                continue;
            }
            if ri == 0
                && region.kind == PlateRasterKind::FrontExteriorRubbing
                && !state.show_elevation_rubbing_overlay
                && plate.raster_regions.len() > 1
            {
                // 좌측 평단면(1:1 평면도 + 1:1 정면 입·단면도)은 일러스트용 순수 벡터 실측 선화로 유지!
                continue;
            }
            let r_off = state.view_transforms[raster_layer_idx].offset_mm();
            let rx0 = origin_x + ((region.rect_mm[0] + r_off[0]) as f32) * scale;
            let ry1 = origin_y - ((region.rect_mm[1] + r_off[1]) as f32) * scale;
            let rx1 = origin_x + ((region.rect_mm[2] + r_off[0]) as f32) * scale;
            let ry0 = origin_y - ((region.rect_mm[3] + r_off[1]) as f32) * scale;
            let mut dst_rect = egui::Rect::from_min_max(
                egui::pos2(rx0.min(rx1), ry0.min(ry1)),
                egui::pos2(rx0.max(rx1), ry0.max(ry1)),
            );

            // 곡면 전개 탁본은 실제 (둘레 × 높이) mm 크기로 1:1 배치해야 모눈 위 계측이 성립한다.
            if region.kind == PlateRasterKind::BottomPlanRubbing {
                if let Some([sw, sh]) = state.unrolled_size_mm {
                    let w_px = (sw as f32) * scale;
                    let h_px = (sh as f32) * scale;
                    let center = dst_rect.center();
                    let mut rect = egui::Rect::from_center_size(center, egui::vec2(w_px, h_px));
                    let dx = if rect.left() < p_rect.left() {
                        p_rect.left() - rect.left()
                    } else if rect.right() > p_rect.right() {
                        p_rect.right() - rect.right()
                    } else {
                        0.0
                    };
                    let dy = if rect.top() < p_rect.top() {
                        p_rect.top() - rect.top()
                    } else if rect.bottom() > p_rect.bottom() {
                        p_rect.bottom() - rect.bottom()
                    } else {
                        0.0
                    };
                    rect = rect.translate(egui::vec2(dx, dy));
                    dst_rect = rect;
                    right_rubbing_box_px = Some(rect);
                }
            }

            // Image 14: 우측 탁본 영역(BottomPlanRubbing 또는 두 번째 보조 탁본 영역)은
            // 사용자가 드래그한 부분 탁본(rubbing_roi_mm)이 있을 때 그 드래그한 실제 크기(1:1 mm 스케일) 그대로 나타나게 함!
            if ri > 0 || region.kind == PlateRasterKind::BottomPlanRubbing {
                if let (Some([ox0, oy0, ox1, oy1]), Some(first_reg)) =
                    (state.rubbing_roi_mm, plate.raster_regions.first())
                {
                    let roi_w_px = (((ox1 - ox0).abs() as f32) * scale).clamp(48.0, 260.0);
                    let roi_h_px = (((oy1 - oy0).abs() as f32) * scale).clamp(48.0, 240.0);
                    let center_pt = dst_rect.center();
                    dst_rect = egui::Rect::from_center_size(center_pt, egui::vec2(roi_w_px, roi_h_px));
                    right_rubbing_box_px = Some(dst_rect);

                    // 드래그한 1:1 크기 탁본을 정면 탁본 텍스처의 해당 UV 영역에서 1:1로 직접 추출하여 표시!
                    if state.cached_front_tex.is_none() {
                        if let Some(ref src_img) = state.cached_front_img {
                            let color_img = rgba_to_color_image_clipped_with_roi(
                                src_img,
                                first_reg.rect_mm,
                                first_reg.clip_half_sign,
                                None,
                            );
                            state.cached_front_tex = Some(ui.ctx().load_texture(
                                "sheet_rubbing_front_0",
                                color_img,
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                    }
                    if let Some(ref ftex) = state.cached_front_tex {
                        let frx0 = first_reg.rect_mm[0].min(first_reg.rect_mm[2]);
                        let frx1 = first_reg.rect_mm[0].max(first_reg.rect_mm[2]);
                        let fry0 = first_reg.rect_mm[1].min(first_reg.rect_mm[3]);
                        let fry1 = first_reg.rect_mm[1].max(first_reg.rect_mm[3]);
                        let frw = (frx1 - frx0).max(1.0);
                        let frh = (fry1 - fry0).max(1.0);
                        let u0 = (((ox0 - frx0) / frw) as f32).clamp(0.0, 0.95);
                        let u1 = (((ox1 - frx0) / frw) as f32).clamp(u0 + 0.04, 1.0);
                        let v0 = (((fry1 - oy1) / frh) as f32).clamp(0.0, 0.95);
                        let v1 = (((fry1 - oy0) / frh) as f32).clamp(v0 + 0.04, 1.0);
                        paper_painter.rect_filled(dst_rect, 2.0, paper);
                        paper_painter.image(
                            ftex.id(),
                            dst_rect,
                            egui::Rect::from_min_max(egui::pos2(u0, v0), egui::pos2(u1, v1)),
                            egui::Color32::WHITE,
                        );
                        paper_painter.rect_stroke(
                            dst_rect,
                            2.0,
                            egui::Stroke::new(1.2_f32, select_blue),
                            egui::StrokeKind::Inside,
                        );
                        paper_painter.text(
                            egui::pos2(dst_rect.center().x, dst_rect.top() - 8.0),
                            egui::Align2::CENTER_CENTER,
                            format!(
                                "1:1 부분 탁본 ({:.1}×{:.1}mm)",
                                (ox1 - ox0).abs(),
                                (oy1 - oy0).abs()
                            ),
                            egui::FontId::proportional(10.0),
                            select_blue,
                        );
                        continue;
                    }
                } else {
                    right_rubbing_box_px = Some(dst_rect);
                }
            }

            let tex_id_opt = match region.kind {
                PlateRasterKind::FrontExteriorRubbing => {
                    if state.cached_front_tex.is_none() {
                        if let Some(ref src_img) = state.cached_front_img {
                            let color_img = rgba_to_color_image_clipped_with_roi(
                                src_img,
                                region.rect_mm,
                                region.clip_half_sign,
                                state.rubbing_roi_mm,
                            );
                            state.cached_front_tex = Some(ui.ctx().load_texture(
                                format!("sheet_rubbing_front_{ri}"),
                                color_img,
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                    }
                    state.cached_front_tex.as_ref().map(|t| t.id())
                }
                PlateRasterKind::BackInteriorRubbing => {
                    if state.cached_back_tex.is_none() {
                        if let Some(ref src_img) = state.cached_back_img {
                            let color_img = rgba_to_color_image_clipped_with_roi(
                                src_img,
                                region.rect_mm,
                                region.clip_half_sign,
                                state.rubbing_roi_mm,
                            );
                            state.cached_back_tex = Some(ui.ctx().load_texture(
                                format!("sheet_rubbing_back_{ri}"),
                                color_img,
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                    }
                    state.cached_back_tex.as_ref().map(|t| t.id())
                }
                PlateRasterKind::BottomPlanRubbing => {
                    if state.cached_bottom_tex.is_none() {
                        if let Some(ref src_img) = state.cached_bottom_img {
                            let color_img = rgba_to_color_image_clipped_with_roi(
                                src_img,
                                region.rect_mm,
                                region.clip_half_sign,
                                None,
                            );
                            state.cached_bottom_tex = Some(ui.ctx().load_texture(
                                format!("sheet_rubbing_bottom_{ri}"),
                                color_img,
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                    }
                    state.cached_bottom_tex.as_ref().map(|t| t.id())
                }
            };

            if let Some(tex_id) = tex_id_opt {
                paper_painter.image(
                    tex_id,
                    dst_rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
        }

        // 사용자가 드래그하여 지정한 탁본 ROI 사각형 표시
        if let Some([ox0, oy0, ox1, oy1]) = state.rubbing_roi_mm {
            let sx0 = origin_x + (ox0 as f32) * scale;
            let sy1 = origin_y - (oy0 as f32) * scale;
            let sx1 = origin_x + (ox1 as f32) * scale;
            let sy0 = origin_y - (oy1 as f32) * scale;
            let roi_rect = egui::Rect::from_min_max(
                egui::pos2(sx0.min(sx1), sy0.min(sy1)),
                egui::pos2(sx0.max(sx1), sy0.max(sy1)),
            );
            paper_painter.rect_stroke(
                roi_rect,
                2.0,
                egui::Stroke::new(1.6_f32, select_blue),
                egui::StrokeKind::Inside,
            );
            paper_painter.text(
                egui::pos2(
                    roi_rect.left() + 4.0,
                    (roi_rect.top() - 12.0).max(sheet_rect.top() + 48.0),
                ),
                egui::Align2::LEFT_TOP,
                format!(
                    "1:1 탁본 선택영역 ({:.1}×{:.1}mm)",
                    (ox1 - ox0).abs(),
                    (oy1 - oy0).abs()
                ),
                egui::FontId::proportional(10.0),
                select_blue,
            );
        }

        // =========================================================================================
        // Image 14: 탁본 농도·대비·선명도·한지여백 4종 미세조정 휠 — 도면 용지(p_rect) 밖 하단 좌측 도크에 4열 병렬 배치하여 도면과 절대 겹치지 않게 함!
        // =========================================================================================
        let _ = right_rubbing_box_px;
        let wheel_w = 136.0_f32;
        let wheel_h = 14.0_f32;
        let wheel_gap_x = 12.0_f32;
        let wheel_row_y = sheet_rect.bottom() - 36.0;
        let wheel_start_x = sheet_rect.left() + 12.0;

        let panel_bg_rect = egui::Rect::from_min_size(
            egui::pos2(wheel_start_x - 4.0, wheel_row_y - 4.0),
            egui::vec2(4.0 * wheel_w + 3.0 * wheel_gap_x + 8.0, 34.0),
        );
        paper_painter.rect_filled(
            panel_bg_rect,
            4.0,
            egui::Color32::from_rgba_unmultiplied(250, 249, 245, 242),
        );
        paper_painter.rect_stroke(
            panel_bg_rect,
            4.0,
            egui::Stroke::new(1.0_f32, border),
            egui::StrokeKind::Inside,
        );

        let wheel_specs = [
            (
                RubbingFineWheelKind::InkDensity,
                "rub_wheel_ink_density",
                "① 먹물농도",
                format!("{:.0}%", state.rubbing_params.ink_density_pct),
                ((state.rubbing_params.ink_density_pct as f32).rem_euclid(5.0)) / 5.0,
            ),
            (
                RubbingFineWheelKind::SigmoidContrast,
                "rub_wheel_sigmoid_contrast",
                "② 음양대비",
                format!("{:.1}", state.rubbing_params.sigmoid_contrast),
                (state.rubbing_params.sigmoid_contrast.rem_euclid(1.0)),
            ),
            (
                RubbingFineWheelKind::MsiiRidgeSharpness,
                "rub_wheel_msii_sharpness",
                "③ 문양선명도",
                format!("{:.2}×", state.rubbing_params.msii_detail_boost),
                ((state.rubbing_params.msii_detail_boost * 10.0).rem_euclid(1.0)),
            ),
            (
                RubbingFineWheelKind::PaperContactThreshold,
                "rub_wheel_contact_thresh",
                "④ 한지여백",
                format!("{:+.2}", state.rubbing_params.contact_threshold),
                ((state.rubbing_params.contact_threshold * 20.0).rem_euclid(1.0)),
            ),
        ];

        for (wi, (wkind, wid, wtitle, wval, wphase)) in wheel_specs.into_iter().enumerate() {
            let wx = wheel_start_x + (wi as f32) * (wheel_w + wheel_gap_x);
            let wrect = egui::Rect::from_min_size(
                egui::pos2(wx, wheel_row_y),
                egui::vec2(wheel_w, wheel_h),
            );
            let steps = draw_rubbing_fine_wheel(
                ui,
                &paper_painter,
                wrect,
                wid,
                wtitle,
                &wval,
                wphase,
            );
            if steps != 0 {
                state.rubbing_params.apply_wheel_step(wkind, steps);
            }
        }

        // =========================================================================================
        // 3. 도판 경로 렌더링 (Image 11: 단면 검게 채우지 않고 선으로 표시 + Image 15: 모든 선 mm 굵기·색상 직결)
        // =========================================================================================
        let sec_col = egui::Color32::from_rgb(
            state.line_style.section_color_rgb[0],
            state.line_style.section_color_rgb[1],
            state.line_style.section_color_rgb[2],
        );
        let out_col = egui::Color32::from_rgb(
            state.line_style.outer_color_rgb[0],
            state.line_style.outer_color_rgb[1],
            state.line_style.outer_color_rgb[2],
        );
        let aux_col = egui::Color32::from_rgb(
            state.line_style.aux_color_rgb[0],
            state.line_style.aux_color_rgb[1],
            state.line_style.aux_color_rgb[2],
        );
        let dim_col = egui::Color32::from_rgb(
            state.line_style.dim_color_rgb[0],
            state.line_style.dim_color_rgb[1],
            state.line_style.dim_color_rgb[2],
        );

        let mut main_vessel_min_x = f32::INFINITY;
        let mut main_vessel_max_x = f32::NEG_INFINITY;
        let mut main_vessel_top_y = f32::INFINITY;
        let mut main_vessel_bot_y = f32::NEG_INFINITY;
        let mut section_badge_anchor: Option<egui::Pos2> = None;

        // 6개 개별 도면 레이어(0: 1:1 평면도, 1: 1:1 단면도, 2: 1:1 정밀 탁본, 3: 1:1 수평 횡단면, 4: 추가 단면 A-A', 5: 치수선·축척자)의 중심점 산출
        let elev_reg_mm = plate
            .raster_regions
            .first()
            .map(|r| r.rect_mm)
            .unwrap_or([36.0, 24.0, 160.0, 88.0]);
        let rub_reg_mm = plate
            .raster_regions
            .get(1)
            .map(|r| r.rect_mm)
            .unwrap_or([180.0, 110.0, 300.0, 230.0]);
        let cx_left_mm = 0.5 * (elev_reg_mm[0] + elev_reg_mm[2]);
        let cy_elev_mm = 0.5 * (elev_reg_mm[1] + elev_reg_mm[3]);
        let top_elev_mm = elev_reg_mm[1].max(elev_reg_mm[3]);
        let half_d_mm = 0.5 * (elev_reg_mm[2] - elev_reg_mm[0]).abs().max(20.0);
        let cy_plan_mm = top_elev_mm + half_d_mm + 34.0;
        let cx_right_mm = 0.5 * (rub_reg_mm[0] + rub_reg_mm[2]);
        let cy_rub_mm = 0.5 * (rub_reg_mm[1] + rub_reg_mm[3]);
        let cy_hsec_mm = cy_elev_mm;
        let cx_custom_mm = cx_right_mm + half_d_mm * 1.85 + 42.0;
        let cy_custom_mm = cy_rub_mm;
        let layer_centers_mm: [[f64; 2]; 6] = [
            [cx_left_mm, cy_plan_mm],
            [cx_left_mm, cy_elev_mm],
            [cx_right_mm, cy_rub_mm],
            [cx_right_mm, cy_hsec_mm],
            [cx_custom_mm, cy_custom_mm],
            [cx_left_mm, cy_elev_mm],
        ];

        for elem in plate.paths.iter().filter(|_| state.unrolled_size_mm.is_none()) {
            if elem.points_mm.len() < 2 {
                continue;
            }
            if state.rubbing_roi_mm.is_some()
                && (elem.layer_id.contains("외저면")
                    || (elem.layer_id.contains("보조_중심선")
                        && elem.points_mm.iter().any(|p| p[0] > 35.0 && p[1] < 95.0)))
            {
                continue;
            }
            let view_idx = classify_plate_path_layer_idx(&elem.layer_id);
            let vtf = state.view_transforms[view_idx];
            if !vtf.visible {
                continue;
            }
            let view_center_mm = layer_centers_mm[view_idx];
            let is_scale_bar = elem.layer_id == "치수_축척막대" || elem.layer_id == "축척자_5cm";
            let transformed_points_mm: Vec<[f64; 2]> = if !is_scale_bar
                && (vtf.rot_90_steps != 0
                    || vtf.flip_forward_steps != 0
                    || vtf.offset_mm_x_tenths != 0
                    || vtf.offset_mm_y_tenths != 0)
            {
                elem.points_mm
                    .iter()
                    .map(|&p| vtf.transform_point_around(p, view_center_mm))
                    .collect()
            } else {
                elem.points_mm.clone()
            };
            let is_section_layer = elem.layer_id.contains("단면_실선");
            let is_outer_layer = elem.layer_id.contains("외곽선")
                || elem.layer_id.contains("구경")
                || elem.layer_id.contains("저경")
                || elem.layer_id.contains("추정복원");
            let is_horiz_or_aux = elem.layer_id.contains("횡단면")
                || elem.layer_id.contains("중심")
                || elem.layer_id.contains("십자선");

            let (stroke_col, width_mm) = if is_section_layer {
                (sec_col, snap_to_standard_weight_mm(state.line_style.section_width_mm))
            } else if is_outer_layer {
                (out_col, snap_to_standard_weight_mm(state.line_style.outer_width_mm))
            } else if is_horiz_or_aux {
                (aux_col, snap_to_standard_weight_mm(state.line_style.aux_width_mm))
            } else {
                let c = match elem.stroke_hex.as_str() {
                    "#B5573A" => aux_col,
                    "#FF0000" => profile_red,
                    "#2A78D6" => select_blue,
                    "#5E5D59" => muted,
                    _ => out_col,
                };
                (c, elem.stroke_width_mm as f32)
            };

            // Image 11: "검게 나오게 하지말고 선으로 나와야한다"
            // state.line_style.fill_section_solid == true 일 때만 내부 채움을 수행하고, 기본값(false)에서는 깨끗한 외곽선(선)으로만 제도한다!
            if state.line_style.fill_section_solid {
                if let Some(ref fill_hex) = elem.fill_hex {
                    if transformed_points_mm.len() >= 3 {
                        let tris = triangulate_polygon_ear_clipping(&transformed_points_mm);
                        if !tris.is_empty() {
                            let fill_col = if fill_hex == "#B5573A" { terra } else { sec_col };
                            let mut mesh_shape = egui::epaint::Mesh::default();
                            for p in &transformed_points_mm {
                                let pos = egui::pos2(
                                    origin_x + (p[0] as f32) * scale,
                                    origin_y - (p[1] as f32) * scale,
                                );
                                mesh_shape.vertices.push(egui::epaint::Vertex {
                                    pos,
                                    uv: egui::epaint::WHITE_UV,
                                    color: fill_col,
                                });
                            }
                            for tri in tris {
                                mesh_shape.indices.push(tri[0] as u32);
                                mesh_shape.indices.push(tri[1] as u32);
                                mesh_shape.indices.push(tri[2] as u32);
                            }
                            paper_painter.add(egui::Shape::mesh(mesh_shape));
                        }
                    }
                }
            }

            // 교차 흑백 블록 축척자 (`치수_축척막대`)
            if (elem.layer_id == "치수_축척막대" || elem.layer_id == "축척자_5cm")
                && transformed_points_mm.len() >= 2
            {
                let p_start = transformed_points_mm[0];
                let p_end = transformed_points_mm[transformed_points_mm.len() - 1];
                let raw_x0 = origin_x + (p_start[0] as f32) * scale;
                let raw_x1 = origin_x + (p_end[0] as f32) * scale;
                // 축척 막대는 선택된 축척의 표준 길이를 쓴다(실무 규격: 5cm/10cm/20cm/40cm 막대).
                let (std_bar_mm, _) = state.active_scale_preset.standard_scale_bar_mm();
                let center_x = (raw_x0 + raw_x1) * 0.5;
                let half_w = (std_bar_mm as f32) * scale * 0.5;
                let x0 = center_x - half_w;
                let x1 = center_x + half_w;
                let y_bar = origin_y - (p_start[1] as f32) * scale;
                let bar_w = (x1 - x0).abs().max(25.0);
                let bar_h = (1.6 * scale).clamp(4.0, 7.5);
                let seg_w = bar_w / 5.0;

                for i in 0..5 {
                    let sx0 = x0.min(x1) + (i as f32) * seg_w;
                    let seg_rect = egui::Rect::from_min_size(
                        egui::pos2(sx0, y_bar - bar_h),
                        egui::vec2(seg_w, bar_h),
                    );
                    let seg_fill = if i % 2 == 0 { dim_col } else { paper };
                    paper_painter.rect_filled(seg_rect, 0.0, seg_fill);
                    paper_painter.rect_stroke(
                        seg_rect,
                        0.0,
                        egui::Stroke::new(1.0_f32, dim_col),
                        egui::StrokeKind::Inside,
                    );
                }
                continue;
            }

            let mut pts: Vec<egui::Pos2> = transformed_points_mm
                .iter()
                .map(|p| {
                    egui::pos2(
                        origin_x + (p[0] as f32) * scale,
                        origin_y - (p[1] as f32) * scale,
                    )
                })
                .collect();

            let is_main_vertical_vessel =
                elem.layer_id == "단면_실선" || elem.layer_id == "외곽선_실선";
            if is_main_vertical_vessel {
                for &pt in &pts {
                    main_vessel_min_x = main_vessel_min_x.min(pt.x);
                    main_vessel_max_x = main_vessel_max_x.max(pt.x);
                    main_vessel_top_y = main_vessel_top_y.min(pt.y);
                    main_vessel_bot_y = main_vessel_bot_y.max(pt.y);
                }
                if is_section_layer && section_badge_anchor.is_none() {
                    if let Some(&mid_pt) = pts.get(pts.len() / 3) {
                        section_badge_anchor = Some(mid_pt);
                    }
                }
            }

            if elem.closed && pts.len() >= 3 {
                if let Some(&first_pt) = pts.first() {
                    pts.push(first_pt);
                }
            }

            let w_px = (width_mm * scale).clamp(0.8, 6.0);
            if let Some((dash_mm, gap_mm)) = elem.dash_pattern_mm {
                let dash_px = ((dash_mm as f32) * scale).max(3.5);
                let gap_px = ((gap_mm as f32) * scale).max(2.5);
                draw_dashed_polyline(&paper_painter, &pts, w_px, stroke_col, dash_px, gap_px);
            } else {
                paper_painter.add(egui::Shape::line(pts, egui::Stroke::new(w_px, stroke_col)));
            }
        }

        // 텍스트 주석 및 치수 콜아웃 (각 레이어의 가시성 및 오프셋 연동)
        for c in plate.callouts.iter().filter(|_| state.unrolled_size_mm.is_none()) {
            if c.text.contains("평균두께")
                || c.text.contains("최대동경")
                || (state.rubbing_roi_mm.is_some() && c.text.contains("외저면"))
            {
                continue;
            }
            let callout_layer = if c.text.contains("추가 단면") {
                4usize
            } else if c.text.contains("평면도") {
                0usize
            } else if c.text.contains("탁본") {
                2usize
            } else if c.text.contains("수평 횡단면") {
                3usize
            } else {
                5usize
            };
            if !state.view_transforms[callout_layer].visible {
                continue;
            }
            let c_off = state.view_transforms[callout_layer].offset_mm();
            let pos = egui::pos2(
                (origin_x + ((c.pos_mm[0] + c_off[0]) as f32) * scale)
                    .clamp(sheet_rect.left() + 34.0, sheet_rect.right() - 150.0),
                (origin_y - ((c.pos_mm[1] + c_off[1]) as f32) * scale)
                    .clamp(sheet_rect.top() + 50.0, sheet_rect.bottom() - 18.0),
            );
            let col = match c.color_hex.as_str() {
                "#B5573A" => aux_col,
                "#5E5D59" => muted,
                _ => dim_col,
            };
            // 축척 라벨은 현재 선택된 축척의 실무 표기(「10cm (1/2)」)로 다시 쓴다.
            let shown_text = if c.layer_id == "치수_축척라벨" {
                state.active_scale_preset.standard_scale_bar_mm().1.to_string()
            } else {
                c.text.clone()
            };
            paper_painter.text(
                pos,
                egui::Align2::LEFT_TOP,
                &shown_text,
                egui::FontId::proportional(10.5),
                col,
            );
        }

        // =========================================================================================
        // 4. Image 15: "이것도 도면안에 필요한 부분에 최대한 가깝게 와야한다. 유물의 크기 이동에따라 이동할수있게"
        // 유물 실제 외곽 경계(main_vessel_min_x .. max_x, top_y .. bot_y)에 최대한 밀착되어 함께 이동하는 치수선!
        // =========================================================================================
        if state.view_transforms[5].visible
            && state.view_transforms[1].visible
            && state.unrolled_size_mm.is_none()
        {
            if let Some(first_reg) = plate.raster_regions.first() {
                let fallback_rx0 =
                    origin_x + (first_reg.rect_mm[0].min(first_reg.rect_mm[2]) as f32) * scale;
                let fallback_rx1 =
                    origin_x + (first_reg.rect_mm[0].max(first_reg.rect_mm[2]) as f32) * scale;
                let fallback_top =
                    origin_y - (first_reg.rect_mm[1].max(first_reg.rect_mm[3]) as f32) * scale;
                let fallback_bot =
                    origin_y - (first_reg.rect_mm[1].min(first_reg.rect_mm[3]) as f32) * scale;

                let rx0 = if main_vessel_min_x.is_finite() {
                    main_vessel_min_x
                } else {
                    fallback_rx0
                };
                let rx1 = if main_vessel_max_x.is_finite() {
                    main_vessel_max_x
                } else {
                    fallback_rx1
                };
                let ry_top = if main_vessel_top_y.is_finite() {
                    main_vessel_top_y
                } else {
                    fallback_top
                };
                let ry_bot = if main_vessel_bot_y.is_finite() {
                    main_vessel_bot_y
                } else {
                    fallback_bot
                };

                let dim_w_px = (snap_to_standard_weight_mm(state.line_style.dim_width_mm) * scale)
                    .clamp(0.9, 3.0);

                // (a) 상단 수평 구경 화살표 치수선 — 구연부 **실측 구경**에 맞춰 표시한다.
                let span_px = (rx1 - rx0).abs();
                let span_mm = if scale > 0.0 { span_px / scale } else { 0.0 };
                let rim_mm = if report.rim_or_outer_diameter_mm > 1.0
                    && span_mm > 1.0
                    && report.rim_or_outer_diameter_mm <= (span_mm as f64) * 1.02
                {
                    report.rim_or_outer_diameter_mm
                } else {
                    span_mm as f64
                };
                let cx_v = (rx0 + rx1) * 0.5;
                let half_rim_px = if span_mm > 0.0 {
                    span_px * 0.5 * (rim_mm / span_mm as f64) as f32
                } else {
                    span_px * 0.5
                };
                let dim_x0 = cx_v - half_rim_px;
                let dim_x1 = cx_v + half_rim_px;

                let dim_y = (ry_top - 9.0).max(sheet_rect.top() + 48.0);
                paper_painter.line_segment(
                    [egui::pos2(dim_x0, ry_top - 1.5), egui::pos2(dim_x0, dim_y - 3.0)],
                    egui::Stroke::new(dim_w_px * 0.85, dim_col),
                );
                paper_painter.line_segment(
                    [egui::pos2(dim_x1, ry_top - 1.5), egui::pos2(dim_x1, dim_y - 3.0)],
                    egui::Stroke::new(dim_w_px * 0.85, dim_col),
                );
                paper_painter.line_segment(
                    [egui::pos2(dim_x0, dim_y), egui::pos2(dim_x1, dim_y)],
                    egui::Stroke::new(dim_w_px, dim_col),
                );
                for &(ax, dir_s) in &[(dim_x0, 1.0_f32), (dim_x1, -1.0_f32)] {
                    paper_painter.add(egui::Shape::convex_polygon(
                        vec![
                            egui::pos2(ax, dim_y),
                            egui::pos2(ax + dir_s * 5.0, dim_y - 2.4),
                            egui::pos2(ax + dir_s * 5.0, dim_y + 2.4),
                        ],
                        dim_col,
                        egui::Stroke::NONE,
                    ));
                }
                paper_painter.text(
                    egui::pos2((dim_x0 + dim_x1) * 0.5, dim_y - 6.5),
                    egui::Align2::CENTER_CENTER,
                    format!("구경 {:.1}mm", rim_mm),
                    egui::FontId::proportional(9.5),
                    dim_col,
                );

                // (b) 좌측 수직 높이 화살표 치수선 — 유물 좌측 외곽선 바로 옆 10px 위치에 밀착!
                let dim_x = (rx0 - 10.0).max(sheet_rect.left() + 36.0);
                paper_painter.line_segment(
                    [egui::pos2(dim_x - 3.0, ry_top), egui::pos2(rx0 - 1.5, ry_top)],
                    egui::Stroke::new(dim_w_px * 0.85, dim_col),
                );
                paper_painter.line_segment(
                    [egui::pos2(dim_x - 3.0, ry_bot), egui::pos2(rx0 - 1.5, ry_bot)],
                    egui::Stroke::new(dim_w_px * 0.85, dim_col),
                );
                paper_painter.line_segment(
                    [egui::pos2(dim_x, ry_top), egui::pos2(dim_x, ry_bot)],
                    egui::Stroke::new(dim_w_px, dim_col),
                );
                for &(ay, dir_s) in &[(ry_top, 1.0_f32), (ry_bot, -1.0_f32)] {
                    paper_painter.add(egui::Shape::convex_polygon(
                        vec![
                            egui::pos2(dim_x, ay),
                            egui::pos2(dim_x - 2.4, ay + dir_s * 5.0),
                            egui::pos2(dim_x + 2.4, ay + dir_s * 5.0),
                        ],
                        dim_col,
                        egui::Stroke::NONE,
                    ));
                }
                let h_mm = (first_reg.rect_mm[3] - first_reg.rect_mm[1]).abs();
                let h_lbl_rect = egui::Rect::from_center_size(
                    egui::pos2(dim_x - 28.0, (ry_top + ry_bot) * 0.5),
                    egui::vec2(52.0, 15.0),
                );
                paper_painter.rect_filled(
                    h_lbl_rect,
                    2.0,
                    egui::Color32::from_rgba_unmultiplied(250, 249, 245, 220),
                );
                paper_painter.text(
                    egui::pos2(dim_x - 4.0, (ry_top + ry_bot) * 0.5),
                    egui::Align2::RIGHT_CENTER,
                    format!("높이 {:.1}mm", h_mm),
                    egui::FontId::proportional(9.5),
                    dim_col,
                );
            }
        }

        // =========================================================================================
        // 4-B. Image 7 & 레이어 관리 ("레이어 모든것에는 방향조정 90도씩 delete키로 지우기 추가단면선 생성시 이것도 레이어에생성 삭제가능하고 키고끌수있게")
        // 5개 도면 뷰(0: 1:1 평면도, 1: 1:1 단면도, 2: 1:1 정밀 탁본, 3: 1:1 수평 횡단면, 4: 추가 단면 A-A') 상단에
        // 각각 독립된 [✥이동] [↻90°] [↕180°] [↻앞으로] [🗑Del] 컨트롤 바를 배치한다!
        // =========================================================================================
        let plan_top_mm = cy_plan_mm + half_d_mm + 14.0;
        let elev_top_ctrl_mm = top_elev_mm + 16.0;
        let rub_top_mm = rub_reg_mm[1].max(rub_reg_mm[3]) + 14.0;
        let hsec_bot_ctrl_mm = (cy_hsec_mm - half_d_mm * 0.86 - 4.0).max(8.0);
        let custom_top_mm = cy_custom_mm + half_d_mm * 0.65 + 14.0;
        let individual_anchors = [
            (0usize, "sheet_view_tf_plan", [cx_left_mm, plan_top_mm], true),
            (1usize, "sheet_view_tf_elev", [cx_left_mm, elev_top_ctrl_mm], true),
            (2usize, "sheet_view_tf_rubbing", [cx_right_mm, rub_top_mm], true),
            (3usize, "sheet_view_tf_hsec", [cx_right_mm, hsec_bot_ctrl_mm], true),
            (
                4usize,
                "sheet_view_tf_custom_aa",
                [cx_custom_mm, custom_top_mm],
                state.has_custom_section_layer,
            ),
        ];
        let mut pending_del_layer = None;
        // 1) 먼저 각 뷰의 앵커를 계산하고, 포인터가 올라가 있는 뷰를 찾는다.
        let half_d_px = (half_d_mm as f32) * scale;
        let mut anchored: Vec<(usize, &str, egui::Pos2)> = Vec::new();
        for (v_idx, v_prefix, base_anchor_mm, exists) in individual_anchors {
            if !exists || !state.view_transforms[v_idx].visible {
                continue;
            }
            let off = state.view_transforms[v_idx].offset_mm();
            let raw_y = (origin_y - ((base_anchor_mm[1] + off[1]) as f32) * scale)
                .clamp(sheet_rect.top() + 76.0, sheet_rect.bottom() - 46.0);
            let max_x_for_bar = if raw_y < sheet_rect.top() + 195.0 {
                sheet_rect.right() - 320.0
            } else {
                sheet_rect.right() - 100.0
            };
            let anchor_px = egui::pos2(
                (origin_x + ((base_anchor_mm[0] + off[0]) as f32) * scale)
                    .clamp(sheet_rect.left() + 100.0, max_x_for_bar.max(sheet_rect.left() + 100.0)),
                raw_y,
            );
            anchored.push((v_idx, v_prefix, anchor_px));
        }
        let visible_layers: Vec<usize> = anchored.iter().map(|(i, _, _)| *i).collect();
        let hovered_layer = anchored.iter().find_map(|(idx, _, anchor)| {
            let zone = egui::Rect::from_center_size(
                egui::pos2(anchor.x, anchor.y + half_d_px * 0.5),
                egui::vec2(half_d_px * 2.0, half_d_px * 2.0),
            );
            ui.rect_contains_pointer(zone).then_some(*idx)
        });
        let active_bar =
            active_action_bar_layer(state.selected_layer_idx, hovered_layer, &visible_layers);

        // 2) 액션바는 활성 레이어 하나에만 그린다.
        for (v_idx, v_prefix, anchor_px) in anchored {
            if active_bar != Some(v_idx) {
                continue;
            }
            let is_sel = state.selected_layer_idx == Some(v_idx);
            let (sel_clicked, del_clicked) = draw_individual_view_flip_controls(
                ui,
                &paper_painter,
                anchor_px,
                v_prefix,
                v_idx,
                is_sel,
                scale,
                &mut state.view_transforms[v_idx],
            );
            if sel_clicked {
                state.selected_layer_idx = Some(v_idx);
            }
            if del_clicked {
                pending_del_layer = Some(v_idx);
            }
        }
        if let Some(d_idx) = pending_del_layer {
            state.delete_layer(d_idx);
        }

        // =========================================================================================
        // 4-C. 우측 상단 『🗂 레이어 관리 (켜기/끄기 · 90° 회전 · Delete 지우기 · 추가단면 A-A' 연동)』 패널
        // =========================================================================================
        if state.show_layer_panel {
            let active_layer_indices: Vec<usize> = if state.has_custom_section_layer {
                vec![0, 1, 2, 3, 4, 5]
            } else {
                vec![0, 1, 2, 3, 5]
            };
            let row_h = 20.0_f32;
            let panel_w = 238.0_f32;
            let panel_h = 26.0 + (active_layer_indices.len() as f32) * row_h + 6.0;
            let layer_panel_rect = egui::Rect::from_min_size(
                egui::pos2(
                    (sheet_rect.right() - panel_w - 8.0).max(sheet_rect.left() + 12.0),
                    sheet_rect.top() + 34.0,
                ),
                egui::vec2(panel_w, panel_h),
            );
            paper_painter.rect_filled(
                layer_panel_rect,
                4.5,
                egui::Color32::from_rgba_unmultiplied(250, 249, 245, 246),
            );
            paper_painter.rect_stroke(
                layer_panel_rect,
                4.5,
                egui::Stroke::new(1.1_f32, terra),
                egui::StrokeKind::Inside,
            );
            paper_painter.text(
                egui::pos2(layer_panel_rect.left() + 8.0, layer_panel_rect.top() + 12.0),
                egui::Align2::LEFT_CENTER,
                "🗂 도면 레이어 (켜기/끄기 · 90°회전 · Del삭제)",
                egui::FontId::proportional(9.0),
                terra,
            );

            let mut row_del_idx = None;
            for (ri_row, l_idx) in active_layer_indices.into_iter().enumerate() {
                let ry = layer_panel_rect.top() + 24.0 + (ri_row as f32) * row_h;
                let r_rect = egui::Rect::from_min_size(
                    egui::pos2(layer_panel_rect.left() + 4.0, ry),
                    egui::vec2(panel_w - 8.0, row_h - 2.0),
                );
                let is_sel = state.selected_layer_idx == Some(l_idx);
                if is_sel {
                    paper_painter.rect_filled(
                        r_rect,
                        2.5,
                        egui::Color32::from_rgba_unmultiplied(226, 238, 252, 230),
                    );
                }
                let mut l_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(r_rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                l_ui.spacing_mut().item_spacing.x = 3.0;
                let vis = &mut state.view_transforms[l_idx].visible;
                let _ = l_ui.checkbox(vis, "");
                let lbl_resp = l_ui.selectable_label(
                    is_sel,
                    egui::RichText::new(sheet_layer_title_kr(l_idx))
                        .size(8.8)
                        .color(if *vis { ink } else { muted }),
                );
                if lbl_resp.clicked() {
                    state.selected_layer_idx = Some(l_idx);
                }
                if l_ui
                    .add(egui::Button::new(egui::RichText::new("↻90°").size(8.2)).corner_radius(2.5))
                    .clicked()
                {
                    state.selected_layer_idx = Some(l_idx);
                    state.view_transforms[l_idx].rotate_cw_90();
                }
                if l_ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("🗑").size(8.2).color(profile_red),
                        )
                        .corner_radius(2.5),
                    )
                    .on_hover_text("이 레이어 지우기 (Delete 키와 동일)")
                    .clicked()
                {
                    state.selected_layer_idx = Some(l_idx);
                    row_del_idx = Some(l_idx);
                }
            }
            if let Some(d_idx) = row_del_idx {
                state.delete_layer(d_idx);
            }
        }

        // =========================================================================================
        // 5. Image 2 & Image 4 ("아이콘형태 ui로 잘보이게하라"):
        // 단면선·외곽선·횡단면·치수선 4개 조정 위젯에 고시인성 벡터 아이콘 배지를 추가하고 유물 좌표와 함께 움직이며 절대 겹치지 않게 배치!
        // =========================================================================================
        let vessel_rect_mm = plate
            .raster_regions
            .first()
            .map(|r| r.rect_mm)
            .unwrap_or([40.0, 20.0, 140.0, 90.0]);
        let wash = egui::Color32::from_rgb(WASH.rgb[0], WASH.rgb[1], WASH.rgb[2]);
        let line_widget_rects = compute_non_overlapping_line_widget_rects(
            egui::pos2(origin_x, origin_y),
            scale,
            vessel_rect_mm,
            [0.0, 0.0, (vessel_rect_mm[1] + vessel_rect_mm[3]) * 0.5],
            sheet_rect,
        );
        let _ = section_badge_anchor;

        for (idx, w_rect) in line_widget_rects.into_iter().enumerate() {
            let stroke_c = match idx {
                0 => sec_col,
                1 => out_col,
                2 => aux_col,
                _ => dim_col,
            };
            paper_painter.rect_filled(
                w_rect,
                4.5,
                egui::Color32::from_rgba_unmultiplied(250, 249, 245, 246),
            );
            paper_painter.rect_stroke(
                w_rect,
                4.5,
                egui::Stroke::new(1.2_f32, stroke_c),
                egui::StrokeKind::Inside,
            );
            let icon_rect = egui::Rect::from_min_size(
                egui::pos2(w_rect.left() + 3.5, w_rect.top() + 3.0),
                egui::vec2(19.0, w_rect.height() - 6.0),
            );
            draw_line_style_vector_icon(&paper_painter, icon_rect, idx, stroke_c);

            let content_rect = egui::Rect::from_min_max(
                egui::pos2(icon_rect.right() + 4.0, w_rect.top() + 2.0),
                egui::pos2(w_rect.right() - 3.0, w_rect.bottom() - 2.0),
            );
            let mut child_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(content_rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            child_ui.spacing_mut().item_spacing.x = 3.0;
            match idx {
                0 => {
                    child_ui.label(egui::RichText::new("단면선").size(9.5).strong().color(ink));
                    let _ = child_ui.add(
                        egui::DragValue::new(&mut state.line_style.section_width_mm)
                            .speed(0.02)
                            .range(0.10..=1.50)
                            .suffix("mm")
                            .fixed_decimals(2),
                    );
                    let _ = child_ui.color_edit_button_srgb(&mut state.line_style.section_color_rgb);
                }
                1 => {
                    child_ui.label(egui::RichText::new("외곽선").size(9.5).strong().color(ink));
                    let _ = child_ui.add(
                        egui::DragValue::new(&mut state.line_style.outer_width_mm)
                            .speed(0.02)
                            .range(0.10..=1.50)
                            .suffix("mm")
                            .fixed_decimals(2),
                    );
                    let _ = child_ui.color_edit_button_srgb(&mut state.line_style.outer_color_rgb);
                }
                2 => {
                    child_ui.label(egui::RichText::new("횡단면").size(9.5).strong().color(ink));
                    let _ = child_ui.add(
                        egui::DragValue::new(&mut state.line_style.aux_width_mm)
                            .speed(0.02)
                            .range(0.10..=1.50)
                            .suffix("mm")
                            .fixed_decimals(2),
                    );
                    let _ = child_ui.color_edit_button_srgb(&mut state.line_style.aux_color_rgb);
                }
                _ => {
                    child_ui.label(egui::RichText::new("치수선").size(9.5).strong().color(ink));
                    let _ = child_ui.add(
                        egui::DragValue::new(&mut state.line_style.dim_width_mm)
                            .speed(0.02)
                            .range(0.10..=1.50)
                            .suffix("mm")
                            .fixed_decimals(2),
                    );
                    let _ = child_ui.color_edit_button_srgb(&mut state.line_style.dim_color_rgb);
                }
            }
        }

        // =========================================================================================
        // 5-B. Image 3 & Image 4: 조판 용지 설정 바 (아이콘 형태 UI로 시인성 극대화!)
        // =========================================================================================
        paper_painter.rect_filled(
            top_style_bar_rect,
            4.5,
            egui::Color32::from_rgba_unmultiplied(250, 249, 245, 246),
        );
        paper_painter.rect_stroke(
            top_style_bar_rect,
            4.5,
            egui::Stroke::new(1.1_f32, terra),
            egui::StrokeKind::Inside,
        );
        let mut bar_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(top_style_bar_rect.shrink2(egui::vec2(6.0, 2.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        bar_ui.spacing_mut().item_spacing.x = 4.0;

        bar_ui.label(egui::RichText::new("■ 조판용지:").size(9.5).strong().color(terra));
        for (preset, icon_lbl) in [
            (PaperSheetPreset::A3Landscape, "■ A3가로"),
            (PaperSheetPreset::A4Landscape, "■ A4가로"),
            (PaperSheetPreset::A4Portrait, "□ A4세로"),
            (PaperSheetPreset::B4Portrait, "□ B4세로"),
            (PaperSheetPreset::B5Portrait, "□ B5세로"),
        ] {
            let is_sel = state.paper_preset == preset;
            let btn = egui::Button::new(
                egui::RichText::new(icon_lbl)
                    .size(9.2)
                    .strong()
                    .color(if is_sel { paper } else { ink }),
            )
            .fill(if is_sel { terra } else { wash })
            .stroke(egui::Stroke::new(1.0_f32, if is_sel { terra } else { border }))
            .corner_radius(3.5);
            if bar_ui.add(btn).clicked() {
                state.paper_preset = preset;
            }
        }
        bar_ui.separator();

        // 곡면 전개 탁본 모드: 평면(기본) ↔ 원통/원뿔/구면 0~360° 전개
        let cur_unroll = state.rubbing_params.unroll_mode;
        let unroll_label = match cur_unroll {
            None => "탁본 평면",
            Some(strata_shade::UnrollMode::Cylindrical) => "원통 전개",
            Some(strata_shade::UnrollMode::Conical) => "원뿔 전개",
            Some(strata_shade::UnrollMode::Spherical) => "구면 전개",
        };
        let mut chosen_unroll = cur_unroll;
        egui::ComboBox::from_id_salt("rubbing_unroll_mode_combo")
            .selected_text(egui::RichText::new(unroll_label).size(9.2))
            .width(86.0)
            .show_ui(&mut bar_ui, |ui| {
                ui.selectable_value(&mut chosen_unroll, None, "평면 탁본");
                ui.selectable_value(
                    &mut chosen_unroll,
                    Some(strata_shade::UnrollMode::Cylindrical),
                    "원통 1:1 전개",
                );
                ui.selectable_value(
                    &mut chosen_unroll,
                    Some(strata_shade::UnrollMode::Conical),
                    "원뿔 전개",
                );
                ui.selectable_value(
                    &mut chosen_unroll,
                    Some(strata_shade::UnrollMode::Spherical),
                    "구면 전개",
                );
            });
        if chosen_unroll != cur_unroll {
            state.rubbing_params.unroll_mode = chosen_unroll;
        }
        bar_ui.separator();
        let roi_btn = egui::Button::new(
            egui::RichText::new(if state.roi_drag_mode {
                "✂ 부분탁본 1:1 드래그중"
            } else {
                "✂ 부분탁본 1:1"
            })
            .size(9.3)
            .strong()
            .color(if state.roi_drag_mode { paper } else { select_blue }),
        )
        .fill(if state.roi_drag_mode {
            select_blue
        } else {
            egui::Color32::from_rgb(235, 243, 252)
        })
        .stroke(egui::Stroke::new(1.1_f32, select_blue))
        .corner_radius(3.5);
        if bar_ui.add(roi_btn).clicked() {
            state.roi_drag_mode = !state.roi_drag_mode;
        }
        if state.rubbing_roi_mm.is_some()
            && bar_ui
                .add(egui::Button::new(egui::RichText::new("↺ 복원").size(9.0)).corner_radius(3.0))
                .clicked()
        {
            state.set_rubbing_roi_mm(None);
            state.roi_drag_mode = false;
        }
        let fill_btn = egui::Button::new(
            egui::RichText::new(if state.line_style.fill_section_solid {
                "■ 단면채움 ON"
            } else {
                "□ 단면채움"
            })
            .size(9.2)
            .strong()
            .color(if state.line_style.fill_section_solid {
                paper
            } else {
                ink
            }),
        )
        .fill(if state.line_style.fill_section_solid {
            ink
        } else {
            wash
        })
        .stroke(egui::Stroke::new(1.0_f32, border))
        .corner_radius(3.5);
        if bar_ui.add(fill_btn).clicked() {
            state.line_style.fill_section_solid = !state.line_style.fill_section_solid;
        }

        // =========================================================================================
        // 5-B2. "프로그램 우측창 상단오른쪽제일끝에 gpu사용여부와 프레임숫자를 작게출력하라"
        // =========================================================================================
        let dt_sec = ui
            .ctx()
            .input(|i| i.stable_dt)
            .clamp(1.0 / 240.0, 0.5);
        let live_fps = (1.0 / dt_sec).round().clamp(30.0, 240.0) as u32;
        let gpu_txt = if let Some(gpu_info) = crate::gpu_backend::primary_hardware_gpu() {
            let gpu_short = gpu_info
                .name
                .replace("NVIDIA GeForce ", "NVIDIA ")
                .replace("AMD Radeon ", "AMD ");
            format!("⚡ GPU: ON ({}) · {} FPS", gpu_short, live_fps)
        } else {
            format!("⚡ GPU: OFF · {} FPS", live_fps)
        };
        let gpu_badge_w = 248.0_f32;
        let gpu_badge_rect = egui::Rect::from_min_size(
            egui::pos2(
                (sheet_rect.right() - gpu_badge_w - 8.0).max(top_style_bar_rect.right() + 6.0),
                sheet_rect.top() + 6.0,
            ),
            egui::vec2(gpu_badge_w, 22.0),
        );
        paper_painter.rect_filled(
            gpu_badge_rect,
            4.0,
            egui::Color32::from_rgb(232, 245, 236),
        );
        paper_painter.rect_stroke(
            gpu_badge_rect,
            4.0,
            egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(38, 128, 68)),
            egui::StrokeKind::Inside,
        );
        paper_painter.text(
            gpu_badge_rect.center(),
            egui::Align2::CENTER_CENTER,
            gpu_txt,
            egui::FontId::monospace(8.5),
            egui::Color32::from_rgb(22, 96, 46),
        );

        // =========================================================================================
        // 5-C. 우측 하단 축척 설정 바 ("우측하단에 축척을 설정할수있게하라. 단면도 평면도는 무조건 들어가야하고 1:1로 기본으로 나오게하라")
        // =========================================================================================
        paper_painter.rect_filled(
            bot_scale_bar_rect,
            4.0,
            egui::Color32::from_rgba_unmultiplied(250, 249, 245, 244),
        );
        paper_painter.rect_stroke(
            bot_scale_bar_rect,
            4.0,
            egui::Stroke::new(1.0_f32, terra),
            egui::StrokeKind::Inside,
        );
        let mut scale_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bot_scale_bar_rect.shrink2(egui::vec2(6.0, 2.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        scale_ui.spacing_mut().item_spacing.x = 4.0;
        scale_ui.label(egui::RichText::new("📐 도면 축척:").size(9.5).strong().color(terra));
        for (preset, lbl) in [
            (ScalePreset::Full1To1, "1/1"),
            (ScalePreset::Half1To2, "1/2"),
            (ScalePreset::Third1To3, "1/3"),
            (ScalePreset::Quarter1To4, "1/4"),
            (ScalePreset::Fifth1To5, "1/5"),
            (ScalePreset::Sixth1To6, "1/6"),
            (ScalePreset::Tenth1To10, "1/10"),
            (ScalePreset::Micro2To1, "2/1"),
        ] {
            let is_sel = state.active_scale_preset == preset;
            let btn = egui::Button::new(
                egui::RichText::new(lbl)
                    .size(9.2)
                    .color(if is_sel { paper } else { ink }),
            )
            .fill(if is_sel { terra } else { wash })
            .stroke(egui::Stroke::new(1.0_f32, if is_sel { terra } else { border }))
            .corner_radius(3.0);
            if scale_ui.add(btn).clicked() {
                state.active_scale_preset = preset;
                state.scale_locked = true;
                state.zoom = match preset {
                    ScalePreset::Full1To1 => 1.0,
                    ScalePreset::Half1To2 => 0.5,
                    ScalePreset::Third1To3 => 1.0 / 3.0,
                    ScalePreset::Quarter1To4 => 0.25,
                    ScalePreset::Micro2To1 => 2.0,
                    _ => 1.0,
                };
            }
        }

        // 6. 『부분 정밀 탁본 창 (ROI Rubbing Studio · 학술 MSII+DoG 엔진)』
        if state.show_roi_rubbing_window {
            let mut open = state.show_roi_rubbing_window;
            egui::Window::new("부분 정밀 탁본 창 (ROI Rubbing Studio · 학술 MSII+DoG 엔진)")
                .open(&mut open)
                .default_pos(egui::pos2(sheet_rect.right() - 365.0, sheet_rect.top() + 52.0))
                .default_width(340.0)
                .resizable(false)
                .collapsible(false)
                .show(ui.ctx(), |ui| {
                    ui.colored_label(
                        muted,
                        "도판 위에서 [⬚ 부분탁본 선택] 후 마우스로 드래그하면 드래그한 1:1 크기로 정밀 탁본이 추출됩니다.",
                    );
                    ui.add_space(4.0);

                    let (prev_rect, _) =
                        ui.allocate_exact_size(egui::vec2(320.0, 128.0), egui::Sense::hover());
                    let pp = ui.painter_at(prev_rect);
                    pp.rect_filled(prev_rect, 4.0, paper);
                    pp.rect_stroke(
                        prev_rect,
                        4.0,
                        egui::Stroke::new(1.0_f32, border),
                        egui::StrokeKind::Inside,
                    );
                    if let Some(ref tex) = state.cached_front_tex {
                        let uv_rect = if let (Some(roi), Some(first_reg)) =
                            (state.rubbing_roi_mm, plate.raster_regions.first())
                        {
                            let rx0 = first_reg.rect_mm[0].min(first_reg.rect_mm[2]);
                            let rx1 = first_reg.rect_mm[0].max(first_reg.rect_mm[2]);
                            let ry0 = first_reg.rect_mm[1].min(first_reg.rect_mm[3]);
                            let ry1 = first_reg.rect_mm[1].max(first_reg.rect_mm[3]);
                            let rw = (rx1 - rx0).max(1.0);
                            let rh = (ry1 - ry0).max(1.0);
                            let u0 = (((roi[0] - rx0) / rw) as f32).clamp(0.0, 0.48);
                            let u1 = (((roi[2] - rx0) / rw) as f32).clamp(u0 + 0.12, 0.50);
                            let v0 = (((ry1 - roi[3]) / rh) as f32).clamp(0.0, 0.85);
                            let v1 = (((ry1 - roi[1]) / rh) as f32).clamp(v0 + 0.12, 1.0);
                            egui::Rect::from_min_max(egui::pos2(u0, v0), egui::pos2(u1, v1))
                        } else {
                            egui::Rect::from_min_max(egui::pos2(0.02, 0.05), egui::pos2(0.49, 0.95))
                        };
                        pp.image(
                            tex.id(),
                            prev_rect.shrink(6.0),
                            uv_rect,
                            egui::Color32::WHITE,
                        );
                    }

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        for st in [
                            AcademicRubbingStyle::WetHanjiInk,
                            AcademicRubbingStyle::DryGraphite,
                            AcademicRubbingStyle::MsiiRidgeEnhance,
                        ] {
                            let is_sel = state.rubbing_params.style == st;
                            if ui.selectable_label(is_sel, st.label_kr()).clicked() {
                                state.rubbing_params.style = st;
                            }
                        }
                    });

                    egui::Grid::new("roi_rubbing_academic_grid")
                        .num_columns(2)
                        .spacing([8.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("먹물 농도:");
                            ui.add(
                                egui::Slider::new(
                                    &mut state.rubbing_params.ink_density_pct,
                                    15.0..=98.0,
                                )
                                .suffix("%")
                                .fixed_decimals(0),
                            );
                            ui.end_row();

                            ui.label("MSII 곡률 감도:");
                            ui.add(
                                egui::Slider::new(
                                    &mut state.rubbing_params.msii_detail_boost,
                                    0.4..=3.0,
                                )
                                .suffix("×")
                                .fixed_decimals(2),
                            );
                            ui.end_row();

                            ui.label("시그모이드 음양각 대비:");
                            ui.add(
                                egui::Slider::new(
                                    &mut state.rubbing_params.sigmoid_contrast,
                                    2.0..=16.0,
                                )
                                .fixed_decimals(1),
                            );
                            ui.end_row();

                            ui.label("한지 여백 임계:");
                            ui.add(
                                egui::Slider::new(
                                    &mut state.rubbing_params.contact_threshold,
                                    -0.40..=0.25,
                                )
                                .fixed_decimals(2),
                            );
                            ui.end_row();
                        });

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if let Some([x0, y0, x1, y1]) = state.rubbing_roi_mm {
                            ui.colored_label(
                                select_blue,
                                format!(
                                    "1:1 크기: {:.1}×{:.1}mm",
                                    (x1 - x0).abs(),
                                    (y1 - y0).abs()
                                ),
                            );
                        } else {
                            ui.colored_label(muted, "현재: 유물 전체 영역 탁본 표시 중");
                        }
                        if ui.button("전체 탁본 복원").clicked() {
                            state.set_rubbing_roi_mm(None);
                            state.roi_drag_mode = false;
                        }
                    });
                });
            state.show_roi_rubbing_window = open;
        }
    });

    (out_header_rect, out_strip_rect, out_sheet_rect)
}
