use arch3d_core::strata_tokens::metrics::{
    CTX_BAR_HEIGHT, RIBBON_HEIGHT, RIBBON_TILE_SIZE, SHEET_DEFAULT_RATIO, SLOT_LIST_WIDTH,
    STATUS_BAR_HEIGHT, TAB_BAR_HEIGHT, VIEW3D_DEFAULT_RATIO,
};
use serde::{Deserialize, Serialize};

/// 4단계 고고학 실측 워크플로 탭 (`UI_RULES.md` R1 & `DESIGN_SPEC.md` 1.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkflowTab {
    Step1LoadAndClassify,
    Step2OrientAndRefit,
    Step3SectionAndRubbing,
    Step4PlateAndExport,
}

impl WorkflowTab {
    pub const ALL: [Self; 4] = [
        Self::Step1LoadAndClassify,
        Self::Step2OrientAndRefit,
        Self::Step3SectionAndRubbing,
        Self::Step4PlateAndExport,
    ];

    pub fn label_kr(self) -> &'static str {
        match self {
            Self::Step1LoadAndClassify => "① 유물 불러오기 · 분류",
            Self::Step2OrientAndRefit => "② 정위 · 중심축 · 접합",
            Self::Step3SectionAndRubbing => "③ 단면 · 정밀 탁본 · 전개",
            Self::Step4PlateAndExport => "④ 도판 · 검증 · 내보내기",
        }
    }
}

/// `TileRibbon` (`98px`) 내 개별 `50 × 50 px` 타일 정의 (`UI_RULES.md` R2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RibbonTileSpec {
    pub group_title: &'static str,
    pub code: &'static str,
    pub label_kr: &'static str,
    pub shortcut: &'static str,
}

impl WorkflowTab {
    pub fn ribbon_tiles(self) -> &'static [RibbonTileSpec] {
        match self {
            Self::Step1LoadAndClassify => &[
                RibbonTileSpec {
                    group_title: "파일 입출력",
                    code: "OPEN",
                    label_kr: "유물 열기",
                    shortcut: "Ctrl+O",
                },
                RibbonTileSpec {
                    group_title: "파일 입출력",
                    code: "AP2",
                    label_kr: "AP2 가져오기",
                    shortcut: "Ctrl+I",
                },
                RibbonTileSpec {
                    group_title: "파일 입출력",
                    code: "SAVE",
                    label_kr: "작업 저장",
                    shortcut: "Ctrl+S",
                },
                RibbonTileSpec {
                    group_title: "유물군 프리셋",
                    code: "CAT_VESSEL",
                    label_kr: "완형 토기",
                    shortcut: "1",
                },
                RibbonTileSpec {
                    group_title: "유물군 프리셋",
                    code: "CAT_SHERD",
                    label_kr: "편조각",
                    shortcut: "2",
                },
                RibbonTileSpec {
                    group_title: "유물군 프리셋",
                    code: "CAT_BEAD",
                    label_kr: "구슬·옥류",
                    shortcut: "3",
                },
                RibbonTileSpec {
                    group_title: "유물군 프리셋",
                    code: "CAT_TILE",
                    label_kr: "기와·석기",
                    shortcut: "4",
                },
            ],
            Self::Step2OrientAndRefit => &[
                RibbonTileSpec {
                    group_title: "정위 연산",
                    code: "SEG_3WAY",
                    label_kr: "3영역 분리",
                    shortcut: "S",
                },
                RibbonTileSpec {
                    group_title: "정위 연산",
                    code: "FIT_AXIS",
                    label_kr: "회전축 피팅",
                    shortcut: "A",
                },
                RibbonTileSpec {
                    group_title: "정위 연산",
                    code: "BEST_ANGLE",
                    label_kr: "최적단면각",
                    shortcut: "G",
                },
                RibbonTileSpec {
                    group_title: "특수 정위",
                    code: "BEAD_BORE",
                    label_kr: "천공축(Ⅰ~Ⅱ)",
                    shortcut: "B",
                },
                RibbonTileSpec {
                    group_title: "특수 정위",
                    code: "REFIT_BAISER",
                    label_kr: "가상 접합",
                    shortcut: "J",
                },
                RibbonTileSpec {
                    group_title: "확정",
                    code: "LOCK_POSE",
                    label_kr: "정위 확정",
                    shortcut: "Enter",
                },
            ],
            Self::Step3SectionAndRubbing => &[
                RibbonTileSpec {
                    group_title: "단면 추출",
                    code: "SLICE_V",
                    label_kr: "수직 단면",
                    shortcut: "V",
                },
                RibbonTileSpec {
                    group_title: "단면 추출",
                    code: "SLICE_H",
                    label_kr: "다단 횡단면",
                    shortcut: "H",
                },
                RibbonTileSpec {
                    group_title: "단면 추출",
                    code: "OCC_DASH",
                    label_kr: "비가시 파선",
                    shortcut: "D",
                },
                RibbonTileSpec {
                    group_title: "정밀 탁본",
                    code: "RUB_HAND",
                    label_kr: "한지 손탁본",
                    shortcut: "F1",
                },
                RibbonTileSpec {
                    group_title: "정밀 탁본",
                    code: "RUB_MSII",
                    label_kr: "16-MSII 탁본",
                    shortcut: "F2",
                },
                RibbonTileSpec {
                    group_title: "정밀 탁본",
                    code: "UNROLL",
                    label_kr: "원통 전개",
                    shortcut: "U",
                },
                RibbonTileSpec {
                    group_title: "정밀 탁본",
                    code: "HEMP_FFT",
                    label_kr: "포목흔 계측",
                    shortcut: "M",
                },
            ],
            Self::Step4PlateAndExport => &[
                RibbonTileSpec {
                    group_title: "도판 제어",
                    code: "FLIP_R",
                    label_kr: "좌우반전(R)",
                    shortcut: "R",
                },
                RibbonTileSpec {
                    group_title: "도판 제어",
                    code: "FULL_TAB",
                    label_kr: "100% 도판",
                    shortcut: "Tab",
                },
                RibbonTileSpec {
                    group_title: "검증 및 조판",
                    code: "VERIFY_L",
                    label_kr: "검증 보고(L)",
                    shortcut: "F5",
                },
                RibbonTileSpec {
                    group_title: "검증 및 조판",
                    code: "MULTI_XL",
                    label_kr: "합본 편집(XL)",
                    shortcut: "F6",
                },
                RibbonTileSpec {
                    group_title: "내보내기",
                    code: "EXP_SVG",
                    label_kr: "SVG 제도선",
                    shortcut: "Ctrl+E",
                },
                RibbonTileSpec {
                    group_title: "내보내기",
                    code: "EXP_PDF",
                    label_kr: "분할 PDF(L)",
                    shortcut: "Ctrl+P",
                },
                RibbonTileSpec {
                    group_title: "내보내기",
                    code: "EXP_RET",
                    label_kr: "ret 패키지",
                    shortcut: "Ctrl+R",
                },
            ],
        }
    }
}

