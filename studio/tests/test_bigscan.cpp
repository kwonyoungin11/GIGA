// 헤드리스 검증: 큰 스캔(25만 면 초과)의 치수를 원본에서 (JOB-009 수용 기준 1-6, 판정 꼬리표 [J9-n])
// 열린 입술 25만·34.6만·50만·100만 면, 기운 채 자동 세우기(34.6만 면 7°), 닫힌 입술 34.7만·100만 면 합성 예제의
// 5대 치수가 같은 모양 7.7만 면 예제와 ±0.1 mm 안(K-Arch RoundTenths 보고 단위)인지, 문서 치수가 원본 메시의
// 독립 계산(K-Arch AnalyzeDimensions·원본 전체 수평 단면 기벽)과 같은지, 정면각을 바꿔도 치수가 그대로인지,
// 바닥선·구연선 폭이 원본 치수인지 판정하고, 확정·놓기 시간은 기록만 한다(공식 기준 없음).
// 시험 쪽 도움 함수(전체 단면 기벽·평균 기벽)는 제품 코드를 쓰지 않고 이 파일 안에 따로 둔다.
#include "ArtifactDoc.h"
#include "DimensionAnalyzer.h"
#include "MeshLoader.h"
#include "MeshSlicer.h"
#include "Plate.h"
#include "RimDetector.h"
#include "SyntheticMeshGen.h"

#include <QGuiApplication>
#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <functional>
#include <vector>

