//! 문화재 발굴조사 보고서용 5단계 이산 미분기하(DDG) + 다중 스케일 구면 적분 불변량(MSII) +
//! 반경 제한 천공 가시성 차폐(PCV AO) + Radiance Scaling + XDoG + 한지 섬유/암석 입자 +
//! 블루 노이즈 점묘(Interleaved Gradient Noise Stippling) 디지털 탁본 파이프라인.
//!
//! 구성 단계:
//! 1. 공간 해시 그리드 기반 중복 정점 병합(`weld_vertices`, `eps = 1e-5`)으로 UV 심(Seam) 단절 제거
//! 2. Nelson Max (1999) 가중 정점 법선 + Meyer et al. (2003) Cotangent Laplace-Beltrami
//!    부호 있는 평균 곡률($H$) 및 각 결손 가우스 곡률($K$) + Mixed Voronoi Area 계산
//! 3. Murtala, 최예찬, 최강선 (2021) Modified 3D Frangi Vesselness Filter (음각/양각 선각 검출)
//! 4. 다중 스케일 구면 적분 불변량(MSII, `r = 0.8, 1.6, 3.2, 6.0 mm`) + 반경 제한 PCV AO (`r_max = 4.0 mm`)
//!    + Vergne et al. (2010) 화면 공간 법선 구배 Radiance Scaling 융합 접촉 확률 맵(`Contact Probability Map`)
//! 5. Winnemöller et al. (2012) XDoG(`σ_fine = 0.8`, `σ_coarse = 14.0`) + 5-Octave FBM Perlin 한지 섬유 노이즈
//!    + Cellular Worley 입자 노이즈 + Interleaved Gradient Noise(Blue-Noise 프로파일) 다계조 점묘 합성

use image::{Rgba, RgbaImage};
use rayon::prelude::*;
use std::collections::HashMap;
use strata_core::TriMesh;

/// 5단계 디지털 탁본 파이프라인 설정 (`RubbingPipelineConfig`)
#[derive(Clone, Debug, PartialEq)]
pub struct RubbingPipelineConfig {
    pub width: u32,
    pub height: u32,
    /// 곡률 강조 가중치 ($\alpha$, 기본 `6.5`)
    pub curvature_weight: f32,
    /// MSII 가중치 (`0.0 ..= 1.0`, 기본 `0.45`)
    pub msii_weight: f32,
    /// 반경 제한 PCV AO 감마 지수 ($\gamma$, 기본 `1.4`)
    pub ao_exponent: f32,
    /// 타솔(솜방망이) 접촉 임계값 (`pad_threshold`, 기본 `0.32`)
    pub pad_threshold: f32,
    /// XDoG 미세 가우시안 표준편차 ($\sigma_{\text{fine}}$, 기본 `0.8`)
    pub sigma_fine: f32,
    /// XDoG 거시 가우시안 표준편차 ($\sigma_{\text{coarse}}$, 기본 `14.0`)
    pub sigma_coarse: f32,
    /// XDoG 계단 기울기 ($\varphi$, 기본 `10.0`)
    pub xdog_phi: f32,
    /// XDoG 임계값 ($\epsilon$, 기본 `-0.015`)
    pub xdog_epsilon: f32,
    /// Worley 암석·태토 입자 강도 (`0.0 ..= 0.6`, 기본 `0.20`)
    pub grain_strength: f32,
    /// 5-Octave FBM Perlin 한지 섬유 강도 (`0.0 ..= 0.5`, 기본 `0.13`)
    pub paper_strength: f32,
    /// Blue-Noise 점묘(Stippling) 부드러움 계수 (`0.04 ..= 0.35`, 기본 `0.14`)
    pub stipple_softness: f32,
    /// 먹물 농도 배율 (`0.2 ..= 1.3`, 기본 `0.95`)
    pub ink_density_scale: f32,
    /// 음각/양각 극성 반전 여부
    pub invert_polarity: bool,
}

impl Default for RubbingPipelineConfig {
    fn default() -> Self {
        Self {
            width: 640,
            height: 640,
            curvature_weight: 6.5,
            msii_weight: 0.45,
            ao_exponent: 1.4,
            pad_threshold: 0.32,
            sigma_fine: 0.8,
            sigma_coarse: 14.0,
            xdog_phi: 10.0,
            xdog_epsilon: -0.015,
            grain_strength: 0.20,
            paper_strength: 0.13,
            stipple_softness: 0.14,
            ink_density_scale: 0.95,
            invert_polarity: false,
        }
    }
}

/// 1~2단계: 정점 병합된 위상 메쉬 및 이산 미분기하(DDG) 곡률 데이터
#[derive(Clone, Debug)]
pub struct TopologicalDdgMesh {
    pub positions: Vec<[f64; 3]>,
    pub faces: Vec<[u32; 3]>,
    pub normals: Vec<[f64; 3]>,
    pub mean_curvature: Vec<f64>,
    pub gaussian_curvature: Vec<f64>,
    pub frangi_vesselness: Vec<f64>,
}

