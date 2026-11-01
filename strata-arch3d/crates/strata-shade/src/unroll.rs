use serde::{Deserialize, Serialize};
use strata_core::TriMesh;

/// 3D 곡면 문양(`타날문·인화문·연판문·각목문`) 무왜곡 `0° ~ 360°` 전개(Rollout / Unroll) 투영 방식.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnrollMode {
    /// 원통형 전개 ($s = \bar{R}\theta$, $h = z$) — 토기 동체부·관옥·수키와
    Cylindrical,
    /// 원뿔형 부채꼴 전개 — 대각(고배)·접시·완 외면
    Conical,
    /// 구면 등거리 전개 — 호(壺) 구형 동체·환옥
    Spherical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnrolledPoint2D {
    pub vertex_index: u32,
    /// 원주 호 길이 좌표 $s \in [-\pi\bar{R}, +\pi\bar{R}]$ (mm)
    pub arc_s_mm: f64,
    /// 수직 또는 모선 높이 좌표 $h$ (mm)
    pub height_h_mm: f64,
    /// 반경 편차 부조 깊이 $\Delta r = r - \bar{R}(z)$ (mm)
    pub relief_depth_mm: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnrollResult {
    pub mode: UnrollMode,
    pub mean_radius_mm: f64,
    pub unrolled_width_mm: f64,
    pub unrolled_height_mm: f64,
    pub points: Vec<UnrolledPoint2D>,
}

/// 회전체 또는 곡면 유물 메쉬를 `0° ~ 360°` 무왜곡 평면 전개도(`UnrollResult`)로 투영한다.
pub fn unroll_mesh_surface(mesh: &TriMesh, mode: UnrollMode) -> UnrollResult {
    let center = mesh.bbox.center();
    let z_min = mesh.bbox.min[2];
    let z_max = mesh.bbox.max[2];
    let height_span = (z_max - z_min).max(1.0);

    let mut r_sum = 0.0;
    let mut r_count = 0usize;
    for p in &mesh.positions {
        let r = ((p[0] - center[0]).powi(2) + (p[1] - center[1]).powi(2)).sqrt();
        if r > 0.5 {
            r_sum += r;
            r_count += 1;
        }
    }
    let mean_radius_mm = if r_count > 0 {
        r_sum / (r_count as f64)
    } else {
        25.0
    };

    let mut points = Vec::with_capacity(mesh.positions.len());
    for (idx, p) in mesh.positions.iter().enumerate() {
        let dx = p[0] - center[0];
        let dy = p[1] - center[1];
        let dz = p[2] - z_min;
        let r = (dx * dx + dy * dy).sqrt();
        let theta = dy.atan2(dx); // [-pi, +pi]

        let (arc_s_mm, height_h_mm) = match mode {
            UnrollMode::Cylindrical => (mean_radius_mm * theta, dz),
            UnrollMode::Conical => (r * theta, (r * r + dz * dz).sqrt()),
            UnrollMode::Spherical => {
                let z_rel = p[2] - center[2];
                let rho = (r * r + z_rel * z_rel).sqrt().max(1.0);
                let phi = (z_rel / rho).clamp(-1.0, 1.0).asin();
                (
                    mean_radius_mm * phi.cos() * theta,
                    mean_radius_mm * (phi + std::f64::consts::FRAC_PI_2),
                )
            }
        };

        points.push(UnrolledPoint2D {
            vertex_index: idx as u32,
            arc_s_mm,
            height_h_mm,
            relief_depth_mm: r - mean_radius_mm,
        });
    }

    UnrollResult {
        mode,
        mean_radius_mm,
        unrolled_width_mm: std::f64::consts::TAU * mean_radius_mm,
        unrolled_height_mm: height_span,
        points,
    }
}
