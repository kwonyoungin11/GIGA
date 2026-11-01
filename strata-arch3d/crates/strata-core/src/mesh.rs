use crate::error::StrataError;
use crate::types::{Aabb3, Mat4};
use serde::{Deserialize, Serialize};

/// 16-bit Octahedral 단위 법선 압축 (`DESIGN_SPEC.md` 2.3: 16B `GpuVertex` 용).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(C)]
pub struct OctahedralNormal16 {
    pub x: i16,
    pub y: i16,
}

impl OctahedralNormal16 {
    pub fn encode(n: [f64; 3]) -> Self {
        let l1 = n[0].abs() + n[1].abs() + n[2].abs();
        if l1 < 1e-12 {
            return Self { x: 0, y: 32767 };
        }
        let mut ox = n[0] / l1;
        let mut oy = n[1] / l1;
        if n[2] < 0.0 {
            let tx = (1.0 - oy.abs()) * if ox >= 0.0 { 1.0 } else { -1.0 };
            let ty = (1.0 - ox.abs()) * if oy >= 0.0 { 1.0 } else { -1.0 };
            ox = tx;
            oy = ty;
        }
        let ix = (ox.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        let iy = (oy.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        Self { x: ix, y: iy }
    }

    pub fn decode(self) -> [f64; 3] {
        let mut nx = f64::from(self.x) / 32767.0;
        let mut ny = f64::from(self.y) / 32767.0;
        let nz = 1.0 - nx.abs() - ny.abs();
        if nz < 0.0 {
            let ox = nx;
            let oy = ny;
            nx = (1.0 - oy.abs()) * if ox >= 0.0 { 1.0 } else { -1.0 };
            ny = (1.0 - ox.abs()) * if oy >= 0.0 { 1.0 } else { -1.0 };
        }
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        if len > 1e-12 {
            [nx / len, ny / len, nz / len]
        } else {
            [0.0, 0.0, 1.0]
        }
    }
}

/// 16바이트 압축 GPU 정점 구조체 (`1,000만 정점 = 160 MB VRAM`).
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct GpuVertex16 {
    pub pos_quantized: [u16; 3],
    pub flags: u16,
    pub normal_oct: OctahedralNormal16,
    pub rgba_packed: u32,
}

const _: () = assert!(std::mem::size_of::<GpuVertex16>() == 16);

/// `Compressed Corner Table` 위상 인덱싱 (`opposite_half_edge: Vec<u32>`).
/// 코너 인덱스 `c = 3 * tri_idx + local_k` (`local_k in 0..3`).
/// 포인터 기반 Half-Edge 대비 메모리를 1/5로 줄이고 `O(1)` 인접 삼각형 조회를 제공한다.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompressedCornerTable {
    /// 각 코너 `c` (`0 .. 3*F`)의 반대편 반모서리(Twin corner) 인덱스. 경계 모서리는 `u32::MAX`.
    pub opposite_corner: Vec<u32>,
}

impl CompressedCornerTable {
    pub const BOUNDARY: u32 = u32::MAX;

    pub fn build(triangles: &[[u32; 3]]) -> Self {
        let num_corners = triangles.len() * 3;
        let mut opposite_corner = vec![Self::BOUNDARY; num_corners];
        let mut directed_edges: Vec<(u32, u32, u32)> = Vec::with_capacity(num_corners);

        for (t_idx, tri) in triangles.iter().enumerate() {
            let base = (t_idx * 3) as u32;
            for k in 0..3usize {
                let u = tri[k];
                let v = tri[(k + 1) % 3];
                let corner = base + k as u32;
                let (min_v, max_v) = if u < v { (u, v) } else { (v, u) };
                directed_edges.push((min_v, max_v, corner));
            }
        }

        directed_edges.sort_unstable_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

        let mut i = 0;
        while i + 1 < directed_edges.len() {
            if directed_edges[i].0 == directed_edges[i + 1].0
                && directed_edges[i].1 == directed_edges[i + 1].1
            {
                let c0 = directed_edges[i].2 as usize;
                let c1 = directed_edges[i + 1].2 as usize;
                opposite_corner[c0] = c1 as u32;
                opposite_corner[c1] = c0 as u32;
                i += 2;
            } else {
                i += 1;
            }
        }

        Self { opposite_corner }
    }

