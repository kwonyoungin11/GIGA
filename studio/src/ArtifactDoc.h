// DongArch3D Studio — 유물 문서: 메시 로드(Artec PLY/OBJ+텍스처/WRL/STL) → 부유
// 노이즈 제거·단위 환산 → ① 바닥 정하기(들어온 자세 그대로 → 사람이 돌려 놓고
// 확정, 좌표: mm · +Z 위 · 바닥판 Z = 0) → 단면·외곽·5대 치수. 계산은 전부
// K-Arch 엔진(GigaMesh/gui/src/dongarch/karch)에 맡기고, 여기서는 순서와 Artec
// 특화 보정 상태만 관리한다.
#pragma once
#include "DimensionAnalyzer.h"
#include "MeshSlicer.h"
#include "Rubbing.h"
#include "SectionComposer.h"
#include "Silhouette.h"

#include <QColor>
#include <QString>
#include <vector>

namespace studio {

namespace KA = DongArch::KArch;

enum class SlotKind { Vertical, Horizontal };

struct SectionSlot {
  QString name; // "A–A′ 정면 실측도"
  QString meta; // "종단면 · 184.5 mm" (좌/우 배치는 도판 머리에 표시)
  QString note; // "보고서 주 도판 (1:1)"
  SlotKind kind = SlotKind::Vertical;
  double angleDeg = 0.0; // Vertical: 정면 회전각
  double z = 0.0;        // Horizontal: 절단 높이 (mm)
};

enum class HeightRef { Base, Rim }; // B: 바닥 0 mm, T: 구연부 0 mm

enum class BaseTiltMode { RimHorizontal, BaseHorizontal };

struct FloorCandidate {
  int id = 1;
  QString name;
  QString desc;
  double flatness = 0.0; // % (0 ~ 100)
  double areaCm2 = 0.0;  // cm2
  double cogScore = 0.0;
  double totalScore = 0.0;
  KA::Point3 normal{0, 0, 1};
  KA::Point3 center{0, 0, 0};
  bool recommended = false;
};

struct RimGapInfo {
  std::vector<double> anglesDeg;
  std::vector<double> gapMm;
  double maxGapMm = 0.0;
  double contactSpanDeg = 0.0;
  double gapSpanDeg = 0.0;
};

struct AngleRecommendation {
  int rank = 1;
  double angleDeg = 0.0;
  double score = 0.0;
  QString why;
  bool star = false;
};

struct CutCheckStatus {
  bool closedLoop = true;
  int breakCount = 0;
  double minWall = 0.0, maxWall = 0.0;
  double rimDiff = 0.0;
  bool isThickAnomaly = false;
  bool passesChip = false;
  bool passesCrack = false;
  QString statusText;
};

struct AngleStripCell {
  double angleDeg = 0.0;
  QString statusLabel;
  bool isRecommended = false;
  bool hasError = false;
  bool hasWarn = false;
  std::vector<KA::Point2> outerPts;
  std::vector<KA::Point2> innerPts;
  std::vector<KA::Point2> extPts;
};

struct Dims5 {
  double height = 0, rim = 0, belly = 0, bellyZ = 0, base = 0, meanWall = 0;
  bool ok = false;
};

// ① 바닥 정하기: 바닥판(Z = 0 평면)의 고정 월드축
enum class PoseAxis { X, Y, Z };

// 도판에서 단면이 놓이는 쪽. D5(2026-10-17 확정): 기본 Right = 외면 왼쪽·단면
// 오른쪽(국내 보고서 표준), Left = R 로 좌/우 반전(같은 단면의 거울상 = 도판을
// 뒤에서 본 모양). 계산값은 바꾸지 않는다.
enum class SectionSide { Right, Left };

// ② 막으로 자르기 — 막은 보는 사람 앞 Y=0 면에 고정, 유물이 +Z 회전판처럼 돈다.
// Y 막 = 정면에서 본 종단면(0°), X 막 = 옆에서(90°). Z 막은 뒤 작업.
enum class CutAxis { Y, X };

// Y=0 단면 한 벌 (자연 좌표: x ≥ 0 = 단면 쪽, K-Arch 좌표 x=R, y=H)
struct SectionLines {
  std::vector<std::vector<KA::Point2>> loops;    // 실제 Y=0 절단 고리
  std::vector<std::vector<KA::Point2>> solids;   // 미스캔 보정 시 실선 구간
  std::vector<std::vector<KA::Point2>> estInner; // 미스캔/홀필 추정 내벽선
  double estZMin = -1.0, estZMax = -1.0;
  double wallMm =
      -1.0; // 미스캔 보정 경로가 정한 기벽 두께, 그 경로가 아니면 −1
};

class ArtifactDoc {
public:
  // Feature 13: 단위 확인 모달 & 잠금 상태 머신
  bool isUnitConfirmed() const { return unitConfirmed_; }
  void setUnitConfirmed(bool c) { unitConfirmed_ = c; }
  void confirmUnit(double factor = 1.0);
  double maxDimension() const;
  double averageEdgeLength() const;

