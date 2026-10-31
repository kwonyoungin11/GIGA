use arch3d_core::strata_tokens::tokens::{
    ACTION, DESK, DONE_BG, DONE_INK, EDGE, GRID_10MM, GRID_1MM, GROUND, INK, LINE, MUTED,
    PROFILE_RED, SELECT_BLUE, SURFACE, WASH,
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

/// 우측 2D 실측 도판 뷰포트(`SheetPane`)의 줌·팬 및 정밀 탁본(`Rubbing`) 래스터 캐시 상태.
#[derive(Clone)]
pub struct SheetPaneState {
    pub zoom: f32,
    pub pan_px: egui::Vec2,
    cached_key: Option<(String, i32, i32)>,
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
            .field("cached_key", &self.cached_key)
            .finish()
    }
}

impl Default for SheetPaneState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan_px: egui::Vec2::ZERO,
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

/// 3D 메쉬로부터 도판 합성용 직교투영(Orthographic) 정밀 탁본(`Rubbing`) RGBA 영상을 생성한다.
/// - 실루엣 바깥 영역은 `alpha = 0`으로 투명 처리하여 도판 용지(`#FFFFFF`) 및 방안 눈금과 자연스럽게 합성된다.
/// - `3×3` 깊이·법선 라플라시안(Laplacian) 음각/양각 곡률 강조 + 2단 해시 한지 섬유 질감을 결합한다.
pub fn render_orthographic_rubbing_rgba(
    mesh: &TriMesh,
    kind: PlateRasterKind,
    psi_deg: f64,
    ink_density_pct: f64,
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

    let has_uv_tex = mesh.texture_image.is_some()
        && mesh.triangle_uvs.len() == mesh.triangles.len();

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

    // 2차 패스: 3×3 깊이·법선 라플라시안(Laplacian) 음각/양각 곡률 강조 + 한지 섬유 질감 합성
    let ink_strength = ((ink_density_pct as f32) / 100.0).clamp(0.18, 1.0);
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            if !hit_buf[idx] {
                continue;
            }

            // 실루엣 경계 여부 확인
            let xl = x.saturating_sub(1);
            let xr = (x + 1).min(w - 1);
            let yu = y.saturating_sub(1);
            let yd = (y + 1).min(h - 1);

            let is_boundary = !hit_buf[y * w + xl]
                || !hit_buf[y * w + xr]
                || !hit_buf[yu * w + x]
                || !hit_buf[yd * w + x];

            if is_boundary {
                img.put_pixel(x as u32, y as u32, Rgba([28, 28, 26, 235]));
                continue;
            }

            let n_c = nbuf[idx];
            let n_l = nbuf[y * w + xl];
            let n_r = nbuf[y * w + xr];
            let n_u = nbuf[yu * w + x];
            let n_d = nbuf[yd * w + x];

            // 법선 변화율(곡률) 및 5×5 국소 깊이 하이패스(High-pass) 계산
            let dn_x = (n_r[0] - n_l[0]) * 0.5;
            let dn_z = (n_u[2] - n_d[2]) * 0.5;
            let mean_curv = (dn_x + dn_z).clamp(-0.85, 0.85);

            let z_c = zbuf[idx];
            let mut z_sum = 0.0f32;
            let mut z_cnt = 0.0f32;
            for dy in -2i32..=2 {
                let ny_i = (y as i32 + dy).clamp(0, (h - 1) as i32) as usize;
                for dx in -2i32..=2 {
                    let nx_i = (x as i32 + dx).clamp(0, (w - 1) as i32) as usize;
                    let n_idx = ny_i * w + nx_i;
                    if hit_buf[n_idx] {
                        z_sum += zbuf[n_idx];
                        z_cnt += 1.0;
                    }
                }
            }
            let z_mean = if z_cnt > 0.0 { z_sum / z_cnt } else { z_c };
            let z_highpass = ((z_c - z_mean) * 1.35).clamp(-1.0, 1.0);

            // 카메라를 향하는 법선 성분 + 사광(Oblique raking light) + 물레속선 수평 리지 강조
            let view_facing = n_c[1].abs().clamp(0.08, 1.0);
            let raking = (-0.48 * n_c[0] + 0.62 * n_c[1].abs() + 0.62 * n_c[2]).clamp(0.05, 1.0);
            let wheel_band = ((y as f32 * 0.55 + n_c[2] * 9.0).sin() * 0.08).clamp(-0.08, 0.08);

            // 표면 미세 명도 편차 (인화문·타날문 음각 홈은 어두운 텍스처/음수 z_highpass -> 먹물이 안 묻어 한지 백색 유지)
            let tex_mod = (luma_buf[idx] - 0.52).clamp(-0.35, 0.35) * 0.55;
            let relief =
                (mean_curv * 0.65 + z_highpass * 0.72 + tex_mod + wheel_band).clamp(-0.95, 0.95);

            let grain = sample_hanji_paper_grain(x as u32, y as u32, 0.16);

            // 양각 능선일수록 먹물 농도가 진해지고, 음각 홈(relief < -0.06)은 한지 여백(#FAF9F5)으로 남음
            let ink_deposit = if relief < -0.06 {
                ((0.14 + relief * 0.45).max(0.02)) * ink_strength * 0.28
            } else {
                let ridge_boost =
                    (0.42 + relief * 0.78 + (1.0 - raking) * 0.34 + view_facing * 0.16)
                        .clamp(0.12, 0.96);
                (ridge_boost * ink_strength * (0.84 + 0.28 * grain)).clamp(0.06, 0.95)
            };

            // 한지 바탕(#FAF9F5: 250, 249, 245)에서 먹물(#141413: 20, 20, 19)로 보간
            let r = (250.0 - ink_deposit * (250.0 - 20.0)).round().clamp(18.0, 252.0) as u8;
            let g = (249.0 - ink_deposit * (249.0 - 20.0)).round().clamp(18.0, 251.0) as u8;
            let b = (245.0 - ink_deposit * (245.0 - 19.0)).round().clamp(17.0, 248.0) as u8;

            img.put_pixel(x as u32, y as u32, Rgba([r, g, b, 244]));
        }
    }

    img
}

