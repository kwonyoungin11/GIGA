use strata_core::{StrataError, Mat4};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

/// `STRATA Liner` `.ap2` 파일 내 `fc`(정면), `rc`(우측면), `tc`(상면) 5슬롯 절단면 설정.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ap2CutPlaneGroup {
    pub canvas_rect: [f64; 4],
    pub canvas_rect_cutline: [f64; 4],
    pub curve_level: i32,
    pub curve_scale: f64,
    pub curved_error: f64,
    pub detail_level: i32,
    pub is_saved: [bool; 5],
    pub merge_points_tolerance: f64,
    pub plane_position: f64,
    pub plane_positions: [f64; 5],
    pub plane_rotation: f64,
    pub plane_rotations: [f64; 5],
    pub point_num_level: i32,
    pub preview_on: bool,
    pub scale_level: i32,
    pub tolerance_on: bool,
}

impl Default for Ap2CutPlaneGroup {
    fn default() -> Self {
        Self {
            canvas_rect: [0.0, 0.0, 500.0, 500.0],
            canvas_rect_cutline: [0.0, 0.0, 500.0, 500.0],
            curve_level: 2,
            curve_scale: 3.0,
            curved_error: 0.05,
            detail_level: 2,
            is_saved: [false; 5],
            merge_points_tolerance: 0.05,
            plane_position: 0.0,
            plane_positions: [0.0; 5],
            plane_rotation: 0.0,
            plane_rotations: [0.0; 5],
            point_num_level: 2,
            preview_on: true,
            scale_level: 1,
            tolerance_on: true,
        }
    }
}

/// `STRATA Liner` `.ap2` 프로젝트 파일 전체 구조 (`7지구 9호-7.ap2` 100% 양방향 호환).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ap2Project {
    pub bb_length_x: f64,
    pub bb_length_y: f64,
    pub bb_length_z: f64,
    pub is_rotation_done: bool,
    pub object_average_cell_area: f64,
    pub object_average_edge_length: f64,
    pub object_file_name: String,
    pub object_num_cell: u32,
    pub object_num_point: u32,
    pub project_result_folder_name: String,
    pub project_version: i32,
    pub transform_matrix: Mat4,
    pub fc: Ap2CutPlaneGroup,
    pub rc: Ap2CutPlaneGroup,
    pub tc: Ap2CutPlaneGroup,
    pub wcenter_ambient: f64,
    pub wcenter_edge_falloff: f64,
    pub wcenter_intensity: f64,
    pub wcenter_xray_rgba: [u8; 4],
}

impl Ap2Project {
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, StrataError> {
        let path_ref = path.as_ref();
        let text = fs::read_to_string(path_ref).map_err(|e| StrataError::Io {
            path: path_ref.display().to_string(),
            message: e.to_string(),
        })?;
        Self::from_json_str(&text)
    }

    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<(), StrataError> {
        let path_ref = path.as_ref();
        let json_text = self.to_json_pretty_string()?;
        fs::write(path_ref, json_text).map_err(|e| StrataError::Io {
            path: path_ref.display().to_string(),
            message: e.to_string(),
        })
    }

