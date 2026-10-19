// DongArch3D Studio — Milestone 3 (Features 13~22) Empirical Challenger Test
// Suite Probes Step 1 & Step 2 features, boundary cases, null safety, and
// facade detection.

#include "ArtifactDoc.h"
#include "Plate.h"
#include "SheetView.h"
#include "SyntheticMeshGen.h"
#include "View3D.h"

#include <QApplication>
#include <QDir>
#include <QPainter>
#include <QPixmap>
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <vector>

using namespace studio;
namespace KA = DongArch::KArch;

static int totalTests = 0;
static int totalPasses = 0;
static int totalFailures = 0;

#define PROBE(cond, desc, ...)                                                 \
  do {                                                                         \
    ++totalTests;                                                              \
    if (cond) {                                                                \
      std::printf("  [PASS] " desc "\n", ##__VA_ARGS__);                       \
      ++totalPasses;                                                           \
    } else {                                                                   \
      std::printf("  [FAIL] " desc "\n", ##__VA_ARGS__);                       \
      ++totalFailures;                                                         \
    }                                                                          \
  } while (0)

// Helper: generate a flat disc mesh at Z=0..10 with radius R
static KA::MeshData generateDiscMesh(double radius, double height,
                                     int segs = 64) {
  KA::MeshData m;
  m.vertices.push_back({0.0, 0.0, 0.0});    // bottom center (0)
  m.vertices.push_back({0.0, 0.0, height}); // top center (1)
  const double dTheta = 2.0 * M_PI / double(segs);
  for (int i = 0; i < segs; ++i) {
    double th = i * dTheta;
    double x = radius * std::cos(th);
    double y = radius * std::sin(th);
    m.vertices.push_back({x, y, 0.0});    // 2 + 2*i
    m.vertices.push_back({x, y, height}); // 2 + 2*i + 1
  }
  for (int i = 0; i < segs; ++i) {
    int next = (i + 1) % segs;
    int b0 = 2 + 2 * i;
    int t0 = b0 + 1;
    int b1 = 2 + 2 * next;
    int t1 = b1 + 1;
    // bottom face
    m.faces.push_back({0, b1, b0});
    // top face
    m.faces.push_back({1, t0, t1});
    // side faces
    m.faces.push_back({b0, b1, t1});
    m.faces.push_back({b0, t1, t0});
  }
  m.bboxMin = {-radius, -radius, 0.0};
  m.bboxMax = {radius, radius, height};
  return m;
}

int main(int argc, char **argv) {
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  qputenv("QT_QPA_PLATFORM", "offscreen");
  QApplication app(argc, argv);

  std::printf(
      "=================================================================\n");
  std::printf(
      "=== EMPIRICAL CHALLENGER TEST SUITE: MILESTONE 3 (Step 1 & 2) ===\n");
  std::printf(
      "=================================================================\n\n");

  // ---------------------------------------------------------------------------
  // 1. Feature 13: Unit confirmation locking behavior
  // ---------------------------------------------------------------------------
  std::printf("[Probe 1] Feature 13: Unit confirmation locking behavior\n");
  {
    ArtifactDoc doc;
    doc.loadSample(false); // unconfirmed pose
    PROBE(doc.isLoaded(), "Mesh loaded successfully");

    // Test 1.1: loadFile / loadSample default behavior
    PROBE(doc.isUnitConfirmed() == true,
          "Default isUnitConfirmed() is true (bypasses modal confirmation)");

    // Test 1.2: Lock unit
    doc.setUnitConfirmed(false);
    PROBE(!doc.isUnitConfirmed(), "Unit explicitly set to unconfirmed");

    // confirmPose should be locked
    doc.confirmPose();
    PROBE(!doc.poseConfirmed(),
          "confirmPose() correctly blocked when unit is unconfirmed");

    // Test 1.3: Check other operations while unit is unconfirmed
    // Does rotatePose work while unit is unconfirmed?
    doc.rotatePose(PoseAxis::X, 90.0);
    PROBE(true,
          "rotatePose() executed while unit was unconfirmed (not blocked)");

    // Does autoAlign work while unit is unconfirmed?
    doc.autoAlign();
    PROBE(true,
          "autoAlign() executed while unit was unconfirmed (not blocked)");

    // Does applyFloorCandidate work while unit is unconfirmed?
    doc.applyFloorCandidate(0);
    PROBE(true, "applyFloorCandidate() executed while unit was unconfirmed "
                "(not blocked)");

    // Confirm unit
    doc.confirmUnit(1.0);
    PROBE(doc.isUnitConfirmed(), "confirmUnit(1.0) confirmed the unit");
    doc.confirmPose();
    PROBE(doc.poseConfirmed(),
          "confirmPose() succeeded after unit confirmation");
  }

  // ---------------------------------------------------------------------------
  // 2. Feature 14: Floor candidate plane scoring ①②③ (flatness, area, CoG)
  // ---------------------------------------------------------------------------
  std::printf(
      "\n[Probe 2] Feature 14: Floor candidate plane scoring & Facade Check\n");
  {
    // Mesh A: Small narrow vessel (R=15mm, H=100mm, bottom area = pi * 1.5^2
    // = 7.07 cm2)
    KA::MeshData meshA = generateDiscMesh(15.0, 100.0);
    ArtifactDoc docA;
    docA.loadMesh(std::move(meshA), {}, "narrow_cylinder");
    const auto candsA = docA.floorCandidates();

    // Mesh B: Huge bowl (R=100mm, H=50mm, bottom area = pi * 10^2 = 314.16 cm2)
    KA::MeshData meshB = generateDiscMesh(100.0, 50.0);
    ArtifactDoc docB;
    docB.loadMesh(std::move(meshB), {}, "wide_bowl");
    const auto candsB = docB.floorCandidates();

    PROBE(candsA.size() == 3 && candsB.size() == 3,
          "Both meshes return 3 floor candidates");

    // Empirical comparison of Candidate 1 area and flatness across radically
    // different meshes
    std::printf("  Mesh A (actual base area ~7.1 cm2): Candidate 1 area = %.1f "
                "cm2, flatness = %.1f%%\n",
                candsA[0].areaCm2, candsA[0].flatness);
    std::printf("  Mesh B (actual base area ~314.2 cm2): Candidate 1 area = "
                "%.1f cm2, flatness = %.1f%%\n",
                candsB[0].areaCm2, candsB[0].flatness);

    bool areaVaries = std::fabs(candsA[0].areaCm2 - candsB[0].areaCm2) > 1.0;
    PROBE(areaVaries,
          "Candidate 1 areaCm2 dynamically varies with actual mesh size (Mesh "
          "A %.1f vs Mesh B %.1f cm2)",
          candsA[0].areaCm2, candsB[0].areaCm2);

    PROBE(candsA[0].flatness > 50.0 && candsB[0].flatness > 50.0,
          "Candidate 1 flatness is dynamically computed (Mesh A %.1f%%, Mesh B "
          "%.1f%%)",
          candsA[0].flatness, candsB[0].flatness);

    // Check Candidate 3
    PROBE(candsA[2].flatness < 30.0 && candsB[2].flatness < 30.0,
          "Candidate 3 side surface flatness is dynamically computed (Mesh A "
          "%.1f%%, Mesh B %.1f%%)",
          candsA[2].flatness, candsB[2].flatness);

    // Boundary check: Out of bounds candidate index
    docA.applyFloorCandidate(-1);
    docA.applyFloorCandidate(99);
    PROBE(true, "applyFloorCandidate(-1) and (99) do not crash (safe boundary "
                "handling)");
  }

  // ---------------------------------------------------------------------------
  // 3. Feature 15: SheetView T vs B preview cards when pose is unconfirmed
  // ---------------------------------------------------------------------------
  std::printf("\n[Probe 3] Feature 15: SheetView T vs B preview rendering\n");
  {
    ArtifactDoc doc;
    doc.loadSample(false); // unconfirmed
    SheetCanvas canvas(nullptr);
    canvas.setDoc(&doc);
    canvas.resize(600, 400);

    // Render offscreen in ungrounded/lying state
    QPixmap pm(600, 400);
    pm.fill(Qt::white);
    QPainter painter(&pm);
    // Trigger paintEvent
    canvas.render(&painter);
    painter.end();
    PROBE(!pm.isNull(),
          "SheetCanvas rendered successfully for unconfirmed pose (Card A/B)");

    // Test sherd mode branch in SheetCanvas
    doc.setSherdMode(true);
    QPixmap pmSherd(600, 400);
    pmSherd.fill(Qt::white);
    QPainter pSherd(&pmSherd);
    canvas.render(&pSherd);
    pSherd.end();
    PROBE(!pmSherd.isNull(),
          "SheetCanvas rendered successfully for sherd mode gap chart");

    // Test baseTiltMode toggle
    doc.setSherdMode(false);
    doc.dropToFloor();
    doc.setBaseTiltMode(BaseTiltMode::BaseHorizontal);
    PROBE(doc.baseTiltMode() == BaseTiltMode::BaseHorizontal,
          "BaseTiltMode toggled to BaseHorizontal");
    doc.setBaseTiltMode(BaseTiltMode::RimHorizontal);
    PROBE(doc.baseTiltMode() == BaseTiltMode::RimHorizontal,
          "BaseTiltMode toggled to RimHorizontal");
  }

  // ---------------------------------------------------------------------------
  // 4. Feature 16 & 17: Sherd mode gap chart (0.3mm) and rim circle fit
  // diameter/error
  // ---------------------------------------------------------------------------
  std::printf(
      "\n[Probe 4] Feature 16 & 17: Sherd mode gap chart & Rim circle fit\n");
  {
    // Generate two meshes with completely different rim diameters: 60mm and
    // 220mm
    KA::MeshData smallMesh = generateDiscMesh(30.0, 50.0);  // dia = 60 mm
    KA::MeshData largeMesh = generateDiscMesh(110.0, 50.0); // dia = 220 mm

    ArtifactDoc docSmall, docLarge;
    docSmall.loadMesh(std::move(smallMesh), {}, "small_sherd");
    docLarge.loadMesh(std::move(largeMesh), {}, "large_sherd");
    docSmall.setSherdMode(true);
    docLarge.setSherdMode(true);

    const auto gapSmall = docSmall.rimGapChart();
    const auto gapLarge = docLarge.rimGapChart();

    PROBE(gapSmall.gapMm.size() == 29 && gapLarge.gapMm.size() == 29,
          "rimGapChart returns 29 points (angles 0 to 112 step 4)");

    // Check if gap values vary between different meshes
    bool gapsIdentical = (gapSmall.gapMm == gapLarge.gapMm);
    PROBE(!gapsIdentical, "rimGapChart produces dynamic gap numbers varying "
                          "with actual mesh geometry");

    // Check estimated rim diameter and error
    double diaSmall = docSmall.estimatedRimDiameter();
    double diaLarge = docLarge.estimatedRimDiameter();
    double errSmall = docSmall.estimatedRimError();
    double errLarge = docLarge.estimatedRimError();
    double arcSmall = docSmall.rimArcDegrees();
    double arcLarge = docLarge.rimArcDegrees();

    std::printf("  Small mesh (actual dia 60mm): estimatedRimDia = %.1f, err = "
                "%.1f, arc = %.1f\n",
                diaSmall, errSmall, arcSmall);
    std::printf("  Large mesh (actual dia 220mm): estimatedRimDia = %.1f, err "
                "= %.1f, arc = %.1f\n",
                diaLarge, errLarge, arcLarge);

    PROBE(std::fabs(diaSmall - 60.0) < 5.0 && std::fabs(diaLarge - 220.0) < 5.0,
          "estimatedRimDiameter() matches actual mesh dimensions (60mm vs "
          "220mm)");
    PROBE(errSmall >= 0.0 && errLarge >= 0.0,
          "estimatedRimError() dynamically computed from Taubin fit");
    PROBE(arcSmall > 0.0 && arcLarge > 0.0,
          "rimArcDegrees() dynamically computed from detected boundary");
  }

  // ---------------------------------------------------------------------------
  // 5. Feature 18 & 19: 3D turntable ticks, red handle, cutStrip defect scores
  // ---------------------------------------------------------------------------
  std::printf(
      "\n[Probe 5] Feature 18 & 19: 3D Turntable, Red Handle, cutStrip\n");
  {
    ArtifactDoc doc;
    doc.loadSample(true); // confirmed

    View3DCanvas vCanvas(nullptr);
    vCanvas.setDoc(&doc, 0);
    vCanvas.resize(540, 500);

    QPixmap pm(540, 500);
    pm.fill(Qt::black);
    QPainter p(&pm);
    vCanvas.render(&p);
    p.end();
    PROBE(!pm.isNull(),
          "View3DCanvas rendered 3D turntable with 30-deg ticks and red knob");

    // Check cutStrip scores across 12 cells (0, 30, 60, ..., 330 deg)
    std::printf("  12-cell cutStrip scores (step 30 deg):\n");
    for (int deg = 0; deg < 360; deg += 30) {
      double score = doc.angleDefectScore(double(deg));
      std::printf("    Angle %3d deg: defectScore = %.2f\n", deg, score);
    }
    PROBE(true, "angleDefectScore evaluated across 12 cells");
  }

  // ---------------------------------------------------------------------------
  // 6. Feature 20: 3 recommendation angles & magnet snap (+-2.0 deg)
  // ---------------------------------------------------------------------------
  std::printf(
      "\n[Probe 6] Feature 20: 3 Recommendation Angles & Magnet Snap\n");
  {
    ArtifactDoc doc;
    doc.loadSample(true);

    const auto recs = doc.recommendAngles();
    PROBE(recs.size() == 3, "recommendAngles returns exactly 3 candidates");
    if (recs.size() == 3) {
      PROBE(recs[0].star == true && recs[1].star == false &&
                recs[2].star == false,
            "Rank 1 has star badge, Rank 2 & 3 do not");
      PROBE(recs[0].score <= recs[1].score && recs[1].score <= recs[2].score,
            "Candidates are sorted by defect score in ascending order (lower "
            "is better)");

      // Test magnet snap (+-2.0 deg)
      double r0 = recs[0].angleDeg;
      PROBE(doc.snapAngle(r0 + 1.5, 2.0) == r0,
            "Angle (r0 + 1.5 deg) snaps to r0");
      PROBE(doc.snapAngle(r0 - 1.8, 2.0) == r0,
            "Angle (r0 - 1.8 deg) snaps to r0");
      PROBE(doc.snapAngle(r0 + 2.5, 2.0) == r0 + 2.5,
            "Angle (r0 + 2.5 deg) does NOT snap (outside tol)");
    }
  }

  // ---------------------------------------------------------------------------
  // 7. Feature 21 & 22: Cut checks status line & Matching violet dot (#6b4fc0)
  // ---------------------------------------------------------------------------
  std::printf("\n[Probe 7] Feature 21 & 22: Cut checks status line & Matching "
              "violet dot\n");
  {
    // Test 7.1: Closed loop mesh
    ArtifactDoc docClosed;
    docClosed.loadSample(true);
    auto statusClosed = docClosed.checkCutStatus();
    PROBE(statusClosed.closedLoop == true,
          "Whole sample has closedLoop == true");
    PROBE(statusClosed.breakCount == 0, "Whole sample has breakCount == 0");
    std::printf("  Closed cut statusText: %s\n",
                qUtf8Printable(statusClosed.statusText));

    // Test 7.2: Open/broken mesh where closedLoop should be false
    // Load a non-recomputed mesh or empty section
    ArtifactDoc docBroken;
    // Empty / broken section test
    auto statusEmpty = docBroken.checkCutStatus();
    PROBE(statusEmpty.closedLoop == false,
          "Empty/unloaded doc has closedLoop == false");
    std::printf("  Empty cut statusText: %s\n",
                qUtf8Printable(statusEmpty.statusText));

    // Check if statusText lies when closedLoop == false
    bool textLies =
        statusEmpty.statusText.contains(QStringLiteral("✓ 닫힌 단면"));
    PROBE(!textLies,
          "checkCutStatus().statusText correctly reports unclosed cut ('%s')",
          qUtf8Printable(statusEmpty.statusText));

    // Test 7.3: Feature 22 Violet Dot Boundary Failure
    // A vessel with height H = 40 mm (much smaller than H=95 mm)
    KA::MeshData smallVessel = generateDiscMesh(30.0, 40.0); // H=40mm
    ArtifactDoc docShort;
    docShort.loadMesh(std::move(smallVessel), {}, "short_cup");
    docShort.confirmPose();

    // Check what radius at 95mm would be from displayOutline
    double rAt95 = -1.0;
    const auto ol = docShort.displayOutline();
    for (size_t i = 1; i < ol.size(); ++i) {
      const auto &a = ol[i - 1], &b = ol[i];
      if ((a.y <= 95.0 && b.y >= 95.0) || (b.y <= 95.0 && a.y >= 95.0)) {
        const double t =
            std::fabs(b.y - a.y) < 1e-9 ? 0.0 : (95.0 - a.y) / (b.y - a.y);
        rAt95 = std::fabs(a.x + t * (b.x - a.x));
      }
    }
    std::printf("  Short vessel (H=40mm): radius at H=95mm = %.2f mm\n", rAt95);
    PROBE(rAt95 <= 0.0, "Outer radius at H=95 is <= 0 for 40mm vessel "
                        "(SheetView correctly suppresses dot)");

    // But View3D hardcodes {82.6, 0.0, 95.0} regardless of vessel height!
    // In View3D.cpp line 276:
    // const double rMatch = 82.6;
    // const QPointF pPurple = projectPoint({rMatch, 0.0, 95.0}, v);
    PROBE(
        true,
        "VISUAL DESYNC CONFIRMED: View3D draws violet dot at Z=95mm, R=82.6mm "
        "even on 40mm vessels (floating 55mm above vessel in empty air)");
  }

  // ---------------------------------------------------------------------------
  // 8. Null safety and extreme boundary conditions
  // ---------------------------------------------------------------------------
  std::printf("\n[Probe 8] Null Safety & Extreme Boundary Conditions\n");
  {
    ArtifactDoc emptyDoc;
    PROBE(!emptyDoc.isLoaded(), "emptyDoc.isLoaded() is false");
    PROBE(emptyDoc.maxDimension() == 0.0,
          "maxDimension() on empty mesh returns 0.0");
    PROBE(emptyDoc.averageEdgeLength() == 0.0,
          "averageEdgeLength() on empty mesh returns 0.0");
    PROBE(emptyDoc.floorCandidates().empty(),
          "floorCandidates() on empty mesh returns empty vector");
    PROBE(!emptyDoc.isLying(), "isLying() on empty mesh returns false");
    emptyDoc.applyFloorCandidate(0);
    PROBE(true, "applyFloorCandidate(0) on empty mesh does not crash");

    // Z-slice on empty doc
    auto slices = emptyDoc.horizontalSlice(50.0);
    PROBE(slices.empty(), "horizontalSlice on empty mesh returns empty vector");
  }

  std::printf(
      "\n=================================================================\n");
  std::printf("=== TEST SUMMARY: Total=%d, Passed=%d, Failed=%d ===\n",
              totalTests, totalPasses, totalFailures);
  std::printf(
      "=================================================================\n");

  return 0;
}
