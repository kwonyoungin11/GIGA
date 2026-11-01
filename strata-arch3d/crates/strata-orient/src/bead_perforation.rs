use strata_core::{BeadDrillType, BeadFabricationKind, BeadSubtype, TriMesh};
use serde::{Deserialize, Serialize};

/// 구슬·옥류(`BeadJade`) 관통공(천공) 기하 분석 및 한국 고고학 4대 천공 단면 유형 판별 결과 (`V5`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BeadPerforationAnalysis {
    pub subtype: BeadSubtype,
    pub fabrication: BeadFabricationKind,
    /// 한국 고고학 4대 천공 단면 유형 (`Ⅰa`, `Ⅰb`, `Ⅰb'`, `Ⅱb`)
    pub drill_type: BeadDrillType,
    /// 길이 $L$ (mm, `0.01 mm` 정밀도)
    pub length_mm: f64,
    /// 최대 외경 $D$ (mm, `0.01 mm` 정밀도)
    pub outer_diameter_mm: f64,
    /// 길이/외경 비율 ($L/D$)
    pub ld_ratio: f64,
    /// 상단 입구 공경 $d_{\text{top}}$ (mm)
    pub top_entry_diameter_mm: f64,
    /// 하단 출구 공경 $d_{\text{bot}}$ (mm, 미관통 맹공 `Ⅰb'`인 경우 `0.0`)
    pub bottom_exit_diameter_mm: f64,
    /// 내부 최소 관통경 $d_{\text{min}}$ (mm)
    pub min_bore_diameter_mm: f64,
    /// 양방향 맞조르기(`Ⅱb`) 시 중앙 교차 단차 $\Delta e$ (mm)
    pub meeting_step_offset_mm: f64,
    /// 양방향 천공 시 상·하 천공축 굴절 각도 (도)
    pub axis_kink_deg: f64,
    /// 일방향 원추형(`Ⅰb`) 출구 가장자리 원추상 박리흔(Breakout spalling scar) 유무
    pub has_exit_spalling_scar: bool,
    /// 높이별($z \in [0, L]$) 천공 반지름 프로파일 `[(z_mm, r_left_mm, r_right_mm)]`
    pub hole_profile_zr: Vec<[f64; 3]>,
}

