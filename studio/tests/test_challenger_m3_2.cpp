// DongArch3D Studio — Milestone 3 (Features 24~33) Challenger 2 Empirical Test
// Suite Rigorous verification of Step 3 features and export pipelines: Feature
// 24: Neck line detection & rendering (solid exterior, long dashed interior)
// Feature 25: Break mark rendering (0.13mm double short diagonal lines)
// Feature 26: Missing base blank principle ("—" when sherd has no base)
// Feature 27: Un-scanned inner wall dotted estimation isolated from average
// wall thickness Feature 28: refusePreview guard blocks export while dragging
// (isPreviewing() == true) Feature 29: 14-layer SVG export structure
// (Inkscape/Illustrator layer groups) Feature 30: CAD DXF R12 ($INSUNITS=4, 11
// layers) Feature 31: 600 DPI PDF export Feature 32: 300 DPI PNG export Feature
// 33: 100 mm calibration square rendering & verification

#include "ArtifactDoc.h"
#include "ExportDialog.h"
#include "MeshLoader.h"
#include "Plate.h"
#include "SheetView.h"
#include "SyntheticMeshGen.h"
#include "Theme.h"

#include <QApplication>
#include <QDir>
#include <QFile>
#include <QImage>
#include <QLabel>
#include <QProcess>
#include <QPushButton>
#include <QXmlStreamReader>
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

using namespace studio;
namespace KA = DongArch::KArch;

static int fails = 0;
#define CHALLENGE_CHECK(cond, ...)                                             \
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

static bool nearVal(double a, double b, double tol) {
  return std::fabs(a - b) <= tol;
}

