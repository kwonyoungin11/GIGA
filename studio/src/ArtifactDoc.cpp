#include "ArtifactDoc.h"

#include "AxisOdr.h"
#include "AxisPca.h"
#include "MeshIO.h"
#include "MeshLoader.h"
#include "RimDetector.h"
#include "SyntheticMeshGen.h"
#include "TaubinFit.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <algorithm>
#include <cmath>
#include <limits>
#include <numbers>
#include <thread>
#include <unordered_map>

namespace studio {

namespace {

constexpr double kDeg = std::numbers::pi / 180.0;

void rotateZ(KA::MeshData &m, double deg) {
  const double c = std::cos(deg * kDeg), s = std::sin(deg * kDeg);
  for (auto &p : m.vertices) {
    const double x = c * p.x - s * p.y, y = s * p.x + c * p.y;
    p.x = x;
    p.y = y;
  }
}

// 방향 d 를 +Z 로 보내는 회전 (Rodrigues, d.z < 0 이면 반전하여 위쪽 방향 유지)
void rotateDirToZ(KA::MeshData &m, KA::Point3 d) {
  const double n = std::sqrt(d.x * d.x + d.y * d.y + d.z * d.z);
  if (n < 1e-12)
    return;
  d = {d.x / n, d.y / n, d.z / n};
  if (d.z < 0)
    d = {-d.x, -d.y, -d.z};
  const double kx = d.y, ky = -d.x; // k = d × z
  const double s = std::sqrt(kx * kx + ky * ky), c = d.z;
  if (s < 1e-12)
    return;
  const double ux = kx / s, uy = ky / s;
  for (auto &p : m.vertices) {
    const double dot = ux * p.x + uy * p.y;
    const double cx = uy * p.z, cy = -ux * p.z, cz = ux * p.y - uy * p.x;
    const KA::Point3 q{p.x * c + cx * s + ux * dot * (1 - c),
                       p.y * c + cy * s + uy * dot * (1 - c), p.z * c + cz * s};
    p = q;
  }
}

// 유향 단위벡터 up 을 +Z(0,0,1) 로 보내는 3x3 회전 행렬 생성 (up.z < 0 도 부호 유지)
void buildRotationToPlusZ(KA::Point3 up, double (&R)[3][3]) {
  R[0][0] = 1.0, R[0][1] = 0.0, R[0][2] = 0.0;
  R[1][0] = 0.0, R[1][1] = 1.0, R[1][2] = 0.0;
  R[2][0] = 0.0, R[2][1] = 0.0, R[2][2] = 1.0;
  const double n = std::sqrt(up.x * up.x + up.y * up.y + up.z * up.z);
  if (n < 1e-12)
    return;
  up = {up.x / n, up.y / n, up.z / n};
  if (up.z > 1.0 - 1e-9) {
    return;
  }
  if (up.z < -1.0 + 1e-9) {
    R[1][1] = -1.0;
    R[2][2] = -1.0;
    return;
  }
  const double kx = up.y, ky = -up.x;
  const double s = std::hypot(kx, ky), c = up.z;
  const double ux = kx / s, uy = ky / s;
  R[0][0] = c + ux * ux * (1.0 - c);
  R[0][1] = ux * uy * (1.0 - c);
  R[0][2] = uy * s;
  R[1][0] = uy * ux * (1.0 - c);
  R[1][1] = c + uy * uy * (1.0 - c);
  R[1][2] = -ux * s;
  R[2][0] = -uy * s;
  R[2][1] = ux * s;
  R[2][2] = c;
}

void applyMat3(KA::MeshData &m, const double (&R)[3][3],
               const KA::Point3 &ctr) {
  for (auto &p : m.vertices) {
    const double dx = p.x - ctr.x, dy = p.y - ctr.y, dz = p.z - ctr.z;
    p = {ctr.x + (R[0][0] * dx + R[0][1] * dy + R[0][2] * dz),
         ctr.y + (R[1][0] * dx + R[1][1] * dy + R[1][2] * dz),
         ctr.z + (R[2][0] * dx + R[2][1] * dy + R[2][2] * dz)};
  }
}

// 수밀(Watertight) 메시 또는 비스듬히 누운 유물의 회전 대칭축(입구 +Z, 바닥 -Z)을
// 3D 법선 교점 중심(C0) + 3D 법선-회전축 교차 점수 + 동축 원 적합(Taubin) + 상하 개구부 비교로 찾는다.
bool findCoarseVesselAxis(const KA::MeshData &work, KA::Point3 &outUpAxis,
                          KA::Point3 *outCenter = nullptr) {
  if (work.vertices.size() < 60)
    return false;
  std::vector<KA::Point3> pts;
  const size_t stride = std::max<size_t>(1, work.vertices.size() / 4000);
  pts.reserve(work.vertices.size() / stride + 1);
  double bMinX = 1e18, bMinY = 1e18, bMinZ = 1e18;
  double bMaxX = -1e18, bMaxY = -1e18, bMaxZ = -1e18;
  for (size_t i = 0; i < work.vertices.size(); i += stride) {
    const auto &p = work.vertices[i];
    pts.push_back(p);
    bMinX = std::min(bMinX, p.x), bMaxX = std::max(bMaxX, p.x);
    bMinY = std::min(bMinY, p.y), bMaxY = std::max(bMaxY, p.y);
    bMinZ = std::min(bMinZ, p.z), bMaxZ = std::max(bMaxZ, p.z);
  }
  const KA::Point3 bMid{0.5 * (bMinX + bMaxX), 0.5 * (bMinY + bMaxY),
                        0.5 * (bMinZ + bMaxZ)};
  const double rChar =
      0.5 * std::max({10.0, bMaxX - bMinX, bMaxY - bMinY, bMaxZ - bMinZ});

  struct FaceSample {
    KA::Point3 c;
    KA::Point3 n;
    KA::Point3 m;
    double area;
    double mLen;
  };
  std::vector<FaceSample> fSamples;
  const size_t nv = work.vertices.size();
  const size_t fStride = std::max<size_t>(1, work.faces.size() / 5000);
  fSamples.reserve(work.faces.size() / fStride + 1);
  for (size_t fi = 0; fi < work.faces.size(); fi += fStride) {
    const auto &f = work.faces[fi];
    if (f[0] < 0 || f[1] < 0 || f[2] < 0 || size_t(f[0]) >= nv ||
        size_t(f[1]) >= nv || size_t(f[2]) >= nv)
      continue;
    const auto &p0 = work.vertices[size_t(f[0])];
    const auto &p1 = work.vertices[size_t(f[1])];
    const auto &p2 = work.vertices[size_t(f[2])];
    const double e1x = p1.x - p0.x, e1y = p1.y - p0.y, e1z = p1.z - p0.z;
    const double e2x = p2.x - p0.x, e2y = p2.y - p0.y, e2z = p2.z - p0.z;
    const double nx = e1y * e2z - e1z * e2y;
    const double ny = e1z * e2x - e1x * e2z;
    const double nz = e1x * e2y - e1y * e2x;
    const double nl = std::sqrt(nx * nx + ny * ny + nz * nz);
    if (nl < 1e-8)
      continue;
    fSamples.push_back(
        {{(p0.x + p1.x + p2.x) / 3.0, (p0.y + p1.y + p2.y) / 3.0,
          (p0.z + p1.z + p2.z) / 3.0},
         {nx / nl, ny / nl, nz / nl},
         {0, 0, 0},
         0.5 * nl,
         0.0});
  }

  // 1) 3D 법선 직선들의 최소제곱 교점 중심 C0 (IRLS 6회 반복)
  KA::Point3 C0 = bMid;
  if (fSamples.size() >= 30) {
    for (int it = 0; it < 6; ++it) {
      double A[3][3] = {{0, 0, 0}, {0, 0, 0}, {0, 0, 0}};
      double B[3] = {0, 0, 0};
      double wSum = 0.0;
      for (const auto &fs : fSamples) {
        double w = fs.area;
        if (it > 0) {
          const double dx = C0.x - fs.c.x, dy = C0.y - fs.c.y,
                       dz = C0.z - fs.c.z;
          const double dot = dx * fs.n.x + dy * fs.n.y + dz * fs.n.z;
          const double d2 =
              std::max(0.0, dx * dx + dy * dy + dz * dz - dot * dot);
          w *= 1.0 / (1.0 + d2 / (0.04 * rChar * rChar));
        }
        wSum += w;
        const double p00 = 1.0 - fs.n.x * fs.n.x;
        const double p01 = -fs.n.x * fs.n.y;
        const double p02 = -fs.n.x * fs.n.z;
        const double p11 = 1.0 - fs.n.y * fs.n.y;
        const double p12 = -fs.n.y * fs.n.z;
        const double p22 = 1.0 - fs.n.z * fs.n.z;
        A[0][0] += w * p00;
        A[0][1] += w * p01;
        A[0][2] += w * p02;
        A[1][1] += w * p11;
        A[1][2] += w * p12;
        A[2][2] += w * p22;
        B[0] += w * (p00 * fs.c.x + p01 * fs.c.y + p02 * fs.c.z);
        B[1] += w * (p01 * fs.c.x + p11 * fs.c.y + p12 * fs.c.z);
        B[2] += w * (p02 * fs.c.x + p12 * fs.c.y + p22 * fs.c.z);
      }
      A[1][0] = A[0][1];
      A[2][0] = A[0][2];
      A[2][1] = A[1][2];
      const double reg = 1e-3 * std::max(1.0, wSum);
      A[0][0] += reg;
      A[1][1] += reg;
      A[2][2] += reg;
      B[0] += reg * bMid.x;
      B[1] += reg * bMid.y;
      B[2] += reg * bMid.z;
      // 3x3 가우스 소거
      double M[3][4] = {{A[0][0], A[0][1], A[0][2], B[0]},
                        {A[1][0], A[1][1], A[1][2], B[1]},
                        {A[2][0], A[2][1], A[2][2], B[2]}};
      bool singular = false;
      for (int i = 0; i < 3; ++i) {
        int piv = i;
        for (int r = i + 1; r < 3; ++r)
          if (std::fabs(M[r][i]) > std::fabs(M[piv][i]))
            piv = r;
        if (std::fabs(M[piv][i]) < 1e-12) {
          singular = true;
          break;
        }
        for (int c = 0; c < 4; ++c)
          std::swap(M[i][c], M[piv][c]);
        const double inv = 1.0 / M[i][i];
        for (int c = i; c < 4; ++c)
          M[i][c] *= inv;
        for (int r = 0; r < 3; ++r) {
          if (r == i)
            continue;
          const double f = M[r][i];
          for (int c = i; c < 4; ++c)
            M[r][c] -= f * M[i][c];
        }
      }
      if (!singular &&
          std::hypot(M[0][3] - bMid.x, M[1][3] - bMid.y, M[2][3] - bMid.z) <
              1.5 * rChar) {
        C0 = {M[0][3], M[1][3], M[2][3]};
      }
    }
  }
  if (outCenter)
    *outCenter = C0;

  for (auto &fs : fSamples) {
    const double rx = fs.c.x - C0.x, ry = fs.c.y - C0.y, rz = fs.c.z - C0.z;
    fs.m = {ry * fs.n.z - rz * fs.n.y, rz * fs.n.x - rx * fs.n.z,
            rx * fs.n.y - ry * fs.n.x};
    fs.mLen = std::sqrt(fs.m.x * fs.m.x + fs.m.y * fs.m.y + fs.m.z * fs.m.z);
  }

  auto makeBasis = [](const KA::Point3 &u, KA::Point3 &e1, KA::Point3 &e2) {
    const KA::Point3 ref =
        (std::fabs(u.z) < 0.85) ? KA::Point3{0, 0, 1} : KA::Point3{1, 0, 0};
    const double d = ref.x * u.x + ref.y * u.y + ref.z * u.z;
    KA::Point3 a{ref.x - d * u.x, ref.y - d * u.y, ref.z - d * u.z};
    const double la = std::sqrt(a.x * a.x + a.y * a.y + a.z * a.z);
    e1 = (la > 1e-12) ? KA::Point3{a.x / la, a.y / la, a.z / la}
                      : KA::Point3{1, 0, 0};
    e2 = {u.y * e1.z - u.z * e1.y, u.z * e1.x - u.x * e1.z,
          u.x * e1.y - u.y * e1.x};
  };

  auto scoreAxis = [&](const KA::Point3 &u) -> double {
    KA::Point3 e1, e2;
    makeBasis(u, e1, e2);
    double zMin = 1e18, zMax = -1e18;
    for (const auto &p : pts) {
      const double z = p.x * u.x + p.y * u.y + p.z * u.z;
      zMin = std::min(zMin, z);
      zMax = std::max(zMax, z);
    }
    const double H = zMax - zMin;
    if (H < 10.0)
      return -1e9;

    // A) 비구면(경부·구연·돌대·어깨·저부) 법선 직선들이 회전축 (C0, u) 와 교차하는 점수
    double normScore = 0.0;
    double normWeight = 0.0;
    const double minM = 0.14 * rChar;
    const double inv2Sig2 = 1.0 / (2.0 * std::pow(0.022 * rChar, 2.0));
    for (const auto &fs : fSamples) {
      if (fs.mLen < minM)
        continue;
      const double un = u.x * fs.n.x + u.y * fs.n.y + u.z * fs.n.z;
      const double sin2 = 1.0 - un * un;
      if (sin2 < 0.08)
        continue;
      const double d =
          std::fabs(fs.m.x * u.x + fs.m.y * u.y + fs.m.z * u.z) /
          std::sqrt(sin2);
      const double w = fs.area * (fs.mLen / rChar);
      normWeight += w;
      normScore += w * std::exp(-d * d * inv2Sig2);
    }
    const double normTerm =
        (normWeight > 1e-6) ? 60.0 * (normScore / normWeight) : 0.0;

    // B) 층별 동축 원 적합 점수 (부분 잔존 경부 호 포함)
    constexpr int kBands = 10;
    constexpr int kSectors = 24;
    const double b0 = zMin + 0.10 * H;
    const double b1 = zMin + 0.92 * H;
    const double bw = (b1 - b0) / double(kBands);
    if (bw <= 0.5)
      return -1e9;

    const double c0x = C0.x * e1.x + C0.y * e1.y + C0.z * e1.z;
    const double c0y = C0.x * e2.x + C0.y * e2.y + C0.z * e2.z;

    struct BandPt {
      double x, y;
    };
    std::array<std::vector<BandPt>, kBands> bins;
    for (const auto &p : pts) {
      const double z = p.x * u.x + p.y * u.y + p.z * u.z;
      if (z < b0 || z >= b1)
        continue;
      const int bi = std::clamp(int((z - b0) / bw), 0, kBands - 1);
      bins[size_t(bi)].push_back(
          {p.x * e1.x + p.y * e1.y + p.z * e1.z,
           p.x * e2.x + p.y * e2.y + p.z * e2.z});
    }

    int okCount = 0;
    double sumRelErr = 0.0;
    double sumR = 0.0;
    std::vector<KA::Point2> centers;
    centers.reserve(kBands);

    for (int bi = 0; bi < kBands; ++bi) {
      const auto &bp = bins[size_t(bi)];
      if (bp.size() < 10)
        continue;

      std::array<double, kSectors> bestR2{};
      bestR2.fill(-1.0);
      std::array<KA::Point2, kSectors> bestPt{};
      for (const auto &q : bp) {
        const double dx = q.x - c0x, dy = q.y - c0y;
        const double r2 = dx * dx + dy * dy;
        double ang = std::atan2(dy, dx);
        if (ang < 0.0)
          ang += 2.0 * std::numbers::pi;
        const int si = std::clamp(
            int(ang / (2.0 * std::numbers::pi) * kSectors), 0, kSectors - 1);
        if (r2 > bestR2[size_t(si)]) {
          bestR2[size_t(si)] = r2;
          bestPt[size_t(si)] = {q.x, q.y};
        }
      }
      std::vector<KA::Point2> ring;
      ring.reserve(kSectors);
      for (int si = 0; si < kSectors; ++si) {
        if (bestR2[size_t(si)] > 0.0)
          ring.push_back(bestPt[size_t(si)]);
      }
      if (ring.size() < 4)
        continue;
      const KA::Circle2D c = KA::FitCircleTaubin(ring);
      if (!c.ok || c.r < 5.0 || c.r > H * 2.5)
        continue;
      const double relErr = c.rmse / c.r;
      const double cOff = std::hypot(c.xc - c0x, c.yc - c0y) / c.r;
      if (relErr > 0.08 || (ring.size() < 12 && cOff > 0.20))
        continue;
      ++okCount;
      sumRelErr += relErr;
      sumR += c.r;
      centers.push_back({c.xc, c.yc});
    }
    if (okCount < 4)
      return normTerm - 50.0;
    const double meanR = sumR / double(okCount);
    const double meanErr = sumRelErr / double(okCount);
    double mx = 0.0, my = 0.0;
    for (const auto &c : centers) {
      mx += c.x;
      my += c.y;
    }
    mx /= double(centers.size());
    my /= double(centers.size());
    double cVar = 0.0;
    for (const auto &c : centers)
      cVar += std::hypot(c.x - mx, c.y - my);
    cVar /= (double(centers.size()) * std::max(10.0, meanR));

    return normTerm - 20.0 * meanErr - 15.0 * cVar;
  };

  KA::Point3 bestU{0, 0, 1};
  double bestScore = scoreAxis(bestU);

  // 상반구 피보나치 격자 128방향 탐색
  constexpr int kDirs = 128;
  const double golden = std::numbers::pi * (3.0 - std::sqrt(5.0));
  for (int i = 0; i < kDirs; ++i) {
    const double z = 1.0 - (double(i) + 0.5) / double(kDirs);
    const double r = std::sqrt(std::max(0.0, 1.0 - z * z));
    const double phi = golden * double(i);
    const KA::Point3 cand{r * std::cos(phi), r * std::sin(phi), z};
    const double sc = scoreAxis(cand);
    if (sc > bestScore) {
      bestScore = sc;
      bestU = cand;
    }
  }

  // 국소 3단계 정밀 탐색 (±10° -> ±3° -> ±0.8°)
  for (double stepDeg : {3.5, 1.0, 0.25}) {
    KA::Point3 e1, e2;
    makeBasis(bestU, e1, e2);
    KA::Point3 stageBest = bestU;
    for (int ix = -3; ix <= 3; ++ix) {
      for (int iy = -3; iy <= 3; ++iy) {
        if (ix == 0 && iy == 0)
          continue;
        const double ax = double(ix) * stepDeg * kDeg;
        const double ay = double(iy) * stepDeg * kDeg;
        KA::Point3 cand{bestU.x + ax * e1.x + ay * e2.x,
                        bestU.y + ax * e1.y + ay * e2.y,
                        bestU.z + ax * e1.z + ay * e2.z};
        const double len =
            std::sqrt(cand.x * cand.x + cand.y * cand.y + cand.z * cand.z);
        if (len < 1e-12)
          continue;
        cand = {cand.x / len, cand.y / len, cand.z / len};
        const double sc = scoreAxis(cand);
        if (sc > bestScore) {
          bestScore = sc;
          stageBest = cand;
        }
      }
    }
    bestU = stageBest;
  }

  if (bestScore < -1e8)
    return false;

  // 상하(+bestU vs -bestU) 판별:
  // 회전축 중심 C0 투영점(c0x, c0y)으로부터의 반경을 비교한다.
  // 토기 저부(바닥)는 중심축 근처(r ≈ 0)까지 메워져 있어 하위 10% 반경이 작고,
  // 구연부(입구)는 가운데가 비어 있는 개구부이므로 하위 10% 반경이 크다.
  KA::Point3 e1, e2;
  makeBasis(bestU, e1, e2);
  const double c0x = C0.x * e1.x + C0.y * e1.y + C0.z * e1.z;
  const double c0y = C0.x * e2.x + C0.y * e2.y + C0.z * e2.z;
  double zMin = 1e18, zMax = -1e18;
  for (const auto &p : pts) {
    const double z = p.x * bestU.x + p.y * bestU.y + p.z * bestU.z;
    zMin = std::min(zMin, z);
    zMax = std::max(zMax, z);
  }
  const double H = zMax - zMin;
  std::vector<double> rTop, rBot;
  for (const auto &p : pts) {
    const double x = p.x * e1.x + p.y * e1.y + p.z * e1.z;
    const double y = p.x * e2.x + p.y * e2.y + p.z * e2.z;
    const double z = p.x * bestU.x + p.y * bestU.y + p.z * bestU.z;
    const double r = std::hypot(x - c0x, y - c0y);
    if (z >= zMax - 0.14 * H)
      rTop.push_back(r);
    if (z <= zMin + 0.14 * H)
      rBot.push_back(r);
  }
  if (rTop.size() >= 6 && rBot.size() >= 6) {
    std::sort(rTop.begin(), rTop.end());
    std::sort(rBot.begin(), rBot.end());
    const double qTop = rTop[rTop.size() / 10];
    const double qBot = rBot[rBot.size() / 10];
    if (qTop < qBot) {
      bestU = {-bestU.x, -bestU.y, -bestU.z};
    }
  }
  outUpAxis = bestU;
  return true;
}

void translate(KA::MeshData &m, double dx, double dy, double dz) {
  for (auto &p : m.vertices) {
    p.x += dx;
    p.y += dy;
    p.z += dz;
  }
}

// 바운딩 박스만 다시 잰다 (K-Arch computeBbox 와 같은 규칙). 강체 변환은 평균
// 변 길이·경계 유무를 바꾸지 않으므로 FillMeshStats(면 수에 비례하는 변 표)를
// 다시 돌리지 않는다.
void updateBbox(KA::MeshData &m) {
  if (m.vertices.empty()) {
    m.bboxMin = KA::Point3{};
    m.bboxMax = KA::Point3{};
    return;
  }
  KA::Point3 lo = m.vertices.front(), hi = lo;
  for (const auto &p : m.vertices) {
    lo.x = std::min(lo.x, p.x);
    lo.y = std::min(lo.y, p.y);
    lo.z = std::min(lo.z, p.z);
    hi.x = std::max(hi.x, p.x);
    hi.y = std::max(hi.y, p.y);
    hi.z = std::max(hi.z, p.z);
  }
  m.bboxMin = lo;
  m.bboxMax = hi;
}

// 각도 deg 의 cos·sin. 90° 정수배는 정확히 {−1, 0, 1} (반복 회전 누적 오차
// 방지)
void cosSinDeg(double deg, double &c, double &s) {
  const double q = deg / 90.0, k = std::round(q);
  if (std::fabs(q - k) < 1e-12) {
    static constexpr int kCos[4] = {1, 0, -1, 0};
    static constexpr int kSin[4] = {0, 1, 0, -1};
    const int i = int(std::fmod(std::fmod(k, 4.0) + 4.0, 4.0));
    c = kCos[i];
    s = kSin[i];
    return;
  }
  c = std::cos(deg * kDeg);
  s = std::sin(deg * kDeg);
}

constexpr double kGroundTolMm = 0.3; // 디자인 v2 접지 판정 권고값

std::vector<double> zBand(const KA::MeshData &m, int n) {
  const double z0 = m.bboxMin.z + 0.12 * (m.bboxMax.z - m.bboxMin.z);
  const double z1 = m.bboxMin.z + 0.88 * (m.bboxMax.z - m.bboxMin.z);
  std::vector<double> zs;
  for (int i = 0; i < n; ++i)
    zs.push_back(z0 + (z1 - z0) * i / double(n - 1));
  return zs;
}

// 도판 좌/우 부호 s(+1 / −1)를 x 에 곱한다 — ×(−1) 은 정확한 부호 반전(오차 0)
std::vector<KA::Point2> signedX(std::vector<KA::Point2> v, double s) {
  for (auto &p : v)
    p.x *= s;
  return v;
}

std::vector<std::vector<KA::Point2>>
signedX(std::vector<std::vector<KA::Point2>> polys, double s) {
  for (auto &v : polys)
    for (auto &p : v)
      p.x *= s;
  return polys;
}

double perpDist(const KA::Point2 &p, const KA::Point2 &a, const KA::Point2 &b) {
  const double dx = b.x - a.x, dy = b.y - a.y, L = std::hypot(dx, dy);
  if (L < 1e-12)
    return std::hypot(p.x - a.x, p.y - a.y);
  return std::fabs(dy * p.x - dx * p.y + b.x * a.y - b.y * a.x) / L;
}

void dpRec(const std::vector<KA::Point2> &p, size_t i0, size_t i1, double tol,
           std::vector<char> &keep) {
  if (i1 <= i0 + 1)
    return;
  double dmax = -1;
  size_t im = i0;
  for (size_t i = i0 + 1; i < i1; ++i) {
    const double d = perpDist(p[i], p[i0], p[i1]);
    if (d > dmax)
      dmax = d, im = i;
  }
  if (dmax > tol) {
    keep[im] = 1;
    dpRec(p, i0, im, tol, keep);
    dpRec(p, im, i1, tol, keep);
  }
}

// 점-선분 거리, 점-꺾은선 최단 거리(닫힘이면 끝→처음 변 포함, l 비면 0)
double segDist(const KA::Point2 &p, const KA::Point2 &a, const KA::Point2 &b) {
  const double dx = b.x - a.x, dy = b.y - a.y, L2 = dx * dx + dy * dy;
  const double t =
      L2 > 0 ? std::clamp(((p.x - a.x) * dx + (p.y - a.y) * dy) / L2, 0.0, 1.0)
             : 0.0;
  return std::hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy));
}

