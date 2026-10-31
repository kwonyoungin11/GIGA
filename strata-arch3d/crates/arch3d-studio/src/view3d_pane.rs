use arch3d_core::strata_tokens::tokens::{
    ACTION, CLAY_FOCUS, CLAY_INK, CLAY_WASH, DESK, EDGE, GROUND, INK, LINE, MUTED, PROFILE_RED,
    SELECT_BLUE, SURFACE, WASH,
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
    /// 카메라 궤도 각도 (`cam_yaw_deg`, `cam_pitch_deg`)
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
    /// 전면 좌측 1/4 절개(`Cutaway`) 활성화 여부 (`c_wy > cut_y_mm && c_wx < -1.5`)
    pub cutaway_open: bool,
    /// 현재 활성 3D 셰이더 모드
    pub shader_mode: View3DShaderMode,
    /// 현재 활성 마우스 드래그 모드
    pub drag_mode: DragMode3D,
    /// 더티 캐시된 Z-버퍼 이미지 및 GPU 텍스처 핸들
    pub cached_zbuffer_key: Option<View3DRenderCacheKey>,
    pub cached_zbuffer_rgba: Option<RgbaImage>,
    pub cached_zbuffer_tex: Option<egui::TextureHandle>,
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
            yaw_deg: -28.0,
            pitch_deg: 22.0,
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
            cutaway_open: true,
            shader_mode: View3DShaderMode::PhotoTexture,
            drag_mode: DragMode3D::TurntableArtifact,
            cached_zbuffer_key: None,
            cached_zbuffer_rgba: None,
            cached_zbuffer_tex: None,
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
            psi_q: (self.active_section_angle_deg * 5.0).round() as i32,
            tilt_q: (self.tilt_deg * 10.0).round() as i32,
            roll_q: (self.roll_deg * 10.0).round() as i32,
            cut_y_q: (self.cut_y_mm * 10.0).round() as i32,
            top_z_q: (self.top_cut_z_mm * 10.0).round() as i32,
        }
    }

    /// 파라미터가 바뀌었을 때만 Z-버퍼를 재렌더링하고, 변경이 없으면 캐시된 버퍼를 즉시 반환한다 (`0ms`).
    /// 반환값: `(width, height, recomputed)`
    pub fn ensure_zbuffer_cached(
        &mut self,
        mesh: &TriMesh,
        region_labels: Option<&[SurfaceRegion]>,
        width: u32,
        height: u32,
    ) -> (u32, u32, bool) {
        let w = width.clamp(160, 720);
        let h = height.clamp(160, 720);
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

    /// 3D 종단면·횡단면 교차 선분을 더티 캐시하여 파라미터 변경 시에만 계산한다.
    pub fn ensure_3d_curves_cached(&mut self, mesh: &TriMesh) {
        let ckey = (
            mesh.name.clone(),
            (self.active_section_angle_deg * 5.0).round() as i32,
            (self.tilt_deg * 10.0).round() as i32,
            (self.roll_deg * 10.0).round() as i32,
            (self.cut_y_mm * 10.0).round() as i32,
            (self.top_cut_z_mm * 10.0).round() as i32,
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

    /// 유물 상하좌우 핸드 다이얼(구면 트랙볼) 드래그 입력을 적용한다.
    /// - `fine_axis_tilt_mode == false`: 손으로 유물을 잡고 좌우(`ψ` 0~360°)·상하(`pitch_deg`)로 자연스럽게 돌려봄
    /// - `fine_axis_tilt_mode == true`: 중심축의 전후(`tilt_deg`)·좌우(`roll_deg`) 기울기를 0.1° 단위로 정밀 보정함
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

    /// 핸드 다이얼 방향을 표준 정위치(`ψ=0°`, `tilt=0°`, `roll=0°`)로 즉시 초기화한다.
    pub fn reset_hand_orientation(&mut self) {
        self.active_section_angle_deg = 0.0;
        self.tilt_deg = 0.0;
        self.roll_deg = 0.0;
        self.yaw_deg = -28.0;
        self.pitch_deg = 22.0;
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

/// 3D 메쉬를 진짜 Z-버퍼 삼각형 래스터라이저로 렌더링하여 `RgbaImage`를 생성한다.
/// - 실사 UV 텍스처(`PhotoTexture`), 솔리드 조명(`SolidClay`), 실시간 정밀 탁본(`RealTimeRubbing`),
///   3분류 파단면(`Segmentation3Way`), X-Ray 투시(`XRayInner`) 및 전면 좌측 1/4 절개(`cutaway_open`)를 지원한다.
pub fn render_mesh_zbuffer_rgba(
    mesh: &TriMesh,
    region_labels: Option<&[SurfaceRegion]>,
    state: &View3DPaneState,
    width: u32,
    height: u32,
) -> RgbaImage {
    let w_usize = width.max(16) as usize;
    let h_usize = height.max(16) as usize;
    let mut img = RgbaImage::new(w_usize as u32, h_usize as u32);
    let mut zbuf = vec![f32::INFINITY; w_usize * h_usize];
    let mut relief_buf = vec![0.0f32; w_usize * h_usize];
    let mut mask_buf = vec![0u8; w_usize * h_usize];

    // 배경: STRATA #F5F4ED Wash + 미세 스튜디오 비네팅
    let cx_scr = w_usize as f32 * 0.5;
    let cy_scr = h_usize as f32 * 0.5;
    let inv_r = 1.0 / (cx_scr.hypot(cy_scr).max(1.0));
    for y in 0..h_usize {
        for x in 0..w_usize {
            let d = ((x as f32 - cx_scr).hypot(y as f32 - cy_scr) * inv_r).clamp(0.0, 1.0);
            let vig = (d * d * 10.0) as i32;
            let r = (i32::from(WASH.rgb[0]) - vig).clamp(225, 255) as u8;
            let g = (i32::from(WASH.rgb[1]) - vig).clamp(224, 255) as u8;
            let b = (i32::from(WASH.rgb[2]) - vig).clamp(215, 255) as u8;
            img.put_pixel(x as u32, y as u32, Rgba([r, g, b, 255]));
        }
    }

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

    let active_mode = if state.show_3way_segmentation {
        View3DShaderMode::Segmentation3Way
    } else if state.xray_inner_mode {
        View3DShaderMode::XRayInner
    } else {
        state.shader_mode
    };

    let has_uvs = mesh.triangle_uvs.len() == mesh.triangles.len();
    let tex_opt = mesh.texture_image.as_deref();
    let light_dir = [-0.38f64, -0.74, 0.55];

    // 정점 단위 1회 선변환(Pre-pass): 삼각형마다 중복 변환하던 병목(3F ≈ 6V)을 1/6로 단축
    let n_verts = mesh.positions.len();
    let mut world_verts = Vec::with_capacity(n_verts);
    let mut scr_verts = Vec::with_capacity(n_verts);
    let z_mid = 0.5 * height_mm;
    for &pos in &mesh.positions {
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
        world_verts.push(w);
        scr_verts.push(s);
    }

    for (t_idx, tri) in mesh.triangles.iter().enumerate() {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;

        let w0 = world_verts[i0];
        let w1 = world_verts[i1];
        let w2 = world_verts[i2];

        let c_wx = (w0[0] + w1[0] + w2[0]) / 3.0;
        let c_wy = (w0[1] + w1[1] + w2[1]) / 3.0;

        // HTML Gate 1 절개 규칙: 절단면 앞쪽 좌측 1/4 분면(wy > cut_y_mm && wx < -1.5)을 절개하여
        // 내면 기벽과 실제 단면 두께가 3D에서 직접 보이도록 컬링한다.
        if state.cutaway_open
            && active_mode != View3DShaderMode::XRayInner
            && c_wy > state.cut_y_mm
            && c_wx < -1.5
        {
            continue;
        }

        let p0 = scr_verts[i0];
        let p1 = scr_verts[i1];
        let p2 = scr_verts[i2];

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

        // 월드 법선 및 양면 조명 계산
        let wn = local_normal_to_world(mesh.face_normals[t_idx], cos_p, sin_p, cos_t, sin_t);
        let raw_dot = wn[0] * light_dir[0] + wn[1] * light_dir[1] + wn[2] * light_dir[2];
        let is_backface = denom < 0.0;
        let ndotl = raw_dot.abs();
        let spec = ndotl.powi(12) * 0.16;
        let back_atten = if is_backface { 0.82 } else { 1.0 };
        let light_gain = ((0.36 + 0.56 * ndotl + spec) * back_atten).clamp(0.22, 1.15) as f32;

        // 절단면(`wy ≈ cut_y_mm`)에 인접한 삼각형인지 여부 (절개 단면 테두리 강조)
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
                if b0 < -1e-4 || b1 < -1e-4 || b2 < -1e-4 {
                    continue;
                }

                let depth = (b0 * p0[2] + b1 * p1[2] + b2 * p2[2]) as f32;
                let buf_idx = py * w_usize + px;

                if active_mode == View3DShaderMode::XRayInner {
                    // X-Ray 반투명 블렌딩 (실루엣 및 내벽 강조)
                    let rim_glow = (1.0 - ndotl as f32).powi(2).clamp(0.18, 0.92);
                    let dst = img.get_pixel(px as u32, py as u32).0;
                    let alpha = (rim_glow * 0.38).clamp(0.08, 0.55);
                    let xr = [181u8, 87, 58];
                    let nr = (f32::from(dst[0]) * (1.0 - alpha) + f32::from(xr[0]) * alpha) as u8;
                    let ng = (f32::from(dst[1]) * (1.0 - alpha) + f32::from(xr[1]) * alpha) as u8;
                    let nb = (f32::from(dst[2]) * (1.0 - alpha) + f32::from(xr[2]) * alpha) as u8;
                    img.put_pixel(px as u32, py as u32, Rgba([nr, ng, nb, 255]));
                    mask_buf[buf_idx] = 1;
                    continue;
                }

                if depth >= zbuf[buf_idx] {
                    continue;
                }
                zbuf[buf_idx] = depth;
                mask_buf[buf_idx] = 1;

                let base_rgb: [u8; 3] = if near_cut_plane {
                    // 절개면 단면 두께 벽면: 먹색(#141413)에 가까운 진한 단면 채움
                    [34, 32, 30]
                } else {
                    match active_mode {
                        View3DShaderMode::PhotoTexture => {
                            if let Some(tex) = tex_opt {
                                if has_uvs {
                                    let u = b0 as f32 * uvs[0][0]
                                        + b1 as f32 * uvs[1][0]
                                        + b2 as f32 * uvs[2][0];
                                    let v = b0 as f32 * uvs[0][1]
                                        + b1 as f32 * uvs[1][1]
                                        + b2 as f32 * uvs[2][1];
                                    let sampled = tex.sample_uv_rgb(u, v);
                                    if sampled[0] > 8 || sampled[1] > 8 || sampled[2] > 8 {
                                        sampled
                                    } else {
                                        [164, 156, 144]
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

                let r = (f32::from(base_rgb[0]) * light_gain).clamp(12.0, 252.0) as u8;
                let g = (f32::from(base_rgb[1]) * light_gain).clamp(12.0, 252.0) as u8;
                let b = (f32::from(base_rgb[2]) * light_gain).clamp(12.0, 250.0) as u8;
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

/// 3D 뷰포트(`27%`) 및 고정 절단틀(`P_cut`)·턴테이블 조작부·`0° ~ 360°` 구연 결손 띠를 렌더링한다.
pub fn render_view3d_pane_egui(
    ui: &mut egui::Ui,
    mesh: &TriMesh,
    region_labels: Option<&[SurfaceRegion]>,
    rim_gap_chart: Option<&RimGapChart360>,
    best_angle_deg: f64,
    state: &mut View3DPaneState,
) {
    let ext = mesh.bbox.extents();
    let max_span = ext[0].max(ext[1]).max(ext[2]).max(1.0);
    let height_mm = ext[2].max(1.0);
    let radius_mm = (0.5 * ext[0].max(ext[1])).max(5.0);

    // 유물이 바뀌었을 때 `.ap2` 절단면 높이(18.0mm) 또는 중앙 높이로 초기화
    if state.top_cut_initialized_for_mesh != mesh.name {
        state.top_cut_initialized_for_mesh = mesh.name.clone();
        state.top_cut_z_mm = mesh
            .ap2_cut_offset_mm
            .unwrap_or_else(|| (height_mm * 0.50).clamp(2.0, height_mm));
    }

    let avail = ui.available_size();
    let controls_h = 114.0;
    let gap_bar_h = 28.0;
    let canvas_h = (avail.y - controls_h - gap_bar_h - 8.0).max(160.0);

    let wash = egui::Color32::from_rgb(WASH.rgb[0], WASH.rgb[1], WASH.rgb[2]);
    let hanji = egui::Color32::from_rgb(GROUND.rgb[0], GROUND.rgb[1], GROUND.rgb[2]);
    let surface = egui::Color32::from_rgb(SURFACE.rgb[0], SURFACE.rgb[1], SURFACE.rgb[2]);
    let desk = egui::Color32::from_rgb(DESK.rgb[0], DESK.rgb[1], DESK.rgb[2]);
    let clay_wash = egui::Color32::from_rgb(CLAY_WASH.rgb[0], CLAY_WASH.rgb[1], CLAY_WASH.rgb[2]);
    let clay_ink = egui::Color32::from_rgb(CLAY_INK.rgb[0], CLAY_INK.rgb[1], CLAY_INK.rgb[2]);
    let border = egui::Color32::from_rgb(EDGE.rgb[0], EDGE.rgb[1], EDGE.rgb[2]);
    let line_col = egui::Color32::from_rgb(LINE.rgb[0], LINE.rgb[1], LINE.rgb[2]);
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
        ui.spacing_mut().item_spacing.y = 3.0;

        // 1. 3D Z-버퍼 뷰포트 캔버스
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(avail.x, canvas_h), egui::Sense::click_and_drag());

        // 마우스 드래그: HTML Gate 1의 3가지 드래그 모드와 1:1 동일하게 작동
        if response.dragged() {
            let delta = response.drag_delta();
            match state.drag_mode {
                DragMode3D::TurntableArtifact => {
                    // 절단틀(P_cut)은 고정하고 유물만 회전축 둘레로 회전(ψ)
                    state.active_section_angle_deg =
                        (state.active_section_angle_deg + f64::from(delta.x) * 0.65).rem_euclid(360.0);
                    state.pitch_deg = (state.pitch_deg + delta.y * 0.35).clamp(-65.0, 75.0);
                }
                DragMode3D::CameraOrbit => {
                    state.yaw_deg = (state.yaw_deg + delta.x * 0.6).rem_euclid(360.0);
                    state.pitch_deg = (state.pitch_deg + delta.y * 0.4).clamp(-75.0, 80.0);
                }
                DragMode3D::AxisTilt => {
                    state.tilt_deg =
                        ((state.tilt_deg + f64::from(delta.x) * 0.1).clamp(-25.0, 25.0) * 10.0)
                            .round()
                            / 10.0;
                }
            }
        }
        // 마우스 휠 줌
        if response.hovered() {
            let scroll_y = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll_y.abs() > 0.1 {
                state.zoom = (state.zoom * (1.0 + scroll_y * 0.0015)).clamp(0.45, 3.2);
            }
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 3.0, wash);

        // 더티 캐시 기반 Z-버퍼 렌더링: 드래그 중에는 고속 인터랙티브 배율(0.75x), 정지 시에는 초고해상도(1.2x, 최대 640px)
        let is_dragging = response.dragged();
        let res_mult = if is_dragging { 0.72 } else { 1.18 };
        let render_w = ((rect.width() * res_mult).round() as u32).clamp(200, 640);
        let render_h = ((rect.height() * res_mult).round() as u32).clamp(200, 640);
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
            // 드래그 종료 직후 고해상도(1.18x) 프레임이 즉시 그려지도록 예약
            ui.ctx().request_repaint();
        }

        // 2. 3D 투영 변환 도우미 (동심원 바닥 격자 + 고정 레이저 절단틀 + 실시간 3D 교차 단면선 오버레이)
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

        // 2-A: 바닥 동심원 정위 타겟 격자 (Z = 0 평면)
        if state.show_concentric_scope {
            let grid_r = radius_mm * 1.15;
            for ring_k in 1..=3 {
                let r = grid_r * (ring_k as f64 / 3.0);
                let mut ring_pts = Vec::with_capacity(49);
                for s in 0..=48 {
                    let a = (s as f64 / 48.0) * std::f64::consts::TAU;
                    ring_pts.push(proj_pos2([r * a.cos(), r * a.sin(), 0.0]));
                }
                painter.add(egui::Shape::line(
                    ring_pts,
                    egui::Stroke::new(
                        if ring_k == 3 { 1.1_f32 } else { 0.7_f32 },
                        egui::Color32::from_rgba_unmultiplied(156, 74, 47, 72),
                    ),
                ));
            }
            painter.line_segment(
                [proj_pos2([-grid_r, 0.0, 0.0]), proj_pos2([grid_r, 0.0, 0.0])],
                egui::Stroke::new(0.8_f32, egui::Color32::from_rgba_unmultiplied(94, 93, 89, 85)),
            );
            painter.line_segment(
                [proj_pos2([0.0, -grid_r, 0.0]), proj_pos2([0.0, grid_r, 0.0])],
                egui::Stroke::new(0.8_f32, egui::Color32::from_rgba_unmultiplied(94, 93, 89, 85)),
            );
        }

        // 2-B: 월드 고정 레이저 절단틀 (`P_cut: Y = cut_y_mm`) 및 수직 중심축선
        if state.show_cutting_frame {
            let hw = radius_mm * 1.16;
            let hh = height_mm + 5.0;
            let y_cut = state.cut_y_mm;
            let f0 = proj_pos2([-hw, y_cut, -3.0]);
            let f1 = proj_pos2([hw, y_cut, -3.0]);
            let f2 = proj_pos2([hw, y_cut, hh]);
            let f3 = proj_pos2([-hw, y_cut, hh]);

            painter.add(egui::Shape::convex_polygon(
                vec![f0, f1, f2, f3],
                egui::Color32::from_rgba_unmultiplied(42, 120, 214, 18),
                egui::Stroke::new(1.2_f32, select_blue),
            ));

            // 회전 중심축 수직선 (X = 0, Y = cut_y_mm)
            let ax_bot = proj_pos2([0.0, y_cut, -6.0]);
            let ax_top = proj_pos2([0.0, y_cut, hh + 4.0]);
            painter.line_segment([ax_bot, ax_top], egui::Stroke::new(1.3_f32, terra));

            // 더티 캐시된 3D 수직/수평 교차 단면 곡선 사용 (정지 시 재계산 0회)
            state.ensure_3d_curves_cached(mesh);
            for seg in &state.cached_v_segs {
                let a = proj_pos2(seg[0]);
                let b = proj_pos2(seg[1]);
                painter.line_segment(
                    [a, b],
                    egui::Stroke::new(
                        3.2_f32,
                        egui::Color32::from_rgba_unmultiplied(255, 244, 180, 180),
                    ),
                );
                painter.line_segment([a, b], egui::Stroke::new(1.8_f32, profile_red));
            }

            // 실시간 수평 횡단면 3D 교차선 (`wz = top_cut_z_mm`)
            for seg in &state.cached_h_segs {
                let a = proj_pos2(seg[0]);
                let b = proj_pos2(seg[1]);
                painter.line_segment([a, b], egui::Stroke::new(1.5_f32, clay_focus));
            }
        }

        // 3. 상단 헤더 텍스트 & 셰이더 전환 알약 칩 (실사텍스처 · 솔리드 · 정밀탁본 · 3분류파단 · X-Ray · 1/4절개)
        let top_bar_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 6.0, rect.top() + 6.0),
            egui::vec2(rect.width() - 12.0, 22.0),
        );
        painter.rect_filled(
            top_bar_rect,
            4.0,
            egui::Color32::from_rgba_unmultiplied(250, 249, 245, 225),
        );
        painter.text(
            egui::pos2(top_bar_rect.left() + 8.0, top_bar_rect.center().y),
            egui::Align2::LEFT_CENTER,
            format!(
                "고정 절단틀 P_cut (ψ={:.0}° · ΔY={:+.1}mm · Z={:.1}mm)",
                state.active_section_angle_deg, state.cut_y_mm, state.top_cut_z_mm
            ),
            egui::FontId::proportional(11.0),
            ink,
        );

        // 좌측 상단 셰이더 칩 행
        let modes = [
            View3DShaderMode::PhotoTexture,
            View3DShaderMode::SolidClay,
            View3DShaderMode::RealTimeRubbing,
            View3DShaderMode::Segmentation3Way,
            View3DShaderMode::XRayInner,
        ];
        let mut chip_x = rect.left() + 6.0;
        let chip_y = rect.top() + 31.0;
        for m in modes {
            let label = m.label_kr();
            let cw = (label.chars().count() as f32) * 10.5 + 12.0;
            let c_rect = egui::Rect::from_min_size(egui::pos2(chip_x, chip_y), egui::vec2(cw, 20.0));
            let c_resp = ui.interact(
                c_rect,
                egui::Id::new(("v3d_shader_chip", label)),
                egui::Sense::click(),
            );
            let is_on = (m == View3DShaderMode::Segmentation3Way && state.show_3way_segmentation)
                || (m == View3DShaderMode::XRayInner && state.xray_inner_mode)
                || (!state.show_3way_segmentation
                    && !state.xray_inner_mode
                    && state.shader_mode == m);
            if c_resp.clicked() {
                state.show_3way_segmentation = m == View3DShaderMode::Segmentation3Way;
                state.xray_inner_mode = m == View3DShaderMode::XRayInner;
                state.shader_mode = m;
            }
            painter.rect_filled(c_rect, 4.0, if is_on { ink } else { surface });
            painter.rect_stroke(
                c_rect,
                4.0,
                egui::Stroke::new(1.0_f32, if is_on { ink } else { border }),
                egui::StrokeKind::Inside,
            );
            painter.text(
                c_rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(10.5),
                if is_on { hanji } else { ink },
            );
            chip_x += cw + 3.0;
        }

        // 1/4 절개 토글 칩 (좌측 2번째 줄)
        let cut_chip_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 6.0, rect.top() + 54.0),
            egui::vec2(92.0, 20.0),
        );
        let cut_resp = ui.interact(
            cut_chip_rect,
            egui::Id::new("v3d_cutaway_toggle"),
            egui::Sense::click(),
        );
        if cut_resp.clicked() {
            state.cutaway_open = !state.cutaway_open;
        }
        painter.rect_filled(
            cut_chip_rect,
            4.0,
            if state.cutaway_open { clay_wash } else { surface },
        );
        painter.rect_stroke(
            cut_chip_rect,
            4.0,
            egui::Stroke::new(1.0_f32, if state.cutaway_open { terra } else { border }),
            egui::StrokeKind::Inside,
        );
        painter.text(
            cut_chip_rect.center(),
            egui::Align2::CENTER_CENTER,
            if state.cutaway_open {
                "● 1/4 절개 ON"
            } else {
                "○ 1/4 절개 OFF"
            },
            egui::FontId::proportional(10.5),
            if state.cutaway_open { clay_ink } else { muted },
        );

        // 4. 우측 상단 `Cross-Plane · 축 스코프` (HTML Gate 1 1:1 계승, 96×98px)
        let scope_w = 96.0;
        let scope_h = 98.0;
        let scope_rect = egui::Rect::from_min_size(
            egui::pos2(rect.right() - scope_w - 6.0, rect.top() + 54.0),
            egui::vec2(scope_w, scope_h),
        );
        painter.rect_filled(
            scope_rect,
            6.0,
            egui::Color32::from_rgba_unmultiplied(250, 249, 245, 235),
        );
        painter.rect_stroke(
            scope_rect,
            6.0,
            egui::Stroke::new(1.0_f32, border),
            egui::StrokeKind::Inside,
        );
        painter.text(
            egui::pos2(scope_rect.center().x, scope_rect.top() + 10.0),
            egui::Align2::CENTER_CENTER,
            "축 스코프 (Top)",
            egui::FontId::proportional(10.0),
            muted,
        );
        let sc_center = egui::pos2(scope_rect.center().x, scope_rect.top() + 48.0);
        for r_sc in [11.0f32, 21.0, 30.0] {
            painter.circle_stroke(sc_center, r_sc, egui::Stroke::new(0.8_f32, line_col));
        }
        painter.circle_stroke(sc_center, 26.0, egui::Stroke::new(1.4_f32, select_blue));
        painter.circle_stroke(sc_center, 14.0, egui::Stroke::new(1.2_f32, clay_focus));
        let psi_rad = state.active_section_angle_deg.to_radians() as f32;
        let needle_end = egui::pos2(
            sc_center.x + 28.0 * psi_rad.cos(),
            sc_center.y + 28.0 * psi_rad.sin(),
        );
        painter.line_segment([sc_center, needle_end], egui::Stroke::new(1.6_f32, profile_red));
        painter.circle_filled(sc_center, 2.5, profile_red);
        painter.text(
            egui::pos2(scope_rect.center().x, scope_rect.bottom() - 10.0),
            egui::Align2::CENTER_CENTER,
            format!("ψ={:.0}° / {:+.1}°", state.active_section_angle_deg, state.tilt_deg),
            egui::FontId::monospace(9.5),
            ink,
        );

        painter.rect_stroke(
            rect,
            3.0,
            egui::Stroke::new(1.0_f32, border),
            egui::StrokeKind::Inside,
        );

        // 5. 하단 유물 상하좌우 3D 핸드 다이얼 (구면 트랙볼 패드 + 360° 링 + 방향 조작부)
        egui::Frame::NONE
            .fill(hanji)
            .stroke(egui::Stroke::new(1.0_f32, border))
            .corner_radius(3.0)
            .inner_margin(egui::Margin::symmetric(6, 4))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;

                    // 좌측: 원형 3D 핸드 다이얼 (사용자가 손으로 유물을 잡고 상하좌우로 굴리는 구면 다이얼)
                    let dial_size = egui::vec2(96.0, 96.0);
                    let (dial_rect, dial_resp) =
                        ui.allocate_exact_size(dial_size, egui::Sense::click_and_drag());
                    if dial_resp.dragged() {
                        let d = dial_resp.drag_delta();
                        state.apply_hand_dial_drag(
                            d.x,
                            d.y,
                            state.drag_mode == DragMode3D::AxisTilt,
                        );
                    }
                    let dp = ui.painter_at(dial_rect);
                    let dc = dial_rect.center();
                    let r_outer = 44.0_f32;
                    let r_sphere = 34.0_f32;

                    // 외곽 360° 방위각 링 및 눈금
                    dp.circle_filled(dc, r_outer, desk);
                    dp.circle_stroke(dc, r_outer, egui::Stroke::new(1.2_f32, border));
                    dp.circle_filled(dc, r_sphere, surface);
                    dp.circle_stroke(dc, r_sphere, egui::Stroke::new(1.1_f32, ink));

                    // 구면 경위도 가이드 선 (상하좌우 3D 구체 감각 표현)
                    dp.line_segment(
                        [
                            egui::pos2(dc.x - r_sphere, dc.y),
                            egui::pos2(dc.x + r_sphere, dc.y),
                        ],
                        egui::Stroke::new(0.8_f32, line_col),
                    );
                    dp.line_segment(
                        [
                            egui::pos2(dc.x, dc.y - r_sphere),
                            egui::pos2(dc.x, dc.y + r_sphere),
                        ],
                        egui::Stroke::new(0.8_f32, line_col),
                    );
                    dp.circle_stroke(dc, r_sphere * 0.58, egui::Stroke::new(0.7_f32, line_col));

                    // 상·하·좌·우 방향 표식
                    dp.text(
                        egui::pos2(dc.x, dc.y - r_outer + 5.0),
                        egui::Align2::CENTER_CENTER,
                        "상",
                        egui::FontId::proportional(8.5),
                        muted,
                    );
                    dp.text(
                        egui::pos2(dc.x, dc.y + r_outer - 5.0),
                        egui::Align2::CENTER_CENTER,
                        "하",
                        egui::FontId::proportional(8.5),
                        muted,
                    );
                    dp.text(
                        egui::pos2(dc.x - r_outer + 5.0, dc.y),
                        egui::Align2::CENTER_CENTER,
                        "좌",
                        egui::FontId::proportional(8.5),
                        muted,
                    );
                    dp.text(
                        egui::pos2(dc.x + r_outer - 5.0, dc.y),
                        egui::Align2::CENTER_CENTER,
                        "우",
                        egui::FontId::proportional(8.5),
                        muted,
                    );

                    // 외곽 360° 회전각(ψ) 마커 & 구면 내부 상하좌우 핸드 볼 노브 위치
                    let psi_r = state.active_section_angle_deg.to_radians() as f32;
                    let ring_pt = egui::pos2(
                        dc.x + (r_outer - 4.0) * psi_r.cos(),
                        dc.y + (r_outer - 4.0) * psi_r.sin(),
                    );
                    dp.circle_filled(ring_pt, 3.5, profile_red);

                    let nx = (psi_r.sin() * 0.65).clamp(-0.85, 0.85);
                    let ny = ((state.pitch_deg - 22.0) / 65.0).clamp(-0.85, 0.85);
                    let puck_pos =
                        egui::pos2(dc.x + nx * (r_sphere - 8.0), dc.y + ny * (r_sphere - 8.0));
                    dp.line_segment([dc, puck_pos], egui::Stroke::new(1.5_f32, terra));
                    dp.circle_filled(puck_pos, 6.5, terra);
                    dp.circle_stroke(puck_pos, 6.5, egui::Stroke::new(1.2_f32, hanji));

                    // 우측: 핸드 다이얼 모드 전환 + 방향 미세조절 버튼 + 절단면 ΔY / Z 컨트롤
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 3.0;
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            for (mode, label) in [
                                (DragMode3D::TurntableArtifact, "손회전(절단틀고정)"),
                                (DragMode3D::AxisTilt, "축기울기(0.1°)"),
                                (DragMode3D::CameraOrbit, "카메라"),
                            ] {
                                let is_sel = state.drag_mode == mode;
                                let btn = egui::Button::new(
                                    egui::RichText::new(label)
                                        .size(10.0)
                                        .color(if is_sel { clay_ink } else { ink }),
                                )
                                .fill(if is_sel { clay_wash } else { desk })
                                .stroke(egui::Stroke::new(
                                    1.0_f32,
                                    if is_sel { terra } else { line_col },
                                ))
                                .corner_radius(3.0);
                                if ui.add(btn).clicked() {
                                    state.drag_mode = mode;
                                }
                            }
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("정위치 0°").size(10.0).color(ink),
                                    )
                                    .fill(surface)
                                    .stroke(egui::Stroke::new(1.0_f32, border))
                                    .corner_radius(3.0),
                                )
                                .clicked()
                            {
                                state.reset_hand_orientation();
                            }
                        });

                        // 상하좌우 스텝 버튼 + 현재 각도 표시
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            if ui.small_button("◀ 좌 5°").clicked() {
                                state.apply_hand_dial_drag(
                                    -6.0,
                                    0.0,
                                    state.drag_mode == DragMode3D::AxisTilt,
                                );
                            }
                            if ui.small_button("▶ 우 5°").clicked() {
                                state.apply_hand_dial_drag(
                                    6.0,
                                    0.0,
                                    state.drag_mode == DragMode3D::AxisTilt,
                                );
                            }
                            if ui.small_button("▲ 상").clicked() {
                                state.apply_hand_dial_drag(
                                    0.0,
                                    -8.0,
                                    state.drag_mode == DragMode3D::AxisTilt,
                                );
                            }
                            if ui.small_button("▼ 하").clicked() {
                                state.apply_hand_dial_drag(
                                    0.0,
                                    8.0,
                                    state.drag_mode == DragMode3D::AxisTilt,
                                );
                            }
                            ui.colored_label(
                                muted,
                                format!(
                                    "ψ={:.0}° · 기울기 {:+.1}°/{:+.1}°",
                                    state.active_section_angle_deg, state.tilt_deg, state.roll_deg
                                ),
                            );
                        });

                        // 절단틀 위치(ΔY, Z) 정밀 드래그값 (컴팩트 1줄)
                        let max_off = (radius_mm * 0.85).max(10.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            ui.colored_label(muted, "종단면 ΔY:");
                            ui.add(
                                egui::DragValue::new(&mut state.cut_y_mm)
                                    .speed(0.2)
                                    .range(-max_off..=max_off)
                                    .suffix("mm")
                                    .fixed_decimals(1),
                            );
                            ui.add_space(4.0);
                            ui.colored_label(muted, "횡단면 Z:");
                            ui.add(
                                egui::DragValue::new(&mut state.top_cut_z_mm)
                                    .speed(0.3)
                                    .range(0.0..=height_mm)
                                    .suffix("mm")
                                    .fixed_decimals(1),
                            );
                        });
                    });
                });
            });

        // 6. 하단 0°~360° 구연 잔존·결손 띠 (`[★ 최적 단면각 ...° 즉시 이동]`)
        let (bar_rect, bar_resp) =
            ui.allocate_exact_size(egui::vec2(avail.x, gap_bar_h), egui::Sense::click());
        if bar_resp.clicked() {
            state.active_section_angle_deg = best_angle_deg;
            state.cut_y_mm = 0.0;
        }
        let bar_painter = ui.painter_at(bar_rect);
        bar_painter.rect_filled(bar_rect, 2.0, hanji);
        bar_painter.rect_stroke(
            bar_rect,
            2.0,
            egui::Stroke::new(1.0_f32, border),
            egui::StrokeKind::Inside,
        );

        if let Some(chart) = rim_gap_chart {
            let n_bins = chart.gap_mm_per_bin.len().max(1);
            let bin_w = bar_rect.width() / (n_bins as f32);
            for (i, &gap_mm) in chart.gap_mm_per_bin.iter().enumerate() {
                let x0 = bar_rect.left() + (i as f32) * bin_w;
                let seg_rect = egui::Rect::from_min_size(
                    egui::pos2(x0, bar_rect.top() + 2.0),
                    egui::vec2(bin_w.max(1.0), bar_rect.height() - 4.0),
                );
                let c = if gap_mm.is_none_or(|g| g < 5.0) {
                    egui::Color32::from_rgb(218, 226, 216)
                } else {
                    egui::Color32::from_rgb(235, 205, 198)
                };
                bar_painter.rect_filled(seg_rect, 0.0, c);
            }
        }

        bar_painter.text(
            egui::pos2(bar_rect.left() + 8.0, bar_rect.center().y),
            egui::Align2::LEFT_CENTER,
            format!(
                "0°~360° 구연 잔존·결손 띠  [★ 최적 단면각 {:.0}° 즉시 이동]",
                best_angle_deg
            ),
            egui::FontId::proportional(11.0),
            ink,
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
}
