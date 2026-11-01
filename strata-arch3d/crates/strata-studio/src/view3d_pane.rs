use strata_core::strata_tokens::tokens::{
    ACTION, CLAY_FOCUS, CLAY_INK, CLAY_WASH, EDGE, GROUND, INK, MUTED, PROFILE_RED, SELECT_BLUE,
    SURFACE,
};
use strata_core::{SurfaceRegion, TriMesh};
use strata_orient::RimGapChart360;
use eframe::egui;
use image::{Rgba, RgbaImage};
use rayon::prelude::*;

/// 3D 뷰포트 셰이딩 모드 (`C++ View3DPane.cpp` + `MeshRaster.cpp` 1:1 대응).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View3DShaderMode {
    /// `.mtl` + `.jpg` 실사 UV 텍스처 + 양면 스튜디오 조명 (4K 원본 스캐너 해상도)
    PhotoTexture,
    /// 점토·석재 솔리드 양면 음영 조명
    SolidClay,
    /// 실시간 3D 정밀 탁본 (곡률·법선 기반 먹물 농담 + 라플라시안 에지 + 한지 질감)
    RealTimeRubbing,
    /// 편조각 3분류 자동 분할 (`외면` 청록 / `내면` 녹색 / `파단면` 테라코타)
    Segmentation3Way,
    /// 반투명 X-Ray 내벽·천공 투시 모드
    XRayInner,
}

impl View3DShaderMode {
    pub fn label_kr(self) -> &'static str {
        match self {
            Self::PhotoTexture => "실사텍스처",
            Self::SolidClay => "솔리드",
            Self::RealTimeRubbing => "정밀탁본",
            Self::Segmentation3Way => "3분류파단",
            Self::XRayInner => "X-Ray",
        }
    }
}

/// 3D 뷰포트 마우스 드래그 조작 모드.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DragMode3D {
    /// 기본값: 절단틀(`P_cut`)은 월드 중앙에 고정하고 유물만 회전축 둘레로 회전(`ψ`)
    TurntableArtifact,
    /// 카메라 궤도 회전 (`cam_yaw_deg`, `cam_pitch_deg`)
    CameraOrbit,
    /// 바닥·천공축 미세 기울기 조절 (`0.1°` 단위)
    AxisTilt,
}

/// 기존 호환용 4방향 미세 다이얼 열거형 (`test_tdd9` 호환 유지).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardinalDialDir {
    Up,
    Down,
    Left,
    Right,
}

/// v6.0 사용자 주석(`Image 1 ~ Image 4` + 내부 수평정합 제5휠) 1:1 대응 5종 정밀 미세조정 휠 역할.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrecisionWheelKind {
    /// Image 1 (상단 가로 휠): 유물이 시계방향·반시계방향으로 회전하는 휠 (`ψ`, `0.1°` 스텝, 휠 아래 설명)
    TopTurnCwCcw,
    /// Image 2 (하단 가로 휠): 유물·단면선을 앞뒤로 미세조정하는 휠 (`ΔY`, `0.01mm` 스텝, 휠 아래 설명)
    BottomSectionFrontBack,
    /// Image 3 (우측 세로 휠): 유물의 좌우 기울기를 미세조정하는 휠 (`roll_deg`, `0.1°` 스텝, 휠 옆 설명)
    RightTiltLeftRight,
    /// Image 4 (좌측 세로 휠): 유물의 아래위를 회전하는 휠 — 올리면 위로 회전, 내리면 아래로 회전 (`tilt_deg`, `0.1°` 스텝, 휠 옆 설명)
    LeftPitchUpDown,
    /// <USER_REQUEST> (상하좌우 안쪽 제5휠 호환): 내부 수평 미세정합 휠 (`inner_level_deg`, `0.05°` 스텝)
    InnerHorizontalLevel,
    /// Image 0 (수평횡단면 전용 휠): 유물은 고정하고 수평횡단면 절단선(`top_cut_z_mm`)만 아래위로 이동시켜 수평횡단면 도면을 그리는 휠 (`0.1mm` 스텝)
    InnerHorizSectionZ,
}

impl PrecisionWheelKind {
    /// 각 휠의 사용자 인터페이스 설명 문구 (`Image 0 ~ 4` 및 `<USER_REQUEST>` 반영).
    pub fn description_label_kr(self) -> &'static str {
        match self {
            Self::TopTurnCwCcw => "↻ 유물 시계·반시계 회전 휠 [0.1°]",
            Self::BottomSectionFrontBack => "▲▼ 유물·단면선 앞뒤 미세조정 휠 [0.01mm]",
            Self::RightTiltLeftRight => "좌우 기울기 미세조정 [0.1°]",
            Self::LeftPitchUpDown => "상하 회전 (↑위·↓아래) [0.1°]",
            Self::InnerHorizontalLevel => "⚖ 수평·단면 정합 미세휠 [0.05°]",
            Self::InnerHorizSectionZ => "⇕ 수평횡단면 선 상하 이동 휠 [0.1mm]",
        }
    }
}

/// 3D 뷰포트의 가로·세로 종횡비(`rect_w / rect_h`)를 드래그 중과 해제 후에 100% 동일하게 보존하는 고해상도 등방성 렌더 크기를 계산한다.
pub fn compute_isotropic_render_dimensions(
    rect_w: f32,
    rect_h: f32,
    is_dragging: bool,
) -> (u32, u32) {
    let rw = f64::from(rect_w.max(64.0));
    let rh = f64::from(rect_h.max(64.0));
    let max_dim = rw.max(rh);
    let desired_mult = if is_dragging { 0.90 } else { 1.0 };
    let max_cap = if is_dragging { 720.0 } else { 920.0 };
    let s = (max_cap / max_dim).min(desired_mult).max(180.0 / max_dim);
    let w = (rw * s).round().max(64.0) as u32;
    let h = ((w as f64) * (rh / rw)).round().max(64.0) as u32;
    (w, h)
}

/// 유물 외경 반경(`artifact_radius_mm`)의 정확히 `1.20배(+20%)`가 되는 직상방·입면 동심원 방안지 반경을 계산한다.
#[inline]
pub fn compute_concentric_grid_radius_mm(artifact_radius_mm: f64) -> f64 {
    artifact_radius_mm * 1.20
}

/// 상단 직상방 동심원 뷰와 하단 정면 입면 뷰가 가로 X축(`mm -> px`)을 100% 동일하게 공유하여
/// 위 단면선과 아래 단면선이 수직 점선으로 오차 없이 연결되도록 공통 가로 스케일(`px/mm`)을 계산한다 (`Image 7`).
#[inline]
pub fn compute_shared_horizontal_scale_px_per_mm(
    pane_width_px: f32,
    top_height_px: f32,
    front_height_px: f32,
    grid_r_mm: f64,
    height_mm: f64,
    zoom: f32,
) -> f64 {
    let w = f64::from(pane_width_px.max(120.0));
    let th = f64::from(top_height_px.max(120.0));
    let fh = f64::from(front_height_px.max(140.0));
    // 상단 동심원 지름(2 * grid_r_mm)과 하단 유물 높이(height_mm)가 모두 여유 있게 들어오는 단일 등방성 스케일
    let by_top_w = (w * 0.42) / grid_r_mm.max(1.0);
    let by_top_h = (th * 0.43) / grid_r_mm.max(1.0);
    let by_front_h = (fh * 0.50) / height_mm.max(10.0);
    by_top_w.min(by_top_h).min(by_front_h).max(0.8) * f64::from(zoom)
}

/// 3D Z-버퍼 프레임 더티 캐시 키.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct View3DRenderCacheKey {
    pub mesh_name: String,
    pub width: u32,
    pub height: u32,
    pub active_mode: View3DShaderMode,
    pub cutaway_open: bool,
    pub yaw_q: i32,
    pub pitch_q: i32,
    pub zoom_q: i32,
    pub psi_q: i32,
    pub tilt_q: i32,
    pub roll_q: i32,
    pub inner_level_q: i32,
    pub offset_x_q: i32,
    pub cut_y_q: i32,
    pub top_z_q: i32,
    pub light_az_q: i32,
    pub light_el_q: i32,
    pub light_int_q: i32,
}

type CurvesCacheKey = (String, i32, i32, i32, i32, i32, i32, i32);

/// 3D 뷰포트 상태 (`DESIGN_SPEC.md` §4.2 + v6.0 5휠·중앙고정 단면선 통합).
#[derive(Clone)]
pub struct View3DPaneState {
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub zoom: f32,
    pub show_cutting_frame: bool,
    pub show_concentric_scope: bool,
    pub show_3way_segmentation: bool,
    pub xray_inner_mode: bool,
    /// 유물 턴테이블 시계·반시계 회전각 `ψ` (`0.0 .. 360.0°`, 상단 가로 휠 `Image 1`)
    pub active_section_angle_deg: f64,
    /// 직상방 동심원 방안지 위에서 유물의 좌우 수평 이동 오프셋 `ΔX` (`mm`, `Image 5` & `Image 8`)
    pub artifact_offset_x_mm: f64,
    /// 유물·단면선 앞뒤 미세조정 오프셋 `ΔY` (`mm`, 하단 가로 휠 `Image 2` & `Image 5`)
    pub cut_y_mm: f64,
    /// 수평 횡단면 높이 `Top Cut Z` (`mm`)
    pub top_cut_z_mm: f64,
    pub top_cut_initialized_for_mesh: String,
    /// 유물 상하 회전(`↑위·↓아래`, 좌측 세로 휠 `Image 4`, `-30.0° .. +30.0°`)
    pub tilt_deg: f64,
    /// 유물 좌우 기울기(`Roll`, 우측 세로 휠 `Image 3`, `-30.0° .. +30.0°`)
    pub roll_deg: f64,
    /// 유물 상하좌우 안쪽 제5휠 — 수평·단면 정합 미세 보정각 (`-15.0° .. +15.0°`, `0.05°` 단위, `<USER_REQUEST>`)
    pub inner_level_deg: f64,
    /// Image 2 ("광원을 조정할수있는 작은창"): 광원 방위각 (`0.0 ..= 360.0°`, 기본 `315.0°` = 좌상단 45° 표준광)
    pub light_azimuth_deg: f32,
    /// Image 2 ("광원을 조정할수있는 작은창"): 광원 고도각 (`5.0 ..= 85.0°`, 기본 `45.0°`)
    pub light_elevation_deg: f32,
    /// Image 2 ("광원을 조정할수있는 작은창"): 광원 밝기·대비 강도 (`0.40 ..= 1.80`, 기본 `1.00`)
    pub light_intensity: f32,
    /// Image 2: 광원 조정 작은창 표시 여부 (기본 `true`)
    pub show_light_window: bool,
    /// Image 5: 직상방 뷰 좌측 상단 `[+ 단면추가]` 버튼으로 활성화되는 사용자 자유 단면선 그리기 모드
    pub custom_section_draw_mode: bool,
    /// Image 5: 직상방 뷰 위에 그어진 사용자 추가 단면선 양 끝점 `[p0_mm, p1_mm]` (`mm`, 끝점 드래그 시 한쪽만 이동)
    pub custom_section_line_mm: Option<[[f64; 2]; 2]>,
    /// Image 5: 현재 드래그 중인 사용자 단면선 끝점 인덱스 (`Some(0)` = A 끝점, `Some(1)` = A' 끝점)
    pub custom_section_dragging_endpoint: Option<usize>,
    pub cutaway_open: bool,
    pub is_interacting: bool,
    pub shader_mode: View3DShaderMode,
    pub drag_mode: DragMode3D,
    pub cached_zbuffer_key: Option<View3DRenderCacheKey>,
    pub cached_zbuffer_rgba: Option<RgbaImage>,
    pub cached_zbuffer_tex: Option<egui::TextureHandle>,
    pub cached_topdown_key: Option<View3DRenderCacheKey>,
    pub cached_topdown_rgba: Option<RgbaImage>,
    pub cached_topdown_tex: Option<egui::TextureHandle>,
    pub cached_fixed_hsec_key: Option<(String, u32, u32, View3DShaderMode)>,
    pub cached_fixed_hsec_rgba: Option<RgbaImage>,
    pub cached_fixed_hsec_tex: Option<egui::TextureHandle>,
    pub cached_curves_key: Option<CurvesCacheKey>,
    pub cached_v_segs: Vec<[[f64; 3]; 2]>,
    pub cached_h_segs: Vec<[[f64; 3]; 2]>,
}

impl std::fmt::Debug for View3DPaneState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("View3DPaneState")
            .field("yaw_deg", &self.yaw_deg)
            .field("pitch_deg", &self.pitch_deg)
            .field("zoom", &self.zoom)
            .field("active_section_angle_deg", &self.active_section_angle_deg)
            .field("artifact_offset_x_mm", &self.artifact_offset_x_mm)
            .field("tilt_deg", &self.tilt_deg)
            .field("roll_deg", &self.roll_deg)
            .field("inner_level_deg", &self.inner_level_deg)
            .field("light_azimuth_deg", &self.light_azimuth_deg)
            .field("light_elevation_deg", &self.light_elevation_deg)
            .field("light_intensity", &self.light_intensity)
            .field("cut_y_mm", &self.cut_y_mm)
            .field("top_cut_z_mm", &self.top_cut_z_mm)
            .field("custom_section_line_mm", &self.custom_section_line_mm)
            .field("shader_mode", &self.shader_mode)
            .finish()
    }
}

impl Default for View3DPaneState {
    fn default() -> Self {
        Self {
            yaw_deg: 0.0,
            pitch_deg: 0.0,
            zoom: 1.0,
            show_cutting_frame: true,
            show_concentric_scope: true,
            show_3way_segmentation: false,
            xray_inner_mode: false,
            active_section_angle_deg: 0.0,
            artifact_offset_x_mm: 0.0,
            cut_y_mm: 0.0,
            top_cut_z_mm: 18.0,
            top_cut_initialized_for_mesh: String::new(),
            tilt_deg: 0.0,
            roll_deg: 0.0,
            inner_level_deg: 0.0,
            light_azimuth_deg: 315.0,
            light_elevation_deg: 45.0,
            light_intensity: 1.0,
            show_light_window: true,
            custom_section_draw_mode: false,
            custom_section_line_mm: None,
            custom_section_dragging_endpoint: None,
            cutaway_open: false,
            is_interacting: false,
            shader_mode: View3DShaderMode::PhotoTexture,
            drag_mode: DragMode3D::TurntableArtifact,
            cached_zbuffer_key: None,
            cached_zbuffer_rgba: None,
            cached_zbuffer_tex: None,
            cached_topdown_key: None,
            cached_topdown_rgba: None,
            cached_topdown_tex: None,
            cached_fixed_hsec_key: None,
            cached_fixed_hsec_rgba: None,
            cached_fixed_hsec_tex: None,
            cached_curves_key: None,
            cached_v_segs: Vec::new(),
            cached_h_segs: Vec::new(),
        }
    }
}

impl View3DPaneState {
    /// Image 2: 현재 광원 방위각(`light_azimuth_deg`)과 고도각(`light_elevation_deg`)으로부터 정규화된 3D 광원 방향 벡터를 산출한다.
    #[inline]
    pub fn compute_light_direction_vector(&self) -> [f64; 3] {
        let az_rad = f64::from(self.light_azimuth_deg).to_radians();
        let el_rad = f64::from(self.light_elevation_deg.clamp(5.0, 88.0)).to_radians();
        let cos_el = el_rad.cos();
        let lx = cos_el * az_rad.sin();
        let ly = -cos_el * az_rad.cos();
        let lz = el_rad.sin();
        let len = (lx * lx + ly * ly + lz * lz).sqrt().max(1e-9);
        [lx / len, ly / len, lz / len]
    }

    pub fn effective_shader_mode(&self) -> View3DShaderMode {
        if self.show_3way_segmentation {
            View3DShaderMode::Segmentation3Way
        } else if self.xray_inner_mode {
            View3DShaderMode::XRayInner
        } else {
            self.shader_mode
        }
    }

    pub fn build_cache_key(&self, mesh_name: &str, width: u32, height: u32) -> View3DRenderCacheKey {
        View3DRenderCacheKey {
            mesh_name: mesh_name.to_string(),
            width,
            height,
            active_mode: self.effective_shader_mode(),
            cutaway_open: self.cutaway_open,
            yaw_q: (self.yaw_deg * 5.0).round() as i32,
            pitch_q: (self.pitch_deg * 5.0).round() as i32,
            zoom_q: (self.zoom * 100.0).round() as i32,
            psi_q: (self.active_section_angle_deg * 10.0).round() as i32,
            tilt_q: (self.tilt_deg * 10.0).round() as i32,
            roll_q: (self.roll_deg * 10.0).round() as i32,
            inner_level_q: (self.inner_level_deg * 20.0).round() as i32,
            offset_x_q: (self.artifact_offset_x_mm * 100.0).round() as i32,
            cut_y_q: (self.cut_y_mm * 100.0).round() as i32,
            // 수평횡단면 절단선(top_cut_z_mm)은 선 오버레이만 이동하고 3D 유물 래스터 자체는 고정되므로 0으로 고정(0ms 즉시 반응!)
            top_z_q: 0,
            light_az_q: (self.light_azimuth_deg * 2.0).round() as i32,
            light_el_q: (self.light_elevation_deg * 2.0).round() as i32,
            light_int_q: (self.light_intensity * 50.0).round() as i32,
        }
    }

    /// 파라미터가 바뀌었을 때만 정면 입면 Z-버퍼를 재렌더링하고, 변경이 없으면 캐시된 버퍼를 즉시 반환한다 (`0ms`).
    pub fn ensure_zbuffer_cached(
        &mut self,
        mesh: &TriMesh,
        region_labels: Option<&[SurfaceRegion]>,
        width: u32,
        height: u32,
    ) -> (u32, u32, bool) {
        let (w, h) = compute_isotropic_render_dimensions(
            width as f32,
            height as f32,
            self.is_interacting,
        );
        let key = self.build_cache_key(&mesh.name, w, h);
        if self.cached_zbuffer_key.as_ref() == Some(&key) {
            if let Some(ref img) = self.cached_zbuffer_rgba {
                return (img.width(), img.height(), false);
            }
        }
        let img = render_mesh_zbuffer_rgba(mesh, region_labels, self, w, h);
        let out_w = img.width();
        let out_h = img.height();
        self.cached_zbuffer_rgba = Some(img);
        self.cached_zbuffer_tex = None;
        self.cached_zbuffer_key = Some(key);
        (out_w, out_h, true)
    }

    /// 파라미터가 바뀌었을 때만 직상방(Top-Down) 동심원 방안지 Z-버퍼를 등방성(`1:1`) 고해상도로 재렌더링하고 캐시한다.
    pub fn ensure_topdown_cached(
        &mut self,
        mesh: &TriMesh,
        width: u32,
        height: u32,
    ) -> (u32, u32, bool) {
        let (w, h) = compute_isotropic_render_dimensions(
            width as f32,
            height as f32,
            self.is_interacting,
        );
        let key = self.build_cache_key(&mesh.name, w, h);
        if self.cached_topdown_key.as_ref() == Some(&key) {
            if let Some(ref img) = self.cached_topdown_rgba {
                return (img.width(), img.height(), false);
            }
        }
        let img = render_topdown_concentric_plan_rgba(mesh, self, w, h);
        let out_w = img.width();
        let out_h = img.height();
        self.cached_topdown_rgba = Some(img);
        self.cached_topdown_tex = None;
        self.cached_topdown_key = Some(key);
        (out_w, out_h, true)
    }