  // Feature 14: 바닥 후보 평면 자동 판별
  std::vector<FloorCandidate> floorCandidates() const;
  void applyFloorCandidate(int index);

  // Feature 15: 구연 수평(T) vs 바닥 수평(B)
  BaseTiltMode baseTiltMode() const { return baseTiltMode_; }
  void setBaseTiltMode(BaseTiltMode m);
  bool isLying() const;

  // Feature 16 & 17: 구연부편(Sherd) 모드 & 틈 보기 & 원호판 가상 중심축
  bool isSherdMode() const { return sherdMode_; }
  void setSherdMode(bool on);
  RimGapInfo rimGapChart() const;
  double estimatedRimDiameter() const;
  double estimatedRimError() const;
  double rimArcDegrees() const;

  // Feature 20: 추천 각도 3종 엔진 & 자석 스냅
  std::vector<AngleRecommendation> recommendAngles() const;
  double angleDefectScore(double angleDeg) const;
  double snapAngle(double deg, double tol = 2.0) const;
  std::vector<AngleStripCell> angleStripCells() const;

  // Feature 21: 실시간 단면 상태 검사선
  CutCheckStatus checkCutStatus() const;

  // 슬롯 높이·단면 빠른 조작 (② 막으로 자르기 · ③ 도면 편집)
  void setSlotHorizontalZ(int slotIdx, double zMm);
  int addVerticalSlotAt(double angleDeg);
  int addHorizontalSlotAt(double zMm);

  // 3D 표면 1클릭 절단 위치 지정 (내가 여기를 잘랐구나 확인 마커 포함)
  bool alignCutToSurfacePoint(const KA::Point3 &worldPt);
  bool hasCutMarker() const { return hasCutMarker_; }
  KA::Point3 cutMarker3D() const;
  double cutMarkerZ() const { return cutMarkerZ_; }

  // 3D 표면 1클릭 문양 탁본 부위 선택 (정밀 문양 먹탁 영역)
  bool selectRubbingSpot(const KA::Point3 &worldPt);
  bool hasRubbingSpot() const { return hasRubbingSpot_; }
  KA::Point3 rubbingCenter3D() const;
  double rubbingAzimuthDeg() const;
  double rubbingZ() const { return rubbingZ_; }
  double rubbingSpanW() const { return rubbingSpanW_; }
  double rubbingSpanH() const { return rubbingSpanH_; }

