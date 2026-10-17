#include "ArtifactDoc.h"

#include "AxisOdr.h"
#include "AxisPca.h"
#include "MeshIO.h"
#include "MeshLoader.h"
#include "RimDetector.h"
#include "SyntheticMeshGen.h"

#include <QFileInfo>
#include <algorithm>
#include <cmath>
#include <limits>
#include <numbers>
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

// 방향 d 를 +Z 로 보내는 회전 (Rodrigues)
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

// 각도 deg 의 cos·sin. 90° 정수배는 정확히 {−1, 0, 1} (반복 회전 누적 오차 방지)
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

// 막과 수직인 실루엣의 외면 반쪽(디자인 v2 「도면 선 규약」 외곽선 = 외면 반쪽의 윤곽): 높이 칸
// zk = z0 + (k + 0.5)·bin (k = 0, 1, …, zk <= z1 인 것 모두, z0·z1 = m.bboxMin.z·bboxMax.z) 마다 삼각형 ∩ 수평면
// 교점 중 x <= 0 인 것의 최소 x. 맞는 점이 없는 칸은 비운다 — 복사·보간·평활 없음(실측 그대로). 면마다 그 면의
// z 범위에 걸치는 칸만 돈다. 교점 규칙은 K-Arch addTriZHits(Silhouette.cpp:171-189)와 같다.
std::vector<KA::Point2> halfOrthoSilhouette(const KA::MeshData &m, double bin) {
  std::vector<KA::Point2> out;
  if (m.vertices.empty() || m.faces.empty() || bin <= 0)
    return out;
  const double z0 = m.bboxMin.z, z1 = m.bboxMax.z;
  long n = 0;
  while (z0 + (double(n) + 0.5) * bin <= z1)
    ++n;
  constexpr double kInf = std::numeric_limits<double>::infinity();
  std::vector<double> best(size_t(n), kInf);
  for (const auto &f : m.faces) {
    if (f[0] < 0 || f[1] < 0 || f[2] < 0)
      continue;
    const KA::Point3 p[3] = {m.vertices[size_t(f[0])], m.vertices[size_t(f[1])],
                             m.vertices[size_t(f[2])]};
    const double lo = std::min({p[0].z, p[1].z, p[2].z}),
                 hi = std::max({p[0].z, p[1].z, p[2].z});
    const long k0 = std::max(0L, long(std::floor((lo - z0) / bin - 0.5)) - 1);
    const long k1 = std::min(n - 1, long(std::ceil((hi - z0) / bin - 0.5)) + 1);
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
        const double t = (zk - u.z) / dz;
        if (t > 0.0 && t <= 1.0) {
          const double x = u.x + t * (v.x - u.x);
          if (x <= 0.0)
            mn = std::min(mn, x);
        }
      }
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

// 높이 zs[k](오름차순)마다 그 높이에 걸치는 면 번호 — 면 순서 그대로. 걸침 = 세 꼭짓점 z 범위 ±1e-6 mm 가 zs[k] 를
// 품음(K-Arch Slice 가 그 높이에서 선분을 만들 수 있는 면은 모두 들어가고, 남는 면은 선분을 만들지 않는다).
std::vector<std::vector<int>> zBandFaces(const KA::MeshData &m,
                                         const std::vector<double> &zs) {
  std::vector<std::vector<int>> out(zs.size());
  const long nv = long(m.vertices.size());
  for (size_t f = 0; f < m.faces.size(); ++f) {
    const auto &F = m.faces[f];
    if (F[0] < 0 || F[1] < 0 || F[2] < 0 || F[0] >= nv || F[1] >= nv || F[2] >= nv)
      continue; // K-Arch Slice 가 건너뛰는 면 (MeshSlicer.cpp:110-116)
    const double a = m.vertices[size_t(F[0])].z, b = m.vertices[size_t(F[1])].z,
                 c = m.vertices[size_t(F[2])].z;
    const auto lo = std::lower_bound(zs.begin(), zs.end(), std::min({a, b, c}) - 1e-6);
    const auto hi = std::upper_bound(zs.begin(), zs.end(), std::max({a, b, c}) + 1e-6);
    for (auto it = lo; it < hi; ++it)
      out[size_t(it - zs.begin())].push_back(int(f));
  }
  return out;
}

// 면 목록만의 작은 메시. 원본 바운딩 박스 두 모서리를 면 없이 넣어 K-Arch Slice 의 이음 허용치(1e-6 × 정점
// 바운딩 박스 대각선, MeshSlicer.cpp:103-106)를 원본과 같게 한다 → 원본 전체를 자른 것과 같은 고리.
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

// 높이 z 수평 단면의 기벽 두께 = 평균 반경이 가장 큰 고리 − 두 번째 고리, 고리가 하나뿐이면 −1
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
  // 곡선 점의 허용 범위: raw 의 높이 범위, raw 가 축 한쪽이면 그쪽(옆 방향 극값은 막지 않음)
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
        return 0.5 * (2 * b + (-a + cc) * t + (2 * a - 5 * b + 4 * cc - d) * t2 +
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
    out.insert(out.end(), piece.begin(), ok ? piece.end() - 1 : piece.begin() + 1);
  }
  if (!closed)
    out.push_back(c.back());
  return out;
}

