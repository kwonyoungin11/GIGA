pub mod dxf_table_export;
pub mod pdf_tiled_print;
pub mod plate_composer;
pub mod ret_exporter;
pub mod svg_illustrator;

pub use dxf_table_export::{
    export_measurements_to_csv, export_measurements_to_markdown, export_plate_to_dxf_r12,
};
pub use pdf_tiled_print::{
    compute_tiled_print_plan, render_plate_to_pdf_bytes, PaperSheetSize, TiledPrintPlan,
};
pub use plate_composer::{
    compose_archaeological_plate, triangulate_polygon_ear_clipping, ComposedPlate2D,
    PlateComposeInput, PlateLayoutMode, PlatePathElement, PlateRasterKind, PlateRasterRegion,
    PlateTextCallout,
};
pub use ret_exporter::{export_ret_folder_package, RetPackageManifest};
pub use svg_illustrator::render_plate_to_illustrator_svg;

#[cfg(test)]
mod tests {
    use super::*;
    use arch3d_core::{
        ArtifactCategory, BeadDrillType, BeadSubtype, HandleAttachmentKind, SherdPlateStrategy,
        SherdSubtype, TriMesh,
    };
    use arch3d_measure::compute_artifact_measurements;
    use arch3d_section::{
        infer_occluded_dashed_spans, slice_mesh_with_fixed_plane, CutPlaneOrientation,
    };
    use arch3d_shade::RubbingRenderParams;

    fn sample_vessel_mesh() -> TriMesh {
        let positions = vec![
            [40.0, 0.0, 0.0],
            [50.0, 0.0, 40.0],
            [62.0, 0.0, 118.0],
            [-40.0, 0.0, 0.0],
            [-50.0, 0.0, 40.0],
            [-62.0, 0.0, 118.0],
            [0.0, 50.0, 60.0],
        ];
        let triangles = vec![[0, 1, 2], [3, 4, 5], [0, 2, 6], [3, 5, 6]];
        TriMesh::from_positions_and_triangles("vessel_test", positions, triangles).expect("ok")
    }

