use arch3d_core::strata_tokens::tokens::{
    ACTION, DONE_BG, DONE_INK, EDGE, GRID_1MM, GROUND, INK, LINE, MUTED, PROFILE_RED, SELECT_BLUE,
    SURFACE, WASH,
};
use arch3d_core::TriMesh;
use arch3d_export::{
    triangulate_polygon_ear_clipping, ComposedPlate2D, PlateRasterKind,
};
use arch3d_measure::ArtifactMeasurementReport;
use eframe::egui;
use image::{Rgba, RgbaImage};

#[inline]
fn sample_hanji_paper_grain(x: u32, y: u32, amplitude: f32) -> f32 {
    let mut h1 = x.wrapping_mul(0x9E37_79B9) ^ y.wrapping_mul(0x85EB_CA6B);
    h1 ^= h1 >> 16;
    h1 = h1.wrapping_mul(0x7FEB_352D);
    h1 ^= h1 >> 15;
    let mut h2 = (x / 3).wrapping_mul(0xC2B2_AE35) ^ (y / 2).wrapping_mul(0x27D4_EB2F);
    h2 ^= h2 >> 16;
    let f1 = (h1 & 0xFFFF) as f32 / 65535.0;
    let f2 = (h2 & 0xFFFF) as f32 / 65535.0;
    ((0.6 * f1 + 0.4 * f2) * 2.0 - 1.0) * amplitude
}

/// 학술 논문 기반 디지털 정밀 탁본 기법 (Mara & Krömker 2010 MSII + Multi-Scale DoG + Sigmoid Ink Transfer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AcademicRubbingStyle {
    /// 전통 한지 습탁 먹물 기법 (젖은 한지가 음각 홈에서 뜨고 양각 능선에 먹물이 깊게 스며듦)
    WetHanjiInk,
    /// 건탁 석채 기법 (미세 양각 모서리와 물레속선 중심의 건조 마찰 탁본)
    DryGraphite,
    /// GigaMesh MSII 다중스케일 곡률 능선 강조 기법 (인화문·명문·타날문 판독 특화)
    MsiiRidgeEnhance,
}

impl AcademicRubbingStyle {
    pub fn label_kr(self) -> &'static str {
        match self {
            Self::WetHanjiInk => "습탁 먹물(Wet Hanji)",
            Self::DryGraphite => "건탁 석채(Dry)",
            Self::MsiiRidgeEnhance => "MSII 능선강조(논문)",
        }
    }
}

/// 학술 디지털 탁본 파라미터 (`부분 정밀 탁본 창`에서 사용자가 실시간 조절).
#[derive(Debug, Clone, PartialEq)]
pub struct AcademicRubbingParams {
    /// 먹물 농도 (`15.0 ..= 98.0 %`)
    pub ink_density_pct: f64,
    /// 다중스케일 곡률(MSII + DoG r=1,3,6) 감도 배율 (`0.5 ..= 2.5`)
    pub msii_detail_boost: f32,
    /// 비선형 시그모이드(Sigmoid) 먹물 전이 곡선 기울기 (`2.0 ..= 14.0`)
    pub sigmoid_contrast: f32,
    /// 탁본 표현 기법
    pub style: AcademicRubbingStyle,
}

impl Default for AcademicRubbingParams {
    fn default() -> Self {
        Self {
            ink_density_pct: 76.0,
            msii_detail_boost: 1.35,
            sigmoid_contrast: 7.5,
            style: AcademicRubbingStyle::WetHanjiInk,
        }
    }
}

/// 화면 전체(`Full-Bleed`) 실측 방안지 격자 및 `cm` 눈금자 명세 (`Todo 5`).
#[derive(Debug, Clone)]
pub struct FullBleedGraphPaperSpec {
    pub paper_rect: egui::Rect,
    pub origin_px: egui::Pos2,
    pub scale_px_per_mm: f32,
    pub lines_1mm_x: Vec<f32>,
    pub lines_1mm_y: Vec<f32>,
    pub lines_5mm_x: Vec<f32>,
    pub lines_5mm_y: Vec<f32>,
    pub lines_10mm_x: Vec<f32>,
    pub lines_10mm_y: Vec<f32>,
    pub cm_labels_top: Vec<(f32, String)>,
    pub cm_labels_left: Vec<(f32, String)>,
}

/// 우측 2D 도판 영역(`sheet_rect`) 전체를 여백 없는 100% 실제 고고학 실측 방안지(1mm·5mm·10mm 격자 + 상/좌 cm 눈금자)로 계산한다.
pub fn compute_full_bleed_graph_paper_spec(
    sheet_rect: egui::Rect,
    canvas_width_mm: f64,
    canvas_height_mm: f64,
    zoom: f32,
    pan_px: egui::Vec2,
) -> FullBleedGraphPaperSpec {
    let cw = (canvas_width_mm.max(10.0)) as f32;
    let ch = (canvas_height_mm.max(10.0)) as f32;
    let base_scale = ((sheet_rect.width() - 56.0) / cw)
        .min((sheet_rect.height() - 52.0) / ch)
        .max(1.2);
    let scale = (base_scale * zoom).max(0.8);

    // 도판 중심을 화면 중앙(+pan_px)에 맞추되, 방안지 격자는 sheet_rect 화면 전체(Full-bleed)에 빈틈없이 깔린다.
    let plate_w = cw * scale;
    let plate_h = ch * scale;
    let center = sheet_rect.center() + pan_px;
    let origin_x = center.x - plate_w * 0.5;
    let origin_y = center.y + plate_h * 0.5;

    let mut lines_1mm_x = Vec::new();
    let mut lines_1mm_y = Vec::new();
    let mut lines_5mm_x = Vec::new();
    let mut lines_5mm_y = Vec::new();
    let mut lines_10mm_x = Vec::new();
    let mut lines_10mm_y = Vec::new();
    let mut cm_labels_top = Vec::new();
    let mut cm_labels_left = Vec::new();

    let min_mm_x = ((sheet_rect.left() - origin_x) / scale).floor() as i32;
    let max_mm_x = ((sheet_rect.right() - origin_x) / scale).ceil() as i32;
    for mm_x in min_mm_x..=max_mm_x {
        let gx = origin_x + (mm_x as f32) * scale;
        if gx < sheet_rect.left() || gx > sheet_rect.right() {
            continue;
        }
        if mm_x % 10 == 0 {
            lines_10mm_x.push(gx);
            cm_labels_top.push((gx, format!("{}cm", mm_x / 10)));
        } else if mm_x % 5 == 0 {
            lines_5mm_x.push(gx);
        } else if scale >= 2.2 {
            lines_1mm_x.push(gx);
        }
    }

    let min_mm_y = ((origin_y - sheet_rect.bottom()) / scale).floor() as i32;
    let max_mm_y = ((origin_y - sheet_rect.top()) / scale).ceil() as i32;
    for mm_y in min_mm_y..=max_mm_y {
        let gy = origin_y - (mm_y as f32) * scale;
        if gy < sheet_rect.top() || gy > sheet_rect.bottom() {
            continue;
        }
        if mm_y % 10 == 0 {
            lines_10mm_y.push(gy);
            cm_labels_left.push((gy, format!("{}cm", mm_y / 10)));
        } else if mm_y % 5 == 0 {
            lines_5mm_y.push(gy);
        } else if scale >= 2.2 {
            lines_1mm_y.push(gy);
        }
    }

    FullBleedGraphPaperSpec {
        paper_rect: sheet_rect,
        origin_px: egui::pos2(origin_x, origin_y),
        scale_px_per_mm: scale,
        lines_1mm_x,
        lines_1mm_y,
        lines_5mm_x,
        lines_5mm_y,
        lines_10mm_x,
        lines_10mm_y,
        cm_labels_top,
        cm_labels_left,
    }
}

type SheetRubbingCacheKey = (
    String,
    i32,
    i32,
    i32,
    i32,
    AcademicRubbingStyle,
    bool,
    Option<[i32; 4]>,
);

/// 우측 2D 실측 도판 뷰포트(`SheetPane`)의 줌·팬, 드래그 부분 탁본(`ROI`), 및 정밀 탁본(`Rubbing`) 래스터 캐시 상태.
#[derive(Clone)]
pub struct SheetPaneState {
    pub zoom: f32,
    pub pan_px: egui::Vec2,
    /// 사용자가 마우스 드래그로 부분 탁본 위치를 지정하는 모드 활성화 여부 (`Todo 7`)
    pub roi_drag_mode: bool,
    /// 드래그 시작 지점 (`mm` 도판 좌표)
    pub roi_drag_start_mm: Option<[f64; 2]>,
    /// 사용자가 드래그하여 지정한 부분 탁본 영역 (`Some([min_x_mm, min_y_mm, max_x_mm, max_y_mm])`)
    pub rubbing_roi_mm: Option<[f64; 4]>,
    /// 『부분 정밀 탁본 창 (ROI Rubbing Studio)』 표시 여부
    pub show_roi_rubbing_window: bool,
    /// 학술 논문 기반 정밀 탁본 파라미터 (농도·MSII 감도·시그모이드 대비·기법)
    pub rubbing_params: AcademicRubbingParams,
    cached_key: Option<SheetRubbingCacheKey>,
    cached_front_img: Option<RgbaImage>,
    cached_back_img: Option<RgbaImage>,
    cached_bottom_img: Option<RgbaImage>,
    cached_front_tex: Option<egui::TextureHandle>,
    cached_back_tex: Option<egui::TextureHandle>,
    cached_bottom_tex: Option<egui::TextureHandle>,
}

impl std::fmt::Debug for SheetPaneState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SheetPaneState")
            .field("zoom", &self.zoom)
            .field("pan_px", &self.pan_px)
            .field("roi_drag_mode", &self.roi_drag_mode)
            .field("rubbing_roi_mm", &self.rubbing_roi_mm)
            .field("show_roi_rubbing_window", &self.show_roi_rubbing_window)
            .field("rubbing_params", &self.rubbing_params)
            .finish()
    }
}