    pub fn from_json_str(text: &str) -> Result<Self, StrataError> {
        let root: Value =
            serde_json::from_str(text).map_err(|e| StrataError::ProjectFormat {
                reason: format!(".ap2 JSON 구문 오류: {e}"),
            })?;

        let pinfo = extract_qvariant_map(&root, "project_info")?;
        let bb_length_x = qv_f64(pinfo, "BBLengthX", 0.0);
        let bb_length_y = qv_f64(pinfo, "BBLengthY", 0.0);
        let bb_length_z = qv_f64(pinfo, "BBLengthZ", 0.0);
        let is_rotation_done = qv_bool(pinfo, "IsRotationDone", true);
        let object_average_cell_area = qv_f64(pinfo, "ObjectAverageCellArea", 0.1);
        let object_average_edge_length = qv_f64(pinfo, "ObjectAverageEdgeLength", 0.5);
        let object_file_name = qv_str(pinfo, "ObjectFileName", "");
        let object_num_cell = qv_u32(pinfo, "ObjectNumCell", 0);
        let object_num_point = qv_u32(pinfo, "ObjectNumPoint", 0);
        let project_result_folder_name = qv_str(pinfo, "ProjectReusltFolderName", "ret");
        let project_version = qv_i32(pinfo, "ProjectVersion", 0);
        let transform_matrix = qv_mat4(pinfo, "TransfomMatrix")?;

        let fc = extract_qvariant_map(&root, "fc")
            .map(parse_cut_plane_group)
            .unwrap_or_default();
        let rc = extract_qvariant_map(&root, "rc")
            .map(parse_cut_plane_group)
            .unwrap_or_default();
        let tc = extract_qvariant_map(&root, "tc")
            .map(parse_cut_plane_group)
            .unwrap_or_default();

        let (wcenter_ambient, wcenter_edge_falloff, wcenter_intensity, wcenter_xray_rgba) =
            if let Ok(wc) = extract_qvariant_map(&root, "wcenter") {
                (
                    qv_f64(wc, "Ambient", 0.1),
                    qv_f64(wc, "EdgeFalloff", 2.0),
                    qv_f64(wc, "Intensity", 0.7),
                    qv_color_rgba(wc, "XRayColor", [255, 255, 255, 255]),
                )
            } else {
                (0.1, 2.0, 0.7, [255, 255, 255, 255])
            };

        Ok(Self {
            bb_length_x,
            bb_length_y,
            bb_length_z,
            is_rotation_done,
            object_average_cell_area,
            object_average_edge_length,
            object_file_name,
            object_num_cell,
            object_num_point,
            project_result_folder_name,
            project_version,
            transform_matrix,
            fc,
            rc,
            tc,
            wcenter_ambient,
            wcenter_edge_falloff,
            wcenter_intensity,
            wcenter_xray_rgba,
        })
    }

    pub fn to_json_pretty_string(&self) -> Result<String, StrataError> {
        let mut mat_arr = Vec::with_capacity(17);
        mat_arr.push(json!(81));
        for &v in &self.transform_matrix.m {
            mat_arr.push(json!(v));
        }

        let root = json!({
            "project_info": [
                8,
                {
                    "BBLengthX": [6, self.bb_length_x],
                    "BBLengthY": [6, self.bb_length_y],
                    "BBLengthZ": [6, self.bb_length_z],
                    "IsRotationDone": [1, self.is_rotation_done],
                    "ObjectAverageCellArea": [6, self.object_average_cell_area],
                    "ObjectAverageEdgeLength": [6, self.object_average_edge_length],
                    "ObjectFileName": [10, self.object_file_name],
                    "ObjectNumCell": [3, self.object_num_cell],
                    "ObjectNumPoint": [3, self.object_num_point],
                    "ProjectReusltFolderName": [10, self.project_result_folder_name],
                    "ProjectVersion": [2, self.project_version],
                    "TransfomMatrix": mat_arr,
                }
            ],
            "fc": encode_cut_plane_group(&self.fc),
            "rc": encode_cut_plane_group(&self.rc),
            "tc": encode_cut_plane_group(&self.tc),
            "wcenter": [
                8,
                {
                    "Ambient": [6, self.wcenter_ambient],
                    "ClipFlipOn": [1, false],
                    "ClipOn": [1, true],
                    "EdgeFalloff": [6, self.wcenter_edge_falloff],
                    "Intensity": [6, self.wcenter_intensity],
                    "PreviewOn": [1, true],
                    "XRayColor": [
                        67,
                        self.wcenter_xray_rgba[0],
                        self.wcenter_xray_rgba[1],
                        self.wcenter_xray_rgba[2],
                        self.wcenter_xray_rgba[3]
                    ],
                    "showClipPlane": [1, true],
                    "showHObject": [1, true]
                }
            ]
        });

        serde_json::to_string_pretty(&root).map_err(|e| StrataError::ProjectFormat {
            reason: format!(".ap2 JSON 직렬화 실패: {e}"),
        })
    }
}

