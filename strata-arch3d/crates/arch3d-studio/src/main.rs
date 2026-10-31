pub mod dialogs;
pub mod harness_runner;
pub mod layout_r1;
pub mod sheet_pane;
pub mod splash;
pub mod view3d_pane;

use arch3d_core::strata_tokens::{
    metrics::{
        CTX_BAR_HEIGHT, RIBBON_HEIGHT, RIBBON_TILE_SIZE, STATUS_BAR_HEIGHT, TAB_BAR_HEIGHT,
    },
    tokens::{
        ACTION, CLAY_INK, CLAY_WASH, DESK, EDGE, GROUND, INK, LINE, MUTED, SURFACE, WASH,
    },
};
use arch3d_core::{
    ArtifactCategory, BeadDrillType, BeadSubtype, HandleAttachmentKind, ScalePreset, SherdSubtype,
    StacklessLbvh, TriMesh,
};
use arch3d_export::{
    compose_archaeological_plate, compute_tiled_print_plan, export_ret_folder_package,
    render_plate_to_illustrator_svg, render_plate_to_pdf_bytes, ComposedPlate2D, PaperSheetSize,
    PlateComposeInput,
};
use arch3d_io::{load_any_mesh, Ap2Project};
use arch3d_measure::{compute_artifact_measurements, ArtifactMeasurementReport};
use arch3d_orient::{
    compute_rim_gap_chart_360, evaluate_section_angles_parallel, segment_sherd_three_regions,
    RimGapChart360, SherdSegmentation,
};
use arch3d_section::{
    infer_occluded_dashed_spans, slice_mesh_with_fixed_plane, CutPlaneOrientation,
    OccludedDashedSpan, PlaneSliceResult,
};
use arch3d_shade::RubbingRenderParams;
use dialogs::{show_active_dialog, ActiveStrataDialog};
use eframe::egui;
use harness_runner::{
    generate_screenshot_tour_and_golden_outputs, run_v1_to_v8_verification_harness,
};
use image::{Rgba, RgbaImage};
use layout_r1::{
    draw_ribbon_tile_vector_icon, unified_studio_ribbon_tiles, ChsReportTemplate,
    R1FiveTierGeometry, WorkflowTab,
};
use sheet_pane::{render_sheet_pane_egui, SheetPaneState};
use splash::SplashState;
use std::path::{Path, PathBuf};
use view3d_pane::{render_view3d_pane_egui, View3DPaneState};

#[derive(Debug)]
struct ArtifactSlotEntry {
    title: String,
    #[allow(dead_code)]
    source_path: Option<PathBuf>,
    category: ArtifactCategory,
    sherd_subtype: Option<SherdSubtype>,
    bead_subtype: Option<BeadSubtype>,
    bead_drill_type: Option<BeadDrillType>,
    mesh: TriMesh,
    bvh: StacklessLbvh,
    segmentation: SherdSegmentation,
    rim_gap_chart: RimGapChart360,
    recommended_best_angle_deg: f64,
    measurements: ArtifactMeasurementReport,
    vertical_section: PlaneSliceResult,
    horizontal_section: PlaneSliceResult,
    occluded_spans: Vec<OccludedDashedSpan>,
    composed_plate: ComposedPlate2D,
}

impl ArtifactSlotEntry {
    fn from_mesh(
        title: impl Into<String>,
        source_path: Option<PathBuf>,
        category: ArtifactCategory,
        sherd_subtype: Option<SherdSubtype>,
        bead_subtype: Option<BeadSubtype>,
        bead_drill_type: Option<BeadDrillType>,
        mesh: TriMesh,
    ) -> Self {
        let title_str = title.into();
        let bvh = StacklessLbvh::build(&mesh);
        let segmentation = segment_sherd_three_regions(&mesh, 35.0, 0.8);
        let rim_gap_chart = compute_rim_gap_chart_360(&mesh, true);
        let angle_scores = evaluate_section_angles_parallel(&mesh, 6);
        let recommended_best_angle_deg = angle_scores.first().map(|s| s.angle_deg).unwrap_or(0.0);
        let measurements = compute_artifact_measurements(&mesh, category);
        let vertical_section = slice_mesh_with_fixed_plane(
            &mesh,
            Some(&bvh),
            CutPlaneOrientation::VerticalLongitudinal,
            recommended_best_angle_deg,
            0.0,
            0.25,
        );
        let default_top_z = mesh
            .ap2_cut_offset_mm
            .unwrap_or_else(|| (mesh.bbox.extents()[2] * 0.50).max(2.0));
        let horizontal_section = slice_mesh_with_fixed_plane(
            &mesh,
            Some(&bvh),
            CutPlaneOrientation::HorizontalTransverse,
            recommended_best_angle_deg,
            mesh.bbox.min[2] + default_top_z,
            0.25,
        );
        let occluded_spans = infer_occluded_dashed_spans(&vertical_section.polylines_2d, 1.5, 24.0);
        let sherd_strategy = measurements
            .sherd_circle_fit
            .as_ref()
            .map(|f| f.plate_strategy);
        let composed_plate = compose_archaeological_plate(&PlateComposeInput {
            title: &title_str,
            measurements: &measurements,
            vertical_section: &vertical_section,
            horizontal_sections: std::slice::from_ref(&horizontal_section),
            occluded_spans: &occluded_spans,
            sherd_subtype,
            sherd_strategy,
            handle_attachment: Some(HandleAttachmentKind::InsertedTenonJoint),
            bead_subtype,
            bead_drill_type,
            wado_cut_ratio: Some(0.64),
            hemp_density_label: Some(String::from("포목흔 8×10올/1㎠")),
            flip_exterior_right: false,
        });

        Self {
            title: title_str,
            source_path,
            category,
            sherd_subtype,
            bead_subtype,
            bead_drill_type,
            mesh,
            bvh,
            segmentation,
            rim_gap_chart,
            recommended_best_angle_deg,
            measurements,
            vertical_section,
            horizontal_section,
            occluded_spans,
            composed_plate,
        }
    }