impl Default for SheetPaneState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan_px: egui::Vec2::ZERO,
            roi_drag_mode: false,
            roi_drag_start_mm: None,
            rubbing_roi_mm: None,
            show_roi_rubbing_window: false,
            rubbing_params: AcademicRubbingParams::default(),
            cached_key: None,
            cached_front_img: None,
            cached_back_img: None,
            cached_bottom_img: None,
            cached_front_tex: None,
            cached_back_tex: None,
            cached_bottom_tex: None,
        }
    }
}

impl SheetPaneState {
    /// 사용자가 드래그한 부분 탁본 영역(`mm`)을 설정하고, 영역이 유효하면 『부분 정밀 탁본 창』을 자동으로 연다.
    pub fn set_rubbing_roi_mm(&mut self, roi_mm: Option<[f64; 4]>) {
        if let Some([x0, y0, x1, y1]) = roi_mm {
            let min_x = x0.min(x1);
            let max_x = x0.max(x1);
            let min_y = y0.min(y1);
            let max_y = y0.max(y1);
            if (max_x - min_x) >= 2.0 && (max_y - min_y) >= 2.0 {
                self.rubbing_roi_mm = Some([min_x, min_y, max_x, max_y]);
                self.show_roi_rubbing_window = true;
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

/// 3D 메쉬로부터 도판 합성용 직교투영(Orthographic) 정밀 탁본(`Rubbing`) RGBA 영상을 생성한다.
pub fn render_orthographic_rubbing_rgba(
    mesh: &TriMesh,
    kind: PlateRasterKind,
    psi_deg: f64,
    ink_density_pct: f64,
    width: u32,
    height: u32,
) -> RgbaImage {
    let params = AcademicRubbingParams {
        ink_density_pct,
        ..Default::default()
    };
    render_orthographic_rubbing_rgba_with_params(mesh, kind, psi_deg, &params, width, height)
}

/// 학술 논문 기반 디지털 정밀 탁본(Mara & Krömker 2010 *GigaMesh MSII* + Multi-Scale Difference of Gaussians
/// `r=1,3,6` + Weingarten 주곡률 법선 발산 + 비선형 시그모이드 먹물 전이 곡선) RGBA 영상을 생성한다.
/// - 실루엣 바깥 영역은 `alpha = 0`으로 투명 처리하여 방안지 눈금과 자연스럽게 합성된다.
/// - 오목한 음각 홈(인화문·타날문·명문)에는 먹솜이 닿지 않아 한지 백색(`#FAF9F5`, `>220`)으로 남고,
///   볼록한 양각 능선과 물레속선에는 시그모이드 곡선을 따라 깊은 먹물(`#141413`, `<75`)이 정밀하게 착색된다.
pub fn render_orthographic_rubbing_rgba_with_params(
    mesh: &TriMesh,
    kind: PlateRasterKind,
    psi_deg: f64,
    params: &AcademicRubbingParams,
    width: u32,
    height: u32,
) -> RgbaImage {
    let w = width.clamp(48, 640) as usize;
    let h = height.clamp(48, 640) as usize;
    let mut img = RgbaImage::from_pixel(w as u32, h as u32, Rgba([255, 255, 255, 0]));
    if mesh.positions.is_empty() || mesh.triangles.is_empty() {
        return img;
    }

    let psi_rad = psi_deg.to_radians();
    let (sin_p, cos_p) = psi_rad.sin_cos();

    // 정점별 직교 투영 좌표 (u, v, depth) 및 회전 법선 계산
    let mut proj_uvz = Vec::with_capacity(mesh.positions.len());
    let mut rot_norms = Vec::with_capacity(mesh.positions.len());
    let mut u_min = f64::INFINITY;
    let mut u_max = f64::NEG_INFINITY;
    let mut v_min = f64::INFINITY;
    let mut v_max = f64::NEG_INFINITY;

    for (vi, &p) in mesh.positions.iter().enumerate() {
        let rx = p[0] * cos_p - p[1] * sin_p;
        let ry = p[0] * sin_p + p[1] * cos_p;
        let rz = p[2];

        let n = mesh.normals.get(vi).copied().unwrap_or([0.0, -1.0, 0.0]);
        let nrx = n[0] * cos_p - n[1] * sin_p;
        let nry = n[0] * sin_p + n[1] * cos_p;
        let nrz = n[2];

        let (u, v, depth, vn) = match kind {
            PlateRasterKind::FrontExteriorRubbing => (rx, rz, -ry, [nrx, -nry, nrz]),
            PlateRasterKind::BackInteriorRubbing => (-rx, rz, ry, [-nrx, nry, nrz]),
            PlateRasterKind::BottomPlanRubbing => (rx, ry, -rz, [nrx, nry, -nrz]),
        };

        u_min = u_min.min(u);
        u_max = u_max.max(u);
        v_min = v_min.min(v);
        v_max = v_max.max(v);
        proj_uvz.push([u, v, depth]);
        rot_norms.push([vn[0] as f32, vn[1] as f32, vn[2] as f32]);
    }

    let du = (u_max - u_min).max(1e-3);
    let dv = (v_max - v_min).max(1e-3);
    let pad = 2.0f64;
    let sx = ((w as f64) - pad * 2.0) / du;
    let sy = ((h as f64) - pad * 2.0) / dv;

    let mut zbuf = vec![f32::NEG_INFINITY; w * h];
    let mut nbuf = vec![[0.0f32, 1.0f32, 0.0f32]; w * h];
    let mut luma_buf = vec![0.75f32; w * h];
    let mut hit_buf = vec![false; w * h];

    let has_uv_tex =
        mesh.texture_image.is_some() && mesh.triangle_uvs.len() == mesh.triangles.len();

    for (ti, tri) in mesh.triangles.iter().enumerate() {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;
        let p0 = proj_uvz[i0];
        let p1 = proj_uvz[i1];
        let p2 = proj_uvz[i2];

        let x0 = (pad + (p0[0] - u_min) * sx) as f32;
        let y0 = ((h as f64) - 1.0 - pad - (p0[1] - v_min) * sy) as f32;
        let x1 = (pad + (p1[0] - u_min) * sx) as f32;
        let y1 = ((h as f64) - 1.0 - pad - (p1[1] - v_min) * sy) as f32;
        let x2 = (pad + (p2[0] - u_min) * sx) as f32;
        let y2 = ((h as f64) - 1.0 - pad - (p2[1] - v_min) * sy) as f32;

        let min_x = x0.min(x1).min(x2).floor().max(0.0) as i32;
        let max_x = x0.max(x1).max(x2).ceil().min((w - 1) as f32) as i32;
        let min_y = y0.min(y1).min(y2).floor().max(0.0) as i32;
        let max_y = y0.max(y1).max(y2).ceil().min((h - 1) as f32) as i32;
        if min_x > max_x || min_y > max_y {
            continue;
        }

        let denom = (y1 - y2) * (x0 - x2) + (x2 - x1) * (y0 - y2);
        if denom.abs() < 1e-6 {
            continue;
        }
        let inv_denom = 1.0 / denom;

        let n0 = rot_norms[i0];
        let n1 = rot_norms[i1];
        let n2 = rot_norms[i2];

        for py in min_y..=max_y {
            let fy = py as f32 + 0.5;
            for px in min_x..=max_x {
                let fx = px as f32 + 0.5;
                let w0 = ((y1 - y2) * (fx - x2) + (x2 - x1) * (fy - y2)) * inv_denom;
                let w1 = ((y2 - y0) * (fx - x2) + (x0 - x2) * (fy - y2)) * inv_denom;
                let w2 = 1.0 - w0 - w1;
                if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                    continue;
                }
                let z = (w0 as f64 * p0[2] + w1 as f64 * p1[2] + w2 as f64 * p2[2]) as f32;
                let idx = (py as usize) * w + (px as usize);
                if z > zbuf[idx] {
                    zbuf[idx] = z;
                    hit_buf[idx] = true;
                    let nx = w0 * n0[0] + w1 * n1[0] + w2 * n2[0];
                    let ny = w0 * n0[1] + w1 * n1[1] + w2 * n2[1];
                    let nz = w0 * n0[2] + w1 * n1[2] + w2 * n2[2];
                    let nlen = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-5);
                    nbuf[idx] = [nx / nlen, ny / nlen, nz / nlen];

                    if has_uv_tex {
                        if let Some(ref tex) = mesh.texture_image {
                            let uvs = mesh.triangle_uvs[ti];
                            let u = w0 * uvs[0][0] + w1 * uvs[1][0] + w2 * uvs[2][0];
                            let v = w0 * uvs[0][1] + w1 * uvs[1][1] + w2 * uvs[2][1];
                            let rgb = tex.sample_uv_rgb(u, v);
                            luma_buf[idx] = (0.299 * rgb[0] as f32
                                + 0.587 * rgb[1] as f32
                                + 0.114 * rgb[2] as f32)
                                / 255.0;
                        }
                    } else if let (Some(&c0), Some(&c1), Some(&c2)) = (
                        mesh.colors_rgb.get(i0),
                        mesh.colors_rgb.get(i1),
                        mesh.colors_rgb.get(i2),
                    ) {
                        let r = w0 * c0[0] as f32 + w1 * c1[0] as f32 + w2 * c2[0] as f32;
                        let g = w0 * c0[1] as f32 + w1 * c1[1] as f32 + w2 * c2[1] as f32;
                        let b = w0 * c0[2] as f32 + w1 * c1[2] as f32 + w2 * c2[2] as f32;
                        luma_buf[idx] = (0.299 * r + 0.587 * g + 0.114 * b) / 255.0;
                    }
                }
            }
        }
    }

    // 2차 패스: 학술 다중스케일 DoG (r=1, 3, 6) + Weingarten 주곡률 법선 발산 + 비선형 시그모이드 먹물 전이 모델
    let ink_strength = ((params.ink_density_pct as f32) / 100.0).clamp(0.15, 0.99);
    let detail_boost = params.msii_detail_boost.clamp(0.4, 2.8);
    let k_sig = params.sigmoid_contrast.clamp(2.0, 16.0);

    let box_mean_z = |cx: usize, cy: usize, radius: i32| -> f32 {
        let mut sum = 0.0f32;
        let mut cnt = 0.0f32;
        for dy in -radius..=radius {
            let ny = (cy as i32 + dy).clamp(0, (h - 1) as i32) as usize;
            for dx in -radius..=radius {
                let nx = (cx as i32 + dx).clamp(0, (w - 1) as i32) as usize;
                let n_idx = ny * w + nx;
                if hit_buf[n_idx] {
                    sum += zbuf[n_idx];
                    cnt += 1.0;
                }
            }
        }
        if cnt > 0.0 { sum / cnt } else { zbuf[cy * w + cx] }
    };

    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            if !hit_buf[idx] {
                continue;
            }

            let xl = x.saturating_sub(1);
            let xr = (x + 1).min(w - 1);
            let yu = y.saturating_sub(1);
            let yd = (y + 1).min(h - 1);

            let is_boundary = !hit_buf[y * w + xl]
                || !hit_buf[y * w + xr]
                || !hit_buf[yu * w + x]
                || !hit_buf[yd * w + x];

            if is_boundary {
                img.put_pixel(x as u32, y as u32, Rgba([22, 22, 20, 240]));
                continue;
            }

            let n_c = nbuf[idx];
            let n_l = nbuf[y * w + xl];
            let n_r = nbuf[y * w + xr];
            let n_u = nbuf[yu * w + x];
            let n_d = nbuf[yd * w + x];

            // (A) Weingarten 주곡률 근사 (법선 벡터장의 수치 발산 div(n) = dn_x/dx + dn_z/dy)
            let dn_x = (n_r[0] - n_l[0]) * 0.5;
            let dn_z = (n_u[2] - n_d[2]) * 0.5;
            let weingarten_curv = ((dn_x + dn_z) * 1.15).clamp(-1.2, 1.2);

            // (B) Mara & Krömker (2010) 다중스케일 적분 불변량 근사: 3-Scale Difference of Gaussians (r=1, 3, 6)
            let z_c = zbuf[idx];
            let z_r1 = box_mean_z(x, y, 1);
            let z_r3 = box_mean_z(x, y, 3);
            let z_r6 = box_mean_z(x, y, 5);
            let dog_fine = (z_c - z_r1) * 1.9;
            let dog_meso = (z_r1 - z_r3) * 1.4;
            let dog_macro = (z_c - z_r6) * 0.85;
            let msii_dog = ((0.45 * dog_fine + 0.35 * dog_meso + 0.20 * dog_macro) * detail_boost)
                .clamp(-1.4, 1.4);

            // (C) 사광(Oblique Raking Light) + 표면 명도 편차
            let raking = (-0.45 * n_c[0] + 0.60 * n_c[1].abs() + 0.65 * n_c[2]).clamp(0.05, 1.0);
            let tex_mod = (luma_buf[idx] - 0.52).clamp(-0.35, 0.35) * 0.45;
            let composite_relief =
                (0.52 * msii_dog + 0.38 * weingarten_curv + 0.18 * (0.65 - raking) + tex_mod)
                    .clamp(-1.3, 1.3);

            // (D) 한지 습탁(Wet Hanji) 비선형 시그모이드 먹물 전이 함수
            let grain = sample_hanji_paper_grain(x as u32, y as u32, 0.18);
            let ink_deposit = if composite_relief < -0.14 {
                // 오목한 음각 홈(인화문·타날문 골짜기): 한지가 떠서 먹솜이 닿지 않음 -> 한지 여백(#FAF9F5) 유지
                ((0.06 + (composite_relief + 0.14) * 0.05).max(0.0)) * ink_strength * 0.15
            } else {
                let style_bias = match params.style {
                    AcademicRubbingStyle::WetHanjiInk => 0.04,
                    AcademicRubbingStyle::DryGraphite => -0.05,
                    AcademicRubbingStyle::MsiiRidgeEnhance => 0.10,
                };
                let sig = 1.0 / (1.0 + (-k_sig * (composite_relief + style_bias)).exp());
                let fiber_mod = match params.style {
                    AcademicRubbingStyle::DryGraphite => 0.78 + 0.38 * grain,
                    _ => 0.88 + 0.22 * grain,
                };
                (sig * ink_strength * fiber_mod).clamp(0.03, 0.96)
            };

            let r = (250.0 - ink_deposit * (250.0 - 18.0)).round().clamp(16.0, 252.0) as u8;
            let g = (249.0 - ink_deposit * (249.0 - 18.0)).round().clamp(16.0, 251.0) as u8;
            let b = (245.0 - ink_deposit * (245.0 - 17.0)).round().clamp(15.0, 248.0) as u8;

            img.put_pixel(x as u32, y as u32, Rgba([r, g, b, 246]));
        }
    }

