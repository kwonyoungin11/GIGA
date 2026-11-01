use strata_core::strata_tokens::metrics::{
    CTX_BAR_HEIGHT, RIBBON_HEIGHT, RIBBON_TILE_SIZE, SHEET_DEFAULT_RATIO, SLOT_LIST_WIDTH,
    STATUS_BAR_HEIGHT, TAB_BAR_HEIGHT, VIEW3D_DEFAULT_RATIO,
};
use strata_core::{
    ArtifactCategory, BeadSubtype, SherdPlateStrategy, SherdSubtype,
};
use eframe::egui;
use serde::{Deserialize, Serialize};

/// 국가유산청(구 문화재청) 『발굴조사보고서 작성 표준』 기반 10종 실측도면 작성양식 템플릿.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChsReportTemplate {
    /// ① 토기·완형 표준도 (좌 외면문양·우 회전단면 + 외저면탁본 + 수평횡단면)
    VesselCompleteStandard,
    /// ② 토기·구연/저부 복원도 (중심축 기준 구경·저경 실선/파선 회전복원)
    VesselRimBaseReconstruct,
    /// ③ 토기·동체편 3단도 (외면 정밀탁본 ── 수직 회전단면 ── 내면 물레속선 3열 병렬)
    SherdBodyThreeColumn,
    /// ④ 토기·파수/주구/굽 특수편 (접합면·삽입촉 파선 + 부속부 횡단면)
    VesselHandleSpout,
    /// ⑤ 기와·막새(수막새/암막새) (와당 문양 정면탁본 + 종단면 + 접합기법)
    RoofTileMaksae,
    /// ⑥ 기와·평기와(수키와/암키와) (외면 타날문 + 단면 와도분할 화살표 + 내면 포목흔 올수)
    RoofTilePlain,
    /// ⑦ 석기·타제 박리도 (정·배·측·단면 + 3단계 선굵기 박리능선)
    LithicFlaked,
    /// ⑧ 석기·마제 실측도 (마연면·사용흔 + 정면·측면·다단 횡단면)
    LithicGround,
    /// ⑨ 금속기·철기/청동기 (외형 실선 + 원심코어/부식팽창층 파선 분리 + 목질흔)
    MetalCorroded,
    /// ⑩ 옥류·장신구 확대도 (상면 3원 + 정면 + 천공 Ⅰa~Ⅱb 종단면, 2:1~10:1)
    JadeBeadMicro,
}

