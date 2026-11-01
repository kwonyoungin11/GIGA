use strata_core::TriMesh;
use eframe::egui;
use rayon::prelude::*;

use super::state::*;

/// 3D 직상방 뷰, 3D 정면 입면 뷰, 3D 적색 단면 교차선, 그리고 우측 2D 도판 단면 슬라이서가
/// **100% 동일한 좌표 변환**을 공유하도록 통합된 정점 변환 함수 (`<USER_REQUEST>` 단면 불일치 근본 해결).
/// - `psi_deg`(`TopTurnCwCcw`): Z축 둘레 시계/반시계 회전
/// - `tilt_deg`(`LeftPitchUpDown`): X축 둘레 상하 회전(올리면 위로, 내리면 아래로)
/// - `roll_deg + inner_level_deg`(`RightTiltLeftRight` + `InnerHorizontalLevel`): Y축 둘레 좌우 수평 기울기 보정
/// - `artifact_offset_x_mm`: 유물 좌우 수평 이동 (`Image 5`, `Image 8`)
#[inline]
#[allow(clippy::too_many_arguments)]
pub fn transform_local_vertex_full(
    p: [f64; 3],
    center_xy: [f64; 2],
    z_min: f64,
    height_mm: f64,
    cos_p: f64,
    sin_p: f64,
    cos_t: f64,
    sin_t: f64,
    cos_r: f64,
    sin_r: f64,
    offset_x_mm: f64,
) -> [f64; 3] {
    let lx = p[0] - center_xy[0];
    let ly = p[1] - center_xy[1];
    let lz = p[2] - z_min;
    let z_mid = 0.5 * height_mm;

    // 1. Z축 둘레 시계·반시계 회전 (ψ)
    let x1 = lx * cos_p - ly * sin_p;
    let y1 = lx * sin_p + ly * cos_p;
    let z1 = lz - z_mid;

    // 2. X축 둘레 상하 회전 (tilt_deg: 올리면 앞면이 위로 회전)
    let x2 = x1;
    let y2 = y1 * cos_t - z1 * sin_t;
    let z2 = y1 * sin_t + z1 * cos_t;

    // 3. Y축 둘레 좌우 수평 기울기 회전 (roll_deg + inner_level_deg)
    let x3 = x2 * cos_r - z2 * sin_r;
    let y3 = y2;
    let z3 = x2 * sin_r + z2 * cos_r;

    [x3 + offset_x_mm, y3, z3 + z_mid]
}

/// 법선 벡터에 동일한 3축 회전(`ψ`, `tilt_deg`, `roll_deg + inner_level_deg`)을 적용한다.
#[inline]
#[allow(clippy::too_many_arguments)]
pub fn transform_local_normal_full(
    n: [f64; 3],
    cos_p: f64,
    sin_p: f64,
    cos_t: f64,
    sin_t: f64,
    cos_r: f64,
    sin_r: f64,
) -> [f64; 3] {
    let x1 = n[0] * cos_p - n[1] * sin_p;
    let y1 = n[0] * sin_p + n[1] * cos_p;
    let z1 = n[2];

    let x2 = x1;
    let y2 = y1 * cos_t - z1 * sin_t;
    let z2 = y1 * sin_t + z1 * cos_t;

    let x3 = x2 * cos_r - z2 * sin_r;
    let y3 = y2;
    let z3 = x2 * sin_r + z2 * cos_r;
    [x3, y3, z3]
}

