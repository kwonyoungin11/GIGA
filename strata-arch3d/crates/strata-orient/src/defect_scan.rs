use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use strata_core::TriMesh;

/// C++ `ArtifactDoc.cpp:2721` 결함(`112°` 하드코딩 절단)을 해결한
/// **`0° ~ 360°` 전 구간(2° 간격 180빈) 구연·바닥 접지 오차 차트** (`V3` 검증 대상).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RimGapChart360 {
    /// 0°, 2°, 4°, ..., 358° 각도 (`len == 180`)
    pub angles_deg: Vec<f64>,
    /// 각 각도 섹터에서의 기준면(바닥 또는 구연 수평면) 간격 오차 (mm) (`None`은 결손/파편 미존재 구간)
    pub gap_mm_per_bin: Vec<Option<f64>>,
    /// 유효하게 잔존하는 각도 빈 수 (`0..=180`)
    pub occupied_bins: usize,
    /// 전체 평균 접지 오차 (mm)
    pub mean_gap_mm: f64,
    /// 전체 최대 접지 오차 (mm)
    pub max_gap_mm: f64,
}

/// 메쉬의 `0° ~ 360°` 전 구간(2° 간격 180개 섹터)을 스캔하여
/// 기준면(구연 상단 또는 바닥 하단)과의 접지 오차(`gap_mm`)를 계산한다.
pub fn compute_rim_gap_chart_360(mesh: &TriMesh, measure_top_rim: bool) -> RimGapChart360 {
    const NUM_BINS: usize = 180;
    let center = mesh.bbox.center();
    let z_min = mesh.bbox.min[2];
    let z_max = mesh.bbox.max[2];

    let mut sector_extreme_z = vec![None::<f64>; NUM_BINS];
    let mut update_bin = |bin: usize, z: f64| match &mut sector_extreme_z[bin] {
        Some(cur) => {
            if measure_top_rim {
                if z > *cur {
                    *cur = z;
                }
            } else if z < *cur {
                *cur = z;
            }
        }
        None => sector_extreme_z[bin] = Some(z),
    };

    for tri in &mesh.triangles {
        let idxs = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
        for k in 0..3 {
            let p0 = mesh.positions[idxs[k]];
            let p1 = mesh.positions[idxs[(k + 1) % 3]];
            // Sample 8 points along each triangle edge so wide triangles cover all 2-degree bins smoothly
            for s in 0..=8 {
                let t = (s as f64) / 8.0;
                let px = p0[0] + t * (p1[0] - p0[0]) - center[0];
                let py = p0[1] + t * (p1[1] - p0[1]) - center[1];
                let pz = p0[2] + t * (p1[2] - p0[2]);
                if px * px + py * py < 1.0 {
                    continue;
                }
                let deg = py.atan2(px).to_degrees().rem_euclid(360.0);
                let bin = ((deg / 2.0).floor() as usize).min(NUM_BINS - 1);
                update_bin(bin, pz);
            }
        }
    }

    let angles_deg: Vec<f64> = (0..NUM_BINS).map(|i| (i * 2) as f64).collect();
    let mut gap_mm_per_bin = Vec::with_capacity(NUM_BINS);
    let mut occupied_bins = 0usize;
    let mut sum_gap = 0.0;
    let mut max_gap_mm = 0.0f64;

    for ext_z in sector_extreme_z {
        if let Some(z) = ext_z {
            let gap = if measure_top_rim {
                (z_max - z).max(0.0)
            } else {
                (z - z_min).max(0.0)
            };
            occupied_bins += 1;
            sum_gap += gap;
            if gap > max_gap_mm {
                max_gap_mm = gap;
            }
            gap_mm_per_bin.push(Some(gap));
        } else {
            gap_mm_per_bin.push(None);
        }
    }

    let mean_gap_mm = if occupied_bins > 0 {
        sum_gap / (occupied_bins as f64)
    } else {
        0.0
    };

    RimGapChart360 {
        angles_deg,
        gap_mm_per_bin,
        occupied_bins,
        mean_gap_mm,
        max_gap_mm,
    }
}

/// 단면 후보 각도(`angle_deg`)별 결함 점수 및 추천 순위 (`V3`: C++ `angleDefectScore` 버그 완치).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionAngleScore {
    pub angle_deg: f64,
    /// 총 결함 점수 (낮을수록 구연·바닥 도달 및 기벽 연속성이 우수한 최적 단면 각도)
    pub total_defect_score: f64,
    pub rim_reach_defect_mm: f64,
    pub base_reach_defect_mm: f64,
    pub vertical_coverage_ratio: f64,
}

/// `rayon` 병렬 스트리밍으로 후보 각도(`0°..360°`)들의 실제 단면 슬라이스 품질을 평가하고,
/// 결함이 가장 적은 순서로 정렬된 추천 단면 각도 목록을 반환한다.
pub fn evaluate_section_angles_parallel(mesh: &TriMesh, step_deg: usize) -> Vec<SectionAngleScore> {
    let step = step_deg.clamp(1, 90);
    let candidate_angles: Vec<f64> = (0..360).step_by(step).map(|a| a as f64).collect();
    let center = mesh.bbox.center();
    let z_min = mesh.bbox.min[2];
    let z_max = mesh.bbox.max[2];
    let total_h = (z_max - z_min).max(1.0);
    let band_half_width = (mesh.bbox.extents()[0].max(mesh.bbox.extents()[1]) * 0.025).max(1.2);

    let mut scores: Vec<SectionAngleScore> = candidate_angles
        .par_iter()
        .map(|&angle_deg| {
            let rad = angle_deg.to_radians();
            let dir_x = rad.cos();
            let dir_y = rad.sin();
            let ortho_x = -dir_y;
            let ortho_y = dir_x;

            let mut max_z_in_slice = f64::NEG_INFINITY;
            let mut min_z_in_slice = f64::INFINITY;
            let mut z_bins = [false; 20];

            for &p in &mesh.positions {
                let dx = p[0] - center[0];
                let dy = p[1] - center[1];
                let radial_proj = dx * dir_x + dy * dir_y;
                let plane_dist = (dx * ortho_x + dy * ortho_y).abs();
                if radial_proj >= 0.0 && plane_dist <= band_half_width {
                    if p[2] > max_z_in_slice {
                        max_z_in_slice = p[2];
                    }
                    if p[2] < min_z_in_slice {
                        min_z_in_slice = p[2];
                    }
                    let bin = (((p[2] - z_min) / total_h) * 20.0).floor() as isize;
                    let idx = bin.clamp(0, 19) as usize;
                    z_bins[idx] = true;
                }
            }

            let filled_bins = z_bins.iter().filter(|&&b| b).count();
            let vertical_coverage_ratio = (filled_bins as f64) / 20.0;
            let rim_reach_defect_mm = if max_z_in_slice.is_finite() {
                (z_max - max_z_in_slice).max(0.0)
            } else {
                total_h
            };
            let base_reach_defect_mm = if min_z_in_slice.is_finite() {
                (min_z_in_slice - z_min).max(0.0)
            } else {
                total_h
            };
            let void_penalty = (1.0 - vertical_coverage_ratio) * total_h * 0.8;
            let total_defect_score =
                rim_reach_defect_mm * 1.5 + base_reach_defect_mm * 1.2 + void_penalty;

            SectionAngleScore {
                angle_deg,
                total_defect_score,
                rim_reach_defect_mm,
                base_reach_defect_mm,
                vertical_coverage_ratio,
            }
        })
        .collect();

    scores.sort_by(|a, b| {
        a.total_defect_score
            .partial_cmp(&b.total_defect_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scores
}
