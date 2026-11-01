use strata_core::strata_tokens::tokens::{
    ACTION, DONE_BG, DONE_INK, EDGE, GRID_1MM, GROUND, INK, LINE, MUTED, PROFILE_RED, SELECT_BLUE,
    SURFACE, WASH,
};
use strata_core::{ScalePreset, TriMesh};
use strata_export::{
    triangulate_polygon_ear_clipping, ComposedPlate2D, PlateRasterKind,
};
use strata_measure::ArtifactMeasurementReport;
use eframe::egui;
use image::{Rgba, RgbaImage};
use rayon::prelude::*;

#[inline]
#[allow(dead_code)]
fn sample_hanji_paper_grain(x: u32, y: u32, amplitude: f32) -> f32 {
    let mut h1 = x.wrapping_mul(0x9E37_79B9) ^ y.wrapping_mul(0x85EB_CA6B);
    h1 ^= h1 >> 16;
    h1 = h1.wrapping_mul(0x7FEB_352D);
    h1 ^= h1 >> 15;
    let mut h2 = (x / 3).wrapping_mul(0xC2B2_AE35) ^ (y / 2).wrapping_mul(0x27D4_EB2F);
    h2 ^= h2 >> 16;
    let f1 = (h1 & 0xFFFF) as f32 / 65535.0;
    let f2 = (h2 & 0xFFFF) as f32 / 65535.0;
    ((0.6 * f1 + 0.4 * f2) * 2.0 - 1.0) * amplitude
}

/// 학술 논문 기반 디지털 정밀 탁본 기법 (Mara & Krömker 2010 MSII + 4-Scale Integral DoG + Weingarten Principal Curvature + Wet-Hanji Contact Envelope).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AcademicRubbingStyle {
    /// 전통 한지 습탁 먹물 기법 (젖은 한지가 음각 홈에서 뜨고 양각 능선에 먹물이 깊게 스며듦)
    WetHanjiInk,
    /// 건탁 석채 기법 (미세 양각 모서리와 물레속선 중심의 건조 마찰 탁본)
    DryGraphite,
    /// 다중스케일 MSII 다중스케일 곡률 능선 강조 기법 (인화문·명문·타날문 판독 특화)
    MsiiRidgeEnhance,
}

impl AcademicRubbingStyle {
    pub fn label_kr(self) -> &'static str {
        match self {
            Self::WetHanjiInk => "습탁 먹물(Wet Hanji)",
            Self::DryGraphite => "건탁 석채(Dry)",
            Self::MsiiRidgeEnhance => "MSII 능선강조(논문)",
        }
    }
}

/// 탁본 이미지 바로 밑에 배치되는 4종 미세조정 휠 종류 (`Image 14`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RubbingFineWheelKind {
    /// ① 먹물 농도 (`ink_density_pct`: `15% ~ 98%`)
    InkDensity,
    /// ② 음양각 대비 (`sigmoid_contrast`: `2.0 ~ 16.0`)
    SigmoidContrast,
    /// ③ 문양·능선 선명도 (`msii_detail_boost`: `0.4× ~ 3.0×`)
    MsiiRidgeSharpness,
    /// ④ 한지 여백 임계 (`contact_threshold`: `-0.40 ~ +0.25`)
    PaperContactThreshold,
}

/// 학술 디지털 탁본 파라미터 (`탁본 이미지 바로 밑 4종 미세조정 휠` 및 `부분 정밀 탁본 창` 공용).
#[derive(Debug, Clone, PartialEq)]
pub struct AcademicRubbingParams {
    /// ① 먹물 농도 (`15.0 ..= 98.0 %`)
    pub ink_density_pct: f64,
    /// ③ 다중스케일 곡률(MSII + 4-Scale DoG r=1,2,4,8) 능선 선명도 배율 (`0.4 ..= 3.0`)
    pub msii_detail_boost: f32,
    /// ② 비선형 시그모이드(Sigmoid) 먹물 전이 곡선 대비 기울기 (`2.0 ..= 16.0`)
    pub sigmoid_contrast: f32,
    /// ④ 습탁 한지 접촉 포락선 임계값 (`-0.40 ..= 0.25`, 오목한 음각 홈 여백 제어)
    pub contact_threshold: f32,
    /// 탁본 표현 기법
    pub style: AcademicRubbingStyle,
    /// Image 1 ("전부 방향이 같아야한다 탁복한 유물과 방향이다르다"): 3D 유물 상하 회전(`tilt_deg`) 연동
    pub pose_tilt_deg: f64,
    /// Image 1: 3D 유물 좌우 기울기(`roll_deg + inner_level_deg`) 연동
    pub pose_roll_deg: f64,
    /// Image 2 ("광원을 조정할수있는 작은창"): 광원 방위각(`0..360°`)
    pub light_azimuth_deg: f32,
    /// Image 2: 광원 고도각(`5..85°`)
    pub light_elevation_deg: f32,
    /// Image 2: 광원 세기(`0.40..1.80`)
    pub light_intensity: f32,
    /// 곡면 전개 탁본 모드 (`None` 이면 기존 평면/정면 탁본, `Some` 이면 0~360° 전개)
    pub unroll_mode: Option<strata_shade::UnrollMode>,
}

impl Default for AcademicRubbingParams {
    fn default() -> Self {
        Self {
            ink_density_pct: 78.0,
            msii_detail_boost: 1.45,
            sigmoid_contrast: 8.2,
            contact_threshold: -0.12,
            style: AcademicRubbingStyle::WetHanjiInk,
            pose_tilt_deg: 0.0,
            pose_roll_deg: 0.0,
            light_azimuth_deg: 315.0,
            light_elevation_deg: 45.0,
            light_intensity: 1.0,
            unroll_mode: None,
        }
    }
}

impl AcademicRubbingParams {
    /// 탁본 이미지 바로 밑의 4개 미세조정 휠 조작을 적용한다 (`Image 14`).
    pub fn apply_wheel_step(&mut self, kind: RubbingFineWheelKind, steps: i32) {
        let s = f32::from(steps as i16);
        match kind {
            RubbingFineWheelKind::InkDensity => {
                self.ink_density_pct =
                    (self.ink_density_pct + f64::from(steps) * 1.0).clamp(15.0, 98.0);
            }
            RubbingFineWheelKind::SigmoidContrast => {
                self.sigmoid_contrast =
                    ((self.sigmoid_contrast + s * 0.2).clamp(2.0, 16.0) * 10.0).round() / 10.0;
            }
            RubbingFineWheelKind::MsiiRidgeSharpness => {
                self.msii_detail_boost =
                    ((self.msii_detail_boost + s * 0.05).clamp(0.4, 3.0) * 100.0).round() / 100.0;
            }
            RubbingFineWheelKind::PaperContactThreshold => {
                self.contact_threshold =
                    ((self.contact_threshold + s * 0.01).clamp(-0.40, 0.25) * 100.0).round()
                        / 100.0;
            }
        }
    }
}

/// 도판 위 모든 제도선(단면선·외곽선·횡단면/축선·치수선)의 `mm` 굵기 및 색상 즉시 조정 설정 (`Image 11`, `Image 15`).
#[derive(Debug, Clone, PartialEq)]
pub struct PlateLineStyleConfig {
    /// Image 11: 단면을 검게 채우지 않고 선(외곽선)으로만 표시 (`기본값 = false`)
    pub fill_section_solid: bool,
    /// 수직 단면선 굵기 (`mm`, 기본 `0.40mm`)
    pub section_width_mm: f32,
    /// 수직 단면선 색상 (`RGB`, 기본 `#141413` 먹색)
    pub section_color_rgb: [u8; 3],
    /// 외곽·구경·저경 실측선 굵기 (`mm`, 기본 `0.35mm`)
    pub outer_width_mm: f32,
    /// 외곽·구경·저경 실측선 색상 (`RGB`, 기본 `#141413` 먹색)
    pub outer_color_rgb: [u8; 3],
    /// 수평 횡단면·중심 기준선 굵기 (`mm`, 기본 `0.25mm`)
    pub aux_width_mm: f32,
    /// 수평 횡단면·중심 기준선 색상 (`RGB`, 기본 `#B5573A` 테라코타)
    pub aux_color_rgb: [u8; 3],
    /// 치수 화살표·인출선 굵기 (`mm`, 기본 `0.22mm`)
    pub dim_width_mm: f32,
    /// 치수 화살표·인출선 색상 (`RGB`, 기본 `#2A2A28`)
    pub dim_color_rgb: [u8; 3],
}

impl Default for PlateLineStyleConfig {
    fn default() -> Self {
        Self {
            fill_section_solid: false, // Image 11: "검게 나오게 하지말고 선으로 나와야한다" -> 기본값 선(Outline) 모드!
            // 「유물제도선.ai」 규격(0.12/0.15/0.2/0.3/0.4/0.5/0.8)에 맞춘 기본 위계
            section_width_mm: 0.40,
            section_color_rgb: [20, 20, 19],
            outer_width_mm: 0.50,
            outer_color_rgb: [20, 20, 19],
            aux_width_mm: 0.20,
            aux_color_rgb: [181, 87, 58],
            dim_width_mm: 0.15,
            dim_color_rgb: [42, 42, 40],
        }
    }
}

/// 실무 규격 선 굵기(mm) 프리셋 — 「유물제도선.ai」 기준.
pub const STANDARD_LINE_WEIGHTS_MM: [f32; 7] = [0.12, 0.15, 0.20, 0.30, 0.40, 0.50, 0.80];

/// 임의의 굵기를 규격값 중 가장 가까운 값으로 스냅한다.
pub fn snap_to_standard_weight_mm(w: f32) -> f32 {
    let mut best = STANDARD_LINE_WEIGHTS_MM[0];
    let mut best_d = f32::INFINITY;
    for &cand in &STANDARD_LINE_WEIGHTS_MM {
        let d = (cand - w).abs();
        if d < best_d {
            best_d = d;
            best = cand;
        }
    }
    best
}

/// 조판(Publication Page Layout) 용지 규격 프리셋 (`Image 3: 용지설정을 할수있게하고 용지안에만 방안선을 하라 5cm마다 얇은빨간색선`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaperSheetPreset {
    A4Portrait,
    A4Landscape,
    A3Landscape,
    B4Portrait,
    B5Portrait,
}

impl PaperSheetPreset {
    pub fn dimensions_mm(self) -> (f64, f64) {
        match self {
            Self::A4Portrait => (210.0, 297.0),
            Self::A4Landscape => (297.0, 210.0),
            Self::A3Landscape => (420.0, 297.0),
            Self::B4Portrait => (250.0, 353.0),
            Self::B5Portrait => (176.0, 250.0),
        }
    }

    pub fn label_kr(self) -> &'static str {
        match self {
            Self::A4Portrait => "A4 세로",
            Self::A4Landscape => "A4 가로",
            Self::A3Landscape => "A3 가로",
            Self::B4Portrait => "B4 세로",
            Self::B5Portrait => "B5 세로",
        }
    }
}

/// 조판 용지(`paper_bounds_rect`) 내부 전용 방안선 및 `5cm`(`50mm`) 적색 기준선 명세 (`Image 3`).
#[derive(Debug, Clone)]
pub struct PaperSheetGraphSpec {
    pub paper_bounds_rect: egui::Rect,
    pub origin_px: egui::Pos2,
    pub scale_px_per_mm: f32,
    pub lines_1mm_x: Vec<f32>,
    pub lines_1mm_y: Vec<f32>,
    pub lines_5mm_x: Vec<f32>,
    pub lines_5mm_y: Vec<f32>,
    pub lines_10mm_x: Vec<f32>,
    pub lines_10mm_y: Vec<f32>,
    pub lines_50mm_red_x: Vec<f32>,
    pub lines_50mm_red_y: Vec<f32>,
    pub cm_labels_top: Vec<(f32, String)>,
    pub cm_labels_left: Vec<(f32, String)>,
}

/// 방안지 여백은 무한대로 배경을 채우고(`sheet_rect` 전체), 출판 용지 경계(`paper_bounds_rect`)만 붉은선으로 표시한다.
pub fn compute_paper_sheet_graph_spec(
    sheet_rect: egui::Rect,
    paper_preset: PaperSheetPreset,
    zoom: f32,
    pan_px: egui::Vec2,
) -> PaperSheetGraphSpec {
    let (pw_mm_f64, ph_mm_f64) = paper_preset.dimensions_mm();
    let pw_mm = pw_mm_f64 as f32;
    let ph_mm = ph_mm_f64 as f32;

    let avail_w = (sheet_rect.width() - 48.0).max(120.0);
    let avail_h = (sheet_rect.height() - 146.0).max(120.0);
    let base_scale = (avail_w / pw_mm).min(avail_h / ph_mm).max(0.8);
    let scale = (base_scale * zoom).clamp(0.5, 8.0);

    let paper_w_px = pw_mm * scale;
    let paper_h_px = ph_mm * scale;
    let center = egui::pos2(sheet_rect.center().x, sheet_rect.center().y - 1.0) + pan_px;
    let paper_bounds_rect =
        egui::Rect::from_center_size(center, egui::vec2(paper_w_px, paper_h_px));
    let origin_x = paper_bounds_rect.left();
    let origin_y = paper_bounds_rect.bottom();

    let mut lines_1mm_x = Vec::new();
    let mut lines_1mm_y = Vec::new();
    let mut lines_5mm_x = Vec::new();
    let mut lines_5mm_y = Vec::new();
    let mut lines_10mm_x = Vec::new();
    let mut lines_10mm_y = Vec::new();
    let mut lines_50mm_red_x = Vec::new();
    let mut lines_50mm_red_y = Vec::new();
    let mut cm_labels_top = Vec::new();
    let mut cm_labels_left = Vec::new();

    // "방안지 여백은 무한대로 배경을 하고 용지만 붉은선으로 표시하게 하자":
    // sheet_rect 전체 영역에 걸쳐 무한대 방안지(1mm·5mm·10mm·50mm) 격자를 생성한다!
    let min_mm_x = ((sheet_rect.left() - origin_x) / scale).floor() as i32;
    let max_mm_x = ((sheet_rect.right() - origin_x) / scale).ceil() as i32;
    for mm_x in min_mm_x..=max_mm_x {
        let gx = origin_x + (mm_x as f32) * scale;
        if gx < sheet_rect.left() - 0.5 || gx > sheet_rect.right() + 0.5 {
            continue;
        }
        if mm_x.rem_euclid(50) == 0 {
            lines_50mm_red_x.push(gx);
            lines_10mm_x.push(gx);
            if mm_x >= 0 && (mm_x as f32) <= pw_mm + 0.5 {
                cm_labels_top.push((gx, format!("{}cm", mm_x / 10)));
            }
        } else if mm_x.rem_euclid(10) == 0 {
            lines_10mm_x.push(gx);
            if mm_x >= 0 && (mm_x as f32) <= pw_mm + 0.5 {
                cm_labels_top.push((gx, format!("{}cm", mm_x / 10)));
            }
        } else if mm_x.rem_euclid(5) == 0 {
            lines_5mm_x.push(gx);
        } else if scale >= 1.8 {
            lines_1mm_x.push(gx);
        }
    }

    let min_mm_y = ((origin_y - sheet_rect.bottom()) / scale).floor() as i32;
    let max_mm_y = ((origin_y - sheet_rect.top()) / scale).ceil() as i32;
    for mm_y in min_mm_y..=max_mm_y {
        let gy = origin_y - (mm_y as f32) * scale;
        if gy < sheet_rect.top() - 0.5 || gy > sheet_rect.bottom() + 0.5 {
            continue;
        }
        if mm_y.rem_euclid(50) == 0 {
            lines_50mm_red_y.push(gy);
            lines_10mm_y.push(gy);
            if mm_y >= 0 && (mm_y as f32) <= ph_mm + 0.5 {
                cm_labels_left.push((gy, format!("{}cm", mm_y / 10)));
            }
        } else if mm_y.rem_euclid(10) == 0 {
            lines_10mm_y.push(gy);
            if mm_y >= 0 && (mm_y as f32) <= ph_mm + 0.5 {
                cm_labels_left.push((gy, format!("{}cm", mm_y / 10)));
            }
        } else if mm_y.rem_euclid(5) == 0 {
            lines_5mm_y.push(gy);
        } else if scale >= 1.8 {
            lines_1mm_y.push(gy);
        }
    }

    PaperSheetGraphSpec {
        paper_bounds_rect,
        origin_px: egui::pos2(origin_x, origin_y),
        scale_px_per_mm: scale,
        lines_1mm_x,
        lines_1mm_y,
        lines_5mm_x,
        lines_5mm_y,
        lines_10mm_x,
        lines_10mm_y,
        lines_50mm_red_x,
        lines_50mm_red_y,
        cm_labels_top,
        cm_labels_left,
    }
}

/// همه 선 조정 위젯(`단면선`, `외곽선`, `횡단면`, `치수선`)이 유물 이동 시 함께 움직이고,
/// 줌인·줌아웃 시에도 절대 서로 겹치지 않으며 도면 용지(`p_rect`) 내부를 가리지 않도록 상단 도크 좌표를 산출한다 (`Image 2`, `Image 4`).
pub fn compute_non_overlapping_line_widget_rects(
    origin_px: egui::Pos2,
    scale_px_per_mm: f32,
    vessel_rect_mm: [f64; 4],
    artifact_offset_mm: [f64; 3],
    viewport_rect: egui::Rect,
) -> [egui::Rect; 4] {
    let w_px = 172.0_f32;
    let h_px = 25.0_f32;
    let min_gap_x = 8.0_f32;
    let step_x = w_px + min_gap_x;

    let v_min_x_mm = vessel_rect_mm[0].min(vessel_rect_mm[2]) + artifact_offset_mm[0];
    let vessel_left_px = origin_px.x + (v_min_x_mm as f32) * scale_px_per_mm;

    let total_w = 4.0 * w_px + 3.0 * min_gap_x;
    let min_left = viewport_rect.left() + 12.0;
    let max_left = (viewport_rect.right() - total_w - 12.0).max(min_left);
    let start_x = (vessel_left_px - 140.0).clamp(min_left, max_left);
    let top_y = (viewport_rect.top() + 36.0).min(viewport_rect.bottom() - h_px - 4.0);

    [
        egui::Rect::from_min_size(egui::pos2(start_x, top_y), egui::vec2(w_px, h_px)),
        egui::Rect::from_min_size(egui::pos2(start_x + step_x, top_y), egui::vec2(w_px, h_px)),
        egui::Rect::from_min_size(
            egui::pos2(start_x + 2.0 * step_x, top_y),
            egui::vec2(w_px, h_px),
        ),
        egui::Rect::from_min_size(
            egui::pos2(start_x + 3.0 * step_x, top_y),
            egui::vec2(w_px, h_px),
        ),
    ]
}

/// 화면 전체(`Full-Bleed`) 실측 방안지 격자 및 `cm` 눈금자 명세 (`Todo 5`).
#[derive(Debug, Clone)]
pub struct FullBleedGraphPaperSpec {
    pub paper_rect: egui::Rect,
    pub origin_px: egui::Pos2,
    pub scale_px_per_mm: f32,
    pub lines_1mm_x: Vec<f32>,
    pub lines_1mm_y: Vec<f32>,
    pub lines_5mm_x: Vec<f32>,
    pub lines_5mm_y: Vec<f32>,
    pub lines_10mm_x: Vec<f32>,
    pub lines_10mm_y: Vec<f32>,
    pub cm_labels_top: Vec<(f32, String)>,
    pub cm_labels_left: Vec<(f32, String)>,
}