/// 우측 2D 도판의 수직 종단면(`vertical_section`) 및 수평 횡단면(`horizontal_section`)을 추출할 때
/// 3D 뷰의 5휠 변환(`ψ`, `tilt_deg`, `roll_deg`, `inner_level_deg`, `artifact_offset_x_mm`, `cut_y_mm`)과
/// **100% 일치하는 단면 폴리라인**을 생성한다 (`<USER_REQUEST>`).
pub fn slice_transformed_mesh_world_locked(
    mesh: &TriMesh,
    state: &View3DPaneState,
    is_vertical_y_plane: bool,
) -> strata_section::PlaneSliceResult {
    let segs = extract_3d_plane_intersection_segments_full(mesh, state, is_vertical_y_plane);
    let mut segments_2d: Vec<[[f64; 2]; 2]> = Vec::with_capacity(segs.len());
    for seg in &segs {
        if is_vertical_y_plane {
            segments_2d.push([[seg[0][0], seg[0][2]], [seg[1][0], seg[1][2]]]);
        } else {
            segments_2d.push([
                [seg[0][0], seg[0][1] - state.cut_y_mm],
                [seg[1][0], seg[1][1] - state.cut_y_mm],
            ]);
        }
    }
    let polylines_2d = chain_2d_segments_into_polylines(&segments_2d, 0.45);
    strata_section::PlaneSliceResult {
        orientation: if is_vertical_y_plane {
            strata_section::CutPlaneOrientation::VerticalLongitudinal
        } else {
            strata_section::CutPlaneOrientation::HorizontalTransverse
        },
        psi_deg: state.active_section_angle_deg,
        offset_mm: if is_vertical_y_plane {
            state.cut_y_mm
        } else {
            // 변환된 월드 좌표는 `z = 0`(유물 바닥) 기준이므로, 절단면 높이도 그 기준값을 그대로 쓴다.
            // (예전에는 `bbox.min[2]` 를 더해 100mm 이상 어긋난 값이 우측 도판 `수평 횡단면 (Z=…)`
            //  라벨·절단 위치선에 찍혔다.)
            state.top_cut_z_mm
        },
        segments_2d,
        polylines_2d,
    }
}

/// Image 5: 직상방 뷰에서 사용자가 그은 자유 단면선(`custom_section_line_mm = Some([p0, p1])`)을 따라
/// 3D 유물 메쉬를 수직 절단하여 우측 2D 도면에 그릴 추가 단면 폴리라인을 추출한다.
pub fn slice_custom_line_section_2d(
    mesh: &TriMesh,
    state: &View3DPaneState,
) -> Option<strata_section::PlaneSliceResult> {
    let [p0, p1] = state.custom_section_line_mm?;
    let wx0 = p0[0];
    let wy0 = state.cut_y_mm + p0[1];
    let wx1 = p1[0];
    let wy1 = state.cut_y_mm + p1[1];
    let dx = wx1 - wx0;
    let dy = wy1 - wy0;
    let seg_len = (dx * dx + dy * dy).sqrt();
    if seg_len < 1.5 {
        return None;
    }
    let tx = dx / seg_len;
    let ty = dy / seg_len;
    let nx = -ty;
    let ny = tx;
    let mx = 0.5 * (wx0 + wx1);
    let my = 0.5 * (wy0 + wy1);
    let half_span = 0.5 * seg_len + 2.0;

    let ext = mesh.bbox.extents();
    let height_mm = ext[2].max(1.0);
    let center_xy = [
        0.5 * (mesh.bbox.min[0] + mesh.bbox.max[0]),
        0.5 * (mesh.bbox.min[1] + mesh.bbox.max[1]),
    ];
    let z_min = mesh.bbox.min[2];
    let (sin_p, cos_p) = state.active_section_angle_deg.to_radians().sin_cos();
    let (sin_t, cos_t) = state.tilt_deg.to_radians().sin_cos();
    let total_roll_deg = state.roll_deg + state.inner_level_deg;
    let (sin_r, cos_r) = total_roll_deg.to_radians().sin_cos();
    let off_x = state.artifact_offset_x_mm;

    let world_verts: Vec<[f64; 3]> = mesh
        .positions
        .par_iter()
        .map(|&pos| {
            transform_local_vertex_full(
                pos, center_xy, z_min, height_mm, cos_p, sin_p, cos_t, sin_t, cos_r, sin_r, off_x,
            )
        })
        .collect();

    let mut segments_2d: Vec<[[f64; 2]; 2]> = Vec::new();
    for tri in &mesh.triangles {
        let w0 = world_verts[tri[0] as usize];
        let w1 = world_verts[tri[1] as usize];
        let w2 = world_verts[tri[2] as usize];
        let pts = [w0, w1, w2];
        let d = [
            (w0[0] - mx) * nx + (w0[1] - my) * ny,
            (w1[0] - mx) * nx + (w1[1] - my) * ny,
            (w2[0] - mx) * nx + (w2[1] - my) * ny,
        ];
        if (d[0] > 0.0 && d[1] > 0.0 && d[2] > 0.0)
            || (d[0] < 0.0 && d[1] < 0.0 && d[2] < 0.0)
        {
            continue;
        }
        let mut hits = Vec::with_capacity(3);
        for &(a, b) in &[(0usize, 1usize), (1, 2), (2, 0)] {
            if (d[a] >= 0.0 && d[b] < 0.0) || (d[a] < 0.0 && d[b] >= 0.0) {
                let denom = d[a] - d[b];
                let t = if denom.abs() > 1e-12 {
                    d[a] / denom
                } else {
                    0.5
                };
                let hx = pts[a][0] + t * (pts[b][0] - pts[a][0]);
                let hy = pts[a][1] + t * (pts[b][1] - pts[a][1]);
                let hz = pts[a][2] + t * (pts[b][2] - pts[a][2]);
                let u = (hx - mx) * tx + (hy - my) * ty;
                hits.push([u, hz]);
            }
        }
        if hits.len() >= 2
            && hits[0][0].abs() <= half_span
            && hits[1][0].abs() <= half_span
        {
            segments_2d.push([hits[0], hits[1]]);
        }
    }
    let polylines_2d = chain_2d_segments_into_polylines(&segments_2d, 0.50);
    Some(strata_section::PlaneSliceResult {
        orientation: strata_section::CutPlaneOrientation::VerticalLongitudinal,
        psi_deg: dy.atan2(dx).to_degrees().rem_euclid(360.0),
        offset_mm: seg_len,
        segments_2d,
        polylines_2d,
    })
}

