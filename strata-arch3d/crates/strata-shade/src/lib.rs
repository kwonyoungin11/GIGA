pub mod feature_lines;
pub mod hemp_fft;
pub mod rubbing_engine;
pub mod unroll;

pub use feature_lines::{
    extract_five_feature_lines, FeatureLineKind, FeatureSegment2D,
};
pub use hemp_fft::{measure_hemp_cloth_threads_per_cm, HempThreadCountResult};
pub use rubbing_engine::{
    compute_vertex_rubbing_ink_weights, render_orthographic_rubbing_image, RubbingAlgorithm,
    RubbingRenderParams,
};
pub use unroll::{unroll_mesh_surface, UnrollMode, UnrollResult, UnrolledPoint2D};

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::{StacklessLbvh, TriMesh};

    #[test]
    fn test_rubbing_3_algorithms_and_5_feature_lines() {
        let positions = vec![
            [-20.0, 0.0, 0.0],
            [0.0, -5.0, 0.0],
            [20.0, 0.0, 0.0],
            [-20.0, 0.0, 30.0],
            [0.0, -8.0, 30.0],
            [20.0, 0.0, 30.0],
        ];
        let triangles = vec![[0, 1, 4], [0, 4, 3], [1, 2, 5], [1, 5, 4]];
        let mesh =
            TriMesh::from_positions_and_triangles("relief_patch", positions, triangles).expect("ok");
        let bvh = StacklessLbvh::build(&mesh);

        for alg in [
            RubbingAlgorithm::NormalGradientHandprint,
            RubbingAlgorithm::Msii16Scale,
            RubbingAlgorithm::TopologicalOpenness,
        ] {
            let params = RubbingRenderParams {
                algorithm: alg,
                ink_density_pct: 75.0,
                curv_radius_mm: 1.5,
                use_handprint_grain: true,
            };
            let w = compute_vertex_rubbing_ink_weights(&mesh, Some(&bvh), params, [0.0, -1.0, 0.0]);
            assert_eq!(w.len(), mesh.positions.len());
            assert!(w.iter().all(|&v| (0.0..=1.0).contains(&v)));
        }

        let img = render_orthographic_rubbing_image(
            &mesh,
            Some(&bvh),
            RubbingRenderParams::default(),
            128,
            128,
        );
        assert_eq!(img.width(), 128);
        assert_eq!(img.height(), 128);

        let flines = extract_five_feature_lines(&mesh, [0.0, -1.0, 0.0]);
        assert!(!flines.is_empty());
    }

    #[test]
    fn test_cylindrical_unroll_and_hemp_fft_thread_count() {
        // Test 1: 1x1 cm hemp cloth grid with 8 warp threads x 10 weft threads per cm
        let n = 64usize;
        let mut grid = vec![0.0f64; n * n];
        for y in 0..n {
            for x in 0..n {
                let wx = (std::f64::consts::TAU * 8.0 * (x as f64) / (n as f64)).cos();
                let wy = (std::f64::consts::TAU * 10.0 * (y as f64) / (n as f64)).cos();
                grid[y * n + x] = wx + wy;
            }
        }
        let hemp = measure_hemp_cloth_threads_per_cm(&grid, n);
        assert_eq!(hemp.warp_threads_per_cm, 8);
        assert_eq!(hemp.weft_threads_per_cm, 10);
        assert!(hemp.format_kr().contains("8×10올"));

        // Test 2: Cylindrical Unroll
        let positions = vec![
            [30.0, 0.0, 0.0],
            [0.0, 30.0, 0.0],
            [-30.0, 0.0, 20.0],
            [0.0, -30.0, 20.0],
        ];
        let triangles = vec![[0, 1, 2], [0, 2, 3]];
        let mesh = TriMesh::from_positions_and_triangles("cyl", positions, triangles).expect("ok");
        let unrolled = unroll_mesh_surface(&mesh, UnrollMode::Cylindrical);
        assert!((unrolled.mean_radius_mm - 30.0).abs() < 1e-6);
        assert_eq!(unrolled.points.len(), 4);
    }
}
