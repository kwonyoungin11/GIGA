use eframe::egui;
use image::RgbaImage;
use strata_core::{SurfaceRegion, TriMesh};

use super::geometry::{clamp_ordered, extract_3d_plane_intersection_segments_full};
use super::raster::{render_mesh_zbuffer_rgba, render_topdown_concentric_plan_rgba};

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

/// 3D 뷰포트 셰이더 칩에 노출되는 모드 목록. 백엔드 5종과 항상 일치해야 한다.
pub const SHADER_CHIP_MODES: [View3DShaderMode; 5] = [
    View3DShaderMode::PhotoTexture,
    View3DShaderMode::SolidClay,
    View3DShaderMode::RealTimeRubbing,
    View3DShaderMode::Segmentation3Way,
    View3DShaderMode::XRayInner,
];

/// 3D 뷰포트 마우스 드래그 조작 모드.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DragMode3D {
    /// 기본값: 절단틀(`P_cut`)은 월드 중앙에 고정하고 유물만 회전축 둘레로 회전(`ψ`)
    TurntableArtifact,
    /// 카메라 궤도 회전 (`cam_yaw_deg`, `cam_pitch_deg`)
    #[allow(dead_code)]
    // ui.rs 드래그 match 는 처리하지만 선택 UI 가 아직 없어 생성 지점이 없다(향후 배선용).
    CameraOrbit,
    /// 바닥·천공축 미세 기울기 조절 (`0.1°` 단위)
    #[allow(dead_code)]
    // ui.rs 드래그 match 는 처리하지만 선택 UI 가 아직 없어 생성 지점이 없다(향후 배선용).
    AxisTilt,
}

/// 기존 호환용 4방향 미세 다이얼 열거형 (`test_tdd9` 호환 유지).
#[allow(dead_code)] // 테스트(test_tdd9)가 4방향 다이얼 스텝을 검증한다(테스트 전용 API).
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
    #[allow(dead_code)]
    // 테스트(test_tdd13)가 5번째 휠을 생성해 검증한다(테스트 전용 조작 경로).
    InnerHorizontalLevel,
    /// Image 0 (수평횡단면 전용 휠): 유물은 고정하고 수평횡단면 절단선(`top_cut_z_mm`)만 아래위로 이동시켜 수평횡단면 도면을 그리는 휠 (`0.1mm` 스텝)
    InnerHorizSectionZ,
}

impl PrecisionWheelKind {
    /// 각 휠의 사용자 인터페이스 설명 문구 (`Image 0 ~ 4` 및 `<USER_REQUEST>` 반영).
    #[allow(dead_code)] // 테스트(test_tdd13)가 UI 문구를 검증한다(테스트 전용 조회 API).
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

    pub fn build_cache_key(
        &self,
        mesh_name: &str,
        width: u32,
        height: u32,
    ) -> View3DRenderCacheKey {
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
        let (w, h) =
            compute_isotropic_render_dimensions(width as f32, height as f32, self.is_interacting);
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
        let (w, h) =
            compute_isotropic_render_dimensions(width as f32, height as f32, self.is_interacting);
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
    #[allow(dead_code)] // 테스트(test_tdd9)가 0.01mm/0.1° 스텝을 검증한다(테스트 전용 API).
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

    /// 새 유물을 열 때 수평횡단면 절단선의 기본 높이를 잡는다.
    /// `.ap2` 절단면이 있으면 그 값을, 없으면 기고의 절반을 쓴다.
    /// **높이가 2mm 미만인 편평 유물(석기·소편)에서도 경계가 뒤집히지 않아야 한다.**
    pub fn initialize_top_cut_for_mesh(&mut self, mesh: &TriMesh) {
        if self.top_cut_initialized_for_mesh == mesh.name {
            return;
        }
        let height_mm = mesh.bbox.extents()[2].max(1.0);
        self.top_cut_initialized_for_mesh = mesh.name.clone();
        self.top_cut_z_mm = mesh
            .ap2_cut_offset_mm
            .unwrap_or_else(|| clamp_ordered(height_mm * 0.50, 2.0, height_mm));
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

    /// 유물 상하좌우 핸드 다이얼(구면 트랙볼) 드래그 입력을 적용한다.
    #[allow(dead_code)] // 테스트(test_tdd4)가 구면 트랙볼 드래그를 검증한다(테스트 전용 API).
    pub fn apply_hand_dial_drag(&mut self, dx_px: f32, dy_px: f32, fine_axis_tilt_mode: bool) {
        if fine_axis_tilt_mode {
            self.roll_deg = ((self.roll_deg + f64::from(dx_px) * 0.15).clamp(-25.0, 25.0) * 10.0)
                .round()
                / 10.0;
            self.tilt_deg = ((self.tilt_deg + f64::from(dy_px) * 0.15).clamp(-25.0, 25.0) * 10.0)
                .round()
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
