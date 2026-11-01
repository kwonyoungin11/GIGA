use eframe::egui;
use image::RgbaImage;
use strata_core::{ArtifactCategory, ScalePreset, TriMesh};
use strata_export::PlateRasterKind;
use strata_measure::compute_artifact_measurements;

use super::config::*;
use super::rubbing::*;
use super::state::*;
use super::widgets::*;

fn make_patterned_bowl_mesh() -> TriMesh {
    let mut positions = Vec::new();
    let mut triangles = Vec::new();
    let nu = 48usize;
    let nv = 28usize;
    for iv in 0..=nv {
        let v_frac = iv as f64 / nv as f64;
        let z = v_frac * 55.0;
        let groove = if (iv % 4) == 2 { -1.35 } else { 0.35 };
        let r = 26.0 + 28.0 * v_frac.powf(0.8) + groove;
        for iu in 0..=nu {
            let theta = (iu as f64 / nu as f64) * std::f64::consts::TAU;
            let stamp = if (iu % 6) == 0 && iv > 6 && iv < 22 {
                -1.15
            } else {
                0.0
            };
            let rr = r + stamp;
            positions.push([rr * theta.cos(), rr * theta.sin(), z]);
        }
    }
    let stride = (nu + 1) as u32;
    for iv in 0..(nv as u32) {
        for iu in 0..(nu as u32) {
            let a = iv * stride + iu;
            let b = a + 1;
            let c = a + stride;
            let d = c + 1;
            triangles.push([a, b, d]);
            triangles.push([a, d, c]);
        }
    }
    TriMesh::from_positions_and_triangles("patterned_bowl", positions, triangles).expect("ok")
}

#[test]
fn test_orthographic_rubbing_raster_produces_visible_ink_within_silhouette() {
    let mesh = make_patterned_bowl_mesh();
    for kind in [
        PlateRasterKind::FrontExteriorRubbing,
        PlateRasterKind::BackInteriorRubbing,
        PlateRasterKind::BottomPlanRubbing,
    ] {
        let img = render_orthographic_rubbing_rgba(&mesh, kind, 0.0, 75.0, 160, 160);
        let mut opaque_px = 0usize;
        let mut ink_shaded_px = 0usize;
        for p in img.pixels() {
            if p[3] > 128 {
                opaque_px += 1;
                if p[0] < 215 {
                    ink_shaded_px += 1;
                }
            }
        }
        assert!(opaque_px > 2_000);
        assert!(ink_shaded_px > 400);
    }
}

#[test]
fn test_rubbing_params_hash_tracks_every_field() {
    let base = AcademicRubbingParams::default();
    let h0 = rubbing_params_hash(&base);
    assert_eq!(
        h0,
        rubbing_params_hash(&base.clone()),
        "해시가 결정론적이어야 함"
    );

    let mut ink = base.clone();
    ink.ink_density_pct = 79.0;
    let mut light = base.clone();
    light.light_azimuth_deg = 316.0;
    let mut style = base.clone();
    style.style = AcademicRubbingStyle::DryGraphite;
    let mut unroll = base.clone();
    unroll.unroll_mode = Some(strata_shade::UnrollMode::Cylindrical);

    for (name, h) in [
        ("먹물농도", rubbing_params_hash(&ink)),
        ("조명방위", rubbing_params_hash(&light)),
        ("표현기법", rubbing_params_hash(&style)),
        ("전개모드", rubbing_params_hash(&unroll)),
    ] {
        assert_ne!(h, h0, "{} 변경이 해시에 반영되지 않음", name);
    }
}

#[test]
fn test_rubbing_provenance_line_records_standard_values() {
    let line = rubbing_provenance_line(&AcademicRubbingParams::default());
    for token in [
        "L-Std 315/45/1.00",
        "MSII 1.45",
        "Ink 78",
        "전개=평면",
        "투영=직교",
    ] {
        assert!(
            line.contains(token),
            "출처 문자열에 '{}' 없음: {}",
            token,
            line
        );
    }
    let unrolled = AcademicRubbingParams {
        unroll_mode: Some(strata_shade::UnrollMode::Cylindrical),
        ..Default::default()
    };
    let u_line = rubbing_provenance_line(&unrolled);
    assert!(u_line.contains("전개=원통"));
    assert!(u_line.contains("투영=원통전개"));
}

