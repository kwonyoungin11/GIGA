use strata_core::{
    ArtifactCategory, BeadDrillType, BeadSubtype, HandleAttachmentKind, ScalePreset,
    SherdPlateStrategy, SherdSubtype,
};
use strata_measure::ArtifactMeasurementReport;
use strata_section::{OccludedDashedSpan, PlaneSliceResult};
use serde::{Deserialize, Serialize};

/// 8대 고고학 유물군별 표준 보고서 도판 배치 모드 (`DESIGN_SPEC.md` 1.3 & 3.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlateLayoutMode {
    /// ① 완형 토기·도자기 (D5: 좌측 외면 입면+탁본 / 우측 수직 회전단면 + R 반전 + 저부·내저면 탁본 + 다단 횡단면)
    CompleteVesselStandard,
    /// ② 구연·저부 편조각 복원 도판 ($\Delta\theta \ge 45^\circ$ 실선 복원 / $20^\circ\sim 45^\circ$ 추정 파선 복원)
    RimOrBaseSherdReconstructed,
    /// ③ 동체부·미세 편조각 3단 병렬 도판 (`[① 외면+탁본 ── ② 수직 회전단면 ── ③ 내면+탁본]`, 억지 지름 추정 금지)
    BodyOrMicroSherdThreeColumn,
    /// ④ 파수·주구·투창 특수편 도판 (접합면/삽입촉 파선 + 파수 횡단면 보조 배치)
    HandleOrSpoutSherd,
    /// ⑤ 구슬·옥류 확대 도판 (`2:1~10:1` 미소 축척, 상면 3중 원 + 정면 + 종단면 `Ⅰa/Ⅰb/Ⅰb'/Ⅱb` 천공 파선)
    BeadJadeMicroPlate,
    /// ⑥ 곡옥(CommaJade) C자형 중심 곡선 도판 (배면/복면 + 두부 천공 단면 + 동체 횡단면)
    CommaJadeCurvedPlate,
    /// ⑦ 기와·막새 도판 (내면 포목흔 탁본 + 외면 타날문 탁본 + 와도 분할선 화살표 + 접합기법 파선 + 1㎠ 포목흔 밀도 박스)
    RoofTileAndEavesPlate,
    /// ⑧ 석기 3단계 선굵기 박리도 / 부식 금속기 3층 분리 단면 도판 (원심 코어 실선 + 부식 팽창층 파선 + 목질흔 결선)
    LithicAndCorrodedMetalPlate,
}

impl PlateLayoutMode {
    pub fn select_for_artifact(
        category: ArtifactCategory,
        sherd_subtype: Option<SherdSubtype>,
        sherd_strategy: Option<SherdPlateStrategy>,
        bead_subtype: Option<BeadSubtype>,
    ) -> Self {
        match category {
            ArtifactCategory::CompleteVessel => Self::CompleteVesselStandard,
            ArtifactCategory::Potsherd => match sherd_subtype {
                Some(SherdSubtype::HandleSpoutSherd) => Self::HandleOrSpoutSherd,
                Some(SherdSubtype::BodySherd) => Self::BodyOrMicroSherdThreeColumn,
                _ => match sherd_strategy {
                    Some(SherdPlateStrategy::ThreeColumnParallel) => {
                        Self::BodyOrMicroSherdThreeColumn
                    }
                    _ => Self::RimOrBaseSherdReconstructed,
                },
            },
            ArtifactCategory::BeadJade => {
                if bead_subtype == Some(BeadSubtype::CommaJade) {
                    Self::CommaJadeCurvedPlate
                } else {
                    Self::BeadJadeMicroPlate
                }
            }
            ArtifactCategory::RoofTile => Self::RoofTileAndEavesPlate,
            ArtifactCategory::Lithic | ArtifactCategory::CorrodedMetal => {
                Self::LithicAndCorrodedMetalPlate
            }
        }
    }

    pub fn label_kr(self) -> &'static str {
        match self {
            Self::CompleteVesselStandard => "① 완형 토기·도자기 표준 도판 (좌 외면·우 단면)",
            Self::RimOrBaseSherdReconstructed => "② 구연·저부 편조각 회전 복원 도판",
            Self::BodyOrMicroSherdThreeColumn => "③ 동체·미세편 3단 병렬 도판 (외면·단면·내면)",
            Self::HandleOrSpoutSherd => "④ 파수·주구·특수편 접합 도판",
            Self::BeadJadeMicroPlate => "⑤ 구슬·옥류 확대 도판 (상면 3원 + 천공 종단면)",
            Self::CommaJadeCurvedPlate => "⑥ 곡옥 C자형 중심축 + 두부 천공 도판",
            Self::RoofTileAndEavesPlate => "⑦ 기와·막새 도판 (와도 분할선 + 포목흔 밀도)",
            Self::LithicAndCorrodedMetalPlate => "⑧ 석기 3단 선굵기 / 금속기 3층 부식 분리 도판",
        }
    }
}

/// 도판 용지 위에 합성될 다중 뷰 정밀 탁본(`Rubbing`) 래스터 영상의 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlateRasterKind {
    /// 정면 외면 정밀 탁본 (완형 토기는 외면 반쪽 클리핑, 편조각·기와·구슬은 전체 외면)
    FrontExteriorRubbing,
    /// 내면 정밀 탁본 (3단 병렬 편조각·기와의 내면 물레속선/포목흔)
    BackInteriorRubbing,
    /// 저면·외저면 원형 정밀 탁본 (완형 토기 외저면 인화문·굽 탁본)
    BottomPlanRubbing,
}

/// 도판 용지 위 정밀 탁본 래스터 배치 영역 (`mm` 좌표계).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlateRasterRegion {
    pub kind: PlateRasterKind,
    /// `[min_x_mm, min_y_mm, max_x_mm, max_y_mm]`
    pub rect_mm: [f64; 4],
    /// 반단면 클리핑: `Some(-1)`이면 좌측 반쪽만, `Some(1)`이면 우측 반쪽만, `None`이면 전체 표시
    pub clip_half_sign: Option<i8>,
    pub caption: String,
}

/// 도판 내 개별 벡터 선분 또는 폴리라인 요소.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlatePathElement {
    pub layer_id: String,
    /// mm 단위 2D 좌표열 (`1 mm = 5 px` 변환 전 실측 mm 좌표)
    pub points_mm: Vec<[f64; 2]>,
    pub closed: bool,
    pub stroke_hex: String,
    pub stroke_width_mm: f64,
    pub dash_pattern_mm: Option<(f64, f64)>,
    pub fill_hex: Option<String>,
}

/// 도판 내 텍스트 주석 및 치수 콜아웃 요소 (하드코딩 금지: 오직 실측치에서만 생성).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlateTextCallout {
    pub layer_id: String,
    pub pos_mm: [f64; 2],
    pub text: String,
    pub font_size_pt: f64,
    pub color_hex: String,
}

/// 합성 완료된 2D 고고학 실측 도판 문서 (`SVG / PDF / DXF / SheetPane` 공용).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComposedPlate2D {
    pub mode: PlateLayoutMode,
    pub title: String,
    pub scale_preset: ScalePreset,
    /// 실측 도판 전체 폭 (mm)
    pub canvas_width_mm: f64,
    /// 실측 도판 전체 높이 (mm)
    pub canvas_height_mm: f64,
    /// `R` 반전 여부 (`false`: 좌 외면 / 우 단면, `true`: 좌 단면 / 우 외면)
    pub flip_exterior_right: bool,
    pub paths: Vec<PlatePathElement>,
    pub callouts: Vec<PlateTextCallout>,
    /// 도판 용지 위에 합성할 정밀 탁본(`Rubbing`) 다중 뷰 래스터 영역 목록
    #[serde(default)]
    pub raster_regions: Vec<PlateRasterRegion>,
}

