use crate::mesh::TriMesh;
use crate::types::Aabb3;
use serde::{Deserialize, Serialize};

/// `Stackless LBVH` 노드 (`DESIGN_SPEC.md` 2.3).
/// 모든 내부/리프 노드에 서브트리 면적(`subtree_area`)과 면적가중 법선합(`subtree_area_normal`)을 사전 적분하여,
/// `16-Scale MSII` 구형 적분 불변량 계산 시 구 내부에 완전히 포함되는 노드를 `O(1)`에 누적한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LbvhNode {
    pub bbox: Aabb3,
    pub subtree_area: f64,
    pub subtree_area_normal: [f64; 3],
    pub left_or_start: u32,
    pub right_or_count: u32,
    pub is_leaf: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StacklessLbvh {
    pub nodes: Vec<LbvhNode>,
    pub tri_indices: Vec<u32>,
}

impl StacklessLbvh {
    pub const LEAF_CAPACITY: usize = 8;

    pub fn build(mesh: &TriMesh) -> Self {
        let n_tris = mesh.triangles.len();
        let mut tri_indices: Vec<u32> = (0..n_tris as u32).collect();
        let mut nodes = Vec::with_capacity(n_tris * 2 / Self::LEAF_CAPACITY + 4);
        if n_tris > 0 {
            Self::build_recursive(mesh, &mut tri_indices, 0, n_tris, &mut nodes);
        }
        Self { nodes, tri_indices }
    }

    fn build_recursive(
        mesh: &TriMesh,
        tri_indices: &mut [u32],
        start: usize,
        end: usize,
        nodes: &mut Vec<LbvhNode>,
    ) -> u32 {
        let node_idx = nodes.len() as u32;
        let mut bbox = Aabb3::EMPTY;
        let mut centroid_bbox = Aabb3::EMPTY;
        let mut subtree_area = 0.0;
        let mut subtree_area_normal = [0.0f64; 3];

        for &ti in &tri_indices[start..end] {
            let t = ti as usize;
            let tri = mesh.triangles[t];
            bbox.include_point(mesh.positions[tri[0] as usize]);
            bbox.include_point(mesh.positions[tri[1] as usize]);
            bbox.include_point(mesh.positions[tri[2] as usize]);
            centroid_bbox.include_point(mesh.face_centroids[t]);

            let area = mesh.face_areas[t];
            let fn_v = mesh.face_normals[t];
            subtree_area += area;
            subtree_area_normal[0] += area * fn_v[0];
            subtree_area_normal[1] += area * fn_v[1];
            subtree_area_normal[2] += area * fn_v[2];
        }

        let count = end - start;
        if count <= Self::LEAF_CAPACITY {
            nodes.push(LbvhNode {
                bbox,
                subtree_area,
                subtree_area_normal,
                left_or_start: start as u32,
                right_or_count: count as u32,
                is_leaf: true,
            });
            return node_idx;
        }

        let ext = centroid_bbox.extents();
        let axis = if ext[0] >= ext[1] && ext[0] >= ext[2] {
            0
        } else if ext[1] >= ext[2] {
            1
        } else {
            2
        };

        let mid = start + count / 2;
        tri_indices[start..end].select_nth_unstable_by(mid - start, |&a, &b| {
            let ca = mesh.face_centroids[a as usize][axis];
            let cb = mesh.face_centroids[b as usize][axis];
            ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
        });

        // Reserve slot for internal node
        nodes.push(LbvhNode {
            bbox,
            subtree_area,
            subtree_area_normal,
            left_or_start: 0,
            right_or_count: 0,
            is_leaf: false,
        });

        let left_child = Self::build_recursive(mesh, tri_indices, start, mid, nodes);
        let right_child = Self::build_recursive(mesh, tri_indices, mid, end, nodes);

        nodes[node_idx as usize].left_or_start = left_child;
        nodes[node_idx as usize].right_or_count = right_child;
        node_idx
    }