int main(int argc, char **argv) {
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  qputenv("QT_QPA_PLATFORM", "offscreen");
  QApplication app(argc, argv);

  if (argc > 1 && std::strcmp(argv[1], "--test-dialog") == 0) {
    ArtifactDoc doc;
    doc.loadSample(true);
    PlateOptions opts;
    auto *dlg = new ExportDialog(doc, 0, opts, ExportFormat::Svg);
    delete dlg;
    return 0;
  }

  const QString outDir =
      argc > 1 ? QString::fromLocal8Bit(argv[1]) : QDir::currentPath();

  std::printf(
      "===============================================================\n");
  std::printf("=== Challenger 2: Milestone 3 (Features 24~33) Empirical "
              "Verification ===\n");
  std::printf(
      "===============================================================\n\n");

  // =========================================================================
  // Suite 1: Feature 24 - Neck Line Detection & Rendering
  // =========================================================================
  std::printf(
      "--- Suite 1: Feature 24 (Neck Line Detection & Rendering) ---\n");
  {
    ArtifactDoc doc;
    doc.loadSample(true); // confirms pose

    // 1.1 Detection on standard vessel
    const PlateRefLines r = plateRefLines(doc, 3.0);
    CHALLENGE_CHECK(r.neck.has_value(),
                    "neck line must be detected on standard vessel");
    if (r.neck.has_value()) {
      CHALLENGE_CHECK(
          nearVal(r.neck->a.y, r.neck->b.y, 1e-4),
          "neck line must be perfectly horizontal: y1=%.3f, y2=%.3f",
          r.neck->a.y, r.neck->b.y);
      CHALLENGE_CHECK(r.neck->a.x < 0.0,
                      "neck line left x must be negative: %.3f", r.neck->a.x);
      CHALLENGE_CHECK(r.neck->b.x > 0.0,
                      "neck line right x must be positive: %.3f", r.neck->b.x);
      CHALLENGE_CHECK(
          nearVal(r.neck->a.x + r.neck->b.x, 0.0, 1e-4),
          "neck line must be symmetric about central axis: %.3f vs %.3f",
          r.neck->a.x, r.neck->b.x);
      const double H = doc.dims().height;
      CHALLENGE_CHECK(r.neck->a.y >= 0.55 * H && r.neck->a.y <= 0.95 * H,
                      "neck Z must lie within [0.55H, 0.95H]: Z=%.2f, H=%.2f",
                      r.neck->a.y, H);
      CHALLENGE_CHECK(
          r.neck->b.x < doc.dims().rim * 0.95,
          "neck radius (%.2f) must be constricted relative to rim (%.2f)",
          r.neck->b.x, doc.dims().rim / 2.0);
    }

    // 1.2 Edge cases: Unloaded doc and unconfirmed pose
    {
      ArtifactDoc unconfirmedDoc;
      unconfirmedDoc.loadSample(false);
      PlateRefLines unconfRefs = plateRefLines(unconfirmedDoc, 3.0);
      CHALLENGE_CHECK(!unconfRefs.neck.has_value(),
                      "neck line must not exist for unconfirmed pose");
      ArtifactDoc emptyDoc;
      PlateRefLines emptyRefs = plateRefLines(emptyDoc, 3.0);
      CHALLENGE_CHECK(!emptyRefs.neck.has_value(),
                      "neck line must not exist for empty doc");
    }

    // 1.3 Edge case: Flat dish / plate (Height <= 50mm)
    {
      KA::Testing::SyntheticParams sp;
      sp.radialSeg = 64;
      sp.heightSeg = 32;
      sp.capTop = false;
      sp.capBottom = false;
      sp.tiltDeg = 0.0;
      // Cylinder with H = 30 mm, R = 60 mm (flat dish shape)
      const auto dish = KA::Testing::MakeCylinderShell(60.0, 30.0, 4.0, sp);
      KA::MeshData dishMesh = dish.toMesh();
      KA::FillMeshStats(dishMesh);

      ArtifactDoc dishDoc;
      dishDoc.loadMesh(std::move(dishMesh), {}, QStringLiteral("Dish30mm"));
      dishDoc.autoAlign();
      dishDoc.confirmPose();
      PlateRefLines dishRefs = plateRefLines(dishDoc, 3.0);
      CHALLENGE_CHECK(!dishRefs.neck.has_value(),
                      "neck line must NOT be created for flat dish with H <= "
                      "50mm and no neck");
    }

    // 1.4 Rendering: 14-Layer SVG neck lines
    {
      ExportRequest req;
      req.path = QDir(outDir).filePath("test_neck_layer.svg");
      req.opts.spec14Layers = true;
      QString err;
      CHALLENGE_CHECK(exportSvg14Layers(doc, req, &err),
                      "exportSvg14Layers for neck check");

      QFile f(req.path);
      CHALLENGE_CHECK(f.open(QIODevice::ReadOnly), "open neck SVG file");
      QString content = QString::fromUtf8(f.readAll());
      f.close();

      // refline.rim layer should contain the solid exterior line and dashed
      // interior line
      CHALLENGE_CHECK(content.contains("inkscape:label=\"구연선\""),
                      "SVG contains refline.rim layer");
      CHALLENGE_CHECK(content.contains("stroke-dasharray=\"12,4\""),
                      "SVG contains neck long dashed interior (12,4)");
    }
  }

  // =========================================================================
  // Suite 2: Feature 25 - Break Mark Rendering
  // =========================================================================
  std::printf("\n--- Suite 2: Feature 25 (Break Mark Rendering) ---\n");
  {
    ArtifactDoc doc;
    doc.loadSample(true);

    // 2.1 Sherd mode: break marks must be generated
    doc.setSherdMode(true);
    CHALLENGE_CHECK(doc.isSherdMode(), "sherd mode enabled");
    {
      ExportRequest req;
      req.path = QDir(outDir).filePath("test_break_sherd.svg");
      req.opts.spec14Layers = true;
      QString err;
      CHALLENGE_CHECK(exportSvg14Layers(doc, req, &err),
                      "exportSvg14Layers with sherdMode=true");

      QFile f(req.path);
      CHALLENGE_CHECK(f.open(QIODevice::ReadOnly), "open sherd SVG file");
      QXmlStreamReader xml(&f);
      int breakLineCount = 0;
      int breakDepth = 0;
      while (!xml.atEnd()) {
        if (xml.readNext() == QXmlStreamReader::StartElement) {
          if (xml.name() == QLatin1String("g")) {
            if (breakDepth > 0) {
              ++breakDepth;
            } else if (xml.attributes().value(QLatin1String("id")) ==
                       QLatin1String("break")) {
              breakDepth = 1;
            }
          } else if (breakDepth > 0 && xml.name() == QLatin1String("line")) {
            ++breakLineCount;
          }
        } else if (xml.tokenType() == QXmlStreamReader::EndElement) {
          if (xml.name() == QLatin1String("g") && breakDepth > 0) {
            --breakDepth;
          }
        }
      }
      f.close();
      CHALLENGE_CHECK(breakLineCount == 4,
                      "break layer must contain exactly 4 lines (2 double "
                      "short diagonal pairs), got %d",
                      breakLineCount);
    }

    // 2.2 Whole vessel mode: break marks must NOT be drawn
    doc.setSherdMode(false);
    CHALLENGE_CHECK(!doc.isSherdMode(), "sherd mode disabled");
    {
      ExportRequest req;
      req.path = QDir(outDir).filePath("test_break_whole.svg");
      req.opts.spec14Layers = true;
      QString err;
      CHALLENGE_CHECK(exportSvg14Layers(doc, req, &err),
                      "exportSvg14Layers with sherdMode=false");

      QFile f(req.path);
      CHALLENGE_CHECK(f.open(QIODevice::ReadOnly),
                      "open whole vessel SVG file");
      QXmlStreamReader xml(&f);
      int breakLineCount = 0;
      int breakDepth = 0;
      while (!xml.atEnd()) {
        if (xml.readNext() == QXmlStreamReader::StartElement) {
          if (xml.name() == QLatin1String("g")) {
            if (breakDepth > 0) {
              ++breakDepth;
            } else if (xml.attributes().value(QLatin1String("id")) ==
                       QLatin1String("break")) {
              breakDepth = 1;
            }
          } else if (breakDepth > 0 && xml.name() == QLatin1String("line")) {
            ++breakLineCount;
          }
        } else if (xml.tokenType() == QXmlStreamReader::EndElement) {
          if (xml.name() == QLatin1String("g") && breakDepth > 0) {
            --breakDepth;
          }
        }
      }
      f.close();
      CHALLENGE_CHECK(
          breakLineCount == 0,
          "break layer must contain 0 lines for whole vessel, got %d",
          breakLineCount);
    }
  }

  // =========================================================================
  // Suite 3: Feature 26 - Missing Base Blank Principle ("—")
  // =========================================================================
  std::printf("\n--- Suite 3: Feature 26 (Missing Base Blank Principle) ---\n");
  {
    ArtifactDoc doc;
    doc.loadSample(true);

    // 3.1 Whole vessel baseline: base diameter > 0
    CHALLENGE_CHECK(doc.dims().base > 0.0,
                    "whole vessel base diameter is > 0: %.2f", doc.dims().base);

    // 3.2 Sherd mode enabled: base diameter becomes 0.0
    doc.setSherdMode(true);
    doc.confirmPose();
    CHALLENGE_CHECK(doc.dims().base == 0.0,
                    "sherd mode base diameter must be 0.0");

    // 3.3 Verify 14-Layer SVG output
    {
      ExportRequest req;
      req.path = QDir(outDir).filePath("test_blank_base_14.svg");
      req.opts.spec14Layers = true;
      req.title = QStringLiteral("구연부편_도판");
      QString err;
      CHALLENGE_CHECK(exportSvg14Layers(doc, req, &err),
                      "exportSvg14Layers for sherd blank base");

      QFile f(req.path);
      CHALLENGE_CHECK(f.open(QIODevice::ReadOnly), "open blank base SVG");
      QString content = QString::fromUtf8(f.readAll());
      f.close();

      // Check refline.base layer has NO lines
      int baseLayerPos = content.indexOf("id=\"refline.base\"");
      CHALLENGE_CHECK(baseLayerPos != -1, "refline.base layer exists");
      int endBaseLayer = content.indexOf("</g>", baseLayerPos);
      QString baseLayerSlice =
          content.mid(baseLayerPos, endBaseLayer - baseLayerPos);
      CHALLENGE_CHECK(!baseLayerSlice.contains("<line"),
                      "refline.base must have no line for sherd");

      // Check label has "저경 — mm"
      CHALLENGE_CHECK(content.contains(QString::fromUtf8("저경 — mm")),
                      "SVG title box must contain '저경 — mm'");
    }

    // 3.4 Verify 7-Layer SVG output
    {
      ExportRequest req;
      req.path = QDir(outDir).filePath("test_blank_base_7.svg");
      req.opts.spec14Layers = false;
      req.title = QStringLiteral("구연부편_도판7");
      QString err;
      CHALLENGE_CHECK(exportSvgLayered(doc, req, &err),
                      "exportSvgLayered for sherd blank base");

      QFile f(req.path);
      CHALLENGE_CHECK(f.open(QIODevice::ReadOnly),
                      "open 7-layer blank base SVG");
      QString content = QString::fromUtf8(f.readAll());
      f.close();
      CHALLENGE_CHECK(content.contains(QString::fromUtf8("저경 — mm")),
                      "7-layer SVG title box must contain '저경 — mm'");
    }

    // 3.5 Revert sherd mode
    doc.setSherdMode(false);
    doc.confirmPose();
    CHALLENGE_CHECK(doc.dims().base > 0.0,
                    "reverted whole vessel base diameter > 0: %.2f",
                    doc.dims().base);
  }

  // =========================================================================
  // Suite 4: Feature 27 - Un-scanned Inner Wall Dotted Estimation Isolation
  // =========================================================================
  std::printf("\n--- Suite 4: Feature 27 (Inner Wall Dotted Estimation "
              "Isolation) ---\n");
  {
    ArtifactDoc blindDoc;
    blindDoc.loadArtecBlindSample(true);

    CHALLENGE_CHECK(blindDoc.hasEstimatedInner(),
                    "blind sample must have estimated inner wall");
    const double meanWall = blindDoc.dims().meanWall;
    CHALLENGE_CHECK(
        meanWall > 1.5 && meanWall < 35.0,
        "meanWall must be calculated within realistic range: %.2f mm",
        meanWall);

    // Verify isWallEstimatedAt(z) functionality
    const double H = blindDoc.dims().height;
    // Lower 65% is blind, top 35% is scanned
    CHALLENGE_CHECK(blindDoc.isWallEstimatedAt(0.20 * H),
                    "z=0.20H must be estimated inner wall region");
    CHALLENGE_CHECK(blindDoc.isWallEstimatedAt(0.50 * H),
                    "z=0.50H must be estimated inner wall region");

    // wallThicknessAt check
    double thickAtBlind = blindDoc.wallThicknessAt(0.30 * H);
    CHALLENGE_CHECK(nearVal(thickAtBlind, blindDoc.estimatedWallMm(), 1e-3),
                    "wallThicknessAt in blind region must return "
                    "estimatedWallMm (%.2f vs %.2f)",
                    thickAtBlind, blindDoc.estimatedWallMm());

    // SVG export check for inner wall dashed styling
    ExportRequest req;
    req.path = QDir(outDir).filePath("test_blind_inner.svg");
    req.opts.spec14Layers = true;
    QString err;
    CHALLENGE_CHECK(exportSvg14Layers(blindDoc, req, &err),
                    "exportSvg14Layers on blind sample");

    QFile f(req.path);
    CHALLENGE_CHECK(f.open(QIODevice::ReadOnly), "open blind sample SVG");
    QString content = QString::fromUtf8(f.readAll());
    f.close();

    CHALLENGE_CHECK(content.contains("id=\"inner\""),
                    "SVG contains inner layer");
    CHALLENGE_CHECK(content.contains("stroke-dasharray=\"2.0,1.3\""),
                    "SVG inner wall contains dotted dasharray '2.0,1.3'");
  }

  // =========================================================================
  // Suite 5: Feature 28 - refusePreview Export Guard
  // =========================================================================
  std::printf("\n--- Suite 5: Feature 28 (refusePreview Export Guard) ---\n");
  {
    ArtifactDoc doc;
    doc.loadSample(true);

    // 5.1 Normal state: exports must succeed
    ExportRequest req;
    QString err;

    req.path = QDir(outDir).filePath("test_guard_normal.svg");
    req.opts.spec14Layers = false;
    CHALLENGE_CHECK(exportSvgLayered(doc, req, &err),
                    "exportSvgLayered normal succeeds");

    req.path = QDir(outDir).filePath("test_guard_normal_14.svg");
    req.opts.spec14Layers = true;
    CHALLENGE_CHECK(exportSvg14Layers(doc, req, &err),
                    "exportSvg14Layers normal succeeds");

    req.path = QDir(outDir).filePath("test_guard_normal.pdf");
    CHALLENGE_CHECK(exportPdf(doc, req, &err), "exportPdf normal succeeds");

    req.path = QDir(outDir).filePath("test_guard_normal.png");
    CHALLENGE_CHECK(exportPng(doc, req, 300, &err),
                    "exportPng normal succeeds");

    req.path = QDir(outDir).filePath("test_guard_normal.dxf");
    CHALLENGE_CHECK(exportDxf(doc, req, &err), "exportDxf normal succeeds");

    // 5.2 Dragging/Previewing state: all exports MUST be rejected
    doc.previewFrontAngle(45.0);
    CHALLENGE_CHECK(doc.isPreviewing(), "doc is in previewing state");

    req.path = QDir(outDir).filePath("test_guard_blocked.svg");
    req.opts.spec14Layers = false;
    err.clear();
    CHALLENGE_CHECK(!exportSvgLayered(doc, req, &err),
                    "exportSvgLayered must fail in preview");
    CHALLENGE_CHECK(!err.isEmpty(), "error returned: %s", qUtf8Printable(err));

    req.opts.spec14Layers = true;
    err.clear();
    CHALLENGE_CHECK(!exportSvg14Layers(doc, req, &err),
                    "exportSvg14Layers must fail in preview");
    CHALLENGE_CHECK(!err.isEmpty(), "error returned: %s", qUtf8Printable(err));

    req.path = QDir(outDir).filePath("test_guard_blocked.pdf");
    err.clear();
    CHALLENGE_CHECK(!exportPdf(doc, req, &err),
                    "exportPdf must fail in preview");
    CHALLENGE_CHECK(!err.isEmpty(), "error returned: %s", qUtf8Printable(err));

    req.path = QDir(outDir).filePath("test_guard_blocked.png");
    err.clear();
    CHALLENGE_CHECK(!exportPng(doc, req, 300, &err),
                    "exportPng must fail in preview");
    CHALLENGE_CHECK(!err.isEmpty(), "error returned: %s", qUtf8Printable(err));

    req.path = QDir(outDir).filePath("test_guard_blocked.dxf");
    err.clear();
    CHALLENGE_CHECK(!exportDxf(doc, req, &err),
                    "exportDxf must fail in preview");
    CHALLENGE_CHECK(!err.isEmpty(), "error returned: %s", qUtf8Printable(err));

    // 5.3 Commit preview: all exports must succeed again
    doc.commitPreview();
    CHALLENGE_CHECK(!doc.isPreviewing(),
                    "doc is no longer previewing after commit");
    CHALLENGE_CHECK(exportSvg14Layers(doc, req, &err),
                    "exportSvg14Layers succeeds after commitPreview");

    // 5.4 Subsequent preview cycle: all exports must succeed again after commit
    doc.previewFrontAngle(90.0);
    CHALLENGE_CHECK(doc.isPreviewing(), "doc is previewing again");
    doc.commitPreview();
    CHALLENGE_CHECK(!doc.isPreviewing(),
                    "doc is no longer previewing after commitPreview");
    CHALLENGE_CHECK(exportPdf(doc, req, &err),
                    "exportPdf succeeds after second commitPreview");
  }

  // =========================================================================
  // Suite 6: Feature 29 - 14-Layer SVG Export Structure
  // =========================================================================
  std::printf("\n--- Suite 6: Feature 29 (14-Layer SVG Structure) ---\n");
  {
    ArtifactDoc doc;
    doc.loadSample(true);

    ExportRequest req;
    req.path = QDir(outDir).filePath("test_14layers_structure.svg");
    req.opts.spec14Layers = true;
    req.opts.calibration100mm = true;
    req.title = QStringLiteral("백자_단경호_1호");
    QString err;
    CHALLENGE_CHECK(exportSvg14Layers(doc, req, &err),
                    "exportSvg14Layers execution");

    QFile f(req.path);
    CHALLENGE_CHECK(f.open(QIODevice::ReadOnly), "open 14-layer SVG file");
    QXmlStreamReader xml(&f);

    QStringList layerIds;
    QStringList layerLabels;
    while (!xml.atEnd()) {
      if (xml.readNext() == QXmlStreamReader::StartElement) {
        if (xml.name() == QLatin1String("g") &&
            xml.attributes().value(QLatin1String("inkscape:groupmode")) ==
                QLatin1String("layer")) {
          layerIds << xml.attributes().value(QLatin1String("id")).toString();
          layerLabels << xml.attributes()
                             .value(QLatin1String("inkscape:label"))
                             .toString();
        }
      }
    }
    f.close();
    CHALLENGE_CHECK(!xml.hasError(), "SVG XML parsing must be clean, error: %s",
                    qUtf8Printable(xml.errorString()));

    CHALLENGE_CHECK(layerIds.size() == 14,
                    "must contain exactly 14 layer groups, got %lld",
                    static_cast<long long>(layerIds.size()));

    const QStringList expectedIds = {
        "grid",     "shade",          "calibration", "reconstruction",
        "inner",    "refline.center", "refline.rim", "refline.base",
        "outline",  "section",        "break",       "dimension",
        "scalebar", "label"};

    for (int i = 0; i < expectedIds.size(); ++i) {
      if (i < layerIds.size()) {
        CHALLENGE_CHECK(
            layerIds[i] == expectedIds[i], "Layer %d must be '%s', got '%s'",
            i + 1, qUtf8Printable(expectedIds[i]), qUtf8Printable(layerIds[i]));
      } else {
        CHALLENGE_CHECK(false, "Missing expected layer '%s'",
                        qUtf8Printable(expectedIds[i]));
      }
    }
  }

  // =========================================================================
  // Suite 7: Feature 30 - CAD DXF R12 ($INSUNITS=4, layers)
  // =========================================================================
  std::printf("\n--- Suite 7: Feature 30 (CAD DXF R12 Export) ---\n");
  {
    ArtifactDoc doc;
    doc.loadSample(true);

    ExportRequest req;
    req.path = QDir(outDir).filePath("test_cad_r12.dxf");
    QString err;
    CHALLENGE_CHECK(exportDxf(doc, req, &err), "exportDxf execution");

    QFile f(req.path);
    CHALLENGE_CHECK(f.open(QIODevice::ReadOnly | QIODevice::Text),
                    "open DXF file");
    QString content = QString::fromUtf8(f.readAll());
    f.close();

    // Verify AutoCAD R12 version: $ACADVER = AC1009
    CHALLENGE_CHECK(content.contains("$ACADVER\n1\nAC1009"),
                    "DXF contains $ACADVER = AC1009 (Release 12)");

    // Verify millimeter units: $INSUNITS = 4
    CHALLENGE_CHECK(content.contains("$INSUNITS\n70\n4"),
                    "DXF contains $INSUNITS = 4 (Millimeters)");

    // Verify all 11 standard layers
    const std::vector<std::string> expectedDxfLayers = {
        "01_AXIS_CENTER",     "02_EXTERIOR_OUTLINE", "03_EXTERIOR_FEATURES",
        "04_SECTION_WALL",    "05_SECTION_HATCH",    "06_INTERIOR_OUTLINE",
        "07_PROJECTION_RIM",  "08_PROJECTION_BASE",  "09_DIMENSION_MEASURE",
        "10_ANNOTATION_TEXT", "11_SCALE_BAR"};

    for (const auto &ly : expectedDxfLayers) {
      std::string pattern = "2\n" + ly + "\n";
      CHALLENGE_CHECK(content.contains(QString::fromStdString(pattern)),
                      "DXF defines layer '%s'", ly.c_str());
    }

    // Verify file termination
    CHALLENGE_CHECK(content.endsWith("0\nEOF\n") || content.contains("0\nEOF"),
                    "DXF properly terminated with 0\\nEOF");

    // Verify no NaN or Inf strings
    CHALLENGE_CHECK(!content.contains("nan", Qt::CaseInsensitive) &&
                        !content.contains("inf", Qt::CaseInsensitive),
                    "DXF contains no NaN or Inf coordinate values");
  }

  // =========================================================================
  // Suite 8: Feature 31 - 600 DPI PDF Export
  // =========================================================================
  std::printf("\n--- Suite 8: Feature 31 (600 DPI PDF Export) ---\n");
  {
    ArtifactDoc doc;
    doc.loadSample(true);

    ExportRequest req;
    req.path = QDir(outDir).filePath("test_print_600dpi.pdf");
    req.paper = Paper::A4Portrait;
    req.scale = 1.0;
    req.title = QStringLiteral("600DPI_테스트_도판");
    req.opts.calibration100mm = true;
    QString err;
    CHALLENGE_CHECK(exportPdf(doc, req, &err), "exportPdf execution");

    QFile f(req.path);
    CHALLENGE_CHECK(f.exists(), "PDF file exists");
    CHALLENGE_CHECK(f.size() > 2048, "PDF file size is substantial: %lld bytes",
                    f.size());
    CHALLENGE_CHECK(f.open(QIODevice::ReadOnly), "open PDF file");
    QByteArray head = f.read(10);
    CHALLENGE_CHECK(head.startsWith("%PDF-1."),
                    "PDF file header matches %%PDF-1.x: %s", head.constData());
    f.close();

    // Test different papers & scales
    req.path = QDir(outDir).filePath("test_print_a4l.pdf");
    req.paper = Paper::A4Landscape;
    req.scale = 0.5;
    CHALLENGE_CHECK(exportPdf(doc, req, &err),
                    "exportPdf A4 Landscape scale 0.5");

    req.path = QDir(outDir).filePath("test_print_a3p.pdf");
    req.paper = Paper::A3Portrait;
    req.scale = 2.0;
    CHALLENGE_CHECK(exportPdf(doc, req, &err),
                    "exportPdf A3 Portrait scale 2.0");
  }

  // =========================================================================
  // Suite 9: Feature 32 - 300 DPI PNG Export
  // =========================================================================
  std::printf("\n--- Suite 9: Feature 32 (300 DPI PNG Export) ---\n");
  {
    ArtifactDoc doc;
    doc.loadSample(true);

    ExportRequest req;
    req.path = QDir(outDir).filePath("test_raster_300dpi.png");
    req.paper = Paper::A4Portrait;
    req.scale = 1.0;
    req.title = QStringLiteral("300DPI_테스트_도판");
    req.opts.calibration100mm = true;
    QString err;
    CHALLENGE_CHECK(exportPng(doc, req, 300, &err),
                    "exportPng 300 DPI execution");

    QFile f(req.path);
    CHALLENGE_CHECK(f.exists(), "PNG file exists");
    CHALLENGE_CHECK(f.size() > 10000,
                    "PNG file size is substantial: %lld bytes", f.size());

    // Load back and verify dimensions & DPI
    QImage img;
    CHALLENGE_CHECK(img.load(req.path), "QImage load generated PNG");
    CHALLENGE_CHECK(!img.isNull(), "QImage is valid");

    // A4 is 210 x 297 mm. At 300 DPI:
    // W = 210 * 300 / 25.4 = 2480.3 px
    // H = 297 * 300 / 25.4 = 3507.8 px
    CHALLENGE_CHECK(std::abs(img.width() - 2480) <= 2,
                    "PNG width matches 300 DPI A4 (expected ~2480, got %d)",
                    img.width());
    CHALLENGE_CHECK(std::abs(img.height() - 3507) <= 2,
                    "PNG height matches 300 DPI A4 (expected ~3507, got %d)",
                    img.height());

    // Check dots per meter (300 / 0.0254 = 11811)
    CHALLENGE_CHECK(img.dotsPerMeterX() >= 11810 &&
                        img.dotsPerMeterX() <= 11812,
                    "PNG dotsPerMeterX is ~11811: %d", img.dotsPerMeterX());
    CHALLENGE_CHECK(img.dotsPerMeterY() >= 11810 &&
                        img.dotsPerMeterY() <= 11812,
                    "PNG dotsPerMeterY is ~11811: %d", img.dotsPerMeterY());

    // Check non-blank pixels
    int nonWhitePixels = 0;
    for (int y = 0; y < img.height(); y += 20) {
      for (int x = 0; x < img.width(); x += 20) {
        QRgb p = img.pixel(x, y);
        if (qRed(p) < 240 || qGreen(p) < 240 || qBlue(p) < 240) {
          ++nonWhitePixels;
        }
      }
    }
    CHALLENGE_CHECK(nonWhitePixels > 50,
                    "PNG image has content (sampled non-white pixel count: %d)",
                    nonWhitePixels);
  }

  // =========================================================================
  // Suite 10: Feature 33 - 100 mm Calibration Square Rendering & Verification
  // =========================================================================
  std::printf("\n--- Suite 10: Feature 33 (100 mm Calibration Square) ---\n");
  {
    ArtifactDoc doc;
    doc.loadSample(true);

    // 10.1 SVG with calibration=true vs calibration=false
    {
      ExportRequest reqTrue;
      reqTrue.path = QDir(outDir).filePath("test_calib_true.svg");
      reqTrue.opts.spec14Layers = true;
      reqTrue.opts.calibration100mm = true;
      QString err;
      CHALLENGE_CHECK(exportSvg14Layers(doc, reqTrue, &err),
                      "exportSvg14Layers with calib=true");

      QFile fTrue(reqTrue.path);
      CHALLENGE_CHECK(fTrue.open(QIODevice::ReadOnly), "open calib_true SVG");
      QString contentTrue = QString::fromUtf8(fTrue.readAll());
      fTrue.close();

      CHALLENGE_CHECK(
          contentTrue.contains(
              "<rect x=\"20\" y=\"182\" width=\"100\" height=\"100\"/>"),
          "SVG contains exact 100x100 mm rect at (20, 182)");
      CHALLENGE_CHECK(contentTrue.contains("CALIBRATION 100 × 100 mm"),
                      "SVG contains calibration text");

      ExportRequest reqFalse;
      reqFalse.path = QDir(outDir).filePath("test_calib_false.svg");
      reqFalse.opts.spec14Layers = true;
      reqFalse.opts.calibration100mm = false;
      CHALLENGE_CHECK(exportSvg14Layers(doc, reqFalse, &err),
                      "exportSvg14Layers with calib=false");

      QFile fFalse(reqFalse.path);
      CHALLENGE_CHECK(fFalse.open(QIODevice::ReadOnly), "open calib_false SVG");
      QString contentFalse = QString::fromUtf8(fFalse.readAll());
      fFalse.close();

      CHALLENGE_CHECK(
          !contentFalse.contains(
              "<rect x=\"20\" y=\"182\" width=\"100\" height=\"100\"/>"),
          "SVG with calib=false does NOT render calibration rect");
    }

    // 10.2 ExportDialog UI verification recording check
    {
      std::printf("  Testing ExportDialog constructor...\n");
      QProcess proc;
      proc.start(QGuiApplication::applicationFilePath(),
                 QStringList() << "--test-dialog");
      proc.waitForFinished(5000);
      if (proc.exitStatus() == QProcess::CrashExit) {
        CHALLENGE_CHECK(false,
                        "CRITICAL BUG: ExportDialog crashed with Access "
                        "Violation on instantiation! Exit code: 0x%08X (cGrid_ "
                        "uninitialized in ExportDialog.cpp:162)",
                        static_cast<unsigned int>(proc.exitCode()));
      } else {
        CHALLENGE_CHECK(
            proc.exitCode() == 0,
            "ExportDialog instantiated successfully (exit code: %d)",
            proc.exitCode());
      }
    }
  }

  std::printf(
      "\n===============================================================\n");
  std::printf("Milestone 3 Challenger 2 Finished. Total Failures: %d\n", fails);
  std::printf(
      "===============================================================\n");

  return fails == 0 ? 0 : 1;
}
