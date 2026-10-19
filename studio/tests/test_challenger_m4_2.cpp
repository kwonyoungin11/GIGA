// DongArch3D Studio — Milestone 4 Challenger 2 (Final Milestone)
// Empirical White-Box Adversarial Hardening Suite
// Focus: 3-step workflow state machine, extreme UI interactions,
// 100mm calibration rectangle in SVG/PDF, refusePreview export guard.

#include "ArtifactDoc.h"
#include "DimensionAnalyzer.h"
#include "MeshLoader.h"
#include "MeshSlicer.h"
#include "Plate.h"
#include "SyntheticMeshGen.h"

#include <QApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QImage>
#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <numbers>
#include <random>
#include <string>
#include <vector>

using namespace studio;
namespace KA = DongArch::KArch;
namespace KAT = DongArch::KArch::Testing;

static int g_totalChecks = 0;
static int g_passedChecks = 0;
static int g_failedChecks = 0;

#define CHALLENGE_CHECK(cond, desc, ...)                                       \
  do {                                                                         \
    ++g_totalChecks;                                                           \
    if (cond) {                                                                \
      std::printf("  [PASS] " desc "\n", ##__VA_ARGS__);                       \
      ++g_passedChecks;                                                        \
    } else {                                                                   \
      std::printf("  [FAIL] " desc "\n", ##__VA_ARGS__);                       \
      ++g_failedChecks;                                                        \
    }                                                                          \
  } while (0)

static bool nearVal(double a, double b, double tol = 1e-5) {
  return std::fabs(a - b) <= tol;
}

int main(int argc, char **argv) {
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  qputenv("QT_QPA_PLATFORM", "offscreen");
  QApplication app(argc, argv);

  std::printf(
      "=================================================================\n");
  std::printf(
      "=== Challenger 2: Milestone 4 Tier 5 Adversarial Hardening   ===\n");
  std::printf(
      "=================================================================\n");

  const QString outDir =
      (argc > 1) ? QString::fromLocal8Bit(argv[1]) : QDir::tempPath();
  QDir().mkpath(outDir);

  // ---------------------------------------------------------------------------
  // Suite 1: 3-Step Workflow State Machine Transitions
  // ---------------------------------------------------------------------------
  std::printf("\n--- Suite 1: 3-Step Workflow State Machine Transitions ---\n");
  {
    ArtifactDoc doc;
    doc.loadSample(false); // Load sample in initial unconfirmed pose
    CHALLENGE_CHECK(doc.isLoaded(), "1.1: Document is loaded");
    CHALLENGE_CHECK(!doc.poseConfirmed(),
                    "1.2: Initial pose is unconfirmed (Step 1)");

    // Step 1 Constraint: Unit locking
    doc.setUnitConfirmed(false);
    CHALLENGE_CHECK(!doc.isUnitConfirmed(), "1.3: Unit explicitly unconfirmed");
    doc.confirmPose();
    CHALLENGE_CHECK(!doc.poseConfirmed(),
                    "1.4: confirmPose() blocked while unit is unconfirmed");

    doc.confirmUnit(1.0);
    CHALLENGE_CHECK(doc.isUnitConfirmed(), "1.5: Unit confirmed");
    doc.autoAlign();
    doc.confirmPose();
    CHALLENGE_CHECK(doc.poseConfirmed(),
                    "1.6: confirmPose() succeeded -> transitioned to Step 2/3");

    // Canceling mid-slicing / resetting while previewing:
    doc.previewFrontAngle(45.0);
    CHALLENGE_CHECK(doc.isPreviewing(),
                    "1.7: Entered previewFrontAngle (dragging)");
    CHALLENGE_CHECK(nearVal(doc.previewAngle(), 45.0),
                    "1.8: Preview angle is 45.0 deg");

    // Pose manipulation while dragging must cleanly unconfirm and reset preview
    doc.rotatePose(PoseAxis::Z, 15.0);
    CHALLENGE_CHECK(!doc.poseConfirmed(), "1.9: rotatePose unconfirmed pose");
    CHALLENGE_CHECK(!doc.isPreviewing(),
                    "1.10: rotatePose cleanly aborted previewing state");

    // Re-confirm pose
    doc.autoAlign();
    doc.confirmPose();
    CHALLENGE_CHECK(doc.poseConfirmed(), "1.11: Pose re-confirmed");

    // Preview and commit
    doc.previewFrontAngle(60.0);
    CHALLENGE_CHECK(doc.isPreviewing(), "1.12: Previewing at 60 deg");
    doc.commitPreview();
    CHALLENGE_CHECK(!doc.isPreviewing(),
                    "1.13: commitPreview() returned isPreviewing to false");
    CHALLENGE_CHECK(nearVal(doc.frontAngle(), 60.0),
                    "1.14: frontAngle() committed to 60.0 deg");
  }

  // ---------------------------------------------------------------------------
  // Suite 2: Extreme UI Interactions (Rapid Dragging Stress)
  // ---------------------------------------------------------------------------
  std::printf(
      "\n--- Suite 2: Extreme UI Interactions (Rapid Dragging Stress) ---\n");
  {
    ArtifactDoc doc;
    doc.loadSample(true); // Confirmed pose

    std::mt19937_64 rng(42);
    std::uniform_real_distribution<double> dist(-720.0, 720.0);

    const int iterations = 5000;
    auto t0 = std::chrono::steady_clock::now();
    for (int i = 0; i < iterations; ++i) {
      double angle = dist(rng);
      doc.previewFrontAngle(angle);
    }
    auto t1 = std::chrono::steady_clock::now();
    double totalMs = std::chrono::duration<double, std::milli>(t1 - t0).count();
    double avgMs = totalMs / double(iterations);

    std::printf(
        "  Rapid dragging: %d iterations in %.2f ms (avg: %.4f ms/op)\n",
        iterations, totalMs, avgMs);
    CHALLENGE_CHECK(
        avgMs < 2.0,
        "2.1: Rapid dragging average latency < 2.0 ms (got %.4f ms)", avgMs);
    CHALLENGE_CHECK(doc.isPreviewing(),
                    "2.2: isPreviewing() is true after dragging");

    // Test extreme boundary values
    doc.previewFrontAngle(180.0);
    CHALLENGE_CHECK(doc.isPreviewing(), "2.3: previewFrontAngle(+180) ok");
    doc.previewFrontAngle(-180.0);
    CHALLENGE_CHECK(doc.isPreviewing(), "2.4: previewFrontAngle(-180) ok");
    doc.previewFrontAngle(0.0);
    CHALLENGE_CHECK(doc.isPreviewing(), "2.5: previewFrontAngle(0) ok");
    doc.previewFrontAngle(3600.0);
    CHALLENGE_CHECK(doc.isPreviewing(), "2.6: previewFrontAngle(3600) ok");

    doc.commitPreview();
    CHALLENGE_CHECK(!doc.isPreviewing(),
                    "2.7: commitPreview successfully terminated drag");
  }

  // ---------------------------------------------------------------------------
  // Suite 3: Rapid Toggle of Sherd Mode (Boundary & Heap Stability)
  // ---------------------------------------------------------------------------
  std::printf("\n--- Suite 3: Rapid Toggle of Sherd Mode ---\n");
  {
    ArtifactDoc doc;
    doc.loadSample(true);
    const double wholeBase = doc.dims().base;
    CHALLENGE_CHECK(wholeBase > 0.0,
                    "3.1: Whole vessel has base > 0 (got %.2f mm)", wholeBase);

    // Rapidly toggle sherd mode 1,000 times
    for (int i = 0; i < 1000; ++i) {
      doc.setSherdMode(true);
      if (i % 250 == 0) {
        CHALLENGE_CHECK(doc.isSherdMode(), "3.2a: Sherd mode active");
        CHALLENGE_CHECK(doc.dims().base == 0.0,
                        "3.2b: Sherd base is 0.0 (blank principle)");
        CHALLENGE_CHECK(doc.estimatedRimDiameter() > 0.0,
                        "3.2c: Estimated rim dia > 0");
      }
      doc.setSherdMode(false);
      if (i % 250 == 0) {
        CHALLENGE_CHECK(!doc.isSherdMode(), "3.2d: Whole mode active");
        CHALLENGE_CHECK(doc.dims().base > 0.0, "3.2e: Whole base restored");
      }
    }
    CHALLENGE_CHECK(
        nearVal(doc.dims().base, wholeBase, 1e-4),
        "3.3: Base diameter perfectly preserved after 1000 toggles");
  }

  // ---------------------------------------------------------------------------
  // Suite 4: Refusal to Export While Dragging (refusePreview Guard)
  // ---------------------------------------------------------------------------
  std::printf("\n--- Suite 4: refusePreview Guard All Export Formats ---\n");
  {
    ArtifactDoc doc;
    doc.loadSample(true);

    ExportRequest req;
    req.paper = Paper::A4Landscape;
    req.scale = 1.0;
    req.title = QStringLiteral("Guard Test Artifact");
    req.opts.spec14Layers = true;

    // Normal export when not dragging: all formats succeed
    QString err;
    req.path = QDir(outDir).filePath("guard_ok_14.svg");
    CHALLENGE_CHECK(exportSvg14Layers(doc, req, &err),
                    "4.1a: exportSvg14Layers ok when not dragging");
    req.path = QDir(outDir).filePath("guard_ok_7.svg");
    CHALLENGE_CHECK(exportSvgLayered(doc, req, &err),
                    "4.1b: exportSvgLayered ok when not dragging");
    req.path = QDir(outDir).filePath("guard_ok.pdf");
    CHALLENGE_CHECK(exportPdf(doc, req, &err),
                    "4.1c: exportPdf ok when not dragging");
    req.path = QDir(outDir).filePath("guard_ok.png");
    CHALLENGE_CHECK(exportPng(doc, req, 150, &err),
                    "4.1d: exportPng ok when not dragging");
    req.path = QDir(outDir).filePath("guard_ok.dxf");
    CHALLENGE_CHECK(exportDxf(doc, req, &err),
                    "4.1e: exportDxf ok when not dragging");

    // Begin dragging
    doc.previewFrontAngle(37.5);
    CHALLENGE_CHECK(doc.isPreviewing(),
                    "4.2: Dragging active (isPreviewing == true)");

    // All 5 exports must be strictly refused
    req.path = QDir(outDir).filePath("guard_blocked_14.svg");
    err.clear();
    CHALLENGE_CHECK(!exportSvg14Layers(doc, req, &err),
                    "4.3a: exportSvg14Layers refused during drag");
    CHALLENGE_CHECK(err.contains(QStringLiteral("놓은 뒤")),
                    "4.3b: err contains Korean guidance");

    req.path = QDir(outDir).filePath("guard_blocked_7.svg");
    err.clear();
    CHALLENGE_CHECK(!exportSvgLayered(doc, req, &err),
                    "4.3c: exportSvgLayered refused during drag");
    CHALLENGE_CHECK(err.contains(QStringLiteral("놓은 뒤")),
                    "4.3d: err contains Korean guidance");

    req.path = QDir(outDir).filePath("guard_blocked.pdf");
    err.clear();
    CHALLENGE_CHECK(!exportPdf(doc, req, &err),
                    "4.3e: exportPdf refused during drag");
    CHALLENGE_CHECK(err.contains(QStringLiteral("놓은 뒤")),
                    "4.3f: err contains Korean guidance");

    req.path = QDir(outDir).filePath("guard_blocked.png");
    err.clear();
    CHALLENGE_CHECK(!exportPng(doc, req, 150, &err),
                    "4.3g: exportPng refused during drag");
    CHALLENGE_CHECK(err.contains(QStringLiteral("놓은 뒤")),
                    "4.3h: err contains Korean guidance");

    req.path = QDir(outDir).filePath("guard_blocked.dxf");
    err.clear();
    CHALLENGE_CHECK(!exportDxf(doc, req, &err),
                    "4.3i: exportDxf refused during drag");
    CHALLENGE_CHECK(err.contains(QStringLiteral("놓은 뒤")),
                    "4.3j: err contains Korean guidance");

    // Commit drag and re-test
    doc.commitPreview();
    CHALLENGE_CHECK(!doc.isPreviewing(), "4.4: commitPreview completed");
    CHALLENGE_CHECK(exportSvg14Layers(doc, req, &err),
                    "4.5: exportSvg14Layers ok immediately after commit");
  }

  // ---------------------------------------------------------------------------
  // Suite 5: 100mm Calibration Rectangle Validity in SVG & PDF
  // ---------------------------------------------------------------------------
  std::printf(
      "\n--- Suite 5: 100mm Calibration Rectangle Validity (SVG & PDF) ---\n");
  {
    ArtifactDoc doc;
    doc.loadSample(true);

    ExportRequest req;
    req.paper = Paper::A4Portrait;
    req.scale = 1.0;
    req.title = QStringLiteral("Calibration Test");
    req.opts.spec14Layers = true;

    // Test with calibration100mm = true
    req.opts.calibration100mm = true;
    req.path = QDir(outDir).filePath("test_calib_m4_true.svg");
    QString err;
    CHALLENGE_CHECK(exportSvg14Layers(doc, req, &err),
                    "5.1a: exportSvg14Layers with calib=true");

    QFile fTrue(req.path);
    CHALLENGE_CHECK(fTrue.open(QIODevice::ReadOnly), "5.1b: SVG opened");
    QString svgTrue = QString::fromUtf8(fTrue.readAll());
    fTrue.close();

    // Check layer group and rect tag
    CHALLENGE_CHECK(
        svgTrue.contains(
            "<rect x=\"20\" y=\"182\" width=\"100\" height=\"100\"/>"),
        "5.2: SVG contains exact 100x100mm rect at (20, 182)");
    CHALLENGE_CHECK(svgTrue.contains("CALIBRATION 100 × 100 mm"),
                    "5.3: SVG contains exact label text");

    // Test with calibration100mm = false
    req.opts.calibration100mm = false;
    req.path = QDir(outDir).filePath("test_calib_m4_false.svg");
    CHALLENGE_CHECK(exportSvg14Layers(doc, req, &err),
                    "5.4a: exportSvg14Layers with calib=false");

    QFile fFalse(req.path);
    CHALLENGE_CHECK(fFalse.open(QIODevice::ReadOnly), "5.4b: SVG opened");
    QString svgFalse = QString::fromUtf8(fFalse.readAll());
    fFalse.close();
    CHALLENGE_CHECK(
        !svgFalse.contains(
            "<rect x=\"20\" y=\"182\" width=\"100\" height=\"100\"/>"),
        "5.5: SVG with calib=false correctly omits calibration rect");

    // Test PDF export with calibration
    req.opts.calibration100mm = true;
    req.path = QDir(outDir).filePath("test_calib_m4.pdf");
    CHALLENGE_CHECK(exportPdf(doc, req, &err),
                    "5.6a: exportPdf with calib=true succeeded");
    QFileInfo fiPdf(req.path);
    CHALLENGE_CHECK(fiPdf.exists() && fiPdf.size() > 2048,
                    "5.6b: PDF file valid and non-trivial size");
  }

  // ---------------------------------------------------------------------------
  // Suite 6: Multiple Successive 14-Layer Exports Stress
  // ---------------------------------------------------------------------------
  std::printf(
      "\n--- Suite 6: Multiple Successive 14-Layer Exports Stress ---\n");
  {
    ArtifactDoc doc;
    doc.loadSample(true);

    ExportRequest req;
    req.paper = Paper::A4Portrait;
    req.scale = 1.0;
    req.title = QStringLiteral("Successive Export Stress");
    req.opts.spec14Layers = true;
    req.opts.calibration100mm = true;

    const int loopCount = 20;
    int successSvg = 0;
    int successPdf = 0;
    int successDxf = 0;
    int successPng = 0;

    for (int i = 0; i < loopCount; ++i) {
      QString err;
      req.path =
          QDir(outDir).filePath(QStringLiteral("stress_14_%1.svg").arg(i));
      if (exportSvg14Layers(doc, req, &err))
        ++successSvg;

      req.path = QDir(outDir).filePath(QStringLiteral("stress_%1.pdf").arg(i));
      if (exportPdf(doc, req, &err))
        ++successPdf;

      req.path = QDir(outDir).filePath(QStringLiteral("stress_%1.dxf").arg(i));
      if (exportDxf(doc, req, &err))
        ++successDxf;

      req.path = QDir(outDir).filePath(QStringLiteral("stress_%1.png").arg(i));
      if (exportPng(doc, req, 100, &err))
        ++successPng;
    }

    CHALLENGE_CHECK(successSvg == loopCount,
                    "6.1: %d/%d successive 14-layer SVG exports succeeded",
                    successSvg, loopCount);
    CHALLENGE_CHECK(successPdf == loopCount,
                    "6.2: %d/%d successive PDF exports succeeded", successPdf,
                    loopCount);
    CHALLENGE_CHECK(successDxf == loopCount,
                    "6.3: %d/%d successive DXF exports succeeded", successDxf,
                    loopCount);
    CHALLENGE_CHECK(successPng == loopCount,
                    "6.4: %d/%d successive PNG exports succeeded", successPng,
                    loopCount);
  }

  std::printf(
      "\n=================================================================\n");
  std::printf("=== Challenger 2 Summary: Total=%d, Passed=%d, Failed=%d ===\n",
              g_totalChecks, g_passedChecks, g_failedChecks);
  std::printf(
      "=================================================================\n");

  return g_failedChecks == 0 ? 0 : 1;
}