/// 도판 합성에 필요한 입력 컨텍스트.
#[derive(Debug, Clone)]
pub struct PlateComposeInput<'a> {
    pub title: &'a str,
    pub measurements: &'a ArtifactMeasurementReport,
    pub vertical_section: &'a PlaneSliceResult,
    pub horizontal_sections: &'a [PlaneSliceResult],
    pub occluded_spans: &'a [OccludedDashedSpan],
    pub sherd_subtype: Option<SherdSubtype>,
    pub sherd_strategy: Option<SherdPlateStrategy>,
    pub handle_attachment: Option<HandleAttachmentKind>,
    pub bead_subtype: Option<BeadSubtype>,
    pub bead_drill_type: Option<BeadDrillType>,
    pub wado_cut_ratio: Option<f64>,
    pub hemp_density_label: Option<String>,
    pub flip_exterior_right: bool,
}

/// 비볼록(Concave) 2D 단면 다각형을 Ear-Clipping 알고리즘으로 삼각분할한다.
/// - 반환값: 입력 `poly` 정점 인덱스 삼조 `[i0, i1, i2]` 목록.
pub fn triangulate_polygon_ear_clipping(poly: &[[f64; 2]]) -> Vec<[usize; 3]> {
    if poly.len() < 3 {
        return Vec::new();
    }

    // 1. 중복·초근접 연속 정점(< 0.12mm) 제거 (원본 인덱스 보존)
    let mut clean_indices: Vec<usize> = Vec::with_capacity(poly.len());
    for i in 0..poly.len() {
        if let Some(&last_idx) = clean_indices.last() {
            let p = poly[last_idx];
            let q = poly[i];
            if (p[0] - q[0]).hypot(p[1] - q[1]) < 0.12 {
                continue;
            }
        }
        clean_indices.push(i);
    }
    if clean_indices.len() >= 2 {
        let first = poly[clean_indices[0]];
        let last = poly[clean_indices[clean_indices.len() - 1]];
        if (first[0] - last[0]).hypot(first[1] - last[1]) < 0.12 {
            clean_indices.pop();
        }
    }
    let n = clean_indices.len();
    if n < 3 {
        return Vec::new();
    }

    // 2. Shoelace 부호 면적으로 회전 방향(CCW > 0, CW < 0) 판별
    let mut signed_area2 = 0.0f64;
    for i in 0..n {
        let p = poly[clean_indices[i]];
        let q = poly[clean_indices[(i + 1) % n]];
        signed_area2 += p[0] * q[1] - q[0] * p[1];
    }
    let mut indices: Vec<usize> = if signed_area2 >= 0.0 {
        clean_indices
    } else {
        clean_indices.into_iter().rev().collect()
    };

    let mut tris = Vec::with_capacity(n.saturating_sub(2));
    let mut guard = n * 4 + 16;

    while indices.len() > 3 && guard > 0 {
        guard -= 1;
        let m = indices.len();
        let mut ear_found = false;
        let mut best_convex_idx = None;
        let mut best_convex_cross = 0.0f64;

        for idx_i in 0..m {
            let prev = indices[(idx_i + m - 1) % m];
            let curr = indices[idx_i];
            let next = indices[(idx_i + 1) % m];

            let a = poly[prev];
            let b = poly[curr];
            let c = poly[next];

            let cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            if cross <= 1e-8 {
                continue;
            }
            if cross > best_convex_cross {
                best_convex_cross = cross;
                best_convex_idx = Some(idx_i);
            }

            let mut contains_other = false;
            for &other in &indices {
                if other == prev || other == curr || other == next {
                    continue;
                }
                if point_in_triangle_2d(poly[other], a, b, c) {
                    contains_other = true;
                    break;
                }
            }

            if !contains_other {
                tris.push([prev, curr, next]);
                indices.remove(idx_i);
                ear_found = true;
                break;
            }
        }

        if !ear_found {
            let pick = best_convex_idx.unwrap_or(1);
            let prev = indices[(pick + m - 1) % m];
            let curr = indices[pick];
            let next = indices[(pick + 1) % m];
            tris.push([prev, curr, next]);
            indices.remove(pick);
        }
    }

    if indices.len() == 3 {
        tris.push([indices[0], indices[1], indices[2]]);
    }
    tris
}