  // [JOB-014] 탁본 범위 (확정 자세 기준 방위각 범위 × 높이 범위) — 끌어서 고른 그대로
  RubbingRegion rubbingRegion() const;
  void setRubbingRegion(const RubbingRegion &r);
  void setRubbingFullTurn(); // 같은 높이 범위를 한 바퀴(360°) 펼친 문양 전개도
  // [JOB-015] 원클릭 D-Tak 프리셋 & 원통/평면 탁본 투영 모드
  void setRubbingPreset(RubbingPreset p);
  RubbingPreset rubbingPreset() const { return rubbingPreset_; }
  void setRubbingProjMode(RubbingProjMode m);
  RubbingProjMode rubbingProjMode() const { return rubbingProjMode_; }
  // [JOB-015] 작업 이력(.darch JSON — Arch3D .ap2 상위호환) 저장·복원
  QString defaultHistoryPath() const;
  bool saveHistory(const QString &darchPath = QString()) const;
  bool loadHistory(const QString &darchPath = QString());
  // 원통 전개 요철 탁본 (캐시 — 범위·배율·자세가 바뀔 때만 다시 계산)
  const UnrolledRubbing &unrolledRubbing(double pxPerMm) const;

  // 들어온 자세 그대로 보관한다(확정 전 — 단면·치수·도판을 계산하지 않음).
  // 경로·스캐너 프로파일·단위 메모는 바꾸지 않는다(loadFile/loadSample 몫).
  void loadMesh(KA::MeshData m, std::vector<QRgb> colors, const QString &name);
  bool loadFile(const QString &path,
                QString *err); // 들어온 자세 그대로 (자동 세우기 안 함)
  // 합성 토기(항아리) — Artec 컬러·타날문 포함 기본 예제.
  // confirm=true: 자동 세우기 → 확정 (예전과 같은 결과), false: 들어온 자세
  void loadSample(bool confirm = true);
  // 좁은 아가리 토기(내벽 미스캔·Artec 홀필 마개 보정 예제), confirm 은 위와
  // 같음
  void loadArtecBlindSample(bool confirm = true);
  bool isLoaded() const { return !mesh_.vertices.empty(); }

  // ---- ① 바닥 정하기 (자세 단계) ----
  // 현재 자세의 바운딩 박스 중심을 지나는 고정 월드축 기준 오른손 법칙 회전,
  // 뒤에 바닥에 내려놓기. 90° 정수배는 정수 행렬(누적 오차 없음).
  void rotatePose(PoseAxis a, double deg);
  void flipPose();    // 뒤집기 = X 축 180°
  void resetPose();   // 처음 자세로: 읽은 정점 좌표 그대로 (접지 안 함)
  void dropToFloor(); // 바닥에 내려놓기: z 만 평행이동해 bboxMin.z = 0
  void centerOnFloor(); // 바닥판 중앙(XY=0) + 바닥(Z=0) 맞춤
  void alignNormalToFloor(const KA::Point3 &downNormal);
  bool alignSurfacePointToFloor(const KA::Point3 &worldPt,
                                double radiusMm = 15.0);
  void clearFloorPickPoints();
  int addFloorPickPoint(const KA::Point3 &pt);
  const std::vector<KA::Point3> &floorPickPoints() const {
    return floorPickPts_;
  }
  // 자동 세우기: 구연부 PCA → 회전 중심축(ODR) 정밀화 → 축을 R 0, 바닥을 H 0
  // 으로. 확정은 하지 않는다.
  void autoAlign();
  // 이 자세로 확정: (자동 세우기 직후가 아니면) 중심선을 원점으로, 접지,
  // 단면·외곽·치수·슬롯 계산
  void confirmPose();
  bool poseConfirmed() const { return poseConfirmed_; }
  double heightAboveFloor() const { return mesh_.bboxMin.z; } // mm
  bool isGrounded() const; // |bboxMin.z| ≤ 0.3 mm (디자인 v2 접지 판정 권고값)
  // 직전 자세 조작이 자동 세우기이고 회전 중심축을 찾았는지
  bool autoCentered() const { return centered_ && pendingAligned_; }
  // 접지 상태 문구 (ctx 줄·3D 정보 줄 공용): "바닥판에서 45.0 mm 위" 등
  QString groundNote() const;

