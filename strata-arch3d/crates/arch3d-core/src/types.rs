use serde::{Deserialize, Serialize};

/// 6대 고고학 출토 유물 대분류 (`DESIGN_SPEC.md` Part 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArtifactCategory {
    /// 도법 ①: 완형 토기·도자기 (좌 외면·탁본 │ 우 단면 + 외·내저면 + 횡단면)
    CompleteVessel,
    /// 도법 ②·③·④: 편조각 및 특수부위 (구연편·저부편·동체부편·파수/주구편·뚜껑·고배투창편)
    Potsherd,
    /// 도법 ⑤·⑥: 구슬·옥류 및 소형 장신구 (환옥·소옥·절구옥·관옥·곡옥·다면옥·늘인/감은구슬)
    BeadJade,
    /// 도법 ⑦: 기와·막새·전돌 (수키와·암키와·수막새·암막새)
    RoofTile,
    /// 도법 ⑧-A: 석기 (타제석기·마제석기·반달돌칼·석촉)
    Lithic,
    /// 도법 ⑧-B: 부식 금속기 (철촉·철모·환두대도·철부·청동기 — 원심/부식층/목질흔 3층 분리)
    CorrodedMetal,
}

impl ArtifactCategory {
    pub fn label_kr(self) -> &'static str {
        match self {
            Self::CompleteVessel => "완형 토기·도자기",
            Self::Potsherd => "편조각·특수부위",
            Self::BeadJade => "구슬·옥류",
            Self::RoofTile => "기와·막새",
            Self::Lithic => "석기",
            Self::CorrodedMetal => "부식 금속기",
        }
    }
}

/// 편조각(`Potsherd`) 6대 세부 기종 분류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SherdSubtype {
    /// 구연부편 (입술·구연단 잔존)
    RimSherd,
    /// 저부·굽·대각편 (바닥·굽 접지환 잔존)
    BaseFootSherd,
    /// 동체부 소편 (구연·바닥 없이 물레속선 Rills 잔존)
    BodySherd,
    /// 파수(손잡이)·주구(귀때)·영락 부착편 (접합식 vs 삽입촉 파선 표기)
    HandleSpoutSherd,
    /// 뚜껑(개, Lid) 편 (꼭지 상단 · 드림턱 하단 정위치)
    LidSherd,
    /// 고배 대각 투창편 (칼질 절단 열린 면 2줄 실선 표기)
    FenestratedStandSherd,
}

/// 편조각 잔존 원호 각도($\Delta\theta$)에 따른 3단계 적응형 도판 전략 (`DESIGN_SPEC.md` 2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SherdPlateStrategy {
    /// 잔존 호 $\Delta\theta \ge 45^\circ$ (잔존율 $\ge 12.5\%$): 완전 복원 도판 (실선 구경선 + 중심축)
    FullReconstruction,
    /// 잔존 호 $20^\circ \le \Delta\theta < 45^\circ$: 추정 복원 도판 (추정 파선 `dash` 구경선 + `(복원구경)` 괄호 표기)
    EstimatedDashedReconstruction,
    /// 잔존 호 $\Delta\theta < 20^\circ$ 또는 동체부편: 억지 지름 복원 없이 `[① 외면+탁본 ── ② 수직 회전단면 ── ③ 내면+탁본]` 3단 병렬 도판
    ThreeColumnParallel,
}

impl SherdPlateStrategy {
    pub fn from_arc_angle_deg(arc_deg: f64, is_body_only: bool) -> Self {
        if is_body_only || arc_deg < 20.0 {
            Self::ThreeColumnParallel
        } else if arc_deg < 45.0 {
            Self::EstimatedDashedReconstruction
        } else {
            Self::FullReconstruction
        }
    }
}

/// 파수·주구 부착 기법 분류 (단면 내부 파선 제도용).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HandleAttachmentKind {
    /// 기벽 외면에 맞붙인 접합식 (부착면 파선)
    SurfaceButtJoint,
    /// 기벽에 구멍을 뚫어 촉을 꽂은 삽입식 (투공 삽입촉 파선)
    InsertedTenonJoint,
}

/// 구슬·옥류(`BeadJade`) 길이/직경 비율($L/D$) 및 형태 기반 6대 세부 분류 (`DESIGN_SPEC.md` 2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BeadSubtype {
    /// 환옥 (둥근옥): $0.95 \le L/D < 1.50$
    AnnularBead,
    /// 소옥 (유리구슬): $0.46 \le L/D \le 0.94$
    SmallBead,
    /// 절구옥 / 평옥 (납작구슬): $L/D \le 0.45$
    MortarDiscBead,
    /// 관옥 (대롱옥): $L/D \ge 1.50$ 원통/각주형
    TubularBead,
    /// 곡옥 (굽은옥): C자형 비대칭 만곡 + 두부 각목문(丁字頭)
    CommaJade,
    /// 다면옥 (14면옥 / 18면옥 등 다면 연마옥)
    FacetedPolyhedralBead,
}

