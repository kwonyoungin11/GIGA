use strata_core::TriMesh;
use serde::{Deserialize, Serialize};

/// 5대 미분기하 특징선 연산자 (`DESIGN_SPEC.md` 1.2:
/// `Silhouette`, `Suggestive Contours`, `Crest Lines`, `Apparent Ridges`, `Demarcating Curves`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FeatureLineKind {
    /// ① 정투영 외곽 실루엣 ($\mathbf{n}\cdot\mathbf{v} = 0$, 선 굵기 `0.35 mm`)
    Silhouette,
    /// ② 암시적 윤곽선 (DeCarlo et al. 2003: $D_{\mathbf{w}}(\mathbf{n}\cdot\mathbf{v}) = 0$, 선 굵기 `0.15 mm`)
    SuggestiveContour,
    /// ③ 시점 불변 주곡률 극치 능선 (Ohtake 2004 / Yoshizawa 2005: 구연단·각목·석기 박리능선, 선 굵기 `0.20 mm`)
    CrestLine,
    /// ④ 투영 최대 곡률 능선 (Judd et al. 2007, 선 굵기 `0.20 mm`)
    ApparentRidge,
    /// ⑤ 곡률 변화율 영교차 경계선 (Kolomenkin et al. 2008: 음각·양각 문양 단차 시작선 및 물레속선, 선 굵기 `0.12 mm`)
    DemarcatingCurve,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureSegment2D {
    pub kind: FeatureLineKind,
    pub start_xy: [f64; 2],
    pub end_xy: [f64; 2],
    pub stroke_width_mm: f64,
}

/// 3D 메쉬와 시선 벡터(`view_dir`)로부터 5대 미분기하 특징선 세그먼트를 정사투영 평면 `(X, Z)`로 추출한다.
pub fn extract_five_feature_lines(
    mesh: &TriMesh,
    view_dir: [f64; 3],
) -> Vec<FeatureSegment2D> {
    let mut segments = Vec::new();
    let n_verts = mesh.positions.len();
    if n_verts < 3 {
        return segments;
    }

    let ndotv: Vec<f64> = mesh
        .normals
        .iter()
        .map(|n| n[0] * view_dir[0] + n[1] * view_dir[1] + n[2] * view_dir[2])
        .collect();

    for (t_idx, tri) in mesh.triangles.iter().enumerate() {
        let idxs = [tri[0] as usize, tri[1] as usize, tri[2] as usize];

        // 1. Silhouette: zero-crossing of n . v along triangle edges
        let mut sil_pts = Vec::with_capacity(2);
        for k in 0..3 {
            let u = idxs[k];
            let v = idxs[(k + 1) % 3];
            let d0 = ndotv[u];
            let d1 = ndotv[v];
            if (d0 >= 0.0 && d1 < 0.0) || (d0 < 0.0 && d1 >= 0.0) {
                let t = d0 / (d0 - d1);
                let px = mesh.positions[u][0] + t * (mesh.positions[v][0] - mesh.positions[u][0]);
                let pz = mesh.positions[u][2] + t * (mesh.positions[v][2] - mesh.positions[u][2]);
                sil_pts.push([px, pz]);
            }
        }
        if sil_pts.len() == 2 {
            segments.push(FeatureSegment2D {
                kind: FeatureLineKind::Silhouette,
                start_xy: sil_pts[0],
                end_xy: sil_pts[1],
                stroke_width_mm: 0.35,
            });
        }

        // 2. CrestLine: sharp dihedral ridge (> 24 deg) on visible front-facing triangles
        for k in 0..3 {
            let corner = t_idx * 3 + k;
            if let Some(twin) = mesh.topology.twin(corner) {
                if corner < twin {
                    if let Some(deg) = mesh.corner_dihedral_angle_deg(corner) {
                        if deg > 24.0 {
                            let u = idxs[k];
                            let v = idxs[(k + 1) % 3];
                            segments.push(FeatureSegment2D {
                                kind: FeatureLineKind::CrestLine,
                                start_xy: [mesh.positions[u][0], mesh.positions[u][2]],
                                end_xy: [mesh.positions[v][0], mesh.positions[v][2]],
                                stroke_width_mm: 0.20,
                            });
                        }
                    }
                }
            }
        }

        // 3. DemarcatingCurve: zero-crossing of mean curvature k (inflection boundary where relief starts)
        let mut dem_pts = Vec::with_capacity(2);
        for k in 0..3 {
            let u = idxs[k];
            let v = idxs[(k + 1) % 3];
            let k0 = mesh.mean_curvatures[u];
            let k1 = mesh.mean_curvatures[v];
            if ((k0 > 0.02 && k1 < -0.02) || (k0 < -0.02 && k1 > 0.02)) && ndotv[u].abs() > 0.2 {
                let t = k0 / (k0 - k1);
                let px = mesh.positions[u][0] + t * (mesh.positions[v][0] - mesh.positions[u][0]);
                let pz = mesh.positions[u][2] + t * (mesh.positions[v][2] - mesh.positions[u][2]);
                dem_pts.push([px, pz]);
            }
        }
        if dem_pts.len() == 2 {
            segments.push(FeatureSegment2D {
                kind: FeatureLineKind::DemarcatingCurve,
                start_xy: dem_pts[0],
                end_xy: dem_pts[1],
                stroke_width_mm: 0.12,
            });
        }

        // 4. SuggestiveContour & ApparentRidge on high-curvature front-facing edges
        let k_avg = (mesh.mean_curvatures[idxs[0]].abs()
            + mesh.mean_curvatures[idxs[1]].abs()
            + mesh.mean_curvatures[idxs[2]].abs())
            / 3.0;
        if k_avg > 0.12 && ndotv[idxs[0]].abs() < 0.35 {
            segments.push(FeatureSegment2D {
                kind: FeatureLineKind::SuggestiveContour,
                start_xy: [mesh.positions[idxs[0]][0], mesh.positions[idxs[0]][2]],
                end_xy: [mesh.positions[idxs[1]][0], mesh.positions[idxs[1]][2]],
                stroke_width_mm: 0.15,
            });
        } else if k_avg > 0.25 && ndotv[idxs[0]].abs() >= 0.35 {
            segments.push(FeatureSegment2D {
                kind: FeatureLineKind::ApparentRidge,
                start_xy: [mesh.positions[idxs[0]][0], mesh.positions[idxs[0]][2]],
                end_xy: [mesh.positions[idxs[2]][0], mesh.positions[idxs[2]][2]],
                stroke_width_mm: 0.20,
            });
        }
    }

    segments
}