    #[inline]
    pub fn twin(&self, corner: usize) -> Option<usize> {
        let opp = *self.opposite_corner.get(corner)?;
        if opp == Self::BOUNDARY {
            None
        } else {
            Some(opp as usize)
        }
    }

    #[inline]
    pub fn is_boundary_corner(&self, corner: usize) -> bool {
        self.twin(corner).is_none()
    }
}

/// UV 텍스처 아틀라스 (`RGBA8` 픽셀 버퍼, `.mtl` `map_Kd` `.jpg`/`.png` 디코딩 결과).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeshTextureRgba {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl MeshTextureRgba {
    /// OBJ UV 좌표 `(u, v)` (`v=0`이 하단, `v=1`이 상단)에서 쌍선형(Bilinear) 보간으로 고화질 RGB 색상을 샘플링한다.
    #[inline]
    pub fn sample_uv_rgb(&self, u: f32, v: f32) -> [u8; 3] {
        if self.width == 0
            || self.height == 0
            || self.rgba.len() < (self.width * self.height * 4) as usize
        {
            return [182, 176, 164];
        }
        let u_wrap = u.rem_euclid(1.0);
        let v_wrap = (1.0 - v).rem_euclid(1.0);
        let max_x = self.width.saturating_sub(1);
        let max_y = self.height.saturating_sub(1);
        let fx = u_wrap * (max_x as f32);
        let fy = v_wrap * (max_y as f32);
        let x0 = (fx.floor() as u32).min(max_x);
        let y0 = (fy.floor() as u32).min(max_y);
        let x1 = (x0 + 1).min(max_x);
        let y1 = (y0 + 1).min(max_y);
        let tx = (fx - x0 as f32).clamp(0.0, 1.0);
        let ty = (fy - y0 as f32).clamp(0.0, 1.0);

        let i00 = ((y0 * self.width + x0) * 4) as usize;
        let i10 = ((y0 * self.width + x1) * 4) as usize;
        let i01 = ((y1 * self.width + x0) * 4) as usize;
        let i11 = ((y1 * self.width + x1) * 4) as usize;

        let w00 = (1.0 - tx) * (1.0 - ty);
        let w10 = tx * (1.0 - ty);
        let w01 = (1.0 - tx) * ty;
        let w11 = tx * ty;

        let mut out = [0u8; 3];
        for (c, out_c) in out.iter_mut().enumerate() {
            let val = f32::from(self.rgba[i00 + c]) * w00
                + f32::from(self.rgba[i10 + c]) * w10
                + f32::from(self.rgba[i01 + c]) * w01
                + f32::from(self.rgba[i11 + c]) * w11;
            *out_c = val.round().clamp(0.0, 255.0) as u8;
        }
        out
    }
}

/// 3차원 삼각형 메쉬 (`TriMesh`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriMesh {
    pub name: String,
    pub positions: Vec<[f64; 3]>,
    pub normals: Vec<[f64; 3]>,
    pub colors_rgb: Vec<[u8; 3]>,
    pub texcoords: Vec<[f32; 2]>,
    /// 삼각면 각 코너별 UV 좌표 (`triangles.len()`과 동일 길이 또는 빈 벡터)
    #[serde(default)]
    pub triangle_uvs: Vec<[[f32; 2]; 3]>,
    /// `.mtl` `map_Kd`로부터 로드된 실사 UV 텍스처 이미지
    #[serde(skip)]
    pub texture_image: Option<std::sync::Arc<MeshTextureRgba>>,
    /// 동반 `.ap2` 파일에서 읽어온 절단면 높이/오프셋 (`rc.PlanePosition0` 등, 단위: `mm`)
    #[serde(default)]
    pub ap2_cut_offset_mm: Option<f64>,
    /// 원본 스캔 파일이 위치한 폴더 경로 (`ret/` 폴더 탐색 등에 활용)
    #[serde(default)]
    pub source_dir: Option<std::path::PathBuf>,
    pub triangles: Vec<[u32; 3]>,
    pub face_normals: Vec<[f64; 3]>,
    pub face_areas: Vec<f64>,
    pub face_centroids: Vec<[f64; 3]>,
    pub mean_curvatures: Vec<f64>,
    pub bbox: Aabb3,
    pub topology: CompressedCornerTable,
}