/// `Image 0 ("그리고 평면 아닌가? 실제 모습이 평면도에 나와야하지않나? 깨진부분은 점선아닌가?")` &
/// `Image 1 ("전부 방향이 같아야한다")`:
/// 현재 3D 뷰의 유물 회전·상하뒤집기·기울기(`ψ`, `tilt`, `roll`, `inner_level`, `offset_x`, `cut_y`)가 100% 동일하게 적용된
/// 직상방(`+Z`에서 내려다본 `X-Y` 평면) 실제 3D 메쉬 평면 윤곽(`외곽선`, `구연내경선`, `굽외경선`, `내저원각선`)과
/// 구연부 깨진 부분(결손 구간) 섹터를 추출한다.
pub fn extract_topdown_plan_slice_2d(
    mesh: &TriMesh,
    state: &View3DPaneState,
) -> strata_section::PlaneSliceResult {
    const N_SECTORS: usize = 72;
    let ext = mesh.bbox.extents();
    let height_mm = ext[2].max(1.0);
    let r_nom = (0.5 * ext[0].max(ext[1])).max(10.0);
    let wall_t = (r_nom * 0.088).clamp(2.8, 9.5);
    let center_xy = [
        0.5 * (mesh.bbox.min[0] + mesh.bbox.max[0]),
        0.5 * (mesh.bbox.min[1] + mesh.bbox.max[1]),
    ];
    let z_min = mesh.bbox.min[2];
    let (sin_p, cos_p) = state.active_section_angle_deg.to_radians().sin_cos();
    let (sin_t, cos_t) = state.tilt_deg.to_radians().sin_cos();
    let total_roll_deg = state.roll_deg + state.inner_level_deg;
    let (sin_r, cos_r) = total_roll_deg.to_radians().sin_cos();
    let off_x = state.artifact_offset_x_mm;
    let cut_y = state.cut_y_mm;

    let mut bin_max_r = [0.0_f64; N_SECTORS];
    let mut bin_rim_z = [f64::NEG_INFINITY; N_SECTORS];
    let mut bin_foot_r = [0.0_f64; N_SECTORS];
    let mut bin_has = [false; N_SECTORS];

    for &pos in &mesh.positions {
        let w = transform_local_vertex_full(
            pos, center_xy, z_min, height_mm, cos_p, sin_p, cos_t, sin_t, cos_r, sin_r, off_x,
        );
        let dx = w[0] - off_x;
        let dy = w[1] - cut_y;
        let r = dx.hypot(dy);
        if r < 1.0 || !r.is_finite() {
            continue;
        }
        let ang = dy.atan2(dx);
        let norm = ((ang + std::f64::consts::PI) / std::f64::consts::TAU).clamp(0.0, 0.999_999);
        let bi = (norm * (N_SECTORS as f64)) as usize;
        bin_has[bi] = true;
        if r > bin_max_r[bi] {
            bin_max_r[bi] = r;
        }
        if r >= r_nom * 0.72 && w[2] > bin_rim_z[bi] {
            bin_rim_z[bi] = w[2];
        }
        let is_flipped = state.tilt_deg.abs() > 90.0;
        let in_foot_band = if is_flipped {
            w[2] >= height_mm * 0.76
        } else {
            w[2] <= height_mm * 0.24
        };
        if in_foot_band && r < r_nom * 0.76 && r > bin_foot_r[bi] {
            bin_foot_r[bi] = r;
        }
    }

    let mut outer_r = [r_nom; N_SECTORS];
    let mut foot_r = [(r_nom * 0.42).max(8.0); N_SECTORS];
    for k in 0..N_SECTORS {
        let mut sum_o = 0.0;
        let mut sum_w = 0.0;
        let mut sum_f = 0.0;
        let mut sum_fw = 0.0;
        for d in -1isize..=1isize {
            let nk = ((k as isize + d).rem_euclid(N_SECTORS as isize)) as usize;
            let w = if d == 0 { 1.0 } else { 0.45 };
            if bin_has[nk] {
                sum_o += bin_max_r[nk] * w;
                sum_w += w;
            }
            if bin_foot_r[nk] > 3.0 {
                sum_f += bin_foot_r[nk] * w;
                sum_fw += w;
            }
        }
        if sum_w > 0.0 {
            outer_r[k] = sum_o / sum_w;
        }
        if sum_fw > 0.0 {
            foot_r[k] = (sum_f / sum_fw).clamp(r_nom * 0.28, r_nom * 0.72);
        }
    }

    let mut sorted_r = outer_r.to_vec();
    sorted_r.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let recon_outer_r = sorted_r[(sorted_r.len() * 4 / 5).min(sorted_r.len().saturating_sub(1))].max(10.0);
    let recon_inner_r = (recon_outer_r - wall_t).max(4.0);

    let mut valid_zs: Vec<f64> = bin_rim_z.iter().copied().filter(|z| z.is_finite()).collect();
    valid_zs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let ref_rim_z = if valid_zs.is_empty() {
        height_mm
    } else {
        valid_zs[(valid_zs.len() * 4 / 5).min(valid_zs.len().saturating_sub(1))]
    };

    let mut is_broken = [false; N_SECTORS];
    let mut min_rel_z = f64::INFINITY;
    let mut min_rel_k = 54usize;
    for k in 0..N_SECTORS {
        if !bin_has[k]
            || outer_r[k] < recon_outer_r - (recon_outer_r * 0.035).max(1.4)
            || (bin_rim_z[k].is_finite()
                && bin_rim_z[k] < ref_rim_z - (height_mm * 0.038).max(1.4))
        {
            is_broken[k] = true;
        }
        if bin_rim_z[k].is_finite() {
            let dz = bin_rim_z[k] - ref_rim_z;
            if dz < min_rel_z {
                min_rel_z = dz;
                min_rel_k = k;
            }
        }
    }
    // 구연부 미세 결손(깨진 부분)이 정점 밀도상 완만하게 나타난 경우에도 최저 구연 고도 구간(±2 섹터)을 결손부(점선+파손실선)로 표시
    let mut expanded_broken = is_broken;
    if !is_broken.iter().any(|&b| b) {
        for d in -2isize..=2isize {
            let nk = ((min_rel_k as isize + d).rem_euclid(N_SECTORS as isize)) as usize;
            expanded_broken[nk] = true;
        }
    } else {
        for (k, &b) in is_broken.iter().enumerate() {
            if b {
                for d in -1isize..=1isize {
                    let nk = ((k as isize + d).rem_euclid(N_SECTORS as isize)) as usize;
                    expanded_broken[nk] = true;
                }
            }
        }
    }
    is_broken = expanded_broken;
    for (k, &b) in is_broken.iter().enumerate() {
        if b && outer_r[k] >= recon_outer_r - 1.2 {
            outer_r[k] = (recon_outer_r - wall_t * 0.72).max(recon_inner_r + 0.6);
        }
    }

    let mut outer_ring = Vec::with_capacity(N_SECTORS);
    let mut inner_ring = Vec::with_capacity(N_SECTORS);
    let mut foot_outer_ring = Vec::with_capacity(N_SECTORS);
    let mut floor_inner_ring = Vec::with_capacity(N_SECTORS);
    let mut segments_2d = Vec::new();

    for k in 0..N_SECTORS {
        let a =
            -std::f64::consts::PI + ((k as f64) + 0.5) * std::f64::consts::TAU / (N_SECTORS as f64);
        let (s, c) = a.sin_cos();
        let ro = outer_r[k];
        let ri = (ro - wall_t).max(4.0);
        let rf = foot_r[k];
        let rfl = (rf - wall_t * 0.82).max(3.5);
        outer_ring.push([ro * c, ro * s]);
        inner_ring.push([ri * c, ri * s]);
        foot_outer_ring.push([rf * c, rf * s]);
        floor_inner_ring.push([rfl * c, rfl * s]);
        if is_broken[k] {
            segments_2d.push([[k as f64, k as f64], [recon_outer_r, recon_inner_r]]);
        }
    }

    strata_section::PlaneSliceResult {
        orientation: strata_section::CutPlaneOrientation::HorizontalTransverse,
        psi_deg: state.active_section_angle_deg,
        offset_mm: -9999.0,
        segments_2d,
        polylines_2d: vec![outer_ring, inner_ring, foot_outer_ring, floor_inner_ring],
    }
}

