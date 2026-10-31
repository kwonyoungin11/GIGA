pub mod metrics;

pub use metrics::{compute_artifact_measurements, ArtifactMeasurementReport};

#[cfg(test)]
mod tests {
    use super::*;
    use arch3d_core::{ArtifactCategory, ScalePreset, TriMesh};

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