impl BeadSubtype {
    /// 길이($L$)와 외경($D$) 비율 및 형태 플래그로 구슬·옥류 세부 기종을 자동 판정한다.
    pub fn classify_by_ratio(
        length_mm: f64,
        outer_diameter_mm: f64,
        is_c_curved: bool,
        facet_count: usize,
    ) -> Self {
        if is_c_curved {
            return Self::CommaJade;
        }
        if facet_count >= 12 {
            return Self::FacetedPolyhedralBead;
        }
        let ratio = if outer_diameter_mm.abs() > 1e-6 {
            length_mm / outer_diameter_mm
        } else {
            1.0
        };
        if ratio >= 1.50 {
            Self::TubularBead
        } else if ratio >= 0.95 {
            Self::AnnularBead
        } else if ratio >= 0.46 {
            Self::SmallBead
        } else {
            Self::MortarDiscBead
        }
    }

    pub fn label_kr(self) -> &'static str {
        match self {
            Self::AnnularBead => "환옥(둥근옥)",
            Self::SmallBead => "소옥(유리구슬)",
            Self::MortarDiscBead => "절구옥(평옥)",
            Self::TubularBead => "관옥(대롱옥)",
            Self::CommaJade => "곡옥(굽은옥)",
            Self::FacetedPolyhedralBead => "다면옥",
        }
    }
}

/// 유리구슬 제작기법 분류 (기포열·말린 흔적 방향 vs 천공축 비교).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BeadFabricationKind {
    /// 늘인구슬 (Drawn Bead): 기포 배열 및 줄무늬가 천공축과 평행
    DrawnParallel,
    /// 감은구슬 (Wound Bead): 유리 융액을 심봉에 감아 흔적이 천공축에 수직 회전
    WoundCircumferential,
    /// 석재·옥 연마 성형 (마노·벽옥·비취·수정)
    LapidaryCarved,
}

/// 한국 고고학 표준 구슬·옥류 4대 천공 단면 유형 (`Ⅰa`, `Ⅰb`, `Ⅰb'`, `Ⅱb`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BeadDrillType {
    /// `Ⅰa형`: 원통형 일방향 천공 (금속 관상추 사용, 상·하 내벽 수직 평행선)
    TypeIa,
    /// `Ⅰb형`: 원추형 일방향 천공 (봉상추+연마재, 입구 넓고 출구 좁은 깔때기형 + 출구 원추상 박리흔)
    TypeIb,
    /// `Ⅰb'형`: 미관통 맹공 (뚫다 멈춘 U자형 홈)
    TypeIbBlind,
    /// `Ⅱb형`: 양방향 맞조르기 천공 (대향 원추 모래시계형 + 중앙 교차 단차 Δe 및 굴절선)
    TypeIIb,
}

impl BeadDrillType {
    pub fn code_str(self) -> &'static str {
        match self {
            Self::TypeIa => "Ⅰa",
            Self::TypeIb => "Ⅰb",
            Self::TypeIbBlind => "Ⅰb'",
            Self::TypeIIb => "Ⅱb",
        }
    }

    pub fn label_kr(self) -> &'static str {
        match self {
            Self::TypeIa => "Ⅰa형 (원통형 일방향 천공 · 관상추 평행 내벽)",
            Self::TypeIb => "Ⅰb형 (원추형 일방향 천공 · 깔때기형 + 출구 박리흔)",
            Self::TypeIbBlind => "Ⅰb'형 (미관통 맹공 · U자형 중단 홈)",
            Self::TypeIIb => "Ⅱb형 (양방향 맞조르기 천공 · 모래시계형 + 중앙 교차 단차)",
        }
    }
}

/// `Structure-from-Sherds++` 정점/삼각면 3영역 분할 레이블.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SurfaceRegion {
    /// 원래 기벽 외면 (청색 시각화)
    OuterWall,
    /// 원래 기벽 내면 (녹색 시각화)
    InnerWall,
    /// 깨진 파단면 (적색 시각화 — 원 피팅·회전축 계산에서 100% 배제!)
    FractureZone,
}

/// 부식 금속기 3층 단면 분리 레이어 (`DESIGN_SPEC.md` 2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetalSectionLayer {
    /// ① 본래 금속 원심 (X-Ray/역산 코어: 굵은 실선 0.35mm · 흑색/해칭)
    OriginalMetalCore,
    /// ② 부식 팽창층·수포 (3D 외형 표면: 파선 dash 표기)
    CorrosionBlisterLayer,
    /// ③ 고착 유기물층 (슴베·자루 광물화 목질흔 `목질흔.ai` 나뭇결 세선 · 직물흔)
    AdheredOrganicWoodTextile,
}

