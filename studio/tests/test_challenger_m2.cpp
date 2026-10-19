#include "ArtifactDoc.h"
#include "MeshRaster.h"
#include "Plate.h"
#include "Theme.h"

#include <QCoreApplication>
#include <QFileInfo>
#include <QGuiApplication>

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <numbers>
#include <random>
#include <thread>
#include <vector>

using namespace studio;
namespace KA = DongArch::KArch;

static int fails = 0;
template <typename... Args>
inline void reportCheck(bool cond, const char *fmt, Args &&...args) {
  if (!cond) {
    std::printf("  [FAIL] ");
    std::printf(fmt, std::forward<Args>(args)...);
    std::printf("\n");
    ++fails;
  } else {
    std::printf("  [PASS] ");
    std::printf(fmt, std::forward<Args>(args)...);
    std::printf("\n");
  }
}
#define TEST_CHECK(cond, msg, ...) reportCheck(bool(cond), msg, ##__VA_ARGS__)

static double maxR(const ArtifactDoc &doc) {
  double r = 10.0;
  for (const auto &q : doc.displayOutline())
    r = std::max(r, std::fabs(q.x));
  for (const auto &poly : doc.displaySection())
    for (const auto &q : poly)
      r = std::max(r, std::fabs(q.x));
  return r;
}

static double height(const ArtifactDoc &doc) {
  return std::max(1.0, doc.mesh().bboxMax.z - doc.mesh().bboxMin.z);
}

using clk = std::chrono::steady_clock;
static double msSince(clk::time_point t) {
  return std::chrono::duration<double, std::milli>(clk::now() - t).count();
}

