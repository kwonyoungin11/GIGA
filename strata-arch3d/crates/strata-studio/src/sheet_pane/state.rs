use super::config::*;
use eframe::egui;
use image::RgbaImage;
use strata_core::ScalePreset;

/// 우측 2D 실측 도판 뷰포트(`SheetPane`)의 줌·팬, 드래그 `1:1` 부분 탁본(`ROI`), 선 굵기(`mm`)·색상 설정 및 정밀 탁본 캐시 상태.
#[derive(Clone)]
pub struct SheetPaneState {
    pub zoom: f32,
    pub pan_px: egui::Vec2,
    /// Image 3: 조판(Publication Page Layout) 용지 규격 설정 (기본 A3 가로)
    pub paper_preset: PaperSheetPreset,
    /// 우측 하단 도판 축척 설정 (기본 1:1 = ScalePreset::Full1To1)
    pub active_scale_preset: ScalePreset,
    /// Image 13: 사용자가 `[⬚ 부분탁본 선택]` 버튼을 눌러 탁본할 곳을 드래그하는 모드 활성화 여부
    pub roi_drag_mode: bool,
    /// 드래그 시작 지점 (`mm` 도판 좌표)
    pub roi_drag_start_mm: Option<[f64; 2]>,
    /// Image 14: 사용자가 드래그하여 지정한 부분 탁본 영역 (`Some([min_x_mm, min_y_mm, max_x_mm, max_y_mm])`)
    pub rubbing_roi_mm: Option<[f64; 4]>,
    /// 『부분 정밀 탁본 창 (ROI Rubbing Studio)』 표시 여부
    pub show_roi_rubbing_window: bool,
    /// Image 0 & Image 1: 좌측 1:1 단면도는 일러스트용 순수 벡터 실측 선화로 표시하고, 우측 상단에 1:1 탁본을 단독 표시 (`기본값 = false`)
    pub show_elevation_rubbing_overlay: bool,
    /// 6개 개별 도면 레이어(`0: 평면도`, `1: 단면도`, `2: 1:1 탁본`, `3: 수평 횡단면`, `4: 추가 단면 A-A'`, `5: 치수선·축척자`) 각각의 표시/90°회전/180°뒤집기/오프셋 상태
    pub view_transforms: [IndividualViewTransform2D; 6],
    /// 현재 선택된 도면 레이어 인덱스 (`Delete` 키로 삭제 또는 `↻90°` 회전 대상)
    pub selected_layer_idx: Option<usize>,
    /// 현재 도판에 `⑤ 추가 단면 A-A'` 레이어가 생성되어 있는지 여부
    pub has_custom_section_layer: bool,
    /// `⑤ 추가 단면 A-A'` 레이어 삭제 요청 플래그 (`main.rs`에서 `custom_section_line_mm = None` 처리)
    pub request_clear_custom_section: bool,
    /// 우측 상단 『🗂 레이어 관리』 패널 펼침 여부
    pub show_layer_panel: bool,
    /// 학술 논문 기반 정밀 탁본 파라미터 (농도·MSII 감도·시그모이드 대비·여백 임계·기법)
    pub rubbing_params: AcademicRubbingParams,
    /// Image 11 & Image 15: 모든 제도선의 색상 및 `mm` 두께 즉시 조정 상태
    pub line_style: PlateLineStyleConfig,
    /// 사용자가 하단 축척 버튼으로 직접 고른 경우 자동 축척 보정을 끈다.
    pub scale_locked: bool,
    pub(crate) cached_key: Option<SheetRubbingCacheKey>,
    pub(crate) cached_front_img: Option<RgbaImage>,
    pub(crate) cached_back_img: Option<RgbaImage>,
    pub(crate) cached_bottom_img: Option<RgbaImage>,
    pub(crate) cached_front_tex: Option<egui::TextureHandle>,
    pub(crate) cached_back_tex: Option<egui::TextureHandle>,
    pub(crate) cached_bottom_tex: Option<egui::TextureHandle>,
    /// 곡면 전개 탁본의 실제 물리 크기 `[둘레 mm, 높이 mm]` (`None` = 평면 탁본)
    pub unrolled_size_mm: Option<[f64; 2]>,
}