double lineDist(const KA::Point2 &p, const std::vector<KA::Point2> &l,
                bool closed) {
  if (l.empty())
    return 0.0;
  double d = std::hypot(p.x - l[0].x, p.y - l[0].y);
  const size_t n = l.size();
  for (size_t i = 0, s = closed ? n : n - 1; i < s && n > 1; ++i)
    d = std::min(d, segDist(p, l[i], l[(i + 1) % n]));
  return d;
}

double dev2(const std::vector<KA::Point2> &raw,
            const std::vector<KA::Point2> &drawn, bool closed) {
  double d = 0.0;
  for (const auto &p : drawn)
    d = std::max(d, lineDist(p, raw, closed));
  for (const auto &p : raw)
    d = std::max(d, lineDist(p, drawn, closed));
  return d;
}

// 막과 수직인 실루엣의 외면 반쪽(디자인 v2 「도면 선 규약」 외곽선 = 외면
// 반쪽의 윤곽): 높이 칸 zk = z0 + (k + 0.5)·bin (k = 0, 1, …, zk <= z1 인 것
// 모두, z0·z1 = m.bboxMin.z·bboxMax.z) 마다 삼각형 ∩ 수평면 교점 중 x <= 0 인
// 것의 최소 x. 맞는 점이 없는 칸은 비운다 — 복사·보간·평활 없음(실측 그대로).
// 면마다 그 면의 z 범위에 걸치는 칸만 돈다. 교점 규칙은 K-Arch
// addTriZHits(Silhouette.cpp:171-189)와 같다.
std::vector<KA::Point2> halfOrthoSilhouette(const KA::MeshData &m, double bin) {
  std::vector<KA::Point2> out;
  if (m.vertices.empty() || m.faces.empty() || bin <= 0)
    return out;
  const double z0 = m.bboxMin.z, z1 = m.bboxMax.z;
  long n = 0;
  while (z0 + (double(n) + 0.5) * bin <= z1)
    ++n;
  constexpr double kInf = std::numeric_limits<double>::infinity();

  const size_t totalFaces = m.faces.size();
  const unsigned int nThreads =
      (totalFaces > 20000) ? std::max(1u, std::thread::hardware_concurrency())
                           : 1u;

  std::vector<std::vector<double>> localBest(
      nThreads, std::vector<double>(size_t(n), kInf));
  std::vector<std::thread> workers;
  workers.reserve(nThreads);

  for (unsigned int t = 0; t < nThreads; ++t) {
    workers.emplace_back([&, t]() {
      const size_t f0 = t * totalFaces / nThreads;
      const size_t f1 = (t + 1) * totalFaces / nThreads;
      auto &best = localBest[t];
      for (size_t fi = f0; fi < f1; ++fi) {
        const auto &f = m.faces[fi];
        if (f[0] < 0 || f[1] < 0 || f[2] < 0)
          continue;
        const KA::Point3 p[3] = {m.vertices[size_t(f[0])],
                                 m.vertices[size_t(f[1])],
                                 m.vertices[size_t(f[2])]};
        const double lo = std::min({p[0].z, p[1].z, p[2].z}),
                     hi = std::max({p[0].z, p[1].z, p[2].z});
        const long k0 =
            std::max(0L, long(std::floor((lo - z0) / bin - 0.5)) - 1);
        const long k1 =
            std::min(n - 1, long(std::ceil((hi - z0) / bin - 0.5)) + 1);
        for (long k = k0; k <= k1; ++k) {
          const double zk = z0 + (double(k) + 0.5) * bin;
          double &mn = best[size_t(k)];
          for (int e = 0; e < 3; ++e) {
            const auto &u = p[e], &v = p[(e + 1) % 3];
            if (std::abs(u.z - zk) <= 1e-12 && u.x <= 0.0)
              mn = std::min(mn, u.x);
            const double dz = v.z - u.z;
            if (std::abs(dz) < 1e-15)
              continue;
            const double param = (zk - u.z) / dz;
            if (param > 0.0 && param <= 1.0) {
              const double x = u.x + param * (v.x - u.x);
              if (x <= 0.0)
                mn = std::min(mn, x);
            }
          }
        }
      }
    });
  }
  for (auto &w : workers)
    w.join();

  std::vector<double> best(size_t(n), kInf);
  for (long k = 0; k < n; ++k) {
    for (unsigned int t = 0; t < nThreads; ++t) {
      best[size_t(k)] = std::min(best[size_t(k)], localBest[t][size_t(k)]);
    }
  }

  for (long k = 0; k < n; ++k)
    if (best[size_t(k)] < kInf)
      out.push_back({best[size_t(k)], z0 + (double(k) + 0.5) * bin});
  return out;
}

// 합성 토기 정점에 자연스러운 Artec 컬러(소성 토기 색조 + 어깨 침선/타날 흔적)
// 부여
std::vector<QRgb> makeSyntheticArtecColors(const KA::MeshData &m) {
  std::vector<QRgb> cols(m.vertices.size());
  double zmin = 1e18, zmax = -1e18;
  for (const auto &p : m.vertices) {
    zmin = std::min(zmin, p.z);
    zmax = std::max(zmax, p.z);
  }
  const double h = std::max(1.0, zmax - zmin);
  for (size_t i = 0; i < m.vertices.size(); ++i) {
    const auto &p = m.vertices[i];
    const double t = (p.z - zmin) / h;
    const double ang = std::atan2(p.y, p.x);
    const double fire = 0.92 +
                        0.08 * std::sin(t * 5.0 + 0.7 * std::cos(2 * ang)) -
                        0.05 * std::exp(-std::pow((t - 0.68) / 0.06, 2.0));
    const int r = std::clamp(int(196 * fire), 60, 240);
    const int g = std::clamp(int(166 * fire), 50, 225);
    const int b = std::clamp(int(134 * fire), 40, 200);
    cols[i] = qRgb(r, g, b);
  }
  return cols;
}

// 높이 zs[k](오름차순)마다 그 높이에 걸치는 면 번호 — 면 순서 그대로. 걸침 = 세
// 꼭짓점 z 범위 ±1e-6 mm 가 zs[k] 를 품음(K-Arch Slice 가 그 높이에서 선분을
// 만들 수 있는 면은 모두 들어가고, 남는 면은 선분을 만들지 않는다).
std::vector<std::vector<int>> zBandFaces(const KA::MeshData &m,
                                         const std::vector<double> &zs) {
  std::vector<std::vector<int>> out(zs.size());
  const long nv = long(m.vertices.size());
  const size_t totalFaces = m.faces.size();
  const unsigned int nThreads =
      (totalFaces > 20000 && zs.size() > 8)
          ? std::max(1u, std::thread::hardware_concurrency())
          : 1u;

  if (nThreads <= 1) {
    for (size_t f = 0; f < totalFaces; ++f) {
      const auto &F = m.faces[f];
      if (F[0] < 0 || F[1] < 0 || F[2] < 0 || F[0] >= nv || F[1] >= nv ||
          F[2] >= nv)
        continue;
      const double a = m.vertices[size_t(F[0])].z,
                   b = m.vertices[size_t(F[1])].z,
                   c = m.vertices[size_t(F[2])].z;
      const auto lo =
          std::lower_bound(zs.begin(), zs.end(), std::min({a, b, c}) - 1e-6);
      const auto hi =
          std::upper_bound(zs.begin(), zs.end(), std::max({a, b, c}) + 1e-6);
      for (auto it = lo; it < hi; ++it)
        out[size_t(it - zs.begin())].push_back(int(f));
    }
    return out;
  }

  std::vector<std::vector<std::vector<int>>> localOut(
      nThreads, std::vector<std::vector<int>>(zs.size()));
  std::vector<std::thread> workers;
  workers.reserve(nThreads);

  for (unsigned int t = 0; t < nThreads; ++t) {
    workers.emplace_back([&, t]() {
      const size_t f0 = t * totalFaces / nThreads;
      const size_t f1 = (t + 1) * totalFaces / nThreads;
      auto &tOut = localOut[t];
      for (size_t f = f0; f < f1; ++f) {
        const auto &F = m.faces[f];
        if (F[0] < 0 || F[1] < 0 || F[2] < 0 || F[0] >= nv || F[1] >= nv ||
            F[2] >= nv)
          continue;
        const double a = m.vertices[size_t(F[0])].z,
                     b = m.vertices[size_t(F[1])].z,
                     c = m.vertices[size_t(F[2])].z;
        const auto lo =
            std::lower_bound(zs.begin(), zs.end(), std::min({a, b, c}) - 1e-6);
        const auto hi =
            std::upper_bound(zs.begin(), zs.end(), std::max({a, b, c}) + 1e-6);
        for (auto it = lo; it < hi; ++it)
          tOut[size_t(it - zs.begin())].push_back(int(f));
      }
    });
  }
  for (auto &w : workers)
    w.join();

  for (size_t i = 0; i < zs.size(); ++i) {
    size_t total = 0;
    for (unsigned int t = 0; t < nThreads; ++t) {
      total += localOut[t][i].size();
    }
    out[i].reserve(total);
    for (unsigned int t = 0; t < nThreads; ++t) {
      out[i].insert(out[i].end(), localOut[t][i].begin(), localOut[t][i].end());
    }
  }
  return out;
}

// 면 목록만의 작은 메시. 원본 바운딩 박스 두 모서리를 면 없이 넣어 K-Arch Slice
// 의 이음 허용치(1e-6 × 정점 바운딩 박스 대각선, MeshSlicer.cpp:103-106)를
// 원본과 같게 한다 → 원본 전체를 자른 것과 같은 고리.
KA::MeshData bandMesh(const KA::MeshData &m, const std::vector<int> &faces) {
  KA::MeshData s;
  s.vertices = {m.bboxMin, m.bboxMax};
  std::unordered_map<int, int> idx;
  for (int f : faces) {
    std::array<int, 3> g{};
    for (size_t k = 0; k < 3; ++k) {
      const int v = m.faces[size_t(f)][k];
      const auto [it, ins] = idx.try_emplace(v, int(s.vertices.size()));
      if (ins)
        s.vertices.push_back(m.vertices[size_t(v)]);
      g[k] = it->second;
    }
    s.faces.push_back(g);
  }
  return s;
}

// 높이 z 수평 단면의 기벽 두께 = 평균 반경이 가장 큰 고리 − 두 번째 고리,
// 고리가 하나뿐이면 −1
double wallBySlice(const KA::MeshData &m, double z) {
  const auto loops = KA::Slice(m, KA::Plane{{0, 0, 1}, -z});
  std::vector<std::pair<double, std::vector<double>>> rs;
  for (const auto &l : loops) {
    if (l.pts.size() < 3)
      continue;
    std::vector<double> r;
    r.reserve(l.pts.size());
    double s = 0;
    for (const auto &p : l.pts)
      r.push_back(std::hypot(p.x, p.y)), s += r.back();
    rs.push_back({s / double(r.size()), std::move(r)});
  }
  if (rs.empty())
    return -1.0;
  std::sort(rs.begin(), rs.end(),
            [](auto &a, auto &b) { return a.first > b.first; });
  if (rs.size() >= 2)
    return rs[0].first - rs[1].first;
  // 단일 열린 쉘(내벽 미스캔)인 경우 -1 반환하여 추정 기벽두께와 구분
  return -1.0;
}

} // namespace

std::vector<KA::Point2> simplifyDP(const std::vector<KA::Point2> &pts,
                                   double tol, bool closed,
                                   std::vector<size_t> *kept) {
  if (kept)
    kept->clear();
  if (pts.size() < 3 || tol <= 0) {
    if (kept)
      for (size_t i = 0; i < pts.size(); ++i)
        kept->push_back(i);
    return pts;
  }
  std::vector<KA::Point2> p = pts;
  if (closed)
    p.push_back(pts.front());
  std::vector<char> keep(p.size(), 0);
  keep.front() = keep.back() = 1;
  if (closed && p.size() > 3) {
    size_t far = 0;
    double dm = -1;
    for (size_t i = 0; i < p.size(); ++i) {
      const double d = std::hypot(p[i].x - p[0].x, p[i].y - p[0].y);
      if (d > dm)
        dm = d, far = i;
    }
    keep[far] = 1;
    dpRec(p, 0, far, tol, keep);
    dpRec(p, far, p.size() - 1, tol, keep);
  } else {
    dpRec(p, 0, p.size() - 1, tol, keep);
  }
  std::vector<KA::Point2> out;
  for (size_t i = 0; i < p.size(); ++i)
    if (keep[i]) {
      out.push_back(p[i]);
      if (kept)
        kept->push_back(i);
    }
  if (closed && out.size() > 1) {
    out.pop_back();
    if (kept)
      kept->pop_back();
  }
  return out;
}