#[test]
fn test_rubbing_render_is_bit_identical() {
    let mesh = make_patterned_bowl_mesh();
    let params = AcademicRubbingParams::default();
    let a = render_orthographic_rubbing_rgba_with_params(
        &mesh,
        PlateRasterKind::BottomPlanRubbing,
        0.0,
        &params,
        160,
        160,
    );
    let b = render_orthographic_rubbing_rgba_with_params(
        &mesh,
        PlateRasterKind::BottomPlanRubbing,
        0.0,
        &params,
        160,
        160,
    );
    assert_eq!(
        a.as_raw(),
        b.as_raw(),
        "같은 입력인데 렌더 결과가 다름(비결정적)"
    );
}

#[test]
fn test_line_weights_snap_to_house_standard() {
    // 「유물제도선.ai」 규격: 0.12/0.15/0.2/0.3/0.4/0.5/0.8
    assert_eq!(STANDARD_LINE_WEIGHTS_MM.len(), 7);
    assert_eq!(snap_to_standard_weight_mm(0.35), 0.30);
    assert_eq!(snap_to_standard_weight_mm(0.26), 0.30);
    assert_eq!(snap_to_standard_weight_mm(0.22), 0.20);
    assert_eq!(snap_to_standard_weight_mm(0.44), 0.40);
    assert_eq!(snap_to_standard_weight_mm(0.9), 0.80);
    // 기본값 자체가 규격값이어야 한다.
    let d = PlateLineStyleConfig::default();
    for w in [
        d.section_width_mm,
        d.outer_width_mm,
        d.aux_width_mm,
        d.dim_width_mm,
    ] {
        assert!(
            STANDARD_LINE_WEIGHTS_MM.contains(&w),
            "기본 선굵기 {} 가 규격값이 아님",
            w
        );
    }
}

#[test]
fn test_fit_scale_preset_keeps_large_plate_on_paper() {
    // 실물 스캔(Φ223.8, 높이 71.2)의 1:1 도판 캔버스는 A3(420×297)를 넘는다.
    let (c_w, c_h) = (653.0, 381.0);
    assert_eq!(
        fit_scale_preset(c_w, c_h, 420.0, 297.0, 12.0),
        ScalePreset::Half1To2
    );
    // 작은 도판은 1:1 유지.
    assert_eq!(
        fit_scale_preset(240.0, 260.0, 420.0, 297.0, 12.0),
        ScalePreset::Full1To1
    );
    // 아주 큰 유물은 1:3 까지 내려간다.
    assert_eq!(
        fit_scale_preset(900.0, 700.0, 420.0, 297.0, 12.0),
        ScalePreset::Third1To3
    );
    // 어떤 축척으로도 안 들어가면 최소 축척(1:4)을 돌려준다.
    assert_eq!(
        fit_scale_preset(2000.0, 1500.0, 420.0, 297.0, 12.0),
        ScalePreset::Quarter1To4
    );
}

#[test]
fn test_parse_plate_mode_cli() {
    assert_eq!(parse_plate_mode("auto"), None);
    assert_eq!(
        parse_plate_mode("1"),
        Some(strata_export::PlateLayoutMode::ALL[0])
    );
    assert_eq!(
        parse_plate_mode("8"),
        Some(strata_export::PlateLayoutMode::ALL[7])
    );
    assert_eq!(parse_plate_mode("9"), None);
    assert_eq!(parse_plate_mode("0"), None);
    assert_eq!(parse_plate_mode("bogus"), None);
}

#[test]
fn test_parse_unroll_mode_cli() {
    assert_eq!(
        parse_unroll_mode("cylindrical"),
        Some(strata_shade::UnrollMode::Cylindrical)
    );
    assert_eq!(
        parse_unroll_mode("conical"),
        Some(strata_shade::UnrollMode::Conical)
    );
    assert_eq!(
        parse_unroll_mode("spherical"),
        Some(strata_shade::UnrollMode::Spherical)
    );
    assert_eq!(parse_unroll_mode("none"), None);
    assert_eq!(parse_unroll_mode("bogus"), None);
}