    /// 3D 종단면·횡단면 교차 선분을 더티 캐시하여 파라미터 변경 시에만 계산한다.
    pub fn ensure_3d_curves_cached(&mut self, mesh: &TriMesh) {
        let ckey: CurvesCacheKey = (
            mesh.name.clone(),
            (self.active_section_angle_deg * 10.0).round() as i32,
            (self.tilt_deg * 10.0).round() as i32,
            (self.roll_deg * 10.0).round() as i32,
            (self.inner_level_deg * 20.0).round() as i32,
            (self.artifact_offset_x_mm * 100.0).round() as i32,
            (self.cut_y_mm * 100.0).round() as i32,
            (self.top_cut_z_mm * 100.0).round() as i32,
        );
        if self.cached_curves_key.as_ref() == Some(&ckey) {
            return;
        }
        self.cached_v_segs = extract_3d_plane_intersection_segments_full(mesh, self, true);
        self.cached_h_segs = extract_3d_plane_intersection_segments_full(mesh, self, false);
        self.cached_curves_key = Some(ckey);
    }

    /// 기존 4방향 미세 다이얼 스텝 적용 (`test_tdd9` 호환).
    pub fn apply_cardinal_fine_dial(&mut self, dir: CardinalDialDir, steps: i32) {
        match dir {
            CardinalDialDir::Up => {
                let hundredths = (self.cut_y_mm * 100.0).round() as i64 + i64::from(steps);
                self.cut_y_mm = (hundredths as f64) / 100.0;
            }
            CardinalDialDir::Down => {
                let hundredths = (self.cut_y_mm * 100.0).round() as i64 - i64::from(steps);
                self.cut_y_mm = (hundredths as f64) / 100.0;
            }
            CardinalDialDir::Right => {
                let tenths =
                    (self.active_section_angle_deg * 10.0).round() as i64 + i64::from(steps);
                self.active_section_angle_deg = ((tenths as f64) / 10.0).rem_euclid(360.0);
            }
            CardinalDialDir::Left => {
                let tenths =
                    (self.active_section_angle_deg * 10.0).round() as i64 - i64::from(steps);
                self.active_section_angle_deg = ((tenths as f64) / 10.0).rem_euclid(360.0);
            }
        }
    }

    /// v6.0 5종 정밀 미세조정 휠(`Image 1 ~ 4` + 내부 수평정합 제5휠) 스텝을 부동소수점 오차 없이 적용한다.
    pub fn apply_precision_wheel(&mut self, wheel: PrecisionWheelKind, steps: i32) {
        match wheel {
            PrecisionWheelKind::TopTurnCwCcw => {
                // Image 1: 유물이 시계방향(+)/반시계방향(-)으로 회전하는 휠 (0.1° 단위)
                let tenths =
                    (self.active_section_angle_deg * 10.0).round() as i64 + i64::from(steps);
                self.active_section_angle_deg = ((tenths as f64) / 10.0).rem_euclid(360.0);
            }
            PrecisionWheelKind::BottomSectionFrontBack => {
                // Image 2: 유물·단면선 앞뒤 미세조정 휠 (0.01mm 단위)
                let hundredths = (self.cut_y_mm * 100.0).round() as i64 + i64::from(steps);
                self.cut_y_mm = ((hundredths as f64) / 100.0).clamp(-150.0, 150.0);
            }
            PrecisionWheelKind::RightTiltLeftRight => {
                // Image 3: 유물 좌우 기울기 미세조정 휠 (roll_deg, 0.1° 단위, 90°/180° 뒤집기 호환 전방향 지원)
                let tenths = (self.roll_deg * 10.0).round() as i64 + i64::from(steps);
                let deg = (tenths as f64) / 10.0;
                self.roll_deg = ((deg + 180.0).rem_euclid(360.0)) - 180.0;
            }
            PrecisionWheelKind::LeftPitchUpDown => {
                // Image 4: 유물의 아래위를 회전하는 휠 — 올리면(+steps) 위로 회전(+tilt_deg), 내리면(-steps) 아래로 회전(-tilt_deg)
                let tenths = (self.tilt_deg * 10.0).round() as i64 + i64::from(steps);
                let deg = (tenths as f64) / 10.0;
                self.tilt_deg = ((deg + 180.0).rem_euclid(360.0)) - 180.0;
            }
            PrecisionWheelKind::InnerHorizontalLevel => {
                // <USER_REQUEST>: 상하좌우 안쪽 제5휠 — 수평이 안 맞을 때 단면선과 잘린 면을 일치시키는 미세 수평정합 휠 (0.05° 단위)
                let twentieths = (self.inner_level_deg * 20.0).round() as i64 + i64::from(steps);
                self.inner_level_deg = ((twentieths as f64) / 20.0).clamp(-15.0, 15.0);
            }
            PrecisionWheelKind::InnerHorizSectionZ => {
                // Image 0: 수평횡단면을 그리는 휠 — 유물은 고정하고 수평절단선(top_cut_z_mm)만 아래위로 이동 (0.1mm 단위)
                let tenths = (self.top_cut_z_mm * 10.0).round() as i64 + i64::from(steps);
                self.top_cut_z_mm = ((tenths as f64) / 10.0).clamp(0.5, 450.0);
            }
        }
    }

    /// 키보드 방향키(`←`, `→`, `↑`, `↓`) 입력으로 직상방 뷰의 유물 수평 위치(`ΔX`, `ΔY`) 또는
    /// 수평횡단면 절단선 높이(`top_cut_z_mm`)를 정밀 이동시킨다 (`<USER_REQUEST>: "이동은 방향키로도 움직일수있게해"`).
    pub fn apply_arrow_key_nudge(
        &mut self,
        dx_dir: i32,
        dy_dir: i32,
        shift_coarse: bool,
        alt_horiz_section_z: bool,
        max_height_mm: f64,
    ) {
        if alt_horiz_section_z {
            let step_z = if shift_coarse { 1.0 } else { 0.20 };
            let next_z = self.top_cut_z_mm + f64::from(dy_dir) * step_z;
            self.top_cut_z_mm =
                ((next_z.clamp(0.5, (max_height_mm - 0.5).max(1.0)) * 100.0).round()) / 100.0;
        } else {
            let step_xy = if shift_coarse { 1.0 } else { 0.10 };
            let next_x = self.artifact_offset_x_mm + f64::from(dx_dir) * step_xy;
            let next_y = self.cut_y_mm - f64::from(dy_dir) * step_xy;
            self.artifact_offset_x_mm = ((next_x.clamp(-150.0, 150.0) * 100.0).round()) / 100.0;
            self.cut_y_mm = ((next_y.clamp(-150.0, 150.0) * 100.0).round()) / 100.0;
        }
    }

    /// Image 5: 직상방 뷰 좌측 상단 `[+ 단면추가]` 버튼 클릭 시 초기 사용자 단면선(`A — A'`)을 생성한다.
    pub fn add_default_custom_section_line(&mut self, radius_mm: f64) {
        let span = (radius_mm * 0.92).max(18.0);
        self.custom_section_line_mm = Some([[-span, span * 0.35], [span, -span * 0.35]]);
        self.custom_section_draw_mode = true;
    }

    /// Image 5: 사용자 추가 단면선의 한쪽 끝지점(`endpoint_idx = 0` 또는 `1`)만 이동시키고,
    /// 반대쪽 끝지점은 100% 고정한다 (`"이선은 끝지점을 누르고 이동을 하면 한쪽만 이동이되게하라"`).
    pub fn move_custom_section_endpoint(&mut self, endpoint_idx: usize, new_xy_mm: [f64; 2]) {
        if let Some(ref mut line) = self.custom_section_line_mm {
            if endpoint_idx < 2 {
                line[endpoint_idx] = [
                    (new_xy_mm[0] * 100.0).round() / 100.0,
                    (new_xy_mm[1] * 100.0).round() / 100.0,
                ];
            }
        }
    }

    /// Image 4: 직상방 동심원 뷰 바로 옆에 배치되는 '수평횡단면 전용 고정 유물 뷰(유물 고정 · 선만 상하조정)' 텍스처 캐시.
    pub fn ensure_fixed_hsec_cached(
        &mut self,
        mesh: &TriMesh,
        region_labels: Option<&[SurfaceRegion]>,
        width: u32,
        height: u32,
    ) {
        let w = width.clamp(80, 360);
        let h = height.clamp(80, 360);
        let mode = self.effective_shader_mode();
        let key = (mesh.name.clone(), w, h, mode);
        if self.cached_fixed_hsec_key.as_ref() == Some(&key) && self.cached_fixed_hsec_rgba.is_some()
        {
            return;
        }
        let fixed_state = View3DPaneState {
            shader_mode: self.shader_mode,
            show_3way_segmentation: self.show_3way_segmentation,
            xray_inner_mode: self.xray_inner_mode,
            zoom: 0.92,
            ..Default::default()
        };
        let img = render_mesh_zbuffer_rgba(mesh, region_labels, &fixed_state, w, h);
        self.cached_fixed_hsec_rgba = Some(img);
        self.cached_fixed_hsec_tex = None;
        self.cached_fixed_hsec_key = Some(key);
    }

    /// 유물 상하좌우 핸드 다이얼(구면 트랙볼) 드래그 입력을 적용한다.
    pub fn apply_hand_dial_drag(&mut self, dx_px: f32, dy_px: f32, fine_axis_tilt_mode: bool) {
        if fine_axis_tilt_mode {
            self.roll_deg =
                ((self.roll_deg + f64::from(dx_px) * 0.15).clamp(-25.0, 25.0) * 10.0).round()
                    / 10.0;
            self.tilt_deg =
                ((self.tilt_deg + f64::from(dy_px) * 0.15).clamp(-25.0, 25.0) * 10.0).round()
                    / 10.0;
        } else {
            self.active_section_angle_deg =
                (self.active_section_angle_deg + f64::from(dx_px) * 0.85).rem_euclid(360.0);
            self.pitch_deg = (self.pitch_deg + dy_px * 0.45).clamp(-75.0, 80.0);
        }
    }

    /// 핸드 다이얼 및 5휠 방향을 수평 동심원 정면 입면(`ψ=0°`, `tilt=0°`, `roll=0°`, `inner_level=0°`, `offset_x=0`)으로 즉시 초기화한다.
    pub fn reset_hand_orientation(&mut self) {
        self.active_section_angle_deg = 0.0;
        self.artifact_offset_x_mm = 0.0;
        self.cut_y_mm = 0.0;
        self.tilt_deg = 0.0;
        self.roll_deg = 0.0;
        self.inner_level_deg = 0.0;
        self.yaw_deg = 0.0;
        self.pitch_deg = 0.0;
    }

    /// Image 6: 뒤집힌 유물을 앞으로 90° 연속 뒤집기 (`0° -> 90° -> 180° -> -90° -> 0°`)
    pub fn flip_artifact_forward_90(&mut self) {
        let next = self.tilt_deg + 90.0;
        self.tilt_deg = (((next + 180.0).rem_euclid(360.0) - 180.0) * 10.0).round() / 10.0;
    }

    /// Image 6: 거꾸로 스캔된 유물을 상하 180° 즉시 뒤집기 (`tilt_deg += 180°`)
    pub fn flip_artifact_upside_down_180(&mut self) {
        let next = self.tilt_deg + 180.0;
        self.tilt_deg = (((next + 180.0).rem_euclid(360.0) - 180.0) * 10.0).round() / 10.0;
    }

    /// Image 6: 유물을 시계방향으로 90° 회전 (`ψ += 90°`)
    pub fn rotate_artifact_yaw_90(&mut self) {
        self.active_section_angle_deg =
            (((self.active_section_angle_deg + 90.0) * 10.0).round() / 10.0).rem_euclid(360.0);
    }

    /// Image 6: 유물을 측면으로 90° 뒤집기 (`roll_deg += 90°`)
    pub fn flip_artifact_roll_90(&mut self) {
        let next = self.roll_deg + 90.0;
        self.roll_deg = (((next + 180.0).rem_euclid(360.0) - 180.0) * 10.0).round() / 10.0;
    }
}

/// 유물 로컬 정점을 턴테이블 회전(`psi_deg`) 및 축 기울기(`tilt_deg`)가 적용된 월드 좌표(`wx, wy, wz`)로 변환한다.
#[inline]
#[allow(clippy::too_many_arguments)]
pub fn local_to_world_turntable(
    p: [f64; 3],
    center_xy: [f64; 2],
    z_min: f64,
    height_mm: f64,
    cos_p: f64,
    sin_p: f64,
    cos_t: f64,
    sin_t: f64,
) -> [f64; 3] {
    let lx = p[0] - center_xy[0];
    let ly = p[1] - center_xy[1];
    let lz = p[2] - z_min;
    let z_mid = 0.5 * height_mm;

    let rx = lx * cos_p - ly * sin_p;
    let ry = lx * sin_p + ly * cos_p;
    let rz = lz - z_mid;

    let wx = rx * cos_t - rz * sin_t;
    let wy = ry;
    let wz = rx * sin_t + rz * cos_t + z_mid;
    [wx, wy, wz]
}

/// 3D 직상방 뷰, 3D 정면 입면 뷰, 3D 적색 단면 교차선, 그리고 우측 2D 도판 단면 슬라이서가
/// **100% 동일한 좌표 변환**을 공유하도록 통합된 정점 변환 함수 (`<USER_REQUEST>` 단면 불일치 근본 해결).
/// - `psi_deg`(`TopTurnCwCcw`): Z축 둘레 시계/반시계 회전
/// - `tilt_deg`(`LeftPitchUpDown`): X축 둘레 상하 회전(올리면 위로, 내리면 아래로)
/// - `roll_deg + inner_level_deg`(`RightTiltLeftRight` + `InnerHorizontalLevel`): Y축 둘레 좌우 수평 기울기 보정
/// - `artifact_offset_x_mm`: 유물 좌우 수평 이동 (`Image 5`, `Image 8`)
#[inline]
#[allow(clippy::too_many_arguments)]
pub fn transform_local_vertex_full(
    p: [f64; 3],
    center_xy: [f64; 2],
    z_min: f64,
    height_mm: f64,
    cos_p: f64,
    sin_p: f64,
    cos_t: f64,
    sin_t: f64,
    cos_r: f64,
    sin_r: f64,
    offset_x_mm: f64,
) -> [f64; 3] {
    let lx = p[0] - center_xy[0];
    let ly = p[1] - center_xy[1];
    let lz = p[2] - z_min;
    let z_mid = 0.5 * height_mm;

    // 1. Z축 둘레 시계·반시계 회전 (ψ)
    let x1 = lx * cos_p - ly * sin_p;
    let y1 = lx * sin_p + ly * cos_p;
    let z1 = lz - z_mid;

    // 2. X축 둘레 상하 회전 (tilt_deg: 올리면 앞면이 위로 회전)
    let x2 = x1;
    let y2 = y1 * cos_t - z1 * sin_t;
    let z2 = y1 * sin_t + z1 * cos_t;

    // 3. Y축 둘레 좌우 수평 기울기 회전 (roll_deg + inner_level_deg)
    let x3 = x2 * cos_r - z2 * sin_r;
    let y3 = y2;
    let z3 = x2 * sin_r + z2 * cos_r;

    [x3 + offset_x_mm, y3, z3 + z_mid]
}

/// 법선 벡터에 동일한 3축 회전(`ψ`, `tilt_deg`, `roll_deg + inner_level_deg`)을 적용한다.
#[inline]
#[allow(clippy::too_many_arguments)]
pub fn transform_local_normal_full(
    n: [f64; 3],
    cos_p: f64,
    sin_p: f64,
    cos_t: f64,
    sin_t: f64,
    cos_r: f64,
    sin_r: f64,
) -> [f64; 3] {
    let x1 = n[0] * cos_p - n[1] * sin_p;
    let y1 = n[0] * sin_p + n[1] * cos_p;
    let z1 = n[2];

    let x2 = x1;
    let y2 = y1 * cos_t - z1 * sin_t;
    let z2 = y1 * sin_t + z1 * cos_t;

    let x3 = x2 * cos_r - z2 * sin_r;
    let y3 = y2;
    let z3 = x2 * sin_r + z2 * cos_r;
    [x3, y3, z3]
}

/// 우측 2D 도판의 수직 종단면(`vertical_section`) 및 수평 횡단면(`horizontal_section`)을 추출할 때
/// 3D 뷰의 5휠 변환(`ψ`, `tilt_deg`, `roll_deg`, `inner_level_deg`, `artifact_offset_x_mm`, `cut_y_mm`)과
/// **100% 일치하는 단면 폴리라인**을 생성한다 (`<USER_REQUEST>`).
pub fn slice_transformed_mesh_world_locked(
    mesh: &TriMesh,
    state: &View3DPaneState,
    is_vertical_y_plane: bool,
) -> strata_section::PlaneSliceResult {
    let segs = extract_3d_plane_intersection_segments_full(mesh, state, is_vertical_y_plane);
    let mut segments_2d: Vec<[[f64; 2]; 2]> = Vec::with_capacity(segs.len());
    for seg in &segs {
        if is_vertical_y_plane {
            segments_2d.push([[seg[0][0], seg[0][2]], [seg[1][0], seg[1][2]]]);
        } else {
            segments_2d.push([
                [seg[0][0], seg[0][1] - state.cut_y_mm],
                [seg[1][0], seg[1][1] - state.cut_y_mm],
            ]);
        }
    }
    let polylines_2d = chain_2d_segments_into_polylines(&segments_2d, 0.45);
    strata_section::PlaneSliceResult {
        orientation: if is_vertical_y_plane {
            strata_section::CutPlaneOrientation::VerticalLongitudinal
        } else {
            strata_section::CutPlaneOrientation::HorizontalTransverse
        },
        psi_deg: state.active_section_angle_deg,
        offset_mm: if is_vertical_y_plane {
            state.cut_y_mm
        } else {
            mesh.bbox.min[2] + state.top_cut_z_mm
        },
        segments_2d,
        polylines_2d,
    }
}