void ArtifactDoc::loadMesh(KA::MeshData m, std::vector<QRgb> colors,
                           const QString &name) {
  base_ = std::move(m);
  KA::FillMeshStats(base_);
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
  unconfirm();
  centered_ = false;
}

bool ArtifactDoc::loadFile(const QString &path, QString *err) {
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
  loadMesh(std::move(m), std::move(cols),
           QStringLiteral("예제_합성토기_01호"));
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
  poseConfirmed_ = false;
  slotList_.clear();
  recompute();
}

void ArtifactDoc::confirmPose() {
  if (!isLoaded())
    return;
  if (centered_) {
    // 자동 세우기 직후: 중심 맞춤·접지가 이미 끝났다 (예전 한 번 누르기와 같은
    // 순서·같은 값)
    aligned_ = pendingAligned_;
    alignNote_ = pendingNote_;
    axisRmse_ = pendingRmse_;
  } else {
    // 사람이 세운 자세 그대로(회전 없음), 중심선(수평 외곽 원 중심 평균)을
    // 원점으로 — autoAlign 마지막 단계와 같은 규칙
    const auto fits = KA::FitHorizontalOuters(base_, zBand(base_, 12));
    double sx = 0, sy = 0;
    int n = 0;
    for (const auto &f : fits)
      if (f.circle.ok)
        sx += f.circle.xc, sy += f.circle.yc, ++n;
    if (n > 0)
      shiftBoth(-sx / n, -sy / n, 0);
    else
      shiftBoth(-0.5 * (base_.bboxMin.x + base_.bboxMax.x),
                -0.5 * (base_.bboxMin.y + base_.bboxMax.y), 0);
    aligned_ = n > 0;
    alignNote_ = n > 0
                     ? QStringLiteral("바닥판 기준 (사람 확정) · 중심선 원 맞춤")
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
  frontAngle_ = 0.0;
  poseConfirmed_ = true;
  // 미리보기(끄는 동안)·3D 보조창용 LOD 를 확정 자세로 다시 만든다 (치수는 원본에서)
  buildLodMesh(base_, baseColors_, lodBase_, lodColors_, 35000);
  resetSlots();
  recompute();
}

void ArtifactDoc::autoAlign() {
  if (!isLoaded())
    return;
  KA::MeshData m = base_;
  KA::FillMeshStats(m);
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

  // 1) 구연부 → PCA 로 입구를 위로
  const KA::RimResult rim = KA::DetectRim(work, KA::RimMode::Auto);
  if (rim.error == KA::RimError::Ok) {
    const auto pca =
        KA::ComputePcaAlignment(rim.rimPoints, KA::MeshCentroid(work));
    if (pca.ok) {
      for (auto &p : work.vertices)
        p = pca.transform.apply(p);
      KA::FillMeshStats(work);
      if (useProxy) {
        for (auto &p : m.vertices)
          p = pca.transform.apply(p);
        KA::FillMeshStats(m);
      }
    }
  }
  // 2) 수평 외곽 원들로 회전 중심축 정밀화
  const KA::Axis3D ax =
      KA::OptimizeRotationAxis(KA::FitHorizontalOuters(work, zBand(work, 24)));
  if (ax.ok) {
    translate(work, -ax.origin.x, -ax.origin.y, -ax.origin.z);
    rotateDirToZ(work, ax.direction);
    KA::FillMeshStats(work);
    if (useProxy) {
      translate(m, -ax.origin.x, -ax.origin.y, -ax.origin.z);
      rotateDirToZ(m, ax.direction);
      KA::FillMeshStats(m);
    }
    const auto fits = KA::FitHorizontalOuters(work, zBand(work, 12));
    double sx = 0, sy = 0;
    int n = 0;
    for (const auto &f : fits)
      if (f.circle.ok)
        sx += f.circle.xc, sy += f.circle.yc, ++n;
    if (n > 0) {
      translate(m, -sx / n, -sy / n, 0);
    }
    aligned = true;
    rmse = ax.rmse;
    note = rim.error == KA::RimError::Ok
               ? QStringLiteral("바닥 수평(0 mm) · 회전 중심축 정렬됨")
               : QStringLiteral("회전 중심축 정렬됨 (구연부 미검출)");
  } else {
    const double cx = 0.5 * (m.bboxMin.x + m.bboxMax.x),
                 cy = 0.5 * (m.bboxMin.y + m.bboxMax.y);
    translate(m, -cx, -cy, 0);
    note = QStringLiteral("중심축 미정렬 — [중심축 맞춤] 필요");
  }
  if (!unitNote_.isEmpty())
    note += QStringLiteral(" · ") + unitNote_;
  if (removedNoiseFaces_ > 0)
    note += QStringLiteral(" · 부유노이즈 %1면 제거").arg(removedNoiseFaces_);

  KA::FillMeshStats(m);
  translate(m, 0, 0, -m.bboxMin.z); // 바닥 H 0
  KA::FillMeshStats(m);
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
  recompute(false); // +Z 회전은 5대 치수·잰 기벽을 바꾸지 않는다 — 확정 때 값 (단일 쉘의 추정 기벽만 각 단면을 따름)
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
  KA::FillMeshStats(mesh_);
  KA::FillMeshStats(lodMesh_);

  // 그리는 외곽선 = 막과 수직인 실루엣의 외면 반쪽, 늘 원본에서(LOD 아님). MaxRadius(2·98 백분위 · 칸 버림 · 빈 칸
  // 복사 · 평활)는 끝이 기준선에 못 미치고 급한 저부에서 선을 지어냈고, K-Arch TrueOrtho 는 x > 0 쪽까지 보고 평활해
  // 기운 바닥·구연에서 중심선을 넘었다(JOB-008).
  ortho_ = halfOrthoSilhouette(mesh_, 0.25);

  if (measureDims) {
    // 5대 치수·기벽은 늘 원본 메시에서(LOD 는 3.8 mm 칸 정점 평균이라 바닥판 높이·기벽 고리가 무너져 25만 면 초과
    // 메시에서 저경 5.2·기벽 3.85 mm 로 틀렸다, JOB-009). K-Arch 실루엣은 치수에 maxBodyZ 대체값(최대 반경 정점이
    // 늘 있어 쓰이지 않음)으로만 들어가므로 계산하지 않는다.
    dims_ = KA::AnalyzeDimensions(mesh_, KA::SilhouetteResult{});
    d5_ = {};
    d5_.ok = dims_.ok;
    d5_.height = dims_.vesselHeight;
    d5_.rim = dims_.rimDiameter;
    d5_.belly = dims_.maxBodyDiameter;
    d5_.bellyZ = dims_.maxBodyZ - mesh_.bboxMin.z;
    d5_.base = dims_.baseDiameter;

    // 기벽 두께 프로파일: 수평 단면을 높이별로 잘라 바깥 고리 - 안쪽 고리 (둘레
    // 평균). 높이마다 그 높이에 걸치는 원본 면만 자른다(결과는 원본 전체를 자른 것과 같다)
    const double h = mesh_.bboxMax.z - mesh_.bboxMin.z;
    wallZ0_ = mesh_.bboxMin.z;
    wallStep_ = std::max(1.0, h / 160.0);
    std::vector<double> zs;
    for (double z = wallZ0_; z <= mesh_.bboxMax.z; z += wallStep_) // 누적 합 높이 — JOB-008 까지와 같은 z 값
      zs.push_back(z);
    const auto bands = zBandFaces(mesh_, zs);
    wallProfile_.clear();
    for (size_t i = 0; i < zs.size(); ++i)
      wallProfile_.push_back(zs[i] <= wallZ0_ + 0.02 * h ||
                                     zs[i] >= mesh_.bboxMax.z - 0.02 * h
                                 ? -1.0
                                 : wallBySlice(bandMesh(mesh_, bands[i]), zs[i]));

    double sum = 0;
    int n = 0;
    for (size_t i = 0; i < wallProfile_.size(); ++i) {
      const double z = wallZ0_ + double(i) * wallStep_;
      if (z < wallZ0_ + 0.1 * h || z > wallZ0_ + 0.9 * h || wallProfile_[i] <= 0)
        continue;
      sum += wallProfile_[i], ++n;
    }
    wallMean_ = n ? sum / n : -1.0;
  }
  // 확정 때 잰 평균 기벽, 두 고리를 못 잡은 단일 쉘(wallMean_ ≤ 0)이면 각 단면이 갱신하는 추정값
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

// ---- 화면용 선: 자연 좌표에서 간략화·곡선 → 마지막에 좌/우 부호 (D5) ----------
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
  // 외곽(반쪽 정사 실루엣, x ≤ 0)도 단면과 같은 부호: Right 는 그대로 왼쪽, Left 는 오른쪽
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
  lines_ = sliceSectionY0(mesh_, inferInnerWall_, estimatedWallMm_);
  if (lines_.wallMm > 0)
    estimatedWallMm_ = lines_.wallMm; // 미스캔 보정 경로가 잰 실측 기벽
}

SectionLines sliceSectionY0(const KA::MeshData &m, bool inferInner,
                            double wallSeedMm) {
  SectionLines s;
  std::vector<std::vector<KA::Point2>>
      openBranches; // 각 가지: 위(zHi) → 아래(zLo) 순서

  for (const auto &pl : KA::Slice(m, KA::Plane{{0, 1, 0}, 0.0})) {
    if (pl.pts.size() < 3)
      continue;
    std::vector<KA::Point2> in;
    in.reserve(pl.pts.size());
    for (const auto &p : pl.pts)
      in.push_back({p.x, p.z});

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

  if (!inferInner || !s.loops.empty() || openBranches.empty())
    return s;

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
  if (!s.solids.empty() && !s.estInner.empty()) { // 미스캔 보정: 실측 실선 + 추정 내벽선
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

} // namespace studio
