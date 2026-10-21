// DongArch3D Studio — 소프트웨어 래스터라이저 (OpenGL 없이 3D 보조창·도판 음영·Artec 컬러·디지털 탁본을 그린다).
#pragma once
#include "RimDetector.h"

#include <QColor>
#include <QImage>
#include <vector>

namespace studio {

enum class ShadeMode { Real, Depth, DTak, XRay };  // 실사(Artec 컬러/점토) / 깊이 음영 / 디지털 탁본 / X-Ray 내부·투창 투시

struct RasterView {
    double yawDeg = 0.0;    // Z축 회전
    double pitchDeg = 0.0;  // 화면 가로축 회전 (+ 위에서 내려다봄)
    double pxPerMm = 2.0;
    QPointF centerPx;       // 원점(R 0, H hMid)이 놓일 화면 위치
    double centerZ = 0.0;   // 화면 중심에 둘 H (mm)
    double centerX = 0.0, centerY = 0.0;  // 화면 중심에 둘 월드 x·y (mm) — 확정 전 바닥판 맞춤용, 기본 0
};

// 정사영 렌더. 카메라는 -Y 쪽에서 +Y 방향을 본다 (화면 x = 월드 x, 화면 위 = 월드 z).
// vertexColors 가 주어지면 실사(ShadeMode::Real) 모드에서 Artec 정점 컬러/UV 텍스처 색상을 보간해 조명과 합성한다.
// 디지털 탁본(ShadeMode::DTak) 모드에서는 표면 얼룩/색상을 배제하고 순수 형상 요철만 추출한다.
QImage renderMesh(const DongArch::KArch::MeshData& mesh, QSize size, const RasterView& v, ShadeMode mode,
                  QColor base, std::vector<float>* depthOut = nullptr,
                  const std::vector<QRgb>* vertexColors = nullptr);

// 월드 점 → 화면 점 (renderMesh 와 같은 투영)
QPointF projectPoint(const DongArch::KArch::Point3& p, const RasterView& v, double* depth = nullptr);

}  // namespace studio