    img
}

/// 문화재청 발굴조사보고서 도판 상단 표제란(Title Block, `media_1793488638383_24317f2d.jpg` 1:1 대응) 텍스트 4종을 생성한다.
/// 반환값: `(좌측 유적명 셀, 중앙 도판 국문 제목, 중앙 도판 영문 부제, 우측 유물·조사기관 셀)`
pub fn build_chs_plate_title_block(
    title: &str,
    mode_label: &str,
    report: &ArtifactMeasurementReport,
) -> (String, String, String, String) {
    let left_cell = String::from("유적명 : 국가유산청 발굴조사 연구단");
    let center_title = format!("圖版 · 출토유물 정밀 실측도 — {}", title);
    let center_sub = format!("(Plate. Excavated Artifact Drawings · {})", mode_label);
    let right_cell = format!(
        "조사기관 : 동국유물실측 · STRATA (축척 {})",
        report.recommended_scale.label()
    );
    (left_cell, center_title, center_sub, right_cell)
}

/// 정밀 탁본 래스터 이미지를 도판 좌/우 반쪽 클리핑 및 사용자 드래그 ROI(`rubbing_roi_mm`) 영역에 맞춰 마스킹한다 (`Todo 7`).
pub fn rgba_to_color_image_clipped_with_roi(
    src: &RgbaImage,
    region_rect_mm: [f64; 4],
    clip_half_sign: Option<i8>,
    roi_mm: Option<[f64; 4]>,
) -> egui::ColorImage {
    let w = src.width() as usize;
    let h = src.height() as usize;
    let mid_x = w / 2;
    let rx_min = region_rect_mm[0].min(region_rect_mm[2]);
    let rx_max = region_rect_mm[0].max(region_rect_mm[2]);
    let ry_min = region_rect_mm[1].min(region_rect_mm[3]);
    let ry_max = region_rect_mm[1].max(region_rect_mm[3]);
    let rw_mm = (rx_max - rx_min).max(1e-3);
    let rh_mm = (ry_max - ry_min).max(1e-3);

    let mut pixels = Vec::with_capacity(w * h);
    for y in 0..h {
        let py_mm = ry_max - (y as f64 / (h.max(1) as f64)) * rh_mm;
        for x in 0..w {
            let px_mm = rx_min + (x as f64 / (w.max(1) as f64)) * rw_mm;
            let clipped_half = match clip_half_sign {
                Some(s) if s < 0 => x > mid_x,
                Some(s) if s > 0 => x < mid_x,
                _ => false,
            };
            let clipped_roi = if let Some([ox0, oy0, ox1, oy1]) = roi_mm {
                px_mm < ox0 || px_mm > ox1 || py_mm < oy0 || py_mm > oy1
            } else {
                false
            };

            if clipped_half || clipped_roi {
                pixels.push(egui::Color32::TRANSPARENT);
            } else {
                let p = src.get_pixel(x as u32, y as u32);
                pixels.push(egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]));
            }
        }
    }
    egui::ColorImage {
        size: [w, h],
        pixels,
    }
}

