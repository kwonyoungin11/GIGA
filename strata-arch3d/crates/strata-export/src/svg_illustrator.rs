use crate::plate_composer::ComposedPlate2D;
use std::fmt::Write as _;

/// `1 mm = 5 px` 표준 SVG 출력 스케일 계약 (`V7`).
pub const PX_PER_MM_1TO1: f64 = 5.0;

/// 합성된 2D 도판(`ComposedPlate2D`)을 Adobe Illustrator 호환 다중 레이어 SVG(`유물제도선.ai` / `.svg`)로 직렬화한다.
/// - `1 mm = 5 px` (`PX_PER_MM_1TO1 = 5.0`) 정밀 스케일 계약을 준수한다.
/// - 네 모서리에 `1×1 px` 투명 정렬 앵커 `<rect>`를 삽입하여 래스터 탁본 PNG와 Illustrator에서 0.00 px 오차로 겹쳐지게 한다 (`V7`).
pub fn render_plate_to_illustrator_svg(plate: &ComposedPlate2D) -> String {
    render_plate_to_illustrator_svg_with_meta(plate, "")
}

/// `provenance` 문자열을 SVG `<metadata>` 와 숨김 레이어로 각인해 도판의 재현 근거를 남긴다.
pub fn render_plate_to_illustrator_svg_with_meta(
    plate: &ComposedPlate2D,
    provenance: &str,
) -> String {
    render_plate_to_illustrator_svg_with_options(plate, provenance, false)
}