  // 정면각 θ (deg, [−180, 180]): 확정 자세를 +Z 오른손 법칙으로 θ 돌린(위에서
  // 보아 반시계 — 정면(−Y)의 면이 화면 오른쪽 +X 로 가는) 메시의 Y=0 면 x ≥ 0
  // 반쪽이 단면. 0° = 확정 자세. 확정 전·미로드는 무시. 단면·외곽은 원본에서
  // 다시 계산하고 5대 치수·잰 기벽은 확정 때 값(+Z 회전은 바꾸지 않는다 — 두
  // 고리를 못 잡은 단일 쉘의 추정 기벽만 각 단면을 따름).
  void setFrontAngle(double deg);
  void alignCut(CutAxis a); // 막 맞춤: Y → 0°, X → 90°
  // 실시간 미리보기: 확정 전·미로드 무시. LOD 만 돌려 Y=0 단면 1장 → 미리보기
  // 버퍼. 외곽·치수·두께·estimatedWallMm·슬롯·원본 메시·frontAngle 은 그대로.
  void previewFrontAngle(double deg);
  void
  commitPreview(); // 미리보기 중이면 setFrontAngle(previewAngle()), 아니면 무시
  bool isPreviewing() const { return previewing_; }
  double previewAngle() const {
    return previewing_ ? previewAngle_ : frontAngle_;
  }
  void setHeightRef(HeightRef r) { heightRef_ = r; }
  void setSimplifyTol(double mm) { simplifyTol_ = mm; }
  void setSmooth(bool on) { smooth_ = on; }

  // 도판 좌/우 (D5). 다시 계산하지 않는다 — 그리기 부호만. 파일을 바꿔 열어도
  // 유지(한 세션 동안의 사용자 선택), 되돌리기에 쌓지 않는다.
  void setSectionSide(SectionSide s) { side_ = s; }
  SectionSide sectionSide() const { return side_; }
  void toggleSectionSide() {
    side_ =
        side_ == SectionSide::Right ? SectionSide::Left : SectionSide::Right;
  }
  double sectionSign() const { // Right → +1.0, Left → −1.0
    return side_ == SectionSide::Right ? 1.0 : -1.0;
  }

  // Artec 스캐너 음영지대(내벽 미스캔·Watertight 홀필 마개) 기벽 자동 추정 보정
  void setInferInnerWall(bool on);
  bool inferInnerWall() const { return inferInnerWall_; }
  void setEstimatedWallMm(double mm);
  double estimatedWallMm() const { return estimatedWallMm_; }
  bool hasEstimatedInner() const {
    return !shown().estInner.empty();
  } // 미리보기 따름

  // 수동 단위 배율 보정 (예: 10.0 = cm→mm, 0.1 = mm→cm)
  void rescaleUnits(double factor);

  const KA::MeshData &mesh() const {
    return mesh_;
  } // 원본 정밀 메시 (단면·D-Tak 용)
  const KA::MeshData &lodMesh() const {
    return previewing_ ? previewLod_ : lodMesh_;
  } // 3D 보조창 고속 회전용 경량 LOD 메시 (미리보기 중이면 미리보기 각)
  const KA::MeshData &meshMaster() const { return base_; }
  const KA::MeshData &meshProxy() const { return lodMesh(); }
  const KA::MeshBVH &bvhMaster() const { return bvhMaster_; }
  const std::vector<QRgb> &vertexColors() const { return baseColors_; }
  const std::vector<QRgb> &lodVertexColors() const { return lodColors_; }
  bool hasColor() const {
    return !baseColors_.empty() && baseColors_.size() == mesh_.vertices.size();
  }
  QString scannerProfile() const { return scannerProfile_; }
  QString unitNote() const { return unitNote_; }
  int removedNoiseFaces() const { return removedNoiseFaces_; }

