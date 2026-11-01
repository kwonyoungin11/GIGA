pub mod metrics;

pub use metrics::{compute_artifact_measurements, ArtifactMeasurementReport};

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::{ArtifactCategory, ScalePreset, TriMesh};

    /// 합성 중공(中空) 항아리 메쉬: 높이별 반경 프로파일을 회전시켜 외벽·내벽 두 겹을 만든다.
    /// 반환값은 `(메쉬, 외벽 최대 반경)` — 최대복경은 중앙 높이에서 발생한다.
    fn hollow_jar_mesh(
        height: f64,
        r_top: f64,
        r_belly: f64,
        r_base: f64,
        wall: f64,
    ) -> TriMesh {
        const RING: usize = 48;
        let t_levels = [0.0_f64, 0.25, 0.5, 0.75, 1.0];
        let radii = [
            r_base,
            r_base + (r_belly - r_base) * 0.8,
            r_belly,
            r_belly - (r_belly - r_top) * 0.4,
            r_top,
        ];

        let mut positions = Vec::new();
        for (level_z, outer_r) in t_levels.iter().zip(radii.iter()) {
            let z = level_z * height;
            let inner_r = (outer_r - wall).max(0.5);
            for j in 0..RING {
                let a = (j as f64) / (RING as f64) * std::f64::consts::TAU;
                positions.push([outer_r * a.cos(), outer_r * a.sin(), z]);
            }
            for j in 0..RING {
                let a = (j as f64) / (RING as f64) * std::f64::consts::TAU;
                positions.push([inner_r * a.cos(), inner_r * a.sin(), z]);
            }
        }

        let outer = |level: usize, j: usize| (level * 2 * RING + j % RING) as u32;
        let inner = |level: usize, j: usize| (level * 2 * RING + RING + j % RING) as u32;

        let mut triangles = Vec::new();
        for level in 0..t_levels.len() - 1 {
            for j in 0..RING {
                triangles.push([outer(level, j), outer(level, j + 1), outer(level + 1, j + 1)]);
                triangles.push([outer(level, j), outer(level + 1, j + 1), outer(level + 1, j)]);
                triangles.push([inner(level, j + 1), inner(level, j), inner(level + 1, j)]);
                triangles.push([inner(level, j + 1), inner(level + 1, j), inner(level + 1, j + 1)]);
            }
        }

        TriMesh::from_positions_and_triangles("합성_중공_항아리", positions, triangles).expect("valid")
    }

    /// 프리셋(`main.rs::make_preset_slot`)과 동일한 **단일 껍질** 회전체: 내벽이 없다.
    fn single_shell_vessel_mesh(height: f64, radius: f64) -> TriMesh {
        const NU: usize = 32;
        const NV: usize = 12;
        let mut positions = Vec::new();
        for iv in 0..=NV {
            let z = (iv as f64 / NV as f64) * height;
            let r = radius * (0.78 + 0.22 * (z / height * std::f64::consts::PI).sin());
            for iu in 0..=NU {
                let theta = (iu as f64 / NU as f64) * std::f64::consts::TAU;
                positions.push([r * theta.cos(), r * theta.sin(), z]);
            }
        }
        let stride = (NU + 1) as u32;
        let mut triangles = Vec::new();
        for iv in 0..NV as u32 {
            for iu in 0..NU as u32 {
                let a = iv * stride + iu;
                let b = a + 1;
                let c = a + stride;
                let d = c + 1;
                triangles.push([a, b, d]);
                triangles.push([a, d, c]);
            }
        }
        TriMesh::from_positions_and_triangles("단일_껍질", positions, triangles).expect("valid")
    }

    #[test]
    fn test_single_shell_mesh_does_not_report_curvature_as_wall_thickness() {
        // 내벽이 스캔되지 않은 단일 껍질에서 곡면 반경 변화(0.6mm)를 기벽으로 오인하면 안 된다.
        let mesh = single_shell_vessel_mesh(64.0, 58.0);
        let report = compute_artifact_measurements(&mesh, ArtifactCategory::CompleteVessel);
        assert!(
            report.mean_wall_thickness_mm >= 2.0,
            "단일 껍질인데 기벽이 {:.1}mm 로 오측정됨",
            report.mean_wall_thickness_mm
        );
        assert!(
            (report.rim_or_outer_diameter_mm - 90.5).abs() < 0.6,
            "구연 실측값 오류: {}",
            report.rim_or_outer_diameter_mm
        );
    }

    #[test]
    fn test_complete_vessel_rim_separated_from_max_belly_and_wall_measured() {
        // 바닥 반경 30, 최대복경 반경 58(Ø116), 구연 반경 40(Ø80), 기벽 6mm 인 완형 항아리.
        let mesh = hollow_jar_mesh(64.0, 40.0, 58.0, 30.0, 6.0);
        let report = compute_artifact_measurements(&mesh, ArtifactCategory::CompleteVessel);

        assert!(
            (report.max_belly_diameter_mm - 116.0).abs() < 0.6,
            "최대복경은 몸통 최대 폭이어야 함: {}",
            report.max_belly_diameter_mm
        );
        assert!(
            (report.rim_or_outer_diameter_mm - 80.0).abs() < 0.6,
            "구경은 구연부 실측이어야 함(최대복경 재사용 금지): {}",
            report.rim_or_outer_diameter_mm
        );
        assert!(
            (report.mean_wall_thickness_mm - 6.0).abs() < 0.4,
            "기벽 두께는 메쉬 실측이어야 함: {}",
            report.mean_wall_thickness_mm
        );
    }

    #[test]
    fn test_bead_perforation_axis_and_micro_scale() {
        // Create a micro glass bead (3.2mm diameter, 2.4mm height -> Micro10To1 scale, 0.01mm precision)
        let positions = vec![
            [-1.60, 0.0, 0.0],
            [1.60, 0.0, 0.0],
            [0.0, 1.60, 0.0],
            [0.0, -1.60, 0.0],
            [-0.45, 0.0, 1.20],
            [0.45, 0.0, 1.20],
            [-1.60, 0.0, 2.40],
            [1.60, 0.0, 2.40],
        ];
        let triangles = vec![
            [0, 1, 4],
            [1, 5, 4],
            [2, 3, 5],
            [4, 5, 6],
            [5, 7, 6],
            [0, 2, 6],
        ];
        let mesh =
            TriMesh::from_positions_and_triangles("소옥_1호", positions, triangles).expect("valid");
        let report = compute_artifact_measurements(&mesh, ArtifactCategory::BeadJade);
        assert_eq!(report.recommended_scale, ScalePreset::Micro10To1);
        assert!((report.height_or_length_mm - 2.40).abs() < 1e-6);
        assert!((report.rim_or_outer_diameter_mm - 3.20).abs() < 1e-6);
        assert!(report.metric_strip_summary.contains("소옥(유리구슬)"));
        assert!(report.metric_strip_summary.contains("2.40 mm"));
    }
}
