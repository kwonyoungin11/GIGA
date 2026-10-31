use arch3d_core::{Arch3dError, TriMesh};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// `.obj` + `.mtl` 메타데이터 및 로드된 메쉬 결과.
#[derive(Debug, Clone)]
pub struct LoadedObjBundle {
    pub mesh: TriMesh,
    pub mtl_filename: Option<String>,
    pub diffuse_texture_filename: Option<String>,
}

/// 대용량 Artec Studio `.obj` 및 `.mtl` 파일을 스트리밍 파싱한다.
/// - 한글/공백 파일명(`7지구 9호-7.obj`), 정점 색상(`v x y z r g b`), 다각형 팬 삼각분할을 지원한다.
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
    let mut texcoords: Vec<[f32; 2]> = Vec::new();
    let mut triangles: Vec<[u32; 3]> = Vec::new();
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
            for tok in rest.split_whitespace() {
                let v_str = tok.split('/').next().unwrap_or("");
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
            }
            if face_verts.len() >= 3 {
                let v0 = face_verts[0];
                for i in 1..(face_verts.len() - 1) {
                    triangles.push([v0, face_verts[i], face_verts[i + 1]]);
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

    let mesh_name = path_ref
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unnamed.obj")
        .to_string();

    let mesh = TriMesh::build_complete(mesh_name, positions, triangles, colors_rgb, texcoords)?;
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