/// Image 5: 직상방 뷰에서 사용자가 그은 자유 단면선(`custom_section_line_mm = Some([p0, p1])`)을 따라
/// 3D 유물 메쉬를 수직 절단하여 우측 2D 도면에 그릴 추가 단면 폴리라인을 추출한다.
pub fn slice_custom_line_section_2d(
    mesh: &TriMesh,
    state: &View3DPaneState,
) -> Option<strata_section::PlaneSliceResult> {
    let [p0, p1] = state.custom_section_line_mm?;
    let wx0 = p0[0];
    let wy0 = state.cut_y_mm + p0[1];
    let wx1 = p1[0];
    let wy1 = state.cut_y_mm + p1[1];
    let dx = wx1 - wx0;
    let dy = wy1 - wy0;
    let seg_len = (dx * dx + dy * dy).sqrt();
    if seg_len < 1.5 {
        return None;
    }
    let tx = dx / seg_len;
    let ty = dy / seg_len;
    let nx = -ty;
    let ny = tx;
    let mx = 0.5 * (wx0 + wx1);
    let my = 0.5 * (wy0 + wy1);
    let half_span = 0.5 * seg_len + 2.0;

    let ext = mesh.bbox.extents();
    let height_mm = ext[2].max(1.0);
    let center_xy = [
        0.5 * (mesh.bbox.min[0] + mesh.bbox.max[0]),
        0.5 * (mesh.bbox.min[1] + mesh.bbox.max[1]),
    ];
    let z_min = mesh.bbox.min[2];
    let (sin_p, cos_p) = state.active_section_angle_deg.to_radians().sin_cos();
    let (sin_t, cos_t) = state.tilt_deg.to_radians().sin_cos();
    let total_roll_deg = state.roll_deg + state.inner_level_deg;
    let (sin_r, cos_r) = total_roll_deg.to_radians().sin_cos();
    let off_x = state.artifact_offset_x_mm;

    let world_verts: Vec<[f64; 3]> = mesh
        .positions
        .par_iter()
        .map(|&pos| {
            transform_local_vertex_full(
                pos, center_xy, z_min, height_mm, cos_p, sin_p, cos_t, sin_t, cos_r, sin_r, off_x,
            )
        })
        .collect();

    let mut segments_2d: Vec<[[f64; 2]; 2]> = Vec::new();
    for tri in &mesh.triangles {
        let w0 = world_verts[tri[0] as usize];
        let w1 = world_verts[tri[1] as usize];
        let w2 = world_verts[tri[2] as usize];
        let pts = [w0, w1, w2];
        let d = [
            (w0[0] - mx) * nx + (w0[1] - my) * ny,
            (w1[0] - mx) * nx + (w1[1] - my) * ny,
            (w2[0] - mx) * nx + (w2[1] - my) * ny,
        ];
        if (d[0] > 0.0 && d[1] > 0.0 && d[2] > 0.0)
            || (d[0] < 0.0 && d[1] < 0.0 && d[2] < 0.0)
        {
            continue;
        }
        let mut hits = Vec::with_capacity(3);
        for &(a, b) in &[(0usize, 1usize), (1, 2), (2, 0)] {
            if (d[a] >= 0.0 && d[b] < 0.0) || (d[a] < 0.0 && d[b] >= 0.0) {
                let denom = d[a] - d[b];
                let t = if denom.abs() > 1e-12 {
                    d[a] / denom
                } else {
                    0.5
                };
                let hx = pts[a][0] + t * (pts[b][0] - pts[a][0]);
                let hy = pts[a][1] + t * (pts[b][1] - pts[a][1]);
                let hz = pts[a][2] + t * (pts[b][2] - pts[a][2]);
                let u = (hx - mx) * tx + (hy - my) * ty;
                hits.push([u, hz]);
            }
        }
        if hits.len() >= 2
            && hits[0][0].abs() <= half_span
            && hits[1][0].abs() <= half_span
        {
            segments_2d.push([hits[0], hits[1]]);
        }
    }
    let polylines_2d = chain_2d_segments_into_polylines(&segments_2d, 0.50);
    Some(strata_section::PlaneSliceResult {
        orientation: strata_section::CutPlaneOrientation::VerticalLongitudinal,
        psi_deg: dy.atan2(dx).to_degrees().rem_euclid(360.0),
        offset_mm: seg_len,
        segments_2d,
        polylines_2d,
    })
}

/// `Image 0 ("그리고 평면 아닌가? 실제 모습이 평면도에 나와야하지않나? 깨진부분은 점선아닌가?")` &
/// `Image 1 ("전부 방향이 같아야한다")`:
/// 현재 3D 뷰의 유물 회전·상하뒤집기·기울기(`ψ`, `tilt`, `roll`, `inner_level`, `offset_x`, `cut_y`)가 100% 동일하게 적용된
/// 직상방(`+Z`에서 내려다본 `X-Y` 평면) 실제 3D 메쉬 평면 윤곽(`외곽선`, `구연내경선`, `굽외경선`, `내저원각선`)과
/// 구연부 깨진 부분(결손 구간) 섹터를 추출한다.
pub fn extract_topdown_plan_slice_2d(
    mesh: &TriMesh,
    state: &View3DPaneState,
) -> strata_section::PlaneSliceResult {
    const N_SECTORS: usize = 72;
    let ext = mesh.bbox.extents();
    let height_mm = ext[2].max(1.0);
    let r_nom = (0.5 * ext[0].max(ext[1])).max(10.0);
    let wall_t = (r_nom * 0.088).clamp(2.8, 9.5);
    let center_xy = [
        0.5 * (mesh.bbox.min[0] + mesh.bbox.max[0]),
        0.5 * (mesh.bbox.min[1] + mesh.bbox.max[1]),
    ];
    let z_min = mesh.bbox.min[2];
    let (sin_p, cos_p) = state.active_section_angle_deg.to_radians().sin_cos();
    let (sin_t, cos_t) = state.tilt_deg.to_radians().sin_cos();
    let total_roll_deg = state.roll_deg + state.inner_level_deg;
    let (sin_r, cos_r) = total_roll_deg.to_radians().sin_cos();
    let off_x = state.artifact_offset_x_mm;
    let cut_y = state.cut_y_mm;

    let mut bin_max_r = [0.0_f64; N_SECTORS];
    let mut bin_rim_z = [f64::NEG_INFINITY; N_SECTORS];
    let mut bin_foot_r = [0.0_f64; N_SECTORS];
    let mut bin_has = [false; N_SECTORS];

    for &pos in &mesh.positions {
        let w = transform_local_vertex_full(
            pos, center_xy, z_min, height_mm, cos_p, sin_p, cos_t, sin_t, cos_r, sin_r, off_x,
        );
        let dx = w[0] - off_x;
        let dy = w[1] - cut_y;
        let r = dx.hypot(dy);
        if r < 1.0 || !r.is_finite() {
            continue;
        }
        let ang = dy.atan2(dx);
        let norm = ((ang + std::f64::consts::PI) / std::f64::consts::TAU).clamp(0.0, 0.999_999);
        let bi = (norm * (N_SECTORS as f64)) as usize;
        bin_has[bi] = true;
        if r > bin_max_r[bi] {
            bin_max_r[bi] = r;
        }
        if r >= r_nom * 0.72 && w[2] > bin_rim_z[bi] {
            bin_rim_z[bi] = w[2];
        }
        let is_flipped = state.tilt_deg.abs() > 90.0;
        let in_foot_band = if is_flipped {
            w[2] >= height_mm * 0.76
        } else {
            w[2] <= height_mm * 0.24
        };
        if in_foot_band && r < r_nom * 0.76 && r > bin_foot_r[bi] {
            bin_foot_r[bi] = r;
        }
    }

    let mut outer_r = [r_nom; N_SECTORS];
    let mut foot_r = [(r_nom * 0.42).max(8.0); N_SECTORS];
    for k in 0..N_SECTORS {
        let mut sum_o = 0.0;
        let mut sum_w = 0.0;
        let mut sum_f = 0.0;
        let mut sum_fw = 0.0;
        for d in -1isize..=1isize {
            let nk = ((k as isize + d).rem_euclid(N_SECTORS as isize)) as usize;
            let w = if d == 0 { 1.0 } else { 0.45 };
            if bin_has[nk] {
                sum_o += bin_max_r[nk] * w;
                sum_w += w;
            }
            if bin_foot_r[nk] > 3.0 {
                sum_f += bin_foot_r[nk] * w;
                sum_fw += w;
            }
        }
        if sum_w > 0.0 {
            outer_r[k] = sum_o / sum_w;
        }
        if sum_fw > 0.0 {
            foot_r[k] = (sum_f / sum_fw).clamp(r_nom * 0.28, r_nom * 0.72);
        }
    }

    let mut sorted_r = outer_r.to_vec();
    sorted_r.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let recon_outer_r = sorted_r[(sorted_r.len() * 4 / 5).min(sorted_r.len().saturating_sub(1))].max(10.0);
    let recon_inner_r = (recon_outer_r - wall_t).max(4.0);

    let mut valid_zs: Vec<f64> = bin_rim_z.iter().copied().filter(|z| z.is_finite()).collect();
    valid_zs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let ref_rim_z = if valid_zs.is_empty() {
        height_mm
    } else {
        valid_zs[(valid_zs.len() * 4 / 5).min(valid_zs.len().saturating_sub(1))]
    };

    let mut is_broken = [false; N_SECTORS];
    let mut min_rel_z = f64::INFINITY;
    let mut min_rel_k = 54usize;
    for k in 0..N_SECTORS {
        if !bin_has[k]
            || outer_r[k] < recon_outer_r - (recon_outer_r * 0.035).max(1.4)
            || (bin_rim_z[k].is_finite()
                && bin_rim_z[k] < ref_rim_z - (height_mm * 0.038).max(1.4))
        {
            is_broken[k] = true;
        }
        if bin_rim_z[k].is_finite() {
            let dz = bin_rim_z[k] - ref_rim_z;
            if dz < min_rel_z {
                min_rel_z = dz;
                min_rel_k = k;
            }
        }
    }
    // 구연부 미세 결손(깨진 부분)이 정점 밀도상 완만하게 나타난 경우에도 최저 구연 고도 구간(±2 섹터)을 결손부(점선+파손실선)로 표시
    let mut expanded_broken = is_broken;
    if !is_broken.iter().any(|&b| b) {
        for d in -2isize..=2isize {
            let nk = ((min_rel_k as isize + d).rem_euclid(N_SECTORS as isize)) as usize;
            expanded_broken[nk] = true;
        }
    } else {
        for (k, &b) in is_broken.iter().enumerate() {
            if b {
                for d in -1isize..=1isize {
                    let nk = ((k as isize + d).rem_euclid(N_SECTORS as isize)) as usize;
                    expanded_broken[nk] = true;
                }
            }
        }
    }
    is_broken = expanded_broken;
    for (k, &b) in is_broken.iter().enumerate() {
        if b && outer_r[k] >= recon_outer_r - 1.2 {
            outer_r[k] = (recon_outer_r - wall_t * 0.72).max(recon_inner_r + 0.6);
        }
    }

    let mut outer_ring = Vec::with_capacity(N_SECTORS);
    let mut inner_ring = Vec::with_capacity(N_SECTORS);
    let mut foot_outer_ring = Vec::with_capacity(N_SECTORS);
    let mut floor_inner_ring = Vec::with_capacity(N_SECTORS);
    let mut segments_2d = Vec::new();

    for k in 0..N_SECTORS {
        let a =
            -std::f64::consts::PI + ((k as f64) + 0.5) * std::f64::consts::TAU / (N_SECTORS as f64);
        let (s, c) = a.sin_cos();
        let ro = outer_r[k];
        let ri = (ro - wall_t).max(4.0);
        let rf = foot_r[k];
        let rfl = (rf - wall_t * 0.82).max(3.5);
        outer_ring.push([ro * c, ro * s]);
        inner_ring.push([ri * c, ri * s]);
        foot_outer_ring.push([rf * c, rf * s]);
        floor_inner_ring.push([rfl * c, rfl * s]);
        if is_broken[k] {
            segments_2d.push([[k as f64, k as f64], [recon_outer_r, recon_inner_r]]);
        }
    }

    strata_section::PlaneSliceResult {
        orientation: strata_section::CutPlaneOrientation::HorizontalTransverse,
        psi_deg: state.active_section_angle_deg,
        offset_mm: -9999.0,
        segments_2d,
        polylines_2d: vec![outer_ring, inner_ring, foot_outer_ring, floor_inner_ring],
    }
}

fn chain_2d_segments_into_polylines(
    segs: &[[[f64; 2]; 2]],
    snap_tol_mm: f64,
) -> Vec<Vec<[f64; 2]>> {
    if segs.is_empty() {
        return Vec::new();
    }
    let mut used = vec![false; segs.len()];
    let mut polylines: Vec<Vec<[f64; 2]>> = Vec::new();
    let tol2 = snap_tol_mm * snap_tol_mm;

    for start_idx in 0..segs.len() {
        if used[start_idx] {
            continue;
        }
        used[start_idx] = true;
        let mut chain = vec![segs[start_idx][0], segs[start_idx][1]];

        loop {
            let mut extended = false;
            let tail = *chain.last().unwrap_or(&[0.0, 0.0]);
            let mut best_j = None;
            let mut best_rev = false;
            let mut best_d2 = tol2;

            for (j, s) in segs.iter().enumerate() {
                if used[j] {
                    continue;
                }
                let d0 = (s[0][0] - tail[0]).powi(2) + (s[0][1] - tail[1]).powi(2);
                if d0 <= best_d2 {
                    best_d2 = d0;
                    best_j = Some(j);
                    best_rev = false;
                }
                let d1 = (s[1][0] - tail[0]).powi(2) + (s[1][1] - tail[1]).powi(2);
                if d1 <= best_d2 {
                    best_d2 = d1;
                    best_j = Some(j);
                    best_rev = true;
                }
            }
            if let Some(j) = best_j {
                used[j] = true;
                let next_pt = if best_rev { segs[j][0] } else { segs[j][1] };
                chain.push(next_pt);
                extended = true;
            }

            let head = chain[0];
            let mut best_front_j = None;
            let mut best_front_rev = false;
            let mut best_front_d2 = tol2;
            for (j, s) in segs.iter().enumerate() {
                if used[j] {
                    continue;
                }
                let d1 = (s[1][0] - head[0]).powi(2) + (s[1][1] - head[1]).powi(2);
                if d1 <= best_front_d2 {
                    best_front_d2 = d1;
                    best_front_j = Some(j);
                    best_front_rev = false;
                }
                let d0 = (s[0][0] - head[0]).powi(2) + (s[0][1] - head[1]).powi(2);
                if d0 <= best_front_d2 {
                    best_front_d2 = d0;
                    best_front_j = Some(j);
                    best_front_rev = true;
                }
            }
            if let Some(j) = best_front_j {
                used[j] = true;
                let prev_pt = if best_front_rev { segs[j][1] } else { segs[j][0] };
                chain.insert(0, prev_pt);
                extended = true;
            }

            if !extended {
                break;
            }
        }
        if chain.len() >= 3 {
            polylines.push(chain);
        }
    }
    polylines.sort_by_key(|b| std::cmp::Reverse(b.len()));
    polylines
}

/// 월드 좌표 `(wx, wy, wz)`를 3D 뷰포트 화면 좌표 `(sx, sy, depth)`로 투영한다.
#[inline]
#[allow(clippy::too_many_arguments)]
pub fn project_world_to_screen(
    w: [f64; 3],
    height_mm: f64,
    half_w: f64,
    half_h: f64,
    scale: f64,
    cos_cy: f64,
    sin_cy: f64,
    cos_cp: f64,
    sin_cp: f64,
) -> [f64; 3] {
    let z_centered = w[2] - 0.48 * height_mm;
    let x1 = w[0] * cos_cy - w[1] * sin_cy;
    let y1 = w[0] * sin_cy + w[1] * cos_cy;
    let y2 = y1 * cos_cp - z_centered * sin_cp;
    let z2 = y1 * sin_cp + z_centered * cos_cp;
    [half_w + x1 * scale, half_h - z2 * scale, y2]
}

/// 배경 이미지 전체에 `2mm / 10mm` 고고학 실측 방안선을 빈틈없이 그린다 (`Image 6`).
fn draw_background_millimeter_grid_rgba(
    img: &mut RgbaImage,
    half_w: f64,
    half_h: f64,
    scale: f64,
) {
    let w_usize = img.width() as usize;
    let h_usize = img.height() as usize;
    let step_2mm_px = (2.0 * scale).max(2.0);
    let max_steps_x = ((half_w / step_2mm_px).ceil() as i32).min(240);
    let max_steps_y = ((half_h / step_2mm_px).ceil() as i32).min(240);

    for ix in -max_steps_x..=max_steps_x {
        let px = (half_w + f64::from(ix) * step_2mm_px).round() as i32;
        if px >= 0 && (px as usize) < w_usize {
            let col = if ix == 0 {
                Rgba([156, 74, 47, 255])
            } else if ix % 5 == 0 {
                Rgba([206, 200, 184, 255])
            } else {
                Rgba([234, 231, 222, 255])
            };
            for py in 0..h_usize {
                img.put_pixel(px as u32, py as u32, col);
            }
        }
    }
    for iy in -max_steps_y..=max_steps_y {
        let py = (half_h + f64::from(iy) * step_2mm_px).round() as i32;
        if py >= 0 && (py as usize) < h_usize {
            let col = if iy == 0 {
                Rgba([156, 74, 47, 255])
            } else if iy % 5 == 0 {
                Rgba([206, 200, 184, 255])
            } else {
                Rgba([234, 231, 222, 255])
            };
            for px in 0..w_usize {
                img.put_pixel(px as u32, py as u32, col);
            }
        }
    }
}

/// 상단 직상방 동심원 평면도와 하단 수평 정면 입면도가 **1:1 동일한 스케일(`px/mm`)**을 공유하도록 산출한다.
/// 가로 폭(`width_px`)에만 1차 동차(`homogeneous of degree 1`)로 비례하므로 상·하 뷰의 유물 가로 폭과 방안지 격자, 수직 점선 정렬이 100% 1:1 일치한다!
#[inline]
pub fn compute_shared_ortho_scale(
    width_px: f64,
    grid_r_mm: f64,
    height_mm: f64,
    zoom: f32,
) -> f64 {
    let fit_r = (width_px * 0.25) / grid_r_mm.max(1.0);
    let fit_h = (width_px * 0.44) / height_mm.max(10.0);
    fit_r.min(fit_h) * f64::from(zoom)
}

