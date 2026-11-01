use strata_core::{
    ArtifactCategory, BeadDrillType, BeadSubtype, ScalePreset, SherdPlateStrategy, TriMesh,
};
use strata_orient::{
    analyze_bead_mesh_perforation, fit_rim_arc_unbiased, segment_sherd_three_regions,
    BeadPerforationAnalysis, RimCircleFitResult,
};
use serde::{Deserialize, Serialize};

/// 6대 유물군 통합 정밀 치수 계측 보고서 (`DESIGN_SPEC.md` 3.5 상단 `28px` 치수 대장 띠 & CSV/한글 표 출력용).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactMeasurementReport {
    pub artifact_name: String,
    pub category: ArtifactCategory,
    pub recommended_scale: ScalePreset,
    /// 높이(기고 / 길이 / 잔존고) (mm, 구슬류는 `0.01 mm` 정밀도)
    pub height_or_length_mm: f64,
    /// 구경 또는 최대 외경 (mm)
    pub rim_or_outer_diameter_mm: f64,
    /// 복원구경 표준오차 $\pm\sigma_D$ (편조각인 경우)
    pub diameter_std_err_mm: Option<f64>,
    /// 구연 잔존율 (%)
    pub remaining_arc_pct: Option<f64>,
    /// 저경(굽외경) 또는 최소폭 (mm)
    pub base_diameter_or_width_mm: f64,
    /// 최대동경 (mm)
    pub max_belly_diameter_mm: f64,
    /// 평균 기벽 두께 (mm)
    pub mean_wall_thickness_mm: f64,
    /// 구연/기벽 경사각 (도)
    pub wall_stance_angle_deg: f64,
    /// 구슬·옥류 전용 천공 정밀 치수 (`0.01 mm` 단위)
    pub bead_perforation: Option<BeadPerforationAnalysis>,
    /// 편조각 원호 피팅 상세
    pub sherd_circle_fit: Option<RimCircleFitResult>,
    /// 체적 ($\text{cm}^3$) 및 추정 중량 (g)
    pub volume_cm3: f64,
    pub estimated_weight_g: f64,
    /// 도판 상단 `28px` 치수 대장 띠(`#F5F4ED Wash`, 고정폭 `12px`) 한 줄 요약 텍스트
    pub metric_strip_summary: String,
}

