use arch3d_core::{Arch3dError, SherdPlateStrategy};
use serde::{Deserialize, Serialize};

/// 소각도 편조각($\Delta\theta < 30^\circ$) 바이어스 제거 원호 피팅 결과 (`Pratt/Taubin + Huber IRLS`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RimCircleFitResult {
    pub center_xy: [f64; 2],
    pub radius_mm: f64,
    pub diameter_mm: f64,
    /// 복원구경 표준오차 $\pm\sigma_D$ (mm)
    pub diameter_std_err_mm: f64,
    /// 잔존 원호 중심각 $\Delta\theta$ (도, `0°~360°`)
    pub arc_span_deg: f64,
    /// 잔존율 ($\Delta\theta / 360^\circ \times 100\%$)
    pub remaining_arc_pct: f64,
    /// 잔존 구간 시작·끝 각도 `[start_deg, end_deg]`
    pub arc_interval_deg: [f64; 2],
    /// 잔존 호 각도에 따라 자동 결정된 도판 전략 (`완전 복원` / `추정 파선 복원` / `3단 병렬 도판`)
    pub plate_strategy: SherdPlateStrategy,
}

/// 2D 점군 `(x, y)` (파단면이 배제된 구연단/외벽 점들)에 대해
/// Taubin 대수 초기해 + Huber 가중치 기하 최소제곱(IRLS Gauss-Newton) 원 피팅을 수행한다.
/// 기존 Kåsa 피팅이 소형 구연편($\Delta\theta < 30^\circ$)에서 반지름을 과소추정하는 바이어스를 제거한다.
pub fn fit_rim_arc_unbiased(
    points_xy: &[[f64; 2]],
    is_body_only: bool,
) -> Result<RimCircleFitResult, Arch3dError> {
    if points_xy.len() < 3 {
        return Err(Arch3dError::InsufficientRimArc {
            arc_deg: 0.0,
            min_req_deg: 20.0,
        });
    }

    // 1. 무게중심 이동 및 스케일 정규화
    let n = points_xy.len() as f64;
    let mx = points_xy.iter().map(|p| p[0]).sum::<f64>() / n;
    let my = points_xy.iter().map(|p| p[1]).sum::<f64>() / n;

    // 2. Taubin (1991) Algebraic Circle Fit initialization
    let (mut cx, mut cy, mut r) = taubin_initial_circle(points_xy, mx, my);

    // 3. Huber IRLS Geometric Gauss-Newton refinement (25 iterations)
    for _iter in 0..25 {
        let mut residuals = Vec::with_capacity(points_xy.len());
        for p in points_xy {
            let dx = p[0] - cx;
            let dy = p[1] - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            residuals.push((dist - r).abs());
        }
        residuals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mad = residuals[residuals.len() / 2].max(1e-4);
        let huber_delta = 1.345 * 1.4826 * mad;

        // Solve 3x3 normal equations J^T W J delta = -J^T W f
        let mut jtwj = [[0.0f64; 3]; 3];
        let mut jtwf = [0.0f64; 3];

        for p in points_xy {
            let dx = p[0] - cx;
            let dy = p[1] - cy;
            let dist = (dx * dx + dy * dy).sqrt().max(1e-9);
            let f_i = dist - r;
            let w = if f_i.abs() <= huber_delta {
                1.0
            } else {
                huber_delta / f_i.abs()
            };
            let j0 = -dx / dist;
            let j1 = -dy / dist;
            let j2 = -1.0;
            let j_row = [j0, j1, j2];
            for a in 0..3 {
                jtwf[a] += w * j_row[a] * f_i;
                for b in 0..3 {
                    jtwj[a][b] += w * j_row[a] * j_row[b];
                }
            }
        }
        // Levenberg-Marquardt damping
        for (d, row) in jtwj.iter_mut().enumerate() {
            row[d] += 1e-6;
        }

        if let Some(step) = solve_3x3(jtwj, [-jtwf[0], -jtwf[1], -jtwf[2]]) {
            cx += step[0];
            cy += step[1];
            r = (r + step[2]).max(0.5);
            if step[0] * step[0] + step[1] * step[1] + step[2] * step[2] < 1e-12 {
                break;
            }
        } else {
            break;
        }
    }

    // 4. 잔존 원호 각도(Arc Span) 및 구간 계산
    let mut angles_deg: Vec<f64> = points_xy
        .iter()
        .map(|p| (p[1] - cy).atan2(p[0] - cx).to_degrees().rem_euclid(360.0))
        .collect();
    angles_deg.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // 가장 큰 빈 구간(Gap)을 찾아 실제 잔존 호 중심각 산출
    let mut max_gap = 0.0;
    let mut gap_after_idx = 0usize;
    for i in 0..angles_deg.len() {
        let a0 = angles_deg[i];
        let a1 = if i + 1 < angles_deg.len() {
            angles_deg[i + 1]
        } else {
            angles_deg[0] + 360.0
        };
        let gap = a1 - a0;
        if gap > max_gap {
            max_gap = gap;
            gap_after_idx = i;
        }
    }
    let arc_span_deg = (360.0 - max_gap).clamp(0.0, 360.0);
    let start_deg = angles_deg[(gap_after_idx + 1) % angles_deg.len()];
    let end_deg = angles_deg[gap_after_idx];

    // 5. 복원구경 불확실성(표준오차 ±σ_D) 추정
    let mut sq_err_sum = 0.0;
    for p in points_xy {
        let dist = ((p[0] - cx).powi(2) + (p[1] - cy).powi(2)).sqrt();
        sq_err_sum += (dist - r).powi(2);
    }
    let rmse = (sq_err_sum / (points_xy.len().saturating_sub(3).max(1) as f64)).sqrt();
    let arc_rad = arc_span_deg.to_radians().max(0.15);
    // 소각도에서 반지름 추정 분산은 (1 / arc_rad^2)에 비례 (Karasik & Smilansky 2008)
    let diameter_std_err_mm = (2.0 * rmse / (arc_rad * (points_xy.len() as f64).sqrt())).max(0.05);

    let plate_strategy = SherdPlateStrategy::from_arc_angle_deg(arc_span_deg, is_body_only);

    Ok(RimCircleFitResult {
        center_xy: [cx, cy],
        radius_mm: r,
        diameter_mm: r * 2.0,
        diameter_std_err_mm,
        arc_span_deg,
        remaining_arc_pct: (arc_span_deg / 360.0) * 100.0,
        arc_interval_deg: [start_deg, end_deg],
        plate_strategy,
    })
}