#[test]
fn test_unrolled_texture_keeps_physical_aspect_ratio() {
    // 원주 364.4mm × 높이 64.0mm 전개 띠: 텍셀 종횡비가 물리 비율과 같아야 1:1 계측이 성립한다.
    let (w, h) = unrolled_texture_dims([364.4, 64.0], 520);
    assert_eq!(w, 520);
    let expected_h = 520.0 * (64.0 / 364.4);
    assert!(
        ((h as f64) - expected_h).abs() < 2.0,
        "전개 텍스처 높이 {}px 는 물리 비율 {:.1}px 와 어긋남",
        h,
        expected_h
    );
}

#[test]
fn test_seam_spanning_triangle_is_culled_in_unroll() {
    // 원주 364mm 전개도에서 봉합선(±π)을 가로지르는 삼각형은 전체 폭 줄무늬를 만든다.
    let width = 364.0;
    assert!(is_seam_spanning_triangle([-182.0, 0.0, 182.0], width));
    // 정상 삼각형은 컬링하지 않는다.
    assert!(!is_seam_spanning_triangle([-100.0, 0.0, 100.0], width));
    assert!(!is_seam_spanning_triangle([10.0, 12.0, 15.0], width));
    // 폭이 0 이면 판정 불가 → 컬링하지 않는다.
    assert!(!is_seam_spanning_triangle([-1.0, 0.0, 1.0], 0.0));
}

#[test]
fn test_unrolled_cylindrical_rubbing_differs_from_plan_and_has_ink() {
    let mesh = make_patterned_bowl_mesh();
    let planar = AcademicRubbingParams::default();
    let unrolled = AcademicRubbingParams {
        unroll_mode: Some(strata_shade::UnrollMode::Cylindrical),
        ..Default::default()
    };

    let img_plan = render_orthographic_rubbing_rgba_with_params(
        &mesh,
        PlateRasterKind::BottomPlanRubbing,
        0.0,
        &planar,
        200,
        200,
    );
    let img_unroll = render_orthographic_rubbing_rgba_with_params(
        &mesh,
        PlateRasterKind::BottomPlanRubbing,
        0.0,
        &unrolled,
        200,
        200,
    );

    let opaque = |img: &RgbaImage| img.pixels().filter(|p| p[3] > 128).count();
    assert!(
        opaque(&img_unroll) > 1_000,
        "원통 전개 탁본이 비어 있음: {} px",
        opaque(&img_unroll)
    );
    assert_ne!(
        img_plan.as_raw(),
        img_unroll.as_raw(),
        "전개 모드를 켜도 평면 탁본과 동일함"
    );
}

#[test]
fn test_action_bar_shows_for_single_view_only() {
    let visible = [0usize, 1, 2, 3];
    // 선택된 레이어가 있으면 그것 하나만.
    assert_eq!(active_action_bar_layer(Some(2), Some(3), &visible), Some(2));
    // 선택이 없으면 호버된 하나만.
    assert_eq!(active_action_bar_layer(None, Some(3), &visible), Some(3));
    // 둘 다 없으면 아무것도 그리지 않는다(도면 가림 방지).
    assert_eq!(active_action_bar_layer(None, None, &visible), None);
    // 숨겨진 레이어가 선택돼 있으면 무시하고 호버로 되돌아간다.
    assert_eq!(active_action_bar_layer(Some(4), Some(1), &visible), Some(1));
}

#[test]
fn test_chs_plate_title_block_and_no_overlapping_aux_views() {
    let mesh = make_patterned_bowl_mesh();
    let report = compute_artifact_measurements(&mesh, ArtifactCategory::CompleteVessel);
    let (left_c, center_t, center_s, right_c) =
        build_chs_plate_title_block("7지구 9호-7 (완형 토기)", "① 완형 토기 표준 도판", &report);
    assert!(left_c.contains("유적명"));
    assert!(center_t.contains("圖版"));
    assert!(center_s.contains("Plate."));
    assert!(right_c.contains("축척"));
}

#[test]
fn test_tdd5_full_bleed_archaeological_millimeter_graph_paper_grid() {
    let sheet_rect = egui::Rect::from_min_size(egui::pos2(400.0, 34.0), egui::vec2(1040.0, 866.0));
    let spec = compute_full_bleed_graph_paper_spec(sheet_rect, 210.0, 140.0, 1.0, egui::Vec2::ZERO);
    assert_eq!(spec.paper_rect, sheet_rect);
    assert!(spec.lines_10mm_x.len() >= 12);
    assert!(spec.lines_10mm_y.len() >= 10);
    assert!(spec.lines_5mm_x.len() >= 10);
    assert!(!spec.cm_labels_top.is_empty());
    assert!(!spec.cm_labels_left.is_empty());
}

