// DongArch3D Studio — Milestone 3 Iteration 2 (Remediation)
// Challenger 3: Empirical Stability, Absence of Hardcoded Outputs, and UI Crash
// Stress Harness

#include "ArtifactDoc.h"
#include "ExportDialog.h"
#include "Plate.h"
#include "SyntheticMeshGen.h"

#include <QApplication>
#include <QCheckBox>
#include <QDir>
#include <QFile>
#include <QImage>
#include <QProcess>
#include <QPushButton>
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <vector>

using namespace studio;
namespace KA = DongArch::KArch;
namespace KAT = DongArch::KArch::Testing;

static int g_totalTests = 0;
static int g_passedTests = 0;
static int g_failedTests = 0;

#define CHALLENGE_CHECK(cond, desc, ...)                                       \
  do {                                                                         \
    ++g_totalTests;                                                            \
    if (cond) {                                                                \
      std::printf("  [PASS] " desc "\n", ##__VA_ARGS__);                       \
      ++g_passedTests;                                                         \
    } else {                                                                   \
      std::printf("  [FAIL] " desc "\n", ##__VA_ARGS__);                       \
      ++g_failedTests;                                                         \
    }                                                                          \
  } while (0)

static bool nearVal(double a, double b, double tol) {
  return std::fabs(a - b) <= tol;
}

int main(int argc, char **argv) {
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  qputenv("QT_QPA_PLATFORM", "offscreen");
  QApplication app(argc, argv);

  // Subprocess test mode for crash-safe UI execution
  if (argc > 1 && std::strcmp(argv[1], "--test-dialog-stress") == 0) {
    // Stress-test ExportDialog with multiple doc types and rapid state changes
    ArtifactDoc emptyDoc;
    PlateOptions opts;

    // 1. Unloaded doc
    {
      ExportDialog dlg(emptyDoc, 0, opts, ExportFormat::Svg);
    }

    // 2. Unconfirmed doc
    {
      ArtifactDoc unconf;
      unconf.loadSample(false);
      ExportDialog dlg(unconf, 0, opts, ExportFormat::Pdf);
    }

    // 3. Sherd mode doc
    {
      ArtifactDoc sherdDoc;
      sherdDoc.loadSample(true);
      sherdDoc.setSherdMode(true);
      ExportDialog dlg(sherdDoc, 0, opts, ExportFormat::Dxf);
    }

    // 4. Full interactive toggle on whole sample
    {
      ArtifactDoc wholeDoc;
      wholeDoc.loadSample(true);
      ExportDialog dlg(wholeDoc, 0, opts, ExportFormat::Png);
      // Toggle all check boxes
      const auto boxes = dlg.findChildren<QCheckBox *>();
      for (auto *b : boxes) {
        b->toggle();
      }
      // Toggle all push buttons (chips)
      const auto btns = dlg.findChildren<QPushButton *>();
      for (auto *btn : btns) {
        if (btn->property("chip").toBool()) {
          btn->click();
        }
      }
    }
    return 0;
  }

  const QString outDir =
      argc > 1 ? QString::fromLocal8Bit(argv[1]) : QDir::currentPath();

  std::printf(
      "=================================================================\n");
  std::printf(
      "=== Challenger 3: Empirical Stability & Dynamic Verification ===\n");
  std::printf(
      "=================================================================\n\n");

  // =========================================================================
  // Challenge 1: Diverse Mesh Shapes — Anti-Hardcoding & Geometry Scaling
  // =========================================================================
  std::printf("--- Challenge 1: Diverse Mesh Shapes & Anti-Hardcoding ---\n");
  {
    // 1.1 Thin Cylinder (R=40mm, H=80mm, wall=3mm, open rim)
    KAT::SyntheticParams p1;
    p1.radialSeg = 72;
    p1.heightSeg = 30;
    p1.capTop = false;
    p1.capBottom = true;
    auto shellA = KAT::MakeCylinderShell(40.0, 80.0, 3.0, p1).toMesh();
    ArtifactDoc docA;
    docA.loadMesh(std::move(shellA), {}, "cyl_40_80");
    docA.confirmPose();

    // 1.2 Wide Bowl / Cone (Rbase=30mm, Rtop=110mm, H=50mm, wall=4mm, open rim)
    auto shellB = KAT::MakeConeShell(30.0, 110.0, 50.0, 4.0, p1).toMesh();
    ArtifactDoc docB;
    docB.loadMesh(std::move(shellB), {}, "cone_bowl");
    docB.confirmPose();

    // 1.3 Tall Constricted Vase (Rbelly=70mm, Rrim=35mm, H=180mm, wall=5mm,
    // open rim)
    auto shellC =
        KAT::MakeTorusProfileVessel(70.0, 35.0, 180.0, 5.0, p1).toMesh();
    ArtifactDoc docC;
    docC.loadMesh(std::move(shellC), {}, "constricted_vase");
    docC.confirmPose();

    // Verify Floor Candidates do NOT return hardcoded constants
    const auto candsA = docA.floorCandidates();
    const auto candsB = docB.floorCandidates();
    const auto candsC = docC.floorCandidates();

    CHALLENGE_CHECK(candsA.size() >= 2 && candsB.size() >= 2 &&
                        candsC.size() >= 2,
                    "All diverse meshes return at least 2 floor candidates");

    std::printf("  Mesh A base area: %.1f cm2, Mesh B base area: %.1f cm2, "
                "Mesh C base area: %.1f cm2\n",
                candsA[0].areaCm2, candsB[0].areaCm2, candsC[0].areaCm2);

    CHALLENGE_CHECK(candsA[0].areaCm2 > 0.0 && candsB[0].areaCm2 > 0.0 &&
                        candsC[0].areaCm2 > 0.0,
                    "Floor candidate areas are positive non-zero");

    CHALLENGE_CHECK(!nearVal(candsA[0].areaCm2, 37.0, 0.01) &&
                        !nearVal(candsB[0].areaCm2, 37.0, 0.01),
                    "Floor candidate area is NOT hardcoded to 37.0 cm2 facade");

    // Verify Estimated Rim Diameter accurately scales with geometry
    // Mesh A (R=40, wall=3 -> rim midline ~77mm)
    // Mesh B (Rtop=110, wall=4 -> rim midline ~216mm)
    // Mesh C (Rrim=35, wall=5 -> rim midline ~65-70mm)
    const double diaA = docA.estimatedRimDiameter();
    const double diaB = docB.estimatedRimDiameter();
    const double diaC = docC.estimatedRimDiameter();
    std::printf("  Estimated Rim Diameters: Mesh A=%.1f mm, Mesh B=%.1f mm, "
                "Mesh C=%.1f mm\n",
                diaA, diaB, diaC);

    CHALLENGE_CHECK(diaA >= 74.0 && diaA <= 81.0,
                    "Mesh A estimated rim diameter matches rim boundary [74, "
                    "81] mm (got %.1f)",
                    diaA);
    CHALLENGE_CHECK(diaB >= 210.0 && diaB <= 222.0,
                    "Mesh B estimated rim diameter matches rim boundary [210, "
                    "222] mm (got %.1f)",
                    diaB);
    CHALLENGE_CHECK(diaC >= 64.0 && diaC <= 72.0,
                    "Mesh C estimated rim diameter matches rim boundary [64, "
                    "72] mm (got %.1f)",
                    diaC);

    // Verify Aspect Ratio and isLying()
    // Mesh B: H=50mm, W=220mm -> H < W * 0.85 -> isLying() == true
    // Mesh C: H=180mm, W=140mm -> H >= W * 0.85 -> isLying() == false
    CHALLENGE_CHECK(
        docB.isLying() == true,
        "Wide bowl (H=50, W=220) correctly identified as isLying() == true");
    CHALLENGE_CHECK(docC.isLying() == false,
                    "Tall vessel (H=180, W=140) correctly identified as "
                    "isLying() == false");

    // Verify Neck Line detection for constricted vessel vs bowl
    const auto refLinesC = plateRefLines(docC, 3.0);
    const auto refLinesB = plateRefLines(docB, 3.0);
    CHALLENGE_CHECK(
        refLinesC.neck.has_value(),
        "Constricted vase exhibits detected neck line at upper section");
    CHALLENGE_CHECK(!refLinesB.neck.has_value(),
                    "Conical bowl correctly has NO neck line");
  }

  // =========================================================================
  // Challenge 2: Sherd Mode Partial Arc & Gap Chart Dynamics
  // =========================================================================
  std::printf(
      "\n--- Challenge 2: Sherd Fragment Arc Estimation & Gap Dynamics ---\n");
  {
    KAT::SyntheticParams p;
    p.radialSeg = 72;
    p.heightSeg = 30;
    p.capTop = false;
    p.capBottom = false;
    p.noiseSigma = 0.05;
    auto fullCone = KAT::MakeConeShell(30.0, 100.0, 60.0, 4.0, p);
    // Cut to 120-degree arc fragment
    auto frag = KAT::MakeArcFragment(fullCone, 120.0).toMesh();

    ArtifactDoc sherdDoc;
    sherdDoc.loadMesh(std::move(frag), {}, "sherd_120deg");
    sherdDoc.setSherdMode(true);
    sherdDoc.confirmPose();

    const double arcDeg = sherdDoc.rimArcDegrees();
    const double estDia = sherdDoc.estimatedRimDiameter();
    std::printf("  Sherd estimated arc: %.1f deg (GT 120 deg), estimated "
                "diameter: %.1f mm (GT 200 mm)\n",
                arcDeg, estDia);

    CHALLENGE_CHECK(arcDeg > 0.0 && arcDeg <= 360.0,
                    "Sherd rim arc degrees is in valid range (got %.1f deg)",
                    arcDeg);
    CHALLENGE_CHECK(estDia > 0.0,
                    "Sherd rim diameter is computed and non-zero (got %.1f mm)",
                    estDia);

    // Missing Base Principle: sherd mode dims().base must be 0.0
    CHALLENGE_CHECK(sherdDoc.dims().base == 0.0,
                    "Sherd mode base diameter is 0.0 (blank principle)");

    // Gap chart dynamic verification: compare sherdDoc with sampleDoc to prove
    // dynamic variance
    ArtifactDoc sampleDoc;
    sampleDoc.loadSample(true);
    sampleDoc.setSherdMode(true);
    const auto gapSherd = sherdDoc.rimGapChart();
    const auto gapSample = sampleDoc.rimGapChart();

    CHALLENGE_CHECK(!gapSherd.gapMm.empty() && !gapSample.gapMm.empty(),
                    "Gap chart returned non-empty samples");
    bool identical = (gapSherd.gapMm == gapSample.gapMm);
    CHALLENGE_CHECK(
        !identical,
        "Gap chart outputs differ between meshes (no static canned array)");
  }

  // =========================================================================
  // Challenge 3: Defect Score Responsiveness & Recommendation Logic
  // =========================================================================
  std::printf(
      "\n--- Challenge 3: Defect Score Responsiveness & Angular Snap ---\n");
  {
    ArtifactDoc doc;
    doc.loadSample(true);

    const auto recs = doc.recommendAngles();
    CHALLENGE_CHECK(recs.size() == 3, "recommendAngles returns 3 candidates");
    if (recs.size() == 3) {
      CHALLENGE_CHECK(recs[0].star, "Rank 1 candidate has star recommendation");
      CHALLENGE_CHECK(!recs[1].star && !recs[2].star,
                      "Rank 2 and 3 do not have star");
      CHALLENGE_CHECK(recs[0].score <= recs[1].score &&
                          recs[1].score <= recs[2].score,
                      "Recommendations sorted by score ascending (best first)");

      // Candidate spacing check: pairwise difference >= 30 deg
      double d01 =
          std::fabs(std::remainder(recs[0].angleDeg - recs[1].angleDeg, 360.0));
      double d02 =
          std::fabs(std::remainder(recs[0].angleDeg - recs[2].angleDeg, 360.0));
      double d12 =
          std::fabs(std::remainder(recs[1].angleDeg - recs[2].angleDeg, 360.0));
      CHALLENGE_CHECK(d01 >= 29.9 && d02 >= 29.9 && d12 >= 29.9,
                      "Recommendation angles well-separated (d01=%.1f, "
                      "d02=%.1f, d12=%.1f deg)",
                      d01, d02, d12);

      // Magnet snap verification
      double r0 = recs[0].angleDeg;
      CHALLENGE_CHECK(
          nearVal(doc.snapAngle(r0 + 1.2, 2.0), r0, 1e-4),
          "Angle within tolerance (+1.2 deg) snaps to recommendation");
      CHALLENGE_CHECK(
          nearVal(doc.snapAngle(r0 - 1.5, 2.0), r0, 1e-4),
          "Angle within tolerance (-1.5 deg) snaps to recommendation");
      CHALLENGE_CHECK(nearVal(doc.snapAngle(r0 + 3.0, 2.0), r0 + 3.0, 1e-4),
                      "Angle outside tolerance (+3.0 deg) does NOT snap");
    }
  }

  // =========================================================================
  // Challenge 4: ExportDialog UI Stress & Crash Safety
  // =========================================================================
  std::printf(
      "\n--- Challenge 4: ExportDialog Crash Safety Subprocess Test ---\n");
  {
    QProcess proc;
    proc.start(QGuiApplication::applicationFilePath(),
               QStringList() << "--test-dialog-stress");
    bool finished = proc.waitForFinished(10000);
    CHALLENGE_CHECK(finished, "ExportDialog stress test completed within 10s");
    CHALLENGE_CHECK(proc.exitStatus() == QProcess::NormalExit,
                    "ExportDialog finished with NormalExit (no crash)");
    CHALLENGE_CHECK(proc.exitCode() == 0,
                    "ExportDialog exit code is 0 (got %d)", proc.exitCode());
  }

  // =========================================================================
  // Challenge 5: End-to-End Export File Integrity
  // =========================================================================
  std::printf(
      "\n--- Challenge 5: End-to-End Export File Format Integrity ---\n");
  {
    ArtifactDoc doc;
    doc.loadSample(true);

    ExportRequest req;
    QString err;

    // SVG 14-Layers
    req.path = QDir(outDir).filePath("test_chal3_14layers.svg");
    req.opts.spec14Layers = true;
    CHALLENGE_CHECK(exportSvg14Layers(doc, req, &err),
                    "exportSvg14Layers succeeded");
    QFile fSvg(req.path);
    CHALLENGE_CHECK(fSvg.open(QIODevice::ReadOnly) && fSvg.size() > 500,
                    "SVG file exists and has non-trivial size (%lld bytes)",
                    fSvg.size());
    fSvg.close();

    // DXF R12
    req.path = QDir(outDir).filePath("test_chal3_cad.dxf");
    CHALLENGE_CHECK(exportDxf(doc, req, &err), "exportDxf succeeded");
    QFile fDxf(req.path);
    CHALLENGE_CHECK(fDxf.open(QIODevice::ReadOnly) && fDxf.size() > 500,
                    "DXF file exists and has non-trivial size (%lld bytes)",
                    fDxf.size());
    QString dxfContent = QString::fromUtf8(fDxf.readAll());
    fDxf.close();
    CHALLENGE_CHECK(dxfContent.contains("AC1009") && dxfContent.contains("EOF"),
                    "DXF file contains AC1009 header and EOF marker");

    // PDF
    req.path = QDir(outDir).filePath("test_chal3_print.pdf");
    CHALLENGE_CHECK(exportPdf(doc, req, &err), "exportPdf succeeded");
    QFile fPdf(req.path);
    CHALLENGE_CHECK(fPdf.open(QIODevice::ReadOnly) && fPdf.size() > 1000,
                    "PDF file exists and has valid size (%lld bytes)",
                    fPdf.size());
    fPdf.close();

    // PNG 300 DPI
    req.path = QDir(outDir).filePath("test_chal3_image.png");
    CHALLENGE_CHECK(exportPng(doc, req, 300, &err), "exportPng succeeded");
    QImage img;
    CHALLENGE_CHECK(img.load(req.path) && !img.isNull(),
                    "PNG loaded back into QImage successfully (%dx%d)",
                    img.width(), img.height());
  }

  std::printf(
      "\n=================================================================\n");
  std::printf("=== Challenger 3 Summary: Total=%d, Passed=%d, Failed=%d ===\n",
              g_totalTests, g_passedTests, g_failedTests);
  std::printf(
      "=================================================================\n");

  return g_failedTests == 0 ? 0 : 1;
}