  const KA::SectionResult &section() const { return sec_; }
  const KA::Dimensions &rawDims() const { return dims_; }
  // 5대 치수 — 확정 때 원본 메시(25만 면 초과도 LOD 아님)에서 한 번 잰다.
  // 정면각(+Z 회전)은 치수를 바꾸지 않는다 (예외: 확정 때 두 고리를 못 잡은
  // 단일 쉘이면 평균 기벽 = 각 단면이 갱신하는 추정값 estimatedWallMm)
  const Dims5 &dims() const { return d5_; }
  // 그리는 외곽선의 원 선 = 막과 수직인 실루엣의 외면 반쪽(x ≤ 0, 원본 mesh_,
  // 평활 없음)
  const std::vector<KA::Point2> &orthoOutline() const { return ortho_; }
  // 지금 보이는 단면의 원 선(시험·판독용)
  const SectionLines &sectionLines() const { return shown(); }

  // 화면/내보내기용 선 (간략화·곡선 옵션 적용, 도면 좌표 x=R, y=H). D5 기본
  // (Right): 외곽 왼쪽 x ≤ 0 · 단면 오른쪽 x ≥ 0, Left: x 부호만 반전(거울상).
  // 부호는 간략화·곡선 다듬기가 모두 끝난 뒤 마지막에 x 에 sectionSign() 을
  // 곱한다. 곡선은 원 선에서 간격/2 안이고 원 선의 높이 범위·축 쪽을 넘지 않는
  // 구간에만 — fitCurve.
  std::vector<std::vector<KA::Point2>> displaySection() const;
  // Artec 미스캔 보정 시 실제 스캔된 실선 구간(외벽·구연·상부내벽·바닥)
  std::vector<std::vector<KA::Point2>> displayScannedSolid() const;
  // Artec 미스캔/홀필 구간 추정 내벽선 (고고학 실측 표준: 빨강 파선 표기용)
  std::vector<std::vector<KA::Point2>> displayEstimatedInner() const;
  std::vector<KA::Point2> displayOutline() const;
  std::vector<std::pair<KA::Point2, KA::Point2>> displayHatch() const;
  std::vector<KA::Polyline3> horizontalSlice(double z) const;
  double wallThicknessAt(double z) const; // 해당 높이 기벽 두께, 없으면 -1
  double outerRadiusAt(double z) const;   // 해당 높이 외경 반경, 없으면 -1
  bool
  isWallEstimatedAt(double z) const; // 해당 높이가 Artec 미스캔 추정 구간인지

  QString name() const { return name_; }
  QString path() const { return path_; }
  bool aligned() const { return aligned_; }
  QString alignNote() const { return alignNote_; }
  double axisRmse() const { return axisRmse_; }
  double frontAngle() const { return frontAngle_; }
  HeightRef heightRef() const { return heightRef_; }
  double simplifyTol() const { return simplifyTol_; }
  bool smooth() const { return smooth_; }
  int pointCount() const; // 단면+외곽 점 수 (간략화 후)
  int rawPointCount() const;
  // 그린 선(간략화·곡선)이 원 선에서 떨어진 가장 큰 거리 (mm). 원 선 = 단면은
  // 막이 자른 선(미스캔 보정이면 실측 실선과 추정 내벽선 — 추정선도 포함),
  // 외곽선은 막과 수직인 실루엣의 외면 반쪽(orthoOutline). 꼭짓점 기준 양방향:
  // 그린 꼭짓점 → 원 꺾은선, 원 꼭짓점 → 그린 꺾은선. 미로드·확정 전 0.
  double maxDeviationMm() const;