/// 우측 2D 도판 영역(`sheet_rect`) 전체를 여백 없는 100% 실제 고고학 실측 방안지(1mm·5mm·10mm 격자 + 상/좌 cm 눈금자)로 계산한다.
pub fn compute_full_bleed_graph_paper_spec(
    sheet_rect: egui::Rect,
    canvas_width_mm: f64,
    canvas_height_mm: f64,
    zoom: f32,
    pan_px: egui::Vec2,
) -> FullBleedGraphPaperSpec {
    let cw = (canvas_width_mm.max(10.0)) as f32;
    let ch = (canvas_height_mm.max(10.0)) as f32;
    let base_scale = ((sheet_rect.width() - 48.0) / cw)
        .min((sheet_rect.height() - 56.0) / ch)
        .max(1.2);
    let scale = (base_scale * zoom).max(0.8);

    let plate_w = cw * scale;
    let plate_h = ch * scale;
    let center = sheet_rect.center() + pan_px;
    let origin_x = center.x - plate_w * 0.5;
    let origin_y = center.y + plate_h * 0.5;

    let mut lines_1mm_x = Vec::new();
    let mut lines_1mm_y = Vec::new();
    let mut lines_5mm_x = Vec::new();
    let mut lines_5mm_y = Vec::new();
    let mut lines_10mm_x = Vec::new();
    let mut lines_10mm_y = Vec::new();
    let mut cm_labels_top = Vec::new();
    let mut cm_labels_left = Vec::new();

    let min_mm_x = ((sheet_rect.left() - origin_x) / scale).floor() as i32;
    let max_mm_x = ((sheet_rect.right() - origin_x) / scale).ceil() as i32;
    for mm_x in min_mm_x..=max_mm_x {
        let gx = origin_x + (mm_x as f32) * scale;
        if gx < sheet_rect.left() || gx > sheet_rect.right() {
            continue;
        }
        if mm_x % 10 == 0 {
            lines_10mm_x.push(gx);
            cm_labels_top.push((gx, format!("{}cm", mm_x / 10)));
        } else if mm_x % 5 == 0 {
            lines_5mm_x.push(gx);
        } else if scale >= 2.2 {
            lines_1mm_x.push(gx);
        }
    }

    let min_mm_y = ((origin_y - sheet_rect.bottom()) / scale).floor() as i32;
    let max_mm_y = ((origin_y - sheet_rect.top()) / scale).ceil() as i32;
    for mm_y in min_mm_y..=max_mm_y {
        let gy = origin_y - (mm_y as f32) * scale;
        if gy < sheet_rect.top() || gy > sheet_rect.bottom() {
            continue;
        }
        if mm_y % 10 == 0 {
            lines_10mm_y.push(gy);
            cm_labels_left.push((gy, format!("{}cm", mm_y / 10)));
        } else if mm_y % 5 == 0 {
            lines_5mm_y.push(gy);
        } else if scale >= 2.2 {
            lines_1mm_y.push(gy);
        }
    }

    FullBleedGraphPaperSpec {
        paper_rect: sheet_rect,
        origin_px: egui::pos2(origin_x, origin_y),
        scale_px_per_mm: scale,
        lines_1mm_x,
        lines_1mm_y,
        lines_5mm_x,
        lines_5mm_y,
        lines_10mm_x,
        lines_10mm_y,
        cm_labels_top,
        cm_labels_left,
    }
}

/// Image 7 & 레이어 관리 ("레이어 모든것에는 방향조정 90도씩 delete키로 지우기 추가단면선 생성시 이것도 레이어에생성 삭제가능하고 키고끌수있게"):
/// 우측 2D 도판의 6개 도면 레이어(`0: ① 1:1 평면도`, `1: ② 1:1 정면 입·단면도`, `2: ③ 1:1 정밀 탁본`, `3: ④ 1:1 수평 횡단면`, `4: ⑤ 추가 단면 A-A'`, `5: ⑥ 치수선·축척자`)를
/// 각각 개별로 켜기/끄기(`visible`), `90° 회전`, `180° 상하뒤집기`, `앞으로 연속뒤집기`, 위치 이동(`offset_mm`), `Delete` 키로 삭제할 수 있는 상태.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IndividualViewTransform2D {
    /// 레이어 표시 여부 (`켜고 끌 수 있게`, 기본 `true`; `Delete` 키 삭제 시 `false`)
    pub visible: bool,
    /// 평면 내 시계방향 90° 회전 단계 (`0..4` -> `0°, 90°, 180°, 270°`)
    pub rot_90_steps: u8,
    /// 앞으로 연속 뒤집기 단계 (`0..4` -> 누를 때마다 90°씩 앞으로 연속 뒤집힘)
    pub flip_forward_steps: u8,
    /// 개별 레이어 자유 배치 오프셋 X (`0.1mm` 단위 정수)
    pub offset_mm_x_tenths: i32,
    /// 개별 레이어 자유 배치 오프셋 Y (`0.1mm` 단위 정수)
    pub offset_mm_y_tenths: i32,
}

impl Default for IndividualViewTransform2D {
    fn default() -> Self {
        Self {
            visible: true,
            rot_90_steps: 0,
            flip_forward_steps: 0,
            offset_mm_x_tenths: 0,
            offset_mm_y_tenths: 0,
        }
    }
}

impl IndividualViewTransform2D {
    /// 시계방향 90° 개별 회전 (`↻90°`)
    pub fn rotate_cw_90(&mut self) {
        self.rot_90_steps = (self.rot_90_steps + 1) % 4;
    }

    /// 180° 상하 뒤집기 (`↕180°`)
    pub fn flip_180(&mut self) {
        self.flip_forward_steps = (self.flip_forward_steps + 2) % 4;
    }

    /// 누를 때마다 앞으로 90°씩 연속 뒤집기 (`↻앞으로연속: 0° -> 90° -> 180° -> 270° -> 0°`)
    pub fn flip_forward_continuous_90(&mut self) {
        self.flip_forward_steps = (self.flip_forward_steps + 1) % 4;
    }

    /// 개별 레이어 자유 배치 오프셋 (`mm`)
    #[inline]
    pub fn offset_mm(&self) -> [f64; 2] {
        [
            f64::from(self.offset_mm_x_tenths) * 0.1,
            f64::from(self.offset_mm_y_tenths) * 0.1,
        ]
    }

    /// 개별 레이어 자유 이동 (`dx_mm`, `dy_mm`)
    pub fn translate_mm(&mut self, dx_mm: f64, dy_mm: f64) {
        self.offset_mm_x_tenths += (dx_mm * 10.0).round() as i32;
        self.offset_mm_y_tenths += (dy_mm * 10.0).round() as i32;
    }

    /// 2D 도면 좌표 `pt_mm`을 해당 개별 뷰의 중심 `center_mm` 기준으로 회전·뒤집기 및 오프셋 변환한다.
    #[inline]
    pub fn transform_point_around(&self, pt_mm: [f64; 2], center_mm: [f64; 2]) -> [f64; 2] {
        let mut dx = pt_mm[0] - center_mm[0];
        let mut dy = pt_mm[1] - center_mm[1];
        // 앞으로 연속 뒤집기: 홀수 스텝(90°/270°)은 상하 반전 + 축 교차, 2스텝(180°)은 상하 반전(dy = -dy)
        match self.flip_forward_steps % 4 {
            1 => {
                dy = -dy;
                std::mem::swap(&mut dx, &mut dy);
            }
            2 => {
                dy = -dy;
            }
            3 => {
                dx = -dx;
                std::mem::swap(&mut dx, &mut dy);
            }
            _ => {}
        }
        // 시계방향 90° 회전 적용
        let (rx, ry) = match self.rot_90_steps % 4 {
            1 => (dy, -dx),
            2 => (-dx, -dy),
            3 => (-dy, dx),
            _ => (dx, dy),
        };
        let off = self.offset_mm();
        [center_mm[0] + rx + off[0], center_mm[1] + ry + off[1]]
    }
}

/// 6개 도판 레이어 명칭 반환
pub fn sheet_layer_title_kr(layer_idx: usize) -> &'static str {
    match layer_idx {
        0 => "① 1:1 평면도",
        1 => "② 1:1 단면·입면도",
        2 => "③ 1:1 정밀 탁본",
        3 => "④ 1:1 수평 횡단면",
        4 => "⑤ 추가 단면 A-A'",
        _ => "⑥ 치수선·축척자",
    }
}

/// 각 `PlatePathElement`의 `layer_id`로부터 6개 레이어 인덱스(`0..6`)를 판별한다.
pub fn classify_plate_path_layer_idx(layer_id: &str) -> usize {
    if layer_id.starts_with("추가단면_") {
        4
    } else if layer_id.starts_with("평면도_") {
        0
    } else if layer_id.starts_with("탁본_") {
        2
    } else if layer_id.starts_with("횡단면_") {
        3
    } else if layer_id.starts_with("치수_") || layer_id == "축척자_5cm" {
        5
    } else {
        1
    }
}

type SheetRubbingCacheKey = (
    (String, i32, i32, i32, i32, i32, i32),
    (
        i32,
        i32,
        i32,
        i32,
        AcademicRubbingStyle,
        bool,
        Option<[i32; 4]>,
        IndividualViewTransform2D,
        Option<strata_shade::UnrollMode>,
    ),
);

/// 우측 2D 실측 도판 뷰포트(`SheetPane`)의 줌·팬, 드래그 `1:1` 부분 탁본(`ROI`), 선 굵기(`mm`)·색상 설정 및 정밀 탁본 캐시 상태.
#[derive(Clone)]
pub struct SheetPaneState {
    pub zoom: f32,
    pub pan_px: egui::Vec2,
    /// Image 3: 조판(Publication Page Layout) 용지 규격 설정 (기본 A3 가로)
    pub paper_preset: PaperSheetPreset,
    /// 우측 하단 도판 축척 설정 (기본 1:1 = ScalePreset::Full1To1)
    pub active_scale_preset: ScalePreset,
    /// Image 13: 사용자가 `[⬚ 부분탁본 선택]` 버튼을 눌러 탁본할 곳을 드래그하는 모드 활성화 여부
    pub roi_drag_mode: bool,
    /// 드래그 시작 지점 (`mm` 도판 좌표)
    pub roi_drag_start_mm: Option<[f64; 2]>,
    /// Image 14: 사용자가 드래그하여 지정한 부분 탁본 영역 (`Some([min_x_mm, min_y_mm, max_x_mm, max_y_mm])`)
    pub rubbing_roi_mm: Option<[f64; 4]>,
    /// 『부분 정밀 탁본 창 (ROI Rubbing Studio)』 표시 여부
    pub show_roi_rubbing_window: bool,
    /// Image 0 & Image 1: 좌측 1:1 단면도는 일러스트용 순수 벡터 실측 선화로 표시하고, 우측 상단에 1:1 탁본을 단독 표시 (`기본값 = false`)
    pub show_elevation_rubbing_overlay: bool,
    /// 6개 개별 도면 레이어(`0: 평면도`, `1: 단면도`, `2: 1:1 탁본`, `3: 수평 횡단면`, `4: 추가 단면 A-A'`, `5: 치수선·축척자`) 각각의 표시/90°회전/180°뒤집기/오프셋 상태
    pub view_transforms: [IndividualViewTransform2D; 6],
    /// 현재 선택된 도면 레이어 인덱스 (`Delete` 키로 삭제 또는 `↻90°` 회전 대상)
    pub selected_layer_idx: Option<usize>,
    /// 현재 도판에 `⑤ 추가 단면 A-A'` 레이어가 생성되어 있는지 여부
    pub has_custom_section_layer: bool,
    /// `⑤ 추가 단면 A-A'` 레이어 삭제 요청 플래그 (`main.rs`에서 `custom_section_line_mm = None` 처리)
    pub request_clear_custom_section: bool,
    /// 우측 상단 『🗂 레이어 관리』 패널 펼침 여부
    pub show_layer_panel: bool,
    /// 학술 논문 기반 정밀 탁본 파라미터 (농도·MSII 감도·시그모이드 대비·여백 임계·기법)
    pub rubbing_params: AcademicRubbingParams,
    /// Image 11 & Image 15: 모든 제도선의 색상 및 `mm` 두께 즉시 조정 상태
    pub line_style: PlateLineStyleConfig,
    /// 사용자가 하단 축척 버튼으로 직접 고른 경우 자동 축척 보정을 끈다.
    pub scale_locked: bool,
    cached_key: Option<SheetRubbingCacheKey>,
    cached_front_img: Option<RgbaImage>,
    cached_back_img: Option<RgbaImage>,
    cached_bottom_img: Option<RgbaImage>,
    cached_front_tex: Option<egui::TextureHandle>,
    cached_back_tex: Option<egui::TextureHandle>,
    cached_bottom_tex: Option<egui::TextureHandle>,
    /// 곡면 전개 탁본의 실제 물리 크기 `[둘레 mm, 높이 mm]` (`None` = 평면 탁본)
    pub unrolled_size_mm: Option<[f64; 2]>,
}

impl std::fmt::Debug for SheetPaneState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SheetPaneState")
            .field("zoom", &self.zoom)
            .field("pan_px", &self.pan_px)
            .field("paper_preset", &self.paper_preset)
            .field("active_scale_preset", &self.active_scale_preset)
            .field("roi_drag_mode", &self.roi_drag_mode)
            .field("rubbing_roi_mm", &self.rubbing_roi_mm)
            .field("show_roi_rubbing_window", &self.show_roi_rubbing_window)
            .field("show_elevation_rubbing_overlay", &self.show_elevation_rubbing_overlay)
            .field("view_transforms", &self.view_transforms)
            .field("selected_layer_idx", &self.selected_layer_idx)
            .field("has_custom_section_layer", &self.has_custom_section_layer)
            .field("rubbing_params", &self.rubbing_params)
            .field("line_style", &self.line_style)
            .finish()
    }
}

impl Default for SheetPaneState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan_px: egui::Vec2::ZERO,
            paper_preset: PaperSheetPreset::A3Landscape,
            active_scale_preset: ScalePreset::Full1To1,
            roi_drag_mode: false,
            roi_drag_start_mm: None,
            rubbing_roi_mm: None,
            show_roi_rubbing_window: false,
            show_elevation_rubbing_overlay: false,
            view_transforms: [IndividualViewTransform2D::default(); 6],
            selected_layer_idx: Some(0),
            has_custom_section_layer: false,
            request_clear_custom_section: false,
            show_layer_panel: true,
            rubbing_params: AcademicRubbingParams::default(),
            line_style: PlateLineStyleConfig::default(),
            scale_locked: false,
            cached_key: None,
            cached_front_img: None,
            cached_back_img: None,
            cached_bottom_img: None,
            cached_front_tex: None,
            cached_back_tex: None,
            cached_bottom_tex: None,
            unrolled_size_mm: None,
        }
    }
}

impl SheetPaneState {
    /// 선택된 레이어를 `Delete` 키(또는 `[🗑]` 버튼)로 삭제(숨김)한다.
    /// `⑤ 추가 단면 A-A'`(인덱스 4) 삭제 시에는 3D 뷰의 추가단면선도 함께 제거하도록 `request_clear_custom_section = true`를 설정한다.
    pub fn delete_selected_layer(&mut self) -> Option<usize> {
        let idx = self.selected_layer_idx?;
        if idx < self.view_transforms.len() {
            self.view_transforms[idx].visible = false;
            if idx == 4 {
                self.request_clear_custom_section = true;
            }
            return Some(idx);
        }
        None
    }

    /// 특정 레이어를 삭제(숨김)한다.
    pub fn delete_layer(&mut self, idx: usize) {
        if idx < self.view_transforms.len() {
            self.view_transforms[idx].visible = false;
            if idx == 4 {
                self.request_clear_custom_section = true;
            }
        }
    }

    /// 선택된 레이어를 시계방향 90° 회전한다.
    pub fn rotate_selected_layer_cw_90(&mut self) {
        if let Some(idx) = self.selected_layer_idx {
            if idx < self.view_transforms.len() {
                self.view_transforms[idx].rotate_cw_90();
            }
        }
    }

    /// 사용자가 드래그한 부분 탁본 영역(`mm`)을 설정하고, 우측 탁본 영역에 `1:1` 실제 크기로 즉시 반영한다 (`Image 13`, `Image 14`).
    pub fn set_rubbing_roi_mm(&mut self, roi_mm: Option<[f64; 4]>) {
        if let Some([x0, y0, x1, y1]) = roi_mm {
            let min_x = x0.min(x1);
            let max_x = x0.max(x1);
            let min_y = y0.min(y1);
            let max_y = y0.max(y1);
            if (max_x - min_x) >= 2.0 && (max_y - min_y) >= 2.0 {
                self.rubbing_roi_mm = Some([min_x, min_y, max_x, max_y]);
                self.cached_front_tex = None;
                self.cached_back_tex = None;
                self.cached_bottom_tex = None;
            }
        } else {
            self.rubbing_roi_mm = None;
            self.cached_front_tex = None;
            self.cached_back_tex = None;
            self.cached_bottom_tex = None;
        }
    }
}

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
fn get_or_build_ddg_mesh(
    mesh: &TriMesh,
) -> std::sync::Arc<crate::rubbing_ddg_pipeline::TopologicalDdgMesh> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    type DdgCacheMap = HashMap<
        (String, usize, usize),
        Arc<crate::rubbing_ddg_pipeline::TopologicalDdgMesh>,
    >;
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
    let built = Arc::new(crate::rubbing_ddg_pipeline::TopologicalDdgMesh::from_trimesh(
        mesh,
    ));
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
                p,
                center_xy,
                z_min,
                height_mm,
                cos_p,
                sin_p,
                cos_t,
                sin_t,
                cos_r,
                sin_r,
                0.0,
            );

            let n = ddg.normals.get(vi).copied().unwrap_or([0.0, -1.0, 0.0]);
            let [nrx, nry, nrz] = crate::view3d_pane::transform_local_normal_full(
                n,
                cos_p,
                sin_p,
                cos_t,
                sin_t,
                cos_r,
                sin_r,
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
        let cnt = sat_c[y1 * sw + x1] + sat_c[y0 * sw + x0]
            - sat_c[y0 * sw + x1]
            - sat_c[y1 * sw + x0];
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
                let msii_val = ((0.46 * curv_cancel_1_2
                    + 0.34 * curv_cancel_2_4
                    + 0.20 * curv_cancel_4_8)
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
                let ign = ((52.982_918_f32
                    * ((0.06711056 * x_f + 0.00583715 * y_f).fract()))
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
                pixels.push(egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]));
            }
        }
    }
    egui::ColorImage {
        size: [w, h],
        pixels,
    }
}

/// 탁본 이미지 바로 밑에 가로형 정밀 미세조정 휠 위젯을 그리고 드래그/클릭 스텝을 반환한다 (`Image 14`, `Image 3: 감도 +30% 향상`).
fn draw_rubbing_fine_wheel(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    wheel_rect: egui::Rect,
    id_str: &str,
    title_kr: &str,
    value_str: &str,
    phase_0_to_1: f32,
) -> i32 {
    let resp = ui.interact(
        wheel_rect,
        egui::Id::new(id_str),
        egui::Sense::click_and_drag(),
    );
    let mut steps = 0i32;
    if resp.clicked() {
        steps = 1;
    } else if resp.dragged() {
        // Image 3 ("감도를 30%올려라"): 기존 (1 / 2.2) 대비 +30% 향상된 (1.30 / 2.2) 감도 적용!
        steps = (resp.drag_delta().x * (1.30 / 2.2)).round() as i32;
    }

    let dial_fill = egui::Color32::from_rgba_unmultiplied(236, 233, 224, 242);
    let dial_stroke = egui::Color32::from_rgb(174, 170, 158);
    let terra = egui::Color32::from_rgb(ACTION.rgb[0], ACTION.rgb[1], ACTION.rgb[2]);
    let ink = egui::Color32::from_rgb(INK.rgb[0], INK.rgb[1], INK.rgb[2]);
    let tick_col = egui::Color32::from_rgb(154, 150, 140);
    let tick_major = egui::Color32::from_rgb(98, 94, 86);

    painter.rect_filled(wheel_rect, 8.0, dial_fill);
    painter.rect_stroke(
        wheel_rect,
        8.0,
        egui::Stroke::new(
            if resp.hovered() { 1.4_f32 } else { 1.0_f32 },
            if resp.hovered() { terra } else { dial_stroke },
        ),
        egui::StrokeKind::Inside,
    );

    let n_ticks = 22usize;
    for ti in 0..=n_ticks {
        let frac = ((ti as f32 + phase_0_to_1).rem_euclid(n_ticks as f32) / (n_ticks as f32))
            .clamp(0.0, 1.0);
        let tx = wheel_rect.left() + 7.0 + frac * (wheel_rect.width() - 14.0);
        let is_mid = ti == n_ticks / 2;
        let th = if is_mid { 10.0 } else if ti % 4 == 0 { 7.5 } else { 5.0 };
        painter.line_segment(
            [
                egui::pos2(tx, wheel_rect.center().y - th * 0.5),
                egui::pos2(tx, wheel_rect.center().y + th * 0.5),
            ],
            egui::Stroke::new(
                if is_mid { 1.2_f32 } else { 0.8_f32 },
                if is_mid { tick_major } else { tick_col },
            ),
        );
    }

    let lbl_rect = egui::Rect::from_min_size(
        egui::pos2(wheel_rect.left(), wheel_rect.bottom() + 1.5),
        egui::vec2(wheel_rect.width(), 13.0),
    );
    painter.rect_filled(
        lbl_rect,
        2.0,
        egui::Color32::from_rgba_unmultiplied(250, 249, 245, 225),
    );
    painter.text(
        lbl_rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("{} {}", title_kr, value_str),
        egui::FontId::proportional(8.8),
        ink,
    );

    steps
}

