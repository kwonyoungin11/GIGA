#include "MeshRaster.h"

#include <algorithm>
#include <cmath>
#include <limits>
#include <numbers>
#include <thread>
#include <vector>

namespace studio {

namespace KA = DongArch::KArch;

namespace {
struct V3 {
  double x, y, z;
};
inline V3 sub(V3 a, V3 b) { return {a.x - b.x, a.y - b.y, a.z - b.z}; }
inline V3 cross(V3 a, V3 b) {
  return {a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x};
}

// 월드 → 카메라 변환 사전 계산 구조체
struct CamMatrix {
  double m00, m01, m03;
  double m10, m11, m12, m13;
  double m20, m21, m22, m23;
};

inline CamMatrix makeCamMatrix(const RasterView &v) {
  const double yw = v.yawDeg * std::numbers::pi / 180.0;
  const double pt = v.pitchDeg * std::numbers::pi / 180.0;
  const double cy = std::cos(yw), sy = std::sin(yw);
  const double cp = std::cos(pt), sp = std::sin(pt);

  CamMatrix m;
  m.m00 = cy;
  m.m01 = -sy;
  m.m03 = -cy * v.centerX + sy * v.centerY;

  m.m10 = -sp * sy;
  m.m11 = -sp * cy;
  m.m12 = cp;
  m.m13 = (sp * sy) * v.centerX + (sp * cy) * v.centerY - cp * v.centerZ;

  m.m20 = -cp * sy;
  m.m21 = -cp * cy;
  m.m22 = -sp;
  m.m23 = (cp * sy) * v.centerX + (cp * cy) * v.centerY + sp * v.centerZ;

  return m;
}

inline V3 toCamFast(const KA::Point3 &p, const CamMatrix &m) {
  const double cx = m.m00 * p.x + m.m01 * p.y + m.m03;
  const double cy = m.m10 * p.x + m.m11 * p.y + m.m12 * p.z + m.m13;
  const double cz = m.m20 * p.x + m.m21 * p.y + m.m22 * p.z + m.m23;
  return {cx, cy, cz};
}

// 월드 → 카메라 (단일 점 투영 호환용)
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

// 스레드별 재사용 래스터 버퍼 (Feature 12: 반복 힙 할당 방지)
struct ThreadRasterData {
  std::vector<float> zb;
  std::vector<float> lum;
  std::vector<QRgb> cbBuf;

  void reset(size_t pixelCount, bool useVCol, QRgb baseCol) {
    if (zb.size() < pixelCount)
      zb.resize(pixelCount);
    std::fill_n(zb.data(), pixelCount, -std::numeric_limits<float>::infinity());

    if (lum.size() < pixelCount)
      lum.resize(pixelCount);
    std::fill_n(lum.data(), pixelCount, 0.f);

    if (useVCol) {
      if (cbBuf.size() < pixelCount)
        cbBuf.resize(pixelCount);
      std::fill_n(cbBuf.data(), pixelCount, baseCol);
    }
  }
};

// 정점 투영 재사용 버퍼 (Feature 12)
struct VertexBuffers {
  std::vector<V3> sv;
  std::vector<V3> cv;

