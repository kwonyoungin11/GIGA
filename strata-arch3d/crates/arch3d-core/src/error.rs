use serde::{Deserialize, Serialize};
use thiserror::Error;

/// `STRATA · 디자인 기준` (p.3, p.6) 규격에 따른 3단 문답형 오류 구조체.
/// 모든 사용자 대면 오류 창(`S · 폭 460px`)은 반드시 아래 3가지 질문에 답해야 한다:
/// 1. `what_failed`: 무엇이 안 됐나
/// 2. `why_it_happened`: 왜 그런가
/// 3. `how_to_fix`: 어떻게 하나
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreePartErrorInfo {
    pub what_failed: String,
    pub why_it_happened: String,
    pub how_to_fix: String,
    pub technical_detail: Option<String>,
}

impl ThreePartErrorInfo {
    pub fn new(
        what_failed: impl Into<String>,
        why_it_happened: impl Into<String>,
        how_to_fix: impl Into<String>,
    ) -> Self {
        Self {
            what_failed: what_failed.into(),
            why_it_happened: why_it_happened.into(),
            how_to_fix: how_to_fix.into(),
            technical_detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.technical_detail = Some(detail.into());
        self
    }
}

#[derive(Debug, Error)]
pub enum Arch3dError {
    #[error("{what} — 원인: {why} — 해결: {how}")]
    UserFacing {
        what: String,
        why: String,
        how: String,
    },

    #[error("파일 입출력 오류 ({path}): {message}")]
    Io { path: String, message: String },

    #[error("3D 메쉬 파싱 오류 ({path}): {reason}")]
    MeshParse { path: String, reason: String },

    #[error("빈 메쉬 또는 유효하지 않은 위상: 정점 {vertices}개, 삼각면 {faces}개")]
    EmptyOrInvalidMesh { vertices: usize, faces: usize },

    #[error("구연/저부 원호 복원 불가: 잔존 호 각도 {arc_deg:.1}° (최소 기준 {min_req_deg:.1}°)")]
    InsufficientRimArc { arc_deg: f64, min_req_deg: f64 },

    #[error(".ap2 / .darch 프로젝트 형식 오류: {reason}")]
    ProjectFormat { reason: String },

    #[error("내보내기 검수 막는 항목 발생 ({blocking_count}건): {summary}")]
    ExportBlocked {
        blocking_count: usize,
        summary: String,
    },
}

impl Arch3dError {
    /// 모든 `Arch3dError`를 `STRATA` 표준 3단 오류 창(`폭 460px`) 데이터로 변환한다.
    pub fn to_three_part(&self) -> ThreePartErrorInfo {
        match self {
            Self::UserFacing { what, why, how } => ThreePartErrorInfo::new(what, why, how),
            Self::Io { path, message } => ThreePartErrorInfo::new(
                "유물 파일이나 프로젝트 파일을 읽거나 저장하지 못했습니다.",
                format!("경로 '{path}' 접근 중 시스템 입출력 오류가 발생했습니다 ({message})."),
                "파일이 다른 프로그램에서 열려 있는지 확인하거나 다른 폴더를 선택해 다시 시도하세요.",
            ),
            Self::MeshParse { path, reason } => ThreePartErrorInfo::new(
                "3D 스캔 파일의 정점·삼각면 구조를 해석하지 못했습니다.",
                format!("'{path}' 파일 내부 형식이 손상되었거나 지원하지 않는 구문입니다 ({reason})."),
                "Artec Studio 또는 스캐너에서 표준 .obj / .ply / .stl 형식으로 다시 내보낸 뒤 열어 주세요.",
            ),
            Self::EmptyOrInvalidMesh { vertices, faces } => ThreePartErrorInfo::new(
                "유효한 3D 표면 삼각면을 찾을 수 없습니다.",
                format!("로드된 파일에 정점 {vertices}개, 삼각면 {faces}개만 존재하여 단면과 탁본을 생성할 수 없습니다."),
                "점군(Point Cloud)만 있는 경우 메쉬화(Triangulation)를 수행한 .obj 또는 .ply 파일을 열어 주세요.",
            ),
            Self::InsufficientRimArc {
                arc_deg,
                min_req_deg,
            } => ThreePartErrorInfo::new(
                "구연부 원호가 너무 짧아 전체 복원구경을 확정하지 않았습니다.",
                format!("남아 있는 구연단 호 각도가 {arc_deg:.1}°로, 신뢰 가능한 원 피팅 최소 기준({min_req_deg:.1}°)보다 작습니다."),
                "『동체·극소편 3단 병렬 도판(외면-단면-내면)』으로 전환하거나, ① 바닥 정하기에서 구연 위에 3점을 직접 찍으세요.",
            ),
            Self::ProjectFormat { reason } => ThreePartErrorInfo::new(
                "실측 작업 파일(.ap2 / .darch)을 불러오지 못했습니다.",
                format!("작업 파일의 JSON 또는 행렬 데이터 구조에 문제가 있습니다 ({reason})."),
                "원본 3D 스캔 파일(.obj/.ply)을 새로 열어 정위치 맞춤부터 진행하거나 백업 파일을 확인하세요.",
            ),
            Self::ExportBlocked {
                blocking_count,
                summary,
            } => ThreePartErrorInfo::new(
                format!("검수에서 막는 항목 {blocking_count}건이 발견되어 내보내기를 멈췄습니다."),
                summary.clone(),
                "검수 창의 [바로 고치기] 버튼을 눌러 바닥 수평·단면선·축척을 확정한 뒤 다시 내보내세요.",
            ),
        }
    }
}
