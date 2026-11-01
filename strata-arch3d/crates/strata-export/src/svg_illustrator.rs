use crate::plate_composer::ComposedPlate2D;
use std::fmt::Write as _;

/// `1 mm = 5 px` 표준 SVG 출력 스케일 계약 (`V7`).
pub const PX_PER_MM_1TO1: f64 = 5.0;

/// 합성된 2D 도판(`ComposedPlate2D`)을 Adobe Illustrator 호환 다중 레이어 SVG(`유물제도선.ai` / `.svg`)로 직렬화한다.
/// - `1 mm = 5 px` (`PX_PER_MM_1TO1 = 5.0`) 정밀 스케일 계약을 준수한다.
/// - 네 모서리에 `1×1 px` 투명 정렬 앵커 `<rect>`를 삽입하여 래스터 탁본 PNG와 Illustrator에서 0.00 px 오차로 겹쳐지게 한다 (`V7`).
pub fn render_plate_to_illustrator_svg(plate: &ComposedPlate2D) -> String {
    let w_px = (plate.canvas_width_mm * PX_PER_MM_1TO1).round().max(50.0);
    let h_px = (plate.canvas_height_mm * PX_PER_MM_1TO1).round().max(50.0);

    let mut svg = String::with_capacity(8192);
    let _ = writeln!(svg, r#"<?xml version="1.0" encoding="UTF-8" standalone="no"?>"#);
    let _ = writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w_px:.0}px" height="{h_px:.0}px" viewBox="0 0 {w_px:.0} {h_px:.0}" data-scale-px-per-mm="{PX_PER_MM_1TO1:.1}" data-plate-mode="{:?}">"#,
        plate.mode
    );
    let _ = writeln!(
        svg,
        r##"  <rect id="한지_배경" x="0" y="0" width="{w_px:.0}" height="{h_px:.0}" fill="#FAF9F5" />"##
    );

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