#[test]
fn test_tdd6_academic_msii_dog_sigmoid_digital_rubbing_contrast() {
    let mesh = make_patterned_bowl_mesh();
    let params = AcademicRubbingParams {
        ink_density_pct: 80.0,
        msii_detail_boost: 1.45,
        sigmoid_contrast: 8.5,
        contact_threshold: -0.12,
        style: AcademicRubbingStyle::WetHanjiInk,
        ..Default::default()
    };
    let img = render_orthographic_rubbing_rgba_with_params(
        &mesh,
        PlateRasterKind::FrontExteriorRubbing,
        0.0,
        &params,
        220,
        220,
    );
    let mut deep_ink_ridges = 0usize;
    let mut uninked_paper_grooves = 0usize;
    for p in img.pixels() {
        if p[3] > 200 {
            if p[0] <= 75 {
                deep_ink_ridges += 1;
            } else if p[0] >= 220 {
                uninked_paper_grooves += 1;
            }
        }
    }
    assert!(deep_ink_ridges > 300);
    assert!(uninked_paper_grooves > 150);
}

#[test]
fn test_tdd7_drag_roi_partial_rubbing_window_and_density_control() {
    let mesh = make_patterned_bowl_mesh();
    let mut state = SheetPaneState::default();
    assert!(state.rubbing_roi_mm.is_none());

    state.set_rubbing_roi_mm(Some([20.0, 15.0, 65.0, 50.0]));
    assert_eq!(state.rubbing_roi_mm, Some([20.0, 15.0, 65.0, 50.0]));

    let src = render_orthographic_rubbing_rgba_with_params(
        &mesh,
        PlateRasterKind::FrontExteriorRubbing,
        0.0,
        &state.rubbing_params,
        200,
        200,
    );
    let full_cimg =
        rgba_to_color_image_clipped_with_roi(&src, [10.0, 10.0, 110.0, 80.0], Some(-1), None);
    let roi_cimg = rgba_to_color_image_clipped_with_roi(
        &src,
        [10.0, 10.0, 110.0, 80.0],
        Some(-1),
        state.rubbing_roi_mm,
    );
    let full_opaque = full_cimg.pixels.iter().filter(|c| c.a() > 100).count();
    let roi_opaque = roi_cimg.pixels.iter().filter(|c| c.a() > 100).count();
    assert!(roi_opaque > 50 && roi_opaque < full_opaque);
}

#[test]
fn test_tdd15_horizontal_section_above_and_outline_section_with_mm_color_controls() {
    // 1. Image 11: 단면 기본값은 검은 채움이 아닌 외곽선(fill_section_solid == false)이어야 함
    let state = SheetPaneState::default();
    assert!(
        !state.line_style.fill_section_solid,
        "cross-section must default to outline-only (no solid black fill)"
    );
    assert!((state.line_style.section_width_mm - 0.40).abs() < 1e-5);

    // 2. "겹치잖아.? 수평횡단면은 아래에만하라고":
    // 완형 토기 도판에서 수평 횡단면(횡단면_외벽윤곽선)이 평면도와 겹치지 않고 우측 하단(vessel_top_y 아래쪽, vessel_right_x 오른쪽)에만 배치되어야 함!
    let mesh = make_patterned_bowl_mesh();
    let m = compute_artifact_measurements(&mesh, ArtifactCategory::CompleteVessel);
    let vsec = strata_section::slice_mesh_with_fixed_plane(
        &mesh,
        None,
        strata_section::CutPlaneOrientation::VerticalLongitudinal,
        0.0,
        0.0,
        0.25,
    );
    let hsec = strata_section::slice_mesh_with_fixed_plane(
        &mesh,
        None,
        strata_section::CutPlaneOrientation::HorizontalTransverse,
        0.0,
        20.0,
        0.25,
    );
    let plate = strata_export::compose_archaeological_plate(&strata_export::PlateComposeInput {
        title: "테스트 완형 토기",
        measurements: &m,
        vertical_section: &vsec,
        horizontal_sections: &[hsec],
        occluded_spans: &[],
        sherd_subtype: None,
        sherd_strategy: None,
        handle_attachment: None,
        bead_subtype: None,
        bead_drill_type: None,
        wado_cut_ratio: None,
        hemp_density_label: None,
        flip_exterior_right: false,
    });
    let vessel_right_x = plate.raster_regions[0].rect_mm[2];
    let vessel_top_y = plate.raster_regions[0].rect_mm[3];
    let hsec_ring = plate
        .paths
        .iter()
        .find(|p| p.layer_id == "횡단면_외벽윤곽선")
        .expect("horizontal section ring must exist");
    let hsec_mean_x: f64 =
        hsec_ring.points_mm.iter().map(|p| p[0]).sum::<f64>() / (hsec_ring.points_mm.len() as f64);
    let hsec_mean_y: f64 =
        hsec_ring.points_mm.iter().map(|p| p[1]).sum::<f64>() / (hsec_ring.points_mm.len() as f64);
    assert!(
        hsec_mean_x > vessel_right_x && hsec_mean_y < vessel_top_y,
        "horizontal cross-section ({hsec_mean_x:.1}, {hsec_mean_y:.1}mm) must be positioned ONLY in the bottom-right column"
    );
}

