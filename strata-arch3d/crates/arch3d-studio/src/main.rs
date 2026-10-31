pub mod dialogs;
pub mod harness_runner;
pub mod layout_r1;
pub mod sheet_pane;
pub mod splash;
pub mod view3d_pane;

use arch3d_core::strata_tokens::{
    metrics::{
        CTX_BAR_HEIGHT, RIBBON_HEIGHT, RIBBON_TILE_SIZE, SLOT_LIST_WIDTH, STATUS_BAR_HEIGHT,
        TAB_BAR_HEIGHT,
    },
    tokens::{ACTION, EDGE, GROUND, INK, MUTED, WASH},
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
use layout_r1::{R1FiveTierGeometry, WorkflowTab};
use sheet_pane::render_sheet_pane_egui;
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
        let occluded_spans = infer_occluded_dashed_spans(&vertical_section.polylines_2d, 1.5, 24.0);
        let sherd_strategy = measurements
            .sherd_circle_fit
            .as_ref()
            .map(|f| f.plate_strategy);
        let composed_plate = compose_archaeological_plate(&PlateComposeInput {
            title: &title_str,
            measurements: &measurements,
            vertical_section: &vertical_section,
            horizontal_sections: &[],
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
            occluded_spans,
            composed_plate,
        }
    }

    fn rebuild_plate(&mut self, flip_exterior_right: bool, section_angle_deg: f64) {
        self.vertical_section = slice_mesh_with_fixed_plane(
            &self.mesh,
            Some(&self.bvh),
            CutPlaneOrientation::VerticalLongitudinal,
            section_angle_deg,
            0.0,
            0.25,
        );
        self.occluded_spans =
            infer_occluded_dashed_spans(&self.vertical_section.polylines_2d, 1.5, 24.0);
        self.measurements = compute_artifact_measurements(&self.mesh, self.category);
        let sherd_strategy = self
            .measurements
            .sherd_circle_fit
            .as_ref()
            .map(|f| f.plate_strategy);
        self.composed_plate = compose_archaeological_plate(&PlateComposeInput {
            title: &self.title,
            measurements: &self.measurements,
            vertical_section: &self.vertical_section,
            horizontal_sections: &[],
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
    active_tab: WorkflowTab,
    active_tile_code: &'static str,
    slots: Vec<ArtifactSlotEntry>,
    selected_slot_idx: usize,
    view3d_state: View3DPaneState,
    rubbing_params: RubbingRenderParams,
    flip_exterior_right: bool,
    sheet_full_tab_mode: bool,
    show_mm_grid: bool,
    pose_locked: bool,
    active_dialog: Option<ActiveStrataDialog>,
    status_message: String,
}

impl StrataStudioApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_korean_fonts_and_strata_theme(&cc.egui_ctx);

        let mut slots = Vec::new();
        let p_7obj = Path::new(r"E:\034(7-실4-4)\7지구 9호-7.obj");
        let p_7ap2 = Path::new(r"E:\034(7-실4-4)\7지구 9호-7.ap2");
        if p_7obj.exists() && p_7ap2.exists() {
            if let (Ok(m), Ok(ap2)) = (load_any_mesh(p_7obj), Ap2Project::load_from_file(p_7ap2))
            {
                let aligned = m.transformed(&ap2.transform_matrix);
                slots.push(ArtifactSlotEntry::from_mesh(
                    "7지구 9호-7 (완형 대접)",
                    Some(p_7obj.to_path_buf()),
                    ArtifactCategory::CompleteVessel,
                    None,
                    None,
                    None,
                    aligned,
                ));
            }
        }

        slots.push(Self::make_preset_slot(
            "구연·동체 편조각 (추정복원)",
            ArtifactCategory::Potsherd,
            Some(SherdSubtype::RimSherd),
            None,
            None,
            58.0,
            64.0,
        ));
        slots.push(Self::make_preset_slot(
            "벽옥제 관옥 (양방향 천공 Ⅱb)",
            ArtifactCategory::BeadJade,
            None,
            Some(BeadSubtype::TubularBead),
            Some(BeadDrillType::TypeIIb),
            4.2,
            12.5,
        ));
        slots.push(Self::make_preset_slot(
            "통일신라 수키와편 (포목흔)",
            ArtifactCategory::RoofTile,
            None,
            None,
            None,
            85.0,
            145.0,
        ));
        slots.push(Self::make_preset_slot(
            "구석기 좀돌날 몸돌 (3단 선굵기)",
            ArtifactCategory::Lithic,
            None,
            None,
            None,
            24.0,
            48.0,
        ));
        slots.push(Self::make_preset_slot(
            "삼국시대 철부 (3층 부식분리)",
            ArtifactCategory::CorrodedMetal,
            None,
            None,
            None,
            34.0,
            96.0,
        ));

        Self {
            splash: SplashState::default(),
            active_tab: WorkflowTab::Step1LoadAndClassify,
            active_tile_code: "OPEN",
            slots,
            selected_slot_idx: 0,
            view3d_state: View3DPaneState::default(),
            rubbing_params: RubbingRenderParams::default(),
            flip_exterior_right: false,
            sheet_full_tab_mode: false,
            show_mm_grid: true,
            pose_locked: true,
            active_dialog: None,
            status_message: String::from(
                "준비 완료 · 1mm = 5.0px 정밀 스케일 잠금 · R(좌우반전) / Tab(100% 도판 확대)",
            ),
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
        let mesh = TriMesh::from_positions_and_triangles(title, positions, triangles)
            .expect("preset mesh ok");
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

    fn handle_tile_click(&mut self, code: &'static str) {
        self.active_tile_code = code;
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
            "CAT_VESSEL" => self.set_current_category(ArtifactCategory::CompleteVessel),
            "CAT_SHERD" => self.set_current_category(ArtifactCategory::Potsherd),
            "CAT_BEAD" => self.set_current_category(ArtifactCategory::BeadJade),
            "CAT_TILE" => self.set_current_category(ArtifactCategory::RoofTile),
            "SEG_3WAY" => {
                self.view3d_state.show_3way_segmentation =
                    !self.view3d_state.show_3way_segmentation;
            }
            "BEST_ANGLE" => {
                if let Some(slot) = self.slots.get_mut(self.selected_slot_idx) {
                    let best = slot.recommended_best_angle_deg;
                    self.view3d_state.active_section_angle_deg = best;
                    slot.rebuild_plate(self.flip_exterior_right, best);
                }
            }
            "BEAD_BORE" => {
                self.view3d_state.xray_inner_mode = !self.view3d_state.xray_inner_mode;
            }
            "LOCK_POSE" => {
                self.pose_locked = true;
                self.status_message = String::from("정위 및 수직 단면각 확정 완료 [단면확정 ✓]");
            }
            "FLIP_R" => {
                self.flip_exterior_right = !self.flip_exterior_right;
                if let Some(slot) = self.slots.get_mut(self.selected_slot_idx) {
                    slot.rebuild_plate(
                        self.flip_exterior_right,
                        self.view3d_state.active_section_angle_deg,
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

    fn set_current_category(&mut self, cat: ArtifactCategory) {
        if let Some(slot) = self.slots.get_mut(self.selected_slot_idx) {
            slot.category = cat;
            slot.rebuild_plate(
                self.flip_exterior_right,
                self.view3d_state.active_section_angle_deg,
            );
        }
    }
}

const ALL_CATEGORIES: [ArtifactCategory; 6] = [
    ArtifactCategory::CompleteVessel,
    ArtifactCategory::Potsherd,
    ArtifactCategory::BeadJade,
    ArtifactCategory::RoofTile,
    ArtifactCategory::Lithic,
    ArtifactCategory::CorrodedMetal,
];

impl eframe::App for StrataStudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
                );
            }
        }

        show_active_dialog(ctx, &mut self.active_dialog);

        let wash = egui::Color32::from_rgb(WASH.rgb[0], WASH.rgb[1], WASH.rgb[2]);
        let hanji = egui::Color32::from_rgb(GROUND.rgb[0], GROUND.rgb[1], GROUND.rgb[2]);
        let ink = egui::Color32::from_rgb(INK.rgb[0], INK.rgb[1], INK.rgb[2]);
        let muted = egui::Color32::from_rgb(MUTED.rgb[0], MUTED.rgb[1], MUTED.rgb[2]);
        let terra = egui::Color32::from_rgb(ACTION.rgb[0], ACTION.rgb[1], ACTION.rgb[2]);

        let tile_sz = RIBBON_TILE_SIZE as f32;

        // Tier 1: TabBar (34px 고정)
        egui::TopBottomPanel::top("tier1_tab_bar")
            .exact_height(TAB_BAR_HEIGHT as f32)
            .frame(
                egui::Frame::NONE
                    .fill(wash)
                    .inner_margin(egui::Margin::symmetric(12, 4)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.colored_label(ink, "STRATA · 동국유물실측");
                    ui.separator();
                    for tab in WorkflowTab::ALL {
                        let is_active = self.active_tab == tab;
                        let btn = egui::Button::new(
                            egui::RichText::new(tab.label_kr())
                                .color(if is_active { hanji } else { ink }),
                        )
                        .fill(if is_active { ink } else { wash });
                        if ui.add(btn).clicked() {
                            self.active_tab = tab;
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let badge = if self.pose_locked {
                            "[단면확정 ✓]"
                        } else {
                            "[정위조정 중]"
                        };
                        ui.colored_label(terra, badge);
                    });
                });
            });

        // Tier 2: TileRibbon (98px 고정, 50×50 타일)
        egui::TopBottomPanel::top("tier2_tile_ribbon")
            .exact_height(RIBBON_HEIGHT as f32)
            .frame(
                egui::Frame::NONE
                    .fill(hanji)
                    .inner_margin(egui::Margin::symmetric(12, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let mut clicked_code = None;
                    for tile in self.active_tab.ribbon_tiles() {
                        let is_active = self.active_tile_code == tile.code;
                        let (rect, resp) = ui.allocate_exact_size(
                            egui::vec2(tile_sz + 18.0, tile_sz + 24.0),
                            egui::Sense::click(),
                        );
                        if resp.clicked() {
                            clicked_code = Some(tile.code);
                        }
                        let p = ui.painter_at(rect);
                        let tile_box = egui::Rect::from_min_size(
                            egui::pos2(rect.center().x - tile_sz * 0.5, rect.top()),
                            egui::vec2(tile_sz, tile_sz),
                        );
                        p.rect_filled(tile_box, 4.0, if is_active { ink } else { wash });
                        if is_active {
                            let bottom_bar = egui::Rect::from_min_size(
                                egui::pos2(tile_box.left(), tile_box.bottom() - 2.0),
                                egui::vec2(tile_sz, 2.0),
                            );
                            p.rect_filled(bottom_bar, 0.0, terra);
                        }
                        p.text(
                            tile_box.center(),
                            egui::Align2::CENTER_CENTER,
                            tile.shortcut,
                            egui::FontId::monospace(11.0),
                            if is_active { hanji } else { ink },
                        );
                        p.text(
                            egui::pos2(rect.center().x, tile_box.bottom() + 4.0),
                            egui::Align2::CENTER_TOP,
                            tile.label_kr,
                            egui::FontId::proportional(11.0),
                            ink,
                        );
                    }
                    if let Some(code) = clicked_code {
                        self.handle_tile_click(code);
                    }
                });
            });

        // Tier 3: ctxBar (36px 고정)
        egui::TopBottomPanel::top("tier3_ctx_bar")
            .exact_height(CTX_BAR_HEIGHT as f32)
            .frame(
                egui::Frame::NONE
                    .fill(wash)
                    .inner_margin(egui::Margin::symmetric(12, 4)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let current_cat = self
                        .slots
                        .get(self.selected_slot_idx)
                        .map(|s| s.category)
                        .unwrap_or(ArtifactCategory::CompleteVessel);

                    ui.colored_label(muted, "유물군:");
                    for cat in ALL_CATEGORIES {
                        if ui
                            .selectable_label(current_cat == cat, cat.label_kr())
                            .clicked()
                        {
                            self.set_current_category(cat);
                        }
                    }
                    ui.separator();
                    ui.colored_label(muted, "먹물농도:");
                    ui.add(
                        egui::Slider::new(&mut self.rubbing_params.ink_density_pct, 10.0..=100.0)
                            .suffix("%"),
                    );
                    ui.separator();
                    if ui
                        .checkbox(&mut self.flip_exterior_right, "R 반전(우측 외면)")
                        .changed()
                    {
                        if let Some(slot) = self.slots.get_mut(self.selected_slot_idx) {
                            slot.rebuild_plate(
                                self.flip_exterior_right,
                                self.view3d_state.active_section_angle_deg,
                            );
                        }
                    }
                    ui.checkbox(&mut self.sheet_full_tab_mode, "Tab 100% 도판 확대");
                });
            });

        // Tier 5: StatusBar (30px 고정)
        egui::TopBottomPanel::bottom("tier5_status_bar")
            .exact_height(STATUS_BAR_HEIGHT as f32)
            .frame(
                egui::Frame::NONE
                    .fill(wash)
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

        // Tier 4-A: Left SlotList (176px 고정)
        egui::SidePanel::left("tier4_slot_list")
            .exact_width(SLOT_LIST_WIDTH as f32)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(wash).inner_margin(8.0))
            .show(ctx, |ui| {
                ui.colored_label(ink, "발굴 유물 슬롯 목록");
                ui.separator();
                for (idx, slot) in self.slots.iter().enumerate() {
                    let is_sel = idx == self.selected_slot_idx;
                    let label = format!(
                        "{}. {}\n   [{} · {}]",
                        idx + 1,
                        slot.title,
                        slot.category.label_kr(),
                        slot.measurements.recommended_scale.label()
                    );
                    if ui.selectable_label(is_sel, label).clicked() {
                        self.selected_slot_idx = idx;
                    }
                    ui.add_space(4.0);
                }
            });

        // Tier 4-B & 4-C: Center View3D (27% or hidden on Tab) + Right SheetPane (73% or 100%)
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(hanji).inner_margin(6.0))
            .show(ctx, |ui| {
                let win_rect = ctx.screen_rect();
                let geom = R1FiveTierGeometry::compute(
                    win_rect.width(),
                    win_rect.height(),
                    self.sheet_full_tab_mode,
                );
                let _ = ScalePreset::Full1To1;

                if let Some(slot) = self.slots.get_mut(self.selected_slot_idx) {
                    let prev_angle = self.view3d_state.active_section_angle_deg;
                    ui.horizontal_top(|ui| {
                        if !self.sheet_full_tab_mode {
                            ui.allocate_ui(egui::vec2(geom.view3d_w, geom.work_area_h), |ui| {
                                render_view3d_pane_egui(
                                    ui,
                                    &slot.mesh,
                                    Some(&slot.segmentation.vertex_regions),
                                    Some(&slot.rim_gap_chart),
                                    slot.recommended_best_angle_deg,
                                    &mut self.view3d_state,
                                );
                            });
                            ui.add_space(4.0);
                        }
                        ui.allocate_ui(
                            egui::vec2(ui.available_width().max(240.0), geom.work_area_h),
                            |ui| {
                                render_sheet_pane_egui(
                                    ui,
                                    &slot.composed_plate,
                                    &slot.measurements,
                                    self.show_mm_grid,
                                );
                            },
                        );
                    });
                    if (self.view3d_state.active_section_angle_deg - prev_angle).abs() > 0.1 {
                        slot.rebuild_plate(
                            self.flip_exterior_right,
                            self.view3d_state.active_section_angle_deg,
                        );
                    }
                }
            });
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

    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.panel_fill = egui::Color32::from_rgb(GROUND.rgb[0], GROUND.rgb[1], GROUND.rgb[2]);
    style.visuals.window_fill =
        egui::Color32::from_rgb(GROUND.rgb[0], GROUND.rgb[1], GROUND.rgb[2]);
    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(
        1.0_f32,
        egui::Color32::from_rgb(EDGE.rgb[0], EDGE.rgb[1], EDGE.rgb[2]),
    );
    ctx.set_style(style);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    let mut verify_all = false;
    let mut screenshot_dir: Option<PathBuf> = None;

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
        Box::new(|cc| Ok(Box::new(StrataStudioApp::new(cc)))),
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
}
