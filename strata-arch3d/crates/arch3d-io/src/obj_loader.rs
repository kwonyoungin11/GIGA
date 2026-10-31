use arch3d_core::{Arch3dError, MeshTextureRgba, TriMesh};
use image::ImageReader;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::Arc;

/// `.obj` + `.mtl` 메타데이터 및 로드된 메쉬 결과.
#[derive(Debug, Clone)]
pub struct LoadedObjBundle {
    pub mesh: TriMesh,
    pub mtl_filename: Option<String>,
    pub diffuse_texture_filename: Option<String>,
}

/// 대용량 Artec Studio `.obj` 및 `.mtl` 파일을 스트리밍 파싱한다.
/// - 한글/공백 파일명(`7지구 9호-7.obj`), 정점 색상(`v x y z r g b`), UV 좌표(`vt u v` 및 `f v/vt/vn`),
///   `.mtl` `map_Kd` 실사 텍스처(`.jpg`/`.png`) 디코딩 및 정점 색상 샘플링을 지원한다.
pub fn load_obj_file(path: impl AsRef<Path>) -> Result<LoadedObjBundle, Arch3dError> {
    let path_ref = path.as_ref();
    let path_str = path_ref.display().to_string();
    let file = File::open(path_ref).map_err(|e| Arch3dError::Io {
        path: path_str.clone(),
        message: e.to_string(),
    })?;

    let reader = BufReader::with_capacity(256 * 1024, file);
    let mut positions: Vec<[f64; 3]> = Vec::new();
    let mut colors_rgb: Vec<[u8; 3]> = Vec::new();
    let mut has_inline_vertex_colors = false;
    let mut texcoords: Vec<[f32; 2]> = Vec::new();
    let mut triangles: Vec<[u32; 3]> = Vec::new();
    let mut triangle_vt_indices: Vec<[Option<u32>; 3]> = Vec::new();
    let mut mtl_filename: Option<String> = None;

    for line_res in reader.lines() {
        let line = line_res.map_err(|e| Arch3dError::Io {
            path: path_str.clone(),
            message: e.to_string(),
        })?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("v ") {
            let mut parts = rest.split_whitespace();
            let (Some(xs), Some(ys), Some(zs)) = (parts.next(), parts.next(), parts.next()) else {
                continue;
            };
            let x = xs.parse::<f64>().map_err(|e| Arch3dError::MeshParse {
                path: path_str.clone(),
                reason: format!("v X 파싱 실패: {e}"),
            })?;
            let y = ys.parse::<f64>().map_err(|e| Arch3dError::MeshParse {
                path: path_str.clone(),
                reason: format!("v Y 파싱 실패: {e}"),
            })?;
            let z = zs.parse::<f64>().map_err(|e| Arch3dError::MeshParse {
                path: path_str.clone(),
                reason: format!("v Z 파싱 실패: {e}"),
            })?;
            positions.push([x, y, z]);

            if let (Some(rs), Some(gs), Some(bs)) = (parts.next(), parts.next(), parts.next()) {
                if let (Ok(r), Ok(g), Ok(b)) =
                    (rs.parse::<f64>(), gs.parse::<f64>(), bs.parse::<f64>())
                {
                    let to_u8 = |v: f64| -> u8 {
                        if v <= 1.0 {
                            (v.clamp(0.0, 1.0) * 255.0).round() as u8
                        } else {
                            v.clamp(0.0, 255.0).round() as u8
                        }
                    };
                    colors_rgb.push([to_u8(r), to_u8(g), to_u8(b)]);
                    has_inline_vertex_colors = true;
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("vt ") {
            let mut parts = rest.split_whitespace();
            if let (Some(us), Some(vs)) = (parts.next(), parts.next()) {
                if let (Ok(u), Ok(v)) = (us.parse::<f32>(), vs.parse::<f32>()) {
                    texcoords.push([u, v]);
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("f ") {
            let mut face_verts: Vec<u32> = Vec::with_capacity(4);
            let mut face_vts: Vec<Option<u32>> = Vec::with_capacity(4);
            for tok in rest.split_whitespace() {
                let mut slash_iter = tok.split('/');
                let v_str = slash_iter.next().unwrap_or("");
                if v_str.is_empty() {
                    continue;
                }
                let idx_raw = v_str.parse::<i64>().map_err(|e| Arch3dError::MeshParse {
                    path: path_str.clone(),
                    reason: format!("f 인덱스 파싱 실패 ({tok}): {e}"),
                })?;
                let v_idx = if idx_raw > 0 {
                    (idx_raw - 1) as u32
                } else {
                    (positions.len() as i64 + idx_raw).max(0) as u32
                };
                face_verts.push(v_idx);

                let vt_opt = slash_iter
                    .next()
                    .and_then(|s| if s.is_empty() { None } else { s.parse::<i64>().ok() })
                    .map(|vt_raw| {
                        if vt_raw > 0 {
                            (vt_raw - 1) as u32
                        } else {
                            (texcoords.len() as i64 + vt_raw).max(0) as u32
                        }
                    });
                face_vts.push(vt_opt);
            }
            if face_verts.len() >= 3 {
                let v0 = face_verts[0];
                let vt0 = face_vts[0];
                for i in 1..(face_verts.len() - 1) {
                    triangles.push([v0, face_verts[i], face_verts[i + 1]]);
                    triangle_vt_indices.push([vt0, face_vts[i], face_vts[i + 1]]);
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("mtllib ") {
            let mtl = rest.trim();
            if !mtl.is_empty() {
                mtl_filename = Some(mtl.to_string());
            }
        }
    }

    let diffuse_texture_filename = if let (Some(parent), Some(mtl_name)) =
        (path_ref.parent(), mtl_filename.as_deref())
    {
        let mtl_path = parent.join(mtl_name);
        parse_mtl_diffuse_texture(&mtl_path)
    } else {
        None
    };

    // 삼각형별 UV 좌표 배열(`triangle_uvs`) 구축
    let mut triangle_uvs: Vec<[[f32; 2]; 3]> = Vec::new();
    if !texcoords.is_empty() && triangle_vt_indices.len() == triangles.len() {
        triangle_uvs.reserve(triangles.len());
        for vt_tri in &triangle_vt_indices {
            let uv0 = vt_tri[0]
                .and_then(|idx| texcoords.get(idx as usize).copied())
                .unwrap_or([0.0, 0.0]);
            let uv1 = vt_tri[1]
                .and_then(|idx| texcoords.get(idx as usize).copied())
                .unwrap_or([0.0, 0.0]);
            let uv2 = vt_tri[2]
                .and_then(|idx| texcoords.get(idx as usize).copied())
                .unwrap_or([0.0, 0.0]);
            triangle_uvs.push([uv0, uv1, uv2]);
        }
    }

    // 동반 `.jpg`/`.png` 텍스처 이미지 디코딩 및 정점 색상 샘플링
    let texture_image = if let (Some(parent), Some(tex_name)) =
        (path_ref.parent(), diffuse_texture_filename.as_deref())
    {
        let tex_path = parent.join(tex_name);
        load_and_downsample_texture(&tex_path, 1024).map(Arc::new)
    } else {
        None
    };

    if !has_inline_vertex_colors {
        if let Some(ref tex) = texture_image {
            if triangle_uvs.len() == triangles.len() {
                colors_rgb = vec![[182, 176, 164]; positions.len()];
                for (tri, uvs) in triangles.iter().zip(triangle_uvs.iter()) {
                    for k in 0..3 {
                        let vi = tri[k] as usize;
                        if vi < colors_rgb.len() {
                            let sampled = tex.sample_uv_rgb(uvs[k][0], uvs[k][1]);
                            // 검정 여백(아틀라스 배경 [0,0,0])이 아닌 경우 정점 색상에 반영
                            if sampled[0] > 8 || sampled[1] > 8 || sampled[2] > 8 {
                                colors_rgb[vi] = sampled;
                            }
                        }
                    }
                }
            }
        }
    }

    let mesh_name = path_ref
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unnamed.obj")
        .to_string();

    let mut mesh =
        TriMesh::build_complete(mesh_name, positions, triangles, colors_rgb, texcoords)?;
    mesh.triangle_uvs = triangle_uvs;
    mesh.texture_image = texture_image;
    mesh.source_dir = path_ref.parent().map(Path::to_path_buf);

    Ok(LoadedObjBundle {
        mesh,
        mtl_filename,
        diffuse_texture_filename,
    })
}

fn parse_mtl_diffuse_texture(mtl_path: &Path) -> Option<String> {
    let file = File::open(mtl_path).ok()?;
    let reader = BufReader::new(file);
    for line in reader.lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("map_Kd ") {
            let tex = rest.trim();
            if !tex.is_empty() {
                return Some(tex.to_string());
            }
        }
    }
    None
}

/// 대용량 `.jpg`/`.png` 텍스처 파일을 메모리 제한 없이 디코딩하고 최대 `max_dim` 해상도로 고속 서브샘플링한다.
fn load_and_downsample_texture(tex_path: &Path, max_dim: u32) -> Option<MeshTextureRgba> {
    if !tex_path.exists() {
        return None;
    }
    let mut reader = ImageReader::open(tex_path).ok()?.with_guessed_format().ok()?;
    reader.no_limits();
    let img = reader.decode().ok()?.into_rgba8();
    let (src_w, src_h) = img.dimensions();
    if src_w == 0 || src_h == 0 {
        return None;
    }
    if src_w <= max_dim && src_h <= max_dim {
        return Some(MeshTextureRgba {
            width: src_w,
            height: src_h,
            rgba: img.into_raw(),
        });
    }

    let scale = (max_dim as f32 / src_w.max(src_h) as f32).min(1.0);
    let dst_w = ((src_w as f32 * scale).round() as u32).max(1);
    let dst_h = ((src_h as f32 * scale).round() as u32).max(1);
    let src_raw = img.as_raw();
    let mut dst_rgba = vec![0u8; (dst_w * dst_h * 4) as usize];

    for dy in 0..dst_h {
        let sy = ((dy as u64 * src_h as u64) / dst_h as u64) as u32;
        for dx in 0..dst_w {
            let sx = ((dx as u64 * src_w as u64) / dst_w as u64) as u32;
            let s_idx = ((sy * src_w + sx) * 4) as usize;
            let d_idx = ((dy * dst_w + dx) * 4) as usize;
            dst_rgba[d_idx..d_idx + 4].copy_from_slice(&src_raw[s_idx..s_idx + 4]);
        }
    }

    Some(MeshTextureRgba {
        width: dst_w,
        height: dst_h,
        rgba: dst_rgba,
    })
}
