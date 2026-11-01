use eframe::egui;

#[inline]
#[allow(dead_code)]
pub(crate) fn sample_hanji_paper_grain(x: u32, y: u32, amplitude: f32) -> f32 {
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
    #[allow(dead_code)] // FullBleedGraphPaperSpec 전용(테스트에서만 사용)
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
#[allow(dead_code)] // 테스트(Todo 5)가 방안선 규격을 검증한다(앱 UI 는 compute_paper_sheet_graph_spec 사용).
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
#[allow(dead_code)] // 테스트가 전체 채움 방안선 규격을 검증한다.
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

pub(crate) type SheetRubbingCacheKey = (
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