std::vector<KA::Point2> fitCurve(const std::vector<KA::Point2> &raw, double tol,
                                 bool closed, bool smooth, int sub) {
  std::vector<size_t> kept;
  const auto c = simplifyDP(raw, tol, closed, &kept);
  const long m = long(c.size());
  if (!smooth || m < 3 || sub < 2)
    return c; // 곡선 끔 = DP 결과 그대로
  // 곡선 점의 허용 범위: raw 의 높이 범위, raw 가 축 한쪽이면 그쪽(옆 방향
  // 극값은 막지 않음)
  constexpr double kInf = std::numeric_limits<double>::infinity();
  double x0 = kInf, x1 = -kInf, y0 = kInf, y1 = -kInf;
  for (const auto &p : raw) {
    x0 = std::min(x0, p.x), x1 = std::max(x1, p.x);
    y0 = std::min(y0, p.y), y1 = std::max(y1, p.y);
  }
  const double xLo = x0 >= 0 ? 0.0 : -kInf, xHi = x1 <= 0 ? 0.0 : kInf;
  auto at = [&](long i) -> const KA::Point2 & {
    return c[size_t(closed ? (i % m + m) % m : std::clamp(i, 0L, m - 1))];
  };
  const size_t n = raw.size();
  const long segs = closed ? m : m - 1;
  std::vector<KA::Point2> out, chain, piece;
  for (long i = 0; i < segs; ++i) {
    const auto &p0 = at(i - 1), &p1 = at(i), &p2 = at(i + 1), &p3 = at(i + 2);
    // 이 구간의 원 점 (닫힘 마지막 구간은 처음 점으로 되감기)
    const size_t e = i + 1 < m ? kept[size_t(i + 1)] : n;
    chain.clear();
    for (size_t k = kept[size_t(i)]; k <= e; ++k)
      chain.push_back(raw[k % n]);
    piece.assign(1, p1);
    bool ok = true;
    for (int k = 1; k < sub; ++k) {
      const double t = double(k) / sub, t2 = t * t, t3 = t2 * t;
      auto f = [&](double a, double b, double cc, double d) {
        return 0.5 *
               (2 * b + (-a + cc) * t + (2 * a - 5 * b + 4 * cc - d) * t2 +
                (-a + 3 * b - 3 * cc + d) * t3);
      };
      const KA::Point2 q{f(p0.x, p1.x, p2.x, p3.x), f(p0.y, p1.y, p2.y, p3.y)};
      ok = ok && q.x >= xLo && q.x <= xHi && q.y >= y0 && q.y <= y1;
      piece.push_back(q);
    }
    piece.push_back(p2);
    for (size_t k = 1; ok && k + 1 < piece.size(); ++k)
      ok = lineDist(piece[k], chain, false) <= tol;
    for (size_t k = 0; ok && k < chain.size(); ++k)
      ok = lineDist(chain[k], piece, false) <= tol;
    // p1, 곡선이 허용치 안이면 그 사이 점들도 (아니면 DP 직선)
    out.insert(out.end(), piece.begin(),
               ok ? piece.end() - 1 : piece.begin() + 1);
  }
  if (!closed)
    out.push_back(c.back());
  return out;
}

void ArtifactDoc::loadMesh(KA::MeshData m, std::vector<QRgb> colors,
                           const QString &name) {
  base_ = std::move(m);
  KA::FillMeshStats(base_);
  bvhMaster_.clear();
  baseColors_ = std::move(colors);
  name_ = name;
  frontAngle_ = 0.0;
  rawVertices_ = base_.vertices; // 처음 자세로 되돌릴 사본 (면은 공유)
  buildLodMesh(base_, baseColors_, lodBase_, lodColors_, 35000);
  rawLod_ = lodBase_;
  rawLodColors_ = lodColors_;
  pendingAligned_ = false;
  pendingNote_.clear();
  pendingRmse_ = 0.0;
  hasSavedWork_ = false;
  savedSlots_.clear();
  savedHasRubbingSpot_ = false;
  rubbingPreset_ = RubbingPreset::Standard;
  rubbingProjMode_ = RubbingProjMode::Cylindrical;
  poseConfirmed_ = false;
  unconfirm();
  centered_ = false;
  computeRimCircleFit();
}

bool ArtifactDoc::loadFile(const QString &path, QString *err) {
  unitConfirmed_ = true;
  MeshReadResult r = readMeshAny(path);
  if (!r.ok) {
    if (err)
      *err = r.message;
    return false;
  }
  scannerProfile_ = r.scannerProfile;
  unitNote_ = r.unitNote;
  removedNoiseFaces_ = r.removedNoiseFaces;
  path_ = path;
  // 들어온 자세 그대로 — 바닥은 사람이 ① 바닥 정하기에서 확정한다
  loadMesh(std::move(r.mesh), std::move(r.vertexColors),
           QFileInfo(path).completeBaseName());
  return true;
}

void ArtifactDoc::loadSample(bool confirm) {
  KA::Testing::SyntheticParams sp;
  sp.radialSeg = 160;
  sp.heightSeg = 120;
  sp.capTop = false; // 열린 구연부
  sp.capBottom = true;
  sp.tiltDeg = 7.0; // 일부러 기울여 정위치 과정을 보여준다
  sp.translation = {12.0, -5.0, 3.0};
  sp.noiseSigma = 0.05;
  const auto vessel =
      KA::Testing::MakeTorusProfileVessel(82.6, 71.0, 184.5, 6.8, sp);
  KA::MeshData m = vessel.toMesh();
  KA::FillMeshStats(m);
  std::vector<QRgb> cols = makeSyntheticArtecColors(m);
  scannerProfile_ = QStringLiteral("Artec Space Spider 급 (내장 샘플)");
  unitNote_.clear();
  removedNoiseFaces_ = 0;
  path_.clear();
  loadMesh(std::move(m), std::move(cols), QStringLiteral("예제_합성토기_01호"));
  if (confirm) {
    autoAlign();
    confirmPose();
  }
}

void ArtifactDoc::loadArtecBlindSample(bool confirm) {
  // 좁은 아가리 항아리에서 광학 스캐너(Artec Space Spider / Eva)가 구연부
  // 안쪽(상부 35%)만 찍고 동체부·저부 내벽은 찍지 못해 외벽 단일 쉘(또는 홀필
  // 마개)로 남은 실제 현장 상황을 재현
  KA::Testing::SyntheticParams sp;
  sp.radialSeg = 160;
  sp.heightSeg = 120;
  sp.capTop = false;
  sp.capBottom = false;
  sp.tiltDeg = 4.5;
  sp.translation = {-6.0, 4.0, 1.5};
  sp.noiseSigma = 0.04;
  const auto vessel =
      KA::Testing::MakeTorusProfileVessel(84.0, 62.0, 190.0, 7.0, sp);
  KA::MeshData full = vessel.toMesh();
  KA::FillMeshStats(full);
  // 내벽 정점(innerBase 이상 인덱스) 중 높이 하위 65% 구간의 삼각형을 제거하여
  // 광학 스캐너 음영(내벽 미스캔) 상태 재현
  const int rings = sp.heightSeg + 1;
  const int innerBase = rings * sp.radialSeg;
  const int cutRing = int(sp.heightSeg * 0.65);
  std::vector<std::array<int, 3>> kept;
  kept.reserve(full.faces.size());
  for (const auto &f : full.faces) {
    const bool isInner =
        f[0] >= innerBase || f[1] >= innerBase || f[2] >= innerBase;
    if (isInner) {
      const int r0 = (f[0] - innerBase) / sp.radialSeg;
      const int r1 = (f[1] - innerBase) / sp.radialSeg;
      const int r2 = (f[2] - innerBase) / sp.radialSeg;
      if (std::min({r0, r1, r2}) < cutRing)
        continue; // 동체부·저부 내벽 미스캔
    }
    kept.push_back(f);
  }
  full.faces = std::move(kept);
  KA::FillMeshStats(full);
  std::vector<QRgb> cols = makeSyntheticArtecColors(full);
  scannerProfile_ =
      QStringLiteral("Artec Space Spider (내벽 미스캔 보정 샘플)");
  unitNote_.clear();
  removedNoiseFaces_ = 0;
  path_.clear();
  inferInnerWall_ = true;
  loadMesh(std::move(full), std::move(cols),
           QStringLiteral("Artec_내벽미스캔_단경호_02호"));
  if (confirm) {
    autoAlign();
    confirmPose();
  }
}

void ArtifactDoc::rescaleUnits(double factor) {
  if (base_.vertices.empty() || factor <= 0 || std::fabs(factor - 1.0) < 1e-9)
    return;
  auto scale = [factor](std::vector<KA::Point3> &vs) {
    for (auto &p : vs) {
      p.x *= factor;
      p.y *= factor;
      p.z *= factor;
    }
  };
  // 원 자세 사본과 현재 자세에 같은 배율
  scale(base_.vertices);
  scale(rawVertices_);
  scale(lodBase_.vertices);
  scale(rawLod_.vertices);
  KA::FillMeshStats(base_);
  KA::FillMeshStats(lodBase_);
  KA::FillMeshStats(rawLod_);
  unitNote_ = QStringLiteral("단위 배율 ×%1 적용됨").arg(factor, 0, 'g', 4);
  unconfirm();
  centered_ = false;
}

void ArtifactDoc::setInferInnerWall(bool on) {
  if (inferInnerWall_ == on)
    return;
  inferInnerWall_ = on;
  if (isLoaded() && poseConfirmed_) {
    buildSectionLoops();
  }
}

void ArtifactDoc::setEstimatedWallMm(double mm) {
  estimatedWallMm_ = std::clamp(mm, 1.5, 40.0);
  if (isLoaded() && poseConfirmed_ && inferInnerWall_) {
    buildSectionLoops();
  }
}

// ---- ① 바닥 정하기 ----------------------------------------------------------
void ArtifactDoc::rotateAbout(const double (&R)[3][3], const KA::Point3 &c) {
  auto rot = [&](KA::MeshData &m) {
    for (auto &p : m.vertices) {
      const double dx = p.x - c.x, dy = p.y - c.y, dz = p.z - c.z;
      p = KA::Point3{c.x + (R[0][0] * dx + R[0][1] * dy + R[0][2] * dz),
                     c.y + (R[1][0] * dx + R[1][1] * dy + R[1][2] * dz),
                     c.z + (R[2][0] * dx + R[2][1] * dy + R[2][2] * dz)};
    }
    updateBbox(m);
  };
  rot(base_);
  rot(lodBase_);
}

void ArtifactDoc::shiftBoth(double dx, double dy, double dz) {
  translate(base_, dx, dy, dz);
  translate(lodBase_, dx, dy, dz);
  updateBbox(base_);
  updateBbox(lodBase_);
}

void ArtifactDoc::rotatePose(PoseAxis a, double deg) {
  if (!isLoaded())
    return;
  double c = 1.0, s = 0.0;
  cosSinDeg(deg, c, s);
  // 고정 월드축 기준 오른손 법칙 (외재적 회전)
  double R[3][3] = {{1, 0, 0}, {0, 1, 0}, {0, 0, 1}};
  switch (a) {
  case PoseAxis::X:
    R[1][1] = c, R[1][2] = -s, R[2][1] = s, R[2][2] = c;
    break;
  case PoseAxis::Y:
    R[0][0] = c, R[0][2] = s, R[2][0] = -s, R[2][2] = c;
    break;
  case PoseAxis::Z:
    R[0][0] = c, R[0][1] = -s, R[1][0] = s, R[1][1] = c;
    break;
  }
  // 회전 중심 = 현재 자세의 축 정렬 바운딩 박스 중심
  const KA::Point3 ctr{0.5 * (base_.bboxMin.x + base_.bboxMax.x),
                       0.5 * (base_.bboxMin.y + base_.bboxMax.y),
                       0.5 * (base_.bboxMin.z + base_.bboxMax.z)};
  rotateAbout(R, ctr);
  shiftBoth(0, 0, -base_.bboxMin.z); // 바닥에 내려놓기
  unconfirm();
  centered_ = false;
}

void ArtifactDoc::flipPose() { rotatePose(PoseAxis::X, 180.0); }

void ArtifactDoc::resetPose() {
  if (!isLoaded())
    return;
  base_.vertices = rawVertices_;
  updateBbox(base_);
  lodBase_ = rawLod_;
  lodColors_ = rawLodColors_;
  floorPickPts_.clear();
  hasSavedWork_ = false;
  savedSlots_.clear();
  savedHasRubbingSpot_ = false;
  poseConfirmed_ = false;
  unconfirm();
  centered_ = false;
}

void ArtifactDoc::dropToFloor() {
  if (!isLoaded())
    return;
  shiftBoth(0, 0, -base_.bboxMin.z);
  unconfirm();
  centered_ = false;
}

void ArtifactDoc::centerOnFloor() {
  if (!isLoaded())
    return;
  const double cx = 0.5 * (base_.bboxMin.x + base_.bboxMax.x);
  const double cy = 0.5 * (base_.bboxMin.y + base_.bboxMax.y);
  shiftBoth(-cx, -cy, -base_.bboxMin.z);
  unconfirm();
  centered_ = false;
}

void ArtifactDoc::alignNormalToFloor(const KA::Point3 &downNormal) {
  if (!isLoaded())
    return;
  const KA::Point3 up{-downNormal.x, -downNormal.y, -downNormal.z};
  const double len = std::sqrt(up.x * up.x + up.y * up.y + up.z * up.z);
  if (len < 1e-12)
    return;
  double R[3][3];
  buildRotationToPlusZ(up, R);
  const KA::Point3 ctr{0.5 * (base_.bboxMin.x + base_.bboxMax.x),
                       0.5 * (base_.bboxMin.y + base_.bboxMax.y),
                       0.5 * (base_.bboxMin.z + base_.bboxMax.z)};
  rotateAbout(R, ctr);
  centerOnFloor();
}

bool ArtifactDoc::alignSurfacePointToFloor(const KA::Point3 &worldPt,
                                           double radiusMm) {
  if (!isLoaded())
    return false;
  const KA::MeshData &m =
      (!lodBase_.faces.empty() && base_.faces.size() > 250000) ? lodBase_
                                                               : base_;
  if (m.faces.empty() || m.vertices.empty())
    return false;
  const KA::Point3 g = KA::MeshCentroid(m);
  const double r2Max = std::max(1.0, radiusMm) * std::max(1.0, radiusMm);

  KA::Point3 nSum{0, 0, 0};
  double bestDist2 = 1e30;
  KA::Point3 closestN{0, 0, -1};
  const int nv = int(m.vertices.size());

  for (const auto &f : m.faces) {
    if (f[0] < 0 || f[1] < 0 || f[2] < 0 || f[0] >= nv || f[1] >= nv ||
        f[2] >= nv)
      continue;
    const auto &p0 = m.vertices[size_t(f[0])];
    const auto &p1 = m.vertices[size_t(f[1])];
    const auto &p2 = m.vertices[size_t(f[2])];
    const KA::Point3 fc{(p0.x + p1.x + p2.x) / 3.0, (p0.y + p1.y + p2.y) / 3.0,
                        (p0.z + p1.z + p2.z) / 3.0};
    const double dx = fc.x - worldPt.x, dy = fc.y - worldPt.y,
                 dz = fc.z - worldPt.z;
    const double d2 = dx * dx + dy * dy + dz * dz;
    const double e1x = p1.x - p0.x, e1y = p1.y - p0.y, e1z = p1.z - p0.z;
    const double e2x = p2.x - p0.x, e2y = p2.y - p0.y, e2z = p2.z - p0.z;
    KA::Point3 fn{e1y * e2z - e1z * e2y, e1z * e2x - e1x * e2z,
                  e1x * e2y - e1y * e2x};
    const double fnLen = std::sqrt(fn.x * fn.x + fn.y * fn.y + fn.z * fn.z);
    if (fnLen < 1e-12)
      continue;
    // 바깥쪽(중심 g 에서 먼 쪽)을 향하도록 부호 정렬
    if (fn.x * (fc.x - g.x) + fn.y * (fc.y - g.y) + fn.z * (fc.z - g.z) < 0.0) {
      fn = {-fn.x, -fn.y, -fn.z};
    }
    if (d2 < bestDist2) {
      bestDist2 = d2;
      closestN = {fn.x / fnLen, fn.y / fnLen, fn.z / fnLen};
    }
    if (d2 <= r2Max) {
      nSum.x += fn.x;
      nSum.y += fn.y;
      nSum.z += fn.z;
    }
  }

  const double sumLen =
      std::sqrt(nSum.x * nSum.x + nSum.y * nSum.y + nSum.z * nSum.z);
  const KA::Point3 chosen = (sumLen > 1e-9)
                                ? KA::Point3{nSum.x / sumLen, nSum.y / sumLen,
                                             nSum.z / sumLen}
                                : closestN;
  alignNormalToFloor(chosen);
  return true;
}

void ArtifactDoc::clearFloorPickPoints() { floorPickPts_.clear(); }

int ArtifactDoc::addFloorPickPoint(const KA::Point3 &pt) {
  if (!isLoaded())
    return 0;
  if (floorPickPts_.size() >= 3)
    floorPickPts_.clear();
  floorPickPts_.push_back(pt);
  if (floorPickPts_.size() == 3) {
    const auto &p0 = floorPickPts_[0];
    const auto &p1 = floorPickPts_[1];
    const auto &p2 = floorPickPts_[2];
    const KA::Point3 u{p1.x - p0.x, p1.y - p0.y, p1.z - p0.z};
    const KA::Point3 v{p2.x - p0.x, p2.y - p0.y, p2.z - p0.z};
    KA::Point3 n{u.y * v.z - u.z * v.y, u.z * v.x - u.x * v.z,
                 u.x * v.y - u.y * v.x};
    const double len = std::sqrt(n.x * n.x + n.y * n.y + n.z * n.z);
    if (len > 1e-6) {
      const KA::Point3 g =
          KA::MeshCentroid(lodBase_.vertices.empty() ? base_ : lodBase_);
      const KA::Point3 pBar{(p0.x + p1.x + p2.x) / 3.0,
                            (p0.y + p1.y + p2.y) / 3.0,
                            (p0.z + p1.z + p2.z) / 3.0};
      // 찍은 3점 평면에서 유물 무게중심 g 로 향하는 쪽이 위(+Z), 반대쪽이 바닥(-Z)
      const double toCenter =
          n.x * (g.x - pBar.x) + n.y * (g.y - pBar.y) + n.z * (g.z - pBar.z);
      const KA::Point3 downNormal =
          (toCenter >= 0.0) ? KA::Point3{-n.x, -n.y, -n.z} : n;
      alignNormalToFloor(downNormal);
    }
  }
  return int(floorPickPts_.size());
}