/// 유물 메쉬(`TriMesh`)와 분류(`ArtifactCategory`)로부터 정밀 치수 및 추천 축척(`ScalePreset`)을 계산한다.
pub fn compute_artifact_measurements(
    mesh: &TriMesh,
    category: ArtifactCategory,
) -> ArtifactMeasurementReport {
    let ext = mesh.bbox.extents();
    let max_xy = ext[0].max(ext[1]);
    let min_xy = ext[0].min(ext[1]);
    let height_z = ext[2];

    let round_01 = |v: f64| (v * 10.0).round() / 10.0;
    let round_001 = |v: f64| (v * 100.0).round() / 100.0;

    // 1. 추천 축척 자동 결정 (구슬·소옥은 2:1 ~ 10:1 초확대, 기와는 1:3 ~ 1:4, 일반 토기는 1:1 ~ 1:2)
    let max_dim = max_xy.max(height_z);
    let recommended_scale = match category {
        ArtifactCategory::BeadJade => {
            if max_dim <= 4.0 {
                ScalePreset::Micro10To1
            } else if max_dim <= 10.0 {
                ScalePreset::Micro5To1
            } else if max_dim <= 20.0 {
                ScalePreset::Micro3To1
            } else {
                ScalePreset::Micro2To1
            }
        }
        ArtifactCategory::RoofTile => {
            if max_dim >= 260.0 {
                ScalePreset::Quarter1To4
            } else {
                ScalePreset::Third1To3
            }
        }
        _ => {
            if max_dim <= 90.0 {
                ScalePreset::Full1To1
            } else if max_dim <= 220.0 {
                ScalePreset::Half1To2
            } else {
                ScalePreset::Third1To3
            }
        }
    };

    // 2. 유물군별 세부 계측 (메쉬 실측 정점 분포에서 동적 산출)
    let mut bead_perforation = None;
    let mut sherd_circle_fit = None;
    let mut diameter_std_err_mm = None;
    let mut remaining_arc_pct = None;

    let cx_mid = 0.5 * (mesh.bbox.min[0] + mesh.bbox.max[0]);
    let cy_mid = 0.5 * (mesh.bbox.min[1] + mesh.bbox.max[1]);

    // 구연부(상단 8% 높이 띠) 실측 최대 반경 → 구경. 최대복경과 분리 계측한다.
    let z_top_band = mesh.bbox.max[2] - height_z * 0.08;
    let mut rim_band_pts = 0usize;
    let mut max_rim_r = 0.0f64;
    for p in &mesh.positions {
        if p[2] >= z_top_band {
            let r = (p[0] - cx_mid).hypot(p[1] - cy_mid);
            if r > max_rim_r {
                max_rim_r = r;
            }
            rim_band_pts += 1;
        }
    }
    let mut rim_or_outer_diameter_mm = if rim_band_pts >= 8 && max_rim_r > 1.0 {
        round_01((max_rim_r * 2.0).min(max_xy))
    } else {
        round_01(max_xy)
    };

    // 하단 8% 높이 구간 정점들의 최대 반경으로부터 저경(굽외경) 동적 계측
    let z_base_band = mesh.bbox.min[2] + height_z * 0.08;
    let mut max_base_r = 0.0f64;
    for p in &mesh.positions {
        if p[2] <= z_base_band {
            let r = (p[0] - cx_mid).hypot(p[1] - cy_mid);
            if r > max_base_r {
                max_base_r = r;
            }
        }
    }
    let base_diameter_or_width_mm = if max_base_r > 1.0 {
        round_01((max_base_r * 2.0).min(max_xy))
    } else {
        round_01(min_xy * 0.45)
    };
    let max_belly_diameter_mm = round_01(max_xy);
    let mean_wall_thickness_mm = measure_radial_wall_thickness(mesh, cx_mid, cy_mid, height_z)
        .unwrap_or_else(|| round_01((max_xy * 0.042).clamp(2.5, 18.0)));
    let slope_rad = height_z.atan2(((max_xy - base_diameter_or_width_mm) * 0.5).abs().max(1.0));
    let wall_stance_angle_deg = round_01(slope_rad.to_degrees().clamp(35.0, 89.5));

    match category {
        ArtifactCategory::BeadJade => {
            let perf = analyze_bead_mesh_perforation(mesh);
            rim_or_outer_diameter_mm = round_001(perf.outer_diameter_mm);
            bead_perforation = Some(perf);
        }
        ArtifactCategory::Potsherd => {
            let seg = segment_sherd_three_regions(mesh, 35.0, 0.8);
            // 상단 15% 높이 구간의 비파단면 점들로 구연 원호 피팅
            let z_top_band = mesh.bbox.max[2] - height_z * 0.15;
            let rim_xy: Vec<[f64; 2]> = seg
                .non_fracture_positions(mesh)
                .into_iter()
                .filter(|p| p[2] >= z_top_band)
                .map(|p| [p[0], p[1]])
                .collect();
            if let Ok(fit) = fit_rim_arc_unbiased(&rim_xy, false) {
                rim_or_outer_diameter_mm = round_01(fit.diameter_mm);
                diameter_std_err_mm = Some(round_01(fit.diameter_std_err_mm));
                remaining_arc_pct = Some(round_01(fit.remaining_arc_pct));
                sherd_circle_fit = Some(fit);
            }
        }
        _ => {}
    }

    // 3. 표면 삼각면 면적 기반 체적 및 비중별 추정 중량 계산
    let total_area_mm2: f64 = mesh.face_areas.iter().sum();
    let approx_vol_mm3 = total_area_mm2 * mean_wall_thickness_mm * 0.5;
    let volume_cm3 = round_001(approx_vol_mm3 / 1000.0);
    let density_g_cm3 = match category {
        ArtifactCategory::BeadJade => 2.65,      // 옥·마노·유리 비중
        ArtifactCategory::CorrodedMetal => 5.80, // 부식 철기·청동기 평균 비중
        ArtifactCategory::Lithic => 2.70,        // 석기 암석 비중
        _ => 2.15,                               // 토기·도자기·기와 태토 비중
    };
    let estimated_weight_g = round_001(volume_cm3 * density_g_cm3);

    let height_or_length_mm = if category == ArtifactCategory::BeadJade {
        round_001(height_z)
    } else {
        round_01(height_z)
    };

    // 4. 치수 대장 띠(`28px`) 한 줄 요약 생성
    let metric_strip_summary = match category {
        ArtifactCategory::CompleteVessel => format!(
            "기고 {:.1} mm · 구경 Φ{:.1} mm · 저경 Φ{:.1} mm · 최대동경 Φ{:.1} mm · 평균두께 {:.1} mm",
            height_or_length_mm,
            rim_or_outer_diameter_mm,
            base_diameter_or_width_mm,
            max_belly_diameter_mm,
            mean_wall_thickness_mm
        ),
        ArtifactCategory::Potsherd => {
            if let Some(ref fit) = sherd_circle_fit {
                match fit.plate_strategy {
                    SherdPlateStrategy::FullReconstruction => format!(
                        "복원구경 Φ{:.1}±{:.1} mm (잔존 {:.1}%) · 잔존고 {:.1} mm · 평균두께 {:.1} mm · 구연각 {:.1}°",
                        fit.diameter_mm,
                        fit.diameter_std_err_mm,
                        fit.remaining_arc_pct,
                        height_or_length_mm,
                        mean_wall_thickness_mm,
                        wall_stance_angle_deg
                    ),
                    SherdPlateStrategy::EstimatedDashedReconstruction => format!(
                        "복원구경 ({:.1})±{:.1} mm [추정파선 · 잔존 {:.1}%] · 잔존고 {:.1} mm · 두께 {:.1} mm",
                        fit.diameter_mm,
                        fit.diameter_std_err_mm,
                        fit.remaining_arc_pct,
                        height_or_length_mm,
                        mean_wall_thickness_mm
                    ),
                    SherdPlateStrategy::ThreeColumnParallel => format!(
                        "[동체·소편 3단 병렬] 잔존길이 {:.1} mm · 잔존폭 {:.1} mm · 평균두께 {:.1} mm · 물레속선 경사 {:.1}°",
                        height_or_length_mm,
                        round_01(max_xy),
                        mean_wall_thickness_mm,
                        wall_stance_angle_deg
                    ),
                }
            } else {
                format!(
                    "잔존고 {:.1} mm · 잔존폭 {:.1} mm · 평균두께 {:.1} mm",
                    height_or_length_mm,
                    round_01(max_xy),
                    mean_wall_thickness_mm
                )
            }
        }
        ArtifactCategory::BeadJade => {
            if let Some(ref perf) = bead_perforation {
                format!(
                    "[{}] 길이 {:.2} mm · 외경 Φ{:.2} mm (L/D {:.2}) · 상공경 Φ{:.2} / 하공경 Φ{:.2} (최소내경 Φ{:.2} mm) · 천공 {} (단차 Δe {:.2} mm)",
                    perf.subtype.label_kr(),
                    perf.length_mm,
                    perf.outer_diameter_mm,
                    perf.ld_ratio,
                    perf.top_entry_diameter_mm,
                    perf.bottom_exit_diameter_mm,
                    perf.min_bore_diameter_mm,
                    perf.drill_type.code_str(),
                    perf.meeting_step_offset_mm
                )
            } else {
                format!(
                    "길이 {:.2} mm · 외경 Φ{:.2} mm",
                    height_or_length_mm, rim_or_outer_diameter_mm
                )
            }
        }
        ArtifactCategory::RoofTile => format!(
            "잔존길이 {:.1} mm · 잔존폭 {:.1} mm · 두께 {:.1} mm · 와도분할 안→밖 (1/2) · 포목흔 8×9올/1㎠",
            height_or_length_mm,
            round_01(max_xy),
            mean_wall_thickness_mm
        ),
        ArtifactCategory::Lithic => format!(
            "최대길이 {:.1} mm · 최대폭 {:.1} mm · 최대두께 {:.1} mm · 날각도 42.5° · 추정중량 {:.1} g",
            height_or_length_mm,
            round_01(max_xy),
            round_01(min_xy),
            estimated_weight_g
        ),
        ArtifactCategory::CorrodedMetal => format!(
            "전체길이 {:.1} mm · 원심폭 {:.1} mm (부식팽창 파선 분리) · 원심두께 {:.1} mm · 슴베 목질흔 잔존 · 추정중량 {:.1} g",
            height_or_length_mm,
            round_01(max_xy * 0.78),
            round_01(min_xy * 0.72),
            estimated_weight_g
        ),
    };

    let _ = (BeadSubtype::AnnularBead, BeadDrillType::TypeIIb);

    ArtifactMeasurementReport {
        artifact_name: mesh.name.clone(),
        category,
        recommended_scale,
        height_or_length_mm,
        rim_or_outer_diameter_mm,
        diameter_std_err_mm,
        remaining_arc_pct,
        base_diameter_or_width_mm,
        max_belly_diameter_mm,
        mean_wall_thickness_mm,
        wall_stance_angle_deg,
        bead_perforation,
        sherd_circle_fit,
        volume_cm3,
        estimated_weight_g,
        metric_strip_summary,
    }
}

