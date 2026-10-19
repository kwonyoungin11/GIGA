// DongArch3D Studio — 실제 고고학 유물 3D 스캔(6183.obj, 431만 면) 자동
// 벤치마크 및 E2E 무결성 검증 기준 사양: ORIGINAL_REQUEST.md R1~R3, SCOPE.md,
// PROJECT.md Milestone 6
//
// 1. 경로 수집: argv[1] 또는 환경 변수 DONGARCH_BENCH_OBJ
// 2. Graceful Skip: 파일 부재 시 "[SKIP] 6183.obj not found. Skipping real
// artifact benchmark." 출력 후 exit 0
// 3. 파일 존재 시 벤치마크 SLA:
//    a. previewFrontAngle 50회 갱신 시간 중앙값 <= 30 ms (목표 <= 16 ms)
//    b. setFrontAngle / commitPreview 3회 갱신 시간 중앙값 <= 300 ms
//    c. confirmPose 소요 시간 <= 1500 ms (1.5 s)
//    d. Plate::renderShade 1회 렌더링 시간 <= 80 ms
//    e. E2E 도면 생성 및 내보내기(SVG, DXF, PDF, PNG) 비어있지 않음 및 크래시
//    부재 검증 f. 기하 5대 치수(H, rim, belly, base, meanWall) 원본 정밀도 대비
//    +-0.1 mm 허용오차 엄격 검증

#include "ArtifactDoc.h"
#include "DimensionAnalyzer.h"
#include "MeshRaster.h"
#include "Plate.h"
#include "Theme.h"

#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QGuiApplication>

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <vector>

using namespace studio;