#[inline]
fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
fn add3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[inline]
fn scale3(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

#[inline]
fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[inline]
fn norm_sq3(a: [f64; 3]) -> f64 {
    dot3(a, a)
}

#[inline]
fn norm3(a: [f64; 3]) -> f64 {
    norm_sq3(a).sqrt()
}

impl TopologicalDdgMesh {
    /// `TriMesh`로부터 중복 정점을 병합(`weld_vertices`)하고 Nelson Max 법선,
    /// Meyer et al. (2003) Cotangent Laplace-Beltrami 곡률($H, K$),
    /// Modified 3D Frangi Vesselness 필터를 일괄 계산한다.
    pub fn from_trimesh(mesh: &TriMesh) -> Self {
        let (positions, faces) = Self::weld_vertices(&mesh.positions, &mesh.triangles, 1e-5);
        let normals = Self::compute_nelson_max_normals(&positions, &faces);
        let (mut mean_curvature, gaussian_curvature) =
            Self::compute_cotangent_curvatures(&positions, &faces, &normals);

        // 메쉬 스케일(평균 변 길이)에 무관하게 곡률 수치 범위를 정규화하기 위해 평균 변 길이 산출
        let mean_edge_len = Self::estimate_mean_edge_length(&positions, &faces).max(1e-3);
        mean_curvature.par_iter_mut().for_each(|h| {
            *h = (*h * mean_edge_len).clamp(-4.0, 4.0);
        });

        let frangi_vesselness =
            Self::compute_modified_frangi_filter(&mean_curvature, &gaussian_curvature, 0.7, 0.15);

        Self {
            positions,
            faces,
            normals,
            mean_curvature,
            gaussian_curvature,
            frangi_vesselness,
        }
    }

    fn estimate_mean_edge_length(positions: &[[f64; 3]], faces: &[[u32; 3]]) -> f64 {
        if faces.is_empty() {
            return 1.0;
        }
        let step = (faces.len() / 2048).max(1);
        let mut sum = 0.0_f64;
        let mut count = 0usize;
        for f in faces.iter().step_by(step) {
            let p0 = positions[f[0] as usize];
            let p1 = positions[f[1] as usize];
            let p2 = positions[f[2] as usize];
            sum += norm3(sub3(p1, p0)) + norm3(sub3(p2, p1)) + norm3(sub3(p0, p2));
            count += 3;
        }
        if count > 0 {
            sum / (count as f64)
        } else {
            1.0
        }
    }

    /// 공간 해시 그리드를 이용한 중복 정점 병합 (Vertex Welding — UV 심 단절 제거)
    fn weld_vertices(
        positions: &[[f64; 3]],
        faces: &[[u32; 3]],
        eps: f64,
    ) -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
        let inv_eps = 1.0 / eps;
        let mut grid: HashMap<(i64, i64, i64), u32> = HashMap::with_capacity(positions.len());
        let mut welded_pos = Vec::with_capacity(positions.len());
        let mut remap = vec![0u32; positions.len()];

        for (idx, &p) in positions.iter().enumerate() {
            let key = (
                (p[0] * inv_eps).round() as i64,
                (p[1] * inv_eps).round() as i64,
                (p[2] * inv_eps).round() as i64,
            );
            let new_idx = *grid.entry(key).or_insert_with(|| {
                let id = welded_pos.len() as u32;
                welded_pos.push(p);
                id
            });
            remap[idx] = new_idx;
        }

        let welded_faces: Vec<[u32; 3]> = faces
            .iter()
            .filter_map(|&[a, b, c]| {
                let (ua, ub, uc) = (remap[a as usize], remap[b as usize], remap[c as usize]);
                if ua != ub && ub != uc && uc != ua {
                    Some([ua, ub, uc])
                } else {
                    None
                }
            })
            .collect();

        (welded_pos, welded_faces)
    }

    /// Nelson Max (1999) 가중 정점 법선 계산
    pub fn compute_nelson_max_normals(
        positions: &[[f64; 3]],
        faces: &[[u32; 3]],
    ) -> Vec<[f64; 3]> {
        let mut accum = vec![[0.0_f64; 3]; positions.len()];
        for &[i0, i1, i2] in faces {
            let (u0, u1, u2) = (i0 as usize, i1 as usize, i2 as usize);
            let p0 = positions[u0];
            let p1 = positions[u1];
            let p2 = positions[u2];

            let e01 = sub3(p1, p0);
            let e12 = sub3(p2, p1);
            let e20 = sub3(p0, p2);

            let fn_raw = cross3(e01, scale3(e20, -1.0));
            let l01 = norm_sq3(e01).max(1e-18);
            let l12 = norm_sq3(e12).max(1e-18);
            let l20 = norm_sq3(e20).max(1e-18);

            accum[u0] = add3(accum[u0], scale3(fn_raw, 1.0 / (l01 * l20)));
            accum[u1] = add3(accum[u1], scale3(fn_raw, 1.0 / (l01 * l12)));
            accum[u2] = add3(accum[u2], scale3(fn_raw, 1.0 / (l12 * l20)));
        }

        accum
            .into_par_iter()
            .map(|n| {
                let len = norm3(n);
                if len > 1e-12 {
                    scale3(n, 1.0 / len)
                } else {
                    [0.0, 0.0, 1.0]
                }
            })
            .collect()
    }

    /// Meyer et al. (2003) Cotangent Laplace-Beltrami 평균 곡률(H) 및 각 결손 가우스 곡률(K) 계산
    pub fn compute_cotangent_curvatures(
        positions: &[[f64; 3]],
        faces: &[[u32; 3]],
        normals: &[[f64; 3]],
    ) -> (Vec<f64>, Vec<f64>) {
        let n_verts = positions.len();
        let mut laplacian = vec![[0.0_f64; 3]; n_verts];
        let mut mixed_area = vec![0.0_f64; n_verts];
        let mut angle_sum = vec![0.0_f64; n_verts];

        for &[i0, i1, i2] in faces {
            let (u0, u1, u2) = (i0 as usize, i1 as usize, i2 as usize);
            let p0 = positions[u0];
            let p1 = positions[u1];
            let p2 = positions[u2];

            let e0 = sub3(p2, p1);
            let e1 = sub3(p0, p2);
            let e2 = sub3(p1, p0);

            let cross = cross3(e1, e2);
            let double_area = norm3(cross);
            if double_area < 1e-12 {
                continue;
            }
            let tri_area = 0.5 * double_area;

            let neg_e1 = scale3(e1, -1.0);
            let neg_e2 = scale3(e2, -1.0);
            let neg_e0 = scale3(e0, -1.0);

            let cot0 = (dot3(neg_e1, e2) / double_area).clamp(-1e5, 1e5);
            let cot1 = (dot3(neg_e2, e0) / double_area).clamp(-1e5, 1e5);
            let cot2 = (dot3(neg_e0, e1) / double_area).clamp(-1e5, 1e5);

            laplacian[u0] = add3(
                laplacian[u0],
                add3(scale3(sub3(p0, p1), cot2), scale3(sub3(p0, p2), cot1)),
            );
            laplacian[u1] = add3(
                laplacian[u1],
                add3(scale3(sub3(p1, p0), cot2), scale3(sub3(p1, p2), cot0)),
            );
            laplacian[u2] = add3(
                laplacian[u2],
                add3(scale3(sub3(p2, p0), cot1), scale3(sub3(p2, p1), cot0)),
            );

            angle_sum[u0] += double_area.atan2(dot3(neg_e1, e2));
            angle_sum[u1] += double_area.atan2(dot3(neg_e2, e0));
            angle_sum[u2] += double_area.atan2(dot3(neg_e0, e1));

            // Mixed Voronoi Area (둔각 삼각형 보정)
            if cot0 < 0.0 {
                mixed_area[u0] += 0.5 * tri_area;
                mixed_area[u1] += 0.25 * tri_area;
                mixed_area[u2] += 0.25 * tri_area;
            } else if cot1 < 0.0 {
                mixed_area[u0] += 0.25 * tri_area;
                mixed_area[u1] += 0.5 * tri_area;
                mixed_area[u2] += 0.25 * tri_area;
            } else if cot2 < 0.0 {
                mixed_area[u0] += 0.25 * tri_area;
                mixed_area[u1] += 0.25 * tri_area;
                mixed_area[u2] += 0.5 * tri_area;
            } else {
                mixed_area[u0] += 0.125 * (norm_sq3(e2) * cot1 + norm_sq3(e1) * cot2);
                mixed_area[u1] += 0.125 * (norm_sq3(e2) * cot0 + norm_sq3(e0) * cot2);
                mixed_area[u2] += 0.125 * (norm_sq3(e1) * cot0 + norm_sq3(e0) * cot1);
            }
        }

        let mut mean_curv = vec![0.0; n_verts];
        let mut gauss_curv = vec![0.0; n_verts];

        mean_curv
            .par_iter_mut()
            .zip(gauss_curv.par_iter_mut())
            .enumerate()
            .for_each(|(i, (h_out, k_out))| {
                let area = mixed_area[i];
                if area > 1e-12 {
                    let hn = scale3(laplacian[i], 1.0 / (2.0 * area));
                    // H > 0: 양각 능선(Convex), H < 0: 음각 골짜기(Concave)
                    *h_out = 0.5 * dot3(hn, normals[i]);
                    *k_out = (2.0 * std::f64::consts::PI - angle_sum[i]) / area;
                }
            });

        (mean_curv, gauss_curv)
    }

    /// Murtala, 최예찬, 최강선 (2021) Modified 3D Frangi Vesselness Filter
    pub fn compute_modified_frangi_filter(
        mean_curv: &[f64],
        gauss_curv: &[f64],
        alpha: f64,
        beta: f64,
    ) -> Vec<f64> {
        mean_curv
            .par_iter()
            .zip(gauss_curv.par_iter())
            .map(|(&h, &k)| {
                let disc = (h * h - k).max(0.0).sqrt();
                let k1 = h - disc;
                let k2 = h + disc;
                let (k_min, k_max) = if k1.abs() <= k2.abs() {
                    (k1, k2)
                } else {
                    (k2, k1)
                };
                if k_max.abs() < 1e-9 {
                    return 0.0;
                }
                let r_a = k_min.abs() / k_max.abs();
                let s = (k_min * k_min + k_max * k_max).sqrt();
                (-r_a * r_a / (2.0 * alpha * alpha)).exp()
                    * (1.0 - (-s * s / (2.0 * beta * beta)).exp())
            })
            .collect()
    }
}