#[test]
fn test_tdd16_roi_1to1_rubbing_with_under_image_wheels_and_tight_dimensions() {
    let mut params = AcademicRubbingParams::default();
    let init_ink = params.ink_density_pct;
    let init_contrast = params.sigmoid_contrast;
    let init_sharp = params.msii_detail_boost;
    let init_thresh = params.contact_threshold;

    // Image 14: 탁본 이미지 바로 밑 4종 미세조정 휠 스텝 동작 검증
    params.apply_wheel_step(RubbingFineWheelKind::InkDensity, 5);
    params.apply_wheel_step(RubbingFineWheelKind::SigmoidContrast, 4);
    params.apply_wheel_step(RubbingFineWheelKind::MsiiRidgeSharpness, 6);
    params.apply_wheel_step(RubbingFineWheelKind::PaperContactThreshold, -3);

    assert!((params.ink_density_pct - (init_ink + 5.0)).abs() < 1e-6);
    assert!((params.sigmoid_contrast - (init_contrast + 0.8)).abs() < 1e-4);
    assert!((params.msii_detail_boost - (init_sharp + 0.30)).abs() < 1e-4);
    assert!((params.contact_threshold - (init_thresh - 0.03)).abs() < 1e-4);
}

#[test]
fn test_tdd20_paper_sheet_5cm_red_grid_and_non_overlapping_line_widgets() {
    // 1. "방안지 여백은 무한대로 배경을 하고 용지만 붉은선으로 표시하게 하자":
    // 방안선이 용지 경계(paper_bounds_rect) 바깥 여백까지 무한대(sheet_rect 전체)로 확장되고, 용지 경계 사각형이 정확히 산출됨을 검증!
    let sheet_rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 760.0));
    let spec = compute_paper_sheet_graph_spec(
        sheet_rect,
        PaperSheetPreset::A3Landscape,
        1.0,
        egui::Vec2::ZERO,
    );
    assert!(
        spec.paper_bounds_rect.width() < sheet_rect.width()
            && spec.paper_bounds_rect.height() < sheet_rect.height(),
        "paper bounds must sit inside the viewport as the red-bordered publication sheet"
    );
    assert!(
        !spec.lines_50mm_red_x.is_empty() && !spec.lines_50mm_red_y.is_empty(),
        "must generate 5cm (50mm) thin red grid lines"
    );
    assert!(
        spec.lines_10mm_x
            .iter()
            .any(|&gx| gx < spec.paper_bounds_rect.left() - 2.0)
            && spec
                .lines_10mm_x
                .iter()
                .any(|&gx| gx > spec.paper_bounds_rect.right() + 2.0),
        "infinite graph paper grid lines must extend into the margin beyond paper_bounds_rect"
    );

    // 2. Image 2: 모든 선 조정 위젯이 유물이 움직이면 함께 이동하고 줌인/줌아웃 시 절대 중복(겹침)되지 않아야 함!
    for zoom in [0.55_f32, 1.0_f32, 2.2_f32] {
        let rects = compute_non_overlapping_line_widget_rects(
            egui::pos2(480.0, 420.0),
            zoom * 2.5,
            [-62.0, 0.0, 62.0, 63.0],
            [0.0, 118.0, 26.0],
            sheet_rect,
        );
        assert_eq!(
            rects.len(),
            4,
            "exact 4 line widgets (단면선, 외곽선, 횡단면, 치수선)"
        );
        for i in 0..rects.len() {
            for j in (i + 1)..rects.len() {
                assert!(
                    !rects[i].intersects(rects[j]),
                    "zoom={zoom}: widget {i} {:?} must NEVER overlap widget {j} {:?}",
                    rects[i],
                    rects[j]
                );
            }
        }
    }
}