bool ArtifactDoc::isGrounded() const {
  return isLoaded() && std::fabs(mesh_.bboxMin.z) <= kGroundTolMm;
}

QString ArtifactDoc::groundNote() const {
  if (!isLoaded())
    return {};
  if (isGrounded())
    return autoCentered() ? QStringLiteral("바닥판에 닿음 · 중심선 자동")
                          : QStringLiteral("바닥판에 닿음");
  const double z = heightAboveFloor();
  return z > 0 ? QStringLiteral("바닥판에서 %1 mm 위").arg(z, 0, 'f', 1)
               : QStringLiteral("바닥판 아래 %1 mm").arg(-z, 0, 'f', 1);
}

void ArtifactDoc::unconfirm() {
  if (poseConfirmed_) {
    hasSavedWork_ = true;
    savedFrontAngle_ = frontAngle_;
    savedSlots_ = slotList_;
    savedHasRubbingSpot_ = hasRubbingSpot_;
    if (hasRubbingSpot_)
      savedRubbingReg_ = rubbingRegion();
  }
  poseConfirmed_ = false;
  bvhMaster_.clear();
  slotList_.clear();
  recompute();
}

void ArtifactDoc::confirmPose() {
  if (!isLoaded())
    return;
  if (!unitConfirmed_)
    return; // Feature 13: 단위 미확정 시 확정 잠금
  if (centered_) {
    // 자동 세우기 직후: 중심 맞춤·접지가 이미 끝났다 (예전 한 번 누르기와 같은
    // 순서·같은 값)
    aligned_ = pendingAligned_;
    alignNote_ = pendingNote_;
    axisRmse_ = pendingRmse_;
  } else {
    // 사람이 세운 자세 그대로(회전 없음), 중심선(수평 외곽 원 중심 평균)을
    // 원점으로 — autoAlign 마지막 단계와 같은 규칙 (초고해상도 메시는 LOD
    // 프록시 활용)
    const auto &centerMesh =
        (base_.faces.size() > 250000 && !lodBase_.faces.empty()) ? lodBase_
                                                                 : base_;
    const auto fits =
        KA::FitHorizontalOuters(centerMesh, zBand(centerMesh, 12));
    const double bx = 0.5 * (base_.bboxMin.x + base_.bboxMax.x);
    const double by = 0.5 * (base_.bboxMin.y + base_.bboxMax.y);
    const double maxSpan = std::max(base_.bboxMax.x - base_.bboxMin.x,
                                    base_.bboxMax.y - base_.bboxMin.y);
    double sx = 0, sy = 0;
    int n = 0;
    for (const auto &f : fits) {
      if (f.circle.ok && f.circle.r <= 0.75 * maxSpan &&
          (f.circle.rmse / std::max(1.0, f.circle.r)) <= 0.05 &&
          std::hypot(f.circle.xc - bx, f.circle.yc - by) <= 0.25 * maxSpan) {
        sx += f.circle.xc, sy += f.circle.yc, ++n;
      }
    }
    if (n > 0)
      shiftBoth(-sx / n, -sy / n, 0);
    else
      shiftBoth(-bx, -by, 0);
    aligned_ = n > 0;
    alignNote_ =
        n > 0 ? QStringLiteral("바닥판 기준 (사람 확정) · 중심선 원 맞춤")
              : QStringLiteral("바닥판 기준 (사람 확정) · 중심선 미검출");
    axisRmse_ = -1.0;
    if (!unitNote_.isEmpty())
      alignNote_ += QStringLiteral(" · ") + unitNote_;
    if (removedNoiseFaces_ > 0)
      alignNote_ +=
          QStringLiteral(" · 부유노이즈 %1면 제거").arg(removedNoiseFaces_);
  }
  if (base_.bboxMin.z != 0.0)
    shiftBoth(0, 0, -base_.bboxMin.z); // 접지
  frontAngle_ = hasSavedWork_ ? savedFrontAngle_ : 0.0;
  poseConfirmed_ = true;
  // Feature 2 & 5: Build compact linear BVH on immutable base_ (MeshMaster)
  bvhMaster_.build(base_);
  // 미리보기(끄는 동안)·3D 보조창용 LOD: shiftBoth로 이미 같은 평행이동이
  // 적용되었으므로 비어있을 때만 새로 생성
  if (lodBase_.faces.empty()) {
    buildLodMesh(base_, baseColors_, lodBase_, lodColors_, 35000);
  }
  if (hasSavedWork_ && !savedSlots_.empty()) {
    slotList_ = savedSlots_;
    const double hMax = std::max(1.0, base_.bboxMax.z - base_.bboxMin.z);
    for (auto &s : slotList_) {
      if (s.kind == SlotKind::Horizontal)
        s.z = std::clamp(s.z, 0.0, hMax);
    }
  } else {
    resetSlots();
  }
  recompute();
  if (hasSavedWork_ && savedHasRubbingSpot_) {
    setRubbingRegion(savedRubbingReg_);
  } else {
    autoInitRubbingSpot();
  }
}

void ArtifactDoc::autoAlign() {
  if (!isLoaded())
    return;
  KA::MeshData m = base_;
  bool aligned = false;
  QString note;
  double rmse = 0.0;

  // 초고해상도 Artec 메시(25만 면 초과)는 경량 프록시로 회전축(PCA+ODR)을 즉시
  // 찾고 원본에 변환 적용
  KA::MeshData proxy;
  std::vector<QRgb> dummyCol;
  const bool useProxy = m.faces.size() > 250000;
  if (useProxy)
    buildLodMesh(m, {}, proxy, dummyCol, 50000);
  KA::MeshData &work = useProxy ? proxy : m;

  // 1) 열린 경계(구연부)가 있는 경우 구연부 PCA 로 입구를 위로,
  //    수밀(Watertight) 메시이거나 구연 경계를 못 찾는 경우
  //    3D 법선 교차 + 동축 원 대칭축 탐색(findCoarseVesselAxis)으로 입구를 위로 세운다.
  // 1) 대용량 실측 스캔(useProxy, 복셀 LOD는 얇은 능선에서 가짜 구멍이 생기므로 경계 검출 금지)
  //    또는 수밀(Watertight) 메시이거나 구연 경계를 못 찾는 경우:
  //    원본 메시(m)의 3D 법선-회전축 교차 + 동축 원 탐색(findCoarseVesselAxis)으로 입구를 위로 세운다.
  const KA::RimResult boundaryRim =
      (!useProxy && m.hasBoundary) ? KA::DetectRimBoundary(work)
                                   : KA::RimResult{};
  bool coarseAligned = false;
  KA::Point3 coarseCtr = KA::MeshCentroid(m);
  if (useProxy || !m.hasBoundary || boundaryRim.error != KA::RimError::Ok) {
    KA::Point3 upAxis{0, 0, 1};
    if (findCoarseVesselAxis(m, upAxis, &coarseCtr)) {
      coarseAligned = true;
      if (std::hypot(upAxis.x, upAxis.y) > 1e-3 || upAxis.z < 0.0) {
        double R[3][3];
        buildRotationToPlusZ(upAxis, R);
        applyMat3(work, R, coarseCtr);
        updateBbox(work);
        if (useProxy) {
          applyMat3(m, R, coarseCtr);
          updateBbox(m);
        }
      }
    }
  }

  const KA::RimResult rim =
      (useProxy || !m.hasBoundary || coarseAligned)
          ? KA::RimResult{}
          : KA::DetectRim(work, KA::RimMode::Auto);
  if (rim.error == KA::RimError::Ok) {
    const auto pca =
        KA::ComputePcaAlignment(rim.rimPoints, KA::MeshCentroid(work));
    if (pca.ok) {
      for (auto &p : work.vertices)
        p = pca.transform.apply(p);
      updateBbox(work);
      if (useProxy) {
        for (auto &p : m.vertices)
          p = pca.transform.apply(p);
        updateBbox(m);
      }
    }
  }
  // 2) 수평 외곽 원들로 회전 중심축 정밀화
  const auto outers24 = KA::FitHorizontalOuters(work, zBand(work, 24));
  int fullCleanCircles = 0;
  for (const auto &f : outers24) {
    if (f.circle.ok && f.circle.pointCount >= 16 &&
        (f.circle.rmse / std::max(1.0, f.circle.r)) <= 0.02) {
      ++fullCleanCircles;
    }
  }
  const KA::Axis3D ax = KA::OptimizeRotationAxis(outers24);
  const double workSpan = std::max(work.bboxMax.x - work.bboxMin.x,
                                   work.bboxMax.y - work.bboxMin.y);
  const double workBx = 0.5 * (work.bboxMin.x + work.bboxMax.x);
  const double workBy = 0.5 * (work.bboxMin.y + work.bboxMax.y);
  const bool odrSane =
      ax.ok &&
      (!coarseAligned ||
       (fullCleanCircles >= 18 && ax.rmse <= 0.8 &&
        std::fabs(ax.direction.z) >= 0.995 &&
        std::hypot(ax.origin.x - workBx, ax.origin.y - workBy) <=
            0.20 * workSpan));
  if (odrSane) {
    translate(work, -ax.origin.x, -ax.origin.y, -ax.origin.z);
    rotateDirToZ(work, ax.direction);
    updateBbox(work);
    if (useProxy) {
      translate(m, -ax.origin.x, -ax.origin.y, -ax.origin.z);
      rotateDirToZ(m, ax.direction);
      updateBbox(m);
    }
    const auto fits = KA::FitHorizontalOuters(work, zBand(work, 12));
    const double bx = 0.5 * (m.bboxMin.x + m.bboxMax.x);
    const double by = 0.5 * (m.bboxMin.y + m.bboxMax.y);
    const double maxSpan =
        std::max(m.bboxMax.x - m.bboxMin.x, m.bboxMax.y - m.bboxMin.y);
    double sx = 0, sy = 0;
    int n = 0;
    for (const auto &f : fits) {
      if (f.circle.ok && f.circle.r <= 0.75 * maxSpan &&
          (f.circle.rmse / std::max(1.0, f.circle.r)) <= 0.05 &&
          std::hypot(f.circle.xc - bx, f.circle.yc - by) <= 0.25 * maxSpan) {
        sx += f.circle.xc, sy += f.circle.yc, ++n;
      }
    }
    if (n > 0) {
      translate(m, -sx / n, -sy / n, 0);
      updateBbox(m);
    } else {
      translate(m, -bx, -by, 0);
      updateBbox(m);
    }
    aligned = true;
    rmse = ax.rmse;
    note = rim.error == KA::RimError::Ok
               ? QStringLiteral("바닥 수평(0 mm) · 회전 중심축 정렬됨")
               : QStringLiteral("회전 중심축 정렬됨 (구연부 미검출)");
  } else if (coarseAligned) {
    const double bx = 0.5 * (m.bboxMin.x + m.bboxMax.x);
    const double by = 0.5 * (m.bboxMin.y + m.bboxMax.y);
    const double maxSpan =
        std::max(m.bboxMax.x - m.bboxMin.x, m.bboxMax.y - m.bboxMin.y);
    double sx = 0.0, sy = 0.0;
    int n = 0;
    for (const auto &f : outers24) {
      if (f.circle.ok && f.circle.r <= 0.75 * maxSpan &&
          (f.circle.rmse / std::max(1.0, f.circle.r)) <= 0.04 &&
          std::hypot(f.circle.xc - bx, f.circle.yc - by) <= 0.25 * maxSpan) {
        sx += f.circle.xc;
        sy += f.circle.yc;
        ++n;
      }
    }
    const double cx = (n > 0) ? (sx / n) : bx;
    const double cy = (n > 0) ? (sy / n) : by;
    translate(work, -cx, -cy, 0);
    updateBbox(work);
    translate(m, -cx, -cy, 0);
    updateBbox(m);

    // 부분 결실 구연부(한쪽만 구연·경부가 남은 유물)인 경우:
    // 잔존 구연·경부 호가 절단면(180°, Y=0)과 정면 좌측 외면(180°~260°, X<=0·Y<=0)에
    // 동시에 놓이도록 +Z 초기 방위각을 자동 정렬한다.
    const double hWork = work.bboxMax.z - work.bboxMin.z;
    if (hWork > 10.0) {
      constexpr int kSec = 72;
      std::array<double, kSec> secMaxZ{};
      secMaxZ.fill(work.bboxMin.z);
      for (const auto &p : work.vertices) {
        double ang = std::atan2(p.y, p.x) * (180.0 / std::numbers::pi);
        if (ang < 0.0)
          ang += 360.0;
        const int si = std::clamp(int(ang / 5.0), 0, kSec - 1);
        secMaxZ[size_t(si)] = std::max(secMaxZ[size_t(si)], p.z);
      }
      int topSectors = 0;
      const double topCut = work.bboxMax.z - 0.10 * hWork;
      for (int si = 0; si < kSec; ++si) {
        if (secMaxZ[size_t(si)] >= topCut)
          ++topSectors;
      }
      if (topSectors >= 6 && topSectors <= 52) {
        int bestShift = 0;
        double bestAzScore = -1e18;
        const double rimCut = work.bboxMax.z - 0.04 * hWork;
        for (int sh = 0; sh < kSec; ++sh) {
          // sh 만큼 회전했을 때 원래 섹터 (s - sh) 가 새 섹터 s 로 온다
          auto rotZAt = [&](int targetSec) {
            const int orig = (targetSec - sh + kSec) % kSec;
            return secMaxZ[size_t(orig)];
          };
          // 180° (절단면 Y=0 좌측, 섹터 35·36·37 = 175°~190°) 전체가 구연부 높이에 도달해야
          // 0° 기본 절단면(Y=0)과 정면 좌측 실루엣이 모두 온전함
          const double z180Min =
              std::min({rotZAt(35), rotZAt(36), rotZAt(37)});
          const double z180Mean =
              (rotZAt(35) + rotZAt(36) + rotZAt(37)) / 3.0;
          double sc = (z180Min >= rimCut)
                          ? 2000.0
                          : (z180Mean - work.bboxMin.z) * 5.0;
          // 175°~225° (섹터 35~45, 절단면 180° 및 정면 좌측 외면) 구연·경부 보존도 합산
          for (int s = 35; s <= 45; ++s) {
            sc += (rotZAt(s) - work.bboxMin.z) / hWork * 10.0;
            if (rotZAt(s) >= topCut)
              sc += 8.0;
          }
          if (sc > bestAzScore) {
            bestAzScore = sc;
            bestShift = sh;
          }
        }
        if (bestShift != 0) {
          const double rotDeg = double(bestShift * 5);
          rotateZ(work, rotDeg);
          updateBbox(work);
          rotateZ(m, rotDeg);
          updateBbox(m);
        }
      }
    }

    aligned = true;
    rmse = 0.0;
    note = QStringLiteral("바닥 수평(0 mm) · 대칭축 자동 직립 정렬됨");
  } else {
    const double cx = 0.5 * (m.bboxMin.x + m.bboxMax.x),
                 cy = 0.5 * (m.bboxMin.y + m.bboxMax.y);
    translate(m, -cx, -cy, 0);
    updateBbox(m);
    note = QStringLiteral("중심축 미정렬 — [중심축 맞춤] 필요");
  }
  if (!unitNote_.isEmpty())
    note += QStringLiteral(" · ") + unitNote_;
  if (removedNoiseFaces_ > 0)
    note += QStringLiteral(" · 부유노이즈 %1면 제거").arg(removedNoiseFaces_);

  translate(m, 0, 0, -m.bboxMin.z); // 바닥 H 0
  updateBbox(m);
  base_ = std::move(m);
  buildLodMesh(base_, baseColors_, lodBase_, lodColors_, 35000);
  // 확정은 사람 몫 — 결과 문구·오차는 확정 때 옮긴다
  pendingAligned_ = aligned;
  pendingNote_ = note;
  pendingRmse_ = rmse;
  unconfirm();
  centered_ = true;
}

void ArtifactDoc::setFrontAngle(double deg) {
  if (!isLoaded() || !poseConfirmed_)
    return; // 확정 전에는 막을 돌리지 않는다
  frontAngle_ = std::remainder(deg, 360.0);
  recompute(false); // +Z 회전은 5대 치수·잰 기벽을 바꾸지 않는다 — 확정 때 값
                    // (단일 쉘의 추정 기벽만 각 단면을 따름)
}

void ArtifactDoc::alignCut(CutAxis a) {
  setFrontAngle(a == CutAxis::Y ? 0.0 : 90.0);
}

void ArtifactDoc::previewFrontAngle(double deg) {
  if (!isLoaded() || !poseConfirmed_)
    return;
  previewAngle_ = std::remainder(deg, 360.0);
  // 확정 자세 LOD 에서 매번 절대각으로(누적 오차 없음), setFrontAngle 과 같은
  // 함수·부호·0 판정. 강체 변환이라 FillMeshStats 대신 바운딩 박스만 다시 잰다.
  previewLod_ = lodBase_;
  if (std::fabs(previewAngle_) > 1e-9)
    rotateZ(previewLod_, previewAngle_);
  updateBbox(previewLod_);
  // 씨앗은 지금 확정된 추정 기벽 두께, 결과의 wallMm 은 버린다(확정값 그대로)
  previewLines_ =
      sliceSectionY0(previewLod_, inferInnerWall_, estimatedWallMm_);
  previewing_ = true;
}

void ArtifactDoc::commitPreview() {
  if (previewing_)
    setFrontAngle(previewAngle_);
}

void ArtifactDoc::clearPreview() {
  previewing_ = false;
  previewAngle_ = 0.0;
  previewLines_ = {};
  previewLod_ = {};
}