impl TriMesh {
    pub fn from_positions_and_triangles(
        name: impl Into<String>,
        positions: Vec<[f64; 3]>,
        triangles: Vec<[u32; 3]>,
    ) -> Result<Self, StrataError> {
        if positions.len() < 3 || triangles.is_empty() {
            return Err(StrataError::EmptyOrInvalidMesh {
                vertices: positions.len(),
                faces: triangles.len(),
            });
        }
        let n_verts = positions.len();
        let colors_rgb = vec![[182, 176, 164]; n_verts];
        let texcoords = Vec::new();
        Self::build_complete(name.into(), positions, triangles, colors_rgb, texcoords)
    }

    pub fn build_complete(
        name: String,
        positions: Vec<[f64; 3]>,
        triangles: Vec<[u32; 3]>,
        mut colors_rgb: Vec<[u8; 3]>,
        texcoords: Vec<[f32; 2]>,
    ) -> Result<Self, StrataError> {
        if positions.len() < 3 || triangles.is_empty() {
            return Err(StrataError::EmptyOrInvalidMesh {
                vertices: positions.len(),
                faces: triangles.len(),
            });
        }
        let n_verts = positions.len();
        if colors_rgb.len() != n_verts {
            colors_rgb.resize(n_verts, [182, 176, 164]);
        }

        let mut bbox = Aabb3::EMPTY;
        for &p in &positions {
            bbox.include_point(p);
        }

        let n_faces = triangles.len();
        let mut normals = vec![[0.0f64; 3]; n_verts];
        let mut face_normals = Vec::with_capacity(n_faces);
        let mut face_areas = Vec::with_capacity(n_faces);
        let mut face_centroids = Vec::with_capacity(n_faces);

        for tri in &triangles {
            let i0 = tri[0] as usize;
            let i1 = tri[1] as usize;
            let i2 = tri[2] as usize;
            if i0 >= n_verts || i1 >= n_verts || i2 >= n_verts {
                return Err(StrataError::MeshParse {
                    path: name,
                    reason: format!(
                        "삼각면 정점 인덱스 범위 초과: ({i0}, {i1}, {i2}) / 총 정점 {n_verts}"
                    ),
                });
            }
            let p0 = positions[i0];
            let p1 = positions[i1];
            let p2 = positions[i2];
            let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let cx = e1[1] * e2[2] - e1[2] * e2[1];
            let cy = e1[2] * e2[0] - e1[0] * e2[2];
            let cz = e1[0] * e2[1] - e1[1] * e2[0];
            let double_area = (cx * cx + cy * cy + cz * cz).sqrt();
            let area = 0.5 * double_area;
            let fn_unit = if double_area > 1e-14 {
                [cx / double_area, cy / double_area, cz / double_area]
            } else {
                [0.0, 0.0, 1.0]
            };
            face_normals.push(fn_unit);
            face_areas.push(area);
            face_centroids.push([
                (p0[0] + p1[0] + p2[0]) / 3.0,
                (p0[1] + p1[1] + p2[1]) / 3.0,
                (p0[2] + p1[2] + p2[2]) / 3.0,
            ]);

            // Area-weighted vertex normal accumulation
            for &vi in &[i0, i1, i2] {
                normals[vi][0] += cx;
                normals[vi][1] += cy;
                normals[vi][2] += cz;
            }
        }

        for n in &mut normals {
            let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if l > 1e-14 {
                n[0] /= l;
                n[1] /= l;
                n[2] /= l;
            } else {
                *n = [0.0, 0.0, 1.0];
            }
        }

        let topology = CompressedCornerTable::build(&triangles);
        let mean_curvatures =
            Self::compute_discrete_mean_curvatures(&positions, &normals, &triangles, &topology);

        Ok(Self {
            name,
            positions,
            normals,
            colors_rgb,
            texcoords,
            triangle_uvs: Vec::new(),
            texture_image: None,
            ap2_cut_offset_mm: None,
            source_dir: None,
            triangles,
            face_normals,
            face_areas,
            face_centroids,
            mean_curvatures,
            bbox,
            topology,
        })
    }