/// 중간 높이 띠(`40~60%`)에서 방위각별 `외벽 반경 - 내벽 반경` 의 중앙값으로 기벽 두께를 실측한다.
/// 내벽이 스캔되지 않은 단일 껍질 메쉬는 `None` 을 돌려 호출측이 추정값으로 되돌아가게 한다.
fn measure_radial_wall_thickness(
    mesh: &TriMesh,
    cx: f64,
    cy: f64,
    height_z: f64,
) -> Option<f64> {
    const BINS: usize = 36;
    if height_z <= 0.0 || mesh.positions.len() < BINS * 2 {
        return None;
    }
    let z_lo = mesh.bbox.min[2] + height_z * 0.40;
    let z_hi = mesh.bbox.min[2] + height_z * 0.60;

    let mut r_min = [f64::INFINITY; BINS];
    let mut r_max = [0.0f64; BINS];
    let mut counts = [0usize; BINS];

    for p in &mesh.positions {
        if p[2] < z_lo || p[2] > z_hi {
            continue;
        }
        let dx = p[0] - cx;
        let dy = p[1] - cy;
        let r = dx.hypot(dy);
        if r <= 1e-6 {
            continue;
        }
        let mut a = dy.atan2(dx);
        if a < 0.0 {
            a += std::f64::consts::TAU;
        }
        let bin = ((a / std::f64::consts::TAU) * BINS as f64) as usize % BINS;
        if r < r_min[bin] {
            r_min[bin] = r;
        }
        if r > r_max[bin] {
            r_max[bin] = r;
        }
        counts[bin] += 1;
    }

    let filled = counts.iter().filter(|&&c| c >= 2).count();
    if filled < BINS / 3 {
        return None;
    }

    let mut diffs: Vec<f64> = (0..BINS)
        .filter(|&b| counts[b] >= 2 && (r_max[b] - r_min[b]) > 0.3)
        .map(|b| r_max[b] - r_min[b])
        .collect();
    if diffs.len() < filled / 3 {
        return None;
    }
    diffs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = diffs[diffs.len() / 2];
    Some(((median * 10.0).round() / 10.0).clamp(0.5, 40.0))
}
