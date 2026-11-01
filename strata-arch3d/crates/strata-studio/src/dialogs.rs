use eframe::egui;
use strata_core::strata_tokens::{
    metrics::{
        DIALOG_L_PRINT_H, DIALOG_L_VERIFY_H, DIALOG_L_W, DIALOG_M_SETTINGS_W, DIALOG_PROGRESS_W,
        DIALOG_S_ERROR_W, DIALOG_XL_H, DIALOG_XL_W,
    },
    tokens::{ACTION, DONE_INK, ERR_INK, GROUND, INK, MUTED, WASH},
};
use strata_core::ThreePartErrorInfo;
use strata_export::TiledPrintPlan;

/// STRATA 5종 표준 대화상자 열거형 (`DESIGN_SPEC.md` 1.3).
#[derive(Debug, Clone)]
pub enum ActiveStrataDialog {
    /// `S` (`460 × 280 px`): 3단 구조(`무엇이 안 됐나 / 왜 그런가 / 어떻게 하나`) 오류 대화상자
    Error3Part(ThreePartErrorInfo),
    /// `진행` (`480 × 240 px`): 4단계 일괄 처리 파이프라인 진행 모달
    BatchProgress {
        current_step_idx: usize,
        progress_pct: f32,
        status_note: String,
    },
    /// `M` (`520 × 440 px`): 환경 및 선 굵기·탁본 기본값 설정 모달
    SettingsM,
    /// `L` (`860 × 720 px`): 내보내기 전 `V1~V8` 품질·스케일·앵커 사전 검증 리포트 모달
    PreExportVerifyL { checks: Vec<(String, bool, String)> },
    /// `L` (`860 × 560 px`): 대형 유물 1:1 다중 시트 분할 인쇄 미리보기 모달
    TiledPrintPreviewL { plan: TiledPrintPlan },
    /// `XL` (`1280 × 860 px`): 다중 유물 보고서 합본 도판 편집기 모달
    MultiArtifactPlateComposerXL { slot_titles: Vec<String> },
}

impl ActiveStrataDialog {
    pub fn target_size_px(&self) -> (f32, f32) {
        match self {
            Self::Error3Part(_) => (DIALOG_S_ERROR_W as f32, 280.0),
            Self::BatchProgress { .. } => (DIALOG_PROGRESS_W as f32, 240.0),
            Self::SettingsM => (DIALOG_M_SETTINGS_W as f32, 440.0),
            Self::PreExportVerifyL { .. } => (DIALOG_L_W as f32, DIALOG_L_VERIFY_H as f32),
            Self::TiledPrintPreviewL { .. } => (DIALOG_L_W as f32, DIALOG_L_PRINT_H as f32),
            Self::MultiArtifactPlateComposerXL { .. } => (DIALOG_XL_W as f32, DIALOG_XL_H as f32),
        }
    }

    pub fn title_kr(&self) -> &'static str {
        match self {
            Self::Error3Part(_) => "오류 안내 (무엇이 안 됐나 · 왜 그런가 · 어떻게 하나)",
            Self::BatchProgress { .. } => "일괄 실측 · 단면 · 탁본 처리 진행",
            Self::SettingsM => "제도 선 굵기 · 탁본 · 축척 환경설정 (M)",
            Self::PreExportVerifyL { .. } => "내보내기 전 도판·스케일·앵커 정밀 검증 리포트 (L)",
            Self::TiledPrintPreviewL { .. } => "대형 유물 1:1 다중 시트 분할 인쇄 설정 (L)",
            Self::MultiArtifactPlateComposerXL { .. } => "보고서 다중 유물 합본 도판 편집기 (XL)",
        }
    }
}

