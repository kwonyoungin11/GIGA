// DongArch3D Studio — [JOB-014] 진짜 전개 탁본(원통 전개 + 요철 먹 농담) · 끌어서 범위 고르기 · 한 바퀴 전개 · 1:1 저장 · 캐시 검증
// 기준 사양: spec_and_plan_job014.md (AC-1 ~ AC-6)
// 허용오차: 전개 폭은 호 길이와 1 px, 범위 변환은 0.5° / 0.5 mm (정사영 역산식은 닫힌 해라 오차는 반올림뿐).
#include "ArtifactDoc.h"
#include "MeshLoader.h"
#include "Rubbing.h"

#include <QDir>
#include <QElapsedTimer>
#include <QFileInfo>
#include <QGuiApplication>
#include <QImage>
#include <QImageReader>
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <numbers>

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

// 반경 50 mm 원통(높이 0..100 mm, 축 = Z) 외면. 돌대(z 40~44, +1 mm)와 홈(z 60~62, −1 mm).
static double profileR(double z) {
  double r = 50.0;
  if (z >= 40.0 && z <= 44.0)
    r += 1.0;
  if (z >= 60.0 && z <= 62.0)
    r -= 1.0;
  return r;
}

static KA::MeshData makeRidgedCylinder() {
  KA::MeshData m;
  const int seg = 720, rings = 401; // 0.5° · 0.25 mm
  m.vertices.reserve(size_t(seg) * rings);
  for (int j = 0; j < rings; ++j) {
    const double z = 0.25 * j;
    const double r = profileR(z);
    for (int i = 0; i < seg; ++i) {
      const double a = 2.0 * std::numbers::pi * i / seg;
      m.vertices.push_back({r * std::cos(a), r * std::sin(a), z});
    }
  }
  for (int j = 0; j + 1 < rings; ++j)
    for (int i = 0; i < seg; ++i) {
      const int a = j * seg + i, b = j * seg + (i + 1) % seg;
      const int c = (j + 1) * seg + i, d = (j + 1) * seg + (i + 1) % seg;
      m.faces.push_back({a, b, d});
      m.faces.push_back({a, d, c});
    }
  KA::FillMeshStats(m);
  return m;
}

// 이미지 행(y) 범위의 유효 화소 평균 밝기 (0..255)
static double rowMeanGray(const QImage &img, int y0, int y1) {
  double s = 0;
  int n = 0;
  for (int y = std::max(0, y0); y <= std::min(img.height() - 1, y1); ++y)
    for (int x = img.width() / 4; x < img.width() * 3 / 4; ++x) {
      s += qGray(img.pixel(x, y));
      ++n;
    }
  return n ? s / n : -1.0;
}