fn extract_qvariant_map<'a>(
    root: &'a Value,
    key: &str,
) -> Result<&'a serde_json::Map<String, Value>, StrataError> {
    let arr = root
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| StrataError::ProjectFormat {
            reason: format!("최상위 키 '{key}' [8, {{...}}] 누락"),
        })?;
    if arr.len() < 2 {
        return Err(StrataError::ProjectFormat {
            reason: format!("키 '{key}' 배열 길이 부족"),
        });
    }
    arr[1].as_object().ok_or_else(|| StrataError::ProjectFormat {
        reason: format!("키 '{key}' QVariantMap 객체 아님"),
    })
}

fn parse_cut_plane_group(map: &serde_json::Map<String, Value>) -> Ap2CutPlaneGroup {
    Ap2CutPlaneGroup {
        canvas_rect: qv_rect(map, "CanvasRect", [0.0, 0.0, 500.0, 500.0]),
        canvas_rect_cutline: qv_rect(map, "CanvasRect_CutLine", [0.0, 0.0, 500.0, 500.0]),
        curve_level: qv_i32(map, "CurveLevel", 2),
        curve_scale: qv_f64(map, "CurveScale", 3.0),
        curved_error: qv_f64(map, "CurvedError", 0.05),
        detail_level: qv_i32(map, "DetailLevel", 2),
        is_saved: [
            qv_bool(map, "IsSaved0", false),
            qv_bool(map, "IsSaved1", false),
            qv_bool(map, "IsSaved2", false),
            qv_bool(map, "IsSaved3", false),
            qv_bool(map, "IsSaved4", false),
        ],
        merge_points_tolerance: qv_f64(map, "MergePointsTolerance", 0.05),
        plane_position: qv_f64(map, "PlanePosition", 0.0),
        plane_positions: [
            qv_f64(map, "PlanePosition0", 0.0),
            qv_f64(map, "PlanePosition1", 0.0),
            qv_f64(map, "PlanePosition2", 0.0),
            qv_f64(map, "PlanePosition3", 0.0),
            qv_f64(map, "PlanePosition4", 0.0),
        ],
        plane_rotation: qv_f64(map, "PlaneRotation", 0.0),
        plane_rotations: [
            qv_f64(map, "PlaneRotation0", 0.0),
            qv_f64(map, "PlaneRotation1", 0.0),
            qv_f64(map, "PlaneRotation2", 0.0),
            qv_f64(map, "PlaneRotation3", 0.0),
            qv_f64(map, "PlaneRotation4", 0.0),
        ],
        point_num_level: qv_i32(map, "PointNumLevel", 2),
        preview_on: qv_bool(map, "PreviewOn", true),
        scale_level: qv_i32(map, "ScaleLevel", 1),
        tolerance_on: qv_bool(map, "ToleranceOn", true),
    }
}

fn encode_cut_plane_group(g: &Ap2CutPlaneGroup) -> Value {
    json!([
        8,
        {
            "CanvasRect": [20, g.canvas_rect[0], g.canvas_rect[1], g.canvas_rect[2], g.canvas_rect[3]],
            "CanvasRect_CutLine": [20, g.canvas_rect_cutline[0], g.canvas_rect_cutline[1], g.canvas_rect_cutline[2], g.canvas_rect_cutline[3]],
            "CurveLevel": [2, g.curve_level],
            "CurveScale": [6, g.curve_scale],
            "CurvedError": [6, g.curved_error],
            "DetailLevel": [2, g.detail_level],
            "IsSaved0": [1, g.is_saved[0]],
            "IsSaved1": [1, g.is_saved[1]],
            "IsSaved2": [1, g.is_saved[2]],
            "IsSaved3": [1, g.is_saved[3]],
            "IsSaved4": [1, g.is_saved[4]],
            "MergePointsTolerance": [6, g.merge_points_tolerance],
            "PlanePosition": [6, g.plane_position],
            "PlanePosition0": [6, g.plane_positions[0]],
            "PlanePosition1": [6, g.plane_positions[1]],
            "PlanePosition2": [6, g.plane_positions[2]],
            "PlanePosition3": [6, g.plane_positions[3]],
            "PlanePosition4": [6, g.plane_positions[4]],
            "PlaneRotation": [6, g.plane_rotation],
            "PlaneRotation0": [6, g.plane_rotations[0]],
            "PlaneRotation1": [6, g.plane_rotations[1]],
            "PlaneRotation2": [6, g.plane_rotations[2]],
            "PlaneRotation3": [6, g.plane_rotations[3]],
            "PlaneRotation4": [6, g.plane_rotations[4]],
            "PointNumLevel": [2, g.point_num_level],
            "PreviewOn": [1, g.preview_on],
            "ScaleLevel": [2, g.scale_level],
            "ToleranceOn": [1, g.tolerance_on],
        }
    ])
}

