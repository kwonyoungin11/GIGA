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
    compose_archaeological_plate, compose_archaeological_plate_with_mode,
    triangulate_polygon_ear_clipping, ComposedPlate2D, PlateComposeInput, PlateLayoutMode,
    PlatePathElement, PlateRasterKind, PlateRasterRegion, PlateTextCallout,
};
pub use ret_exporter::{export_ret_folder_package, RetPackageManifest};
pub use svg_illustrator::render_plate_to_illustrator_svg;

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::{
        ArtifactCategory, BeadDrillType, BeadSubtype, HandleAttachmentKind, SherdPlateStrategy,
        SherdSubtype, TriMesh,
    };
    use strata_measure::compute_artifact_measurements;
    use strata_section::{
        infer_occluded_dashed_spans, slice_mesh_with_fixed_plane, CutPlaneOrientation,
    };
    use strata_shade::RubbingRenderParams;

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

    /// 실제 샘플(Ø116 · 높이 64)처럼 **넓고 낮은** 완형 유물. 4판 레이아웃이 가로로 밀려나는 조건을 만든다.
    fn wide_short_vessel_mesh() -> TriMesh {
        const RING: usize = 16;
        let profile = [
            (30.0_f64, 0.0_f64),
            (52.0, 16.0),
            (58.0, 32.0),
            (50.0, 48.0),
            (40.0, 64.0),
        ];
        let mut positions = Vec::new();
        for (r, z) in profile {
            for j in 0..RING {
                let a = (j as f64) / (RING as f64) * std::f64::consts::TAU;
                positions.push([r * a.cos(), r * a.sin(), z]);
            }
        }
        let idx = |level: usize, j: usize| (level * RING + j % RING) as u32;
        let mut triangles = Vec::new();
        for level in 0..profile.len() - 1 {
            for j in 0..RING {
                triangles.push([idx(level, j), idx(level, j + 1), idx(level + 1, j + 1)]);
                triangles.push([idx(level, j), idx(level + 1, j + 1), idx(level + 1, j)]);
            }
        }
        TriMesh::from_positions_and_triangles("넓은_완형_항아리", positions, triangles).expect("ok")
    }

    #[test]
    fn test_complete_vessel_plate_stays_inside_canvas_and_views_do_not_overlap() {
        let mesh = wide_short_vessel_mesh();
        let m = compute_artifact_measurements(&mesh, ArtifactCategory::CompleteVessel);
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
            32.0,
            0.0,
            0.25,
        );
        let plate = compose_archaeological_plate(&PlateComposeInput {
            title: "넓은 완형 항아리",
            measurements: &m,
            vertical_section: &vsec,
            horizontal_sections: std::slice::from_ref(&hsec),
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

        // 1. 어떤 선도 도판 캔버스 폭을 넘지 않아야 한다(용지 밖으로 나가면 인쇄에서 잘린다).
        for path in &plate.paths {
            for p in &path.points_mm {
                assert!(
                    p[0] >= -0.01 && p[0] <= plate.canvas_width_mm + 0.01,
                    "레이어 '{}' x={:.2} 가 캔버스 폭 {:.2} 를 벗어남",
                    path.layer_id,
                    p[0],
                    plate.canvas_width_mm
                );
            }
        }

        // 2. 정밀 탁본 판과 수평 횡단면 판의 Y 구간이 겹치지 않아야 한다.
        let rub = plate
            .raster_regions
            .iter()
            .find(|r| r.kind == PlateRasterKind::BottomPlanRubbing)
            .expect("탁본 영역");
        let mut hsec_min_y = f64::INFINITY;
        let mut hsec_max_y = f64::NEG_INFINITY;
        for path in plate
            .paths
            .iter()
            .filter(|p| p.layer_id.starts_with("횡단면_"))
        {
            for p in &path.points_mm {
                hsec_min_y = hsec_min_y.min(p[1]);
                hsec_max_y = hsec_max_y.max(p[1]);
            }
        }
        assert!(
            hsec_max_y <= rub.rect_mm[1] || hsec_min_y >= rub.rect_mm[3],
            "탁본 Y[{:.1},{:.1}] 와 횡단면 Y[{:.1},{:.1}] 가 겹침",
            rub.rect_mm[1],
            rub.rect_mm[3],
            hsec_min_y,
            hsec_max_y
        );
    }

    #[test]
    fn test_plate_layout_mode_all_exposes_eight_short_labels() {
        assert_eq!(
            PlateLayoutMode::ALL.len(),
            8,
            "8대 도판 모드 전체가 UI 목록에 있어야 한다"
        );
        for m in PlateLayoutMode::ALL {
            let s = m.short_label_kr();
            assert!(!s.is_empty());
            assert!(
                s.chars().count() <= 14,
                "{:?} 짧은 라벨이 너무 김: {}",
                m,
                s
            );
        }
    }

    #[test]
    fn test_plate_layout_mode_can_be_forced_from_ui() {
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
        let input = PlateComposeInput {
            title: "강제 모드",
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
        };

        // 자동 판정은 완형 표준.
        assert_eq!(
            compose_archaeological_plate(&input).mode,
            PlateLayoutMode::CompleteVesselStandard
        );

        // 사용자가 고른 모드가 그대로 적용되어야 한다(8대 도판 모드 UI 노출용).
        for forced in [
            PlateLayoutMode::RimOrBaseSherdReconstructed,
            PlateLayoutMode::BodyOrMicroSherdThreeColumn,
            PlateLayoutMode::HandleOrSpoutSherd,
            PlateLayoutMode::BeadJadeMicroPlate,
            PlateLayoutMode::CommaJadeCurvedPlate,
            PlateLayoutMode::RoofTileAndEavesPlate,
            PlateLayoutMode::LithicAndCorrodedMetalPlate,
        ] {
            let plate = compose_archaeological_plate_with_mode(&input, forced);
            assert_eq!(plate.mode, forced);
        }
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
        plate.scale_preset = strata_core::ScalePreset::Full1To1;
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