using namespace studio;
static int fails = 0;
#define CHECK(c, ...)                                                                                                  \
  do {                                                                                                                 \
    if (!(c)) {                                                                                                        \
      std::printf("FAIL %s: ", #c);                                                                                    \
      std::printf(__VA_ARGS__);                                                                                        \
      std::printf("\n");                                                                                               \
      ++fails;                                                                                                         \
    }                                                                                                                  \
  } while (0)
using clk = std::chrono::steady_clock;
static double msSince(clk::time_point t) { return std::chrono::duration<double, std::milli>(clk::now() - t).count(); }

// 높이 z 수평 단면(원본 전체)의 기벽 = 점 3개 이상 고리의 평균 반경 1·2위 차, 고리가 2개 미만이면 −1
static double wallAt(const KA::MeshData &m, double z) {
  std::vector<double> rs;
  for (const auto &l : KA::Slice(m, KA::Plane{{0, 0, 1}, -z})) {
    if (l.pts.size() < 3) continue;
    double s = 0;
    for (const auto &p : l.pts) s += std::hypot(p.x, p.y);
    rs.push_back(s / double(l.pts.size()));
  }
  if (rs.size() < 2) return -1.0;
  std::sort(rs.begin(), rs.end(), std::greater<>());
  return rs[0] - rs[1];
}
// 평균 기벽 — 높이는 zr(원본)의 바닥부터 max(1, h/160) 누적, 위·아래 2 % 는 빼고, 10–90 % 의 양수만 평균. 자르는 메시는 m
static double meanWallFull(const KA::MeshData &zr, const KA::MeshData &m) {
  const double h = zr.bboxMax.z - zr.bboxMin.z, z0 = zr.bboxMin.z, st = std::max(1.0, h / 160.0);
  std::vector<double> prof;
  for (double z = z0; z <= zr.bboxMax.z; z += st)
    prof.push_back(z <= z0 + 0.02 * h || z >= zr.bboxMax.z - 0.02 * h ? -1.0 : wallAt(m, z));
  double sum = 0;
  int n = 0;
  for (size_t i = 0; i < prof.size(); ++i) {
    const double z = z0 + double(i) * st;
    if (z < z0 + 0.1 * h || z > z0 + 0.9 * h || prof[i] <= 0) continue;
    sum += prof[i], ++n;
  }
  return n ? sum / n : -1.0;
}
// 예제와 같은 모양(기고 184.5 · 구경 142 · 최대경 165.2 · 기벽 6.8 mm, 잡음 σ 0.05, 시드 42)의 해상도만 다른 메시
static KA::MeshData vessel(int rs, int hs, bool capTop = false, double tilt = 0, KA::Point3 tr = {0, 0, 0}) {
  KA::Testing::SyntheticParams sp;
  sp.radialSeg = rs;
  sp.heightSeg = hs;
  sp.capTop = capTop;
  sp.capBottom = true;
  sp.tiltDeg = tilt;
  sp.translation = tr;
  sp.noiseSigma = 0.05;
  KA::MeshData m = KA::Testing::MakeTorusProfileVessel(82.6, 71.0, 184.5, 6.8, sp).toMesh();
  KA::FillMeshStats(m);
  return m;
}
static bool near(double a, double b, double t) { return std::fabs(a - b) <= t + 1e-9; }
static bool near5(const Dims5 &d, const Dims5 &r) {
  return near(d.height, r.height, 0.1) && near(d.rim, r.rim, 0.1) && near(d.belly, r.belly, 0.1) && near(d.base, r.base, 0.1) &&
         near(d.meanWall, r.meanWall, 0.1);
}
static double rawH(const ArtifactDoc &h) { return h.rawDims().zMax - h.rawDims().zMin; }

int main(int argc, char **argv) {
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  qputenv("QT_QPA_PLATFORM", "offscreen");
  QGuiApplication app(argc, argv);
  const auto T0 = clk::now();

  // 기준: 7.7만 면 예제 (자동 세우기 → 확정)
  ArtifactDoc s;
  s.loadSample(false);
  s.autoAlign();
  auto t = clk::now();
  s.confirmPose();
  const double sc = msSince(t);
  const Dims5 ref = s.dims();
  std::printf("[J9-1] ref faces=%zu H=%.2f rim=%.2f belly=%.2f base=%.2f wall=%.3f bellyZ=%.2f rawH=%.4f\n", s.mesh().faces.size(),
              ref.height, ref.rim, ref.belly, ref.base, ref.meanWall, ref.bellyZ, rawH(s));
  {
    const auto k = KA::AnalyzeDimensions(s.mesh(), KA::SilhouetteResult{});
    const double w = meanWallFull(s.mesh(), s.mesh());
    const bool orig = ref.height == k.vesselHeight && ref.rim == k.rimDiameter && ref.belly == k.maxBodyDiameter &&
                      ref.base == k.baseDiameter && ref.bellyZ == k.maxBodyZ - s.mesh().bboxMin.z && std::fabs(ref.meanWall - w) <= 1e-9;
    CHECK(orig, "[J9-2] sample not from original");
    std::printf("[J9-2] sample faces=%zu orig==doc %d wall %.6f/%.6f\n", s.mesh().faces.size(), int(orig), ref.meanWall, w);
  }
  t = clk::now();
  s.setFrontAngle(12);
  const double ss = msSince(t);
  std::printf("[J9-5] sample faces=%zu confirm=%.0f ms setFront=%.0f ms (기록만)\n", s.mesh().faces.size(), sc, ss);
  for (double a : {12.0, -20.0, 105.0}) {
    s.setFrontAngle(a);
    const auto &d = s.dims();
    const auto k = KA::AnalyzeDimensions(s.mesh(), KA::SilhouetteResult{});
    const double w = meanWallFull(s.mesh(), s.mesh());
    const bool same = d.height == ref.height && d.rim == ref.rim && d.belly == ref.belly && d.base == ref.base &&
                      d.meanWall == ref.meanWall && d.bellyZ == ref.bellyZ;
    const bool indep = near(k.vesselHeight, ref.height, 0.1) && near(k.rimDiameter, ref.rim, 0.1) &&
                       near(k.maxBodyDiameter, ref.belly, 0.1) && near(k.baseDiameter, ref.base, 0.1) &&
                       std::fabs(w - ref.meanWall) <= 1e-6;
    CHECK(same && indep, "[J9-3] sample %g° same=%d indep=%d", a, int(same), int(indep));
    std::printf("[J9-3] sample θ=%g same=%d · indep H=%.2f rim=%.2f belly=%.2f base=%.2f wall=%.6f (Δ %.1e)\n", a, int(same), k.vesselHeight,
                k.rimDiameter, k.maxBodyDiameter, k.baseDiameter, w, w - ref.meanWall);
  }

  // 열린 입술 네 크기: 251,440 · 346,320 · 500,000 · 1,000,160 면 (들어온 자세가 이미 바름 — 자동 세우기 없이 확정)
  struct Sz { int r, h; } sizes[] = {{280, 224}, {360, 240}, {400, 312}, {560, 446}};
  for (const auto &z : sizes) {
    ArtifactDoc h;
    h.loadMesh(vessel(z.r, z.h), {}, QStringLiteral("큰 스캔 예제"));
    t = clk::now();
    h.confirmPose();
    const double tc = msSince(t);
    const size_t F = h.mesh().faces.size(), L = h.lodMesh().faces.size();
    const Dims5 d = h.dims();
    CHECK(F > 250000 && L < F && d.ok, "[J9-1] faces %zu lod %zu ok %d", F, L, int(d.ok));
    CHECK(near5(d, ref), "[J9-1] faces %zu dims differ", F);
    std::printf("[J9-1] faces=%zu lod=%zu H=%.2f rim=%.2f belly=%.2f base=%.2f wall=%.3f bellyZ=%.2f rawH=%.4f | Δ %.2f %.2f %.2f %.2f %.3f\n", F, L,
                d.height, d.rim, d.belly, d.base, d.meanWall, d.bellyZ, rawH(h), d.height - ref.height, d.rim - ref.rim,
                d.belly - ref.belly, d.base - ref.base, d.meanWall - ref.meanWall);
    const auto k = KA::AnalyzeDimensions(h.mesh(), KA::SilhouetteResult{});
    const double w = meanWallFull(h.mesh(), h.mesh());
    const bool orig = d.height == k.vesselHeight && d.rim == k.rimDiameter && d.belly == k.maxBodyDiameter && d.base == k.baseDiameter &&
                      d.bellyZ == k.maxBodyZ - h.mesh().bboxMin.z && std::fabs(d.meanWall - w) <= 1e-9;
    CHECK(orig, "[J9-2] faces %zu not from original (wall %.9f vs %.9f)", F, d.meanWall, w);
    const auto kl = KA::AnalyzeDimensions(h.lodMesh(), KA::SilhouetteResult{});
    std::printf("[J9-2] faces=%zu orig==doc %d wall %.6f/%.6f · LOD였다면 H=%.2f rim=%.2f belly=%.2f base=%.2f wall=%.3f bellyZ=%.2f (기록만)\n", F,
                int(orig), d.meanWall, w, kl.vesselHeight, kl.rimDiameter, kl.maxBodyDiameter, kl.baseDiameter,
                meanWallFull(h.mesh(), h.lodMesh()), kl.maxBodyZ - h.lodMesh().bboxMin.z);
    const auto rl = plateRefLines(h, 3.0);
    const bool rlOk = rl.base && rl.rim && rl.base->b.x - rl.base->a.x == d.base && rl.rim->b.x - rl.rim->a.x == d.rim &&
                      near(d.base, ref.base, 0.1) && near(d.rim, ref.rim, 0.1);
    CHECK(rlOk, "[J9-4] faces %zu reflines", F);
    std::printf("[J9-4] faces=%zu baseLine=%.3f rimLine=%.3f\n", F, rl.base ? rl.base->b.x - rl.base->a.x : -1.0,
                rl.rim ? rl.rim->b.x - rl.rim->a.x : -1.0);
    t = clk::now();
    h.setFrontAngle(12);
    const double ts = msSince(t);
    std::printf("[J9-5] faces=%zu confirm=%.0f ms setFront=%.0f ms (기록만)\n", F, tc, ts);
    if (z.r == 360) {
      for (double a : {12.0, -20.0, 105.0}) {
        h.setFrontAngle(a);
        const auto &e = h.dims();
        const auto ka = KA::AnalyzeDimensions(h.mesh(), KA::SilhouetteResult{});
        const bool same = e.height == d.height && e.rim == d.rim && e.belly == d.belly && e.base == d.base && e.meanWall == d.meanWall &&
                          e.bellyZ == d.bellyZ;
        const bool indep = near(ka.vesselHeight, d.height, 0.1) && near(ka.rimDiameter, d.rim, 0.1) &&
                           near(ka.maxBodyDiameter, d.belly, 0.1) && near(ka.baseDiameter, d.base, 0.1);
        CHECK(same && indep, "[J9-3] 346k %g°", a);
        std::printf("[J9-3] faces=%zu θ=%g same=%d · indep H=%.2f rim=%.2f belly=%.2f base=%.2f\n", F, a, int(same), ka.vesselHeight,
                    ka.rimDiameter, ka.maxBodyDiameter, ka.baseDiameter);
      }
    }
  }

  // 실제 흐름: 기운 큰 스캔 → 자동 세우기(5만 면 프록시) → 확정 → 원본 치수
  {
    ArtifactDoc h;
    h.loadMesh(vessel(360, 240, false, 7.0, {12.0, -5.0, 3.0}), {}, QStringLiteral("큰 스캔 예제"));
    t = clk::now();
    h.autoAlign();
    const double ta = msSince(t);
    t = clk::now();
    h.confirmPose();
    const double tc = msSince(t);
    const Dims5 d = h.dims();
    CHECK(h.aligned() && d.ok && near5(d, ref), "[J9-1] tilt7 aligned=%d", int(h.aligned()));
    std::printf("[J9-1] tilt7 faces=%zu aligned=%d rmse=%.4f H=%.2f rim=%.2f belly=%.2f base=%.2f wall=%.3f bellyZ=%.2f rawH=%.4f | autoAlign=%.0f ms confirm=%.0f ms\n",
                h.mesh().faces.size(), int(h.aligned()), h.axisRmse(), d.height, d.rim, d.belly, d.base, d.meanWall, d.bellyZ, rawH(h), ta, tc);
  }

  // 닫힌 입술(경계 고리 없음 → K-Arch 곡률·정점 대체 경로): 7.7만 면 상당과 비교
  Dims5 cref;
  {
    ArtifactDoc h;
    h.loadMesh(vessel(160, 120, true), {}, QStringLiteral("닫힌 입술 예제"));
    h.confirmPose();
    cref = h.dims();
    const auto rm = KA::DetectRim(h.mesh(), KA::RimMode::Auto);
    std::printf("[J9-1] closed ref faces=%zu H=%.2f rim=%.2f belly=%.2f base=%.2f wall=%.3f bellyZ=%.2f rawH=%.4f rimMode=%d rimErr=%d n=%zu\n",
                h.mesh().faces.size(), cref.height, cref.rim, cref.belly, cref.base, cref.meanWall, cref.bellyZ, rawH(h), int(rm.usedMode),
                int(rm.error), rm.rimPoints.size());
  }
  for (const auto &z : {Sz{360, 240}, Sz{560, 446}}) {
    ArtifactDoc h;
    h.loadMesh(vessel(z.r, z.h, true), {}, QStringLiteral("닫힌 입술 예제"));
    t = clk::now();
    h.confirmPose();
    const double tc = msSince(t);
    const Dims5 d = h.dims();
    const size_t F = h.mesh().faces.size();
    CHECK(F > 250000 && d.ok && near5(d, cref), "[J9-1] closed faces %zu", F);
    const auto kl = KA::AnalyzeDimensions(h.lodMesh(), KA::SilhouetteResult{});
    std::printf("[J9-1] closed faces=%zu H=%.2f rim=%.2f belly=%.2f base=%.2f wall=%.3f bellyZ=%.2f rawH=%.4f | Δ %.2f %.2f %.2f %.2f %.3f · LOD였다면 H=%.2f rim=%.2f belly=%.2f base=%.2f · confirm=%.0f ms\n",
                F, d.height, d.rim, d.belly, d.base, d.meanWall, d.bellyZ, rawH(h), d.height - cref.height, d.rim - cref.rim,
                d.belly - cref.belly, d.base - cref.base, d.meanWall - cref.meanWall, kl.vesselHeight, kl.rimDiameter,
                kl.maxBodyDiameter, kl.baseDiameter, tc);
  }
  std::printf("total %.1f s\n", msSince(T0) / 1000);
  std::puts(fails ? "FAILED" : "PASS");
  return fails ? 1 : 0;
}
