use arch3d_core::{StacklessLbvh, TriMesh};
use serde::{Deserialize, Serialize};

/// 월드 고정 절단틀(`P_cut`) 방향 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CutPlaneOrientation {
    /// 수직 종단면 (`Y = offset_mm` 평면, 유물은 Z축 둘레로 `psi_deg`만큼 회전)
    VerticalLongitudinal,
    /// 수평 횡단면 (`Z = offset_mm` 평면)
    HorizontalTransverse,
}

/// 2D 절단면 위의 교차 선분 (`[p0, p1]`) 및 체이닝된 폴리라인 결과.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaneSliceResult {
    pub orientation: CutPlaneOrientation,
    pub psi_deg: f64,
    pub offset_mm: f64,
    /// 원시 교차 선분 목록 (`[ [x0, y0], [x1, y1] ]`)
    pub segments_2d: Vec<[[f64; 2]; 2]>,
    /// 끝점 병합(`merge_tolerance_mm`)으로 연결된 연속 폴리라인 목록
    pub polylines_2d: Vec<Vec<[f64; 2]>>,
}

/// 월드 공간에 고정된 절단틀(`P_cut`)에 대해 유물 메쉬(`TriMesh`)를 `psi_deg`만큼 회전시켜
/// 교차 단면 선분과 연속 폴리라인을 추출한다 (`StacklessLbvh` 가속).
pub fn slice_mesh_with_fixed_plane(
    mesh: &TriMesh,
    bvh: Option<&StacklessLbvh>,
    orientation: CutPlaneOrientation,
    psi_deg: f64,
    offset_mm: f64,
    merge_tolerance_mm: f64,
) -> PlaneSliceResult {
    let psi_rad = psi_deg.to_radians();
    let cos_p = psi_rad.cos();
    let sin_p = psi_rad.sin();

    // Local-to-world rotation by +psi means world plane normal rotated into local space by -psi:
    // World Y = x*sin(psi) + y*cos(psi) = offset_mm
    let (local_normal, plane_d) = match orientation {
        CutPlaneOrientation::VerticalLongitudinal => ([sin_p, cos_p, 0.0], offset_mm),
        CutPlaneOrientation::HorizontalTransverse => ([0.0, 0.0, 1.0], offset_mm),
    };

    let candidate_tris: Vec<u32> = if let Some(tree) = bvh {
        tree.query_plane_triangles(local_normal, plane_d)
    } else {
        (0..mesh.triangles.len() as u32).collect()
    };

    let mut segments_2d = Vec::new();
    for ti in candidate_tris {
        let tri = mesh.triangles[ti as usize];
        let idxs = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
        let mut world_pts = [[0.0f64; 3]; 3];
        let mut d_vals = [0.0f64; 3];

        for k in 0..3 {
            let p = mesh.positions[idxs[k]];
            let wx = p[0] * cos_p - p[1] * sin_p;
            let wy = p[0] * sin_p + p[1] * cos_p;
            let wz = p[2];
            world_pts[k] = [wx, wy, wz];
            d_vals[k] = match orientation {
                CutPlaneOrientation::VerticalLongitudinal => wy - offset_mm,
                CutPlaneOrientation::HorizontalTransverse => wz - offset_mm,
            };
        }

        if (d_vals[0] > 0.0 && d_vals[1] > 0.0 && d_vals[2] > 0.0)
            || (d_vals[0] < 0.0 && d_vals[1] < 0.0 && d_vals[2] < 0.0)
        {
            continue;
        }

        let pairs = [(0usize, 1usize), (1, 2), (2, 0)];
        let mut hits = Vec::with_capacity(3);
        for &(a, b) in &pairs {
            let da = d_vals[a];
            let db = d_vals[b];
            if (da >= 0.0 && db < 0.0) || (da < 0.0 && db >= 0.0) {
                let denom = da - db;
                let t = if denom.abs() > 1e-12 { da / denom } else { 0.5 };
                let hx = world_pts[a][0] + t * (world_pts[b][0] - world_pts[a][0]);
                let hy = world_pts[a][1] + t * (world_pts[b][1] - world_pts[a][1]);
                let hz = world_pts[a][2] + t * (world_pts[b][2] - world_pts[a][2]);
                let pt_2d = match orientation {
                    CutPlaneOrientation::VerticalLongitudinal => [hx, hz],
                    CutPlaneOrientation::HorizontalTransverse => [hx, hy],
                };
                hits.push(pt_2d);
            }
        }
        if hits.len() >= 2 {
            let dx = hits[0][0] - hits[1][0];
            let dy = hits[0][1] - hits[1][1];
            if dx * dx + dy * dy > 1e-10 {
                segments_2d.push([hits[0], hits[1]]);
            }
        }
    }

    let polylines_2d = chain_segments_into_polylines(&segments_2d, merge_tolerance_mm.max(0.05));

    PlaneSliceResult {
        orientation,
        psi_deg,
        offset_mm,
        segments_2d,
        polylines_2d,
    }
}

fn chain_segments_into_polylines(
    segments: &[[[f64; 2]; 2]],
    tol_mm: f64,
) -> Vec<Vec<[f64; 2]>> {
    let mut used = vec![false; segments.len()];
    let tol2 = tol_mm * tol_mm;
    let mut polylines = Vec::new();

    for i in 0..segments.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let mut chain = vec![segments[i][0], segments[i][1]];

        loop {
            let mut extended = false;
            let tail = *chain.last().unwrap_or(&[0.0, 0.0]);
            for (j, seg) in segments.iter().enumerate() {
                if used[j] {
                    continue;
                }
                let d0 = (seg[0][0] - tail[0]).powi(2) + (seg[0][1] - tail[1]).powi(2);
                let d1 = (seg[1][0] - tail[0]).powi(2) + (seg[1][1] - tail[1]).powi(2);
                if d0 <= tol2 {
                    used[j] = true;
                    chain.push(seg[1]);
                    extended = true;
                    break;
                } else if d1 <= tol2 {
                    used[j] = true;
                    chain.push(seg[0]);
                    extended = true;
                    break;
                }
            }
            if !extended {
                break;
            }
        }
        polylines.push(chain);
    }

    polylines.sort_by_key(|b| std::cmp::Reverse(b.len()));
    polylines
}