impl ChsReportTemplate {
    pub const ALL: [Self; 10] = [
        Self::VesselCompleteStandard,
        Self::VesselRimBaseReconstruct,
        Self::SherdBodyThreeColumn,
        Self::VesselHandleSpout,
        Self::RoofTileMaksae,
        Self::RoofTilePlain,
        Self::LithicFlaked,
        Self::LithicGround,
        Self::MetalCorroded,
        Self::JadeBeadMicro,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::VesselCompleteStandard => "TPL_VESSEL_STD",
            Self::VesselRimBaseReconstruct => "TPL_RIM_RECON",
            Self::SherdBodyThreeColumn => "TPL_BODY_3COL",
            Self::VesselHandleSpout => "TPL_HANDLE",
            Self::RoofTileMaksae => "TPL_MAKSAE",
            Self::RoofTilePlain => "TPL_PLAIN_TILE",
            Self::LithicFlaked => "TPL_LITHIC_FLAKED",
            Self::LithicGround => "TPL_LITHIC_GROUND",
            Self::MetalCorroded => "TPL_METAL",
            Self::JadeBeadMicro => "TPL_JADE_BEAD",
        }
    }

    pub fn label_kr(self) -> &'static str {
        match self {
            Self::VesselCompleteStandard => "완형 표준도",
            Self::VesselRimBaseReconstruct => "구연·저부 복원",
            Self::SherdBodyThreeColumn => "동체편 3단도",
            Self::VesselHandleSpout => "파수·주구·굽",
            Self::RoofTileMaksae => "수·암막새 도면",
            Self::RoofTilePlain => "수·암키와 탁본도",
            Self::LithicFlaked => "타제석기 박리도",
            Self::LithicGround => "마제석기 실측도",
            Self::MetalCorroded => "철기·청동기 단면",
            Self::JadeBeadMicro => "옥·구슬 확대도",
        }
    }

    pub fn standard_spec_kr(self) -> &'static str {
        match self {
            Self::VesselCompleteStandard => {
                "국가유산청 표준 ① 완형 토기·도자기 (좌 외면·우 먹단면 + 외저면 탁본 + 수평 횡단면)"
            }
            Self::VesselRimBaseReconstruct => {
                "국가유산청 표준 ② 구연·저부편 회전복원도 (중심축·구경선·저경선 실선/추정파선 복원)"
            }
            Self::SherdBodyThreeColumn => {
                "국가유산청 표준 ③ 동체부·무문양편 3단 병렬도 (외면 탁본 ── 수직 단면 ── 내면 탁본)"
            }
            Self::VesselHandleSpout => {
                "국가유산청 표준 ④ 파수·주구·굽 접합도 (삽입촉·부착면 파선 및 부속부 횡단면 병기)"
            }
            Self::RoofTileMaksae => {
                "국가유산청 표준 ⑤ 막새(수막새·암막새) 실측도 (와당 드림새 정면 탁본 + 접합 종단면)"
            }
            Self::RoofTilePlain => {
                "국가유산청 표준 ⑥ 평기와(수키와·암키와) 실측도 (외면 타날문 + 와도분할 + 내면 포목흔)"
            }
            Self::LithicFlaked => {
                "국가유산청 표준 ⑦ 타제석기 박리도 (정·배·측·단면 4면 + 박리능선 3단 선굵기)"
            }
            Self::LithicGround => {
                "국가유산청 표준 ⑧ 마제석기 실측도 (마연 방향·사용흔 + 정면·측면·다단 횡단면)"
            }
            Self::MetalCorroded => {
                "국가유산청 표준 ⑨ 부식 금속기 실측도 (외형선 + 원심코어 먹채움/부식팽창층 파선 분리)"
            }
            Self::JadeBeadMicro => {
                "국가유산청 표준 ⑩ 옥류·구슬 미소확대도 (2:1~10:1 · 상면 3중원 + 천공 Ⅰa~Ⅱb 종단면)"
            }
        }
    }

    pub fn to_compose_profile(
        self,
    ) -> (
        ArtifactCategory,
        Option<SherdSubtype>,
        Option<SherdPlateStrategy>,
        Option<BeadSubtype>,
    ) {
        match self {
            Self::VesselCompleteStandard => (ArtifactCategory::CompleteVessel, None, None, None),
            Self::VesselRimBaseReconstruct => (
                ArtifactCategory::Potsherd,
                Some(SherdSubtype::RimSherd),
                Some(SherdPlateStrategy::FullReconstruction),
                None,
            ),
            Self::SherdBodyThreeColumn => (
                ArtifactCategory::Potsherd,
                Some(SherdSubtype::BodySherd),
                Some(SherdPlateStrategy::ThreeColumnParallel),
                None,
            ),
            Self::VesselHandleSpout => (
                ArtifactCategory::Potsherd,
                Some(SherdSubtype::HandleSpoutSherd),
                Some(SherdPlateStrategy::FullReconstruction),
                None,
            ),
            Self::RoofTileMaksae | Self::RoofTilePlain => {
                (ArtifactCategory::RoofTile, None, None, None)
            }
            Self::LithicFlaked | Self::LithicGround => {
                (ArtifactCategory::Lithic, None, None, None)
            }
            Self::MetalCorroded => (ArtifactCategory::CorrodedMetal, None, None, None),
            Self::JadeBeadMicro => (
                ArtifactCategory::BeadJade,
                None,
                None,
                Some(BeadSubtype::TubularBead),
            ),
        }
    }
}

