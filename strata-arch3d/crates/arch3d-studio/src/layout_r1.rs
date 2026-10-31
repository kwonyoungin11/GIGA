use arch3d_core::strata_tokens::metrics::{
    CTX_BAR_HEIGHT, RIBBON_HEIGHT, RIBBON_TILE_SIZE, SHEET_DEFAULT_RATIO, SLOT_LIST_WIDTH,
    STATUS_BAR_HEIGHT, TAB_BAR_HEIGHT, VIEW3D_DEFAULT_RATIO,
};
use eframe::egui;
use serde::{Deserialize, Serialize};

/// 4단계 고고학 실측 워크플로 탭 (`UI_RULES.md` R1 & `DESIGN_SPEC.md` 3.3).
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
            Self::Step2OrientAndRefit => "② 정위 · 중심축 · 절개단면",
            Self::Step3SectionAndRubbing => "③ 단면 · 정밀 탁본 · 전개",
            Self::Step4PlateAndExport => "④ 도판 · 검증 · 내보내기",
        }
    }
}

/// `50 × 50 px` 리본 타일 내부에 `1.75px` 흑연(`#141413`) + 테라코타(`#B5573A`)로 그리는 전용 벡터 아이콘 종류 (`DESIGN_SPEC.md` 3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TileIconKind {
    FolderOpen,
    Ap2Project,
    SaveDisk,
    VesselComplete,
    PotsherdFragment,
    BeadJade,
    RoofTileLithic,
    Fracture3Way,
    AxisSolver,
    BestSectionAngle,
    BeadPerforation,
    VirtualRefit,
    LockPoseAction,
    SliceVertical,
    SliceHorizontal,
    OccludedDashed,
    RubbingHandprint,
    RubbingMsii,
    CylinderUnroll,
    HempFftCount,
    FlipLeftRight,
    FullSheetTab,
    VerifyReport,
    MultiPlateComposer,
    ExportSvg,
    ExportTiledPdf,
    ExportRetPackage,
}

