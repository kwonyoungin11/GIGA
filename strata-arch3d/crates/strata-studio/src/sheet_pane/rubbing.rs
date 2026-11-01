use super::config::*;
use eframe::egui;
use image::{Rgba, RgbaImage};
use rayon::prelude::*;
use strata_core::{ScalePreset, TriMesh};
use strata_export::PlateRasterKind;
use strata_measure::ArtifactMeasurementReport;

/// 3D 메쉬로부터 도판 합성용 직교투영(Orthographic) 정밀 탁본(`Rubbing`) RGBA 영상을 생성한다.
pub fn render_orthographic_rubbing_rgba(
    mesh: &TriMesh,
    kind: PlateRasterKind,
    psi_deg: f64,
    ink_density_pct: f64,
    width: u32,
    height: u32,
) -> RgbaImage {
    let params = AcademicRubbingParams {
        ink_density_pct,
        ..Default::default()
    };
    render_orthographic_rubbing_rgba_with_params(mesh, kind, psi_deg, &params, width, height)
}

/// 메쉬별 1~2단계 이산 미분기하(`weld_vertices` + `Nelson Max 법선` + `Meyer Cotangent H/K` + `Modified Frangi Vesselness`) 캐시 조회
pub(crate) fn get_or_build_ddg_mesh(
    mesh: &TriMesh,
) -> std::sync::Arc<crate::rubbing_ddg_pipeline::TopologicalDdgMesh> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    type DdgCacheMap =
        HashMap<(String, usize, usize), Arc<crate::rubbing_ddg_pipeline::TopologicalDdgMesh>>;
    static DDG_CACHE: OnceLock<Mutex<DdgCacheMap>> = OnceLock::new();
    let cache = DDG_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (
        mesh.name.clone(),
        mesh.positions.len(),
        mesh.triangles.len(),
    );
    if let Ok(guard) = cache.lock() {
        if let Some(found) = guard.get(&key) {
            return Arc::clone(found);
        }
    }
    let built = Arc::new(crate::rubbing_ddg_pipeline::TopologicalDdgMesh::from_trimesh(mesh));
    if let Ok(mut guard) = cache.lock() {
        guard.insert(key, Arc::clone(&built));
    }
    built
}

/// 학술 논문 및 오픈소스 5단계 DDG 디지털 탁본 파이프라인:
/// 1) `weld_vertices` 중복 정점 병합 + Nelson Max (1999) 가중 정점 법선
/// 2) Meyer et al. (2003) Cotangent Laplace-Beltrami 곡률($H, K$) + Murtala et al. (2021) Modified Frangi Vesselness
/// 3) Mara et al. (2010) 다중 스케일 구면 적분 불변량(MSII) + 반경 제한 PCV AO + Vergne et al. (2010) Radiance Scaling
/// 4) Winnemöller et al. (2012) XDoG (`σ_fine = 0.8`, `σ_coarse = 14.0`) 대역통과 필터링
/// 5) 5-Octave FBM Perlin 한지 섬유 + Cellular Worley 암석 입자 + Interleaved Gradient Noise (Blue-Noise) 다계조 점묘 합성
///
/// 도판 캔버스가 용지 안에 들어가는 **가장 큰 축척**을 고른다(1:1 우선, 안 되면 1:2 → 1:3 → 1:4).
/// 대형 유물에서 1:1 을 고집해 도판이 종이 밖으로 잘리는 문제를 막는다.
pub fn fit_scale_preset(
    canvas_w_mm: f64,
    canvas_h_mm: f64,
    paper_w_mm: f64,
    paper_h_mm: f64,
    margin_mm: f64,
) -> ScalePreset {
    let avail_w = (paper_w_mm - margin_mm * 2.0).max(10.0);
    let avail_h = (paper_h_mm - margin_mm * 2.0).max(10.0);
    for preset in [
        ScalePreset::Full1To1,
        ScalePreset::Half1To2,
        ScalePreset::Third1To3,
        ScalePreset::Quarter1To4,
    ] {
        let r = preset.ratio();
        if canvas_w_mm * r <= avail_w && canvas_h_mm * r <= avail_h {
            return preset;
        }
    }
    ScalePreset::Quarter1To4
}

