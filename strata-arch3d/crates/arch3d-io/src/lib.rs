pub mod ap2_codec;
pub mod darch_codec;
pub mod obj_loader;
pub mod ply_stl;

pub use ap2_codec::{Ap2CutPlaneGroup, Ap2Project};
pub use darch_codec::DarchProject;
pub use obj_loader::{load_obj_file, LoadedObjBundle};
pub use ply_stl::load_ply_or_stl_file;

use arch3d_core::{Arch3dError, TriMesh};
use std::path::Path;

/// 확장자(`.obj`, `.ply`, `.stl`)를 자동 감지하여 3D 유물 메쉬를 로드한다.
pub fn load_any_mesh(path: impl AsRef<Path>) -> Result<TriMesh, Arch3dError> {
    let path_ref = path.as_ref();
    let ext = path_ref
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "obj" => Ok(load_obj_file(path_ref)?.mesh),
        "ply" | "stl" => load_ply_or_stl_file(path_ref),
        other => Err(Arch3dError::MeshParse {
            path: path_ref.display().to_string(),
            reason: format!("지원하지 않는 3D 확장자 '.{other}' (지원: .obj, .ply, .stl)"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_ap2_qvariant_roundtrip_7_9_7() {
        let ap2_path = PathBuf::from(r"E:\034(7-실4-4)\7지구 9호-7.ap2");
        if ap2_path.exists() {
            let proj = Ap2Project::load_from_file(&ap2_path).expect("load real .ap2");
            assert_eq!(proj.object_file_name, "7지구 9호-7.obj");
            assert_eq!(proj.project_result_folder_name, "ret");
            assert!((proj.bb_length_x - 125.17424011230469).abs() < 1e-6);
            assert!((proj.bb_length_y - 123.93498992919922).abs() < 1e-6);
            assert!((proj.bb_length_z - 63.13291931152349).abs() < 1e-6);
            assert_eq!(proj.object_num_cell, 335824);
            assert_eq!(proj.object_num_point, 1007472);
            assert!((proj.transform_matrix.m[0] - 0.9975640773773193).abs() < 1e-9);
            assert!((proj.transform_matrix.m[11] - 33.51960372924805).abs() < 1e-9);

            // Verify 1mm = 5px CanvasRect in rc
            assert!((proj.rc.canvas_rect[2] - proj.bb_length_x * 5.0).abs() < 1e-3);
            assert!((proj.rc.canvas_rect[3] - proj.bb_length_y * 5.0).abs() < 1e-3);

            // Roundtrip JSON serialization -> deserialization
            let encoded = proj.to_json_pretty_string().expect("serialize ap2");
            let decoded = Ap2Project::from_json_str(&encoded).expect("deserialize ap2");
            assert_eq!(proj.object_file_name, decoded.object_file_name);
            assert!((proj.bb_length_x - decoded.bb_length_x).abs() < 1e-9);
            assert!((proj.transform_matrix.m[11] - decoded.transform_matrix.m[11]).abs() < 1e-9);
        }
    }

    #[test]
    fn test_load_real_e_drive_1obj_and_7_9_7_obj() {
        let obj1_path = PathBuf::from(r"E:\1\1.obj");
        if obj1_path.exists() {
            let bundle = load_obj_file(&obj1_path).expect("load E:\\1\\1.obj");
            assert_eq!(bundle.mesh.positions.len(), 149_047);
            assert_eq!(bundle.mesh.triangles.len(), 298_090);
            assert_eq!(bundle.mtl_filename.as_deref(), Some("1.mtl"));
            assert_eq!(bundle.diffuse_texture_filename.as_deref(), Some("1_1.jpg"));
            let ext = bundle.mesh.bbox.extents();
            assert!(ext[0] > 140.0 && ext[0] < 170.0, "1.obj ext={ext:?}");
        }

        let obj7_path = PathBuf::from(r"E:\034(7-실4-4)\7지구 9호-7.obj");
        let ap2_path = PathBuf::from(r"E:\034(7-실4-4)\7지구 9호-7.ap2");
        if obj7_path.exists() && ap2_path.exists() {
            let bundle7 = load_obj_file(&obj7_path).expect("load 7지구 9호-7.obj");
            let ap2 = Ap2Project::load_from_file(&ap2_path).expect("load 7지구 9호-7.ap2");
            assert_eq!(bundle7.mesh.triangles.len() as u32, ap2.object_num_cell);

            // Apply TransfomMatrix from .ap2 and verify aligned bounding box matches BBLengthX/Y/Z!
            let aligned = bundle7.mesh.transformed(&ap2.transform_matrix);
            let ext = aligned.bbox.extents();
            assert!(
                (ext[0] - ap2.bb_length_x).abs() < 0.05,
                "Aligned X {} vs AP2 {}",
                ext[0],
                ap2.bb_length_x
            );
            assert!(
                (ext[1] - ap2.bb_length_y).abs() < 0.05,
                "Aligned Y {} vs AP2 {}",
                ext[1],
                ap2.bb_length_y
            );
            assert!(
                (ext[2] - ap2.bb_length_z).abs() < 0.05,
                "Aligned Z {} vs AP2 {}",
                ext[2],
                ap2.bb_length_z
            );
        }
    }

    #[test]
    fn test_darch_sanitize_clamped() {
        let p = DarchProject {
            primary_section_angle_deg: -45.0,
            dtak_ink_density_pct: 999.0,
            dtak_radius_mm: -5.0,
            ..Default::default()
        };
        let clean = p.sanitize_clamped();
        assert!((clean.primary_section_angle_deg - 315.0).abs() < 1e-9);
        assert!((clean.dtak_ink_density_pct - 100.0).abs() < 1e-9);
        assert!((clean.dtak_radius_mm - 0.1).abs() < 1e-9);
    }
}
