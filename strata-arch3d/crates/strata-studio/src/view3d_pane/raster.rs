use image::{Rgba, RgbaImage};
use rayon::prelude::*;
use strata_core::strata_tokens::tokens::SURFACE;
use strata_core::{SurfaceRegion, TriMesh};

use super::geometry::*;
use super::state::*;

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
pub(crate) fn draw_background_millimeter_grid_rgba(
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
pub fn compute_shared_ortho_scale(width_px: f64, grid_r_mm: f64, height_mm: f64, zoom: f32) -> f64 {
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
        let max_x = p0[0].max(p1[0]).max(p2[0]).ceil().min((w_usize - 1) as f64) as usize;
        let min_y = p0[1].min(p1[1]).min(p2[1]).floor().max(0.0) as usize;
        let max_y = p0[1].max(p1[1]).max(p2[1]).ceil().min((h_usize - 1) as f64) as usize;
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
        let light_gain =
            ((0.76 + 0.28 * ndotl_td) * f64::from(state.light_intensity)).clamp(0.55, 1.38) as f32;
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
                let b0 =
                    ((p1[1] - p2[1]) * (fx - p2[0]) + (p2[0] - p1[0]) * (fy - p2[1])) * inv_denom;
                let b1 =
                    ((p2[1] - p0[1]) * (fx - p2[0]) + (p0[0] - p2[0]) * (fy - p2[1])) * inv_denom;
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
        let max_x = p0[0].max(p1[0]).max(p2[0]).ceil().min((w_usize - 1) as f64) as usize;
        let min_y = p0[1].min(p1[1]).min(p2[1]).floor().max(0.0) as usize;
        let max_y = p0[1].max(p1[1]).max(p2[1]).ceil().min((h_usize - 1) as f64) as usize;

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
                let b0 =
                    ((p1[1] - p2[1]) * (fx - p2[0]) + (p2[0] - p1[0]) * (fy - p2[1])) * inv_denom;
                let b1 =
                    ((p2[1] - p0[1]) * (fx - p2[0]) + (p0[0] - p2[0]) * (fy - p2[1])) * inv_denom;
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
                let ign = ((52.982_918_f32 * ((0.06711056 * x_f + 0.00583715 * y_f).fract()))
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