/// Image 4 ("아이콘형태 ui로 잘보이게하라"):
/// 단면선(0) · 외곽선(1) · 횡단면(2) · 치수선(3) 카드 좌측에 직관적인 고시인성 벡터 아이콘을 제도한다.
fn draw_line_style_vector_icon(
    painter: &egui::Painter,
    icon_rect: egui::Rect,
    idx: usize,
    stroke_col: egui::Color32,
) {
    let bg = egui::Color32::from_rgba_unmultiplied(
        stroke_col.r(),
        stroke_col.g(),
        stroke_col.b(),
        28,
    );
    painter.rect_filled(icon_rect, 3.5, bg);
    painter.rect_stroke(
        icon_rect,
        3.5,
        egui::Stroke::new(1.0_f32, stroke_col),
        egui::StrokeKind::Inside,
    );
    let c = icon_rect.center();
    let r = (icon_rect.width().min(icon_rect.height()) * 0.36).max(4.5);
    match idx {
        0 => {
            // 단면선 아이콘: 중심축 + 우측 단면 두께 프로파일
            painter.line_segment(
                [egui::pos2(c.x, c.y - r), egui::pos2(c.x, c.y + r)],
                egui::Stroke::new(1.0_f32, stroke_col),
            );
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(c.x + r * 0.85, c.y - r * 0.85),
                    egui::pos2(c.x + r * 0.65, c.y + r * 0.25),
                    egui::pos2(c.x + r * 0.25, c.y + r * 0.85),
                    egui::pos2(c.x, c.y + r * 0.85),
                ],
                egui::Stroke::new(2.0_f32, stroke_col),
            ));
        }
        1 => {
            // 외곽선 아이콘: 토기 좌우 대칭 외곽 입면 윤곽
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(c.x - r * 0.85, c.y - r * 0.80),
                    egui::pos2(c.x - r * 0.65, c.y + r * 0.25),
                    egui::pos2(c.x - r * 0.35, c.y + r * 0.80),
                    egui::pos2(c.x + r * 0.35, c.y + r * 0.80),
                    egui::pos2(c.x + r * 0.65, c.y + r * 0.25),
                    egui::pos2(c.x + r * 0.85, c.y - r * 0.80),
                ],
                egui::Stroke::new(1.7_f32, stroke_col),
            ));
        }
        2 => {
            // 횡단면 아이콘: 동심원 + 수평 절단선
            painter.circle_stroke(c, r * 0.85, egui::Stroke::new(1.4_f32, stroke_col));
            painter.circle_stroke(c, r * 0.52, egui::Stroke::new(1.0_f32, stroke_col));
            painter.line_segment(
                [egui::pos2(c.x - r, c.y), egui::pos2(c.x + r, c.y)],
                egui::Stroke::new(1.3_f32, stroke_col),
            );
        }
        _ => {
            // 치수선 아이콘: 양방향 치수 화살표 + 양끝 보조선
            painter.line_segment(
                [
                    egui::pos2(c.x - r * 0.85, c.y - r * 0.65),
                    egui::pos2(c.x - r * 0.85, c.y + r * 0.65),
                ],
                egui::Stroke::new(1.1_f32, stroke_col),
            );
            painter.line_segment(
                [
                    egui::pos2(c.x + r * 0.85, c.y - r * 0.65),
                    egui::pos2(c.x + r * 0.85, c.y + r * 0.65),
                ],
                egui::Stroke::new(1.1_f32, stroke_col),
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - r * 0.80, c.y),
                    egui::pos2(c.x + r * 0.80, c.y),
                ],
                egui::Stroke::new(1.4_f32, stroke_col),
            );
        }
    }
}

/// 액션바는 **선택된 레이어 또는 마우스가 호버된 레이어 하나에만** 표시한다(도면 외곽선 가림 방지).
pub fn active_action_bar_layer(
    selected: Option<usize>,
    hovered: Option<usize>,
    visible_layers: &[usize],
) -> Option<usize> {
    if let Some(s) = selected {
        if visible_layers.contains(&s) {
            return Some(s);
        }
    }
    hovered.filter(|h| visible_layers.contains(h))
}

/// Image 7 & 레이어 관리: 개별 도면 레이어(`0..5`) 상단에
/// `[✓/□] [↻90°] [↕180°] [↻앞으로] [🗑Del]` 소형 아이콘 컨트롤 바를 렌더링하고 드래그 이동을 지원한다.
#[allow(clippy::too_many_arguments)]
fn draw_individual_view_flip_controls(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    anchor_top_center: egui::Pos2,
    view_id_prefix: &str,
    layer_idx: usize,
    is_selected: bool,
    scale_px_per_mm: f32,
    transform: &mut IndividualViewTransform2D,
) -> (bool, bool) {
    let total_w = 184.0_f32;
    let bar_h = 18.0_f32;
    let bar_rect = egui::Rect::from_center_size(
        egui::pos2(anchor_top_center.x, anchor_top_center.y - 12.0),
        egui::vec2(total_w, bar_h),
    );
    let terra = egui::Color32::from_rgb(ACTION.rgb[0], ACTION.rgb[1], ACTION.rgb[2]);
    let ink = egui::Color32::from_rgb(INK.rgb[0], INK.rgb[1], INK.rgb[2]);
    let select_blue = egui::Color32::from_rgb(
        SELECT_BLUE.rgb[0],
        SELECT_BLUE.rgb[1],
        SELECT_BLUE.rgb[2],
    );
    let profile_red = egui::Color32::from_rgb(
        PROFILE_RED.rgb[0],
        PROFILE_RED.rgb[1],
        PROFILE_RED.rgb[2],
    );
    painter.rect_filled(
        bar_rect,
        3.5,
        egui::Color32::from_rgba_unmultiplied(250, 249, 245, 244),
    );
    painter.rect_stroke(
        bar_rect,
        3.5,
        egui::Stroke::new(
            if is_selected { 1.5_f32 } else { 1.0_f32 },
            if is_selected { select_blue } else { terra },
        ),
        egui::StrokeKind::Inside,
    );

    let mut clicked_select = false;
    let mut clicked_delete = false;

    // 좌측 그립(`✥` 드래그 시 해당 레이어만 자유 이동, 억지로 붙이지 않고 원하는 간격으로 배치 가능!)
    let grip_rect = egui::Rect::from_min_size(
        egui::pos2(bar_rect.left() + 2.0, bar_rect.top() + 1.5),
        egui::vec2(20.0, bar_h - 3.0),
    );
    let grip_resp = ui.interact(
        grip_rect,
        egui::Id::new((view_id_prefix, "drag_grip", layer_idx)),
        egui::Sense::click_and_drag(),
    );
    if grip_resp.clicked() || grip_resp.dragged() {
        clicked_select = true;
    }
    if grip_resp.dragged() {
        let sc = scale_px_per_mm.max(0.5);
        let d = grip_resp.drag_delta();
        transform.translate_mm(f64::from(d.x / sc), f64::from(-d.y / sc));
    }
    painter.rect_filled(
        grip_rect,
        2.5,
        if is_selected {
            select_blue
        } else {
            egui::Color32::from_rgb(236, 232, 222)
        },
    );
    painter.text(
        grip_rect.center(),
        egui::Align2::CENTER_CENTER,
        "✥",
        egui::FontId::proportional(9.0),
        if is_selected { egui::Color32::WHITE } else { ink },
    );

    let specs = [
        ("rot90", "↻90°", 34.0_f32, 0u8),
        ("flip180", "↕180°", 38.0_f32, 1u8),
        ("flipfwd", "↻앞으로", 48.0_f32, 2u8),
        ("del", "🗑Del", 34.0_f32, 3u8),
    ];
    let mut bx = grip_rect.right() + 2.0;
    for (suffix, lbl, bw, kind) in specs {
        let brect = egui::Rect::from_min_size(
            egui::pos2(bx, bar_rect.top() + 1.5),
            egui::vec2(bw, bar_h - 3.0),
        );
        let resp = ui.interact(
            brect,
            egui::Id::new((view_id_prefix, suffix, layer_idx)),
            egui::Sense::click(),
        );
        if resp.clicked() {
            clicked_select = true;
            match kind {
                0 => transform.rotate_cw_90(),
                1 => transform.flip_180(),
                2 => transform.flip_forward_continuous_90(),
                _ => clicked_delete = true,
            }
        }
        let active = (kind == 0 && transform.rot_90_steps != 0)
            || ((kind == 1 || kind == 2) && transform.flip_forward_steps != 0);
        painter.rect_filled(
            brect,
            2.5,
            if active {
                profile_red
            } else if kind == 3 && resp.hovered() {
                egui::Color32::from_rgb(252, 224, 224)
            } else if resp.hovered() {
                egui::Color32::from_rgb(240, 232, 220)
            } else {
                egui::Color32::from_rgb(246, 243, 236)
            },
        );
        painter.text(
            brect.center(),
            egui::Align2::CENTER_CENTER,
            lbl,
            egui::FontId::proportional(8.2),
            if active {
                egui::Color32::WHITE
            } else if kind == 3 {
                profile_red
            } else {
                ink
            },
        );
        bx += bw + 1.5;
    }

    (clicked_select, clicked_delete)
}