void ArtifactDoc::recompute(bool measureDims) {
  clearPreview();
  mesh_ = base_;
  lodMesh_ = lodBase_;
  if (!poseConfirmed_) {
    // 확정 전: 들어온(또는 사람이 돌리는) 자세만 보여 주고 단면·외곽·치수·
    // 기벽은 계산하지 않는다. 바운딩 박스는 자세 조작마다 갱신되어 있고
    // 강체 변환은 평균 변 길이·경계 유무를 바꾸지 않는다.
    ortho_.clear();
    sec_ = {};
    dims_ = {};
    d5_ = {};
    wallProfile_.clear();
    wallZ0_ = 0.0;
    wallStep_ = 1.0;
    wallMean_ = -1.0;
    lines_ = {};
    aligned_ = false;
    alignNote_ = QStringLiteral("바닥 미정");
    axisRmse_ = -1.0;
    return;
  }
  if (std::fabs(frontAngle_) > 1e-9) { // +Z 오른손 법칙 (위에서 보아 반시계)
    rotateZ(mesh_, frontAngle_);
    rotateZ(lodMesh_, frontAngle_);
  }
  updateBbox(mesh_);
  updateBbox(lodMesh_);
  mesh_.meanEdgeLength = base_.meanEdgeLength;
  mesh_.hasBoundary = base_.hasBoundary;
  lodMesh_.meanEdgeLength = lodBase_.meanEdgeLength;
  lodMesh_.hasBoundary = lodBase_.hasBoundary;

  // 그리는 외곽선 = 막과 수직인 실루엣의 외면 반쪽, 늘 원본에서(LOD 아님).
  // MaxRadius(2·98 백분위 · 칸 버림 · 빈 칸 복사 · 평활)는 끝이 기준선에 못
  // 미치고 급한 저부에서 선을 지어냈고, K-Arch TrueOrtho 는 x > 0 쪽까지 보고
  // 평활해 기운 바닥·구연에서 중심선을 넘었다(JOB-008).
  ortho_ = halfOrthoSilhouette(mesh_, 0.25);

  if (measureDims) {
    // 5대 치수·기벽은 늘 원본 메시에서(LOD 는 3.8 mm 칸 정점 평균이라 바닥판
    // 높이·기벽 고리가 무너져 25만 면 초과 메시에서 저경 5.2·기벽 3.85 mm 로
    // 틀렸다, JOB-009). K-Arch 실루엣은 치수에 maxBodyZ 대체값(최대 반경 정점이
    // 늘 있어 쓰이지 않음)으로만 들어가므로 계산하지 않는다.
    dims_ = KA::AnalyzeDimensions(mesh_, KA::SilhouetteResult{});
    d5_ = {};
    d5_.ok = dims_.ok;
    d5_.height = dims_.vesselHeight;
    d5_.rim = dims_.rimDiameter;
    d5_.belly = dims_.maxBodyDiameter;
    d5_.bellyZ = dims_.maxBodyZ - mesh_.bboxMin.z;
    d5_.base = (sherdMode_ || !isGrounded()) ? 0.0 : dims_.baseDiameter;

    // 기벽 두께 프로파일: 수평 단면을 높이별로 잘라 바깥 고리 - 안쪽 고리 (둘레
    // 평균). 높이마다 그 높이에 걸치는 원본 면만 자른다(결과는 원본 전체를 자른
    // 것과 같다). Feature 6: 병렬 160층 절단.
    const double h = mesh_.bboxMax.z - mesh_.bboxMin.z;
    wallZ0_ = mesh_.bboxMin.z;
    wallStep_ = std::max(1.0, h / 160.0);
    std::vector<double> zs;
    for (double z = wallZ0_; z <= mesh_.bboxMax.z;
         z += wallStep_) // 누적 합 높이 — JOB-008 까지와 같은 z 값
      zs.push_back(z);
    const auto bands = zBandFaces(mesh_, zs);
    wallProfile_.resize(zs.size());

    const unsigned int nThreads =
        (zs.size() > 8) ? std::max(1u, std::thread::hardware_concurrency())
                        : 1u;
    if (nThreads <= 1) {
      for (size_t i = 0; i < zs.size(); ++i) {
        wallProfile_[i] =
            (zs[i] <= wallZ0_ + 0.02 * h || zs[i] >= mesh_.bboxMax.z - 0.02 * h)
                ? -1.0
                : wallBySlice(bandMesh(mesh_, bands[i]), zs[i]);
      }
    } else {
      std::vector<std::thread> workers;
      workers.reserve(nThreads);
      for (unsigned int t = 0; t < nThreads; ++t) {
        workers.emplace_back([&, t]() {
          for (size_t i = t; i < zs.size(); i += nThreads) {
            wallProfile_[i] =
                (zs[i] <= wallZ0_ + 0.02 * h ||
                 zs[i] >= mesh_.bboxMax.z - 0.02 * h)
                    ? -1.0
                    : wallBySlice(bandMesh(mesh_, bands[i]), zs[i]);
          }
        });
      }
      for (auto &w : workers)
        w.join();
    }

    double sum = 0;
    int n = 0;
    for (size_t i = 0; i < wallProfile_.size(); ++i) {
      const double z = wallZ0_ + double(i) * wallStep_;
      if (z < wallZ0_ + 0.1 * h || z > wallZ0_ + 0.9 * h ||
          wallProfile_[i] <= 0 || isWallEstimatedAt(z))
        continue;
      sum += wallProfile_[i], ++n;
    }
    wallMean_ = n ? sum / n : -1.0;
  }
  // 확정 때 잰 평균 기벽, 두 고리를 못 잡은 단일 쉘(wallMean_ ≤ 0)이면 각
  // 단면이 갱신하는 추정값
  d5_.meanWall = wallMean_ > 0 ? wallMean_ : estimatedWallMm_;
  if (d5_.meanWall > 1.5 && d5_.meanWall < 35.0)
    estimatedWallMm_ = d5_.meanWall;

  buildSectionLoops();
}

void ArtifactDoc::resetSlots() {
  slotList_.clear();
  const double h = base_.bboxMax.z - base_.bboxMin.z;
  slotList_.push_back(
      {QStringLiteral("A–A′ 정면 실측도"), QStringLiteral("종단면"),
       QStringLiteral("보고서 주 도판 (1:1)"), SlotKind::Vertical, 0.0, 0.0});
  slotList_.push_back({QStringLiteral("B–B′ 구연부 횡단면"),
                       QStringLiteral("수평절단"),
                       QStringLiteral("구연부 두께 확인용"),
                       SlotKind::Horizontal, 0.0, h * 0.94});
  slotList_.push_back(
      {QStringLiteral("C–C′ 동체부 횡단면"), QStringLiteral("최대복경"),
       QStringLiteral("몸통 외경 확인"), SlotKind::Horizontal, 0.0, h * 0.5});
  slotList_.push_back({QStringLiteral("저부(바닥) 평면·단면"),
                       QStringLiteral("바닥 굽"),
                       QStringLiteral("바닥 굽 접지면"), SlotKind::Horizontal,
                       0.0, std::min(3.0, h * 0.03)});
}

// ---- 화면용 선: 자연 좌표에서 간략화·곡선 → 마지막에 좌/우 부호 (D5)
// ----------
std::vector<std::vector<KA::Point2>> ArtifactDoc::sectionNatural() const {
  const SectionLines &s = shown();
  if (!s.solids.empty() && !s.estInner.empty()) {
    const auto solids = scannedSolidNatural();
    const auto dashed = estimatedInnerNatural();
    if (!solids.empty() && !dashed.empty()) {
      std::vector<KA::Point2> poly = solids.front();
      for (size_t i = dashed.front().size(); i-- > 0;)
        poly.push_back(dashed.front()[i]);
      if (poly.size() >= 3)
        return {std::move(poly)};
    }
  }
  std::vector<std::vector<KA::Point2>> out;
  for (const auto &poly : s.loops) {
    auto p = fitSectionLine(poly, true);
    if (p.size() >= 3)
      out.push_back(std::move(p));
  }
  return out;
}

std::vector<std::vector<KA::Point2>> ArtifactDoc::scannedSolidNatural() const {
  std::vector<std::vector<KA::Point2>> out;
  for (const auto &line : shown().solids) {
    auto p = fitSectionLine(line, false);
    if (p.size() >= 2)
      out.push_back(std::move(p));
  }
  return out;
}

std::vector<std::vector<KA::Point2>>
ArtifactDoc::estimatedInnerNatural() const {
  std::vector<std::vector<KA::Point2>> out;
  for (const auto &line : shown().estInner) {
    auto p = fitSectionLine(line, false);
    if (p.size() >= 2)
      out.push_back(std::move(p));
  }
  return out;
}

std::vector<KA::Point2>
ArtifactDoc::fitSectionLine(const std::vector<KA::Point2> &raw,
                            bool closed) const {
  return fitCurve(raw, simplifyTol_ * 0.5, closed, smooth_, 3);
}

std::vector<KA::Point2> ArtifactDoc::outlineNatural() const {
  return fitCurve(ortho_, simplifyTol_ * 0.5, false, smooth_, 4);
}

std::vector<std::vector<KA::Point2>> ArtifactDoc::displaySection() const {
  return signedX(sectionNatural(), sectionSign());
}

std::vector<std::vector<KA::Point2>> ArtifactDoc::displayScannedSolid() const {
  return signedX(scannedSolidNatural(), sectionSign());
}

std::vector<std::vector<KA::Point2>>
ArtifactDoc::displayEstimatedInner() const {
  return signedX(estimatedInnerNatural(), sectionSign());
}

std::vector<KA::Point2> ArtifactDoc::displayOutline() const {
  // 외곽(반쪽 정사 실루엣, x ≤ 0)도 단면과 같은 부호: Right 는 그대로 왼쪽,
  // Left 는 오른쪽
  return signedX(outlineNatural(), sectionSign());
}

std::vector<std::pair<KA::Point2, KA::Point2>>
ArtifactDoc::displayHatch() const {
  // 빗금은 부호 적용 전(자연 좌표) 단면으로 계산한 뒤 끝점 x 에 같은 부호를
  // 곱한다 — 정확한 거울상 (Right "/", Left "\")
  std::vector<std::pair<KA::Point2, KA::Point2>> out;
  const auto polys = sectionNatural();
  const double c = std::sqrt(0.5), spacing = 1.5;
  auto rot = [&](KA::Point2 p, double s) {
    return KA::Point2{c * p.x + s * c * p.y, -s * c * p.x + c * p.y};
  };
  double lo = 1e18, hi = -1e18;
  std::vector<std::vector<KA::Point2>> rp;
  for (const auto &poly : polys) {
    std::vector<KA::Point2> q;
    for (const auto &p : poly)
      q.push_back(rot(p, 1.0)), lo = std::min(lo, q.back().y),
                                hi = std::max(hi, q.back().y);
    rp.push_back(std::move(q));
  }
  for (double y = std::ceil(lo / spacing) * spacing; y <= hi; y += spacing) {
    std::vector<double> xs;
    for (const auto &q : rp)
      for (size_t i = 0; i < q.size(); ++i) {
        const auto &a = q[i], &b = q[(i + 1) % q.size()];
        if ((a.y <= y && b.y > y) || (b.y <= y && a.y > y))
          xs.push_back(a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y));
      }
    std::sort(xs.begin(), xs.end());
    for (size_t i = 0; i + 1 < xs.size(); i += 2)
      if (xs[i + 1] - xs[i] > 0.3)
        out.push_back({rot({xs[i], y}, -1.0), rot({xs[i + 1], y}, -1.0)});
  }
  const double sgn = sectionSign();
  for (auto &[a, b] : out) {
    a.x *= sgn;
    b.x *= sgn;
  }
  return out;
}

void ArtifactDoc::buildSectionLoops() {
  clearPreview();
  lines_ = sliceSectionY0(base_, inferInnerWall_, estimatedWallMm_, frontAngle_,
                          &bvhMaster_);
  if (lines_.wallMm > 0)
    estimatedWallMm_ = lines_.wallMm; // 미스캔 보정 경로가 잰 실측 기벽
}

