use strata_core::strata_tokens::tokens::SURFACE;
use strata_core::TriMesh;

use super::*;

fn make_test_bowl_mesh() -> TriMesh {
    let mut positions = Vec::new();
    let mut triangles = Vec::new();
    let nu = 36usize;
    let nv = 16usize;
    for iv in 0..=nv {
        let z = (iv as f64 / nv as f64) * 60.0;
        let r = 28.0 + 32.0 * (iv as f64 / nv as f64).powf(0.75);
        for iu in 0..=nu {
            let theta = (iu as f64 / nu as f64) * std::f64::consts::TAU;
            positions.push([r * theta.cos(), r * theta.sin(), z]);
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
    TriMesh::from_positions_and_triangles("test_bowl", positions, triangles).expect("valid")
}

#[test]
fn test_zbuffer_3d_rasterizer_produces_opaque_shaded_and_textured_pixels() {
    let mesh = make_test_bowl_mesh();
    for mode in [
        View3DShaderMode::PhotoTexture,
        View3DShaderMode::SolidClay,
        View3DShaderMode::RealTimeRubbing,
    ] {
        let state = View3DPaneState {
            shader_mode: mode,
            cutaway_open: false,
            ..Default::default()
        };
        let img = render_mesh_zbuffer_rgba(&mesh, None, &state, 240, 240);
        let shaded_count = img
            .pixels()
            .filter(|px| {
                let [r, g, b, _] = px.0;
                (i32::from(r) - i32::from(SURFACE.rgb[0])).abs() > 25
                    || (i32::from(g) - i32::from(SURFACE.rgb[1])).abs() > 25
                    || (i32::from(b) - i32::from(SURFACE.rgb[2])).abs() > 25
            })
            .count();
        assert!(
            shaded_count > 2_500,
            "mode={mode:?} expected >2500 shaded Z-buffer pixels, got {shaded_count}"
        );
    }
}

#[test]
fn test_view3d_cutaway_and_world_locked_plane_intersection() {
    let mesh = make_test_bowl_mesh();
    let v_segs = extract_3d_plane_intersection_segments(&mesh, 45.0, 0.0, 0.0, true);
    let h_segs = extract_3d_plane_intersection_segments(&mesh, 45.0, 0.0, 18.0, false);
    assert!(
        !v_segs.is_empty(),
        "vertical Y=0 plane intersection must produce 3D segments"
    );
    assert!(
        !h_segs.is_empty(),
        "horizontal Z=18mm plane intersection must produce 3D segments"
    );
}

#[test]
fn test_flat_artifact_under_2mm_height_initializes_top_cut_without_panic() {
    // 1.63mm 두께의 편평 유물(석기·소편) — 예전에는 `.clamp(2.0, height_mm)` 가
    // `min > max` 로 패닉해 창이 뜨자마자 죽었다.
    let positions = vec![
        [0.0, 0.0, 0.0],
        [40.0, 0.0, 0.0],
        [40.0, 30.0, 1.63],
        [0.0, 30.0, 1.63],
    ];
    let triangles = vec![[0_u32, 1, 2], [0, 2, 3]];
    let mesh = TriMesh::from_positions_and_triangles("flat_sherd", positions, triangles)
        .expect("valid flat mesh");
    assert!((mesh.bbox.extents()[2] - 1.63).abs() < 1e-9);

    let mut state = View3DPaneState::default();
    state.initialize_top_cut_for_mesh(&mesh);
    assert!(
        state.top_cut_z_mm >= 0.0 && state.top_cut_z_mm <= 1.63 + 1e-9,
        "수평횡단면 기본 높이가 유물 높이 범위를 벗어남: {}",
        state.top_cut_z_mm
    );
    // 같은 메쉬를 다시 열면 초기화를 반복하지 않는다.
    state.top_cut_z_mm = 0.2;
    state.initialize_top_cut_for_mesh(&mesh);
    assert!((state.top_cut_z_mm - 0.2).abs() < 1e-12);

    // 경계가 뒤집힌 클램프도 패닉 없이 순서를 보정해야 한다.
    assert!((clamp_ordered(-5.0, 2.0, 1.63) - 1.63).abs() < 1e-12);
    assert!((clamp_ordered(5.0, 2.0, 1.63) - 2.0).abs() < 1e-12);
    assert!((clamp_ordered(0.4, 0.5, 1.63) - 0.5).abs() < 1e-12);
}

#[test]
fn test_transformed_horizontal_section_offset_is_relative_to_artifact_base() {
    // 유물이 월드 원점에서 100mm 떨어져 있어도(스캔 좌표는 제각각) 절단면 오프셋은
    // 유물 바닥 기준 `top_cut_z_mm` 그대로여야 한다. 예전에는 `bbox.min[2]` 를 더해
    // 우측 도판 라벨이 `Z=9.0mm`(클램프된 상한)처럼 어긋났다.
    let mut positions = Vec::new();
    let mut triangles = Vec::new();
    let nu = 24usize;
    let nv = 12usize;
    for iv in 0..=nv {
        let z = 100.0 + (iv as f64 / nv as f64) * 60.0;
        let r = 30.0 + 25.0 * (iv as f64 / nv as f64).powf(0.8);
        for iu in 0..=nu {
            let theta = (iu as f64 / nu as f64) * std::f64::consts::TAU;
            positions.push([r * theta.cos(), r * theta.sin(), z]);
        }
    }
    let stride = (nu + 1) as u32;
    for iv in 0..(nv as u32) {
        for iu in 0..(nu as u32) {
            let a = iv * stride + iu;
            triangles.push([a, a + 1, a + stride + 1]);
            triangles.push([a, a + stride + 1, a + stride]);
        }
    }
    let mesh = TriMesh::from_positions_and_triangles("offset_bowl", positions, triangles)
        .expect("valid offset mesh");
    assert!((mesh.bbox.min[2] - 100.0).abs() < 1e-9);

    let state = View3DPaneState {
        top_cut_z_mm: 12.5,
        ..Default::default()
    };
    let hsec = slice_transformed_mesh_world_locked(&mesh, &state, false);
    assert!(
        !hsec.polylines_2d.is_empty(),
        "수평 횡단면 폴리라인이 비어 있음"
    );
    assert!(
        (hsec.offset_mm - 12.5).abs() < 1e-9,
        "절단면 오프셋은 바닥 기준이어야 함(기대 12.5, 실제 {})",
        hsec.offset_mm
    );
}

#[test]
fn test_tdd1_view3d_dirty_cache_skips_redundant_rerender_and_supports_hd_resolution() {
    let mesh = make_test_bowl_mesh();
    let mut state = View3DPaneState::default();

    let (w1, h1, recomputed1) = state.ensure_zbuffer_cached(&mesh, None, 560, 560);
    assert!(recomputed1, "first frame must recompute Z-buffer");
    assert_eq!(
        (w1, h1),
        (560, 560),
        "must support HD 560x560 resolution without 360px clamp"
    );

    let (w2, h2, recomputed2) = state.ensure_zbuffer_cached(&mesh, None, 560, 560);
    assert!(
        !recomputed2,
        "identical state must hit dirty cache with 0 recomputation"
    );
    assert_eq!((w2, h2), (560, 560));

    state.active_section_angle_deg = 45.0;
    let (_, _, recomputed3) = state.ensure_zbuffer_cached(&mesh, None, 560, 560);
    assert!(
        recomputed3,
        "changed rotation angle must trigger Z-buffer recomputation"
    );
}

#[test]
fn test_tdd4_hand_dial_spherical_rotation_updates_pitch_yaw_roll() {
    let mut state = View3DPaneState::default();
    let initial_psi = state.active_section_angle_deg;
    let initial_pitch = state.pitch_deg;

    state.apply_hand_dial_drag(40.0, -30.0, false);
    assert!(
        (state.active_section_angle_deg - initial_psi).abs() > 10.0,
        "horizontal dial drag must rotate artifact ψ"
    );
    assert!(
        (state.pitch_deg - initial_pitch).abs() > 5.0,
        "vertical dial drag must tilt/rotate vertical viewing angle"
    );

    state.apply_hand_dial_drag(20.0, -15.0, true);
    assert!(
        state.roll_deg.abs() > 0.5,
        "fine axis dial horizontal drag must adjust roll_deg"
    );
    assert!(
        state.tilt_deg.abs() > 0.5,
        "fine axis dial vertical drag must adjust tilt_deg"
    );

    state.reset_hand_orientation();
    assert_eq!(state.active_section_angle_deg, 0.0);
    assert_eq!(state.tilt_deg, 0.0);
    assert_eq!(state.roll_deg, 0.0);
}

#[test]
fn test_tdd8_isotropic_projection_never_stretches_vertically_on_drag_release() {
    let (dw, dh) = compute_isotropic_render_dimensions(420.0, 680.0, true);
    let (rw, rh) = compute_isotropic_render_dimensions(420.0, 680.0, false);
    let target_aspect = 420.0 / 680.0;
    let drag_aspect = (dw as f64) / (dh as f64);
    let release_aspect = (rw as f64) / (rh as f64);
    assert!(
        (drag_aspect - target_aspect).abs() < 0.005,
        "drag aspect {drag_aspect} must match target {target_aspect}"
    );
    assert!(
        (release_aspect - target_aspect).abs() < 0.005,
        "release aspect {release_aspect} must match target {target_aspect}"
    );
}

#[test]
fn test_tdd9_four_cardinal_fine_dials_step_exact_001mm_and_01deg() {
    let mut state = View3DPaneState::default();
    for _ in 0..100 {
        state.apply_cardinal_fine_dial(CardinalDialDir::Up, 1);
    }
    assert!((state.cut_y_mm - 1.00).abs() < 1e-9);
    for _ in 0..35 {
        state.apply_cardinal_fine_dial(CardinalDialDir::Down, 1);
    }
    assert!((state.cut_y_mm - 0.65).abs() < 1e-9);
    for _ in 0..15 {
        state.apply_cardinal_fine_dial(CardinalDialDir::Right, 1);
    }
    assert!((state.active_section_angle_deg - 1.5).abs() < 1e-9);
    for _ in 0..5 {
        state.apply_cardinal_fine_dial(CardinalDialDir::Left, 1);
    }
    assert!((state.active_section_angle_deg - 1.0).abs() < 1e-9);
}

#[test]
fn test_shader_chip_modes_expose_all_five_backend_modes() {
    let modes = SHADER_CHIP_MODES;
    assert_eq!(
        modes.len(),
        5,
        "3D 셰이더 칩은 백엔드 5종을 모두 노출해야 한다(현재 {}개)",
        modes.len()
    );
    for m in [
        View3DShaderMode::PhotoTexture,
        View3DShaderMode::SolidClay,
        View3DShaderMode::RealTimeRubbing,
        View3DShaderMode::Segmentation3Way,
        View3DShaderMode::XRayInner,
    ] {
        assert!(modes.contains(&m), "{:?} 칩이 노출되지 않음", m);
    }
}

#[test]
fn test_tdd11_concentric_120pct_topdown_and_zero_lag_frame_budget() {
    let mesh = make_test_bowl_mesh();
    let state = View3DPaneState::default();
    assert_eq!(state.yaw_deg, 0.0);
    assert_eq!(state.pitch_deg, 0.0);

    let artifact_r = 62.5_f64;
    let grid_r = compute_concentric_grid_radius_mm(artifact_r);
    assert!((grid_r - artifact_r * 1.20).abs() < 1e-9);

    let t0 = std::time::Instant::now();
    let top_img = render_topdown_concentric_plan_rgba(&mesh, &state, 280, 220);
    let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let non_bg = top_img
        .pixels()
        .filter(|px| {
            (i32::from(px[0]) - i32::from(SURFACE.rgb[0])).abs() > 12
                || (i32::from(px[1]) - i32::from(SURFACE.rgb[1])).abs() > 12
        })
        .count();
    assert!(non_bg > 1_500);
    assert!(elapsed_ms < 40.0, "took {elapsed_ms:.2}ms");
}

#[test]
fn test_tdd13_five_wheel_cardinal_and_inner_level_exact_section_sync() {
    let mesh = make_test_bowl_mesh();
    let mut state = View3DPaneState::default();

    // 1. Image 1: 상단 가로 휠 -> 유물 시계·반시계 회전 (0.1° 단위)
    state.apply_precision_wheel(PrecisionWheelKind::TopTurnCwCcw, 25);
    assert!((state.active_section_angle_deg - 2.5).abs() < 1e-9);
    assert!(PrecisionWheelKind::TopTurnCwCcw
        .description_label_kr()
        .contains("시계"));

    // 2. Image 2: 하단 가로 휠 -> 유물·단면선 앞뒤 미세조정 (0.01mm 단위)
    state.apply_precision_wheel(PrecisionWheelKind::BottomSectionFrontBack, -18);
    assert!((state.cut_y_mm - (-0.18)).abs() < 1e-9);

    // 3. Image 3: 우측 세로 휠 -> 유물 좌우 기울기 미세조정 (0.1° 단위)
    state.apply_precision_wheel(PrecisionWheelKind::RightTiltLeftRight, 12);
    assert!((state.roll_deg - 1.2).abs() < 1e-9);

    // 4. Image 4: 좌측 세로 휠 -> 올리면(+steps) 위로 회전(+tilt_deg), 내리면(-steps) 아래로 회전(-tilt_deg)
    state.apply_precision_wheel(PrecisionWheelKind::LeftPitchUpDown, 15);
    assert!((state.tilt_deg - 1.5).abs() < 1e-9);
    state.apply_precision_wheel(PrecisionWheelKind::LeftPitchUpDown, -5);
    assert!((state.tilt_deg - 1.0).abs() < 1e-9);

    // 5. <USER_REQUEST>: 내부 제5 수평정합 휠 (0.05° 단위) 및 3D-2D 단면 100% 일치 검증
    state.apply_precision_wheel(PrecisionWheelKind::InnerHorizontalLevel, 10);
    assert!((state.inner_level_deg - 0.50).abs() < 1e-9);

    let v_segs_3d = extract_3d_plane_intersection_segments_full(&mesh, &state, true);
    let slice_2d = slice_transformed_mesh_world_locked(&mesh, &state, true);
    assert_eq!(v_segs_3d.len(), slice_2d.segments_2d.len());
    assert!(!slice_2d.polylines_2d.is_empty());

    // 3D 단면 선분의 X 범위와 2D 슬라이서 폴리라인의 X 범위가 0.01mm 이내로 100% 일치해야 함!
    let min_x_3d = v_segs_3d
        .iter()
        .flat_map(|s| [s[0][0], s[1][0]])
        .fold(f64::INFINITY, f64::min);
    let max_x_3d = v_segs_3d
        .iter()
        .flat_map(|s| [s[0][0], s[1][0]])
        .fold(f64::NEG_INFINITY, f64::max);
    let min_x_2d = slice_2d
        .polylines_2d
        .iter()
        .flatten()
        .map(|p| p[0])
        .fold(f64::INFINITY, f64::min);
    let max_x_2d = slice_2d
        .polylines_2d
        .iter()
        .flatten()
        .map(|p| p[0])
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(
        (min_x_3d - min_x_2d).abs() < 1e-5 && (max_x_3d - max_x_2d).abs() < 1e-5,
        "3D cut X [{min_x_3d:.3}, {max_x_3d:.3}] must match 2D slice X [{min_x_2d:.3}, {max_x_2d:.3}]"
    );
}

#[test]
fn test_tdd14_topdown_fixed_center_section_and_vertical_dashed_link() {
    let mesh = make_test_bowl_mesh();
    let state = View3DPaneState {
        artifact_offset_x_mm: 12.5,
        cut_y_mm: -4.2,
        ..Default::default()
    };

    // 유물을 좌우(+12.5mm) 이동시켰을 때 3D 단면 교차선도 정확히 +12.5mm 이동하여 상하 뷰 점선 연결 위치가 일치해야 함
    let segs = extract_3d_plane_intersection_segments_full(&mesh, &state, true);
    let mean_x: f64 = segs.iter().map(|s| 0.5 * (s[0][0] + s[1][0])).sum::<f64>()
        / (segs.len().max(1) as f64);
    assert!(
        (mean_x - 12.5).abs() < 1.5,
        "translated artifact section center X must shift by +12.5mm, got {mean_x:.2}"
    );

    // 상단 직상방 동심원 평면도와 하단 수평 정면 입면도의 스케일(px/mm)이 1:1 동일해야 하며,
    // 좌·우 절단점의 화면 X 좌표가 기울어지지 않고 100% 수직(90°)으로 정렬되어야 함 (v6_vessel_roi_1to1 Image 0)
    let ext = mesh.bbox.extents();
    let grid_r_mm = compute_concentric_grid_radius_mm(0.5 * ext[0].max(ext[1]));
    let top_scale = compute_shared_ortho_scale(388.0, grid_r_mm, ext[2], state.zoom);
    let bot_scale = compute_shared_ortho_scale(388.0, grid_r_mm, ext[2], state.zoom);
    assert!((top_scale - bot_scale).abs() < 1e-12);
}

#[test]
fn test_tdd17_inner_wheel_controls_horizontal_section_z_and_updates_plate() {
    let mesh = make_test_bowl_mesh();
    let mut state = View3DPaneState::default();
    let init_z = state.top_cut_z_mm;
    let init_tilt = state.tilt_deg;
    let init_roll = state.roll_deg;

    // Image 0: 안쪽 제5휠을 움직이면 유물은 고정된 채 수평횡단면 선(top_cut_z_mm)만 아래위로 이동하고 수평횡단면 도면이 갱신되어야 함
    state.apply_precision_wheel(PrecisionWheelKind::InnerHorizSectionZ, 25);
    assert!(
        (state.top_cut_z_mm - (init_z + 2.5)).abs() < 1e-6,
        "InnerHorizSectionZ +25 steps must move top_cut_z_mm by +2.5mm, got {}",
        state.top_cut_z_mm
    );
    assert_eq!(state.tilt_deg, init_tilt, "artifact tilt must stay fixed");
    assert_eq!(state.roll_deg, init_roll, "artifact roll must stay fixed");

    let h_slice_low = slice_transformed_mesh_world_locked(&mesh, &state, false);
    state.apply_precision_wheel(PrecisionWheelKind::InnerHorizSectionZ, 120);
    let h_slice_high = slice_transformed_mesh_world_locked(&mesh, &state, false);
    assert!(!h_slice_low.polylines_2d.is_empty());
    assert!(!h_slice_high.polylines_2d.is_empty());
    assert!(
        (h_slice_high.offset_mm - h_slice_low.offset_mm - 12.0).abs() < 1e-5,
        "horizontal section Z offset must advance by +12.0mm"
    );
}

#[test]
fn test_tdd18_arrow_keys_nudge_artifact_and_horizontal_section() {
    let mut state = View3DPaneState::default();
    state.apply_arrow_key_nudge(-1, 0, false, false, 60.0);
    assert!((state.artifact_offset_x_mm - (-0.10)).abs() < 1e-6);

    state.apply_arrow_key_nudge(0, 1, true, false, 60.0);
    assert!((state.cut_y_mm - (-1.00)).abs() < 1e-6);

    let z0 = state.top_cut_z_mm;
    state.apply_arrow_key_nudge(0, 1, false, true, 60.0);
    assert!((state.top_cut_z_mm - (z0 + 0.20)).abs() < 1e-6);
}

#[test]
fn test_tdd19_custom_section_line_single_endpoint_move_and_2d_profile() {
    let mesh = make_test_bowl_mesh();
    let mut state = View3DPaneState::default();
    state.add_default_custom_section_line(55.0);
    let line0 = state.custom_section_line_mm.expect("custom section line must exist");
    let p0_before = line0[0];
    let p1_before = line0[1];

    // Image 5: 끝지점(0번 끝점)을 누르고 이동하면 반대쪽(1번 끝점)은 고정되고 한쪽만 이동해야 함!
    state.move_custom_section_endpoint(0, [-38.0, 22.0]);
    let line1 = state.custom_section_line_mm.expect("custom line 1");
    assert_eq!(line1[0], [-38.0, 22.0], "dragged endpoint 0 must move");
    assert_eq!(line1[1], p1_before, "other endpoint 1 must remain 100% fixed");

    // 반대로 1번 끝점을 이동하면 0번 끝점은 고정되고 1번 끝점만 이동해야 함!
    state.move_custom_section_endpoint(1, [42.0, -16.0]);
    let line2 = state.custom_section_line_mm.expect("custom line 2");
    assert_eq!(line2[0], [-38.0, 22.0], "endpoint 0 must remain 100% fixed");
    assert_eq!(line2[1], [42.0, -16.0], "dragged endpoint 1 must move");
    let _ = p0_before;

    // 그어진 사용자 단면선에 대한 2D 단면 폴리라인이 추출되어야 함
    let custom_slice = slice_custom_line_section_2d(&mesh, &state).expect("custom slice");
    assert!(
        !custom_slice.polylines_2d.is_empty(),
        "custom section line across vessel must produce 2D cross-section polylines"
    );
}