#[test]
fn test_tdd21_all8_feedback_illustrator_vector_plan_measured_hsec_and_individual_flips() {
    // 1. Image 0 & Image 1: 기본값에서 좌측 평단면은 일러스트용 순수 벡터 선화(show_elevation_rubbing_overlay == false)이고,
    //    우측 상단에 1:1 정밀 탁본(BottomPlanRubbing)이 배치됨을 검증
    let state = SheetPaneState::default();
    assert!(
        !state.show_elevation_rubbing_overlay,
        "left column (1:1 plan + 1:1 section) must default to pure Illustrator vector line art"
    );

    let mesh = make_patterned_bowl_mesh();
    let m = compute_artifact_measurements(&mesh, ArtifactCategory::CompleteVessel);
    let vsec = strata_section::slice_mesh_with_fixed_plane(
        &mesh,
        None,
        strata_section::CutPlaneOrientation::VerticalLongitudinal,
        0.0,
        0.0,
        0.25,
    );
    let hsec = strata_section::slice_mesh_with_fixed_plane(
        &mesh,
        None,
        strata_section::CutPlaneOrientation::HorizontalTransverse,
        0.0,
        24.0,
        0.25,
    );
    let plate = strata_export::compose_archaeological_plate(&strata_export::PlateComposeInput {
        title: "테스트 완형 토기",
        measurements: &m,
        vertical_section: &vsec,
        horizontal_sections: &[hsec],
        occluded_spans: &[],
        sherd_subtype: None,
        sherd_strategy: None,
        handle_attachment: None,
        bead_subtype: None,
        bead_drill_type: None,
        wado_cut_ratio: None,
        hemp_density_label: None,
        flip_exterior_right: false,
    });
    assert!(
        plate
            .paths
            .iter()
            .any(|p| p.layer_id == "평면도_구연내경선"),
        "1:1 plan view must contain Illustrator concentric vector lines"
    );
    assert!(
        plate.paths.iter().any(|p| p.layer_id == "횡단면_단면_실선"),
        "1:1 horizontal cross-section must contain measured cross-section polygon"
    );
    assert!(
        plate.raster_regions.len() >= 2
            && plate.raster_regions[1].rect_mm[0] > plate.raster_regions[0].rect_mm[2],
        "1:1 precision rubbing must be placed in the right column at 1:1 scale"
    );

    // 2. Image 7: 개별 도면 뷰(0~3)의 90° 회전 · 180° 뒤집기 · 앞으로 연속 뒤집기 동작 검증
    let mut tf = IndividualViewTransform2D::default();
    let center = [100.0, 100.0];
    let p = [120.0, 110.0];
    tf.rotate_cw_90();
    let p_rot90 = tf.transform_point_around(p, center);
    assert!((p_rot90[0] - 110.0).abs() < 1e-6 && (p_rot90[1] - 80.0).abs() < 1e-6);

    let mut tf_flip = IndividualViewTransform2D::default();
    tf_flip.flip_forward_continuous_90();
    assert_eq!(tf_flip.flip_forward_steps, 1);
    tf_flip.flip_forward_continuous_90();
    assert_eq!(tf_flip.flip_forward_steps, 2);
    let p_flip180 = tf_flip.transform_point_around(p, center);
    assert!((p_flip180[0] - 120.0).abs() < 1e-6 && (p_flip180[1] - 90.0).abs() < 1e-6);

    // 3. Image 6: 3D 유물 90° 앞으로뒤집기 · 180° 상하뒤집기 검증
    let mut v3d = crate::view3d_pane::View3DPaneState::default();
    v3d.flip_artifact_forward_90();
    assert!((v3d.tilt_deg - 90.0).abs() < 1e-5);
    v3d.flip_artifact_forward_90();
    assert!((v3d.tilt_deg.abs() - 180.0).abs() < 1e-5);
    v3d.flip_artifact_upside_down_180();
    assert!(v3d.tilt_deg.abs() < 1e-5);
}