impl std::fmt::Debug for SheetPaneState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SheetPaneState")
            .field("zoom", &self.zoom)
            .field("pan_px", &self.pan_px)
            .field("paper_preset", &self.paper_preset)
            .field("active_scale_preset", &self.active_scale_preset)
            .field("roi_drag_mode", &self.roi_drag_mode)
            .field("rubbing_roi_mm", &self.rubbing_roi_mm)
            .field("show_roi_rubbing_window", &self.show_roi_rubbing_window)
            .field(
                "show_elevation_rubbing_overlay",
                &self.show_elevation_rubbing_overlay,
            )
            .field("view_transforms", &self.view_transforms)
            .field("selected_layer_idx", &self.selected_layer_idx)
            .field("has_custom_section_layer", &self.has_custom_section_layer)
            .field("rubbing_params", &self.rubbing_params)
            .field("line_style", &self.line_style)
            .finish()
    }
}

impl Default for SheetPaneState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan_px: egui::Vec2::ZERO,
            paper_preset: PaperSheetPreset::A3Landscape,
            active_scale_preset: ScalePreset::Full1To1,
            roi_drag_mode: false,
            roi_drag_start_mm: None,
            rubbing_roi_mm: None,
            show_roi_rubbing_window: false,
            show_elevation_rubbing_overlay: false,
            view_transforms: [IndividualViewTransform2D::default(); 6],
            selected_layer_idx: Some(0),
            has_custom_section_layer: false,
            request_clear_custom_section: false,
            show_layer_panel: true,
            rubbing_params: AcademicRubbingParams::default(),
            line_style: PlateLineStyleConfig::default(),
            scale_locked: false,
            cached_key: None,
            cached_front_img: None,
            cached_back_img: None,
            cached_bottom_img: None,
            cached_front_tex: None,
            cached_back_tex: None,
            cached_bottom_tex: None,
            unrolled_size_mm: None,
        }
    }
}

impl SheetPaneState {
    /// 선택된 레이어를 `Delete` 키(또는 `[🗑]` 버튼)로 삭제(숨김)한다.
    /// `⑤ 추가 단면 A-A'`(인덱스 4) 삭제 시에는 3D 뷰의 추가단면선도 함께 제거하도록 `request_clear_custom_section = true`를 설정한다.
    pub fn delete_selected_layer(&mut self) -> Option<usize> {
        let idx = self.selected_layer_idx?;
        if idx < self.view_transforms.len() {
            self.view_transforms[idx].visible = false;
            if idx == 4 {
                self.request_clear_custom_section = true;
            }
            return Some(idx);
        }
        None
    }

    /// 특정 레이어를 삭제(숨김)한다.
    pub fn delete_layer(&mut self, idx: usize) {
        if idx < self.view_transforms.len() {
            self.view_transforms[idx].visible = false;
            if idx == 4 {
                self.request_clear_custom_section = true;
            }
        }
    }

    /// 선택된 레이어를 시계방향 90° 회전한다.
    #[allow(dead_code)] // 테스트가 레이어 90° 회전 상태 전이를 검증한다.
    pub fn rotate_selected_layer_cw_90(&mut self) {
        if let Some(idx) = self.selected_layer_idx {
            if idx < self.view_transforms.len() {
                self.view_transforms[idx].rotate_cw_90();
            }
        }
    }

    /// 사용자가 드래그한 부분 탁본 영역(`mm`)을 설정하고, 우측 탁본 영역에 `1:1` 실제 크기로 즉시 반영한다 (`Image 13`, `Image 14`).
    pub fn set_rubbing_roi_mm(&mut self, roi_mm: Option<[f64; 4]>) {
        if let Some([x0, y0, x1, y1]) = roi_mm {
            let min_x = x0.min(x1);
            let max_x = x0.max(x1);
            let min_y = y0.min(y1);
            let max_y = y0.max(y1);
            if (max_x - min_x) >= 2.0 && (max_y - min_y) >= 2.0 {
                self.rubbing_roi_mm = Some([min_x, min_y, max_x, max_y]);
                self.cached_front_tex = None;
                self.cached_back_tex = None;
                self.cached_bottom_tex = None;
            }
        } else {
            self.rubbing_roi_mm = None;
            self.cached_front_tex = None;
            self.cached_back_tex = None;
            self.cached_bottom_tex = None;
        }
    }
}