/// 분리형(Separable) 1D×1D 가우시안 블러 (`imageproc::filter::gaussian_blur_f32` 동치 구현)
pub fn gaussian_blur_separable_f32(
    buf: &[f32],
    width: u32,
    height: u32,
    sigma: f32,
) -> Vec<f32> {
    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 || sigma <= 0.05 {
        return buf.to_vec();
    }
    let radius = (sigma * 3.0).ceil().clamp(1.0, 42.0) as isize;
    let two_sigma2 = 2.0 * sigma * sigma;
    let mut kernel = Vec::with_capacity((2 * radius + 1) as usize);
    let mut ksum = 0.0_f32;
    for r in -radius..=radius {
        let weight = (-(r as f32 * r as f32) / two_sigma2).exp();
        kernel.push(weight);
        ksum += weight;
    }
    for weight in &mut kernel {
        *weight /= ksum.max(1e-9);
    }

    // 수평 패스
    let mut tmp = vec![0.0_f32; w * h];
    tmp.par_chunks_mut(w)
        .enumerate()
        .for_each(|(y, out_row)| {
            let in_row = &buf[y * w..(y + 1) * w];
            for (x, out_val) in out_row.iter_mut().enumerate().take(w) {
                let mut acc = 0.0_f32;
                for (ki, &kw) in kernel.iter().enumerate() {
                    let dx = (ki as isize) - radius;
                    let sx = (x as isize + dx).clamp(0, (w - 1) as isize) as usize;
                    acc += in_row[sx] * kw;
                }
                *out_val = acc;
            }
        });

    // 수직 패스
    let mut out = vec![0.0_f32; w * h];
    out.par_chunks_mut(w)
        .enumerate()
        .for_each(|(y, out_row)| {
            for (x, out_val) in out_row.iter_mut().enumerate().take(w) {
                let mut acc = 0.0_f32;
                for (ki, &kw) in kernel.iter().enumerate() {
                    let dy = (ki as isize) - radius;
                    let sy = (y as isize + dy).clamp(0, (h - 1) as isize) as usize;
                    acc += tmp[sy * w + x] * kw;
                }
                *out_val = acc;
            }
        });

    out
}