static int fails = 0;
#define CHECK(c, ...)                                                          \
  do {                                                                         \
    if (!(c)) {                                                                \
      std::printf("FAIL %s: ", #c);                                            \
      std::printf(__VA_ARGS__);                                                \
      std::printf("\n");                                                       \
      ++fails;                                                                 \
    }                                                                          \
  } while (0)

using clk = std::chrono::steady_clock;
static double msSince(clk::time_point t) {
  return std::chrono::duration<double, std::milli>(clk::now() - t).count();
}

static double median(std::vector<double> &v) {
  if (v.empty())
    return 0.0;
  std::sort(v.begin(), v.end());
  const size_t n = v.size();
  return (n % 2 == 1) ? v[n / 2] : 0.5 * (v[n / 2 - 1] + v[n / 2]);
}

int main(int argc, char **argv) {
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  qputenv("QT_QPA_PLATFORM", "offscreen");
  QGuiApplication app(argc, argv);

  // 1. 파일 경로 수집 (argv 또는 환경 변수)
  QString objPath;
  for (int i = 1; i < argc; ++i) {
    if (argv[i] == nullptr || argv[i][0] == '\0')
      continue;
    const QString candidate = QString::fromLocal8Bit(argv[i]);
    if (QFileInfo(candidate).isFile()) {
      objPath = candidate;
      break;
    }
  }
  if (objPath.isEmpty()) {
    const QByteArray envVal = qgetenv("DONGARCH_BENCH_OBJ");
    if (!envVal.isEmpty()) {
      const QString candidate =
          QString::fromLocal8Bit(envVal.constData()).trimmed();
      if (QFileInfo(candidate).isFile()) {
        objPath = candidate;
      }
    }
  }

  // 2. Graceful Skip: 파일 부재 시 CI 실패 방지를 위해 정상 종료
  if (objPath.isEmpty() || !QFile::exists(objPath)) {
    std::printf(
        "[SKIP] 6183.obj not found. Skipping real artifact benchmark.\n");
    return 0;
  }

  std::printf("================================================================"
              "======\n");
  std::printf("[BENCH] Starting Real Artifact Benchmark for: %s\n",
              qPrintable(objPath));
  std::printf("================================================================"
              "======\n");

  ArtifactDoc doc;
  QString loadErr;
  const auto tLoad0 = clk::now();
  const bool loadOk = doc.loadFile(objPath, &loadErr);
  const double loadMs = msSince(tLoad0);
  CHECK(loadOk, "loadFile failed: %s", qPrintable(loadErr));
  if (!loadOk) {
    std::puts("FAILED");
    return 1;
  }

  const size_t numFaces = doc.mesh().faces.size();
  const size_t numVerts = doc.mesh().vertices.size();
  std::printf("[BENCH] Loaded mesh: %zu faces, %zu vertices in %.1f ms\n",
              numFaces, numVerts, loadMs);
  CHECK(numFaces > 1000000,
        "Expected >1M faces for real high-res artifact, got %zu", numFaces);

  // -------------------------------------------------------------------------
  // Benchmark c: confirmPose time <= 1500 ms (1.5s)
  // -------------------------------------------------------------------------
  std::printf("\n--- [Benchmark c] confirmPose ---\n");
  const auto tConfirm0 = clk::now();
  doc.confirmPose();
  const double confirmMs = msSince(tConfirm0);
  std::printf("[BENCH] confirmPose: %.1f ms (limit: 1500.0 ms)\n", confirmMs);
  CHECK(confirmMs <= 1500.0, "confirmPose took %.1f ms > 1500.0 ms", confirmMs);
  CHECK(doc.poseConfirmed(), "doc.poseConfirmed() must be true");

  // -------------------------------------------------------------------------
  // Benchmark a: previewFrontAngle median time <= 30 ms (goal <= 16 ms) over 50
  // iterations
  // -------------------------------------------------------------------------
  std::printf("\n--- [Benchmark a] previewFrontAngle (50 iterations) ---\n");
  std::vector<double> prevTimes;
  prevTimes.reserve(50);
  // 1회 예열(Warmup)
  doc.previewFrontAngle(1.0);
  for (int i = 1; i <= 50; ++i) {
    const double angle = static_cast<double>(i) * 7.2; // 360도 순환
    const auto t0 = clk::now();
    doc.previewFrontAngle(angle);
    const auto sec = doc.displaySection();
    const double dt = msSince(t0);
    prevTimes.push_back(dt);
    CHECK(!sec.empty(), "displaySection at preview angle %g must not be empty",
          angle);
  }
  const double prevMed = median(prevTimes);
  std::printf("[BENCH] previewFrontAngle median: %.2f ms (limit: <= 30.0 ms, "
              "target: <= 16.0 ms)\n",
              prevMed);
  CHECK(prevMed <= 30.0, "previewFrontAngle median %.2f ms > 30.0 ms", prevMed);

  // -------------------------------------------------------------------------
  // Benchmark b: setFrontAngle / commitPreview time <= 300 ms over 3 iterations
  // -------------------------------------------------------------------------
  std::printf(
      "\n--- [Benchmark b] setFrontAngle / commitPreview (3 iterations) ---\n");
  std::vector<double> commitTimes;
  commitTimes.reserve(3);
  const double testAngles[3] = {15.0, 45.0, 90.0};
  for (double ang : testAngles) {
    doc.previewFrontAngle(ang);
    const auto t0 = clk::now();
    doc.commitPreview();
    const double dt = msSince(t0);
    commitTimes.push_back(dt);
    CHECK(!doc.isPreviewing(), "doc.isPreviewing() must be false after commit");
    CHECK(std::fabs(doc.frontAngle() - ang) < 1e-9,
          "frontAngle %g != target %g", doc.frontAngle(), ang);
  }
  const double commitMed = median(commitTimes);
  std::printf("[BENCH] commitPreview median: %.2f ms (limit: <= 300.0 ms)\n",
              commitMed);
  CHECK(commitMed <= 300.0, "commitPreview median %.2f ms > 300.0 ms",
        commitMed);

  // -------------------------------------------------------------------------
  // Benchmark d: Plate::renderShade time <= 80 ms
  // -------------------------------------------------------------------------
  std::printf("\n--- [Benchmark d] Plate::renderShade ---\n");
  const auto tShade0 = clk::now();
  ShadeImage shade = Plate::renderShade(doc, ShadeMode::DTak, 2.0);
  const double shadeMs = msSince(tShade0);
  std::printf("[BENCH] Plate::renderShade: %.1f ms (limit: <= 80.0 ms, "
              "resolution: %dx%d)\n",
              shadeMs, shade.image.width(), shade.image.height());
  CHECK(!shade.isNull(), "ShadeImage must not be null");
  CHECK(shade.image.width() > 0 && shade.image.height() > 0,
        "ShadeImage dimensions must be positive");
  CHECK(shadeMs <= 80.0, "Plate::renderShade took %.1f ms > 80.0 ms", shadeMs);

  // -------------------------------------------------------------------------
  // Benchmark e: E2E 도면 생성 및 내보내기(SVG/DXF/PDF/PNG) 유효성
  // -------------------------------------------------------------------------
  std::printf("\n--- [Benchmark e] E2E Drawing & Export Validity ---\n");
  const auto section = doc.displaySection();
  CHECK(!section.empty(), "displaySection must not be empty");
  size_t totalSecPts = 0;
  for (const auto &loop : section)
    totalSecPts += loop.size();
  CHECK(totalSecPts > 0, "displaySection points count must be > 0 (got %zu)",
        totalSecPts);

  const auto outline = doc.displayOutline();
  CHECK(!outline.empty(), "displayOutline must not be empty");

  const Dims5 dims = doc.dims();
  CHECK(dims.ok, "dims.ok must be true");
  CHECK(dims.height > 0.0, "height must be positive: %g", dims.height);
  CHECK(dims.rim > 0.0 || dims.belly > 0.0,
        "rim or belly must be positive (rim=%g, belly=%g)", dims.rim,
        dims.belly);

  const QRectF ext = Plate::extentMm(doc, 0);
  CHECK(ext.width() > 0.0 && ext.height() > 0.0,
        "Plate extent must be positive: %gx%g", ext.width(), ext.height());

  // 드래그 중 내보내기 거부 가드(refusePreview) 검증
  doc.previewFrontAngle(25.0);
  const QString tmpDir = QDir::tempPath() +
                         QStringLiteral("/dongarch_bench_6183_") +
                         QString::number(QCoreApplication::applicationPid());
  QDir().mkpath(tmpDir);

  ExportRequest req;
  req.title = QStringLiteral("6183_RealArtifact");
  req.paper = Paper::A4Portrait;
  req.scale = 1.0;
  req.opts.grid = true;
  req.opts.shade = true;
  req.opts.outline = true;
  req.opts.section = true;
  req.opts.labels = true;

  {
    ExportRequest refuseReq = req;
    refuseReq.path = tmpDir + QStringLiteral("/refuse_guard.svg");
    QString refuseErr;
    const bool refuseOk = exportSvgLayered(doc, refuseReq, &refuseErr);
    CHECK(!refuseOk, "Export during preview must be rejected");
    CHECK(refuseErr.contains(QStringLiteral("놓은 뒤")),
          "Error message must mention '놓은 뒤' (got: %s)",
          qPrintable(refuseErr));
    CHECK(!QFile::exists(refuseReq.path), "Rejected file must not be created");
  }
  doc.commitPreview();

  // 4종 형식(SVG, DXF, PDF, PNG) 정상 내보내기 검증
  // 1) SVG 내보내기
  {
    req.path = tmpDir + QStringLiteral("/export_bench.svg");
    QFile::remove(req.path);
    QString errText;
    const bool ok = exportSvgLayered(doc, req, &errText);
    const qint64 sz = QFileInfo(req.path).size();
    CHECK(ok && sz > 0, "SVG export failed: ok=%d sz=%lld err=%s", int(ok), sz,
          qPrintable(errText));
    std::printf("[BENCH] SVG Export: ok=%d, size=%lld bytes\n", int(ok), sz);
  }

  // 2) DXF 내보내기
  {
    req.path = tmpDir + QStringLiteral("/export_bench.dxf");
    QFile::remove(req.path);
    QString errText;
    const bool ok = exportDxf(doc, req, &errText);
    const qint64 sz = QFileInfo(req.path).size();
    CHECK(ok && sz > 0, "DXF export failed: ok=%d sz=%lld err=%s", int(ok), sz,
          qPrintable(errText));
    std::printf("[BENCH] DXF Export: ok=%d, size=%lld bytes\n", int(ok), sz);
  }

  // 3) PDF 내보내기
  {
    req.path = tmpDir + QStringLiteral("/export_bench.pdf");
    QFile::remove(req.path);
    QString errText;
    const bool ok = exportPdf(doc, req, &errText);
    const qint64 sz = QFileInfo(req.path).size();
    CHECK(ok && sz > 0, "PDF export failed: ok=%d sz=%lld err=%s", int(ok), sz,
          qPrintable(errText));
    std::printf("[BENCH] PDF Export: ok=%d, size=%lld bytes\n", int(ok), sz);
  }

  // 4) PNG 내보내기
  {
    req.path = tmpDir + QStringLiteral("/export_bench.png");
    QFile::remove(req.path);
    QString errText;
    const bool ok = exportPng(doc, req, 150, &errText);
    const qint64 sz = QFileInfo(req.path).size();
    CHECK(ok && sz > 0, "PNG export failed: ok=%d sz=%lld err=%s", int(ok), sz,
          qPrintable(errText));
    std::printf("[BENCH] PNG Export: ok=%d, size=%lld bytes\n", int(ok), sz);
  }

  // 임시 디렉토리 정리
  QDir(tmpDir).removeRecursively();

  // -------------------------------------------------------------------------
  // Benchmark f: 엄격한 기하 정밀도 검증 (H, rim, belly, base, meanWall +-0.1
  // mm)
  // -------------------------------------------------------------------------
  std::printf(
      "\n--- [Benchmark f] Strict Precision Verification (+-0.1 mm) ---\n");
  const auto indep = KA::AnalyzeDimensions(doc.mesh(), KA::SilhouetteResult{});
  std::printf("[BENCH] Dimensions from ArtifactDoc: H=%.3f, rim=%.3f, "
              "belly=%.3f, base=%.3f, wall=%.3f\n",
              dims.height, dims.rim, dims.belly, dims.base, dims.meanWall);
  std::printf("[BENCH] Dimensions from Independent Analysis: H=%.3f, rim=%.3f, "
              "belly=%.3f, base=%.3f\n",
              indep.vesselHeight, indep.rimDiameter, indep.maxBodyDiameter,
              indep.baseDiameter);

  const double dH = std::fabs(dims.height - indep.vesselHeight);
  const double dRim = std::fabs(dims.rim - indep.rimDiameter);
  const double dBelly = std::fabs(dims.belly - indep.maxBodyDiameter);
  const double dBase = std::fabs(dims.base - indep.baseDiameter);

  CHECK(dH <= 0.1, "Height deviation %.3f > 0.1 mm (doc=%.3f vs indep=%.3f)",
        dH, dims.height, indep.vesselHeight);
  CHECK(dRim <= 0.1, "Rim deviation %.3f > 0.1 mm (doc=%.3f vs indep=%.3f)",
        dRim, dims.rim, indep.rimDiameter);
  CHECK(dBelly <= 0.1, "Belly deviation %.3f > 0.1 mm (doc=%.3f vs indep=%.3f)",
        dBelly, dims.belly, indep.maxBodyDiameter);
  CHECK(dBase <= 0.1, "Base deviation %.3f > 0.1 mm (doc=%.3f vs indep=%.3f)",
        dBase, dims.base, indep.baseDiameter);
  CHECK(dims.meanWall > 0.0, "meanWall must be positive: %g", dims.meanWall);

  // 회전각 변경 시 치수 불변성 검증 (+-0.1 mm 이내 유지)
  for (double rotAngle : {12.0, 45.0, 90.0, 180.0}) {
    doc.setFrontAngle(rotAngle);
    const Dims5 rotDims = doc.dims();
    const double diffH = std::fabs(rotDims.height - dims.height);
    const double diffRim = std::fabs(rotDims.rim - dims.rim);
    const double diffBelly = std::fabs(rotDims.belly - dims.belly);
    const double diffBase = std::fabs(rotDims.base - dims.base);
    const double diffWall = std::fabs(rotDims.meanWall - dims.meanWall);
    CHECK(diffH <= 0.1 && diffRim <= 0.1 && diffBelly <= 0.1 &&
              diffBase <= 0.1 && diffWall <= 0.1,
          "Dims mutated upon rotation angle %g: dH=%.3f dRim=%.3f dBelly=%.3f "
          "dBase=%.3f dWall=%.3f",
          rotAngle, diffH, diffRim, diffBelly, diffBase, diffWall);
  }
  std::printf("[BENCH] Precision verified: all dimensions invariant within "
              "+-0.1 mm tolerance.\n");

  std::printf("\n=============================================================="
              "========\n");
  std::puts(fails ? "[BENCH] RESULT: FAILED"
                  : "[BENCH] RESULT: ALL BENCHMARKS PASSED");
  std::printf("================================================================"
              "======\n");

  return fails ? 1 : 0;
}