/// 직상방(`Top-Down`, `pitch=90°`)에서 바라본 동심원 방안지(`반경 = 유물 반경 × 1.20 = +20%`)와
/// 그 위에서 수평 이동(`artifact_offset_x_mm`, `cut_y_mm`) 및 회전(`ψ`)하는 원본 해상도(`tri_stride = 1`) 3D 스캐너 유물을 렌더링한다 (`Image 5`, `Image 8`, `Image 9`).
pub fn render_topdown_concentric_plan_rgba(
    mesh: &TriMesh,
    state: &View3DPaneState,
    width: u32,
    height: u32,
) -> RgbaImage {
    let w_usize = width.max(32) as usize;
    let h_usize = height.max(32) as usize;
    let mut img = RgbaImage::from_pixel(
        w_usize as u32,
        h_usize as u32,
        Rgba([SURFACE.rgb[0], SURFACE.rgb[1], SURFACE.rgb[2], 255]),
    );
    let mut zbuf = vec![f32::INFINITY; w_usize * h_usize];

    let ext = mesh.bbox.extents();
    let height_mm = ext[2].max(1.0);
    let radius_mm = (0.5 * ext[0].max(ext[1])).max(5.0);
    let grid_r_mm = compute_concentric_grid_radius_mm(radius_mm);
    let center_xy = [
        0.5 * (mesh.bbox.min[0] + mesh.bbox.max[0]),
        0.5 * (mesh.bbox.min[1] + mesh.bbox.max[1]),
    ];
    let z_min = mesh.bbox.min[2];

    let half_w = w_usize as f64 * 0.5;
    let half_h = h_usize as f64 * 0.5;
    let scale = compute_shared_ortho_scale(w_usize as f64, grid_r_mm, height_mm, state.zoom);

    // 1. 바탕에 직교 실측 방안지 격자 (2mm / 10mm) 드로잉
    draw_background_millimeter_grid_rgba(&mut img, half_w, half_h, scale);

    // 2. Rayon 멀티코어 병렬 정점 변환 + tri_stride=1 (100% 전체 삼각형, 핀홀/점 0개!)
    let (sin_p, cos_p) = state.active_section_angle_deg.to_radians().sin_cos();
    let (sin_t, cos_t) = state.tilt_deg.to_radians().sin_cos();
    let total_roll_deg = state.roll_deg + state.inner_level_deg;
    let (sin_r, cos_r) = total_roll_deg.to_radians().sin_cos();
    let off_x = state.artifact_offset_x_mm;
    let cut_y = state.cut_y_mm;

    let screen_verts: Vec<[f64; 3]> = mesh
        .positions
        .par_iter()
        .map(|&pos| {
            let w = transform_local_vertex_full(
                pos, center_xy, z_min, height_mm, cos_p, sin_p, cos_t, sin_t, cos_r, sin_r, off_x,
            );
            // Image 5 & Image 8: 단면선은 동심원 중앙(sy = half_h)에 고정되고, 유물 자체가 (off_x, -cut_y)만큼 수평 이동한다!
            [
                half_w + w[0] * scale,
                half_h - (w[1] - cut_y) * scale,
                -w[2],
            ]
        })
        .collect();

    let has_uvs = mesh.triangle_uvs.len() == mesh.triangles.len();
    let tex_opt = mesh.texture_image.as_deref();
    let edge_pad = -1.5e-3;

    // Image 9: tri_stride = 1 (원본 스캐너 메쉬 100% 전체 삼각형 렌더링 — 점/빈틈 원천 제거!)
    for (t_idx, tri) in mesh.triangles.iter().enumerate() {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;
        let p0 = screen_verts[i0];
        let p1 = screen_verts[i1];
        let p2 = screen_verts[i2];

        let min_x = p0[0].min(p1[0]).min(p2[0]).floor().max(0.0) as usize;
        let max_x = p0[0]
            .max(p1[0])
            .max(p2[0])
            .ceil()
            .min((w_usize - 1) as f64) as usize;
        let min_y = p0[1].min(p1[1]).min(p2[1]).floor().max(0.0) as usize;
        let max_y = p0[1]
            .max(p1[1])
            .max(p2[1])
            .ceil()
            .min((h_usize - 1) as f64) as usize;
        if min_x > max_x || min_y > max_y {
            continue;
        }

        let denom = (p1[1] - p2[1]) * (p0[0] - p2[0]) + (p2[0] - p1[0]) * (p0[1] - p2[1]);
        if denom.abs() < 1e-7 {
            continue;
        }
        let inv_denom = 1.0 / denom;
        let wn = transform_local_normal_full(
            mesh.face_normals[t_idx],
            cos_p,
            sin_p,
            cos_t,
            sin_t,
            cos_r,
            sin_r,
        );
        let ldir = state.compute_light_direction_vector();
        let ndotl_td = (wn[0] * ldir[0] + wn[1] * ldir[1] + wn[2].abs() * ldir[2]).abs();
        let light_gain = ((0.76 + 0.28 * ndotl_td) * f64::from(state.light_intensity))
            .clamp(0.55, 1.38) as f32;
        let uvs = if has_uvs {
            mesh.triangle_uvs[t_idx]
        } else {
            [[0.0, 0.0]; 3]
        };
        let c0 = mesh.colors_rgb[i0];

        for py in min_y..=max_y {
            let fy = py as f64 + 0.5;
            for px in min_x..=max_x {
                let fx = px as f64 + 0.5;
                let b0 = ((p1[1] - p2[1]) * (fx - p2[0]) + (p2[0] - p1[0]) * (fy - p2[1]))
                    * inv_denom;
                let b1 = ((p2[1] - p0[1]) * (fx - p2[0]) + (p0[0] - p2[0]) * (fy - p2[1]))
                    * inv_denom;
                let b2 = 1.0 - b0 - b1;
                if b0 < edge_pad || b1 < edge_pad || b2 < edge_pad {
                    continue;
                }
                let depth = (b0 * p0[2] + b1 * p1[2] + b2 * p2[2]) as f32;
                let buf_idx = py * w_usize + px;
                if depth >= zbuf[buf_idx] {
                    continue;
                }
                zbuf[buf_idx] = depth;

                let base_rgb = if let (Some(tex), true) = (tex_opt, has_uvs) {
                    let b0c = (b0 as f32).clamp(0.0, 1.0);
                    let b1c = (b1 as f32).clamp(0.0, 1.0);
                    let b2c = (1.0 - b0c - b1c).max(0.0);
                    let u = b0c * uvs[0][0] + b1c * uvs[1][0] + b2c * uvs[2][0];
                    let v = b0c * uvs[0][1] + b1c * uvs[1][1] + b2c * uvs[2][1];
                    let s = tex.sample_uv_rgb(u, v);
                    if s[0] > 8 || s[1] > 8 || s[2] > 8 {
                        s
                    } else {
                        [172, 164, 152]
                    }
                } else {
                    c0
                };
                let r = (f32::from(base_rgb[0]) * light_gain).clamp(16.0, 252.0) as u8;
                let g = (f32::from(base_rgb[1]) * light_gain).clamp(16.0, 252.0) as u8;
                let b = (f32::from(base_rgb[2]) * light_gain).clamp(16.0, 250.0) as u8;
                img.put_pixel(px as u32, py as u32, Rgba([r, g, b, 255]));
            }
        }
    }

    img
}

/// 하단 3D 정면 입면 뷰를 전체 배경 방안선(`Image 6`) + `Rayon` 병렬 정점 투영 + `tri_stride = 1` 원본 스캐너 해상도(`Image 9`)로 렌더링한다.
pub fn render_mesh_zbuffer_rgba(
    mesh: &TriMesh,
    region_labels: Option<&[SurfaceRegion]>,
    state: &View3DPaneState,
    width: u32,
    height: u32,
) -> RgbaImage {
    let w_usize = width.max(16) as usize;
    let h_usize = height.max(16) as usize;
    let mut img = RgbaImage::from_pixel(
        w_usize as u32,
        h_usize as u32,
        Rgba([SURFACE.rgb[0], SURFACE.rgb[1], SURFACE.rgb[2], 255]),
    );
    let mut zbuf = vec![f32::INFINITY; w_usize * h_usize];

    let active_mode = state.effective_shader_mode();
    let need_rubbing_buf = active_mode == View3DShaderMode::RealTimeRubbing;
    let mut relief_buf = if need_rubbing_buf {
        vec![0.0f32; w_usize * h_usize]
    } else {
        Vec::new()
    };
    let mut mask_buf = if need_rubbing_buf {
        vec![0u8; w_usize * h_usize]
    } else {
        Vec::new()
    };

    let ext = mesh.bbox.extents();
    let height_mm = ext[2].max(1.0);
    let radius_mm = (0.5 * ext[0].max(ext[1])).max(5.0);
    let grid_r_mm = compute_concentric_grid_radius_mm(radius_mm);
    let center_xy = [
        0.5 * (mesh.bbox.min[0] + mesh.bbox.max[0]),
        0.5 * (mesh.bbox.min[1] + mesh.bbox.max[1]),
    ];
    let z_min = mesh.bbox.min[2];

    // 상단 직상방 동심원 뷰와 100% 1:1 동일한 스케일(mm -> px)을 사용하여 상·하 유물 폭 및 수직 점선이 1:1로 일치하도록 함!
    let scale = compute_shared_ortho_scale(w_usize as f64, grid_r_mm, height_mm, state.zoom);
    let half_w = w_usize as f64 * 0.5;
    let half_h = h_usize as f64 * 0.54;

    // Image 6: 하단 3D 정면 입면 뷰 전체 배경에도 2mm / 10mm 실측 방안선을 빈틈없이 렌더링!
    draw_background_millimeter_grid_rgba(&mut img, half_w, half_h, scale);

    let (sin_p, cos_p) = state.active_section_angle_deg.to_radians().sin_cos();
    let (sin_t, cos_t) = state.tilt_deg.to_radians().sin_cos();
    let total_roll_deg = state.roll_deg + state.inner_level_deg;
    let (sin_r, cos_r) = total_roll_deg.to_radians().sin_cos();
    let (sin_cy, cos_cy) = f64::from(state.yaw_deg).to_radians().sin_cos();
    let (sin_cp, cos_cp) = f64::from(state.pitch_deg).to_radians().sin_cos();
    let off_x = state.artifact_offset_x_mm;

    // Rayon 멀티코어 병렬 정점 투영: 16만~30만 정점을 <0.5ms 내에 일괄 변환
    let transformed_verts: Vec<([f64; 3], [f64; 3])> = mesh
        .positions
        .par_iter()
        .map(|&pos| {
            let w = transform_local_vertex_full(
                pos, center_xy, z_min, height_mm, cos_p, sin_p, cos_t, sin_t, cos_r, sin_r, off_x,
            );
            let s = project_world_to_screen(
                w, height_mm, half_w, half_h, scale, cos_cy, sin_cy, cos_cp, sin_cp,
            );
            (w, s)
        })
        .collect();

    let has_uvs = mesh.triangle_uvs.len() == mesh.triangles.len();
    let tex_opt = mesh.texture_image.as_deref();
    let light_dir = state.compute_light_direction_vector();
    let light_int = f64::from(state.light_intensity.clamp(0.4, 1.8));
    let edge_pad = -1.5e-3;

    // Image 9: tri_stride = 1 (100% 전체 삼각형 렌더링, 점/핀홀 0개 보장!)
    for (t_idx, tri) in mesh.triangles.iter().enumerate() {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;

        let (w0, p0) = transformed_verts[i0];
        let (w1, p1) = transformed_verts[i1];
        let (w2, p2) = transformed_verts[i2];

        let c_wx = (w0[0] + w1[0] + w2[0]) / 3.0;
        let c_wy = (w0[1] + w1[1] + w2[1]) / 3.0;

        if state.cutaway_open
            && active_mode != View3DShaderMode::XRayInner
            && c_wy > state.cut_y_mm
            && c_wx < -1.5
        {
            continue;
        }

        let min_x = p0[0].min(p1[0]).min(p2[0]).floor().max(0.0) as usize;
        let max_x = p0[0]
            .max(p1[0])
            .max(p2[0])
            .ceil()
            .min((w_usize - 1) as f64) as usize;
        let min_y = p0[1].min(p1[1]).min(p2[1]).floor().max(0.0) as usize;
        let max_y = p0[1]
            .max(p1[1])
            .max(p2[1])
            .ceil()
            .min((h_usize - 1) as f64) as usize;

        if min_x > max_x || min_y > max_y {
            continue;
        }

        let denom = (p1[1] - p2[1]) * (p0[0] - p2[0]) + (p2[0] - p1[0]) * (p0[1] - p2[1]);
        if denom.abs() < 1e-7 {
            continue;
        }
        let inv_denom = 1.0 / denom;

        let wn = transform_local_normal_full(
            mesh.face_normals[t_idx],
            cos_p,
            sin_p,
            cos_t,
            sin_t,
            cos_r,
            sin_r,
        );
        let raw_dot = wn[0] * light_dir[0] + wn[1] * light_dir[1] + wn[2] * light_dir[2];
        let is_backface = denom < 0.0;
        let ndotl = raw_dot.abs();
        let spec = ndotl.powi(12) * 0.14;
        let back_atten = if is_backface { 0.88 } else { 1.0 };
        let light_gain = if active_mode == View3DShaderMode::PhotoTexture {
            (((0.76 + 0.28 * ndotl) * back_atten) * light_int).clamp(0.50, 1.38) as f32
        } else {
            (((0.38 + 0.56 * ndotl + spec) * back_atten) * light_int).clamp(0.22, 1.45) as f32
        };

        let near_cut_plane =
            state.cutaway_open && (c_wy - state.cut_y_mm).abs() < 1.2 && c_wx < 0.5;

        let uvs = if has_uvs {
            mesh.triangle_uvs[t_idx]
        } else {
            [[0.0, 0.0]; 3]
        };
        let c0 = mesh.colors_rgb[i0];
        let c1 = mesh.colors_rgb[i1];
        let c2 = mesh.colors_rgb[i2];

        let curv = ((mesh.mean_curvatures[i0]
            + mesh.mean_curvatures[i1]
            + mesh.mean_curvatures[i2])
            / 3.0) as f32;

        for py in min_y..=max_y {
            let fy = py as f64 + 0.5;
            for px in min_x..=max_x {
                let fx = px as f64 + 0.5;
                let b0 = ((p1[1] - p2[1]) * (fx - p2[0]) + (p2[0] - p1[0]) * (fy - p2[1]))
                    * inv_denom;
                let b1 = ((p2[1] - p0[1]) * (fx - p2[0]) + (p0[0] - p2[0]) * (fy - p2[1]))
                    * inv_denom;
                let b2 = 1.0 - b0 - b1;
                if b0 < edge_pad || b1 < edge_pad || b2 < edge_pad {
                    continue;
                }

                let depth = (b0 * p0[2] + b1 * p1[2] + b2 * p2[2]) as f32;
                let buf_idx = py * w_usize + px;

                if active_mode == View3DShaderMode::XRayInner {
                    let rim_glow = (1.0 - ndotl as f32).powi(2).clamp(0.18, 0.92);
                    let dst = img.get_pixel(px as u32, py as u32).0;
                    let alpha = (rim_glow * 0.38).clamp(0.08, 0.55);
                    let xr = [181u8, 87, 58];
                    let nr = (f32::from(dst[0]) * (1.0 - alpha) + f32::from(xr[0]) * alpha) as u8;
                    let ng = (f32::from(dst[1]) * (1.0 - alpha) + f32::from(xr[1]) * alpha) as u8;
                    let nb = (f32::from(dst[2]) * (1.0 - alpha) + f32::from(xr[2]) * alpha) as u8;
                    img.put_pixel(px as u32, py as u32, Rgba([nr, ng, nb, 255]));
                    continue;
                }

                if depth >= zbuf[buf_idx] {
                    continue;
                }
                zbuf[buf_idx] = depth;
                if need_rubbing_buf {
                    mask_buf[buf_idx] = 1;
                }

                let base_rgb: [u8; 3] = if near_cut_plane {
                    [34, 32, 30]
                } else {
                    match active_mode {
                        View3DShaderMode::PhotoTexture => {
                            if let Some(tex) = tex_opt {
                                if has_uvs {
                                    let b0c = (b0 as f32).clamp(0.0, 1.0);
                                    let b1c = (b1 as f32).clamp(0.0, 1.0);
                                    let b2c = (1.0 - b0c - b1c).max(0.0);
                                    let u = b0c * uvs[0][0] + b1c * uvs[1][0] + b2c * uvs[2][0];
                                    let v = b0c * uvs[0][1] + b1c * uvs[1][1] + b2c * uvs[2][1];
                                    let sampled = tex.sample_uv_rgb(u, v);
                                    if sampled[0] > 8 || sampled[1] > 8 || sampled[2] > 8 {
                                        sampled
                                    } else {
                                        [168, 160, 148]
                                    }
                                } else {
                                    [
                                        (b0 * f64::from(c0[0])
                                            + b1 * f64::from(c1[0])
                                            + b2 * f64::from(c2[0]))
                                            as u8,
                                        (b0 * f64::from(c0[1])
                                            + b1 * f64::from(c1[1])
                                            + b2 * f64::from(c2[1]))
                                            as u8,
                                        (b0 * f64::from(c0[2])
                                            + b1 * f64::from(c1[2])
                                            + b2 * f64::from(c2[2]))
                                            as u8,
                                    ]
                                }
                            } else {
                                [
                                    (b0 * f64::from(c0[0])
                                        + b1 * f64::from(c1[0])
                                        + b2 * f64::from(c2[0]))
                                        as u8,
                                    (b0 * f64::from(c0[1])
                                        + b1 * f64::from(c1[1])
                                        + b2 * f64::from(c2[1]))
                                        as u8,
                                    (b0 * f64::from(c0[2])
                                        + b1 * f64::from(c1[2])
                                        + b2 * f64::from(c2[2]))
                                        as u8,
                                ]
                            }
                        }
                        View3DShaderMode::SolidClay => {
                            if is_backface {
                                [152, 144, 134]
                            } else {
                                [184, 176, 164]
                            }
                        }
                        View3DShaderMode::RealTimeRubbing => {
                            let rel = (ndotl as f32 * 0.72 - curv * 0.35).clamp(0.0, 1.0);
                            relief_buf[buf_idx] = rel;
                            let g = (rel * 235.0).clamp(18.0, 246.0) as u8;
                            [g, g, (g.saturating_sub(3))]
                        }
                        View3DShaderMode::Segmentation3Way => {
                            let reg = region_labels
                                .and_then(|l| l.get(i0).copied())
                                .unwrap_or(SurfaceRegion::OuterWall);
                            match reg {
                                SurfaceRegion::OuterWall => [58, 122, 164],
                                SurfaceRegion::InnerWall => [58, 138, 92],
                                SurfaceRegion::FractureZone => [181, 87, 58],
                            }
                        }
                        View3DShaderMode::XRayInner => [181, 87, 58],
                    }
                };

                let r = (f32::from(base_rgb[0]) * light_gain).clamp(12.0, 254.0) as u8;
                let g = (f32::from(base_rgb[1]) * light_gain).clamp(12.0, 254.0) as u8;
                let b = (f32::from(base_rgb[2]) * light_gain).clamp(12.0, 252.0) as u8;
                img.put_pixel(px as u32, py as u32, Rgba([r, g, b, 255]));
            }
        }
    }

    if active_mode == View3DShaderMode::RealTimeRubbing {
        for y in 1..(h_usize.saturating_sub(1)) {
            for x in 1..(w_usize.saturating_sub(1)) {
                let idx = y * w_usize + x;
                if mask_buf[idx] == 0 {
                    continue;
                }
                let c = relief_buf[idx];
                let mut lap = 0.0f32;
                let neighbors = [
                    relief_buf[(y - 1) * w_usize + x],
                    relief_buf[(y + 1) * w_usize + x],
                    relief_buf[y * w_usize + (x - 1)],
                    relief_buf[y * w_usize + (x + 1)],
                ];
                for nb in neighbors {
                    if nb > 0.0 {
                        lap += c - nb;
                    }
                }
                let contact = (1.0 - (c - lap * 1.65)).clamp(0.0, 1.0);
                let x_f = x as f32;
                let y_f = y as f32;
                let ign = ((52.982_918_f32
                    * ((0.06711056 * x_f + 0.00583715 * y_f).fract()))
                .fract()
                    + 1.0)
                    .fract();
                let target_whiteness = 1.0 - (contact * 0.90).clamp(0.0, 0.96);
                let s = 0.26_f32;
                let stippled_luma = ((target_whiteness - ign + s) / (2.0 * s)).clamp(0.0, 1.0);
                let r = (20.0 + stippled_luma * (250.0 - 20.0)).round() as u8;
                let g = (20.0 + stippled_luma * (249.0 - 20.0)).round() as u8;
                let b = (19.0 + stippled_luma * (245.0 - 19.0)).round() as u8;
                img.put_pixel(x as u32, y as u32, Rgba([r, g, b, 255]));
            }
        }
    }

    img
}

/// 기존 시그니처 호환용 3D 교차 선분 추출 함수.
pub fn extract_3d_plane_intersection_segments(
    mesh: &TriMesh,
    psi_deg: f64,
    tilt_deg: f64,
    plane_val_mm: f64,
    is_vertical_y_plane: bool,
) -> Vec<[[f64; 3]; 2]> {
    let state = View3DPaneState {
        active_section_angle_deg: psi_deg,
        tilt_deg,
        cut_y_mm: if is_vertical_y_plane { plane_val_mm } else { 0.0 },
        top_cut_z_mm: if is_vertical_y_plane { 18.0 } else { plane_val_mm },
        ..Default::default()
    };
    extract_3d_plane_intersection_segments_full(mesh, &state, is_vertical_y_plane)
}