// ============================================================================
// Oracle: 17a830a 이전의 보수적(Conservative) 기준 래스터라이저
// (서브픽셀 탈락 및 행렬 최적화 이전의 엄격한 바운딩 박스 & 단일 스레드 순회)
// ============================================================================
namespace oracle {
struct V3 {
  double x, y, z;
};
inline V3 sub(V3 a, V3 b) { return {a.x - b.x, a.y - b.y, a.z - b.z}; }
inline V3 cross(V3 a, V3 b) {
  return {a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x};
}

inline V3 toCam(const KA::Point3 &p, const RasterView &v) {
  const double yw = v.yawDeg * std::numbers::pi / 180.0,
               pt = v.pitchDeg * std::numbers::pi / 180.0;
  const double cy = std::cos(yw), sy = std::sin(yw);
  const double px = p.x - v.centerX, py = p.y - v.centerY;
  const double x1 = cy * px - sy * py, y1 = sy * px + cy * py,
               z1 = p.z - v.centerZ;
  const double cp = std::cos(pt), sp = std::sin(pt);
  const double up = cp * z1 - sp * y1;
  const double depth = -(sp * z1 + cp * y1);
  return {x1, up, depth};
}

QImage renderMeshOracle(const KA::MeshData &mesh, QSize size,
                        const RasterView &v, ShadeMode mode, QColor base,
                        std::vector<float> *depthOut,
                        const std::vector<QRgb> *vertexColors) {
  const int W = std::max(1, size.width()), H = std::max(1, size.height());
  QImage img(W, H, QImage::Format_ARGB32_Premultiplied);
  img.fill(Qt::transparent);
  std::vector<float> zb(size_t(W) * H, -std::numeric_limits<float>::infinity());
  std::vector<float> lum(size_t(W) * H, 0.f);
  const bool useVCol = (mode == ShadeMode::Real) && vertexColors &&
                       vertexColors->size() == mesh.vertices.size();
  std::vector<QRgb> cbBuf;
  if (useVCol)
    cbBuf.assign(size_t(W) * H, base.rgb());

  std::vector<V3> sv(mesh.vertices.size()), cv(mesh.vertices.size());
  for (size_t i = 0; i < mesh.vertices.size(); ++i) {
    const V3 c = toCam(mesh.vertices[i], v);
    cv[i] = c;
    sv[i] = {v.centerPx.x() + c.x * v.pxPerMm, v.centerPx.y() - c.y * v.pxPerMm,
             c.z};
  }
  const V3 L = [] {
    V3 l{-0.45, 0.55, 0.70};
    const double n = std::sqrt(l.x * l.x + l.y * l.y + l.z * l.z);
    return V3{l.x / n, l.y / n, l.z / n};
  }();

  for (const auto &f : mesh.faces) {
    if (size_t(f[0]) >= sv.size() || size_t(f[1]) >= sv.size() ||
        size_t(f[2]) >= sv.size())
      continue;
    const V3 a = sv[size_t(f[0])], b = sv[size_t(f[1])], c = sv[size_t(f[2])];
    const V3 ca = cv[size_t(f[0])], cb = cv[size_t(f[1])],
             cc = cv[size_t(f[2])];
    V3 n = cross(sub(cb, ca), sub(cc, ca));
    const double nl = std::sqrt(n.x * n.x + n.y * n.y + n.z * n.z);
    if (nl < 1e-15)
      continue;
    n = {n.x / nl, n.y / nl, n.z / nl};
    if (n.z < 0)
      n = {-n.x, -n.y, -n.z};
    const float shade =
        float(0.28 + 0.72 * std::max(0.0, n.x * L.x + n.y * L.y + n.z * L.z));

    // 오라클: 0.5 감산 서브픽셀 기각 없이 floor/ceil 전면 순회
    const int x0 = std::max(0, int(std::floor(std::min({a.x, b.x, c.x}))));
    const int x1 = std::min(W - 1, int(std::ceil(std::max({a.x, b.x, c.x}))));
    const int y0 = std::max(0, int(std::floor(std::min({a.y, b.y, c.y}))));
    const int y1 = std::min(H - 1, int(std::ceil(std::max({a.y, b.y, c.y}))));
    if (x0 > x1 || y0 > y1)
      continue;
    const double area = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    if (std::fabs(area) < 1e-12)
      continue;
    QRgb c0 = 0, c1 = 0, c2 = 0;
    if (useVCol) {
      c0 = (*vertexColors)[size_t(f[0])];
      c1 = (*vertexColors)[size_t(f[1])];
      c2 = (*vertexColors)[size_t(f[2])];
    }
    const double invS = 1.0 / std::fabs(area);
    for (int y = y0; y <= y1; ++y) {
      const double py = y + 0.5;
      for (int x = x0; x <= x1; ++x) {
        const double px = x + 0.5;
        double w0 = (b.x - px) * (c.y - py) - (b.y - py) * (c.x - px);
        double w1 = (c.x - px) * (a.y - py) - (c.y - py) * (a.x - px);
        double w2 = (a.x - px) * (b.y - py) - (a.y - py) * (b.x - px);
        if (area < 0)
          w0 = -w0, w1 = -w1, w2 = -w2;
        if (w0 < 0 || w1 < 0 || w2 < 0)
          continue;
        const float z = float((w0 * a.z + w1 * b.z + w2 * c.z) * invS);
        const size_t k = size_t(y) * W + x;
        if (z > zb[k]) {
          zb[k] = z;
          lum[k] = shade;
          if (useVCol) {
            const int rr = int(
                (w0 * qRed(c0) + w1 * qRed(c1) + w2 * qRed(c2)) * invS + 0.5);
            const int gg = int(
                (w0 * qGreen(c0) + w1 * qGreen(c1) + w2 * qGreen(c2)) * invS +
                0.5);
            const int bb =
                int((w0 * qBlue(c0) + w1 * qBlue(c1) + w2 * qBlue(c2)) * invS +
                    0.5);
            cbBuf[k] = qRgb(std::clamp(rr, 0, 255), std::clamp(gg, 0, 255),
                            std::clamp(bb, 0, 255));
          }
        }
      }
    }
  }

  // DTak 오라클
  std::vector<float> rel;
  if (mode == ShadeMode::DTak) {
    const int r = std::max(2, int(std::lround(2.2 * v.pxPerMm)));
    std::vector<double> sat(size_t(W + 1) * (H + 1), 0.0),
        cnt(size_t(W + 1) * (H + 1), 0.0);
    for (int y = 0; y < H; ++y)
      for (int x = 0; x < W; ++x) {
        const float z = zb[size_t(y) * W + x];
        const bool ok = std::isfinite(z);
        const size_t k = size_t(y + 1) * (W + 1) + (x + 1);
        sat[k] = (ok ? z : 0.0) + sat[k - 1] + sat[k - (W + 1)] -
                 sat[k - (W + 1) - 1];
        cnt[k] = (ok ? 1.0 : 0.0) + cnt[k - 1] + cnt[k - (W + 1)] -
                 cnt[k - (W + 1) - 1];
      }
    rel.assign(size_t(W) * H, 0.f);
    for (int y = 0; y < H; ++y)
      for (int x = 0; x < W; ++x) {
        const size_t k = size_t(y) * W + x;
        if (!std::isfinite(zb[k]))
          continue;
        const int ax = std::max(0, x - r), bx = std::min(W, x + r + 1),
                  ay = std::max(0, y - r), by = std::min(H, y + r + 1);
        auto S = [&](const std::vector<double> &t) {
          return t[size_t(by) * (W + 1) + bx] - t[size_t(ay) * (W + 1) + bx] -
                 t[size_t(by) * (W + 1) + ax] + t[size_t(ay) * (W + 1) + ax];
        };
        const double c = S(cnt);
        rel[k] = c > 0 ? float(zb[k] - S(sat) / c) : 0.f;
      }
  }

  float zmin = 0.f, zr = 1.f;
  if (mode == ShadeMode::Depth) {
    float minVal = std::numeric_limits<float>::infinity();
    float maxVal = -minVal;
    for (float z : zb)
      if (std::isfinite(z)) {
        minVal = std::min(minVal, z);
        maxVal = std::max(maxVal, z);
      }
    zmin = minVal;
    zr = std::max(1e-3f, maxVal - minVal);
  }

  for (int y = 0; y < H; ++y) {
    auto *line = reinterpret_cast<QRgb *>(img.scanLine(y));
    for (int x = 0; x < W; ++x) {
      const size_t k = size_t(y) * W + x;
      if (!std::isfinite(zb[k]))
        continue;
      QColor c;
      if (mode == ShadeMode::Real) {
        const float s = lum[k];
        const QColor vc = useVCol ? QColor::fromRgb(cbBuf[k]) : base;
        c = QColor::fromRgbF(std::min(1.f, float(vc.redF()) * s * 1.12f),
                             std::min(1.f, float(vc.greenF()) * s * 1.12f),
                             std::min(1.f, float(vc.blueF()) * s * 1.12f));
      } else if (mode == ShadeMode::Depth) {
        const float t = (zb[k] - zmin) / zr;
        const float g = 0.55f + 0.40f * t * (0.6f + 0.4f * lum[k]);
        c = QColor::fromRgbF(g * 0.98f, g * 0.97f, g * 0.93f);
      } else {
        const float t = std::clamp(0.5f - rel[k] * 0.95f, 0.f, 1.f);
        const float g = (0.38f + 0.58f * t) * (0.82f + 0.18f * lum[k]);
        c = QColor::fromRgbF(g * 0.97f, g * 0.96f, g * 0.93f);
      }
      line[x] = c.rgba();
    }
  }

  if (depthOut) {
    depthOut->assign(size_t(W) * H, std::numeric_limits<float>::quiet_NaN());
    for (size_t k = 0; k < size_t(W) * H; ++k)
      if (std::isfinite(zb[k]))
        (*depthOut)[k] = zb[k];
  }
  return img;
}
} // namespace oracle