/// R1~R8 5단 고정 레이아웃 픽셀 기하 계산 결과 (`V8` 검증용).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct R1FiveTierGeometry {
    pub window_w: f32,
    pub window_h: f32,
    pub tab_bar_h: f32,
    pub tile_ribbon_h: f32,
    pub tile_size: f32,
    pub ctx_bar_h: f32,
    pub status_bar_h: f32,
    pub work_area_h: f32,
    pub slot_list_w: f32,
    pub view3d_w: f32,
    pub sheet_pane_w: f32,
    pub sheet_full_tab_mode: bool,
}

impl R1FiveTierGeometry {
    pub fn compute(window_w: f32, window_h: f32, sheet_full_tab_mode: bool) -> Self {
        let tab_bar_h = TAB_BAR_HEIGHT as f32;
        let tile_ribbon_h = RIBBON_HEIGHT as f32;
        let tile_size = RIBBON_TILE_SIZE as f32;
        let ctx_bar_h = CTX_BAR_HEIGHT as f32;
        let status_bar_h = STATUS_BAR_HEIGHT as f32;
        let slot_list_w = SLOT_LIST_WIDTH as f32;

        let top_chrome = tab_bar_h + tile_ribbon_h + ctx_bar_h;
        let work_area_h = (window_h - top_chrome - status_bar_h).max(200.0);
        let remaining_w = (window_w - slot_list_w).max(400.0);

        let (view3d_w, sheet_pane_w) = if sheet_full_tab_mode {
            (0.0, remaining_w)
        } else {
            let v3d = (remaining_w * VIEW3D_DEFAULT_RATIO).round();
            let sheet = remaining_w - v3d;
            (v3d, sheet)
        };

        Self {
            window_w,
            window_h,
            tab_bar_h,
            tile_ribbon_h,
            tile_size,
            ctx_bar_h,
            status_bar_h,
            work_area_h,
            slot_list_w,
            view3d_w,
            sheet_pane_w,
            sheet_full_tab_mode,
        }
    }

    pub fn verify_r1_contract(&self) -> bool {
        (self.tab_bar_h - 34.0).abs() < 1e-3
            && (self.tile_ribbon_h - 98.0).abs() < 1e-3
            && (self.tile_size - 50.0).abs() < 1e-3
            && (self.ctx_bar_h - 36.0).abs() < 1e-3
            && (self.status_bar_h - 30.0).abs() < 1e-3
            && (self.slot_list_w - 176.0).abs() < 1e-3
            && (self.sheet_full_tab_mode
                || ((self.sheet_pane_w / (self.view3d_w + self.sheet_pane_w)) - SHEET_DEFAULT_RATIO)
                    .abs()
                    < 0.01)
    }
}