SectionLines sliceSectionY0(const KA::MeshData &m, bool inferInner,
                            double wallSeedMm, double angleDeg,
                            const KA::MeshBVH *bvh) {
  SectionLines s;
  std::vector<std::vector<KA::Point2>>
      openBranches; // 각 가지: 위(zHi) → 아래(zLo) 순서

  KA::Plane cutPlane{{0.0, 1.0, 0.0}, 0.0};
  double cosA = 1.0, sinA = 0.0;
  const bool useRot = std::fabs(angleDeg) > 1e-9;
  if (useRot) {
    cosSinDeg(angleDeg, cosA, sinA);
    cutPlane.n = {sinA, cosA, 0.0};
    cutPlane.d = 0.0;
  }

  auto rawSlices = KA::Slice(m, cutPlane, bvh);
  // 대용량 실측 스캔(수백만 면)에서 미세 비매니폴드 틈이나 꼭짓점 스침으로
  // 단면 윤곽선이 여러 열린 선분(open Polyline3)으로 쪼개진 경우 끝점을 이어 붙인다.
  std::vector<KA::Polyline3> closedSlices;
  std::vector<KA::Polyline3> openSlices;
  auto dist3 = [](const KA::Point3 &a, const KA::Point3 &b) {
    return std::hypot(a.x - b.x, a.y - b.y, a.z - b.z);
  };
  auto spanZ3 = [](const std::vector<KA::Point3> &pts) {
    if (pts.empty())
      return 0.0;
    double zLo = pts.front().z, zHi = zLo;
    for (const auto &p : pts) {
      zLo = std::min(zLo, p.z);
      zHi = std::max(zHi, p.z);
    }
    return zHi - zLo;
  };
  for (auto &pl : rawSlices) {
    if (pl.pts.size() < 2)
      continue;
    const double endGap = dist3(pl.pts.front(), pl.pts.back());
    if (pl.pts.size() >= 3 &&
        (endGap <= 0.05 || (endGap <= 3.0 && spanZ3(pl.pts) >= 20.0))) {
      closedSlices.push_back(std::move(pl));
    } else {
      double len = 0.0;
      for (size_t i = 1; i < pl.pts.size(); ++i)
        len += dist3(pl.pts[i - 1], pl.pts[i]);
      if (len >= 0.3 || rawSlices.size() <= 4)
        openSlices.push_back(std::move(pl));
    }
  }
  if (openSlices.size() >= 2) {
    constexpr double kStitchMaxMm = 15.0;
    while (openSlices.size() >= 2) {
      double bestD = kStitchMaxMm + 1.0;
      size_t bi = 0, bj = 0;
      int bMode = 0;
      for (size_t i = 0; i < openSlices.size(); ++i) {
        for (size_t j = i + 1; j < openSlices.size(); ++j) {
          const auto &pi = openSlices[i].pts;
          const auto &pj = openSlices[j].pts;
          const double d0 = dist3(pi.back(), pj.front());
          if (d0 < bestD) {
            bestD = d0, bi = i, bj = j, bMode = 0;
          }
          const double d1 = dist3(pi.back(), pj.back());
          if (d1 < bestD) {
            bestD = d1, bi = i, bj = j, bMode = 1;
          }
          const double d2 = dist3(pi.front(), pj.back());
          if (d2 < bestD) {
            bestD = d2, bi = i, bj = j, bMode = 2;
          }
          const double d3 = dist3(pi.front(), pj.front());
          if (d3 < bestD) {
            bestD = d3, bi = i, bj = j, bMode = 3;
          }
        }
      }
      if (bestD > kStitchMaxMm)
        break;
      auto &pi = openSlices[bi].pts;
      auto &pj = openSlices[bj].pts;
      if (bMode == 1) {
        std::reverse(pj.begin(), pj.end());
      } else if (bMode == 2) {
        pj.insert(pj.end(), pi.begin(), pi.end());
        pi = std::move(pj);
      } else if (bMode == 3) {
        std::reverse(pi.begin(), pi.end());
      }
      if (bMode != 2) {
        pi.insert(pi.end(), pj.begin(), pj.end());
      }
      openSlices.erase(openSlices.begin() + long(bj));
    }
  }
  for (auto &op : openSlices)
    closedSlices.push_back(std::move(op));

  // 부분 결실 유물에서 절단면(Y=0)의 +X 반쪽은 결실되었으나 -X 반쪽(180°)에
  // 온전한 구연·몸통 단면이 있는 경우, 더 온전한 반쪽을 단면으로 추출한다.
  double zMinPos = 1e18, zMaxPos = -1e18;
  double zMinNeg = 1e18, zMaxNeg = -1e18;
  for (const auto &pl : closedSlices) {
    for (const auto &p : pl.pts) {
      const double rx = useRot ? (p.x * cosA - p.y * sinA) : p.x;
      if (rx >= 0.0) {
        zMinPos = std::min(zMinPos, p.z);
        zMaxPos = std::max(zMaxPos, p.z);
      }
      if (rx <= 0.0) {
        zMinNeg = std::min(zMinNeg, p.z);
        zMaxNeg = std::max(zMaxNeg, p.z);
      }
    }
  }
  const double spanPos = (zMaxPos > zMinPos) ? (zMaxPos - zMinPos) : 0.0;
  const double spanNeg = (zMaxNeg > zMinNeg) ? (zMaxNeg - zMinNeg) : 0.0;
  const double sgnX = (spanNeg > spanPos + 5.0) ? -1.0 : 1.0;

  for (const auto &pl : closedSlices) {
    if (pl.pts.size() < 3)
      continue;
    std::vector<KA::Point2> in;
    in.reserve(pl.pts.size());
    if (useRot) {
      for (const auto &p : pl.pts) {
        in.push_back({sgnX * (p.x * cosA - p.y * sinA), p.z});
      }
    } else {
      for (const auto &p : pl.pts)
        in.push_back({sgnX * p.x, p.z});
    }

    // 중심축(x=0) 기준 오른쪽(x >= 0) 반평면 클리핑
    std::vector<KA::Point2> clip;
    for (size_t i = 0; i < in.size(); ++i) {
      const auto &a = in[i], &b = in[(i + 1) % in.size()];
      const bool ia = a.x >= 0, ib = b.x >= 0;
      if (ia)
        clip.push_back(a);
      if (ia != ib) {
        const double t = a.x / (a.x - b.x);
        clip.push_back({0.0, a.y + t * (b.y - a.y)});
      }
    }
    if (clip.size() < 3)
      continue;

    double maxJump = 0.0;
    size_t jumpIdx = 0;
    for (size_t i = 0; i < clip.size(); ++i) {
      const auto &a = clip[i], &b = clip[(i + 1) % clip.size()];
      if (a.x < 1.0 && b.x < 1.0)
        continue;
      const double d = std::hypot(b.x - a.x, b.y - a.y);
      if (d > maxJump) {
        maxJump = d;
        jumpIdx = i;
      }
    }

    if (maxJump <= 25.0) {
      double area = 0;
      for (size_t i = 0; i < clip.size(); ++i) {
        const auto &a = clip[i], &b = clip[(i + 1) % clip.size()];
        area += a.x * b.y - b.x * a.y;
      }
      if (std::fabs(area) * 0.5 >= 0.5)
        s.loops.push_back(std::move(clip));
      continue;
    }

    // 열린 사슬을 끊어진 변(jumpIdx) 다음 점부터 순서대로 정렬
    std::vector<KA::Point2> chain;
    chain.reserve(clip.size());
    for (size_t k = 1; k <= clip.size(); ++k)
      chain.push_back(clip[(jumpIdx + k) % clip.size()]);

    size_t iTop = 0;
    for (size_t i = 1; i < chain.size(); ++i)
      if (chain[i].y > chain[iTop].y)
        iTop = i;

    const bool isUTurn = iTop > 2 && iTop + 2 < chain.size() &&
                         (chain[iTop].y - chain.front().y > 12.0) &&
                         (chain[iTop].y - chain.back().y > 12.0);
    if (isUTurn) {
      // 구연부 입술이 연결된 U자형 열린 사슬 → 두 가지로 분리
      std::vector<KA::Point2> b1(chain.begin(), chain.begin() + long(iTop) + 1);
      std::vector<KA::Point2> b2(chain.begin() + long(iTop), chain.end());
      std::reverse(b1.begin(), b1.end());
      if (std::fabs(b1.front().y - b1.back().y) > 8.0)
        openBranches.push_back(std::move(b1));
      if (std::fabs(b2.front().y - b2.back().y) > 8.0)
        openBranches.push_back(std::move(b2));
    } else {
      if (chain.front().y < chain.back().y)
        std::reverse(chain.begin(), chain.end());
      if (std::fabs(chain.front().y - chain.back().y) > 8.0)
        openBranches.push_back(std::move(chain));
    }
  }

  auto polySpanY = [](const std::vector<KA::Point2> &poly) {
    if (poly.empty())
      return 0.0;
    double yLo = poly.front().y, yHi = yLo;
    for (const auto &p : poly) {
      yLo = std::min(yLo, p.y);
      yHi = std::max(yHi, p.y);
    }
    return yHi - yLo;
  };
  if (s.loops.size() > 1) {
    std::stable_sort(s.loops.begin(), s.loops.end(),
                     [&](const auto &a, const auto &b) {
                       return polySpanY(a) > polySpanY(b);
                     });
  }
  const double maxLoopSpan =
      s.loops.empty() ? 0.0 : polySpanY(s.loops.front());
  double maxBranchSpan = 0.0;
  for (const auto &b : openBranches)
    maxBranchSpan = std::max(maxBranchSpan, polySpanY(b));

  // 주 단면 고리가 따로 열린 가지(openBranches)에 있고 s.loops 에는 미세 노이즈 섬만 있는 경우 제거
  if (maxLoopSpan < 15.0 && maxBranchSpan >= 20.0) {
    s.loops.clear();
  } else if (maxLoopSpan >= 20.0 && s.loops.size() > 1) {
    s.loops.erase(
        std::remove_if(s.loops.begin() + 1, s.loops.end(),
                       [&](const auto &l) { return polySpanY(l) < 5.0; }),
        s.loops.end());
  }

  if (!s.loops.empty() || openBranches.empty())
    return s;
  if (!inferInner) {
    // 외벽과 내벽이 모두 스캔되었으나 구연·저부 결실(>25mm)로 열린 가지로 나뉜 경우 닫힌 단면 고리로 결합
    if (openBranches.size() >= 2 && maxBranchSpan >= 20.0) {
      size_t oIdx = 0;
      double bestSc = -1e18;
      for (size_t i = 0; i < openBranches.size(); ++i) {
        double mx = 0.0;
        for (const auto &p : openBranches[i])
          mx += p.x;
        mx /= double(openBranches[i].size());
        const double sc = polySpanY(openBranches[i]) * 2.0 + mx;
        if (sc > bestSc) {
          bestSc = sc;
          oIdx = i;
        }
      }
      size_t iIdx = oIdx;
      double bestInSpan = 0.0;
      for (size_t i = 0; i < openBranches.size(); ++i) {
        if (i == oIdx)
          continue;
        const double sp = polySpanY(openBranches[i]);
        if (sp > bestInSpan) {
          bestInSpan = sp;
          iIdx = i;
        }
      }
      if (iIdx != oIdx && bestInSpan >= 0.45 * polySpanY(openBranches[oIdx])) {
        std::vector<KA::Point2> closedPoly = openBranches[oIdx];
        const auto &inBr = openBranches[iIdx];
        for (auto it = inBr.rbegin(); it != inBr.rend(); ++it)
          closedPoly.push_back(*it);
        if (closedPoly.size() >= 3)
          s.loops.push_back(std::move(closedPoly));
      }
    }
    return s;
  }

  // 가장 바닥(minY)까지 내려가고 평균 반경이 바깥쪽인 가지를 외벽(outer)으로,
  // 나머지를 스캔된 상부 내벽(innerScanned)으로 결합
  size_t outerIdx = 0;
  double bestScore = -1e18;
  for (size_t i = 0; i < openBranches.size(); ++i) {
    const auto &b = openBranches[i];
    double meanX = 0;
    for (const auto &p : b)
      meanX += p.x;
    meanX /= double(b.size());
    const double spanY = b.front().y - b.back().y;
    const double score = spanY * 2.0 + meanX;
    if (score > bestScore) {
      bestScore = score;
      outerIdx = i;
    }
  }
  const std::vector<KA::Point2> outer = openBranches[outerIdx];
  std::vector<KA::Point2> innerScanned;
  for (size_t i = 0; i < openBranches.size(); ++i) {
    if (i == outerIdx)
      continue;
    if (openBranches[i].size() > innerScanned.size())
      innerScanned = openBranches[i];
  }

  double zScanEnd = outer.front().y;
  double realWall = std::max(2.0, wallSeedMm);
  if (innerScanned.size() >= 2) {
    zScanEnd = innerScanned.back().y;
    for (size_t i = 1; i < outer.size(); ++i) {
      const auto &a = outer[i - 1], &b = outer[i];
      if ((a.y >= zScanEnd && b.y <= zScanEnd) ||
          (b.y >= zScanEnd && a.y <= zScanEnd)) {
        const double t =
            std::fabs(b.y - a.y) < 1e-9 ? 0.0 : (zScanEnd - a.y) / (b.y - a.y);
        const double rOut = a.x + t * (b.x - a.x);
        const double w = rOut - innerScanned.back().x;
        if (w > 1.5 && w < 35.0)
          realWall = w;
        break;
      }
    }
  }
  s.wallMm = realWall;

  std::vector<KA::Point2> estInner;
  if (!innerScanned.empty())
    estInner.push_back(innerScanned.back());
  for (size_t i = 0; i < outer.size(); ++i) {
    if (outer[i].y > zScanEnd)
      continue;
    const auto &p0 = outer[i > 0 ? i - 1 : i];
    const auto &p1 = outer[i + 1 < outer.size() ? i + 1 : i];
    const double tx = p1.x - p0.x, ty = p1.y - p0.y;
    const double L = std::hypot(tx, ty);
    double nx = -1.0, ny = 0.0;
    if (L > 1e-9) {
      nx = ty / L;
      ny = -tx / L;
      if (nx > 0) {
        nx = -nx;
        ny = -ny;
      }
    }
    estInner.push_back({std::max(0.0, outer[i].x + nx * realWall),
                        std::max(realWall * 0.7, outer[i].y + ny * realWall)});
  }
  if (estInner.size() >= 2) {
    for (const auto &q : estInner) {
      s.estZMin = (s.estZMin < 0) ? q.y : std::min(s.estZMin, q.y);
      s.estZMax = std::max(s.estZMax, q.y);
    }
    s.estInner.push_back(estInner);

    std::vector<KA::Point2> solidLine;
    if (!innerScanned.empty()) {
      for (size_t i = innerScanned.size(); i-- > 0;)
        solidLine.push_back(innerScanned[i]);
    }
    for (const auto &p : outer)
      solidLine.push_back(p);
    solidLine.push_back(estInner.back());
    s.solids.push_back(std::move(solidLine));
  }

  std::vector<KA::Point2> closedPoly = outer;
  for (auto it = estInner.rbegin(); it != estInner.rend(); ++it)
    closedPoly.push_back(*it);
  if (innerScanned.size() >= 2) {
    for (size_t i = innerScanned.size() - 1; i-- > 0;)
      closedPoly.push_back(innerScanned[i]);
  }
  if (closedPoly.size() >= 3)
    s.loops.push_back(std::move(closedPoly));
  return s;
}

std::vector<KA::Polyline3> ArtifactDoc::horizontalSlice(double z) const {
  return KA::Slice(mesh_, KA::Plane{{0, 0, 1}, -z});
}

bool ArtifactDoc::isWallEstimatedAt(double z) const { // 확정 각 (놓은 뒤 갱신)
  return !lines_.estInner.empty() && z >= lines_.estZMin - 1.0 &&
         z <= lines_.estZMax + 1.0;
}

double ArtifactDoc::wallThicknessAt(double z) const {
  if (isWallEstimatedAt(z))
    return estimatedWallMm_;
  if (wallProfile_.size() < 2)
    return -1.0;
  const double t = (z - wallZ0_) / wallStep_;
  const long i = long(std::floor(t));
  if (i < 0 || i + 1 >= long(wallProfile_.size()))
    return -1.0;
  const double a = wallProfile_[size_t(i)], b = wallProfile_[size_t(i + 1)];
  if (a <= 0 || b <= 0)
    return std::max(a, b);
  return a + (b - a) * (t - double(i));
}

double ArtifactDoc::maxDeviationMm() const {
  const SectionLines &s = shown();
  double d = dev2(ortho_, outlineNatural(), false);
  if (!s.solids.empty() &&
      !s.estInner.empty()) { // 미스캔 보정: 실측 실선 + 추정 내벽선
    for (const auto *lines : {&s.solids, &s.estInner})
      for (const auto &l : *lines)
        if (const auto q = fitSectionLine(l, false); q.size() >= 2)
          d = std::max(d, dev2(l, q, false));
  } else {
    for (const auto &l : s.loops) // 3점 미만으로 줄어 그리지 않는 고리는 뺀다
      if (const auto q = fitSectionLine(l, true); q.size() >= 3)
        d = std::max(d, dev2(l, q, true));
  }
  return d;
}

int ArtifactDoc::pointCount() const {
  int n = int(displayOutline().size());
  for (const auto &p : displaySection())
    n += int(p.size());
  return n;
}

int ArtifactDoc::rawPointCount() const {
  int n = int(ortho_.size());
  for (const auto &p : shown().loops)
    n += int(p.size());
  return n;
}

double ArtifactDoc::maxDimension() const {
  if (base_.vertices.empty())
    return 0.0;
  return std::max({base_.bboxMax.x - base_.bboxMin.x,
                   base_.bboxMax.y - base_.bboxMin.y,
                   base_.bboxMax.z - base_.bboxMin.z});
}

double ArtifactDoc::averageEdgeLength() const {
  if (base_.meanEdgeLength > 0.0)
    return base_.meanEdgeLength;
  if (base_.vertices.empty() || base_.faces.empty())
    return 0.0;
  double sum = 0.0;
  size_t count = 0;
  const size_t step = std::max(size_t(1), base_.faces.size() / 10000);
  for (size_t i = 0; i < base_.faces.size(); i += step) {
    const auto &f = base_.faces[i];
    const auto &p0 = base_.vertices[f[0]];
    const auto &p1 = base_.vertices[f[1]];
    const auto &p2 = base_.vertices[f[2]];
    sum += std::hypot(p1.x - p0.x, p1.y - p0.y, p1.z - p0.z);
    sum += std::hypot(p2.x - p1.x, p2.y - p1.y, p2.z - p1.z);
    sum += std::hypot(p0.x - p2.x, p0.y - p2.y, p0.z - p2.z);
    count += 3;
  }
  return count > 0 ? sum / double(count) : 0.0;
}

void ArtifactDoc::confirmUnit(double factor) {
  if (std::fabs(factor - 1.0) > 1e-6 && factor > 0.0) {
    rescaleUnits(factor);
  }
  unitConfirmed_ = true;
}

double ArtifactDoc::outerRadiusAt(double h) const {
  if (!isLoaded())
    return -1.0;
  const auto ol = displayOutline();
  for (size_t i = 1; i < ol.size(); ++i) {
    const auto &a = ol[i - 1], &b = ol[i];
    if ((a.y <= h && b.y >= h) || (b.y <= h && a.y >= h)) {
      const double t =
          std::fabs(b.y - a.y) < 1e-9 ? 0.0 : (h - a.y) / (b.y - a.y);
      return std::fabs(a.x + t * (b.x - a.x));
    }
  }
  return -1.0;
}

void ArtifactDoc::computeRimCircleFit() {
  const auto &m = base_.vertices.empty() ? mesh_ : base_;
  if (m.vertices.empty()) {
    estimatedRimDia_ = 0.0;
    estimatedRimErr_ = 0.0;
    rimArcDeg_ = 0.0;
    return;
  }

  std::vector<KA::Point2> pts2d;
  auto rimRes = KA::DetectRimBoundary(m);
  if (rimRes.error == KA::RimError::Ok && !rimRes.rimPoints.empty()) {
    pts2d.reserve(rimRes.rimPoints.size());
    for (const auto &p : rimRes.rimPoints) {
      pts2d.push_back({p.x, p.y});
    }
  }

  if (pts2d.size() < 10) {
    pts2d.clear();
    const double H = m.bboxMax.z - m.bboxMin.z;
    const double zTol = std::max(1.0, 0.05 * H);
    const double cx = 0.5 * (m.bboxMin.x + m.bboxMax.x);
    const double cy = 0.5 * (m.bboxMin.y + m.bboxMax.y);
    double maxR = 0.0;
    for (const auto &v : m.vertices) {
      if (v.z >= m.bboxMax.z - zTol) {
        maxR = std::max(maxR, std::hypot(v.x - cx, v.y - cy));
      }
    }
    for (const auto &v : m.vertices) {
      if (v.z >= m.bboxMax.z - zTol) {
        const double r = std::hypot(v.x - cx, v.y - cy);
        if (r >= 0.7 * maxR && maxR > 0.0) {
          pts2d.push_back({v.x, v.y});
        }
      }
    }
  }

  if (pts2d.size() < 4) {
    const double cx = 0.5 * (m.bboxMin.x + m.bboxMax.x);
    const double cy = 0.5 * (m.bboxMin.y + m.bboxMax.y);
    double maxR = 0.0;
    for (const auto &v : m.vertices) {
      maxR = std::max(maxR, std::hypot(v.x - cx, v.y - cy));
    }
    estimatedRimDia_ = 2.0 * maxR;
    estimatedRimErr_ = 0.0;
    rimArcDeg_ = 360.0;
    return;
  }

  const KA::Circle2D c = KA::FitCircleTaubin(pts2d);
  if (c.ok && c.r > 0.0) {
    estimatedRimDia_ = 2.0 * c.r;
    estimatedRimErr_ = c.rmse;

    std::vector<double> angles;
    angles.reserve(pts2d.size());
    for (const auto &pt : pts2d) {
      double ang =
          std::atan2(pt.y - c.yc, pt.x - c.xc) * (180.0 / std::numbers::pi);
      if (ang < 0.0)
        ang += 360.0;
      angles.push_back(ang);
    }
    std::sort(angles.begin(), angles.end());
    double maxGap = 0.0;
    for (size_t i = 1; i < angles.size(); ++i) {
      maxGap = std::max(maxGap, angles[i] - angles[i - 1]);
    }
    if (!angles.empty()) {
      maxGap = std::max(maxGap, (360.0 - angles.back()) + angles.front());
    }
    rimArcDeg_ = std::clamp(360.0 - maxGap, 10.0, 360.0);
  } else {
    const double wX = m.bboxMax.x - m.bboxMin.x;
    const double wY = m.bboxMax.y - m.bboxMin.y;
    estimatedRimDia_ = std::max(wX, wY);
    estimatedRimErr_ = 0.0;
    rimArcDeg_ = 360.0;
  }
}