/// 탁본 파라미터를 결정론적으로 요약한 64비트 해시(FNV-1a).
/// 같은 값이면 언제나 같은 해시가 나오므로 도판 출처(provenance)에 각인할 수 있다.
pub fn rubbing_params_hash(p: &AcademicRubbingParams) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |v: i64| {
        for b in v.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    feed(p.ink_density_pct.round() as i64);
    feed((p.msii_detail_boost * 100.0).round() as i64);
    feed((p.sigmoid_contrast * 10.0).round() as i64);
    feed((p.contact_threshold * 100.0).round() as i64);
    feed(match p.style {
        AcademicRubbingStyle::WetHanjiInk => 1,
        AcademicRubbingStyle::DryGraphite => 2,
        AcademicRubbingStyle::MsiiRidgeEnhance => 3,
    });
    feed((p.pose_tilt_deg * 10.0).round() as i64);
    feed((p.pose_roll_deg * 10.0).round() as i64);
    feed(p.light_azimuth_deg.round() as i64);
    feed(p.light_elevation_deg.round() as i64);
    feed((p.light_intensity * 100.0).round() as i64);
    feed(match p.unroll_mode {
        None => 0,
        Some(strata_shade::UnrollMode::Cylindrical) => 1,
        Some(strata_shade::UnrollMode::Conical) => 2,
        Some(strata_shade::UnrollMode::Spherical) => 3,
    });
    h
}

/// 도판에 각인할 탁본 출처 한 줄. 값이 바뀌면 문자열도 바뀐다.
pub fn rubbing_provenance_line(p: &AcademicRubbingParams) -> String {
    let unroll = match p.unroll_mode {
        None => "평면",
        Some(strata_shade::UnrollMode::Cylindrical) => "원통",
        Some(strata_shade::UnrollMode::Conical) => "원뿔",
        Some(strata_shade::UnrollMode::Spherical) => "구면",
    };
    format!(
        "L-Std {:.0}/{:.0}/{:.2} · MSII {:.2} · Sigmoid {:.1} · Ink {:.0}% · 전개={} · 투영={} · #{:08x}",
        p.light_azimuth_deg,
        p.light_elevation_deg,
        p.light_intensity,
        p.msii_detail_boost,
        p.sigmoid_contrast,
        p.ink_density_pct,
        unroll,
        if p.unroll_mode.is_some() { "원통전개" } else { "직교" },
        (rubbing_params_hash(p) & 0xffff_ffff) as u32
    )
}

/// CLI `--plate-mode <1..8|auto>` 값을 도판 모드로 해석한다.
#[allow(dead_code)] // 테스트·CLI 인자 해석용(앱 UI 는 템플릿 코드로 전환한다).
pub fn parse_plate_mode(arg: &str) -> Option<strata_export::PlateLayoutMode> {
    let s = arg.trim();
    if s.eq_ignore_ascii_case("auto") || s.is_empty() {
        return None;
    }
    let n = s.parse::<usize>().ok()?;
    if (1..=strata_export::PlateLayoutMode::ALL.len()).contains(&n) {
        Some(strata_export::PlateLayoutMode::ALL[n - 1])
    } else {
        None
    }
}

/// CLI `--unroll <mode>` 값을 전개 모드로 해석한다. `none`/미지정은 평면 탁본(`None`).
pub fn parse_unroll_mode(arg: &str) -> Option<strata_shade::UnrollMode> {
    match arg.trim().to_ascii_lowercase().as_str() {
        "cylindrical" | "cylinder" | "원통" => Some(strata_shade::UnrollMode::Cylindrical),
        "conical" | "cone" | "원뿔" => Some(strata_shade::UnrollMode::Conical),
        "spherical" | "sphere" | "구면" => Some(strata_shade::UnrollMode::Spherical),
        _ => None,
    }
}

/// 전개 탁본 텍스처 해상도를 물리 종횡비에 맞춰 계산한다.
/// 정사각형으로 굽으면 모눈 위 1:1 계측이 깨지므로, 긴 변을 `base_px` 로 두고 짧은 변을 비례 축소한다.
pub fn unrolled_texture_dims(size_mm: [f64; 2], base_px: u32) -> (u32, u32) {
    let w_mm = size_mm[0].max(1e-3);
    let h_mm = size_mm[1].max(1e-3);
    let long = w_mm.max(h_mm);
    let base = base_px.clamp(64, 960) as f64;
    if (w_mm - long).abs() < 1e-9 {
        let h = (base * h_mm / w_mm).round().clamp(48.0, 960.0);
        (base as u32, h as u32)
    } else {
        let w = (base * w_mm / h_mm).round().clamp(48.0, 960.0);
        (w as u32, base as u32)
    }
}