#[test]
fn test_tdd22_measured_plan_dashed_fractures_light_control_and_6layer_delete() {
    // 1. Image 0 ("실제 모습이 평면도에 나와야하지않나? 깨진부분은 점선아닌가?"):
    // 구연부 일부가 결손(깨진)된 메쉬에서 extract_topdown_plan_slice_2d가 실제 윤곽과 결손 점선(평면도_결손복원_점선)을 생성함을 검증!
    let mut mesh = make_patterned_bowl_mesh();
    for pos in &mut mesh.positions {
        let ang_deg = pos[1].atan2(pos[0]).to_degrees().rem_euclid(360.0);
        if (25.0..=55.0).contains(&ang_deg) && pos[2] > 42.0 {
            pos[2] = 36.0; // 구연부 결손(깨진 부분) 모사
            pos[0] *= 0.86;
            pos[1] *= 0.86;
        }
    }
    let mut v3d = crate::view3d_pane::View3DPaneState::default();
    let top_plan = crate::view3d_pane::extract_topdown_plan_slice_2d(&mesh, &v3d);
    assert!(
        top_plan.polylines_2d.len() >= 3,
        "top-down plan slice must extract outer/inner rim, foot, and broken sector polylines"
    );
    let m = compute_artifact_measurements(&mesh, ArtifactCategory::CompleteVessel);
    let vsec = crate::view3d_pane::slice_transformed_mesh_world_locked(&mesh, &v3d, true);
    let hsec = crate::view3d_pane::slice_transformed_mesh_world_locked(&mesh, &v3d, false);
    v3d.custom_section_line_mm = Some([[-42.0, -18.0], [42.0, 22.0]]);
    let custom_sec = crate::view3d_pane::slice_custom_line_section_2d(&mesh, &v3d)
        .expect("custom section A-A' must be extracted");

    let plate = strata_export::compose_archaeological_plate(&strata_export::PlateComposeInput {
        title: "결손 구연 완형 토기",
        measurements: &m,
        vertical_section: &vsec,
        horizontal_sections: &[hsec, top_plan, custom_sec],
        occluded_spans: &[],
        sherd_subtype: None,
        sherd_strategy: None,
        handle_attachment: None,
        bead_subtype: None,
        bead_drill_type: None,
        wado_cut_ratio: None,
        hemp_density_label: None,
        flip_exterior_right: false,
    });
    assert!(
        plate
            .paths
            .iter()
            .any(|p| p.layer_id == "평면도_결손복원_점선" && p.dash_pattern_mm.is_some()),
        "chipped/broken rim segment in 1:1 plan view must be rendered as a dashed reconstruction line"
    );
    assert!(
        plate
            .paths
            .iter()
            .any(|p| p.layer_id.starts_with("추가단면_")),
        "custom section A-A' must be placed in its dedicated layer"
    );

    // 2. Image 2 ("광원을 조정할수있는 작은창"): 광원 방위각/고도각/세기 벡터 정규화 검증
    v3d.light_azimuth_deg = 225.0;
    v3d.light_elevation_deg = 35.0;
    v3d.light_intensity = 1.35;
    let lvec = v3d.compute_light_direction_vector();
    let llen = (lvec[0] * lvec[0] + lvec[1] * lvec[1] + lvec[2] * lvec[2]).sqrt();
    assert!((llen - 1.0).abs() < 1e-4);

    // 3. 전 레이어 관리 ("레이어 모든것에는 방향조정 90도씩 delete키로 지우기 추가단면선 생성시 이것도 레이어에생성 삭제가능하고 키고끌수있게")
    let mut sheet = SheetPaneState {
        selected_layer_idx: Some(4), // ⑤ 추가 단면 A-A' 선택
        ..SheetPaneState::default()
    };
    sheet.rotate_selected_layer_cw_90();
    assert_eq!(sheet.view_transforms[4].rot_90_steps, 1);
    let deleted = sheet.delete_selected_layer();
    assert_eq!(deleted, Some(4));
    assert!(!sheet.view_transforms[4].visible);
    assert!(
        sheet.request_clear_custom_section,
        "deleting layer 4 (추가 단면 A-A') must also signal clearing custom_section_line_mm"
    );
}