int main(int argc, char **argv) {
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  qputenv("QT_QPA_PLATFORM", "offscreen");
  QGuiApplication app(argc, argv);
  const QString outDir =
      argc > 1 ? QString::fromLocal8Bit(argv[1]) : QDir::currentPath();
  std::printf("=== [JOB-014] Unrolled Rubbing Test Suite ===\n");

  // ---- AC-1 진짜 전개 탁본 ----------------------------------------------------
  std::printf("\n--- [AC-1] unrolled relief rubbing on ridged cylinder ---\n");
  const KA::MeshData cyl = makeRidgedCylinder();
  RubbingRegion reg;
  reg.azCenterDeg = 90.0;
  reg.azSpanDeg = 60.0;
  reg.zLo = 30.0;
  reg.zHi = 70.0;
  RubbingParams prm;
  prm.pxPerMm = 6.0;
  const UnrolledRubbing rub = renderUnrolledRubbing(cyl, reg, prm);
  CHECK(rub.ok(), "rubbing image must exist");
  const double arcMm = 50.0 * reg.azSpanDeg * std::numbers::pi / 180.0;
  CHECK(std::fabs(rub.refRadiusMm - 50.0) < 0.6, "reference radius %.3f",
        rub.refRadiusMm);
  CHECK(std::fabs(rub.widthMm - arcMm) < 0.6,
        "unrolled width %.2f mm == arc length %.2f mm", rub.widthMm, arcMm);
  CHECK(std::abs(rub.image.width() - int(std::lround(arcMm * prm.pxPerMm))) <= 1,
        "image width %d px vs %ld px", rub.image.width(),
        std::lround(arcMm * prm.pxPerMm));
  CHECK(std::fabs(rub.heightMm - 40.0) < 1e-6, "height %.2f mm", rub.heightMm);
  CHECK(rub.coverage > 0.97, "coverage %.3f", rub.coverage);
  // 행 y = (zHi − z)·ppm
  auto rowOf = [&](double z) { return int(std::lround((reg.zHi - z) * prm.pxPerMm)); };
  const double gRidge = rowMeanGray(rub.image, rowOf(43.5), rowOf(40.5));
  const double gGroove = rowMeanGray(rub.image, rowOf(61.7), rowOf(60.3));
  const double gFlat = rowMeanGray(rub.image, rowOf(52.0), rowOf(50.0));
  std::printf("  gray ridge=%.1f flat=%.1f groove=%.1f\n", gRidge, gFlat, gGroove);
  CHECK(gRidge < gFlat, "raised ridge must be darker (ink) than flat surface");
  CHECK(gGroove > gFlat + 60.0, "groove must stay white paper (%.1f vs %.1f)",
        gGroove, gFlat);
  CHECK(gGroove > 200.0, "groove near paper white (%.1f)", gGroove);
  rub.image.save(outDir + QStringLiteral("/rubbing_cylinder.png"));

  // ---- AC-2 화면 상자 → 범위 ---------------------------------------------------
  std::printf("\n--- [AC-2] screen box -> rubbing region ---\n");
  {
    RasterView v;
    v.yawDeg = 0.0;
    v.pitchDeg = 0.0;
    v.pxPerMm = 2.0;
    v.centerPx = QPointF(200, 200);
    v.centerZ = 50.0;
    // x1 ∈ [−25, 25] mm, z ∈ [30, 70] mm, 막 30° 회전된 화면
    const QRectF box(QPointF(150, 160), QPointF(250, 240));
    const RubbingRegion r =
        regionFromScreenBox(box, v, 30.0, [](double) { return 50.0; });
    std::printf("  center=%.2f span=%.2f z=[%.2f, %.2f]\n", r.azCenterDeg,
                r.azSpanDeg, r.zLo, r.zHi);
    CHECK(std::fabs(r.azSpanDeg - 60.0) < 0.5, "span 60 deg (%.2f)", r.azSpanDeg);
    CHECK(std::fabs(std::remainder(r.azCenterDeg - 240.0, 360.0)) < 0.5,
          "center 240 deg (%.2f)", r.azCenterDeg);
    CHECK(std::fabs(r.zLo - 30.0) < 0.5 && std::fabs(r.zHi - 70.0) < 0.5,
          "z range 30..70 (%.2f..%.2f)", r.zLo, r.zHi);
    // 깨진 편 너머로 뒤쪽 벽 안쪽이 보일 때: 같은 상자 = 뒤 벽 α ∈ [60°, 120°] → 원본 30°..90°
    const RubbingRegion rf =
        regionFromScreenBox(box, v, 30.0, [](double) { return 50.0; }, true);
    CHECK(std::fabs(rf.azSpanDeg - 60.0) < 0.5 &&
              std::fabs(std::remainder(rf.azCenterDeg - 60.0, 360.0)) < 0.5,
          "far wall center 60 deg span 60 (%.2f, %.2f)", rf.azCenterDeg, rf.azSpanDeg);
  }

  // ---- AC-4 한 바퀴 전개 --------------------------------------------------------
  std::printf("\n--- [AC-4] full-turn unrolled band ---\n");
  {
    RubbingRegion full = reg;
    full.azSpanDeg = 360.0;
    RubbingParams p2;
    p2.pxPerMm = 3.0;
    const UnrolledRubbing fr = renderUnrolledRubbing(cyl, full, p2);
    const double circ = 2.0 * std::numbers::pi * 50.0;
    CHECK(fr.ok() && std::fabs(fr.widthMm - circ) < 1.5,
          "full turn width %.2f mm vs circumference %.2f mm", fr.widthMm, circ);
    CHECK(fr.coverage > 0.97, "full turn coverage %.3f (seam closed)", fr.coverage);
    fr.image.save(outDir + QStringLiteral("/rubbing_fullturn.png"));
  }

  // ---- AC-5 1:1 실제 크기 PNG ----------------------------------------------------
  std::printf("\n--- [AC-5] 1:1 PNG with dpi ---\n");
  {
    const QString path = outDir + QStringLiteral("/rubbing_600dpi.png");
    QString err;
    const bool ok = saveRubbingPng(cyl, reg, path, 600.0, &err);
    CHECK(ok, "saveRubbingPng: %s", qPrintable(err));
    QImage back(path);
    const double mmW = back.width() / (back.dotsPerMeterX() / 1000.0);
    CHECK(!back.isNull() && std::fabs(mmW - arcMm) < 0.5,
          "saved PNG physical width %.2f mm vs %.2f mm (dpm %d)", mmW, arcMm,
          back.dotsPerMeterX());
  }

  // ---- AC-6 문서 범위·캐시 -------------------------------------------------------
  std::printf("\n--- [AC-6] ArtifactDoc region + cache ---\n");
  {
    ArtifactDoc doc;
    doc.loadSample(true);
    RubbingRegion r2;
    r2.azCenterDeg = 200.0;
    r2.azSpanDeg = 50.0;
    r2.zLo = 100.0;
    r2.zHi = 140.0;
    doc.setRubbingRegion(r2);
    const RubbingRegion got = doc.rubbingRegion();
    CHECK(std::fabs(got.azSpanDeg - 50.0) < 1e-6 && std::fabs(got.zLo - 100.0) < 1e-6 &&
              std::fabs(got.zHi - 140.0) < 1e-6 &&
              std::fabs(got.azCenterDeg - 200.0) < 1e-6,
          "region stored exactly");
    CHECK(std::fabs(doc.rubbingZ() - 120.0) < 1e-6, "rubbingZ = middle (%.2f)",
          doc.rubbingZ());
    QElapsedTimer t;
    t.start();
    const UnrolledRubbing &a = doc.unrolledRubbing(4.0);
    const double first = t.nsecsElapsed() / 1e6;
    t.restart();
    const UnrolledRubbing &b = doc.unrolledRubbing(4.0);
    const double second = t.nsecsElapsed() / 1e6;
    std::printf("  first %.2f ms, cached %.4f ms\n", first, second);
    CHECK(a.ok() && &a == &b && second < 1.0, "second call uses cache");
    const double partW = a.widthMm; // a 는 캐시 참조 — 다시 계산되면 같은 자리가 바뀐다
    doc.setRubbingFullTurn();
    CHECK(std::fabs(doc.rubbingRegion().azSpanDeg - 360.0) < 1e-6 &&
              std::fabs(doc.rubbingRegion().zLo - 100.0) < 1e-6,
          "full turn keeps height range");
    const UnrolledRubbing &c = doc.unrolledRubbing(4.0);
    CHECK(c.ok() && c.widthMm > 3.0 * partW, "full turn recomputed (%.1f mm vs %.1f mm)",
          c.widthMm, partW);
  }

  // ---- 선택: 실제 유물 시간 기록 -------------------------------------------------
  if (argc > 2 && QFileInfo(QString::fromLocal8Bit(argv[2])).isFile()) {
    std::printf("\n--- [real artifact] %s ---\n", argv[2]);
    ArtifactDoc doc;
    QString err;
    if (doc.loadFile(QString::fromLocal8Bit(argv[2]), &err)) {
      doc.autoAlign();
      doc.confirmPose();
      QElapsedTimer t;
      t.start();
      const UnrolledRubbing &a = doc.unrolledRubbing(4.0);
      const double first = t.nsecsElapsed() / 1e6;
      t.restart();
      doc.unrolledRubbing(4.0);
      const double second = t.nsecsElapsed() / 1e6;
      std::printf("  region c=%.1f span=%.1f z=[%.1f,%.1f] -> %.1fx%.1f mm, cover %.2f\n",
                  doc.rubbingRegion().azCenterDeg, doc.rubbingRegion().azSpanDeg,
                  doc.rubbingRegion().zLo, doc.rubbingRegion().zHi, a.widthMm,
                  a.heightMm, a.coverage);
      std::printf("  first %.1f ms, cached %.4f ms\n", first, second);
      CHECK(a.ok() && second < 1.0, "real artifact rubbing + cache");
      a.image.save(outDir + QStringLiteral("/rubbing_real.png"));
      doc.setRubbingFullTurn();
      t.restart();
      const UnrolledRubbing &f = doc.unrolledRubbing(4.0);
      std::printf("  full turn %.1f ms -> %.1fx%.1f mm\n", t.nsecsElapsed() / 1e6,
                  f.widthMm, f.heightMm);
      f.image.save(outDir + QStringLiteral("/rubbing_real_fullturn.png"));
    } else {
      CHECK(false, "load real artifact: %s", qPrintable(err));
    }
  }

  std::printf(fails ? "\n=== [JOB-014] FAILED: %d ===\n" : "\n=== [JOB-014] ALL PASSED ===\n",
              fails);
  return fails ? 1 : 0;
}
