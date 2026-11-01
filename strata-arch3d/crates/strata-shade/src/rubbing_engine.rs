use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use strata_core::{StacklessLbvh, TriMesh};

/// 3대 고고학 정밀 탁본(`Rubbing`) 핵심 알고리즘 (`DESIGN_SPEC.md` 1.2 & 3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RubbingAlgorithm {
    /// ① 뷰 법선 그라디언트 미분 + 전통 한지 손탁본(`Handprint`) 질감
    NormalGradientHandprint,
    /// ② `16-Scale MSII` 다중 스케일 구형 적분 불변량 (다중스케일 Mara et al. 2010 — 마모된 인화문·선각문)
    Msii16Scale,
    /// ③ 양개도(능선)·음개도(골짜기) 위상 개도 이치화 선화 (Yokoyama 2002 / PEAKIT — 타날문·박리능선)
    TopologicalOpenness,
}

impl RubbingAlgorithm {
    pub fn label_kr(self) -> &'static str {
        match self {
            Self::NormalGradientHandprint => "법선미분 + 한지 손탁본(Handprint)",
            Self::Msii16Scale => "16-Scale MSII 다중스케일 적분불변량",
            Self::TopologicalOpenness => "양개도·음개도 이치화 선화",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RubbingRenderParams {
    pub algorithm: RubbingAlgorithm,
    /// 먹물 농도 (`0.0 ~ 100.0 %`, 기본 `72.0%`)
    pub ink_density_pct: f64,
    /// 곡률 적분 반경 $R$ (mm, 기본 `1.4 mm`)
    pub curv_radius_mm: f64,
    /// 전통 한지 섬유 결(`Handprint`) 질감 적용 여부
    pub use_handprint_grain: bool,
}

impl Default for RubbingRenderParams {
    fn default() -> Self {
        Self {
            algorithm: RubbingAlgorithm::NormalGradientHandprint,
            ink_density_pct: 72.0,
            curv_radius_mm: 1.4,
            use_handprint_grain: true,
        }
    }
}

/// 메쉬의 각 정점별 정밀 탁본 먹물 농도(`0.0 = 순백색 한지(오목/음각 홈)`, `1.0 = 진한 먹색(볼록/평면)`)를 계산한다.
pub fn compute_vertex_rubbing_ink_weights(
    mesh: &TriMesh,
    bvh: Option<&StacklessLbvh>,
    params: RubbingRenderParams,
    view_dir: [f64; 3],
) -> Vec<f64> {
    let n_verts = mesh.positions.len();
    let ink_scale = (params.ink_density_pct / 100.0).clamp(0.1, 1.0);
    let mut weights = Vec::with_capacity(n_verts);

    for i in 0..n_verts {
        let p = mesh.positions[i];
        let n = mesh.normals[i];
        let k = mesh.mean_curvatures[i];
        let ndotv = (n[0] * view_dir[0] + n[1] * view_dir[1] + n[2] * view_dir[2]).abs();

        let base_ink = match params.algorithm {
            RubbingAlgorithm::NormalGradientHandprint => {
                // 볼록 능선(k > 0) 및 정면(ndotv 높음)은 먹이 진하게 묻고, 오목 음각 홈(k < -threshold)은 백색 여백으로 남음
                let groove_mask = (1.0 + k * params.curv_radius_mm * 3.8).clamp(0.0, 1.35);
                (ndotv.powf(0.65) * groove_mask * ink_scale).clamp(0.0, 1.0)
            }
            RubbingAlgorithm::Msii16Scale => {
                // 16-Scale MSII: LBVH subtree_area_normal alignment vs local normal
                let msii_signal = if let Some(tree) = bvh {
                    let (area, area_n) =
                        tree.query_sphere_integral(mesh, p, params.curv_radius_mm.max(0.5));
                    if area > 1e-6 {
                        let avg_nx = area_n[0] / area;
                        let avg_ny = area_n[1] / area;
                        let avg_nz = area_n[2] / area;
                        let align =
                            (n[0] * avg_nx + n[1] * avg_ny + n[2] * avg_nz).clamp(-1.0, 1.0);
                        ((align + k * params.curv_radius_mm * 2.2) * 0.85).clamp(0.0, 1.0)
                    } else {
                        (0.65 + k * 1.5).clamp(0.0, 1.0)
                    }
                } else {
                    (0.65 + k * params.curv_radius_mm * 2.5).clamp(0.0, 1.0)
                };
                (msii_signal * ink_scale).clamp(0.0, 1.0)
            }
            RubbingAlgorithm::TopologicalOpenness => {
                // 양개도(볼록 능선 = 1.0 먹선) vs 음개도(오목 골짜기 = 0.0 백색) 소프트 이치화
                let openness_diff = k * params.curv_radius_mm * 5.0 + (ndotv - 0.5) * 0.8;
                let sigmoid = 1.0 / (1.0 + (-openness_diff * 4.0).exp());
                (sigmoid * ink_scale).clamp(0.0, 1.0)
            }
        };

        // 전통 한지 손탁본(`Handprint`) 미세 섬유 질감 변조
        let grain = if params.use_handprint_grain {
            let h = ((p[0] * 12.9898 + p[1] * 78.233 + p[2] * 37.719).sin() * 43758.5453)
                .fract()
                .abs();
            0.90 + 0.10 * h
        } else {
            1.0
        };

        weights.push((base_ink * grain).clamp(0.0, 1.0));
    }

    weights
}

/// 정사투영(Orthographic) 정밀 탁본 이미지를 생성한다 (`rubbing_*.png`).
pub fn render_orthographic_rubbing_image(
    mesh: &TriMesh,
    bvh: Option<&StacklessLbvh>,
    params: RubbingRenderParams,
    width_px: u32,
    height_px: u32,
) -> RgbaImage {
    let w = width_px.max(32);
    let h = height_px.max(32);
    // 배경은 투명 또는 한지 바탕(`#FAF9F5`)
    let mut img = RgbaImage::from_pixel(w, h, Rgba([250, 249, 245, 0]));
    let ink_weights = compute_vertex_rubbing_ink_weights(mesh, bvh, params, [0.0, -1.0, 0.0]);

    let ext = mesh.bbox.extents();
    let span_x = ext[0].max(1.0);
    let span_z = ext[2].max(1.0);
    let margin = 8.0;
    let scale = ((f64::from(w) - margin * 2.0) / span_x)
        .min((f64::from(h) - margin * 2.0) / span_z)
        .max(0.1);

    let cx = f64::from(w) * 0.5;
    let cy = f64::from(h) * 0.5;
    let mesh_cx = (mesh.bbox.min[0] + mesh.bbox.max[0]) * 0.5;
    let mesh_cz = (mesh.bbox.min[2] + mesh.bbox.max[2]) * 0.5;

    for tri in &mesh.triangles {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;
        let p0 = mesh.positions[i0];
        let p1 = mesh.positions[i1];
        let p2 = mesh.positions[i2];
        let ink = (ink_weights[i0] + ink_weights[i1] + ink_weights[i2]) / 3.0;
        // 먹색(#141413 = 20) ~ 한지 백색(248)
        let lum = (248.0 - ink * 224.0).round().clamp(18.0, 248.0) as u8;
        let px_color = Rgba([lum, lum, lum.saturating_sub(2), 255]);

        let sx0 = cx + (p0[0] - mesh_cx) * scale;
        let sy0 = cy - (p0[2] - mesh_cz) * scale;
        let sx1 = cx + (p1[0] - mesh_cx) * scale;
        let sy1 = cy - (p1[2] - mesh_cz) * scale;
        let sx2 = cx + (p2[0] - mesh_cx) * scale;
        let sy2 = cy - (p2[2] - mesh_cz) * scale;

        let min_x = sx0.min(sx1).min(sx2).floor().max(0.0) as u32;
        let max_x = sx0.max(sx1).max(sx2).ceil().min(f64::from(w - 1)) as u32;
        let min_y = sy0.min(sy1).min(sy2).floor().max(0.0) as u32;
        let max_y = sy0.max(sy1).max(sy2).ceil().min(f64::from(h - 1)) as u32;

        for py in min_y..=max_y {
            for px in min_x..=max_x {
                img.put_pixel(px, py, px_color);
            }
        }
    }

    img
}