std::vector<FloorCandidate> ArtifactDoc::floorCandidates() const {
  std::vector<FloorCandidate> out;
  if (!isLoaded())
    return out;

  const auto &m = base_.vertices.empty() ? mesh_ : base_;
  if (m.vertices.empty())
    return out;

  const double H = m.bboxMax.z - m.bboxMin.z;
  const double Wx = m.bboxMax.x - m.bboxMin.x;
  const double Wy = m.bboxMax.y - m.bboxMin.y;

  KA::Point3 cog{0, 0, 0};
  for (const auto &v : m.vertices) {
    cog.x += v.x;
    cog.y += v.y;
    cog.z += v.z;
  }
  cog.x /= double(m.vertices.size());
  cog.y /= double(m.vertices.size());
  cog.z /= double(m.vertices.size());

  auto evalCandidate = [&](int id, const QString &name,
                           const KA::Point3 &normal, double targetZ,
                           bool invertNormal) -> FloorCandidate {
    FloorCandidate c;
    c.id = id;
    c.name = name;
    c.normal = normal;
    c.center = {cog.x, cog.y, targetZ};

    const double slabTol = std::max(1.0, 0.15 * H);
    double contactAreaMm2 = 0.0;
    KA::Point3 contactCenter{0, 0, 0};
    double sumAlignArea = 0.0;
    KA::Point3 normalSum{0, 0, 0};

    for (const auto &f : m.faces) {
      const auto &p0 = m.vertices[f[0]];
      const auto &p1 = m.vertices[f[1]];
      const auto &p2 = m.vertices[f[2]];
      const double fz = (p0.z + p1.z + p2.z) / 3.0;
      if (std::fabs(fz - targetZ) <= slabTol) {
        const double e1x = p1.x - p0.x, e1y = p1.y - p0.y, e1z = p1.z - p0.z;
        const double e2x = p2.x - p0.x, e2y = p2.y - p0.y, e2z = p2.z - p0.z;
        const double nx = e1y * e2z - e1z * e2y;
        const double ny = e1z * e2x - e1x * e2z;
        const double nz = e1x * e2y - e1y * e2x;
        const double area2 = std::sqrt(nx * nx + ny * ny + nz * nz);
        const double area = 0.5 * area2;
        if (area2 > 1e-9) {
          const double unz = nz / area2;
          const double align = invertNormal ? -unz : unz;
          if (align > 0.3) {
            contactAreaMm2 += area * align;
            contactCenter.x += ((p0.x + p1.x + p2.x) / 3.0) * area;
            contactCenter.y += ((p0.y + p1.y + p2.y) / 3.0) * area;
            normalSum.x += nx * 0.5 * (invertNormal ? -1.0 : 1.0);
            normalSum.y += ny * 0.5 * (invertNormal ? -1.0 : 1.0);
            normalSum.z += nz * 0.5 * (invertNormal ? -1.0 : 1.0);
            sumAlignArea += area;
          }
        }
      }
    }

    if (sumAlignArea > 1e-4) {
      contactCenter.x /= sumAlignArea;
      contactCenter.y /= sumAlignArea;
    } else {
      contactCenter.x = cog.x;
      contactCenter.y = cog.y;
      double maxR = 0.0;
      for (const auto &v : m.vertices) {
        if (std::fabs(v.z - targetZ) <= slabTol)
          maxR = std::max(maxR, std::hypot(v.x - cog.x, v.y - cog.y));
      }
      contactAreaMm2 = std::numbers::pi * maxR * maxR;
    }

    if (sumAlignArea > 1e-4) {
      const double normLen =
          std::sqrt(normalSum.x * normalSum.x + normalSum.y * normalSum.y +
                    normalSum.z * normalSum.z);
      c.flatness = std::clamp((normLen / sumAlignArea) * 100.0, 15.0, 98.0);
    } else {
      c.flatness = 50.0;
    }
    c.areaCm2 = contactAreaMm2 / 100.0;

    const double dCog =
        std::hypot(cog.x - contactCenter.x, cog.y - contactCenter.y);
    c.cogScore = std::clamp(1.0 - dCog / std::max(10.0, 0.5 * std::max(Wx, Wy)),
                            0.0, 1.0);
    c.totalScore =
        c.flatness * 0.5 + std::min(50.0, c.areaCm2) * 0.5 + c.cogScore * 20.0;
    return c;
  };

  FloorCandidate c1 =
      evalCandidate(1, QStringLiteral("① 저부 접지면 — 이 면을 바닥으로"),
                    {0.0, 0.0, 1.0}, m.bboxMin.z, true);
  FloorCandidate c2 =
      evalCandidate(2, QStringLiteral("② 구연 평면 — 엎어 놓기"),
                    {0.0, 0.0, -1.0}, m.bboxMax.z, false);
  FloorCandidate c3;
  c3.id = 3;
  c3.name = QStringLiteral("③ 동체 옆면 — 부적합");
  c3.normal = {1.0, 0.0, 0.0};
  c3.center = {m.bboxMax.x, cog.y, 0.5 * (m.bboxMin.z + m.bboxMax.z)};
  double sideContactArea = 0.0;
  KA::Point3 sideNormalSum{0, 0, 0};
  double sideSumArea = 0.0;
  const double sideTolX = std::max(1.0, 0.15 * Wx);
  for (const auto &f : m.faces) {
    const auto &p0 = m.vertices[f[0]];
    const auto &p1 = m.vertices[f[1]];
    const auto &p2 = m.vertices[f[2]];
    const double fx = (p0.x + p1.x + p2.x) / 3.0;
    if (std::fabs(fx - m.bboxMax.x) <= sideTolX) {
      const double e1x = p1.x - p0.x, e1y = p1.y - p0.y, e1z = p1.z - p0.z;
      const double e2x = p2.x - p0.x, e2y = p2.y - p0.y, e2z = p2.z - p0.z;
      const double nx = e1y * e2z - e1z * e2y;
      const double ny = e1z * e2x - e1x * e2z;
      const double nz = e1x * e2y - e1y * e2x;
      const double area2 = std::sqrt(nx * nx + ny * ny + nz * nz);
      const double area = 0.5 * area2;
      if (area2 > 1e-9) {
        const double unx = nx / area2;
        if (unx > 0.3) {
          sideContactArea += area * unx;
          sideNormalSum.x += nx * 0.5;
          sideNormalSum.y += ny * 0.5;
          sideNormalSum.z += nz * 0.5;
          sideSumArea += area;
        }
      }
    }
  }
  c3.areaCm2 = std::max(1.0, sideContactArea / 100.0);
  if (sideSumArea > 1e-4) {
    const double normLen = std::sqrt(sideNormalSum.x * sideNormalSum.x +
                                     sideNormalSum.y * sideNormalSum.y +
                                     sideNormalSum.z * sideNormalSum.z);
    c3.flatness = std::clamp((normLen / sideSumArea) * 25.0, 8.0, 24.0);
  } else {
    c3.flatness = 12.0;
  }
  c3.cogScore = 0.0;
  c3.totalScore = c3.flatness * 0.5 + std::min(50.0, c3.areaCm2) * 0.5;

  out.push_back(c1);
  out.push_back(c2);
  out.push_back(c3);

  if (out[0].totalScore >= out[1].totalScore &&
      out[0].totalScore >= out[2].totalScore) {
    out[0].recommended = true;
  } else if (out[1].totalScore >= out[2].totalScore) {
    out[1].recommended = true;
  } else {
    out[2].recommended = true;
  }

  for (auto &cand : out) {
    cand.desc =
        QStringLiteral("평평함 %1 % · 넓이 %2 ㎠ · %3")
            .arg(cand.flatness, 0, 'f', 0)
            .arg(cand.areaCm2, 0, 'f', 1)
            .arg(cand.recommended
                     ? QStringLiteral("추천")
                     : (cand.flatness < 30.0 ? QStringLiteral("둥글어서 굴러감")
                                             : QStringLiteral("부적합")));
  }
  return out;
}

void ArtifactDoc::applyFloorCandidate(int index) {
  if (!isLoaded())
    return;
  if (index == 0) {
    autoAlign();
    dropToFloor();
  } else if (index == 1) {
    flipPose();
    dropToFloor();
  } else if (index == 2) {
    rotatePose(PoseAxis::X, 90.0);
    dropToFloor();
  }
}

void ArtifactDoc::setBaseTiltMode(BaseTiltMode m) {
  if (baseTiltMode_ == m)
    return;
  baseTiltMode_ = m;
  if (isLoaded() && poseConfirmed_)
    recompute(false);
}

bool ArtifactDoc::isLying() const {
  if (!isLoaded())
    return false;
  const double h = base_.bboxMax.z - base_.bboxMin.z;
  const double w = std::max(base_.bboxMax.x - base_.bboxMin.x,
                            base_.bboxMax.y - base_.bboxMin.y);
  return h < w * 0.85;
}

void ArtifactDoc::setSherdMode(bool on) {
  if (sherdMode_ == on)
    return;
  sherdMode_ = on;
  if (isLoaded()) {
    computeRimCircleFit();
    if (poseConfirmed_)
      recompute(true);
  }
}

RimGapInfo ArtifactDoc::rimGapChart() const {
  RimGapInfo info;
  info.maxGapMm = 0.0;
  info.contactSpanDeg = 0.0;
  info.gapSpanDeg = 0.0;

  if (!isLoaded())
    return info;

  const auto &m = base_.vertices.empty() ? mesh_ : base_;
  const double fitR = estimatedRimDiameter() * 0.5;

  for (int a = 0; a <= 112; a += 4) {
    info.anglesDeg.push_back(double(a));
    const double rad = double(a) * (std::numbers::pi / 180.0);
    const double targetX = fitR * std::cos(rad);
    const double targetY = fitR * std::sin(rad);

    double minSq = 1e9;
    double vertexZ = m.bboxMin.z;
    for (const auto &v : m.vertices) {
      const double sq =
          (v.x - targetX) * (v.x - targetX) + (v.y - targetY) * (v.y - targetY);
      if (sq < minSq) {
        minSq = sq;
        vertexZ = v.z;
      }
    }

    const double zGap = std::max(0.0, vertexZ - m.bboxMin.z);
    const double rDev = std::sqrt(minSq);
    const double gap = zGap + 0.05 * std::min(10.0, rDev);
    info.gapMm.push_back(gap);
    info.maxGapMm = std::max(info.maxGapMm, gap);
    if (gap <= 0.3)
      info.contactSpanDeg += 4.0;
    else
      info.gapSpanDeg += 4.0;
  }
  return info;
}

double ArtifactDoc::estimatedRimDiameter() const {
  if (estimatedRimDia_ <= 0.0 && isLoaded()) {
    const_cast<ArtifactDoc *>(this)->computeRimCircleFit();
  }
  return estimatedRimDia_;
}

double ArtifactDoc::estimatedRimError() const {
  if (estimatedRimDia_ <= 0.0 && isLoaded()) {
    const_cast<ArtifactDoc *>(this)->computeRimCircleFit();
  }
  return estimatedRimErr_;
}

double ArtifactDoc::rimArcDegrees() const {
  if (estimatedRimDia_ <= 0.0 && isLoaded()) {
    const_cast<ArtifactDoc *>(this)->computeRimCircleFit();
  }
  return rimArcDeg_;
}

double ArtifactDoc::angleDefectScore(double a) const {
  a = std::fmod(std::fmod(a, 360.0) + 360.0, 360.0);
  if (!isLoaded())
    return 0.0;

  const auto &meshToSlice = lodBase_.vertices.empty()
                                ? (base_.vertices.empty() ? mesh_ : base_)
                                : lodBase_;
  if (meshToSlice.vertices.empty())
    return 0.0;

  SectionLines s = sliceSectionY0(meshToSlice, false, -1.0, a);

  double breakCount = 0.0;
  if (s.loops.empty()) {
    breakCount = s.solids.empty() ? 1.0 : double(s.solids.size());
  } else {
    const auto &loop = s.loops[0];
    if (loop.size() >= 2) {
      double dEnd = std::hypot(loop.front().x - loop.back().x,
                               loop.front().y - loop.back().y);
      if (dEnd >= 2.0)
        breakCount = 1.0;
    }
  }

  double crackCount = s.loops.size() > 1 ? double(s.loops.size() - 1) : 0.0;

  double wallVar = 0.0;
  if (wallProfile_.size() >= 2) {
    double mean = 0.0;
    for (double w : wallProfile_)
      mean += w;
    mean /= double(wallProfile_.size());
    double var = 0.0;
    for (double w : wallProfile_)
      var += (w - mean) * (w - mean);
    wallVar = std::sqrt(var / double(wallProfile_.size()));
  }

  const double H = dims_.vesselHeight > 0.0
                       ? dims_.vesselHeight
                       : (meshToSlice.bboxMax.z - meshToSlice.bboxMin.z);
  const double DEG = std::numbers::pi / 180.0;
  const double cosA = std::cos(a * DEG);
  const double sinA = std::sin(a * DEG);
  double rRight = 0.0, rLeft = 0.0;
  double extQ3MaxZ = 0.0;
  for (const auto &v : meshToSlice.vertices) {
    const double xRot = cosA * v.x - sinA * v.y;
    const double yRot = sinA * v.x + cosA * v.y;
    if (xRot <= -4.0 && yRot <= 2.0) {
      extQ3MaxZ = std::max(extQ3MaxZ, v.z);
    }
    if (v.z >= 0.85 * H) {
      const double distNormal = yRot;
      if (std::fabs(distNormal) <= 2.0) {
        const double proj = xRot;
        if (proj > 0.0)
          rRight = std::max(rRight, proj);
        else
          rLeft = std::max(rLeft, -proj);
      }
    }
  }
  const double diaDiff =
      (rRight > 0.0 && rLeft > 0.0) ? std::fabs(rRight - rLeft) : 0.0;
  double loopSpan = 0.0;
  if (!s.loops.empty() && !s.loops[0].empty()) {
    double yLo = s.loops[0].front().y, yHi = yLo;
    for (const auto &p : s.loops[0]) {
      yLo = std::min(yLo, p.y);
      yHi = std::max(yHi, p.y);
    }
    loopSpan = yHi - yLo;
  } else if (!s.solids.empty() && !s.solids[0].empty()) {
    double yLo = s.solids[0].front().y, yHi = yLo;
    for (const auto &p : s.solids[0]) {
      yLo = std::min(yLo, p.y);
      yHi = std::max(yHi, p.y);
    }
    loopSpan = yHi - yLo;
  }
  const double missingPenalty =
      (H > 1.0 && loopSpan < 0.96 * H) ? 180.0 * (0.96 - loopSpan / H) : 0.0;
  const double extMissingPenalty =
      (H > 1.0 && extQ3MaxZ < 0.96 * H) ? 180.0 * (0.96 - extQ3MaxZ / H) : 0.0;

  const double score = 5.0 * breakCount + 5.0 * crackCount +
                       2.0 * std::min(5.0, wallVar) +
                       0.3 * (s.solids.empty() ? 0.0 : 1.0) + diaDiff +
                       missingPenalty + extMissingPenalty +
                       0.05 * (1.0 - std::cos(a * DEG));
  return score;
}

std::vector<AngleRecommendation> ArtifactDoc::recommendAngles() const {
  std::vector<std::pair<double, double>> all;
  all.reserve(72);
  for (int a = 0; a < 360; a += 5) {
    all.push_back({double(a), angleDefectScore(double(a))});
  }
  std::sort(all.begin(), all.end(),
            [](const auto &x, const auto &y) { return x.second < y.second; });

  const auto &meshToSlice = lodBase_.vertices.empty()
                                ? (base_.vertices.empty() ? mesh_ : base_)
                                : lodBase_;
  const double H = dims_.vesselHeight > 0.0
                       ? dims_.vesselHeight
                       : (meshToSlice.bboxMax.z - meshToSlice.bboxMin.z);
  const double DEG = std::numbers::pi / 180.0;

  std::vector<AngleRecommendation> out;
  for (const auto &c : all) {
    bool ok = true;
    for (const auto &p : out) {
      double diff = std::fabs(std::remainder(c.first - p.angleDeg, 360.0));
      if (diff < 30.0) {
        ok = false;
        break;
      }
    }
    if (ok) {
      AngleRecommendation rec;
      rec.rank = int(out.size()) + 1;
      rec.angleDeg = c.first;
      rec.score = c.second;
      rec.star = (out.empty());
      const double cosA = std::cos(c.first * DEG);
      const double sinA = std::sin(c.first * DEG);
      double rRight = 0.0, rLeft = 0.0;
      for (const auto &v : meshToSlice.vertices) {
        if (v.z >= 0.85 * H && std::fabs(-sinA * v.x + cosA * v.y) <= 2.0) {
          const double proj = cosA * v.x + sinA * v.y;
          if (proj > 0.0)
            rRight = std::max(rRight, proj);
          else
            rLeft = std::max(rLeft, -proj);
        }
      }
      const double measuredDiff = std::fabs(rRight - rLeft);
      if (rec.rank == 1) {
        rec.why = QStringLiteral("끊김 없음 · 두께 고름 · 구경 차 %1 mm")
                      .arg(measuredDiff, 0, 'f', 1);
      } else {
        rec.why = QStringLiteral("결실 회피 · 단면 온전 · 구경 차 %1 mm")
                      .arg(measuredDiff, 0, 'f', 1);
      }
      out.push_back(rec);
      if (out.size() == 3)
        break;
    }
  }
  return out;
}

double ArtifactDoc::snapAngle(double deg, double tol) const {
  const auto recs = recommendAngles();
  for (const auto &r : recs) {
    double diff = std::fabs(std::remainder(deg - r.angleDeg, 360.0));
    if (diff <= tol)
      return r.angleDeg;
  }
  return deg;
}

CutCheckStatus ArtifactDoc::checkCutStatus() const {
  CutCheckStatus st;
  const auto &s = shown();
  st.closedLoop = !s.loops.empty();
  st.breakCount = st.closedLoop ? 0 : int(std::max<size_t>(1, s.solids.size()));
  if (!wallProfile_.empty()) {
    double mn = 1e9, mx = -1e9;
    for (double w : wallProfile_) {
      if (w > 0.5) {
        mn = std::min(mn, w);
        mx = std::max(mx, w);
      }
    }
    if (mn <= mx) {
      st.minWall = mn;
      st.maxWall = mx;
    }
  }
  if (st.minWall <= 0.0 && wallMean_ > 0.0) {
    st.minWall = wallMean_;
    st.maxWall = wallMean_;
  }
  double rRight = 0.0, rLeft = 0.0;
  const auto ol = displayOutline();
  const auto sec = displaySection();
  if (!ol.empty() && !sec.empty()) {
    double maxZ = dims_.vesselHeight > 0 ? dims_.vesselHeight : 100.0;
    for (const auto &p : ol) {
      if (p.y >= 0.85 * maxZ)
        rLeft = std::max(rLeft, std::fabs(p.x));
    }
    for (const auto &loop : sec) {
      for (const auto &p : loop) {
        if (p.y >= 0.85 * maxZ)
          rRight = std::max(rRight, std::fabs(p.x));
      }
    }
  }
  st.rimDiff = std::fabs(rRight - rLeft);

  const QString wallStr = st.minWall > 0.0 ? QStringLiteral("두께 %1–%2 mm")
                                                 .arg(st.minWall, 0, 'f', 1)
                                                 .arg(st.maxWall, 0, 'f', 1)
                                           : QStringLiteral("두께 확인 필요");
  if (st.closedLoop && st.breakCount == 0) {
    st.statusText =
        QStringLiteral("✓ 닫힌 단면 · 끊김 0 · %1 · 좌우 구경 차 %2 mm")
            .arg(wallStr)
            .arg(st.rimDiff, 0, 'f', 1);
  } else {
    st.statusText =
        QStringLiteral("✗ 끊긴 단면 · 끊김 %1개 · %2 · 좌우 구경 차 %3 mm")
            .arg(st.breakCount)
            .arg(wallStr)
            .arg(st.rimDiff, 0, 'f', 1);
  }
  return st;
}

std::vector<AngleStripCell> ArtifactDoc::angleStripCells() const {
  std::vector<AngleStripCell> cells;
  if (!isLoaded())
    return cells;
  cells.reserve(12);
  const auto &meshToSlice = lodBase_.vertices.empty()
                                ? (base_.vertices.empty() ? mesh_ : base_)
                                : lodBase_;
  const auto recs = recommendAngles();

  for (int i = 0; i < 12; ++i) {
    const double deg = double(i * 30);
    AngleStripCell c;
    c.angleDeg = deg;
    for (const auto &r : recs) {
      if (std::fabs(std::remainder(deg - r.angleDeg, 360.0)) <= 15.0) {
        c.isRecommended = true;
        break;
      }
    }
    const SectionLines s =
        sliceSectionY0(meshToSlice, inferInnerWall_, estimatedWallMm_, deg);
    const double H = dims_.vesselHeight > 0.0
                         ? dims_.vesselHeight
                         : (meshToSlice.bboxMax.z - meshToSlice.bboxMin.z);
    if (!s.loops.empty()) {
      c.outerPts = s.loops.front();
      double yLo = c.outerPts.front().y, yHi = yLo;
      for (const auto &p : c.outerPts) {
        yLo = std::min(yLo, p.y);
        yHi = std::max(yHi, p.y);
      }
      const bool partialWall = H > 1.0 && (yHi - yLo) < 0.65 * H;
      if (!s.estInner.empty()) {
        c.extPts = s.estInner.front();
        c.hasWarn = true;
        c.statusLabel = QStringLiteral("파선보정");
      } else if (partialWall) {
        c.hasWarn = true;
        c.isRecommended = false;
        c.statusLabel = QStringLiteral("결실구간");
      } else {
        c.statusLabel =
            c.isRecommended ? QStringLiteral("★추천") : QStringLiteral("양호");
      }
    } else if (!s.solids.empty()) {
      c.outerPts = s.solids.front();
      if (!s.estInner.empty())
        c.extPts = s.estInner.front();
      c.hasWarn = true;
      c.statusLabel = QStringLiteral("결실·보정");
    } else {
      c.hasError = true;
      c.statusLabel = QStringLiteral("끊김");
    }
    cells.push_back(std::move(c));
  }
  return cells;
}

