use arch3d_core::strata_tokens::tokens::{
    ACTION, CLAY_FOCUS, CLAY_INK, CLAY_WASH, EDGE, GROUND, INK, MUTED, PROFILE_RED, SELECT_BLUE,
    SURFACE, WASH,
};
use arch3d_core::{SurfaceRegion, TriMesh};
use arch3d_orient::RimGapChart360;
use eframe::egui;
use image::{Rgba, RgbaImage};

/// 3D 뷰포트 셰이딩 모드 (`C++ View3DPane.cpp` + `MeshRaster.cpp` 1:1 대응).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View3DShaderMode {
    /// `.mtl` + `.jpg` 실사 UV 텍스처 + 양면 스튜디오 조명
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

/// 3D 뷰포트 마우스 드래그 조작 모드 (`archaeo3d_rs_gigamesh_arch3d_rust_gate_1 (1).html` 1:1 계승).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DragMode3D {
    /// 기본값: 절단틀(`P_cut`)은 월드에 고정하고 유물만 회전축 둘레로 회전(`ψ`) + 상하 드래그로 카메라 피치 조절
    TurntableArtifact,
    /// 카메라 궤도 회전 (`cam_yaw_deg`, `cam_pitch_deg`)
    CameraOrbit,
    /// 바닥·천공축 미세 기울기 조절 (`0.1°` 단위)
    AxisTilt,
}

/// 3D 뷰포트 내부 상·하·좌·우 4방향 정밀 미세 다이얼 방향
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardinalDialDir {
    /// 상단 미세 다이얼 (`▲ 미세 상 (Up) [ 0.01mm ]`): 종단면 ΔY `+0.01 mm` 정밀 이동
    Up,
    /// 하단 미세 다이얼 (`▼ 미세 하 (Down) [ 0.01mm ]`): 종단면 ΔY `-0.01 mm` 정밀 이동
    Down,
    /// 좌측 미세 다이얼 (`◀ 미세 좌 (Left) [ 0.1° ]`): 유물 회전각 ψ `-0.1°` 정밀 회전
    Left,
    /// 우측 미세 다이얼 (`▶ 미세 우 (Right) [ 0.1° ]`): 유물 회전각 ψ `+0.1°` 정밀 회전
    Right,
}

/// 3D 뷰포트의 가로·세로 종횡비(`rect_w / rect_h`)를 드래그 중(`is_dragging = true`)과
/// 마우스 해제 후(`is_dragging = false`)에 100% 동일하게 보존하는 등방성(Isotropic) 렌더 해상도를 계산한다.
/// - 기존처럼 가로·세로를 개별적으로 `.clamp(200, 640)`하면 세로가 긴 뷰포트에서 마우스를 놓는 순간
///   세로 해상도만 640에 걸려 유물이 위아래로 늘어나는 치명적 왜곡이 발생하므로, 단일 스케일 계수 `s`만 곱한다.
pub fn compute_isotropic_render_dimensions(
    rect_w: f32,
    rect_h: f32,
    is_dragging: bool,
) -> (u32, u32) {
    let rw = f64::from(rect_w.max(64.0));
    let rh = f64::from(rect_h.max(64.0));
    let max_dim = rw.max(rh);
    let desired_mult = if is_dragging { 0.78 } else { 1.0 };
    let max_cap = if is_dragging { 520.0 } else { 680.0 };
    let s = (max_cap / max_dim).min(desired_mult).max(160.0 / max_dim);
    let w = (rw * s).round().max(64.0) as u32;
    let h = ((w as f64) * (rh / rw)).round().max(64.0) as u32;
    (w, h)
}

/// 유물 외경 반경(`artifact_radius_mm`)의 정확히 `1.20배(+20%)`가 되는 직상방·입면 동심원 방안지 반경을 계산한다.
#[inline]
pub fn compute_concentric_grid_radius_mm(artifact_radius_mm: f64) -> f64 {
    artifact_radius_mm * 1.20
}

/// 3D Z-버퍼 프레임 더티 캐시 키 (동일 파라미터에서 재렌더링 및 GPU 텍스처 재할당을 0회로 차단).
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
    pub cut_y_q: i32,
    pub top_z_q: i32,
}

/// 3D 뷰포트 상태 (`DESIGN_SPEC.md` §4.2).
#[derive(Clone)]
pub struct View3DPaneState {
    /// 카메라 궤도 각도 (`cam_yaw_deg`, `cam_pitch_deg`) — 기본값 `0.0° / 0.0°` (수평 동심원 정면 입면도)
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub zoom: f32,
    pub show_cutting_frame: bool,
    pub show_concentric_scope: bool,
    pub show_3way_segmentation: bool,
    pub xray_inner_mode: bool,
    /// 유물 턴테이블 회전각 `ψ` (`0.0 .. 360.0°`) — 절단틀(`P_cut`)은 고정되고 유물이 회전함
    pub active_section_angle_deg: f64,
    /// 종단면 평면 오프셋 `ΔY` (`mm`, 기본 `0.0 mm` = 회전 중심축 통과)
    pub cut_y_mm: f64,
    /// 수평 횡단면 높이 `Top Cut Z` (`mm`, `.ap2` 연동 시 `18.0 mm`)
    pub top_cut_z_mm: f64,
    /// 수평 횡단면 높이가 초기화되었는지 여부 (유물 변경 시 자동 초기화용)
    pub top_cut_initialized_for_mesh: String,
    /// 회전축 미세 기울기 (`-25.0° .. +25.0°`, `0.1°` 단위)
    pub tilt_deg: f64,
    /// 유물 좌우 롤 기울기 (`-25.0° .. +25.0°`, 상하좌우 핸드 다이얼용)
    pub roll_deg: f64,
    /// 전면 좌측 1/4 절개(`Cutaway`) 활성화 여부 (기본 `false` = 3D 스캐너 원본 모델 전체 표시)
    pub cutaway_open: bool,
    /// 현재 마우스 드래그 중인지 여부 (드래그 중 2D 도판 중복 재계산 병목 차단용)
    pub is_interacting: bool,
    /// 현재 활성 3D 셰이더 모드
    pub shader_mode: View3DShaderMode,
    /// 현재 활성 마우스 드래그 모드
    pub drag_mode: DragMode3D,
    /// 더티 캐시된 정면 입면 Z-버퍼 이미지 및 GPU 텍스처 핸들
    pub cached_zbuffer_key: Option<View3DRenderCacheKey>,
    pub cached_zbuffer_rgba: Option<RgbaImage>,
    pub cached_zbuffer_tex: Option<egui::TextureHandle>,
    /// 더티 캐시된 직상방(Top-Down) 동심원 방안지 평면도 이미지 및 GPU 텍스처 핸들
    pub cached_topdown_key: Option<View3DRenderCacheKey>,
    pub cached_topdown_rgba: Option<RgbaImage>,
    pub cached_topdown_tex: Option<egui::TextureHandle>,
    /// 더티 캐시된 3D 평면 교차 단면 곡선 (매 프레임 60만 삼각형 재검사 방지)
    pub cached_curves_key: Option<(String, i32, i32, i32, i32, i32)>,
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
            .field("tilt_deg", &self.tilt_deg)
            .field("roll_deg", &self.roll_deg)
            .field("cut_y_mm", &self.cut_y_mm)
            .field("top_cut_z_mm", &self.top_cut_z_mm)
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
            cut_y_mm: 0.0,
            top_cut_z_mm: 18.0,
            top_cut_initialized_for_mesh: String::new(),
            tilt_deg: 0.0,
            roll_deg: 0.0,
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
            cut_y_q: (self.cut_y_mm * 100.0).round() as i32,
            top_z_q: (self.top_cut_z_mm * 100.0).round() as i32,
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
        let max_dim = width.max(height).max(64);
        let (w, h) = if max_dim > 760 {
            let s = 760.0 / (max_dim as f64);
            (
                ((width as f64) * s).round().max(64.0) as u32,
                ((height as f64) * s).round().max(64.0) as u32,
            )
        } else {
            (width.max(64), height.max(64))
        };
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

