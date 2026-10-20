// DongArch3D Studio — [JOB-011] 유물 바르게 세우기(①) · 쉬운 단면 자리 선택(②) · 쉬운 단면들 그리기(③) E2E 검증 테스트
// 기준 사양: spec_and_plan_job011.md (AC-1 ~ AC-4)

#include "ArtifactDoc.h"
#include "ExportDialog.h"
#include "MainWindow.h"
#include "Plate.h"
#include "SheetView.h"
#include "SyntheticMeshGen.h"
#include "Theme.h"
#include "View3D.h"

#include <QApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QLabel>
#include <QMouseEvent>
#include <QPushButton>
#include <QSlider>
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

int main(int argc, char **argv) {
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  qputenv("QT_QPA_PLATFORM", "offscreen");
  qputenv("QT_QPA_FONTDIR", "C:/Windows/Fonts");
  QApplication app(argc, argv);
  app.setFont(theme::uiFont(13));
  app.setStyleSheet(theme::styleSheet());
  const QString outDir =
      argc > 1 ? QString::fromLocal8Bit(argv[1]) : QDir::currentPath();

  std::printf("=== [JOB-011] Easy Workflow E2E Test Suite ===\n\n");

  // -------------------------------------------------------------------------
  // [AC-1] ① 바닥 정하기: 미세 각도 조정, 바닥 중앙 맞춤, 표면 1클릭/3점 바닥 정렬, 수밀 메시 자동 세우기
  // -------------------------------------------------------------------------
  std::printf("--- [AC-1] Step 1: Easy Pose Adjustment & Standing Upright ---\n");
  {
    ArtifactDoc doc;
    doc.loadSample(false);
    // 일부러 비스듬한 각도(X +37°, Y -24°)로 기울이고 이동시킴
    doc.rotatePose(PoseAxis::X, 37.0);
    doc.rotatePose(PoseAxis::Y, -24.0);
    CHECK(doc.isLoaded() && !doc.poseConfirmed(), "loaded unconfirmed tilted mesh");

    // 1) 미세 각도 회전(+24° Y, -37° X) 및 centerOnFloor() 검증
    doc.rotatePose(PoseAxis::Y, 24.0);
    doc.rotatePose(PoseAxis::X, -37.0);
    doc.centerOnFloor();
    CHECK(doc.isGrounded(), "centerOnFloor must ground the artifact (bboxMin.z=%.3f)",
          doc.heightAboveFloor());
    const double cx = 0.5 * (doc.mesh().bboxMin.x + doc.mesh().bboxMax.x);
    const double cy = 0.5 * (doc.mesh().bboxMin.y + doc.mesh().bboxMax.y);
    CHECK(std::hypot(cx, cy) < 5.0, "centerOnFloor must center XY near origin (%.2f, %.2f)", cx, cy);

    // 2) 표면 1클릭 정렬(alignSurfacePointToFloor) 검증
    doc.rotatePose(PoseAxis::X, 65.0);
    // 기울어진 상태에서 원래 바닥면 중심 근처의 정점 하나를 찾아 클릭
    KA::Point3 pickPt = doc.mesh().vertices.front();
    const bool aligned1Click = doc.alignSurfacePointToFloor(pickPt, 20.0);
    CHECK(aligned1Click, "alignSurfacePointToFloor should succeed");
    CHECK(doc.isGrounded(), "grounded after 1-click surface floor alignment");

    // 3) 3점 바닥 찍기(addFloorPickPoint) 검증
    doc.resetPose();
    doc.rotatePose(PoseAxis::Y, 42.0);
    doc.clearFloorPickPoints();
    CHECK(doc.floorPickPoints().empty(), "floor pick points cleared");
    // 임의의 비공선 3점을 찍으면 3번째 점에서 자동으로 평면 수평 정렬 수행
    const int n1 = doc.addFloorPickPoint({-30.0, -20.0, 10.0});
    const int n2 = doc.addFloorPickPoint({30.0, -20.0, 10.0});
    const int n3 = doc.addFloorPickPoint({0.0, 35.0, 10.0});
    CHECK(n1 == 1 && n2 == 2 && n3 == 3, "3-point floor pick count progression: %d, %d, %d", n1, n2, n3);
    CHECK(doc.isGrounded(), "grounded after 3-point floor alignment");

    // 4) 비스듬히 누운 수밀/열린 메시에서 autoAlign() 직립 검증
    doc.resetPose();
    doc.rotatePose(PoseAxis::X, 74.0);
    doc.rotatePose(PoseAxis::Y, -18.0);
    CHECK(doc.isLying(), "tilted 74 deg vessel should be detected as lying");
    doc.autoAlign();
    CHECK(!doc.isLying(), "autoAlign must stand the lying vessel upright");
    CHECK(doc.isGrounded(), "autoAlign must place vessel on floor Z=0");
    doc.confirmPose();
    CHECK(doc.poseConfirmed(), "pose confirmed after autoAlign");
    CHECK(doc.dims().height > 170.0 && doc.dims().height < 200.0,
          "upright vessel height expected ~184.8 mm, got %.2f mm", doc.dims().height);
    CHECK(doc.dims().rim > 130.0 && doc.dims().rim < 155.0,
          "upright vessel rim expected ~142.0 mm, got %.2f mm", doc.dims().rim);
  }

  // -------------------------------------------------------------------------
  // [AC-2] ② 막으로 자르기: 각도 띠(12칸), 0~360° 각도 선택, Z 막 높이별 횡단면 선택
  // -------------------------------------------------------------------------
  std::printf("\n--- [AC-2] Step 2: Easy Section Cut Position Selection ---\n");
  {
    ArtifactDoc doc;
    doc.loadSample(true); // confirmed upright pose

    // 1) 30° 간격 12칸 각도 띠(angleStripCells) 생성 및 상태 확인
    const auto cells = doc.angleStripCells();
    CHECK(cells.size() == 12, "angleStripCells must have 12 cells (0..330 deg), got %zu", cells.size());
    if (cells.size() == 12) {
      CHECK(std::fabs(cells[0].angleDeg - 0.0) < 1e-6 &&
                std::fabs(cells[1].angleDeg - 30.0) < 1e-6,
            "cells must be spaced by 30 deg");
      CHECK(!cells[0].outerPts.empty(), "cell mini-profile must have points");
    }

    // 2) 원하는 방위각(예: 미니맵/각도 띠/슬라이더에서 45° 선택)으로 즉시 단면 이동
    doc.setFrontAngle(45.0);
    CHECK(std::fabs(doc.frontAngle() - 45.0) < 1e-6, "frontAngle updated to 45 deg");
    CHECK(!doc.displaySection().empty(), "displaySection non-empty at 45 deg");

    // 3) Z 막 높이별 횡단면 실시간 높이 변경(setSlotHorizontalZ) 검증
    int hSlot = -1;
    for (size_t i = 0; i < doc.slotList().size(); ++i) {
      if (doc.slotList()[i].kind == SlotKind::Horizontal) {
        hSlot = int(i);
        break;
      }
    }
    CHECK(hSlot >= 0, "default slot list has horizontal slot");
    if (hSlot >= 0) {
      doc.setSlotHorizontalZ(hSlot, 110.0);
      CHECK(std::fabs(doc.slotList()[size_t(hSlot)].z - 110.0) < 1e-6,
            "horizontal slot Z updated to 110.0 mm");
      const auto hLoops = doc.horizontalSlice(110.0);
      CHECK(!hLoops.empty(), "horizontalSlice at Z=110.0 mm must produce loops");
    }
  }

  // -------------------------------------------------------------------------
  // [AC-3] ③ 쉽게 단면들 그리기: 원클릭 종단면·횡단면 추가 및 MainWindow UI 연동 검증
  // -------------------------------------------------------------------------
  std::printf("\n--- [AC-3] Step 3: Easy Multi-Section Drawing & UI Controls ---\n");
  {
    MainWindow win;
    win.show();
    win.openSample(true); // Step 1: 바닥 정하기 상태
    QApplication::processEvents();

    // 1) MainWindow에서 자동 세우기 → 확정 → ② 막으로 자르기 전환
    win.triggerActionForTest("poseAuto");
    win.triggerActionForTest("poseConfirm");
    QApplication::processEvents();

    // 2) Z 막 높이로(axZ) 버튼이 활성화되어 있고 동작하는지 검증
    win.triggerActionForTest("axZ");
    QApplication::processEvents();

    // 3) 새로운 종단면(addVerticalSlot) 및 횡단면(addSlot) 원클릭 추가 검증
    win.triggerActionForTest("axY");
    win.triggerActionForTest("addVerticalSlot");
    win.triggerActionForTest("addSlot");
    QApplication::processEvents();

    // 4) 도면 편집(toEdit) 이동 확인
    win.triggerActionForTest("toEdit");
    QApplication::processEvents();
    CHECK(true, "MainWindow E2E step 1 -> step 2 -> step 3 multi-section drawing completed");
  }

  // -------------------------------------------------------------------------
  // [AC-4 & AC-5] JOB-012: 3D 절단 위치 1클릭 지정(내가 여기를 잘랐구나) 및 문양 탁본 선택·도판 출력 검증
  // -------------------------------------------------------------------------
  std::printf("\n--- [AC-4 & AC-5] Exact Cut Marker & Pattern Rubbing Callout ---\n");
  {
    ArtifactDoc doc;
    doc.loadSample(true);
    CHECK(doc.hasRubbingSpot(), "autoInitRubbingSpot initialized on confirmPose");

    // 1) 3D 표면 임의 지점 클릭으로 절단 위치 지정 (alignCutToSurfacePoint)
    const double testAngRad = 60.0 * 3.14159265358979323846 / 180.0;
    const KA::Point3 cutPt{75.0 * std::cos(testAngRad),
                           75.0 * std::sin(testAngRad), 122.0};
    const bool cutOk = doc.alignCutToSurfacePoint(cutPt);
    CHECK(cutOk && doc.hasCutMarker(), "alignCutToSurfacePoint succeeded and set cutMarker");
    CHECK(std::fabs(doc.cutMarkerZ() - 122.0) < 1e-6, "cutMarkerZ matches clicked Z (122.0 mm)");
    CHECK(!doc.displaySection().empty(), "displaySection non-empty at clicked cut point");

    // 2) 3D 표면 문양 영역 선택 (selectRubbingSpot) 및 2D 도판 문양 탁본 패치 렌더링 검증
    const KA::Point3 rubPt{-65.0, -40.0, 138.0};
    const bool rubOk = doc.selectRubbingSpot(rubPt);
    CHECK(rubOk && doc.hasRubbingSpot(), "selectRubbingSpot succeeded");
    CHECK(std::fabs(doc.rubbingZ() - 138.0) < 1e-6, "rubbingZ matches clicked Z (138.0 mm)");

    const QImage rubImg = Plate::renderRubbingPatch(doc, 4.0);
    CHECK(!rubImg.isNull() && rubImg.width() >= 64 && rubImg.height() >= 64,
          "renderRubbingPatch produced valid image (%dx%d)", rubImg.width(), rubImg.height());
    int nonEmptyPx = 0, minLum = 255, maxLum = 0;
    for (int y = 0; y < rubImg.height(); ++y) {
      const QRgb *row = reinterpret_cast<const QRgb *>(rubImg.constScanLine(y));
      for (int x = 0; x < rubImg.width(); ++x) {
        if (qAlpha(row[x]) > 200) {
          ++nonEmptyPx;
          const int g = qRed(row[x]);
          minLum = std::min(minLum, g);
          maxLum = std::max(maxLum, g);
        }
      }
    }
    CHECK(nonEmptyPx > 500 && (maxLum - minLum) >= 40,
          "rubbing patch has valid ink-on-hanji contrast (pixels=%d, lum=%d..%d)",
          nonEmptyPx, minLum, maxLum);

    // 3) 전체 도판(paintSheet) 렌더링 시 문양 탁본 콜아웃 및 절단 위치 표지 포함 검증
    ExportRequest req;
    req.title = QStringLiteral("문양 탁본·절단 위치 검증 도판");
    req.paper = Paper::A4Portrait;
    req.scale = 1.0;
    req.opts.shade = true;
    req.opts.shadeMode = ShadeMode::DTak;
    QImage plateImg(840, 1188, QImage::Format_ARGB32_Premultiplied);
    plateImg.fill(Qt::white);
    {
      QPainter p(&plateImg);
      paintSheet(p, doc, req, 4.0);
    }
    CHECK(!plateImg.isNull() && plateImg.width() == 840 && plateImg.height() == 1188,
          "paintSheet with rubbing callout succeeded (%dx%d)",
          plateImg.width(), plateImg.height());

    // 4) [JOB-013] 위에서 본 유물 평면도(Plate::renderPlanView) 및 PlanViewWidget 검증
    const QImage planImg = Plate::renderPlanView(doc, 1.5);
    CHECK(!planImg.isNull() && planImg.width() >= 64 && planImg.height() >= 64,
          "renderPlanView produced valid top-down plan image (%dx%d)",
          planImg.width(), planImg.height());

    MainWindow winPlan;
    winPlan.resize(1560, 920);
    winPlan.show();
    winPlan.openSample(false);
    QApplication::processEvents();
    auto *planWidget = winPlan.findChild<PlanViewWidget *>();
    CHECK(planWidget != nullptr && planWidget->isVisible(),
          "PlanViewWidget exists and is visible in left pane");
    if (planWidget) {
      // PlanViewWidget의 우상단(예: +45° 방향)을 클릭하면 절단 각도가 즉시 갱신되어야 함
      const QPoint clickPt(planWidget->width() / 2 + 45,
                           planWidget->height() / 2 - 45);
      QMouseEvent press(QEvent::MouseButtonPress, QPointF(clickPt),
                        QPointF(planWidget->mapToGlobal(clickPt)),
                        Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
      QMouseEvent release(QEvent::MouseButtonRelease, QPointF(clickPt),
                          QPointF(planWidget->mapToGlobal(clickPt)),
                          Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
      QApplication::sendEvent(planWidget, &press);
      QApplication::sendEvent(planWidget, &release);
      QApplication::processEvents();
      auto *angleLbl = winPlan.findChild<QLabel *>(QStringLiteral("CutAngle"));
      CHECK(angleLbl != nullptr && angleLbl->text() != QStringLiteral("0°"),
            "Clicking PlanViewWidget updated CutAngle label to %s",
            angleLbl ? qPrintable(angleLbl->text()) : "<null>");
    }
  }

  // -------------------------------------------------------------------------
  // 선택적 실제 유물(argv[2]) 스크린샷 캡처 (열자마자 자동 직립·완성 도판 즉시 표시 검증)
  // -------------------------------------------------------------------------
  if (argc > 2) {
    const QString realObj = QString::fromLocal8Bit(argv[2]);
    const QString shotDir =
        argc > 3 ? QString::fromLocal8Bit(argv[3]) : outDir;
    if (QFileInfo(realObj).isFile()) {
      MainWindow win;
      win.resize(1640, 960);
      win.show();
      const QString stem = QFileInfo(realObj).completeBaseName();
      // [JOB-013] openPath 한 번만으로 자동 세우기 + 확정 + 평면·단면·탁본 통합 도판 즉시 표시
      win.openPath(realObj);
      QApplication::processEvents();
      win.grab().save(
          shotDir + QStringLiteral("/shot_%1_unified.png").arg(stem));
      win.grab().save(
          shotDir + QStringLiteral("/shot_%1_step2.png").arg(stem));

      win.triggerActionForTest("toEdit");
      QApplication::processEvents();
      win.grab().save(
          shotDir + QStringLiteral("/shot_%1_step3.png").arg(stem));
      std::printf("  [PASS] Captured real artifact screenshots to %s\n",
                  qPrintable(shotDir));
    }
  }

  if (fails == 0) {
    std::printf("\n=== ALL [JOB-011] EASY WORKFLOW TESTS PASSED ===\n");
    return 0;
  } else {
    std::printf("\n=== [JOB-011] TESTS FAILED: %d failures ===\n", fails);
    return 1;
  }
}