pub(crate) fn chain_2d_segments_into_polylines(
    segs: &[[[f64; 2]; 2]],
    snap_tol_mm: f64,
) -> Vec<Vec<[f64; 2]>> {
    if segs.is_empty() {
        return Vec::new();
    }
    let mut used = vec![false; segs.len()];
    let mut polylines: Vec<Vec<[f64; 2]>> = Vec::new();
    let tol2 = snap_tol_mm * snap_tol_mm;

    for start_idx in 0..segs.len() {
        if used[start_idx] {
            continue;
        }
        used[start_idx] = true;
        let mut chain = vec![segs[start_idx][0], segs[start_idx][1]];

        loop {
            let mut extended = false;
            let tail = *chain.last().unwrap_or(&[0.0, 0.0]);
            let mut best_j = None;
            let mut best_rev = false;
            let mut best_d2 = tol2;

            for (j, s) in segs.iter().enumerate() {
                if used[j] {
                    continue;
                }
                let d0 = (s[0][0] - tail[0]).powi(2) + (s[0][1] - tail[1]).powi(2);
                if d0 <= best_d2 {
                    best_d2 = d0;
                    best_j = Some(j);
                    best_rev = false;
                }
                let d1 = (s[1][0] - tail[0]).powi(2) + (s[1][1] - tail[1]).powi(2);
                if d1 <= best_d2 {
                    best_d2 = d1;
                    best_j = Some(j);
                    best_rev = true;
                }
            }
            if let Some(j) = best_j {
                used[j] = true;
                let next_pt = if best_rev { segs[j][0] } else { segs[j][1] };
                chain.push(next_pt);
                extended = true;
            }

            let head = chain[0];
            let mut best_front_j = None;
            let mut best_front_rev = false;
            let mut best_front_d2 = tol2;
            for (j, s) in segs.iter().enumerate() {
                if used[j] {
                    continue;
                }
                let d1 = (s[1][0] - head[0]).powi(2) + (s[1][1] - head[1]).powi(2);
                if d1 <= best_front_d2 {
                    best_front_d2 = d1;
                    best_front_j = Some(j);
                    best_front_rev = false;
                }
                let d0 = (s[0][0] - head[0]).powi(2) + (s[0][1] - head[1]).powi(2);
                if d0 <= best_front_d2 {
                    best_front_d2 = d0;
                    best_front_j = Some(j);
                    best_front_rev = true;
                }
            }
            if let Some(j) = best_front_j {
                used[j] = true;
                let prev_pt = if best_front_rev { segs[j][1] } else { segs[j][0] };
                chain.insert(0, prev_pt);
                extended = true;
            }

            if !extended {
                break;
            }
        }
        if chain.len() >= 3 {
            polylines.push(chain);
        }
    }
    polylines.sort_by_key(|b| std::cmp::Reverse(b.len()));
    polylines
}