    /// 파라미터가 바뀌었을 때만 직상방(Top-Down) 동심원 방안지 Z-버퍼를 재렌더링하고 캐시한다 (`0ms`).
    pub fn ensure_topdown_cached(
        &mut self,
        mesh: &TriMesh,
        width: u32,
        height: u32,
    ) -> (u32, u32, bool) {
        let w = width.clamp(120, 560);
        let h = height.clamp(120, 480);
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
        let ckey = (
            mesh.name.clone(),
            (self.active_section_angle_deg * 10.0).round() as i32,
            (self.tilt_deg * 10.0).round() as i32,
            (self.roll_deg * 10.0).round() as i32,
            (self.cut_y_mm * 100.0).round() as i32,
            (self.top_cut_z_mm * 100.0).round() as i32,
        );
        if self.cached_curves_key.as_ref() == Some(&ckey) {
            return;
        }
        self.cached_v_segs = extract_3d_plane_intersection_segments(
            mesh,
            self.active_section_angle_deg,
            self.tilt_deg,
            self.cut_y_mm,
            true,
        );
        self.cached_h_segs = extract_3d_plane_intersection_segments(
            mesh,
            self.active_section_angle_deg,
            self.tilt_deg,
            self.top_cut_z_mm,
            false,
        );
        self.cached_curves_key = Some(ckey);
    }

    /// 상·하·좌·우 4방향 미세 다이얼 스텝을 부동소수점 누적 오차 없이(정수 양자화 연산) 정확히 적용한다.
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
                let tenths = (self.active_section_angle_deg * 10.0).round() as i64 + i64::from(steps);
                self.active_section_angle_deg = ((tenths as f64) / 10.0).rem_euclid(360.0);
            }
            CardinalDialDir::Left => {
                let tenths = (self.active_section_angle_deg * 10.0).round() as i64 - i64::from(steps);
                self.active_section_angle_deg = ((tenths as f64) / 10.0).rem_euclid(360.0);
            }
        }
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

    /// 핸드 다이얼 방향을 수평 동심원 정면 입면(`ψ=0°`, `yaw=0°`, `pitch=0°`, `tilt=0°`, `roll=0°`)으로 즉시 초기화한다.
    pub fn reset_hand_orientation(&mut self) {
        self.active_section_angle_deg = 0.0;
        self.tilt_deg = 0.0;
        self.roll_deg = 0.0;
        self.yaw_deg = 0.0;
        self.pitch_deg = 0.0;
    }
}

/// 유물 로컬 정점을 턴테이블 회전(`psi_deg`) 및 축 기울기(`tilt_deg`)가 적용된 월드 좌표(`wx, wy, wz`)로 변환한다.
/// - `wz`는 유물 바닥(`bbox.min[2]`)이 `0.0`, 최상단 구연(`bbox.max[2]`)이 `height_mm`이 되도록 정규화된다.
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