fn taubin_initial_circle(points: &[[f64; 2]], mx: f64, my: f64) -> (f64, f64, f64) {
    let n = points.len() as f64;
    let mut mxx = 0.0;
    let mut myy = 0.0;
    let mut mxy = 0.0;
    let mut mxz = 0.0;
    let mut myz = 0.0;
    let mut mzz = 0.0;

    for p in points {
        let x = p[0] - mx;
        let y = p[1] - my;
        let z = x * x + y * y;
        mxx += x * x;
        myy += y * y;
        mxy += x * y;
        mxz += x * z;
        myz += y * z;
        mzz += z * z;
    }
    mxx /= n;
    myy /= n;
    mxy /= n;
    mxz /= n;
    myz /= n;
    mzz /= n;

    let mz = mxx + myy;
    let cov_xy = mxx * myy - mxy * mxy;
    let var_z = mzz - mz * mz;
    let a3 = 4.0 * mz;
    let a2 = -3.0 * mz * mz - mzz;
    let a1 = var_z * mz + 4.0 * cov_xy * mz - mxz * mxz - myz * myz;
    let a0 = mxz * (mxz * myy - myz * mxy) + myz * (myz * mxx - mxz * mxy) - var_z * cov_xy;

    // Newton-Raphson root of characteristic polynomial near y=0
    let mut x_root = 0.0;
    for _ in 0..20 {
        let y_val = a0 + x_root * (a1 + x_root * (a2 + x_root * a3));
        let dy = a1 + x_root * (2.0 * a2 + x_root * 3.0 * a3);
        if dy.abs() < 1e-14 {
            break;
        }
        let x_next = (x_root - y_val / dy).max(0.0);
        if (x_next - x_root).abs() < 1e-12 {
            x_root = x_next;
            break;
        }
        x_root = x_next;
    }

    let det = (mxx - x_root) * (myy - x_root) - mxy * mxy;
    if det.abs() > 1e-12 {
        let uc = (mxz * (myy - x_root) - myz * mxy) / (2.0 * det);
        let vc = (myz * (mxx - x_root) - mxz * mxy) / (2.0 * det);
        let r = (uc * uc + vc * vc + mz).sqrt();
        (mx + uc, my + vc, r.max(1.0))
    } else {
        (mx, my, mz.sqrt().max(1.0))
    }
}

pub(crate) fn solve_3x3(a: [[f64; 3]; 3], b: [f64; 3]) -> Option<[f64; 3]> {
    let det = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
        - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
    if det.abs() < 1e-15 {
        return None;
    }
    let inv_det = 1.0 / det;
    let x0 = (b[0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
        - a[0][1] * (b[1] * a[2][2] - a[1][2] * b[2])
        + a[0][2] * (b[1] * a[2][1] - a[1][1] * b[2]))
        * inv_det;
    let x1 = (a[0][0] * (b[1] * a[2][2] - a[1][2] * b[2])
        - b[0] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * b[2] - b[1] * a[2][0]))
        * inv_det;
    let x2 = (a[0][0] * (a[1][1] * b[2] - b[1] * a[2][1])
        - a[0][1] * (a[1][0] * b[2] - b[1] * a[2][0])
        + b[0] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]))
        * inv_det;
    Some([x0, x1, x2])
}