  std::vector<SectionSlot> &slotList() { return slotList_; }
  const std::vector<SectionSlot> &slotList() const { return slotList_; }

private:
  void resetSlots();
  // measureDims: 5대 치수·기벽 두께를 원본에서 잰다(확정 때). 정면각만 바뀌면
  // false — 확정 때 값을 그대로 쓴다
  void recompute(bool measureDims = true);
  void unconfirm(); // 자세가 바뀜 → 확정 풀기·슬롯 비우기·계산 비우기
  // 회전 p' = c + R(p − c) 를 원본·LOD 에 함께 적용
  void rotateAbout(const double (&R)[3][3], const KA::Point3 &c);
  // 원본·LOD 를 같은 거리만큼 평행이동 (바운딩 박스 갱신)
  void shiftBoth(double dx, double dy, double dz);
  void buildSectionLoops(); // 확정 각 단면 (미리보기 지움)
  void clearPreview();
  // 화면에 보이는 단면: 미리보기 중이면 미리보기, 아니면 확정 각
  const SectionLines &shown() const {
    return previewing_ ? previewLines_ : lines_;
  }
  // 자연 좌표(K-Arch: 단면 x ≥ 0, 외곽 x < 0)의 화면용 선 — 좌/우 부호 적용 전
  std::vector<std::vector<KA::Point2>> sectionNatural() const;
  std::vector<std::vector<KA::Point2>> scannedSolidNatural() const;
  std::vector<std::vector<KA::Point2>> estimatedInnerNatural() const;
  // 원 선 → 그릴 선(자연 좌표): 간격/2 의 fitCurve — 표시·최대 편차 공용
  std::vector<KA::Point2> fitSectionLine(const std::vector<KA::Point2> &raw,
                                         bool closed) const; // 단면 sub 3
  std::vector<KA::Point2> outlineNatural() const;            // 외곽선 sub 4

  SectionLines lines_;        // 확정 각 Y=0 단면 (원본 메시)
  SectionLines previewLines_; // 미리보기 각 Y=0 단면 (LOD)
  KA::MeshData previewLod_;   // 미리보기 각으로 돌린 LOD
  bool previewing_ = false;
  double previewAngle_ = 0.0;

  std::vector<double> wallProfile_;
  double wallZ0_ = 0.0, wallStep_ = 1.0;
  double wallMean_ =
      -1.0; // 확정 때 잰 평균 기벽 (두 고리가 잡힌 10–90 % 높이), 없으면 −1

  KA::MeshData base_; // 정위치까지 적용된 메시 (정면 회전 전)
  KA::MeshData mesh_;
  KA::MeshData lodBase_;
  KA::MeshData lodMesh_;
  std::vector<QRgb> baseColors_;
  std::vector<QRgb> lodColors_;
  QString scannerProfile_;
  QString unitNote_;
  int removedNoiseFaces_ = 0;

  std::vector<KA::Point2>
      ortho_; // 그리는 외곽선의 원 선 = 막과 수직인 실루엣의 외면 반쪽(x ≤ 0,
              // 원본 mesh_, 평활 없음)
  KA::SectionResult sec_;
  KA::Dimensions dims_;
  Dims5 d5_;
  QString name_, path_, alignNote_;
  bool aligned_ = false;
  double axisRmse_ = 0.0;
  double frontAngle_ = 0.0;
  HeightRef heightRef_ = HeightRef::Base;
  double simplifyTol_ = 0.5;
  bool smooth_ = true;
  SectionSide side_ = SectionSide::Right; // D5 기본: 외면 왼쪽 · 단면 오른쪽
  bool inferInnerWall_ = true;   // Artec 미스캔 내벽/홀필 마개 자동 보정
  double estimatedWallMm_ = 6.8; // 추정 기벽 두께 (mm)
  std::vector<SectionSlot> slotList_;
  KA::MeshBVH bvhMaster_;

  // ① 바닥 정하기
  std::vector<KA::Point3> rawVertices_; // 들어온 자세의 정점 (면은 공유)
  KA::MeshData rawLod_;                 // 들어온 자세의 LOD
  std::vector<QRgb> rawLodColors_;
  std::vector<KA::Point3> floorPickPts_;
  bool poseConfirmed_ = false;
  bool centered_ = false; // 직전 자세 조작이 자동 세우기(중심 맞춤 끝남)
  // 자동 세우기 결과 — 확정 때 aligned_/alignNote_/axisRmse_ 로 옮긴다
  bool pendingAligned_ = false;
  QString pendingNote_;
  double pendingRmse_ = 0.0;

