use super::rubbing::*;
use eframe::egui;
use image::{Rgba, RgbaImage};
use strata_core::strata_tokens::tokens::GROUND;
use strata_export::{triangulate_polygon_ear_clipping, ComposedPlate2D};

pub(crate) fn draw_dashed_polyline(
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
    mesh: &strata_core::TriMesh,
    sheet_full_tab_mode: bool,
    with_dialog_overlay: bool,
) -> RgbaImage {
    let w = 1440u32;
    let h = 900u32;
    let mut img = RgbaImage::from_pixel(
        w,
        h,
        Rgba([GROUND.rgb[0], GROUND.rgb[1], GROUND.rgb[2], 255]),
    );

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

    let fill_tri =
        |img: &mut RgbaImage, p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], col: Rgba<u8>| {
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
                    let w0 =
                        ((p1[1] - p2[1]) * (fx - p2[0]) + (p2[0] - p1[0]) * (fy - p2[1])) * inv;
                    let w1 =
                        ((p2[1] - p0[1]) * (fx - p2[0]) + (p0[0] - p2[0]) * (fy - p2[1])) * inv;
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
            let x0 = (ox + seg[0][0] * p_scale).round() as i32;
            let y0 = (oy - seg[0][1] * p_scale).round() as i32;
            let x1 = (ox + seg[1][0] * p_scale).round() as i32;
            let y1 = (oy - seg[1][1] * p_scale).round() as i32;
            draw_line(&mut img, x0, y0, x1, y1, col);
        }
    }

    if let Some(first_reg) = plate.raster_regions.first() {
        let rub_img = render_orthographic_rubbing_rgba(mesh, first_reg.kind, 0.0, 76.0, 180, 180);
        let rx0 = (ox + first_reg.rect_mm[0].min(first_reg.rect_mm[2]) * p_scale).round() as i32;
        let rx1 = (ox + first_reg.rect_mm[0].max(first_reg.rect_mm[2]) * p_scale).round() as i32;
        let ry0 = (oy - first_reg.rect_mm[1].max(first_reg.rect_mm[3]) * p_scale).round() as i32;
        let ry1 = (oy - first_reg.rect_mm[1].min(first_reg.rect_mm[3]) * p_scale).round() as i32;
        let rw = (rx1 - rx0).max(1);
        let rh = (ry1 - ry0).max(1);
        let mid_x = rx0 + rw / 2;
        for py in ry0.max(work_y as i32 + 30)..ry1.min((h - 32) as i32) {
            let v = ((py - ry0) as f32 / rh as f32).clamp(0.0, 0.999);
            let sy = (v * rub_img.height() as f32) as u32;
            for px in rx0.max(sheet_x as i32 + 4)..rx1.min((w - 4) as i32) {
                if first_reg.clip_half_sign == Some(-1) && px > mid_x {
                    continue;
                }
                if first_reg.clip_half_sign == Some(1) && px < mid_x {
                    continue;
                }
                let u = ((px - rx0) as f32 / rw as f32).clamp(0.0, 0.999);
                let sx = (u * rub_img.width() as f32) as u32;
                let rp = rub_img.get_pixel(sx, sy);
                if rp[3] > 32 {
                    img.put_pixel(px as u32, py as u32, Rgba([rp[0], rp[1], rp[2], 255]));
                }
            }
        }
    }

    if with_dialog_overlay {
        for y in 0..h {
            for x in 0..w {
                let p = img.get_pixel(x, y).0;
                img.put_pixel(
                    x,
                    y,
                    Rgba([
                        ((u16::from(p[0]) * 180) / 255) as u8,
                        ((u16::from(p[1]) * 180) / 255) as u8,
                        ((u16::from(p[2]) * 180) / 255) as u8,
                        255,
                    ]),
                );
            }
        }
        let mw = 640u32;
        let mh = 420u32;
        let mx = (w - mw) / 2;
        let my = (h - mh) / 2;
        fill_rect(&mut img, mx, my, mw, mh, Rgba([255, 255, 255, 255]));
        fill_rect(&mut img, mx, my, mw, 38, wash);
        fill_rect(&mut img, mx, my + 38, mw, 2, terra);
    }

    img
}