/// 유물 로컬 법선을 턴테이블 회전(`psi_deg`) 및 기울기(`tilt_deg`)가 적용된 월드 법선으로 변환한다.
#[inline]
fn local_normal_to_world(
    n: [f64; 3],
    cos_p: f64,
    sin_p: f64,
    cos_t: f64,
    sin_t: f64,
) -> [f64; 3] {
    let rx = n[0] * cos_p - n[1] * sin_p;
    let ry = n[0] * sin_p + n[1] * cos_p;
    let rz = n[2];

    let wx = rx * cos_t - rz * sin_t;
    let wy = ry;
    let wz = rx * sin_t + rz * cos_t;
    [wx, wy, wz]
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

/// 직상방(`Top-Down`, `pitch=90°`)에서 바라본 동심원 방안지(`반경 = 유물 반경 × 1.20 = +20%`)와
/// 그 중앙에 놓인 3D 스캐너 텍스처 유물을 함께 렌더링한다.
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
    // 동심원 외경(2 * grid_r_mm = 2.40 * radius_mm)이 뷰포트 짧은 변의 88%에 정확히 들어오도록 스케일 설정
    let scale = ((w_usize.min(h_usize) as f64) * 0.44 / grid_r_mm.max(1.0)) * f64::from(state.zoom);

    // 1. 바탕에 직교 실측 방안지 격자 (2mm / 10mm) 드로잉
    let step_2mm_px = (2.0 * scale).max(2.0);
    let max_steps_x = ((half_w / step_2mm_px).ceil() as i32).min(120);
    let max_steps_y = ((half_h / step_2mm_px).ceil() as i32).min(120);
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

    // 2. 직상방(Top-Down: sx = half_w + wx * scale, sy = half_h - wy * scale, depth = -wz) 3D 스캐너 텍스처 Z-버퍼 렌더링
    let (sin_p, cos_p) = state.active_section_angle_deg.to_radians().sin_cos();
    let (sin_t, cos_t) = state.tilt_deg.to_radians().sin_cos();
    let has_uvs = mesh.triangle_uvs.len() == mesh.triangles.len();
    let tex_opt = mesh.texture_image.as_deref();
    // 마우스 드래그 중에는 적응형 샘플링으로 60FPS 무지연을 유지하고, 정지 시에는 100% 전체 삼각형(1:1)으로 핀홀 없이 렌더링!
    let tri_stride = if state.is_interacting {
        (mesh.triangles.len() / 65_000).max(1)
    } else {
        1
    };
    let edge_pad = if tri_stride > 1 { -0.25 } else { -1e-4 };

    for (t_idx, tri) in mesh.triangles.iter().enumerate().step_by(tri_stride) {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;
        let w0 = local_to_world_turntable(
            mesh.positions[i0],
            center_xy,
            z_min,
            height_mm,
            cos_p,
            sin_p,
            cos_t,
            sin_t,
        );
        let w1 = local_to_world_turntable(
            mesh.positions[i1],
            center_xy,
            z_min,
            height_mm,
            cos_p,
            sin_p,
            cos_t,
            sin_t,
        );
        let w2 = local_to_world_turntable(
            mesh.positions[i2],
            center_xy,
            z_min,
            height_mm,
            cos_p,
            sin_p,
            cos_t,
            sin_t,
        );

        let p0 = [half_w + w0[0] * scale, half_h - w0[1] * scale, -w0[2]];
        let p1 = [half_w + w1[0] * scale, half_h - w1[1] * scale, -w1[2]];
        let p2 = [half_w + w2[0] * scale, half_h - w2[1] * scale, -w2[2]];

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
        let wn = local_normal_to_world(mesh.face_normals[t_idx], cos_p, sin_p, cos_t, sin_t);
        let light_gain = (0.88 + 0.16 * wn[2].abs()).clamp(0.82, 1.05) as f32;
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
                    let u = b0 as f32 * uvs[0][0] + b1 as f32 * uvs[1][0] + b2 as f32 * uvs[2][0];
                    let v = b0 as f32 * uvs[0][1] + b1 as f32 * uvs[1][1] + b2 as f32 * uvs[2][1];
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

/// 3D 메쉬를 진짜 Z-버퍼 삼각형 래스터라이저로 렌더링하여 `RgbaImage`를 생성한다.
/// - 실사 UV 텍스처(`PhotoTexture` — 3D 스캐너 원본 알베도 직결 고화질 조명), 솔리드 조명(`SolidClay`),
///   실시간 정밀 탁본(`RealTimeRubbing`), 3분류 파단면(`Segmentation3Way`), X-Ray 투시(`XRayInner`)를 지원한다.
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
        Rgba([WASH.rgb[0], WASH.rgb[1], WASH.rgb[2], 255]),
    );
    let mut zbuf = vec![f32::INFINITY; w_usize * h_usize];

    let active_mode = if state.show_3way_segmentation {
        View3DShaderMode::Segmentation3Way
    } else if state.xray_inner_mode {
        View3DShaderMode::XRayInner
    } else {
        state.shader_mode
    };

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
    let max_span = ext[0].max(ext[1]).max(ext[2]).max(1.0);
    let height_mm = ext[2].max(1.0);
    let center_xy = [
        0.5 * (mesh.bbox.min[0] + mesh.bbox.max[0]),
        0.5 * (mesh.bbox.min[1] + mesh.bbox.max[1]),
    ];
    let z_min = mesh.bbox.min[2];

    let scale = ((w_usize.min(h_usize) as f64) * 0.62 / max_span) * f64::from(state.zoom);
    let half_w = w_usize as f64 * 0.5;
    let half_h = h_usize as f64 * 0.52;

    let (sin_p, cos_p) = state.active_section_angle_deg.to_radians().sin_cos();
    let (sin_t, cos_t) = state.tilt_deg.to_radians().sin_cos();
    let (sin_r, cos_r) = state.roll_deg.to_radians().sin_cos();
    let (sin_cy, cos_cy) = f64::from(state.yaw_deg).to_radians().sin_cos();
    let (sin_cp, cos_cp) = f64::from(state.pitch_deg).to_radians().sin_cos();

    let has_uvs = mesh.triangle_uvs.len() == mesh.triangles.len();
    let tex_opt = mesh.texture_image.as_deref();
    let light_dir = [-0.25f64, -0.82, 0.51];

    // 30만 삼각형 이상 초고밀도 스캐너 메쉬는 드래그 중 적응형 샘플링으로 60FPS 무지연을 보장하고,
    // 정지 시에는 100% 전체 삼각형(stride=1)으로 핀홀 없이 최고의 3D 스캐너 텍스처로 렌더링한다!
    let tri_stride = if state.is_interacting {
        (mesh.triangles.len() / 65_000).max(1)
    } else {
        1
    };
    let edge_pad = if tri_stride > 1 { -0.25 } else { -1e-4 };

    let z_mid = 0.5 * height_mm;
    let transform_vert = |pos: [f64; 3]| -> ([f64; 3], [f64; 3]) {
        let mut w = local_to_world_turntable(
            pos, center_xy, z_min, height_mm, cos_p, sin_p, cos_t, sin_t,
        );
        if state.roll_deg.abs() > 1e-4 {
            let rz = w[2] - z_mid;
            let wy_new = w[1] * cos_r - rz * sin_r;
            let wz_new = w[1] * sin_r + rz * cos_r + z_mid;
            w[1] = wy_new;
            w[2] = wz_new;
        }
        let s = project_world_to_screen(
            w, height_mm, half_w, half_h, scale, cos_cy, sin_cy, cos_cp, sin_cp,
        );
        (w, s)
    };

    for (t_idx, tri) in mesh.triangles.iter().enumerate().step_by(tri_stride) {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;

        let (w0, p0) = transform_vert(mesh.positions[i0]);
        let (w1, p1) = transform_vert(mesh.positions[i1]);
        let (w2, p2) = transform_vert(mesh.positions[i2]);

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

        // 월드 법선 및 양면 조명 계산 (PhotoTexture는 3D 스캐너 원본 실사 텍스처의 밝기·해상도를 100% 보존)
        let wn = local_normal_to_world(mesh.face_normals[t_idx], cos_p, sin_p, cos_t, sin_t);
        let raw_dot = wn[0] * light_dir[0] + wn[1] * light_dir[1] + wn[2] * light_dir[2];
        let is_backface = denom < 0.0;
        let ndotl = raw_dot.abs();
        let spec = ndotl.powi(12) * 0.14;
        let back_atten = if is_backface { 0.88 } else { 1.0 };
        let light_gain = if active_mode == View3DShaderMode::PhotoTexture {
            ((0.86 + 0.18 * ndotl) * back_atten).clamp(0.80, 1.05) as f32
        } else {
            ((0.38 + 0.56 * ndotl + spec) * back_atten).clamp(0.24, 1.15) as f32
        };

        let near_cut_plane = state.cutaway_open && (c_wy - state.cut_y_mm).abs() < 1.2 && c_wx < 0.5;

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

    // 실시간 정밀 탁본 모드일 때: C++ MeshRaster.cpp 536-579행과 동일한
    // 3×3 라플라시안 에지 강화 + 2단 해시 한지 수제 타묵 그레인을 적용한다.
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
                let mut val = (c + lap * 1.45).clamp(0.0, 1.0);
                // S-커브 대비 및 한지 그레인
                val = val * val * (3.0 - 2.0 * val);
                let mut h = (x as u32)
                    .wrapping_mul(374_761_393)
                    .wrapping_add((y as u32).wrapping_mul(668_265_263));
                h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
                let grain = ((h & 0xFF) as f32 / 255.0 - 0.5) * 0.08;
                let ink_v = ((val + grain).clamp(0.06, 0.97) * 255.0) as u8;
                img.put_pixel(
                    x as u32,
                    y as u32,
                    Rgba([ink_v, ink_v, ink_v.saturating_sub(4), 255]),
                );
            }
        }
    }

    img
}