/// `TileRibbon` (`98px`) 내 개별 `50 × 50 px` 타일 정의 (`UI_RULES.md` R2 & `DESIGN_SPEC.md` 3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RibbonTileSpec {
    pub group_title: &'static str,
    pub code: &'static str,
    pub label_kr: &'static str,
    pub shortcut: &'static str,
    pub icon_kind: TileIconKind,
    pub is_primary_action: bool,
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
                    icon_kind: TileIconKind::FolderOpen,
                    is_primary_action: false,
                },
                RibbonTileSpec {
                    group_title: "파일 입출력",
                    code: "AP2",
                    label_kr: "AP2 정합",
                    shortcut: "Ctrl+I",
                    icon_kind: TileIconKind::Ap2Project,
                    is_primary_action: false,
                },
                RibbonTileSpec {
                    group_title: "파일 입출력",
                    code: "SAVE",
                    label_kr: "작업 저장",
                    shortcut: "Ctrl+S",
                    icon_kind: TileIconKind::SaveDisk,
                    is_primary_action: false,
                },
                RibbonTileSpec {
                    group_title: "유물군 도법 전환",
                    code: "CAT_VESSEL",
                    label_kr: "완형 토기",
                    shortcut: "1",
                    icon_kind: TileIconKind::VesselComplete,
                    is_primary_action: false,
                },
                RibbonTileSpec {
                    group_title: "유물군 도법 전환",
                    code: "CAT_SHERD",
                    label_kr: "편조각",
                    shortcut: "2",
                    icon_kind: TileIconKind::PotsherdFragment,
                    is_primary_action: false,
                },
                RibbonTileSpec {
                    group_title: "유물군 도법 전환",
                    code: "CAT_BEAD",
                    label_kr: "구슬·옥류",
                    shortcut: "3",
                    icon_kind: TileIconKind::BeadJade,
                    is_primary_action: false,
                },
                RibbonTileSpec {
                    group_title: "유물군 도법 전환",
                    code: "CAT_TILE",
                    label_kr: "기와·석기",
                    shortcut: "4",
                    icon_kind: TileIconKind::RoofTileLithic,
                    is_primary_action: false,
                },
            ],
            Self::Step2OrientAndRefit => &[
                RibbonTileSpec {
                    group_title: "파단면 · 축 추론",
                    code: "SEG_3WAY",
                    label_kr: "3분류 파단면",
                    shortcut: "S",
                    icon_kind: TileIconKind::Fracture3Way,
                    is_primary_action: false,
                },
                RibbonTileSpec {
                    group_title: "파단면 · 축 추론",
                    code: "FIT_AXIS",
                    label_kr: "회전축 맞춤",
                    shortcut: "A",
                    icon_kind: TileIconKind::AxisSolver,
                    is_primary_action: false,
                },
                RibbonTileSpec {
                    group_title: "파단면 · 축 추론",
                    code: "BEST_ANGLE",
                    label_kr: "★추천단면각",
                    shortcut: "G",
                    icon_kind: TileIconKind::BestSectionAngle,
                    is_primary_action: false,
                },
                RibbonTileSpec {
                    group_title: "특수 정위 · 접합",
                    code: "BEAD_BORE",
                    label_kr: "X-Ray 천공",
                    shortcut: "B",
                    icon_kind: TileIconKind::BeadPerforation,
                    is_primary_action: false,
                },
                RibbonTileSpec {
                    group_title: "특수 정위 · 접합",
                    code: "REFIT_BAISER",
                    label_kr: "가상 접합",
                    shortcut: "J",
                    icon_kind: TileIconKind::VirtualRefit,
                    is_primary_action: false,
                },
                RibbonTileSpec {
                    group_title: "자세 확정",
                    code: "LOCK_POSE",
                    label_kr: "이자세 확정",
                    shortcut: "Enter",
                    icon_kind: TileIconKind::LockPoseAction,
                    is_primary_action: true,
                },
            ],
            Self::Step3SectionAndRubbing => &[
                RibbonTileSpec {
                    group_title: "고정절단틀 단면",
                    code: "SLICE_V",
                    label_kr: "종단면 절개",
                    shortcut: "V",
                    icon_kind: TileIconKind::SliceVertical,
                    is_primary_action: false,
                },
                RibbonTileSpec {
                    group_title: "고정절단틀 단면",
                    code: "SLICE_H",
                    label_kr: "횡단면(TopZ)",
                    shortcut: "H",
                    icon_kind: TileIconKind::SliceHorizontal,
                    is_primary_action: false,
                },
                RibbonTileSpec {
                    group_title: "고정절단틀 단면",
                    code: "OCC_DASH",
                    label_kr: "미스캔 파선",
                    shortcut: "D",
                    icon_kind: TileIconKind::OccludedDashed,
                    is_primary_action: false,
                },
                RibbonTileSpec {
                    group_title: "정밀 탁본 · 전개",
                    code: "RUB_HAND",
                    label_kr: "한지 정밀탁본",
                    shortcut: "F1",
                    icon_kind: TileIconKind::RubbingHandprint,
                    is_primary_action: false,
                },
                RibbonTileSpec {
                    group_title: "정밀 탁본 · 전개",
                    code: "RUB_MSII",
                    label_kr: "다중곡률 탁본",
                    shortcut: "F2",
                    icon_kind: TileIconKind::RubbingMsii,
                    is_primary_action: false,
                },
                RibbonTileSpec {
                    group_title: "정밀 탁본 · 전개",
                    code: "UNROLL",
                    label_kr: "360° 전개도",
                    shortcut: "U",
                    icon_kind: TileIconKind::CylinderUnroll,
                    is_primary_action: false,
                },
                RibbonTileSpec {
                    group_title: "정밀 탁본 · 전개",
                    code: "HEMP_FFT",
                    label_kr: "포목흔 올수",
                    shortcut: "M",
                    icon_kind: TileIconKind::HempFftCount,
                    is_primary_action: false,
                },
            ],
            Self::Step4PlateAndExport => &[
                RibbonTileSpec {
                    group_title: "도판 표시 제어",
                    code: "FLIP_R",
                    label_kr: "좌우반전(R)",
                    shortcut: "R",
                    icon_kind: TileIconKind::FlipLeftRight,
                    is_primary_action: false,
                },
                RibbonTileSpec {
                    group_title: "도판 표시 제어",
                    code: "FULL_TAB",
                    label_kr: "100% 도판",
                    shortcut: "Tab",
                    icon_kind: TileIconKind::FullSheetTab,
                    is_primary_action: false,
                },
                RibbonTileSpec {
                    group_title: "검수 · 편집판",
                    code: "VERIFY_L",
                    label_kr: "실측 검수(L)",
                    shortcut: "F5",
                    icon_kind: TileIconKind::VerifyReport,
                    is_primary_action: false,
                },
                RibbonTileSpec {
                    group_title: "검수 · 편집판",
                    code: "MULTI_XL",
                    label_kr: "합본 편집(XL)",
                    shortcut: "F6",
                    icon_kind: TileIconKind::MultiPlateComposer,
                    is_primary_action: false,
                },
                RibbonTileSpec {
                    group_title: "출판 · 내보내기",
                    code: "EXP_SVG",
                    label_kr: "SVG 제도선",
                    shortcut: "Ctrl+E",
                    icon_kind: TileIconKind::ExportSvg,
                    is_primary_action: false,
                },
                RibbonTileSpec {
                    group_title: "출판 · 내보내기",
                    code: "EXP_PDF",
                    label_kr: "분할인쇄 PDF",
                    shortcut: "Ctrl+P",
                    icon_kind: TileIconKind::ExportTiledPdf,
                    is_primary_action: false,
                },
                RibbonTileSpec {
                    group_title: "출판 · 내보내기",
                    code: "EXP_RET",
                    label_kr: "도면·내보내기",
                    shortcut: "Ctrl+R",
                    icon_kind: TileIconKind::ExportRetPackage,
                    is_primary_action: true,
                },
            ],
        }
    }
}