// ============================================================================
// Brute Force Box Blur (O(R^2) 무결성 오라클)
// ============================================================================
static std::vector<float> bruteForceBoxBlur(const std::vector<float> &zb, int W,
                                            int H, int r) {
  std::vector<float> rel(size_t(W) * H, 0.f);
  for (int y = 0; y < H; ++y) {
    for (int x = 0; x < W; ++x) {
      const size_t k = size_t(y) * W + x;
      if (!std::isfinite(zb[k]))
        continue;
      const int ax = std::max(0, x - r), bx = std::min(W - 1, x + r);
      const int ay = std::max(0, y - r), by = std::min(H - 1, y + r);
      double sumZ = 0.0;
      double count = 0.0;
      for (int cy = ay; cy <= by; ++cy) {
        for (int cx = ax; cx <= bx; ++cx) {
          const float val = zb[size_t(cy) * W + cx];
          if (std::isfinite(val)) {
            sumZ += double(val);
            count += 1.0;
          }
        }
      }
      rel[k] = (count > 0.0) ? float(zb[k] - sumZ / count) : 0.f;
    }
  }
  return rel;
}

int main(int argc, char **argv) {
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  qputenv("QT_QPA_PLATFORM", "offscreen");
  QGuiApplication app(argc, argv);

  std::printf(
      "================================================================="
      "=====\n");
  std::printf("[CHALLENGER 2] Milestone 2 Empirical Adversarial Test Suite\n");
  std::printf(
      "================================================================="
      "=====\n");

  QString objPath;
  for (int i = 1; i < argc; ++i) {
    if (argv[i] && argv[i][0] != '\0') {
      const QString cand = QString::fromLocal8Bit(argv[i]);
      if (QFileInfo(cand).isFile()) {
        objPath = cand;
        break;
      }
    }
  }
  if (objPath.isEmpty()) {
    const QString envObj = qEnvironmentVariable("DONGARCH_BENCH_OBJ");
    if (!envObj.isEmpty() && QFileInfo(envObj).isFile()) {
      objPath = envObj;
    }
  }

  // -------------------------------------------------------------------------
  // TEST 1: DTak Box Blur 수학적 정확성 및 포화(Saturation) 거동 검증
  // -------------------------------------------------------------------------
  std::printf("\n--- [TEST 1] DTak Box Blur & Saturation Mathematical "
              "Verification ---\n");
  {
    const int W = 64, H = 64, r = 5;
    std::vector<float> zb(W * H, std::numeric_limits<float>::quiet_NaN());

    // 인공 지형: 계단, 임펄스 스파이크, 음각/양각 릴리프, 실루엣 결손(NaN)
    for (int y = 10; y < 54; ++y) {
      for (int x = 10; x < 54; ++x) {
        zb[y * W + x] = 100.0f + 0.1f * x - 0.05f * y;
      }
    }
    // 양각 스파이크 (+5.0 mm -> 명백한 먹 포화)
    zb[25 * W + 25] = 105.0f;
    // 음각 스파이크 (-5.0 mm -> 명백한 종이 포화)
    zb[35 * W + 35] = 95.0f;
    // 미세 양각 (+0.2 mm -> 중간조)
    zb[20 * W + 20] = 100.2f;

    // Brute force vs SAT 기반 검증
    const std::vector<float> refRel = bruteForceBoxBlur(zb, W, H, r);

    // MeshRaster.cpp 내 SAT 방식 재현
    const size_t satSize = size_t(W + 1) * (H + 1);
    std::vector<double> sat(satSize, 0.0), cnt(satSize, 0.0);
    for (int y = 0; y < H; ++y) {
      sat[size_t(y + 1) * (W + 1)] = 0.0;
      cnt[size_t(y + 1) * (W + 1)] = 0.0;
      double rowSat = 0.0, rowCnt = 0.0;
      const size_t prevRow = size_t(y) * (W + 1);
      const size_t currRow = size_t(y + 1) * (W + 1);
      for (int x = 0; x < W; ++x) {
        const float z = zb[size_t(y) * W + x];
        const bool ok = std::isfinite(z);
        rowSat += (ok ? double(z) : 0.0);
        rowCnt += (ok ? 1.0 : 0.0);
        sat[currRow + size_t(x + 1)] = sat[prevRow + size_t(x + 1)] + rowSat;
        cnt[currRow + size_t(x + 1)] = cnt[prevRow + size_t(x + 1)] + rowCnt;
      }
    }

    std::vector<float> satRel(W * H, 0.f);
    double maxRelDiff = 0.0;
    for (int y = 0; y < H; ++y) {
      for (int x = 0; x < W; ++x) {
        const size_t k = size_t(y) * W + x;
        if (!std::isfinite(zb[k]))
          continue;
        const int ax = std::max(0, x - r), bx = std::min(W, x + r + 1);
        const int ay = std::max(0, y - r), by = std::min(H, y + r + 1);
        const double count =
            cnt[size_t(by) * (W + 1) + bx] - cnt[size_t(ay) * (W + 1) + bx] -
            cnt[size_t(by) * (W + 1) + ax] + cnt[size_t(ay) * (W + 1) + ax];
        const double sumZ =
            sat[size_t(by) * (W + 1) + bx] - sat[size_t(ay) * (W + 1) + bx] -
            sat[size_t(by) * (W + 1) + ax] + sat[size_t(ay) * (W + 1) + ax];
        satRel[k] = count > 0.0 ? float(zb[k] - sumZ / count) : 0.f;
        const double diff = std::fabs(double(satRel[k]) - double(refRel[k]));
        maxRelDiff = std::max(maxRelDiff, diff);
      }
    }

    std::printf(
        "  [INFO] Max difference between SAT BoxBlur and Brute Force: %e\n",
        maxRelDiff);
    TEST_CHECK(
        maxRelDiff < 1e-4,
        "SAT BoxBlur perfectly matches O(R^2) Brute Force (diff < 1e-4)");

    // 포화(Saturation) 거동 확인
    // t = clamp(0.5 - rel * 0.95, 0, 1)
    // 1) 양각 스파이크 (+5 mm): rel = +5.0 -> t = clamp(0.5 - 4.75, 0, 1) = 0.0
    // (완전 먹 포화)
    const float t_spike_pos =
        std::clamp(0.5f - satRel[25 * W + 25] * 0.95f, 0.f, 1.f);
    TEST_CHECK(t_spike_pos == 0.0f,
               "Positive relief spike saturates to 0.0 (maximum ink/darkness)");

    // 2) 음각 스파이크 (-5 mm): rel = -5.0 -> t = clamp(0.5 - (-4.75), 0, 1)
    // = 1.0 (완전 종이 포화)
    const float t_spike_neg =
        std::clamp(0.5f - satRel[35 * W + 35] * 0.95f, 0.f, 1.f);
    TEST_CHECK(
        t_spike_neg == 1.0f,
        "Negative relief spike saturates to 1.0 (maximum paper/brightness)");

    // 3) 평탄부: rel = 0 -> t = 0.5 (중간 톤)
    const float t_flat =
        std::clamp(0.5f - satRel[15 * W + 15] * 0.95f, 0.f, 1.f);
    TEST_CHECK(std::fabs(t_flat - 0.5f) < 0.05f,
               "Flat region maintains neutral midtone (~0.5)");

    // 4) 포화 임계값 수치 증명: rel >= 0.5/0.95 = ~0.5263 mm -> t=0, rel <=
    // -0.5263 mm -> t=1
    const float t_thresh_pos = std::clamp(0.5f - (0.5264f) * 0.95f, 0.f, 1.f);
    const float t_thresh_neg = std::clamp(0.5f - (-0.5264f) * 0.95f, 0.f, 1.f);
    TEST_CHECK(t_thresh_pos == 0.0f && t_thresh_neg == 1.0f,
               "Saturation thresholds verified at +-0.5263 mm relief");
  }

  // -------------------------------------------------------------------------
  // TEST 2: 서브픽셀 기각(Subpixel Rejection) 아티팩트 및 핀홀/실루엣 결손
  // 스트레스 검증
  // -------------------------------------------------------------------------
  // -------------------------------------------------------------------------
  // TEST 2.1: 결함 실증 (Defect Confirmation): numFaces < 5000 시 면 누락 버그
  // MeshRaster.cpp:305-306에서 numFaces < 5000일 때 rasterizeFaces(0)만
  // 호출하나, rasterizeFaces(0) 내부에서는 f1 = numFaces / nThreads 로
  // 1/nThreads만 래스터화하여 저다면체(numFaces < 5000) 메시의 면 93.75%가
  // 누락되는 치명적 버그 확인
  // -------------------------------------------------------------------------
  std::printf("\n--- [TEST 2.1] Defect Confirmation: MeshRaster.cpp numFaces < "
              "5000 Face Dropping Bug ---\n");
  {
    KA::MeshData lowPolyMesh;
    // 500개의 전면 가시 사각형 (1000개 삼각형, XZ 평면 배치 -> 카메라 시선
    // 방향)
    for (int i = 0; i < 500; ++i) {
      const double x = -20.0 + (i % 25) * 1.6;
      const double z = 30.0 + (i / 25) * 1.6;
      const size_t b = lowPolyMesh.vertices.size();
      lowPolyMesh.vertices.push_back({x, 0.0, z});
      lowPolyMesh.vertices.push_back({x + 1.2, 0.0, z});
      lowPolyMesh.vertices.push_back({x + 1.2, 0.0, z + 1.2});
      lowPolyMesh.vertices.push_back({x, 0.0, z + 1.2});
      lowPolyMesh.faces.push_back({int(b), int(b + 1), int(b + 2)});
      lowPolyMesh.faces.push_back({int(b), int(b + 2), int(b + 3)});
    }
    const size_t totalF = lowPolyMesh.faces.size();
    RasterView v;
    v.yawDeg = 0.0;
    v.pitchDeg = 0.0;
    v.pxPerMm = 4.0;
    const QSize testSz(200, 200);
    v.centerPx = QPointF(100.0, 100.0);
    v.centerX = 0.0;
    v.centerY = 0.0;
    v.centerZ = 45.0;

    std::vector<float> dWorker, dOracle;
    renderMesh(lowPolyMesh, testSz, v, ShadeMode::Real, theme::ModelClay,
               &dWorker, nullptr);
    oracle::renderMeshOracle(lowPolyMesh, testSz, v, ShadeMode::Real,
                             theme::ModelClay, &dOracle, nullptr);

    int litWorker = 0, litOracle = 0;
    for (size_t k = 0; k < size_t(testSz.width()) * testSz.height(); ++k) {
      if (std::isfinite(dWorker[k]))
        ++litWorker;
      if (std::isfinite(dOracle[k]))
        ++litOracle;
    }
    std::printf("  [DEFECT EVIDENCE] 1000-face mesh: Oracle lit %d pixels, "
                "Worker lit %d pixels (Ratio: %.1f%%)\n",
                litOracle, litWorker, (100.0 * litWorker) / litOracle);
    const bool defectPresent = (litWorker < litOracle / 2);
    if (defectPresent) {
      std::printf(
          "  [CONFIRMED BUG] MeshRaster.cpp:305-306 drops %zu / %zu faces when "
          "numFaces < 5000!\n",
          totalF - (totalF / std::max(1u, std::thread::hardware_concurrency())),
          totalF);
    }
    TEST_CHECK(
        litWorker == litOracle,
        "All 1000 faces of low-poly mesh must be rendered (BUG FOUND if fail)");
  }

  // -------------------------------------------------------------------------
  // TEST 2.2: 순수 서브픽셀 기각(Subpixel Rejection) 무결성 검증 (numFaces >=
  // 5000) 멀티스레드 분할 버그(numFaces < 5000)의 간섭을 배제하고, 6000개의
  // 병리적 서브픽셀/슬리버/경계 삼각형에 대해 Subpixel Reject의 핀홀/결손 여부
  // 단독 검증
  // -------------------------------------------------------------------------
  std::printf("\n--- [TEST 2.2] Pure Subpixel Rejection Fidelity (6000 "
              "Pathological Faces >= 5000) ---\n");
  {
    KA::MeshData stressMesh;
    std::mt19937 rng(42);
    std::uniform_real_distribution<double> distCoord(-20.0, 20.0);
    std::uniform_real_distribution<double> distTiny(-0.3, 0.3);

    // 5900개의 무작위 서브픽셀 삼각형 군집
    for (int i = 0; i < 5900; ++i) {
      const double cx = distCoord(rng);
      const double cy = distCoord(rng);
      const double cz = 50.0 + distTiny(rng);
      const size_t baseIdx = stressMesh.vertices.size();
      stressMesh.vertices.push_back({cx, cy, cz});
      stressMesh.vertices.push_back(
          {cx + distTiny(rng), cy + distTiny(rng), cz});
      stressMesh.vertices.push_back(
          {cx + distTiny(rng), cy + distTiny(rng), cz});
      stressMesh.faces.push_back(
          {int(baseIdx), int(baseIdx + 1), int(baseIdx + 2)});
    }

    // 100개의 픽셀 경계(0.5) 슬리버 (합계 6000개 >= 5000)
    for (int i = 0; i < 100; ++i) {
      const double cx = -10.0 + i * 0.2;
      const size_t baseIdx = stressMesh.vertices.size();
      stressMesh.vertices.push_back({cx, -5.0, 60.0});
      stressMesh.vertices.push_back({cx + 0.001, 5.0, 60.0});
      stressMesh.vertices.push_back({cx + 0.002, -5.0, 60.0});
      stressMesh.faces.push_back(
          {int(baseIdx), int(baseIdx + 1), int(baseIdx + 2)});
    }

    RasterView v;
    v.yawDeg = 15.0;
    v.pitchDeg = 10.0;
    v.pxPerMm = 4.0;
    const QSize testSz(200, 200);
    v.centerPx = QPointF(100.0, 100.0);
    v.centerX = 0.0;
    v.centerY = 0.0;
    v.centerZ = 50.0;

    std::vector<float> dWorker, dOracle;
    const QImage imgWorker = renderMesh(stressMesh, testSz, v, ShadeMode::Real,
                                        theme::ModelClay, &dWorker, nullptr);
    const QImage imgOracle =
        oracle::renderMeshOracle(stressMesh, testSz, v, ShadeMode::Real,
                                 theme::ModelClay, &dOracle, nullptr);

    int pinholes = 0, falsePositives = 0, colorMismatches = 0;
    for (int y = 0; y < testSz.height(); ++y) {
      for (int x = 0; x < testSz.width(); ++x) {
        const size_t k = size_t(y) * testSz.width() + x;
        const bool workerLit = std::isfinite(dWorker[k]);
        const bool oracleLit = std::isfinite(dOracle[k]);

        if (oracleLit && !workerLit) {
          ++pinholes;
        } else if (!oracleLit && workerLit) {
          ++falsePositives;
        } else if (workerLit && oracleLit) {
          const QRgb cw = imgWorker.pixel(x, y);
          const QRgb co = imgOracle.pixel(x, y);
          if (std::abs(qRed(cw) - qRed(co)) > 1 ||
              std::abs(qGreen(cw) - qGreen(co)) > 1 ||
              std::abs(qBlue(cw) - qBlue(co)) > 1) {
            ++colorMismatches;
          }
        }
      }
    }

    std::printf("  [STATS] 6000-face Pathological Mesh: Pinholes=%d, "
                "FalsePositives=%d, ColorMismatches=%d\n",
                pinholes, falsePositives, colorMismatches);
    TEST_CHECK(pinholes == 0, "Subpixel rejection creates ZERO pinholes when "
                              "all faces are processed");
    TEST_CHECK(falsePositives == 0,
               "Subpixel rejection creates ZERO false positives");
    TEST_CHECK(colorMismatches == 0,
               "Subpixel rejection maintains bit-exact colors");
  }

  // -------------------------------------------------------------------------
  // TEST 3 & 4: 실제 유물 6183.obj 기준 실증 검증 (성능 & 오라클 대비 전수 화소
  // 무결성)
  // -------------------------------------------------------------------------
  if (objPath.isEmpty() || !QFile::exists(objPath)) {
    std::printf(
        "\n[SKIP] Real artifact 6183.obj not found. Skipping Tests 3-5.\n");
    std::printf("Final verdict on available tests: %s\n",
                fails == 0 ? "PASS" : "FAIL");
    return fails == 0 ? 0 : 1;
  }

  std::printf("\n--- [TEST 3] Real Scan 6183.obj Loading & Benchmarking ---\n");
  ArtifactDoc doc;
  QString loadErr;
  const auto t0 = clk::now();
  const bool ok = doc.loadFile(objPath, &loadErr);
  const double loadDt = msSince(t0);
  TEST_CHECK(ok, "Loaded 6183.obj in %.1f ms", loadDt);
  if (!ok) {
    std::printf("Cannot load 6183.obj. Exiting.\n");
    return 1;
  }
  doc.confirmPose();

  // Benchmark Plate::renderShade under multiple modes and 10 iterations
  std::printf("\n--- [TEST 3.1] 10-Iteration Timing Benchmark on "
              "Plate::renderShade (Limit: <= 80 ms) ---\n");
  std::vector<double> dtakTimes;
  dtakTimes.reserve(10);
  ShadeImage sampleShade;
  for (int iter = 0; iter < 10; ++iter) {
    const auto tBench = clk::now();
    sampleShade = Plate::renderShade(doc, ShadeMode::DTak, 2.0);
    const double dt = msSince(tBench);
    dtakTimes.push_back(dt);
    std::printf(
        "  [Run %2d] Plate::renderShade(DTak, 2.0): %.2f ms (Size: %dx%d)\n",
        iter + 1, dt, sampleShade.image.width(), sampleShade.image.height());
  }
  std::sort(dtakTimes.begin(), dtakTimes.end());
  const double dtakMin = dtakTimes.front();
  const double dtakMed = dtakTimes[dtakTimes.size() / 2];
  const double dtakMax = dtakTimes.back();
  std::printf(
      "  [STATS] DTak (2.0 px/mm): Min=%.2f ms, Median=%.2f ms, Max=%.2f ms\n",
      dtakMin, dtakMed, dtakMax);
  TEST_CHECK(dtakMed <= 80.0, "DTak median time %.2f ms <= 80.0 ms limit",
             dtakMed);
  TEST_CHECK(dtakMax <= 80.0, "DTak max time %.2f ms <= 80.0 ms limit",
             dtakMax);

  // Other modes timing
  std::vector<double> realTimes, depthTimes;
  for (int iter = 0; iter < 5; ++iter) {
    const auto tR = clk::now();
    Plate::renderShade(doc, ShadeMode::Real, 2.0);
    realTimes.push_back(msSince(tR));

    const auto tD = clk::now();
    Plate::renderShade(doc, ShadeMode::Depth, 2.0);
    depthTimes.push_back(msSince(tD));
  }
  std::sort(realTimes.begin(), realTimes.end());
  std::sort(depthTimes.begin(), depthTimes.end());
  std::printf(
      "  [STATS] Real (2.0 px/mm): Median=%.2f ms (Min=%.2f, Max=%.2f)\n",
      realTimes[2], realTimes.front(), realTimes.back());
  std::printf(
      "  [STATS] Depth (2.0 px/mm): Median=%.2f ms (Min=%.2f, Max=%.2f)\n",
      depthTimes[2], depthTimes.front(), depthTimes.back());
  TEST_CHECK(realTimes[2] <= 80.0, "Real median time %.2f ms <= 80.0 ms",
             realTimes[2]);
  TEST_CHECK(depthTimes[2] <= 80.0, "Depth median time %.2f ms <= 80.0 ms",
             depthTimes[2]);

  // -------------------------------------------------------------------------
  // TEST 4: 실제 6183.obj (4,307,352 faces) 기준 오라클 대비 전수 화소 무결성
  // 검증
  // -------------------------------------------------------------------------
  std::printf("\n--- [TEST 4] Full Pixel-by-Pixel Oracle Comparison on "
              "6183.obj (4.31M faces) ---\n");
  {
    const double r = maxR(doc) + 2.0, h = height(doc) + 4.0;
    const double pxPerMm = 2.0;
    RasterView v;
    v.yawDeg = doc.sectionSide() == SectionSide::Right ? 0.0 : 180.0;
    v.pitchDeg = 0.0;
    v.pxPerMm = pxPerMm;
    const QSize sz(int(std::ceil(2 * r * pxPerMm)),
                   int(std::ceil(h * pxPerMm)));
    v.centerPx = QPointF(sz.width() / 2.0, sz.height() / 2.0);
    v.centerZ = (h - 4.0) / 2.0 + doc.mesh().bboxMin.z;

    std::printf("  Rendering with Worker (optimized multi-threaded + subpixel "
                "reject)...\n");
    std::vector<float> workerDepth;
    const auto tW0 = clk::now();
    const QImage workerImg =
        renderMesh(doc.mesh(), sz, v, ShadeMode::Real, theme::ModelClay,
                   &workerDepth, nullptr);
    const double workerMs = msSince(tW0);
    std::printf("  Worker finished in %.2f ms\n", workerMs);

    std::printf("  Rendering with Oracle (unoptimized single-threaded, no "
                "subpixel reject)...\n");
    std::vector<float> oracleDepth;
    const auto tO0 = clk::now();
    const QImage oracleImg =
        oracle::renderMeshOracle(doc.mesh(), sz, v, ShadeMode::Real,
                                 theme::ModelClay, &oracleDepth, nullptr);
    const double oracleMs = msSince(tO0);
    std::printf("  Oracle finished in %.2f ms (Worker Speedup: %.1fx)\n",
                oracleMs, oracleMs / workerMs);

    const int totalPixels = sz.width() * sz.height();
    int workerLitCount = 0, oracleLitCount = 0;
    int pinholeGaps = 0;      // Oracle에 켜졌으나 Worker에 꺼진 화소
    int falseSilhouettes = 0; // Worker에 켜졌으나 Oracle에 꺼진 화소
    int colorDiffCount = 0;
    double maxZDiff = 0.0;

    for (int y = 0; y < sz.height(); ++y) {
      for (int x = 0; x < sz.width(); ++x) {
        const size_t k = size_t(y) * sz.width() + x;
        const bool wLit = std::isfinite(workerDepth[k]);
        const bool oLit = std::isfinite(oracleDepth[k]);
        if (wLit)
          ++workerLitCount;
        if (oLit)
          ++oracleLitCount;

        if (oLit && !wLit) {
          ++pinholeGaps;
        } else if (!oLit && wLit) {
          ++falseSilhouettes;
        } else if (wLit && oLit) {
          const double zDiff =
              std::fabs(double(workerDepth[k]) - double(oracleDepth[k]));
          maxZDiff = std::max(maxZDiff, zDiff);

          const QRgb cw = workerImg.pixel(x, y);
          const QRgb co = oracleImg.pixel(x, y);
          if (std::abs(qRed(cw) - qRed(co)) > 1 ||
              std::abs(qGreen(cw) - qGreen(co)) > 1 ||
              std::abs(qBlue(cw) - qBlue(co)) > 1) {
            ++colorDiffCount;
          }
        }
      }
    }

    std::printf("  [STATS] Total Pixels: %d\n", totalPixels);
    std::printf("          Worker lit: %d, Oracle lit: %d\n", workerLitCount,
                oracleLitCount);
    std::printf("          Pinhole Gaps (missed silhouette/interior): %d\n",
                pinholeGaps);
    std::printf("          False Silhouettes: %d\n", falseSilhouettes);
    std::printf("          Max Depth Difference on lit pixels: %e mm\n",
                maxZDiff);
    std::printf(
        "          RGB Differences > 1 LSB: %d (out of %d lit pixels)\n",
        colorDiffCount, workerLitCount);

    TEST_CHECK(
        pinholeGaps == 0,
        "6183.obj has ZERO pinhole gaps or missed silhouette pixels (0 / %d)",
        totalPixels);
    TEST_CHECK(falseSilhouettes == 0,
               "6183.obj has ZERO false silhouette pixels");
    TEST_CHECK(
        maxZDiff < 1e-4,
        "Depth buffer matches Oracle within float tolerance (max diff: %e mm)",
        maxZDiff);
    TEST_CHECK(colorDiffCount == 0,
               "Rendered colors match Oracle 100%% within 1 LSB rounding");
  }

  // -------------------------------------------------------------------------
  // TEST 5: 멀티스레드 결정론(Determinism) 및 재사용 버퍼 메모리 누수 스트레스
  // 검증
  // -------------------------------------------------------------------------
  std::printf(
      "\n--- [TEST 5] Multi-threading Determinism & Buffer Reuse Stress ---\n");
  {
    // 동일한 조건에서 5회 연속 렌더링 후 비트 단위 완벽 일치(Determinism) 검증
    ShadeImage first = Plate::renderShade(doc, ShadeMode::DTak, 2.0);
    int bitwiseIdenticalCount = 0;
    for (int run = 1; run <= 5; ++run) {
      ShadeImage next = Plate::renderShade(doc, ShadeMode::DTak, 2.0);
      if (first.image.sizeInBytes() == next.image.sizeInBytes() &&
          std::memcmp(first.image.constBits(), next.image.constBits(),
                      first.image.sizeInBytes()) == 0) {
        ++bitwiseIdenticalCount;
      }
    }
    TEST_CHECK(bitwiseIdenticalCount == 5,
               "5 consecutive DTak renders are 100%% bitwise identical "
               "(Deterministic)");

    // 서로 다른 스레드에서 동시 호출 시 thread_local 격리 검증
    std::vector<std::thread> pool;
    std::atomic<int> successCount{0};
    for (int t = 0; t < 4; ++t) {
      pool.emplace_back([&, t]() {
        ShadeImage res = Plate::renderShade(doc, ShadeMode::DTak, 2.0);
        if (!res.isNull() && res.image.width() > 0 &&
            res.image.sizeInBytes() == first.image.sizeInBytes()) {
          if (std::memcmp(first.image.constBits(), res.image.constBits(),
                          first.image.sizeInBytes()) == 0) {
            ++successCount;
          }
        }
      });
    }
    for (auto &th : pool)
      th.join();
    TEST_CHECK(successCount == 4, "Concurrent calls from 4 separate threads "
                                  "produce 100%% identical results");
  }

  std::printf("\n=============================================================="
              "========\n");
  std::printf("[CHALLENGER 2] Final Verdict: %s (Total Fails: %d)\n",
              fails == 0 ? "ALL TESTS PASSED" : "FAILED", fails);
  std::printf("================================================================"
              "======\n");

  return fails == 0 ? 0 : 1;
}