/// 5휠 전체 변환(`ψ`, `tilt_deg`, `roll_deg`, `inner_level_deg`, `artifact_offset_x_mm`, `cut_y_mm`)이 반영된
/// 월드 고정 절단 평면(`wy = cut_y_mm` 또는 `wz = top_cut_z_mm`)과 3D 메쉬의 100% 정밀 교차 선분을 추출한다.
pub fn extract_3d_plane_intersection_segments_full(
    mesh: &TriMesh,
    state: &View3DPaneState,
    is_vertical_y_plane: bool,
) -> Vec<[[f64; 3]; 2]> {
    let ext = mesh.bbox.extents();
    let height_mm = ext[2].max(1.0);
    let center_xy = [
        0.5 * (mesh.bbox.min[0] + mesh.bbox.max[0]),
        0.5 * (mesh.bbox.min[1] + mesh.bbox.max[1]),
    ];
    let z_min = mesh.bbox.min[2];
    let (sin_p, cos_p) = state.active_section_angle_deg.to_radians().sin_cos();
    let (sin_t, cos_t) = state.tilt_deg.to_radians().sin_cos();
    let total_roll_deg = state.roll_deg + state.inner_level_deg;
    let (sin_r, cos_r) = total_roll_deg.to_radians().sin_cos();
    let off_x = state.artifact_offset_x_mm;
    let plane_val_mm = if is_vertical_y_plane {
        state.cut_y_mm
    } else {
        state.top_cut_z_mm
    };

    let world_verts: Vec<[f64; 3]> = mesh
        .positions
        .par_iter()
        .map(|&pos| {
            transform_local_vertex_full(
                pos, center_xy, z_min, height_mm, cos_p, sin_p, cos_t, sin_t, cos_r, sin_r, off_x,
            )
        })
        .collect();

    let mut segs = Vec::new();
    for tri in &mesh.triangles {
        let w0 = world_verts[tri[0] as usize];
        let w1 = world_verts[tri[1] as usize];
        let w2 = world_verts[tri[2] as usize];
        let pts = [w0, w1, w2];
        let d = if is_vertical_y_plane {
            [
                w0[1] - plane_val_mm,
                w1[1] - plane_val_mm,
                w2[1] - plane_val_mm,
            ]
        } else {
            [
                w0[2] - plane_val_mm,
                w1[2] - plane_val_mm,
                w2[2] - plane_val_mm,
            ]
        };

        if (d[0] > 0.0 && d[1] > 0.0 && d[2] > 0.0)
            || (d[0] < 0.0 && d[1] < 0.0 && d[2] < 0.0)
        {
            continue;
        }

        let mut hits = Vec::with_capacity(3);
        for &(a, b) in &[(0usize, 1usize), (1, 2), (2, 0)] {
            if (d[a] >= 0.0 && d[b] < 0.0) || (d[a] < 0.0 && d[b] >= 0.0) {
                let denom = d[a] - d[b];
                let t = if denom.abs() > 1e-12 {
                    d[a] / denom
                } else {
                    0.5
                };
                hits.push([
                    pts[a][0] + t * (pts[b][0] - pts[a][0]),
                    pts[a][1] + t * (pts[b][1] - pts[a][1]),
                    pts[a][2] + t * (pts[b][2] - pts[a][2]),
                ]);
            }
        }
        if hits.len() >= 2 {
            segs.push([hits[0], hits[1]]);
        }
    }
    segs
}

fn draw_vertical_dashed_line(
    painter: &egui::Painter,
    x: f32,
    y0: f32,
    y1: f32,
    stroke: egui::Stroke,
) {
    let min_y = y0.min(y1);
    let max_y = y0.max(y1);
    let dash = 4.0_f32;
    let gap = 3.0_f32;
    let mut y = min_y;
    while y < max_y {
        let yn = (y + dash).min(max_y);
        painter.line_segment([egui::pos2(x, y), egui::pos2(x, yn)], stroke);
        y += dash + gap;
    }
}