fn rgba_to_color_image_clipped(
    src: &RgbaImage,
    clip_half_sign: Option<i8>,
) -> egui::ColorImage {
    let w = src.width() as usize;
    let h = src.height() as usize;
    let mid_x = w / 2;
    let mut pixels = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let clipped_out = match clip_half_sign {
                Some(s) if s < 0 => x > mid_x,
                Some(s) if s > 0 => x < mid_x,
                _ => false,
            };
            if clipped_out {
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
/// 반드시 수직(`top_down`) 레이아웃 안에서 도판 머리줄(`32px`) → 치수 대장 띠(`28px`) → 실측 도판 용지(남은 높이 100%) 순으로 쌓는다.
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
    let desk = egui::Color32::from_rgb(DESK.rgb[0], DESK.rgb[1], DESK.rgb[2]);
    let paper = egui::Color32::from_rgb(SURFACE.rgb[0], SURFACE.rgb[1], SURFACE.rgb[2]);
    let border = egui::Color32::from_rgb(EDGE.rgb[0], EDGE.rgb[1], EDGE.rgb[2]);
    let line_col = egui::Color32::from_rgb(LINE.rgb[0], LINE.rgb[1], LINE.rgb[2]);
    let grid_10 = egui::Color32::from_rgb(GRID_10MM.rgb[0], GRID_10MM.rgb[1], GRID_10MM.rgb[2]);
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

    // 정밀 탁본 래스터 캐시 갱신 (유물 이름, 회전각 ψ, 먹물 농도 변경 시에만 재합성)
    let key = (
        mesh.name.clone(),
        psi_deg.round() as i32,
        ink_density_pct.round() as i32,
    );
    if state.cached_key.as_ref() != Some(&key) {
        let front_img = render_orthographic_rubbing_rgba(
            mesh,
            PlateRasterKind::FrontExteriorRubbing,
            psi_deg,
            ink_density_pct,
            240,
            240,
        );
        let back_img = render_orthographic_rubbing_rgba(
            mesh,
            PlateRasterKind::BackInteriorRubbing,
            psi_deg,
            ink_density_pct,
            220,
            220,
        );
        let bottom_img = render_orthographic_rubbing_rgba(
            mesh,
            PlateRasterKind::BottomPlanRubbing,
            psi_deg,
            ink_density_pct,
            200,
            200,
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

        // 1. 도판 머리줄 (32px, #FAF9F5 Ground)
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
            egui::FontId::proportional(13.5),
            ink,
        );

        // 우측 정위치·스케일 상태 배지 (22px)
        let badge_w = 224.0;
        let badge_rect = egui::Rect::from_min_size(
            egui::pos2(header_rect.right() - badge_w - 10.0, header_rect.center().y - 11.0),
            egui::vec2(badge_w, 22.0),
        );
        hp.rect_filled(badge_rect, 4.0, done_bg);
        hp.text(
            badge_rect.center(),
            egui::Align2::CENTER_CENTER,
            format!(
                "● 정위치 · 축척 {} · 배율 {:.0}%",
                report.recommended_scale.label(),
                state.zoom * 100.0
            ),
            egui::FontId::proportional(11.5),
            done_fg,
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

        // 3. 책상 배경(#F0EEE6) 위 흰 면(#FFFFFF Surface) 실측 도판 용지 + 방안 눈금
        let (sheet_rect, sheet_resp) = ui.allocate_exact_size(
            egui::vec2(avail.x, canvas_h),
            egui::Sense::click_and_drag(),
        );
        out_sheet_rect = sheet_rect;

        // 마우스 휠 줌(0.45x ~ 4.0x), 드래그 팬, 더블클릭 원복 인터랙션
        if sheet_resp.hovered() {
            let scroll_y = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll_y.abs() > 0.1 {
                let factor = (1.0 + scroll_y * 0.0025).clamp(0.85, 1.18);
                state.zoom = (state.zoom * factor).clamp(0.45, 4.0);
            }
        }
        if sheet_resp.dragged() {
            state.pan_px += sheet_resp.drag_delta();
        }
        if sheet_resp.double_clicked() {
            state.zoom = 1.0;
            state.pan_px = egui::Vec2::ZERO;
        }

        let painter = ui.painter_at(sheet_rect);
        painter.rect_filled(sheet_rect, 3.0, desk);
        painter.rect_stroke(
            sheet_rect,
            3.0,
            egui::Stroke::new(1.0_f32, border),
            egui::StrokeKind::Inside,
        );

        let cw = plate.canvas_width_mm.max(10.0) as f32;
        let ch = plate.canvas_height_mm.max(10.0) as f32;
        let base_scale = ((sheet_rect.width() - 36.0) / cw)
            .min((sheet_rect.height() - 36.0) / ch)
            .max(0.5);
        let scale = base_scale * state.zoom;

        let paper_w = cw * scale;
        let paper_h = ch * scale;
        let paper_rect = egui::Rect::from_center_size(
            sheet_rect.center() + state.pan_px,
            egui::vec2(paper_w, paper_h),
        );

        // 용지 그림자 & 흰색(#FFFFFF) 용지 면
        let shadow_rect = paper_rect.translate(egui::vec2(3.0, 4.0));
        painter.rect_filled(
            shadow_rect,
            2.0,
            egui::Color32::from_rgba_unmultiplied(20, 20, 19, 22),
        );
        painter.rect_filled(paper_rect, 1.0, paper);
        painter.rect_stroke(
            paper_rect,
            1.0,
            egui::Stroke::new(1.2_f32, border),
            egui::StrokeKind::Inside,
        );

        let paper_clip = paper_rect.intersect(sheet_rect);
        let paper_painter = ui.painter_at(paper_clip);

        let origin_x = paper_rect.left();
        let origin_y = paper_rect.bottom();

        // 3-A. 방안 눈금 (1mm = #F2F0E8, 10mm = #E4E2D8)
        if show_mm_grid {
            let step_1mm = scale;
            if step_1mm >= 3.2 {
                let mut gx = origin_x;
                while gx <= paper_rect.right() {
                    paper_painter.line_segment(
                        [egui::pos2(gx, paper_rect.top()), egui::pos2(gx, paper_rect.bottom())],
                        egui::Stroke::new(0.5_f32, grid_1),
                    );
                    gx += step_1mm;
                }
                let mut gy = origin_y;
                while gy >= paper_rect.top() {
                    paper_painter.line_segment(
                        [egui::pos2(paper_rect.left(), gy), egui::pos2(paper_rect.right(), gy)],
                        egui::Stroke::new(0.5_f32, grid_1),
                    );
                    gy -= step_1mm;
                }
            }

            let step_10mm = 10.0 * scale;
            if step_10mm >= 6.0 {
                let mut gx = origin_x;
                while gx <= paper_rect.right() + 0.5 {
                    paper_painter.line_segment(
                        [egui::pos2(gx, paper_rect.top()), egui::pos2(gx, paper_rect.bottom())],
                        egui::Stroke::new(0.85_f32, grid_10),
                    );
                    gx += step_10mm;
                }
                let mut gy = origin_y;
                while gy >= paper_rect.top() - 0.5 {
                    paper_painter.line_segment(
                        [egui::pos2(paper_rect.left(), gy), egui::pos2(paper_rect.right(), gy)],
                        egui::Stroke::new(0.85_f32, grid_10),
                    );
                    gy -= step_10mm;
                }
            }
        }

        // 3-B. 다중 뷰 정밀 탁본(`Rubbing`) 래스터 영역 합성 (외면·내면·외저면)
        for (ri, region) in plate.raster_regions.iter().enumerate() {
            let rx0 = origin_x + (region.rect_mm[0] as f32) * scale;
            let ry1 = origin_y - (region.rect_mm[1] as f32) * scale;
            let rx1 = origin_x + (region.rect_mm[2] as f32) * scale;
            let ry0 = origin_y - (region.rect_mm[3] as f32) * scale;
            let dst_rect = egui::Rect::from_min_max(
                egui::pos2(rx0.min(rx1), ry0.min(ry1)),
                egui::pos2(rx0.max(rx1), ry0.max(ry1)),
            );

            let src_opt = match region.kind {
                PlateRasterKind::FrontExteriorRubbing => state.cached_front_img.as_ref(),
                PlateRasterKind::BackInteriorRubbing => state.cached_back_img.as_ref(),
                PlateRasterKind::BottomPlanRubbing => state.cached_bottom_img.as_ref(),
            };

            if let Some(src_img) = src_opt {
                let color_img = rgba_to_color_image_clipped(src_img, region.clip_half_sign);
                let tex = ui.ctx().load_texture(
                    format!("sheet_rubbing_region_{ri}_{:?}", region.kind),
                    color_img,
                    egui::TextureOptions::LINEAR,
                );
                paper_painter.image(
                    tex.id(),
                    dst_rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
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

        // 3-D. 텍스트 주석 및 치수 콜아웃
        for c in &plate.callouts {
            let pos = egui::pos2(
                (origin_x + (c.pos_mm[0] as f32) * scale)
                    .clamp(paper_rect.left() + 6.0, paper_rect.right() - 150.0),
                (origin_y - (c.pos_mm[1] as f32) * scale)
                    .clamp(paper_rect.top() + 6.0, paper_rect.bottom() - 18.0),
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