/// 방안지 격자 포함 여부까지 지정하는 전체 옵션판.
/// 「유물 편집판」 템플릿은 인쇄물에 방안지(1mm/5mm/10mm)를 깔므로 `draw_grid = true` 를 쓴다.
pub fn render_plate_to_illustrator_svg_with_options(
    plate: &ComposedPlate2D,
    provenance: &str,
    draw_grid: bool,
) -> String {
    let w_px = (plate.canvas_width_mm * PX_PER_MM_1TO1).round().max(50.0);
    let h_px = (plate.canvas_height_mm * PX_PER_MM_1TO1).round().max(50.0);

    let mut svg = String::with_capacity(8192);
    let _ = writeln!(
        svg,
        r#"<?xml version="1.0" encoding="UTF-8" standalone="no"?>"#
    );
    let _ = writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w_px:.0}px" height="{h_px:.0}px" viewBox="0 0 {w_px:.0} {h_px:.0}" data-scale-px-per-mm="{PX_PER_MM_1TO1:.1}" data-plate-mode="{:?}">"#,
        plate.mode
    );
    if !provenance.is_empty() {
        let _ = writeln!(
            svg,
            "  <metadata id=\"STRATA_PROVENANCE\">{}</metadata>",
            provenance
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
        );
        let _ = writeln!(
            svg,
            r#"  <g id="출처_비인쇄" display="none"><text x="0" y="0" font-size="1">{}</text></g>"#,
            provenance
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
        );
    }
    let _ = writeln!(
        svg,
        r##"  <rect id="한지_배경" x="0" y="0" width="{w_px:.0}" height="{h_px:.0}" fill="#FAF9F5" />"##
    );

    // 0. 방안지 격자 (1mm / 5mm / 10mm) — 인쇄물에 깔리는 실무 템플릿 규격
    if draw_grid {
        let w_mm = plate.canvas_width_mm;
        let h_mm = plate.canvas_height_mm;
        let px = |mm: f64| mm * PX_PER_MM_1TO1;
        for (step, color, width) in [
            (1.0_f64, "#E6E2D6", "0.10"),
            (5.0, "#D8D2C0", "0.14"),
            (10.0, "#C7C0AA", "0.18"),
        ] {
            let _ = writeln!(
                svg,
                r#"  <path id="방안지_{step:.0}mm" d="" fill="none" stroke="{color}" stroke-width="{width}" />"#
            );
            let mut d = String::new();
            let mut v = step;
            while v < w_mm {
                d.push_str(&format!("M{:.2} 0 V{:.2} ", px(v), px(h_mm)));
                v += step;
            }
            let mut u = step;
            while u < h_mm {
                d.push_str(&format!("M0 {:.2} H{:.2} ", px(u), px(w_mm)));
                u += step;
            }
            // 방금 빈 path 를 실제 좌표로 교체
            let empty = format!(
                r#"  <path id="방안지_{step:.0}mm" d="" fill="none" stroke="{color}" stroke-width="{width}" />"#
            );
            let filled = format!(
                r#"  <path id="방안지_{step:.0}mm" d="{d}" fill="none" stroke="{color}" stroke-width="{width}" />"#
            );
            if let Some(pos) = svg.rfind(&empty) {
                svg.replace_range(pos..pos + empty.len(), &filled);
            }
        }
    }

    // 1. 4-Corner 1x1px transparent alignment anchors (Illustrator 0.00px registration)
    let _ = writeln!(svg, r#"  <g id="정렬_앵커">"#);
    for (ax, ay) in [
        (0.0, 0.0),
        ((w_px - 1.0).max(0.0), 0.0),
        (0.0, (h_px - 1.0).max(0.0)),
        ((w_px - 1.0).max(0.0), (h_px - 1.0).max(0.0)),
    ] {
        let _ = writeln!(
            svg,
            r#"    <rect x="{ax:.0}" y="{ay:.0}" width="1" height="1" fill="none" stroke="none" />"#
        );
    }
    let _ = writeln!(svg, r#"  </g>"#);

    // 2. Vector path layers (grouped by layer_id)
    let mut layer_names: Vec<String> = plate.paths.iter().map(|p| p.layer_id.clone()).collect();
    layer_names.sort();
    layer_names.dedup();

    for layer in &layer_names {
        let _ = writeln!(svg, r#"  <g id="{layer}">"#);
        for elem in plate.paths.iter().filter(|p| &p.layer_id == layer) {
            if elem.points_mm.len() < 2 {
                continue;
            }
            let mut d = String::new();
            for (idx, pt) in elem.points_mm.iter().enumerate() {
                let px = pt[0] * PX_PER_MM_1TO1;
                let py = h_px - pt[1] * PX_PER_MM_1TO1;
                if idx == 0 {
                    let _ = write!(d, "M {px:.2} {py:.2}");
                } else {
                    let _ = write!(d, " L {px:.2} {py:.2}");
                }
            }
            if elem.closed {
                d.push_str(" Z");
            }
            let fill_attr = elem.fill_hex.as_deref().unwrap_or("none");
            let stroke_w_px = (elem.stroke_width_mm * PX_PER_MM_1TO1).max(0.5);
            let dash_attr = if let Some((on_mm, off_mm)) = elem.dash_pattern_mm {
                format!(
                    r#" stroke-dasharray="{:.2},{:.2}""#,
                    on_mm * PX_PER_MM_1TO1,
                    off_mm * PX_PER_MM_1TO1
                )
            } else {
                String::new()
            };
            let _ = writeln!(
                svg,
                r#"    <path d="{d}" fill="{fill_attr}" stroke="{}" stroke-width="{stroke_w_px:.2}" stroke-linecap="round" stroke-linejoin="round"{dash_attr} />"#,
                elem.stroke_hex
            );
        }
        let _ = writeln!(svg, r#"  </g>"#);
    }

    // 3. Callouts & Dynamic Dimension annotations
    let _ = writeln!(svg, r#"  <g id="치수_및_주기">"#);
    for c in &plate.callouts {
        let px = c.pos_mm[0] * PX_PER_MM_1TO1;
        let py = (h_px - c.pos_mm[1] * PX_PER_MM_1TO1).clamp(12.0, h_px - 6.0);
        let escaped = c
            .text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let _ = writeln!(
            svg,
            r#"    <text x="{px:.2}" y="{py:.2}" fill="{}" font-family="Pretendard, JetBrains Mono, sans-serif" font-size="{:.1}pt">{escaped}</text>"#,
            c.color_hex, c.font_size_pt
        );
    }
    let _ = writeln!(svg, r#"  </g>"#);
    let _ = writeln!(svg, "</svg>");

    svg
}