/// 2D Perlin Gradient Noise (`noise::Perlin` 동치)
#[inline]
fn hash2_u32(ix: i32, iy: i32, seed: u32) -> u32 {
    let mut h = (ix as u32).wrapping_mul(374761393)
        ^ (iy as u32).wrapping_mul(668265263)
        ^ seed.wrapping_mul(2246822519);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    h ^ (h >> 16)
}

#[inline]
fn perlin2d(x: f32, y: f32, seed: u32) -> f32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let fx = x - (x0 as f32);
    let fy = y - (y0 as f32);
    let u = fx * fx * fx * (fx * (fx * 6.0 - 15.0) + 10.0);
    let v = fy * fy * fy * (fy * (fy * 6.0 - 15.0) + 10.0);

    let grad = |hx: i32, hy: i32, dx: f32, dy: f32| -> f32 {
        match hash2_u32(hx, hy, seed) & 7 {
            0 => dx + dy,
            1 => -dx + dy,
            2 => dx - dy,
            3 => -dx - dy,
            4 => dx,
            5 => -dx,
            6 => dy,
            _ => -dy,
        }
    };

    let n00 = grad(x0, y0, fx, fy);
    let n10 = grad(x0 + 1, y0, fx - 1.0, fy);
    let n01 = grad(x0, y0 + 1, fx, fy - 1.0);
    let n11 = grad(x0 + 1, y0 + 1, fx - 1.0, fy - 1.0);

    let nx0 = n00 + u * (n10 - n00);
    let nx1 = n01 + u * (n11 - n01);
    (nx0 + v * (nx1 - nx0)).clamp(-1.0, 1.0)
}

/// 5-Octave FBM Perlin 한지 섬유 노이즈 (`Fbm::<Perlin>::new(42).set_octaves(5).set_frequency(0.018).set_persistence(0.55)`)
#[inline]
pub fn fbm_perlin_paper_5oct(x_px: f32, y_px: f32) -> f32 {
    let mut freq = 0.018_f32;
    let mut amp = 1.0_f32;
    let mut sum = 0.0_f32;
    let mut norm = 0.0_f32;
    for oct in 0..5u32 {
        sum += amp * perlin2d(x_px * freq, y_px * freq, 42 + oct * 97);
        norm += amp;
        freq *= 2.0;
        amp *= 0.55;
    }
    ((sum / norm) * 0.5 + 0.5).clamp(0.0, 1.0)
}

/// Cellular Worley 암석·태토 입자 노이즈 (`Worley::new(1337).set_frequency(0.09)`)
#[inline]
pub fn worley_stone_grain(x_px: f32, y_px: f32) -> f32 {
    let sx = x_px * 0.09;
    let sy = y_px * 0.09;
    let ix = sx.floor() as i32;
    let iy = sy.floor() as i32;
    let mut min_dist2 = 4.0_f32;
    for dy in -1..=1 {
        for dx in -1..=1 {
            let cx = ix + dx;
            let cy = iy + dy;
            let h = hash2_u32(cx, cy, 1337);
            let jx = ((h & 0xFFFF) as f32) / 65535.0;
            let jy = ((h >> 16) as f32) / 65535.0;
            let px = (cx as f32) + jx - sx;
            let py = (cy as f32) + jy - sy;
            let d2 = px * px + py * py;
            if d2 < min_dist2 {
                min_dist2 = d2;
            }
        }
    }
    min_dist2.sqrt().clamp(0.0, 1.0)
}

/// MSII(다중 스케일 구면 적분) 반경과 PCV AO 상한 반경을 **유물 실제 크기에 비례**해 산출한다.
/// 문헌 기본값 `[0.8, 1.6, 3.2, 6.0]mm` / `4.0mm` 는 대각 100mm 급 유물 기준이며,
/// 3mm 소옥에서는 반경이 유물보다 커지고 400mm 기와에서는 너무 작아지는 문제를 막는다.
pub fn msii_radii_for_extent(extent_mm: f32) -> ([f32; 4], f32) {
    let scale = if extent_mm.is_finite() && extent_mm > 1e-3 {
        (extent_mm / 100.0).clamp(0.08, 4.0)
    } else {
        1.0
    };
    (
        [
            0.8 * scale,
            1.6 * scale,
            3.2 * scale,
            6.0 * scale,
        ],
        4.0 * scale,
    )
}

