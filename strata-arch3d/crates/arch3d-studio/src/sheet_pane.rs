use arch3d_core::strata_tokens::tokens::{ACTION, EDGE, GRID_10MM, GROUND, INK, WASH};
use arch3d_export::ComposedPlate2D;
use arch3d_measure::ArtifactMeasurementReport;
use eframe::egui;
use image::{Rgba, RgbaImage};

/// 우측 `73%` (또는 `Tab` 키 `100%`) 2D 실측 도판 작업대(`SheetPane`)를 렌더링한다.
pub fn render_sheet_pane_egui(
    ui: &mut egui::Ui,
    plate: &ComposedPlate2D,
    report: &ArtifactMeasurementReport,
    show_mm_grid: bool,
) {
    let avail = ui.available_size();
    let strip_h = 28.0;
    let canvas_h = (avail.y - strip_h - 4.0).max(120.0);

    let wash = egui::Color32::from_rgb(WASH.rgb[0], WASH.rgb[1], WASH.rgb[2]);
    let hanji = egui::Color32::from_rgb(GROUND.rgb[0], GROUND.rgb[1], GROUND.rgb[2]);
    let border = egui::Color32::from_rgb(EDGE.rgb[0], EDGE.rgb[1], EDGE.rgb[2]);
    let hairline = egui::Color32::from_rgb(GRID_10MM.rgb[0], GRID_10MM.rgb[1], GRID_10MM.rgb[2]);
    let ink = egui::Color32::from_rgb(INK.rgb[0], INK.rgb[1], INK.rgb[2]);
    let terra = egui::Color32::from_rgb(ACTION.rgb[0], ACTION.rgb[1], ACTION.rgb[2]);

    // 1. 상단 28px 정밀 치수 대장 띠 (Metric Ledger Strip)
    let (strip_rect, _) =
        ui.allocate_exact_size(egui::vec2(avail.x, strip_h), egui::Sense::hover());
    let strip_painter = ui.painter_at(strip_rect);
    strip_painter.rect_filled(strip_rect, 2.0, wash);
    strip_painter.rect_stroke(
        strip_rect,
        2.0,
        egui::Stroke::new(1.0_f32, border),
        egui::StrokeKind::Inside,
    );
    strip_painter.text(
        egui::pos2(strip_rect.left() + 10.0, strip_rect.center().y),
        egui::Align2::LEFT_CENTER,
        format!(
            "[{}]  {}",
            plate.mode.label_kr(),
            report.metric_strip_summary
        ),
        egui::FontId::monospace(11.5),
        ink,
    );

    ui.add_space(4.0);

    // 2. 한지(#FAF9F5) 2D 도판 캔버스 (1mm/10mm 모눈 + 8대 유물군 도판 렌더링)
    let (sheet_rect, _) =
        ui.allocate_exact_size(egui::vec2(avail.x, canvas_h), egui::Sense::hover());
    let painter = ui.painter_at(sheet_rect);
    painter.rect_filled(sheet_rect, 2.0, hanji);
    painter.rect_stroke(
        sheet_rect,
        2.0,
        egui::Stroke::new(1.2_f32, border),
        egui::StrokeKind::Inside,
    );

    let cw = plate.canvas_width_mm.max(10.0) as f32;
    let ch = plate.canvas_height_mm.max(10.0) as f32;
    let scale = ((sheet_rect.width() - 32.0) / cw)
        .min((sheet_rect.height() - 32.0) / ch)
        .max(0.5);

    let origin_x = sheet_rect.center().x - (cw * scale) * 0.5;
    let origin_y = sheet_rect.center().y + (ch * scale) * 0.5;

    if show_mm_grid {
        let step_10mm = 10.0 * scale;
        if step_10mm >= 6.0 {
            let mut gx = origin_x;
            while gx <= origin_x + cw * scale {
                painter.line_segment(
                    [
                        egui::pos2(gx, origin_y - ch * scale),
                        egui::pos2(gx, origin_y),
                    ],
                    egui::Stroke::new(0.5_f32, hairline),
                );
                gx += step_10mm;
            }
            let mut gy = origin_y;
            while gy >= origin_y - ch * scale {
                painter.line_segment(
                    [
                        egui::pos2(origin_x, gy),
                        egui::pos2(origin_x + cw * scale, gy),
                    ],
                    egui::Stroke::new(0.5_f32, hairline),
                );
                gy -= step_10mm;
            }
        }
    }

    for elem in &plate.paths {
        if elem.points_mm.len() < 2 {
            continue;
        }
        let stroke_col = if elem.stroke_hex == "#B5573A" {
            terra
        } else {
            ink
        };
        let pts: Vec<egui::Pos2> = elem
            .points_mm
            .iter()
            .map(|p| {
                egui::pos2(
                    origin_x + (p[0] as f32) * scale,
                    origin_y - (p[1] as f32) * scale,
                )
            })
            .collect();
        let w_px = ((elem.stroke_width_mm as f32) * scale).clamp(1.0, 3.5);
        painter.add(egui::Shape::line(pts, egui::Stroke::new(w_px, stroke_col)));
    }

    for c in &plate.callouts {
        let pos = egui::pos2(
            origin_x + (c.pos_mm[0] as f32) * scale,
            (origin_y - (c.pos_mm[1] as f32) * scale)
                .clamp(sheet_rect.top() + 8.0, sheet_rect.bottom() - 18.0),
        );
        let col = if c.color_hex == "#B5573A" { terra } else { ink };
        painter.text(
            pos,
            egui::Align2::LEFT_TOP,
            &c.text,
            egui::FontId::proportional(11.5),
            col,
        );
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
    let mut img = RgbaImage::from_pixel(w, h, Rgba([GROUND.rgb[0], GROUND.rgb[1], GROUND.rgb[2], 255]));

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
        fill_rect(&mut img, 177, work_y, v3d_w, work_h, Rgba([242, 240, 232, 255]));
        fill_rect(&mut img, 177 + v3d_w - 1, work_y, 1, work_h, border);
        let vcx = 177.0 + (v3d_w as f64) * 0.5;
        let vcy = (work_y as f64) + (work_h as f64) * 0.48;
        let ext = mesh.bbox.extents();
        let max_s = ext[0].max(ext[1]).max(ext[2]).max(1.0);
        let sc = ((v3d_w.min(work_h) as f64) * 0.65) / max_s;
        let mcx = (mesh.bbox.min[0] + mesh.bbox.max[0]) * 0.5;
        let mcz = (mesh.bbox.min[2] + mesh.bbox.max[2]) * 0.5;
        let stride = (mesh.triangles.len() / 1200).max(1);
        for tri in mesh.triangles.iter().step_by(stride) {
            let p0 = mesh.positions[tri[0] as usize];
            let p1 = mesh.positions[tri[1] as usize];
            let x0 = (vcx + (p0[0] - mcx) * sc) as i32;
            let y0 = (vcy - (p0[2] - mcz) * sc) as i32;
            let x1 = (vcx + (p1[0] - mcx) * sc) as i32;
            let y1 = (vcy - (p1[2] - mcz) * sc) as i32;
            draw_line(&mut img, x0, y0, x1, y1, ink);
        }
        draw_line(
            &mut img,
            vcx as i32,
            (work_y + 24) as i32,
            vcx as i32,
            (work_y + work_h - 40) as i32,
            terra,
        );
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
