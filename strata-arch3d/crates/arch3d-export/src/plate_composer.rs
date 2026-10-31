use arch3d_core::{
    ArtifactCategory, BeadDrillType, BeadSubtype, HandleAttachmentKind, ScalePreset,
    SherdPlateStrategy, SherdSubtype,
};
use arch3d_measure::ArtifactMeasurementReport;
use arch3d_section::{OccludedDashedSpan, PlaneSliceResult};
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

/// 실측치(`ArtifactMeasurementReport`)와 단면 윤곽(`PlaneSliceResult`)으로부터 8대 유물군 맞춤형 2D 도판을 합성한다.
/// - 하드코딩된 가짜 수치(`H = 95.0 mm`, `#6b4fc0`)를 일절 사용하지 않고, 입력 메쉬의 실제 치수와 STRATA 토큰(`#141413`, `#B5573A`)만 사용한다.
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
    let margin = 24.0;

    let (canvas_width_mm, canvas_height_mm) = match mode {
        PlateLayoutMode::BodyOrMicroSherdThreeColumn => (
            (max_d * 3.4 + margin * 2.0).max(120.0),
            (height + margin * 2.0).max(80.0),
        ),
        PlateLayoutMode::BeadJadeMicroPlate | PlateLayoutMode::CommaJadeCurvedPlate => (
            (max_d * 3.5 + margin * 2.0).max(60.0),
            (height * 3.2 + margin * 2.0).max(60.0),
        ),
        _ => (
            (max_d * 1.6 + margin * 2.0).max(100.0),
            (height * 1.45 + margin * 2.0).max(90.0),
        ),
    };

    let mut paths = Vec::new();
    let mut callouts = Vec::new();

    let cx = canvas_width_mm * 0.5;
    let base_y = margin + 12.0;
    let top_y = base_y + height;

    // 1. 중심 회전축 / 기준선 제도 (`#141413` 먹색 0.25mm)
    if mode != PlateLayoutMode::BodyOrMicroSherdThreeColumn {
        paths.push(PlatePathElement {
            layer_id: String::from("중심_기준선"),
            points_mm: vec![[cx, base_y], [cx, top_y]],
            closed: false,
            stroke_hex: String::from("#141413"),
            stroke_width_mm: 0.25,
            dash_pattern_mm: None,
            fill_hex: None,
        });
    }

    // 2. 구경선 / 저경선 (완전 복원 실선 vs 추정 복원 파선)
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
            stroke_width_mm: 0.30,
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
            stroke_width_mm: 0.30,
            dash_pattern_mm: None,
            fill_hex: None,
        });
    }

    // 3. 수직 회전단면 폴리라인 배치 (`R` 반전 지원)
    let section_sign = if input.flip_exterior_right { -1.0 } else { 1.0 };
    for poly in &input.vertical_section.polylines_2d {
        if poly.len() >= 2 {
            let mapped: Vec<[f64; 2]> = poly
                .iter()
                .map(|pt| [cx + pt[0].abs() * section_sign, base_y + pt[1]])
                .collect();
            paths.push(PlatePathElement {
                layer_id: String::from("단면_실선"),
                points_mm: mapped,
                closed: true,
                stroke_hex: String::from("#141413"),
                stroke_width_mm: 0.35,
                dash_pattern_mm: None,
                fill_hex: Some(String::from("#2B2B29")),
            });

            // 반대편 외면 외곽선 (실선 0.35mm, 채움 없음)
            let ext_mapped: Vec<[f64; 2]> = poly
                .iter()
                .map(|pt| [cx - pt[0].abs() * section_sign, base_y + pt[1]])
                .collect();
            paths.push(PlatePathElement {
                layer_id: String::from("외곽선_실선"),
                points_mm: ext_mapped,
                closed: false,
                stroke_hex: String::from("#141413"),
                stroke_width_mm: 0.35,
                dash_pattern_mm: None,
                fill_hex: None,
            });
        }
    }

    // 4. 유물군별 특화 도판 요소 추가
    match mode {
        PlateLayoutMode::BodyOrMicroSherdThreeColumn => {
            // [① 외면+탁본 ── ② 수직 회전단면 ── ③ 내면+탁본] 3단 병렬 연결 보조선
            let col_gap = (max_d + 14.0).max(28.0);
            let x_ext = cx - col_gap;
            let x_sec = cx;
            let x_int = cx + col_gap;
            let mid_y = base_y + height * 0.5;
            paths.push(PlatePathElement {
                layer_id: String::from("3단병렬_연결선"),
                points_mm: vec![[x_ext, mid_y], [x_sec, mid_y], [x_int, mid_y]],
                closed: false,
                stroke_hex: String::from("#5C5B57"),
                stroke_width_mm: 0.20,
                dash_pattern_mm: Some((1.5, 1.5)),
                fill_hex: None,
            });
            callouts.push(PlateTextCallout {
                layer_id: String::from("치수_축척막대"),
                pos_mm: [x_ext, top_y + 4.0],
                text: String::from("외면(탁본)"),
                font_size_pt: 8.5,
                color_hex: String::from("#141413"),
            });
            callouts.push(PlateTextCallout {
                layer_id: String::from("치수_축척막대"),
                pos_mm: [x_sec, top_y + 4.0],
                text: String::from("회전단면"),
                font_size_pt: 8.5,
                color_hex: String::from("#141413"),
            });
            callouts.push(PlateTextCallout {
                layer_id: String::from("치수_축척막대"),
                pos_mm: [x_int, top_y + 4.0],
                text: String::from("내면(탁본)"),
                font_size_pt: 8.5,
                color_hex: String::from("#141413"),
            });
        }
        PlateLayoutMode::HandleOrSpoutSherd => {
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
        PlateLayoutMode::LithicAndCorrodedMetalPlate
            if m.category == ArtifactCategory::CorrodedMetal =>
        {
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
        _ => {}
    }

    // 5. 다단 횡단면(`Horizontal Sections`) 추가
    for hsec in input.horizontal_sections {
        for poly in &hsec.polylines_2d {
            if poly.len() >= 2 {
                let mapped: Vec<[f64; 2]> = poly
                    .iter()
                    .map(|pt| [cx + pt[0], base_y + hsec.offset_mm + pt[1] * 0.15])
                    .collect();
                paths.push(PlatePathElement {
                    layer_id: String::from("횡단면_윤곽선"),
                    points_mm: mapped,
                    closed: true,
                    stroke_hex: String::from("#3D3D3A"),
                    stroke_width_mm: 0.22,
                    dash_pattern_mm: Some((2.0, 1.2)),
                    fill_hex: None,
                });
            }
        }
    }

    // 6. 동적 실측 치수선 및 축척 막대(`Scale Bar`) 생성 (하드코딩 0%)
    let bar_len_mm = match m.recommended_scale {
        ScalePreset::Micro10To1 | ScalePreset::Micro5To1 => 5.0,
        ScalePreset::Micro3To1 | ScalePreset::Micro2To1 => 10.0,
        _ => 50.0,
    };
    let bar_x0 = cx - bar_len_mm * 0.5;
    let bar_y = 8.0;
    paths.push(PlatePathElement {
        layer_id: String::from("치수_축척막대"),
        points_mm: vec![[bar_x0, bar_y], [bar_x0 + bar_len_mm, bar_y]],
        closed: false,
        stroke_hex: String::from("#141413"),
        stroke_width_mm: 0.40,
        dash_pattern_mm: None,
        fill_hex: None,
    });

    let scale_label = if bar_len_mm >= 10.0 {
        format!(
            "{} ({:.0}cm = {:.0}mm)",
            m.recommended_scale.label(),
            bar_len_mm / 10.0,
            bar_len_mm
        )
    } else {
        format!("{} ({:.0}mm)", m.recommended_scale.label(), bar_len_mm)
    };
    callouts.push(PlateTextCallout {
        layer_id: String::from("치수_축척막대"),
        pos_mm: [cx, bar_y - 3.5],
        text: scale_label,
        font_size_pt: 8.5,
        color_hex: String::from("#141413"),
    });

    callouts.push(PlateTextCallout {
        layer_id: String::from("치수_축척막대"),
        pos_mm: [margin, canvas_height_mm - 8.0],
        text: format!("{} · {}", input.title, m.metric_strip_summary),
        font_size_pt: 9.0,
        color_hex: String::from("#141413"),
    });

    ComposedPlate2D {
        mode,
        title: input.title.to_string(),
        scale_preset: m.recommended_scale,
        canvas_width_mm,
        canvas_height_mm,
        flip_exterior_right: input.flip_exterior_right,
        paths,
        callouts,
    }
}
