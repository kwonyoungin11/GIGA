use strata_core::{
    StrataError, ArtifactCategory, BeadDrillType, BeadSubtype, Mat4, ScalePreset, SherdSubtype,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// `STRATA · 동국유물실측` 전용 `.darch` 프로젝트 작업 기록 구조체.
/// 손상된 외부 JSON 입력에 대비해 모든 수치 필드를 안전 범위로 클램핑(`sanitize_clamped`)한다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DarchProject {
    pub version: u32,
    pub artifact_title: String,
    pub site_and_feature: String,
    pub mesh_path: String,
    pub category: ArtifactCategory,
    pub sherd_subtype: Option<SherdSubtype>,
    pub bead_subtype: Option<BeadSubtype>,
    pub bead_drill_type: Option<BeadDrillType>,
    pub pose_matrix: Mat4,
    pub pose_confirmed: bool,
    pub primary_section_angle_deg: f64,
    pub extra_section_angles_deg: Vec<f64>,
    pub horizontal_cut_heights_mm: Vec<f64>,
    pub flip_exterior_right: bool,
    pub scale_preset: ScalePreset,
    pub rubbing_ink_density_pct: f64,
    pub rubbing_radius_mm: f64,
    pub rubbing_handprint_texture: bool,
}

impl Default for DarchProject {
    fn default() -> Self {
        Self {
            version: 2,
            artifact_title: String::from("무제 유물"),
            site_and_feature: String::new(),
            mesh_path: String::new(),
            category: ArtifactCategory::CompleteVessel,
            sherd_subtype: None,
            bead_subtype: None,
            bead_drill_type: None,
            pose_matrix: Mat4::IDENTITY,
            pose_confirmed: false,
            primary_section_angle_deg: 0.0,
            extra_section_angles_deg: Vec::new(),
            horizontal_cut_heights_mm: Vec::new(),
            flip_exterior_right: false,
            scale_preset: ScalePreset::Half1To2,
            rubbing_ink_density_pct: 72.0,
            rubbing_radius_mm: 1.4,
            rubbing_handprint_texture: true,
        }
    }
}

impl DarchProject {
    /// 수치 필드를 안전 범위로 클램핑하여 비정상 값으로 인한 렌더링/계산 오류를 방지한다.
    pub fn sanitize_clamped(mut self) -> Self {
        if !self.primary_section_angle_deg.is_finite() {
            self.primary_section_angle_deg = 0.0;
        } else {
            self.primary_section_angle_deg = self.primary_section_angle_deg.rem_euclid(360.0);
        }
        if !self.rubbing_ink_density_pct.is_finite() {
            self.rubbing_ink_density_pct = 72.0;
        } else {
            self.rubbing_ink_density_pct = self.rubbing_ink_density_pct.clamp(5.0, 100.0);
        }
        if !self.rubbing_radius_mm.is_finite() {
            self.rubbing_radius_mm = 1.4;
        } else {
            self.rubbing_radius_mm = self.rubbing_radius_mm.clamp(0.1, 15.0);
        }
        self.extra_section_angles_deg
            .retain(|a| a.is_finite());
        for a in &mut self.extra_section_angles_deg {
            *a = a.rem_euclid(360.0);
        }
        self.horizontal_cut_heights_mm
            .retain(|h| h.is_finite() && *h >= -1000.0 && *h <= 5000.0);
        self
    }

    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, StrataError> {
        let path_ref = path.as_ref();
        let raw = fs::read_to_string(path_ref).map_err(|e| StrataError::Io {
            path: path_ref.display().to_string(),
            message: e.to_string(),
        })?;
        let parsed: Self =
            serde_json::from_str(&raw).map_err(|e| StrataError::ProjectFormat {
                reason: format!(".darch JSON 파싱 오류: {e}"),
            })?;
        Ok(parsed.sanitize_clamped())
    }

    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<(), StrataError> {
        let path_ref = path.as_ref();
        let clean = self.clone().sanitize_clamped();
        let json_str =
            serde_json::to_string_pretty(&clean).map_err(|e| StrataError::ProjectFormat {
                reason: format!(".darch JSON 직렬화 오류: {e}"),
            })?;
        fs::write(path_ref, json_str).map_err(|e| StrataError::Io {
            path: path_ref.display().to_string(),
            message: e.to_string(),
        })
    }
}