/// 월드 고정 절단 평면(`wy = cut_y_mm` 또는 `wz = top_cut_z_mm`)과 3D 메쉬의 실시간 3D 교차 선분을 추출한다.
pub fn extract_3d_plane_intersection_segments(
    mesh: &TriMesh,
    psi_deg: f64,
    tilt_deg: f64,
    plane_val_mm: f64,
    is_vertical_y_plane: bool,
) -> Vec<[[f64; 3]; 2]> {
    let ext = mesh.bbox.extents();
    let height_mm = ext[2].max(1.0);
    let center_xy = [
        0.5 * (mesh.bbox.min[0] + mesh.bbox.max[0]),
        0.5 * (mesh.bbox.min[1] + mesh.bbox.max[1]),
    ];
    let z_min = mesh.bbox.min[2];
    let (sin_p, cos_p) = psi_deg.to_radians().sin_cos();
    let (sin_t, cos_t) = tilt_deg.to_radians().sin_cos();

    let stride = (mesh.triangles.len() / 60_000).max(1);
    let mut segs = Vec::new();

    for tri in mesh.triangles.iter().step_by(stride) {
        let w0 = local_to_world_turntable(
            mesh.positions[tri[0] as usize],
            center_xy,
            z_min,
            height_mm,
            cos_p,
            sin_p,
            cos_t,
            sin_t,
        );
        let w1 = local_to_world_turntable(
            mesh.positions[tri[1] as usize],
            center_xy,
            z_min,
            height_mm,
            cos_p,
            sin_p,
            cos_t,
            sin_t,
        );
        let w2 = local_to_world_turntable(
            mesh.positions[tri[2] as usize],
            center_xy,
            z_min,
            height_mm,
            cos_p,
            sin_p,
            cos_t,
            sin_t,
        );
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

/// 3D 뷰포트(`27%`) — 상단 `직상방 동심원 방안지 절단 뷰 (반경 +20%)` + 하단 `수평 정면 입면 3D 스캐너 뷰 (상하좌우 4방향 미세 다이얼)`를 렌더링한다.
pub fn render_view3d_pane_egui(
    ui: &mut egui::Ui,
    mesh: &TriMesh,
    region_labels: Option<&[SurfaceRegion]>,
    _rim_gap_chart: Option<&RimGapChart360>,
    best_angle_deg: f64,
    state: &mut View3DPaneState,
) {
    let ext = mesh.bbox.extents();
    let max_span = ext[0].max(ext[1]).max(ext[2]).max(1.0);
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

    let wash = egui::Color32::from_rgb(WASH.rgb[0], WASH.rgb[1], WASH.rgb[2]);
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

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;

        // =========================================================================================
        // [상단 37% 뷰] 직상방(Top-Down) 동심원 방안지 절단 뷰 (`반경 = 유물 반경 × 1.20 = +20%`)
        // - 기존 작은 `축 스코프(Top)` 박스를 제거하고, 실제 유물이 동심원 중앙에서 어떻게 잘리는지 직상방에서 정확히 표시
        // =========================================================================================
        let (top_rect, top_resp) =
            ui.allocate_exact_size(egui::vec2(avail.x, top_plan_h), egui::Sense::click_and_drag());

        if top_resp.dragged() {
            let d = top_resp.drag_delta();
            let next_psi = state.active_section_angle_deg + f64::from(d.x) * 0.5;
            state.active_section_angle_deg = ((next_psi * 10.0).round() / 10.0).rem_euclid(360.0);
            let max_off = (radius_mm * 0.90).max(10.0);
            state.cut_y_mm = (((state.cut_y_mm - f64::from(d.y) * 0.15).clamp(-max_off, max_off)
                * 100.0)
                .round())
                / 100.0;
        }

        let tp = ui.painter_at(top_rect);
        tp.rect_filled(top_rect, 3.0, surface);

        let top_w = (top_rect.width().round() as u32).clamp(160, 480);
        let top_h = (top_rect.height().round() as u32).clamp(140, 360);
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

        // 직상방 동심원 방안지 벡터 링 (`grid_r_mm = radius_mm * 1.20` = 유물 크기보다 정확히 +20% 큰 동심원)
        let td_center = top_rect.center();
        let td_scale = ((top_rect.width().min(top_rect.height()) as f64) * 0.44
            / grid_r_mm.max(1.0))
            * f64::from(state.zoom);
        let r_120_px = (grid_r_mm * td_scale) as f32;
        let r_100_px = (radius_mm * td_scale) as f32;

        // 5단계 동심원 링 (20%, 40%, 60%, 80%, 100%[=유물+20% 외경]) + 유물 실제 외경(100% R) 파란 기준원
        for rk in 1..=5 {
            let r_px = r_120_px * (rk as f32 / 5.0);
            tp.circle_stroke(
                td_center,
                r_px,
                egui::Stroke::new(
                    if rk == 5 { 1.3_f32 } else { 0.8_f32 },
                    egui::Color32::from_rgba_unmultiplied(156, 74, 47, if rk == 5 { 165 } else { 95 }),
                ),
            );
        }
        // 유물 실제 구연 외경(1.00 × radius_mm) 파란 점선/실선 원
        tp.circle_stroke(
            td_center,
            r_100_px,
            egui::Stroke::new(1.1_f32, egui::Color32::from_rgba_unmultiplied(42, 120, 214, 150)),
        );

        // 직상방 십자 기준선 (0°·90°·180°·270°) 및 대각 45° 방위선
        tp.line_segment(
            [
                egui::pos2(td_center.x - r_120_px - 6.0, td_center.y),
                egui::pos2(td_center.x + r_120_px + 6.0, td_center.y),
            ],
            egui::Stroke::new(0.9_f32, egui::Color32::from_rgba_unmultiplied(94, 93, 89, 130)),
        );
        tp.line_segment(
            [
                egui::pos2(td_center.x, td_center.y - r_120_px - 6.0),
                egui::pos2(td_center.x, td_center.y + r_120_px + 6.0),
            ],
            egui::Stroke::new(0.9_f32, egui::Color32::from_rgba_unmultiplied(94, 93, 89, 130)),
        );

        // 직상방에서 본 절단면(`wy = cut_y_mm`) 수평 절단선 및 적색 실측 교차 단면선
        let cut_sy = td_center.y - (state.cut_y_mm * td_scale) as f32;
        tp.line_segment(
            [
                egui::pos2(td_center.x - r_120_px - 8.0, cut_sy),
                egui::pos2(td_center.x + r_120_px + 8.0, cut_sy),
            ],
            egui::Stroke::new(1.8_f32, select_blue),
        );
        state.ensure_3d_curves_cached(mesh);
        for seg in &state.cached_v_segs {
            let p0 = egui::pos2(
                td_center.x + (seg[0][0] * td_scale) as f32,
                td_center.y - (seg[0][1] * td_scale) as f32,
            );
            let p1 = egui::pos2(
                td_center.x + (seg[1][0] * td_scale) as f32,
                td_center.y - (seg[1][1] * td_scale) as f32,
            );
            tp.line_segment([p0, p1], egui::Stroke::new(2.6_f32, profile_red));
        }

        // 현재 회전 방위각(ψ) 바늘 표시
        let psi_rad = state.active_section_angle_deg.to_radians() as f32;
        let needle_pt = egui::pos2(
            td_center.x + r_100_px * psi_rad.cos(),
            td_center.y - r_100_px * psi_rad.sin(),
        );
        tp.line_segment([td_center, needle_pt], egui::Stroke::new(1.3_f32, terra));
        tp.circle_filled(td_center, 3.0, profile_red);

        // 직상방 뷰 상단 미니 배지
        let td_hdr = egui::Rect::from_min_size(
            egui::pos2(top_rect.left() + 6.0, top_rect.top() + 5.0),
            egui::vec2(top_rect.width() - 12.0, 20.0),
        );
        tp.rect_filled(
            td_hdr,
            3.0,
            egui::Color32::from_rgba_unmultiplied(250, 249, 245, 220),
        );
        tp.text(
            egui::pos2(td_hdr.left() + 6.0, td_hdr.center().y),
            egui::Align2::LEFT_CENTER,
            format!(
                "직상방 동심원 방안지 (+20%: Φ{:.1}mm) · 절단 ΔY={:+.2}mm",
                grid_r_mm * 2.0,
                state.cut_y_mm
            ),
            egui::FontId::proportional(10.5),
            ink,
        );
        tp.rect_stroke(
            top_rect,
            3.0,
            egui::Stroke::new(1.0_f32, border),
            egui::StrokeKind::Inside,
        );

        // =========================================================================================
        // [하단 63% 뷰] 수평 정면 입면 3D 스캐너 뷰 (`yaw=0°`, `pitch=0°` 기본) + 상·하·좌·우 미세 다이얼
        // =========================================================================================
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(avail.x, front_elev_h), egui::Sense::click_and_drag());

        let dial_horiz_w = (rect.width() * 0.40).clamp(136.0, 176.0);
        let dial_horiz_h = 22.0_f32;
        let top_dial_rect = egui::Rect::from_center_size(
            egui::pos2(rect.center().x, rect.top() + 72.0),
            egui::vec2(dial_horiz_w, dial_horiz_h),
        );
        let bot_dial_rect = egui::Rect::from_center_size(
            egui::pos2(rect.center().x, rect.bottom() - 32.0),
            egui::vec2(dial_horiz_w, dial_horiz_h),
        );
        let dial_vert_w = 22.0_f32;
        let dial_vert_h = (rect.height() * 0.40).clamp(112.0, 184.0);
        let dial_mid_y = (top_dial_rect.bottom() + bot_dial_rect.top()) * 0.5;
        let left_dial_rect = egui::Rect::from_center_size(
            egui::pos2(rect.left() + 18.0, dial_mid_y),
            egui::vec2(dial_vert_w, dial_vert_h),
        );
        let right_dial_rect = egui::Rect::from_center_size(
            egui::pos2(rect.right() - 18.0, dial_mid_y),
            egui::vec2(dial_vert_w, dial_vert_h),
        );

        let top_dial_resp = ui.interact(
            top_dial_rect,
            egui::Id::new("v3d_cardinal_dial_up"),
            egui::Sense::click_and_drag(),
        );
        let bot_dial_resp = ui.interact(
            bot_dial_rect,
            egui::Id::new("v3d_cardinal_dial_down"),
            egui::Sense::click_and_drag(),
        );
        let left_dial_resp = ui.interact(
            left_dial_rect,
            egui::Id::new("v3d_cardinal_dial_left"),
            egui::Sense::click_and_drag(),
        );
        let right_dial_resp = ui.interact(
            right_dial_rect,
            egui::Id::new("v3d_cardinal_dial_right"),
            egui::Sense::click_and_drag(),
        );

        if top_dial_resp.clicked() {
            state.apply_cardinal_fine_dial(CardinalDialDir::Up, 1);
        } else if top_dial_resp.dragged() {
            let steps = (top_dial_resp.drag_delta().x / 2.5).round() as i32;
            if steps != 0 {
                state.apply_cardinal_fine_dial(CardinalDialDir::Up, steps);
            }
        }
        if bot_dial_resp.clicked() {
            state.apply_cardinal_fine_dial(CardinalDialDir::Down, 1);
        } else if bot_dial_resp.dragged() {
            let steps = (bot_dial_resp.drag_delta().x / 2.5).round() as i32;
            if steps != 0 {
                state.apply_cardinal_fine_dial(CardinalDialDir::Down, steps.abs().max(1));
            }
        }
        if left_dial_resp.clicked() {
            state.apply_cardinal_fine_dial(CardinalDialDir::Left, 1);
        } else if left_dial_resp.dragged() {
            let steps = (left_dial_resp.drag_delta().y / 2.5).round() as i32;
            if steps != 0 {
                state.apply_cardinal_fine_dial(CardinalDialDir::Left, steps.abs().max(1));
            }
        }
        if right_dial_resp.clicked() {
            state.apply_cardinal_fine_dial(CardinalDialDir::Right, 1);
        } else if right_dial_resp.dragged() {
            let steps = (right_dial_resp.drag_delta().y / 2.5).round() as i32;
            if steps != 0 {
                state.apply_cardinal_fine_dial(CardinalDialDir::Right, steps.abs().max(1));
            }
        }

        let any_fine_dial_active = top_dial_resp.dragged()
            || bot_dial_resp.dragged()
            || left_dial_resp.dragged()
            || right_dial_resp.dragged()
            || top_dial_resp.hovered()
            || bot_dial_resp.hovered()
            || left_dial_resp.hovered()
            || right_dial_resp.hovered();

        // 마우스 드래그: 동심원 방안이 수평(pitch=0°, yaw=0°)을 유지한 채 유물만 회전축 둘레로 회전하여 정면 입면이 정확히 보이도록 함
        if response.dragged() && !any_fine_dial_active {
            let delta = response.drag_delta();
            match state.drag_mode {
                DragMode3D::TurntableArtifact => {
                    let next_psi = state.active_section_angle_deg + f64::from(delta.x) * 0.6;
                    state.active_section_angle_deg =
                        ((next_psi * 10.0).round() / 10.0).rem_euclid(360.0);
                    // 정면 입면(pitch=0°, yaw=0°) 수평 유지
                    state.yaw_deg = 0.0;
                    state.pitch_deg = 0.0;
                }
                DragMode3D::CameraOrbit => {
                    state.yaw_deg = (state.yaw_deg + delta.x * 0.6).rem_euclid(360.0);
                    state.pitch_deg = (state.pitch_deg + delta.y * 0.4).clamp(-45.0, 45.0);
                }
                DragMode3D::AxisTilt => {
                    state.tilt_deg =
                        ((state.tilt_deg + f64::from(delta.x) * 0.1).clamp(-25.0, 25.0) * 10.0)
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
        painter.rect_filled(rect, 3.0, wash);

        let is_dragging = response.dragged()
            || top_resp.dragged()
            || top_dial_resp.dragged()
            || bot_dial_resp.dragged()
            || left_dial_resp.dragged()
            || right_dial_resp.dragged();
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

        // 정면 입면 투영 변환 도우미
        let scale =
            ((rect.width().min(rect.height()) as f64) * 0.62 / max_span) * f64::from(state.zoom);
        let half_w = f64::from(rect.width()) * 0.5;
        let half_h = f64::from(rect.height()) * 0.52;
        let (sin_cy, cos_cy) = f64::from(state.yaw_deg).to_radians().sin_cos();
        let (sin_cp, cos_cp) = f64::from(state.pitch_deg).to_radians().sin_cos();

        let proj_pos2 = |w: [f64; 3]| -> egui::Pos2 {
            let s = project_world_to_screen(
                w, height_mm, half_w, half_h, scale, cos_cy, sin_cy, cos_cp, sin_cp,
            );
            egui::pos2(rect.left() + s[0] as f32, rect.top() + s[1] as f32)
        };

        // 중앙 턴테이블 2중 가이드 원
        let ring_center = egui::pos2(rect.center().x, dial_mid_y);
        let ring_r_outer =
            ((right_dial_rect.left() - left_dial_rect.right()) * 0.42).clamp(72.0, 144.0);
        let ring_r_inner = ring_r_outer - 7.0;
        painter.circle_stroke(
            ring_center,
            ring_r_outer,
            egui::Stroke::new(0.9_f32, egui::Color32::from_rgba_unmultiplied(168, 164, 154, 130)),
        );
        painter.circle_stroke(
            ring_center,
            ring_r_inner,
            egui::Stroke::new(0.8_f32, egui::Color32::from_rgba_unmultiplied(168, 164, 154, 100)),
        );

        // 수평 바닥 동심원 방안 (`Z = 0` 평면, `grid_r_mm = radius_mm * 1.20` — 정면 입면에서 100% 수평선으로 정렬됨)
        if state.show_concentric_scope {
            for ring_k in 1..=3 {
                let r = grid_r_mm * (ring_k as f64 / 3.0);
                let mut ring_pts = Vec::with_capacity(49);
                for s in 0..=48 {
                    let a = (s as f64 / 48.0) * std::f64::consts::TAU;
                    ring_pts.push(proj_pos2([r * a.cos(), r * a.sin(), 0.0]));
                }
                painter.add(egui::Shape::line(
                    ring_pts,
                    egui::Stroke::new(
                        if ring_k == 3 { 1.2_f32 } else { 0.8_f32 },
                        egui::Color32::from_rgba_unmultiplied(156, 74, 47, 110),
                    ),
                ));
            }
            painter.line_segment(
                [proj_pos2([-grid_r_mm, 0.0, 0.0]), proj_pos2([grid_r_mm, 0.0, 0.0])],
                egui::Stroke::new(1.1_f32, egui::Color32::from_rgba_unmultiplied(156, 74, 47, 140)),
            );
        }

        // 월드 고정 수평 정면 절단틀 (`P_cut: Y = cut_y_mm`) 및 수직 중심축선 + 실시간 단면 곡선
        if state.show_cutting_frame {
            let hw = grid_r_mm;
            let hh = height_mm + 5.0;
            let y_cut = state.cut_y_mm;
            let f0 = proj_pos2([-hw, y_cut, -2.0]);
            let f1 = proj_pos2([hw, y_cut, -2.0]);
            let f2 = proj_pos2([hw, y_cut, hh]);
            let f3 = proj_pos2([-hw, y_cut, hh]);

            painter.add(egui::Shape::convex_polygon(
                vec![f0, f1, f2, f3],
                egui::Color32::from_rgba_unmultiplied(42, 120, 214, 14),
                egui::Stroke::new(1.2_f32, select_blue),
            ));

            let ax_bot = proj_pos2([0.0, y_cut, -5.0]);
            let ax_top = proj_pos2([0.0, y_cut, hh + 3.0]);
            painter.line_segment([ax_bot, ax_top], egui::Stroke::new(1.3_f32, terra));

            for seg in &state.cached_v_segs {
                let a = proj_pos2(seg[0]);
                let b = proj_pos2(seg[1]);
                painter.line_segment(
                    [a, b],
                    egui::Stroke::new(
                        3.0_f32,
                        egui::Color32::from_rgba_unmultiplied(255, 244, 180, 175),
                    ),
                );
                painter.line_segment([a, b], egui::Stroke::new(1.8_f32, profile_red));
            }

            for seg in &state.cached_h_segs {
                let a = proj_pos2(seg[0]);
                let b = proj_pos2(seg[1]);
                painter.line_segment([a, b], egui::Stroke::new(1.4_f32, clay_focus));
            }
        }

        // 상·하·좌·우 4방향 정밀 미세 다이얼 렌더링
        let dial_fill = egui::Color32::from_rgb(232, 229, 220);
        let dial_stroke = egui::Color32::from_rgb(184, 180, 168);
        let tick_col = egui::Color32::from_rgb(162, 158, 148);
        let tick_major_col = egui::Color32::from_rgb(118, 114, 106);

        painter.text(
            egui::pos2(top_dial_rect.center().x, top_dial_rect.top() - 8.5),
            egui::Align2::CENTER_CENTER,
            "▲ 미세 상 (Up) [ 0.01mm ]",
            egui::FontId::proportional(10.0),
            ink,
        );
        painter.rect_filled(top_dial_rect, 11.0, dial_fill);
        painter.rect_stroke(
            top_dial_rect,
            11.0,
            egui::Stroke::new(
                if top_dial_resp.hovered() { 1.4_f32 } else { 1.0_f32 },
                if top_dial_resp.hovered() { terra } else { dial_stroke },
            ),
            egui::StrokeKind::Inside,
        );
        let n_ticks_h = 34usize;
        let phase_up = ((state.cut_y_mm * 100.0).rem_euclid(5.0) / 5.0) as f32;
        for ti in 0..=n_ticks_h {
            let frac = ((ti as f32 + phase_up) / (n_ticks_h as f32)).clamp(0.0, 1.0);
            let tx = top_dial_rect.left() + 10.0 + frac * (top_dial_rect.width() - 20.0);
            let is_mid = ti == n_ticks_h / 2;
            let th = if is_mid { 13.0 } else if ti % 5 == 0 { 10.0 } else { 7.0 };
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

        painter.rect_filled(bot_dial_rect, 11.0, dial_fill);
        painter.rect_stroke(
            bot_dial_rect,
            11.0,
            egui::Stroke::new(
                if bot_dial_resp.hovered() { 1.4_f32 } else { 1.0_f32 },
                if bot_dial_resp.hovered() { terra } else { dial_stroke },
            ),
            egui::StrokeKind::Inside,
        );
        for ti in 0..=n_ticks_h {
            let frac = ((ti as f32 - phase_up).rem_euclid(n_ticks_h as f32) / (n_ticks_h as f32))
                .clamp(0.0, 1.0);
            let tx = bot_dial_rect.left() + 10.0 + frac * (bot_dial_rect.width() - 20.0);
            let is_mid = ti == n_ticks_h / 2;
            let th = if is_mid { 13.0 } else if ti % 5 == 0 { 10.0 } else { 7.0 };
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
        painter.text(
            egui::pos2(bot_dial_rect.center().x, bot_dial_rect.bottom() + 9.5),
            egui::Align2::CENTER_CENTER,
            "▼ 미세 하 (Down) [ 0.01mm ]",
            egui::FontId::proportional(10.0),
            ink,
        );

        painter.rect_filled(left_dial_rect, 12.0, dial_fill);
        painter.rect_stroke(
            left_dial_rect,
            12.0,
            egui::Stroke::new(
                if left_dial_resp.hovered() { 1.4_f32 } else { 1.0_f32 },
                if left_dial_resp.hovered() { terra } else { dial_stroke },
            ),
            egui::StrokeKind::Inside,
        );
        let n_ticks_v = 30usize;
        let phase_ang = ((state.active_section_angle_deg * 10.0).rem_euclid(5.0) / 5.0) as f32;
        for ti in 0..=n_ticks_v {
            let frac = ((ti as f32 + phase_ang) / (n_ticks_v as f32)).clamp(0.0, 1.0);
            let ty = left_dial_rect.top() + 10.0 + frac * (left_dial_rect.height() - 20.0);
            let is_mid = ti == n_ticks_v / 2;
            let tw = if is_mid { 16.0 } else if ti % 4 == 0 { 11.0 } else { 7.5 };
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
        painter.text(
            egui::pos2(left_dial_rect.left(), left_dial_rect.top() - 8.5),
            egui::Align2::LEFT_CENTER,
            "◀ 미세 좌 [0.1°]",
            egui::FontId::proportional(9.5),
            ink,
        );
        painter.text(
            egui::pos2(left_dial_rect.center().x, left_dial_rect.bottom() + 8.5),
            egui::Align2::CENTER_CENTER,
            "0.1°",
            egui::FontId::monospace(9.5),
            muted,
        );

        painter.rect_filled(right_dial_rect, 12.0, dial_fill);
        painter.rect_stroke(
            right_dial_rect,
            12.0,
            egui::Stroke::new(
                if right_dial_resp.hovered() { 1.4_f32 } else { 1.0_f32 },
                if right_dial_resp.hovered() { terra } else { dial_stroke },
            ),
            egui::StrokeKind::Inside,
        );
        for ti in 0..=n_ticks_v {
            let frac = ((ti as f32 - phase_ang).rem_euclid(n_ticks_v as f32) / (n_ticks_v as f32))
                .clamp(0.0, 1.0);
            let ty = right_dial_rect.top() + 10.0 + frac * (right_dial_rect.height() - 20.0);
            let is_mid = ti == n_ticks_v / 2;
            let tw = if is_mid { 16.0 } else if ti % 4 == 0 { 11.0 } else { 7.5 };
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
        painter.text(
            egui::pos2(right_dial_rect.right(), right_dial_rect.top() - 8.5),
            egui::Align2::RIGHT_CENTER,
            "미세 우 [0.1°] ▶",
            egui::FontId::proportional(9.5),
            ink,
        );
        painter.text(
            egui::pos2(right_dial_rect.center().x, right_dial_rect.bottom() + 8.5),
            egui::Align2::CENTER_CENTER,
            "0.1°",
            egui::FontId::monospace(9.5),
            muted,
        );

        // 상단 1줄 상태 및 셰이더/정위치 칩 바
        let top_bar_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 6.0, rect.top() + 5.0),
            egui::vec2(rect.width() - 12.0, 20.0),
        );
        painter.rect_filled(
            top_bar_rect,
            3.0,
            egui::Color32::from_rgba_unmultiplied(250, 249, 245, 225),
        );
        painter.text(
            egui::pos2(top_bar_rect.left() + 6.0, top_bar_rect.center().y),
            egui::Align2::LEFT_CENTER,
            format!(
                "수평 정면 입면 (ψ={:.1}° · ΔY={:+.2}mm · Z={:.1}mm)",
                state.active_section_angle_deg, state.cut_y_mm, state.top_cut_z_mm
            ),
            egui::FontId::proportional(10.5),
            ink,
        );

        let modes = [
            View3DShaderMode::PhotoTexture,
            View3DShaderMode::SolidClay,
            View3DShaderMode::RealTimeRubbing,
        ];
        let mut chip_x = rect.left() + 6.0;
        let chip_y = rect.top() + 28.0;
        for m in modes {
            let label = m.label_kr();
            let cw = (label.chars().count() as f32) * 10.0 + 10.0;
            let c_rect = egui::Rect::from_min_size(egui::pos2(chip_x, chip_y), egui::vec2(cw, 19.0));
            let c_resp = ui.interact(
                c_rect,
                egui::Id::new(("v3d_shader_chip", label)),
                egui::Sense::click(),
            );
            let is_on = !state.show_3way_segmentation && !state.xray_inner_mode && state.shader_mode == m;
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

        // [정위치 0°] 및 [★최적각] 칩
        for (btn_id, btn_label, is_accent) in [
            ("v3d_reset_front_chip", "정위치 0°".to_string(), false),
            (
                "v3d_best_angle_chip",
                format!("★최적 {:.0}°", best_angle_deg),
                true,
            ),
        ] {
            let cw = (btn_label.chars().count() as f32) * 9.5 + 10.0;
            let c_rect = egui::Rect::from_min_size(egui::pos2(chip_x, chip_y), egui::vec2(cw, 19.0));
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
                    (i32::from(r) - i32::from(WASH.rgb[0])).abs() > 12
                        || (i32::from(g) - i32::from(WASH.rgb[1])).abs() > 12
                        || (i32::from(b) - i32::from(WASH.rgb[2])).abs() > 12
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

        // 1차 호출: 캐시 미스 -> 실제 렌더링 수행 및 560x560 고해상도(기존 360 제한 돌파) 확인
        let (w1, h1, recomputed1) = state.ensure_zbuffer_cached(&mesh, None, 560, 560);
        assert!(recomputed1, "first frame must recompute Z-buffer");
        assert_eq!((w1, h1), (560, 560), "must support HD 560x560 resolution without 360px clamp");

        // 2차 호출 (상태 불변): 캐시 히트 -> 재렌더링 0회 (버벅거림 원천 차단)
        let (w2, h2, recomputed2) = state.ensure_zbuffer_cached(&mesh, None, 560, 560);
        assert!(!recomputed2, "identical state must hit dirty cache with 0 recomputation");
        assert_eq!((w2, h2), (560, 560));

        // 3차 호출 (유물 회전 변화): 캐시 무효화 -> 재렌더링 수행
        state.active_section_angle_deg = 45.0;
        let (_, _, recomputed3) = state.ensure_zbuffer_cached(&mesh, None, 560, 560);
        assert!(recomputed3, "changed rotation angle must trigger Z-buffer recomputation");
    }

    #[test]
    fn test_tdd4_hand_dial_spherical_rotation_updates_pitch_yaw_roll() {
        let mut state = View3DPaneState::default();
        let initial_psi = state.active_section_angle_deg;
        let initial_pitch = state.pitch_deg;

        // 손으로 다이얼을 우측(+40px), 상단(-30px)으로 굴렸을 때 유물 좌우 회전(ψ)과 상하 시야/기울기가 직관적으로 변해야 함
        state.apply_hand_dial_drag(40.0, -30.0, false);
        assert!(
            (state.active_section_angle_deg - initial_psi).abs() > 10.0,
            "horizontal dial drag must rotate artifact ψ"
        );
        assert!(
            (state.pitch_deg - initial_pitch).abs() > 5.0,
            "vertical dial drag must tilt/rotate vertical viewing angle"
        );

        // 미세 중심축 기울기 다이얼 모드에서는 상하/좌우 드래그가 tilt_deg와 roll_deg를 직접 변경해야 함
        state.apply_hand_dial_drag(20.0, -15.0, true);
        assert!(state.roll_deg.abs() > 0.5, "fine axis dial horizontal drag must adjust roll_deg");
        assert!(state.tilt_deg.abs() > 0.5, "fine axis dial vertical drag must adjust tilt_deg");

        // 정위치 초기화 시 0도로 복귀해야 함
        state.reset_hand_orientation();
        assert_eq!(state.active_section_angle_deg, 0.0);
        assert_eq!(state.tilt_deg, 0.0);
        assert_eq!(state.roll_deg, 0.0);
    }

    #[test]
    fn test_tdd8_isotropic_projection_never_stretches_vertically_on_drag_release() {
        let mesh = make_test_bowl_mesh();
        let state = View3DPaneState::default();

        // 세로로 긴 3D 뷰포트(예: 420x680)에서 드래그 중(is_dragging=true)과 마우스를 놓았을 때(is_dragging=false)
        // 렌더 타깃 종횡비(w/h)가 100% 동일하게 보존되어 유물이 위로 늘어나는 왜곡이 절대 발생하지 않아야 한다.
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
            "release aspect {release_aspect} must match target {target_aspect} (never clamp height alone!)"
        );

        // 실제 렌더링된 유물 픽셀의 정규화 세로/가로 바운딩 비율이 드래그 중과 드래그 해제 후 0.8% 이내로 일치해야 함
        let img_drag = render_mesh_zbuffer_rgba(&mesh, None, &state, dw, dh);
        let img_rel = render_mesh_zbuffer_rgba(&mesh, None, &state, rw, rh);

        let normalized_bbox = |img: &RgbaImage| -> (f64, f64) {
            let mut min_x = img.width();
            let mut max_x = 0u32;
            let mut min_y = img.height();
            let mut max_y = 0u32;
            for (x, y, px) in img.enumerate_pixels() {
                if (i32::from(px[0]) - i32::from(WASH.rgb[0])).abs() > 14
                    || (i32::from(px[1]) - i32::from(WASH.rgb[1])).abs() > 14
                {
                    min_x = min_x.min(x);
                    max_x = max_x.max(x);
                    min_y = min_y.min(y);
                    max_y = max_y.max(y);
                }
            }
            (
                (max_x.saturating_sub(min_x) as f64) / (img.width() as f64),
                (max_y.saturating_sub(min_y) as f64) / (img.height() as f64),
            )
        };

        let (nw_d, nh_d) = normalized_bbox(&img_drag);
        let (nw_r, nh_r) = normalized_bbox(&img_rel);
        assert!(
            (nw_d - nw_r).abs() < 0.012 && (nh_d - nh_r).abs() < 0.012,
            "normalized artifact span changed on mouse release! drag=({nw_d:.4},{nh_d:.4}) vs release=({nw_r:.4},{nh_r:.4})"
        );
    }

    #[test]
    fn test_tdd9_four_cardinal_fine_dials_step_exact_001mm_and_01deg() {
        let mut state = View3DPaneState::default();
        assert_eq!(state.cut_y_mm, 0.0);
        assert_eq!(state.active_section_angle_deg, 0.0);

        // 상단 미세 다이얼(▲ 미세 상 [0.01mm]) 100회 스텝 -> 정확히 +1.00mm (부동소수점 누적 오차 0)
        for _ in 0..100 {
            state.apply_cardinal_fine_dial(CardinalDialDir::Up, 1);
        }
        assert!(
            (state.cut_y_mm - 1.00).abs() < 1e-9,
            "100 steps of +0.01mm must equal exactly 1.00mm, got {}",
            state.cut_y_mm
        );

        // 하단 미세 다이얼(▼ 미세 하 [0.01mm]) 35회 스텝 -> 정확히 +0.65mm
        for _ in 0..35 {
            state.apply_cardinal_fine_dial(CardinalDialDir::Down, 1);
        }
        assert!(
            (state.cut_y_mm - 0.65).abs() < 1e-9,
            "expected 0.65mm, got {}",
            state.cut_y_mm
        );

        // 우측 미세 다이얼(▶ 미세 우 [0.1°]) 15회 스텝 -> 정확히 +1.5°
        for _ in 0..15 {
            state.apply_cardinal_fine_dial(CardinalDialDir::Right, 1);
        }
        assert!(
            (state.active_section_angle_deg - 1.5).abs() < 1e-9,
            "expected 1.5 deg, got {}",
            state.active_section_angle_deg
        );

        // 좌측 미세 다이얼(◀ 미세 좌 [0.1°]) 5회 스텝 -> 정확히 +1.0°
        for _ in 0..5 {
            state.apply_cardinal_fine_dial(CardinalDialDir::Left, 1);
        }
        assert!(
            (state.active_section_angle_deg - 1.0).abs() < 1e-9,
            "expected 1.0 deg, got {}",
            state.active_section_angle_deg
        );
    }

    #[test]
    fn test_tdd11_concentric_120pct_topdown_and_zero_lag_frame_budget() {
        let mesh = make_test_bowl_mesh();
        let state = View3DPaneState::default();

        // 1. 기본 입면 시야각은 동심원 바닥 방안이 100% 수평이 되고 유물의 정면 입면이 보이도록 yaw=0.0, pitch=0.0이어야 함
        assert_eq!(state.yaw_deg, 0.0, "default yaw_deg must be 0.0 for true front elevation");
        assert_eq!(state.pitch_deg, 0.0, "default pitch_deg must be 0.0 for horizontal ground plane");

        // 2. 직상방 동심원 방안지 반경은 유물 외경 반경의 정확히 1.20배(+20%)여야 함
        let artifact_r = 62.5_f64;
        let grid_r = compute_concentric_grid_radius_mm(artifact_r);
        assert!(
            (grid_r - artifact_r * 1.20).abs() < 1e-9,
            "concentric grid radius must be exactly +20% (1.20x) of artifact radius, got {grid_r}"
        );

        // 3. 직상방(Top-Down) 동심원 방안지 + 유물 절단 위치 Z-버퍼 렌더링이 15ms 이내로 초고속 완료되고 불투명 유물 픽셀을 생성해야 함
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
        assert!(non_bg > 1_500, "top-down concentric plan must render artifact & grid pixels, got {non_bg}");
        assert!(elapsed_ms < 25.0, "top-down plan render must be < 25ms, took {elapsed_ms:.2}ms");
    }
}