/// 4~5단계 통합: 정사투영 G-Buffer + MSII(`0.8, 1.6, 3.2, 6.0 mm`) + 반경 제한 PCV AO(`4.0 mm`)
/// + Radiance Scaling + XDoG + 한지 섬유/암석 입자 + Blue-Noise Stippling 기반 발굴보고서용 정밀 탁본 합성
pub fn render_ddg_msii_xdog_stipple_rubbing_rgba<F>(
    ddg_mesh: &TopologicalDdgMesh,
    project_point_and_normal: F,
    cfg: &RubbingPipelineConfig,
) -> RgbaImage
where
    F: Fn([f64; 3], [f64; 3]) -> ([f64; 3], [f64; 3]) + Sync,
{
    let w = cfg.width.max(32);
    let h = cfg.height.max(32);
    let n_verts = ddg_mesh.positions.len();
    if n_verts == 0 || ddg_mesh.faces.is_empty() {
        return RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 0]));
    }

    // 1. 정점 및 법선을 목표 정사투영 뷰로 변환
    let mut proj_pos = vec![[0.0_f32; 3]; n_verts];
    let mut proj_nrm = vec![[0.0_f32; 3]; n_verts];
    proj_pos
        .par_iter_mut()
        .zip(proj_nrm.par_iter_mut())
        .enumerate()
        .for_each(|(i, (pp, pn))| {
            let (q, qn) = project_point_and_normal(ddg_mesh.positions[i], ddg_mesh.normals[i]);
            *pp = [q[0] as f32, q[1] as f32, q[2] as f32];
            *pn = [qn[0] as f32, qn[1] as f32, qn[2] as f32];
        });

    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for p in &proj_pos {
        min_x = min_x.min(p[0]);
        max_x = max_x.max(p[0]);
        min_y = min_y.min(p[1]);
        max_y = max_y.max(p[1]);
    }
    let span_x = (max_x - min_x).max(1e-3);
    let span_y = (max_y - min_y).max(1e-3);
    let pad = 2.0_f32;
    let scale_x = ((w as f32) - 2.0 * pad) / span_x;
    let scale_y = ((h as f32) - 2.0 * pad) / span_y;
    let px_per_mm = 0.5 * (scale_x + scale_y);

    let to_screen = |x: f32, y: f32| -> (f32, f32) {
        (pad + (x - min_x) * scale_x, (h as f32) - pad - (y - min_y) * scale_y)
    };

    // 2. 정사투영 다중 채널 G-Buffer 래스터화 (Z, Normal, Cotangent H, Frangi Vesselness)
    let n_pix = (w * h) as usize;
    let mut z_buf = vec![f32::NEG_INFINITY; n_pix];
    let mut nx_buf = vec![0.0_f32; n_pix];
    let mut ny_buf = vec![0.0_f32; n_pix];
    let mut nz_buf = vec![1.0_f32; n_pix];
    let mut curv_buf = vec![0.0_f32; n_pix];
    let mut frangi_buf = vec![0.0_f32; n_pix];
    let mut mask_buf = vec![0u8; n_pix];

    for &[i0, i1, i2] in &ddg_mesh.faces {
        let (u0, u1, u2) = (i0 as usize, i1 as usize, i2 as usize);
        let p0 = proj_pos[u0];
        let p1 = proj_pos[u1];
        let p2 = proj_pos[u2];

        let (x0, y0) = to_screen(p0[0], p0[1]);
        let (x1, y1) = to_screen(p1[0], p1[1]);
        let (x2, y2) = to_screen(p2[0], p2[1]);

        let min_px = x0.min(x1).min(x2).floor().max(0.0) as u32;
        let max_px = x0.max(x1).max(x2).ceil().min((w - 1) as f32) as u32;
        let min_py = y0.min(y1).min(y2).floor().max(0.0) as u32;
        let max_py = y0.max(y1).max(y2).ceil().min((h - 1) as f32) as u32;

        let denom = (y1 - y2) * (x0 - x2) + (x2 - x1) * (y0 - y2);
        if denom.abs() < 1e-6 {
            continue;
        }
        let inv_denom = 1.0 / denom;

        let n0 = proj_nrm[u0];
        let n1 = proj_nrm[u1];
        let n2 = proj_nrm[u2];
        let h0 = ddg_mesh.mean_curvature[u0] as f32;
        let h1 = ddg_mesh.mean_curvature[u1] as f32;
        let h2 = ddg_mesh.mean_curvature[u2] as f32;
        let f0 = ddg_mesh.frangi_vesselness[u0] as f32;
        let f1 = ddg_mesh.frangi_vesselness[u1] as f32;
        let f2 = ddg_mesh.frangi_vesselness[u2] as f32;

        for py in min_py..=max_py {
            let fy = py as f32 + 0.5;
            let row_off = (py * w) as usize;
            for px in min_px..=max_px {
                let fx = px as f32 + 0.5;
                let w0 = ((y1 - y2) * (fx - x2) + (x2 - x1) * (fy - y2)) * inv_denom;
                let w1 = ((y2 - y0) * (fx - x2) + (x0 - x2) * (fy - y2)) * inv_denom;
                let w2 = 1.0 - w0 - w1;
                if w0 >= -1e-4 && w1 >= -1e-4 && w2 >= -1e-4 {
                    let z = w0 * p0[2] + w1 * p1[2] + w2 * p2[2];
                    let idx = row_off + (px as usize);
                    if z > z_buf[idx] {
                        z_buf[idx] = z;
                        mask_buf[idx] = 255;

                        let mut nx = w0 * n0[0] + w1 * n1[0] + w2 * n2[0];
                        let mut ny = w0 * n0[1] + w1 * n1[1] + w2 * n2[1];
                        let mut nz = w0 * n0[2] + w1 * n1[2] + w2 * n2[2];
                        if nz < 0.0 {
                            nx = -nx;
                            ny = -ny;
                            nz = -nz;
                        }
                        let nlen = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-6);
                        nx_buf[idx] = nx / nlen;
                        ny_buf[idx] = ny / nlen;
                        nz_buf[idx] = nz / nlen;
                        curv_buf[idx] = w0 * h0 + w1 * h1 + w2 * h2;
                        frangi_buf[idx] = w0 * f0 + w1 * f1 + w2 * f2;
                    }
                }
            }
        }
    }

    // 3. 다중 스케일 구면 적분 불변량(MSII, radii = [0.8, 1.6, 3.2, 6.0] mm) +
    //    반경 제한 PCV Ambient Occlusion (r_max = 4.0 mm) +
    //    Vergne et al. (2010) 화면 공간 Radiance Scaling 융합 -> Contact Probability Map 산출
    let dirs_12: [(f32, f32); 12] = [
        (1.0, 0.0),
        (0.8660254, 0.5),
        (0.5, 0.8660254),
        (0.0, 1.0),
        (-0.5, 0.8660254),
        (-0.8660254, 0.5),
        (-1.0, 0.0),
        (-0.8660254, -0.5),
        (-0.5, -0.8660254),
        (0.0, -1.0),
        (0.5, -0.8660254),
        (0.8660254, -0.5),
    ];
    let (msii_radii_mm, ao_max_r_mm) = msii_radii_for_extent(span_x.max(span_y));
    let w_isize = w as isize;
    let h_isize = h as isize;
    let w_usize = w as usize;

    let mut contact_buf = vec![0.0_f32; n_pix];
    contact_buf
        .par_chunks_mut(w_usize)
        .enumerate()
        .for_each(|(y, out_row)| {
            let py = y as isize;
            for x in 0..w_usize {
                let idx = y * w_usize + x;
                if mask_buf[idx] == 0 {
                    out_row[x] = 0.0;
                    continue;
                }
                let px = x as isize;
                let z0 = z_buf[idx];
                let nx0 = nx_buf[idx];
                let ny0 = ny_buf[idx];
                let nz0 = nz_buf[idx].clamp(0.05, 1.0);

                // (a) Vergne et al. (2010) Screen-Space Radiance Scaling 곡률 (shader.wgsl line 32)
                let xl = (px - 1).max(0) as usize;
                let xr = (px + 1).min(w_isize - 1) as usize;
                let yd = (py + 1).min(h_isize - 1) as usize;
                let yu = (py - 1).max(0) as usize;
                let n_left_x = if mask_buf[y * w_usize + xl] != 0 {
                    nx_buf[y * w_usize + xl]
                } else {
                    nx0
                };
                let n_right_x = if mask_buf[y * w_usize + xr] != 0 {
                    nx_buf[y * w_usize + xr]
                } else {
                    nx0
                };
                let n_down_y = if mask_buf[yd * w_usize + x] != 0 {
                    ny_buf[yd * w_usize + x]
                } else {
                    ny0
                };
                let n_up_y = if mask_buf[yu * w_usize + x] != 0 {
                    ny_buf[yu * w_usize + x]
                } else {
                    ny0
                };
                // 화면 공간에서 볼록 능선(Convex ridge)이 양수(> 0)가 되도록 부호 정렬
                let screen_curv = -0.5 * ((n_right_x - n_left_x) + (n_up_y - n_down_y));

                // (b) Mara et al. (2010) 다중 스케일 구면 적분 불변량 (msii_ao.rs lines 97-122)
                //     양각(Convex)일 때 이웃 중심이 접평면 아래에 위치하므로 signed_h = -diff.dot(n) / r > 0
                let mut msii_scale_sum = 0.0_f32;
                for &r_mm in &msii_radii_mm {
                    let r_px = (r_mm * px_per_mm).clamp(1.0, 28.0);
                    let mut signed_h_acc = 0.0_f32;
                    let mut valid_cnt = 0.0_f32;
                    for &(dx_u, dy_u) in &dirs_12 {
                        let sx = (px + (dx_u * r_px).round() as isize).clamp(0, w_isize - 1) as usize;
                        let sy = (py - (dy_u * r_px).round() as isize).clamp(0, h_isize - 1) as usize;
                        let s_idx = sy * w_usize + sx;
                        if mask_buf[s_idx] != 0 {
                            let dx_mm = dx_u * r_mm;
                            let dy_mm = dy_u * r_mm;
                            let dz_mm = z_buf[s_idx] - z0;
                            let dot_dn = dx_mm * nx0 + dy_mm * ny0 + dz_mm * nz0;
                            let signed_h = (-dot_dn / r_mm).clamp(-1.0, 1.0);
                            signed_h_acc += signed_h;
                            valid_cnt += 1.0;
                        }
                    }
                    if valid_cnt > 0.0 {
                        msii_scale_sum += (signed_h_acc / valid_cnt).clamp(-1.0, 1.0);
                    }
                }
                let m_val = msii_scale_sum * 0.25;

                // (c) CloudCompare qPCV 반경 제한 Ambient Occlusion (msii_ao.rs lines 58-67)
                let mut visible_rays = 0.0_f32;
                let mut total_rays = 0.0_f32;
                for &(dx_u, dy_u) in &dirs_12 {
                    for step_r in [1.0_f32, 2.2_f32, ao_max_r_mm] {
                        let r_px = (step_r * px_per_mm).clamp(1.0, 24.0);
                        let sx = (px + (dx_u * r_px).round() as isize).clamp(0, w_isize - 1) as usize;
                        let sy = (py - (dy_u * r_px).round() as isize).clamp(0, h_isize - 1) as usize;
                        let s_idx = sy * w_usize + sx;
                        total_rays += 1.0;
                        if mask_buf[s_idx] == 0 {
                            visible_rays += 1.0;
                        } else {
                            let dz = z_buf[s_idx] - z0;
                            // 접평면 위쪽으로 솟아오른 벽면에 의해 차폐되는지 판정
                            let tangent_dz = -(dx_u * step_r * nx0 + dy_u * step_r * ny0) / nz0;
                            let horizon_elev = dz - tangent_dz;
                            if horizon_elev <= step_r * 0.18 {
                                visible_rays += 1.0;
                            }
                        }
                    }
                }
                let ao_val = (visible_rays / total_rays.max(1.0)).clamp(0.0, 1.0);

                // (d) raster_rubbing.rs lines 132-142: 곡률 + MSII + Radiance Scaling + Frangi 융합 및 타솔 접촉 확률
                let curv = curv_buf[idx];
                let frangi = frangi_buf[idx] * curv.signum();
                let polarity = if cfg.invert_polarity { -1.0_f32 } else { 1.0_f32 };
                let combined = polarity
                    * ((1.0 - cfg.msii_weight) * curv
                        + cfg.msii_weight * m_val
                        + 0.35 * screen_curv
                        + 0.15 * frangi);
                let norm_curv = 0.5 * ((cfg.curvature_weight * combined).tanh() + 1.0);

                // Smoothstep(pad_threshold, 1.0, norm_curv)
                let denom_pad = (1.0 - cfg.pad_threshold).max(0.05);
                let t = ((norm_curv - cfg.pad_threshold) / denom_pad).clamp(0.0, 1.0);
                let smooth = t * t * (3.0 - 2.0 * t);

                out_row[x] = ao_val.powf(cfg.ao_exponent) * smooth * nz0.powf(0.28);
            }
        });

    // 4. 5단계: XDoG (Extended Difference of Gaussians) + 5-Octave FBM Perlin 한지 섬유 +
    //    Worley 암석 입자 + Interleaved Gradient Noise (Blue-Noise 프로파일) 점묘 합성
    //    (raster_rubbing.rs lines 156-224)
    let (blur_fine, blur_coarse) = rayon::join(
        || gaussian_blur_separable_f32(&contact_buf, w, h, cfg.sigma_fine),
        || gaussian_blur_separable_f32(&contact_buf, w, h, cfg.sigma_coarse),
    );

    let mut raw_rgba = vec![0u8; n_pix * 4];
    raw_rgba
        .par_chunks_mut(w_usize * 4)
        .enumerate()
        .for_each(|(y, row_bytes)| {
            let y_f = y as f32;
            for x in 0..w_usize {
                let idx = y * w_usize + x;
                if mask_buf[idx] == 0 {
                    continue;
                }
                let x_f = x as f32;
                let c_val = (contact_buf[idx] * cfg.ink_density_scale).clamp(0.0, 1.0);
                let g_fine = blur_fine[idx];
                let g_coarse = blur_coarse[idx];

                let dog = g_fine - g_coarse;
                let xdog = if dog >= cfg.xdog_epsilon {
                    1.0
                } else {
                    1.0 + (cfg.xdog_phi * (dog - cfg.xdog_epsilon)).tanh()
                };

                // 볼록한 양각 문양(인화문·타날문·능선, dog > 0)은 더욱 진하게 부각하고,
                // 오목한 음각 홈(c_val < 0.08)은 순백 한지 여백(#FAF9F5)으로 보존
                let dog_boost = (dog * 2.4).clamp(0.0, 0.55);
                let base_ink = ((c_val * xdog) + dog_boost).clamp(0.0, 1.0);

                let n_paper = fbm_perlin_paper_5oct(x_f, y_f);
                let n_stone = worley_stone_grain(x_f, y_f);

                if base_ink < 0.08 {
                    // 오목한 음각 홈 바닥: 먹물이 전혀 묻지 않은 깨끗한 한지 여백 (#FAF9F5)
                    let paper_grain = (n_paper * 4.0) as u8;
                    let off = x * 4;
                    row_bytes[off] = 250u8.saturating_sub(paper_grain);
                    row_bytes[off + 1] = 249u8.saturating_sub(paper_grain);
                    row_bytes[off + 2] = 245u8.saturating_sub(paper_grain);
                    row_bytes[off + 3] = 255;
                    continue;
                }

                let textured_ink = (base_ink
                    * (1.0 - cfg.grain_strength * n_stone - cfg.paper_strength * n_paper))
                    .clamp(0.0, 1.0);

                // Interleaved Gradient Noise (Blue-noise profile spatial hash, raster_rubbing.rs line 210)
                let ign = ((52.982_918_f32
                    * ((0.06711056 * x_f + 0.00583715 * y_f).fract()))
                .fract()
                    + 1.0)
                    .fract();

                let target_whiteness = 1.0 - textured_ink;
                let s = cfg.stipple_softness.max(1e-4);
                let stippled_luma = ((target_whiteness - ign + s) / (2.0 * s)).clamp(0.0, 1.0);

                // 전통 송연먹(#141413) ~ 순백 한지(#FAF9F5) 다계조 점묘 보간
                let r = (20.0 + stippled_luma * (250.0 - 20.0)).round() as u8;
                let g = (20.0 + stippled_luma * (249.0 - 20.0)).round() as u8;
                let b = (19.0 + stippled_luma * (245.0 - 19.0)).round() as u8;

                let off = x * 4;
                row_bytes[off] = r;
                row_bytes[off + 1] = g;
                row_bytes[off + 2] = b;
                row_bytes[off + 3] = 255;
            }
        });

    RgbaImage::from_raw(w, h, raw_rgba)
        .unwrap_or_else(|| RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 0])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_msii_radii_scale_with_artifact_size() {
        // 기준 크기(100mm)에서는 문헌 기본값 그대로.
        let (base, ao) = msii_radii_for_extent(100.0);
        assert_eq!(base, [0.8, 1.6, 3.2, 6.0]);
        assert!((ao - 4.0).abs() < 1e-6);

        // 3mm 소옥: 반경이 유물보다 커지면 안 된다.
        let (tiny, tiny_ao) = msii_radii_for_extent(3.0);
        assert!(
            tiny[3] < 1.0,
            "3mm 유물에 MSII 최대 반경 {:.2}mm 는 과대",
            tiny[3]
        );
        assert!(tiny_ao < 1.0);

        // 400mm 기와: 반경이 너무 작으면 문양이 사라진다.
        let (large, large_ao) = msii_radii_for_extent(400.0);
        assert!(large[3] > 12.0, "400mm 유물에 MSII 반경 {:.2}mm 는 과소", large[3]);
        assert!(large_ao > 8.0);

        // 크기에 대해 단조 증가해야 한다.
        let (mid, _) = msii_radii_for_extent(50.0);
        for k in 0..4 {
            assert!(mid[k] <= base[k] && base[k] <= large[k]);
        }
    }

    #[test]
    fn test_ddg_cotangent_curvatures_and_xdog_stipple_rubbing_pipeline() {
        // 양각 인화문 능선과 음각 홈이 있는 반구형 토기 메쉬 생성
        let n_rings = 24usize;
        let n_segs = 48usize;
        let mut positions = Vec::with_capacity((n_rings + 1) * n_segs);
        let mut triangles = Vec::with_capacity(n_rings * n_segs * 2);

        for r in 0..=n_rings {
            let v = r as f64 / n_rings as f64;
            let z = v * 60.0;
            let base_rad = 26.0 + 34.0 * (v * std::f64::consts::FRAC_PI_2).sin();
            for s in 0..n_segs {
                let u = s as f64 / n_segs as f64;
                let theta = u * std::f64::consts::TAU;
                // 뚜렷한 양각 능선 및 음각 침선 부여
                let groove = 1.6 * (v * 14.0 * std::f64::consts::PI).cos()
                    + 1.1 * (u * 18.0 * std::f64::consts::TAU).sin();
                let rad = base_rad + groove;
                positions.push([rad * theta.cos(), rad * theta.sin(), z]);
            }
        }
        for r in 0..n_rings {
            for s in 0..n_segs {
                let next_s = (s + 1) % n_segs;
                let i00 = (r * n_segs + s) as u32;
                let i01 = (r * n_segs + next_s) as u32;
                let i10 = ((r + 1) * n_segs + s) as u32;
                let i11 = ((r + 1) * n_segs + next_s) as u32;
                triangles.push([i00, i01, i11]);
                triangles.push([i00, i11, i10]);
            }
        }
        let mesh = TriMesh::from_positions_and_triangles("ddg_test_vessel", positions, triangles)
            .expect("valid test vessel mesh");
        let ddg = TopologicalDdgMesh::from_trimesh(&mesh);

        assert!(!ddg.mean_curvature.is_empty());
        let pos_h = ddg.mean_curvature.iter().filter(|&&h| h > 0.02).count();
        let neg_h = ddg.mean_curvature.iter().filter(|&&h| h < -0.02).count();
        assert!(pos_h > 50 && neg_h > 50, "must detect both convex ridges (H>0) and concave grooves (H<0)");

        let cfg = RubbingPipelineConfig {
            width: 320,
            height: 320,
            ..Default::default()
        };
        let img = render_ddg_msii_xdog_stipple_rubbing_rgba(
            &ddg,
            |p, n| ([p[0], p[2], -p[1]], [n[0], n[2], -n[1]]),
            &cfg,
        );
        let mut dark_ink = 0usize;
        let mut white_paper = 0usize;
        for px in img.pixels() {
            if px[3] > 200 {
                if px[0] < 95 {
                    dark_ink += 1;
                } else if px[0] >= 235 {
                    white_paper += 1;
                }
            }
        }
        assert!(dark_ink > 400, "must deposit dark charcoal ink on ridges: dark_ink={dark_ink}");
        assert!(white_paper > 400, "must preserve uninked Hanji white paper in grooves: white_paper={white_paper}");
    }
}