/// 곡면 전개도에서 봉합선($\theta = \pm\pi$)을 가로지르는 삼각형 판별.
/// 세 정점의 전개 좌표가 총 폭의 절반 이상 벌어지면 봉합선 스패닝으로 보고 컬링한다.
pub fn is_seam_spanning_triangle(u: [f64; 3], total_width_mm: f64) -> bool {
    if total_width_mm <= 1e-6 {
        return false;
    }
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for v in u {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    // 전개 범위는 [-πR, +πR] = [-w/2, +w/2] 이므로,
    // 양 끝 극단(±45% 폭)에 정점이 동시에 걸린 삼각형만 봉합선 스패닝으로 본다.
    lo < -total_width_mm * 0.45 && hi > total_width_mm * 0.45
}

pub fn render_orthographic_rubbing_rgba_with_params(
    mesh: &TriMesh,
    kind: PlateRasterKind,
    psi_deg: f64,
    params: &AcademicRubbingParams,
    width: u32,
    height: u32,
) -> RgbaImage {
    let w = width.clamp(48, 960) as usize;
    let h = height.clamp(48, 960) as usize;
    let mut img = RgbaImage::from_pixel(w as u32, h as u32, Rgba([255, 255, 255, 0]));
    if mesh.positions.is_empty() || mesh.triangles.is_empty() {
        return img;
    }

    let ddg = get_or_build_ddg_mesh(mesh);
    let center_xy = [
        0.5 * (mesh.bbox.min[0] + mesh.bbox.max[0]),
        0.5 * (mesh.bbox.min[1] + mesh.bbox.max[1]),
    ];
    let z_min = mesh.bbox.min[2];
    let height_mm = (mesh.bbox.max[2] - mesh.bbox.min[2]).max(1.0);
    let psi_rad = psi_deg.to_radians();
    let (sin_p, cos_p) = psi_rad.sin_cos();
    let tilt_rad = params.pose_tilt_deg.to_radians();
    let (sin_t, cos_t) = tilt_rad.sin_cos();
    let roll_rad = params.pose_roll_deg.to_radians();
    let (sin_r, cos_r) = roll_rad.sin_cos();
    let az_rad = params.light_azimuth_deg.to_radians();
    let el_rad = params.light_elevation_deg.clamp(5.0, 88.0).to_radians();
    let light_lx = el_rad.cos() * az_rad.cos();
    let light_lz = el_rad.cos() * (-az_rad.sin());
    let light_ly = el_rad.sin();
    let light_int = params.light_intensity.clamp(0.4, 1.8);

    // 1. Rayon 병렬 정점 직교 투영 좌표 (u, v, depth) 및 Nelson Max 가중 정점 법선 회전 계산
    // 곡면 전개 탁본 모드에서는 정점별 (호 길이 s, 높이 h, 부조 깊이 Δr) 로 대체한다.
    let unrolled: Option<Vec<[f64; 3]>> = params.unroll_mode.map(|mode| {
        let res = strata_shade::unroll_mesh_surface(mesh, mode);
        let mut acc = vec![[0.0_f64; 3]; ddg.positions.len()];
        for pt in &res.points {
            if let Some(slot) = acc.get_mut(pt.vertex_index as usize) {
                *slot = [pt.arc_s_mm, pt.height_h_mm, pt.relief_depth_mm];
            }
        }
        acc
    });
    // Image 1 ("전부 방향이 같아야한다 탁복한 유물과 방향이다르다"):
    // 3D 직상방/정면 뷰 및 1:1 평면도와 동일한 3D 회전(psi_deg, pose_tilt_deg, pose_roll_deg)을 적용한다!
    let projected: Vec<([f64; 3], [f32; 3])> = ddg
        .positions
        .par_iter()
        .enumerate()
        .map(|(vi, &p)| {
            let [rx, ry, rz] = crate::view3d_pane::transform_local_vertex_full(
                p, center_xy, z_min, height_mm, cos_p, sin_p, cos_t, sin_t, cos_r, sin_r, 0.0,
            );

            let n = ddg.normals.get(vi).copied().unwrap_or([0.0, -1.0, 0.0]);
            let [nrx, nry, nrz] = crate::view3d_pane::transform_local_normal_full(
                n, cos_p, sin_p, cos_t, sin_t, cos_r, sin_r,
            );

            let (u, v, depth, vn) = match &unrolled {
                // 곡면 전개 탁본: 원통/원뿔/구면 0~360° 무왜곡 전개 좌표 사용
                Some(coords) if kind == PlateRasterKind::BottomPlanRubbing => {
                    match coords.get(vi) {
                        Some(&[su, sv, sd]) => (su, sv, sd, [nrx, nrz, nry]),
                        None => (rx, ry, rz, [nrx, nrz, nry]),
                    }
                }
                _ => match kind {
                    PlateRasterKind::FrontExteriorRubbing => (rx, rz, -ry, [nrx, -nry, nrz]),
                    PlateRasterKind::BackInteriorRubbing => (-rx, rz, ry, [-nrx, nry, nrz]),
                    // 3D 직상방 뷰 및 1:1 평면도와 100% 동일한 직상방(위에서 아래로 내려다보는 +rz 우선) 투영!
                    PlateRasterKind::BottomPlanRubbing => (rx, ry, rz, [nrx, nrz, nry]),
                },
            };
            ([u, v, depth], [vn[0] as f32, vn[1] as f32, vn[2] as f32])
        })
        .collect();

    let mut u_min = f64::INFINITY;
    let mut u_max = f64::NEG_INFINITY;
    let mut v_min = f64::INFINITY;
    let mut v_max = f64::NEG_INFINITY;
    for &(uvz, _) in &projected {
        u_min = u_min.min(uvz[0]);
        u_max = u_max.max(uvz[0]);
        v_min = v_min.min(uvz[1]);
        v_max = v_max.max(uvz[1]);
    }

    let du = (u_max - u_min).max(1e-3);
    let dv = (v_max - v_min).max(1e-3);
    let pad = 2.0f64;
    let sx = ((w as f64) - pad * 2.0) / du;
    let sy = ((h as f64) - pad * 2.0) / dv;

    let mut zbuf = vec![f32::NEG_INFINITY; w * h];
    let mut nbuf = vec![[0.0f32, 1.0f32, 0.0f32]; w * h];
    let mut curv_buf = vec![0.0f32; w * h];
    let mut frangi_buf = vec![0.0f32; w * h];
    let mut hit_buf = vec![false; w * h];

    let edge_pad = -1.5e-3_f32;

    for tri in &ddg.faces {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;
        let (p0, n0) = projected[i0];
        let (p1, n1) = projected[i1];
        let (p2, n2) = projected[i2];

        // 봉합선을 가로지르는 삼각형은 전개도 전체를 가로지르는 줄무늬를 만든다 → 컬링
        if unrolled.is_some() && is_seam_spanning_triangle([p0[0], p1[0], p2[0]], du) {
            continue;
        }

        let x0 = (pad + (p0[0] - u_min) * sx) as f32;
        let y0 = ((h as f64) - 1.0 - pad - (p0[1] - v_min) * sy) as f32;
        let x1 = (pad + (p1[0] - u_min) * sx) as f32;
        let y1 = ((h as f64) - 1.0 - pad - (p1[1] - v_min) * sy) as f32;
        let x2 = (pad + (p2[0] - u_min) * sx) as f32;
        let y2 = ((h as f64) - 1.0 - pad - (p2[1] - v_min) * sy) as f32;

        let min_x = x0.min(x1).min(x2).floor().max(0.0) as i32;
        let max_x = x0.max(x1).max(x2).ceil().min((w - 1) as f32) as i32;
        let min_y = y0.min(y1).min(y2).floor().max(0.0) as i32;
        let max_y = y0.max(y1).max(y2).ceil().min((h - 1) as f32) as i32;
        if min_x > max_x || min_y > max_y {
            continue;
        }

        let denom = (y1 - y2) * (x0 - x2) + (x2 - x1) * (y0 - y2);
        if denom.abs() < 1e-6 {
            continue;
        }
        let inv_denom = 1.0 / denom;

        let mc0 = ddg.mean_curvature[i0] as f32;
        let mc1 = ddg.mean_curvature[i1] as f32;
        let mc2 = ddg.mean_curvature[i2] as f32;
        let fr0 = ddg.frangi_vesselness[i0] as f32;
        let fr1 = ddg.frangi_vesselness[i1] as f32;
        let fr2 = ddg.frangi_vesselness[i2] as f32;

        // 삼각형 플랫 법선 + Nelson Max 정점 법선 결합
        let e01 = [
            (p1[0] - p0[0]) as f32,
            (p1[2] - p0[2]) as f32,
            (p1[1] - p0[1]) as f32,
        ];
        let e02 = [
            (p2[0] - p0[0]) as f32,
            (p2[2] - p0[2]) as f32,
            (p2[1] - p0[1]) as f32,
        ];
        let mut fn_proj = [
            e01[1] * e02[2] - e01[2] * e02[1],
            e01[2] * e02[0] - e01[0] * e02[2],
            e01[0] * e02[1] - e01[1] * e02[0],
        ];
        let fn_len = (fn_proj[0] * fn_proj[0] + fn_proj[1] * fn_proj[1] + fn_proj[2] * fn_proj[2])
            .sqrt()
            .max(1e-6);
        fn_proj[0] /= fn_len;
        fn_proj[1] /= fn_len;
        fn_proj[2] /= fn_len;
        if fn_proj[1] < 0.0 {
            fn_proj[0] = -fn_proj[0];
            fn_proj[1] = -fn_proj[1];
            fn_proj[2] = -fn_proj[2];
        }

        for py in min_y..=max_y {
            let fy = py as f32 + 0.5;
            for px in min_x..=max_x {
                let fx = px as f32 + 0.5;
                let w0 = ((y1 - y2) * (fx - x2) + (x2 - x1) * (fy - y2)) * inv_denom;
                let w1 = ((y2 - y0) * (fx - x2) + (x0 - x2) * (fy - y2)) * inv_denom;
                let w2 = 1.0 - w0 - w1;
                if w0 < edge_pad || w1 < edge_pad || w2 < edge_pad {
                    continue;
                }
                let z = (w0 as f64 * p0[2] + w1 as f64 * p1[2] + w2 as f64 * p2[2]) as f32;
                let idx = (py as usize) * w + (px as usize);
                if z > zbuf[idx] {
                    zbuf[idx] = z;
                    hit_buf[idx] = true;
                    let mut vnx = w0 * n0[0] + w1 * n1[0] + w2 * n2[0];
                    let mut vny = w0 * n0[1] + w1 * n1[1] + w2 * n2[1];
                    let mut vnz = w0 * n0[2] + w1 * n1[2] + w2 * n2[2];
                    if vny < 0.0 {
                        vnx = -vnx;
                        vny = -vny;
                        vnz = -vnz;
                    }
                    let nx = 0.62 * fn_proj[0] + 0.38 * vnx;
                    let ny = 0.62 * fn_proj[1] + 0.38 * vny;
                    let nz = 0.62 * fn_proj[2] + 0.38 * vnz;
                    let nlen = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-5);
                    nbuf[idx] = [nx / nlen, ny / nlen, nz / nlen];
                    curv_buf[idx] = w0 * mc0 + w1 * mc1 + w2 * mc2;
                    frangi_buf[idx] = w0 * fr0 + w1 * fr1 + w2 * fr2;
                }
            }
        }
    }

    // 2. Summed-Area Table (Integral Image) 구축: 다중스케일 구면 적분 불변량(MSII) 초고속 계산
    let sw = w + 1;
    let sh = h + 1;
    let mut sat_z = vec![0.0f64; sw * sh];
    let mut sat_c = vec![0u32; sw * sh];
    for y in 0..h {
        let mut row_z = 0.0f64;
        let mut row_c = 0u32;
        for x in 0..w {
            let idx = y * w + x;
            if hit_buf[idx] {
                row_z += f64::from(zbuf[idx]);
                row_c += 1;
            }
            let s_idx = (y + 1) * sw + (x + 1);
            sat_z[s_idx] = sat_z[y * sw + (x + 1)] + row_z;
            sat_c[s_idx] = sat_c[y * sw + (x + 1)] + row_c;
        }
    }

    let box_mean_z_fast = |cx: usize, cy: usize, radius: usize| -> f32 {
        let x0 = cx.saturating_sub(radius);
        let y0 = cy.saturating_sub(radius);
        let x1 = (cx + radius + 1).min(w);
        let y1 = (cy + radius + 1).min(h);
        let cnt =
            sat_c[y1 * sw + x1] + sat_c[y0 * sw + x0] - sat_c[y0 * sw + x1] - sat_c[y1 * sw + x0];
        if cnt > 0 {
            let sum = sat_z[y1 * sw + x1] + sat_z[y0 * sw + x0]
                - sat_z[y0 * sw + x1]
                - sat_z[y1 * sw + x0];
            (sum / f64::from(cnt)) as f32
        } else {
            zbuf[cy * w + cx]
        }
    };

    // 3. 4단계: Cotangent H + Modified Frangi + MSII + 반경 제한 PCV AO + Vergne (2010) Radiance Scaling
    //    -> Contact Probability Map (contact_map) 산출
    let detail_boost = params.msii_detail_boost.clamp(0.4, 3.0);
    let k_sig = params.sigmoid_contrast.clamp(2.0, 16.0);
    let contact_thresh = params.contact_threshold.clamp(-0.45, 0.25);
    let pad_threshold = (0.44 + contact_thresh).clamp(0.10, 0.72);
    let curv_weight = (k_sig * 0.80).clamp(2.0, 14.0);

    let mut contact_map = vec![0.0_f32; w * h];
    contact_map
        .par_chunks_mut(w)
        .enumerate()
        .for_each(|(y, out_row)| {
            let yu = y.saturating_sub(1);
            let yd = (y + 1).min(h - 1);
            for x in 0..w {
                let idx = y * w + x;
                if !hit_buf[idx] {
                    continue;
                }
                let xl = x.saturating_sub(1);
                let xr = (x + 1).min(w - 1);
                let n_l = nbuf[y * w + xl];
                let n_r = nbuf[y * w + xr];
                let n_u = nbuf[yu * w + x];
                let n_d = nbuf[yd * w + x];

                // (a) Vergne et al. (2010) Screen-Space Radiance Scaling (shader.wgsl line 32)
                let dn_x = (n_r[0] - n_l[0]) * 0.5;
                let dn_z = (n_u[2] - n_d[2]) * 0.5;
                let dn_edge = ((n_r[0] - n_l[0]).hypot(n_r[2] - n_l[2])
                    + (n_u[0] - n_d[0]).hypot(n_u[2] - n_d[2]))
                    * 0.55;
                let screen_curv = (dn_x + dn_z) * 1.35 + dn_edge * 0.65;

                // (b) Meyer et al. (2003) Cotangent Mean Curvature (H) + Modified Frangi Vesselness
                let cot_h = (curv_buf[idx].abs() * screen_curv.signum()).clamp(-1.5, 1.5);
                let frangi_v = (frangi_buf[idx] * 0.65).clamp(0.0, 1.2);

                // (c) Mara et al. (2010) 다중 스케일 구면 적분 불변량(MSII) & 곡면 기울기 제거 DoG
                let z_c = zbuf[idx];
                let z_r1 = box_mean_z_fast(x, y, 1);
                let z_r2 = box_mean_z_fast(x, y, 2);
                let z_r4 = box_mean_z_fast(x, y, 4);
                let z_r8 = box_mean_z_fast(x, y, 8);
                let curv_cancel_1_2 = ((z_c - z_r1) - 0.333_333 * (z_c - z_r2)) * 9.5;
                let curv_cancel_2_4 = ((z_c - z_r2) - 0.20 * (z_c - z_r4)) * 5.6;
                let curv_cancel_4_8 = ((z_c - z_r4) - 0.22 * (z_c - z_r8)) * 3.2;
                let msii_val =
                    ((0.46 * curv_cancel_1_2 + 0.34 * curv_cancel_2_4 + 0.20 * curv_cancel_4_8)
                        * detail_boost)
                        .clamp(-1.8, 1.8);

                // (d) 반경 제한 PCV Ambient Occlusion (타솔이 음각 홈 바닥에 닿지 못하는 차폐율)
                let local_cavity_depth = (z_r4 - z_c).max(0.0);
                let ao_val = (-local_cavity_depth * 2.2).exp().clamp(0.05, 1.0);

                // (e) raster_rubbing.rs lines 133-142: tanh 곡률 정규화 + Smoothstep 타솔 접촉 확률
                let combined = 0.44 * msii_val
                    + 0.32 * screen_curv
                    + 0.14 * cot_h
                    + 0.10 * frangi_v * msii_val.signum();
                let norm_curv = 0.5 * ((curv_weight * combined).tanh() + 1.0);

                let denom_pad = (1.0 - pad_threshold).max(0.05);
                let t = ((norm_curv - pad_threshold) / denom_pad).clamp(0.0, 1.0);
                let smooth = t * t * (3.0 - 2.0 * t);
                let n_cur = nbuf[idx];
                let nz_vis = n_cur[1].clamp(0.15, 1.0);
                let ndotl = (n_cur[0] * light_lx + n_cur[1] * light_ly + n_cur[2] * light_lz)
                    .clamp(0.12, 1.0);
                let relief_light = (0.78 + 0.28 * ndotl * light_int).clamp(0.55, 1.35);

                out_row[x] =
                    (ao_val.powf(1.35) * smooth * nz_vis.powf(0.28) * relief_light).clamp(0.0, 1.0);
            }
        });

    // 4. 5단계: Winnemöller et al. (2012) XDoG (`σ_fine = 0.8`, `σ_coarse = 14.0`) +
    //    5-Octave FBM Perlin 한지 섬유 노이즈 + Cellular Worley 암석 입자 노이즈 +
    //    Interleaved Gradient Noise (Blue-Noise 프로파일) 다계조 점묘(Stippling) 합성
    //    (raster_rubbing.rs lines 156-224)
    let (blur_fine, blur_coarse) = rayon::join(
        || {
            crate::rubbing_ddg_pipeline::gaussian_blur_separable_f32(
                &contact_map,
                w as u32,
                h as u32,
                0.8,
            )
        },
        || {
            crate::rubbing_ddg_pipeline::gaussian_blur_separable_f32(
                &contact_map,
                w as u32,
                h as u32,
                14.0,
            )
        },
    );

    let ink_strength = ((params.ink_density_pct as f32) / 78.0).clamp(0.25, 1.35);
    let xdog_phi = (detail_boost * 7.0).clamp(4.0, 20.0);
    let xdog_epsilon = -0.015_f32;
    let (grain_strength, paper_strength, stipple_softness) = match params.style {
        AcademicRubbingStyle::WetHanjiInk => (0.20_f32, 0.13_f32, 0.14_f32),
        AcademicRubbingStyle::DryGraphite => (0.28_f32, 0.20_f32, 0.22_f32),
        AcademicRubbingStyle::MsiiRidgeEnhance => (0.12_f32, 0.08_f32, 0.09_f32),
    };

    img.as_mut()
        .par_chunks_mut(w * 4)
        .enumerate()
        .for_each(|(y, row_bytes)| {
            let yu = y.saturating_sub(1);
            let yd = (y + 1).min(h - 1);
            let y_f = y as f32;
            for x in 0..w {
                let idx = y * w + x;
                if !hit_buf[idx] {
                    continue;
                }

                let xl = x.saturating_sub(1);
                let xr = (x + 1).min(w - 1);
                let is_boundary = !hit_buf[y * w + xl]
                    || !hit_buf[y * w + xr]
                    || !hit_buf[yu * w + x]
                    || !hit_buf[yd * w + x];

                let px_off = x * 4;
                if is_boundary {
                    row_bytes[px_off] = 20;
                    row_bytes[px_off + 1] = 20;
                    row_bytes[px_off + 2] = 19;
                    row_bytes[px_off + 3] = 248;
                    continue;
                }

                let x_f = x as f32;
                let c_val = (contact_map[idx] * ink_strength).clamp(0.0, 1.0);
                let g_fine = blur_fine[idx];
                let g_coarse = blur_coarse[idx];

                let dog = g_fine - g_coarse;
                let xdog = if dog >= xdog_epsilon {
                    1.0
                } else {
                    1.0 + (xdog_phi * (dog - xdog_epsilon)).tanh()
                };

                // 양각 문양(인화문·물레속선 마루, dog > 0) 부각 + 음각 홈(base_ink < 0.08) 순백 한지 보존
                let dog_boost = (dog * 2.2 * ink_strength).clamp(0.0, 0.52);
                let base_ink = ((c_val * xdog) + dog_boost).clamp(0.0, 1.0);

                let n_paper = crate::rubbing_ddg_pipeline::fbm_perlin_paper_5oct(x_f, y_f);
                let n_stone = crate::rubbing_ddg_pipeline::worley_stone_grain(x_f, y_f);

                if base_ink < 0.08 {
                    let paper_grain = (n_paper * 4.0) as u8;
                    row_bytes[px_off] = 250u8.saturating_sub(paper_grain);
                    row_bytes[px_off + 1] = 249u8.saturating_sub(paper_grain);
                    row_bytes[px_off + 2] = 245u8.saturating_sub(paper_grain);
                    row_bytes[px_off + 3] = 248;
                    continue;
                }

                let textured_ink = (base_ink
                    * (1.0 - grain_strength * n_stone - paper_strength * n_paper))
                    .clamp(0.0, 1.0);

                // Interleaved Gradient Noise (Blue-noise profile spatial hash, raster_rubbing.rs line 210)
                let ign = ((52.982_918_f32 * ((0.06711056 * x_f + 0.00583715 * y_f).fract()))
                    .fract()
                    + 1.0)
                    .fract();

                let target_whiteness = 1.0 - textured_ink;
                let s = stipple_softness.max(1e-4);
                let stippled_luma = ((target_whiteness - ign + s) / (2.0 * s)).clamp(0.0, 1.0);

                row_bytes[px_off] = (18.0 + stippled_luma * (250.0 - 18.0)).round() as u8;
                row_bytes[px_off + 1] = (18.0 + stippled_luma * (249.0 - 18.0)).round() as u8;
                row_bytes[px_off + 2] = (17.0 + stippled_luma * (245.0 - 17.0)).round() as u8;
                row_bytes[px_off + 3] = 248;
            }
        });

    img
}