void ArtifactDoc::setSlotHorizontalZ(int slotIdx, double zMm) {
  if (!isLoaded() || slotIdx < 0 || slotIdx >= int(slotList_.size()))
    return;
  auto &s = slotList_[size_t(slotIdx)];
  if (s.kind != SlotKind::Horizontal)
    return;
  const double maxH =
      d5_.height > 1.0 ? d5_.height : (base_.bboxMax.z - base_.bboxMin.z);
  s.z = std::clamp(zMm, 0.5, std::max(1.0, maxH - 0.5));
  s.note = QStringLiteral("Z 막 높이 H %1 mm").arg(s.z, 0, 'f', 1);
}

int ArtifactDoc::addVerticalSlotAt(double angleDeg) {
  if (!isLoaded())
    return -1;
  const double normAng = std::remainder(angleDeg, 360.0);
  int nv = 0;
  for (const auto &s : slotList_)
    if (s.kind == SlotKind::Vertical)
      ++nv;
  const QChar c = QChar('A' + std::min(24, int(slotList_.size())));
  const QString memo =
      QStringLiteral("막 %1°").arg(normAng >= 0 ? normAng : normAng + 360.0, 0,
                                  'f', 0);
  slotList_.push_back(
      {QStringLiteral("%1–%1′ 종단면 (%2)").arg(c).arg(memo),
       QStringLiteral("종단면"),
       nv == 0 ? QStringLiteral("보고서 주 도판 (1:1)") : memo,
       SlotKind::Vertical, normAng, 0.0});
  return int(slotList_.size()) - 1;
}

int ArtifactDoc::addHorizontalSlotAt(double zMm) {
  if (!isLoaded())
    return -1;
  const double maxH =
      d5_.height > 1.0 ? d5_.height : (base_.bboxMax.z - base_.bboxMin.z);
  const double z = std::clamp(zMm, 0.5, std::max(1.0, maxH - 0.5));
  const QChar c = QChar('A' + std::min(24, int(slotList_.size())));
  slotList_.push_back({QStringLiteral("%1–%1′ 횡단면").arg(c),
                       QStringLiteral("수평절단"),
                       QStringLiteral("높이 H %1 mm 절단").arg(z, 0, 'f', 1),
                       SlotKind::Horizontal, 0.0, z});
  return int(slotList_.size()) - 1;
}

bool ArtifactDoc::alignCutToSurfacePoint(const KA::Point3 &worldPt) {
  if (!isLoaded() || !poseConfirmed_)
    return false;
  const double r = std::hypot(worldPt.x, worldPt.y);
  if (r < 1e-3)
    return false;
  // 현재 mesh_ 는 base_ 를 +frontAngle_ 만큼 회전한 상태이므로,
  // 클릭한 점(worldPt)을 절단면 Y=0 위에 놓는 새 정면각을 계산한다.
  const double curAng = previewAngle();
  const double ptDeg = std::atan2(worldPt.y, worldPt.x) * (180.0 / std::numbers::pi);
  // 클릭한 위치가 좌측(x < 0)이면 180° 축에, 우측(x >= 0)이면 0° 축에 놓아 시점 급반전을 막는다.
  const double targetAxis = (worldPt.x < 0.0) ? 180.0 : 0.0;
  const double deltaDeg = targetAxis - ptDeg;
  const double newFront = std::remainder(curAng + deltaDeg, 360.0);
  const double H = std::max(1.0, base_.bboxMax.z - base_.bboxMin.z);
  cutMarkerZ_ = std::clamp(worldPt.z, 1.0, H - 1.0);
  hasCutMarker_ = true;
  setFrontAngle(newFront);
  return true;
}

KA::Point3 ArtifactDoc::cutMarker3D() const {
  if (!isLoaded())
    return {0.0, 0.0, 0.0};
  const double z = cutMarkerZ_;
  double r = outerRadiusAt(z);
  if (r <= 0.0) {
    r = 0.5 * std::max(d5_.belly, 40.0);
  }
  return {sectionSign() * r, 0.0, z};
}

void ArtifactDoc::autoInitRubbingSpot() {
  if (!isLoaded())
    return;
  const double H = std::max(1.0, base_.bboxMax.z - base_.bboxMin.z);
  const double maxR = std::max({std::fabs(base_.bboxMin.x), std::fabs(base_.bboxMax.x),
                                std::fabs(base_.bboxMin.y), std::fabs(base_.bboxMax.y)});
  rubbingSpanW_ = std::clamp(0.68 * maxR, 32.0, 66.0);
  rubbingSpanH_ = std::clamp(0.30 * H, 28.0, 56.0);
  // 정면 좌측 외면(Quadrant 3: 195°~215°) 상부 경부/어깨 문양대(0.78 H)를 기본 정밀 탁본 영역으로 설정
  rubbingBaseAzimuthDeg_ = 202.0;
  rubbingZ_ = std::clamp(0.78 * H, rubbingSpanH_ * 0.5, H - rubbingSpanH_ * 0.35);
  rubbingAzSpanDeg_ = -1.0;
  hasRubbingSpot_ = true;

  // 기본 절단 확인 표지도 상부 경부/어깨(0.72 H) 절단선 위에 함께 표시하여 3D↔2D 절단 대응을 직관화
  cutMarkerZ_ = std::clamp(0.72 * H, 1.0, H - 1.0);
  hasCutMarker_ = true;
}

bool ArtifactDoc::selectRubbingSpot(const KA::Point3 &worldPt) {
  if (!isLoaded() || !poseConfirmed_)
    return false;
  const double r = std::hypot(worldPt.x, worldPt.y);
  if (r < 1e-3)
    return false;
  const double H = std::max(1.0, base_.bboxMax.z - base_.bboxMin.z);
  const double maxR = std::max({std::fabs(base_.bboxMin.x), std::fabs(base_.bboxMax.x),
                                std::fabs(base_.bboxMin.y), std::fabs(base_.bboxMax.y)});
  rubbingSpanW_ = std::clamp(0.68 * maxR, 32.0, 66.0);
  rubbingSpanH_ = std::clamp(0.30 * H, 28.0, 56.0);

  double ptDeg = std::atan2(worldPt.y, worldPt.x) * (180.0 / std::numbers::pi);
  if (ptDeg < 0.0)
    ptDeg += 360.0;
  rubbingBaseAzimuthDeg_ =
      std::fmod(std::fmod(ptDeg - frontAngle_, 360.0) + 360.0, 360.0);
  rubbingZ_ = std::clamp(worldPt.z, rubbingSpanH_ * 0.35, H - rubbingSpanH_ * 0.25);
  rubbingAzSpanDeg_ = -1.0;
  hasRubbingSpot_ = true;
  return true;
}

double ArtifactDoc::rubbingAzimuthDeg() const {
  return std::fmod(std::fmod(rubbingBaseAzimuthDeg_ + frontAngle_, 360.0) + 360.0,
                   360.0);
}

RubbingRegion ArtifactDoc::rubbingRegion() const {
  RubbingRegion r;
  r.azCenterDeg = std::fmod(std::fmod(rubbingBaseAzimuthDeg_, 360.0) + 360.0, 360.0);
  r.zLo = rubbingZ_ - 0.5 * rubbingSpanH_;
  r.zHi = rubbingZ_ + 0.5 * rubbingSpanH_;
  if (rubbingAzSpanDeg_ > 0.0) {
    r.azSpanDeg = rubbingAzSpanDeg_;
  } else { // 점으로 고른 범위: 폭(mm)을 그 높이 외면 반경으로 방위각 폭으로 환산
    const double rr = outerRadiusAt(rubbingZ_);
    const double R = rr > 5.0 ? rr : std::max(5.0, 0.5 * d5_.belly);
    r.azSpanDeg = std::clamp(rubbingSpanW_ / R * (180.0 / std::numbers::pi), 1.0, 360.0);
  }
  return r;
}

void ArtifactDoc::setRubbingRegion(const RubbingRegion &r) {
  if (!isLoaded())
    return;
  const double zMin = base_.bboxMin.z, zMax = base_.bboxMax.z;
  const double lo = std::clamp(std::min(r.zLo, r.zHi), zMin, zMax);
  const double hi = std::clamp(std::max(r.zLo, r.zHi), zMin, zMax);
  rubbingSpanH_ = std::max(1.0, hi - lo);
  rubbingZ_ = std::min(zMax, lo + 0.5 * rubbingSpanH_);
  rubbingBaseAzimuthDeg_ = std::fmod(std::fmod(r.azCenterDeg, 360.0) + 360.0, 360.0);
  rubbingAzSpanDeg_ = std::clamp(r.azSpanDeg, 1.0, 360.0);
  const double rr = outerRadiusAt(rubbingZ_);
  const double R = rr > 5.0 ? rr : std::max(5.0, 0.5 * d5_.belly);
  rubbingSpanW_ = R * rubbingAzSpanDeg_ * std::numbers::pi / 180.0;
  hasRubbingSpot_ = true;
}

void ArtifactDoc::setRubbingFullTurn() {
  if (!isLoaded())
    return;
  RubbingRegion r = rubbingRegion();
  r.azSpanDeg = 360.0;
  setRubbingRegion(r);
}

void ArtifactDoc::setRubbingPreset(RubbingPreset p) {
  if (rubbingPreset_ == p)
    return;
  rubbingPreset_ = p;
  rubbingCache_.valid = false;
}

void ArtifactDoc::setRubbingProjMode(RubbingProjMode m) {
  if (rubbingProjMode_ == m)
    return;
  rubbingProjMode_ = m;
  rubbingCache_.valid = false;
}

QString ArtifactDoc::defaultHistoryPath() const {
  if (path_.isEmpty())
    return {};
  const QFileInfo fi(path_);
  return fi.dir().filePath(fi.completeBaseName() + QStringLiteral(".darch"));
}

bool ArtifactDoc::saveHistory(const QString &darchPath) const {
  if (!isLoaded())
    return false;
  const QString outPath = darchPath.isEmpty() ? defaultHistoryPath() : darchPath;
  if (outPath.isEmpty())
    return false;
  QJsonObject root;
  root[QStringLiteral("format")] = QStringLiteral("DongArch3D.History.v1");
  root[QStringLiteral("artifactName")] = name_;
  root[QStringLiteral("frontAngle")] = frontAngle_;
  root[QStringLiteral("sectionSide")] =
      (side_ == SectionSide::Left) ? QStringLiteral("Left") : QStringLiteral("Right");
  root[QStringLiteral("heightRef")] =
      (heightRef_ == HeightRef::Rim) ? QStringLiteral("Rim") : QStringLiteral("Base");
  root[QStringLiteral("simplifyTol")] = simplifyTol_;
  root[QStringLiteral("smooth")] = smooth_;
  root[QStringLiteral("inferInnerWall")] = inferInnerWall_;
  root[QStringLiteral("estimatedWallMm")] = estimatedWallMm_;
  root[QStringLiteral("rubbingPreset")] = int(rubbingPreset_);
  root[QStringLiteral("rubbingProjMode")] = int(rubbingProjMode_);
  root[QStringLiteral("hasRubbingSpot")] = hasRubbingSpot_;
  if (hasRubbingSpot_) {
    const RubbingRegion reg = rubbingRegion();
    QJsonObject rObj;
    rObj[QStringLiteral("azCenterDeg")] = reg.azCenterDeg;
    rObj[QStringLiteral("azSpanDeg")] = reg.azSpanDeg;
    rObj[QStringLiteral("zLo")] = reg.zLo;
    rObj[QStringLiteral("zHi")] = reg.zHi;
    root[QStringLiteral("rubbingRegion")] = rObj;
  }
  QJsonArray slotsArr;
  for (const auto &s : slotList_) {
    QJsonObject sObj;
    sObj[QStringLiteral("name")] = s.name;
    sObj[QStringLiteral("meta")] = s.meta;
    sObj[QStringLiteral("note")] = s.note;
    sObj[QStringLiteral("kind")] = int(s.kind);
    sObj[QStringLiteral("angleDeg")] = s.angleDeg;
    sObj[QStringLiteral("z")] = s.z;
    slotsArr.append(sObj);
  }
  root[QStringLiteral("slots")] = slotsArr;

  QFile f(outPath);
  if (!f.open(QIODevice::WriteOnly | QIODevice::Truncate))
    return false;
  f.write(QJsonDocument(root).toJson(QJsonDocument::Indented));
  return true;
}

bool ArtifactDoc::loadHistory(const QString &darchPath) {
  if (!isLoaded())
    return false;
  const QString inPath = darchPath.isEmpty() ? defaultHistoryPath() : darchPath;
  if (inPath.isEmpty() || !QFileInfo::exists(inPath))
    return false;
  QFile f(inPath);
  if (!f.open(QIODevice::ReadOnly))
    return false;
  const QJsonDocument doc = QJsonDocument::fromJson(f.readAll());
  if (!doc.isObject())
    return false;
  const QJsonObject root = doc.object();
  if (root[QStringLiteral("format")].toString() !=
      QStringLiteral("DongArch3D.History.v1"))
    return false;
  if (!poseConfirmed_) {
    autoAlign();
    confirmPose();
  }
  side_ = (root[QStringLiteral("sectionSide")].toString() == QStringLiteral("Left"))
              ? SectionSide::Left
              : SectionSide::Right;
  heightRef_ = (root[QStringLiteral("heightRef")].toString() == QStringLiteral("Rim"))
                   ? HeightRef::Rim
                   : HeightRef::Base;
  simplifyTol_ = root[QStringLiteral("simplifyTol")].toDouble(simplifyTol_);
  smooth_ = root[QStringLiteral("smooth")].toBool(smooth_);
  inferInnerWall_ = root[QStringLiteral("inferInnerWall")].toBool(inferInnerWall_);
  estimatedWallMm_ = root[QStringLiteral("estimatedWallMm")].toDouble(estimatedWallMm_);
  rubbingPreset_ = RubbingPreset(
      std::clamp(root[QStringLiteral("rubbingPreset")].toInt(0), 0, 4));
  rubbingProjMode_ = RubbingProjMode(
      std::clamp(root[QStringLiteral("rubbingProjMode")].toInt(0), 0, 1));

  if (root.contains(QStringLiteral("slots")) &&
      root[QStringLiteral("slots")].isArray()) {
    const QJsonArray arr = root[QStringLiteral("slots")].toArray();
    if (!arr.isEmpty()) {
      slotList_.clear();
      for (const auto &v : arr) {
        if (!v.isObject())
          continue;
        const QJsonObject sObj = v.toObject();
        SectionSlot s;
        s.name = sObj[QStringLiteral("name")].toString();
        s.meta = sObj[QStringLiteral("meta")].toString();
        s.note = sObj[QStringLiteral("note")].toString();
        s.kind = SlotKind(std::clamp(sObj[QStringLiteral("kind")].toInt(0), 0, 1));
        s.angleDeg = sObj[QStringLiteral("angleDeg")].toDouble(0.0);
        s.z = sObj[QStringLiteral("z")].toDouble(0.0);
        slotList_.push_back(std::move(s));
      }
    }
  }
  setFrontAngle(root[QStringLiteral("frontAngle")].toDouble(0.0));
  if (root[QStringLiteral("hasRubbingSpot")].toBool(false) &&
      root[QStringLiteral("rubbingRegion")].isObject()) {
    const QJsonObject rObj = root[QStringLiteral("rubbingRegion")].toObject();
    RubbingRegion reg;
    reg.azCenterDeg = rObj[QStringLiteral("azCenterDeg")].toDouble(0.0);
    reg.azSpanDeg = rObj[QStringLiteral("azSpanDeg")].toDouble(40.0);
    reg.zLo = rObj[QStringLiteral("zLo")].toDouble(0.0);
    reg.zHi = rObj[QStringLiteral("zHi")].toDouble(40.0);
    setRubbingRegion(reg);
  }
  rubbingCache_.valid = false;
  return true;
}

const UnrolledRubbing &ArtifactDoc::unrolledRubbing(double pxPerMm) const {
  static const UnrolledRubbing kEmpty;
  if (!isLoaded() || !poseConfirmed_ || !hasRubbingSpot_)
    return kEmpty;
  const RubbingRegion reg = rubbingRegion();
  auto &c = rubbingCache_;
  const auto same = [](const KA::Point3 &a, const KA::Point3 &b) {
    return a.x == b.x && a.y == b.y && a.z == b.z;
  };
  const KA::Point3 v0 = base_.vertices.empty() ? KA::Point3{} : base_.vertices.front();
  if (c.valid && c.region == reg && c.pxPerMm == pxPerMm &&
      c.preset == rubbingPreset_ && c.projMode == rubbingProjMode_ &&
      c.vertexCount == base_.vertices.size() && same(c.bboxMin, base_.bboxMin) &&
      same(c.bboxMax, base_.bboxMax) && same(c.firstVertex, v0))
    return c.result;
  RubbingParams p;
  p.pxPerMm = pxPerMm;
  p.preset = rubbingPreset_;
  p.projMode = rubbingProjMode_;
  p.maxSidePx = 6000; // 화면용 (저장은 saveRubbingPng 가 실제 dpi 로 다시 계산)
  const double rr = outerRadiusAt(0.5 * (reg.zLo + reg.zHi));
  p.fallbackRadiusMm = rr > 5.0 ? rr : 0.5 * d5_.belly;
  try {
    c.result = renderUnrolledRubbing(base_, reg, p);
  } catch (const std::exception &) { // 메모리 부족 — 도판은 탁본 없이 그린다
    c.result = UnrolledRubbing{};
  }
  c.region = reg;
  c.pxPerMm = pxPerMm;
  c.preset = rubbingPreset_;
  c.projMode = rubbingProjMode_;
  c.vertexCount = base_.vertices.size();
  c.bboxMin = base_.bboxMin;
  c.bboxMax = base_.bboxMax;
  c.firstVertex = v0;
  c.valid = true;
  return c.result;
}

KA::Point3 ArtifactDoc::rubbingCenter3D() const {
  if (!isLoaded())
    return {0.0, 0.0, 0.0};
  const double ang = rubbingAzimuthDeg() * (std::numbers::pi / 180.0);
  double r = outerRadiusAt(rubbingZ_);
  if (r <= 0.0)
    r = 0.45 * std::max(d5_.belly, 50.0);
  return {r * std::cos(ang), r * std::sin(ang), rubbingZ_};
}

} // namespace studio