/// 기존 시그니처 호환용 3D 교차 선분 추출 함수.
#[allow(dead_code)] // 테스트 전용 원시 절단면 헬퍼: 앱 UI 경로는 `extract_3d_plane_intersection_segments_full` 만 쓴다.
pub fn extract_3d_plane_intersection_segments(
    mesh: &TriMesh,
    psi_deg: f64,
    tilt_deg: f64,
    plane_val_mm: f64,
    is_vertical_y_plane: bool,
) -> Vec<[[f64; 3]; 2]> {
    let state = View3DPaneState {
        active_section_angle_deg: psi_deg,
        tilt_deg,
        cut_y_mm: if is_vertical_y_plane { plane_val_mm } else { 0.0 },
        top_cut_z_mm: if is_vertical_y_plane { 18.0 } else { plane_val_mm },
        ..Default::default()
    };
    extract_3d_plane_intersection_segments_full(mesh, &state, is_vertical_y_plane)
}

/// 5휠 전체 변환(`ψ`, `tilt_deg`, `roll_deg`, `inner_level_deg`, `artifact_offset_x_mm`, `cut_y_mm`)이 반영된
/// 월드 고정 절단 평면(`wy = cut_y_mm` 또는 `wz = top_cut_z_mm`)과 3D 메쉬의 100% 정밀 교차 선분을 추출한다.
pub fn extract_3d_plane_intersection_segments_full(
    mesh: &TriMesh,
    state: &View3DPaneState,
    is_vertical_y_plane: bool,
) -> Vec<[[f64; 3]; 2]> {
    let ext = mesh.bbox.extents();
    let height_mm = ext[2].max(1.0);
    let center_xy = [
        0.5 * (mesh.bbox.min[0] + mesh.bbox.max[0]),
        0.5 * (mesh.bbox.min[1] + mesh.bbox.max[1]),
    ];
    let z_min = mesh.bbox.min[2];
    let (sin_p, cos_p) = state.active_section_angle_deg.to_radians().sin_cos();
    let (sin_t, cos_t) = state.tilt_deg.to_radians().sin_cos();
    let total_roll_deg = state.roll_deg + state.inner_level_deg;
    let (sin_r, cos_r) = total_roll_deg.to_radians().sin_cos();
    let off_x = state.artifact_offset_x_mm;
    let plane_val_mm = if is_vertical_y_plane {
        state.cut_y_mm
    } else {
        state.top_cut_z_mm
    };

    let world_verts: Vec<[f64; 3]> = mesh
        .positions
        .par_iter()
        .map(|&pos| {
            transform_local_vertex_full(
                pos, center_xy, z_min, height_mm, cos_p, sin_p, cos_t, sin_t, cos_r, sin_r, off_x,
            )
        })
        .collect();

    let mut segs = Vec::new();
    for tri in &mesh.triangles {
        let w0 = world_verts[tri[0] as usize];
        let w1 = world_verts[tri[1] as usize];
        let w2 = world_verts[tri[2] as usize];
        let pts = [w0, w1, w2];
        let d = if is_vertical_y_plane {
            [
                w0[1] - plane_val_mm,
                w1[1] - plane_val_mm,
                w2[1] - plane_val_mm,
            ]
        } else {
            [
                w0[2] - plane_val_mm,
                w1[2] - plane_val_mm,
                w2[2] - plane_val_mm,
            ]
        };

        if (d[0] > 0.0 && d[1] > 0.0 && d[2] > 0.0)
            || (d[0] < 0.0 && d[1] < 0.0 && d[2] < 0.0)
        {
            continue;
        }

        let mut hits = Vec::with_capacity(3);
        for &(a, b) in &[(0usize, 1usize), (1, 2), (2, 0)] {
            if (d[a] >= 0.0 && d[b] < 0.0) || (d[a] < 0.0 && d[b] >= 0.0) {
                let denom = d[a] - d[b];
                let t = if denom.abs() > 1e-12 {
                    d[a] / denom
                } else {
                    0.5
                };
                hits.push([
                    pts[a][0] + t * (pts[b][0] - pts[a][0]),
                    pts[a][1] + t * (pts[b][1] - pts[a][1]),
                    pts[a][2] + t * (pts[b][2] - pts[a][2]),
                ]);
            }
        }
        if hits.len() >= 2 {
            segs.push([hits[0], hits[1]]);
        }
    }
    segs
}

/// 경계가 뒤집힌 경우에도 패닉 없이 `[a, b]`(순서 보정) 범위로 클램프한다.
/// 편평하거나 아주 얇은 유물(석기·소편)에서 `min > max` 로 `f64::clamp` 가 패닉하는 것을 막는다.
pub(crate) fn clamp_ordered(value: f64, a: f64, b: f64) -> f64 {
    if value.is_nan() {
        return a.min(b);
    }
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    value.clamp(lo, hi)
}

pub(crate) fn draw_vertical_dashed_line(
    painter: &egui::Painter,
    x: f32,
    y0: f32,
    y1: f32,
    stroke: egui::Stroke,
) {
    let min_y = y0.min(y1);
    let max_y = y0.max(y1);
    let dash = 4.0_f32;
    let gap = 3.0_f32;
    let mut y = min_y;
    while y < max_y {
        let yn = (y + dash).min(max_y);
        painter.line_segment([egui::pos2(x, y), egui::pos2(x, yn)], stroke);
        y += dash + gap;
    }
}