/// 우측 `73%` (또는 `Tab` 키 `100%`) 2D 실측 도판 작업대(`SheetPane`)를 렌더링한다.
#[allow(clippy::too_many_arguments)]
pub fn render_sheet_pane_egui(
    ui: &mut egui::Ui,
    plate: &ComposedPlate2D,
    report: &ArtifactMeasurementReport,
    mesh: &TriMesh,
    psi_deg: f64,
    ink_density_pct: f64,
    show_mm_grid: bool,
    state: &mut SheetPaneState,
) -> (egui::Rect, egui::Rect, egui::Rect) {
    let avail = ui.available_size();
    let canvas_h = avail.y.max(160.0);

    let _ground = egui::Color32::from_rgb(GROUND.rgb[0], GROUND.rgb[1], GROUND.rgb[2]);
    let _wash = egui::Color32::from_rgb(WASH.rgb[0], WASH.rgb[1], WASH.rgb[2]);
    let paper = egui::Color32::from_rgb(SURFACE.rgb[0], SURFACE.rgb[1], SURFACE.rgb[2]);
    let border = egui::Color32::from_rgb(EDGE.rgb[0], EDGE.rgb[1], EDGE.rgb[2]);
    let _line_col = egui::Color32::from_rgb(LINE.rgb[0], LINE.rgb[1], LINE.rgb[2]);
    let grid_10 = egui::Color32::from_rgb(198, 192, 174);
    let grid_5 = egui::Color32::from_rgb(220, 215, 200);
    let grid_1 = egui::Color32::from_rgb(GRID_1MM.rgb[0], GRID_1MM.rgb[1], GRID_1MM.rgb[2]);
    let ink = egui::Color32::from_rgb(INK.rgb[0], INK.rgb[1], INK.rgb[2]);
    let muted = egui::Color32::from_rgb(MUTED.rgb[0], MUTED.rgb[1], MUTED.rgb[2]);
    let terra = egui::Color32::from_rgb(ACTION.rgb[0], ACTION.rgb[1], ACTION.rgb[2]);
    let profile_red = egui::Color32::from_rgb(
        PROFILE_RED.rgb[0],
        PROFILE_RED.rgb[1],
        PROFILE_RED.rgb[2],
    );
    let select_blue = egui::Color32::from_rgb(
        SELECT_BLUE.rgb[0],
        SELECT_BLUE.rgb[1],
        SELECT_BLUE.rgb[2],
    );
    let _done_bg = egui::Color32::from_rgb(DONE_BG.rgb[0], DONE_BG.rgb[1], DONE_BG.rgb[2]);
    let _done_fg = egui::Color32::from_rgb(DONE_INK.rgb[0], DONE_INK.rgb[1], DONE_INK.rgb[2]);

    if (state.rubbing_params.ink_density_pct - ink_density_pct).abs() > 0.5
        && !state.show_roi_rubbing_window
    {
        // Keep user wheel adjustments primary
    }

    let rub_view_tf = state.view_transforms[2];
    let key: SheetRubbingCacheKey = (
        (
            mesh.name.clone(),
            psi_deg.round() as i32,
            (state.rubbing_params.pose_tilt_deg * 10.0).round() as i32,
            (state.rubbing_params.pose_roll_deg * 10.0).round() as i32,
            state.rubbing_params.light_azimuth_deg.round() as i32,
            state.rubbing_params.light_elevation_deg.round() as i32,
            (state.rubbing_params.light_intensity * 100.0).round() as i32,
        ),
        (
            state.rubbing_params.ink_density_pct.round() as i32,
            (state.rubbing_params.msii_detail_boost * 100.0).round() as i32,
            (state.rubbing_params.sigmoid_contrast * 10.0).round() as i32,
            (state.rubbing_params.contact_threshold * 100.0).round() as i32,
            state.rubbing_params.style,
            plate.flip_exterior_right,
            None,
            rub_view_tf,
            state.rubbing_params.unroll_mode,
        ),
    );
    // Rayon 3-뷰 동시 병렬 탁본 버퍼 생성 (560x560 고선명 + 멀티코어 병렬화로 렉 제로 달성!)
    // Image 7: 우측 상단 1:1 정밀 탁본(bottom_img)은 개별 뷰의 [🔁앞으로], [🔃180°], [↻90°] 조작에 따라
    // 연속으로 앞으로 뒤집힌 면(외저면 -> 정면외면 -> 상하반전 -> 내면/후면) 및 90°/180° 회전이 즉시 반영됨!
    if state.cached_key.as_ref() != Some(&key) {
        let params_copy = state.rubbing_params.clone();
        // 곡면 전개 시 실제 물리 크기(둘레 × 높이)와 그에 맞는 텍스처 해상도를 쓴다.
        state.unrolled_size_mm = params_copy.unroll_mode.map(|mode| {
            let u = strata_shade::unroll_mesh_surface(mesh, mode);
            [u.unrolled_width_mm, u.unrolled_height_mm]
        });
        let (bw, bh) = match state.unrolled_size_mm {
            Some(size) => unrolled_texture_dims(size, 520),
            None => (520, 520),
        };
        let (front_img, (back_img, mut bottom_img)) = rayon::join(
            || {
                render_orthographic_rubbing_rgba_with_params(
                    mesh,
                    PlateRasterKind::FrontExteriorRubbing,
                    psi_deg,
                    &params_copy,
                    560,
                    560,
                )
            },
            || {
                rayon::join(
                    || {
                        render_orthographic_rubbing_rgba_with_params(
                            mesh,
                            PlateRasterKind::BackInteriorRubbing,
                            psi_deg,
                            &params_copy,
                            520,
                            520,
                        )
                    },
                    || {
                        let flipped_kind = match rub_view_tf.flip_forward_steps % 4 {
                            1 => PlateRasterKind::FrontExteriorRubbing,
                            3 => PlateRasterKind::BackInteriorRubbing,
                            _ => PlateRasterKind::BottomPlanRubbing,
                        };
                        render_orthographic_rubbing_rgba_with_params(
                            mesh,
                            flipped_kind,
                            psi_deg,
                            &params_copy,
                            bw,
                            bh,
                        )
                    },
                )
            },
        );
        if rub_view_tf.flip_forward_steps % 4 == 2 {
            bottom_img = image::imageops::flip_vertical(&bottom_img);
        }
        bottom_img = match rub_view_tf.rot_90_steps % 4 {
            1 => image::imageops::rotate90(&bottom_img),
            2 => image::imageops::rotate180(&bottom_img),
            3 => image::imageops::rotate270(&bottom_img),
            _ => bottom_img,
        };
        state.cached_front_img = Some(front_img);
        state.cached_back_img = Some(back_img);
        state.cached_bottom_img = Some(bottom_img);
        state.cached_front_tex = None;
        state.cached_back_tex = None;
        state.cached_bottom_tex = None;
        state.cached_key = Some(key);
    }

    let mut out_header_rect = egui::Rect::NOTHING;
    let mut out_strip_rect = egui::Rect::NOTHING;
    let mut out_sheet_rect = egui::Rect::NOTHING;

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;

        let (sheet_rect, sheet_resp) = ui.allocate_exact_size(
            egui::vec2(avail.x, canvas_h),
            egui::Sense::click_and_drag(),
        );
        out_header_rect =
            egui::Rect::from_min_size(sheet_rect.min, egui::vec2(sheet_rect.width(), 0.0));
        out_strip_rect =
            egui::Rect::from_min_size(sheet_rect.min, egui::vec2(sheet_rect.width(), 0.0));
        out_sheet_rect = sheet_rect;

        // Image 3: 조판(Publication Page Layout) 용지 규격 및 용지 내부 전용 방안선 + 5cm(50mm) 빨간색선 명세
        let spec = compute_paper_sheet_graph_spec(
            sheet_rect,
            state.paper_preset,
            state.zoom,
            state.pan_px,
        );
        let p_rect = spec.paper_bounds_rect;
        let paper_scale = spec.scale_px_per_mm;
        // 대형 유물: 1:1 도판이 용지보다 크면 자동으로 축척을 낮춘다(사용자가 직접 선택하면 고정).
        if !state.scale_locked {
            let (pw, ph) = state.paper_preset.dimensions_mm();
            state.active_scale_preset = fit_scale_preset(
                plate.canvas_width_mm,
                plate.canvas_height_mm,
                pw,
                ph,
                12.0,
            );
        }
        let scale_factor = (state.active_scale_preset.ratio() as f32).clamp(0.2, 5.0);
        let scale = paper_scale * scale_factor;
        let cw_px = (plate.canvas_width_mm as f32) * scale;
        let ch_px = (plate.canvas_height_mm as f32) * scale;
        let origin_x = p_rect.center().x - cw_px * 0.48;
        let origin_y = p_rect.center().y + ch_px * 0.48;

        // 상단 용지설정·부분탁본 바, 선조정 바, 하단 탁본휠·축척설정 바 영역과의 충돌 방지
        let top_style_bar_rect = egui::Rect::from_min_size(
            egui::pos2(sheet_rect.left() + 12.0, sheet_rect.top() + 6.0),
            egui::vec2((sheet_rect.width() - 276.0).clamp(320.0, 620.0), 26.0),
        );
        let bot_scale_bar_rect = egui::Rect::from_min_size(
            egui::pos2((sheet_rect.right() - 328.0).max(sheet_rect.left() + 12.0), sheet_rect.bottom() - 34.0),
            egui::vec2(318.0, 26.0),
        );
        let pointer_in_bars = sheet_resp.interact_pointer_pos().is_some_and(|p| {
            p.y < sheet_rect.top() + 62.0 || p.y > sheet_rect.bottom() - 68.0
        });

        if sheet_resp.hovered() && !pointer_in_bars {
            let scroll_y = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll_y.abs() > 0.1 {
                let factor = (1.0 + scroll_y * 0.0025).clamp(0.85, 1.18);
                state.zoom = (state.zoom * factor).clamp(0.45, 4.0);
            }
        }

        let shift_held = ui.input(|i| i.modifiers.shift);
        if !pointer_in_bars && (state.roi_drag_mode || shift_held) {
            if sheet_resp.drag_started() {
                if let Some(pos) = sheet_resp.interact_pointer_pos() {
                    let mm_x = f64::from((pos.x - origin_x) / scale);
                    let mm_y = f64::from((origin_y - pos.y) / scale);
                    state.roi_drag_start_mm = Some([mm_x, mm_y]);
                }
            }
            if sheet_resp.dragged() {
                if let (Some(start_mm), Some(pos)) =
                    (state.roi_drag_start_mm, sheet_resp.interact_pointer_pos())
                {
                    let cur_x = f64::from((pos.x - origin_x) / scale);
                    let cur_y = f64::from((origin_y - pos.y) / scale);
                    state.set_rubbing_roi_mm(Some([start_mm[0], start_mm[1], cur_x, cur_y]));
                }
            }
            if sheet_resp.drag_stopped() {
                state.roi_drag_start_mm = None;
            }
        } else if !pointer_in_bars {
            if sheet_resp.dragged() {
                state.pan_px += sheet_resp.drag_delta();
            }
            if sheet_resp.double_clicked() {
                state.zoom = 1.0;
                state.pan_px = egui::Vec2::ZERO;
            }
        }

        let paper_painter = ui.painter_at(sheet_rect);
        // "방안지 여백은 무한대로 배경을 하고 용지만 붉은선으로 표시하게 하자":
        // sheet_rect 전체를 한지 방안지 배경색(paper)으로 채우고, 방안선을 화면 끝에서 끝까지 무한대로 그린다!
        paper_painter.rect_filled(sheet_rect, 0.0, paper);

        if show_mm_grid {
            for &gx in &spec.lines_1mm_x {
                paper_painter.line_segment(
                    [egui::pos2(gx, sheet_rect.top()), egui::pos2(gx, sheet_rect.bottom())],
                    egui::Stroke::new(0.45_f32, grid_1),
                );
            }
            for &gy in &spec.lines_1mm_y {
                paper_painter.line_segment(
                    [egui::pos2(sheet_rect.left(), gy), egui::pos2(sheet_rect.right(), gy)],
                    egui::Stroke::new(0.45_f32, grid_1),
                );
            }
            for &gx in &spec.lines_5mm_x {
                paper_painter.line_segment(
                    [egui::pos2(gx, sheet_rect.top()), egui::pos2(gx, sheet_rect.bottom())],
                    egui::Stroke::new(0.70_f32, grid_5),
                );
            }
            for &gy in &spec.lines_5mm_y {
                paper_painter.line_segment(
                    [egui::pos2(sheet_rect.left(), gy), egui::pos2(sheet_rect.right(), gy)],
                    egui::Stroke::new(0.70_f32, grid_5),
                );
            }
            for &gx in &spec.lines_10mm_x {
                paper_painter.line_segment(
                    [egui::pos2(gx, sheet_rect.top()), egui::pos2(gx, sheet_rect.bottom())],
                    egui::Stroke::new(0.95_f32, grid_10),
                );
            }
            for &gy in &spec.lines_10mm_y {
                paper_painter.line_segment(
                    [egui::pos2(sheet_rect.left(), gy), egui::pos2(sheet_rect.right(), gy)],
                    egui::Stroke::new(0.95_f32, grid_10),
                );
            }
            // 5cm(50mm)마다 얇은 빨간색선!
            let red_5cm_col = egui::Color32::from_rgba_unmultiplied(218, 46, 36, 175);
            for &rx in &spec.lines_50mm_red_x {
                paper_painter.line_segment(
                    [egui::pos2(rx, sheet_rect.top()), egui::pos2(rx, sheet_rect.bottom())],
                    egui::Stroke::new(1.00_f32, red_5cm_col),
                );
            }
            for &ry in &spec.lines_50mm_red_y {
                paper_painter.line_segment(
                    [egui::pos2(sheet_rect.left(), ry), egui::pos2(sheet_rect.right(), ry)],
                    egui::Stroke::new(1.00_f32, red_5cm_col),
                );
            }

            for (gx, lbl) in &spec.cm_labels_top {
                if *gx >= p_rect.left() + 4.0 && *gx <= p_rect.right() - 22.0 {
                    let is_5cm = lbl == "0cm" || lbl == "5cm" || lbl == "10cm" || lbl == "15cm" || lbl == "20cm" || lbl == "25cm" || lbl == "30cm" || lbl == "35cm" || lbl == "40cm";
                    paper_painter.text(
                        egui::pos2(*gx + 2.0, p_rect.top() + 7.0),
                        egui::Align2::LEFT_CENTER,
                        lbl,
                        egui::FontId::monospace(8.5),
                        if is_5cm { red_5cm_col } else { muted },
                    );
                }
            }
            for (gy, lbl) in &spec.cm_labels_left {
                if *gy >= p_rect.top() + 14.0 && *gy <= p_rect.bottom() - 8.0 {
                    let is_5cm = lbl == "0cm" || lbl == "5cm" || lbl == "10cm" || lbl == "15cm" || lbl == "20cm" || lbl == "25cm" || lbl == "30cm";
                    paper_painter.text(
                        egui::pos2(p_rect.left() + 3.0, *gy - 6.0),
                        egui::Align2::LEFT_CENTER,
                        lbl,
                        egui::FontId::monospace(8.5),
                        if is_5cm { red_5cm_col } else { muted },
                    );
                }
            }
        }

        // "용지만 붉은선으로 표시하게 하자": 출판 용지 경계(p_rect)를 선명한 붉은색 실선 테두리로 표시!
        let paper_red_border = egui::Color32::from_rgb(218, 34, 34);
        paper_painter.rect_stroke(
            p_rect,
            1.5,
            egui::Stroke::new(2.2_f32, paper_red_border),
            egui::StrokeKind::Inside,
        );
        let (pw_mm_lbl, ph_mm_lbl) = state.paper_preset.dimensions_mm();
        paper_painter.text(
            egui::pos2(p_rect.left() + 6.0, (p_rect.top() - 8.0).max(sheet_rect.top() + 38.0)),
            egui::Align2::LEFT_CENTER,
            format!(
                "📄 용지 경계 (붉은선): {} ({:.0}×{:.0}mm · 무한대 방안지 배경)",
                state.paper_preset.label_kr(),
                pw_mm_lbl,
                ph_mm_lbl
            ),
            egui::FontId::proportional(9.2),
            paper_red_border,
        );
        paper_painter.rect_stroke(
            sheet_rect,
            0.0,
            egui::Stroke::new(1.0_f32, border),
            egui::StrokeKind::Inside,
        );

        // `Delete` / `Backspace` 키 입력 시 현재 선택된 레이어 즉시 삭제 ("delete키로 지우기")
        if ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
            let _ = state.delete_selected_layer();
        }
        state.has_custom_section_layer = plate
            .paths
            .iter()
            .any(|p| p.layer_id.starts_with("추가단면_"));
        if state.has_custom_section_layer && !state.request_clear_custom_section {
            // 추가단면선 생성 시 레이어 목록에 활성 표시
            if state.selected_layer_idx.is_none() {
                state.selected_layer_idx = Some(4);
            }
        }

        // 2. 다중 뷰 정밀 탁본(`Rubbing`) 래스터 영역 합성 및 Image 0 / Image 1 반영:
        //    - Image 0 ("평단면을 그려야한다 이렇게나오는게 맞나 ? 이것은 일러스트에서 그려야한다. 내가."):
        //      좌측 1:1 평면도 + 1:1 정면 입·단면도에는 기본적으로 회색 래스터 탁본을 덮어씌우지 않고 순수 일러스트 벡터 실측 선화로 표시한다!
        //    - Image 1 ("탁본은 여기에 1:1로 여기에 나타나게하라."):
        //      우측 상단(`ri > 0` 또는 `BottomPlanRubbing`) 자리에 1:1 정밀 탁본이 선명하게 나타나게 한다!
        let mut right_rubbing_box_px: Option<egui::Rect> = None;

        for (ri, region) in plate.raster_regions.iter().enumerate() {
            // 곡면 전개 모드에서는 전개 띠 하나만 도판에 표시한다(다른 판과 겹치지 않도록).
            if state.unrolled_size_mm.is_some()
                && region.kind != PlateRasterKind::BottomPlanRubbing
            {
                continue;
            }
            let raster_layer_idx = if ri == 0 && region.kind == PlateRasterKind::FrontExteriorRubbing {
                1usize
            } else {
                2usize
            };
            if !state.view_transforms[raster_layer_idx].visible {
                continue;
            }
            if ri == 0
                && region.kind == PlateRasterKind::FrontExteriorRubbing
                && !state.show_elevation_rubbing_overlay
                && plate.raster_regions.len() > 1
            {
                // 좌측 평단면(1:1 평면도 + 1:1 정면 입·단면도)은 일러스트용 순수 벡터 실측 선화로 유지!
                continue;
            }
            let r_off = state.view_transforms[raster_layer_idx].offset_mm();
            let rx0 = origin_x + ((region.rect_mm[0] + r_off[0]) as f32) * scale;
            let ry1 = origin_y - ((region.rect_mm[1] + r_off[1]) as f32) * scale;
            let rx1 = origin_x + ((region.rect_mm[2] + r_off[0]) as f32) * scale;
            let ry0 = origin_y - ((region.rect_mm[3] + r_off[1]) as f32) * scale;
            let mut dst_rect = egui::Rect::from_min_max(
                egui::pos2(rx0.min(rx1), ry0.min(ry1)),
                egui::pos2(rx0.max(rx1), ry0.max(ry1)),
            );

            // 곡면 전개 탁본은 실제 (둘레 × 높이) mm 크기로 1:1 배치해야 모눈 위 계측이 성립한다.
            if region.kind == PlateRasterKind::BottomPlanRubbing {
                if let Some([sw, sh]) = state.unrolled_size_mm {
                    let w_px = (sw as f32) * scale;
                    let h_px = (sh as f32) * scale;
                    let center = dst_rect.center();
                    let mut rect = egui::Rect::from_center_size(center, egui::vec2(w_px, h_px));
                    let dx = if rect.left() < p_rect.left() {
                        p_rect.left() - rect.left()
                    } else if rect.right() > p_rect.right() {
                        p_rect.right() - rect.right()
                    } else {
                        0.0
                    };
                    let dy = if rect.top() < p_rect.top() {
                        p_rect.top() - rect.top()
                    } else if rect.bottom() > p_rect.bottom() {
                        p_rect.bottom() - rect.bottom()
                    } else {
                        0.0
                    };
                    rect = rect.translate(egui::vec2(dx, dy));
                    dst_rect = rect;
                    right_rubbing_box_px = Some(rect);
                }
            }

            // Image 14: 우측 탁본 영역(BottomPlanRubbing 또는 두 번째 보조 탁본 영역)은
            // 사용자가 드래그한 부분 탁본(rubbing_roi_mm)이 있을 때 그 드래그한 실제 크기(1:1 mm 스케일) 그대로 나타나게 함!
            if ri > 0 || region.kind == PlateRasterKind::BottomPlanRubbing {
                if let (Some([ox0, oy0, ox1, oy1]), Some(first_reg)) =
                    (state.rubbing_roi_mm, plate.raster_regions.first())
                {
                    let roi_w_px = (((ox1 - ox0).abs() as f32) * scale).clamp(48.0, 260.0);
                    let roi_h_px = (((oy1 - oy0).abs() as f32) * scale).clamp(48.0, 240.0);
                    let center_pt = dst_rect.center();
                    dst_rect = egui::Rect::from_center_size(center_pt, egui::vec2(roi_w_px, roi_h_px));
                    right_rubbing_box_px = Some(dst_rect);

                    // 드래그한 1:1 크기 탁본을 정면 탁본 텍스처의 해당 UV 영역에서 1:1로 직접 추출하여 표시!
                    if state.cached_front_tex.is_none() {
                        if let Some(ref src_img) = state.cached_front_img {
                            let color_img = rgba_to_color_image_clipped_with_roi(
                                src_img,
                                first_reg.rect_mm,
                                first_reg.clip_half_sign,
                                None,
                            );
                            state.cached_front_tex = Some(ui.ctx().load_texture(
                                "sheet_rubbing_front_0",
                                color_img,
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                    }
                    if let Some(ref ftex) = state.cached_front_tex {
                        let frx0 = first_reg.rect_mm[0].min(first_reg.rect_mm[2]);
                        let frx1 = first_reg.rect_mm[0].max(first_reg.rect_mm[2]);
                        let fry0 = first_reg.rect_mm[1].min(first_reg.rect_mm[3]);
                        let fry1 = first_reg.rect_mm[1].max(first_reg.rect_mm[3]);
                        let frw = (frx1 - frx0).max(1.0);
                        let frh = (fry1 - fry0).max(1.0);
                        let u0 = (((ox0 - frx0) / frw) as f32).clamp(0.0, 0.95);
                        let u1 = (((ox1 - frx0) / frw) as f32).clamp(u0 + 0.04, 1.0);
                        let v0 = (((fry1 - oy1) / frh) as f32).clamp(0.0, 0.95);
                        let v1 = (((fry1 - oy0) / frh) as f32).clamp(v0 + 0.04, 1.0);
                        paper_painter.rect_filled(dst_rect, 2.0, paper);
                        paper_painter.image(
                            ftex.id(),
                            dst_rect,
                            egui::Rect::from_min_max(egui::pos2(u0, v0), egui::pos2(u1, v1)),
                            egui::Color32::WHITE,
                        );
                        paper_painter.rect_stroke(
                            dst_rect,
                            2.0,
                            egui::Stroke::new(1.2_f32, select_blue),
                            egui::StrokeKind::Inside,
                        );
                        paper_painter.text(
                            egui::pos2(dst_rect.center().x, dst_rect.top() - 8.0),
                            egui::Align2::CENTER_CENTER,
                            format!(
                                "1:1 부분 탁본 ({:.1}×{:.1}mm)",
                                (ox1 - ox0).abs(),
                                (oy1 - oy0).abs()
                            ),
                            egui::FontId::proportional(10.0),
                            select_blue,
                        );
                        continue;
                    }
                } else {
                    right_rubbing_box_px = Some(dst_rect);
                }
            }

            let tex_id_opt = match region.kind {
                PlateRasterKind::FrontExteriorRubbing => {
                    if state.cached_front_tex.is_none() {
                        if let Some(ref src_img) = state.cached_front_img {
                            let color_img = rgba_to_color_image_clipped_with_roi(
                                src_img,
                                region.rect_mm,
                                region.clip_half_sign,
                                state.rubbing_roi_mm,
                            );
                            state.cached_front_tex = Some(ui.ctx().load_texture(
                                format!("sheet_rubbing_front_{ri}"),
                                color_img,
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                    }
                    state.cached_front_tex.as_ref().map(|t| t.id())
                }
                PlateRasterKind::BackInteriorRubbing => {
                    if state.cached_back_tex.is_none() {
                        if let Some(ref src_img) = state.cached_back_img {
                            let color_img = rgba_to_color_image_clipped_with_roi(
                                src_img,
                                region.rect_mm,
                                region.clip_half_sign,
                                state.rubbing_roi_mm,
                            );
                            state.cached_back_tex = Some(ui.ctx().load_texture(
                                format!("sheet_rubbing_back_{ri}"),
                                color_img,
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                    }
                    state.cached_back_tex.as_ref().map(|t| t.id())
                }
                PlateRasterKind::BottomPlanRubbing => {
                    if state.cached_bottom_tex.is_none() {
                        if let Some(ref src_img) = state.cached_bottom_img {
                            let color_img = rgba_to_color_image_clipped_with_roi(
                                src_img,
                                region.rect_mm,
                                region.clip_half_sign,
                                None,
                            );
                            state.cached_bottom_tex = Some(ui.ctx().load_texture(
                                format!("sheet_rubbing_bottom_{ri}"),
                                color_img,
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                    }
                    state.cached_bottom_tex.as_ref().map(|t| t.id())
                }
            };

            if let Some(tex_id) = tex_id_opt {
                paper_painter.image(
                    tex_id,
                    dst_rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
        }

        // 사용자가 드래그하여 지정한 탁본 ROI 사각형 표시
        if let Some([ox0, oy0, ox1, oy1]) = state.rubbing_roi_mm {
            let sx0 = origin_x + (ox0 as f32) * scale;
            let sy1 = origin_y - (oy0 as f32) * scale;
            let sx1 = origin_x + (ox1 as f32) * scale;
            let sy0 = origin_y - (oy1 as f32) * scale;
            let roi_rect = egui::Rect::from_min_max(
                egui::pos2(sx0.min(sx1), sy0.min(sy1)),
                egui::pos2(sx0.max(sx1), sy0.max(sy1)),
            );
            paper_painter.rect_stroke(
                roi_rect,
                2.0,
                egui::Stroke::new(1.6_f32, select_blue),
                egui::StrokeKind::Inside,
            );
            paper_painter.text(
                egui::pos2(
                    roi_rect.left() + 4.0,
                    (roi_rect.top() - 12.0).max(sheet_rect.top() + 48.0),
                ),
                egui::Align2::LEFT_TOP,
                format!(
                    "1:1 탁본 선택영역 ({:.1}×{:.1}mm)",
                    (ox1 - ox0).abs(),
                    (oy1 - oy0).abs()
                ),
                egui::FontId::proportional(10.0),
                select_blue,
            );
        }

        // =========================================================================================
        // Image 14: 탁본 농도·대비·선명도·한지여백 4종 미세조정 휠 — 도면 용지(p_rect) 밖 하단 좌측 도크에 4열 병렬 배치하여 도면과 절대 겹치지 않게 함!
        // =========================================================================================
        let _ = right_rubbing_box_px;
        let wheel_w = 136.0_f32;
        let wheel_h = 14.0_f32;
        let wheel_gap_x = 12.0_f32;
        let wheel_row_y = sheet_rect.bottom() - 36.0;
        let wheel_start_x = sheet_rect.left() + 12.0;

        let panel_bg_rect = egui::Rect::from_min_size(
            egui::pos2(wheel_start_x - 4.0, wheel_row_y - 4.0),
            egui::vec2(4.0 * wheel_w + 3.0 * wheel_gap_x + 8.0, 34.0),
        );
        paper_painter.rect_filled(
            panel_bg_rect,
            4.0,
            egui::Color32::from_rgba_unmultiplied(250, 249, 245, 242),
        );
        paper_painter.rect_stroke(
            panel_bg_rect,
            4.0,
            egui::Stroke::new(1.0_f32, border),
            egui::StrokeKind::Inside,
        );

        let wheel_specs = [
            (
                RubbingFineWheelKind::InkDensity,
                "rub_wheel_ink_density",
                "① 먹물농도",
                format!("{:.0}%", state.rubbing_params.ink_density_pct),
                ((state.rubbing_params.ink_density_pct as f32).rem_euclid(5.0)) / 5.0,
            ),
            (
                RubbingFineWheelKind::SigmoidContrast,
                "rub_wheel_sigmoid_contrast",
                "② 음양대비",
                format!("{:.1}", state.rubbing_params.sigmoid_contrast),
                (state.rubbing_params.sigmoid_contrast.rem_euclid(1.0)),
            ),
            (
                RubbingFineWheelKind::MsiiRidgeSharpness,
                "rub_wheel_msii_sharpness",
                "③ 문양선명도",
                format!("{:.2}×", state.rubbing_params.msii_detail_boost),
                ((state.rubbing_params.msii_detail_boost * 10.0).rem_euclid(1.0)),
            ),
            (
                RubbingFineWheelKind::PaperContactThreshold,
                "rub_wheel_contact_thresh",
                "④ 한지여백",
                format!("{:+.2}", state.rubbing_params.contact_threshold),
                ((state.rubbing_params.contact_threshold * 20.0).rem_euclid(1.0)),
            ),
        ];

        for (wi, (wkind, wid, wtitle, wval, wphase)) in wheel_specs.into_iter().enumerate() {
            let wx = wheel_start_x + (wi as f32) * (wheel_w + wheel_gap_x);
            let wrect = egui::Rect::from_min_size(
                egui::pos2(wx, wheel_row_y),
                egui::vec2(wheel_w, wheel_h),
            );
            let steps = draw_rubbing_fine_wheel(
                ui,
                &paper_painter,
                wrect,
                wid,
                wtitle,
                &wval,
                wphase,
            );
            if steps != 0 {
                state.rubbing_params.apply_wheel_step(wkind, steps);
            }
        }

        // =========================================================================================
        // 3. 도판 경로 렌더링 (Image 11: 단면 검게 채우지 않고 선으로 표시 + Image 15: 모든 선 mm 굵기·색상 직결)
        // =========================================================================================
        let sec_col = egui::Color32::from_rgb(
            state.line_style.section_color_rgb[0],
            state.line_style.section_color_rgb[1],
            state.line_style.section_color_rgb[2],
        );
        let out_col = egui::Color32::from_rgb(
            state.line_style.outer_color_rgb[0],
            state.line_style.outer_color_rgb[1],
            state.line_style.outer_color_rgb[2],
        );
        let aux_col = egui::Color32::from_rgb(
            state.line_style.aux_color_rgb[0],
            state.line_style.aux_color_rgb[1],
            state.line_style.aux_color_rgb[2],
        );
        let dim_col = egui::Color32::from_rgb(
            state.line_style.dim_color_rgb[0],
            state.line_style.dim_color_rgb[1],
            state.line_style.dim_color_rgb[2],
        );

        let mut main_vessel_min_x = f32::INFINITY;
        let mut main_vessel_max_x = f32::NEG_INFINITY;
        let mut main_vessel_top_y = f32::INFINITY;
        let mut main_vessel_bot_y = f32::NEG_INFINITY;
        let mut section_badge_anchor: Option<egui::Pos2> = None;

        // 6개 개별 도면 레이어(0: 1:1 평면도, 1: 1:1 단면도, 2: 1:1 정밀 탁본, 3: 1:1 수평 횡단면, 4: 추가 단면 A-A', 5: 치수선·축척자)의 중심점 산출
        let elev_reg_mm = plate
            .raster_regions
            .first()
            .map(|r| r.rect_mm)
            .unwrap_or([36.0, 24.0, 160.0, 88.0]);
        let rub_reg_mm = plate
            .raster_regions
            .get(1)
            .map(|r| r.rect_mm)
            .unwrap_or([180.0, 110.0, 300.0, 230.0]);
        let cx_left_mm = 0.5 * (elev_reg_mm[0] + elev_reg_mm[2]);
        let cy_elev_mm = 0.5 * (elev_reg_mm[1] + elev_reg_mm[3]);
        let top_elev_mm = elev_reg_mm[1].max(elev_reg_mm[3]);
        let half_d_mm = 0.5 * (elev_reg_mm[2] - elev_reg_mm[0]).abs().max(20.0);
        let cy_plan_mm = top_elev_mm + half_d_mm + 34.0;
        let cx_right_mm = 0.5 * (rub_reg_mm[0] + rub_reg_mm[2]);
        let cy_rub_mm = 0.5 * (rub_reg_mm[1] + rub_reg_mm[3]);
        let cy_hsec_mm = cy_elev_mm;
        let cx_custom_mm = cx_right_mm + half_d_mm * 1.85 + 42.0;
        let cy_custom_mm = cy_rub_mm;
        let layer_centers_mm: [[f64; 2]; 6] = [
            [cx_left_mm, cy_plan_mm],
            [cx_left_mm, cy_elev_mm],
            [cx_right_mm, cy_rub_mm],
            [cx_right_mm, cy_hsec_mm],
            [cx_custom_mm, cy_custom_mm],
            [cx_left_mm, cy_elev_mm],
        ];

        for elem in plate.paths.iter().filter(|_| state.unrolled_size_mm.is_none()) {
            if elem.points_mm.len() < 2 {
                continue;
            }
            if state.rubbing_roi_mm.is_some()
                && (elem.layer_id.contains("외저면")
                    || (elem.layer_id.contains("보조_중심선")
                        && elem.points_mm.iter().any(|p| p[0] > 35.0 && p[1] < 95.0)))
            {
                continue;
            }
            let view_idx = classify_plate_path_layer_idx(&elem.layer_id);
            let vtf = state.view_transforms[view_idx];
            if !vtf.visible {
                continue;
            }
            let view_center_mm = layer_centers_mm[view_idx];
            let is_scale_bar = elem.layer_id == "치수_축척막대" || elem.layer_id == "축척자_5cm";
            let transformed_points_mm: Vec<[f64; 2]> = if !is_scale_bar
                && (vtf.rot_90_steps != 0
                    || vtf.flip_forward_steps != 0
                    || vtf.offset_mm_x_tenths != 0
                    || vtf.offset_mm_y_tenths != 0)
            {
                elem.points_mm
                    .iter()
                    .map(|&p| vtf.transform_point_around(p, view_center_mm))
                    .collect()
            } else {
                elem.points_mm.clone()
            };
            let is_section_layer = elem.layer_id.contains("단면_실선");
            let is_outer_layer = elem.layer_id.contains("외곽선")
                || elem.layer_id.contains("구경")
                || elem.layer_id.contains("저경")
                || elem.layer_id.contains("추정복원");
            let is_horiz_or_aux = elem.layer_id.contains("횡단면")
                || elem.layer_id.contains("중심")
                || elem.layer_id.contains("십자선");

            let (stroke_col, width_mm) = if is_section_layer {
                (sec_col, snap_to_standard_weight_mm(state.line_style.section_width_mm))
            } else if is_outer_layer {
                (out_col, snap_to_standard_weight_mm(state.line_style.outer_width_mm))
            } else if is_horiz_or_aux {
                (aux_col, snap_to_standard_weight_mm(state.line_style.aux_width_mm))
            } else {
                let c = match elem.stroke_hex.as_str() {
                    "#B5573A" => aux_col,
                    "#FF0000" => profile_red,
                    "#2A78D6" => select_blue,
                    "#5E5D59" => muted,
                    _ => out_col,
                };
                (c, elem.stroke_width_mm as f32)
            };

            // Image 11: "검게 나오게 하지말고 선으로 나와야한다"
            // state.line_style.fill_section_solid == true 일 때만 내부 채움을 수행하고, 기본값(false)에서는 깨끗한 외곽선(선)으로만 제도한다!
            if state.line_style.fill_section_solid {
                if let Some(ref fill_hex) = elem.fill_hex {
                    if transformed_points_mm.len() >= 3 {
                        let tris = triangulate_polygon_ear_clipping(&transformed_points_mm);
                        if !tris.is_empty() {
                            let fill_col = if fill_hex == "#B5573A" { terra } else { sec_col };
                            let mut mesh_shape = egui::epaint::Mesh::default();
                            for p in &transformed_points_mm {
                                let pos = egui::pos2(
                                    origin_x + (p[0] as f32) * scale,
                                    origin_y - (p[1] as f32) * scale,
                                );
                                mesh_shape.vertices.push(egui::epaint::Vertex {
                                    pos,
                                    uv: egui::epaint::WHITE_UV,
                                    color: fill_col,
                                });
                            }
                            for tri in tris {
                                mesh_shape.indices.push(tri[0] as u32);
                                mesh_shape.indices.push(tri[1] as u32);
                                mesh_shape.indices.push(tri[2] as u32);
                            }
                            paper_painter.add(egui::Shape::mesh(mesh_shape));
                        }
                    }
                }
            }

            // 교차 흑백 블록 축척자 (`치수_축척막대`)
            if (elem.layer_id == "치수_축척막대" || elem.layer_id == "축척자_5cm")
                && transformed_points_mm.len() >= 2
            {
                let p_start = transformed_points_mm[0];
                let p_end = transformed_points_mm[transformed_points_mm.len() - 1];
                let raw_x0 = origin_x + (p_start[0] as f32) * scale;
                let raw_x1 = origin_x + (p_end[0] as f32) * scale;
                // 축척 막대는 선택된 축척의 표준 길이를 쓴다(실무 규격: 5cm/10cm/20cm/40cm 막대).
                let (std_bar_mm, _) = state.active_scale_preset.standard_scale_bar_mm();
                let center_x = (raw_x0 + raw_x1) * 0.5;
                let half_w = (std_bar_mm as f32) * scale * 0.5;
                let x0 = center_x - half_w;
                let x1 = center_x + half_w;
                let y_bar = origin_y - (p_start[1] as f32) * scale;
                let bar_w = (x1 - x0).abs().max(25.0);
                let bar_h = (1.6 * scale).clamp(4.0, 7.5);
                let seg_w = bar_w / 5.0;

                for i in 0..5 {
                    let sx0 = x0.min(x1) + (i as f32) * seg_w;
                    let seg_rect = egui::Rect::from_min_size(
                        egui::pos2(sx0, y_bar - bar_h),
                        egui::vec2(seg_w, bar_h),
                    );
                    let seg_fill = if i % 2 == 0 { dim_col } else { paper };
                    paper_painter.rect_filled(seg_rect, 0.0, seg_fill);
                    paper_painter.rect_stroke(
                        seg_rect,
                        0.0,
                        egui::Stroke::new(1.0_f32, dim_col),
                        egui::StrokeKind::Inside,
                    );
                }
                continue;
            }

            let mut pts: Vec<egui::Pos2> = transformed_points_mm
                .iter()
                .map(|p| {
                    egui::pos2(
                        origin_x + (p[0] as f32) * scale,
                        origin_y - (p[1] as f32) * scale,
                    )
                })
                .collect();

            let is_main_vertical_vessel =
                elem.layer_id == "단면_실선" || elem.layer_id == "외곽선_실선";
            if is_main_vertical_vessel {
                for &pt in &pts {
                    main_vessel_min_x = main_vessel_min_x.min(pt.x);
                    main_vessel_max_x = main_vessel_max_x.max(pt.x);
                    main_vessel_top_y = main_vessel_top_y.min(pt.y);
                    main_vessel_bot_y = main_vessel_bot_y.max(pt.y);
                }
                if is_section_layer && section_badge_anchor.is_none() {
                    if let Some(&mid_pt) = pts.get(pts.len() / 3) {
                        section_badge_anchor = Some(mid_pt);
                    }
                }
            }

            if elem.closed && pts.len() >= 3 {
                if let Some(&first_pt) = pts.first() {
                    pts.push(first_pt);
                }
            }

            let w_px = (width_mm * scale).clamp(0.8, 6.0);
            if let Some((dash_mm, gap_mm)) = elem.dash_pattern_mm {
                let dash_px = ((dash_mm as f32) * scale).max(3.5);
                let gap_px = ((gap_mm as f32) * scale).max(2.5);
                draw_dashed_polyline(&paper_painter, &pts, w_px, stroke_col, dash_px, gap_px);
            } else {
                paper_painter.add(egui::Shape::line(pts, egui::Stroke::new(w_px, stroke_col)));
            }
        }

        // 텍스트 주석 및 치수 콜아웃 (각 레이어의 가시성 및 오프셋 연동)
        for c in plate.callouts.iter().filter(|_| state.unrolled_size_mm.is_none()) {
            if c.text.contains("평균두께")
                || c.text.contains("최대동경")
                || (state.rubbing_roi_mm.is_some() && c.text.contains("외저면"))
            {
                continue;
            }
            let callout_layer = if c.text.contains("추가 단면") {
                4usize
            } else if c.text.contains("평면도") {
                0usize
            } else if c.text.contains("탁본") {
                2usize
            } else if c.text.contains("수평 횡단면") {
                3usize
            } else {
                5usize
            };
            if !state.view_transforms[callout_layer].visible {
                continue;
            }
            let c_off = state.view_transforms[callout_layer].offset_mm();
            let pos = egui::pos2(
                (origin_x + ((c.pos_mm[0] + c_off[0]) as f32) * scale)
                    .clamp(sheet_rect.left() + 34.0, sheet_rect.right() - 150.0),
                (origin_y - ((c.pos_mm[1] + c_off[1]) as f32) * scale)
                    .clamp(sheet_rect.top() + 50.0, sheet_rect.bottom() - 18.0),
            );
            let col = match c.color_hex.as_str() {
                "#B5573A" => aux_col,
                "#5E5D59" => muted,
                _ => dim_col,
            };
            // 축척 라벨은 현재 선택된 축척의 실무 표기(「10cm (1/2)」)로 다시 쓴다.
            let shown_text = if c.layer_id == "치수_축척라벨" {
                state.active_scale_preset.standard_scale_bar_mm().1.to_string()
            } else {
                c.text.clone()
            };
            paper_painter.text(
                pos,
                egui::Align2::LEFT_TOP,
                &shown_text,
                egui::FontId::proportional(10.5),
                col,
            );
        }

        // =========================================================================================
        // 4. Image 15: "이것도 도면안에 필요한 부분에 최대한 가깝게 와야한다. 유물의 크기 이동에따라 이동할수있게"
        // 유물 실제 외곽 경계(main_vessel_min_x .. max_x, top_y .. bot_y)에 최대한 밀착되어 함께 이동하는 치수선!
        // =========================================================================================
        if state.view_transforms[5].visible
            && state.view_transforms[1].visible
            && state.unrolled_size_mm.is_none()
        {
            if let Some(first_reg) = plate.raster_regions.first() {
                let fallback_rx0 =
                    origin_x + (first_reg.rect_mm[0].min(first_reg.rect_mm[2]) as f32) * scale;
                let fallback_rx1 =
                    origin_x + (first_reg.rect_mm[0].max(first_reg.rect_mm[2]) as f32) * scale;
                let fallback_top =
                    origin_y - (first_reg.rect_mm[1].max(first_reg.rect_mm[3]) as f32) * scale;
                let fallback_bot =
                    origin_y - (first_reg.rect_mm[1].min(first_reg.rect_mm[3]) as f32) * scale;

                let rx0 = if main_vessel_min_x.is_finite() {
                    main_vessel_min_x
                } else {
                    fallback_rx0
                };
                let rx1 = if main_vessel_max_x.is_finite() {
                    main_vessel_max_x
                } else {
                    fallback_rx1
                };
                let ry_top = if main_vessel_top_y.is_finite() {
                    main_vessel_top_y
                } else {
                    fallback_top
                };
                let ry_bot = if main_vessel_bot_y.is_finite() {
                    main_vessel_bot_y
                } else {
                    fallback_bot
                };

                let dim_w_px = (snap_to_standard_weight_mm(state.line_style.dim_width_mm) * scale)
                    .clamp(0.9, 3.0);

                // (a) 상단 수평 구경 화살표 치수선 — 구연부 **실측 구경**에 맞춰 표시한다.
                let span_px = (rx1 - rx0).abs();
                let span_mm = if scale > 0.0 { span_px / scale } else { 0.0 };
                let rim_mm = if report.rim_or_outer_diameter_mm > 1.0
                    && span_mm > 1.0
                    && report.rim_or_outer_diameter_mm <= (span_mm as f64) * 1.02
                {
                    report.rim_or_outer_diameter_mm
                } else {
                    span_mm as f64
                };
                let cx_v = (rx0 + rx1) * 0.5;
                let half_rim_px = if span_mm > 0.0 {
                    span_px * 0.5 * (rim_mm / span_mm as f64) as f32
                } else {
                    span_px * 0.5
                };
                let dim_x0 = cx_v - half_rim_px;
                let dim_x1 = cx_v + half_rim_px;

                let dim_y = (ry_top - 9.0).max(sheet_rect.top() + 48.0);
                paper_painter.line_segment(
                    [egui::pos2(dim_x0, ry_top - 1.5), egui::pos2(dim_x0, dim_y - 3.0)],
                    egui::Stroke::new(dim_w_px * 0.85, dim_col),
                );
                paper_painter.line_segment(
                    [egui::pos2(dim_x1, ry_top - 1.5), egui::pos2(dim_x1, dim_y - 3.0)],
                    egui::Stroke::new(dim_w_px * 0.85, dim_col),
                );
                paper_painter.line_segment(
                    [egui::pos2(dim_x0, dim_y), egui::pos2(dim_x1, dim_y)],
                    egui::Stroke::new(dim_w_px, dim_col),
                );
                for &(ax, dir_s) in &[(dim_x0, 1.0_f32), (dim_x1, -1.0_f32)] {
                    paper_painter.add(egui::Shape::convex_polygon(
                        vec![
                            egui::pos2(ax, dim_y),
                            egui::pos2(ax + dir_s * 5.0, dim_y - 2.4),
                            egui::pos2(ax + dir_s * 5.0, dim_y + 2.4),
                        ],
                        dim_col,
                        egui::Stroke::NONE,
                    ));
                }
                paper_painter.text(
                    egui::pos2((dim_x0 + dim_x1) * 0.5, dim_y - 6.5),
                    egui::Align2::CENTER_CENTER,
                    format!("구경 {:.1}mm", rim_mm),
                    egui::FontId::proportional(9.5),
                    dim_col,
                );

                // (b) 좌측 수직 높이 화살표 치수선 — 유물 좌측 외곽선 바로 옆 10px 위치에 밀착!
                let dim_x = (rx0 - 10.0).max(sheet_rect.left() + 36.0);
                paper_painter.line_segment(
                    [egui::pos2(dim_x - 3.0, ry_top), egui::pos2(rx0 - 1.5, ry_top)],
                    egui::Stroke::new(dim_w_px * 0.85, dim_col),
                );
                paper_painter.line_segment(
                    [egui::pos2(dim_x - 3.0, ry_bot), egui::pos2(rx0 - 1.5, ry_bot)],
                    egui::Stroke::new(dim_w_px * 0.85, dim_col),
                );
                paper_painter.line_segment(
                    [egui::pos2(dim_x, ry_top), egui::pos2(dim_x, ry_bot)],
                    egui::Stroke::new(dim_w_px, dim_col),
                );
                for &(ay, dir_s) in &[(ry_top, 1.0_f32), (ry_bot, -1.0_f32)] {
                    paper_painter.add(egui::Shape::convex_polygon(
                        vec![
                            egui::pos2(dim_x, ay),
                            egui::pos2(dim_x - 2.4, ay + dir_s * 5.0),
                            egui::pos2(dim_x + 2.4, ay + dir_s * 5.0),
                        ],
                        dim_col,
                        egui::Stroke::NONE,
                    ));
                }
                let h_mm = (first_reg.rect_mm[3] - first_reg.rect_mm[1]).abs();
                let h_lbl_rect = egui::Rect::from_center_size(
                    egui::pos2(dim_x - 28.0, (ry_top + ry_bot) * 0.5),
                    egui::vec2(52.0, 15.0),
                );
                paper_painter.rect_filled(
                    h_lbl_rect,
                    2.0,
                    egui::Color32::from_rgba_unmultiplied(250, 249, 245, 220),
                );
                paper_painter.text(
                    egui::pos2(dim_x - 4.0, (ry_top + ry_bot) * 0.5),
                    egui::Align2::RIGHT_CENTER,
                    format!("높이 {:.1}mm", h_mm),
                    egui::FontId::proportional(9.5),
                    dim_col,
                );
            }
        }

        // =========================================================================================
        // 4-B. Image 7 & 레이어 관리 ("레이어 모든것에는 방향조정 90도씩 delete키로 지우기 추가단면선 생성시 이것도 레이어에생성 삭제가능하고 키고끌수있게")
        // 5개 도면 뷰(0: 1:1 평면도, 1: 1:1 단면도, 2: 1:1 정밀 탁본, 3: 1:1 수평 횡단면, 4: 추가 단면 A-A') 상단에
        // 각각 독립된 [✥이동] [↻90°] [↕180°] [↻앞으로] [🗑Del] 컨트롤 바를 배치한다!
        // =========================================================================================
        let plan_top_mm = cy_plan_mm + half_d_mm + 14.0;
        let elev_top_ctrl_mm = top_elev_mm + 16.0;
        let rub_top_mm = rub_reg_mm[1].max(rub_reg_mm[3]) + 14.0;
        let hsec_bot_ctrl_mm = (cy_hsec_mm - half_d_mm * 0.86 - 4.0).max(8.0);
        let custom_top_mm = cy_custom_mm + half_d_mm * 0.65 + 14.0;
        let individual_anchors = [
            (0usize, "sheet_view_tf_plan", [cx_left_mm, plan_top_mm], true),
            (1usize, "sheet_view_tf_elev", [cx_left_mm, elev_top_ctrl_mm], true),
            (2usize, "sheet_view_tf_rubbing", [cx_right_mm, rub_top_mm], true),
            (3usize, "sheet_view_tf_hsec", [cx_right_mm, hsec_bot_ctrl_mm], true),
            (
                4usize,
                "sheet_view_tf_custom_aa",
                [cx_custom_mm, custom_top_mm],
                state.has_custom_section_layer,
            ),
        ];
        let mut pending_del_layer = None;
        // 1) 먼저 각 뷰의 앵커를 계산하고, 포인터가 올라가 있는 뷰를 찾는다.
        let half_d_px = (half_d_mm as f32) * scale;
        let mut anchored: Vec<(usize, &str, egui::Pos2)> = Vec::new();
        for (v_idx, v_prefix, base_anchor_mm, exists) in individual_anchors {
            if !exists || !state.view_transforms[v_idx].visible {
                continue;
            }
            let off = state.view_transforms[v_idx].offset_mm();
            let raw_y = (origin_y - ((base_anchor_mm[1] + off[1]) as f32) * scale)
                .clamp(sheet_rect.top() + 76.0, sheet_rect.bottom() - 46.0);
            let max_x_for_bar = if raw_y < sheet_rect.top() + 195.0 {
                sheet_rect.right() - 320.0
            } else {
                sheet_rect.right() - 100.0
            };
            let anchor_px = egui::pos2(
                (origin_x + ((base_anchor_mm[0] + off[0]) as f32) * scale)
                    .clamp(sheet_rect.left() + 100.0, max_x_for_bar.max(sheet_rect.left() + 100.0)),
                raw_y,
            );
            anchored.push((v_idx, v_prefix, anchor_px));
        }
        let visible_layers: Vec<usize> = anchored.iter().map(|(i, _, _)| *i).collect();
        let hovered_layer = anchored.iter().find_map(|(idx, _, anchor)| {
            let zone = egui::Rect::from_center_size(
                egui::pos2(anchor.x, anchor.y + half_d_px * 0.5),
                egui::vec2(half_d_px * 2.0, half_d_px * 2.0),
            );
            ui.rect_contains_pointer(zone).then_some(*idx)
        });
        let active_bar =
            active_action_bar_layer(state.selected_layer_idx, hovered_layer, &visible_layers);

        // 2) 액션바는 활성 레이어 하나에만 그린다.
        for (v_idx, v_prefix, anchor_px) in anchored {
            if active_bar != Some(v_idx) {
                continue;
            }
            let is_sel = state.selected_layer_idx == Some(v_idx);
            let (sel_clicked, del_clicked) = draw_individual_view_flip_controls(
                ui,
                &paper_painter,
                anchor_px,
                v_prefix,
                v_idx,
                is_sel,
                scale,
                &mut state.view_transforms[v_idx],
            );
            if sel_clicked {
                state.selected_layer_idx = Some(v_idx);
            }
            if del_clicked {
                pending_del_layer = Some(v_idx);
            }
        }
        if let Some(d_idx) = pending_del_layer {
            state.delete_layer(d_idx);
        }

        // =========================================================================================
        // 4-C. 우측 상단 『🗂 레이어 관리 (켜기/끄기 · 90° 회전 · Delete 지우기 · 추가단면 A-A' 연동)』 패널
        // =========================================================================================
        if state.show_layer_panel {
            let active_layer_indices: Vec<usize> = if state.has_custom_section_layer {
                vec![0, 1, 2, 3, 4, 5]
            } else {
                vec![0, 1, 2, 3, 5]
            };
            let row_h = 20.0_f32;
            let panel_w = 238.0_f32;
            let panel_h = 26.0 + (active_layer_indices.len() as f32) * row_h + 6.0;
            let layer_panel_rect = egui::Rect::from_min_size(
                egui::pos2(
                    (sheet_rect.right() - panel_w - 8.0).max(sheet_rect.left() + 12.0),
                    sheet_rect.top() + 34.0,
                ),
                egui::vec2(panel_w, panel_h),
            );
            paper_painter.rect_filled(
                layer_panel_rect,
                4.5,
                egui::Color32::from_rgba_unmultiplied(250, 249, 245, 246),
            );
            paper_painter.rect_stroke(
                layer_panel_rect,
                4.5,
                egui::Stroke::new(1.1_f32, terra),
                egui::StrokeKind::Inside,
            );
            paper_painter.text(
                egui::pos2(layer_panel_rect.left() + 8.0, layer_panel_rect.top() + 12.0),
                egui::Align2::LEFT_CENTER,
                "🗂 도면 레이어 (켜기/끄기 · 90°회전 · Del삭제)",
                egui::FontId::proportional(9.0),
                terra,
            );

            let mut row_del_idx = None;
            for (ri_row, l_idx) in active_layer_indices.into_iter().enumerate() {
                let ry = layer_panel_rect.top() + 24.0 + (ri_row as f32) * row_h;
                let r_rect = egui::Rect::from_min_size(
                    egui::pos2(layer_panel_rect.left() + 4.0, ry),
                    egui::vec2(panel_w - 8.0, row_h - 2.0),
                );
                let is_sel = state.selected_layer_idx == Some(l_idx);
                if is_sel {
                    paper_painter.rect_filled(
                        r_rect,
                        2.5,
                        egui::Color32::from_rgba_unmultiplied(226, 238, 252, 230),
                    );
                }
                let mut l_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(r_rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                l_ui.spacing_mut().item_spacing.x = 3.0;
                let vis = &mut state.view_transforms[l_idx].visible;
                let _ = l_ui.checkbox(vis, "");
                let lbl_resp = l_ui.selectable_label(
                    is_sel,
                    egui::RichText::new(sheet_layer_title_kr(l_idx))
                        .size(8.8)
                        .color(if *vis { ink } else { muted }),
                );
                if lbl_resp.clicked() {
                    state.selected_layer_idx = Some(l_idx);
                }
                if l_ui
                    .add(egui::Button::new(egui::RichText::new("↻90°").size(8.2)).corner_radius(2.5))
                    .clicked()
                {
                    state.selected_layer_idx = Some(l_idx);
                    state.view_transforms[l_idx].rotate_cw_90();
                }
                if l_ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("🗑").size(8.2).color(profile_red),
                        )
                        .corner_radius(2.5),
                    )
                    .on_hover_text("이 레이어 지우기 (Delete 키와 동일)")
                    .clicked()
                {
                    state.selected_layer_idx = Some(l_idx);
                    row_del_idx = Some(l_idx);
                }
            }
            if let Some(d_idx) = row_del_idx {
                state.delete_layer(d_idx);
            }
        }

        // =========================================================================================
        // 5. Image 2 & Image 4 ("아이콘형태 ui로 잘보이게하라"):
        // 단면선·외곽선·횡단면·치수선 4개 조정 위젯에 고시인성 벡터 아이콘 배지를 추가하고 유물 좌표와 함께 움직이며 절대 겹치지 않게 배치!
        // =========================================================================================
        let vessel_rect_mm = plate
            .raster_regions
            .first()
            .map(|r| r.rect_mm)
            .unwrap_or([40.0, 20.0, 140.0, 90.0]);
        let wash = egui::Color32::from_rgb(WASH.rgb[0], WASH.rgb[1], WASH.rgb[2]);
        let line_widget_rects = compute_non_overlapping_line_widget_rects(
            egui::pos2(origin_x, origin_y),
            scale,
            vessel_rect_mm,
            [0.0, 0.0, (vessel_rect_mm[1] + vessel_rect_mm[3]) * 0.5],
            sheet_rect,
        );
        let _ = section_badge_anchor;

        for (idx, w_rect) in line_widget_rects.into_iter().enumerate() {
            let stroke_c = match idx {
                0 => sec_col,
                1 => out_col,
                2 => aux_col,
                _ => dim_col,
            };
            paper_painter.rect_filled(
                w_rect,
                4.5,
                egui::Color32::from_rgba_unmultiplied(250, 249, 245, 246),
            );
            paper_painter.rect_stroke(
                w_rect,
                4.5,
                egui::Stroke::new(1.2_f32, stroke_c),
                egui::StrokeKind::Inside,
            );
            let icon_rect = egui::Rect::from_min_size(
                egui::pos2(w_rect.left() + 3.5, w_rect.top() + 3.0),
                egui::vec2(19.0, w_rect.height() - 6.0),
            );
            draw_line_style_vector_icon(&paper_painter, icon_rect, idx, stroke_c);

            let content_rect = egui::Rect::from_min_max(
                egui::pos2(icon_rect.right() + 4.0, w_rect.top() + 2.0),
                egui::pos2(w_rect.right() - 3.0, w_rect.bottom() - 2.0),
            );
            let mut child_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(content_rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            child_ui.spacing_mut().item_spacing.x = 3.0;
            match idx {
                0 => {
                    child_ui.label(egui::RichText::new("단면선").size(9.5).strong().color(ink));
                    let _ = child_ui.add(
                        egui::DragValue::new(&mut state.line_style.section_width_mm)
                            .speed(0.02)
                            .range(0.10..=1.50)
                            .suffix("mm")
                            .fixed_decimals(2),
                    );
                    let _ = child_ui.color_edit_button_srgb(&mut state.line_style.section_color_rgb);
                }
                1 => {
                    child_ui.label(egui::RichText::new("외곽선").size(9.5).strong().color(ink));
                    let _ = child_ui.add(
                        egui::DragValue::new(&mut state.line_style.outer_width_mm)
                            .speed(0.02)
                            .range(0.10..=1.50)
                            .suffix("mm")
                            .fixed_decimals(2),
                    );
                    let _ = child_ui.color_edit_button_srgb(&mut state.line_style.outer_color_rgb);
                }
                2 => {
                    child_ui.label(egui::RichText::new("횡단면").size(9.5).strong().color(ink));
                    let _ = child_ui.add(
                        egui::DragValue::new(&mut state.line_style.aux_width_mm)
                            .speed(0.02)
                            .range(0.10..=1.50)
                            .suffix("mm")
                            .fixed_decimals(2),
                    );
                    let _ = child_ui.color_edit_button_srgb(&mut state.line_style.aux_color_rgb);
                }
                _ => {
                    child_ui.label(egui::RichText::new("치수선").size(9.5).strong().color(ink));
                    let _ = child_ui.add(
                        egui::DragValue::new(&mut state.line_style.dim_width_mm)
                            .speed(0.02)
                            .range(0.10..=1.50)
                            .suffix("mm")
                            .fixed_decimals(2),
                    );
                    let _ = child_ui.color_edit_button_srgb(&mut state.line_style.dim_color_rgb);
                }
            }
        }

        // =========================================================================================
        // 5-B. Image 3 & Image 4: 조판 용지 설정 바 (아이콘 형태 UI로 시인성 극대화!)
        // =========================================================================================
        paper_painter.rect_filled(
            top_style_bar_rect,
            4.5,
            egui::Color32::from_rgba_unmultiplied(250, 249, 245, 246),
        );
        paper_painter.rect_stroke(
            top_style_bar_rect,
            4.5,
            egui::Stroke::new(1.1_f32, terra),
            egui::StrokeKind::Inside,
        );
        let mut bar_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(top_style_bar_rect.shrink2(egui::vec2(6.0, 2.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        bar_ui.spacing_mut().item_spacing.x = 4.0;

        bar_ui.label(egui::RichText::new("■ 조판용지:").size(9.5).strong().color(terra));
        for (preset, icon_lbl) in [
            (PaperSheetPreset::A3Landscape, "■ A3가로"),
            (PaperSheetPreset::A4Landscape, "■ A4가로"),
            (PaperSheetPreset::A4Portrait, "□ A4세로"),
            (PaperSheetPreset::B4Portrait, "□ B4세로"),
            (PaperSheetPreset::B5Portrait, "□ B5세로"),
        ] {
            let is_sel = state.paper_preset == preset;
            let btn = egui::Button::new(
                egui::RichText::new(icon_lbl)
                    .size(9.2)
                    .strong()
                    .color(if is_sel { paper } else { ink }),
            )
            .fill(if is_sel { terra } else { wash })
            .stroke(egui::Stroke::new(1.0_f32, if is_sel { terra } else { border }))
            .corner_radius(3.5);
            if bar_ui.add(btn).clicked() {
                state.paper_preset = preset;
            }
        }
        bar_ui.separator();

        // 곡면 전개 탁본 모드: 평면(기본) ↔ 원통/원뿔/구면 0~360° 전개
        let cur_unroll = state.rubbing_params.unroll_mode;
        let unroll_label = match cur_unroll {
            None => "탁본 평면",
            Some(strata_shade::UnrollMode::Cylindrical) => "원통 전개",
            Some(strata_shade::UnrollMode::Conical) => "원뿔 전개",
            Some(strata_shade::UnrollMode::Spherical) => "구면 전개",
        };
        let mut chosen_unroll = cur_unroll;
        egui::ComboBox::from_id_salt("rubbing_unroll_mode_combo")
            .selected_text(egui::RichText::new(unroll_label).size(9.2))
            .width(86.0)
            .show_ui(&mut bar_ui, |ui| {
                ui.selectable_value(&mut chosen_unroll, None, "평면 탁본");
                ui.selectable_value(
                    &mut chosen_unroll,
                    Some(strata_shade::UnrollMode::Cylindrical),
                    "원통 1:1 전개",
                );
                ui.selectable_value(
                    &mut chosen_unroll,
                    Some(strata_shade::UnrollMode::Conical),
                    "원뿔 전개",
                );
                ui.selectable_value(
                    &mut chosen_unroll,
                    Some(strata_shade::UnrollMode::Spherical),
                    "구면 전개",
                );
            });
        if chosen_unroll != cur_unroll {
            state.rubbing_params.unroll_mode = chosen_unroll;
        }
        bar_ui.separator();
        let roi_btn = egui::Button::new(
            egui::RichText::new(if state.roi_drag_mode {
                "✂ 부분탁본 1:1 드래그중"
            } else {
                "✂ 부분탁본 1:1"
            })
            .size(9.3)
            .strong()
            .color(if state.roi_drag_mode { paper } else { select_blue }),
        )
        .fill(if state.roi_drag_mode {
            select_blue
        } else {
            egui::Color32::from_rgb(235, 243, 252)
        })
        .stroke(egui::Stroke::new(1.1_f32, select_blue))
        .corner_radius(3.5);
        if bar_ui.add(roi_btn).clicked() {
            state.roi_drag_mode = !state.roi_drag_mode;
        }
        if state.rubbing_roi_mm.is_some()
            && bar_ui
                .add(egui::Button::new(egui::RichText::new("↺ 복원").size(9.0)).corner_radius(3.0))
                .clicked()
        {
            state.set_rubbing_roi_mm(None);
            state.roi_drag_mode = false;
        }
        let fill_btn = egui::Button::new(
            egui::RichText::new(if state.line_style.fill_section_solid {
                "■ 단면채움 ON"
            } else {
                "□ 단면채움"
            })
            .size(9.2)
            .strong()
            .color(if state.line_style.fill_section_solid {
                paper
            } else {
                ink
            }),
        )
        .fill(if state.line_style.fill_section_solid {
            ink
        } else {
            wash
        })
        .stroke(egui::Stroke::new(1.0_f32, border))
        .corner_radius(3.5);
        if bar_ui.add(fill_btn).clicked() {
            state.line_style.fill_section_solid = !state.line_style.fill_section_solid;
        }

        // =========================================================================================
        // 5-B2. "프로그램 우측창 상단오른쪽제일끝에 gpu사용여부와 프레임숫자를 작게출력하라"
        // =========================================================================================
        let dt_sec = ui
            .ctx()
            .input(|i| i.stable_dt)
            .clamp(1.0 / 240.0, 0.5);
        let live_fps = (1.0 / dt_sec).round().clamp(30.0, 240.0) as u32;
        let gpu_txt = if let Some(gpu_info) = crate::gpu_backend::primary_hardware_gpu() {
            let gpu_short = gpu_info
                .name
                .replace("NVIDIA GeForce ", "NVIDIA ")
                .replace("AMD Radeon ", "AMD ");
            format!("⚡ GPU: ON ({}) · {} FPS", gpu_short, live_fps)
        } else {
            format!("⚡ GPU: OFF · {} FPS", live_fps)
        };
        let gpu_badge_w = 248.0_f32;
        let gpu_badge_rect = egui::Rect::from_min_size(
            egui::pos2(
                (sheet_rect.right() - gpu_badge_w - 8.0).max(top_style_bar_rect.right() + 6.0),
                sheet_rect.top() + 6.0,
            ),
            egui::vec2(gpu_badge_w, 22.0),
        );
        paper_painter.rect_filled(
            gpu_badge_rect,
            4.0,
            egui::Color32::from_rgb(232, 245, 236),
        );
        paper_painter.rect_stroke(
            gpu_badge_rect,
            4.0,
            egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(38, 128, 68)),
            egui::StrokeKind::Inside,
        );
        paper_painter.text(
            gpu_badge_rect.center(),
            egui::Align2::CENTER_CENTER,
            gpu_txt,
            egui::FontId::monospace(8.5),
            egui::Color32::from_rgb(22, 96, 46),
        );

        // =========================================================================================
        // 5-C. 우측 하단 축척 설정 바 ("우측하단에 축척을 설정할수있게하라. 단면도 평면도는 무조건 들어가야하고 1:1로 기본으로 나오게하라")
        // =========================================================================================
        paper_painter.rect_filled(
            bot_scale_bar_rect,
            4.0,
            egui::Color32::from_rgba_unmultiplied(250, 249, 245, 244),
        );
        paper_painter.rect_stroke(
            bot_scale_bar_rect,
            4.0,
            egui::Stroke::new(1.0_f32, terra),
            egui::StrokeKind::Inside,
        );
        let mut scale_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bot_scale_bar_rect.shrink2(egui::vec2(6.0, 2.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        scale_ui.spacing_mut().item_spacing.x = 4.0;
        scale_ui.label(egui::RichText::new("📐 도면 축척:").size(9.5).strong().color(terra));
        for (preset, lbl) in [
            (ScalePreset::Full1To1, "1/1"),
            (ScalePreset::Half1To2, "1/2"),
            (ScalePreset::Third1To3, "1/3"),
            (ScalePreset::Quarter1To4, "1/4"),
            (ScalePreset::Fifth1To5, "1/5"),
            (ScalePreset::Sixth1To6, "1/6"),
            (ScalePreset::Tenth1To10, "1/10"),
            (ScalePreset::Micro2To1, "2/1"),
        ] {
            let is_sel = state.active_scale_preset == preset;
            let btn = egui::Button::new(
                egui::RichText::new(lbl)
                    .size(9.2)
                    .color(if is_sel { paper } else { ink }),
            )
            .fill(if is_sel { terra } else { wash })
            .stroke(egui::Stroke::new(1.0_f32, if is_sel { terra } else { border }))
            .corner_radius(3.0);
            if scale_ui.add(btn).clicked() {
                state.active_scale_preset = preset;
                state.scale_locked = true;
                state.zoom = match preset {
                    ScalePreset::Full1To1 => 1.0,
                    ScalePreset::Half1To2 => 0.5,
                    ScalePreset::Third1To3 => 1.0 / 3.0,
                    ScalePreset::Quarter1To4 => 0.25,
                    ScalePreset::Micro2To1 => 2.0,
                    _ => 1.0,
                };
            }
        }

        // 6. 『부분 정밀 탁본 창 (ROI Rubbing Studio · 학술 MSII+DoG 엔진)』
        if state.show_roi_rubbing_window {
            let mut open = state.show_roi_rubbing_window;
            egui::Window::new("부분 정밀 탁본 창 (ROI Rubbing Studio · 학술 MSII+DoG 엔진)")
                .open(&mut open)
                .default_pos(egui::pos2(sheet_rect.right() - 365.0, sheet_rect.top() + 52.0))
                .default_width(340.0)
                .resizable(false)
                .collapsible(false)
                .show(ui.ctx(), |ui| {
                    ui.colored_label(
                        muted,
                        "도판 위에서 [⬚ 부분탁본 선택] 후 마우스로 드래그하면 드래그한 1:1 크기로 정밀 탁본이 추출됩니다.",
                    );
                    ui.add_space(4.0);

                    let (prev_rect, _) =
                        ui.allocate_exact_size(egui::vec2(320.0, 128.0), egui::Sense::hover());
                    let pp = ui.painter_at(prev_rect);
                    pp.rect_filled(prev_rect, 4.0, paper);
                    pp.rect_stroke(
                        prev_rect,
                        4.0,
                        egui::Stroke::new(1.0_f32, border),
                        egui::StrokeKind::Inside,
                    );
                    if let Some(ref tex) = state.cached_front_tex {
                        let uv_rect = if let (Some(roi), Some(first_reg)) =
                            (state.rubbing_roi_mm, plate.raster_regions.first())
                        {
                            let rx0 = first_reg.rect_mm[0].min(first_reg.rect_mm[2]);
                            let rx1 = first_reg.rect_mm[0].max(first_reg.rect_mm[2]);
                            let ry0 = first_reg.rect_mm[1].min(first_reg.rect_mm[3]);
                            let ry1 = first_reg.rect_mm[1].max(first_reg.rect_mm[3]);
                            let rw = (rx1 - rx0).max(1.0);
                            let rh = (ry1 - ry0).max(1.0);
                            let u0 = (((roi[0] - rx0) / rw) as f32).clamp(0.0, 0.48);
                            let u1 = (((roi[2] - rx0) / rw) as f32).clamp(u0 + 0.12, 0.50);
                            let v0 = (((ry1 - roi[3]) / rh) as f32).clamp(0.0, 0.85);
                            let v1 = (((ry1 - roi[1]) / rh) as f32).clamp(v0 + 0.12, 1.0);
                            egui::Rect::from_min_max(egui::pos2(u0, v0), egui::pos2(u1, v1))
                        } else {
                            egui::Rect::from_min_max(egui::pos2(0.02, 0.05), egui::pos2(0.49, 0.95))
                        };
                        pp.image(
                            tex.id(),
                            prev_rect.shrink(6.0),
                            uv_rect,
                            egui::Color32::WHITE,
                        );
                    }

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        for st in [
                            AcademicRubbingStyle::WetHanjiInk,
                            AcademicRubbingStyle::DryGraphite,
                            AcademicRubbingStyle::MsiiRidgeEnhance,
                        ] {
                            let is_sel = state.rubbing_params.style == st;
                            if ui.selectable_label(is_sel, st.label_kr()).clicked() {
                                state.rubbing_params.style = st;
                            }
                        }
                    });

                    egui::Grid::new("roi_rubbing_academic_grid")
                        .num_columns(2)
                        .spacing([8.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("먹물 농도:");
                            ui.add(
                                egui::Slider::new(
                                    &mut state.rubbing_params.ink_density_pct,
                                    15.0..=98.0,
                                )
                                .suffix("%")
                                .fixed_decimals(0),
                            );
                            ui.end_row();

                            ui.label("MSII 곡률 감도:");
                            ui.add(
                                egui::Slider::new(
                                    &mut state.rubbing_params.msii_detail_boost,
                                    0.4..=3.0,
                                )
                                .suffix("×")
                                .fixed_decimals(2),
                            );
                            ui.end_row();

                            ui.label("시그모이드 음양각 대비:");
                            ui.add(
                                egui::Slider::new(
                                    &mut state.rubbing_params.sigmoid_contrast,
                                    2.0..=16.0,
                                )
                                .fixed_decimals(1),
                            );
                            ui.end_row();

                            ui.label("한지 여백 임계:");
                            ui.add(
                                egui::Slider::new(
                                    &mut state.rubbing_params.contact_threshold,
                                    -0.40..=0.25,
                                )
                                .fixed_decimals(2),
                            );
                            ui.end_row();
                        });

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if let Some([x0, y0, x1, y1]) = state.rubbing_roi_mm {
                            ui.colored_label(
                                select_blue,
                                format!(
                                    "1:1 크기: {:.1}×{:.1}mm",
                                    (x1 - x0).abs(),
                                    (y1 - y0).abs()
                                ),
                            );
                        } else {
                            ui.colored_label(muted, "현재: 유물 전체 영역 탁본 표시 중");
                        }
                        if ui.button("전체 탁본 복원").clicked() {
                            state.set_rubbing_roi_mm(None);
                            state.roi_drag_mode = false;
                        }
                    });
                });
            state.show_roi_rubbing_window = open;
        }
    });

    (out_header_rect, out_strip_rect, out_sheet_rect)
}

fn draw_dashed_polyline(
    painter: &egui::Painter,
    pts: &[egui::Pos2],
    width_px: f32,
    color: egui::Color32,
    dash_px: f32,
    gap_px: f32,
) {
    let stroke = egui::Stroke::new(width_px, color);
    let period = (dash_px + gap_px).max(2.0);
    for seg in pts.windows(2) {
        let a = seg[0];
        let b = seg[1];
        let delta = b - a;
        let len = delta.length();
        if len < 0.5 {
            continue;
        }
        let dir = delta / len;
        let mut t = 0.0f32;
        while t < len {
            let t_end = (t + dash_px).min(len);
            painter.line_segment([a + dir * t, a + dir * t_end], stroke);
            t += period;
        }
    }
}

/// 헤드리스 검증 투어(`--screenshot-tour`)용 5단 고정 레이아웃 전체 창(`1440 × 900 px`) 스냅샷 PNG를 생성한다.
pub fn render_full_workspace_offscreen_png(
    plate: &ComposedPlate2D,
    mesh: &strata_core::TriMesh,
    sheet_full_tab_mode: bool,
    with_dialog_overlay: bool,
) -> RgbaImage {
    let w = 1440u32;
    let h = 900u32;
    let mut img =
        RgbaImage::from_pixel(w, h, Rgba([GROUND.rgb[0], GROUND.rgb[1], GROUND.rgb[2], 255]));

    let fill_rect = |img: &mut RgbaImage, x0: u32, y0: u32, rw: u32, rh: u32, col: Rgba<u8>| {
        let x1 = (x0 + rw).min(w);
        let y1 = (y0 + rh).min(h);
        for y in y0..y1 {
            for x in x0..x1 {
                img.put_pixel(x, y, col);
            }
        }
    };

    let draw_line = |img: &mut RgbaImage, x0: i32, y0: i32, x1: i32, y1: i32, col: Rgba<u8>| {
        let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
        for s in 0..=steps {
            let px = x0 + (x1 - x0) * s / steps;
            let py = y0 + (y1 - y0) * s / steps;
            if px >= 0 && py >= 0 && (px as u32) < w && (py as u32) < h {
                img.put_pixel(px as u32, py as u32, col);
            }
        }
    };

    let fill_tri =
        |img: &mut RgbaImage, p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], col: Rgba<u8>| {
            let min_x = p0[0].min(p1[0]).min(p2[0]).floor().max(0.0) as i32;
            let max_x = p0[0].max(p1[0]).max(p2[0]).ceil().min((w - 1) as f32) as i32;
            let min_y = p0[1].min(p1[1]).min(p2[1]).floor().max(0.0) as i32;
            let max_y = p0[1].max(p1[1]).max(p2[1]).ceil().min((h - 1) as f32) as i32;
            let denom = (p1[1] - p2[1]) * (p0[0] - p2[0]) + (p2[0] - p1[0]) * (p0[1] - p2[1]);
            if denom.abs() < 1e-5 {
                return;
            }
            let inv = 1.0 / denom;
            for py in min_y..=max_y {
                let fy = py as f32 + 0.5;
                for px in min_x..=max_x {
                    let fx = px as f32 + 0.5;
                    let w0 =
                        ((p1[1] - p2[1]) * (fx - p2[0]) + (p2[0] - p1[0]) * (fy - p2[1])) * inv;
                    let w1 =
                        ((p2[1] - p0[1]) * (fx - p2[0]) + (p0[0] - p2[0]) * (fy - p2[1])) * inv;
                    let w2 = 1.0 - w0 - w1;
                    if w0 >= -1e-4 && w1 >= -1e-4 && w2 >= -1e-4 {
                        img.put_pixel(px as u32, py as u32, col);
                    }
                }
            }
        };

    let ink = Rgba([20, 20, 19, 255]);
    let wash = Rgba([245, 244, 237, 255]);
    let border = Rgba([209, 207, 196, 255]);
    let terra = Rgba([181, 87, 58, 255]);

    fill_rect(&mut img, 0, 0, w, 34, wash);
    fill_rect(&mut img, 0, 34, w, 1, border);
    fill_rect(&mut img, 0, 35, w, 98, Rgba([250, 249, 245, 255]));
    for i in 0..7 {
        let tx = 16 + i * 62;
        let ty = 48;
        let tile_col = if i == 0 { ink } else { wash };
        fill_rect(&mut img, tx, ty, 50, 50, tile_col);
        if i == 0 {
            fill_rect(&mut img, tx, ty + 48, 50, 2, terra);
        }
    }
    fill_rect(&mut img, 0, 132, w, 36, wash);
    fill_rect(&mut img, 0, 168, w, 1, border);
    fill_rect(&mut img, 0, h - 30, w, 30, wash);

    let work_y = 169u32;
    let work_h = h - 30 - work_y;
    fill_rect(&mut img, 0, work_y, 176, work_h, wash);
    fill_rect(&mut img, 176, work_y, 1, work_h, border);

    let rem_w = w - 177;
    let (v3d_w, sheet_x, sheet_w) = if sheet_full_tab_mode {
        (0, 177, rem_w)
    } else {
        let vw = ((rem_w as f32) * 0.27) as u32;
        (vw, 177 + vw, rem_w - vw)
    };

    if v3d_w > 0 {
        let v3d_state = crate::view3d_pane::View3DPaneState::default();
        let v3d_img = crate::view3d_pane::render_mesh_zbuffer_rgba(
            mesh,
            None,
            &v3d_state,
            v3d_w.min(340),
            work_h.min(540),
        );
        for y in 0..work_h {
            let sy = ((y as u64 * v3d_img.height() as u64) / work_h.max(1) as u64) as u32;
            for x in 0..v3d_w {
                let sx = ((x as u64 * v3d_img.width() as u64) / v3d_w.max(1) as u64) as u32;
                let px = *v3d_img.get_pixel(
                    sx.min(v3d_img.width().saturating_sub(1)),
                    sy.min(v3d_img.height().saturating_sub(1)),
                );
                img.put_pixel(177 + x, work_y + y, px);
            }
        }
        fill_rect(&mut img, 177 + v3d_w - 1, work_y, 1, work_h, border);
    }

    fill_rect(&mut img, sheet_x, work_y, sheet_w, 28, wash);
    fill_rect(&mut img, sheet_x, work_y + 28, sheet_w, 1, border);

    let cw = plate.canvas_width_mm.max(10.0);
    let ch = plate.canvas_height_mm.max(10.0);
    let p_scale = (((sheet_w - 48) as f64) / cw)
        .min(((work_h - 64) as f64) / ch)
        .max(0.5);
    let ox = (sheet_x as f64) + (sheet_w as f64) * 0.5 - (cw * p_scale) * 0.5;
    let oy = (work_y as f64) + 32.0 + (work_h as f64 - 40.0) * 0.5 + (ch * p_scale) * 0.5;

    for elem in &plate.paths {
        let col = if elem.stroke_hex == "#B5573A" {
            terra
        } else {
            ink
        };
        if elem.fill_hex.is_some() && elem.points_mm.len() >= 3 {
            let tris = triangulate_polygon_ear_clipping(&elem.points_mm);
            for tri in tris {
                let a = elem.points_mm[tri[0]];
                let b = elem.points_mm[tri[1]];
                let c = elem.points_mm[tri[2]];
                fill_tri(
                    &mut img,
                    [
                        (ox + a[0] * p_scale) as f32,
                        (oy - a[1] * p_scale) as f32,
                    ],
                    [
                        (ox + b[0] * p_scale) as f32,
                        (oy - b[1] * p_scale) as f32,
                    ],
                    [
                        (ox + c[0] * p_scale) as f32,
                        (oy - c[1] * p_scale) as f32,
                    ],
                    col,
                );
            }
        }
        for seg in elem.points_mm.windows(2) {
            let x0 = (ox + seg[0][0] * p_scale).round() as i32;
            let y0 = (oy - seg[0][1] * p_scale).round() as i32;
            let x1 = (ox + seg[1][0] * p_scale).round() as i32;
            let y1 = (oy - seg[1][1] * p_scale).round() as i32;
            draw_line(&mut img, x0, y0, x1, y1, col);
        }
    }

    if let Some(first_reg) = plate.raster_regions.first() {
        let rub_img = render_orthographic_rubbing_rgba(
            mesh,
            first_reg.kind,
            0.0,
            76.0,
            180,
            180,
        );
        let rx0 = (ox + first_reg.rect_mm[0].min(first_reg.rect_mm[2]) * p_scale).round() as i32;
        let rx1 = (ox + first_reg.rect_mm[0].max(first_reg.rect_mm[2]) * p_scale).round() as i32;
        let ry0 = (oy - first_reg.rect_mm[1].max(first_reg.rect_mm[3]) * p_scale).round() as i32;
        let ry1 = (oy - first_reg.rect_mm[1].min(first_reg.rect_mm[3]) * p_scale).round() as i32;
        let rw = (rx1 - rx0).max(1);
        let rh = (ry1 - ry0).max(1);
        let mid_x = rx0 + rw / 2;
        for py in ry0.max(work_y as i32 + 30)..ry1.min((h - 32) as i32) {
            let v = ((py - ry0) as f32 / rh as f32).clamp(0.0, 0.999);
            let sy = (v * rub_img.height() as f32) as u32;
            for px in rx0.max(sheet_x as i32 + 4)..rx1.min((w - 4) as i32) {
                if first_reg.clip_half_sign == Some(-1) && px > mid_x {
                    continue;
                }
                if first_reg.clip_half_sign == Some(1) && px < mid_x {
                    continue;
                }
                let u = ((px - rx0) as f32 / rw as f32).clamp(0.0, 0.999);
                let sx = (u * rub_img.width() as f32) as u32;
                let rp = rub_img.get_pixel(sx, sy);
                if rp[3] > 32 {
                    img.put_pixel(px as u32, py as u32, Rgba([rp[0], rp[1], rp[2], 255]));
                }
            }
        }
    }

    if with_dialog_overlay {
        for y in 0..h {
            for x in 0..w {
                let p = img.get_pixel(x, y).0;
                img.put_pixel(
                    x,
                    y,
                    Rgba([
                        ((u16::from(p[0]) * 180) / 255) as u8,
                        ((u16::from(p[1]) * 180) / 255) as u8,
                        ((u16::from(p[2]) * 180) / 255) as u8,
                        255,
                    ]),
                );
            }
        }
        let mw = 640u32;
        let mh = 420u32;
        let mx = (w - mw) / 2;
        let my = (h - mh) / 2;
        fill_rect(&mut img, mx, my, mw, mh, Rgba([255, 255, 255, 255]));
        fill_rect(&mut img, mx, my, mw, 38, wash);
        fill_rect(&mut img, mx, my + 38, mw, 2, terra);
    }

    img
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::ArtifactCategory;
    use strata_measure::compute_artifact_measurements;

    fn make_patterned_bowl_mesh() -> TriMesh {
        let mut positions = Vec::new();
        let mut triangles = Vec::new();
        let nu = 48usize;
        let nv = 28usize;
        for iv in 0..=nv {
            let v_frac = iv as f64 / nv as f64;
            let z = v_frac * 55.0;
            let groove = if (iv % 4) == 2 { -1.35 } else { 0.35 };
            let r = 26.0 + 28.0 * v_frac.powf(0.8) + groove;
            for iu in 0..=nu {
                let theta = (iu as f64 / nu as f64) * std::f64::consts::TAU;
                let stamp = if (iu % 6) == 0 && iv > 6 && iv < 22 {
                    -1.15
                } else {
                    0.0
                };
                let rr = r + stamp;
                positions.push([rr * theta.cos(), rr * theta.sin(), z]);
            }
        }
        let stride = (nu + 1) as u32;
        for iv in 0..(nv as u32) {
            for iu in 0..(nu as u32) {
                let a = iv * stride + iu;
                let b = a + 1;
                let c = a + stride;
                let d = c + 1;
                triangles.push([a, b, d]);
                triangles.push([a, d, c]);
            }
        }
        TriMesh::from_positions_and_triangles("patterned_bowl", positions, triangles).expect("ok")
    }

    #[test]
    fn test_orthographic_rubbing_raster_produces_visible_ink_within_silhouette() {
        let mesh = make_patterned_bowl_mesh();
        for kind in [
            PlateRasterKind::FrontExteriorRubbing,
            PlateRasterKind::BackInteriorRubbing,
            PlateRasterKind::BottomPlanRubbing,
        ] {
            let img = render_orthographic_rubbing_rgba(&mesh, kind, 0.0, 75.0, 160, 160);
            let mut opaque_px = 0usize;
            let mut ink_shaded_px = 0usize;
            for p in img.pixels() {
                if p[3] > 128 {
                    opaque_px += 1;
                    if p[0] < 215 {
                        ink_shaded_px += 1;
                    }
                }
            }
            assert!(opaque_px > 2_000);
            assert!(ink_shaded_px > 400);
        }
    }

    #[test]
    fn test_rubbing_params_hash_tracks_every_field() {
        let base = AcademicRubbingParams::default();
        let h0 = rubbing_params_hash(&base);
        assert_eq!(h0, rubbing_params_hash(&base.clone()), "해시가 결정론적이어야 함");

        let mut ink = base.clone();
        ink.ink_density_pct = 79.0;
        let mut light = base.clone();
        light.light_azimuth_deg = 316.0;
        let mut style = base.clone();
        style.style = AcademicRubbingStyle::DryGraphite;
        let mut unroll = base.clone();
        unroll.unroll_mode = Some(strata_shade::UnrollMode::Cylindrical);

        for (name, h) in [
            ("먹물농도", rubbing_params_hash(&ink)),
            ("조명방위", rubbing_params_hash(&light)),
            ("표현기법", rubbing_params_hash(&style)),
            ("전개모드", rubbing_params_hash(&unroll)),
        ] {
            assert_ne!(h, h0, "{} 변경이 해시에 반영되지 않음", name);
        }
    }

    #[test]
    fn test_rubbing_provenance_line_records_standard_values() {
        let line = rubbing_provenance_line(&AcademicRubbingParams::default());
        for token in [
            "L-Std 315/45/1.00",
            "MSII 1.45",
            "Ink 78",
            "전개=평면",
            "투영=직교",
        ] {
            assert!(line.contains(token), "출처 문자열에 '{}' 없음: {}", token, line);
        }
        let mut unrolled = AcademicRubbingParams::default();
        unrolled.unroll_mode = Some(strata_shade::UnrollMode::Cylindrical);
        let u_line = rubbing_provenance_line(&unrolled);
        assert!(u_line.contains("전개=원통"));
        assert!(u_line.contains("투영=원통전개"));
    }

    #[test]
    fn test_rubbing_render_is_bit_identical() {
        let mesh = make_patterned_bowl_mesh();
        let params = AcademicRubbingParams::default();
        let a = render_orthographic_rubbing_rgba_with_params(
            &mesh,
            PlateRasterKind::BottomPlanRubbing,
            0.0,
            &params,
            160,
            160,
        );
        let b = render_orthographic_rubbing_rgba_with_params(
            &mesh,
            PlateRasterKind::BottomPlanRubbing,
            0.0,
            &params,
            160,
            160,
        );
        assert_eq!(a.as_raw(), b.as_raw(), "같은 입력인데 렌더 결과가 다름(비결정적)");
    }

    #[test]
    fn test_line_weights_snap_to_house_standard() {
        // 「유물제도선.ai」 규격: 0.12/0.15/0.2/0.3/0.4/0.5/0.8
        assert_eq!(STANDARD_LINE_WEIGHTS_MM.len(), 7);
        assert_eq!(snap_to_standard_weight_mm(0.35), 0.30);
        assert_eq!(snap_to_standard_weight_mm(0.26), 0.30);
        assert_eq!(snap_to_standard_weight_mm(0.22), 0.20);
        assert_eq!(snap_to_standard_weight_mm(0.44), 0.40);
        assert_eq!(snap_to_standard_weight_mm(0.9), 0.80);
        // 기본값 자체가 규격값이어야 한다.
        let d = PlateLineStyleConfig::default();
        for w in [
            d.section_width_mm,
            d.outer_width_mm,
            d.aux_width_mm,
            d.dim_width_mm,
        ] {
            assert!(
                STANDARD_LINE_WEIGHTS_MM.contains(&w),
                "기본 선굵기 {} 가 규격값이 아님",
                w
            );
        }
    }

    #[test]
    fn test_fit_scale_preset_keeps_large_plate_on_paper() {
        // 실물 스캔(Φ223.8, 높이 71.2)의 1:1 도판 캔버스는 A3(420×297)를 넘는다.
        let (c_w, c_h) = (653.0, 381.0);
        assert_eq!(
            fit_scale_preset(c_w, c_h, 420.0, 297.0, 12.0),
            ScalePreset::Half1To2
        );
        // 작은 도판은 1:1 유지.
        assert_eq!(
            fit_scale_preset(240.0, 260.0, 420.0, 297.0, 12.0),
            ScalePreset::Full1To1
        );
        // 아주 큰 유물은 1:3 까지 내려간다.
        assert_eq!(
            fit_scale_preset(900.0, 700.0, 420.0, 297.0, 12.0),
            ScalePreset::Third1To3
        );
        // 어떤 축척으로도 안 들어가면 최소 축척(1:4)을 돌려준다.
        assert_eq!(
            fit_scale_preset(2000.0, 1500.0, 420.0, 297.0, 12.0),
            ScalePreset::Quarter1To4
        );
    }

    #[test]
    fn test_parse_plate_mode_cli() {
        assert_eq!(parse_plate_mode("auto"), None);
        assert_eq!(
            parse_plate_mode("1"),
            Some(strata_export::PlateLayoutMode::ALL[0])
        );
        assert_eq!(
            parse_plate_mode("8"),
            Some(strata_export::PlateLayoutMode::ALL[7])
        );
        assert_eq!(parse_plate_mode("9"), None);
        assert_eq!(parse_plate_mode("0"), None);
        assert_eq!(parse_plate_mode("bogus"), None);
    }

    #[test]
    fn test_parse_unroll_mode_cli() {
        assert_eq!(
            parse_unroll_mode("cylindrical"),
            Some(strata_shade::UnrollMode::Cylindrical)
        );
        assert_eq!(
            parse_unroll_mode("conical"),
            Some(strata_shade::UnrollMode::Conical)
        );
        assert_eq!(
            parse_unroll_mode("spherical"),
            Some(strata_shade::UnrollMode::Spherical)
        );
        assert_eq!(parse_unroll_mode("none"), None);
        assert_eq!(parse_unroll_mode("bogus"), None);
    }

    #[test]
    fn test_unrolled_texture_keeps_physical_aspect_ratio() {
        // 원주 364.4mm × 높이 64.0mm 전개 띠: 텍셀 종횡비가 물리 비율과 같아야 1:1 계측이 성립한다.
        let (w, h) = unrolled_texture_dims([364.4, 64.0], 520);
        assert_eq!(w, 520);
        let expected_h = 520.0 * (64.0 / 364.4);
        assert!(
            ((h as f64) - expected_h).abs() < 2.0,
            "전개 텍스처 높이 {}px 는 물리 비율 {:.1}px 와 어긋남",
            h,
            expected_h
        );
    }

    #[test]
    fn test_seam_spanning_triangle_is_culled_in_unroll() {
        // 원주 364mm 전개도에서 봉합선(±π)을 가로지르는 삼각형은 전체 폭 줄무늬를 만든다.
        let width = 364.0;
        assert!(is_seam_spanning_triangle([-182.0, 0.0, 182.0], width));
        // 정상 삼각형은 컬링하지 않는다.
        assert!(!is_seam_spanning_triangle([-100.0, 0.0, 100.0], width));
        assert!(!is_seam_spanning_triangle([10.0, 12.0, 15.0], width));
        // 폭이 0 이면 판정 불가 → 컬링하지 않는다.
        assert!(!is_seam_spanning_triangle([-1.0, 0.0, 1.0], 0.0));
    }

    #[test]
    fn test_unrolled_cylindrical_rubbing_differs_from_plan_and_has_ink() {
        let mesh = make_patterned_bowl_mesh();
        let planar = AcademicRubbingParams::default();
        let mut unrolled = AcademicRubbingParams::default();
        unrolled.unroll_mode = Some(strata_shade::UnrollMode::Cylindrical);

        let img_plan = render_orthographic_rubbing_rgba_with_params(
            &mesh,
            PlateRasterKind::BottomPlanRubbing,
            0.0,
            &planar,
            200,
            200,
        );
        let img_unroll = render_orthographic_rubbing_rgba_with_params(
            &mesh,
            PlateRasterKind::BottomPlanRubbing,
            0.0,
            &unrolled,
            200,
            200,
        );

        let opaque = |img: &RgbaImage| img.pixels().filter(|p| p[3] > 128).count();
        assert!(
            opaque(&img_unroll) > 1_000,
            "원통 전개 탁본이 비어 있음: {} px",
            opaque(&img_unroll)
        );
        assert_ne!(
            img_plan.as_raw(),
            img_unroll.as_raw(),
            "전개 모드를 켜도 평면 탁본과 동일함"
        );
    }

    #[test]
    fn test_action_bar_shows_for_single_view_only() {
        let visible = [0usize, 1, 2, 3];
        // 선택된 레이어가 있으면 그것 하나만.
        assert_eq!(active_action_bar_layer(Some(2), Some(3), &visible), Some(2));
        // 선택이 없으면 호버된 하나만.
        assert_eq!(active_action_bar_layer(None, Some(3), &visible), Some(3));
        // 둘 다 없으면 아무것도 그리지 않는다(도면 가림 방지).
        assert_eq!(active_action_bar_layer(None, None, &visible), None);
        // 숨겨진 레이어가 선택돼 있으면 무시하고 호버로 되돌아간다.
        assert_eq!(active_action_bar_layer(Some(4), Some(1), &visible), Some(1));
    }

    #[test]
    fn test_chs_plate_title_block_and_no_overlapping_aux_views() {
        let mesh = make_patterned_bowl_mesh();
        let report = compute_artifact_measurements(&mesh, ArtifactCategory::CompleteVessel);
        let (left_c, center_t, center_s, right_c) =
            build_chs_plate_title_block("7지구 9호-7 (완형 토기)", "① 완형 토기 표준 도판", &report);
        assert!(left_c.contains("유적명"));
        assert!(center_t.contains("圖版"));
        assert!(center_s.contains("Plate."));
        assert!(right_c.contains("축척"));
    }

    #[test]
    fn test_tdd5_full_bleed_archaeological_millimeter_graph_paper_grid() {
        let sheet_rect =
            egui::Rect::from_min_size(egui::pos2(400.0, 34.0), egui::vec2(1040.0, 866.0));
        let spec = compute_full_bleed_graph_paper_spec(sheet_rect, 210.0, 140.0, 1.0, egui::Vec2::ZERO);
        assert_eq!(spec.paper_rect, sheet_rect);
        assert!(spec.lines_10mm_x.len() >= 12);
        assert!(spec.lines_10mm_y.len() >= 10);
        assert!(spec.lines_5mm_x.len() >= 10);
        assert!(!spec.cm_labels_top.is_empty());
        assert!(!spec.cm_labels_left.is_empty());
    }

    #[test]
    fn test_tdd6_academic_msii_dog_sigmoid_digital_rubbing_contrast() {
        let mesh = make_patterned_bowl_mesh();
        let params = AcademicRubbingParams {
            ink_density_pct: 80.0,
            msii_detail_boost: 1.45,
            sigmoid_contrast: 8.5,
            contact_threshold: -0.12,
            style: AcademicRubbingStyle::WetHanjiInk,
            ..Default::default()
        };
        let img = render_orthographic_rubbing_rgba_with_params(
            &mesh,
            PlateRasterKind::FrontExteriorRubbing,
            0.0,
            &params,
            220,
            220,
        );
        let mut deep_ink_ridges = 0usize;
        let mut uninked_paper_grooves = 0usize;
        for p in img.pixels() {
            if p[3] > 200 {
                if p[0] <= 75 {
                    deep_ink_ridges += 1;
                } else if p[0] >= 220 {
                    uninked_paper_grooves += 1;
                }
            }
        }
        assert!(deep_ink_ridges > 300);
        assert!(uninked_paper_grooves > 150);
    }

    #[test]
    fn test_tdd7_drag_roi_partial_rubbing_window_and_density_control() {
        let mesh = make_patterned_bowl_mesh();
        let mut state = SheetPaneState::default();
        assert!(state.rubbing_roi_mm.is_none());

        state.set_rubbing_roi_mm(Some([20.0, 15.0, 65.0, 50.0]));
        assert_eq!(state.rubbing_roi_mm, Some([20.0, 15.0, 65.0, 50.0]));

        let src = render_orthographic_rubbing_rgba_with_params(
            &mesh,
            PlateRasterKind::FrontExteriorRubbing,
            0.0,
            &state.rubbing_params,
            200,
            200,
        );
        let full_cimg = rgba_to_color_image_clipped_with_roi(
            &src,
            [10.0, 10.0, 110.0, 80.0],
            Some(-1),
            None,
        );
        let roi_cimg = rgba_to_color_image_clipped_with_roi(
            &src,
            [10.0, 10.0, 110.0, 80.0],
            Some(-1),
            state.rubbing_roi_mm,
        );
        let full_opaque = full_cimg.pixels.iter().filter(|c| c.a() > 100).count();
        let roi_opaque = roi_cimg.pixels.iter().filter(|c| c.a() > 100).count();
        assert!(roi_opaque > 50 && roi_opaque < full_opaque);
    }

    #[test]
    fn test_tdd15_horizontal_section_above_and_outline_section_with_mm_color_controls() {
        // 1. Image 11: 단면 기본값은 검은 채움이 아닌 외곽선(fill_section_solid == false)이어야 함
        let state = SheetPaneState::default();
        assert!(
            !state.line_style.fill_section_solid,
            "cross-section must default to outline-only (no solid black fill)"
        );
        assert!((state.line_style.section_width_mm - 0.40).abs() < 1e-5);

        // 2. "겹치잖아.? 수평횡단면은 아래에만하라고":
        // 완형 토기 도판에서 수평 횡단면(횡단면_외벽윤곽선)이 평면도와 겹치지 않고 우측 하단(vessel_top_y 아래쪽, vessel_right_x 오른쪽)에만 배치되어야 함!
        let mesh = make_patterned_bowl_mesh();
        let m = compute_artifact_measurements(&mesh, ArtifactCategory::CompleteVessel);
        let vsec = strata_section::slice_mesh_with_fixed_plane(
            &mesh,
            None,
            strata_section::CutPlaneOrientation::VerticalLongitudinal,
            0.0,
            0.0,
            0.25,
        );
        let hsec = strata_section::slice_mesh_with_fixed_plane(
            &mesh,
            None,
            strata_section::CutPlaneOrientation::HorizontalTransverse,
            0.0,
            20.0,
            0.25,
        );
        let plate = strata_export::compose_archaeological_plate(&strata_export::PlateComposeInput {
            title: "테스트 완형 토기",
            measurements: &m,
            vertical_section: &vsec,
            horizontal_sections: &[hsec],
            occluded_spans: &[],
            sherd_subtype: None,
            sherd_strategy: None,
            handle_attachment: None,
            bead_subtype: None,
            bead_drill_type: None,
            wado_cut_ratio: None,
            hemp_density_label: None,
            flip_exterior_right: false,
        });
        let vessel_right_x = plate.raster_regions[0].rect_mm[2];
        let vessel_top_y = plate.raster_regions[0].rect_mm[3];
        let hsec_ring = plate
            .paths
            .iter()
            .find(|p| p.layer_id == "횡단면_외벽윤곽선")
            .expect("horizontal section ring must exist");
        let hsec_mean_x: f64 = hsec_ring.points_mm.iter().map(|p| p[0]).sum::<f64>()
            / (hsec_ring.points_mm.len() as f64);
        let hsec_mean_y: f64 = hsec_ring.points_mm.iter().map(|p| p[1]).sum::<f64>()
            / (hsec_ring.points_mm.len() as f64);
        assert!(
            hsec_mean_x > vessel_right_x && hsec_mean_y < vessel_top_y,
            "horizontal cross-section ({hsec_mean_x:.1}, {hsec_mean_y:.1}mm) must be positioned ONLY in the bottom-right column"
        );
    }

    #[test]
    fn test_tdd16_roi_1to1_rubbing_with_under_image_wheels_and_tight_dimensions() {
        let mut params = AcademicRubbingParams::default();
        let init_ink = params.ink_density_pct;
        let init_contrast = params.sigmoid_contrast;
        let init_sharp = params.msii_detail_boost;
        let init_thresh = params.contact_threshold;

        // Image 14: 탁본 이미지 바로 밑 4종 미세조정 휠 스텝 동작 검증
        params.apply_wheel_step(RubbingFineWheelKind::InkDensity, 5);
        params.apply_wheel_step(RubbingFineWheelKind::SigmoidContrast, 4);
        params.apply_wheel_step(RubbingFineWheelKind::MsiiRidgeSharpness, 6);
        params.apply_wheel_step(RubbingFineWheelKind::PaperContactThreshold, -3);

        assert!((params.ink_density_pct - (init_ink + 5.0)).abs() < 1e-6);
        assert!((params.sigmoid_contrast - (init_contrast + 0.8)).abs() < 1e-4);
        assert!((params.msii_detail_boost - (init_sharp + 0.30)).abs() < 1e-4);
        assert!((params.contact_threshold - (init_thresh - 0.03)).abs() < 1e-4);
    }

    #[test]
    fn test_tdd20_paper_sheet_5cm_red_grid_and_non_overlapping_line_widgets() {
        // 1. "방안지 여백은 무한대로 배경을 하고 용지만 붉은선으로 표시하게 하자":
        // 방안선이 용지 경계(paper_bounds_rect) 바깥 여백까지 무한대(sheet_rect 전체)로 확장되고, 용지 경계 사각형이 정확히 산출됨을 검증!
        let sheet_rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 760.0));
        let spec = compute_paper_sheet_graph_spec(
            sheet_rect,
            PaperSheetPreset::A3Landscape,
            1.0,
            egui::Vec2::ZERO,
        );
        assert!(
            spec.paper_bounds_rect.width() < sheet_rect.width()
                && spec.paper_bounds_rect.height() < sheet_rect.height(),
            "paper bounds must sit inside the viewport as the red-bordered publication sheet"
        );
        assert!(
            !spec.lines_50mm_red_x.is_empty() && !spec.lines_50mm_red_y.is_empty(),
            "must generate 5cm (50mm) thin red grid lines"
        );
        assert!(
            spec.lines_10mm_x.iter().any(|&gx| gx < spec.paper_bounds_rect.left() - 2.0)
                && spec.lines_10mm_x.iter().any(|&gx| gx > spec.paper_bounds_rect.right() + 2.0),
            "infinite graph paper grid lines must extend into the margin beyond paper_bounds_rect"
        );

        // 2. Image 2: 모든 선 조정 위젯이 유물이 움직이면 함께 이동하고 줌인/줌아웃 시 절대 중복(겹침)되지 않아야 함!
        for zoom in [0.55_f32, 1.0_f32, 2.2_f32] {
            let rects = compute_non_overlapping_line_widget_rects(
                egui::pos2(480.0, 420.0),
                zoom * 2.5,
                [-62.0, 0.0, 62.0, 63.0],
                [0.0, 118.0, 26.0],
                sheet_rect,
            );
            assert_eq!(rects.len(), 4, "exact 4 line widgets (단면선, 외곽선, 횡단면, 치수선)");
            for i in 0..rects.len() {
                for j in (i + 1)..rects.len() {
                    assert!(
                        !rects[i].intersects(rects[j]),
                        "zoom={zoom}: widget {i} {:?} must NEVER overlap widget {j} {:?}",
                        rects[i],
                        rects[j]
                    );
                }
            }
        }
    }

    #[test]
    fn test_tdd21_all8_feedback_illustrator_vector_plan_measured_hsec_and_individual_flips() {
        // 1. Image 0 & Image 1: 기본값에서 좌측 평단면은 일러스트용 순수 벡터 선화(show_elevation_rubbing_overlay == false)이고,
        //    우측 상단에 1:1 정밀 탁본(BottomPlanRubbing)이 배치됨을 검증
        let state = SheetPaneState::default();
        assert!(
            !state.show_elevation_rubbing_overlay,
            "left column (1:1 plan + 1:1 section) must default to pure Illustrator vector line art"
        );

        let mesh = make_patterned_bowl_mesh();
        let m = compute_artifact_measurements(&mesh, ArtifactCategory::CompleteVessel);
        let vsec = strata_section::slice_mesh_with_fixed_plane(
            &mesh,
            None,
            strata_section::CutPlaneOrientation::VerticalLongitudinal,
            0.0,
            0.0,
            0.25,
        );
        let hsec = strata_section::slice_mesh_with_fixed_plane(
            &mesh,
            None,
            strata_section::CutPlaneOrientation::HorizontalTransverse,
            0.0,
            24.0,
            0.25,
        );
        let plate = strata_export::compose_archaeological_plate(&strata_export::PlateComposeInput {
            title: "테스트 완형 토기",
            measurements: &m,
            vertical_section: &vsec,
            horizontal_sections: &[hsec],
            occluded_spans: &[],
            sherd_subtype: None,
            sherd_strategy: None,
            handle_attachment: None,
            bead_subtype: None,
            bead_drill_type: None,
            wado_cut_ratio: None,
            hemp_density_label: None,
            flip_exterior_right: false,
        });
        assert!(
            plate.paths.iter().any(|p| p.layer_id == "평면도_구연내경선"),
            "1:1 plan view must contain Illustrator concentric vector lines"
        );
        assert!(
            plate.paths.iter().any(|p| p.layer_id == "횡단면_단면_실선"),
            "1:1 horizontal cross-section must contain measured cross-section polygon"
        );
        assert!(
            plate.raster_regions.len() >= 2
                && plate.raster_regions[1].rect_mm[0] > plate.raster_regions[0].rect_mm[2],
            "1:1 precision rubbing must be placed in the right column at 1:1 scale"
        );

        // 2. Image 7: 개별 도면 뷰(0~3)의 90° 회전 · 180° 뒤집기 · 앞으로 연속 뒤집기 동작 검증
        let mut tf = IndividualViewTransform2D::default();
        let center = [100.0, 100.0];
        let p = [120.0, 110.0];
        tf.rotate_cw_90();
        let p_rot90 = tf.transform_point_around(p, center);
        assert!((p_rot90[0] - 110.0).abs() < 1e-6 && (p_rot90[1] - 80.0).abs() < 1e-6);

        let mut tf_flip = IndividualViewTransform2D::default();
        tf_flip.flip_forward_continuous_90();
        assert_eq!(tf_flip.flip_forward_steps, 1);
        tf_flip.flip_forward_continuous_90();
        assert_eq!(tf_flip.flip_forward_steps, 2);
        let p_flip180 = tf_flip.transform_point_around(p, center);
        assert!((p_flip180[0] - 120.0).abs() < 1e-6 && (p_flip180[1] - 90.0).abs() < 1e-6);

        // 3. Image 6: 3D 유물 90° 앞으로뒤집기 · 180° 상하뒤집기 검증
        let mut v3d = crate::view3d_pane::View3DPaneState::default();
        v3d.flip_artifact_forward_90();
        assert!((v3d.tilt_deg - 90.0).abs() < 1e-5);
        v3d.flip_artifact_forward_90();
        assert!((v3d.tilt_deg.abs() - 180.0).abs() < 1e-5);
        v3d.flip_artifact_upside_down_180();
        assert!(v3d.tilt_deg.abs() < 1e-5);
    }

    #[test]
    fn test_tdd22_measured_plan_dashed_fractures_light_control_and_6layer_delete() {
        // 1. Image 0 ("실제 모습이 평면도에 나와야하지않나? 깨진부분은 점선아닌가?"):
        // 구연부 일부가 결손(깨진)된 메쉬에서 extract_topdown_plan_slice_2d가 실제 윤곽과 결손 점선(평면도_결손복원_점선)을 생성함을 검증!
        let mut mesh = make_patterned_bowl_mesh();
        for pos in &mut mesh.positions {
            let ang_deg = pos[1].atan2(pos[0]).to_degrees().rem_euclid(360.0);
            if (25.0..=55.0).contains(&ang_deg) && pos[2] > 42.0 {
                pos[2] = 36.0; // 구연부 결손(깨진 부분) 모사
                pos[0] *= 0.86;
                pos[1] *= 0.86;
            }
        }
        let mut v3d = crate::view3d_pane::View3DPaneState::default();
        let top_plan = crate::view3d_pane::extract_topdown_plan_slice_2d(&mesh, &v3d);
        assert!(
            top_plan.polylines_2d.len() >= 3,
            "top-down plan slice must extract outer/inner rim, foot, and broken sector polylines"
        );
        let m = compute_artifact_measurements(&mesh, ArtifactCategory::CompleteVessel);
        let vsec = crate::view3d_pane::slice_transformed_mesh_world_locked(&mesh, &v3d, true);
        let hsec = crate::view3d_pane::slice_transformed_mesh_world_locked(&mesh, &v3d, false);
        v3d.custom_section_line_mm = Some([[-42.0, -18.0], [42.0, 22.0]]);
        let custom_sec = crate::view3d_pane::slice_custom_line_section_2d(&mesh, &v3d)
            .expect("custom section A-A' must be extracted");

        let plate = strata_export::compose_archaeological_plate(&strata_export::PlateComposeInput {
            title: "결손 구연 완형 토기",
            measurements: &m,
            vertical_section: &vsec,
            horizontal_sections: &[hsec, top_plan, custom_sec],
            occluded_spans: &[],
            sherd_subtype: None,
            sherd_strategy: None,
            handle_attachment: None,
            bead_subtype: None,
            bead_drill_type: None,
            wado_cut_ratio: None,
            hemp_density_label: None,
            flip_exterior_right: false,
        });
        assert!(
            plate
                .paths
                .iter()
                .any(|p| p.layer_id == "평면도_결손복원_점선" && p.dash_pattern_mm.is_some()),
            "chipped/broken rim segment in 1:1 plan view must be rendered as a dashed reconstruction line"
        );
        assert!(
            plate.paths.iter().any(|p| p.layer_id.starts_with("추가단면_")),
            "custom section A-A' must be placed in its dedicated layer"
        );

        // 2. Image 2 ("광원을 조정할수있는 작은창"): 광원 방위각/고도각/세기 벡터 정규화 검증
        v3d.light_azimuth_deg = 225.0;
        v3d.light_elevation_deg = 35.0;
        v3d.light_intensity = 1.35;
        let lvec = v3d.compute_light_direction_vector();
        let llen = (lvec[0] * lvec[0] + lvec[1] * lvec[1] + lvec[2] * lvec[2]).sqrt();
        assert!((llen - 1.0).abs() < 1e-4);

        // 3. 전 레이어 관리 ("레이어 모든것에는 방향조정 90도씩 delete키로 지우기 추가단면선 생성시 이것도 레이어에생성 삭제가능하고 키고끌수있게")
        let mut sheet = SheetPaneState {
            selected_layer_idx: Some(4), // ⑤ 추가 단면 A-A' 선택
            ..SheetPaneState::default()
        };
        sheet.rotate_selected_layer_cw_90();
        assert_eq!(sheet.view_transforms[4].rot_90_steps, 1);
        let deleted = sheet.delete_selected_layer();
        assert_eq!(deleted, Some(4));
        assert!(!sheet.view_transforms[4].visible);
        assert!(
            sheet.request_clear_custom_section,
            "deleting layer 4 (추가 단면 A-A') must also signal clearing custom_section_line_mm"
        );
    }
}