/// 실측 도판 축척 프리셋 (소형 구슬 `10:1` 초확대 ~ 대형 기와/옹기 `1:4` 축소).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScalePreset {
    Micro10To1,
    Micro5To1,
    Micro4To1,
    Micro3To1,
    Micro2To1,
    Full1To1,
    Half1To2,
    Third1To3,
    Quarter1To4,
}

impl ScalePreset {
    pub fn ratio(self) -> f64 {
        match self {
            Self::Micro10To1 => 10.0,
            Self::Micro5To1 => 5.0,
            Self::Micro4To1 => 4.0,
            Self::Micro3To1 => 3.0,
            Self::Micro2To1 => 2.0,
            Self::Full1To1 => 1.0,
            Self::Half1To2 => 0.5,
            Self::Third1To3 => 1.0 / 3.0,
            Self::Quarter1To4 => 0.25,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Micro10To1 => "10:1",
            Self::Micro5To1 => "5:1",
            Self::Micro4To1 => "4:1",
            Self::Micro3To1 => "3:1",
            Self::Micro2To1 => "2:1",
            Self::Full1To1 => "1:1",
            Self::Half1To2 => "1:2",
            Self::Third1To3 => "1:3",
            Self::Quarter1To4 => "1:4",
        }
    }

    /// 축척에 맞는 표준 축척 막대 길이(mm) 및 표시 텍스트 반환.
    /// 구슬·옥류 확대 축척(`2:1~10:1`)에서는 `5 mm` / `10 mm` 미세 눈금 막대를 사용한다.
    pub fn standard_scale_bar_mm(self) -> (f64, &'static str) {
        match self {
            Self::Micro10To1 | Self::Micro5To1 => (5.0, "5 mm"),
            Self::Micro4To1 | Self::Micro3To1 | Self::Micro2To1 => (10.0, "10 mm (1 cm)"),
            Self::Full1To1 | Self::Half1To2 => (50.0, "5 cm"),
            Self::Third1To3 | Self::Quarter1To4 => (100.0, "10 cm"),
        }
    }
}

/// 3차원 경계 상자 (단위: mm).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Aabb3 {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Aabb3 {
    pub const EMPTY: Self = Self {
        min: [f64::INFINITY; 3],
        max: [f64::NEG_INFINITY; 3],
    };

    pub fn include_point(&mut self, p: [f64; 3]) {
        for (i, &val) in p.iter().enumerate() {
            if val < self.min[i] {
                self.min[i] = val;
            }
            if val > self.max[i] {
                self.max[i] = val;
            }
        }
    }

    pub fn union(&self, other: &Self) -> Self {
        Self {
            min: [
                self.min[0].min(other.min[0]),
                self.min[1].min(other.min[1]),
                self.min[2].min(other.min[2]),
            ],
            max: [
                self.max[0].max(other.max[0]),
                self.max[1].max(other.max[1]),
                self.max[2].max(other.max[2]),
            ],
        }
    }

    pub fn extents(&self) -> [f64; 3] {
        if self.min[0] > self.max[0] {
            return [0.0, 0.0, 0.0];
        }
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }

    pub fn center(&self) -> [f64; 3] {
        [
            (self.min[0] + self.max[0]) * 0.5,
            (self.min[1] + self.max[1]) * 0.5,
            (self.min[2] + self.max[2]) * 0.5,
        ]
    }
}

/// 4×4 강체/아핀 변환 행렬 (`Arch3D .ap2`의 `TransfomMatrix [81, m0..m15]`와 1:1 호환).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Mat4 {
    /// Row-major 16 elements (`m[row * 4 + col]`)
    pub m: [f64; 16],
}

impl Default for Mat4 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mat4 {
    pub const IDENTITY: Self = Self {
        m: [
            1.0, 0.0, 0.0, 0.0, //
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ],
    };

    pub fn transform_point(&self, p: [f64; 3]) -> [f64; 3] {
        [
            self.m[0] * p[0] + self.m[1] * p[1] + self.m[2] * p[2] + self.m[3],
            self.m[4] * p[0] + self.m[5] * p[1] + self.m[6] * p[2] + self.m[7],
            self.m[8] * p[0] + self.m[9] * p[1] + self.m[10] * p[2] + self.m[11],
        ]
    }

    pub fn transform_normal(&self, n: [f64; 3]) -> [f64; 3] {
        let rx = self.m[0] * n[0] + self.m[1] * n[1] + self.m[2] * n[2];
        let ry = self.m[4] * n[0] + self.m[5] * n[1] + self.m[6] * n[2];
        let rz = self.m[8] * n[0] + self.m[9] * n[1] + self.m[10] * n[2];
        let len = (rx * rx + ry * ry + rz * rz).sqrt();
        if len > 1e-12 {
            [rx / len, ry / len, rz / len]
        } else {
            [0.0, 0.0, 1.0]
        }
    }
}