    fn rebuild_plate(
        &mut self,
        flip_exterior_right: bool,
        section_angle_deg: f64,
        cut_y_mm: f64,
        top_cut_z_mm: f64,
    ) {
        self.vertical_section = slice_mesh_with_fixed_plane(
            &self.mesh,
            Some(&self.bvh),
            CutPlaneOrientation::VerticalLongitudinal,
            section_angle_deg,
            cut_y_mm,
            0.25,
        );
        self.horizontal_section = slice_mesh_with_fixed_plane(
            &self.mesh,
            Some(&self.bvh),
            CutPlaneOrientation::HorizontalTransverse,
            section_angle_deg,
            self.mesh.bbox.min[2] + top_cut_z_mm,
            0.25,
        );
        self.occluded_spans =
            infer_occluded_dashed_spans(&self.vertical_section.polylines_2d, 1.5, 24.0);
        let sherd_strategy = self
            .measurements
            .sherd_circle_fit
            .as_ref()
            .map(|f| f.plate_strategy);
        self.composed_plate = compose_archaeological_plate(&PlateComposeInput {
            title: &self.title,
            measurements: &self.measurements,
            vertical_section: &self.vertical_section,
            horizontal_sections: std::slice::from_ref(&self.horizontal_section),
            occluded_spans: &self.occluded_spans,
            sherd_subtype: self.sherd_subtype,
            sherd_strategy,
            handle_attachment: Some(HandleAttachmentKind::InsertedTenonJoint),
            bead_subtype: self.bead_subtype,
            bead_drill_type: self.bead_drill_type,
            wado_cut_ratio: Some(0.64),
            hemp_density_label: Some(String::from("포목흔 8×10올/1㎠")),
            flip_exterior_right,
        });
    }
}

#[derive(Debug)]
pub struct StrataStudioApp {
    splash: SplashState,
    #[allow(dead_code)]
    active_tab: WorkflowTab,
    active_chs_template: ChsReportTemplate,
    active_tile_code: &'static str,
    slots: Vec<ArtifactSlotEntry>,
    selected_slot_idx: usize,
    view3d_state: View3DPaneState,
    sheet_state: SheetPaneState,
    rubbing_params: RubbingRenderParams,
    flip_exterior_right: bool,
    sheet_full_tab_mode: bool,
    show_mm_grid: bool,
    pose_locked: bool,
    active_dialog: Option<ActiveStrataDialog>,
    status_message: String,
    capture_window_path: Option<PathBuf>,
    capture_frames_rendered: u32,
    screenshot_requested: bool,
}