/// 현재 열린 표준 모달 대화상자를 렌더링하고, 닫기 요청 시 `true`를 반환한다.
pub fn show_active_dialog(ctx: &egui::Context, dialog: &mut Option<ActiveStrataDialog>) -> bool {
    let Some(active) = dialog.as_ref() else {
        return false;
    };
    let (w, h) = active.target_size_px();
    let title = active.title_kr();
    let mut close_requested = false;

    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .fixed_size(egui::vec2(w, h))
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            let ink = egui::Color32::from_rgb(INK.rgb[0], INK.rgb[1], INK.rgb[2]);
            let muted = egui::Color32::from_rgb(MUTED.rgb[0], MUTED.rgb[1], MUTED.rgb[2]);
            let crimson = egui::Color32::from_rgb(ERR_INK.rgb[0], ERR_INK.rgb[1], ERR_INK.rgb[2]);
            let jade = egui::Color32::from_rgb(DONE_INK.rgb[0], DONE_INK.rgb[1], DONE_INK.rgb[2]);
            let terra = egui::Color32::from_rgb(ACTION.rgb[0], ACTION.rgb[1], ACTION.rgb[2]);
            let wash = egui::Color32::from_rgb(WASH.rgb[0], WASH.rgb[1], WASH.rgb[2]);
            let _ = GROUND;

            match active {
                ActiveStrataDialog::Error3Part(info) => {
                    ui.add_space(6.0);
                    egui::Frame::NONE
                        .fill(wash)
                        .inner_margin(10.0)
                        .show(ui, |ui| {
                            ui.colored_label(
                                crimson,
                                format!("① 무엇이 안 됐나: {}", info.what_failed),
                            );
                            ui.add_space(6.0);
                            ui.colored_label(
                                muted,
                                format!("② 왜 그런가: {}", info.why_it_happened),
                            );
                            ui.add_space(6.0);
                            ui.colored_label(jade, format!("③ 어떻게 하나: {}", info.how_to_fix));
                        });
                }
                ActiveStrataDialog::BatchProgress {
                    current_step_idx,
                    progress_pct,
                    status_note,
                } => {
                    let steps = [
                        "1. 3D 메쉬 위상 점검 및 정위 행렬 적용",
                        "2. 고정 절단틀(World-Locked) 수직·횡단면 추출",
                        "3. 정밀 탁본(법선미분 / 16-MSII) 래스터 합성",
                        "4. 1mm=5px SVG 제도선 및 분할 PDF 패키징",
                    ];
                    for (idx, s) in steps.iter().enumerate() {
                        let col = if idx < *current_step_idx {
                            jade
                        } else if idx == *current_step_idx {
                            terra
                        } else {
                            muted
                        };
                        let prefix = if idx < *current_step_idx {
                            "✓"
                        } else if idx == *current_step_idx {
                            "▶"
                        } else {
                            "·"
                        };
                        ui.colored_label(col, format!("{prefix} {s}"));
                    }
                    ui.add_space(8.0);
                    ui.add(egui::ProgressBar::new(*progress_pct / 100.0).show_percentage());
                    ui.colored_label(ink, status_note);
                }
                ActiveStrataDialog::SettingsM => {
                    ui.colored_label(ink, "• 단면 외곽 실선 두께: 0.35 mm (구슬 확대 시 0.25 mm)");
                    ui.colored_label(ink, "• 내부 박리 능선 / 보조선 두께: 0.20 mm");
                    ui.colored_label(
                        ink,
                        "• 추정 복원선 / 천공 내벽 파선: 0.25 mm (dash 2.0, 1.5 mm)",
                    );
                    ui.colored_label(
                        ink,
                        "• SVG / PNG 출력 스케일 고정: 1 mm = 5.0 px (600 DPI 급)",
                    );
                    ui.colored_label(ink, "• 한지(Hanji #FAF9F5) 손탁본 미세 섬유 결: 사용");
                }
                ActiveStrataDialog::PreExportVerifyL { checks } => {
                    ui.colored_label(
                        ink,
                        "출판 및 일러스트레이터(AI) 납품 전 8대 자동 검증(V1~V8) 결과:",
                    );
                    ui.add_space(6.0);
                    for (code, pass, detail) in checks {
                        let col = if *pass { jade } else { crimson };
                        let badge = if *pass {
                            "[통과 ✓]"
                        } else {
                            "[점검 필요 !]"
                        };
                        ui.colored_label(col, format!("{badge} {code}: {detail}"));
                    }
                }
                ActiveStrataDialog::TiledPrintPreviewL { plan } => {
                    ui.colored_label(
                        ink,
                        format!(
                            "• 1:1 실측 분할 매수: 총 {}매 ({}열 × {}행)",
                            plan.total_pages, plan.cols, plan.rows
                        ),
                    );
                    ui.colored_label(
                        ink,
                        format!(
                            "• 인접 시트 조립 여유분(Overlap): {:.1} mm + 재단 파선 + ⊕ 정합 마크",
                            plan.overlap_mm
                        ),
                    );
                    ui.colored_label(
                        terra,
                        "• 각 시트 하단 5cm(50.0mm) 물리 자 대조용 검증 막대 자동 인쇄",
                    );
                }
                ActiveStrataDialog::MultiArtifactPlateComposerXL { slot_titles } => {
                    ui.colored_label(
                        ink,
                        "발굴조사 보고서 다중 유물 합본 도판 (A4/A3 통일 축척 자동 정렬):",
                    );
                    ui.add_space(6.0);
                    for (idx, t) in slot_titles.iter().enumerate() {
                        ui.colored_label(ink, format!("  도면 {} — {}", idx + 1, t));
                    }
                }
            }

            ui.add_space(12.0);
            if ui.button("확인 / 닫기 (Esc)").clicked() {
                close_requested = true;
            }
        });

    if close_requested {
        *dialog = None;
    }
    close_requested
}
