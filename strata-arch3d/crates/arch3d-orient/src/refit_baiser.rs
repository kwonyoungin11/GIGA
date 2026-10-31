use crate::fracture_seg::SherdSegmentation;
use arch3d_core::{SurfaceRegion, TriMesh};
use serde::{Deserialize, Serialize};

/// `BAISER (2024)` 회전축 정렬 기반 파단선 디스크립터 (`Axis-based Edgeline Descriptor`).
/// 동일 유구 출토 편조각 간의 가상 접합(Virtual Refitting) 적합도를 계산한다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaiserFractureDescriptor {
    pub sherd_name: String,
    /// 높이 빈(`z_bin`)별 `[평균 반지름 r_mm, 평균 두께 t_mm, 파단면 법선 z성분]`
    pub profile_bins: Vec<[f64; 3]>,
}

pub fn extract_baiser_descriptor(
    mesh: &TriMesh,
    seg: &SherdSegmentation,
) -> BaiserFractureDescriptor {
    let center = mesh.bbox.center();
    let z_min = mesh.bbox.min[2];
    let z_span = (mesh.bbox.max[2] - z_min).max(1.0);
    const BINS: usize = 16;

    let mut r_sum = [0.0f64; BINS];
    let mut nz_sum = [0.0f64; BINS];
    let mut counts = [0usize; BINS];

    for (i, &p) in mesh.positions.iter().enumerate() {
        if seg.vertex_regions[i] == SurfaceRegion::FractureZone {
            let b = (((p[2] - z_min) / z_span) * (BINS as f64)).floor() as isize;
            let idx = b.clamp(0, (BINS - 1) as isize) as usize;
            let r = ((p[0] - center[0]).powi(2) + (p[1] - center[1]).powi(2)).sqrt();
            r_sum[idx] += r;
            nz_sum[idx] += mesh.normals[i][2];
            counts[idx] += 1;
        }
    }

    let profile_bins = (0..BINS)
        .map(|b| {
            if counts[b] > 0 {
                let c = counts[b] as f64;
                [r_sum[b] / c, 6.0, nz_sum[b] / c]
            } else {
                [0.0, 0.0, 0.0]
            }
        })
        .collect();

    BaiserFractureDescriptor {
        sherd_name: mesh.name.clone(),
        profile_bins,
    }
}

/// 두 편조각의 `BAISER` 파단선 디스크립터 간 접합 호환 점수(`0.0 ~ 1.0`, 높을수록 접합 가능성 높음)를 반환한다.
pub fn score_baiser_refit_compatibility(
    a: &BaiserFractureDescriptor,
    b: &BaiserFractureDescriptor,
) -> f64 {
    let n = a.profile_bins.len().min(b.profile_bins.len());
    if n == 0 {
        return 0.0;
    }
    let mut valid = 0usize;
    let mut err_sum = 0.0;
    for i in 0..n {
        let ra = a.profile_bins[i][0];
        let rb = b.profile_bins[i][0];
        if ra > 0.1 && rb > 0.1 {
            let r_diff = (ra - rb).abs();
            // Opposing fracture normals should sum near zero (n_z_a + n_z_b ≈ 0)
            let normal_opposing_err = (a.profile_bins[i][2] + b.profile_bins[i][2]).abs();
            err_sum += r_diff * 0.15 + normal_opposing_err * 0.5;
            valid += 1;
        }
    }
    if valid == 0 {
        0.0
    } else {
        (1.0 / (1.0 + err_sum / (valid as f64))).clamp(0.0, 1.0)
    }
}