  void computeRimCircleFit();
  void autoInitRubbingSpot();

  bool unitConfirmed_ = true;
  BaseTiltMode baseTiltMode_ = BaseTiltMode::RimHorizontal;
  bool sherdMode_ = false;
  double estimatedRimDia_ = 0.0;
  double estimatedRimErr_ = 0.0;
  double rimArcDeg_ = 0.0;

  bool hasCutMarker_ = false;
  double cutMarkerZ_ = 0.0;
  bool hasRubbingSpot_ = false;
  double rubbingBaseAzimuthDeg_ = 215.0;
  double rubbingZ_ = 0.0;
  double rubbingSpanW_ = 54.0;
  double rubbingSpanH_ = 46.0;
  double rubbingAzSpanDeg_ = -1.0; // > 0: 사용자가 끌어 정한 방위각 폭 (≤ 0: 폭 mm 로 환산)
  RubbingPreset rubbingPreset_ = RubbingPreset::Standard;
  RubbingProjMode rubbingProjMode_ = RubbingProjMode::Cylindrical;

  // [JOB-015] 비파괴 미세 자세 조정용 작업 상태 보존 버퍼
  bool hasSavedWork_ = false;
  double savedFrontAngle_ = 0.0;
  std::vector<SectionSlot> savedSlots_;
  bool savedHasRubbingSpot_ = false;
  RubbingRegion savedRubbingReg_{};

  // [JOB-014] 전개 탁본 캐시 — 범위·배율·확정 자세가 같으면 다시 계산하지 않는다
  struct RubbingCache {
    bool valid = false;
    RubbingRegion region;
    double pxPerMm = 0.0;
    RubbingPreset preset = RubbingPreset::Standard;
    RubbingProjMode projMode = RubbingProjMode::Cylindrical;
    size_t vertexCount = 0;
    KA::Point3 bboxMin{}, bboxMax{}, firstVertex{};
    UnrolledRubbing result;
  };
  mutable RubbingCache rubbingCache_;
};

// 메시 m 의 Y=0 단면 x ≥ 0 반쪽 (+ inferInner 면 Artec 미스캔 내벽 추정, 씨앗
// 두께 wallSeedMm). angleDeg 가 주어지면 n = (sin theta, cos theta, 0) 가상
// 평면으로 Zero-Copy 회전 절단.
SectionLines sliceSectionY0(const KA::MeshData &m, bool inferInner,
                            double wallSeedMm, double angleDeg = 0.0,
                            const KA::MeshBVH *bvh = nullptr);

// 선 다듬기 유틸 (Douglas–Peucker, 편차 제한 Catmull–Rom). kept 가 있으면 남긴
// 점의 pts 인덱스를 차례로 넣는다(닫힘의 되감기 점 제외).
std::vector<KA::Point2> simplifyDP(const std::vector<KA::Point2> &pts,
                                   double tol, bool closed,
                                   std::vector<size_t> *kept = nullptr);
// 원 선 raw → 그릴 선: DP(tol) 뒤, smooth 면 DP 구간마다 Catmull–Rom 점 sub−1
// 개를 넣되 (가) 모두 raw 의 높이(y) 범위 안이고 raw 가 축 한쪽(모두 x ≥ 0 또는
// 모두 x ≤ 0)이면 그쪽에 있고 (나) 그 점들과 이 구간의 원 점이 서로 tol 안일
// 때만 — 아니면 그 구간은 DP 직선(실측 점을 잇는 간략 꺾은선). 곡선 점은
// 기준선(높이 범위)·중심선 너머로 나가지 않는다.
std::vector<KA::Point2> fitCurve(const std::vector<KA::Point2> &raw, double tol,
                                 bool closed, bool smooth, int sub);

} // namespace studio
