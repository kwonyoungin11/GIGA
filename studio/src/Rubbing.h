// DongArch3D Studio — 진짜 탁본: 원통 전개(unroll) + 기준면 요철(relief) 먹 농담.
// 실제 탁본은 종이를 곡면에 눌러 붙여 뜨므로 "펼친 표면"이고, 먹은 튀어나온 곳에 묻고
// 파인 곳(침선·인화문·타날 홈)은 한지 흰색으로 남는다. GigaMesh 의 원통 전개 + 기준면 거리
// 색칠과 같은 생각으로, 확정 자세(중심축 = Z, 원점) 메시의 외면 반경을 (방위각, 높이) 격자에
// 펼친 뒤 넓게 평활한 기준면과의 차이로 먹 농담을 정한다. 가로 = 실제 호 길이(mm), 세로 = 높이(mm).
#pragma once
#include "MeshRaster.h"
#include "RimDetector.h"

#include <QImage>
#include <QRectF>
#include <QString>
#include <functional>

namespace studio {

// 탁본 범위 — 확정 자세(회전 전 원본) 기준. 방위각은 위에서 보아 반시계, +X = 0°.
struct RubbingRegion {
  double azCenterDeg = 0.0; // [0, 360)
  double azSpanDeg = 40.0;  // (0, 360]
  double zLo = 0.0, zHi = 0.0;
  bool operator==(const RubbingRegion &) const = default;
};

struct RubbingParams {
  double pxPerMm = 6.0;
  double baseSigmaMm = 1.5; // 기준면(넓은 곡면) 평활 폭 — 이보다 좁은 요철(인화문·파상문·침선)이 문양으로 드러난다
  int maxSidePx = 16000;
  // 전체 화소 상한 — 중간 버퍼가 화소당 약 36 B 이므로 16 M 화소 ≈ 0.6 GB 가 최대.
  // 넘으면 배율을 낮추고(결과의 pxPerMm 에 실제 값), 1:1 크기는 dpi 메타데이터로 지킨다.
  size_t maxPixels = size_t(16) * 1000 * 1000;
  double fallbackRadiusMm = 0.0; // 범위 안 정점이 없을 때 쓸 외면 반경 (> 0 일 때)
};

struct UnrolledRubbing {
  QImage image; // 한지 바탕 위 먹 농담 (스캔 없는 곳 = 연회색)
  double widthMm = 0.0, heightMm = 0.0;
  double refRadiusMm = 0.0; // 호 길이 환산 반경
  double pxPerMm = 0.0;
  double coverage = 0.0; // 스캔이 있는 화소 비율 0..1
  bool ok() const { return !image.isNull(); }
};

UnrolledRubbing renderUnrolledRubbing(const DongArch::KArch::MeshData &mesh,
                                      const RubbingRegion &region,
                                      const RubbingParams &params = {});

// 정면 정사영 3D 화면에서 끈 상자 → 확정 자세 기준 탁본 범위.
// frontAngleDeg: 화면 메시가 원본에서 +Z 로 돈 각(막 각도). outerRadiusAt(z): 외면 반경.
// farSide: 상자 안에 보이는 면이 뒤쪽 벽(깨진 편의 안쪽이 보이는 경우)이면 true.
RubbingRegion regionFromScreenBox(const QRectF &box, const RasterView &v,
                                  double frontAngleDeg,
                                  const std::function<double(double)> &outerRadiusAt,
                                  bool farSide = false);

// 1:1 실제 크기 PNG (dpi 메타데이터 포함, 기본 600 dpi). 범위가 아주 넓으면 화소 상한 때문에
// 실제 dpi 가 낮아질 수 있어 actualDpi 로 돌려준다. 메모리가 부족하면 false + err.
bool saveRubbingPng(const DongArch::KArch::MeshData &mesh,
                    const RubbingRegion &region, const QString &path,
                    double dpi = 600.0, QString *err = nullptr,
                    double *actualDpi = nullptr, double fallbackRadiusMm = 0.0);

} // namespace studio
