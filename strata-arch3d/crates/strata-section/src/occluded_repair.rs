use serde::{Deserialize, Serialize};

/// 광학 3D 스캐너가 찍지 못한 깊은 내벽 음영부 또는 구슬 관통공 중앙 미스캔 구간을
/// 한국 발굴보고서 도법에 따라 **추정 파선(`dash 2.0, 1.5 mm`)**으로 자동 연결한 구간.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OccludedDashedSpan {
    pub start_pt: [f64; 2],
    pub end_pt: [f64; 2],
    pub control_pt: [f64; 2],
    pub gap_length_mm: f64,
}

/// 끊어진 단면 폴리라인들의 끝점 간격을 검사하여,
/// 미스캔 음영 구간(`min_gap_mm ..= max_gap_mm`)을 부드러운 접선 보간 파선(`OccludedDashedSpan`)으로 생성한다.
pub fn infer_occluded_dashed_spans(
    polylines: &[Vec<[f64; 2]>],
    min_gap_mm: f64,
    max_gap_mm: f64,
) -> Vec<OccludedDashedSpan> {
    let mut spans = Vec::new();
    if polylines.len() < 2 {
        return spans;
    }

    for i in 0..polylines.len() {
        let poly_a = &polylines[i];
        if poly_a.len() < 2 {
            continue;
        }
        let end_a = poly_a[poly_a.len() - 1];
        let prev_a = poly_a[poly_a.len() - 2];
        let tan_x = end_a[0] - prev_a[0];
        let tan_y = end_a[1] - prev_a[1];
        let tan_len = (tan_x * tan_x + tan_y * tan_y).sqrt().max(1e-6);

        let mut best_match: Option<([f64; 2], f64)> = None;
        for (j, poly_b) in polylines.iter().enumerate() {
            if i == j || poly_b.is_empty() {
                continue;
            }
            for &candidate in &[poly_b[0], poly_b[poly_b.len() - 1]] {
                let d =
                    ((candidate[0] - end_a[0]).powi(2) + (candidate[1] - end_a[1]).powi(2)).sqrt();
                if d >= min_gap_mm
                    && d <= max_gap_mm
                    && best_match.is_none_or(|(_, best_d)| d < best_d)
                {
                    best_match = Some((candidate, d));
                }
            }
        }

        if let Some((end_b, dist)) = best_match {
            let mid_x = 0.5 * (end_a[0] + end_b[0]) + (tan_x / tan_len) * (dist * 0.15);
            let mid_y = 0.5 * (end_a[1] + end_b[1]) + (tan_y / tan_len) * (dist * 0.15);
            spans.push(OccludedDashedSpan {
                start_pt: end_a,
                end_pt: end_b,
                control_pt: [mid_x, mid_y],
                gap_length_mm: dist,
            });
        }
    }

    spans
}