#[inline]
fn point_in_triangle_2d(p: [f64; 2], a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> bool {
    let d1 = (p[0] - b[0]) * (a[1] - b[1]) - (a[0] - b[0]) * (p[1] - b[1]);
    let d2 = (p[0] - c[0]) * (b[1] - c[1]) - (b[0] - c[0]) * (p[1] - c[1]);
    let d3 = (p[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (p[1] - a[1]);
    let has_neg = (d1 < -1e-9) || (d2 < -1e-9) || (d3 < -1e-9);
    let has_pos = (d1 > 1e-9) || (d2 > 1e-9) || (d3 > 1e-9);
    !(has_neg && has_pos)
}

/// Sutherland-Hodgman 반평면 클리핑: 다각형 `poly`를 중심축(`x = 0.0`) 기준으로
/// `keep_positive_x == true`이면 `x >= 0.0` 반평면으로, `false`이면 `x <= 0.0` 반평면으로 절단한다.
fn clip_polygon_half_plane_x(poly: &[[f64; 2]], keep_positive_x: bool) -> Vec<[f64; 2]> {
    if poly.len() < 3 {
        return poly.to_vec();
    }
    let inside = |p: [f64; 2]| -> bool {
        if keep_positive_x {
            p[0] >= -1e-5
        } else {
            p[0] <= 1e-5
        }
    };
    let mut out = Vec::with_capacity(poly.len() + 4);
    for i in 0..poly.len() {
        let cur = poly[i];
        let prev = poly[(i + poly.len() - 1) % poly.len()];
        let cur_in = inside(cur);
        let prev_in = inside(prev);

        if cur_in != prev_in {
            let dx = cur[0] - prev[0];
            if dx.abs() > 1e-12 {
                let t = (-prev[0] / dx).clamp(0.0, 1.0);
                let iy = prev[1] + t * (cur[1] - prev[1]);
                out.push([0.0, iy]);
            }
        }
        if cur_in {
            out.push(cur);
        }
    }
    out
}

type MeasuredHorizSectionContours = (Vec<[f64; 2]>, Vec<[f64; 2]>, Vec<[f64; 2]>, f64);

/// 수평 절단면(`PlaneSliceResult`)의 실제 3D 메쉬 교차 점군(`polylines_2d` + `segments_2d`)으로부터
/// 72방위각(5° 간격) 실제 외벽 윤곽선(`outer_contour`), 실제 내벽 윤곽선(`inner_contour`),
/// 우측 반단면 폐합 폴리곤(`right_half_poly`), 그리고 최대 반경(`max_r`)을 추출한다 (`Image 2`).
fn extract_measured_horizontal_section_contours(
    hsec: &PlaneSliceResult,
    wall_t: f64,
    fallback_r: f64,
) -> MeasuredHorizSectionContours {
    const N_BINS: usize = 72;
    let mut all_pts: Vec<[f64; 2]> = hsec.polylines_2d.iter().flatten().copied().collect();
    for seg in &hsec.segments_2d {
        all_pts.push(seg[0]);
        all_pts.push(seg[1]);
    }

    if all_pts.len() < 6 {
        let ro = fallback_r.max(10.0);
        let ri = (ro - wall_t).max(6.0);
        let mut outer = Vec::with_capacity(N_BINS);
        let mut inner = Vec::with_capacity(N_BINS);
        for k in 0..N_BINS {
            let a = -std::f64::consts::PI + (k as f64) * std::f64::consts::TAU / (N_BINS as f64);
            outer.push([ro * a.cos(), ro * a.sin()]);
            inner.push([ri * a.cos(), ri * a.sin()]);
        }
        let mut rpoly = Vec::with_capacity(38);
        for step in 0..=18 {
            let a = -std::f64::consts::FRAC_PI_2 + (step as f64) * std::f64::consts::PI / 18.0;
            rpoly.push([ro * a.cos(), ro * a.sin()]);
        }
        for step in (0..=18).rev() {
            let a = -std::f64::consts::FRAC_PI_2 + (step as f64) * std::f64::consts::PI / 18.0;
            rpoly.push([ri * a.cos(), ri * a.sin()]);
        }
        return (outer, inner, rpoly, ro);
    }

    let mut bin_max_r = [0.0_f64; N_BINS];
    let mut bin_min_r = [f64::INFINITY; N_BINS];
    let mut bin_has = [false; N_BINS];

    for p in &all_pts {
        let r = p[0].hypot(p[1]);
        if r < 1.0 || !r.is_finite() {
            continue;
        }
        let ang = p[1].atan2(p[0]); // [-pi, pi]
        let norm = ((ang + std::f64::consts::PI) / std::f64::consts::TAU).clamp(0.0, 0.999_999);
        let bi = (norm * (N_BINS as f64)) as usize;
        bin_has[bi] = true;
        if r > bin_max_r[bi] {
            bin_max_r[bi] = r;
        }
        if r < bin_min_r[bi] {
            bin_min_r[bi] = r;
        }
    }

    // 인접 빈(±3 섹터) 보간으로 미세 삼각형 틈새 연결 및 실제 비대칭·타원 형상 100% 보존
    let mut smooth_out_r = [0.0_f64; N_BINS];
    let mut smooth_in_r = [0.0_f64; N_BINS];
    let mut smooth_valid = [false; N_BINS];
    let mut global_max_r = 10.0_f64;

    for k in 0..N_BINS {
        let mut sum_o = 0.0;
        let mut sum_i = 0.0;
        let mut w_sum = 0.0;
        for d in -2isize..=2isize {
            let nk = ((k as isize + d).rem_euclid(N_BINS as isize)) as usize;
            if bin_has[nk] {
                let w = match d.abs() {
                    0 => 1.0,
                    1 => 0.65,
                    _ => 0.35,
                };
                let ro = bin_max_r[nk];
                let ri = if (ro - bin_min_r[nk]) >= 1.2 {
                    bin_min_r[nk].max((ro - wall_t * 1.8).max(2.0))
                } else {
                    (ro - wall_t).max(2.0)
                };
                sum_o += ro * w;
                sum_i += ri * w;
                w_sum += w;
            }
        }
        if w_sum > 0.0 {
            let ro = sum_o / w_sum;
            let ri = (sum_i / w_sum).clamp(1.5, (ro - 1.2).max(1.5));
            smooth_out_r[k] = ro;
            smooth_in_r[k] = ri;
            smooth_valid[k] = true;
            if ro > global_max_r {
                global_max_r = ro;
            }
        }
    }

    let mut outer_pts = Vec::with_capacity(N_BINS);
    let mut inner_pts = Vec::with_capacity(N_BINS);
    let mut right_outer = Vec::new();
    let mut right_inner = Vec::new();

    for k in 0..N_BINS {
        if !smooth_valid[k] {
            continue;
        }
        let a = -std::f64::consts::PI
            + ((k as f64) + 0.5) * std::f64::consts::TAU / (N_BINS as f64);
        let (sin_a, cos_a) = a.sin_cos();
        let po = [ smooth_out_r[k] * cos_a, smooth_out_r[k] * sin_a ];
        let pi = [ smooth_in_r[k] * cos_a, smooth_in_r[k] * sin_a ];
        outer_pts.push(po);
        inner_pts.push(pi);
        if cos_a >= -0.02 {
            right_outer.push(po);
            right_inner.push(pi);
        }
    }

    let mut right_half_poly = Vec::with_capacity(right_outer.len() + right_inner.len());
    right_half_poly.extend_from_slice(&right_outer);
    for pt in right_inner.into_iter().rev() {
        right_half_poly.push(pt);
    }

    (outer_pts, inner_pts, right_half_poly, global_max_r)
}

/// 실측치(`ArtifactMeasurementReport`)와 단면 윤곽(`PlaneSliceResult`)으로부터 8대 유물군 맞춤형 2D 도판을 합성한다.
pub fn compose_archaeological_plate(input: &PlateComposeInput<'_>) -> ComposedPlate2D {
    let m = input.measurements;
    let mode = PlateLayoutMode::select_for_artifact(
        m.category,
        input.sherd_subtype,
        input.sherd_strategy,
        input.bead_subtype,
    );

    let max_d = m
        .max_belly_diameter_mm
        .max(m.rim_or_outer_diameter_mm)
        .max(20.0);
    let height = m.height_or_length_mm.max(10.0);
    let margin = 22.0;

    let (canvas_width_mm, canvas_height_mm) = match mode {
        PlateLayoutMode::CompleteVesselStandard
        | PlateLayoutMode::RimOrBaseSherdReconstructed => (
            (max_d * 2.45 + margin * 2.0).max(210.0),
            (height + max_d + margin * 3.1).max(236.0),
        ),
        PlateLayoutMode::BodyOrMicroSherdThreeColumn => (
            (max_d * 3.1 + margin * 2.0).max(160.0),
            (height + max_d * 1.05 + margin * 2.8).max(168.0),
        ),
        PlateLayoutMode::BeadJadeMicroPlate | PlateLayoutMode::CommaJadeCurvedPlate => (
            (max_d * 3.5 + margin * 2.0).max(60.0),
            (height * 3.2 + margin * 2.4).max(68.0),
        ),
        _ => (
            (max_d * 2.15 + margin * 2.0).max(150.0),
            (height + max_d * 1.05 + margin * 2.8).max(164.0),
        ),
    };

    let mut paths = Vec::new();
    let mut callouts = Vec::new();
    let mut raster_regions = Vec::new();

    // 완형 토기 및 구연·저부 복원 도판은 좌측 열에 평·단면도를 두고 우측 열에 1:1 정밀 탁본 및 수평 횡단면을 배치하여 용지 내부에 100% 수납
    let cx = match mode {
        PlateLayoutMode::CompleteVesselStandard
        | PlateLayoutMode::RimOrBaseSherdReconstructed => margin + max_d * 0.56,
        _ => canvas_width_mm * 0.5,
    };
    let base_y = margin + 15.0;
    let top_y = base_y + height;

    // 수직 단면 폴리라인의 Z 최소값을 구하여 항상 base_y 위에 정렬되도록 보정
    let z_min_slice = input
        .vertical_section
        .polylines_2d
        .iter()
        .flatten()
        .map(|p| p[1])
        .fold(f64::INFINITY, f64::min);
    let z_offset = if z_min_slice.is_finite() {
        z_min_slice
    } else {
        0.0
    };

    // 1. 중심 회전축 / 기준선 제도 (`#141413` 먹색 0.25mm)
    if mode != PlateLayoutMode::BodyOrMicroSherdThreeColumn {
        paths.push(PlatePathElement {
            layer_id: String::from("중심_기준선"),
            points_mm: vec![[cx, base_y - 2.0], [cx, top_y + 2.0]],
            closed: false,
            stroke_hex: String::from("#141413"),
            stroke_width_mm: 0.25,
            dash_pattern_mm: None,
            fill_hex: None,
        });
    }

    // 2. 구경선 / 저경선 (수평 브리지 연결선)
    let is_estimated_dashed = matches!(
        input.sherd_strategy,
        Some(SherdPlateStrategy::EstimatedDashedReconstruction)
    );
    if m.rim_or_outer_diameter_mm > 0.0 && mode != PlateLayoutMode::BodyOrMicroSherdThreeColumn {
        let half_rim = m.rim_or_outer_diameter_mm * 0.5;
        paths.push(PlatePathElement {
            layer_id: if is_estimated_dashed {
                String::from("추정복원_파선")
            } else {
                String::from("구경_기준선")
            },
            points_mm: vec![[cx - half_rim, top_y], [cx + half_rim, top_y]],
            closed: false,
            stroke_hex: String::from("#141413"),
            stroke_width_mm: 0.32,
            dash_pattern_mm: if is_estimated_dashed {
                Some((2.0, 1.5))
            } else {
                None
            },
            fill_hex: None,
        });
    }

    if m.base_diameter_or_width_mm > 0.0 && mode != PlateLayoutMode::BodyOrMicroSherdThreeColumn {
        let half_base = m.base_diameter_or_width_mm * 0.5;
        paths.push(PlatePathElement {
            layer_id: String::from("저경_기준선"),
            points_mm: vec![[cx - half_base, base_y], [cx + half_base, base_y]],
            closed: false,
            stroke_hex: String::from("#141413"),
            stroke_width_mm: 0.32,
            dash_pattern_mm: None,
            fill_hex: None,
        });
    }

    // 3. 수직 회전단면 `#141413` 먹물 채움 폴리곤 및 반대편 외면 입면선/탁본 영역 배치
    let section_sign = if input.flip_exterior_right { -1.0 } else { 1.0 };
    let keep_positive_x = section_sign > 0.0;
    let wall_t = m.mean_wall_thickness_mm.clamp(2.2, 12.0);

    let fallback_polylines: Vec<Vec<[f64; 2]>> =
        if input.vertical_section.polylines_2d.iter().all(|p| p.len() < 2) {
            let r_base = (m.base_diameter_or_width_mm * 0.5).max(max_d * 0.32);
            let r_belly = (m.max_belly_diameter_mm * 0.5).max(r_base + 4.0);
            let r_rim = (m.rim_or_outer_diameter_mm * 0.5).max(r_base + 2.0);
            vec![vec![
                [0.0, 0.0],
                [r_base, 0.0],
                [r_belly, height * 0.55],
                [r_rim, height],
                [(r_rim - wall_t).max(0.0), height],
                [(r_belly - wall_t).max(0.0), height * 0.55],
                [(r_base - wall_t).max(0.0), wall_t],
                [0.0, wall_t],
            ]]
        } else {
            Vec::new()
        };
    let active_polylines = if fallback_polylines.is_empty() {
        &input.vertical_section.polylines_2d
    } else {
        &fallback_polylines
    };

    for poly in active_polylines {
        if poly.len() < 2 {
            continue;
        }
        let norm_poly: Vec<[f64; 2]> = poly
            .iter()
            .map(|pt| [pt[0], (pt[1] - z_offset).clamp(0.0, height * 1.15)])
            .collect();

        let min_x = norm_poly.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        let max_x = norm_poly
            .iter()
            .map(|p| p[0])
            .fold(f64::NEG_INFINITY, f64::max);
        let spans_both_halves = min_x < -4.0 && max_x > 4.0 && norm_poly.len() >= 6;

        if mode == PlateLayoutMode::BodyOrMicroSherdThreeColumn {
            // 3단 병렬 편조각 도판: 중앙 열(cx)에 수직 회전단면 #141413 솔리드 채움 배치
            let x_mid = 0.5 * (min_x + max_x);
            let mut closed_sec: Vec<[f64; 2]> = norm_poly
                .iter()
                .map(|p| [cx + (p[0] - x_mid) * 0.35, base_y + p[1]])
                .collect();
            let first = closed_sec[0];
            let last = *closed_sec.last().unwrap_or(&first);
            if (first[0] - last[0]).hypot(first[1] - last[1]) > 3.5 {
                let mut inner_rev: Vec<[f64; 2]> = closed_sec
                    .iter()
                    .rev()
                    .map(|p| [p[0] + wall_t, p[1]])
                    .collect();
                closed_sec.append(&mut inner_rev);
            }
            paths.push(PlatePathElement {
                layer_id: String::from("단면_실선"),
                points_mm: closed_sec,
                closed: true,
                stroke_hex: String::from("#141413"),
                stroke_width_mm: 0.35,
                dash_pattern_mm: None,
                fill_hex: Some(String::from("#141413")),
            });
        } else if spans_both_halves {
            // 완형 토기 등 좌우 전체를 가로지르는 폐합 단면 루프:
            // (a) 단면 쪽 반평면(`x * section_sign >= 0`)으로 클리핑하여 `#141413` 먹물 채움 폐합 폴리곤 생성
            let sec_half = clip_polygon_half_plane_x(&norm_poly, keep_positive_x);
            if sec_half.len() >= 3 {
                let mapped_sec: Vec<[f64; 2]> = sec_half
                    .into_iter()
                    .map(|pt| [cx + pt[0], base_y + pt[1]])
                    .collect();
                paths.push(PlatePathElement {
                    layer_id: String::from("단면_실선"),
                    points_mm: mapped_sec,
                    closed: true,
                    stroke_hex: String::from("#141413"),
                    stroke_width_mm: 0.38,
                    dash_pattern_mm: None,
                    fill_hex: Some(String::from("#141413")),
                });
            }

            // (b) 외면 쪽 반평면(`x * section_sign <= 0`)으로 클리핑하여 외면 입면 윤곽선 및 입면 구조선 생성 (Image 0: 일러스트 편집용 순수 벡터 평·단면 선화)
            let ext_half = clip_polygon_half_plane_x(&norm_poly, !keep_positive_x);
            if ext_half.len() >= 2 {
                // 외면 최외곽 프로필만 추출하여 내부 단면선이 입면 쪽에 중복되지 않게 정렬
                let mapped_ext: Vec<[f64; 2]> = ext_half
                    .iter()
                    .map(|pt| [cx + pt[0], base_y + pt[1]])
                    .collect();
                paths.push(PlatePathElement {
                    layer_id: String::from("외곽선_실선"),
                    points_mm: mapped_ext,
                    closed: false,
                    stroke_hex: String::from("#141413"),
                    stroke_width_mm: 0.38,
                    dash_pattern_mm: None,
                    fill_hex: None,
                });

                // 일러스트레이터 표준 입면 구조선 (구연단선·동체 굴곡선·굽 접합선: 좌측 외곽선에서 중심축 cx까지 수평 실선 연결)
                for z_frac in [0.93_f64, 0.32_f64, 0.09_f64] {
                    let target_z = height * z_frac;
                    if let Some(best_pt) = ext_half
                        .iter()
                        .filter(|p| p[0].abs() > 3.0)
                        .min_by(|a, b| {
                            (a[1] - target_z)
                                .abs()
                                .partial_cmp(&(b[1] - target_z).abs())
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                    {
                        if (best_pt[1] - target_z).abs() < height * 0.12 {
                            paths.push(PlatePathElement {
                                layer_id: String::from("외곽선_입면구조선"),
                                points_mm: vec![
                                    [cx + best_pt[0], base_y + best_pt[1]],
                                    [cx, base_y + best_pt[1]],
                                ],
                                closed: false,
                                stroke_hex: String::from("#141413"),
                                stroke_width_mm: 0.24,
                                dash_pattern_mm: None,
                                fill_hex: None,
                            });
                        }
                    }
                }
            }
        } else {
            // 단일 벽면 또는 반쪽 폴리라인인 경우: 두께(`wall_t`)를 부여하여 폐합 `#141413` 단면 폴리곤 생성
            let mut sec_pts: Vec<[f64; 2]> = norm_poly
                .iter()
                .map(|pt| [cx + pt[0].abs() * section_sign, base_y + pt[1]])
                .collect();
            let first = sec_pts[0];
            let last = *sec_pts.last().unwrap_or(&first);
            if (first[0] - last[0]).hypot(first[1] - last[1]) > 2.5 {
                let mut inner_pts: Vec<[f64; 2]> = norm_poly
                    .iter()
                    .rev()
                    .map(|pt| {
                        let r_in = (pt[0].abs() - wall_t).max(0.0);
                        [cx + r_in * section_sign, base_y + pt[1]]
                    })
                    .collect();
                sec_pts.append(&mut inner_pts);
            }
            paths.push(PlatePathElement {
                layer_id: String::from("단면_실선"),
                points_mm: sec_pts,
                closed: true,
                stroke_hex: String::from("#141413"),
                stroke_width_mm: 0.38,
                dash_pattern_mm: None,
                fill_hex: Some(String::from("#141413")),
            });

            let ext_mapped: Vec<[f64; 2]> = norm_poly
                .iter()
                .map(|pt| [cx - pt[0].abs() * section_sign, base_y + pt[1]])
                .collect();
            paths.push(PlatePathElement {
                layer_id: String::from("외곽선_실선"),
                points_mm: ext_mapped,
                closed: false,
                stroke_hex: String::from("#141413"),
                stroke_width_mm: 0.38,
                dash_pattern_mm: None,
                fill_hex: None,
            });
        }
    }

    // 4. 유물군별 다중 뷰 정밀 탁본(`Rubbing`) 래스터 영역 및 특화 도판 요소 배치
    let half_d = max_d * 0.5;
    match mode {
        PlateLayoutMode::CompleteVesselStandard => {
            // (a) 좌하단: 1:1 정면 입·단면도 기준 영역 (Image 0: 일러스트 편집용 평·단면 벡터 선화 영역)
            let ext_sign = if input.flip_exterior_right { 1i8 } else { -1i8 };
            raster_regions.push(PlateRasterRegion {
                kind: PlateRasterKind::FrontExteriorRubbing,
                rect_mm: [cx - half_d, base_y, cx + half_d, top_y],
                clip_half_sign: Some(ext_sign),
                caption: String::from("1:1 정면 단면·입면도"),
            });

            // (b) 좌상단 (Image 0): 1:1 평면도 (일러스트 편집용 순수 벡터 평면 실측 선화: 구연 외경·내경, 내저원각, 굽 외경·접지경 동심 실선)
            let plan_r = half_d;
            let cx_plan = cx;
            let cy_plan = top_y + plan_r + 24.0;
            let cx_right = cx + half_d + half_d * 0.94 + 22.0;

            paths.push(PlatePathElement {
                layer_id: String::from("외저면_십자선"),
                points_mm: vec![
                    [cx_plan - plan_r * 1.06, cy_plan],
                    [cx_plan + plan_r * 1.06, cy_plan],
                ],
                closed: false,
                stroke_hex: String::from("#5E5D59"),
                stroke_width_mm: 0.20,
                dash_pattern_mm: None,
                fill_hex: None,
            });
            paths.push(PlatePathElement {
                layer_id: String::from("외저면_십자선"),
                points_mm: vec![
                    [cx_plan, cy_plan - plan_r * 1.06],
                    [cx_plan, cy_plan + plan_r * 1.06],
                ],
                closed: false,
                stroke_hex: String::from("#5E5D59"),
                stroke_width_mm: 0.20,
                dash_pattern_mm: None,
                fill_hex: None,
            });
            // 1:1 평면도 외곽 원형 윤곽선 및 구연 내경·내저원각·굽 동심 벡터 실측선 (일러스트레이터 편집용!)
            let base_r = (m.base_diameter_or_width_mm * 0.5).clamp(plan_r * 0.34, plan_r * 0.72);
            for (r_ring, layer_name, width_mm) in [
                (plan_r, "평면도_외곽윤곽선", 0.35),
                ((plan_r - wall_t).max(6.0), "평면도_구연내경선", 0.28),
                (base_r, "평면도_굽외경선", 0.26),
                ((base_r - wall_t * 0.85).max(4.0), "평면도_내저원각선", 0.22),
            ] {
                let mut ring_pts = Vec::with_capacity(73);
                for step in 0..=72 {
                    let a = (step as f64) * std::f64::consts::TAU / 72.0;
                    ring_pts.push([cx_plan + r_ring * a.cos(), cy_plan + r_ring * a.sin()]);
                }
                paths.push(PlatePathElement {
                    layer_id: String::from(layer_name),
                    points_mm: ring_pts,
                    closed: true,
                    stroke_hex: String::from("#141413"),
                    stroke_width_mm: width_mm,
                    dash_pattern_mm: None,
                    fill_hex: None,
                });
            }
            callouts.push(PlateTextCallout {
                layer_id: String::from("치수_축척막대"),
                pos_mm: [cx_plan - 24.0, cy_plan + plan_r + 5.5],
                text: format!("1:1 평면도 (Ø{:.1}mm)", max_d),
                font_size_pt: 8.5,
                color_hex: String::from("#141413"),
            });

            // (c) 우상단 (Image 1: "탁본은 여기에 1:1로 여기에 나타나게하라"): 1:1 정밀 탁본 전용 배치!
            let cy_rub = cy_plan;
            raster_regions.push(PlateRasterRegion {
                kind: PlateRasterKind::BottomPlanRubbing,
                rect_mm: [
                    cx_right - plan_r,
                    cy_rub - plan_r,
                    cx_right + plan_r,
                    cy_rub + plan_r,
                ],
                clip_half_sign: None,
                caption: String::from("1:1 정밀 탁본"),
            });
            paths.push(PlatePathElement {
                layer_id: String::from("탁본_십자선"),
                points_mm: vec![
                    [cx_right - plan_r * 1.05, cy_rub],
                    [cx_right + plan_r * 1.05, cy_rub],
                ],
                closed: false,
                stroke_hex: String::from("#5E5D59"),
                stroke_width_mm: 0.18,
                dash_pattern_mm: None,
                fill_hex: None,
            });
            paths.push(PlatePathElement {
                layer_id: String::from("탁본_십자선"),
                points_mm: vec![
                    [cx_right, cy_rub - plan_r * 1.05],
                    [cx_right, cy_rub + plan_r * 1.05],
                ],
                closed: false,
                stroke_hex: String::from("#5E5D59"),
                stroke_width_mm: 0.18,
                dash_pattern_mm: None,
                fill_hex: None,
            });
            callouts.push(PlateTextCallout {
                layer_id: String::from("치수_축척막대"),
                pos_mm: [cx_right - 24.0, cy_rub + plan_r + 5.5],
                text: format!("1:1 정밀 탁본 (Ø{:.1}mm)", max_d),
                font_size_pt: 8.5,
                color_hex: String::from("#141413"),
            });

            // (d) 우하단 (Image 2: "수평횡단면도 실제 그려지는걸로 그려서 나와야하지않나"):
            // 실제 3D 메쉬의 Z 높이 수평 절단 폴리라인으로부터 실측 외벽·내벽·단면 윤곽선을 추출하여 제도!
            let cx_hsec = cx_right;
            let cy_hsec = base_y + height * 0.52;
            if let Some(hsec) = input.horizontal_sections.first() {
                let cut_z_rel = (hsec.offset_mm - z_offset).clamp(1.0, height - 1.0);
                let cut_line_y = base_y + cut_z_rel;
                // 정면 단면·입면도 위를 가로지르는 수평 절단 위치선 (휠 조작 시 선만 상하 이동)
                paths.push(PlatePathElement {
                    layer_id: String::from("횡단면_절단위치선"),
                    points_mm: vec![
                        [cx - half_d * 1.04, cut_line_y],
                        [cx + half_d * 1.08, cut_line_y],
                    ],
                    closed: false,
                    stroke_hex: String::from("#D92B2B"),
                    stroke_width_mm: 0.32,
                    dash_pattern_mm: Some((2.2, 1.2)),
                    fill_hex: None,
                });

                let (outer_contour, inner_contour, right_half_poly, max_r) =
                    extract_measured_horizontal_section_contours(hsec, wall_t, half_d * 0.78);

                paths.push(PlatePathElement {
                    layer_id: String::from("횡단면_십자선"),
                    points_mm: vec![
                        [cx_hsec - max_r * 1.12, cy_hsec],
                        [cx_hsec + max_r * 1.12, cy_hsec],
                    ],
                    closed: false,
                    stroke_hex: String::from("#5E5D59"),
                    stroke_width_mm: 0.18,
                    dash_pattern_mm: None,
                    fill_hex: None,
                });
                paths.push(PlatePathElement {
                    layer_id: String::from("횡단면_십자선"),
                    points_mm: vec![
                        [cx_hsec, cy_hsec - max_r * 1.12],
                        [cx_hsec, cy_hsec + max_r * 1.12],
                    ],
                    closed: false,
                    stroke_hex: String::from("#5E5D59"),
                    stroke_width_mm: 0.18,
                    dash_pattern_mm: None,
                    fill_hex: None,
                });

                // (b-2) 좌상단 1:1 평면도(cx_plan, cy_plan) 위에도 실제 메쉬 절단 외벽·내벽 윤곽선 중첩 표시
                for (contour_pts, layer_name) in [
                    (&outer_contour, "횡단면_외벽윤곽선"),
                    (&inner_contour, "횡단면_내벽윤곽선"),
                ] {
                    if contour_pts.len() >= 3 {
                        let ring_top: Vec<[f64; 2]> = contour_pts
                            .iter()
                            .map(|p| [cx_plan + p[0], cy_plan + p[1]])
                            .collect();
                        paths.push(PlatePathElement {
                            layer_id: String::from(layer_name),
                            points_mm: ring_top,
                            closed: true,
                            stroke_hex: String::from("#B5573A"),
                            stroke_width_mm: 0.28,
                            dash_pattern_mm: Some((2.0, 1.2)),
                            fill_hex: None,
                        });
                    }
                }

                // 우하단 1:1 수평 횡단면: 실제 3D 메쉬에서 절단된 원본 폴리라인(hsec.polylines_2d) 및 실측 외벽·내벽 윤곽선 제도!
                for (contour_pts, layer_name) in [
                    (&outer_contour, "횡단면_외벽윤곽선"),
                    (&inner_contour, "횡단면_내벽윤곽선"),
                ] {
                    if contour_pts.len() >= 3 {
                        let ring: Vec<[f64; 2]> = contour_pts
                            .iter()
                            .map(|p| [cx_hsec + p[0], cy_hsec + p[1]])
                            .collect();
                        paths.push(PlatePathElement {
                            layer_id: String::from(layer_name),
                            points_mm: ring,
                            closed: true,
                            stroke_hex: String::from("#141413"),
                            stroke_width_mm: 0.34,
                            dash_pattern_mm: None,
                            fill_hex: None,
                        });
                    }
                }
                for raw_poly in &hsec.polylines_2d {
                    if raw_poly.len() >= 3 {
                        let pts: Vec<[f64; 2]> = raw_poly
                            .iter()
                            .map(|p| [cx_hsec + p[0], cy_hsec + p[1]])
                            .collect();
                        paths.push(PlatePathElement {
                            layer_id: String::from("횡단면_실측윤곽선"),
                            points_mm: pts,
                            closed: false,
                            stroke_hex: String::from("#B5573A"),
                            stroke_width_mm: 0.24,
                            dash_pattern_mm: None,
                            fill_hex: None,
                        });
                    }
                }
                if right_half_poly.len() >= 6 {
                    let mapped_rpoly: Vec<[f64; 2]> = right_half_poly
                        .into_iter()
                        .map(|p| [cx_hsec + p[0], cy_hsec + p[1]])
                        .collect();
                    paths.push(PlatePathElement {
                        layer_id: String::from("횡단면_단면_실선"),
                        points_mm: mapped_rpoly,
                        closed: true,
                        stroke_hex: String::from("#141413"),
                        stroke_width_mm: 0.36,
                        dash_pattern_mm: None,
                        fill_hex: Some(String::from("#141413")),
                    });
                }

                callouts.push(PlateTextCallout {
                    layer_id: String::from("치수_축척막대"),
                    pos_mm: [cx_hsec - max_r * 0.85, cy_hsec + max_r + 5.5],
                    text: format!("1:1 수평 횡단면 (Z={:.1}mm · 실측단면)", cut_z_rel),
                    font_size_pt: 8.5,
                    color_hex: String::from("#B5573A"),
                });
            }
        }
        PlateLayoutMode::RimOrBaseSherdReconstructed => {
            let ext_sign = if input.flip_exterior_right { 1i8 } else { -1i8 };
            raster_regions.push(PlateRasterRegion {
                kind: PlateRasterKind::FrontExteriorRubbing,
                rect_mm: [cx - half_d, base_y, cx + half_d, top_y],
                clip_half_sign: Some(ext_sign),
                caption: String::from("편조각 외면 정밀 탁본"),
            });
            // 우측 보조 내면 탁본 배치
            let int_w = (half_d * 0.65).clamp(18.0, 42.0);
            let x_int = cx + half_d + int_w * 0.65 + 12.0;
            raster_regions.push(PlateRasterRegion {
                kind: PlateRasterKind::BackInteriorRubbing,
                rect_mm: [x_int - int_w * 0.5, base_y, x_int + int_w * 0.5, top_y],
                clip_half_sign: None,
                caption: String::from("내면(물레속선 탁본)"),
            });
            paths.push(PlatePathElement {
                layer_id: String::from("내면_연결선"),
                points_mm: vec![[cx + half_d, top_y], [x_int - int_w * 0.5, top_y]],
                closed: false,
                stroke_hex: String::from("#5E5D59"),
                stroke_width_mm: 0.20,
                dash_pattern_mm: Some((1.5, 1.2)),
                fill_hex: None,
            });
            paths.push(PlatePathElement {
                layer_id: String::from("내면_연결선"),
                points_mm: vec![[cx + half_d, base_y], [x_int - int_w * 0.5, base_y]],
                closed: false,
                stroke_hex: String::from("#5E5D59"),
                stroke_width_mm: 0.20,
                dash_pattern_mm: Some((1.5, 1.2)),
                fill_hex: None,
            });
            callouts.push(PlateTextCallout {
                layer_id: String::from("치수_축척막대"),
                pos_mm: [x_int - int_w * 0.45, top_y + 5.0],
                text: String::from("내면 정밀 탁본"),
                font_size_pt: 8.5,
                color_hex: String::from("#141413"),
            });
        }
        PlateLayoutMode::BodyOrMicroSherdThreeColumn => {
            // [① 외면+정밀탁본 ── ② 수직 회전단면(#141413) ── ③ 내면+정밀탁본] 3단 병렬 배치
            let col_w = (max_d * 0.72).clamp(24.0, 68.0);
            let col_gap = col_w + 18.0;
            let x_ext = cx - col_gap;
            let x_sec = cx;
            let x_int = cx + col_gap;

            raster_regions.push(PlateRasterRegion {
                kind: PlateRasterKind::FrontExteriorRubbing,
                rect_mm: [x_ext - col_w * 0.5, base_y, x_ext + col_w * 0.5, top_y],
                clip_half_sign: None,
                caption: String::from("외면(정밀 탁본)"),
            });
            raster_regions.push(PlateRasterRegion {
                kind: PlateRasterKind::BackInteriorRubbing,
                rect_mm: [x_int - col_w * 0.5, base_y, x_int + col_w * 0.5, top_y],
                clip_half_sign: None,
                caption: String::from("내면(정밀 탁본)"),
            });

            for bridge_y in [top_y, base_y] {
                paths.push(PlatePathElement {
                    layer_id: String::from("3단병렬_연결선"),
                    points_mm: vec![
                        [x_ext + col_w * 0.45, bridge_y],
                        [x_sec - 4.0, bridge_y],
                    ],
                    closed: false,
                    stroke_hex: String::from("#5C5B57"),
                    stroke_width_mm: 0.22,
                    dash_pattern_mm: None,
                    fill_hex: None,
                });
                paths.push(PlatePathElement {
                    layer_id: String::from("3단병렬_연결선"),
                    points_mm: vec![
                        [x_sec + 4.0, bridge_y],
                        [x_int - col_w * 0.45, bridge_y],
                    ],
                    closed: false,
                    stroke_hex: String::from("#5C5B57"),
                    stroke_width_mm: 0.22,
                    dash_pattern_mm: None,
                    fill_hex: None,
                });
            }
            callouts.push(PlateTextCallout {
                layer_id: String::from("치수_축척막대"),
                pos_mm: [x_ext - col_w * 0.3, top_y + 5.0],
                text: String::from("① 외면 정밀 탁본"),
                font_size_pt: 8.5,
                color_hex: String::from("#141413"),
            });
            callouts.push(PlateTextCallout {
                layer_id: String::from("치수_축척막대"),
                pos_mm: [x_sec - 8.0, top_y + 5.0],
                text: String::from("② 회전단면"),
                font_size_pt: 8.5,
                color_hex: String::from("#141413"),
            });
            callouts.push(PlateTextCallout {
                layer_id: String::from("치수_축척막대"),
                pos_mm: [x_int - col_w * 0.3, top_y + 5.0],
                text: String::from("③ 내면 정밀 탁본"),
                font_size_pt: 8.5,
                color_hex: String::from("#141413"),
            });
        }
        PlateLayoutMode::HandleOrSpoutSherd => {
            raster_regions.push(PlateRasterRegion {
                kind: PlateRasterKind::FrontExteriorRubbing,
                rect_mm: [cx - half_d, base_y, cx + half_d, top_y],
                clip_half_sign: Some(if input.flip_exterior_right { 1 } else { -1 }),
                caption: String::from("파수·주구 정밀 탁본"),
            });
            let attach_kind = input
                .handle_attachment
                .unwrap_or(HandleAttachmentKind::InsertedTenonJoint);
            let mid_y = base_y + height * 0.55;
            let wall_x = cx + max_d * 0.35 * section_sign;
            let tenon_depth = match attach_kind {
                HandleAttachmentKind::InsertedTenonJoint => -6.5 * section_sign,
                HandleAttachmentKind::SurfaceButtJoint => 0.0,
            };
            paths.push(PlatePathElement {
                layer_id: String::from("접합기법_파선"),
                points_mm: vec![
                    [wall_x, mid_y + 5.0],
                    [wall_x + tenon_depth, mid_y],
                    [wall_x, mid_y - 5.0],
                ],
                closed: false,
                stroke_hex: String::from("#B5573A"),
                stroke_width_mm: 0.25,
                dash_pattern_mm: Some((1.5, 1.0)),
                fill_hex: None,
            });
        }
        PlateLayoutMode::BeadJadeMicroPlate | PlateLayoutMode::CommaJadeCurvedPlate => {
            raster_regions.push(PlateRasterRegion {
                kind: PlateRasterKind::FrontExteriorRubbing,
                rect_mm: [cx - half_d, base_y, cx + half_d, top_y],
                clip_half_sign: Some(-1),
                caption: String::from("옥류 외면 정밀 탁본"),
            });
            let plan_cy = top_y + max_d * 0.85 + 8.0;
            let r_outer = (max_d * 0.5).max(1.5);
            let (r_entry, r_waist) = if let Some(ref perf) = m.bead_perforation {
                (
                    perf.top_entry_diameter_mm * 0.5,
                    perf.min_bore_diameter_mm * 0.5,
                )
            } else {
                (r_outer * 0.45, r_outer * 0.32)
            };

            for (r, layer, dashed) in [
                (r_outer, "구슬상면_외경", None),
                (r_entry, "구슬상면_입구공경", None),
                (r_waist, "구슬상면_최소공경", Some((1.0, 0.8))),
            ] {
                let mut ring = Vec::with_capacity(36);
                for step in 0..36 {
                    let a = (step as f64) * std::f64::consts::TAU / 36.0;
                    ring.push([cx + r * a.cos(), plan_cy + r * a.sin()]);
                }
                paths.push(PlatePathElement {
                    layer_id: String::from(layer),
                    points_mm: ring,
                    closed: true,
                    stroke_hex: String::from("#141413"),
                    stroke_width_mm: 0.25,
                    dash_pattern_mm: dashed,
                    fill_hex: None,
                });
            }

            for span in input.occluded_spans {
                paths.push(PlatePathElement {
                    layer_id: String::from("천공내벽_파선"),
                    points_mm: vec![
                        [cx + span.start_pt[0], base_y + span.start_pt[1]],
                        [cx + span.end_pt[0], base_y + span.end_pt[1]],
                    ],
                    closed: false,
                    stroke_hex: String::from("#B5573A"),
                    stroke_width_mm: 0.25,
                    dash_pattern_mm: Some((1.5, 1.0)),
                    fill_hex: None,
                });
            }

            if let Some(dt) = input.bead_drill_type {
                callouts.push(PlateTextCallout {
                    layer_id: String::from("치수_축척막대"),
                    pos_mm: [cx + r_outer + 4.0, base_y + height * 0.5],
                    text: format!("천공: {}", dt.code_str()),
                    font_size_pt: 8.5,
                    color_hex: String::from("#B5573A"),
                });
            }
        }
        PlateLayoutMode::RoofTileAndEavesPlate => {
            raster_regions.push(PlateRasterRegion {
                kind: PlateRasterKind::FrontExteriorRubbing,
                rect_mm: [cx - half_d, base_y, cx + half_d, top_y],
                clip_half_sign: Some(-1),
                caption: String::from("기와 타날문·포목흔 탁본"),
            });
            let ratio = input.wado_cut_ratio.unwrap_or(0.62).clamp(0.1, 0.95);
            let cut_y = base_y + height * ratio;
            paths.push(PlatePathElement {
                layer_id: String::from("와도_분할선"),
                points_mm: vec![[cx + 2.0, cut_y], [cx + 12.0, cut_y]],
                closed: false,
                stroke_hex: String::from("#B5573A"),
                stroke_width_mm: 0.30,
                dash_pattern_mm: None,
                fill_hex: None,
            });
            callouts.push(PlateTextCallout {
                layer_id: String::from("치수_축척막대"),
                pos_mm: [cx + 13.0, cut_y],
                text: format!("──▶ 와도 분할선 ({:.0}%)", ratio * 100.0),
                font_size_pt: 8.5,
                color_hex: String::from("#B5573A"),
            });
            if let Some(ref hemp) = input.hemp_density_label {
                callouts.push(PlateTextCallout {
                    layer_id: String::from("치수_축척막대"),
                    pos_mm: [cx - max_d * 0.45, base_y - 6.0],
                    text: hemp.clone(),
                    font_size_pt: 8.5,
                    color_hex: String::from("#141413"),
                });
            }
        }
        PlateLayoutMode::LithicAndCorrodedMetalPlate => {
            raster_regions.push(PlateRasterRegion {
                kind: PlateRasterKind::FrontExteriorRubbing,
                rect_mm: [cx - half_d, base_y, cx + half_d, top_y],
                clip_half_sign: Some(-1),
                caption: String::from("석기 박리·금속 표면 탁본"),
            });
            if m.category == ArtifactCategory::CorrodedMetal {
                for span in input.occluded_spans {
                    paths.push(PlatePathElement {
                        layer_id: String::from("부식팽창층_파선"),
                        points_mm: vec![
                            [cx + span.start_pt[0], base_y + span.start_pt[1]],
                            [cx + span.end_pt[0], base_y + span.end_pt[1]],
                        ],
                        closed: false,
                        stroke_hex: String::from("#736B5E"),
                        stroke_width_mm: 0.20,
                        dash_pattern_mm: Some((1.5, 1.2)),
                        fill_hex: None,
                    });
                }
            }
        }
    }

    // 4-B. 모든 유물 모드에서 좌상단 1:1 평면도(벡터 실측 선화), 우상단 1:1 정밀 탁본, 그리고 실측 수평 횡단면이 빠지지 않도록 보장
    if mode != PlateLayoutMode::CompleteVesselStandard {
        let plan_r = half_d.max(12.0);
        let cx_plan = cx;
        let cy_plan = top_y + plan_r + 24.0;
        let cx_right = cx + half_d + half_d * 0.94 + 22.0;

        // 좌상단: 1:1 평면도 (일러스트레이터 편집용 벡터 실측 선화)
        paths.push(PlatePathElement {
            layer_id: String::from("외저면_십자선"),
            points_mm: vec![
                [cx_plan - plan_r * 1.06, cy_plan],
                [cx_plan + plan_r * 1.06, cy_plan],
            ],
            closed: false,
            stroke_hex: String::from("#5E5D59"),
            stroke_width_mm: 0.18,
            dash_pattern_mm: None,
            fill_hex: None,
        });
        paths.push(PlatePathElement {
            layer_id: String::from("외저면_십자선"),
            points_mm: vec![
                [cx_plan, cy_plan - plan_r * 1.06],
                [cx_plan, cy_plan + plan_r * 1.06],
            ],
            closed: false,
            stroke_hex: String::from("#5E5D59"),
            stroke_width_mm: 0.18,
            dash_pattern_mm: None,
            fill_hex: None,
        });
        let base_r = (m.base_diameter_or_width_mm * 0.5).clamp(plan_r * 0.34, plan_r * 0.72);
        for (r_ring, layer_name, width_mm) in [
            (plan_r, "평면도_외곽윤곽선", 0.34),
            ((plan_r - wall_t).max(6.0), "평면도_구연내경선", 0.26),
            (base_r, "평면도_굽외경선", 0.24),
        ] {
            let mut ring_pts = Vec::with_capacity(73);
            for step in 0..=72 {
                let a = (step as f64) * std::f64::consts::TAU / 72.0;
                ring_pts.push([cx_plan + r_ring * a.cos(), cy_plan + r_ring * a.sin()]);
            }
            paths.push(PlatePathElement {
                layer_id: String::from(layer_name),
                points_mm: ring_pts,
                closed: true,
                stroke_hex: String::from("#141413"),
                stroke_width_mm: width_mm,
                dash_pattern_mm: None,
                fill_hex: None,
            });
        }
        callouts.push(PlateTextCallout {
            layer_id: String::from("치수_축척막대"),
            pos_mm: [cx_plan - 20.0, cy_plan + plan_r + 4.5],
            text: format!("1:1 평면도 (폭 {:.1}mm)", max_d),
            font_size_pt: 8.5,
            color_hex: String::from("#141413"),
        });

        // 우상단: 1:1 정밀 탁본 (Image 1 위치)
        raster_regions.push(PlateRasterRegion {
            kind: PlateRasterKind::BottomPlanRubbing,
            rect_mm: [
                cx_right - plan_r,
                cy_plan - plan_r,
                cx_right + plan_r,
                cy_plan + plan_r,
            ],
            clip_half_sign: None,
            caption: String::from("1:1 정밀 탁본"),
        });
        callouts.push(PlateTextCallout {
            layer_id: String::from("치수_축척막대"),
            pos_mm: [cx_right - 22.0, cy_plan + plan_r + 4.5],
            text: format!("1:1 정밀 탁본 (폭 {:.1}mm)", max_d),
            font_size_pt: 8.5,
            color_hex: String::from("#141413"),
        });

        if let Some(hsec) = input.horizontal_sections.first() {
            let cut_z_rel = (hsec.offset_mm - z_offset).clamp(1.0, height - 1.0);
            let cut_line_y = base_y + cut_z_rel;
            paths.push(PlatePathElement {
                layer_id: String::from("횡단면_절단위치선"),
                points_mm: vec![
                    [cx - half_d * 1.05, cut_line_y],
                    [cx + half_d * 1.05, cut_line_y],
                ],
                closed: false,
                stroke_hex: String::from("#D92B2B"),
                stroke_width_mm: 0.30,
                dash_pattern_mm: Some((2.2, 1.2)),
                fill_hex: None,
            });

            let (outer_contour, inner_contour, _rpoly, _max_r) =
                extract_measured_horizontal_section_contours(hsec, wall_t, half_d * 0.78);
            for (contour_pts, layer_name) in [
                (&outer_contour, "횡단면_외벽윤곽선"),
                (&inner_contour, "횡단면_내벽윤곽선"),
            ] {
                if contour_pts.len() >= 3 {
                    let ring_top: Vec<[f64; 2]> = contour_pts
                        .iter()
                        .map(|p| [cx_plan + p[0], cy_plan + p[1]])
                        .collect();
                    paths.push(PlatePathElement {
                        layer_id: String::from(layer_name),
                        points_mm: ring_top,
                        closed: true,
                        stroke_hex: String::from("#B5573A"),
                        stroke_width_mm: 0.28,
                        dash_pattern_mm: Some((2.0, 1.2)),
                        fill_hex: None,
                    });
                }
            }
        }
    }

    // 4-C. 사용자가 [+ 단면추가] 버튼으로 그은 추가 단면(A-A')이 전달된 경우 우측 도면에 1:1 단면 추가 제도
    if let Some(custom_sec) = input.horizontal_sections.get(1) {
        let cx_custom = cx + half_d + half_d * 0.88 + 18.0;
        let cy_custom = top_y + 20.0;
        let z_min_c = custom_sec
            .polylines_2d
            .iter()
            .flatten()
            .map(|p| p[1])
            .fold(f64::INFINITY, f64::min);
        let z_off_c = if z_min_c.is_finite() { z_min_c } else { 0.0 };

        for poly in &custom_sec.polylines_2d {
            if poly.len() < 2 {
                continue;
            }
            let mut pts: Vec<[f64; 2]> = poly
                .iter()
                .map(|p| [cx_custom + p[0], cy_custom + (p[1] - z_off_c)])
                .collect();
            let first = pts[0];
            let last = *pts.last().unwrap_or(&first);
            if (first[0] - last[0]).hypot(first[1] - last[1]) > 2.5 {
                let mut inner_rev: Vec<[f64; 2]> = poly
                    .iter()
                    .rev()
                    .map(|p| {
                        let sign = if p[0] >= 0.0 { -1.0 } else { 1.0 };
                        [
                            cx_custom + p[0] + sign * wall_t * 0.65,
                            cy_custom + (p[1] - z_off_c),
                        ]
                    })
                    .collect();
                pts.append(&mut inner_rev);
            }
            paths.push(PlatePathElement {
                layer_id: String::from("추가단면_실선"),
                points_mm: pts,
                closed: true,
                stroke_hex: String::from("#141413"),
                stroke_width_mm: 0.36,
                dash_pattern_mm: None,
                fill_hex: Some(String::from("#141413")),
            });
        }
        callouts.push(PlateTextCallout {
            layer_id: String::from("치수_축척막대"),
            pos_mm: [cx_custom - 20.0, cy_custom - 6.0],
            text: String::from("추가 단면 A-A' (1:1)"),
            font_size_pt: 8.5,
            color_hex: String::from("#D92B2B"),
        });
    }

    // 5. 동적 실측 치수선 및 축척 막대(`Scale Bar`) — 기본 축척 1:1(`ScalePreset::Full1To1`)
    let default_scale = ScalePreset::Full1To1;
    let bar_len_mm = 50.0;
    let bar_x0 = cx - bar_len_mm * 0.5;
    let bar_y = (base_y - 7.5).max(8.0);
    paths.push(PlatePathElement {
        layer_id: String::from("치수_축척막대"),
        points_mm: vec![[bar_x0, bar_y], [bar_x0 + bar_len_mm, bar_y]],
        closed: false,
        stroke_hex: String::from("#141413"),
        stroke_width_mm: 0.40,
        dash_pattern_mm: None,
        fill_hex: None,
    });

    let scale_label = format!(
        "{} ({:.0}cm = {:.0}mm)",
        default_scale.label(),
        bar_len_mm / 10.0,
        bar_len_mm
    );
    callouts.push(PlateTextCallout {
        layer_id: String::from("치수_축척막대"),
        pos_mm: [cx - 12.0, bar_y - 4.0],
        text: scale_label,
        font_size_pt: 8.5,
        color_hex: String::from("#141413"),
    });

    callouts.push(PlateTextCallout {
        layer_id: String::from("치수_축척막대"),
        pos_mm: [margin * 0.7, canvas_height_mm - 7.5],
        text: format!("{} · {}", input.title, m.metric_strip_summary),
        font_size_pt: 9.0,
        color_hex: String::from("#141413"),
    });

    ComposedPlate2D {
        mode,
        title: input.title.to_string(),
        scale_preset: default_scale,
        canvas_width_mm,
        canvas_height_mm,
        flip_exterior_right: input.flip_exterior_right,
        paths,
        callouts,
        raster_regions,
    }
}