    /// 절단 평면 $n \cdot x = d$ 와 AABB가 교차하는 후보 삼각면 인덱스만 빠르게 필터링한다.
    pub fn query_plane_triangles(&self, plane_normal: [f64; 3], plane_d: f64) -> Vec<u32> {
        let mut result = Vec::new();
        if self.nodes.is_empty() {
            return result;
        }
        let mut stack = Vec::with_capacity(64);
        stack.push(0u32);

        while let Some(node_idx) = stack.pop() {
            let node = &self.nodes[node_idx as usize];
            if !Self::aabb_intersects_plane(&node.bbox, plane_normal, plane_d) {
                continue;
            }
            if node.is_leaf {
                let start = node.left_or_start as usize;
                let end = start + node.right_or_count as usize;
                result.extend_from_slice(&self.tri_indices[start..end]);
            } else {
                stack.push(node.right_or_count);
                stack.push(node.left_or_start);
            }
        }
        result
    }

    /// `16-Scale MSII` 구형 반경 $r$ 적분 불변량 쿼리:
    /// 노드 AABB가 구 $B_r(c)$ 내부에 완전히 포함되면 리프까지 내려가지 않고
    /// 사전 적분된 `subtree_area`와 `subtree_area_normal`을 `O(1)`에 합산한다.
    pub fn query_sphere_integral(
        &self,
        mesh: &TriMesh,
        center: [f64; 3],
        radius: f64,
    ) -> (f64, [f64; 3]) {
        if self.nodes.is_empty() || radius <= 0.0 {
            return (0.0, [0.0; 3]);
        }
        let r2 = radius * radius;
        let mut total_area = 0.0;
        let mut total_normal = [0.0f64; 3];
        let mut stack = Vec::with_capacity(64);
        stack.push(0u32);

        while let Some(node_idx) = stack.pop() {
            let node = &self.nodes[node_idx as usize];
            if !Self::aabb_intersects_sphere(&node.bbox, center, r2) {
                continue;
            }
            if Self::aabb_contained_in_sphere(&node.bbox, center, r2) {
                // O(1) Pre-integrated Subtree Short-Circuit!
                total_area += node.subtree_area;
                total_normal[0] += node.subtree_area_normal[0];
                total_normal[1] += node.subtree_area_normal[1];
                total_normal[2] += node.subtree_area_normal[2];
                continue;
            }
            if node.is_leaf {
                let start = node.left_or_start as usize;
                let end = start + node.right_or_count as usize;
                for &ti in &self.tri_indices[start..end] {
                    let t = ti as usize;
                    let c = mesh.face_centroids[t];
                    let dx = c[0] - center[0];
                    let dy = c[1] - center[1];
                    let dz = c[2] - center[2];
                    if dx * dx + dy * dy + dz * dz <= r2 {
                        let a = mesh.face_areas[t];
                        let fn_v = mesh.face_normals[t];
                        total_area += a;
                        total_normal[0] += a * fn_v[0];
                        total_normal[1] += a * fn_v[1];
                        total_normal[2] += a * fn_v[2];
                    }
                }
            } else {
                stack.push(node.right_or_count);
                stack.push(node.left_or_start);
            }
        }
        (total_area, total_normal)
    }

    fn aabb_intersects_plane(bbox: &Aabb3, n: [f64; 3], d: f64) -> bool {
        let c = bbox.center();
        let ext = bbox.extents();
        let r = 0.5 * (ext[0] * n[0].abs() + ext[1] * n[1].abs() + ext[2] * n[2].abs());
        let s = n[0] * c[0] + n[1] * c[1] + n[2] * c[2] - d;
        s.abs() <= r + 1e-6
    }

    fn aabb_intersects_sphere(bbox: &Aabb3, center: [f64; 3], r2: f64) -> bool {
        let mut d2 = 0.0;
        for (i, &c_val) in center.iter().enumerate() {
            if c_val < bbox.min[i] {
                let diff = bbox.min[i] - c_val;
                d2 += diff * diff;
            } else if c_val > bbox.max[i] {
                let diff = c_val - bbox.max[i];
                d2 += diff * diff;
            }
        }
        d2 <= r2
    }

    fn aabb_contained_in_sphere(bbox: &Aabb3, center: [f64; 3], r2: f64) -> bool {
        let mut max_d2 = 0.0;
        for (i, &c_val) in center.iter().enumerate() {
            let d_min = (bbox.min[i] - c_val).abs();
            let d_max = (bbox.max[i] - c_val).abs();
            let far = d_min.max(d_max);
            max_d2 += far * far;
        }
        max_d2 <= r2
    }
}
