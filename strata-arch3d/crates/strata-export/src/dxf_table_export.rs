use crate::plate_composer::ComposedPlate2D;
use strata_measure::ArtifactMeasurementReport;
use std::fmt::Write as _;

/// 합성된 2D 도판(`ComposedPlate2D`)을 AutoCAD 호환 `DXF R12` 텍스트 포맷으로 변환한다.
pub fn export_plate_to_dxf_r12(plate: &ComposedPlate2D) -> String {
    let mut dxf = String::with_capacity(4096);
    dxf.push_str("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n0\nENDSEC\n");
    dxf.push_str("0\nSECTION\n2\nENTITIES\n");

    for elem in &plate.paths {
        if elem.points_mm.len() < 2 {
            continue;
        }
        let layer = if elem.dash_pattern_mm.is_some() {
            "RECON_DASHED"
        } else {
            "SECTION_SOLID"
        };
        for seg in elem.points_mm.windows(2) {
            let p0 = seg[0];
            let p1 = seg[1];
            let _ = write!(
                dxf,
                "0\nLINE\n8\n{layer}\n10\n{:.4}\n20\n{:.4}\n30\n0.0\n11\n{:.4}\n21\n{:.4}\n31\n0.0\n",
                p0[0], p0[1], p1[0], p1[1]
            );
        }
        if elem.closed {
            if let (Some(first), Some(last)) = (elem.points_mm.first(), elem.points_mm.last()) {
                let _ = write!(
                    dxf,
                    "0\nLINE\n8\n{layer}\n10\n{:.4}\n20\n{:.4}\n30\n0.0\n11\n{:.4}\n21\n{:.4}\n31\n0.0\n",
                    last[0], last[1], first[0], first[1]
                );
            }
        }
    }

    dxf.push_str("0\nENDSEC\n0\nEOF\n");
    dxf
}

/// 발굴조사 보고서 유물 속성표(`기고·구경·저경·최대경·두께·복원신뢰도·천공유형`)를 CSV 포맷으로 생성한다.
pub fn export_measurements_to_csv(rows: &[(&str, &ArtifactMeasurementReport)]) -> String {
    let mut csv = String::from(
        "\u{FEFF}유물번호,분류,기고_전장(mm),구경_외경(mm),저경_폭(mm),최대동경(mm),평균두께(mm),잔존율(%),권장축척\n",
    );
    for (title, m) in rows {
        let arc = m
            .remaining_arc_pct
            .map(|v| format!("{v:.1}"))
            .unwrap_or_else(|| String::from("-"));
        let _ = writeln!(
            csv,
            "{},{},{:.2},{:.2},{:.2},{:.2},{:.2},{},{}",
            title,
            m.category.label_kr(),
            m.height_or_length_mm,
            m.rim_or_outer_diameter_mm,
            m.base_diameter_or_width_mm,
            m.max_belly_diameter_mm,
            m.mean_wall_thickness_mm,
            arc,
            m.recommended_scale.label()
        );
    }
    csv
}

/// 발굴조사 보고서 유물 속성표를 Markdown 표로 생성한다.
pub fn export_measurements_to_markdown(rows: &[(&str, &ArtifactMeasurementReport)]) -> String {
    let mut md = String::from(
        "| 유물번호 | 분류 | 기고/전장(mm) | 구경/외경(mm) | 저경/폭(mm) | 최대동경(mm) | 두께(mm) | 권장축척 |\n|---|---|---:|---:|---:|---:|---:|---|\n",
    );
    for (title, m) in rows {
        let _ = writeln!(
            md,
            "| {} | {} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} | {} |",
            title,
            m.category.label_kr(),
            m.height_or_length_mm,
            m.rim_or_outer_diameter_mm,
            m.base_diameter_or_width_mm,
            m.max_belly_diameter_mm,
            m.mean_wall_thickness_mm,
            m.recommended_scale.label()
        );
    }
    md
}