impl StrataStudioApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        capture_window_path: Option<PathBuf>,
        initial_tab: Option<WorkflowTab>,
        initial_slot: Option<usize>,
    ) -> Self {
        configure_korean_fonts_and_strata_theme(&cc.egui_ctx);

        let mut slots = Vec::new();
        let p_7obj = Path::new(r"E:\034(7-실4-4)\7지구 9호-7.obj");
        if p_7obj.exists() {
            if let Ok(aligned) = load_any_mesh(p_7obj) {
                slots.push(ArtifactSlotEntry::from_mesh(
                    "7지구 9호-7 (완형 토기 세트)",
                    Some(p_7obj.to_path_buf()),
                    ArtifactCategory::CompleteVessel,
                    None,
                    None,
                    None,
                    aligned,
                ));
            }
        }

        let p_1obj = Path::new(r"E:\1\1.obj");
        if p_1obj.exists() {
            if let Ok(m1) = load_any_mesh(p_1obj) {
                slots.push(ArtifactSlotEntry::from_mesh(
                    "1.obj (구연·저부 편조각 세트)",
                    Some(p_1obj.to_path_buf()),
                    ArtifactCategory::Potsherd,
                    Some(SherdSubtype::RimSherd),
                    None,
                    None,
                    m1,
                ));
            }
        }

        if slots.is_empty() {
            slots.push(Self::make_preset_slot(
                "표준 발굴 유물 (기본 실측 샘플)",
                ArtifactCategory::CompleteVessel,
                None,
                None,
                None,
                58.0,
                64.0,
            ));
        }

        let mut splash = SplashState::default();
        if capture_window_path.is_some() {
            splash.elapsed_sec = splash.duration_sec;
        }

        let selected_slot_idx = initial_slot
            .unwrap_or(0)
            .min(slots.len().saturating_sub(1));
        let initial_chs = if selected_slot_idx == 1 {
            ChsReportTemplate::VesselRimBaseReconstruct
        } else {
            ChsReportTemplate::VesselCompleteStandard
        };

        Self {
            splash,
            active_tab: initial_tab.unwrap_or(WorkflowTab::Step1LoadAndClassify),
            active_chs_template: initial_chs,
            active_tile_code: initial_chs.code(),
            slots,
            selected_slot_idx,
            view3d_state: View3DPaneState::default(),
            sheet_state: SheetPaneState::default(),
            rubbing_params: RubbingRenderParams::default(),
            flip_exterior_right: false,
            sheet_full_tab_mode: false,
            show_mm_grid: true,
            pose_locked: true,
            active_dialog: None,
            status_message: String::from(
                "준비 완료 · 국가유산청(구 문화재청) 발굴조사보고서 실측도면 표준 양식 · 1mm=5.0px 정밀 방안지",
            ),
            capture_window_path,
            capture_frames_rendered: 0,
            screenshot_requested: false,
        }
    }

    fn make_preset_slot(
        title: &str,
        category: ArtifactCategory,
        sherd_subtype: Option<SherdSubtype>,
        bead_subtype: Option<BeadSubtype>,
        bead_drill_type: Option<BeadDrillType>,
        radius_mm: f64,
        height_mm: f64,
    ) -> ArtifactSlotEntry {
        let mut positions = Vec::new();
        let mut triangles = Vec::new();
        let nu = 32usize;
        let nv = 12usize;
        for iv in 0..=nv {
            let z = (iv as f64 / nv as f64) * height_mm;
            let r = radius_mm * (0.78 + 0.22 * (z / height_mm * std::f64::consts::PI).sin());
            for iu in 0..=nu {
                let theta = (iu as f64 / nu as f64) * std::f64::consts::TAU;
                positions.push([r * theta.cos(), r * theta.sin(), z]);
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
        let mesh = match TriMesh::from_positions_and_triangles(title, positions, triangles) {
            Ok(m) => m,
            Err(_) => unreachable!("preset mesh has >= 3 vertices and >= 1 triangle"),
        };
        ArtifactSlotEntry::from_mesh(
            title,
            None,
            category,
            sherd_subtype,
            bead_subtype,
            bead_drill_type,
            mesh,
        )
    }

    fn apply_chs_report_template(&mut self, tpl: ChsReportTemplate) {
        self.active_chs_template = tpl;
        self.active_tile_code = tpl.code();
        if let Some(slot) = self.slots.get_mut(self.selected_slot_idx) {
            match tpl {
                ChsReportTemplate::VesselCompleteStandard => {
                    slot.category = ArtifactCategory::CompleteVessel;
                    slot.sherd_subtype = None;
                }
                ChsReportTemplate::VesselRimBaseReconstruct => {
                    slot.category = ArtifactCategory::Potsherd;
                    slot.sherd_subtype = Some(SherdSubtype::RimSherd);
                }
                ChsReportTemplate::SherdBodyThreeColumn => {
                    slot.category = ArtifactCategory::Potsherd;
                    slot.sherd_subtype = Some(SherdSubtype::BodySherd);
                }
                ChsReportTemplate::VesselHandleSpout => {
                    slot.category = ArtifactCategory::Potsherd;
                    slot.sherd_subtype = Some(SherdSubtype::HandleSpoutSherd);
                }
                ChsReportTemplate::RoofTileMaksae | ChsReportTemplate::RoofTilePlain => {
                    slot.category = ArtifactCategory::RoofTile;
                    slot.sherd_subtype = None;
                }
                ChsReportTemplate::LithicFlaked | ChsReportTemplate::LithicGround => {
                    slot.category = ArtifactCategory::Lithic;
                    slot.sherd_subtype = None;
                }
                ChsReportTemplate::MetalCorroded => {
                    slot.category = ArtifactCategory::CorrodedMetal;
                    slot.sherd_subtype = None;
                }
                ChsReportTemplate::JadeBeadMicro => {
                    slot.category = ArtifactCategory::BeadJade;
                    slot.bead_subtype = Some(BeadSubtype::TubularBead);
                    slot.bead_drill_type = Some(BeadDrillType::TypeIIb);
                }
            }
            slot.measurements = compute_artifact_measurements(&slot.mesh, slot.category);
            slot.rebuild_plate(
                self.flip_exterior_right,
                self.view3d_state.active_section_angle_deg,
                self.view3d_state.cut_y_mm,
                self.view3d_state.top_cut_z_mm,
            );
            self.status_message = format!(
                "문화재청 보고서 도면양식 적용: [{}] — {}",
                tpl.label_kr(),
                tpl.standard_spec_kr()
            );
        }
    }

    fn handle_tile_click(&mut self, code: &'static str) {
        self.active_tile_code = code;
        for tpl in ChsReportTemplate::ALL {
            if tpl.code() == code {
                self.apply_chs_report_template(tpl);
                return;
            }
        }
        match code {
            "OPEN" => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("3D 유물 스캔 (OBJ/PLY/STL)", &["obj", "ply", "stl"])
                    .pick_file()
                {
                    match load_any_mesh(&path) {
                        Ok(mesh) => {
                            let name = path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("외부 스캔 유물")
                                .to_string();
                            let slot = ArtifactSlotEntry::from_mesh(
                                name,
                                Some(path.clone()),
                                ArtifactCategory::Potsherd,
                                Some(SherdSubtype::RimSherd),
                                None,
                                None,
                                mesh,
                            );
                            self.slots.push(slot);
                            self.selected_slot_idx = self.slots.len() - 1;
                            self.status_message = format!("파일 로드 완료: {}", path.display());
                        }
                        Err(err) => {
                            self.active_dialog =
                                Some(ActiveStrataDialog::Error3Part(err.to_three_part()));
                        }
                    }
                }
            }
            "AP2" | "AP2_SYNC" => {
                if let Some(ap2_path) = rfd::FileDialog::new()
                    .add_filter("Arch3D Liner 프로젝트 (.ap2)", &["ap2"])
                    .pick_file()
                {
                    match Ap2Project::load_from_file(&ap2_path) {
                        Ok(ap2) => {
                            if let Some(slot) = self.slots.get_mut(self.selected_slot_idx) {
                                slot.mesh = slot.mesh.transformed(&ap2.transform_matrix);
                                slot.mesh.ap2_cut_offset_mm = Some(ap2.rc.plane_positions[0]);
                                self.view3d_state.top_cut_z_mm = ap2.rc.plane_positions[0];
                                slot.rebuild_plate(
                                    self.flip_exterior_right,
                                    self.view3d_state.active_section_angle_deg,
                                    self.view3d_state.cut_y_mm,
                                    self.view3d_state.top_cut_z_mm,
                                );
                                self.status_message = format!(
                                    ".ap2 정위치 행렬 및 절단면({:.1}mm) 연동 완료: {}",
                                    ap2.rc.plane_positions[0],
                                    ap2_path.display()
                                );
                            }
                        }
                        Err(err) => {
                            self.active_dialog =
                                Some(ActiveStrataDialog::Error3Part(err.to_three_part()));
                        }
                    }
                }
            }
            "SEG_3WAY" => {
                self.view3d_state.show_3way_segmentation =
                    !self.view3d_state.show_3way_segmentation;
            }
            "BEST_ANGLE" => {
                if let Some(slot) = self.slots.get_mut(self.selected_slot_idx) {
                    let best = slot.recommended_best_angle_deg;
                    self.view3d_state.active_section_angle_deg = best;
                    slot.rebuild_plate(
                        self.flip_exterior_right,
                        best,
                        self.view3d_state.cut_y_mm,
                        self.view3d_state.top_cut_z_mm,
                    );
                }
            }
            "BEAD_BORE" => {
                self.view3d_state.xray_inner_mode = !self.view3d_state.xray_inner_mode;
            }
            "LOCK_POSE" => {
                self.pose_locked = true;
                self.status_message = String::from("정위 및 수직 단면각 확정 완료 [단면확정 ●]");
            }
            "FIT_AXIS" => {
                self.view3d_state.tilt_deg = 0.0;
                self.view3d_state.roll_deg = 0.0;
                self.status_message = String::from("3D 수직 회전축 자동 정합 완료 (기울기 0.0°)");
            }
            "SLICE_V" => {
                self.view3d_state.cutaway_open = !self.view3d_state.cutaway_open;
                self.status_message = format!(
                    "전방 좌측 1/4 절개 단면: {}",
                    if self.view3d_state.cutaway_open { "ON" } else { "OFF" }
                );
            }
            "SLICE_H" => {
                if let Some(slot) = self.slots.get_mut(self.selected_slot_idx) {
                    let z = slot
                        .mesh
                        .ap2_cut_offset_mm
                        .unwrap_or(self.view3d_state.top_cut_z_mm);
                    self.view3d_state.top_cut_z_mm = z;
                    slot.rebuild_plate(
                        self.flip_exterior_right,
                        self.view3d_state.active_section_angle_deg,
                        self.view3d_state.cut_y_mm,
                        z,
                    );
                    self.status_message = format!("수평 횡단면(Top Cut Z = {z:.1}mm) 갱신 완료");
                }
            }
            "ROI_RUBBING_WIN" | "RUB_ROI" | "RUB_HAND" | "RUB_MSII" => {
                self.sheet_state.roi_drag_mode = true;
                self.sheet_state.show_roi_rubbing_window = true;
                if self.sheet_state.rubbing_roi_mm.is_none() {
                    if let Some(slot) = self.slots.get(self.selected_slot_idx) {
                        if let Some(reg) = slot.composed_plate.raster_regions.first() {
                            let rx0 = reg.rect_mm[0].min(reg.rect_mm[2]);
                            let rx1 = reg.rect_mm[0].max(reg.rect_mm[2]);
                            let ry0 = reg.rect_mm[1].min(reg.rect_mm[3]);
                            let ry1 = reg.rect_mm[1].max(reg.rect_mm[3]);
                            let rw = rx1 - rx0;
                            let rh = ry1 - ry0;
                            self.sheet_state.set_rubbing_roi_mm(Some([
                                rx0 + 0.04 * rw,
                                ry0 + 0.15 * rh,
                                rx0 + 0.48 * rw,
                                ry0 + 0.88 * rh,
                            ]));
                        }
                    }
                }
                self.status_message = String::from(
                    "부분 정밀 탁본창 활성화 · 도판 위에서 탁본할 위치를 마우스로 드래그하세요",
                );
            }
            "FLIP_R" => {
                self.flip_exterior_right = !self.flip_exterior_right;
                if let Some(slot) = self.slots.get_mut(self.selected_slot_idx) {
                    slot.rebuild_plate(
                        self.flip_exterior_right,
                        self.view3d_state.active_section_angle_deg,
                        self.view3d_state.cut_y_mm,
                        self.view3d_state.top_cut_z_mm,
                    );
                }
            }
            "FULL_TAB" => {
                self.sheet_full_tab_mode = !self.sheet_full_tab_mode;
            }
            "VERIFY_L" => {
                let checks = run_v1_to_v8_verification_harness()
                    .into_iter()
                    .map(|c| (format!("{} {}", c.gate_id, c.title), c.passed, c.detail))
                    .collect();
                self.active_dialog = Some(ActiveStrataDialog::PreExportVerifyL { checks });
            }
            "MULTI_XL" => {
                let slot_titles = self.slots.iter().map(|s| s.title.clone()).collect();
                self.active_dialog =
                    Some(ActiveStrataDialog::MultiArtifactPlateComposerXL { slot_titles });
            }
            "EXP_PDF" => {
                if let Some(slot) = self.slots.get(self.selected_slot_idx) {
                    let plan = compute_tiled_print_plan(
                        &slot.composed_plate,
                        PaperSheetSize::A4Portrait,
                        10.0,
                    );
                    self.active_dialog = Some(ActiveStrataDialog::TiledPrintPreviewL { plan });
                }
            }
            "EXP_SVG" => {
                if let Some(slot) = self.slots.get(self.selected_slot_idx) {
                    let out_path = std::env::temp_dir().join("strata_cutline_rc0.svg");
                    let svg = render_plate_to_illustrator_svg(&slot.composed_plate);
                    if std::fs::write(&out_path, svg.as_bytes()).is_ok() {
                        self.status_message =
                            format!("1mm=5px SVG 제도선 저장 완료: {}", out_path.display());
                    }
                }
            }
            "EXP_RET" => {
                if let Some(slot) = self.slots.get(self.selected_slot_idx) {
                    let out_dir = std::env::temp_dir().join("strata_ret_export");
                    match export_ret_folder_package(
                        &out_dir,
                        &slot.composed_plate,
                        &slot.mesh,
                        Some(&slot.bvh),
                        self.rubbing_params,
                    ) {
                        Ok(manifest) => {
                            self.status_message = format!(
                                "ret/ 패키지 내보내기 완료: {}",
                                manifest.output_dir.display()
                            );
                        }
                        Err(err) => {
                            self.active_dialog =
                                Some(ActiveStrataDialog::Error3Part(err.to_three_part()));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

impl eframe::App for StrataStudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 실제 실행 창 스왑체인 스크린샷 캡처 이벤트 처리 (`--capture-window`)
        if let Some(ref out_path) = self.capture_window_path.clone() {
            let mut captured_image = None;
            ctx.input(|i| {
                for ev in &i.events {
                    if let egui::Event::Screenshot { image, .. } = ev {
                        captured_image = Some(image.clone());
                    }
                }
            });
            if let Some(color_img) = captured_image {
                let w = color_img.width() as u32;
                let h = color_img.height() as u32;
                let mut rgba = RgbaImage::new(w, h);
                for y in 0..h {
                    for x in 0..w {
                        let px = color_img[(x as usize, y as usize)];
                        rgba.put_pixel(x, y, Rgba([px.r(), px.g(), px.b(), px.a()]));
                    }
                }
                if let Some(parent) = out_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = rgba.save(out_path);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }

        if !self.splash.is_finished() {
            let dt = ctx.input(|i| i.stable_dt).clamp(0.005, 0.1);
            self.splash.show_egui(ctx, dt);
            ctx.request_repaint();
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Tab)) {
            self.sheet_full_tab_mode = !self.sheet_full_tab_mode;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::R)) {
            self.flip_exterior_right = !self.flip_exterior_right;
            if let Some(slot) = self.slots.get_mut(self.selected_slot_idx) {
                slot.rebuild_plate(
                    self.flip_exterior_right,
                    self.view3d_state.active_section_angle_deg,
                    self.view3d_state.cut_y_mm,
                    self.view3d_state.top_cut_z_mm,
                );
            }
        }

        show_active_dialog(ctx, &mut self.active_dialog);

        let wash = egui::Color32::from_rgb(WASH.rgb[0], WASH.rgb[1], WASH.rgb[2]);
        let hanji = egui::Color32::from_rgb(GROUND.rgb[0], GROUND.rgb[1], GROUND.rgb[2]);
        let surface = egui::Color32::from_rgb(SURFACE.rgb[0], SURFACE.rgb[1], SURFACE.rgb[2]);
        let desk = egui::Color32::from_rgb(DESK.rgb[0], DESK.rgb[1], DESK.rgb[2]);
        let clay_wash = egui::Color32::from_rgb(CLAY_WASH.rgb[0], CLAY_WASH.rgb[1], CLAY_WASH.rgb[2]);
        let clay_ink = egui::Color32::from_rgb(CLAY_INK.rgb[0], CLAY_INK.rgb[1], CLAY_INK.rgb[2]);
        let line_col = egui::Color32::from_rgb(LINE.rgb[0], LINE.rgb[1], LINE.rgb[2]);
        let edge_col = egui::Color32::from_rgb(EDGE.rgb[0], EDGE.rgb[1], EDGE.rgb[2]);
        let ink = egui::Color32::from_rgb(INK.rgb[0], INK.rgb[1], INK.rgb[2]);
        let muted = egui::Color32::from_rgb(MUTED.rgb[0], MUTED.rgb[1], MUTED.rgb[2]);
        let terra = egui::Color32::from_rgb(ACTION.rgb[0], ACTION.rgb[1], ACTION.rgb[2]);

        let tile_sz = RIBBON_TILE_SIZE as f32;

        // Tier 1: 상단 통합 유물 선택 및 상태 바 (34px 고정, 불필요한 ①~④ 단계 탭 제거)
        egui::TopBottomPanel::top("tier1_tab_bar")
            .exact_height(TAB_BAR_HEIGHT as f32)
            .frame(
                egui::Frame::NONE
                    .fill(surface)
                    .inner_margin(egui::Margin::symmetric(12, 4)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.colored_label(
                        ink,
                        "◆ 동국유물실측 · STRATA  (국가유산청 발굴조사보고서 실측도면 표준 시스템)",
                    );
                    ui.separator();
                    ui.colored_label(muted, "작업 유물:");
                    let mut newly_selected = None;
                    for (idx, slot) in self.slots.iter().enumerate() {
                        let is_sel = idx == self.selected_slot_idx;
                        let btn = egui::Button::new(
                            egui::RichText::new(format!("{}. {}", idx + 1, slot.title))
                                .size(12.0)
                                .color(if is_sel { hanji } else { ink }),
                        )
                        .fill(if is_sel { ink } else { wash })
                        .corner_radius(4.0);
                        if ui.add(btn).clicked() {
                            newly_selected = Some(idx);
                        }
                    }
                    if let Some(idx) = newly_selected {
                        self.selected_slot_idx = idx;
                    }
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("+ 3D 유물 열기...").size(11.5).color(terra),
                            )
                            .fill(clay_wash)
                            .stroke(egui::Stroke::new(1.0_f32, terra))
                            .corner_radius(4.0),
                        )
                        .clicked()
                    {
                        self.handle_tile_click("OPEN");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let badge = if self.pose_locked {
                            "● 정위치 · 절단틀 확정됨"
                        } else {
                            "▲ 자세 미세조정 중"
                        };
                        ui.colored_label(terra, badge);
                    });
                });
            });

        // Tier 2: TileRibbon (98px 고정, 국가유산청(구 문화재청) 보고서 도면 작성양식 10종 + 핵심 실측 도구 통합 리본)
        egui::TopBottomPanel::top("tier2_tile_ribbon")
            .exact_height(RIBBON_HEIGHT as f32)
            .frame(
                egui::Frame::NONE
                    .fill(surface)
                    .stroke(egui::Stroke::new(1.0_f32, line_col))
                    .inner_margin(egui::Margin::symmetric(10, 6)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt("tier2_ribbon_scroll")
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let tiles = unified_studio_ribbon_tiles();
                            let mut clicked_code = None;
                            let mut idx = 0usize;

                            while idx < tiles.len() {
                                let grp_name = tiles[idx].group_title;
                                let start_idx = idx;
                                while idx < tiles.len() && tiles[idx].group_title == grp_name {
                                    idx += 1;
                                }
                                let grp_slice = &tiles[start_idx..idx];
                                let cell_w = tile_sz + 20.0;
                                let grp_w = (grp_slice.len() as f32) * cell_w;

                                let (grp_rect, _) = ui.allocate_exact_size(
                                    egui::vec2(grp_w, 84.0),
                                    egui::Sense::hover(),
                                );

                                for (local_i, tile) in grp_slice.iter().enumerate() {
                                    let cell_left = grp_rect.left() + (local_i as f32) * cell_w;
                                    let cell_rect = egui::Rect::from_min_size(
                                        egui::pos2(cell_left, grp_rect.top()),
                                        egui::vec2(cell_w, 68.0),
                                    );
                                    let resp = ui.interact(
                                        cell_rect,
                                        egui::Id::new(("ribbon_tile", tile.code)),
                                        egui::Sense::click(),
                                    );
                                    if resp.clicked() {
                                        clicked_code = Some(tile.code);
                                    }

                                    let is_active = self.active_tile_code == tile.code;
                                    let is_hovered = resp.hovered();
                                    let p = ui.painter_at(grp_rect);

                                    let tile_box = egui::Rect::from_min_size(
                                        egui::pos2(
                                            cell_rect.center().x - tile_sz * 0.5,
                                            grp_rect.top(),
                                        ),
                                        egui::vec2(tile_sz, tile_sz),
                                    );

                                    let (bg_col, border_col, icon_primary, icon_accent) =
                                        if tile.is_primary_action {
                                            (terra, terra, hanji, hanji)
                                        } else if is_active {
                                            (clay_wash, terra, clay_ink, terra)
                                        } else if is_hovered {
                                            (wash, edge_col, ink, terra)
                                        } else {
                                            (desk, line_col, ink, terra)
                                        };

                                    p.rect_filled(tile_box, 10.0, bg_col);
                                    p.rect_stroke(
                                        tile_box,
                                        10.0,
                                        egui::Stroke::new(1.0_f32, border_col),
                                        egui::StrokeKind::Inside,
                                    );

                                    draw_ribbon_tile_vector_icon(
                                        &p,
                                        tile_box,
                                        tile.icon_kind,
                                        icon_primary,
                                        icon_accent,
                                    );

                                    p.text(
                                        egui::pos2(cell_rect.center().x, tile_box.bottom() + 2.5),
                                        egui::Align2::CENTER_TOP,
                                        tile.label_kr,
                                        egui::FontId::proportional(11.0),
                                        if is_active { clay_ink } else { ink },
                                    );
                                }

                                let grp_painter = ui.painter_at(grp_rect);
                                grp_painter.text(
                                    egui::pos2(grp_rect.center().x, grp_rect.bottom() - 12.0),
                                    egui::Align2::CENTER_TOP,
                                    grp_name,
                                    egui::FontId::proportional(10.5),
                                    muted,
                                );

                                if idx < tiles.len() {
                                    let (sep_rect, _) = ui.allocate_exact_size(
                                        egui::vec2(10.0, 82.0),
                                        egui::Sense::hover(),
                                    );
                                    let sp = ui.painter_at(sep_rect);
                                    sp.line_segment(
                                        [
                                            egui::pos2(sep_rect.center().x, sep_rect.top() + 4.0),
                                            egui::pos2(
                                                sep_rect.center().x,
                                                sep_rect.bottom() - 4.0,
                                            ),
                                        ],
                                        egui::Stroke::new(1.0_f32, line_col),
                                    );
                                }
                            }

                            if let Some(code) = clicked_code {
                                self.handle_tile_click(code);
                            }
                        });
                    });
            });

        // Tier 3: ctxBar (36px 고정 — 중복 유물군/농도바 제거, 문화재청 보고서 도면양식 요약 및 부분 탁본·방안지 제어)
        egui::TopBottomPanel::top("tier3_ctx_bar")
            .exact_height(CTX_BAR_HEIGHT as f32)
            .frame(
                egui::Frame::NONE
                    .fill(wash)
                    .stroke(egui::Stroke::new(1.0_f32, line_col))
                    .inner_margin(egui::Margin::symmetric(12, 4)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.colored_label(
                        clay_ink,
                        format!("보고서 도면양식: [{}]", self.active_chs_template.label_kr()),
                    );
                    ui.colored_label(muted, self.active_chs_template.standard_spec_kr());
                    ui.separator();
                    if ui
                        .selectable_label(
                            self.sheet_state.roi_drag_mode,
                            "⬚ 탁본할 위치 드래그(부분 탁본)",
                        )
                        .clicked()
                    {
                        self.sheet_state.roi_drag_mode = !self.sheet_state.roi_drag_mode;
                    }
                    if ui
                        .selectable_label(
                            self.sheet_state.show_roi_rubbing_window,
                            "정밀 탁본창(농도·논문 MSII)",
                        )
                        .clicked()
                    {
                        self.sheet_state.show_roi_rubbing_window =
                            !self.sheet_state.show_roi_rubbing_window;
                    }
                    ui.separator();
                    if ui
                        .checkbox(&mut self.flip_exterior_right, "R 반전(우측 외면)")
                        .changed()
                    {
                        if let Some(slot) = self.slots.get_mut(self.selected_slot_idx) {
                            slot.rebuild_plate(
                                self.flip_exterior_right,
                                self.view3d_state.active_section_angle_deg,
                                self.view3d_state.cut_y_mm,
                                self.view3d_state.top_cut_z_mm,
                            );
                        }
                    }
                    ui.checkbox(&mut self.sheet_full_tab_mode, "Tab 100% 도판 확대");
                    ui.checkbox(&mut self.show_mm_grid, "실측 방안지(1·5·10mm)");
                });
            });

        // Tier 5: StatusBar (30px 고정, #FAF9F5 Ground)
        egui::TopBottomPanel::bottom("tier5_status_bar")
            .exact_height(STATUS_BAR_HEIGHT as f32)
            .frame(
                egui::Frame::NONE
                    .fill(hanji)
                    .stroke(egui::Stroke::new(1.0_f32, line_col))
                    .inner_margin(egui::Margin::symmetric(12, 4)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.colored_label(ink, &self.status_message);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(slot) = self.slots.get(self.selected_slot_idx) {
                            ui.colored_label(
                                muted,
                                format!(
                                    "정점: {} · 삼각면: {} · 권장축척: {} · 1mm=5.0px",
                                    slot.mesh.positions.len(),
                                    slot.mesh.triangles.len(),
                                    slot.measurements.recommended_scale.label()
                                ),
                            );
                        }
                    });
                });
            });

        // Tier 4: 좌측 슬롯 패널(176px)을 완전히 제거하고 전체 화면 폭 100%(1440px)를 3D 뷰포트 + 2D 실측 방안지로 확장!
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(hanji).inner_margin(6.0))
            .show(ctx, |ui| {
                let win_rect = ctx.screen_rect();
                let geom = R1FiveTierGeometry::compute_full_width_workspace(
                    win_rect.width(),
                    win_rect.height(),
                    self.sheet_full_tab_mode,
                );
                let _ = ScalePreset::Full1To1;

                if let Some(slot) = self.slots.get_mut(self.selected_slot_idx) {
                    let prev_angle = self.view3d_state.active_section_angle_deg;
                    let prev_cut_y = self.view3d_state.cut_y_mm;
                    let prev_top_z = self.view3d_state.top_cut_z_mm;
                    let avail_h = ui.available_height().max(200.0);
                    ui.horizontal_top(|ui| {
                        if !self.sheet_full_tab_mode {
                            ui.allocate_ui_with_layout(
                                egui::vec2(geom.view3d_w, avail_h),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    render_view3d_pane_egui(
                                        ui,
                                        &slot.mesh,
                                        Some(&slot.segmentation.vertex_regions),
                                        Some(&slot.rim_gap_chart),
                                        slot.recommended_best_angle_deg,
                                        &mut self.view3d_state,
                                    );
                                },
                            );
                            ui.add_space(6.0);
                        }
                        let sheet_w = ui.available_width().max(240.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(sheet_w, avail_h),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                let _ = render_sheet_pane_egui(
                                    ui,
                                    &slot.composed_plate,
                                    &slot.measurements,
                                    &slot.mesh,
                                    self.view3d_state.active_section_angle_deg,
                                    self.rubbing_params.ink_density_pct,
                                    self.show_mm_grid,
                                    &mut self.sheet_state,
                                );
                            },
                        );
                    });
                    if (self.view3d_state.active_section_angle_deg - prev_angle).abs() > 0.1
                        || (self.view3d_state.cut_y_mm - prev_cut_y).abs() > 0.1
                        || (self.view3d_state.top_cut_z_mm - prev_top_z).abs() > 0.1
                    {
                        slot.rebuild_plate(
                            self.flip_exterior_right,
                            self.view3d_state.active_section_angle_deg,
                            self.view3d_state.cut_y_mm,
                            self.view3d_state.top_cut_z_mm,
                        );
                    }
                }
            });

        // 실제 실행 창 프레임버퍼 캡처 요청 (`--capture-window`)
        if self.capture_window_path.is_some() {
            self.capture_frames_rendered += 1;
            if self.capture_frames_rendered >= 6 && !self.screenshot_requested {
                self.screenshot_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            ctx.request_repaint();
        }
    }
}

