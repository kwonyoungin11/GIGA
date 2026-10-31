use arch3d_core::{Arch3dError, TriMesh};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// ASCII `.ply` 및 ASCII `.stl` 스캔 파일을 로드한다.
pub fn load_ply_or_stl_file(path: impl AsRef<Path>) -> Result<TriMesh, Arch3dError> {
    let path_ref = path.as_ref();
    let ext = path_ref
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "ply" {
        load_ascii_ply(path_ref)
    } else {
        load_ascii_stl(path_ref)
    }
}

fn load_ascii_ply(path: &Path) -> Result<TriMesh, Arch3dError> {
    let path_str = path.display().to_string();
    let file = File::open(path).map_err(|e| Arch3dError::Io {
        path: path_str.clone(),
        message: e.to_string(),
    })?;
    let reader = BufReader::new(file);

    let mut in_header = true;
    let mut num_verts = 0usize;
    let mut num_faces = 0usize;
    let mut positions = Vec::new();
    let mut triangles = Vec::new();

    for line_res in reader.lines() {
        let line = line_res.map_err(|e| Arch3dError::Io {
            path: path_str.clone(),
            message: e.to_string(),
        })?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("comment") {
            continue;
        }
        if in_header {
            if let Some(rest) = trimmed.strip_prefix("element vertex ") {
                num_verts = rest.trim().parse::<usize>().unwrap_or(0);
                positions.reserve(num_verts);
            } else if let Some(rest) = trimmed.strip_prefix("element face ") {
                num_faces = rest.trim().parse::<usize>().unwrap_or(0);
                triangles.reserve(num_faces);
            } else if trimmed == "end_header" {
                in_header = false;
            }
        } else if positions.len() < num_verts {
            let mut sp = trimmed.split_whitespace();
            if let (Some(xs), Some(ys), Some(zs)) = (sp.next(), sp.next(), sp.next()) {
                if let (Ok(x), Ok(y), Ok(z)) =
                    (xs.parse::<f64>(), ys.parse::<f64>(), zs.parse::<f64>())
                {
                    positions.push([x, y, z]);
                }
            }
        } else if triangles.len() < num_faces {
            let nums: Vec<u32> = trimmed
                .split_whitespace()
                .filter_map(|t| t.parse::<u32>().ok())
                .collect();
            if nums.len() >= 4 && nums[0] >= 3 {
                let v0 = nums[1];
                for i in 2..(nums.len() - 1) {
                    triangles.push([v0, nums[i], nums[i + 1]]);
                }
            }
        }
    }

    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("mesh.ply")
        .to_string();
    TriMesh::from_positions_and_triangles(name, positions, triangles)
}

fn load_ascii_stl(path: &Path) -> Result<TriMesh, Arch3dError> {
    let path_str = path.display().to_string();
    let file = File::open(path).map_err(|e| Arch3dError::Io {
        path: path_str.clone(),
        message: e.to_string(),
    })?;
    let reader = BufReader::new(file);
    let mut positions = Vec::new();
    let mut triangles = Vec::new();
    let mut cur_tri = Vec::with_capacity(3);

    for line_res in reader.lines() {
        let line = line_res.map_err(|e| Arch3dError::Io {
            path: path_str.clone(),
            message: e.to_string(),
        })?;
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("vertex ") {
            let mut sp = rest.split_whitespace();
            if let (Some(xs), Some(ys), Some(zs)) = (sp.next(), sp.next(), sp.next()) {
                if let (Ok(x), Ok(y), Ok(z)) =
                    (xs.parse::<f64>(), ys.parse::<f64>(), zs.parse::<f64>())
                {
                    let idx = positions.len() as u32;
                    positions.push([x, y, z]);
                    cur_tri.push(idx);
                    if cur_tri.len() == 3 {
                        triangles.push([cur_tri[0], cur_tri[1], cur_tri[2]]);
                        cur_tri.clear();
                    }
                }
            }
        }
    }

    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("mesh.stl")
        .to_string();
    TriMesh::from_positions_and_triangles(name, positions, triangles)
}
