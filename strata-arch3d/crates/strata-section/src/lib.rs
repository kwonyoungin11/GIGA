pub mod bezier_fit;
pub mod occluded_repair;
pub mod plane_slicer;

pub use bezier_fit::{fit_polyline_to_cubic_bezier, CubicBezierSegment2D, FittedSectionCurve};
pub use occluded_repair::{infer_occluded_dashed_spans, OccludedDashedSpan};
pub use plane_slicer::{slice_mesh_with_fixed_plane, CutPlaneOrientation, PlaneSliceResult};

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::{StacklessLbvh, TriMesh};

    #[test]
    fn test_world_locked_plane_slice_and_kurbo_bezier_fit() {
        // Build a cylindrical vessel wall mesh (R=50mm, H=40mm)
        let mut positions = Vec::new();
        let mut triangles = Vec::new();
        let nu = 36usize;
        let nv = 12usize;
        for iv in 0..=nv {
            let z = (iv as f64 / nv as f64) * 40.0;
            let r = 50.0 + (z * 0.08).sin() * 2.0;
            for iu in 0..=nu {
                let theta = (iu as f64 / nu as f64) * std::f64::consts::TAU;
                positions.push([r * theta.cos(), r * theta.sin(), z]);
            }
        }
        let stride = (nu + 1) as u32;
        for iv in 0..(nv as u32) {
            for iu in 0..(nu as u32) {
                let a = iv * stride + iu;
                let b = a + 1;
                let c = a + stride;
                let d = c + 1;
                triangles.push([a, b, d]);
                triangles.push([a, d, c]);
            }
        }
        let mesh =
            TriMesh::from_positions_and_triangles("vessel_cyl", positions, triangles).expect("ok");
        let bvh = StacklessLbvh::build(&mesh);

        // Slice vertically at Y = 0.0mm
        let v_slice = slice_mesh_with_fixed_plane(
            &mesh,
            Some(&bvh),
            CutPlaneOrientation::VerticalLongitudinal,
            0.0,
            0.0,
            0.25,
        );
        assert!(!v_slice.segments_2d.is_empty());
        assert!(!v_slice.polylines_2d.is_empty());

        // Fit Cubic Bézier curve and verify sub-0.02mm accuracy and 1mm=5px SVG path generation
        let poly = &v_slice.polylines_2d[0];
        let bez = fit_polyline_to_cubic_bezier(poly, 0.015, 5.0);
        assert!(
            bez.max_deviation_mm < 0.02,
            "max_deviation_mm={}",
            bez.max_deviation_mm
        );
        assert!(bez.svg_path_d_1mm_5px.starts_with('M'));
        assert!(bez.svg_path_d_1mm_5px.contains(" C"));
    }

    #[test]
    fn test_occluded_inner_wall_and_drill_dashed_repair() {
        let p1 = vec![[10.0, 40.0], [10.0, 28.0]];
        let p2 = vec![[10.2, 16.0], [10.2, 0.0]];
        let spans = infer_occluded_dashed_spans(&[p1, p2], 2.0, 25.0);
        assert!(!spans.is_empty());
        assert!((spans[0].gap_length_mm - 12.0).abs() < 0.2);
    }
}
