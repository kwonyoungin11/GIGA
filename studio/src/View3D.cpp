#include "View3D.h"

#include "CutPageText.h"
#include "Theme.h"

#include <QGuiApplication>
#include <QHBoxLayout>
#include <QLocale>
#include <QMouseEvent>
#include <QPainter>
#include <QPainterPath>
#include <QPushButton>
#include <QSignalBlocker>
#include <QSlider>
#include <QVBoxLayout>
#include <QWheelEvent>
#include <algorithm>
#include <cmath>
#include <numbers>

namespace studio {

namespace {
// ① 바닥 정하기(확정 전) 바닥판(실측판): Z = 0 평면 위 정사각형.
// 중심 = 메시 바운딩 박스 중심 (x, y), 반쪽 길이 = (반폭 최대 + 20 mm) 를 10 mm
// 단위로 올림.
struct FloorPlate {
  double cx, cy, half;
};
FloorPlate floorPlate(const KA::MeshData &m) {
  const double hx = 0.5 * (m.bboxMax.x - m.bboxMin.x),
               hy = 0.5 * (m.bboxMax.y - m.bboxMin.y);
  return {0.5 * (m.bboxMin.x + m.bboxMax.x), 0.5 * (m.bboxMin.y + m.bboxMax.y),
          std::ceil((std::max(hx, hy) + 20.0) / 10.0) * 10.0};
}

double distToPolyline(const QPointF &pt, const QPolygonF &poly) {
  if (poly.empty())
    return 1e9;
  double best = 1e9;
  for (int i = 0; i + 1 < poly.size(); ++i) {
    const QPointF a = poly[i], b = poly[i + 1];
    const double dx = b.x() - a.x(), dy = b.y() - a.y();
    const double l2 = dx * dx + dy * dy;
    const double t =
        l2 > 1e-9
            ? std::clamp(((pt.x() - a.x()) * dx + (pt.y() - a.y()) * dy) / l2,
                         0.0, 1.0)
            : 0.0;
    best = std::min(best,
                    std::hypot(pt.x() - (a.x() + t * dx),
                               pt.y() - (a.y() + t * dy)));
  }
  return best;
}
} // namespace

// ---- AngleStripWidget (30° 간격 12칸 미니 단면 띠) -------------------------
AngleStripWidget::AngleStripWidget(QWidget *parent) : QWidget(parent) {
  setFixedHeight(64);
  setMinimumWidth(180);
  setCursor(Qt::PointingHandCursor);
  setToolTip(QStringLiteral(
      "각도 띠 (0°~330°, 30° 간격) — 원하는 단면 모양을 클릭하면 즉시 그 각도로 "
      "이동합니다 (★ = 추천 각도)"));
}

void AngleStripWidget::setCells(std::vector<AngleStripCell> cells,
                                double currentAngleDeg) {
  cells_ = std::move(cells);
  currentAngleDeg_ =
      std::fmod(std::fmod(currentAngleDeg, 360.0) + 360.0, 360.0);
  update();
}

void AngleStripWidget::paintEvent(QPaintEvent *) {
  QPainter p(this);
  p.setRenderHint(QPainter::Antialiasing);
  p.fillRect(rect(), theme::Surface);
  p.setPen(QPen(theme::Line, 1));
  p.drawLine(0, 0, width(), 0);

  if (cells_.empty())
    return;

  const int n = int(cells_.size());
  const double pad = 4.0;
  const double cw = std::max(12.0, (width() - 2.0 * pad) / double(n));
  const double ch = height() - 8.0;

  for (int i = 0; i < n; ++i) {
    const auto &c = cells_[size_t(i)];
    const QRectF r(pad + i * cw + 1.0, 4.0, cw - 2.0, ch);
    const double diff =
        std::fabs(std::remainder(c.angleDeg - currentAngleDeg_, 360.0));
    const bool active = diff <= 15.0;

    p.setPen(QPen(active            ? theme::ProfileRed
                  : c.isRecommended ? QColor(0x2f, 0x7a, 0x4f)
                                    : theme::Line,
                  active ? 1.8 : 1.0));
    p.setBrush(active            ? QColor(253, 242, 241)
               : c.isRecommended ? QColor(242, 250, 245)
                                 : theme::Ground);
    p.drawRoundedRect(r, 3.0, 3.0);

    // 미니 단면 윤곽선 그리기
    if (!c.outerPts.empty()) {
      double minX = 1e18, maxX = -1e18, minY = 1e18, maxY = -1e18;
      for (const auto &pt : c.outerPts) {
        minX = std::min(minX, pt.x);
        maxX = std::max(maxX, pt.x);
        minY = std::min(minY, pt.y);
        maxY = std::max(maxY, pt.y);
      }
      const double spanX = std::max(1.0, maxX - minX);
      const double spanY = std::max(1.0, maxY - minY);
      const QRectF drawBox(r.left() + 3.0, r.top() + 14.0,
                           std::max(4.0, r.width() - 6.0),
                           std::max(8.0, r.height() - 26.0));
      const double sc =
          std::min(drawBox.width() / spanX, drawBox.height() / spanY);
      QPolygonF poly;
      const size_t step = std::max<size_t>(1, c.outerPts.size() / 24);
      for (size_t k = 0; k < c.outerPts.size(); k += step) {
        const auto &pt = c.outerPts[k];
        const double sx =
            drawBox.center().x() + (pt.x - 0.5 * (minX + maxX)) * sc;
        const double sy = drawBox.bottom() - (pt.y - minY) * sc;
        poly << QPointF(sx, sy);
      }
      p.setPen(QPen(active ? theme::ProfileRed : theme::Ink2, 1.2));
      p.setBrush(Qt::NoBrush);
      p.drawPolyline(poly);
    }

    // 각도 숫자 + ★ 표시
    p.setFont(theme::monoFont(8, active || c.isRecommended));
    p.setPen(active            ? theme::ProfileRed
             : c.isRecommended ? QColor(0x2f, 0x7a, 0x4f)
                               : theme::Ink2);
    const QString topTxt =
        c.isRecommended
            ? QStringLiteral("★%1°").arg(int(std::lround(c.angleDeg)))
            : QStringLiteral("%1°").arg(int(std::lround(c.angleDeg)));
    p.drawText(QRectF(r.left(), r.top() + 1.0, r.width(), 12.0),
               Qt::AlignCenter, topTxt);

    p.setFont(theme::uiFont(7));
    p.setPen(c.hasError ? theme::ProfileRed : theme::Muted);
    p.drawText(QRectF(r.left(), r.bottom() - 11.0, r.width(), 10.0),
               Qt::AlignCenter, c.statusLabel);
  }
}

void AngleStripWidget::mousePressEvent(QMouseEvent *e) {
  if (cells_.empty() || e->button() != Qt::LeftButton)
    return;
  const double pad = 4.0;
  const double cw =
      std::max(12.0, (width() - 2.0 * pad) / double(cells_.size()));
  const int idx = std::clamp(int((e->position().x() - pad) / cw), 0,
                             int(cells_.size()) - 1);
  emit angleClicked(cells_[size_t(idx)].angleDeg);
}

// ---- PlanViewWidget (왼쪽 상단: 위에서 내려다본 실제 유물 평면도) ----------
PlanViewWidget::PlanViewWidget(QWidget *parent) : QWidget(parent) {
  setObjectName("PlanViewWidget");
  setMinimumSize(200, 175);
  setMaximumHeight(255);
  setCursor(Qt::CrossCursor);
  setToolTip(QStringLiteral(
      "위에서 내려다본 실제 유물 평면도 — 자르고 싶은 방향을 마우스로 클릭하거나 "
      "끌면 즉시 그 자리의 세로 단면(A–A′)이 오른쪽 도면에 나타납니다"));
}

void PlanViewWidget::setDoc(const ArtifactDoc *doc) {
  const bool docChanged = (doc_ != doc);
  doc_ = doc;
  if (!doc_ || !doc_->isLoaded()) {
    baseTopImg_ = QImage();
    dirty_ = true;
    update();
    return;
  }
  const auto &m = doc_->meshMaster();
  if (docChanged || m.vertices.size() != cachedVertCount_ ||
      std::fabs(m.bboxMax.z - cachedBboxZ_) > 1e-4) {
    cachedVertCount_ = m.vertices.size();
    cachedBboxZ_ = m.bboxMax.z;
    dirty_ = true;
  }
  update();
}

void PlanViewWidget::rebuildTopImage() {
  dirty_ = false;
  if (!doc_ || !doc_->isLoaded() || width() < 32 || height() < 40) {
    baseTopImg_ = QImage();
    return;
  }
  // 확정 자세(frontAngle 회전 전)의 정지된 유물 메시를 위에서 내려다본 평면 영상으로 캐싱
  const auto &m = doc_->meshMaster();
  if (m.vertices.empty()) {
    baseTopImg_ = QImage();
    return;
  }
  maxR_ = std::max({20.0, std::fabs(m.bboxMin.x), std::fabs(m.bboxMax.x),
                    std::fabs(m.bboxMin.y), std::fabs(m.bboxMax.y)});
  const int headerH = 22;
  const int availW = std::max(32, width() - 12);
  const int availH = std::max(32, height() - headerH - 8);
  const int side = std::min(availW, availH);
  const double spanMm = 2.0 * (maxR_ + 14.0);
  pxPerMm_ = double(side) / std::max(1.0, spanMm);
  ctrPx_ = QPointF(width() * 0.5, headerH + (height() - headerH) * 0.5);

  RasterView v;
  v.yawDeg = 0.0;
  v.pitchDeg = -90.0;
  v.pxPerMm = pxPerMm_;
  v.centerPx = ctrPx_;
  v.centerX = 0.0;
  v.centerY = 0.0;
  v.centerZ = 0.5 * (m.bboxMin.z + m.bboxMax.z);

  // 초고해상도 메시는 1회 캐싱 시 원본 또는 LOD에서 빠르게 렌더링
  KA::MeshData topMesh;
  const KA::MeshData *srcMesh = &m;
  if (m.faces.size() > 350000 && !doc_->lodMesh().faces.empty()) {
    // lodMesh()는 previewAngle 회전이 포함되어 있으므로 역회전하여 정지 좌표계로 복원
    topMesh = doc_->lodMesh();
    const double curAng = doc_->previewAngle();
    if (std::fabs(curAng) > 1e-9) {
      const double rad = -curAng * std::numbers::pi / 180.0;
      const double c = std::cos(rad), s = std::sin(rad);
      for (auto &pt : topMesh.vertices) {
        const double x = c * pt.x - s * pt.y;
        const double y = s * pt.x + c * pt.y;
        pt.x = x;
        pt.y = y;
      }
    }
    srcMesh = &topMesh;
  }
  baseTopImg_ = renderMesh(*srcMesh, size(), v, ShadeMode::Real,
                           theme::ModelClay, nullptr, nullptr);
}

double PlanViewWidget::angleForScreenPt(const QPointF &pt) const {
  if (!doc_ || !doc_->isLoaded())
    return 0.0;
  const double dx = pt.x() - ctrPx_.x();
  const double dy = -(pt.y() - ctrPx_.y());
  if (std::hypot(dx, dy) < 2.0)
    return doc_->previewAngle();
  const double thetaDeg = std::atan2(dy, dx) * (180.0 / std::numbers::pi);
  // 클릭한 방향(thetaDeg)과 반대 방향(thetaDeg + 180°)의 높이 범위를 비교해
  // 자른 단면이 정확히 클릭한 방향을 나타내도록 정면각(frontAngle)을 산출
  const auto &m = doc_->meshMaster();
  const double thRad = thetaDeg * (std::numbers::pi / 180.0);
  const double ux = std::cos(thRad), uy = std::sin(thRad);
  double zMinClick = 1e18, zMaxClick = -1e18;
  double zMinOpp = 1e18, zMaxOpp = -1e18;
  const size_t step = std::max<size_t>(1, m.vertices.size() / 8000);
  for (size_t i = 0; i < m.vertices.size(); i += step) {
    const auto &v = m.vertices[i];
    const double perp = -uy * v.x + ux * v.y;
    if (std::fabs(perp) <= 3.0) {
      const double proj = ux * v.x + uy * v.y;
      if (proj >= 0.0) {
        zMinClick = std::min(zMinClick, v.z);
        zMaxClick = std::max(zMaxClick, v.z);
      } else {
        zMinOpp = std::min(zMinOpp, v.z);
        zMaxOpp = std::max(zMaxOpp, v.z);
      }
    }
  }
  const double spanClick = (zMaxClick > zMinClick) ? (zMaxClick - zMinClick) : 0.0;
  const double spanOpp = (zMaxOpp > zMinOpp) ? (zMaxOpp - zMinOpp) : 0.0;
  if (spanClick > spanOpp + 5.0) {
    return std::remainder(180.0 - thetaDeg, 360.0);
  }
  return std::remainder(-thetaDeg, 360.0);
}

void PlanViewWidget::paintEvent(QPaintEvent *) {
  QPainter p(this);
  p.setRenderHint(QPainter::Antialiasing, true);
  p.fillRect(rect(), theme::Ground);

  // 상단 제목 줄
  const QRectF hdr(0, 0, width(), 22);
  p.fillRect(hdr, theme::Surface);
  p.setPen(QPen(theme::Line, 1.0));
  p.drawLine(0, 22, width(), 22);
  p.setFont(theme::uiFont(9, true));
  p.setPen(theme::Ink);
  p.drawText(QRectF(8, 0, width() - 90, 22), Qt::AlignVCenter | Qt::AlignLeft,
             QStringLiteral("◉ 위에서 본 유물 평면도 (클릭·끌어 단면 회전)"));

  if (!doc_ || !doc_->isLoaded())
    return;
  if (dirty_ || baseTopImg_.size() != size())
    rebuildTopImage();

  const double curAng =
      std::fmod(std::fmod(doc_->previewAngle(), 360.0) + 360.0, 360.0);
  p.setFont(theme::monoFont(9, true));
  p.setPen(theme::ProfileRed);
  p.drawText(QRectF(width() - 82, 0, 74, 22), Qt::AlignVCenter | Qt::AlignRight,
             QStringLiteral("막 %1°").arg(int(std::lround(curAng)) % 360));

  // 중심 십자선 및 방위 눈금 원
  const double ringR = (maxR_ + 6.0) * pxPerMm_;
  p.setPen(QPen(theme::Grid10mm, 1.0, Qt::DashLine));
  p.drawLine(QPointF(ctrPx_.x() - ringR - 8, ctrPx_.y()),
             QPointF(ctrPx_.x() + ringR + 8, ctrPx_.y()));
  p.drawLine(QPointF(ctrPx_.x(), ctrPx_.y() - ringR - 8),
             QPointF(ctrPx_.x(), ctrPx_.y() + ringR + 8));

  if (!baseTopImg_.isNull())
    p.drawImage(0, 0, baseTopImg_);

  // 구연부 측정 원 (점선)
  if (doc_->poseConfirmed() && doc_->dims().rim > 5.0) {
    const double rimR = 0.5 * doc_->dims().rim * pxPerMm_;
    p.setPen(QPen(theme::Ink2, 1.1, Qt::DotLine));
    p.setBrush(Qt::NoBrush);
    p.drawEllipse(ctrPx_, rimR, rimR);
  }

  // 바깥 회전 안내 링
  p.setPen(QPen(theme::Line, 1.0));
  p.setBrush(Qt::NoBrush);
  p.drawEllipse(ctrPx_, ringR, ringR);

  // 절단선 A-A' 표시 (정지된 base_ 좌표계에서 절단선은 -frontAngle 방위각)
  const double angRad = -doc_->previewAngle() * (std::numbers::pi / 180.0);
  const QPointF dir(std::cos(angRad), -std::sin(angRad));
  const QPointF ptPos = ctrPx_ + dir * ringR;
  const QPointF ptNeg = ctrPx_ - dir * ringR;

  // 어느 쪽 반평면이 주 단면인지 판정
  const auto &m = doc_->lodMesh().faces.empty() ? doc_->mesh() : doc_->lodMesh();
  double zMinP = 1e18, zMaxP = -1e18, zMinN = 1e18, zMaxN = -1e18;
  for (const auto &v : m.vertices) {
    if (std::fabs(v.y) <= 2.5) {
      if (v.x >= 0.0) {
        zMinP = std::min(zMinP, v.z);
        zMaxP = std::max(zMaxP, v.z);
      } else {
        zMinN = std::min(zMinN, v.z);
        zMaxN = std::max(zMaxN, v.z);
      }
    }
  }
  const bool preferNeg =
      ((zMaxN > zMinN ? zMaxN - zMinN : 0.0) >
       (zMaxP > zMinP ? zMaxP - zMinP : 0.0) + 5.0);
  const QPointF &ptActive = preferNeg ? ptNeg : ptPos;
  const QPointF &ptOther = preferNeg ? ptPos : ptNeg;

  p.setPen(QPen(theme::ProfileRed, 1.4, Qt::DashLine));
  p.drawLine(ctrPx_, ptOther);
  p.setPen(QPen(Qt::white, 4.5, Qt::SolidLine, Qt::RoundCap));
  p.drawLine(ctrPx_, ptActive);
  p.setPen(QPen(theme::ProfileRed, 2.4, Qt::SolidLine, Qt::RoundCap));
  p.drawLine(ctrPx_, ptActive);

  // 중심점 및 손잡이
  p.setPen(QPen(Qt::white, 1.5));
  p.setBrush(theme::ProfileRed);
  p.drawEllipse(ctrPx_, 3.5, 3.5);
  p.drawEllipse(ptActive, 6.0, 6.0);
  p.setFont(theme::monoFont(8, true));
  p.setPen(Qt::white);
  p.drawText(QRectF(ptActive.x() - 6, ptActive.y() - 6, 12, 12),
             Qt::AlignCenter, QStringLiteral("A"));

  p.setPen(QPen(theme::Line, 1.0));
  p.drawLine(0, height() - 1, width(), height() - 1);
}

void PlanViewWidget::mousePressEvent(QMouseEvent *e) {
  if (!doc_ || !doc_->isLoaded() || !doc_->poseConfirmed() ||
      e->button() != Qt::LeftButton)
    return;
  dragging_ = true;
  const double newAng = angleForScreenPt(e->position());
  emit anglePreviewed(newAng);
  update();
}

void PlanViewWidget::mouseMoveEvent(QMouseEvent *e) {
  if (!dragging_ || !doc_ || !doc_->isLoaded() || !doc_->poseConfirmed())
    return;
  const double newAng = angleForScreenPt(e->position());
  emit anglePreviewed(newAng);
  update();
}

void PlanViewWidget::mouseReleaseEvent(QMouseEvent *e) {
  if (!dragging_ || e->button() != Qt::LeftButton)
    return;
  dragging_ = false;
  if (doc_ && doc_->isLoaded() && doc_->poseConfirmed()) {
    const double newAng = angleForScreenPt(e->position());
    emit angleCommitted(newAng);
    update();
  }
}

// ---- View3DCanvas ----------------------------------------------------------
View3DCanvas::View3DCanvas(QWidget *parent) : QWidget(parent) {
  setMinimumSize(200, 180);
  setAttribute(Qt::WA_OpaquePaintEvent);
  setCursor(Qt::OpenHandCursor);
}

void View3DCanvas::setDoc(const ArtifactDoc *d, int slot) {
  doc_ = d;
  slot_ = slot;
  cut_.clear();
  if (doc_ && doc_->isLoaded() &&
      doc_->poseConfirmed()) { // 확정 전에는 절단하지 않는다
    const auto &sl = doc_->slotList();
    const auto &m =
        doc_->lodMesh().faces.empty() ? doc_->mesh() : doc_->lodMesh();
    if (slot >= 0 && slot < int(sl.size()) &&
        sl[size_t(slot)].kind == SlotKind::Horizontal)
      cut_ = KA::Slice(m, KA::Plane{{0, 0, 1}, -sl[size_t(slot)].z});
    else
      cut_ = KA::Slice(m, KA::Plane{{0, 1, 0}, 0.0});
  }
  dirty_ = true;
  update();
}

void View3DCanvas::setView(double yaw, double pitch) {
  yaw_ = yaw;
  pitch_ = pitch;
  dirty_ = true;
  update();
}

RasterView View3DCanvas::view() const {
  RasterView v;
  v.yawDeg = yaw_;
  v.pitchDeg = pitch_;
  if (!doc_ || !doc_->isLoaded())
    return v;
  const auto &m = doc_->mesh();
  v.centerPx = QPointF(width() / 2.0, height() / 2.0);
  if (!doc_->poseConfirmed()) {
    // 확정 전: (메시 바운딩 박스 ∪ 바닥판) 의 중심·크기로 맞춘다. 판이 메시
    // 발자국을 덮으므로 x·y 는 판 기준, 돌려 봐도 판 모서리가 들어오도록 반경 =
    // 반쪽 길이 × √2.
    const FloorPlate f = floorPlate(m);
    const double z0 = std::min(m.bboxMin.z, 0.0),
                 z1 = std::max(m.bboxMax.z, 0.0);
    const double r = f.half * std::sqrt(2.0), h = z1 - z0;
    v.pxPerMm = std::min(width(), height()) /
                std::max(1.0, std::sqrt(4 * r * r + h * h) * 1.05);
    v.centerX = f.cx;
    v.centerY = f.cy;
    v.centerZ = 0.5 * (z0 + z1);
    return v;
  }
  // D5: R 로 도판을 뒤집으면(단면 왼쪽) 3D 도 뒤에서 보아 "3D 왼쪽·오른쪽 =
  // 도판" 을 지킨다
  if (doc_->sectionSide() == SectionSide::Left)
    v.yawDeg = yaw_ + 180.0;
  const double r = std::max({std::fabs(m.bboxMin.x), std::fabs(m.bboxMax.x),
                             std::fabs(m.bboxMin.y), std::fabs(m.bboxMax.y)});
  const double h = m.bboxMax.z - m.bboxMin.z;
  const double extent = std::sqrt(4 * r * r + h * h) * 1.12;
  v.pxPerMm = std::min(width(), height()) / std::max(1.0, extent);
  v.centerZ = m.bboxMin.z + h / 2.0;
  return v;
}

int View3DCanvas::hitGizmoAxis(const QPointF &screenPt) const {
  if (!doc_ || !doc_->isLoaded() || doc_->poseConfirmed())
    return -1;
  const auto &m = doc_->mesh();
  const RasterView v = view();
  const KA::Point3 ctr{0.5 * (m.bboxMin.x + m.bboxMax.x),
                       0.5 * (m.bboxMin.y + m.bboxMax.y),
                       0.5 * (m.bboxMin.z + m.bboxMax.z)};
  const double rad = 0.58 * std::max({m.bboxMax.x - m.bboxMin.x,
                                      m.bboxMax.y - m.bboxMin.y,
                                      m.bboxMax.z - m.bboxMin.z});
  QPolygonF ringX, ringY, ringZ;
  for (int deg = 0; deg <= 360; deg += 10) {
    const double a = deg * std::numbers::pi / 180.0;
    const double ca = std::cos(a) * rad, sa = std::sin(a) * rad;
    ringX << projectPoint({ctr.x, ctr.y + ca, ctr.z + sa}, v);
    ringY << projectPoint({ctr.x + ca, ctr.y, ctr.z + sa}, v);
    ringZ << projectPoint({ctr.x + ca, ctr.y + sa, 0.0}, v);
  }
  const double dx = distToPolyline(screenPt, ringX);
  const double dy = distToPolyline(screenPt, ringY);
  const double dz = distToPolyline(screenPt, ringZ);
  const double minD = std::min({dx, dy, dz});
  if (minD > 12.0)
    return -1;
  if (dx <= dy && dx <= dz)
    return 0;
  if (dy <= dx && dy <= dz)
    return 1;
  return 2;
}

bool View3DCanvas::pickSurfacePoint(const QPointF &screenPt,
                                    KA::Point3 &outWorld) const {
  if (!doc_ || !doc_->isLoaded())
    return false;
  const auto &m = doc_->lodMesh().faces.empty() ? doc_->mesh() : doc_->lodMesh();
  if (m.vertices.empty())
    return false;
  const RasterView v = view();
  const double yw = v.yawDeg * std::numbers::pi / 180.0;
  const double pt = v.pitchDeg * std::numbers::pi / 180.0;
  const double cy = std::cos(yw), sy = std::sin(yw);
  const double cp = std::cos(pt), sp = std::sin(pt);
  auto camDepth = [&](const KA::Point3 &p) {
    const double px = p.x - v.centerX, py = p.y - v.centerY;
    const double y1 = sy * px + cy * py;
    const double z1 = p.z - v.centerZ;
    return -(sp * z1 + cp * y1);
  };

  double bestNearDepth = -1e18;
  size_t bestNearIdx = 0;
  bool foundNear = false;
  double bestDist2 = 1e18;
  size_t bestIdx = 0;
  const size_t step = std::max<size_t>(1, m.vertices.size() / 12000);
  for (size_t i = 0; i < m.vertices.size(); i += step) {
    const QPointF spPt = projectPoint(m.vertices[i], v);
    const double dx = spPt.x() - screenPt.x(), dy = spPt.y() - screenPt.y();
    const double d2 = dx * dx + dy * dy;
    if (d2 <= 16.0 * 16.0) {
      const double cz = camDepth(m.vertices[i]);
      if (!foundNear || cz > bestNearDepth) {
        bestNearDepth = cz;
        bestNearIdx = i;
        foundNear = true;
      }
    }
    if (d2 < bestDist2) {
      bestDist2 = d2;
      bestIdx = i;
    }
  }
  if (foundNear) {
    outWorld = m.vertices[bestNearIdx];
    return true;
  }
  if (bestDist2 > 45.0 * 45.0)
    return false;
  outWorld = m.vertices[bestIdx];
  return true;
}

void View3DCanvas::paintEvent(QPaintEvent *) {
  QPainter p(this);
  p.fillRect(rect(), theme::Ground);
  if (!doc_ || !doc_->isLoaded())
    return;
  const RasterView v = view();
  if (dirty_ || img_.size() != size()) {
    const auto &lm =
        doc_->lodMesh().faces.empty() ? doc_->mesh() : doc_->lodMesh();
    const auto &lc = doc_->lodVertexColors();
    img_ = renderMesh(lm, size(), v, ShadeMode::Real, theme::ModelClay, nullptr,
                      lc.size() == lm.vertices.size() ? &lc : nullptr);
    dirty_ = false;
  }
  p.setRenderHint(QPainter::Antialiasing);
  if (!doc_->poseConfirmed()) {
    // ① 바닥 정하기: 바닥판(Wash) + 1 cm 방안(Grid10mm) + 테두리(Edge) 를
    // 유물보다 먼저(뒤쪽에) 그린다
    const auto &m = doc_->mesh();
    const FloorPlate f = floorPlate(m);
    auto at = [&](double x, double y) {
      return projectPoint({f.cx + x, f.cy + y, 0.0}, v);
    };
    const QPolygonF plate{at(-f.half, -f.half), at(f.half, -f.half),
                          at(f.half, f.half), at(-f.half, f.half)};
    p.setPen(Qt::NoPen);
    p.setBrush(theme::Wash);
    p.drawPolygon(plate);
    p.setPen(QPen(theme::Grid10mm, 1));
    const int n = int(std::lround(2.0 * f.half / 10.0));
    for (int i = 1; i < n; ++i) {
      const double t = -f.half + 10.0 * i;
      p.drawLine(at(t, -f.half), at(t, f.half));
      p.drawLine(at(-f.half, t), at(f.half, t));
    }
    p.setPen(QPen(theme::Edge, 1));
    p.setBrush(Qt::NoBrush);
    p.drawPolygon(plate);
    p.drawImage(0, 0, img_);

    // 3D 직관 회전 기즈모 링 (X: 주황 #c8641e, Y: 초록 #3f8f4f, Z: 파랑 #3f6fb5)
    const KA::Point3 ctr{0.5 * (m.bboxMin.x + m.bboxMax.x),
                         0.5 * (m.bboxMin.y + m.bboxMax.y),
                         0.5 * (m.bboxMin.z + m.bboxMax.z)};
    const double rad = 0.58 * std::max({m.bboxMax.x - m.bboxMin.x,
                                        m.bboxMax.y - m.bboxMin.y,
                                        m.bboxMax.z - m.bboxMin.z});
    QPolygonF ringX, ringY, ringZ;
    for (int deg = 0; deg <= 360; deg += 8) {
      const double a = deg * std::numbers::pi / 180.0;
      const double ca = std::cos(a) * rad, sa = std::sin(a) * rad;
      ringX << projectPoint({ctr.x, ctr.y + ca, ctr.z + sa}, v);
      ringY << projectPoint({ctr.x + ca, ctr.y, ctr.z + sa}, v);
      ringZ << projectPoint({ctr.x + ca, ctr.y + sa, 0.0}, v);
    }
    p.setPen(QPen(QColor(0xc8, 0x64, 0x1e, 190),
                  poseDragAxis_ == 0 ? 2.6 : 1.4, Qt::DashLine));
    p.drawPolyline(ringX);
    p.setPen(QPen(QColor(0x3f, 0x8f, 0x4f, 190),
                  poseDragAxis_ == 1 ? 2.6 : 1.4, Qt::DashLine));
    p.drawPolyline(ringY);
    p.setPen(QPen(QColor(0x3f, 0x6f, 0xb5, 190),
                  poseDragAxis_ == 2 ? 2.6 : 1.4, Qt::SolidLine));
    p.drawPolyline(ringZ);

    // 수직 기준축 점선 (+Z)
    p.setPen(QPen(theme::Ink2, 1.0, Qt::DashDotLine));
    p.drawLine(projectPoint({ctr.x, ctr.y, 0.0}, v),
               projectPoint({ctr.x, ctr.y, m.bboxMax.z + 20.0}, v));

    // Feature 14: 바닥 후보 평면 배지 ①②③ (클릭 가능)
    floorBadgeRects_.fill(QRectF{});
    const auto cands = doc_->floorCandidates();
    for (size_t i = 0; i < cands.size() && i < 3; ++i) {
      const auto &c = cands[i];
      const QPointF pt = projectPoint(c.center, v);
      const QString badgeNum =
          (i == 0 ? QStringLiteral("①")
                  : (i == 1 ? QStringLiteral("②") : QStringLiteral("③")));
      const QString label =
          QStringLiteral("%1 %2% · %3㎠%4")
              .arg(badgeNum)
              .arg(c.flatness, 0, 'f', 0)
              .arg(c.areaCm2, 0, 'f', 1)
              .arg(c.recommended ? QStringLiteral(" [추천·클릭]")
                                 : QStringLiteral(" [클릭]"));
      p.setFont(theme::uiFont(10, true));
      const QFontMetricsF fm(p.font());
      const double bw = fm.horizontalAdvance(label) + 12;
      const QRectF br(pt.x() - bw / 2, pt.y() - 11, bw, 22);
      floorBadgeRects_[i] = br;
      p.setPen(QPen(c.recommended ? QColor(0x2f, 0x7a, 0x4f) : theme::Line,
                    c.recommended ? 1.6 : 1.0));
      p.setBrush(c.recommended ? QColor(240, 250, 244, 235)
                               : QColor(255, 255, 255, 225));
      p.drawRoundedRect(br, 4, 4);
      p.setPen(c.recommended ? QColor(0x2f, 0x7a, 0x4f) : theme::Ink);
      p.drawText(br, Qt::AlignCenter, label);
    }

    // 3점 바닥 찍기 표시점
    const auto &picks = doc_->floorPickPoints();
    for (size_t i = 0; i < picks.size(); ++i) {
      const QPointF sp = projectPoint(picks[i], v);
      p.setPen(QPen(Qt::white, 2.0));
      p.setBrush(QColor(0x2f, 0x7a, 0x4f));
      p.drawEllipse(sp, 7.0, 7.0);
      p.setFont(theme::monoFont(9, true));
      p.setPen(Qt::white);
      p.drawText(QRectF(sp.x() - 7, sp.y() - 7, 14, 14), Qt::AlignCenter,
                 QString::number(i + 1));
    }

    // 상단 안내 배지 (드래그로 유물이 쓰러지지 않음을 명확히 안내)
    const QString guideTxt =
        posePickMode_ == PosePickMode::Surface1Click
            ? QStringLiteral("🎯 바닥으로 쓸 유물 표면을 마우스로 클릭하세요")
        : posePickMode_ == PosePickMode::Floor3Point
            ? QStringLiteral("🎯 바닥 둘레 3점을 차례로 클릭하세요 (%1/3)")
                  .arg(picks.size())
            : QStringLiteral(
                  "🔒 유물 자세 고정됨 (드래그: 시점 회전 · 기즈모 고리/버튼: "
                  "미세 조정 · ①②③: 바닥 선택)");
    p.setFont(theme::uiFont(9, posePickMode_ != PosePickMode::None));
    const QFontMetricsF gfm(p.font());
    const double gw = std::min<double>(width() - 16,
                                       gfm.horizontalAdvance(guideTxt) + 16);
    const QRectF gr(8, 8, gw, 22);
    p.setPen(QPen(posePickMode_ != PosePickMode::None ? QColor(0x2f, 0x7a, 0x4f)
                                                      : theme::Line,
                  1.0));
    p.setBrush(QColor(255, 255, 255, 225));
    p.drawRoundedRect(gr, 4, 4);
    p.setPen(posePickMode_ != PosePickMode::None ? QColor(0x2f, 0x7a, 0x4f)
                                                 : theme::Ink2);
    p.drawText(gr, Qt::AlignCenter, guideTxt);
    return;
  }

  minimapRect_ = QRectF{};
  horizHandleRect_ = QRectF{};

  const auto &m = doc_->mesh();
  const double R = std::max({std::fabs(m.bboxMin.x), std::fabs(m.bboxMax.x),
                             std::fabs(m.bboxMin.y), std::fabs(m.bboxMax.y)}) +
                   8;
  const double z0 = m.bboxMin.z - 6, z1 = m.bboxMax.z + 8;
  const auto &sl = doc_->slotList();
  const bool horiz = slot_ >= 0 && slot_ < int(sl.size()) &&
                     sl[size_t(slot_)].kind == SlotKind::Horizontal;

  // 절단면(평면) — 뒤쪽 반투명
  QPolygonF quad;
  if (horiz) {
    const double z = sl[size_t(slot_)].z;
    for (auto [x, y] : {std::pair{-R, -R}, {R, -R}, {R, R}, {-R, R}})
      quad << projectPoint({x, y, z}, v);
  } else {
    for (auto [x, z] : {std::pair{-R, z0}, {R, z0}, {R, z1}, {-R, z1}})
      quad << projectPoint({x, 0, z}, v);
  }
  p.setPen(Qt::NoPen);
  p.setBrush(horiz ? QColor(43, 108, 176, 28) : QColor(255, 0, 0, 22));
  p.drawPolygon(quad);
  p.drawImage(0, 0, img_);
  p.setPen(QPen(horiz ? QColor(0x2b, 0x6c, 0xb0) : theme::ProfileRed, 1,
                Qt::DashLine));
  p.setBrush(Qt::NoBrush);
  p.drawPolygon(quad);

  // 중심축
  p.setPen(QPen(theme::Ink, 1, Qt::DashDotLine));
  p.drawLine(projectPoint({0, 0, z0}, v), projectPoint({0, 0, z1}, v));

  // 절단 단면선 (편평·결실 유물에서 -X 반쪽이 주 단면일 때는 -X 반쪽을 굵은 실선으로 표시)
  double zMinPos = 1e18, zMaxPos = -1e18, zMinNeg = 1e18, zMaxNeg = -1e18;
  if (!horiz) {
    for (const auto &pl : cut_) {
      for (const auto &pt : pl.pts) {
        if (pt.x >= 0.0) {
          zMinPos = std::min(zMinPos, pt.z);
          zMaxPos = std::max(zMaxPos, pt.z);
        } else {
          zMinNeg = std::min(zMinNeg, pt.z);
          zMaxNeg = std::max(zMaxNeg, pt.z);
        }
      }
    }
  }
  const double spanPos = (zMaxPos > zMinPos) ? (zMaxPos - zMinPos) : 0.0;
  const double spanNeg = (zMaxNeg > zMinNeg) ? (zMaxNeg - zMinNeg) : 0.0;
  const bool preferNegSide = !horiz && (spanNeg > spanPos + 5.0);

  std::vector<QPolygonF> thick, thin;
  for (const auto &pl : cut_) {
    for (size_t i = 0; i + 1 < pl.pts.size(); ++i) {
      const bool isPos = pl.pts[i].x + pl.pts[i + 1].x >= 0.0;
      const bool sec = horiz || (preferNegSide ? !isPos : isPos);
      auto &runs = sec ? thick : thin;
      const bool prevPos = i > 0 && (pl.pts[i - 1].x + pl.pts[i].x >= 0.0);
      const bool prevSec = horiz || (preferNegSide ? !prevPos : prevPos);
      const bool cont = i > 0 && (prevSec == sec);
      if (!cont)
        runs.push_back(QPolygonF{projectPoint(pl.pts[i], v)});
      runs.back() << projectPoint(pl.pts[i + 1], v);
    }
  }
  p.setPen(QPen(theme::ProfileRed, 1, Qt::DashLine));
  for (const auto &poly : thin)
    p.drawPolyline(poly);
  for (int pass = 0; pass < 2; ++pass) {
    p.setPen(QPen(pass == 0 ? QColor(Qt::white) : theme::ProfileRed,
                  pass == 0 ? 5.0 : 2.0, Qt::SolidLine, Qt::RoundCap,
                  Qt::RoundJoin));
    for (const auto &poly : thick)
      p.drawPolyline(poly);
  }
  // A / A′ 표지
  const QString a =
      horiz ? sl[size_t(slot_)].name.left(1) : QStringLiteral("A");
  auto tag = [&](QPointF c, const QString &s) {
    const QRectF r(c.x() - 13, c.y() - 10, 26, 20);
    p.setPen(QPen(theme::ProfileRed, 1));
    p.setBrush(theme::Surface);
    p.drawRoundedRect(r, 3, 3);
    p.setFont(theme::monoFont(12, true));
    p.setPen(theme::ProfileRed);
    p.drawText(r, Qt::AlignCenter, s);
  };
  tag((quad[2] + quad[3]) / 2, a);
  tag((quad[0] + quad[1]) / 2, a + QStringLiteral("′"));

  // [JOB-012] 3D 문양 탁본 선택 영역 표시 (황토·금색 점선 테두리 + 배지)
  if (doc_->hasRubbingSpot()) {
    const double azRad = doc_->rubbingAzimuthDeg() * std::numbers::pi / 180.0;
    const double zC = doc_->rubbingZ();
    const double halfH = 0.45 * doc_->rubbingSpanH();
    const double rSurf =
        std::max(15.0, doc_->outerRadiusAt(zC) > 0.0 ? doc_->outerRadiusAt(zC)
                                                     : 0.45 * doc_->dims().belly);
    const double dAng = std::clamp(0.45 * doc_->rubbingSpanW() / rSurf, 0.12, 0.55);
    QPolygonF patchPoly;
    for (double ang : {azRad - dAng, azRad, azRad + dAng})
      patchPoly << projectPoint(
          {rSurf * std::cos(ang), rSurf * std::sin(ang), zC + halfH}, v);
    for (double ang : {azRad + dAng, azRad, azRad - dAng})
      patchPoly << projectPoint(
          {rSurf * std::cos(ang), rSurf * std::sin(ang), zC - halfH}, v);
    patchPoly << patchPoly.front();

    p.setPen(QPen(QColor(255, 255, 255, 210), 3.2, Qt::SolidLine));
    p.setBrush(QColor(237, 137, 54, 34));
    p.drawPolygon(patchPoly);
    p.setPen(QPen(QColor(0xb7, 0x55, 0x16), 1.8, Qt::DashLine));
    p.drawPolygon(patchPoly);

    const QPointF rcPt = projectPoint(doc_->rubbingCenter3D(), v);
    p.setPen(QPen(Qt::white, 1.8));
    p.setBrush(QColor(0xb7, 0x55, 0x16));
    p.drawEllipse(rcPt, 4.5, 4.5);
    p.setFont(theme::uiFont(8, true));
    const QString rubLbl =
        QStringLiteral("◈ 문양 탁본 (H%1)").arg(int(std::lround(zC)));
    const QFontMetricsF rfm(p.font());
    const double rw = rfm.horizontalAdvance(rubLbl) + 10.0;
    const QRectF rBox(rcPt.x() - rw * 0.5, rcPt.y() - 22.0, rw, 16.0);
    p.setPen(QPen(QColor(0xb7, 0x55, 0x16), 1.0));
    p.setBrush(QColor(255, 252, 245, 235));
    p.drawRoundedRect(rBox, 3.0, 3.0);
    p.drawText(rBox, Qt::AlignCenter, rubLbl);
  }

  // [JOB-012] 사용자가 클릭해 지정한 3D 절단 위치 표지 (내가 여기를 잘랐구나 확인)
  if (!horiz && doc_->hasCutMarker()) {
    const double zM = doc_->cutMarkerZ();
    double rM = doc_->outerRadiusAt(zM);
    if (rM <= 0.0)
      rM = 0.45 * std::max(doc_->dims().belly, 40.0);
    const double xSign = preferNegSide ? -1.0 : 1.0;
    const QPointF cmPt = projectPoint({xSign * rM, 0.0, zM}, v);
    p.setPen(QPen(Qt::white, 2.5));
    p.setBrush(theme::ProfileRed);
    p.drawEllipse(cmPt, 6.5, 6.5);
    p.setPen(QPen(Qt::white, 1.5));
    p.drawLine(cmPt + QPointF(-4, 0), cmPt + QPointF(4, 0));
    p.drawLine(cmPt + QPointF(0, -4), cmPt + QPointF(0, 4));

    p.setFont(theme::uiFont(8, true));
    const QString cutLbl =
        QStringLiteral("✂ 절단 H %1 mm").arg(zM, 0, 'f', 1);
    const QFontMetricsF cfm(p.font());
    const double cw = cfm.horizontalAdvance(cutLbl) + 10.0;
    const QRectF cBox(cmPt.x() + 9.0, cmPt.y() - 9.0, cw, 18.0);
    p.setPen(QPen(theme::ProfileRed, 1.2));
    p.setBrush(QColor(255, 255, 255, 240));
    p.drawRoundedRect(cBox, 3.0, 3.0);
    p.setPen(theme::ProfileRed);
    p.drawText(cBox, Qt::AlignCenter, cutLbl);
  }

  // 수평 슬롯(Z 막 높이로)일 때 파란 높이 조절 손잡이(↕) 표시
  if (horiz) {
    const double z = sl[size_t(slot_)].z;
    const QPointF hPt = projectPoint({R, 0.0, z}, v);
    const QString hTxt = QStringLiteral("↕ H %1 mm").arg(z, 0, 'f', 1);
    p.setFont(theme::monoFont(10, true));
    const QFontMetricsF fm(p.font());
    const double hw = fm.horizontalAdvance(hTxt) + 14;
    horizHandleRect_ = QRectF(hPt.x() - hw / 2, hPt.y() - 12, hw, 24);
    p.setPen(QPen(Qt::white, 2.0));
    p.setBrush(QColor(0x2b, 0x6c, 0xb0));
    p.drawRoundedRect(horizHandleRect_, 5, 5);
    p.setPen(Qt::white);
    p.drawText(horizHandleRect_, Qt::AlignCenter, hTxt);
  }

  // Feature 18: 3D 회전판 및 빨간 손잡이 (#d8352a) + 우상단 2D 평면도 미니맵
  if (!horiz) {
    const double ringR = R * 1.12;
    QPolygonF ringPoly;
    for (int deg = 0; deg <= 360; deg += 6) {
      const double rad = deg * std::numbers::pi / 180.0;
      ringPoly << projectPoint(
          {ringR * std::cos(rad), ringR * std::sin(rad), z0}, v);
    }
    p.setPen(QPen(theme::Line, 1.2, Qt::SolidLine));
    p.setBrush(Qt::NoBrush);
    p.drawPolyline(ringPoly);

    // 눈금 (30° 간격)
    p.setPen(QPen(theme::Muted, 1.0));
    for (int deg = 0; deg < 360; deg += 30) {
      const double rad = deg * std::numbers::pi / 180.0;
      const QPointF p0 = projectPoint(
          {ringR * 0.96 * std::cos(rad), ringR * 0.96 * std::sin(rad), z0}, v);
      const QPointF p1 = projectPoint(
          {ringR * 1.04 * std::cos(rad), ringR * 1.04 * std::sin(rad), z0}, v);
      p.drawLine(p0, p1);
    }

    // 현재 각도 빨간 손잡이 (Red Knob #d8352a)
    const double curAng = doc_->previewAngle();
    const double fa = curAng * std::numbers::pi / 180.0;
    const QPointF knobPt =
        projectPoint({ringR * std::cos(fa), ringR * std::sin(fa), z0}, v);
    p.setPen(QPen(Qt::white, 2.0));
    p.setBrush(QColor(0xd8, 0x35, 0x2a));
    p.drawEllipse(knobPt, 7.0, 7.0);

    const QString angStr = QStringLiteral("%1°").arg(curAng, 0, 'f', 0);
    p.setFont(theme::monoFont(10, true));
    p.setPen(QColor(0xd8, 0x35, 0x2a));
    p.drawText(knobPt + QPointF(10, 4), angStr);

    // Feature 22: 짝 표시 보라 점 (#6b4fc0) at H=95 mm
    const double rMatch = doc_->outerRadiusAt(95.0);
    if (rMatch > 0.0) {
      const QPointF pPurple = projectPoint({rMatch, 0.0, 95.0}, v);
      p.setPen(QPen(Qt::white, 1.5));
      p.setBrush(QColor(0x6b, 0x4f, 0xc0));
      p.drawEllipse(pPurple, 5.0, 5.0);
      p.setFont(theme::monoFont(9, true));
      p.setPen(QColor(0x6b, 0x4f, 0xc0));
      p.drawText(pPurple + QPointF(8, 3), QStringLiteral("H 95 짝"));
    }
  }

  // 단면 위치 / 문양 탁본 1클릭 지정 모드 상단 안내 배지
  if (cutPickMode_ != CutPickMode::None) {
    const QString pickTxt =
        cutPickMode_ == CutPickMode::CutPoint1Click
            ? QStringLiteral("✂ 자르고 싶은 3D 유물 표면을 마우스로 클릭하세요")
            : QStringLiteral(
                  "◈ 문양 탁본을 뜰 3D 유물 표면을 마우스로 클릭하세요");
    const QColor pickCol = cutPickMode_ == CutPickMode::CutPoint1Click
                               ? theme::ProfileRed
                               : QColor(0xb7, 0x55, 0x16);
    p.setFont(theme::uiFont(9, true));
    const QFontMetricsF pfm(p.font());
    const double pw =
        std::min<double>(width() - 16, pfm.horizontalAdvance(pickTxt) + 16);
    const QRectF pr(8, 8, pw, 22);
    p.setPen(QPen(pickCol, 1.4));
    p.setBrush(QColor(255, 255, 255, 238));
    p.drawRoundedRect(pr, 4, 4);
    p.setPen(pickCol);
    p.drawText(pr, Qt::AlignCenter, pickTxt);
  }
}

void View3DCanvas::mousePressEvent(QMouseEvent *e) {
  last_ = e->pos();
  poseDrag_ = false;
  poseDragAxis_ = -1;
  cutDrag_ = false;
  minimapDrag_ = false;
  horizDrag_ = false;

  if (!doc_ || !doc_->isLoaded()) {
    drag_ = true;
    setCursor(Qt::ClosedHandCursor);
    return;
  }

  // ① 바닥 정하기 단계: 배지 클릭 / 표면 찍기 / 기즈모 고리 직접 드래그만 유물 회전 (일반 드래그는 시점 회전)
  if (!doc_->poseConfirmed()) {
    if (e->button() == Qt::LeftButton) {
      for (int i = 0; i < 3; ++i) {
        if (floorBadgeRects_[size_t(i)].isValid() &&
            floorBadgeRects_[size_t(i)].contains(e->position())) {
          emit floorCandidateClicked(i);
          return;
        }
      }
      if (posePickMode_ != PosePickMode::None) {
        KA::Point3 worldPt;
        if (pickSurfacePoint(e->position(), worldPt)) {
          emit surfacePointPicked(worldPt);
          return;
        }
      }
      poseDragAxis_ = hitGizmoAxis(e->position());
      poseDrag_ = (poseDragAxis_ >= 0);
      drag_ = true;
      setCursor(Qt::ClosedHandCursor);
      update();
      return;
    }
    drag_ = true;
    setCursor(Qt::ClosedHandCursor);
    return;
  }

  // ②·③ 확정 후 단계
  if (e->button() == Qt::LeftButton) {
    if (cutPickMode_ != CutPickMode::None) {
      KA::Point3 worldPt;
      if (pickSurfacePoint(e->position(), worldPt)) {
        const CutPickMode mode = cutPickMode_;
        cutPickMode_ = CutPickMode::None;
        if (mode == CutPickMode::CutPoint1Click)
          emit cutPointPicked(worldPt);
        else if (mode == CutPickMode::RubbingSpot1Click)
          emit rubbingSpotPicked(worldPt);
        update();
        return;
      }
    }
    if (minimapRect_.isValid() && minimapRect_.contains(e->position())) {
      minimapDrag_ = true;
      drag_ = true;
      const QPointF mc(minimapRect_.center().x(),
                       minimapRect_.top() + 16.0 +
                           (minimapRect_.height() - 18.0) * 0.5);
      double deg = std::atan2(mc.y() - e->position().y(),
                              e->position().x() - mc.x()) *
                   180.0 / std::numbers::pi;
      if (deg < 0.0)
        deg += 360.0;
      emit cutAngleSelected(deg);
      return;
    }
    const auto &sl = doc_->slotList();
    const bool horiz = slot_ >= 0 && slot_ < int(sl.size()) &&
                       sl[size_t(slot_)].kind == SlotKind::Horizontal;
    if (horiz && (cutTool_ || (horizHandleRect_.isValid() &&
                               horizHandleRect_.contains(e->position())))) {
      horizDrag_ = true;
      drag_ = true;
      setCursor(Qt::SizeVerCursor);
      return;
    }
  }

  drag_ = true;
  cutDrag_ = cutTool_ && e->button() == Qt::LeftButton;
  setCursor(Qt::ClosedHandCursor);
}

void View3DCanvas::mouseMoveEvent(QMouseEvent *e) {
  if (!drag_)
    return;
  const QPoint d = e->pos() - last_;
  last_ = e->pos();

  if (minimapDrag_ && minimapRect_.isValid() && doc_) {
    const QPointF mc(minimapRect_.center().x(),
                     minimapRect_.top() + 16.0 +
                         (minimapRect_.height() - 18.0) * 0.5);
    double deg = std::atan2(mc.y() - e->position().y(),
                            e->position().x() - mc.x()) *
                 180.0 / std::numbers::pi;
    if (deg < 0.0)
      deg += 360.0;
    emit cutAngleDragged(std::remainder(deg - doc_->previewAngle(), 360.0));
    return;
  }

  if (horizDrag_ && doc_ && doc_->isLoaded()) {
    const auto &sl = doc_->slotList();
    if (slot_ >= 0 && slot_ < int(sl.size()) &&
        sl[size_t(slot_)].kind == SlotKind::Horizontal) {
      const RasterView v = view();
      const double dz = -double(d.y()) / std::max(0.2, v.pxPerMm);
      emit horizontalHeightDragged(sl[size_t(slot_)].z + dz);
      return;
    }
  }

  if (poseDrag_ && doc_ && doc_->isLoaded() && !doc_->poseConfirmed()) {
    const bool snap15 =
        QGuiApplication::keyboardModifiers().testFlag(Qt::ShiftModifier);
    const double scale = snap15 ? 1.0 : 0.45;
    if (poseDragAxis_ == 0) {
      emit poseRotated(PoseAxis::X, (d.y() + d.x()) * scale);
    } else if (poseDragAxis_ == 1) {
      emit poseRotated(PoseAxis::Y, (d.x() - d.y()) * scale);
    } else if (poseDragAxis_ == 2) {
      emit poseRotated(PoseAxis::Z, d.x() * scale);
    }
    return;
  }

  if (cutDrag_) {
    emit cutAngleDragged(d.x() * 0.5);
    return;
  }
  yaw_ += d.x() * 0.6;
  pitch_ = std::clamp(pitch_ + d.y() * 0.5, -89.0, 89.0);
  dirty_ = true;
  update();
}

void View3DCanvas::mouseReleaseEvent(QMouseEvent *) {
  const bool cutDrag = drag_ && (cutDrag_ || minimapDrag_);
  drag_ = false;
  poseDrag_ = false;
  poseDragAxis_ = -1;
  minimapDrag_ = false;
  horizDrag_ = false;
  setCursor(Qt::OpenHandCursor);
  update();
  if (cutDrag)
    emit cutDragReleased();
}

void View3DCanvas::wheelEvent(QWheelEvent *e) {
  // 마우스 휠로 유물 자세가 기울어지지 않도록 보호 (유물은 바닥판에 고정)
  QWidget::wheelEvent(e);
}

// ---- View3DPane -----------------------------------------------------------
View3DPane::View3DPane(QWidget *parent) : QWidget(parent) {
  setObjectName("View3DPane");
  setAttribute(Qt::WA_StyledBackground, true);
  auto *v = new QVBoxLayout(this);
  v->setContentsMargins(0, 0, 0, 0);
  v->setSpacing(0);

  auto *head = new QWidget(this);
  head->setObjectName("PaneHead");
  head->setAttribute(Qt::WA_StyledBackground, true);
  head->setFixedHeight(32);
  auto *h = new QHBoxLayout(head);
  h->setContentsMargins(12, 0, 6, 0);
  h->setSpacing(4);
  title_ = new QLabel(
      QStringLiteral("<b>3D 유물</b> <span style='color:%1'>막 · 중심축</span>")
          .arg(theme::hex(theme::Muted)),
      head);
  title_->setObjectName("View3DTitle");
  title_->setProperty("ph", true);
  h->addWidget(title_);
  h->addStretch();
  auto mk = [&](const QString &s, const QString &tip) {
    auto *b = new QPushButton(s, head);
    b->setProperty("icon-btn", true);
    b->setStyleSheet(QStringLiteral("min-width:34px; max-width:40px;"));
    b->setToolTip(tip);
    b->setFocusPolicy(Qt::NoFocus);
    h->addWidget(b);
    return b;
  };
  auto *front = mk(QStringLiteral("정면"), QStringLiteral("정면에서 보기"));
  auto *top = mk(QStringLiteral("위"), QStringLiteral("위에서 보기"));
  auto *coll = mk(QStringLiteral("◀"), QStringLiteral("3D 보조창 접기"));
  v->addWidget(head);

  auto *info = new QWidget(this);
  info->setObjectName("View3DInfo");
  info->setAttribute(Qt::WA_StyledBackground, true);
  info->setFixedHeight(28);
  auto *ih = new QHBoxLayout(info);
  ih->setContentsMargins(12, 0, 12, 0);
  info_ = new QLabel(info);
  info2_ = new QLabel(info);
  info_->setFont(theme::monoFont(12));
  info2_->setFont(theme::monoFont(12));
  ih->addWidget(info_);
  ih->addStretch();
  ih->addWidget(info2_);
  v->addWidget(info);

  // ① 바닥 정하기 전용 빠른 자세 조절 막대 (미세 각도 · 중앙 맞춤 · 표면 클릭 · 3점 찍기)
  poseBar_ = new QWidget(this);
  poseBar_->setObjectName("PoseQuickBar");
  poseBar_->setStyleSheet(
      QStringLiteral("background:%1; border-bottom:1px solid %2;")
          .arg(theme::hex(theme::Surface), theme::hex(theme::Line)));
  auto *pv = new QVBoxLayout(poseBar_);
  pv->setContentsMargins(8, 6, 8, 6);
  pv->setSpacing(4);

  auto makeMiniBtn = [&](const QString &txt, const QString &tip,
                         QWidget *rowParent) {
    auto *b = new QPushButton(txt, rowParent);
    b->setProperty("sbtn", true);
    b->setFocusPolicy(Qt::NoFocus);
    b->setToolTip(tip);
    b->setStyleSheet(QStringLiteral(
        "padding:1px 4px; font-size:10px; min-height:20px; min-width:18px;"));
    return b;
  };

  auto *poseRow1 = new QWidget(poseBar_);
  auto *pr1 = new QHBoxLayout(poseRow1);
  pr1->setContentsMargins(0, 0, 0, 0);
  pr1->setSpacing(2);
  auto addAxisStepper = [&](const QString &axName, PoseAxis ax,
                            const QString &colorHex) {
    auto *lbl = new QLabel(
        QStringLiteral("<b style='color:%1'>%2</b>").arg(colorHex, axName),
        poseRow1);
    lbl->setStyleSheet(QStringLiteral("font-size:10px;"));
    pr1->addWidget(lbl);
    for (double deg : {-15.0, -5.0, -1.0, 1.0, 5.0, 15.0}) {
      const QString s =
          deg > 0 ? QStringLiteral("+%1").arg(int(deg))
                  : QStringLiteral("%1").arg(int(deg));
      auto *b = makeMiniBtn(
          s, QStringLiteral("%1축 %2° 미세 회전").arg(axName, s), poseRow1);
      connect(b, &QPushButton::clicked, this,
              [this, ax, deg] { emit poseRotateRequested(ax, deg); });
      pr1->addWidget(b);
    }
  };
  addAxisStepper(QStringLiteral("X"), PoseAxis::X, QStringLiteral("#c8641e"));
  pr1->addSpacing(3);
  addAxisStepper(QStringLiteral("Y"), PoseAxis::Y, QStringLiteral("#3f8f4f"));
  pr1->addStretch();
  pv->addWidget(poseRow1);

  auto *poseRow2 = new QWidget(poseBar_);
  auto *pr2 = new QHBoxLayout(poseRow2);
  pr2->setContentsMargins(0, 0, 0, 0);
  pr2->setSpacing(3);
  auto *btnAuto = makeMiniBtn(
      QStringLiteral("⚡ 자동 세우기"),
      QStringLiteral("구연부·대칭축을 자동으로 찾아 바르게 세웁니다"), poseRow2);
  auto *btnCenter = makeMiniBtn(
      QStringLiteral("⊕ 중앙 맞춤"),
      QStringLiteral("유물을 바닥판 한가운데(XY=0, Z=0)에 반듯하게 놓습니다"),
      poseRow2);
  btnSurfacePick_ = makeMiniBtn(
      QStringLiteral("👇 바닥 1클릭"),
      QStringLiteral("3D 유물에서 바닥이 될 면을 클릭하면 즉시 그 면으로 세웁니다"),
      poseRow2);
  btnSurfacePick_->setCheckable(true);
  btn3PtPick_ = makeMiniBtn(
      QStringLiteral("①②③ 3점 바닥"),
      QStringLiteral("바닥 둘레 3점을 클릭해 수평 바닥면을 지정합니다"),
      poseRow2);
  btn3PtPick_->setCheckable(true);

  connect(btnAuto, &QPushButton::clicked, this,
          [this] { emit actionRequested(QStringLiteral("poseAuto")); });
  connect(btnCenter, &QPushButton::clicked, this,
          [this] { emit actionRequested(QStringLiteral("poseCenter")); });
  connect(btnSurfacePick_, &QPushButton::clicked, this, [this](bool checked) {
    if (checked)
      btn3PtPick_->setChecked(false);
    canvas_->setPosePickMode(checked ? PosePickMode::Surface1Click
                                     : PosePickMode::None);
  });
  connect(btn3PtPick_, &QPushButton::clicked, this, [this](bool checked) {
    if (checked)
      btnSurfacePick_->setChecked(false);
    canvas_->setPosePickMode(checked ? PosePickMode::Floor3Point
                                     : PosePickMode::None);
    emit actionRequested(QStringLiteral("pose3PtClear"));
  });

  pr2->addWidget(btnAuto);
  pr2->addWidget(btnCenter);
  pr2->addWidget(btnSurfacePick_);
  pr2->addWidget(btn3PtPick_);
  pr2->addStretch();
  pv->addWidget(poseRow2);
  poseBar_->hide();
  v->addWidget(poseBar_);

  // ② 막으로 자르기 전용 빠른 단면 자리 선택 막대 (각도 슬라이더 · Z막 높이 슬라이더 · 단면 원클릭 추가)
  cutBar_ = new QWidget(this);
  cutBar_->setObjectName("CutQuickBar");
  cutBar_->setStyleSheet(
      QStringLiteral("background:%1; border-bottom:1px solid %2;")
          .arg(theme::hex(theme::Surface), theme::hex(theme::Line)));
  auto *cv = new QVBoxLayout(cutBar_);
  cv->setContentsMargins(8, 5, 8, 5);
  cv->setSpacing(4);

  auto *angRow = new QWidget(cutBar_);
  auto *ah = new QHBoxLayout(angRow);
  ah->setContentsMargins(0, 0, 0, 0);
  ah->setSpacing(4);
  auto *angTitle = new QLabel(QStringLiteral("<b>단면 각도</b>"), angRow);
  angTitle->setStyleSheet(QStringLiteral("font-size:11px;"));
  angleSlider_ = new QSlider(Qt::Horizontal, angRow);
  angleSlider_->setRange(0, 360);
  angleSlider_->setToolTip(
      QStringLiteral("슬라이더를 끌어 0°~360° 어디든 쉽게 단면 자리를 고릅니다"));
  angleSliderLbl_ = new QLabel(QStringLiteral("0°"), angRow);
  angleSliderLbl_->setFont(theme::monoFont(10, true));
  angleSliderLbl_->setFixedWidth(36);
  auto *btnAddVert = makeMiniBtn(
      QStringLiteral("+ 이 자리 종단면 추가"),
      QStringLiteral("지금 보고 있는 각도의 종단면을 도면 목록에 추가합니다"),
      angRow);
  auto *btnZMode = makeMiniBtn(
      QStringLiteral("↕ Z 막 높이로"),
      QStringLiteral("수평 횡단면(Z 막) 모드로 전환하여 높이를 위아래로 골라 자릅니다"),
      angRow);
  ah->addWidget(angTitle);
  ah->addWidget(angleSlider_, 1);
  ah->addWidget(angleSliderLbl_);
  ah->addWidget(btnAddVert);
  ah->addWidget(btnZMode);
  cv->addWidget(angRow);

  heightRow_ = new QWidget(cutBar_);
  auto *hh = new QHBoxLayout(heightRow_);
  hh->setContentsMargins(0, 0, 0, 0);
  hh->setSpacing(4);
  auto *hTitle =
      new QLabel(QStringLiteral("<b style='color:#2b6cb0'>횡단면 높이 H</b>"),
                 heightRow_);
  hTitle->setStyleSheet(QStringLiteral("font-size:11px;"));
  heightSlider_ = new QSlider(Qt::Horizontal, heightRow_);
  heightSlider_->setRange(5, 2000);
  heightSlider_->setToolTip(
      QStringLiteral("수평 절단면(Z 막)의 높이를 위아래로 쉽게 조절합니다"));
  heightSliderLbl_ = new QLabel(QStringLiteral("95.0 mm"), heightRow_);
  heightSliderLbl_->setFont(theme::monoFont(10, true));
  heightSliderLbl_->setFixedWidth(56);
  auto *btnAddHoriz = makeMiniBtn(
      QStringLiteral("+ 이 높이 횡단면 추가"),
      QStringLiteral("지금 높이의 수평 횡단면을 도면 목록에 추가합니다"),
      heightRow_);
  hh->addWidget(hTitle);
  hh->addWidget(heightSlider_, 1);
  hh->addWidget(heightSliderLbl_);
  hh->addWidget(btnAddHoriz);
  cv->addWidget(heightRow_);

  connect(angleSlider_, &QSlider::valueChanged, this, [this](int val) {
    if (angleSlider_->isSliderDown()) {
      angleSliderLbl_->setText(QStringLiteral("%1°").arg(val % 360));
    } else {
      emit angleSelected(double(val));
    }
  });
  connect(angleSlider_, &QSlider::sliderReleased, this, [this] {
    emit angleSelected(double(angleSlider_->value()));
  });
  connect(heightSlider_, &QSlider::valueChanged, this, [this](int tenthsMm) {
    emit horizontalHeightSelected(double(tenthsMm) * 0.1);
  });
  connect(btnAddVert, &QPushButton::clicked, this,
          [this] { emit actionRequested(QStringLiteral("addVerticalSlot")); });
  connect(btnZMode, &QPushButton::clicked, this,
          [this] { emit actionRequested(QStringLiteral("axZ")); });
  connect(btnAddHoriz, &QPushButton::clicked, this,
          [this] { emit actionRequested(QStringLiteral("addSlot")); });

  cutBar_->hide();
  v->addWidget(cutBar_);

  // ②·③ 확정 후: 3D 표면 1클릭 단면 위치 지정 · 문양 탁본 부위 선택 빠른 막대
  pickBar_ = new QWidget(this);
  pickBar_->setObjectName("PickQuickBar");
  pickBar_->setStyleSheet(
      QStringLiteral("background:%1; border-bottom:1px solid %2;")
          .arg(theme::hex(theme::Surface), theme::hex(theme::Line)));
  auto *pk = new QHBoxLayout(pickBar_);
  pk->setContentsMargins(6, 4, 6, 4);
  pk->setSpacing(4);
  btnPickCut_ = makeMiniBtn(
      QStringLiteral("✂ 클릭해 단면 위치 지정"),
      QStringLiteral(
          "3D 유물에서 자르고 싶은 곳을 마우스로 클릭하면 정확히 그 자리로 "
          "단면을 자르고 도면에 표시합니다"),
      pickBar_);
  btnPickCut_->setCheckable(true);
  btnPickRubbing_ = makeMiniBtn(
      QStringLiteral("◈ 클릭해 문양 탁본 선택"),
      QStringLiteral(
          "3D 유물에서 탁본을 뜰 문양 부위를 마우스로 클릭하면 옆 도면에 "
          "계산·색표현이 완료된 정밀 문양 탁본을 출력합니다"),
      pickBar_);
  btnPickRubbing_->setCheckable(true);
  pk->addWidget(btnPickCut_);
  pk->addWidget(btnPickRubbing_);
  pk->addStretch();
  pickBar_->hide();
  v->addWidget(pickBar_);

  // [JOB-013] 왼쪽 상단: 위에서 본 실제 유물 평면도 (클릭·끌어 세로단면 회전)
  planView_ = new PlanViewWidget(this);
  planView_->hide();
  v->addWidget(planView_);

  // [JOB-013] 왼쪽 하단: 앞에서 본 유물 정면·3D (가로단면 높이 · 문양 탁본 선택)
  canvas_ = new View3DCanvas(this);
  v->addWidget(canvas_, 1);

  connect(planView_, &PlanViewWidget::anglePreviewed, this,
          &View3DPane::anglePreviewed);
  connect(planView_, &PlanViewWidget::angleCommitted, this,
          &View3DPane::angleSelected);

  connect(btnPickCut_, &QPushButton::clicked, this, [this](bool checked) {
    if (checked)
      btnPickRubbing_->setChecked(false);
    canvas_->setCutPickMode(checked ? CutPickMode::CutPoint1Click
                                    : CutPickMode::None);
  });
  connect(btnPickRubbing_, &QPushButton::clicked, this, [this](bool checked) {
    if (checked)
      btnPickCut_->setChecked(false);
    canvas_->setCutPickMode(checked ? CutPickMode::RubbingSpot1Click
                                    : CutPickMode::None);
  });
  connect(canvas_, &View3DCanvas::cutPointPicked, this,
          &View3DPane::cutPointPicked);
  connect(canvas_, &View3DCanvas::rubbingSpotPicked, this,
          &View3DPane::rubbingSpotPicked);

  angleStrip_ = new AngleStripWidget(this);
  angleStrip_->setObjectName("AngleStrip");
  angleStrip_->hide();
  v->addWidget(angleStrip_);

  connect(angleStrip_, &AngleStripWidget::angleClicked, this,
          &View3DPane::angleSelected);
  connect(canvas_, &View3DCanvas::cutAngleSelected, this,
          &View3DPane::angleSelected);
  connect(canvas_, &View3DCanvas::poseRotated, this,
          &View3DPane::poseRotateRequested);
  connect(canvas_, &View3DCanvas::floorCandidateClicked, this, [this](int idx) {
    emit actionRequested(idx == 0   ? QStringLiteral("floorCand1")
                         : idx == 1 ? QStringLiteral("floorCand2")
                                    : QStringLiteral("floorCand3"));
  });
  connect(canvas_, &View3DCanvas::horizontalHeightDragged, this,
          &View3DPane::horizontalHeightSelected);

  connect(front, &QPushButton::clicked, this,
          [this] { canvas_->setView(0, 0); });
  connect(top, &QPushButton::clicked, this,
          [this] { canvas_->setView(0, 89); });
  connect(coll, &QPushButton::clicked, this, &View3DPane::collapseRequested);
}

void View3DPane::setCutPage(bool on) {
  cutPage_ = on;
  canvas_->setToolTip(on ? cutpage::k3dTip : QString());
}

void View3DPane::refresh(const ArtifactDoc &doc, int slot) {
  canvas_->setDoc(&doc, slot);
  if (planView_) {
    planView_->setDoc(&doc);
    planView_->setVisible(doc.isLoaded() && doc.poseConfirmed());
  }
  const QString mu = theme::hex(theme::Muted);
  const bool base = doc.isLoaded() && !doc.poseConfirmed();
  const bool showCutControls = doc.isLoaded() && doc.poseConfirmed() && cutPage_;

  poseBar_->setVisible(base);
  if (btnSurfacePick_) {
    QSignalBlocker b(btnSurfacePick_);
    btnSurfacePick_->setChecked(canvas_->posePickMode() ==
                                PosePickMode::Surface1Click);
  }
  if (btn3PtPick_) {
    QSignalBlocker b(btn3PtPick_);
    btn3PtPick_->setChecked(canvas_->posePickMode() ==
                            PosePickMode::Floor3Point);
  }
  cutBar_->setVisible(showCutControls);
  angleStrip_->setVisible(showCutControls);
  if (pickBar_) {
    pickBar_->setVisible(doc.isLoaded() && doc.poseConfirmed());
    if (btnPickCut_) {
      QSignalBlocker b(btnPickCut_);
      btnPickCut_->setChecked(canvas_->cutPickMode() ==
                              CutPickMode::CutPoint1Click);
    }
    if (btnPickRubbing_) {
      QSignalBlocker b(btnPickRubbing_);
      btnPickRubbing_->setChecked(canvas_->cutPickMode() ==
                                  CutPickMode::RubbingSpot1Click);
    }
  }

  if (showCutControls) {
    const double curAng =
        std::fmod(std::fmod(doc.previewAngle(), 360.0) + 360.0, 360.0);
    {
      QSignalBlocker b(angleSlider_);
      angleSlider_->setValue(int(std::lround(curAng)) % 360);
    }
    angleSliderLbl_->setText(
        QStringLiteral("%1°").arg(int(std::lround(curAng)) % 360));

    const auto &sl = doc.slotList();
    const bool horiz = slot >= 0 && slot < int(sl.size()) &&
                       sl[size_t(slot)].kind == SlotKind::Horizontal;
    heightRow_->setVisible(true);
    const double maxH = std::max(
        10.0, doc.dims().height > 1.0
                  ? doc.dims().height
                  : (doc.mesh().bboxMax.z - doc.mesh().bboxMin.z));
    {
      QSignalBlocker b(heightSlider_);
      heightSlider_->setRange(5, int(std::lround(maxH * 10.0)) - 5);
      const double curZ = horiz ? sl[size_t(slot)].z
                                : (doc.dims().bellyZ > 0.0 ? doc.dims().bellyZ
                                                           : maxH * 0.5);
      heightSlider_->setValue(int(std::lround(curZ * 10.0)));
      heightSliderLbl_->setText(QStringLiteral("%1 mm").arg(curZ, 0, 'f', 1));
    }
    if (!doc.isPreviewing()) {
      angleStrip_->setCells(doc.angleStripCells(), curAng);
    }
  }

  title_->setText(
      base       ? QStringLiteral("<b>3D — 바닥판(실측판)에 올려놓기</b>")
      : cutPage_ ? QStringLiteral("<b>%1</b>").arg(cutpage::k3dTitle)
      : editPage_
          ? QStringLiteral("<b>%1</b>").arg(cutpage::k3dEditTitle)
          : QStringLiteral(
                "<b>3D 유물</b> <span style='color:%1'>막 · 중심축</span>")
                .arg(mu));
  info_->setText(
      QStringLiteral("<span style='color:%1; font-family:Malgun "
                     "Gothic'>메시</span> <b>%2</b><span style='color:%1; "
                     "font-family:Malgun Gothic'>면</span>")
          .arg(mu, QLocale(QLocale::English)
                       .toString(qlonglong(doc.mesh().faces.size()))));
  info_->setToolTip(doc.scannerProfile());
  if (base) {
    info2_->setText(
        QStringLiteral("<span style='font-family:Malgun Gothic'>%1</span>")
            .arg(doc.groundNote()));
  } else {
    // 사람이 확정한 자세(자동 세우기 없음)는 축 오차가 없다 → —
    const QString rmse =
        doc.axisRmse() >= 0
            ? QStringLiteral("%1 mm").arg(doc.axisRmse(), 0, 'f', 2)
            : QStringLiteral("—");
    info2_->setText(QStringLiteral("<span style='color:%1; font-family:Malgun "
                                   "Gothic'>중심축 오차</span> %2")
                        .arg(mu, rmse));
  }
}

} // namespace studio
