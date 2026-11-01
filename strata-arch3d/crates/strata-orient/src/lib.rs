pub mod axis_solver;
pub mod bead_perforation;
pub mod circle_fit;
pub mod defect_scan;
pub mod fracture_seg;
pub mod refit_baiser;
pub mod tile_lithic_metal;

pub use axis_solver::{
    analyze_special_vessel_part, recover_body_sherd_stance_from_rills,
    solve_rotational_axis_huber_irls, RotationalAxisEstimate, SpecialPartDiagnostics,
};
pub use bead_perforation::{
    analyze_bead_mesh_perforation, classify_bead_and_perforation, BeadPerforationAnalysis,
};
pub use circle_fit::{fit_rim_arc_unbiased, RimCircleFitResult};
pub use defect_scan::{
    compute_rim_gap_chart_360, evaluate_section_angles_parallel, RimGapChart360, SectionAngleScore,
};
pub use fracture_seg::{segment_sherd_three_regions, SherdSegmentation};
pub use refit_baiser::{
    extract_baiser_descriptor, score_baiser_refit_compatibility, BaiserFractureDescriptor,
};
pub use tile_lithic_metal::{
    analyze_corroded_metal_layers, analyze_lithic_diacritical, analyze_roof_tile,
    CorrodedMetalLayerAnalysis, LithicDiacriticalAnalysis, RoofTileDiagnostics,
};

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::{BeadDrillType, BeadSubtype, SherdPlateStrategy, SurfaceRegion, TriMesh};

    fn make_synthetic_rim_sherd_mesh(arc_deg: f64, radius_mm: f64) -> TriMesh {
        let mut positions = Vec::new();
        let mut triangles = Vec::new();
        let nu = 24usize;
        let nv = 10usize;
        let arc_rad = arc_deg.to_radians();

        for iv in 0..=nv {
            let z = (iv as f64 / nv as f64) * 40.0;
            for iu in 0..=nu {
                let theta = (iu as f64 / nu as f64) * arc_rad;
                positions.push([radius_mm * theta.cos(), radius_mm * theta.sin(), z]);
            }
        }
        let stride = (nu + 1) as u32;
        for iv in 0..(nv as u32) {
            for iu in 0..(nu as u32) {
                let a = iv * stride + iu;
                let b = a + 1;
                let c = a + stride;
                let d = c + 1;
                triangles.push([a, b, d]);
                triangles.push([a, d, c]);
            }
        }
        TriMesh::from_positions_and_triangles("rim_sherd", positions, triangles).expect("valid")
    }

    #[test]
    fn test_rim_gap_chart_covers_full_360_degrees() {
        let mesh = make_synthetic_rim_sherd_mesh(270.0, 75.0);
        let chart = compute_rim_gap_chart_360(&mesh, true);
        // C++ ArtifactDoc.cpp:2721 stopped at 112°; verify our chart covers all 180 bins (0°..358°)
        assert_eq!(chart.angles_deg.len(), 180);
        assert!((chart.angles_deg[179] - 358.0).abs() < 1e-6);
        // 270 degrees is ~135 bins (> 56 bins which was 112 deg)
        assert!(
            chart.occupied_bins > 110,
            "occupied_bins={}",
            chart.occupied_bins
        );
    }

    #[test]
    fn test_parallel_angle_defect_score_varies_by_angle() {
        // 90-degree sherd only occupies 0..90 deg, so angles in 0..90 have low defect and angles in 180..270 have high defect
        let mesh = make_synthetic_rim_sherd_mesh(90.0, 80.0);
        let scores = evaluate_section_angles_parallel(&mesh, 15);
        assert!(!scores.is_empty());
        let best = &scores[0];
        let worst = scores.last().expect("non-empty");
        assert!(
            best.total_defect_score < worst.total_defect_score,
            "best={} vs worst={}",
            best.total_defect_score,
            worst.total_defect_score
        );
        assert!(best.angle_deg <= 90.0);
    }

    #[test]
    fn test_sherd_fracture_3way_segmentation_and_unbiased_circle_fit() {
        // Generate a 28-degree small rim sherd of true radius 90.0 mm (diameter 180.0 mm)
        let true_r = 90.0;
        let mesh = make_synthetic_rim_sherd_mesh(28.0, true_r);
        let seg = segment_sherd_three_regions(&mesh, 35.0, 0.8);
        assert_eq!(seg.vertex_regions.len(), mesh.positions.len());

        // Verify non_fracture_positions excludes all FractureZone vertices
        for (&p, &r) in mesh.positions.iter().zip(&seg.vertex_regions) {
            if r == SurfaceRegion::FractureZone {
                assert!(!seg.outer_wall_positions(&mesh).contains(&p));
            }
        }

        // Fit circle on 28-degree arc points using Taubin + Huber IRLS
        let xy_pts: Vec<[f64; 2]> = seg
            .non_fracture_positions(&mesh)
            .into_iter()
            .map(|p| [p[0], p[1]])
            .collect();
        let fit = fit_rim_arc_unbiased(&xy_pts, false).expect("fit small arc");
        assert!(
            (fit.radius_mm - true_r).abs() < 0.15,
            "recovered R={} vs true R={true_r}",
            fit.radius_mm
        );
        assert_eq!(
            fit.plate_strategy,
            SherdPlateStrategy::EstimatedDashedReconstruction
        );
    }

    #[test]
    fn test_bead_ld_classification_and_4_drill_types() {
        // 1. Type IIb: Biconical hourglass with central meeting step offset Δe = 0.35 mm
        let samples_iib = vec![
            [0.0, 1.80, -0.15],
            [4.0, 1.35, -0.15],
            [8.0, 0.90, -0.15],
            [8.5, 0.92, 0.20], // Meeting step offset Δe = 0.35mm!
            [12.0, 1.40, 0.20],
            [16.0, 1.85, 0.20],
        ];
        let res_iib = classify_bead_and_perforation(16.0, 8.0, false, 0, &samples_iib);
        assert_eq!(res_iib.subtype, BeadSubtype::TubularBead);
        assert_eq!(res_iib.drill_type, BeadDrillType::TypeIIb);
        assert!((res_iib.meeting_step_offset_mm - 0.35).abs() < 0.02);

        // 2. Type Ia: Cylindrical tubular drill
        let samples_ia = vec![[0.0, 1.20, 0.0], [3.0, 1.20, 0.0], [6.0, 1.21, 0.0]];
        let res_ia = classify_bead_and_perforation(6.0, 6.2, false, 0, &samples_ia);
        assert_eq!(res_ia.subtype, BeadSubtype::AnnularBead);
        assert_eq!(res_ia.drill_type, BeadDrillType::TypeIa);

        // 3. Type Ib: Conical one-way drill + exit breakout scar
        let samples_ib = vec![
            [0.0, 0.95, 0.0], // exit breakout flare
            [0.5, 0.80, 0.0],
            [2.0, 1.15, 0.0],
            [4.0, 1.65, 0.0],
        ];
        let res_ib = classify_bead_and_perforation(4.0, 6.0, false, 0, &samples_ib);
        assert_eq!(res_ib.subtype, BeadSubtype::SmallBead);
        assert_eq!(res_ib.drill_type, BeadDrillType::TypeIb);
        assert!(res_ib.has_exit_spalling_scar);

        // 4. Type Ib': Blind incomplete hole
        let samples_blind = vec![
            [2.5, 0.65, 0.0], // stops at z=2.5mm out of 8.0mm
            [5.0, 1.10, 0.0],
            [8.0, 1.50, 0.0],
        ];
        let res_blind = classify_bead_and_perforation(8.0, 8.0, false, 0, &samples_blind);
        assert_eq!(res_blind.drill_type, BeadDrillType::TypeIbBlind);
        assert_eq!(res_blind.bottom_exit_diameter_mm, 0.0);
    }
}
