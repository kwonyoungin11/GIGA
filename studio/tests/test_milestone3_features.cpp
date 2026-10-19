// DongArch3D Studio — Milestone 3 (Features 13~33) Comprehensive Unit &
// Integration Test Step 1: 바닥 정하기 (Features 13~17) Step 2: 막으로 자르기
// (Features 18~23) Step 3: 도면 편집 & 내보내기 (Features 24~33)

#include "ArtifactDoc.h"
#include "ExportDialog.h"
#include "Plate.h"
#include "SheetView.h"
#include "SyntheticMeshGen.h"
#include "View3D.h"

#include <QDir>
#include <QFile>
#include <QGuiApplication>
#include <QXmlStreamReader>
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <vector>

using namespace studio;
namespace KA = DongArch::KArch;

static int fails = 0;
#define CHECK(cond, ...)                                                       \
  do {                                                                         \
    if (!(cond)) {                                                             \
      std::printf("  [FAIL] %s: ", #cond);                                     \
      std::printf(__VA_ARGS__);                                                \
      std::printf("\n");                                                       \
      ++fails;                                                                 \
    } else {                                                                   \
      std::printf("  [PASS] %s\n", #cond);                                     \
    }                                                                          \
  } while (0)

static bool near(double a, double b, double tol) {
  return std::fabs(a - b) <= tol;
}

int main(int argc, char **argv) {
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  qputenv("QT_QPA_PLATFORM", "offscreen");
  QGuiApplication app(argc, argv);
  const QString outDir =
      argc > 1 ? QString::fromLocal8Bit(argv[1]) : QDir::currentPath();

  std::printf("=== Milestone 3 (Features 13~33) Test Suite ===\n\n");

  // -------------------------------------------------------------
  // [Step 1: 바닥 정하기 (Features 13~17)]
  // -------------------------------------------------------------
  std::printf("--- Step 1: 바닥 정하기 ---\n");
  {
    ArtifactDoc doc;
    // Feature 13: 단위 확인 모달 & 상태 머신
    doc.loadSample(false); // unconfirmed pose
    CHECK(doc.isLoaded(), "doc must be loaded");
    CHECK(!doc.poseConfirmed(), "pose must be unconfirmed initially");

    // Check dimension metrics
    const double maxDim = doc.maxDimension();
    const double avgEdge = doc.averageEdgeLength();
    CHECK(maxDim > 100.0 && maxDim < 300.0, "maxDimension: %.2f mm", maxDim);
    CHECK(avgEdge > 0.1 && avgEdge < 10.0, "averageEdgeLength: %.2f mm",
          avgEdge);

    // Unit locking: when unit is unconfirmed, confirmPose should be guarded
    doc.setUnitConfirmed(false);
    CHECK(!doc.isUnitConfirmed(), "unit is unconfirmed");
    doc.confirmPose();
    CHECK(!doc.poseConfirmed(),
          "confirmPose must be locked when unit is unconfirmed");

    // Confirm unit with mm (factor 1.0)
    doc.confirmUnit(1.0);
    CHECK(doc.isUnitConfirmed(), "unit is confirmed");

    // Feature 14: 바닥 후보 평면 자동 판별 (Flatness, Area, CoG)
    const auto cands = doc.floorCandidates();
    CHECK(cands.size() >= 2,
          "must discover at least 2 floor candidates, got %zu", cands.size());
    if (!cands.empty()) {
      CHECK(cands[0].flatness > 50.0,
            "Candidate 1 flatness should be > 50%%, got %.1f%%",
            cands[0].flatness);
      CHECK(cands[0].areaCm2 > 0.0,
            "Candidate 1 area should be > 0 cm2, got %.1f cm2",
            cands[0].areaCm2);
      CHECK(cands[0].recommended, "Candidate 1 should be recommended");
    }

    // Apply candidate 1
    doc.applyFloorCandidate(0);
    CHECK(doc.isGrounded(),
          "doc should be grounded after applying floor candidate");

    // Feature 15: 구연 수평(T) vs 바닥 수평(B)
    CHECK(doc.baseTiltMode() == BaseTiltMode::RimHorizontal,
          "default base tilt mode should be RimHorizontal");
    doc.setBaseTiltMode(BaseTiltMode::BaseHorizontal);
    CHECK(doc.baseTiltMode() == BaseTiltMode::BaseHorizontal,
          "base tilt mode set to BaseHorizontal");
    doc.setBaseTiltMode(BaseTiltMode::RimHorizontal);

    // Feature 16: 구연부편 틈 보기 (Gap Chart)
    doc.setSherdMode(true);
    CHECK(doc.isSherdMode(), "sherd mode enabled");
    const auto gap = doc.rimGapChart();
    CHECK(!gap.gapMm.empty(), "gap chart must have samples");
    CHECK(gap.maxGapMm >= 0.0, "max gap mm: %.2f", gap.maxGapMm);

    // Feature 17: 원호판 가상 중심축
    const double rimDia = doc.estimatedRimDiameter();
    const double rimErr = doc.estimatedRimError();
    const double arcDeg = doc.rimArcDegrees();
    CHECK(rimDia > 100.0 && rimDia < 200.0, "estimated rim dia: %.2f mm",
          rimDia);
    CHECK(rimErr >= 0.0, "estimated rim error: %.2f mm", rimErr);
    CHECK(arcDeg > 0.0 && arcDeg <= 360.0, "rim arc deg: %.1f deg", arcDeg);

    // Revert sherd mode for subsequent steps
    doc.setSherdMode(false);
    doc.confirmPose();
    CHECK(doc.poseConfirmed(), "pose confirmed successfully");
  }

  // -------------------------------------------------------------
  // [Step 2: 막으로 자르기 (Features 18~23)]
  // -------------------------------------------------------------
  std::printf("\n--- Step 2: 막으로 자르기 ---\n");
  {
    ArtifactDoc doc;
    doc.loadSample(true); // confirms pose
    CHECK(doc.poseConfirmed(), "pose must be confirmed");

    // Feature 20: 추천 각도 3종 엔진 & 자석 스냅
    const auto recs = doc.recommendAngles();
    CHECK(recs.size() == 3, "must have 3 recommended angles, got %zu",
          recs.size());
    if (recs.size() == 3) {
      std::printf("  Rec 1: %.1f deg (score %.2f) - %s\n", recs[0].angleDeg,
                  recs[0].score, qUtf8Printable(recs[0].why));
      std::printf("  Rec 2: %.1f deg (score %.2f) - %s\n", recs[1].angleDeg,
                  recs[1].score, qUtf8Printable(recs[1].why));
      std::printf("  Rec 3: %.1f deg (score %.2f) - %s\n", recs[2].angleDeg,
                  recs[2].score, qUtf8Printable(recs[2].why));
      CHECK(recs[0].star, "Rec 1 should have star badge");
      // Check angular separation >= 30 degrees
      double diff01 =
          std::fabs(std::remainder(recs[0].angleDeg - recs[1].angleDeg, 360.0));
      CHECK(diff01 >= 30.0, "Rec 0 and 1 separated by %.1f deg", diff01);
    }

    // Magnet snap: if angle is within +-2 deg of rec 0, snaps to rec 0
    if (!recs.empty()) {
      double testAng = recs[0].angleDeg + 1.2;
      double snapped = doc.snapAngle(testAng, 2.0);
      CHECK(near(snapped, recs[0].angleDeg, 1e-4),
            "snapAngle snapped %.1f to %.1f", testAng, snapped);
    }

    // Feature 21: 실시간 단면 상태 검사선 (cutChecks)
    const auto status = doc.checkCutStatus();
    CHECK(status.closedLoop,
          "cut status: closed loop expected for whole sample");
    CHECK(status.breakCount == 0, "cut status: breakCount == 0 expected");
    CHECK(status.minWall > 0.0 && status.maxWall >= status.minWall,
          "wall range: %.1f ~ %.1f mm", status.minWall, status.maxWall);
    CHECK(!status.statusText.isEmpty(), "statusText must not be empty: %s",
          qUtf8Printable(status.statusText));

    // Feature 23: Z 막 횡단면 슬라이스
    const auto zSlices = doc.horizontalSlice(95.0);
    CHECK(!zSlices.empty(), "horizontal slice at z=95 mm must return loops");
  }

  // -------------------------------------------------------------
  // [Step 3: 도면 편집 & 내보내기 (Features 24~33)]
  // -------------------------------------------------------------
  std::printf("\n--- Step 3: 도면 편집 & 내보내기 ---\n");
  {
    ArtifactDoc doc;
    doc.loadSample(true);

    // Feature 24: 기준선 4종 (경부선 포함)
    const PlateRefLines r = plateRefLines(doc, 3.0);
    CHECK(r.center.has_value(), "center line must exist");
    CHECK(r.rim.has_value(), "rim line must exist");
    CHECK(r.base.has_value(), "base line must exist");
    CHECK(r.neck.has_value(), "neck line must exist");
    if (r.neck.has_value()) {
      std::printf("  Neck line from (%.1f, %.1f) to (%.1f, %.1f)\n",
                  r.neck->a.x, r.neck->a.y, r.neck->b.x, r.neck->b.y);
      CHECK(r.neck->a.y > 100.0 && r.neck->a.y < doc.dims().height,
            "neck Z within vessel height: %.1f", r.neck->a.y);
    }

    // Feature 26: 결실 저경 공란 원칙
    doc.setSherdMode(true);
    doc.confirmPose();
    CHECK(doc.dims().base == 0.0,
          "sherd mode base diameter must be 0.0 (blank)");
    doc.setSherdMode(false);
    doc.confirmPose();
    CHECK(doc.dims().base > 0.0,
          "whole vessel base diameter must be > 0.0: %.2f", doc.dims().base);

    // Feature 27: 미스캔 내벽 점선 추정 고립
    ArtifactDoc blindDoc;
    blindDoc.loadArtecBlindSample(true);
    CHECK(blindDoc.hasEstimatedInner(),
          "blind sample must have estimated inner wall");
    const double meanWall = blindDoc.dims().meanWall;
    CHECK(meanWall > 1.5 && meanWall < 20.0, "blind sample meanWall: %.2f mm",
          meanWall);

    // Feature 28: 내보내기 거부 가드 (refusePreview)
    doc.previewFrontAngle(45.0);
    CHECK(doc.isPreviewing(), "doc is previewing");
    ExportRequest req;
    req.path = QDir(outDir).filePath("test_refuse.svg");
    QString err;
    bool exported = exportSvgLayered(doc, req, &err);
    CHECK(!exported, "exportSvgLayered must fail during preview");
    CHECK(!err.isEmpty(), "error string must be returned: %s",
          qUtf8Printable(err));
    doc.commitPreview();
    CHECK(!doc.isPreviewing(), "doc is no longer previewing");

    // Feature 29: 14개 레이어 분리 SVG 내보내기
    req.path = QDir(outDir).filePath("test_14layers.svg");
    req.opts.spec14Layers = true;
    req.opts.calibration100mm = true;
    bool svgOk = exportSvg14Layers(doc, req, &err);
    CHECK(svgOk, "exportSvg14Layers returned true");

    // Inspect SVG layers
    QFile svgFile(req.path);
    CHECK(svgFile.open(QIODevice::ReadOnly), "open generated SVG file");
    QXmlStreamReader xml(&svgFile);
    QStringList layerIds;
    while (!xml.atEnd()) {
      if (xml.readNext() == QXmlStreamReader::StartElement) {
        if (xml.name() == QLatin1String("g") &&
            xml.attributes().hasAttribute(QLatin1String("inkscape:label"))) {
          layerIds << xml.attributes().value(QLatin1String("id")).toString();
        }
      }
    }
    std::printf("  14-Layer SVG produced layers: %s\n",
                qUtf8Printable(layerIds.join(',')));
    CHECK(layerIds.contains("grid"), "contains grid layer");
    CHECK(layerIds.contains("shade"), "contains shade layer");
    CHECK(layerIds.contains("calibration"), "contains calibration layer");
    CHECK(layerIds.contains("reconstruction"), "contains reconstruction layer");
    CHECK(layerIds.contains("inner"), "contains inner layer");
    CHECK(layerIds.contains("refline.center"), "contains refline.center layer");
    CHECK(layerIds.contains("refline.rim"), "contains refline.rim layer");
    CHECK(layerIds.contains("refline.base"), "contains refline.base layer");
    CHECK(layerIds.contains("outline"), "contains outline layer");
    CHECK(layerIds.contains("section"), "contains section layer");
    CHECK(layerIds.contains("break"), "contains break layer");
    CHECK(layerIds.contains("dimension"), "contains dimension layer");
    CHECK(layerIds.contains("scalebar"), "contains scalebar layer");
    CHECK(layerIds.contains("label"), "contains label layer");

    // Feature 30: CAD 호환 DXF R12 내보내기
    req.path = QDir(outDir).filePath("test_out.dxf");
    bool dxfOk = exportDxf(doc, req, &err);
    CHECK(dxfOk, "exportDxf returned true");

    // Feature 31: 600 DPI PDF 내보내기
    req.path = QDir(outDir).filePath("test_out.pdf");
    bool pdfOk = exportPdf(doc, req, &err);
    CHECK(pdfOk, "exportPdf returned true");

    // Feature 32: 300 DPI PNG 내보내기
    req.path = QDir(outDir).filePath("test_out.png");
    bool pngOk = exportPng(doc, req, 300, &err);
    CHECK(pngOk, "exportPng 300 DPI returned true");
  }

  std::printf("\nMilestone 3 Test Finished. Total Failures: %d\n", fails);
  return fails == 0 ? 0 : 1;
}