    fn compute_discrete_mean_curvatures(
        positions: &[[f64; 3]],
        normals: &[[f64; 3]],
        triangles: &[[u32; 3]],
        _topology: &CompressedCornerTable,
    ) -> Vec<f64> {
        let n_verts = positions.len();
        let mut curv_sum = vec![0.0f64; n_verts];
        let mut weight_sum = vec![0.0f64; n_verts];

        for tri in triangles {
            let idxs = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
            for k in 0..3 {
                let u = idxs[k];
                let v = idxs[(k + 1) % 3];
                let pu = positions[u];
                let pv = positions[v];
                let dx = pv[0] - pu[0];
                let dy = pv[1] - pu[1];
                let dz = pv[2] - pu[2];
                let len2 = dx * dx + dy * dy + dz * dz;
                if len2 > 1e-12 {
                    let nu = normals[u];
                    let nv = normals[v];
                    let dn_dot_dp =
                        (nv[0] - nu[0]) * dx + (nv[1] - nu[1]) * dy + (nv[2] - nu[2]) * dz;
                    let edge_k = dn_dot_dp / len2;
                    curv_sum[u] += edge_k;
                    weight_sum[u] += 1.0;
                    curv_sum[v] += edge_k;
                    weight_sum[v] += 1.0;
                }
            }
        }

        curv_sum
            .into_iter()
            .zip(weight_sum)
            .map(|(s, w)| if w > 0.0 { s / w } else { 0.0 })
            .collect()
    }

    /// 인접한 두 삼각형 사이의 이면각(Dihedral Angle, 단위: 도 `0°~180°`)을 반환한다.
    pub fn corner_dihedral_angle_deg(&self, corner: usize) -> Option<f64> {
        let twin_c = self.topology.twin(corner)?;
        let f0 = corner / 3;
        let f1 = twin_c / 3;
        let n0 = self.face_normals[f0];
        let n1 = self.face_normals[f1];
        let dot = (n0[0] * n1[0] + n0[1] * n1[1] + n0[2] * n1[2]).clamp(-1.0, 1.0);
        Some(dot.acos().to_degrees())
    }

    /// 변환 행렬(`Mat4`, 예: `.ap2`의 `TransfomMatrix`)을 적용한 새 메쉬를 생성한다.
    pub fn transformed(&self, mat: &Mat4) -> Self {
        let mut new_positions = Vec::with_capacity(self.positions.len());
        let mut new_normals = Vec::with_capacity(self.normals.len());
        let mut bbox = Aabb3::EMPTY;

        for &p in &self.positions {
            let tp = mat.transform_point(p);
            bbox.include_point(tp);
            new_positions.push(tp);
        }
        for &n in &self.normals {
            new_normals.push(mat.transform_normal(n));
        }

        let new_face_normals: Vec<[f64; 3]> = self
            .face_normals
            .iter()
            .map(|&fn_vec| mat.transform_normal(fn_vec))
            .collect();
        let new_face_centroids: Vec<[f64; 3]> = self
            .face_centroids
            .iter()
            .map(|&c| mat.transform_point(c))
            .collect();

        Self {
            name: self.name.clone(),
            positions: new_positions,
            normals: new_normals,
            colors_rgb: self.colors_rgb.clone(),
            texcoords: self.texcoords.clone(),
            triangle_uvs: self.triangle_uvs.clone(),
            texture_image: self.texture_image.clone(),
            ap2_cut_offset_mm: self.ap2_cut_offset_mm,
            source_dir: self.source_dir.clone(),
            triangles: self.triangles.clone(),
            face_normals: new_face_normals,
            face_areas: self.face_areas.clone(),
            face_centroids: new_face_centroids,
            mean_curvatures: self.mean_curvatures.clone(),
            bbox,
            topology: self.topology.clone(),
        }
    }
}
