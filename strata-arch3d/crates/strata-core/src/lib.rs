pub mod bvh;
pub mod error;
pub mod mesh;
pub mod strata_tokens;
pub mod types;

pub use bvh::{LbvhNode, StacklessLbvh};
pub use error::{StrataError, ThreePartErrorInfo};
pub use mesh::{CompressedCornerTable, GpuVertex16, MeshTextureRgba, OctahedralNormal16, TriMesh};
pub use strata_tokens::{metrics, tokens, ColorToken};
pub use types::{
    Aabb3, ArtifactCategory, BeadDrillType, BeadFabricationKind, BeadSubtype,
    HandleAttachmentKind, Mat4, MetalSectionLayer, ScalePreset, SherdPlateStrategy, SherdSubtype,
    SurfaceRegion,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strata_tokens_and_3part_error_contract() {
        // Verify STRATA PDF token hex codes & R1 layout metrics
        assert_eq!(tokens::GROUND.hex, "#FAF9F5");
        assert_eq!(tokens::WASH.hex, "#F5F4ED");
        assert_eq!(tokens::DESK.hex, "#F0EEE6");
        assert_eq!(tokens::SURFACE.hex, "#FFFFFF");
        assert_eq!(tokens::INK.hex, "#141413");
        assert_eq!(tokens::ACTION.hex, "#B5573A");
        assert_eq!(tokens::CLAY_FOCUS.hex, "#D97757");
        assert_eq!(tokens::PROFILE_RED.hex, "#FF0000");

        assert_eq!(metrics::TAB_BAR_HEIGHT, 34);
        assert_eq!(metrics::RIBBON_HEIGHT, 98);
        assert_eq!(metrics::RIBBON_TILE_SIZE, 50);
        assert_eq!(metrics::CTX_BAR_HEIGHT, 36);
        assert_eq!(metrics::SLOT_LIST_WIDTH, 176);
        assert_eq!(metrics::STATUS_BAR_HEIGHT, 30);
        assert_eq!(metrics::SPLASH_WIDTH, 700);
        assert_eq!(metrics::SPLASH_HEIGHT, 400);

        // Verify 3-part error dialog contract (무엇이 안 됐나 / 왜 그런가 / 어떻게 하나)
        let err = StrataError::InsufficientRimArc {
            arc_deg: 11.5,
            min_req_deg: 20.0,
        };
        let info = err.to_three_part();
        assert!(!info.what_failed.is_empty());
        assert!(info.why_it_happened.contains("11.5°"));
        assert!(info.how_to_fix.contains("3단 병렬 도판"));
    }

    #[test]
    fn test_bead_ld_ratio_and_sherd_plate_strategy() {
        // 환옥(>=0.95), 소옥(0.46..=0.94), 절구옥(<=0.45), 관옥(>=1.50), 곡옥
        assert_eq!(
            BeadSubtype::classify_by_ratio(3.8, 3.8, false, 0),
            BeadSubtype::AnnularBead
        );
        assert_eq!(
            BeadSubtype::classify_by_ratio(2.5, 4.0, false, 0),
            BeadSubtype::SmallBead
        );
        assert_eq!(
            BeadSubtype::classify_by_ratio(1.2, 4.0, false, 0),
            BeadSubtype::MortarDiscBead
        );
        assert_eq!(
            BeadSubtype::classify_by_ratio(18.0, 6.0, false, 0),
            BeadSubtype::TubularBead
        );
        assert_eq!(
            BeadSubtype::classify_by_ratio(28.0, 12.0, true, 0),
            BeadSubtype::CommaJade
        );
        assert_eq!(
            BeadSubtype::classify_by_ratio(12.0, 10.0, false, 14),
            BeadSubtype::FacetedPolyhedralBead
        );

        // Sherd plate strategy branching
        assert_eq!(
            SherdPlateStrategy::from_arc_angle_deg(65.0, false),
            SherdPlateStrategy::FullReconstruction
        );
        assert_eq!(
            SherdPlateStrategy::from_arc_angle_deg(30.0, false),
            SherdPlateStrategy::EstimatedDashedReconstruction
        );
        assert_eq!(
            SherdPlateStrategy::from_arc_angle_deg(14.0, false),
            SherdPlateStrategy::ThreeColumnParallel
        );
        assert_eq!(
            SherdPlateStrategy::from_arc_angle_deg(90.0, true),
            SherdPlateStrategy::ThreeColumnParallel
        );
    }

    #[test]
    fn test_corner_table_and_lbvh_subtree_area() {
        // Create a closed tetrahedron mesh (4 vertices, 4 triangles, 0 boundary corners)
        let positions = vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 10.0],
        ];
        let triangles = vec![[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]];
        let mesh =
            TriMesh::from_positions_and_triangles("tetra", positions, triangles).expect("valid");

        // Every corner in a closed manifold tetrahedron has an opposite twin
        for c in 0..12 {
            assert!(
                !mesh.topology.is_boundary_corner(c),
                "corner {c} should have twin"
            );
        }

        // Verify OctahedralNormal16 roundtrip accuracy
        let orig_n = [0.577350269, -0.577350269, 0.577350269];
        let enc = OctahedralNormal16::encode(orig_n);
        let dec = enc.decode();
        let dot = orig_n[0] * dec[0] + orig_n[1] * dec[1] + orig_n[2] * dec[2];
        assert!(dot > 0.9999, "octahedral normal dot={dot}");

        // Verify Stackless LBVH subtree_area pre-integration
        let bvh = StacklessLbvh::build(&mesh);
        let sum_face_areas: f64 = mesh.face_areas.iter().sum();
        let (sphere_area, _) = bvh.query_sphere_integral(&mesh, [2.0, 2.0, 2.0], 50.0);
        assert!((sphere_area - sum_face_areas).abs() < 1e-9);
    }
}
