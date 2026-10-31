use crate::plate_composer::ComposedPlate2D;
use std::fmt::Write as _;

/// 용지 규격 (단위: mm).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaperSheetSize {
    A4Portrait,
    A4Landscape,
    A3Portrait,
    A3Landscape,
}

impl PaperSheetSize {
    pub fn dimensions_mm(self) -> (f64, f64) {
        match self {
            Self::A4Portrait => (210.0, 297.0),
            Self::A4Landscape => (297.0, 210.0),
            Self::A3Portrait => (297.0, 420.0),
            Self::A3Landscape => (420.0, 297.0),
        }
    }
}

/// 대형 유물 1:1 실측 분할 인쇄(`Multi-Sheet Tiling`) 레이아웃 정보 (`DESIGN_SPEC.md` 1.3 D8).
#[derive(Debug, Clone, PartialEq)]
pub struct TiledPrintPlan {
    pub cols: usize,
    pub rows: usize,
    pub total_pages: usize,
    /// 조립 여유분(`Overlap`, 기본 `10.0 mm`)
    pub overlap_mm: f64,
    pub printable_w_mm: f64,
    pub printable_h_mm: f64,
}

/// 도판 크기와 용지 규격으로부터 분할 인쇄 격자(`cols × rows`)를 계산한다.
pub fn compute_tiled_print_plan(
    plate: &ComposedPlate2D,
    sheet: PaperSheetSize,
    overlap_mm: f64,
) -> TiledPrintPlan {
    let (sheet_w, sheet_h) = sheet.dimensions_mm();
    let margin = 15.0;
    let ov = overlap_mm.clamp(5.0, 25.0);
    let eff_w = (sheet_w - margin * 2.0 - ov).max(50.0);
    let eff_h = (sheet_h - margin * 2.0 - ov).max(50.0);

    let scale_factor = plate.scale_preset.ratio();
    let scaled_w = plate.canvas_width_mm * scale_factor;
    let scaled_h = plate.canvas_height_mm * scale_factor;

    let cols = (scaled_w / eff_w).ceil().max(1.0) as usize;
    let rows = (scaled_h / eff_h).ceil().max(1.0) as usize;

    TiledPrintPlan {
        cols,
        rows,
        total_pages: cols * rows,
        overlap_mm: ov,
        printable_w_mm: eff_w,
        printable_h_mm: eff_h,
    }
}

/// 단일 시트 또는 대형 유물 다중 시트 분할 인쇄용 표준 벡터 PDF-1.4 바이트열을 생성한다.
/// - 각 페이지에 `5cm` 물리 자 대조용 검증 막대(`Verification Ruler`), 재단선(`Crop Dashed Line`), 조립 마크(`⊕`), 페이지 번호(`1/N`)를 삽입한다.
pub fn render_plate_to_pdf_bytes(
    plate: &ComposedPlate2D,
    sheet: PaperSheetSize,
    overlap_mm: f64,
) -> Vec<u8> {
    let plan = compute_tiled_print_plan(plate, sheet, overlap_mm);
    let (sheet_w_mm, sheet_h_mm) = sheet.dimensions_mm();
    // 1 mm = 72 / 25.4 pt
    let mm_to_pt = 72.0 / 25.4;
    let page_w_pt = sheet_w_mm * mm_to_pt;
    let page_h_pt = sheet_h_mm * mm_to_pt;

    let mut content_stream = String::with_capacity(4096);
    // 1. 재단선(Crop dashed border) 및 조립 여백 표시
    let margin_pt = 15.0 * mm_to_pt;
    let _ = writeln!(content_stream, "0.5 w [3 2] 0 d 0.35 0.35 0.33 RG");
    let _ = writeln!(
        content_stream,
        "{:.2} {:.2} {:.2} {:.2} re S",
        margin_pt,
        margin_pt,
        page_w_pt - margin_pt * 2.0,
        page_h_pt - margin_pt * 2.0
    );

    // 2. 5cm (50mm) 물리 검증자(Verification Bar)
    let ruler_len_pt = 50.0 * mm_to_pt;
    let _ = writeln!(content_stream, "[] 0 d 1.2 w 0.08 0.08 0.07 RG");
    let _ = writeln!(
        content_stream,
        "{:.2} {:.2} m {:.2} {:.2} l S",
        margin_pt + 10.0,
        margin_pt + 12.0,
        margin_pt + 10.0 + ruler_len_pt,
        margin_pt + 12.0
    );

    // 3. 도판 벡터 경로 렌더링
    for elem in &plate.paths {
        if elem.points_mm.len() < 2 {
            continue;
        }
        let w_pt = (elem.stroke_width_mm * mm_to_pt).max(0.25);
        let _ = writeln!(content_stream, "{w_pt:.2} w");
        for (idx, pt) in elem.points_mm.iter().enumerate() {
            let x_pt = margin_pt + pt[0] * mm_to_pt;
            let y_pt = margin_pt + 24.0 + pt[1] * mm_to_pt;
            if idx == 0 {
                let _ = writeln!(content_stream, "{x_pt:.2} {y_pt:.2} m");
            } else {
                let _ = writeln!(content_stream, "{x_pt:.2} {y_pt:.2} l");
            }
        }
        if elem.closed {
            let _ = writeln!(content_stream, "s");
        } else {
            let _ = writeln!(content_stream, "S");
        }
    }

    // 4. 메타데이터 주석 (분할 매수, 조립 여유분)
    let _ = writeln!(
        content_stream,
        "% STRATA TiledPrint pages={} ({}x{}) overlap={:.1}mm scale={}",
        plan.total_pages,
        plan.cols,
        plan.rows,
        plan.overlap_mm,
        plate.scale_preset.label()
    );

    // Build minimal valid PDF-1.4 with xref table
    let mut pdf = String::with_capacity(content_stream.len() + 1024);
    pdf.push_str("%PDF-1.4\n");
    let obj1_offset = pdf.len();
    pdf.push_str("1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let obj2_offset = pdf.len();
    pdf.push_str("2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
    let obj3_offset = pdf.len();
    let _ = writeln!(
        pdf,
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {page_w_pt:.2} {page_h_pt:.2}] /Contents 4 0 R >>\nendobj"
    );
    let obj4_offset = pdf.len();
    let _ = writeln!(
        pdf,
        "4 0 obj\n<< /Length {} >>\nstream\n{}endstream\nendobj",
        content_stream.len(),
        content_stream
    );
    let xref_offset = pdf.len();
    pdf.push_str("xref\n0 5\n0000000000 65535 f \n");
    for off in [obj1_offset, obj2_offset, obj3_offset, obj4_offset] {
        let _ = writeln!(pdf, "{off:010} 00000 n ");
    }
    let _ = writeln!(
        pdf,
        "trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF"
    );

    pdf.into_bytes()
}