    #[test]
    fn test_all_8_plate_modes_and_svg_1mm_5px_anchors() {
        let mesh = sample_vessel_mesh();
        let m_vessel = compute_artifact_measurements(&mesh, ArtifactCategory::CompleteVessel);
        let vsec = slice_mesh_with_fixed_plane(
            &mesh,
            None,
            CutPlaneOrientation::VerticalLongitudinal,
            0.0,
            0.0,
            0.25,
        );
        let p1_wall = vec![[10.0, 40.0], [10.0, 28.0]];
        let p2_wall = vec![[10.2, 16.0], [10.2, 0.0]];
        let occ = infer_occluded_dashed_spans(&[p1_wall, p2_wall], 2.0, 25.0);

        // 1. CompleteVesselStandard
        let p1 = compose_archaeological_plate(&PlateComposeInput {
            title: "7지구 9호-7 완형 토기",
            measurements: &m_vessel,
            vertical_section: &vsec,
            horizontal_sections: &[],
            occluded_spans: &[],
            sherd_subtype: None,
            sherd_strategy: None,
            handle_attachment: None,
            bead_subtype: None,
            bead_drill_type: None,
            wado_cut_ratio: None,
            hemp_density_label: None,
            flip_exterior_right: false,
        });
        assert_eq!(p1.mode, PlateLayoutMode::CompleteVesselStandard);

        let svg = render_plate_to_illustrator_svg(&p1);
        assert!(svg.contains(r#"data-scale-px-per-mm="5.0""#));
        assert!(svg.contains(r#"<g id="정렬_앵커">"#));
        assert!(svg.contains(r#"width="1" height="1" fill="none" stroke="none""#));
        // Must NEVER contain legacy hardcoded fake height or purple color
        assert!(!svg.contains("95.0 mm"));
        assert!(!svg.contains("#6b4fc0"));

        // 2. RimOrBaseSherdReconstructed (EstimatedDashedReconstruction)
        let m_sherd = compute_artifact_measurements(&mesh, ArtifactCategory::Potsherd);
        let p2 = compose_archaeological_plate(&PlateComposeInput {
            title: "구연부 편조각",
            measurements: &m_sherd,
            vertical_section: &vsec,
            horizontal_sections: &[],
            occluded_spans: &[],
            sherd_subtype: Some(SherdSubtype::RimSherd),
            sherd_strategy: Some(SherdPlateStrategy::EstimatedDashedReconstruction),
            handle_attachment: None,
            bead_subtype: None,
            bead_drill_type: None,
            wado_cut_ratio: None,
            hemp_density_label: None,
            flip_exterior_right: false,
        });
        assert_eq!(p2.mode, PlateLayoutMode::RimOrBaseSherdReconstructed);
        let svg2 = render_plate_to_illustrator_svg(&p2);
        assert!(svg2.contains("추정복원_파선"));
        assert!(svg2.contains("stroke-dasharray"));

        // 3. BodyOrMicroSherdThreeColumn
        let p3 = compose_archaeological_plate(&PlateComposeInput {
            title: "동체부 무문양 편조각",
            measurements: &m_sherd,
            vertical_section: &vsec,
            horizontal_sections: &[],
            occluded_spans: &[],
            sherd_subtype: Some(SherdSubtype::BodySherd),
            sherd_strategy: Some(SherdPlateStrategy::ThreeColumnParallel),
            handle_attachment: None,
            bead_subtype: None,
            bead_drill_type: None,
            wado_cut_ratio: None,
            hemp_density_label: None,
            flip_exterior_right: false,
        });
        assert_eq!(p3.mode, PlateLayoutMode::BodyOrMicroSherdThreeColumn);

        // 4. HandleOrSpoutSherd
        let p4 = compose_archaeological_plate(&PlateComposeInput {
            title: "우각형 파수편",
            measurements: &m_sherd,
            vertical_section: &vsec,
            horizontal_sections: &[],
            occluded_spans: &[],
            sherd_subtype: Some(SherdSubtype::HandleSpoutSherd),
            sherd_strategy: None,
            handle_attachment: Some(HandleAttachmentKind::InsertedTenonJoint),
            bead_subtype: None,
            bead_drill_type: None,
            wado_cut_ratio: None,
            hemp_density_label: None,
            flip_exterior_right: false,
        });
        assert_eq!(p4.mode, PlateLayoutMode::HandleOrSpoutSherd);

        // 5 & 6. BeadJadeMicroPlate & CommaJadeCurvedPlate
        let m_bead = compute_artifact_measurements(&mesh, ArtifactCategory::BeadJade);
        let p5 = compose_archaeological_plate(&PlateComposeInput {
            title: "벽옥제 관옥",
            measurements: &m_bead,
            vertical_section: &vsec,
            horizontal_sections: &[],
            occluded_spans: &occ,
            sherd_subtype: None,
            sherd_strategy: None,
            handle_attachment: None,
            bead_subtype: Some(BeadSubtype::TubularBead),
            bead_drill_type: Some(BeadDrillType::TypeIIb),
            wado_cut_ratio: None,
            hemp_density_label: None,
            flip_exterior_right: false,
        });
        assert_eq!(p5.mode, PlateLayoutMode::BeadJadeMicroPlate);

        let p6 = compose_archaeological_plate(&PlateComposeInput {
            title: "비취 곡옥",
            measurements: &m_bead,
            vertical_section: &vsec,
            horizontal_sections: &[],
            occluded_spans: &occ,
            sherd_subtype: None,
            sherd_strategy: None,
            handle_attachment: None,
            bead_subtype: Some(BeadSubtype::CommaJade),
            bead_drill_type: Some(BeadDrillType::TypeIb),
            wado_cut_ratio: None,
            hemp_density_label: None,
            flip_exterior_right: false,
        });
        assert_eq!(p6.mode, PlateLayoutMode::CommaJadeCurvedPlate);

        // 7. RoofTileAndEavesPlate
        let m_tile = compute_artifact_measurements(&mesh, ArtifactCategory::RoofTile);
        let p7 = compose_archaeological_plate(&PlateComposeInput {
            title: "연화문 수막새",
            measurements: &m_tile,
            vertical_section: &vsec,
            horizontal_sections: &[],
            occluded_spans: &[],
            sherd_subtype: None,
            sherd_strategy: None,
            handle_attachment: None,
            bead_subtype: None,
            bead_drill_type: None,
            wado_cut_ratio: Some(0.65),
            hemp_density_label: Some(String::from("포목흔 8×10올/1㎠")),
            flip_exterior_right: false,
        });
        assert_eq!(p7.mode, PlateLayoutMode::RoofTileAndEavesPlate);

        // 8. LithicAndCorrodedMetalPlate
        let m_metal = compute_artifact_measurements(&mesh, ArtifactCategory::CorrodedMetal);
        let p8 = compose_archaeological_plate(&PlateComposeInput {
            title: "부식 철부",
            measurements: &m_metal,
            vertical_section: &vsec,
            horizontal_sections: &[],
            occluded_spans: &occ,
            sherd_subtype: None,
            sherd_strategy: None,
            handle_attachment: None,
            bead_subtype: None,
            bead_drill_type: None,
            wado_cut_ratio: None,
            hemp_density_label: None,
            flip_exterior_right: false,
        });
        assert_eq!(p8.mode, PlateLayoutMode::LithicAndCorrodedMetalPlate);
    }

    #[test]
    fn test_pdf_tiled_print_and_dxf_csv_ret_package() {
        let mesh = sample_vessel_mesh();
        let m = compute_artifact_measurements(&mesh, ArtifactCategory::CompleteVessel);
        let vsec = slice_mesh_with_fixed_plane(
            &mesh,
            None,
            CutPlaneOrientation::VerticalLongitudinal,
            0.0,
            0.0,
            0.25,
        );
        let mut plate = compose_archaeological_plate(&PlateComposeInput {
            title: "대형 옹기 1:1 분할 인쇄 테스트",
            measurements: &m,
            vertical_section: &vsec,
            horizontal_sections: &[],
            occluded_spans: &[],
            sherd_subtype: None,
            sherd_strategy: None,
            handle_attachment: None,
            bead_subtype: None,
            bead_drill_type: None,
            wado_cut_ratio: None,
            hemp_density_label: None,
            flip_exterior_right: false,
        });
        plate.scale_preset = arch3d_core::ScalePreset::Full1To1;
        plate.canvas_width_mm = 420.0;
        plate.canvas_height_mm = 520.0;

        let plan = compute_tiled_print_plan(&plate, PaperSheetSize::A4Portrait, 10.0);
        assert!(plan.total_pages >= 4);

        let pdf_bytes = render_plate_to_pdf_bytes(&plate, PaperSheetSize::A4Portrait, 10.0);
        assert!(pdf_bytes.starts_with(b"%PDF-1.4"));
        assert!(pdf_bytes.ends_with(b"%%EOF\n") || pdf_bytes.ends_with(b"%%EOF"));

        let dxf = export_plate_to_dxf_r12(&plate);
        assert!(dxf.contains("SECTION"));
        assert!(dxf.contains("ENTITIES"));
        assert!(dxf.contains("EOF"));

        let csv = export_measurements_to_csv(&[("유물-01", &m)]);
        assert!(csv.contains("유물-01"));
        let md = export_measurements_to_markdown(&[("유물-01", &m)]);
        assert!(md.contains("| 유물-01 |"));

        let tmp_dir = std::env::temp_dir().join("strata_ret_pkg_test");
        let manifest = export_ret_folder_package(
            &tmp_dir,
            &plate,
            &mesh,
            None,
            RubbingRenderParams::default(),
        )
        .expect("ret package ok");
        assert!(manifest.cutline_rc0_svg.exists());
        assert!(manifest.rubbing_ext_png.exists());
        assert!(manifest.rubbing_meta_txt.exists());
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_ear_clipping_concave_section_polygon_triangulation() {
        // L자형 비볼록(Concave) 토기 구연·기벽 단면 다각형 (6개 정점 -> 4개 삼각형)
        let concave_poly = [
            [0.0, 0.0],
            [20.0, 0.0],
            [20.0, 8.0],
            [7.0, 8.0],
            [7.0, 45.0],
            [0.0, 45.0],
        ];
        let tris = triangulate_polygon_ear_clipping(&concave_poly);
        assert_eq!(tris.len(), concave_poly.len() - 2);
        // 삼각형 넓이 총합이 원래 L자 다각형 넓이(20*8 + 7*37 = 160 + 259 = 419)와 일치하는지 검증
        let mut area_sum = 0.0f64;
        for tri in &tris {
            let a = concave_poly[tri[0]];
            let b = concave_poly[tri[1]];
            let c = concave_poly[tri[2]];
            let cross = ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs();
            area_sum += 0.5 * cross;
        }
        assert!((area_sum - 419.0).abs() < 1e-4, "area_sum={area_sum}");
    }

    #[test]
    fn test_plate_composer_generates_multiview_rubbing_rasters_and_solid_ink_sections() {
        let mesh = sample_vessel_mesh();
        let m_vessel = compute_artifact_measurements(&mesh, ArtifactCategory::CompleteVessel);
        let vsec = slice_mesh_with_fixed_plane(
            &mesh,
            None,
            CutPlaneOrientation::VerticalLongitudinal,
            0.0,
            0.0,
            0.25,
        );
        let hsec = slice_mesh_with_fixed_plane(
            &mesh,
            None,
            CutPlaneOrientation::HorizontalTransverse,
            0.0,
            18.0,
            0.25,
        );
        let plate_vessel = compose_archaeological_plate(&PlateComposeInput {
            title: "7지구 9호-7 완형 토기",
            measurements: &m_vessel,
            vertical_section: &vsec,
            horizontal_sections: &[hsec],
            occluded_spans: &[],
            sherd_subtype: None,
            sherd_strategy: None,
            handle_attachment: None,
            bead_subtype: None,
            bead_drill_type: None,
            wado_cut_ratio: None,
            hemp_density_label: None,
            flip_exterior_right: false,
        });
        assert!(
            plate_vessel
                .raster_regions
                .iter()
                .any(|r| r.kind == PlateRasterKind::FrontExteriorRubbing),
            "Complete vessel plate must include FrontExteriorRubbing raster region"
        );
        assert!(
            plate_vessel
                .raster_regions
                .iter()
                .any(|r| r.kind == PlateRasterKind::BottomPlanRubbing),
            "Complete vessel plate must include BottomPlanRubbing raster region"
        );
        assert!(
            plate_vessel
                .paths
                .iter()
                .any(|p| p.fill_hex.as_deref() == Some("#141413")),
            "Complete vessel plate must include #141413 solid-filled section paths"
        );

        let m_sherd = compute_artifact_measurements(&mesh, ArtifactCategory::Potsherd);
        let plate_3col = compose_archaeological_plate(&PlateComposeInput {
            title: "1.obj 편조각 3단 병렬",
            measurements: &m_sherd,
            vertical_section: &vsec,
            horizontal_sections: &[],
            occluded_spans: &[],
            sherd_subtype: Some(SherdSubtype::BodySherd),
            sherd_strategy: Some(SherdPlateStrategy::ThreeColumnParallel),
            handle_attachment: None,
            bead_subtype: None,
            bead_drill_type: None,
            wado_cut_ratio: None,
            hemp_density_label: None,
            flip_exterior_right: false,
        });
        assert!(
            plate_3col
                .raster_regions
                .iter()
                .any(|r| r.kind == PlateRasterKind::FrontExteriorRubbing)
                && plate_3col
                    .raster_regions
                    .iter()
                    .any(|r| r.kind == PlateRasterKind::BackInteriorRubbing),
            "3-column sherd plate must include both exterior and interior rubbing raster regions"
        );
    }
}
