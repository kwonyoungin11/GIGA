use strata_core::{SurfaceRegion, TriMesh};
use serde::{Deserialize, Serialize};

/// `Structure-from-Sherds++` (Yao & Zhou 2023 + 이면각 능선 그래프) 기반
/// 편조각 3영역 분할 결과 (`외벽 OuterWall` / `내벽 InnerWall` / `파단면 FractureZone`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SherdSegmentation {
    pub vertex_regions: Vec<SurfaceRegion>,
    pub face_regions: Vec<SurfaceRegion>,
    pub outer_vertex_count: usize,
    pub inner_vertex_count: usize,
    pub fracture_vertex_count: usize,
}

impl SherdSegmentation {
    /// 파단면(`FractureZone`)을 100% 배제한 순수 기벽(`OuterWall` 또는 `InnerWall`) 정점 좌표 목록만 반환한다.
    pub fn non_fracture_positions(&self, mesh: &TriMesh) -> Vec<[f64; 3]> {
        mesh.positions
            .iter()
            .zip(&self.vertex_regions)
            .filter_map(|(&p, &r)| {
                if r != SurfaceRegion::FractureZone {
                    Some(p)
                } else {
                    None
                }
            })
            .collect()
    }

    /// 외벽(`OuterWall`) 정점 좌표만 반환한다.
    pub fn outer_wall_positions(&self, mesh: &TriMesh) -> Vec<[f64; 3]> {
        mesh.positions
            .iter()
            .zip(&self.vertex_regions)
            .filter_map(|(&p, &r)| {
                if r == SurfaceRegion::OuterWall {
                    Some(p)
                } else {
                    None
                }
            })
            .collect()
    }
}

/// 이면각(`> 35.0°`), 주곡률 크기(`|k|`), 법선 방향성을 결합하여
/// 편조각 메쉬를 `외벽(OuterWall)` / `내벽(InnerWall)` / `파단면(FractureZone)`으로 자동 분할한다.
pub fn segment_sherd_three_regions(
    mesh: &TriMesh,
    dihedral_ridge_threshold_deg: f64,
    curvature_threshold: f64,
) -> SherdSegmentation {
    let n_verts = mesh.positions.len();
    let n_faces = mesh.triangles.len();
    let center = mesh.bbox.center();

    // 1. 각 정점이 속한 급격한 능선(이면각 > 35° 또는 경계 모서리) 인접 여부 계산
    let mut is_ridge_or_boundary_vert = vec![false; n_verts];
    for c in 0..(n_faces * 3) {
        let tri = mesh.triangles[c / 3];
        let u = tri[c % 3] as usize;
        let v = tri[(c + 1) % 3] as usize;
        if mesh.topology.is_boundary_corner(c) {
            is_ridge_or_boundary_vert[u] = true;
            is_ridge_or_boundary_vert[v] = true;
        } else if let Some(deg) = mesh.corner_dihedral_angle_deg(c) {
            if deg >= dihedral_ridge_threshold_deg {
                is_ridge_or_boundary_vert[u] = true;
                is_ridge_or_boundary_vert[v] = true;
            }
        }
    }

    // 2. 정점별 영역 판정:
    // - 곡률 절댓값이 높거나 능선/경계이면서 법선이 수직/접선 방향(파단 단면 방향)이면 `FractureZone`
    // - 그 외 매끄러운 회전 기벽 중 방사 방향 법선이면 `OuterWall`, 내향 법선이면 `InnerWall`
    let mut vertex_regions = Vec::with_capacity(n_verts);
    let mut outer_vertex_count = 0usize;
    let mut inner_vertex_count = 0usize;
    let mut fracture_vertex_count = 0usize;

    for (i, &is_ridge_or_boundary) in is_ridge_or_boundary_vert.iter().enumerate() {
        let p = mesh.positions[i];
        let n = mesh.normals[i];
        let k_abs = mesh.mean_curvatures[i].abs();

        // Radial vector in XY plane from center
        let rx = p[0] - center[0];
        let ry = p[1] - center[1];
        let r_len = (rx * rx + ry * ry).sqrt();
        let (ux, uy) = if r_len > 1e-6 {
            (rx / r_len, ry / r_len)
        } else {
            (1.0, 0.0)
        };

        let radial_dot = n[0] * ux + n[1] * uy;
        let tangential_dot = (-n[0] * uy + n[1] * ux).abs();

        // 파단면 조건:
        // (a) 높은 평균곡률 거칠기(`k_abs > curvature_threshold`) 또는
        // (b) 능선/경계 인접이면서 법선이 원주 접선 방향(`tangential_dot > 0.68`)을 향하는 측면 파단벽
        let is_fracture = k_abs > curvature_threshold
            || (is_ridge_or_boundary && tangential_dot > 0.68 && radial_dot.abs() < 0.55);

        if is_fracture {
            vertex_regions.push(SurfaceRegion::FractureZone);
            fracture_vertex_count += 1;
        } else if radial_dot >= 0.0 {
            vertex_regions.push(SurfaceRegion::OuterWall);
            outer_vertex_count += 1;
        } else {
            vertex_regions.push(SurfaceRegion::InnerWall);
            inner_vertex_count += 1;
        }
    }

    // 3. 삼각면 다수결 레이블 결정
    let mut face_regions = Vec::with_capacity(n_faces);
    for tri in &mesh.triangles {
        let r0 = vertex_regions[tri[0] as usize];
        let r1 = vertex_regions[tri[1] as usize];
        let r2 = vertex_regions[tri[2] as usize];
        let region = if r0 == SurfaceRegion::FractureZone
            || r1 == SurfaceRegion::FractureZone
            || r2 == SurfaceRegion::FractureZone
        {
            SurfaceRegion::FractureZone
        } else if r0 == SurfaceRegion::InnerWall && r1 == SurfaceRegion::InnerWall {
            SurfaceRegion::InnerWall
        } else {
            SurfaceRegion::OuterWall
        };
        face_regions.push(region);
    }

    SherdSegmentation {
        vertex_regions,
        face_regions,
        outer_vertex_count,
        inner_vertex_count,
        fracture_vertex_count,
    }
}