fn qv_f64(map: &serde_json::Map<String, Value>, key: &str, default: f64) -> f64 {
    map.get(key)
        .and_then(Value::as_array)
        .and_then(|a| a.get(1))
        .and_then(Value::as_f64)
        .unwrap_or(default)
}

fn qv_i32(map: &serde_json::Map<String, Value>, key: &str, default: i32) -> i32 {
    map.get(key)
        .and_then(Value::as_array)
        .and_then(|a| a.get(1))
        .and_then(Value::as_i64)
        .map(|v| v as i32)
        .unwrap_or(default)
}

fn qv_u32(map: &serde_json::Map<String, Value>, key: &str, default: u32) -> u32 {
    map.get(key)
        .and_then(Value::as_array)
        .and_then(|a| a.get(1))
        .and_then(Value::as_u64)
        .map(|v| v as u32)
        .unwrap_or(default)
}

fn qv_bool(map: &serde_json::Map<String, Value>, key: &str, default: bool) -> bool {
    map.get(key)
        .and_then(Value::as_array)
        .and_then(|a| a.get(1))
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

fn qv_str(map: &serde_json::Map<String, Value>, key: &str, default: &str) -> String {
    map.get(key)
        .and_then(Value::as_array)
        .and_then(|a| a.get(1))
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn qv_rect(map: &serde_json::Map<String, Value>, key: &str, default: [f64; 4]) -> [f64; 4] {
    let Some(arr) = map.get(key).and_then(Value::as_array) else {
        return default;
    };
    if arr.len() < 5 {
        return default;
    }
    [
        arr[1].as_f64().unwrap_or(default[0]),
        arr[2].as_f64().unwrap_or(default[1]),
        arr[3].as_f64().unwrap_or(default[2]),
        arr[4].as_f64().unwrap_or(default[3]),
    ]
}

fn qv_color_rgba(map: &serde_json::Map<String, Value>, key: &str, default: [u8; 4]) -> [u8; 4] {
    let Some(arr) = map.get(key).and_then(Value::as_array) else {
        return default;
    };
    if arr.len() < 5 {
        return default;
    }
    [
        arr[1].as_u64().unwrap_or(u64::from(default[0])) as u8,
        arr[2].as_u64().unwrap_or(u64::from(default[1])) as u8,
        arr[3].as_u64().unwrap_or(u64::from(default[2])) as u8,
        arr[4].as_u64().unwrap_or(u64::from(default[3])) as u8,
    ]
}

fn qv_mat4(map: &serde_json::Map<String, Value>, key: &str) -> Result<Mat4, StrataError> {
    let arr = map
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| StrataError::ProjectFormat {
            reason: format!("행렬 키 '{key}' [81, m0..m15] 누락"),
        })?;
    if arr.len() < 17 {
        return Err(StrataError::ProjectFormat {
            reason: format!("행렬 '{key}' 원소 수 부족: {} (기대값 17)", arr.len()),
        });
    }
    let mut m = [0.0f64; 16];
    for i in 0..16 {
        m[i] = arr[i + 1]
            .as_f64()
            .ok_or_else(|| StrataError::ProjectFormat {
                reason: format!("행렬 '{key}' 원소 {i} 부동소수점 변환 실패"),
            })?;
    }
    Ok(Mat4 { m })
}