/// 높이별 내공 반지름 샘플 `[(z_mm, r_hole_mm, axis_offset_x_mm)]` 및 외경·길이로부터
/// 구슬·옥류 세부 기종($L/D$)과 4대 천공 단면 유형(`Ⅰa`, `Ⅰb`, `Ⅰb'`, `Ⅱb`)을 자동 판별한다.
pub fn classify_bead_and_perforation(
    length_mm: f64,
    outer_diameter_mm: f64,
    is_c_curved: bool,
    facet_count: usize,
    z_bore_samples: &[[f64; 3]], // [z_mm, bore_radius_mm, center_offset_x_mm]
) -> BeadPerforationAnalysis {
    let subtype =
        BeadSubtype::classify_by_ratio(length_mm, outer_diameter_mm, is_c_curved, facet_count);
    let ld_ratio = if outer_diameter_mm.abs() > 1e-6 {
        length_mm / outer_diameter_mm
    } else {
        1.0
    };

    if z_bore_samples.is_empty() {
        return BeadPerforationAnalysis {
            subtype,
            fabrication: BeadFabricationKind::LapidaryCarved,
            drill_type: BeadDrillType::TypeIa,
            length_mm,
            outer_diameter_mm,
            ld_ratio,
            top_entry_diameter_mm: 0.0,
            bottom_exit_diameter_mm: 0.0,
            min_bore_diameter_mm: 0.0,
            meeting_step_offset_mm: 0.0,
            axis_kink_deg: 0.0,
            has_exit_spalling_scar: false,
            hole_profile_zr: Vec::new(),
        };
    }

    let n = z_bore_samples.len();
    let r_bot = z_bore_samples[0][1].max(0.0);
    let r_top = z_bore_samples[n - 1][1].max(0.0);

    // Find minimum internal radius and its index (waist location)
    let mut min_r = f64::INFINITY;
    let mut min_idx = 0usize;
    for (idx, s) in z_bore_samples.iter().enumerate() {
        if s[1] < min_r {
            min_r = s[1];
            min_idx = idx;
        }
    }
    min_r = min_r.max(0.0);

    // Check if blind hole (Ⅰb'): bottom radius is zero or penetration stops before z=0
    let is_blind = r_bot <= 0.05 || z_bore_samples[0][0] > length_mm * 0.15;

    // Check if interior waist exists (Ⅱb Biconical Hourglass):
    // Waist must be in the central [25%..75%] height zone, AND both top and bottom entries
    // must be substantially wider than the waist (whereas Ib exit spalling is near z < 20% and much smaller than top entry)
    let min_z = z_bore_samples[min_idx][0];
    let waist_in_central_zone =
        min_idx > 0 && min_idx + 1 < n && min_z >= length_mm * 0.22 && min_z <= length_mm * 0.78;
    let end_min_r = r_top.min(r_bot);
    let end_max_r = r_top.max(r_bot).max(0.1);
    let both_ends_wide = end_min_r >= end_max_r * 0.72;
    let is_biconical_hourglass = !is_blind
        && waist_in_central_zone
        && both_ends_wide
        && (end_min_r - min_r) > end_min_r * 0.14;

    // Check central meeting step offset Δe around the waist
    let mut max_step_offset = 0.0f64;
    for w in z_bore_samples.windows(2) {
        let step = (w[1][2] - w[0][2]).abs();
        if step > max_step_offset {
            max_step_offset = step;
        }
    }

    // Check cylindrical (Ⅰa) vs one-way conical (Ⅰb)
    let max_end_r = r_top.max(r_bot).max(0.1);
    let end_taper_ratio = (r_top - r_bot).abs() / max_end_r;

    let drill_type = if is_blind {
        BeadDrillType::TypeIbBlind
    } else if is_biconical_hourglass {
        BeadDrillType::TypeIIb
    } else if end_taper_ratio < 0.12 {
        BeadDrillType::TypeIa
    } else {
        BeadDrillType::TypeIb
    };

    // For Ⅰb one-way conical, check if exit end has a sudden flare (breakout spalling scar)
    let has_exit_spalling_scar = if drill_type == BeadDrillType::TypeIb && n >= 4 {
        z_bore_samples[0][1] > z_bore_samples[1][1] * 1.08
    } else {
        false
    };

    let axis_kink_deg = if drill_type == BeadDrillType::TypeIIb && length_mm > 0.5 {
        (max_step_offset / (length_mm * 0.5)).atan().to_degrees()
    } else {
        0.0
    };

    let hole_profile_zr: Vec<[f64; 3]> = z_bore_samples
        .iter()
        .map(|s| [s[0], -s[1] + s[2], s[1] + s[2]])
        .collect();

    let fabrication = match subtype {
        BeadSubtype::SmallBead | BeadSubtype::AnnularBead | BeadSubtype::MortarDiscBead => {
            if drill_type == BeadDrillType::TypeIa {
                BeadFabricationKind::DrawnParallel
            } else {
                BeadFabricationKind::WoundCircumferential
            }
        }
        _ => BeadFabricationKind::LapidaryCarved,
    };

    let round_001 = |v: f64| (v * 100.0).round() / 100.0;

    BeadPerforationAnalysis {
        subtype,
        fabrication,
        drill_type,
        length_mm: round_001(length_mm),
        outer_diameter_mm: round_001(outer_diameter_mm),
        ld_ratio: round_001(ld_ratio),
        top_entry_diameter_mm: round_001(r_top * 2.0),
        bottom_exit_diameter_mm: if is_blind {
            0.0
        } else {
            round_001(r_bot * 2.0)
        },
        min_bore_diameter_mm: if is_blind {
            0.0
        } else {
            round_001(min_r * 2.0)
        },
        meeting_step_offset_mm: round_001(max_step_offset),
        axis_kink_deg: round_001(axis_kink_deg),
        has_exit_spalling_scar,
        hole_profile_zr,
    }
}

/// 3D 구슬·옥류 메쉬(`TriMesh`)에서 직접 내공 정점(`InnerWall` / 중심부 반지름)을 추출해
/// `BeadPerforationAnalysis`를 수행한다.
pub fn analyze_bead_mesh_perforation(mesh: &TriMesh) -> BeadPerforationAnalysis {
    let ext = mesh.bbox.extents();
    let center = mesh.bbox.center();
    let length_mm = ext[2].max(0.5);
    let outer_diameter_mm = ext[0].max(ext[1]).max(0.5);
    let outer_r = outer_diameter_mm * 0.5;

    let n_slices = 12usize;
    let mut z_samples = Vec::with_capacity(n_slices);
    for i in 0..n_slices {
        let z_frac = (i as f64 + 0.5) / (n_slices as f64);
        let z_val = mesh.bbox.min[2] + z_frac * length_mm;
        let z_band = length_mm / (n_slices as f64) * 0.65;

        let mut min_r = f64::INFINITY;
        let mut sum_x = 0.0;
        let mut cnt = 0usize;
        for &p in &mesh.positions {
            if (p[2] - z_val).abs() <= z_band {
                let dx = p[0] - center[0];
                let dy = p[1] - center[1];
                let r = (dx * dx + dy * dy).sqrt();
                if r < outer_r * 0.65 {
                    if r < min_r {
                        min_r = r;
                    }
                    sum_x += dx;
                    cnt += 1;
                }
            }
        }
        if cnt > 0 && min_r.is_finite() {
            z_samples.push([z_val - mesh.bbox.min[2], min_r, sum_x / (cnt as f64)]);
        } else {
            // 스캐너가 구멍 내부를 못 찍은 경우 기본 양방향(Ⅱb) 또는 원통(Ⅰa) 보간을 위해 입구 값 추정
            let fallback_r = outer_r * 0.22 * (1.0 - 0.35 * (1.0 - (2.0 * z_frac - 1.0).abs()));
            z_samples.push([z_val - mesh.bbox.min[2], fallback_r, 0.0]);
        }
    }

    classify_bead_and_perforation(length_mm, outer_diameter_mm, false, 0, &z_samples)
}