  void ensureSize(size_t n) {
    if (sv.size() < n)
      sv.resize(n);
    if (cv.size() < n)
      cv.resize(n);
  }
};

thread_local VertexBuffers tl_vertBuffers;
thread_local std::vector<ThreadRasterData> tl_threadPoolData;
thread_local std::vector<float> tl_final_zb;
thread_local std::vector<float> tl_final_lum;
thread_local std::vector<QRgb> tl_final_cbBuf;
thread_local std::vector<double> tl_sat;
thread_local std::vector<double> tl_cnt;
thread_local std::vector<float> tl_rel;

} // namespace

QPointF projectPoint(const KA::Point3 &p, const RasterView &v, double *depth) {
  const V3 c = toCam(p, v);
  if (depth)
    *depth = c.z;
  return {v.centerPx.x() + c.x * v.pxPerMm, v.centerPx.y() - c.y * v.pxPerMm};
}

QImage renderMesh(const KA::MeshData &mesh, QSize size, const RasterView &v,
                  ShadeMode mode, QColor base, std::vector<float> *depthOut,
                  const std::vector<QRgb> *vertexColors) {
  const int W = std::max(1, size.width()), H = std::max(1, size.height());
  QImage img(W, H, QImage::Format_ARGB32_Premultiplied);
  img.fill(Qt::transparent);
  const size_t numPixels = size_t(W) * H;
  const size_t numVerts = mesh.vertices.size();
  const size_t numFaces = mesh.faces.size();

  if (numVerts == 0 || numFaces == 0) {
    if (depthOut) {
      depthOut->assign(numPixels, std::numeric_limits<float>::quiet_NaN());
    }
    return img;
  }

  const bool useVCol = (mode == ShadeMode::Real) && vertexColors &&
                       vertexColors->size() == numVerts;
  const QRgb baseRgb = base.rgb();

  // Feature 12: 재사용 버퍼 확보 (100MB+ 반복 힙 할당 제거)
  tl_vertBuffers.ensureSize(numVerts);
  V3 *sv = tl_vertBuffers.sv.data();
  V3 *cv = tl_vertBuffers.cv.data();

  const unsigned int hwThreads = std::thread::hardware_concurrency();
  const unsigned int nThreads =
      std::max(1u, std::min(16u, hwThreads ? hwThreads : 4u));

  const CamMatrix camM = makeCamMatrix(v);
  const double pxPerMm = v.pxPerMm;
  const double centerPxX = v.centerPx.x();
  const double centerPxY = v.centerPx.y();

  // 1. 정점 변환 병렬화 (Feature 11)
  if (nThreads <= 1 || numVerts < 4096) {
    for (size_t i = 0; i < numVerts; ++i) {
      const V3 c = toCamFast(mesh.vertices[i], camM);
      cv[i] = c;
      sv[i] = {centerPxX + c.x * pxPerMm, centerPxY - c.y * pxPerMm, c.z};
    }
  } else {
    std::vector<std::thread> workers;
    workers.reserve(nThreads);
    for (unsigned int t = 0; t < nThreads; ++t) {
      const size_t v0 = t * numVerts / nThreads;
      const size_t v1 = (t + 1) * numVerts / nThreads;
      workers.emplace_back([=, &mesh]() {
        for (size_t i = v0; i < v1; ++i) {
          const V3 c = toCamFast(mesh.vertices[i], camM);
          cv[i] = c;
          sv[i] = {centerPxX + c.x * pxPerMm, centerPxY - c.y * pxPerMm, c.z};
        }
      });
    }
    for (auto &w : workers)
      w.join();
  }

  const V3 L = [] {
    V3 l{-0.45, 0.55, 0.70};
    const double n = std::sqrt(l.x * l.x + l.y * l.y + l.z * l.z);
    return V3{l.x / n, l.y / n, l.z / n};
  }();

  // 2. 스레드별 래스터 버퍼 준비 (Feature 12)
  const bool singleThread = (numFaces < 5000 || nThreads <= 1);
  const unsigned int numActiveBuffers = singleThread ? 1 : nThreads;
  if (tl_threadPoolData.size() < numActiveBuffers) {
    tl_threadPoolData.resize(numActiveBuffers);
  }
  for (unsigned int t = 0; t < numActiveBuffers; ++t) {
    tl_threadPoolData[t].reset(numPixels, useVCol, baseRgb);
  }
  ThreadRasterData *allThreadData = tl_threadPoolData.data();

  // 3. 면 래스터화 병렬화 (Feature 10 & 11)
  auto rasterizeFaces = [=, &mesh](size_t f0, size_t f1, unsigned int t) {
    ThreadRasterData &tData = allThreadData[t];
    float *zb = tData.zb.data();
    float *lum = tData.lum.data();
    QRgb *cbBuf = useVCol ? tData.cbBuf.data() : nullptr;

    const double maxCoordX = double(W - 1) + 0.5;
    const double maxCoordY = double(H - 1) + 0.5;

    for (size_t fi = f0; fi < f1; ++fi) {
      const auto &f = mesh.faces[fi];
      if (size_t(f[0]) >= numVerts || size_t(f[1]) >= numVerts ||
          size_t(f[2]) >= numVerts)
        continue;
      const V3 a = sv[size_t(f[0])], b = sv[size_t(f[1])], c = sv[size_t(f[2])];

      // Feature 10: Early BBox & Subpixel Reject (화면 밖 및 서브픽셀 삼각형
      // 조기 탈락)
      const double minX = std::min({a.x, b.x, c.x});
      if (minX > maxCoordX)
        continue;
      const double maxX = std::max({a.x, b.x, c.x});
      if (maxX < 0.5)
        continue;
      const int x0 = std::max(0, int(std::ceil(minX - 0.5)));
      const int x1 = std::min(W - 1, int(std::floor(maxX - 0.5)));
      if (x0 > x1)
        continue;

      const double minY = std::min({a.y, b.y, c.y});
      if (minY > maxCoordY)
        continue;
      const double maxY = std::max({a.y, b.y, c.y});
      if (maxY < 0.5)
        continue;
      const int y0 = std::max(0, int(std::ceil(minY - 0.5)));
      const int y1 = std::min(H - 1, int(std::floor(maxY - 0.5)));
      if (y0 > y1)
        continue;

      const double area = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
      if (std::fabs(area) < 1e-12)
        continue;

      // 통과된 삼각형에 대해서만 고비용 법선·조명·정점색 계산 수행
      const V3 ca = cv[size_t(f[0])], cb = cv[size_t(f[1])],
               cc = cv[size_t(f[2])];
      V3 n = cross(sub(cb, ca), sub(cc, ca));
      const double nl = std::sqrt(n.x * n.x + n.y * n.y + n.z * n.z);
      if (nl < 1e-15)
        continue;
      n = {n.x / nl, n.y / nl, n.z / nl};
      if (n.z < 0)
        n = {-n.x, -n.y, -n.z}; // 양면 조명
      const float shade =
          float(0.28 + 0.72 * std::max(0.0, n.x * L.x + n.y * L.y + n.z * L.z));

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
          if (w0 < -1e-12 || w1 < -1e-12 || w2 < -1e-12)
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
              const int bb = int(
                  (w0 * qBlue(c0) + w1 * qBlue(c1) + w2 * qBlue(c2)) * invS +
                  0.5);
              cbBuf[k] = qRgb(std::clamp(rr, 0, 255), std::clamp(gg, 0, 255),
                              std::clamp(bb, 0, 255));
            }
          }
        }
      }
    }
  };

  if (singleThread) {
    rasterizeFaces(0, numFaces, 0);
  } else {
    std::vector<std::thread> workers;
    workers.reserve(nThreads);
    for (unsigned int t = 0; t < nThreads; ++t) {
      const size_t f0 = t * numFaces / nThreads;
      const size_t f1 = (t + 1) * numFaces / nThreads;
      workers.emplace_back(rasterizeFaces, f0, f1, t);
    }
    for (auto &w : workers)
      w.join();
  }

  // 4. 스레드별 버퍼 병합 (Feature 11)
  if (tl_final_zb.size() < numPixels)
    tl_final_zb.resize(numPixels);
  if (tl_final_lum.size() < numPixels)
    tl_final_lum.resize(numPixels);
  if (useVCol && tl_final_cbBuf.size() < numPixels)
    tl_final_cbBuf.resize(numPixels);

  float *finalZb = tl_final_zb.data();
  float *finalLum = tl_final_lum.data();
  QRgb *finalCbBuf = useVCol ? tl_final_cbBuf.data() : nullptr;

  if (singleThread) {
    std::copy_n(tl_threadPoolData[0].zb.data(), numPixels, finalZb);
    std::copy_n(tl_threadPoolData[0].lum.data(), numPixels, finalLum);
    if (useVCol) {
      std::copy_n(tl_threadPoolData[0].cbBuf.data(), numPixels, finalCbBuf);
    }
  } else {
    auto mergeSlices = [=](size_t k0, size_t k1) {
      for (size_t k = k0; k < k1; ++k) {
        float bestZ = -std::numeric_limits<float>::infinity();
        int bestT = -1;
        for (unsigned int t = 0; t < nThreads; ++t) {
          const float z = allThreadData[t].zb[k];
          if (z > bestZ) {
            bestZ = z;
            bestT = int(t);
          }
        }
        if (bestT >= 0) {
          finalZb[k] = bestZ;
          finalLum[k] = allThreadData[size_t(bestT)].lum[k];
          if (useVCol)
            finalCbBuf[k] = allThreadData[size_t(bestT)].cbBuf[k];
        } else {
          finalZb[k] = -std::numeric_limits<float>::infinity();
          finalLum[k] = 0.f;
        }
      }
    };

    std::vector<std::thread> workers;
    workers.reserve(nThreads);
    for (unsigned int t = 0; t < nThreads; ++t) {
      const size_t k0 = t * numPixels / nThreads;
      const size_t k1 = (t + 1) * numPixels / nThreads;
      workers.emplace_back(mergeSlices, k0, k1);
    }
    for (auto &w : workers)
      w.join();
  }

  // 5. 디지털 탁본(DTak) 모드: 2D SAT 및 국소 평균(반경 ≈ 2.2 mm) 연산
  if (mode == ShadeMode::DTak) {
    const int r = std::max(2, int(std::lround(2.2 * v.pxPerMm)));
    const size_t satSize = size_t(W + 1) * (H + 1);
    if (tl_sat.size() < satSize)
      tl_sat.resize(satSize);
    if (tl_cnt.size() < satSize)
      tl_cnt.resize(satSize);
    double *sat = tl_sat.data();
    double *cnt = tl_cnt.data();
    std::fill_n(sat, W + 1, 0.0);
    std::fill_n(cnt, W + 1, 0.0);

    for (int y = 0; y < H; ++y) {
      sat[size_t(y + 1) * (W + 1)] = 0.0;
      cnt[size_t(y + 1) * (W + 1)] = 0.0;
      double rowSat = 0.0;
      double rowCnt = 0.0;
      const size_t prevRow = size_t(y) * (W + 1);
      const size_t currRow = size_t(y + 1) * (W + 1);
      for (int x = 0; x < W; ++x) {
        const float z = finalZb[size_t(y) * W + x];
        const bool ok = std::isfinite(z);
        rowSat += (ok ? double(z) : 0.0);
        rowCnt += (ok ? 1.0 : 0.0);
        sat[currRow + size_t(x + 1)] = sat[prevRow + size_t(x + 1)] + rowSat;
        cnt[currRow + size_t(x + 1)] = cnt[prevRow + size_t(x + 1)] + rowCnt;
      }
    }

    if (tl_rel.size() < numPixels)
      tl_rel.resize(numPixels);
    float *relData = tl_rel.data();

    auto computeRelRows = [=](int yStart, int yEnd) {
      for (int y = yStart; y < yEnd; ++y) {
        for (int x = 0; x < W; ++x) {
          const size_t k = size_t(y) * W + x;
          if (!std::isfinite(finalZb[k])) {
            relData[k] = 0.f;
            continue;
          }
          const int ax = std::max(0, x - r), bx = std::min(W, x + r + 1);
          const int ay = std::max(0, y - r), by = std::min(H, y + r + 1);
          const double count =
              cnt[size_t(by) * (W + 1) + bx] - cnt[size_t(ay) * (W + 1) + bx] -
              cnt[size_t(by) * (W + 1) + ax] + cnt[size_t(ay) * (W + 1) + ax];
          const double sumZ =
              sat[size_t(by) * (W + 1) + bx] - sat[size_t(ay) * (W + 1) + bx] -
              sat[size_t(by) * (W + 1) + ax] + sat[size_t(ay) * (W + 1) + ax];
          relData[k] = count > 0.0 ? float(finalZb[k] - sumZ / count) : 0.f;
        }
      }
    };

    if (nThreads <= 1 || H < 16) {
      computeRelRows(0, H);
    } else {
      std::vector<std::thread> workers;
      workers.reserve(nThreads);
      for (unsigned int t = 0; t < nThreads; ++t) {
        const int y0 = int(t * H / nThreads);
        const int y1 = int((t + 1) * H / nThreads);
        workers.emplace_back(computeRelRows, y0, y1);
      }
      for (auto &w : workers)
        w.join();
    }
  }

  // 6. 스캔라인 병렬 이미지 변환
  float zmin = 0.f, zr = 1.f;
  if (mode == ShadeMode::Depth) {
    float minVal = std::numeric_limits<float>::infinity();
    float maxVal = -minVal;
    for (size_t k = 0; k < numPixels; ++k) {
      const float z = finalZb[k];
      if (std::isfinite(z)) {
        minVal = std::min(minVal, z);
        maxVal = std::max(maxVal, z);
      }
    }
    zmin = minVal;
    zr = std::max(1e-3f, maxVal - minVal);
  }

  const float *relData = (mode == ShadeMode::DTak) ? tl_rel.data() : nullptr;

  auto renderRows = [=, &img](int yStart, int yEnd) {
    for (int y = yStart; y < yEnd; ++y) {
      auto *line = reinterpret_cast<QRgb *>(img.scanLine(y));
      for (int x = 0; x < W; ++x) {
        const size_t k = size_t(y) * W + x;
        const float z = finalZb[k];
        if (!std::isfinite(z))
          continue;
        QColor c;
        if (mode == ShadeMode::Real) {
          const float s = finalLum[k];
          const QColor vc = useVCol ? QColor::fromRgb(finalCbBuf[k]) : base;
          c = QColor::fromRgbF(std::min(1.f, float(vc.redF()) * s * 1.12f),
                               std::min(1.f, float(vc.greenF()) * s * 1.12f),
                               std::min(1.f, float(vc.blueF()) * s * 1.12f));
        } else if (mode == ShadeMode::Depth) {
          const float t = (z - zmin) / zr;
          const float g = 0.55f + 0.40f * t * (0.6f + 0.4f * finalLum[k]);
          c = QColor::fromRgbF(g * 0.98f, g * 0.97f, g * 0.93f);
        } else {
          const float t = std::clamp(0.5f - relData[k] * 0.95f, 0.f, 1.f);
          const float g = (0.38f + 0.58f * t) * (0.82f + 0.18f * finalLum[k]);
          c = QColor::fromRgbF(g * 0.97f, g * 0.96f, g * 0.93f);
        }
        line[x] = c.rgba();
      }
    }
  };

  if (nThreads <= 1 || H < 16) {
    renderRows(0, H);
  } else {
    std::vector<std::thread> workers;
    workers.reserve(nThreads);
    for (unsigned int t = 0; t < nThreads; ++t) {
      const int y0 = int(t * H / nThreads);
      const int y1 = int((t + 1) * H / nThreads);
      workers.emplace_back(renderRows, y0, y1);
    }
    for (auto &w : workers)
      w.join();
  }

  if (depthOut) {
    depthOut->assign(numPixels, std::numeric_limits<float>::quiet_NaN());
    for (size_t k = 0; k < numPixels; ++k) {
      if (std::isfinite(finalZb[k])) {
        (*depthOut)[k] = finalZb[k];
      }
    }
  }

  return img;
}

} // namespace studio