/// 우측 `73%` (또는 `Tab` 키 `100%`) 2D 실측 도판 작업대(`SheetPane`)를 렌더링한다.
/// - 화면 전체(`sheet_rect` 100%)를 여백 없는 고고학 실측 방안지(1mm·5mm·10mm 격자 + 상/좌 cm 눈금자)로 렌더링한다 (`Todo 5`).
/// - 사용자가 탁본할 위치를 마우스로 드래그하면 해당 ROI만 탁본이 표시되고 『부분 정밀 탁본 창』이 열려 먹물 농도와 학술 MSII 감도를 실시간 조정할 수 있다 (`Todo 6 & Todo 7`).
#[allow(clippy::too_many_arguments)]
pub fn render_sheet_pane_egui(
    ui: &mut egui::Ui,
    plate: &ComposedPlate2D,
    report: &ArtifactMeasurementReport,
    mesh: &TriMesh,
    psi_deg: f64,
    ink_density_pct: f64,
    show_mm_grid: bool,
    state: &mut SheetPaneState,
) -> (egui::Rect, egui::Rect, egui::Rect) {
    let avail = ui.available_size();
    let header_h = 32.0;
    let strip_h = 28.0;
    let canvas_h = (avail.y - header_h - strip_h - 8.0).max(160.0);

    let ground = egui::Color32::from_rgb(GROUND.rgb[0], GROUND.rgb[1], GROUND.rgb[2]);
    let wash = egui::Color32::from_rgb(WASH.rgb[0], WASH.rgb[1], WASH.rgb[2]);
    let paper = egui::Color32::from_rgb(SURFACE.rgb[0], SURFACE.rgb[1], SURFACE.rgb[2]);
    let border = egui::Color32::from_rgb(EDGE.rgb[0], EDGE.rgb[1], EDGE.rgb[2]);
    let line_col = egui::Color32::from_rgb(LINE.rgb[0], LINE.rgb[1], LINE.rgb[2]);
    let grid_10 = egui::Color32::from_rgb(198, 192, 174);
    let grid_5 = egui::Color32::from_rgb(220, 215, 200);
    let grid_1 = egui::Color32::from_rgb(GRID_1MM.rgb[0], GRID_1MM.rgb[1], GRID_1MM.rgb[2]);
    let ink = egui::Color32::from_rgb(INK.rgb[0], INK.rgb[1], INK.rgb[2]);
    let muted = egui::Color32::from_rgb(MUTED.rgb[0], MUTED.rgb[1], MUTED.rgb[2]);
    let terra = egui::Color32::from_rgb(ACTION.rgb[0], ACTION.rgb[1], ACTION.rgb[2]);
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
    let done_bg = egui::Color32::from_rgb(DONE_BG.rgb[0], DONE_BG.rgb[1], DONE_BG.rgb[2]);
    let done_fg = egui::Color32::from_rgb(DONE_INK.rgb[0], DONE_INK.rgb[1], DONE_INK.rgb[2]);

    // 외부 ink_density_pct와 동기화하되, 부분 탁본창에서 조정한 값을 우선 반영
    if (state.rubbing_params.ink_density_pct - ink_density_pct).abs() > 0.5
        && !state.show_roi_rubbing_window
    {
        state.rubbing_params.ink_density_pct = ink_density_pct;
    }

    let roi_q = state.rubbing_roi_mm.map(|r| {
        [
            (r[0] * 10.0).round() as i32,
            (r[1] * 10.0).round() as i32,
            (r[2] * 10.0).round() as i32,
            (r[3] * 10.0).round() as i32,
        ]
    });
    let key = (
        mesh.name.clone(),
        psi_deg.round() as i32,
        state.rubbing_params.ink_density_pct.round() as i32,
        (state.rubbing_params.msii_detail_boost * 100.0).round() as i32,
        (state.rubbing_params.sigmoid_contrast * 10.0).round() as i32,
        state.rubbing_params.style,
        plate.flip_exterior_right,
        roi_q,
    );
    if state.cached_key.as_ref() != Some(&key) {
        let front_img = render_orthographic_rubbing_rgba_with_params(
            mesh,
            PlateRasterKind::FrontExteriorRubbing,
            psi_deg,
            &state.rubbing_params,
            256,
            256,
        );
        let back_img = render_orthographic_rubbing_rgba_with_params(
            mesh,
            PlateRasterKind::BackInteriorRubbing,
            psi_deg,
            &state.rubbing_params,
            232,
            232,
        );
        let bottom_img = render_orthographic_rubbing_rgba_with_params(
            mesh,
            PlateRasterKind::BottomPlanRubbing,
            psi_deg,
            &state.rubbing_params,
            208,
            208,
        );
        state.cached_front_img = Some(front_img);
        state.cached_back_img = Some(back_img);
        state.cached_bottom_img = Some(bottom_img);
        state.cached_front_tex = None;
        state.cached_back_tex = None;
        state.cached_bottom_tex = None;
        state.cached_key = Some(key);
    }

    let mut out_header_rect = egui::Rect::NOTHING;
    let mut out_strip_rect = egui::Rect::NOTHING;
    let mut out_sheet_rect = egui::Rect::NOTHING;

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;

        // 1. 도판 머리줄 (32px): 도판 제목 + [영역 드래그 탁본] + [부분 탁본창] + 축척 배지
        let (header_rect, _) =
            ui.allocate_exact_size(egui::vec2(avail.x, header_h), egui::Sense::hover());
        out_header_rect = header_rect;
        let hp = ui.painter_at(header_rect);
        hp.rect_filled(header_rect, 3.0, ground);
        hp.rect_stroke(
            header_rect,
            3.0,
            egui::Stroke::new(1.0_f32, line_col),
            egui::StrokeKind::Inside,
        );
        hp.text(
            egui::pos2(header_rect.left() + 12.0, header_rect.center().y),
            egui::Align2::LEFT_CENTER,
            format!("{}  ·  {}", plate.title, plate.mode.label_kr()),
            egui::FontId::proportional(13.0),
            ink,
        );

        // 우측 컨트롤 버튼들: [영역 드래그 탁본] · [탁본 농도·학술창] · [전체 탁본] · 상태 배지
        let badge_w = 196.0;
        let badge_rect = egui::Rect::from_min_size(
            egui::pos2(
                header_rect.right() - badge_w - 8.0,
                header_rect.center().y - 11.0,
            ),
            egui::vec2(badge_w, 22.0),
        );
        hp.rect_filled(badge_rect, 4.0, done_bg);
        hp.text(
            badge_rect.center(),
            egui::Align2::CENTER_CENTER,
            format!(
                "● 축척 {} · 방안지 {:.0}%",
                report.recommended_scale.label(),
                state.zoom * 100.0
            ),
            egui::FontId::proportional(11.0),
            done_fg,
        );

        // [영역 드래그 탁본] 버튼 및 [탁본창 열기] 버튼을 헤더 우측 배지 왼쪽에 배치
        let btn_drag_rect = egui::Rect::from_min_size(
            egui::pos2(badge_rect.left() - 252.0, header_rect.center().y - 11.0),
            egui::vec2(124.0, 22.0),
        );
        let drag_btn_resp = ui.interact(
            btn_drag_rect,
            egui::Id::new("sheet_roi_drag_mode_btn"),
            egui::Sense::click(),
        );
        if drag_btn_resp.clicked() {
            state.roi_drag_mode = !state.roi_drag_mode;
        }
        hp.rect_filled(
            btn_drag_rect,
            4.0,
            if state.roi_drag_mode { terra } else { wash },
        );
        hp.rect_stroke(
            btn_drag_rect,
            4.0,
            egui::Stroke::new(1.0_f32, if state.roi_drag_mode { terra } else { border }),
            egui::StrokeKind::Inside,
        );
        hp.text(
            btn_drag_rect.center(),
            egui::Align2::CENTER_CENTER,
            if state.roi_drag_mode {
                "● 탁본영역 드래그중"
            } else {
                "⬚ 탁본영역 드래그"
            },
            egui::FontId::proportional(11.0),
            if state.roi_drag_mode { paper } else { ink },
        );

        let btn_win_rect = egui::Rect::from_min_size(
            egui::pos2(badge_rect.left() - 122.0, header_rect.center().y - 11.0),
            egui::vec2(114.0, 22.0),
        );
        let win_btn_resp = ui.interact(
            btn_win_rect,
            egui::Id::new("sheet_roi_window_btn"),
            egui::Sense::click(),
        );
        if win_btn_resp.clicked() {
            state.show_roi_rubbing_window = !state.show_roi_rubbing_window;
        }
        hp.rect_filled(
            btn_win_rect,
            4.0,
            if state.show_roi_rubbing_window { ink } else { wash },
        );
        hp.rect_stroke(
            btn_win_rect,
            4.0,
            egui::Stroke::new(1.0_f32, border),
            egui::StrokeKind::Inside,
        );
        hp.text(
            btn_win_rect.center(),
            egui::Align2::CENTER_CENTER,
            "정밀 탁본창(농도)",
            egui::FontId::proportional(11.0),
            if state.show_roi_rubbing_window {
                paper
            } else {
                ink
            },
        );

        // 2. 상단 28px 정밀 치수 대장 띠 (Metric Ledger Strip, #F5F4ED Wash)
        let (strip_rect, _) =
            ui.allocate_exact_size(egui::vec2(avail.x, strip_h), egui::Sense::hover());
        out_strip_rect = strip_rect;
        let strip_painter = ui.painter_at(strip_rect);
        strip_painter.rect_filled(strip_rect, 2.0, wash);
        strip_painter.rect_stroke(
            strip_rect,
            2.0,
            egui::Stroke::new(1.0_f32, border),
            egui::StrokeKind::Inside,
        );
        strip_painter.text(
            egui::pos2(strip_rect.left() + 12.0, strip_rect.center().y),
            egui::Align2::LEFT_CENTER,
            &report.metric_strip_summary,
            egui::FontId::monospace(12.0),
            ink,
        );

        // 3. 화면 전체(`sheet_rect` 100% Full-Bleed) 실제 고고학 실측 방안지 (1mm·5mm·10mm + 상/좌 cm 눈금자)
        let (sheet_rect, sheet_resp) = ui.allocate_exact_size(
            egui::vec2(avail.x, canvas_h),
            egui::Sense::click_and_drag(),
        );
        out_sheet_rect = sheet_rect;

        let spec = compute_full_bleed_graph_paper_spec(
            sheet_rect,
            plate.canvas_width_mm,
            plate.canvas_height_mm,
            state.zoom,
            state.pan_px,
        );
        let origin_x = spec.origin_px.x;
        let origin_y = spec.origin_px.y;
        let scale = spec.scale_px_per_mm;

        // 마우스 휠 줌, 팬 또는 탁본 ROI 드래그 인터랙션
        if sheet_resp.hovered() {
            let scroll_y = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll_y.abs() > 0.1 {
                let factor = (1.0 + scroll_y * 0.0025).clamp(0.85, 1.18);
                state.zoom = (state.zoom * factor).clamp(0.45, 4.0);
            }
        }

        let shift_held = ui.input(|i| i.modifiers.shift);
        if state.roi_drag_mode || shift_held {
            if sheet_resp.drag_started() {
                if let Some(pos) = sheet_resp.interact_pointer_pos() {
                    let mm_x = f64::from((pos.x - origin_x) / scale);
                    let mm_y = f64::from((origin_y - pos.y) / scale);
                    state.roi_drag_start_mm = Some([mm_x, mm_y]);
                }
            }
            if sheet_resp.dragged() {
                if let (Some(start_mm), Some(pos)) =
                    (state.roi_drag_start_mm, sheet_resp.interact_pointer_pos())
                {
                    let cur_x = f64::from((pos.x - origin_x) / scale);
                    let cur_y = f64::from((origin_y - pos.y) / scale);
                    state.set_rubbing_roi_mm(Some([start_mm[0], start_mm[1], cur_x, cur_y]));
                }
            }
            if sheet_resp.drag_stopped() {
                state.roi_drag_start_mm = None;
            }
        } else {
            if sheet_resp.dragged() {
                state.pan_px += sheet_resp.drag_delta();
            }
            if sheet_resp.double_clicked() {
                state.zoom = 1.0;
                state.pan_px = egui::Vec2::ZERO;
            }
        }

        let paper_painter = ui.painter_at(sheet_rect);
        // 화면 전체를 여백 없이 따뜻한 실측 방안지 원지(#FFFEFA)로 채움
        paper_painter.rect_filled(sheet_rect, 0.0, paper);

        // 3-A. 전체 화면 실제 방안지 격자 (1mm 세선, 5mm 보조 중선, 10mm(1cm) 주 격자선 + 상/좌 cm 눈금자)
        if show_mm_grid {
            for &gx in &spec.lines_1mm_x {
                paper_painter.line_segment(
                    [egui::pos2(gx, sheet_rect.top()), egui::pos2(gx, sheet_rect.bottom())],
                    egui::Stroke::new(0.45_f32, grid_1),
                );
            }
            for &gy in &spec.lines_1mm_y {
                paper_painter.line_segment(
                    [egui::pos2(sheet_rect.left(), gy), egui::pos2(sheet_rect.right(), gy)],
                    egui::Stroke::new(0.45_f32, grid_1),
                );
            }
            for &gx in &spec.lines_5mm_x {
                paper_painter.line_segment(
                    [egui::pos2(gx, sheet_rect.top()), egui::pos2(gx, sheet_rect.bottom())],
                    egui::Stroke::new(0.75_f32, grid_5),
                );
            }
            for &gy in &spec.lines_5mm_y {
                paper_painter.line_segment(
                    [egui::pos2(sheet_rect.left(), gy), egui::pos2(sheet_rect.right(), gy)],
                    egui::Stroke::new(0.75_f32, grid_5),
                );
            }
            for &gx in &spec.lines_10mm_x {
                paper_painter.line_segment(
                    [egui::pos2(gx, sheet_rect.top()), egui::pos2(gx, sheet_rect.bottom())],
                    egui::Stroke::new(1.05_f32, grid_10),
                );
            }
            for &gy in &spec.lines_10mm_y {
                paper_painter.line_segment(
                    [egui::pos2(sheet_rect.left(), gy), egui::pos2(sheet_rect.right(), gy)],
                    egui::Stroke::new(1.05_f32, grid_10),
                );
            }

            // 방안지 상단(18px) 및 좌측(28px) 실제 cm 눈금자(Ruler) 헤더 띠
            let top_ruler = egui::Rect::from_min_size(
                sheet_rect.min,
                egui::vec2(sheet_rect.width(), 18.0),
            );
            let left_ruler = egui::Rect::from_min_size(
                sheet_rect.min,
                egui::vec2(30.0, sheet_rect.height()),
            );
            paper_painter.rect_filled(
                top_ruler,
                0.0,
                egui::Color32::from_rgba_unmultiplied(245, 244, 237, 225),
            );
            paper_painter.rect_filled(
                left_ruler,
                0.0,
                egui::Color32::from_rgba_unmultiplied(245, 244, 237, 225),
            );
            for (gx, lbl) in &spec.cm_labels_top {
                if *gx > sheet_rect.left() + 34.0 && *gx < sheet_rect.right() - 18.0 {
                    paper_painter.text(
                        egui::pos2(*gx + 2.0, sheet_rect.top() + 9.0),
                        egui::Align2::LEFT_CENTER,
                        lbl,
                        egui::FontId::monospace(9.0),
                        muted,
                    );
                }
            }
            for (gy, lbl) in &spec.cm_labels_left {
                if *gy > sheet_rect.top() + 22.0 && *gy < sheet_rect.bottom() - 12.0 {
                    paper_painter.text(
                        egui::pos2(sheet_rect.left() + 3.0, *gy - 6.0),
                        egui::Align2::LEFT_CENTER,
                        lbl,
                        egui::FontId::monospace(9.0),
                        muted,
                    );
                }
            }
        }

        paper_painter.rect_stroke(
            sheet_rect,
            0.0,
            egui::Stroke::new(1.2_f32, border),
            egui::StrokeKind::Inside,
        );

        // 3-B. 다중 뷰 정밀 탁본(`Rubbing`) 래스터 영역 합성 (사용자 드래그 ROI 클리핑 + GPU 텍스처 캐시 재사용)
        for (ri, region) in plate.raster_regions.iter().enumerate() {
            let rx0 = origin_x + (region.rect_mm[0] as f32) * scale;
            let ry1 = origin_y - (region.rect_mm[1] as f32) * scale;
            let rx1 = origin_x + (region.rect_mm[2] as f32) * scale;
            let ry0 = origin_y - (region.rect_mm[3] as f32) * scale;
            let dst_rect = egui::Rect::from_min_max(
                egui::pos2(rx0.min(rx1), ry0.min(ry1)),
                egui::pos2(rx0.max(rx1), ry0.max(ry1)),
            );

            let tex_id_opt = match region.kind {
                PlateRasterKind::FrontExteriorRubbing => {
                    if state.cached_front_tex.is_none() {
                        if let Some(ref src_img) = state.cached_front_img {
                            let color_img = rgba_to_color_image_clipped_with_roi(
                                src_img,
                                region.rect_mm,
                                region.clip_half_sign,
                                state.rubbing_roi_mm,
                            );
                            state.cached_front_tex = Some(ui.ctx().load_texture(
                                format!("sheet_rubbing_front_{ri}"),
                                color_img,
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                    }
                    state.cached_front_tex.as_ref().map(|t| t.id())
                }
                PlateRasterKind::BackInteriorRubbing => {
                    if state.cached_back_tex.is_none() {
                        if let Some(ref src_img) = state.cached_back_img {
                            let color_img = rgba_to_color_image_clipped_with_roi(
                                src_img,
                                region.rect_mm,
                                region.clip_half_sign,
                                state.rubbing_roi_mm,
                            );
                            state.cached_back_tex = Some(ui.ctx().load_texture(
                                format!("sheet_rubbing_back_{ri}"),
                                color_img,
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                    }
                    state.cached_back_tex.as_ref().map(|t| t.id())
                }
                PlateRasterKind::BottomPlanRubbing => {
                    if state.cached_bottom_tex.is_none() {
                        if let Some(ref src_img) = state.cached_bottom_img {
                            let color_img = rgba_to_color_image_clipped_with_roi(
                                src_img,
                                region.rect_mm,
                                region.clip_half_sign,
                                state.rubbing_roi_mm,
                            );
                            state.cached_bottom_tex = Some(ui.ctx().load_texture(
                                format!("sheet_rubbing_bottom_{ri}"),
                                color_img,
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                    }
                    state.cached_bottom_tex.as_ref().map(|t| t.id())
                }
            };

            if let Some(tex_id) = tex_id_opt {
                paper_painter.image(
                    tex_id,
                    dst_rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
        }

        // 사용자가 드래그하여 지정한 탁본 ROI 사각형 표시 (파란색/테라코타 가이드 프레임)
        if let Some([ox0, oy0, ox1, oy1]) = state.rubbing_roi_mm {
            let sx0 = origin_x + (ox0 as f32) * scale;
            let sy1 = origin_y - (oy0 as f32) * scale;
            let sx1 = origin_x + (ox1 as f32) * scale;
            let sy0 = origin_y - (oy1 as f32) * scale;
            let roi_rect = egui::Rect::from_min_max(
                egui::pos2(sx0.min(sx1), sy0.min(sy1)),
                egui::pos2(sx0.max(sx1), sy0.max(sy1)),
            );
            paper_painter.rect_stroke(
                roi_rect,
                2.0,
                egui::Stroke::new(1.6_f32, select_blue),
                egui::StrokeKind::Inside,
            );
            paper_painter.text(
                egui::pos2(roi_rect.left() + 4.0, (roi_rect.top() - 12.0).max(sheet_rect.top() + 20.0)),
                egui::Align2::LEFT_TOP,
                format!("부분 탁본 영역 ({:.0}×{:.0}mm)", (ox1 - ox0).abs(), (oy1 - oy0).abs()),
                egui::FontId::proportional(10.5),
                select_blue,
            );
        }

        // 3-C. 도판 경로 렌더링 (Ear-Clipping `#141413` 먹물 채움 단면 폴리곤 + 실선/파선/교차 블록 축척자)
        for elem in &plate.paths {
            if elem.points_mm.len() < 2 {
                continue;
            }
            let stroke_col = match elem.stroke_hex.as_str() {
                "#B5573A" => terra,
                "#FF0000" => profile_red,
                "#2A78D6" => select_blue,
                "#5E5D59" => muted,
                _ => ink,
            };

            // (1) `#141413` 솔리드 채움 다각형 (수직 회전단면, 기와 단면, 금속 코어 단면 등)
            if let Some(ref fill_hex) = elem.fill_hex {
                if elem.points_mm.len() >= 3 {
                    let tris = triangulate_polygon_ear_clipping(&elem.points_mm);
                    if !tris.is_empty() {
                        let fill_col = if fill_hex == "#B5573A" { terra } else { ink };
                        let mut mesh_shape = egui::epaint::Mesh::default();
                        for p in &elem.points_mm {
                            let pos = egui::pos2(
                                origin_x + (p[0] as f32) * scale,
                                origin_y - (p[1] as f32) * scale,
                            );
                            mesh_shape.vertices.push(egui::epaint::Vertex {
                                pos,
                                uv: egui::epaint::WHITE_UV,
                                color: fill_col,
                            });
                        }
                        for tri in tris {
                            mesh_shape.indices.push(tri[0] as u32);
                            mesh_shape.indices.push(tri[1] as u32);
                            mesh_shape.indices.push(tri[2] as u32);
                        }
                        paper_painter.add(egui::Shape::mesh(mesh_shape));
                    }
                }
            }

            // (2) 고고학 표준 교차 흑백 블록 축척자 (`치수_축척막대` / `축척자_5cm` 레이어)
            if (elem.layer_id == "치수_축척막대" || elem.layer_id == "축척자_5cm")
                && elem.points_mm.len() >= 2
            {
                let p_start = elem.points_mm[0];
                let p_end = elem.points_mm[elem.points_mm.len() - 1];
                let x0 = origin_x + (p_start[0] as f32) * scale;
                let x1 = origin_x + (p_end[0] as f32) * scale;
                let y_bar = origin_y - (p_start[1] as f32) * scale;
                let bar_w = (x1 - x0).abs().max(25.0);
                let bar_h = (1.8 * scale).clamp(4.5, 8.5);
                let seg_w = bar_w / 5.0;

                for i in 0..5 {
                    let sx0 = x0.min(x1) + (i as f32) * seg_w;
                    let seg_rect = egui::Rect::from_min_size(
                        egui::pos2(sx0, y_bar - bar_h),
                        egui::vec2(seg_w, bar_h),
                    );
                    let seg_fill = if i % 2 == 0 { ink } else { paper };
                    paper_painter.rect_filled(seg_rect, 0.0, seg_fill);
                    paper_painter.rect_stroke(
                        seg_rect,
                        0.0,
                        egui::Stroke::new(1.0_f32, ink),
                        egui::StrokeKind::Inside,
                    );
                }
                continue;
            }

            // (3) 일반 실선 및 파선(Dashed) 제도 경로
            let mut pts: Vec<egui::Pos2> = elem
                .points_mm
                .iter()
                .map(|p| {
                    egui::pos2(
                        origin_x + (p[0] as f32) * scale,
                        origin_y - (p[1] as f32) * scale,
                    )
                })
                .collect();
            if elem.closed && pts.len() >= 3 {
                if let Some(&first_pt) = pts.first() {
                    pts.push(first_pt);
                }
            }

            let w_px = ((elem.stroke_width_mm as f32) * scale).clamp(1.0, 3.8);
            if let Some((dash_mm, gap_mm)) = elem.dash_pattern_mm {
                let dash_px = ((dash_mm as f32) * scale).max(3.5);
                let gap_px = ((gap_mm as f32) * scale).max(2.5);
                draw_dashed_polyline(&paper_painter, &pts, w_px, stroke_col, dash_px, gap_px);
            } else {
                paper_painter.add(egui::Shape::line(pts, egui::Stroke::new(w_px, stroke_col)));
            }
        }

        // 3-D. 텍스트 주석 및 치수 콜아웃 (상단 표제란·대장 띠와 중복되는 장문 요약줄은 생략하여 도판 여백 정밀 유지)
        for c in &plate.callouts {
            if c.text.contains("평균두께") || c.text.contains("최대동경") {
                continue;
            }
            let pos = egui::pos2(
                (origin_x + (c.pos_mm[0] as f32) * scale)
                    .clamp(sheet_rect.left() + 34.0, sheet_rect.right() - 150.0),
                (origin_y - (c.pos_mm[1] as f32) * scale)
                    .clamp(sheet_rect.top() + 58.0, sheet_rect.bottom() - 18.0),
            );
            let col = match c.color_hex.as_str() {
                "#B5573A" => terra,
                "#5E5D59" => muted,
                _ => ink,
            };
            paper_painter.text(
                pos,
                egui::Align2::LEFT_TOP,
                &c.text,
                egui::FontId::proportional(11.0),
                col,
            );
        }

        // 3-E. 문화재청 발굴조사보고서 도판 상단 표제란(Title Block) & 정밀 치수 화살표 보조선 (`media_1793488638383_24317f2d.jpg` 1:1 구현)
        let (left_cell, center_title, center_sub, right_cell) =
            build_chs_plate_title_block(&plate.title, plate.mode.label_kr(), report);
        let tb_rect = egui::Rect::from_min_max(
            egui::pos2(sheet_rect.left() + 26.0, sheet_rect.top() + 20.0),
            egui::pos2(sheet_rect.right() - 4.0, sheet_rect.top() + 54.0),
        );
        paper_painter.rect_filled(
            tb_rect,
            0.0,
            egui::Color32::from_rgba_unmultiplied(250, 249, 245, 236),
        );
        paper_painter.rect_stroke(
            tb_rect,
            0.0,
            egui::Stroke::new(1.2_f32, ink),
            egui::StrokeKind::Inside,
        );
        let col1_x = tb_rect.left() + tb_rect.width() * 0.25;
        let col2_x = tb_rect.left() + tb_rect.width() * 0.72;
        paper_painter.line_segment(
            [
                egui::pos2(col1_x, tb_rect.top()),
                egui::pos2(col1_x, tb_rect.bottom()),
            ],
            egui::Stroke::new(1.0_f32, ink),
        );
        paper_painter.line_segment(
            [
                egui::pos2(col2_x, tb_rect.top()),
                egui::pos2(col2_x, tb_rect.bottom()),
            ],
            egui::Stroke::new(1.0_f32, ink),
        );
        paper_painter.line_segment(
            [
                egui::pos2(col2_x, tb_rect.center().y),
                egui::pos2(tb_rect.right(), tb_rect.center().y),
            ],
            egui::Stroke::new(0.8_f32, border),
        );
        paper_painter.text(
            egui::pos2(tb_rect.left() + 8.0, tb_rect.center().y),
            egui::Align2::LEFT_CENTER,
            &left_cell,
            egui::FontId::proportional(10.5),
            ink,
        );
        paper_painter.text(
            egui::pos2((col1_x + col2_x) * 0.5, tb_rect.top() + 10.0),
            egui::Align2::CENTER_CENTER,
            &center_title,
            egui::FontId::proportional(11.5),
            ink,
        );
        paper_painter.text(
            egui::pos2((col1_x + col2_x) * 0.5, tb_rect.bottom() - 9.0),
            egui::Align2::CENTER_CENTER,
            &center_sub,
            egui::FontId::proportional(9.5),
            muted,
        );
        paper_painter.text(
            egui::pos2(col2_x + 8.0, tb_rect.top() + 8.5),
            egui::Align2::LEFT_CENTER,
            format!("유구·유물 : {}", plate.title),
            egui::FontId::proportional(9.5),
            ink,
        );
        paper_painter.text(
            egui::pos2(col2_x + 8.0, tb_rect.bottom() - 8.5),
            egui::Align2::LEFT_CENTER,
            &right_cell,
            egui::FontId::proportional(9.5),
            ink,
        );

        // 유물 외곽 구경/복원 구역 수평 치수선(◀─── 복원 구역 / 구경 ───▶) 및 기고/높이 수직 치수선(▲ │ 높이 │ ▼)
        if let Some(first_reg) = plate.raster_regions.first() {
            let rx0 = origin_x + (first_reg.rect_mm[0].min(first_reg.rect_mm[2]) as f32) * scale;
            let rx1 = origin_x + (first_reg.rect_mm[0].max(first_reg.rect_mm[2]) as f32) * scale;
            let ry_top = origin_y - (first_reg.rect_mm[1].max(first_reg.rect_mm[3]) as f32) * scale;
            let ry_bot = origin_y - (first_reg.rect_mm[1].min(first_reg.rect_mm[3]) as f32) * scale;

            // (a) 상단 수평 복원 구역·구경 화살표 치수선
            let dim_y = (ry_top - 16.0).max(tb_rect.bottom() + 12.0);
            paper_painter.line_segment(
                [egui::pos2(rx0, ry_top - 2.0), egui::pos2(rx0, dim_y - 4.0)],
                egui::Stroke::new(0.8_f32, muted),
            );
            paper_painter.line_segment(
                [egui::pos2(rx1, ry_top - 2.0), egui::pos2(rx1, dim_y - 4.0)],
                egui::Stroke::new(0.8_f32, muted),
            );
            paper_painter.line_segment(
                [egui::pos2(rx0, dim_y), egui::pos2(rx1, dim_y)],
                egui::Stroke::new(1.0_f32, ink),
            );
            // 좌우 화살촉
            for &(ax, dir_s) in &[(rx0, 1.0_f32), (rx1, -1.0_f32)] {
                paper_painter.add(egui::Shape::convex_polygon(
                    vec![
                        egui::pos2(ax, dim_y),
                        egui::pos2(ax + dir_s * 5.5, dim_y - 2.6),
                        egui::pos2(ax + dir_s * 5.5, dim_y + 2.6),
                    ],
                    ink,
                    egui::Stroke::NONE,
                ));
            }
            let rim_mm = (first_reg.rect_mm[2] - first_reg.rect_mm[0]).abs();
            paper_painter.text(
                egui::pos2((rx0 + rx1) * 0.5, dim_y - 7.0),
                egui::Align2::CENTER_CENTER,
                format!("복원 구역 · 구경 ({:.1}mm)", rim_mm),
                egui::FontId::proportional(9.5),
                ink,
            );

            // (b) 좌측 수직 높이·잔존고 화살표 치수선
            let dim_x = (rx0 - 18.0).max(sheet_rect.left() + 36.0);
            paper_painter.line_segment(
                [egui::pos2(dim_x - 4.0, ry_top), egui::pos2(rx0 - 2.0, ry_top)],
                egui::Stroke::new(0.8_f32, muted),
            );
            paper_painter.line_segment(
                [egui::pos2(dim_x - 4.0, ry_bot), egui::pos2(rx0 - 2.0, ry_bot)],
                egui::Stroke::new(0.8_f32, muted),
            );
            paper_painter.line_segment(
                [egui::pos2(dim_x, ry_top), egui::pos2(dim_x, ry_bot)],
                egui::Stroke::new(1.0_f32, ink),
            );
            for &(ay, dir_s) in &[(ry_top, 1.0_f32), (ry_bot, -1.0_f32)] {
                paper_painter.add(egui::Shape::convex_polygon(
                    vec![
                        egui::pos2(dim_x, ay),
                        egui::pos2(dim_x - 2.6, ay + dir_s * 5.5),
                        egui::pos2(dim_x + 2.6, ay + dir_s * 5.5),
                    ],
                    ink,
                    egui::Stroke::NONE,
                ));
            }
            let h_mm = (first_reg.rect_mm[3] - first_reg.rect_mm[1]).abs();
            paper_painter.text(
                egui::pos2(dim_x - 6.0, (ry_top + ry_bot) * 0.5),
                egui::Align2::RIGHT_CENTER,
                format!("높이 {:.1}mm", h_mm),
                egui::FontId::proportional(9.5),
                ink,
            );
        }

        // 하단 좌측 0..5..10cm 교차 블록 보조 축척자 (`media_1793488638383_24317f2d.jpg` 하단 좌측 10cm 바 대응)
        let left_sb_x = sheet_rect.left() + 46.0;
        let left_sb_y = sheet_rect.bottom() - 22.0;
        let left_sb_w = (50.0 * scale).clamp(80.0, 160.0);
        let left_sb_h = 5.5_f32;
        for bi in 0..4 {
            let bx = left_sb_x + (bi as f32) * (left_sb_w / 4.0);
            let b_rect = egui::Rect::from_min_size(
                egui::pos2(bx, left_sb_y),
                egui::vec2(left_sb_w / 4.0, left_sb_h),
            );
            paper_painter.rect_filled(b_rect, 0.0, if bi % 2 == 0 { ink } else { paper });
            paper_painter.rect_stroke(
                b_rect,
                0.0,
                egui::Stroke::new(0.9_f32, ink),
                egui::StrokeKind::Inside,
            );
        }
        paper_painter.text(
            egui::pos2(left_sb_x, left_sb_y + left_sb_h + 7.0),
            egui::Align2::LEFT_CENTER,
            "0",
            egui::FontId::monospace(9.5),
            ink,
        );
        paper_painter.text(
            egui::pos2(left_sb_x + left_sb_w * 0.5, left_sb_y + left_sb_h + 7.0),
            egui::Align2::CENTER_CENTER,
            "5",
            egui::FontId::monospace(9.5),
            ink,
        );
        paper_painter.text(
            egui::pos2(left_sb_x + left_sb_w, left_sb_y + left_sb_h + 7.0),
            egui::Align2::RIGHT_CENTER,
            "10cm",
            egui::FontId::monospace(9.5),
            ink,
        );

        // 4. 『부분 정밀 탁본 창 (ROI Rubbing Studio · 학술 MSII+DoG+Sigmoid 엔진)』 (Todo 6 & Todo 7)
        if state.show_roi_rubbing_window {
            let mut open = state.show_roi_rubbing_window;
            egui::Window::new("부분 정밀 탁본 창 (ROI Rubbing Studio · 학술 MSII+DoG 엔진)")
                .open(&mut open)
                .default_pos(egui::pos2(sheet_rect.right() - 365.0, sheet_rect.top() + 14.0))
                .default_width(340.0)
                .resizable(false)
                .collapsible(false)
                .show(ui.ctx(), |ui| {
                    ui.colored_label(
                        muted,
                        "도판 위에서 [⬚ 탁본영역 드래그] 후 마우스로 드래그하면 선택한 부위만 정밀 탁본이 추출됩니다.",
                    );
                    ui.add_space(4.0);

                    // 선택된 부분 탁본 확대 미리보기 뷰포트 (320×128, ROI 영역으로 자동 UV 줌인)
                    let (prev_rect, _) =
                        ui.allocate_exact_size(egui::vec2(320.0, 128.0), egui::Sense::hover());
                    let pp = ui.painter_at(prev_rect);
                    pp.rect_filled(prev_rect, 4.0, paper);
                    pp.rect_stroke(
                        prev_rect,
                        4.0,
                        egui::Stroke::new(1.0_f32, border),
                        egui::StrokeKind::Inside,
                    );
                    if let Some(ref tex) = state.cached_front_tex {
                        let uv_rect = if let (Some(roi), Some(first_reg)) =
                            (state.rubbing_roi_mm, plate.raster_regions.first())
                        {
                            let rx0 = first_reg.rect_mm[0].min(first_reg.rect_mm[2]);
                            let rx1 = first_reg.rect_mm[0].max(first_reg.rect_mm[2]);
                            let ry0 = first_reg.rect_mm[1].min(first_reg.rect_mm[3]);
                            let ry1 = first_reg.rect_mm[1].max(first_reg.rect_mm[3]);
                            let rw = (rx1 - rx0).max(1.0);
                            let rh = (ry1 - ry0).max(1.0);
                            let u0 = (((roi[0] - rx0) / rw) as f32).clamp(0.0, 0.48);
                            let u1 = (((roi[2] - rx0) / rw) as f32).clamp(u0 + 0.12, 0.50);
                            let v0 = (((ry1 - roi[3]) / rh) as f32).clamp(0.0, 0.85);
                            let v1 = (((ry1 - roi[1]) / rh) as f32).clamp(v0 + 0.12, 1.0);
                            egui::Rect::from_min_max(egui::pos2(u0, v0), egui::pos2(u1, v1))
                        } else {
                            egui::Rect::from_min_max(egui::pos2(0.02, 0.05), egui::pos2(0.49, 0.95))
                        };
                        pp.image(
                            tex.id(),
                            prev_rect.shrink(6.0),
                            uv_rect,
                            egui::Color32::WHITE,
                        );
                    }

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        for st in [
                            AcademicRubbingStyle::WetHanjiInk,
                            AcademicRubbingStyle::DryGraphite,
                            AcademicRubbingStyle::MsiiRidgeEnhance,
                        ] {
                            let is_sel = state.rubbing_params.style == st;
                            if ui
                                .selectable_label(is_sel, st.label_kr())
                                .clicked()
                            {
                                state.rubbing_params.style = st;
                            }
                        }
                    });

                    egui::Grid::new("roi_rubbing_academic_grid")
                        .num_columns(2)
                        .spacing([8.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("먹물 농도:");
                            ui.add(
                                egui::Slider::new(
                                    &mut state.rubbing_params.ink_density_pct,
                                    18.0..=98.0,
                                )
                                .suffix("%")
                                .fixed_decimals(0),
                            );
                            ui.end_row();

                            ui.label("MSII 곡률 감도:");
                            ui.add(
                                egui::Slider::new(
                                    &mut state.rubbing_params.msii_detail_boost,
                                    0.5..=2.5,
                                )
                                .suffix("×")
                                .fixed_decimals(2),
                            );
                            ui.end_row();

                            ui.label("시그모이드 음양각 대비:");
                            ui.add(
                                egui::Slider::new(
                                    &mut state.rubbing_params.sigmoid_contrast,
                                    2.5..=14.0,
                                )
                                .fixed_decimals(1),
                            );
                            ui.end_row();
                        });

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if let Some([x0, y0, x1, y1]) = state.rubbing_roi_mm {
                            ui.colored_label(
                                select_blue,
                                format!(
                                    "드래그 영역: X {:.0}~{:.0}mm · Y {:.0}~{:.0}mm",
                                    x0, x1, y0, y1
                                ),
                            );
                        } else {
                            ui.colored_label(muted, "현재: 유물 전체 영역 탁본 표시 중");
                        }
                        if ui.button("전체 탁본 복원").clicked() {
                            state.set_rubbing_roi_mm(None);
                            state.roi_drag_mode = false;
                        }
                    });
                });
            state.show_roi_rubbing_window = open;
        }
    });

    (out_header_rect, out_strip_rect, out_sheet_rect)
}

fn draw_dashed_polyline(
    painter: &egui::Painter,
    pts: &[egui::Pos2],
    width_px: f32,
    color: egui::Color32,
    dash_px: f32,
    gap_px: f32,
) {
    let stroke = egui::Stroke::new(width_px, color);
    let period = (dash_px + gap_px).max(2.0);
    for seg in pts.windows(2) {
        let a = seg[0];
        let b = seg[1];
        let delta = b - a;
        let len = delta.length();
        if len < 0.5 {
            continue;
        }
        let dir = delta / len;
        let mut t = 0.0f32;
        while t < len {
            let t_end = (t + dash_px).min(len);
            painter.line_segment([a + dir * t, a + dir * t_end], stroke);
            t += period;
        }
    }
}

/// 헤드리스 검증 투어(`--screenshot-tour`)용 5단 고정 레이아웃 전체 창(`1440 × 900 px`) 스냅샷 PNG를 생성한다.
pub fn render_full_workspace_offscreen_png(
    plate: &ComposedPlate2D,
    mesh: &arch3d_core::TriMesh,
    sheet_full_tab_mode: bool,
    with_dialog_overlay: bool,
) -> RgbaImage {
    let w = 1440u32;
    let h = 900u32;
    let mut img =
        RgbaImage::from_pixel(w, h, Rgba([GROUND.rgb[0], GROUND.rgb[1], GROUND.rgb[2], 255]));

    let fill_rect = |img: &mut RgbaImage, x0: u32, y0: u32, rw: u32, rh: u32, col: Rgba<u8>| {
        let x1 = (x0 + rw).min(w);
        let y1 = (y0 + rh).min(h);
        for y in y0..y1 {
            for x in x0..x1 {
                img.put_pixel(x, y, col);
            }
        }
    };

    let draw_line = |img: &mut RgbaImage, x0: i32, y0: i32, x1: i32, y1: i32, col: Rgba<u8>| {
        let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
        for s in 0..=steps {
            let px = x0 + (x1 - x0) * s / steps;
            let py = y0 + (y1 - y0) * s / steps;
            if px >= 0 && py >= 0 && (px as u32) < w && (py as u32) < h {
                img.put_pixel(px as u32, py as u32, col);
            }
        }
    };

    let fill_tri = |img: &mut RgbaImage, p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], col: Rgba<u8>| {
        let min_x = p0[0].min(p1[0]).min(p2[0]).floor().max(0.0) as i32;
        let max_x = p0[0].max(p1[0]).max(p2[0]).ceil().min((w - 1) as f32) as i32;
        let min_y = p0[1].min(p1[1]).min(p2[1]).floor().max(0.0) as i32;
        let max_y = p0[1].max(p1[1]).max(p2[1]).ceil().min((h - 1) as f32) as i32;
        let denom = (p1[1] - p2[1]) * (p0[0] - p2[0]) + (p2[0] - p1[0]) * (p0[1] - p2[1]);
        if denom.abs() < 1e-5 {
            return;
        }
        let inv = 1.0 / denom;
        for py in min_y..=max_y {
            let fy = py as f32 + 0.5;
            for px in min_x..=max_x {
                let fx = px as f32 + 0.5;
                let w0 = ((p1[1] - p2[1]) * (fx - p2[0]) + (p2[0] - p1[0]) * (fy - p2[1])) * inv;
                let w1 = ((p2[1] - p0[1]) * (fx - p2[0]) + (p0[0] - p2[0]) * (fy - p2[1])) * inv;
                let w2 = 1.0 - w0 - w1;
                if w0 >= -1e-4 && w1 >= -1e-4 && w2 >= -1e-4 {
                    img.put_pixel(px as u32, py as u32, col);
                }
            }
        }
    };

    let ink = Rgba([20, 20, 19, 255]);
    let wash = Rgba([245, 244, 237, 255]);
    let border = Rgba([209, 207, 196, 255]);
    let terra = Rgba([181, 87, 58, 255]);

    fill_rect(&mut img, 0, 0, w, 34, wash);
    fill_rect(&mut img, 0, 34, w, 1, border);
    fill_rect(&mut img, 0, 35, w, 98, Rgba([250, 249, 245, 255]));
    for i in 0..7 {
        let tx = 16 + i * 62;
        let ty = 48;
        let tile_col = if i == 0 { ink } else { wash };
        fill_rect(&mut img, tx, ty, 50, 50, tile_col);
        if i == 0 {
            fill_rect(&mut img, tx, ty + 48, 50, 2, terra);
        }
    }
    fill_rect(&mut img, 0, 132, w, 36, wash);
    fill_rect(&mut img, 0, 168, w, 1, border);
    fill_rect(&mut img, 0, h - 30, w, 30, wash);

    let work_y = 169u32;
    let work_h = h - 30 - work_y;
    fill_rect(&mut img, 0, work_y, 176, work_h, wash);
    fill_rect(&mut img, 176, work_y, 1, work_h, border);

    let rem_w = w - 177;
    let (v3d_w, sheet_x, sheet_w) = if sheet_full_tab_mode {
        (0, 177, rem_w)
    } else {
        let vw = ((rem_w as f32) * 0.27) as u32;
        (vw, 177 + vw, rem_w - vw)
    };

    if v3d_w > 0 {
        let v3d_state = crate::view3d_pane::View3DPaneState::default();
        let v3d_img = crate::view3d_pane::render_mesh_zbuffer_rgba(
            mesh,
            None,
            &v3d_state,
            v3d_w.min(340),
            work_h.min(540),
        );
        for y in 0..work_h {
            let sy = ((y as u64 * v3d_img.height() as u64) / work_h.max(1) as u64) as u32;
            for x in 0..v3d_w {
                let sx = ((x as u64 * v3d_img.width() as u64) / v3d_w.max(1) as u64) as u32;
                let px = *v3d_img.get_pixel(
                    sx.min(v3d_img.width().saturating_sub(1)),
                    sy.min(v3d_img.height().saturating_sub(1)),
                );
                img.put_pixel(177 + x, work_y + y, px);
            }
        }
        fill_rect(&mut img, 177 + v3d_w - 1, work_y, 1, work_h, border);
    }

    fill_rect(&mut img, sheet_x, work_y, sheet_w, 28, wash);
    fill_rect(&mut img, sheet_x, work_y + 28, sheet_w, 1, border);

    let cw = plate.canvas_width_mm.max(10.0);
    let ch = plate.canvas_height_mm.max(10.0);
    let p_scale = (((sheet_w - 48) as f64) / cw)
        .min(((work_h - 64) as f64) / ch)
        .max(0.5);
    let ox = (sheet_x as f64) + (sheet_w as f64) * 0.5 - (cw * p_scale) * 0.5;
    let oy = (work_y as f64) + 32.0 + (work_h as f64 - 40.0) * 0.5 + (ch * p_scale) * 0.5;

    for elem in &plate.paths {
        let col = if elem.stroke_hex == "#B5573A" {
            terra
        } else {
            ink
        };
        if elem.fill_hex.is_some() && elem.points_mm.len() >= 3 {
            let tris = triangulate_polygon_ear_clipping(&elem.points_mm);
            for tri in tris {
                let a = elem.points_mm[tri[0]];
                let b = elem.points_mm[tri[1]];
                let c = elem.points_mm[tri[2]];
                fill_tri(
                    &mut img,
                    [(ox + a[0] * p_scale) as f32, (oy - a[1] * p_scale) as f32],
                    [(ox + b[0] * p_scale) as f32, (oy - b[1] * p_scale) as f32],
                    [(ox + c[0] * p_scale) as f32, (oy - c[1] * p_scale) as f32],
                    col,
                );
            }
        }
        for seg in elem.points_mm.windows(2) {
            let x0 = (ox + seg[0][0] * p_scale) as i32;
            let y0 = (oy - seg[0][1] * p_scale) as i32;
            let x1 = (ox + seg[1][0] * p_scale) as i32;
            let y1 = (oy - seg[1][1] * p_scale) as i32;
            draw_line(&mut img, x0, y0, x1, y1, col);
        }
    }

    if with_dialog_overlay {
        let dw = 460u32;
        let dh = 280u32;
        let dx = (w - dw) / 2;
        let dy = (h - dh) / 2;
        fill_rect(&mut img, dx, dy, dw, dh, Rgba([250, 249, 245, 255]));
        fill_rect(&mut img, dx, dy, dw, 4, Rgba([168, 51, 51, 255]));
        fill_rect(&mut img, dx + 20, dy + 40, dw - 40, 60, wash);
        fill_rect(&mut img, dx + 20, dy + 115, dw - 40, 60, wash);
        fill_rect(&mut img, dx + 20, dy + 190, dw - 40, 60, wash);
    }

    img
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_corrugated_stamped_sherd_mesh() -> TriMesh {
        let mut positions = Vec::new();
        let mut triangles = Vec::new();
        let nx = 48usize;
        let nz = 48usize;
        for iz in 0..=nz {
            let z = (iz as f64 / nz as f64) * 60.0;
            for ix in 0..=nx {
                let x = (ix as f64 / nx as f64 - 0.5) * 60.0;
                // 물레속선 양각 능선 + 중앙 음각 인화문 홈(Groove)
                let wheel_ridge = (z * 0.9).sin() * 1.2;
                let dist_center = (x * x + (z - 30.0) * (z - 30.0)).sqrt();
                let stamped_groove = if (dist_center - 10.0).abs() < 2.2 {
                    -2.4
                } else {
                    0.0
                };
                let y = -(wheel_ridge + stamped_groove);
                positions.push([x, y, z]);
            }
        }
        let stride = (nx + 1) as u32;
        for iz in 0..(nz as u32) {
            for ix in 0..(nx as u32) {
                let a = iz * stride + ix;
                let b = a + 1;
                let c = a + stride;
                let d = c + 1;
                triangles.push([a, b, d]);
                triangles.push([a, d, c]);
            }
        }
        TriMesh::from_positions_and_triangles("stamped_sherd", positions, triangles).expect("mesh")
    }

    #[test]
    fn test_tdd5_full_bleed_archaeological_millimeter_graph_paper_grid() {
        let sheet_rect = egui::Rect::from_min_size(egui::pos2(400.0, 120.0), egui::vec2(1000.0, 680.0));
        let spec = compute_full_bleed_graph_paper_spec(
            sheet_rect,
            210.0,
            148.0,
            1.0,
            egui::Vec2::ZERO,
        );

        // 화면 전체가 여백 없이 100% 실제 방안지로 채워져야 함 (Full-bleed)
        assert_eq!(spec.paper_rect, sheet_rect);
        // 1mm 세선, 5mm 중선, 10mm(1cm) 주 격자선이 모두 존재해야 함
        assert!(spec.lines_1mm_x.len() >= 80, "must have 1mm fine subdivision lines");
        assert!(spec.lines_5mm_x.len() >= 15, "must have 5mm half-cm medium lines");
        assert!(spec.lines_10mm_x.len() >= 8, "must have 10mm (1cm) major lines");
        assert!(spec.cm_labels_top.len() >= 8, "must have cm ruler labels along top margin");
        assert!(spec.cm_labels_left.len() >= 5, "must have cm ruler labels along left margin");
    }

    #[test]
    fn test_tdd6_academic_msii_dog_sigmoid_digital_rubbing_contrast() {
        let mesh = make_corrugated_stamped_sherd_mesh();
        let params = AcademicRubbingParams::default();
        let img = render_orthographic_rubbing_rgba_with_params(
            &mesh,
            PlateRasterKind::FrontExteriorRubbing,
            0.0,
            &params,
            200,
            200,
        );

        let mut min_luma = 255u8;
        let mut max_luma = 0u8;
        let mut opaque_count = 0usize;
        for px in img.pixels() {
            if px[3] > 180 {
                opaque_count += 1;
                min_luma = min_luma.min(px[0]);
                max_luma = max_luma.max(px[0]);
            }
        }
        assert!(opaque_count > 10_000, "must render dense orthographic rubbing pixels");
        assert!(
            min_luma < 75,
            "convex ridges must receive deep charcoal ink (<75), got {min_luma}"
        );
        assert!(
            max_luma > 220,
            "concave stamped grooves must stay un-inked Hanji white (>220), got {max_luma}"
        );
    }

    #[test]
    fn test_tdd7_drag_roi_partial_rubbing_window_and_density_control() {
        let mut state = SheetPaneState::default();
        state.set_rubbing_roi_mm(Some([20.0, 15.0, 75.0, 60.0]));
        assert!(
            state.show_roi_rubbing_window,
            "dragging a rubbing ROI must automatically open the ROI Rubbing Studio window"
        );
        assert_eq!(state.rubbing_roi_mm, Some([20.0, 15.0, 75.0, 60.0]));

        let src = RgbaImage::from_pixel(100, 100, Rgba([30, 30, 28, 240]));
        let clipped = rgba_to_color_image_clipped_with_roi(
            &src,
            [0.0, 0.0, 100.0, 100.0],
            None,
            state.rubbing_roi_mm,
        );
        // ROI 바깥 픽셀(예: (5, 95))은 투명(0)이어야 하고, ROI 안쪽 픽셀(예: (45, 60))은 불투명해야 함
        let outside_px = clipped.pixels[95 * 100 + 5];
        let inside_px = clipped.pixels[60 * 100 + 45];
        assert_eq!(outside_px.a(), 0, "pixels outside dragged ROI must be transparent");
        assert!(inside_px.a() > 200, "pixels inside dragged ROI must show rubbing");
    }

    #[test]
    fn test_tdd10_chs_plate_title_block_and_dimension_spans() {
        let mesh = make_corrugated_stamped_sherd_mesh();
        let report = arch3d_measure::compute_artifact_measurements(
            &mesh,
            arch3d_core::ArtifactCategory::CompleteVessel,
        );
        let (left_cell, center_title, center_sub, right_cell) =
            build_chs_plate_title_block("7지구 9호-7 (완형 세트)", "완형 토기 표준 도법", &report);

        assert!(left_cell.contains("유적명"), "left cell must contain 유적명");
        assert!(center_title.contains("圖版"), "center title must contain 圖版");
        assert!(center_sub.contains("Plate"), "center subtitle must contain Plate");
        assert!(right_cell.contains("조사기관"), "right cell must contain 조사기관");
    }
}