/// `50 × 50 px` 리본 타일 내부에 `1.75px` 흑연(`#141413`) 주 선과 `#B5573A` 테라코타 강조 포인트로 전용 벡터 아이콘을 그린다.
pub fn draw_ribbon_tile_vector_icon(
    painter: &egui::Painter,
    tile_box: egui::Rect,
    icon: TileIconKind,
    primary_col: egui::Color32,
    accent_col: egui::Color32,
) {
    let c = egui::pos2(tile_box.center().x, tile_box.center().y - 1.0);
    let s_main = egui::Stroke::new(1.75_f32, primary_col);
    let s_acc = egui::Stroke::new(1.75_f32, accent_col);
    let s_thin = egui::Stroke::new(1.25_f32, primary_col);

    match icon {
        TileIconKind::FolderOpen => {
            let r = egui::Rect::from_center_size(egui::pos2(c.x, c.y + 2.0), egui::vec2(24.0, 16.0));
            painter.rect_stroke(r, 2.0, s_main, egui::StrokeKind::Inside);
            painter.line_segment(
                [
                    egui::pos2(r.left() + 2.0, r.top() - 3.0),
                    egui::pos2(r.left() + 10.0, r.top() - 3.0),
                ],
                s_main,
            );
            // 테라코타 위쪽 화살표
            painter.line_segment([egui::pos2(c.x, c.y + 6.0), egui::pos2(c.x, c.y - 4.0)], s_acc);
            painter.line_segment(
                [egui::pos2(c.x - 4.0, c.y), egui::pos2(c.x, c.y - 4.0)],
                s_acc,
            );
            painter.line_segment(
                [egui::pos2(c.x + 4.0, c.y), egui::pos2(c.x, c.y - 4.0)],
                s_acc,
            );
        }
        TileIconKind::Ap2Project => {
            let r = egui::Rect::from_center_size(c, egui::vec2(22.0, 22.0));
            painter.rect_stroke(r, 3.0, s_main, egui::StrokeKind::Inside);
            painter.line_segment([egui::pos2(c.x, r.top() + 3.0), egui::pos2(c.x, r.bottom() - 3.0)], s_acc);
            painter.line_segment([egui::pos2(r.left() + 3.0, c.y), egui::pos2(r.right() - 3.0, c.y)], s_acc);
            painter.circle_filled(c, 2.5, accent_col);
        }
        TileIconKind::SaveDisk => {
            let r = egui::Rect::from_center_size(c, egui::vec2(22.0, 22.0));
            painter.rect_stroke(r, 2.5, s_main, egui::StrokeKind::Inside);
            let top_slot = egui::Rect::from_min_max(
                egui::pos2(r.left() + 5.0, r.top() + 2.0),
                egui::pos2(r.right() - 5.0, r.top() + 9.0),
            );
            painter.rect_stroke(top_slot, 1.0, s_thin, egui::StrokeKind::Inside);
            painter.circle_filled(egui::pos2(c.x, r.bottom() - 6.0), 3.2, accent_col);
        }
        TileIconKind::VesselComplete => {
            // 완형 토기 좌우 대칭 윤곽 + 중심축(테라코타)
            painter.line_segment([egui::pos2(c.x, c.y - 12.0), egui::pos2(c.x, c.y + 12.0)], s_acc);
            painter.line_segment([egui::pos2(c.x - 11.0, c.y - 9.0), egui::pos2(c.x + 11.0, c.y - 9.0)], s_main);
            painter.line_segment([egui::pos2(c.x - 11.0, c.y - 9.0), egui::pos2(c.x - 8.0, c.y + 5.0)], s_main);
            painter.line_segment([egui::pos2(c.x - 8.0, c.y + 5.0), egui::pos2(c.x - 5.0, c.y + 10.0)], s_main);
            painter.line_segment([egui::pos2(c.x + 11.0, c.y - 9.0), egui::pos2(c.x + 8.0, c.y + 5.0)], s_main);
            painter.line_segment([egui::pos2(c.x + 8.0, c.y + 5.0), egui::pos2(c.x + 5.0, c.y + 10.0)], s_main);
            painter.line_segment([egui::pos2(c.x - 6.0, c.y + 10.0), egui::pos2(c.x + 6.0, c.y + 10.0)], s_main);
        }
        TileIconKind::PotsherdFragment => {
            // 비대칭 편조각 다각형 + 파단 경계(테라코타)
            let pts = [
                egui::pos2(c.x - 9.0, c.y - 10.0),
                egui::pos2(c.x + 8.0, c.y - 8.0),
                egui::pos2(c.x + 11.0, c.y + 3.0),
                egui::pos2(c.x + 2.0, c.y + 11.0),
                egui::pos2(c.x - 10.0, c.y + 6.0),
                egui::pos2(c.x - 9.0, c.y - 10.0),
            ];
            for w in pts.windows(2) {
                painter.line_segment([w[0], w[1]], s_main);
            }
            painter.line_segment(
                [egui::pos2(c.x + 8.0, c.y - 8.0), egui::pos2(c.x + 2.0, c.y + 11.0)],
                s_acc,
            );
        }
        TileIconKind::BeadJade => {
            // 구슬 외경 원 + 관통공 내경 동심원(테라코타)
            painter.circle_stroke(c, 11.0, s_main);
            painter.circle_stroke(c, 4.2, s_acc);
            painter.circle_filled(c, 1.5, accent_col);
        }
        TileIconKind::RoofTileLithic => {
            // 기와 호형 단면 + 타날 격자선
            let mut arc_pts = Vec::new();
            for i in 0..=12 {
                let t = -0.85 + (i as f32 / 12.0) * 1.7;
                arc_pts.push(egui::pos2(c.x + t.sin() * 13.0, c.y + 6.0 - t.cos() * 12.0));
            }
            painter.add(egui::Shape::line(arc_pts, s_main));
            painter.line_segment([egui::pos2(c.x - 7.0, c.y + 5.0), egui::pos2(c.x + 7.0, c.y + 5.0)], s_acc);
            painter.line_segment([egui::pos2(c.x - 5.0, c.y + 9.0), egui::pos2(c.x + 5.0, c.y + 9.0)], s_thin);
        }
        TileIconKind::Fracture3Way => {
            // 내벽/외벽/파단면 3영역 분할 아이콘
            let r = egui::Rect::from_center_size(c, egui::vec2(22.0, 20.0));
            painter.rect_stroke(r, 2.0, s_main, egui::StrokeKind::Inside);
            painter.line_segment(
                [egui::pos2(r.left() + 3.0, c.y - 3.0), egui::pos2(c.x + 2.0, c.y + 2.0)],
                s_acc,
            );
            painter.line_segment(
                [egui::pos2(c.x + 2.0, c.y + 2.0), egui::pos2(r.right() - 3.0, c.y - 4.0)],
                s_acc,
            );
            painter.line_segment([egui::pos2(c.x + 2.0, c.y + 2.0), egui::pos2(c.x, r.bottom() - 2.0)], s_main);
        }
        TileIconKind::AxisSolver => {
            // 수평 바닥 기준선 + 수직 회전 중심축 화살표
            painter.line_segment([egui::pos2(c.x - 11.0, c.y + 10.0), egui::pos2(c.x + 11.0, c.y + 10.0)], s_main);
            painter.line_segment([egui::pos2(c.x, c.y + 10.0), egui::pos2(c.x, c.y - 11.0)], s_acc);
            painter.line_segment([egui::pos2(c.x - 4.0, c.y - 7.0), egui::pos2(c.x, c.y - 11.0)], s_acc);
            painter.line_segment([egui::pos2(c.x + 4.0, c.y - 7.0), egui::pos2(c.x, c.y - 11.0)], s_acc);
            painter.circle_stroke(egui::pos2(c.x, c.y + 1.0), 6.5, s_thin);
        }
        TileIconKind::BestSectionAngle => {
            // 동심원 타깃 + 최적 각도 바늘(테라코타)
            painter.circle_stroke(c, 10.5, s_main);
            painter.line_segment([c, egui::pos2(c.x + 8.5, c.y - 6.5)], s_acc);
            painter.circle_filled(egui::pos2(c.x + 8.5, c.y - 6.5), 2.5, accent_col);
        }
        TileIconKind::BeadPerforation => {
            // 모래시계형 양방향 맞조르기(IIb) 천공 단면 아이콘
            let r = egui::Rect::from_center_size(c, egui::vec2(18.0, 22.0));
            painter.rect_stroke(r, 2.0, s_main, egui::StrokeKind::Inside);
            painter.line_segment([egui::pos2(c.x - 5.0, r.top()), egui::pos2(c.x - 1.5, c.y)], s_acc);
            painter.line_segment([egui::pos2(c.x - 1.5, c.y), egui::pos2(c.x - 5.0, r.bottom())], s_acc);
            painter.line_segment([egui::pos2(c.x + 5.0, r.top()), egui::pos2(c.x + 1.5, c.y)], s_acc);
            painter.line_segment([egui::pos2(c.x + 1.5, c.y), egui::pos2(c.x + 5.0, r.bottom())], s_acc);
        }
        TileIconKind::VirtualRefit => {
            // 두 파편 맞물림 화살표
            painter.rect_stroke(
                egui::Rect::from_min_size(egui::pos2(c.x - 11.0, c.y - 8.0), egui::vec2(9.0, 16.0)),
                1.5,
                s_main,
                egui::StrokeKind::Inside,
            );
            painter.rect_stroke(
                egui::Rect::from_min_size(egui::pos2(c.x + 2.0, c.y - 8.0), egui::vec2(9.0, 16.0)),
                1.5,
                s_main,
                egui::StrokeKind::Inside,
            );
            painter.line_segment([egui::pos2(c.x - 3.0, c.y), egui::pos2(c.x + 3.0, c.y)], s_acc);
        }
        TileIconKind::LockPoseAction => {
            // 주 행동 타일: 체크 마크 + 기준틀 고정
            painter.circle_stroke(c, 10.5, s_main);
            painter.line_segment([egui::pos2(c.x - 5.0, c.y), egui::pos2(c.x - 1.0, c.y + 4.5)], s_acc);
            painter.line_segment([egui::pos2(c.x - 1.0, c.y + 4.5), egui::pos2(c.x + 6.0, c.y - 4.5)], s_acc);
        }
        TileIconKind::SliceVertical => {
            // 고정 절단틀 사각형 + 수직 절개 단면선
            let r = egui::Rect::from_center_size(c, egui::vec2(22.0, 18.0));
            painter.rect_stroke(r, 1.5, s_thin, egui::StrokeKind::Inside);
            painter.line_segment([egui::pos2(c.x, r.top() - 3.0), egui::pos2(c.x, r.bottom() + 3.0)], s_acc);
            painter.line_segment([egui::pos2(c.x - 6.0, c.y - 6.0), egui::pos2(c.x - 4.0, c.y + 6.0)], s_main);
        }
        TileIconKind::SliceHorizontal => {
            // 수평 횡단면(Top Cut Z)
            painter.circle_stroke(c, 9.5, s_main);
            painter.line_segment([egui::pos2(c.x - 12.0, c.y), egui::pos2(c.x + 12.0, c.y)], s_acc);
        }
        TileIconKind::OccludedDashed => {
            // 미스캔 내벽 파선 보간
            painter.line_segment([egui::pos2(c.x - 7.0, c.y - 10.0), egui::pos2(c.x - 5.0, c.y + 10.0)], s_main);
            for dy in [-8.0_f32, -2.0, 4.0] {
                painter.line_segment(
                    [egui::pos2(c.x + 4.0, c.y + dy), egui::pos2(c.x + 4.0, c.y + dy + 3.5)],
                    s_acc,
                );
            }
        }
        TileIconKind::RubbingHandprint => {
            // 한지 정밀 탁본 먹물 농담 스트로크
            let r = egui::Rect::from_center_size(c, egui::vec2(22.0, 20.0));
            painter.rect_stroke(r, 2.0, s_main, egui::StrokeKind::Inside);
            for i in -2..=2 {
                let y = c.y + (i as f32) * 3.5;
                painter.line_segment([egui::pos2(c.x - 7.0, y), egui::pos2(c.x + 7.0, y)], s_thin);
            }
            painter.circle_filled(egui::pos2(r.right() - 4.0, r.top() + 4.0), 2.5, accent_col);
        }
        TileIconKind::RubbingMsii => {
            // 다중 스케일 구형 적분 곡률(MSII) 동심 반경
            painter.circle_stroke(c, 10.5, s_main);
            painter.circle_stroke(c, 6.5, s_thin);
            painter.circle_stroke(c, 3.0, s_acc);
        }
        TileIconKind::CylinderUnroll => {
            // 360° 원통 전개도 아이콘
            painter.rect_stroke(
                egui::Rect::from_center_size(egui::pos2(c.x, c.y + 2.0), egui::vec2(22.0, 14.0)),
                1.5,
                s_main,
                egui::StrokeKind::Inside,
            );
            painter.line_segment([egui::pos2(c.x - 9.0, c.y - 8.0), egui::pos2(c.x + 9.0, c.y - 8.0)], s_acc);
        }
        TileIconKind::HempFftCount => {
            // 1×1 cm 포목흔 격자 올수 계측
            let r = egui::Rect::from_center_size(c, egui::vec2(20.0, 20.0));
            painter.rect_stroke(r, 1.5, s_acc, egui::StrokeKind::Inside);
            for off in [-4.0_f32, 0.0, 4.0] {
                painter.line_segment([egui::pos2(c.x + off, r.top() + 2.0), egui::pos2(c.x + off, r.bottom() - 2.0)], s_thin);
                painter.line_segment([egui::pos2(r.left() + 2.0, c.y + off), egui::pos2(r.right() - 2.0, c.y + off)], s_thin);
            }
        }
        TileIconKind::FlipLeftRight => {
            // 중심축 기준 좌/우 반전(R)
            painter.line_segment([egui::pos2(c.x, c.y - 11.0), egui::pos2(c.x, c.y + 11.0)], s_main);
            painter.line_segment([egui::pos2(c.x - 9.0, c.y), egui::pos2(c.x + 9.0, c.y)], s_acc);
            painter.line_segment([egui::pos2(c.x - 9.0, c.y), egui::pos2(c.x - 5.0, c.y - 4.0)], s_acc);
            painter.line_segment([egui::pos2(c.x + 9.0, c.y), egui::pos2(c.x + 5.0, c.y - 4.0)], s_acc);
        }
        TileIconKind::FullSheetTab => {
            // 100% 도판 최대화(Tab)
            let r = egui::Rect::from_center_size(c, egui::vec2(22.0, 18.0));
            painter.rect_stroke(r, 2.0, s_main, egui::StrokeKind::Inside);
            painter.line_segment([egui::pos2(r.left() + 3.0, r.top() + 3.0), egui::pos2(r.right() - 3.0, r.bottom() - 3.0)], s_acc);
        }
        TileIconKind::VerifyReport => {
            // 실측 검수 보고서(L)
            let r = egui::Rect::from_center_size(c, egui::vec2(18.0, 22.0));
            painter.rect_stroke(r, 2.0, s_main, egui::StrokeKind::Inside);
            painter.line_segment([egui::pos2(c.x - 4.0, c.y + 1.0), egui::pos2(c.x - 1.0, c.y + 4.0)], s_acc);
            painter.line_segment([egui::pos2(c.x - 1.0, c.y + 4.0), egui::pos2(c.x + 5.0, c.y - 3.0)], s_acc);
        }
        TileIconKind::MultiPlateComposer => {
            // 다중 유물 합본 편집판(XL)
            for (dx, dy) in [(-5.5_f32, -4.5_f32), (5.5, -4.5), (-5.5, 4.5), (5.5, 4.5)] {
                let cell = egui::Rect::from_center_size(egui::pos2(c.x + dx, c.y + dy), egui::vec2(8.5, 7.0));
                painter.rect_stroke(cell, 1.0, s_main, egui::StrokeKind::Inside);
            }
        }
        TileIconKind::ExportSvg => {
            // 베지어 핸들 곡선 아이콘
            painter.line_segment([egui::pos2(c.x - 9.0, c.y + 6.0), egui::pos2(c.x, c.y - 7.0)], s_main);
            painter.line_segment([egui::pos2(c.x, c.y - 7.0), egui::pos2(c.x + 9.0, c.y + 6.0)], s_main);
            painter.circle_filled(egui::pos2(c.x, c.y - 7.0), 3.0, accent_col);
            painter.circle_filled(egui::pos2(c.x - 9.0, c.y + 6.0), 2.5, primary_col);
            painter.circle_filled(egui::pos2(c.x + 9.0, c.y + 6.0), 2.5, primary_col);
        }
        TileIconKind::ExportTiledPdf => {
            // 분할 인쇄 PDF 용지 격자
            let r = egui::Rect::from_center_size(c, egui::vec2(20.0, 22.0));
            painter.rect_stroke(r, 2.0, s_main, egui::StrokeKind::Inside);
            painter.line_segment([egui::pos2(c.x, r.top()), egui::pos2(c.x, r.bottom())], s_acc);
            painter.line_segment([egui::pos2(r.left(), c.y), egui::pos2(r.right(), c.y)], s_acc);
        }
        TileIconKind::ExportRetPackage => {
            // 도면·내보내기 주 행동 아이콘
            let r = egui::Rect::from_center_size(egui::pos2(c.x, c.y + 3.0), egui::vec2(20.0, 14.0));
            painter.rect_stroke(r, 2.0, s_main, egui::StrokeKind::Inside);
            painter.line_segment([egui::pos2(c.x, c.y + 4.0), egui::pos2(c.x, c.y - 9.0)], s_acc);
            painter.line_segment([egui::pos2(c.x - 5.0, c.y - 4.0), egui::pos2(c.x, c.y - 9.0)], s_acc);
            painter.line_segment([egui::pos2(c.x + 5.0, c.y - 4.0), egui::pos2(c.x, c.y - 9.0)], s_acc);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_all_ribbon_tiles_have_distinct_vector_icon_kind() {
        let mut seen_icons = HashSet::new();
        let mut total_tiles = 0usize;
        for tab in WorkflowTab::ALL {
            for tile in tab.ribbon_tiles() {
                total_tiles += 1;
                seen_icons.insert(tile.icon_kind);
                assert!(!tile.label_kr.is_empty());
                assert!(!tile.group_title.is_empty());
            }
        }
        assert_eq!(total_tiles, 27);
        assert_eq!(seen_icons.len(), 27, "모든 27개 리본 타일은 고유한 벡터 아이콘 종류를 가져야 한다");
    }
}
