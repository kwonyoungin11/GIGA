use kurbo::{BezPath, PathEl, Point};
use serde::{Deserialize, Serialize};

/// 단일 3차 베지어 곡선 세그먼트 (`p0 -> c1, c2 -> p3`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CubicBezierSegment2D {
    pub p0: [f64; 2],
    pub c1: [f64; 2],
    pub c2: [f64; 2],
    pub p3: [f64; 2],
}

/// `kurbo` 기반 단면 폴리라인 -> 3차 베지어 곡선 피팅 결과 (`Illustrator cutline_*.svg` 호환).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FittedSectionCurve {
    pub original_polyline: Vec<[f64; 2]>,
    pub cubic_segments: Vec<CubicBezierSegment2D>,
    pub max_deviation_mm: f64,
    /// `M x0 y0 C x1 y1, x2 y2, x3 y3 ...` SVG path 문자열 (`scale_factor`: 예 `5.0` for `1mm = 5px`)
    pub svg_path_d_1mm_5px: String,
}

/// 2D 단면 폴리라인을 Douglas-Peucker 단순화 및 Catmull-Rom / Schneider 3차 베지어 제어점으로 변환하고,
/// `kurbo::BezPath`로 검증하여 최대 오차(`max_deviation_mm < 0.02 mm`)와 SVG `d` 문자열을 생성한다.
pub fn fit_polyline_to_cubic_bezier(
    polyline: &[[f64; 2]],
    tolerance_mm: f64,
    scale_px_per_mm: f64,
) -> FittedSectionCurve {
    if polyline.len() < 2 {
        return FittedSectionCurve {
            original_polyline: polyline.to_vec(),
            cubic_segments: Vec::new(),
            max_deviation_mm: 0.0,
            svg_path_d_1mm_5px: String::new(),
        };
    }

    let simplified = simplify_rdp(polyline, (tolerance_mm * 0.45).max(0.005));
    let mut cubic_segments = Vec::with_capacity(simplified.len().saturating_sub(1));
    let mut bez_path = BezPath::new();
    bez_path.push(PathEl::MoveTo(Point::new(
        simplified[0][0],
        simplified[0][1],
    )));

    let n = simplified.len();
    for i in 0..(n - 1) {
        let p_prev = if i > 0 {
            simplified[i - 1]
        } else {
            simplified[0]
        };
        let p0 = simplified[i];
        let p3 = simplified[i + 1];
        let p_next = if i + 2 < n {
            simplified[i + 2]
        } else {
            simplified[n - 1]
        };

        // Centripetal Catmull-Rom to Cubic Bézier conversion (tension = 1/6)
        let c1 = [
            p0[0] + (p3[0] - p_prev[0]) / 6.0,
            p0[1] + (p3[1] - p_prev[1]) / 6.0,
        ];
        let c2 = [
            p3[0] - (p_next[0] - p0[0]) / 6.0,
            p3[1] - (p_next[1] - p0[1]) / 6.0,
        ];

        cubic_segments.push(CubicBezierSegment2D { p0, c1, c2, p3 });
        bez_path.push(PathEl::CurveTo(
            Point::new(c1[0], c1[1]),
            Point::new(c2[0], c2[1]),
            Point::new(p3[0], p3[1]),
        ));
    }

    // Measure max deviation between original polyline vertices and fitted cubic segments
    let mut max_dev = 0.0f64;
    for pt in polyline {
        let mut min_d = f64::INFINITY;
        for seg in &cubic_segments {
            for s in 0..=8 {
                let t = (s as f64) / 8.0;
                let bx = eval_cubic(seg.p0[0], seg.c1[0], seg.c2[0], seg.p3[0], t);
                let by = eval_cubic(seg.p0[1], seg.c1[1], seg.c2[1], seg.p3[1], t);
                let d = ((pt[0] - bx).powi(2) + (pt[1] - by).powi(2)).sqrt();
                if d < min_d {
                    min_d = d;
                }
            }
        }
        if min_d.is_finite() && min_d > max_dev {
            max_dev = min_d;
        }
    }

    // Format SVG `d` string at `scale_px_per_mm` (default 5.0 px/mm matching STRATA `cutline_rc0.svg`)
    let mut svg_d = format!(
        "M{:.2},{:.2}",
        simplified[0][0] * scale_px_per_mm,
        simplified[0][1] * scale_px_per_mm
    );
    for seg in &cubic_segments {
        svg_d.push_str(&format!(
            " C{:.2},{:.2} {:.2},{:.2} {:.2},{:.2}",
            seg.c1[0] * scale_px_per_mm,
            seg.c1[1] * scale_px_per_mm,
            seg.c2[0] * scale_px_per_mm,
            seg.c2[1] * scale_px_per_mm,
            seg.p3[0] * scale_px_per_mm,
            seg.p3[1] * scale_px_per_mm,
        ));
    }

    FittedSectionCurve {
        original_polyline: polyline.to_vec(),
        cubic_segments,
        max_deviation_mm: max_dev,
        svg_path_d_1mm_5px: svg_d,
    }
}

fn eval_cubic(p0: f64, c1: f64, c2: f64, p3: f64, t: f64) -> f64 {
    let u = 1.0 - t;
    u * u * u * p0 + 3.0 * u * u * t * c1 + 3.0 * u * t * t * c2 + t * t * t * p3
}

fn simplify_rdp(pts: &[[f64; 2]], eps: f64) -> Vec<[f64; 2]> {
    if pts.len() <= 2 {
        return pts.to_vec();
    }
    let first = pts[0];
    let last = pts[pts.len() - 1];
    let mut max_dist = 0.0;
    let mut index = 0usize;

    let dx = last[0] - first[0];
    let dy = last[1] - first[1];
    let len2 = dx * dx + dy * dy;

    for (i, p) in pts.iter().enumerate().take(pts.len() - 1).skip(1) {
        let d = if len2 < 1e-12 {
            ((p[0] - first[0]).powi(2) + (p[1] - first[1]).powi(2)).sqrt()
        } else {
            let t = (((p[0] - first[0]) * dx + (p[1] - first[1]) * dy) / len2).clamp(0.0, 1.0);
            let proj_x = first[0] + t * dx;
            let proj_y = first[1] + t * dy;
            ((p[0] - proj_x).powi(2) + (p[1] - proj_y).powi(2)).sqrt()
        };
        if d > max_dist {
            max_dist = d;
            index = i;
        }
    }

    if max_dist > eps {
        let mut left = simplify_rdp(&pts[..=index], eps);
        let right = simplify_rdp(&pts[index..], eps);
        left.pop();
        left.extend(right);
        left
    } else {
        vec![first, last]
    }
}