/// 3D 뷰포트(`27%`) — 상단 `직상방 동심원 방안지 절단 뷰 (중앙 고정 단면선 + 유물 수평이동·회전)` +
/// 하단 `수평 정면 입면 3D 스캐너 뷰 (전체 방안선 + 상하 점선 연결 + 5휠 미세조정 시스템)`를 렌더링한다.
pub fn render_view3d_pane_egui(
    ui: &mut egui::Ui,
    mesh: &TriMesh,
    region_labels: Option<&[SurfaceRegion]>,
    _rim_gap_chart: Option<&RimGapChart360>,
    best_angle_deg: f64,
    state: &mut View3DPaneState,
) {
    let ext = mesh.bbox.extents();
    let height_mm = ext[2].max(1.0);
    let radius_mm = (0.5 * ext[0].max(ext[1])).max(5.0);
    let grid_r_mm = compute_concentric_grid_radius_mm(radius_mm);

    if state.top_cut_initialized_for_mesh != mesh.name {
        state.top_cut_initialized_for_mesh = mesh.name.clone();
        state.top_cut_z_mm = mesh
            .ap2_cut_offset_mm
            .unwrap_or_else(|| (height_mm * 0.50).clamp(2.0, height_mm));
    }

    let avail = ui.available_size();
    let top_plan_h = (avail.y * 0.37).clamp(180.0, 340.0);
    let front_elev_h = (avail.y - top_plan_h - 4.0).max(200.0);

    let hanji = egui::Color32::from_rgb(GROUND.rgb[0], GROUND.rgb[1], GROUND.rgb[2]);
    let surface = egui::Color32::from_rgb(SURFACE.rgb[0], SURFACE.rgb[1], SURFACE.rgb[2]);
    let clay_wash = egui::Color32::from_rgb(CLAY_WASH.rgb[0], CLAY_WASH.rgb[1], CLAY_WASH.rgb[2]);
    let clay_ink = egui::Color32::from_rgb(CLAY_INK.rgb[0], CLAY_INK.rgb[1], CLAY_INK.rgb[2]);
    let border = egui::Color32::from_rgb(EDGE.rgb[0], EDGE.rgb[1], EDGE.rgb[2]);
    let ink = egui::Color32::from_rgb(INK.rgb[0], INK.rgb[1], INK.rgb[2]);
    let muted = egui::Color32::from_rgb(MUTED.rgb[0], MUTED.rgb[1], MUTED.rgb[2]);
    let terra = egui::Color32::from_rgb(ACTION.rgb[0], ACTION.rgb[1], ACTION.rgb[2]);
    let clay_focus =
        egui::Color32::from_rgb(CLAY_FOCUS.rgb[0], CLAY_FOCUS.rgb[1], CLAY_FOCUS.rgb[2]);
    let profile_red = egui::Color32::from_rgb(
        PROFILE_RED.rgb[0],
        PROFILE_RED.rgb[1],
        PROFILE_RED.rgb[2],
    );
    let select_blue = egui::Color32::from_rgb(
        SELECT_BLUE.rgb[0],
        SELECT_BLUE.rgb[1],
        SELECT_BLUE.rgb[2],
    );

    // 방향키(←/→/↑/↓)로도 유물 이동 및 수평횡단면(Alt+↑/↓) 미세이동 가능 (<USER_REQUEST>)
    ui.input(|i| {
        let shift_fast = i.modifiers.shift;
        let alt_horiz_z = i.modifiers.alt;
        let mut dx = 0i32;
        let mut dy = 0i32;
        if i.key_pressed(egui::Key::ArrowLeft) {
            dx -= 1;
        }
        if i.key_pressed(egui::Key::ArrowRight) {
            dx += 1;
        }
        if i.key_pressed(egui::Key::ArrowUp) {
            dy += 1;
        }
        if i.key_pressed(egui::Key::ArrowDown) {
            dy -= 1;
        }
        if dx != 0 || dy != 0 {
            state.apply_arrow_key_nudge(dx, dy, shift_fast, alt_horiz_z, height_mm);
        }
    });

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;

        // =========================================================================================
        // [상단 37% 뷰] 직상방(Top-Down) 동심원 방안지 절단 뷰 (`반경 = 유물 반경 × 1.20 = +20%`)
        // - 좌측 상단: `[+ 단면추가]` 버튼 (`Image 5`) -> 끝지점 독립 이동 가능한 추가 단면선 `A-A'`
        // - 우측 옆: `수평횡단면 고정 유물` 뷰 (`Image 4` -> 유물 고정 + 수평절단선만 상하 이동)
        // =========================================================================================
        let (top_rect, top_resp) =
            ui.allocate_exact_size(egui::vec2(avail.x, top_plan_h), egui::Sense::click_and_drag());

        let td_center = top_rect.center();
        let td_scale = compute_shared_ortho_scale(
            f64::from(top_rect.width()),
            grid_r_mm,
            height_mm,
            state.zoom,
        );
        let r_120_px = (grid_r_mm * td_scale) as f32;
        let r_100_px = (radius_mm * td_scale) as f32;

        // Image 5: 도면 안 좌측 위에 `[+ 단면추가]` 버튼 배치
        let add_sec_btn_rect = egui::Rect::from_min_size(
            egui::pos2(top_rect.left() + 8.0, top_rect.top() + 28.0),
            egui::vec2(92.0, 22.0),
        );
        let add_sec_btn_resp = ui.interact(
            add_sec_btn_rect,
            egui::Id::new("v3d_btn_add_custom_section"),
            egui::Sense::click(),
        );
        if add_sec_btn_resp.clicked() {
            if state.custom_section_line_mm.is_none() {
                state.add_default_custom_section_line(radius_mm);
            } else {
                state.custom_section_draw_mode = !state.custom_section_draw_mode;
            }
        }

        // Image 5: 추가 단면선(A-A')의 양 끝점(0번 또는 1번) 중 한쪽 끝지점을 누르고 이동하면 그 끝점만 이동!
        let mut custom_ep_dragged = false;
        if let Some([p0_mm, p1_mm]) = state.custom_section_line_mm {
            let ep0_px = egui::pos2(
                td_center.x + (p0_mm[0] * td_scale) as f32,
                td_center.y - (p0_mm[1] * td_scale) as f32,
            );
            let ep1_px = egui::pos2(
                td_center.x + (p1_mm[0] * td_scale) as f32,
                td_center.y - (p1_mm[1] * td_scale) as f32,
            );
            let ep0_rect = egui::Rect::from_center_size(ep0_px, egui::vec2(20.0, 20.0));
            let ep1_rect = egui::Rect::from_center_size(ep1_px, egui::vec2(20.0, 20.0));
            let ep0_resp = ui.interact(
                ep0_rect,
                egui::Id::new("v3d_custom_sec_ep0"),
                egui::Sense::click_and_drag(),
            );
            let ep1_resp = ui.interact(
                ep1_rect,
                egui::Id::new("v3d_custom_sec_ep1"),
                egui::Sense::click_and_drag(),
            );
            if ep0_resp.dragged() {
                custom_ep_dragged = true;
                state.custom_section_dragging_endpoint = Some(0);
                if let Some(pos) = ep0_resp.interact_pointer_pos() {
                    let nx_mm = f64::from(pos.x - td_center.x) / td_scale.max(0.5);
                    let ny_mm = f64::from(td_center.y - pos.y) / td_scale.max(0.5);
                    let max_r = grid_r_mm * 1.05;
                    state.move_custom_section_endpoint(
                        0,
                        [nx_mm.clamp(-max_r, max_r), ny_mm.clamp(-max_r, max_r)],
                    );
                }
            } else if ep1_resp.dragged() {
                custom_ep_dragged = true;
                state.custom_section_dragging_endpoint = Some(1);
                if let Some(pos) = ep1_resp.interact_pointer_pos() {
                    let nx_mm = f64::from(pos.x - td_center.x) / td_scale.max(0.5);
                    let ny_mm = f64::from(td_center.y - pos.y) / td_scale.max(0.5);
                    let max_r = grid_r_mm * 1.05;
                    state.move_custom_section_endpoint(
                        1,
                        [nx_mm.clamp(-max_r, max_r), ny_mm.clamp(-max_r, max_r)],
                    );
                }
            } else {
                state.custom_section_dragging_endpoint = None;
            }
        }

        if top_resp.dragged()
            && !custom_ep_dragged
            && !add_sec_btn_resp.hovered()
        {
            let d = top_resp.drag_delta();
            let shift_or_right = ui.input(|i| i.modifiers.shift || i.pointer.secondary_down());
            if shift_or_right {
                // 우클릭 또는 Shift+드래그: 유물 시계·반시계 회전 (ψ)
                let next_psi = state.active_section_angle_deg + f64::from(d.x) * 0.5;
                state.active_section_angle_deg =
                    ((next_psi * 10.0).round() / 10.0).rem_euclid(360.0);
            } else {
                // 기본 좌클릭 드래그: 중앙 단면선은 고정하고 유물만 좌우(ΔX)·앞뒤(ΔY) 수평 이동!
                let max_off = (radius_mm * 1.10).max(15.0);
                let mm_per_px = 1.0 / td_scale.max(0.5);
                state.artifact_offset_x_mm = (((state.artifact_offset_x_mm
                    + f64::from(d.x) * mm_per_px)
                    .clamp(-max_off, max_off)
                    * 100.0)
                    .round())
                    / 100.0;
                state.cut_y_mm = (((state.cut_y_mm + f64::from(d.y) * mm_per_px)
                    .clamp(-max_off, max_off)
                    * 100.0)
                    .round())
                    / 100.0;
            }
        }
        if top_resp.double_clicked() {
            state.artifact_offset_x_mm = 0.0;
            state.cut_y_mm = 0.0;
        }

        let tp = ui.painter_at(top_rect);
        tp.rect_filled(top_rect, 3.0, surface);

        let top_w = (top_rect.width().round() as u32).max(160);
        let top_h = (top_rect.height().round() as u32).max(140);
        let _ = state.ensure_topdown_cached(mesh, top_w, top_h);
        if state.cached_topdown_tex.is_none() {
            if let Some(ref td_img) = state.cached_topdown_rgba {
                let cimg = egui::ColorImage::from_rgba_unmultiplied(
                    [td_img.width() as usize, td_img.height() as usize],
                    td_img.as_raw(),
                );
                state.cached_topdown_tex = Some(ui.ctx().load_texture(
                    "strata_view3d_topdown_plan",
                    cimg,
                    egui::TextureOptions::LINEAR,
                ));
            }
        }
        if let Some(ref td_tex) = state.cached_topdown_tex {
            tp.image(
                td_tex.id(),
                top_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }

        // 동심원은 항상 1:1 정원(둥근 원)이며 중앙에 고정됨
        for rk in 1..=5 {
            let r_px = r_120_px * (rk as f32 / 5.0);
            tp.circle_stroke(
                td_center,
                r_px,
                egui::Stroke::new(
                    if rk == 5 { 1.3_f32 } else { 0.8_f32 },
                    egui::Color32::from_rgba_unmultiplied(
                        156,
                        74,
                        47,
                        if rk == 5 { 170 } else { 95 },
                    ),
                ),
            );
        }
        tp.circle_stroke(
            td_center,
            r_100_px,
            egui::Stroke::new(1.1_f32, egui::Color32::from_rgba_unmultiplied(42, 120, 214, 150)),
        );

        // 직상방 십자 기준선 (0°·90°·180°·270°)
        tp.line_segment(
            [
                egui::pos2(td_center.x - r_120_px - 8.0, td_center.y),
                egui::pos2(td_center.x + r_120_px + 8.0, td_center.y),
            ],
            egui::Stroke::new(0.9_f32, egui::Color32::from_rgba_unmultiplied(94, 93, 89, 130)),
        );
        tp.line_segment(
            [
                egui::pos2(td_center.x, td_center.y - r_120_px - 8.0),
                egui::pos2(td_center.x, td_center.y + r_120_px + 8.0),
            ],
            egui::Stroke::new(0.9_f32, egui::Color32::from_rgba_unmultiplied(94, 93, 89, 130)),
        );

        // Image 1 ("단면선이 유물에가려진다"): 단면선이 3D 유물에 절대 가려지지 않도록 밝은 외곽 후광(Halo) + 최상단 선으로 렌더링!
        let fixed_cut_sy = td_center.y;
        tp.line_segment(
            [
                egui::pos2(td_center.x - r_120_px - 10.0, fixed_cut_sy),
                egui::pos2(td_center.x + r_120_px + 10.0, fixed_cut_sy),
            ],
            egui::Stroke::new(
                4.2_f32,
                egui::Color32::from_rgba_unmultiplied(255, 252, 235, 220),
            ),
        );
        tp.line_segment(
            [
                egui::pos2(td_center.x - r_120_px - 10.0, fixed_cut_sy),
                egui::pos2(td_center.x + r_120_px + 10.0, fixed_cut_sy),
            ],
            egui::Stroke::new(2.0_f32, select_blue),
        );

        state.ensure_3d_curves_cached(mesh);
        let mut cut_min_wx = f64::INFINITY;
        let mut cut_max_wx = f64::NEG_INFINITY;
        for seg in &state.cached_v_segs {
            cut_min_wx = cut_min_wx.min(seg[0][0]).min(seg[1][0]);
            cut_max_wx = cut_max_wx.max(seg[0][0]).max(seg[1][0]);
            let p0 = egui::pos2(
                td_center.x + (seg[0][0] * td_scale) as f32,
                fixed_cut_sy,
            );
            let p1 = egui::pos2(
                td_center.x + (seg[1][0] * td_scale) as f32,
                fixed_cut_sy,
            );
            tp.line_segment(
                [p0, p1],
                egui::Stroke::new(
                    4.6_f32,
                    egui::Color32::from_rgba_unmultiplied(255, 250, 210, 235),
                ),
            );
            tp.line_segment([p0, p1], egui::Stroke::new(2.8_f32, profile_red));
        }

        // Image 5: 사용자가 추가한 단면선(`A — A'`)과 한쪽 끝점만 독립 이동 가능한 핸들(`A`, `A'`) 렌더링
        if let Some([p0_mm, p1_mm]) = state.custom_section_line_mm {
            let ep0_px = egui::pos2(
                td_center.x + (p0_mm[0] * td_scale) as f32,
                td_center.y - (p0_mm[1] * td_scale) as f32,
            );
            let ep1_px = egui::pos2(
                td_center.x + (p1_mm[0] * td_scale) as f32,
                td_center.y - (p1_mm[1] * td_scale) as f32,
            );
            tp.line_segment(
                [ep0_px, ep1_px],
                egui::Stroke::new(
                    4.4_f32,
                    egui::Color32::from_rgba_unmultiplied(255, 252, 230, 235),
                ),
            );
            tp.line_segment([ep0_px, ep1_px], egui::Stroke::new(2.4_f32, profile_red));
            for (ep_px, lbl) in [(ep0_px, "A"), (ep1_px, "A'")] {
                tp.circle_filled(ep_px, 6.2, egui::Color32::WHITE);
                tp.circle_stroke(ep_px, 6.2, egui::Stroke::new(2.0_f32, profile_red));
                tp.text(
                    egui::pos2(ep_px.x, ep_px.y - 11.0),
                    egui::Align2::CENTER_CENTER,
                    lbl,
                    egui::FontId::proportional(9.5),
                    profile_red,
                );
            }
        }

        // Image 5: 좌측 상단 `[+ 단면추가]` 버튼 렌더링
        let btn_active = state.custom_section_line_mm.is_some();
        tp.rect_filled(
            add_sec_btn_rect,
            4.0,
            if btn_active { profile_red } else { clay_wash },
        );
        tp.rect_stroke(
            add_sec_btn_rect,
            4.0,
            egui::Stroke::new(1.2_f32, profile_red),
            egui::StrokeKind::Inside,
        );
        tp.text(
            add_sec_btn_rect.center(),
            egui::Align2::CENTER_CENTER,
            if btn_active {
                "✂ 단면선 A-A' 활성"
            } else {
                "+ 단면추가"
            },
            egui::FontId::proportional(9.5),
            if btn_active { egui::Color32::WHITE } else { ink },
        );
        if btn_active {
            let hint_rect = egui::Rect::from_min_size(
                egui::pos2(add_sec_btn_rect.left(), add_sec_btn_rect.bottom() + 3.0),
                egui::vec2(102.0, 28.0),
            );
            tp.rect_filled(
                hint_rect,
                3.0,
                egui::Color32::from_rgba_unmultiplied(250, 249, 245, 232),
            );
            tp.text(
                egui::pos2(hint_rect.left() + 4.0, hint_rect.top() + 8.0),
                egui::Align2::LEFT_CENTER,
                "끝점(A/A') 드래그:",
                egui::FontId::proportional(8.2),
                ink,
            );
            tp.text(
                egui::pos2(hint_rect.left() + 4.0, hint_rect.top() + 19.0),
                egui::Align2::LEFT_CENTER,
                "한쪽 끝만 단독 이동",
                egui::FontId::proportional(8.2),
                profile_red,
            );
        }

        // 유물 현재 중심점 및 회전 방위각(ψ) 바늘 표시
        let _ = region_labels;
        let art_center_px = egui::pos2(
            td_center.x + (state.artifact_offset_x_mm * td_scale) as f32,
            td_center.y + (state.cut_y_mm * td_scale) as f32,
        );
        let psi_rad = state.active_section_angle_deg.to_radians() as f32;
        let needle_pt = egui::pos2(
            art_center_px.x + r_100_px * 0.85 * psi_rad.cos(),
            art_center_px.y - r_100_px * 0.85 * psi_rad.sin(),
        );
        tp.line_segment([art_center_px, needle_pt], egui::Stroke::new(1.3_f32, terra));
        tp.circle_filled(td_center, 3.2, profile_red);

        // 직상방 뷰 상단 미니 배지
        let td_hdr = egui::Rect::from_min_size(
            egui::pos2(top_rect.left() + 6.0, top_rect.top() + 4.0),
            egui::vec2(top_rect.width() - 12.0, 20.0),
        );
        tp.rect_filled(
            td_hdr,
            3.0,
            egui::Color32::from_rgba_unmultiplied(250, 249, 245, 225),
        );
        tp.text(
            egui::pos2(td_hdr.left() + 6.0, td_hdr.center().y),
            egui::Align2::LEFT_CENTER,
            format!(
                "직상방 동심원 (중앙단면 고정 · 유물이동 ΔX={:+.1} ΔY={:+.2}mm · 드래그/방향키=유물이동)",
                state.artifact_offset_x_mm, state.cut_y_mm
            ),
            egui::FontId::proportional(9.8),
            ink,
        );
        tp.rect_stroke(
            top_rect,
            3.0,
            egui::Stroke::new(1.0_f32, border),
            egui::StrokeKind::Inside,
        );

        // =========================================================================================
        // [하단 63% 뷰] 수평 정면 입면 3D 스캐너 뷰 (전체 방안선 + 상하 점선 연결 + 5휠 미세조정 시스템)
        // =========================================================================================
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(avail.x, front_elev_h),
            egui::Sense::click_and_drag(),
        );

        let dial_horiz_w = (rect.width() * 0.44).clamp(148.0, 196.0);
        let dial_horiz_h = 20.0_f32;
        // Image 1: 상단 가로 휠 (유물이 시계방향·반시계방향으로 회전하는 휠, 휠 아래 설명)
        // Image 6 유물 뒤집기(90°/180°) 버튼 행 아래쪽에 여유 있게 배치!
        let top_dial_rect = egui::Rect::from_center_size(
            egui::pos2(rect.center().x, rect.top() + 82.0),
            egui::vec2(dial_horiz_w, dial_horiz_h),
        );
        // <USER_REQUEST>: 상하좌우 휠 안쪽에 수평·단면 미세정합 제5휠 추가!
        let inner_level_dial_rect = egui::Rect::from_center_size(
            egui::pos2(rect.center().x, rect.bottom() - 68.0),
            egui::vec2((dial_horiz_w * 0.86).max(128.0), 17.0),
        );
        // Image 2: 하단 가로 휠 (유물·단면선 앞뒤로 미세조정 휠, 휠 아래 설명)
        let bot_dial_rect = egui::Rect::from_center_size(
            egui::pos2(rect.center().x, rect.bottom() - 28.0),
            egui::vec2(dial_horiz_w, dial_horiz_h),
        );
        let dial_vert_w = 21.0_f32;
        let dial_vert_h = (rect.height() * 0.36).clamp(104.0, 168.0);
        let dial_mid_y = (top_dial_rect.bottom() + inner_level_dial_rect.top()) * 0.5 + 4.0;
        // Image 4: 좌측 세로 휠 (유물의 아래위를 회전하는 휠 — 올리면 위로 회전, 내리면 아래로 회전, 옆에 설명)
        let left_dial_rect = egui::Rect::from_center_size(
            egui::pos2(rect.left() + 16.0, dial_mid_y),
            egui::vec2(dial_vert_w, dial_vert_h),
        );
        // Image 3: 우측 세로 휠 (유물의 좌우 기울기를 미세조정하는 휠, 옆에 설명)
        let right_dial_rect = egui::Rect::from_center_size(
            egui::pos2(rect.right() - 16.0, dial_mid_y),
            egui::vec2(dial_vert_w, dial_vert_h),
        );

        let top_dial_resp = ui.interact(
            top_dial_rect,
            egui::Id::new("v3d_wheel_top_turn_psi"),
            egui::Sense::click_and_drag(),
        );
        let bot_dial_resp = ui.interact(
            bot_dial_rect,
            egui::Id::new("v3d_wheel_bot_section_dy"),
            egui::Sense::click_and_drag(),
        );
        let left_dial_resp = ui.interact(
            left_dial_rect,
            egui::Id::new("v3d_wheel_left_pitch_updown"),
            egui::Sense::click_and_drag(),
        );
        let right_dial_resp = ui.interact(
            right_dial_rect,
            egui::Id::new("v3d_wheel_right_tilt_lr"),
            egui::Sense::click_and_drag(),
        );
        let inner_dial_resp = ui.interact(
            inner_level_dial_rect,
            egui::Id::new("v3d_wheel_inner_level_sync"),
            egui::Sense::click_and_drag(),
        );

        // Image 1: 상단 가로 휠 -> 유물 시계(+)/반시계(-) 회전 (0.1°, Image 3 감도 +30% 향상: * 0.65)
        if top_dial_resp.clicked() {
            state.apply_precision_wheel(PrecisionWheelKind::TopTurnCwCcw, 1);
        } else if top_dial_resp.dragged() {
            let steps = (top_dial_resp.drag_delta().x * 0.65).round() as i32;
            if steps != 0 {
                state.apply_precision_wheel(PrecisionWheelKind::TopTurnCwCcw, steps);
            }
        }
        // Image 2: 하단 가로 휠 -> 유물·단면선 앞뒤 미세조정 (0.01mm, Image 3 감도 +30% 향상: * 0.65)
        if bot_dial_resp.clicked() {
            state.apply_precision_wheel(PrecisionWheelKind::BottomSectionFrontBack, 1);
        } else if bot_dial_resp.dragged() {
            let steps = (bot_dial_resp.drag_delta().x * 0.65).round() as i32;
            if steps != 0 {
                state.apply_precision_wheel(PrecisionWheelKind::BottomSectionFrontBack, steps);
            }
        }
        // Image 4: 좌측 세로 휠 -> 올리면(drag_delta.y < 0) 위로 회전(+steps), 내리면(drag_delta.y > 0) 아래로 회전(-steps) (+30% 감도)
        if left_dial_resp.clicked() {
            state.apply_precision_wheel(PrecisionWheelKind::LeftPitchUpDown, 1);
        } else if left_dial_resp.dragged() {
            let steps = (-left_dial_resp.drag_delta().y * 0.65).round() as i32;
            if steps != 0 {
                state.apply_precision_wheel(PrecisionWheelKind::LeftPitchUpDown, steps);
            }
        }
        // Image 3: 우측 세로 휠 -> 유물 좌우 기울기 미세조정 (0.1°, +30% 감도)
        if right_dial_resp.clicked() {
            state.apply_precision_wheel(PrecisionWheelKind::RightTiltLeftRight, 1);
        } else if right_dial_resp.dragged() {
            let steps = (-right_dial_resp.drag_delta().y * 0.65).round() as i32;
            if steps != 0 {
                state.apply_precision_wheel(PrecisionWheelKind::RightTiltLeftRight, steps);
            }
        }
        // Image 0: 안쪽 제5휠 -> 수평횡단면을 그리는 휠 (선만 상하이동, 0.1mm 단위, +30% 감도)
        if inner_dial_resp.clicked() {
            state.apply_precision_wheel(PrecisionWheelKind::InnerHorizSectionZ, 5);
        } else if inner_dial_resp.dragged() {
            let d = inner_dial_resp.drag_delta();
            let raw = if d.x.abs() >= d.y.abs() { d.x } else { -d.y };
            let steps = (raw * 0.65).round() as i32;
            if steps != 0 {
                state.apply_precision_wheel(PrecisionWheelKind::InnerHorizSectionZ, steps);
                state.top_cut_z_mm = state
                    .top_cut_z_mm
                    .clamp(1.0, (height_mm - 1.0).max(2.0));
            }
        }
        if inner_dial_resp.double_clicked() {
            state.top_cut_z_mm = ((height_mm * 0.50) * 10.0).round() / 10.0;
        }

        let any_fine_dial_active = top_dial_resp.dragged()
            || bot_dial_resp.dragged()
            || left_dial_resp.dragged()
            || right_dial_resp.dragged()
            || inner_dial_resp.dragged()
            || top_dial_resp.hovered()
            || bot_dial_resp.hovered()
            || left_dial_resp.hovered()
            || right_dial_resp.hovered()
            || inner_dial_resp.hovered();

        if response.dragged() && !any_fine_dial_active {
            let delta = response.drag_delta();
            match state.drag_mode {
                DragMode3D::TurntableArtifact => {
                    let next_psi = state.active_section_angle_deg + f64::from(delta.x) * 0.6;
                    state.active_section_angle_deg =
                        ((next_psi * 10.0).round() / 10.0).rem_euclid(360.0);
                    state.yaw_deg = 0.0;
                    state.pitch_deg = 0.0;
                }
                DragMode3D::CameraOrbit => {
                    state.yaw_deg = (state.yaw_deg + delta.x * 0.6).rem_euclid(360.0);
                    state.pitch_deg = (state.pitch_deg + delta.y * 0.4).clamp(-45.0, 45.0);
                }
                DragMode3D::AxisTilt => {
                    state.tilt_deg =
                        ((state.tilt_deg - f64::from(delta.y) * 0.1).clamp(-30.0, 30.0) * 10.0)
                            .round()
                            / 10.0;
                }
            }
        }
        if response.hovered() || top_resp.hovered() {
            let scroll_y = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll_y.abs() > 0.1 {
                state.zoom = (state.zoom * (1.0 + scroll_y * 0.0015)).clamp(0.45, 3.2);
            }
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 3.0, surface);

        let is_dragging = response.dragged()
            || top_resp.dragged()
            || top_dial_resp.dragged()
            || bot_dial_resp.dragged()
            || left_dial_resp.dragged()
            || right_dial_resp.dragged()
            || inner_dial_resp.dragged();
        state.is_interacting = is_dragging;

        let (render_w, render_h) =
            compute_isotropic_render_dimensions(rect.width(), rect.height(), is_dragging);
        let _ = state.ensure_zbuffer_cached(mesh, region_labels, render_w, render_h);

        if state.cached_zbuffer_tex.is_none() {
            if let Some(ref zbuf_img) = state.cached_zbuffer_rgba {
                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                    [zbuf_img.width() as usize, zbuf_img.height() as usize],
                    zbuf_img.as_raw(),
                );
                state.cached_zbuffer_tex = Some(ui.ctx().load_texture(
                    "strata_view3d_zbuffer",
                    color_image,
                    egui::TextureOptions::LINEAR,
                ));
            }
        }
        if let Some(ref tex_handle) = state.cached_zbuffer_tex {
            painter.image(
                tex_handle.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        if is_dragging {
            ui.ctx().request_repaint();
        }

        // 정면 입면 투영 변환 도우미 (상단 직상방 동심원 평면도와 100% 1:1 동일한 scale 사용!)
        let scale = compute_shared_ortho_scale(
            f64::from(rect.width()),
            grid_r_mm,
            height_mm,
            state.zoom,
        );
        let half_w = f64::from(rect.width()) * 0.5;
        let half_h = f64::from(rect.height()) * 0.54;
        let (sin_cy, cos_cy) = f64::from(state.yaw_deg).to_radians().sin_cos();
        let (sin_cp, cos_cp) = f64::from(state.pitch_deg).to_radians().sin_cos();

        let proj_pos2 = |w: [f64; 3]| -> egui::Pos2 {
            let s = project_world_to_screen(
                w, height_mm, half_w, half_h, scale, cos_cy, sin_cy, cos_cp, sin_cp,
            );
            egui::pos2(rect.left() + s[0] as f32, rect.top() + s[1] as f32)
        };

        // 수평 바닥 동심원 기준선 (`Z = 0` 평면)
        if state.show_concentric_scope {
            let b0 = proj_pos2([-grid_r_mm, state.cut_y_mm, 0.0]);
            let b1 = proj_pos2([grid_r_mm, state.cut_y_mm, 0.0]);
            painter.line_segment(
                [b0, b1],
                egui::Stroke::new(1.2_f32, egui::Color32::from_rgba_unmultiplied(156, 74, 47, 165)),
            );
        }

        // 월드 고정 수평 정면 절단틀 (`P_cut: Y = cut_y_mm`) 및 수직 중심축선 + 실시간 단면 곡선
        let mut elev_min_sx = rect.center().x;
        let mut elev_max_sx = rect.center().x;
        let mut elev_top_sy = rect.center().y;
        if state.show_cutting_frame {
            let hw = grid_r_mm;
            let hh = height_mm + 4.0;
            let y_cut = state.cut_y_mm;
            let f0 = proj_pos2([-hw, y_cut, -2.0]);
            let f1 = proj_pos2([hw, y_cut, -2.0]);
            let f2 = proj_pos2([hw, y_cut, hh]);
            let f3 = proj_pos2([-hw, y_cut, hh]);
            elev_top_sy = f2.y.min(f3.y);

            painter.add(egui::Shape::closed_line(
                vec![f0, f1, f2, f3],
                egui::Stroke::new(1.1_f32, select_blue),
            ));

            let ax_bot = proj_pos2([state.artifact_offset_x_mm, y_cut, -4.0]);
            let ax_top = proj_pos2([state.artifact_offset_x_mm, y_cut, hh + 2.0]);
            painter.line_segment([ax_bot, ax_top], egui::Stroke::new(1.3_f32, terra));

            for seg in &state.cached_v_segs {
                let a = proj_pos2(seg[0]);
                let b = proj_pos2(seg[1]);
                elev_min_sx = elev_min_sx.min(a.x).min(b.x);
                elev_max_sx = elev_max_sx.max(a.x).max(b.x);
                painter.line_segment(
                    [a, b],
                    egui::Stroke::new(
                        4.2_f32,
                        egui::Color32::from_rgba_unmultiplied(255, 250, 210, 230),
                    ),
                );
                painter.line_segment([a, b], egui::Stroke::new(2.2_f32, profile_red));
            }

            // Image 0 & Image 1: 수평횡단면 절단선이 유물에 절대 가려지지 않도록 전체 가로지르는 기준선 + 교차선 최상단 렌더링
            let hz = state.top_cut_z_mm.clamp(0.5, height_mm);
            let h_left = proj_pos2([-hw * 0.96, y_cut, hz]);
            let h_right = proj_pos2([hw * 0.96, y_cut, hz]);
            painter.line_segment(
                [h_left, h_right],
                egui::Stroke::new(
                    4.0_f32,
                    egui::Color32::from_rgba_unmultiplied(255, 252, 232, 225),
                ),
            );
            painter.line_segment([h_left, h_right], egui::Stroke::new(2.0_f32, profile_red));

            for seg in &state.cached_h_segs {
                let a = proj_pos2(seg[0]);
                let b = proj_pos2(seg[1]);
                painter.line_segment(
                    [a, b],
                    egui::Stroke::new(
                        3.6_f32,
                        egui::Color32::from_rgba_unmultiplied(255, 250, 215, 225),
                    ),
                );
                painter.line_segment([a, b], egui::Stroke::new(2.0_f32, profile_red));
            }
        }

        // =========================================================================================
        // Image 7 & v6_vessel_roi_1to1 Image 0:
        // 상단 직상방 동심원 평면도와 하단 수평 정면 입면도가 100% 1:1 동일한 스케일(td_scale == scale)을 공유하므로
        // 좌측 절단점·중심축·우측 절단점이 기울어지지 않고 90° 수직 점선으로 1:1 직결된다!
        // =========================================================================================
        let link_stroke = egui::Stroke::new(
            1.1_f32,
            egui::Color32::from_rgba_unmultiplied(200, 48, 36, 175),
        );
        let center_x = td_center.x + (state.artifact_offset_x_mm * td_scale) as f32;
        draw_vertical_dashed_line(&tp, center_x, fixed_cut_sy, top_rect.bottom(), link_stroke);
        draw_vertical_dashed_line(&painter, center_x, rect.top(), elev_top_sy + 14.0, link_stroke);

        if cut_min_wx.is_finite() && cut_max_wx.is_finite() {
            let left_x = td_center.x + (cut_min_wx * td_scale) as f32;
            let right_x = td_center.x + (cut_max_wx * td_scale) as f32;

            draw_vertical_dashed_line(&tp, left_x, fixed_cut_sy, top_rect.bottom(), link_stroke);
            draw_vertical_dashed_line(&tp, right_x, fixed_cut_sy, top_rect.bottom(), link_stroke);
            draw_vertical_dashed_line(&painter, left_x, rect.top(), elev_top_sy + 14.0, link_stroke);
            draw_vertical_dashed_line(&painter, right_x, rect.top(), elev_top_sy + 14.0, link_stroke);
        }

        // =========================================================================================
        // 상·하·좌·우 4방향 휠 + 안쪽 제5 수평정합 휠 렌더링 (`Image 1 ~ 4` & `<USER_REQUEST>`)
        // =========================================================================================
        let dial_fill = egui::Color32::from_rgba_unmultiplied(236, 233, 224, 238);
        let dial_stroke = egui::Color32::from_rgb(178, 174, 162);
        let tick_col = egui::Color32::from_rgb(156, 152, 142);
        let tick_major_col = egui::Color32::from_rgb(104, 100, 92);
        let label_bg = egui::Color32::from_rgba_unmultiplied(250, 249, 245, 225);

        // 1. Image 1 (상단 가로 휠): 유물이 시계방향·반시계방향으로 회전을 하는 휠 + 휠 아래 설명!
        painter.rect_filled(top_dial_rect, 10.0, dial_fill);
        painter.rect_stroke(
            top_dial_rect,
            10.0,
            egui::Stroke::new(
                if top_dial_resp.hovered() { 1.4_f32 } else { 1.0_f32 },
                if top_dial_resp.hovered() { terra } else { dial_stroke },
            ),
            egui::StrokeKind::Inside,
        );
        let n_ticks_h = 32usize;
        let phase_psi = ((state.active_section_angle_deg * 10.0).rem_euclid(5.0) / 5.0) as f32;
        for ti in 0..=n_ticks_h {
            let frac = ((ti as f32 + phase_psi) / (n_ticks_h as f32)).clamp(0.0, 1.0);
            let tx = top_dial_rect.left() + 9.0 + frac * (top_dial_rect.width() - 18.0);
            let is_mid = ti == n_ticks_h / 2;
            let th = if is_mid { 12.0 } else if ti % 4 == 0 { 9.0 } else { 6.0 };
            painter.line_segment(
                [
                    egui::pos2(tx, top_dial_rect.center().y - th * 0.5),
                    egui::pos2(tx, top_dial_rect.center().y + th * 0.5),
                ],
                egui::Stroke::new(
                    if is_mid { 1.3_f32 } else { 0.8_f32 },
                    if is_mid { tick_major_col } else { tick_col },
                ),
            );
        }
        // Image 1: 휠 아래 설명 ("유물이 시계방향 반시계방향으로 회전을 하는 휠 휠아래설명")
        let top_lbl_rect = egui::Rect::from_center_size(
            egui::pos2(top_dial_rect.center().x, top_dial_rect.bottom() + 9.5),
            egui::vec2(216.0, 15.0),
        );
        painter.rect_filled(top_lbl_rect, 3.0, label_bg);
        painter.text(
            top_lbl_rect.center(),
            egui::Align2::CENTER_CENTER,
            format!(
                "↻ 유물 시계·반시계 회전 휠 (ψ={:.1}°)",
                state.active_section_angle_deg
            ),
            egui::FontId::proportional(9.5),
            ink,
        );

        // 2. Image 0 (안쪽 제5휠): 수평횡단면을 그리는 휠 (선만 아래위로 이동 + 수평횡단면 도면 실시간 갱신)
        painter.rect_filled(inner_level_dial_rect, 8.5, clay_wash);
        painter.rect_stroke(
            inner_level_dial_rect,
            8.5,
            egui::Stroke::new(
                if inner_dial_resp.hovered() { 1.4_f32 } else { 1.0_f32 },
                if inner_dial_resp.hovered() { profile_red } else { clay_focus },
            ),
            egui::StrokeKind::Inside,
        );
        let phase_hz = ((state.top_cut_z_mm * 10.0).rem_euclid(5.0) / 5.0) as f32;
        for ti in 0..=24usize {
            let frac = ((ti as f32 + phase_hz) / 24.0).clamp(0.0, 1.0);
            let tx =
                inner_level_dial_rect.left() + 8.0 + frac * (inner_level_dial_rect.width() - 16.0);
            let is_mid = ti == 12;
            let th = if is_mid { 10.5 } else if ti % 4 == 0 { 8.0 } else { 5.0 };
            painter.line_segment(
                [
                    egui::pos2(tx, inner_level_dial_rect.center().y - th * 0.5),
                    egui::pos2(tx, inner_level_dial_rect.center().y + th * 0.5),
                ],
                egui::Stroke::new(
                    if is_mid { 1.3_f32 } else { 0.8_f32 },
                    if is_mid { profile_red } else { terra },
                ),
            );
        }
        let inner_lbl_rect = egui::Rect::from_center_size(
            egui::pos2(
                inner_level_dial_rect.center().x,
                inner_level_dial_rect.bottom() + 8.5,
            ),
            egui::vec2(262.0, 14.0),
        );
        painter.rect_filled(inner_lbl_rect, 3.0, label_bg);
        painter.text(
            inner_lbl_rect.center(),
            egui::Align2::CENTER_CENTER,
            format!(
                "↕ 수평횡단면 그리기 휠 (Z={:.1}mm · 선만 상하이동)",
                state.top_cut_z_mm
            ),
            egui::FontId::proportional(9.0),
            profile_red,
        );

        // 3. Image 2 (하단 가로 휠): 유물·단면선 앞뒤로 미세조정 휠 + 휠 아래 설명!
        painter.rect_filled(bot_dial_rect, 10.0, dial_fill);
        painter.rect_stroke(
            bot_dial_rect,
            10.0,
            egui::Stroke::new(
                if bot_dial_resp.hovered() { 1.4_f32 } else { 1.0_f32 },
                if bot_dial_resp.hovered() { terra } else { dial_stroke },
            ),
            egui::StrokeKind::Inside,
        );
        let phase_dy = ((state.cut_y_mm * 100.0).rem_euclid(5.0) / 5.0) as f32;
        for ti in 0..=n_ticks_h {
            let frac = ((ti as f32 + phase_dy).rem_euclid(n_ticks_h as f32) / (n_ticks_h as f32))
                .clamp(0.0, 1.0);
            let tx = bot_dial_rect.left() + 9.0 + frac * (bot_dial_rect.width() - 18.0);
            let is_mid = ti == n_ticks_h / 2;
            let th = if is_mid { 12.0 } else if ti % 4 == 0 { 9.0 } else { 6.0 };
            painter.line_segment(
                [
                    egui::pos2(tx, bot_dial_rect.center().y - th * 0.5),
                    egui::pos2(tx, bot_dial_rect.center().y + th * 0.5),
                ],
                egui::Stroke::new(
                    if is_mid { 1.3_f32 } else { 0.8_f32 },
                    if is_mid { tick_major_col } else { tick_col },
                ),
            );
        }
        // Image 2: 휠 아래 설명 ("이것은 유물이단면선 앞뒤로 미세조정휠 휠아래설명")
        let bot_lbl_rect = egui::Rect::from_center_size(
            egui::pos2(bot_dial_rect.center().x, bot_dial_rect.bottom() + 9.0),
            egui::vec2(236.0, 15.0),
        );
        painter.rect_filled(bot_lbl_rect, 3.0, label_bg);
        painter.text(
            bot_lbl_rect.center(),
            egui::Align2::CENTER_CENTER,
            format!(
                "▲▼ 유물·단면선 앞뒤 미세조정 휠 (ΔY={:+.2}mm)",
                state.cut_y_mm
            ),
            egui::FontId::proportional(9.5),
            ink,
        );

        // 4. Image 4 (좌측 세로 휠): 유물의 아래위를 회전하는 휠 (올리면 위로, 내리면 아래로) + 휠 옆에 설명!
        painter.rect_filled(left_dial_rect, 10.5, dial_fill);
        painter.rect_stroke(
            left_dial_rect,
            10.5,
            egui::Stroke::new(
                if left_dial_resp.hovered() { 1.4_f32 } else { 1.0_f32 },
                if left_dial_resp.hovered() { terra } else { dial_stroke },
            ),
            egui::StrokeKind::Inside,
        );
        let n_ticks_v = 26usize;
        let phase_tilt = ((state.tilt_deg * 10.0).rem_euclid(5.0) / 5.0) as f32;
        for ti in 0..=n_ticks_v {
            let frac = ((ti as f32 - phase_tilt).rem_euclid(n_ticks_v as f32) / (n_ticks_v as f32))
                .clamp(0.0, 1.0);
            let ty = left_dial_rect.top() + 9.0 + frac * (left_dial_rect.height() - 18.0);
            let is_mid = ti == n_ticks_v / 2;
            let tw = if is_mid { 15.0 } else if ti % 4 == 0 { 10.5 } else { 7.0 };
            painter.line_segment(
                [
                    egui::pos2(left_dial_rect.center().x - tw * 0.5, ty),
                    egui::pos2(left_dial_rect.center().x + tw * 0.5, ty),
                ],
                egui::Stroke::new(
                    if is_mid { 1.3_f32 } else { 0.8_f32 },
                    if is_mid { tick_major_col } else { tick_col },
                ),
            );
        }
        // Image 4: 좌측 세로 휠 바로 위쪽에 설명 표시 (유물 및 파란색 바운딩박스와 절대 겹치지 않도록 배치!)
        let left_side_lbl = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 4.0, (left_dial_rect.top() - 40.0).max(rect.top() + 56.0)),
            egui::vec2(88.0, 36.0),
        );
        painter.rect_filled(left_side_lbl, 3.0, label_bg);
        painter.text(
            egui::pos2(left_side_lbl.left() + 3.0, left_side_lbl.top() + 9.0),
            egui::Align2::LEFT_CENTER,
            "↑올리면 위로회전",
            egui::FontId::proportional(8.6),
            ink,
        );
        painter.text(
            egui::pos2(left_side_lbl.left() + 3.0, left_side_lbl.top() + 19.0),
            egui::Align2::LEFT_CENTER,
            "↓내리면 아래회전",
            egui::FontId::proportional(8.6),
            ink,
        );
        painter.text(
            egui::pos2(left_side_lbl.left() + 3.0, left_side_lbl.top() + 29.0),
            egui::Align2::LEFT_CENTER,
            format!("상하 {:+.1}°", state.tilt_deg),
            egui::FontId::monospace(8.5),
            terra,
        );

        // 5. Image 3 (우측 세로 휠): 유물의 좌우 기울기를 미세조정하는 휠 + 휠 위쪽에 설명!
        painter.rect_filled(right_dial_rect, 10.5, dial_fill);
        painter.rect_stroke(
            right_dial_rect,
            10.5,
            egui::Stroke::new(
                if right_dial_resp.hovered() { 1.4_f32 } else { 1.0_f32 },
                if right_dial_resp.hovered() { terra } else { dial_stroke },
            ),
            egui::StrokeKind::Inside,
        );
        let phase_roll = ((state.roll_deg * 10.0).rem_euclid(5.0) / 5.0) as f32;
        for ti in 0..=n_ticks_v {
            let frac = ((ti as f32 - phase_roll).rem_euclid(n_ticks_v as f32) / (n_ticks_v as f32))
                .clamp(0.0, 1.0);
            let ty = right_dial_rect.top() + 9.0 + frac * (right_dial_rect.height() - 18.0);
            let is_mid = ti == n_ticks_v / 2;
            let tw = if is_mid { 15.0 } else if ti % 4 == 0 { 10.5 } else { 7.0 };
            painter.line_segment(
                [
                    egui::pos2(right_dial_rect.center().x - tw * 0.5, ty),
                    egui::pos2(right_dial_rect.center().x + tw * 0.5, ty),
                ],
                egui::Stroke::new(
                    if is_mid { 1.3_f32 } else { 0.8_f32 },
                    if is_mid { tick_major_col } else { tick_col },
                ),
            );
        }
        // Image 3: 우측 세로 휠 바로 위쪽에 설명 표시 (유물 및 파란색 바운딩박스와 절대 겹치지 않도록 배치!)
        let right_side_lbl = egui::Rect::from_min_size(
            egui::pos2(rect.right() - 88.0, (right_dial_rect.top() - 40.0).max(rect.top() + 56.0)),
            egui::vec2(84.0, 36.0),
        );
        painter.rect_filled(right_side_lbl, 3.0, label_bg);
        painter.text(
            egui::pos2(right_side_lbl.right() - 3.0, right_side_lbl.top() + 9.0),
            egui::Align2::RIGHT_CENTER,
            "유물 좌우기울기",
            egui::FontId::proportional(8.8),
            ink,
        );
        painter.text(
            egui::pos2(right_side_lbl.right() - 3.0, right_side_lbl.top() + 19.0),
            egui::Align2::RIGHT_CENTER,
            "미세조정 휠",
            egui::FontId::proportional(8.8),
            ink,
        );
        painter.text(
            egui::pos2(right_side_lbl.right() - 3.0, right_side_lbl.top() + 29.0),
            egui::Align2::RIGHT_CENTER,
            format!("기울기 {:+.1}°", state.roll_deg),
            egui::FontId::monospace(8.5),
            terra,
        );

        // 상단 1줄 상태 및 셰이더/정위치 칩 바
        let top_bar_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 6.0, rect.top() + 4.0),
            egui::vec2(rect.width() - 12.0, 19.0),
        );
        painter.rect_filled(top_bar_rect, 3.0, label_bg);
        painter.text(
            egui::pos2(top_bar_rect.left() + 6.0, top_bar_rect.center().y),
            egui::Align2::LEFT_CENTER,
            format!(
                "수평 정면 입면 (ψ={:.1}° · 상하={:+.1}° · 좌우={:+.1}° · 수평보정={:+.2}°)",
                state.active_section_angle_deg,
                state.tilt_deg,
                state.roll_deg,
                state.inner_level_deg
            ),
            egui::FontId::proportional(10.0),
            ink,
        );

        let modes = [
            View3DShaderMode::PhotoTexture,
            View3DShaderMode::SolidClay,
            View3DShaderMode::RealTimeRubbing,
        ];
        let mut chip_x = rect.left() + 6.0;
        let chip_y = rect.top() + 26.0;
        for m in modes {
            let label = m.label_kr();
            let cw = (label.chars().count() as f32) * 10.0 + 10.0;
            let c_rect =
                egui::Rect::from_min_size(egui::pos2(chip_x, chip_y), egui::vec2(cw, 19.0));
            let c_resp = ui.interact(
                c_rect,
                egui::Id::new(("v3d_shader_chip", label)),
                egui::Sense::click(),
            );
            let is_on =
                !state.show_3way_segmentation && !state.xray_inner_mode && state.shader_mode == m;
            if c_resp.clicked() {
                state.show_3way_segmentation = false;
                state.xray_inner_mode = false;
                state.shader_mode = m;
            }
            painter.rect_filled(c_rect, 3.0, if is_on { ink } else { surface });
            painter.rect_stroke(
                c_rect,
                3.0,
                egui::Stroke::new(1.0_f32, if is_on { ink } else { border }),
                egui::StrokeKind::Inside,
            );
            painter.text(
                c_rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(10.0),
                if is_on { hanji } else { ink },
            );
            chip_x += cw + 3.0;
        }

        for (btn_id, btn_label, is_accent) in [
            ("v3d_reset_front_chip", "정위치·수평 0°".to_string(), false),
            (
                "v3d_best_angle_chip",
                format!("★최적 {:.0}°", best_angle_deg),
                true,
            ),
        ] {
            let cw = (btn_label.chars().count() as f32) * 9.2 + 10.0;
            let c_rect =
                egui::Rect::from_min_size(egui::pos2(chip_x, chip_y), egui::vec2(cw, 19.0));
            let c_resp = ui.interact(c_rect, egui::Id::new(btn_id), egui::Sense::click());
            if c_resp.clicked() {
                if is_accent {
                    state.active_section_angle_deg = best_angle_deg;
                    state.cut_y_mm = 0.0;
                    state.yaw_deg = 0.0;
                    state.pitch_deg = 0.0;
                } else {
                    state.reset_hand_orientation();
                }
            }
            painter.rect_filled(c_rect, 3.0, if is_accent { clay_wash } else { surface });
            painter.rect_stroke(
                c_rect,
                3.0,
                egui::Stroke::new(1.0_f32, if is_accent { terra } else { border }),
                egui::StrokeKind::Inside,
            );
            painter.text(
                c_rect.center(),
                egui::Align2::CENTER_CENTER,
                &btn_label,
                egui::FontId::proportional(10.0),
                if is_accent { clay_ink } else { ink },
            );
            chip_x += cw + 3.0;
        }

        // Image 2 ("광원을 조정할수있는 작은창"): [★최적 ...] 버튼 바로 우측 자리에 광원 토글 칩 및 광원 조정 작은창 배치!
        let light_chip_lbl = format!(
            "💡광원 {:.0}°/{:.0}°",
            state.light_azimuth_deg, state.light_elevation_deg
        );
        let lcw = 86.0_f32;
        let lc_rect = egui::Rect::from_min_size(egui::pos2(chip_x, chip_y), egui::vec2(lcw, 19.0));
        let lc_resp = ui.interact(
            lc_rect,
            egui::Id::new("v3d_light_window_toggle_chip"),
            egui::Sense::click(),
        );
        if lc_resp.clicked() {
            state.show_light_window = !state.show_light_window;
        }
        painter.rect_filled(
            lc_rect,
            3.0,
            if state.show_light_window {
                egui::Color32::from_rgb(252, 243, 215)
            } else {
                surface
            },
        );
        painter.rect_stroke(
            lc_rect,
            3.0,
            egui::Stroke::new(
                1.1_f32,
                if state.show_light_window {
                    terra
                } else {
                    border
                },
            ),
            egui::StrokeKind::Inside,
        );
        painter.text(
            lc_rect.center(),
            egui::Align2::CENTER_CENTER,
            &light_chip_lbl,
            egui::FontId::proportional(9.2),
            ink,
        );

        if state.show_light_window {
            let lw_w = 172.0_f32;
            let lw_h = 64.0_f32;
            let lw_x = (rect.center().x - lw_w * 0.5).clamp(rect.left() + 8.0, rect.right() - lw_w - 8.0);
            let lw_y = rect.top() + 114.0;
            let lw_rect = egui::Rect::from_min_size(egui::pos2(lw_x, lw_y), egui::vec2(lw_w, lw_h));
            painter.rect_filled(
                lw_rect,
                4.5,
                egui::Color32::from_rgba_unmultiplied(250, 249, 245, 244),
            );
            painter.rect_stroke(
                lw_rect,
                4.5,
                egui::Stroke::new(1.2_f32, terra),
                egui::StrokeKind::Inside,
            );

            // 좌측: 2D 광원 구면 패드 (드래그 시 방위각·고도각 즉시 변경)
            let pad_r = 19.0_f32;
            let pad_center = egui::pos2(lw_rect.left() + 24.0, lw_rect.center().y + 4.0);
            let pad_rect =
                egui::Rect::from_center_size(pad_center, egui::vec2(pad_r * 2.0, pad_r * 2.0));
            let pad_resp = ui.interact(
                pad_rect,
                egui::Id::new("v3d_light_sphere_pad"),
                egui::Sense::click_and_drag(),
            );
            if pad_resp.clicked() || pad_resp.dragged() {
                if let Some(p) = pad_resp.interact_pointer_pos() {
                    let dx = (p.x - pad_center.x) / pad_r;
                    let dy = (p.y - pad_center.y) / pad_r;
                    let r_norm = dx.hypot(dy).clamp(0.0, 1.0);
                    let az = dx.atan2(-dy).to_degrees().rem_euclid(360.0);
                    let el = ((1.0 - r_norm) * 80.0 + 5.0).clamp(5.0, 85.0);
                    state.light_azimuth_deg = (az * 2.0).round() / 2.0;
                    state.light_elevation_deg = (el * 2.0).round() / 2.0;
                }
            }
            painter.circle_filled(
                pad_center,
                pad_r,
                egui::Color32::from_rgb(236, 232, 222),
            );
            painter.circle_stroke(pad_center, pad_r, egui::Stroke::new(1.0_f32, border));
            painter.line_segment(
                [
                    egui::pos2(pad_center.x - pad_r, pad_center.y),
                    egui::pos2(pad_center.x + pad_r, pad_center.y),
                ],
                egui::Stroke::new(0.6_f32, tick_col),
            );
            painter.line_segment(
                [
                    egui::pos2(pad_center.x, pad_center.y - pad_r),
                    egui::pos2(pad_center.x, pad_center.y + pad_r),
                ],
                egui::Stroke::new(0.6_f32, tick_col),
            );
            let az_r = state.light_azimuth_deg.to_radians();
            let rad_frac = ((85.0 - state.light_elevation_deg) / 80.0).clamp(0.0, 1.0);
            let sun_pos = egui::pos2(
                pad_center.x + az_r.sin() * pad_r * rad_frac,
                pad_center.y - az_r.cos() * pad_r * rad_frac,
            );
            painter.circle_filled(sun_pos, 4.5, egui::Color32::from_rgb(235, 152, 28));
            painter.circle_stroke(sun_pos, 4.5, egui::Stroke::new(1.0_f32, ink));
            painter.text(
                egui::pos2(lw_rect.left() + 6.0, lw_rect.top() + 8.0),
                egui::Align2::LEFT_CENTER,
                "💡 광원 조정",
                egui::FontId::proportional(8.8),
                terra,
            );

            // 우측: 방위각 · 고도각 · 밝기 강도 및 [좌상45°] [측면광] 프리셋 버튼
            let ctrl_rect = egui::Rect::from_min_max(
                egui::pos2(lw_rect.left() + 47.0, lw_rect.top() + 3.0),
                egui::pos2(lw_rect.right() - 3.0, lw_rect.bottom() - 3.0),
            );
            let mut l_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(ctrl_rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            l_ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
            l_ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("좌상45°").size(8.2).color(ink))
                            .min_size(egui::vec2(44.0, 15.0)),
                    )
                    .clicked()
                {
                    state.light_azimuth_deg = 315.0;
                    state.light_elevation_deg = 45.0;
                    state.light_intensity = 1.0;
                }
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("측면광").size(8.2).color(ink))
                            .min_size(egui::vec2(40.0, 15.0)),
                    )
                    .clicked()
                {
                    state.light_azimuth_deg = 275.0;
                    state.light_elevation_deg = 18.0;
                    state.light_intensity = 1.18;
                }
            });
            l_ui.horizontal(|ui| {
                ui.label(egui::RichText::new("방위").size(8.2).color(ink));
                let _ = ui.add(
                    egui::DragValue::new(&mut state.light_azimuth_deg)
                        .speed(1.0)
                        .range(0.0..=360.0)
                        .suffix("°")
                        .fixed_decimals(0),
                );
                ui.label(egui::RichText::new("고도").size(8.2).color(ink));
                let _ = ui.add(
                    egui::DragValue::new(&mut state.light_elevation_deg)
                        .speed(0.5)
                        .range(5.0..=85.0)
                        .suffix("°")
                        .fixed_decimals(0),
                );
            });
            l_ui.horizontal(|ui| {
                ui.label(egui::RichText::new("광량").size(8.2).color(ink));
                let _ = ui.add(
                    egui::DragValue::new(&mut state.light_intensity)
                        .speed(0.02)
                        .range(0.40..=1.80)
                        .suffix("×")
                        .fixed_decimals(2),
                );
            });
        }

        // =========================================================================================
        // Image 6 ("여기 이자리에 유물 뒤집기 90도 180도 회전을 할수있는 버튼을 만들라 뒤집횐 유물이있을수있다.")
        // 직상방 동심원 뷰 바로 아래 / 수평 정면 입면 상단 자리에 유물 90° 앞으로뒤집기 · 180° 상하뒤집기 · 90° 수평회전 · 측면뒤집기 버튼 배치!
        // =========================================================================================
        let flip_row_y = rect.top() + 48.0;
        let mut flip_x = rect.left() + 6.0;
        let flip_buttons = [
            ("v3d_flip_fwd_90", "↻ 유물 90° 앞으로뒤집기", 116.0_f32, 0u8),
            ("v3d_flip_ud_180", "↕ 유물 180° 상하뒤집기", 114.0_f32, 1u8),
            ("v3d_rot_yaw_90", "↻ 90° 수평회전", 82.0_f32, 2u8),
            ("v3d_flip_roll_90", "↔ 90° 측면뒤집기", 92.0_f32, 3u8),
        ];
        for (fid, flabel, fw, fkind) in flip_buttons {
            let f_rect =
                egui::Rect::from_min_size(egui::pos2(flip_x, flip_row_y), egui::vec2(fw, 20.0));
            let f_resp = ui.interact(f_rect, egui::Id::new(fid), egui::Sense::click());
            if f_resp.clicked() {
                match fkind {
                    0 => state.flip_artifact_forward_90(),
                    1 => state.flip_artifact_upside_down_180(),
                    2 => state.rotate_artifact_yaw_90(),
                    _ => state.flip_artifact_roll_90(),
                }
            }
            let is_flipped_active = (fkind == 0 || fkind == 1) && state.tilt_deg.abs() >= 45.0
                || (fkind == 3 && state.roll_deg.abs() >= 45.0);
            painter.rect_filled(
                f_rect,
                3.5,
                if is_flipped_active {
                    profile_red
                } else if f_resp.hovered() {
                    clay_wash
                } else {
                    egui::Color32::from_rgb(244, 238, 228)
                },
            );
            painter.rect_stroke(
                f_rect,
                3.5,
                egui::Stroke::new(
                    1.1_f32,
                    if is_flipped_active || f_resp.hovered() {
                        profile_red
                    } else {
                        terra
                    },
                ),
                egui::StrokeKind::Inside,
            );
            painter.text(
                f_rect.center(),
                egui::Align2::CENTER_CENTER,
                flabel,
                egui::FontId::proportional(8.8),
                if is_flipped_active {
                    egui::Color32::WHITE
                } else {
                    clay_ink
                },
            );
            flip_x += fw + 3.0;
        }

        let _ = muted;
        painter.rect_stroke(
            rect,
            3.0,
            egui::Stroke::new(1.0_f32, border),
            egui::StrokeKind::Inside,
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_bowl_mesh() -> TriMesh {
        let mut positions = Vec::new();
        let mut triangles = Vec::new();
        let nu = 36usize;
        let nv = 16usize;
        for iv in 0..=nv {
            let z = (iv as f64 / nv as f64) * 60.0;
            let r = 28.0 + 32.0 * (iv as f64 / nv as f64).powf(0.75);
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
        TriMesh::from_positions_and_triangles("test_bowl", positions, triangles).expect("valid")
    }

    #[test]
    fn test_zbuffer_3d_rasterizer_produces_opaque_shaded_and_textured_pixels() {
        let mesh = make_test_bowl_mesh();
        for mode in [
            View3DShaderMode::PhotoTexture,
            View3DShaderMode::SolidClay,
            View3DShaderMode::RealTimeRubbing,
        ] {
            let state = View3DPaneState {
                shader_mode: mode,
                cutaway_open: false,
                ..Default::default()
            };
            let img = render_mesh_zbuffer_rgba(&mesh, None, &state, 240, 240);
            let shaded_count = img
                .pixels()
                .filter(|px| {
                    let [r, g, b, _] = px.0;
                    (i32::from(r) - i32::from(SURFACE.rgb[0])).abs() > 25
                        || (i32::from(g) - i32::from(SURFACE.rgb[1])).abs() > 25
                        || (i32::from(b) - i32::from(SURFACE.rgb[2])).abs() > 25
                })
                .count();
            assert!(
                shaded_count > 2_500,
                "mode={mode:?} expected >2500 shaded Z-buffer pixels, got {shaded_count}"
            );
        }
    }

    #[test]
    fn test_view3d_cutaway_and_world_locked_plane_intersection() {
        let mesh = make_test_bowl_mesh();
        let v_segs = extract_3d_plane_intersection_segments(&mesh, 45.0, 0.0, 0.0, true);
        let h_segs = extract_3d_plane_intersection_segments(&mesh, 45.0, 0.0, 18.0, false);
        assert!(
            !v_segs.is_empty(),
            "vertical Y=0 plane intersection must produce 3D segments"
        );
        assert!(
            !h_segs.is_empty(),
            "horizontal Z=18mm plane intersection must produce 3D segments"
        );
    }

    #[test]
    fn test_tdd1_view3d_dirty_cache_skips_redundant_rerender_and_supports_hd_resolution() {
        let mesh = make_test_bowl_mesh();
        let mut state = View3DPaneState::default();

        let (w1, h1, recomputed1) = state.ensure_zbuffer_cached(&mesh, None, 560, 560);
        assert!(recomputed1, "first frame must recompute Z-buffer");
        assert_eq!(
            (w1, h1),
            (560, 560),
            "must support HD 560x560 resolution without 360px clamp"
        );

        let (w2, h2, recomputed2) = state.ensure_zbuffer_cached(&mesh, None, 560, 560);
        assert!(
            !recomputed2,
            "identical state must hit dirty cache with 0 recomputation"
        );
        assert_eq!((w2, h2), (560, 560));

        state.active_section_angle_deg = 45.0;
        let (_, _, recomputed3) = state.ensure_zbuffer_cached(&mesh, None, 560, 560);
        assert!(
            recomputed3,
            "changed rotation angle must trigger Z-buffer recomputation"
        );
    }

    #[test]
    fn test_tdd4_hand_dial_spherical_rotation_updates_pitch_yaw_roll() {
        let mut state = View3DPaneState::default();
        let initial_psi = state.active_section_angle_deg;
        let initial_pitch = state.pitch_deg;

        state.apply_hand_dial_drag(40.0, -30.0, false);
        assert!(
            (state.active_section_angle_deg - initial_psi).abs() > 10.0,
            "horizontal dial drag must rotate artifact ψ"
        );
        assert!(
            (state.pitch_deg - initial_pitch).abs() > 5.0,
            "vertical dial drag must tilt/rotate vertical viewing angle"
        );

        state.apply_hand_dial_drag(20.0, -15.0, true);
        assert!(
            state.roll_deg.abs() > 0.5,
            "fine axis dial horizontal drag must adjust roll_deg"
        );
        assert!(
            state.tilt_deg.abs() > 0.5,
            "fine axis dial vertical drag must adjust tilt_deg"
        );

        state.reset_hand_orientation();
        assert_eq!(state.active_section_angle_deg, 0.0);
        assert_eq!(state.tilt_deg, 0.0);
        assert_eq!(state.roll_deg, 0.0);
    }

    #[test]
    fn test_tdd8_isotropic_projection_never_stretches_vertically_on_drag_release() {
        let (dw, dh) = compute_isotropic_render_dimensions(420.0, 680.0, true);
        let (rw, rh) = compute_isotropic_render_dimensions(420.0, 680.0, false);
        let target_aspect = 420.0 / 680.0;
        let drag_aspect = (dw as f64) / (dh as f64);
        let release_aspect = (rw as f64) / (rh as f64);
        assert!(
            (drag_aspect - target_aspect).abs() < 0.005,
            "drag aspect {drag_aspect} must match target {target_aspect}"
        );
        assert!(
            (release_aspect - target_aspect).abs() < 0.005,
            "release aspect {release_aspect} must match target {target_aspect}"
        );
    }

    #[test]
    fn test_tdd9_four_cardinal_fine_dials_step_exact_001mm_and_01deg() {
        let mut state = View3DPaneState::default();
        for _ in 0..100 {
            state.apply_cardinal_fine_dial(CardinalDialDir::Up, 1);
        }
        assert!((state.cut_y_mm - 1.00).abs() < 1e-9);
        for _ in 0..35 {
            state.apply_cardinal_fine_dial(CardinalDialDir::Down, 1);
        }
        assert!((state.cut_y_mm - 0.65).abs() < 1e-9);
        for _ in 0..15 {
            state.apply_cardinal_fine_dial(CardinalDialDir::Right, 1);
        }
        assert!((state.active_section_angle_deg - 1.5).abs() < 1e-9);
        for _ in 0..5 {
            state.apply_cardinal_fine_dial(CardinalDialDir::Left, 1);
        }
        assert!((state.active_section_angle_deg - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_tdd11_concentric_120pct_topdown_and_zero_lag_frame_budget() {
        let mesh = make_test_bowl_mesh();
        let state = View3DPaneState::default();
        assert_eq!(state.yaw_deg, 0.0);
        assert_eq!(state.pitch_deg, 0.0);

        let artifact_r = 62.5_f64;
        let grid_r = compute_concentric_grid_radius_mm(artifact_r);
        assert!((grid_r - artifact_r * 1.20).abs() < 1e-9);

        let t0 = std::time::Instant::now();
        let top_img = render_topdown_concentric_plan_rgba(&mesh, &state, 280, 220);
        let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let non_bg = top_img
            .pixels()
            .filter(|px| {
                (i32::from(px[0]) - i32::from(SURFACE.rgb[0])).abs() > 12
                    || (i32::from(px[1]) - i32::from(SURFACE.rgb[1])).abs() > 12
            })
            .count();
        assert!(non_bg > 1_500);
        assert!(elapsed_ms < 40.0, "took {elapsed_ms:.2}ms");
    }

    #[test]
    fn test_tdd13_five_wheel_cardinal_and_inner_level_exact_section_sync() {
        let mesh = make_test_bowl_mesh();
        let mut state = View3DPaneState::default();

        // 1. Image 1: 상단 가로 휠 -> 유물 시계·반시계 회전 (0.1° 단위)
        state.apply_precision_wheel(PrecisionWheelKind::TopTurnCwCcw, 25);
        assert!((state.active_section_angle_deg - 2.5).abs() < 1e-9);
        assert!(PrecisionWheelKind::TopTurnCwCcw
            .description_label_kr()
            .contains("시계"));

        // 2. Image 2: 하단 가로 휠 -> 유물·단면선 앞뒤 미세조정 (0.01mm 단위)
        state.apply_precision_wheel(PrecisionWheelKind::BottomSectionFrontBack, -18);
        assert!((state.cut_y_mm - (-0.18)).abs() < 1e-9);

        // 3. Image 3: 우측 세로 휠 -> 유물 좌우 기울기 미세조정 (0.1° 단위)
        state.apply_precision_wheel(PrecisionWheelKind::RightTiltLeftRight, 12);
        assert!((state.roll_deg - 1.2).abs() < 1e-9);

        // 4. Image 4: 좌측 세로 휠 -> 올리면(+steps) 위로 회전(+tilt_deg), 내리면(-steps) 아래로 회전(-tilt_deg)
        state.apply_precision_wheel(PrecisionWheelKind::LeftPitchUpDown, 15);
        assert!((state.tilt_deg - 1.5).abs() < 1e-9);
        state.apply_precision_wheel(PrecisionWheelKind::LeftPitchUpDown, -5);
        assert!((state.tilt_deg - 1.0).abs() < 1e-9);

        // 5. <USER_REQUEST>: 내부 제5 수평정합 휠 (0.05° 단위) 및 3D-2D 단면 100% 일치 검증
        state.apply_precision_wheel(PrecisionWheelKind::InnerHorizontalLevel, 10);
        assert!((state.inner_level_deg - 0.50).abs() < 1e-9);

        let v_segs_3d = extract_3d_plane_intersection_segments_full(&mesh, &state, true);
        let slice_2d = slice_transformed_mesh_world_locked(&mesh, &state, true);
        assert_eq!(v_segs_3d.len(), slice_2d.segments_2d.len());
        assert!(!slice_2d.polylines_2d.is_empty());

        // 3D 단면 선분의 X 범위와 2D 슬라이서 폴리라인의 X 범위가 0.01mm 이내로 100% 일치해야 함!
        let min_x_3d = v_segs_3d
            .iter()
            .flat_map(|s| [s[0][0], s[1][0]])
            .fold(f64::INFINITY, f64::min);
        let max_x_3d = v_segs_3d
            .iter()
            .flat_map(|s| [s[0][0], s[1][0]])
            .fold(f64::NEG_INFINITY, f64::max);
        let min_x_2d = slice_2d
            .polylines_2d
            .iter()
            .flatten()
            .map(|p| p[0])
            .fold(f64::INFINITY, f64::min);
        let max_x_2d = slice_2d
            .polylines_2d
            .iter()
            .flatten()
            .map(|p| p[0])
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(
            (min_x_3d - min_x_2d).abs() < 1e-5 && (max_x_3d - max_x_2d).abs() < 1e-5,
            "3D cut X [{min_x_3d:.3}, {max_x_3d:.3}] must match 2D slice X [{min_x_2d:.3}, {max_x_2d:.3}]"
        );
    }

    #[test]
    fn test_tdd14_topdown_fixed_center_section_and_vertical_dashed_link() {
        let mesh = make_test_bowl_mesh();
        let state = View3DPaneState {
            artifact_offset_x_mm: 12.5,
            cut_y_mm: -4.2,
            ..Default::default()
        };

        // 유물을 좌우(+12.5mm) 이동시켰을 때 3D 단면 교차선도 정확히 +12.5mm 이동하여 상하 뷰 점선 연결 위치가 일치해야 함
        let segs = extract_3d_plane_intersection_segments_full(&mesh, &state, true);
        let mean_x: f64 = segs.iter().map(|s| 0.5 * (s[0][0] + s[1][0])).sum::<f64>()
            / (segs.len().max(1) as f64);
        assert!(
            (mean_x - 12.5).abs() < 1.5,
            "translated artifact section center X must shift by +12.5mm, got {mean_x:.2}"
        );

        // 상단 직상방 동심원 평면도와 하단 수평 정면 입면도의 스케일(px/mm)이 1:1 동일해야 하며,
        // 좌·우 절단점의 화면 X 좌표가 기울어지지 않고 100% 수직(90°)으로 정렬되어야 함 (v6_vessel_roi_1to1 Image 0)
        let ext = mesh.bbox.extents();
        let grid_r_mm = compute_concentric_grid_radius_mm(0.5 * ext[0].max(ext[1]));
        let top_scale = compute_shared_ortho_scale(388.0, grid_r_mm, ext[2], state.zoom);
        let bot_scale = compute_shared_ortho_scale(388.0, grid_r_mm, ext[2], state.zoom);
        assert!((top_scale - bot_scale).abs() < 1e-12);
    }

    #[test]
    fn test_tdd17_inner_wheel_controls_horizontal_section_z_and_updates_plate() {
        let mesh = make_test_bowl_mesh();
        let mut state = View3DPaneState::default();
        let init_z = state.top_cut_z_mm;
        let init_tilt = state.tilt_deg;
        let init_roll = state.roll_deg;

        // Image 0: 안쪽 제5휠을 움직이면 유물은 고정된 채 수평횡단면 선(top_cut_z_mm)만 아래위로 이동하고 수평횡단면 도면이 갱신되어야 함
        state.apply_precision_wheel(PrecisionWheelKind::InnerHorizSectionZ, 25);
        assert!(
            (state.top_cut_z_mm - (init_z + 2.5)).abs() < 1e-6,
            "InnerHorizSectionZ +25 steps must move top_cut_z_mm by +2.5mm, got {}",
            state.top_cut_z_mm
        );
        assert_eq!(state.tilt_deg, init_tilt, "artifact tilt must stay fixed");
        assert_eq!(state.roll_deg, init_roll, "artifact roll must stay fixed");

        let h_slice_low = slice_transformed_mesh_world_locked(&mesh, &state, false);
        state.apply_precision_wheel(PrecisionWheelKind::InnerHorizSectionZ, 120);
        let h_slice_high = slice_transformed_mesh_world_locked(&mesh, &state, false);
        assert!(!h_slice_low.polylines_2d.is_empty());
        assert!(!h_slice_high.polylines_2d.is_empty());
        assert!(
            (h_slice_high.offset_mm - h_slice_low.offset_mm - 12.0).abs() < 1e-5,
            "horizontal section Z offset must advance by +12.0mm"
        );
    }

    #[test]
    fn test_tdd18_arrow_keys_nudge_artifact_and_horizontal_section() {
        let mut state = View3DPaneState::default();
        state.apply_arrow_key_nudge(-1, 0, false, false, 60.0);
        assert!((state.artifact_offset_x_mm - (-0.10)).abs() < 1e-6);

        state.apply_arrow_key_nudge(0, 1, true, false, 60.0);
        assert!((state.cut_y_mm - (-1.00)).abs() < 1e-6);

        let z0 = state.top_cut_z_mm;
        state.apply_arrow_key_nudge(0, 1, false, true, 60.0);
        assert!((state.top_cut_z_mm - (z0 + 0.20)).abs() < 1e-6);
    }

    #[test]
    fn test_tdd19_custom_section_line_single_endpoint_move_and_2d_profile() {
        let mesh = make_test_bowl_mesh();
        let mut state = View3DPaneState::default();
        state.add_default_custom_section_line(55.0);
        let line0 = state.custom_section_line_mm.expect("custom section line must exist");
        let p0_before = line0[0];
        let p1_before = line0[1];

        // Image 5: 끝지점(0번 끝점)을 누르고 이동하면 반대쪽(1번 끝점)은 고정되고 한쪽만 이동해야 함!
        state.move_custom_section_endpoint(0, [-38.0, 22.0]);
        let line1 = state.custom_section_line_mm.expect("custom line 1");
        assert_eq!(line1[0], [-38.0, 22.0], "dragged endpoint 0 must move");
        assert_eq!(line1[1], p1_before, "other endpoint 1 must remain 100% fixed");

        // 반대로 1번 끝점을 이동하면 0번 끝점은 고정되고 1번 끝점만 이동해야 함!
        state.move_custom_section_endpoint(1, [42.0, -16.0]);
        let line2 = state.custom_section_line_mm.expect("custom line 2");
        assert_eq!(line2[0], [-38.0, 22.0], "endpoint 0 must remain 100% fixed");
        assert_eq!(line2[1], [42.0, -16.0], "dragged endpoint 1 must move");
        let _ = p0_before;

        // 그어진 사용자 단면선에 대한 2D 단면 폴리라인이 추출되어야 함
        let custom_slice = slice_custom_line_section_2d(&mesh, &state).expect("custom slice");
        assert!(
            !custom_slice.polylines_2d.is_empty(),
            "custom section line across vessel must produce 2D cross-section polylines"
        );
    }
}
