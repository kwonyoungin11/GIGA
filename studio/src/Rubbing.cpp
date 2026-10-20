#include "Rubbing.h"

#include <algorithm>
#include <cmath>
#include <limits>
#include <numbers>
#include <thread>
#include <vector>

namespace studio {

namespace KA = DongArch::KArch;

namespace {
constexpr double kPi = std::numbers::pi;
constexpr float kNoData = -std::numeric_limits<float>::infinity();

inline double deg2rad(double d) { return d * kPi / 180.0; }
inline double rad2deg(double r) { return r * 180.0 / kPi; }

unsigned workerCount() {
  const unsigned hw = std::thread::hardware_concurrency();
  return std::clamp(hw ? hw : 4u, 1u, 16u);
}

// [0, n) 을 스레드로 나눠 f(begin, end) 실행
template <class F> void parallelRange(size_t n, F &&f) {
  const unsigned T = workerCount();
  if (n < 2048 || T <= 1) {
    f(size_t(0), n);
    return;
  }
  std::vector<std::thread> ws;
  ws.reserve(T);
  for (unsigned t = 0; t < T; ++t)
    ws.emplace_back(f, t * n / T, (t + 1) * n / T);
  for (auto &w : ws)
    w.join();
}

// 1D 상자 평균(누적 합) — 바깥은 0 으로 보고, 정규화는 호출자가 마스크로 한다
void boxLine(const float *src, float *dst, int n, int stride, int r) {
  double s = 0.0;
  for (int i = 0; i <= std::min(r, n - 1); ++i)
    s += src[size_t(i) * stride];
  for (int i = 0; i < n; ++i) {
    dst[size_t(i) * stride] = float(s);
    const int add = i + r + 1, rem = i - r;
    if (add < n)
      s += src[size_t(add) * stride];
    if (rem >= 0)
      s -= src[size_t(rem) * stride];
  }
}

void boxBlur2D(std::vector<float> &img, int W, int H, int r) {
  if (r < 1)
    return;
  std::vector<float> tmp(img.size());
  parallelRange(size_t(H), [&](size_t y0, size_t y1) {
    for (size_t y = y0; y < y1; ++y)
      boxLine(img.data() + y * W, tmp.data() + y * W, W, 1, r);
  });
  parallelRange(size_t(W), [&](size_t x0, size_t x1) {
    for (size_t x = x0; x < x1; ++x)
      boxLine(tmp.data() + x, img.data() + x, H, W, r);
  });
}

// 마스크 정규화 가우스 근사(상자 3번): 유효 화소만 평균한다
std::vector<float> maskedSmooth(const std::vector<float> &val, int W, int H,
                                double sigmaPx, int passes = 3) {
  const size_t N = val.size();
  std::vector<float> a(N), m(N);
  for (size_t k = 0; k < N; ++k) {
    const bool ok = val[k] != kNoData;
    a[k] = ok ? val[k] : 0.f;
    m[k] = ok ? 1.f : 0.f;
  }
  const int r = std::max(
      1, int(std::lround((std::sqrt(4.0 * sigmaPx * sigmaPx / (passes / 3.0) + 1.0) - 1.0) / 2.0)));
  for (int i = 0; i < passes; ++i) {
    boxBlur2D(a, W, H, r);
    boxBlur2D(m, W, H, r);
  }
  std::vector<float> out(N, kNoData);
  for (size_t k = 0; k < N; ++k)
    if (m[k] > 1e-4f)
      out[k] = a[k] / m[k];
  return out;
}
} // namespace

UnrolledRubbing renderUnrolledRubbing(const KA::MeshData &mesh,
                                      const RubbingRegion &region,
                                      const RubbingParams &params) {
  UnrolledRubbing out;
  const double zLo = std::min(region.zLo, region.zHi);
  const double zHi = std::max(region.zLo, region.zHi);
  const double spanDeg = std::clamp(region.azSpanDeg, 0.5, 360.0);
  const double halfSpan = 0.5 * deg2rad(spanDeg);
  const bool fullTurn = spanDeg >= 359.999;
  if (mesh.vertices.empty() || mesh.faces.empty() || zHi - zLo < 0.5 ||
      params.pxPerMm <= 0.0)
    return out;

  // 1) 정점마다 (중심 기준 방위각 a ∈ (−π, π], 반경 r, 높이 z)
  const size_t nV = mesh.vertices.size();
  const double cA = deg2rad(region.azCenterDeg);
  std::vector<float> va(nV), vr(nV);
  parallelRange(nV, [&](size_t i0, size_t i1) {
    for (size_t i = i0; i < i1; ++i) {
      const auto &p = mesh.vertices[i];
      vr[i] = float(std::hypot(p.x, p.y));
      va[i] = float(std::remainder(std::atan2(p.y, p.x) - cA, 2.0 * kPi));
    }
  });

  // 2) 호 길이 환산 반경: 1 mm 높이 칸마다 범위 안 정점의 최대 반경(= 외면) → 중앙값
  const int nb = std::max(1, int(std::ceil(zHi - zLo)));
  std::vector<float> binMax(size_t(nb), 0.f);
  for (size_t i = 0; i < nV; ++i) {
    const double z = mesh.vertices[i].z;
    if (z < zLo || z > zHi || std::fabs(va[i]) > halfSpan + 1e-9)
      continue;
    const int b = std::min(nb - 1, int((z - zLo)));
    binMax[size_t(b)] = std::max(binMax[size_t(b)], vr[i]);
  }
  std::vector<float> filled;
  for (float b : binMax)
    if (b > 0.5f)
      filled.push_back(b);
  if (filled.empty())
    return out;
  std::nth_element(filled.begin(), filled.begin() + filled.size() / 2, filled.end());
  const double Rref = filled[filled.size() / 2];

  // 3) 격자 크기 (가로 = 호 길이, 세로 = 높이)
  double ppm = params.pxPerMm;
  const double widthMm = 2.0 * halfSpan * Rref, heightMm = zHi - zLo;
  ppm = std::min({ppm, params.maxSidePx / std::max(1.0, widthMm),
                  params.maxSidePx / std::max(1.0, heightMm)});
  const int W = std::max(2, int(std::lround(widthMm * ppm)));
  const int H = std::max(2, int(std::lround(heightMm * ppm)));
  const size_t N = size_t(W) * H;
  const double uScale = Rref * ppm; // 라디안 → 화소

  // 4) 면 래스터: 각 화소에서 가장 바깥 반경(외면)을 남긴다
  // 스레드별 버퍼 합계가 256 MB 를 넘지 않게 (600 dpi 한 바퀴 전개는 수천만 화소)
  const size_t budget = size_t(256) << 20;
  const unsigned T = std::max(
      1u, std::min(workerCount(),
                   unsigned(std::min<size_t>(16, budget / std::max<size_t>(1, N * sizeof(float))))));
  std::vector<std::vector<float>> bufs(T, std::vector<float>(N, kNoData));
  const double pad = 1.0;
  auto rasterTri = [&](float *R, double u0, double v0, double r0, double u1,
                       double v1, double r1, double u2, double v2, double r2) {
    const double minU = std::min({u0, u1, u2}), maxU = std::max({u0, u1, u2});
    const double minV = std::min({v0, v1, v2}), maxV = std::max({v0, v1, v2});
    if (maxU < 0.0 || minU > W || maxV < 0.0 || minV > H)
      return;
    const int x0 = std::max(0, int(std::ceil(minU - 0.5)));
    const int x1 = std::min(W - 1, int(std::floor(maxU - 0.5)));
    const int y0 = std::max(0, int(std::ceil(minV - 0.5)));
    const int y1 = std::min(H - 1, int(std::floor(maxV - 0.5)));
    if (x0 > x1 || y0 > y1)
      return;
    const double area = (u1 - u0) * (v2 - v0) - (v1 - v0) * (u2 - u0);
    if (std::fabs(area) < 1e-12)
      return;
    const double inv = 1.0 / area;
    for (int y = y0; y <= y1; ++y) {
      const double py = y + 0.5;
      for (int x = x0; x <= x1; ++x) {
        const double px = x + 0.5;
        const double w0 = ((u1 - px) * (v2 - py) - (v1 - py) * (u2 - px)) * inv;
        const double w1 = ((u2 - px) * (v0 - py) - (v2 - py) * (u0 - px)) * inv;
        const double w2 = 1.0 - w0 - w1;
        if (w0 < -1e-9 || w1 < -1e-9 || w2 < -1e-9)
          continue;
        const float r = float(w0 * r0 + w1 * r1 + w2 * r2);
        float &dst = R[size_t(y) * W + x];
        if (r > dst)
          dst = r;
      }
    }
  };
  const size_t nF = mesh.faces.size();
  auto faceWorker = [&](unsigned t) {
    float *R = bufs[t].data();
    const size_t f0 = t * nF / T, f1 = (t + 1) * nF / T;
    for (size_t fi = f0; fi < f1; ++fi) {
      const auto &f = mesh.faces[fi];
      if (size_t(f[0]) >= nV || size_t(f[1]) >= nV || size_t(f[2]) >= nV)
        continue;
      const auto &p0 = mesh.vertices[size_t(f[0])], &p1 = mesh.vertices[size_t(f[1])],
                 &p2 = mesh.vertices[size_t(f[2])];
      if (std::max({p0.z, p1.z, p2.z}) < zLo - pad ||
          std::min({p0.z, p1.z, p2.z}) > zHi + pad)
        continue;
      double a0 = va[size_t(f[0])], a1 = va[size_t(f[1])], a2 = va[size_t(f[2])];
      const double r0 = vr[size_t(f[0])], r1 = vr[size_t(f[1])], r2 = vr[size_t(f[2])];
      if (std::min({r0, r1, r2}) < 1.0)
        continue; // 축 근처(바닥 가운데) — 전개 대상 아님
      bool seam = false;
      if (std::max({a0, a1, a2}) - std::min({a0, a1, a2}) > kPi) {
        seam = true; // ±π 이음매를 걸친 면: 음수 쪽을 +2π 로 옮겨 이어 붙인다
        if (a0 < 0)
          a0 += 2.0 * kPi;
        if (a1 < 0)
          a1 += 2.0 * kPi;
        if (a2 < 0)
          a2 += 2.0 * kPi;
      }
      if (std::max({a0, a1, a2}) - std::min({a0, a1, a2}) > 0.25 * kPi)
        continue; // 한 면이 45° 넘게 걸치면 축 가까운 면 — 버림
      const double v0 = (zHi - p0.z) * ppm, v1 = (zHi - p1.z) * ppm,
                   v2 = (zHi - p2.z) * ppm;
      auto rasterShift = [&](double shift) {
        rasterTri(R, (a0 + shift + halfSpan) * uScale, v0, r0,
                  (a1 + shift + halfSpan) * uScale, v1, r1,
                  (a2 + shift + halfSpan) * uScale, v2, r2);
      };
      rasterShift(0.0);
      if (seam || fullTurn)
        rasterShift(-2.0 * kPi);
    }
  };
  {
    std::vector<std::thread> ws;
    ws.reserve(T);
    for (unsigned t = 0; t < T; ++t)
      ws.emplace_back(faceWorker, t);
    for (auto &w : ws)
      w.join();
  }
  std::vector<float> R(std::move(bufs[0]));
  for (unsigned t = 1; t < T; ++t) {
    const float *b = bufs[t].data();
    parallelRange(N, [&](size_t k0, size_t k1) {
      for (size_t k = k0; k < k1; ++k)
        R[k] = std::max(R[k], b[k]);
    });
  }
  bufs.clear();

  // 5) 작은 구멍 메우기 (주변 3개 이상 유효)
  for (int it = 0; it < 2; ++it) {
    std::vector<float> nxt(R);
    parallelRange(size_t(H), [&](size_t y0, size_t y1) {
      for (size_t y = y0; y < y1; ++y)
        for (int x = 0; x < W; ++x) {
          const size_t k = y * W + x;
          if (R[k] != kNoData)
            continue;
          double s = 0;
          int c = 0;
          for (int dy = -1; dy <= 1; ++dy)
            for (int dx = -1; dx <= 1; ++dx) {
              const int yy = int(y) + dy, xx = x + dx;
              if (yy < 0 || yy >= H || xx < 0 || xx >= W)
                continue;
              const float q = R[size_t(yy) * W + xx];
              if (q != kNoData) {
                s += q;
                ++c;
              }
            }
          if (c >= 3)
            nxt[k] = float(s / c);
        }
    });
    R.swap(nxt);
  }

  // 6) 요철 = 외면 반경 − 넓게 평활한 기준면 (+ 스캔 잡음만 살짝 누름)
  const std::vector<float> base = maskedSmooth(R, W, H, params.baseSigmaMm * ppm);
  std::vector<float> D(N, kNoData);
  size_t valid = 0;
  for (size_t k = 0; k < N; ++k)
    if (R[k] != kNoData && base[k] != kNoData) {
      D[k] = R[k] - base[k];
      ++valid;
    }
  if (valid == 0)
    return out;
  const double denoisePx = 0.12 * ppm;
  if (denoisePx >= 1.0) {
    const std::vector<float> sm = maskedSmooth(D, W, H, denoisePx / 1.5, 1);
    for (size_t k = 0; k < N; ++k)
      if (D[k] != kNoData && sm[k] != kNoData)
        D[k] = sm[k];
  }

  // 7) 농담 기준: |요철| 95 백분위 (유물마다 문양 깊이가 달라 고정값을 쓰지 않는다)
  std::vector<float> mags;
  mags.reserve(std::min<size_t>(valid, 400000));
  const size_t stride = std::max<size_t>(1, valid / 400000);
  size_t seen = 0;
  for (size_t k = 0; k < N; ++k)
    if (D[k] != kNoData && (seen++ % stride) == 0)
      mags.push_back(std::fabs(D[k]));
  const size_t q = std::min(mags.size() - 1, size_t(0.95 * double(mags.size())));
  std::nth_element(mags.begin(), mags.begin() + q, mags.end());
  const double s = std::clamp(double(mags[q]), 0.06, 1.5);

  // 8) 먹 농담: 기면·돌대 = 먹, 파인 곳 = 한지 흰색 (실제 탁본처럼 평면도 먹이 묻는다).
  //    스캔이 없는 곳은 한지와 헷갈리지 않게 푸른 회색 빗금.
  QImage img(W, H, QImage::Format_RGB32);
  const QRgb noData = qRgb(203, 209, 217), noDataHatch = qRgb(176, 184, 195);
  const double paper = 0.965, ink = 0.10, w = 0.25 * s;
  parallelRange(size_t(H), [&](size_t y0, size_t y1) {
    for (size_t y = y0; y < y1; ++y) {
      auto *line = reinterpret_cast<QRgb *>(img.scanLine(int(y)));
      for (int x = 0; x < W; ++x) {
        const float d = D[y * W + x];
        if (d == kNoData) {
          line[x] = ((x + int(y)) % 10 == 0) ? noDataHatch : noData;
          continue;
        }
        const double k = 1.0 / (1.0 + std::exp(-(d + w) / w));
        const double g = paper - k * (paper - ink);
        line[x] = qRgb(int(std::lround(255.0 * g * 0.99)),
                       int(std::lround(255.0 * g * 0.98)),
                       int(std::lround(255.0 * g * 0.95)));
      }
    }
  });

  out.image = std::move(img);
  out.widthMm = widthMm;
  out.heightMm = heightMm;
  out.refRadiusMm = Rref;
  out.pxPerMm = ppm;
  out.coverage = double(valid) / double(N);
  return out;
}

RubbingRegion regionFromScreenBox(const QRectF &box, const RasterView &v,
                                  double frontAngleDeg,
                                  const std::function<double(double)> &outerRadiusAt,
                                  bool farSide) {
  const QRectF b = box.normalized();
  const double yw = deg2rad(v.yawDeg), pt = deg2rad(v.pitchDeg);
  const double cyw = std::cos(yw), syw = std::sin(yw);
  const double cp = std::max(0.15, std::cos(pt)), sp = std::sin(pt);
  const double ppm = std::max(1e-6, v.pxPerMm);
  const double side = farSide ? 1.0 : -1.0; // 카메라 깊이축 y1 부호: 앞 벽 −, 뒤 벽 +
  // 중심축(0, 0)의 카메라 좌표 — 화면 중심이 축에서 벗어나 있어도 축 기준으로 잰다
  const double x1Axis = -(cyw * v.centerX - syw * v.centerY);
  const double y1Axis = -(syw * v.centerX + cyw * v.centerY);
  auto xRel = [&](double sx) { return (sx - v.centerPx.x()) / ppm - x1Axis; };
  auto upOf = [&](double sy) { return (v.centerPx.y() - sy) / ppm; };
  auto radius = [&](double z, double xr) {
    const double r = outerRadiusAt ? outerRadiusAt(z) : -1.0;
    return r > 0.0 ? r : std::max(1.0, std::fabs(xr));
  };
  const double xMid = xRel(b.center().x());
  auto zAt = [&](double up) { // 보이는 벽 점의 높이 (기울어진 시점이면 앞뒤 깊이 보정)
    double z = v.centerZ + up / cp;
    for (int it = 0; it < 3; ++it) {
      const double R = radius(z, xMid);
      const double y1 = y1Axis + side * std::sqrt(std::max(0.0, R * R - xMid * xMid));
      z = v.centerZ + (up + sp * y1) / cp;
    }
    return z;
  };
  const double zA = zAt(upOf(b.top())), zB = zAt(upOf(b.bottom()));
  RubbingRegion r;
  r.zLo = std::min(zA, zB);
  r.zHi = std::max(zA, zB);
  const double R = radius(0.5 * (r.zLo + r.zHi), xMid);
  // 화면 x1 = R·cos(α + yaw), 앞 벽은 sin(α + yaw) < 0 → α = −acos(x1/R) − yaw,
  // 뒤 벽은 sin > 0 → α = +acos(x1/R) − yaw
  auto alpha = [&](double sx) {
    return side * std::acos(std::clamp(xRel(sx) / R, -1.0, 1.0)) - yw;
  };
  const double aL = alpha(b.left()), aR = alpha(b.right());
  r.azSpanDeg = std::clamp(rad2deg(std::fabs(aR - aL)), 0.5, 180.0);
  double c = rad2deg(0.5 * (aL + aR)) - frontAngleDeg;
  c = std::fmod(std::fmod(c, 360.0) + 360.0, 360.0);
  r.azCenterDeg = c;
  return r;
}

bool saveRubbingPng(const KA::MeshData &mesh, const RubbingRegion &region,
                    const QString &path, double dpi, QString *err) {
  RubbingParams p;
  p.pxPerMm = std::clamp(dpi, 72.0, 1200.0) / 25.4;
  p.maxSidePx = 20000;
  UnrolledRubbing rub = renderUnrolledRubbing(mesh, region, p);
  if (!rub.ok()) {
    if (err)
      *err = QStringLiteral("선택한 범위에 스캔 면이 없습니다");
    return false;
  }
  const int dpm = int(std::lround(rub.pxPerMm * 1000.0)); // 실제 화소/mm → dots per meter
  rub.image.setDotsPerMeterX(dpm);
  rub.image.setDotsPerMeterY(dpm);
  rub.image.setText(QStringLiteral("Description"),
                    QStringLiteral("DongArch3D unrolled rubbing %1 x %2 mm")
                        .arg(rub.widthMm, 0, 'f', 1)
                        .arg(rub.heightMm, 0, 'f', 1));
  if (!rub.image.save(path, "PNG")) {
    if (err)
      *err = QStringLiteral("파일을 저장할 수 없습니다: %1").arg(path);
    return false;
  }
  return true;
}

} // namespace studio