/// 문화재청 발굴조사보고서 도판 상단 표제란 텍스트 4종을 생성한다.
#[allow(dead_code)] // 테스트가 표제란 문자열을 검증한다.
pub fn build_chs_plate_title_block(
    title: &str,
    mode_label: &str,
    report: &ArtifactMeasurementReport,
) -> (String, String, String, String) {
    let left_cell = String::from("유적명 : 국가유산청 발굴조사 연구단");
    let center_title = format!("圖版 · 출토유물 정밀 실측도 — {}", title);
    let center_sub = format!("(Plate. Excavated Artifact Drawings · {})", mode_label);
    let right_cell = format!(
        "조사기관 : 동국유물실측 · STRATA (축척 {})",
        report.recommended_scale.label()
    );
    (left_cell, center_title, center_sub, right_cell)
}

/// 정밀 탁본 래스터 이미지를 도판 좌/우 반쪽 클리핑 및 사용자 드래그 ROI(`rubbing_roi_mm`) 영역에 맞춰 마스킹한다.
pub fn rgba_to_color_image_clipped_with_roi(
    src: &RgbaImage,
    region_rect_mm: [f64; 4],
    clip_half_sign: Option<i8>,
    roi_mm: Option<[f64; 4]>,
) -> egui::ColorImage {
    let w = src.width() as usize;
    let h = src.height() as usize;
    let mid_x = w / 2;
    let rx_min = region_rect_mm[0].min(region_rect_mm[2]);
    let rx_max = region_rect_mm[0].max(region_rect_mm[2]);
    let ry_min = region_rect_mm[1].min(region_rect_mm[3]);
    let ry_max = region_rect_mm[1].max(region_rect_mm[3]);
    let rw_mm = (rx_max - rx_min).max(1e-3);
    let rh_mm = (ry_max - ry_min).max(1e-3);

    let mut pixels = Vec::with_capacity(w * h);
    for y in 0..h {
        let py_mm = ry_max - (y as f64 / (h.max(1) as f64)) * rh_mm;
        for x in 0..w {
            let px_mm = rx_min + (x as f64 / (w.max(1) as f64)) * rw_mm;
            let clipped_half = match clip_half_sign {
                Some(s) if s < 0 => x > mid_x,
                Some(s) if s > 0 => x < mid_x,
                _ => false,
            };
            let clipped_roi = if let Some([ox0, oy0, ox1, oy1]) = roi_mm {
                px_mm < ox0 || px_mm > ox1 || py_mm < oy0 || py_mm > oy1
            } else {
                false
            };

            if clipped_half || clipped_roi {
                pixels.push(egui::Color32::TRANSPARENT);
            } else {
                let p = src.get_pixel(x as u32, y as u32);
                pixels.push(egui::Color32::from_rgba_unmultiplied(
                    p[0], p[1], p[2], p[3],
                ));
            }
        }
    }
    egui::ColorImage {
        size: [w, h],
        pixels,
    }
}