fn configure_korean_fonts_and_strata_theme(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let font_candidates = [
        r"C:\Windows\Fonts\malgun.ttf",
        r"C:\Windows\Fonts\NanumGothic.ttf",
    ];
    for candidate in font_candidates {
        if let Ok(bytes) = std::fs::read(candidate) {
            fonts.font_data.insert(
                String::from("korean_sys"),
                egui::FontData::from_owned(bytes).into(),
            );
            if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                vec.insert(0, String::from("korean_sys"));
            }
            if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                vec.push(String::from("korean_sys"));
            }
            break;
        }
    }
    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|style| {
        style.visuals = egui::Visuals::light();
        style.visuals.panel_fill =
            egui::Color32::from_rgb(GROUND.rgb[0], GROUND.rgb[1], GROUND.rgb[2]);
        style.visuals.window_fill =
            egui::Color32::from_rgb(GROUND.rgb[0], GROUND.rgb[1], GROUND.rgb[2]);
        style.visuals.selection.bg_fill =
            egui::Color32::from_rgb(CLAY_WASH.rgb[0], CLAY_WASH.rgb[1], CLAY_WASH.rgb[2]);
        style.visuals.selection.stroke = egui::Stroke::new(
            1.0_f32,
            egui::Color32::from_rgb(CLAY_INK.rgb[0], CLAY_INK.rgb[1], CLAY_INK.rgb[2]),
        );
        style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(
            1.0_f32,
            egui::Color32::from_rgb(EDGE.rgb[0], EDGE.rgb[1], EDGE.rgb[2]),
        );
    });
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    let mut verify_all = false;
    let mut screenshot_dir: Option<PathBuf> = None;
    let mut capture_window_path: Option<PathBuf> = None;
    let mut initial_tab: Option<WorkflowTab> = None;
    let mut initial_slot: Option<usize> = None;
    let mut open_rubbing_win = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--verify-all" | "--export-golden" => {
                verify_all = true;
            }
            "--screenshot-tour" if i + 1 < args.len() => {
                screenshot_dir = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--capture-window" if i + 1 < args.len() => {
                capture_window_path = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--tab" if i + 1 < args.len() => {
                initial_tab = match args[i + 1].as_str() {
                    "1" => Some(WorkflowTab::Step1LoadAndClassify),
                    "2" => Some(WorkflowTab::Step2OrientAndRefit),
                    "3" => Some(WorkflowTab::Step3SectionAndRubbing),
                    "4" => Some(WorkflowTab::Step4PlateAndExport),
                    _ => None,
                };
                i += 1;
            }
            "--slot" if i + 1 < args.len() => {
                initial_slot = args[i + 1].parse::<usize>().ok();
                i += 1;
            }
            "--open-rubbing-win" => {
                open_rubbing_win = true;
            }
            _ => {}
        }
        i += 1;
    }

    if verify_all || screenshot_dir.is_some() {
        let checks = run_v1_to_v8_verification_harness();
        println!("=== STRATA · 동국유물실측 V1~V8 하네스 검증 결과 ===");
        let mut all_passed = true;
        for c in &checks {
            let status = if c.passed { "PASS" } else { "FAIL" };
            if !c.passed {
                all_passed = false;
            }
            println!("[{}] {} — {}: {}", status, c.gate_id, c.title, c.detail);
        }

        if let Some(ref dir) = screenshot_dir {
            let files = generate_screenshot_tour_and_golden_outputs(dir)?;
            println!(
                "=== 시각 검증 투어 및 골든 산출물 생성 완료 ({}개 파일) ===",
                files.len()
            );
            for f in &files {
                println!("  -> {f}");
            }
        }

        let _ = render_plate_to_pdf_bytes;
        if !all_passed {
            std::process::exit(1);
        }
        return Ok(());
    }

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("STRATA · 동국유물실측 (Rust Archaeological 3D Studio v2.0)")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1180.0, 760.0]),
        ..Default::default()
    };

    eframe::run_native(
        "STRATA · 동국유물실측",
        native_options,
        Box::new(move |cc| {
            let mut app = StrataStudioApp::new(
                cc,
                capture_window_path,
                initial_tab,
                initial_slot,
            );
            if open_rubbing_win {
                app.handle_tile_click("ROI_RUBBING_WIN");
            }
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| format!("GUI 실행 오류: {e}").into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_v1_to_v8_harness_and_r1_layout() {
        let checks = run_v1_to_v8_verification_harness();
        assert_eq!(checks.len(), 8);
        for c in &checks {
            assert!(c.passed, "Gate {} failed: {}", c.gate_id, c.detail);
        }
    }

    #[test]
    fn test_sheet_pane_and_view3d_vertical_stack_stays_inside_window() {
        let slot = StrataStudioApp::make_preset_slot(
            "레이아웃 검증용 완형 토기",
            ArtifactCategory::CompleteVessel,
            None,
            None,
            None,
            60.0,
            70.0,
        );
        let mut view3d_state = View3DPaneState::default();
        let mut sheet_state = SheetPaneState::default();
        let ctx = egui::Context::default();
        let mut raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440.0, 900.0),
            )),
            ..Default::default()
        };

        let mut captured_rects = None;
        let _ = ctx.run(raw_input.take(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let geom = R1FiveTierGeometry::compute(1440.0, 900.0, false);
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(geom.view3d_w, geom.work_area_h),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            render_view3d_pane_egui(
                                ui,
                                &slot.mesh,
                                Some(&slot.segmentation.vertex_regions),
                                Some(&slot.rim_gap_chart),
                                slot.recommended_best_angle_deg,
                                &mut view3d_state,
                            );
                        },
                    );
                    ui.add_space(6.0);
                    let sheet_w = ui.available_width().max(240.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(sheet_w, geom.work_area_h),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            captured_rects = Some(render_sheet_pane_egui(
                                ui,
                                &slot.composed_plate,
                                &slot.measurements,
                                &slot.mesh,
                                view3d_state.active_section_angle_deg,
                                80.0,
                                true,
                                &mut sheet_state,
                            ));
                        },
                    );
                });
            });
        });

        let (header_rect, strip_rect, sheet_rect) =
            captured_rects.expect("SheetPane rects must be rendered");
        // 도판 머리줄, 치수 대장 띠, 도판 캔버스가 모두 화면 가로 폭(1440px) 내부에 있고 수직으로 쌓여야 한다!
        assert!(
            sheet_rect.right() <= 1440.0,
            "sheet_rect pushed offscreen to the right: {:?}",
            sheet_rect
        );
        assert!(
            (header_rect.left() - sheet_rect.left()).abs() < 2.0,
            "header_rect and sheet_rect must share the same left X coordinate"
        );
        assert!(
            strip_rect.top() >= header_rect.bottom() - 1.0,
            "strip_rect must be below header_rect"
        );
        assert!(
            sheet_rect.top() >= strip_rect.bottom() - 1.0,
            "sheet_rect must be below strip_rect"
        );
        assert!(
            sheet_rect.width() > 600.0 && sheet_rect.height() > 400.0,
            "sheet_rect must occupy the 73% right workspace: {:?}",
            sheet_rect
        );
    }
}