/// 단계 탭 없이 한눈에 모든 파일·국가유산청 도면양식·단면·탁본·출판 도구를 조작하는 통합 스튜디오 리본 타일 목록.
pub fn unified_studio_ribbon_tiles() -> &'static [RibbonTileSpec] {
    &[
        RibbonTileSpec {
            group_title: "파일 · 정합",
            code: "OPEN",
            label_kr: "유물 열기",
            shortcut: "Ctrl+O",
            icon_kind: TileIconKind::FolderOpen,
            is_primary_action: false,
        },
        RibbonTileSpec {
            group_title: "파일 · 정합",
            code: "AP2",
            label_kr: "AP2 정합",
            shortcut: "Ctrl+I",
            icon_kind: TileIconKind::Ap2Project,
            is_primary_action: false,
        },
        RibbonTileSpec {
            group_title: "국가유산청 토기·도자기 도면양식",
            code: "TPL_VESSEL_STD",
            label_kr: "완형 표준도",
            shortcut: "1",
            icon_kind: TileIconKind::VesselComplete,
            is_primary_action: false,
        },
        RibbonTileSpec {
            group_title: "국가유산청 토기·도자기 도면양식",
            code: "TPL_RIM_RECON",
            label_kr: "구연·저부 복원",
            shortcut: "2",
            icon_kind: TileIconKind::PotsherdFragment,
            is_primary_action: false,
        },
        RibbonTileSpec {
            group_title: "국가유산청 토기·도자기 도면양식",
            code: "TPL_BODY_3COL",
            label_kr: "동체편 3단도",
            shortcut: "3",
            icon_kind: TileIconKind::CylinderUnroll,
            is_primary_action: false,
        },
        RibbonTileSpec {
            group_title: "국가유산청 토기·도자기 도면양식",
            code: "TPL_HANDLE",
            label_kr: "파수·주구·굽",
            shortcut: "4",
            icon_kind: TileIconKind::VirtualRefit,
            is_primary_action: false,
        },
        RibbonTileSpec {
            group_title: "국가유산청 기와·석기·금속·옥 양식",
            code: "TPL_MAKSAE",
            label_kr: "수·암막새",
            shortcut: "5",
            icon_kind: TileIconKind::RoofTileLithic,
            is_primary_action: false,
        },
        RibbonTileSpec {
            group_title: "국가유산청 기와·석기·금속·옥 양식",
            code: "TPL_PLAIN_TILE",
            label_kr: "수·암키와",
            shortcut: "6",
            icon_kind: TileIconKind::HempFftCount,
            is_primary_action: false,
        },
        RibbonTileSpec {
            group_title: "국가유산청 기와·석기·금속·옥 양식",
            code: "TPL_LITHIC_FLAKED",
            label_kr: "타제 박리도",
            shortcut: "7",
            icon_kind: TileIconKind::Fracture3Way,
            is_primary_action: false,
        },
        RibbonTileSpec {
            group_title: "국가유산청 기와·석기·금속·옥 양식",
            code: "TPL_LITHIC_GROUND",
            label_kr: "마제 실측도",
            shortcut: "8",
            icon_kind: TileIconKind::AxisSolver,
            is_primary_action: false,
        },
        RibbonTileSpec {
            group_title: "국가유산청 기와·석기·금속·옥 양식",
            code: "TPL_METAL",
            label_kr: "철·청동기 단면",
            shortcut: "9",
            icon_kind: TileIconKind::OccludedDashed,
            is_primary_action: false,
        },
        RibbonTileSpec {
            group_title: "국가유산청 기와·석기·금속·옥 양식",
            code: "TPL_JADE_BEAD",
            label_kr: "옥·구슬 확대",
            shortcut: "0",
            icon_kind: TileIconKind::BeadJade,
            is_primary_action: false,
        },
        RibbonTileSpec {
            group_title: "부분 정밀탁본 · 출판",
            code: "ROI_RUBBING_WIN",
            label_kr: "영역 탁본창",
            shortcut: "T",
            icon_kind: TileIconKind::RubbingHandprint,
            is_primary_action: true,
        },
        RibbonTileSpec {
            group_title: "부분 정밀탁본 · 출판",
            code: "FLIP_R",
            label_kr: "좌우반전(R)",
            shortcut: "R",
            icon_kind: TileIconKind::FlipLeftRight,
            is_primary_action: false,
        },
        RibbonTileSpec {
            group_title: "부분 정밀탁본 · 출판",
            code: "EXP_SVG",
            label_kr: "SVG 제도선",
            shortcut: "Ctrl+E",
            icon_kind: TileIconKind::ExportSvg,
            is_primary_action: false,
        },
        RibbonTileSpec {
            group_title: "부분 정밀탁본 · 출판",
            code: "EXP_PDF",
            label_kr: "분할인쇄 PDF",
            shortcut: "Ctrl+P",
            icon_kind: TileIconKind::ExportTiledPdf,
            is_primary_action: false,
        },
        RibbonTileSpec {
            group_title: "부분 정밀탁본 · 출판",
            code: "EXP_RET",
            label_kr: "ret/ 패키지",
            shortcut: "Ctrl+Shift+E",
            icon_kind: TileIconKind::ExportRetPackage,
            is_primary_action: false,
        },
    ]
}

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

    /// 좌측 슬롯 패널(`176px`)과 4단계 탭을 제거하고 창 전체 가로폭(`100%`)을 3D 뷰포트와 2D 방안지 도판에 할당한다.
    pub fn compute_full_width_workspace(
        window_w: f32,
        window_h: f32,
        sheet_full_tab_mode: bool,
    ) -> Self {
        let tab_bar_h = 0.0;
        let tile_ribbon_h = RIBBON_HEIGHT as f32;
        let tile_size = RIBBON_TILE_SIZE as f32;
        let ctx_bar_h = CTX_BAR_HEIGHT as f32;
        let status_bar_h = STATUS_BAR_HEIGHT as f32;
        let slot_list_w = 0.0;

        let top_chrome = tile_ribbon_h + ctx_bar_h;
        let work_area_h = (window_h - top_chrome - status_bar_h).max(240.0);
        let remaining_w = window_w.max(400.0);

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

    #[test]
    fn test_tdd2_full_width_workspace_without_left_slot_panel_or_step_tabs() {
        let geom = R1FiveTierGeometry::compute_full_width_workspace(1440.0, 900.0, false);
        assert_eq!(geom.slot_list_w, 0.0, "좌측 슬롯 패널(176px)을 제거하여 작업창을 최대화해야 한다");
        assert!(
            ((geom.view3d_w + geom.sheet_pane_w) - 1440.0).abs() < 1e-3,
            "3D 뷰포트와 2D 방안지 도판의 합이 창 전체 가로폭(1440px) 100%를 차지해야 한다"
        );
        let unified = unified_studio_ribbon_tiles();
        assert!(unified.len() >= 14, "단계 탭 없이 한눈에 조작 가능한 통합 리본 타일이 제공되어야 한다");
    }

    #[test]
    fn test_tdd3_chs_10_archaeological_report_drawing_templates() {
        assert_eq!(
            ChsReportTemplate::ALL.len(),
            10,
            "국가유산청 발굴조사보고서 실측도면 작성양식 10종으로 세분화되어야 한다"
        );
        for tpl in ChsReportTemplate::ALL {
            assert!(!tpl.label_kr().is_empty());
            assert!(!tpl.standard_spec_kr().is_empty());
            let (_cat, _sherd_sub, _strat, _bead_sub) = tpl.to_compose_profile();
        }
    }
}
