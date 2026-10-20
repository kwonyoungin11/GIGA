#include "Plate.h"

#include "DxfWriter.h"
#include "Theme.h"

#include <QBuffer>
#include <QFile>
#include <QPainterPath>
#include <QPdfWriter>
#include <QTextStream>
#include <algorithm>
#include <cmath>
#include <string>

namespace studio {

namespace {

constexpr double kSvgSection = 0.35, kSvgOutline = 0.50;

double maxR(const ArtifactDoc &doc) {
  double r = 10.0;
  for (const auto &q : doc.displayOutline())
    r = std::max(r, std::fabs(q.x));
  for (const auto &poly : doc.displaySection())
    for (const auto &q : poly)
      r = std::max(r, std::fabs(q.x));
  return r;
}

double height(const ArtifactDoc &doc) {
  return std::max(1.0, doc.mesh().bboxMax.z - doc.mesh().bboxMin.z);
}

bool isHorizontal(const ArtifactDoc &doc, int slot) {
  return slot >= 0 && slot < int(doc.slotList().size()) &&
         doc.slotList()[size_t(slot)].kind == SlotKind::Horizontal;
}

QString num(double v, int prec = 1) { return QString::number(v, 'f', prec); }

// 정면 방위각 글자 — 정수 0.1° 로 부호·소수를 직접 조립(/fp:fast 에서도 −0 이
// 생기지 않음). 도판 글: 12° · 37.5° · −20°(U+2212) · −0.04 → 0°, DXF 글: 12.0
// · -20.0 · 0.0
QString azimuthText(double deg) {
  const long long t = std::llround(deg * 10.0), a = t < 0 ? -t : t;
  return (t < 0 ? QStringLiteral("−") : QString()) + QString::number(a / 10) +
         (a % 10 ? QStringLiteral(".%1").arg(a % 10) : QString()) +
         QStringLiteral("°");
}
std::string azimuthDxf(double deg) {
  const long long t = std::llround(deg * 10.0), a = t < 0 ? -t : t;
  return (t < 0 ? "-" : "") + std::to_string(a / 10) + "." +
         std::to_string(a % 10);
}

// 외곽선 닫는 가로선(축 → 끝점)을 그릴 끝: y 가 작은 끝은 바닥선, 큰 끝은
// 구연선이 있으면 뺀다 — 그 높이에 기준선이 있어 두 번째 가로선이 생기지 않게
std::vector<KA::Point2> closingEnds(const std::vector<KA::Point2> &ol,
                                    const PlateRefLines &refs) {
  std::vector<KA::Point2> v;
  if (ol.size() < 2)
    return v;
  const bool frontLow = ol.front().y <= ol.back().y;
  if (!refs.base)
    v.push_back(frontLow ? ol.front() : ol.back());
  if (!refs.rim)
    v.push_back(frontLow ? ol.back() : ol.front());
  return v;
}

// 기준선 세 줄의 출력 규약 (디자인 v2 가이드 「도면 선 규약」, 그리는 순서
// 중심선 → 구연선 → 바닥선): SVG 층 id·이름, 굵기 종이 mm / 화면 px, DXF 층
struct RefStyle {
  std::optional<RefLine> PlateRefLines::*line;
  const char *id, *name, *dxfLayer;
  double paperMm, screenPx;
};
const RefStyle kRefStyles[] = {{&PlateRefLines::center, "refline_center",
                                "중심선", "01_AXIS_CENTER", 0.10, 1.0},
                               {&PlateRefLines::rim, "refline_rim", "구연선",
                                "07_PROJECTION_RIM", 0.13, 1.0},
                               {&PlateRefLines::base, "refline_base", "바닥선",
                                "08_PROJECTION_BASE", 0.25, 1.4}};

// 막을 움직이는 중(LOD 미리보기 단면 · 치수·두께는 옛 각)에는 파일을 만들기
// 전에 거부한다
bool refusePreview(const ArtifactDoc &doc, QString *err) {
  if (!doc.isPreviewing())
    return false;
  if (err)
    *err = QStringLiteral(
        "막을 움직이는 중입니다 — 놓은 뒤(원본 단면 계산 뒤) 내보내세요.");
  return true;
}

QPolygonF mapPoly(const std::vector<KA::Point2> &pts, const QTransform &t) {
  QPolygonF poly;
  poly.reserve(qsizetype(pts.size()));
  for (const auto &q : pts)
    poly << t.map(QPointF(q.x, q.y));
  return poly;
}

void labelBox(QPainter &p, QPointF anchor, const QString &text, const QFont &f,
              Qt::Alignment al) {
  p.setFont(f);
  const QFontMetricsF fm(f);
  const double pad = fm.height() * 0.3;
  QRectF r(0, 0, fm.horizontalAdvance(text) + 2 * pad, fm.height() + pad);
  if (al & Qt::AlignHCenter)
    r.moveCenter(QPointF(anchor.x(), r.center().y()));
  else if (al & Qt::AlignRight)
    r.moveRight(anchor.x());
  else
    r.moveLeft(anchor.x());
  if (al & Qt::AlignVCenter)
    r.moveTop(anchor.y() - r.height() / 2);
  else if (al & Qt::AlignBottom)
    r.moveBottom(anchor.y());
  else
    r.moveTop(anchor.y());
  p.setPen(QPen(theme::Edge, 1));
  p.setBrush(theme::Surface);
  p.drawRect(r);
  p.setPen(theme::Ink);
  p.drawText(r, Qt::AlignCenter, text);
}

void scaleBar(QPainter &p, QPointF leftBottom, double px1mm, double lenMm,
              const QFont &f, const QString &caption) {
  const double w = lenMm * px1mm, h = std::max(3.0, 0.8 * f.pixelSize() * 0.5);
  p.setPen(QPen(theme::Ink, 1));
  for (int i = 0; i < 2; ++i) {
    p.setBrush(i == 0 ? theme::Ink : theme::Surface);
    p.drawRect(
        QRectF(leftBottom.x() + i * w / 2, leftBottom.y() - h, w / 2, h));
  }
  p.setFont(f);
  const QFontMetricsF fm(f);
  p.drawText(QPointF(leftBottom.x() - fm.horizontalAdvance("0") / 2,
                     leftBottom.y() + fm.ascent() + 2),
             "0");
  const QString t = caption.isEmpty()
                        ? QStringLiteral("%1 mm").arg(lenMm, 0, 'f', 0)
                        : caption;
  p.drawText(QPointF(leftBottom.x() + w - fm.horizontalAdvance(t) / 2,
                     leftBottom.y() + fm.ascent() + 2),
             t);
}

} // namespace

PlateRefLines plateRefLines(const ArtifactDoc &doc, double extMm) {
  PlateRefLines r;
  if (!doc.isLoaded() || !doc.poseConfirmed())
    return r;
  const double H = height(doc);
  const auto &d = doc.dims();
  r.center = RefLine{{0, -extMm}, {0, H + extMm}};
  if (d.rim > 0)
    r.rim = RefLine{{-d.rim / 2, H}, {d.rim / 2, H}};
  if (d.base > 0)
    r.base = RefLine{{-d.base / 2, 0}, {d.base / 2, 0}};

  // Feature 24: 경부선 (목 높이) — H의 0.55 ~ 0.95 구간에서 외곽선/단면의 최소
  // 반경(잘록한 목) 탐색
  double minR = 1e9;
  double neckZ = -1.0;
  double neckR = 0.0;
  const auto outline = doc.displayOutline();
  for (const auto &p : outline) {
    if (p.y >= 0.55 * H && p.y <= 0.95 * H) {
      double rad = std::fabs(p.x);
      if (rad < minR) {
        minR = rad;
        neckZ = p.y;
        neckR = rad;
      }
    }
  }
  if (neckZ <= 0.0) {
    for (const auto &poly : doc.displaySection()) {
      for (const auto &p : poly) {
        if (p.y >= 0.55 * H && p.y <= 0.95 * H) {
          double rad = std::fabs(p.x);
          if (rad < minR) {
            minR = rad;
            neckZ = p.y;
            neckR = rad;
          }
        }
      }
    }
  }
  if (neckZ > 0.0 && neckZ < H) {
    r.neck = RefLine{{-neckR, neckZ}, {neckR, neckZ}};
  }
  return r;
}

QString azimuthNote(const ArtifactDoc &doc, int slot) {
  if (!doc.isLoaded() || !doc.poseConfirmed() || isHorizontal(doc, slot))
    return QString();
  return QStringLiteral("정면 방위각 ") + azimuthText(doc.frontAngle()) +
         QStringLiteral(" (위에서 보아 반시계)");
}

QRectF Plate::extentMm(const ArtifactDoc &doc, int slot) {
  if (!doc.isLoaded())
    return QRectF(-60, -10, 120, 140);
  if (isHorizontal(doc, slot)) {
    const auto &m = doc.mesh();
    const double r =
        std::max({std::fabs(m.bboxMin.x), std::fabs(m.bboxMax.x),
                  std::fabs(m.bboxMin.y), std::fabs(m.bboxMax.y)}) +
        10.0;
    return QRectF(-r, -r, 2 * r, 2 * r);
  }
  const double r = maxR(doc) + 8.0, h = height(doc);
  return QRectF(-r, -8.0, 2 * r, h + 26.0);
}

ShadeImage Plate::renderShade(const ArtifactDoc &doc, ShadeMode mode,
                              double pxPerMm) {
  ShadeImage s;
  if (!doc.isLoaded())
    return s;
  const double r = maxR(doc) + 2.0, h = height(doc) + 4.0;
  pxPerMm = std::clamp(pxPerMm, 0.5, std::min(4000.0 / (2 * r), 4000.0 / h));
  RasterView v;
  // D5: Right(기본) = 정면(카메라 −Y, yaw 0°)에서 본 x<0 반쪽 → 영상 왼쪽
  // 절반이 외면. Left(R 반전) = 뒤(yaw 180°)에서 본 모양 → 영상 오른쪽 절반이
  // 외면.
  v.yawDeg = doc.sectionSide() == SectionSide::Right ? 0.0 : 180.0;
  v.pitchDeg = 0.0;
  v.pxPerMm = pxPerMm;
  const QSize sz(int(std::ceil(2 * r * pxPerMm)), int(std::ceil(h * pxPerMm)));
  v.centerPx = QPointF(sz.width() / 2.0, sz.height() / 2.0);
  v.centerZ = (h - 4.0) / 2.0 + doc.mesh().bboxMin.z;
  s.image = renderMesh(doc.mesh(), sz, v, mode, theme::ModelClay, nullptr,
                       doc.hasColor() ? &doc.vertexColors() : nullptr);
  s.rectMm = QRectF(-r, v.centerZ - h / 2.0, 2 * r, h);
  return s;
}

QImage Plate::renderRubbingPatch(const ArtifactDoc &doc, double pxPerMm) {
  if (!doc.isLoaded() || !doc.poseConfirmed() || !doc.hasRubbingSpot())
    return {};
  const double wMm = std::clamp(doc.rubbingSpanW(), 20.0, 90.0);
  const double hMm = std::clamp(doc.rubbingSpanH(), 20.0, 80.0);
  pxPerMm = std::clamp(pxPerMm, 1.5, 8.0);
  RasterView v;
  // 선택된 문양 방위각(rubbingAzimuthDeg)의 외면 법선 정면에서 직교 투영
  v.yawDeg = 270.0 - doc.rubbingAzimuthDeg();
  v.pitchDeg = 0.0;
  v.pxPerMm = pxPerMm;
  const QSize sz(std::max(32, int(std::ceil(wMm * pxPerMm))),
                 std::max(32, int(std::ceil(hMm * pxPerMm))));
  v.centerPx = QPointF(sz.width() / 2.0, sz.height() / 2.0);
  v.centerX = 0.0;
  v.centerY = 0.0;
  v.centerZ = doc.rubbingZ();
  const auto &m =
      (doc.isPreviewing() && !doc.lodMesh().faces.empty()) ? doc.lodMesh()
                                                           : doc.mesh();
  QImage raw = renderMesh(m, sz, v, ShadeMode::DTak, theme::ModelClay, nullptr,
                          nullptr);
  // 배경을 따뜻한 한지 바탕색(#f8f6f0)으로 합성해 완성된 탁본 도판으로 반환
  QImage out(sz, QImage::Format_RGB32);
  out.fill(QColor(248, 246, 240));
  QPainter qp(&out);
  qp.drawImage(0, 0, raw);
  qp.end();
  return out;
}

void Plate::paint(QPainter &p, const ArtifactDoc &doc, const QTransform &t,
                  double px1mm, double textPx, const PlateOptions &o,
                  const ShadeImage *shade) {
  if (!doc.isLoaded())
    return;
  p.save();
  p.setRenderHint(QPainter::Antialiasing, true);
  const QRectF E = extentMm(doc, o.slot);
  const QRectF Edev = t.mapRect(E);
  const QFont mono = theme::monoFont(int(std::lround(std::max(8.0, textPx))));
  const QFont sans = theme::uiFont(int(std::lround(std::max(8.0, textPx))));
  const bool horiz = isHorizontal(doc, o.slot);
  const double H = height(doc);
  const double lineSec = std::max(1.5, 0.35 * px1mm),
               lineOut = std::max(1.2, 0.30 * px1mm);

  // 1) 방안 눈금 ------------------------------------------------------------
  if (o.grid) {
    p.setClipRect(Edev.adjusted(-1, -1, 1, 1));
    const bool fine = px1mm >= 2.5;
    for (int pass = 0; pass < 2; ++pass) {
      const int step = pass == 0 ? 1 : 10;
      if (pass == 0 && !fine)
        continue;
      p.setPen(QPen(pass == 0 ? theme::Grid1mm : theme::Grid10mm,
                    pass == 0 ? 0.75 : 1.0));
      for (int x = int(std::ceil(E.left())); x <= int(std::floor(E.right()));
           ++x) {
        if (x % step)
          continue;
        if (pass == 0 && x % 10 == 0)
          continue;
        p.drawLine(t.map(QPointF(x, E.top())), t.map(QPointF(x, E.bottom())));
      }
      for (int y = int(std::ceil(E.top())); y <= int(std::floor(E.bottom()));
           ++y) {
        if (y % step)
          continue;
        if (pass == 0 && y % 10 == 0)
          continue;
        p.drawLine(t.map(QPointF(E.left(), y)), t.map(QPointF(E.right(), y)));
      }
    }
    p.setClipping(false);
    p.setPen(QPen(theme::Edge, 1));
    p.setBrush(Qt::NoBrush);
    p.drawRect(Edev);
    // 50 mm 숫자 (모노)
    p.setFont(mono);
    p.setPen(theme::Ink2);
    const QFontMetricsF fm(mono);
    const bool rimRef = doc.heightRef() == HeightRef::Rim;
    if (!horiz) {
      for (int y = 0; y <= int(E.bottom()); y += 50) {
        const QPointF a = t.map(QPointF(E.left(), y)),
                      b = t.map(QPointF(E.right(), y));
        const QString s = QString::number(rimRef ? y - std::lround(H) : y);
        p.drawText(QPointF(a.x() - fm.horizontalAdvance(s) - 6,
                           a.y() + fm.ascent() / 2 - 1),
                   s);
        p.drawText(QPointF(b.x() + 6, b.y() + fm.ascent() / 2 - 1),
                   y == 0 ? (rimRef ? s + QStringLiteral(" mm")
                                    : QStringLiteral("0 mm (바닥)"))
                          : s);
      }
      for (int x = 0; x <= int(E.right()); x += 50) {
        for (int sgn : {-1, 1}) {
          if (x == 0 && sgn < 0)
            continue;
          const QPointF a = t.map(QPointF(sgn * x, E.top()));
          const QString s =
              x == 0 ? QStringLiteral("0 (중심축)") : QString::number(x);
          p.drawText(QPointF(a.x() - fm.horizontalAdvance(s) / 2,
                             a.y() + fm.ascent() + 6),
                     s);
        }
      }
    } else {
      p.setPen(QPen(theme::Ink2, 1, Qt::DashDotLine));
      p.drawLine(t.map(QPointF(E.left(), 0)), t.map(QPointF(E.right(), 0)));
      p.drawLine(t.map(QPointF(0, E.top())), t.map(QPointF(0, E.bottom())));
    }
  }

  if (horiz) {
    // 수평 슬롯: 평면 단면 ------------------------------------------------
    const auto &slot = doc.slotList()[size_t(o.slot)];
    double rOut = 0.0;
    QPainterPath path;
    for (const auto &pl : doc.horizontalSlice(slot.z)) {
      if (pl.pts.size() < 2)
        continue;
      QPolygonF poly;
      for (const auto &q : pl.pts) {
        poly << t.map(QPointF(q.x, q.y));
        rOut = std::max(rOut, std::hypot(q.x, q.y));
      }
      path.addPolygon(poly);
      path.closeSubpath();
    }
    if (o.section) {
      path.setFillRule(Qt::OddEvenFill);
      p.setPen(QPen(theme::ProfileRed, lineSec));
      p.setBrush(theme::ProfileFill);
      p.drawPath(path);
    }
    if (o.labels) {
      labelBox(p, t.map(QPointF(E.left() + 3, E.bottom() - 3)),
               QStringLiteral("H %1 mm 수평 절단 · 외경 ⌀ %2 mm")
                   .arg(num(slot.z), num(2 * rOut)),
               mono, Qt::AlignLeft | Qt::AlignTop);
    }
    p.restore();
    return;
  }

  // 2) 유물 음영 (외면 쪽 절반: Right 왼쪽 x ≤ 0, Left 오른쪽 x ≥ 0) ---------
  const auto outline = doc.displayOutline();
  if (o.shade && shade && !shade->isNull()) {
    const QRectF dst = t.mapRect(shade->rectMm);
    const QRectF &sr = shade->rectMm;
    const QRectF half =
        t.mapRect(doc.sectionSide() == SectionSide::Right
                      ? QRectF(sr.left(), sr.top(), -sr.left(), sr.height())
                      : QRectF(0, sr.top(), sr.right(), sr.height()));
    p.save();
    if (outline.size() >= 4) {
      QPolygonF clipPoly;
      clipPoly.reserve(qsizetype(outline.size()) + 2);
      clipPoly << t.map(QPointF(0.0, outline.front().y));
      for (const auto &q : outline)
        clipPoly << t.map(QPointF(q.x, q.y));
      clipPoly << t.map(QPointF(0.0, outline.back().y));
      QPainterPath clipPath;
      clipPath.addPolygon(clipPoly);
      clipPath.closeSubpath();
      p.setClipPath(clipPath);
    } else {
      p.setClipRect(half);
    }
    p.setRenderHint(QPainter::SmoothPixmapTransform, true);
    p.drawImage(dst, shade->image);
    p.restore();
  }

  // 2.5) 기준선 (중심선 · 구연선 · 바닥선, 먹 — 외곽선과 함께 켜고 끈다) ------
  // 용지(paperPxPerMm > 0): 종이 기준 굵기·중심선 연장 3 mm, 화면: px 굵기·도면
  // 3 mm
  constexpr double kRefExtPaperMm = 3.0;
  const bool paper = o.paperPxPerMm > 0;
  PlateRefLines refs;
  if (o.outline) {
    refs = plateRefLines(doc, paper ? kRefExtPaperMm * o.paperPxPerMm / px1mm
                                    : kRefExtPaperMm);
    for (const auto &k : kRefStyles)
      if (const auto &l = refs.*k.line) {
        p.setPen(
            QPen(theme::OuterInk,
                 paper ? std::max(1.0, k.paperMm * o.paperPxPerMm) : k.screenPx,
                 Qt::SolidLine, Qt::FlatCap));
        p.drawLine(t.map(QPointF(l->a.x, l->a.y)),
                   t.map(QPointF(l->b.x, l->b.y)));
      }
    if (refs.neck) {
      const double nw = paper ? std::max(1.0, 0.13 * o.paperPxPerMm) : 1.0;
      // Feature 24: 경부선 — 외면 실선, 내면 긴 파선 (12, 4)
      p.setPen(QPen(theme::OuterInk, nw, Qt::SolidLine, Qt::FlatCap));
      p.drawLine(t.map(QPointF(refs.neck->a.x, refs.neck->a.y)),
                 t.map(QPointF(0, refs.neck->a.y)));
      QPen neckDash(theme::OuterInk, nw, Qt::CustomDashLine, Qt::FlatCap);
      neckDash.setDashPattern({12.0, 4.0});
      p.setPen(neckDash);
      p.drawLine(t.map(QPointF(0, refs.neck->b.y)),
                 t.map(QPointF(refs.neck->b.x, refs.neck->b.y)));
    }
    // Feature 25: 파단 표시 (0.13 mm 짧은 2중 사선)
    if (doc.isSherdMode()) {
      const double bw = paper ? std::max(1.0, 0.13 * o.paperPxPerMm) : 1.0;
      p.setPen(QPen(theme::OuterInk, bw));
      auto drawBreak = [&](double bx, double by) {
        QPointF p1 = t.map(QPointF(bx - 0.4, by - 1.2)),
                p2 = t.map(QPointF(bx + 0.4, by + 1.2));
        QPointF p3 = t.map(QPointF(bx + 0.4, by - 1.2)),
                p4 = t.map(QPointF(bx + 1.2, by + 1.2));
        p.drawLine(p1, p2);
        p.drawLine(p3, p4);
      };
      drawBreak(doc.dims().rim / 2.0, H);
      drawBreak(0.0, 0.0);
    }
  }

  // 3) 외곽선 -----------------------------------------------------------------
  if (o.outline) {
    if (outline.size() >= 2) {
      p.setPen(QPen(theme::OuterInk, lineOut, Qt::SolidLine, Qt::RoundCap,
                    Qt::RoundJoin));
      p.setBrush(Qt::NoBrush);
      p.drawPolyline(mapPoly(outline, t));
      for (const auto &q : closingEnds(outline, refs))
        p.drawLine(t.map(QPointF(0, q.y)), t.map(QPointF(q.x, q.y)));
    }
  }

  // 4) 단면선 (맨 위, 순수 빨강) ---------------------------------------------
  if (o.section) {
    if (doc.hasEstimatedInner()) {
      p.setPen(Qt::NoPen);
      p.setBrush(theme::ProfileFill);
      for (const auto &poly : doc.displaySection()) {
        if (poly.size() < 3)
          continue;
        p.drawPolygon(mapPoly(poly, t));
      }
      p.setBrush(Qt::NoBrush);
      p.setPen(QPen(theme::ProfileRed, lineSec, Qt::SolidLine, Qt::RoundCap,
                    Qt::RoundJoin));
      for (const auto &solid : doc.displayScannedSolid()) {
        if (solid.size() < 2)
          continue;
        p.drawPolyline(mapPoly(solid, t));
      }
      QPen dashPen(theme::ProfileRed, lineSec, Qt::CustomDashLine, Qt::RoundCap,
                   Qt::RoundJoin);
      dashPen.setDashPattern({3.2, 2.2});
      p.setPen(dashPen);
      for (const auto &est : doc.displayEstimatedInner()) {
        if (est.size() < 2)
          continue;
        p.drawPolyline(mapPoly(est, t));
      }
    } else {
      for (const auto &poly : doc.displaySection()) {
        if (poly.size() < 3)
          continue;
        p.setPen(QPen(theme::ProfileRed, lineSec, Qt::SolidLine, Qt::RoundCap,
                      Qt::RoundJoin));
        p.setBrush(theme::ProfileFill);
        p.drawPolygon(mapPoly(poly, t));
      }
    }
    if (o.hatch) {
      p.setPen(QPen(theme::ProfileRed, std::max(0.8, 0.13 * px1mm)));
      for (const auto &[a, b] : doc.displayHatch())
        p.drawLine(t.map(QPointF(a.x, a.y)), t.map(QPointF(b.x, b.y)));
    }
  }

  // 5) 머리 치수 · 축척 막대 -------------------------------------------------
  if (o.labels && doc.dims().ok) {
    const auto &d = doc.dims();
    const double yDim = H + 8.0;
    p.setPen(QPen(theme::Ink, 1));
    const QPointF a = t.map(QPointF(-d.rim / 2, yDim)),
                  b = t.map(QPointF(d.rim / 2, yDim));
    p.drawLine(a, b);
    p.drawLine(a + QPointF(0, -4), a + QPointF(0, 4));
    p.drawLine(b + QPointF(0, -4), b + QPointF(0, 4));
    labelBox(p, (a + b) / 2, QStringLiteral("구경 ⌀ %1 mm").arg(num(d.rim)),
             mono, Qt::AlignHCenter | Qt::AlignVCenter);
    // 구연선이 있으면 기고 파선은 구연선 바깥 끝 2 mm 앞까지, 라벨은 구연선
    // 바로 위 (구연선 외면 쪽 반과 외곽선 끝의 틈을 덮지 않게)
    const bool rimLine = o.outline && refs.rim;
    p.setPen(QPen(theme::Caution, 1, Qt::DashLine));
    const double hx = rimLine ? -d.rim / 2 - 2 : 0.0;
    const QPointF h0 = t.map(QPointF(E.left() + 2, H));
    if (hx > E.left() + 2)
      p.drawLine(h0, t.map(QPointF(hx, H)));
    labelBox(p, h0 + QPointF(4, rimLine ? -2 : 0),
             QStringLiteral("기고 H %1 mm").arg(num(d.height)), mono,
             Qt::AlignLeft | (rimLine ? Qt::AlignBottom : Qt::AlignVCenter));
  }
  if (o.grid) {
    const double len = px1mm * 50 > Edev.width() * 0.3 ? 10.0 : 50.0;
    const QPointF lb = t.map(QPointF(E.right() - len - 6, E.top() + 4));
    scaleBar(p, lb, px1mm, len, mono, QString());
  }

  // 6) 선택 부위 정밀 문양 탁본 콜아웃 박스 & 3D 절단 위치 확인 표지 -----------
  if (o.shade && doc.hasRubbingSpot() && !horiz && Edev.width() >= 180.0) {
    const QImage rubImg = renderRubbingPatch(doc, 4.2);
    if (!rubImg.isNull()) {
      const double boxW = std::clamp(Edev.width() * 0.24, 76.0, 148.0);
      const double boxH = boxW * double(rubImg.height()) /
                          std::max(1.0, double(rubImg.width()));
      const double capH = std::max(16.0, textPx * 1.45);
      const bool onRight = (doc.sectionSide() == SectionSide::Left);
      const double bx =
          onRight ? (Edev.right() - boxW - 8.0) : (Edev.left() + 8.0);
      const double by = std::clamp(
          t.map(QPointF(0.0, doc.rubbingZ())).y() - (boxH + capH) * 0.5,
          Edev.top() + 34.0, Edev.bottom() - boxH - capH - 28.0);
      const QRectF cardRect(bx, by, boxW, boxH + capH);
      const QRectF imgRect(bx + 3.0, by + capH, boxW - 6.0, boxH - 3.0);

      // 외면 선택 높이(rubbingZ)로 이어지는 인출선
      const double rAtZ = std::max(8.0, doc.outerRadiusAt(doc.rubbingZ()) * 0.65);
      const double srcX =
          (doc.sectionSide() == SectionSide::Right ? -1.0 : 1.0) * rAtZ;
      const QPointF srcPt = t.map(QPointF(srcX, doc.rubbingZ()));
      const QPointF dstPt(onRight ? cardRect.left() : cardRect.right(),
                          cardRect.center().y());

      p.save();
      p.setPen(QPen(theme::OuterInk, 1.0, Qt::DashLine));
      p.drawLine(srcPt, dstPt);
      p.setBrush(theme::OuterInk);
      p.drawEllipse(srcPt, 2.5, 2.5);

      p.setPen(QPen(theme::OuterInk, 1.2));
      p.setBrush(theme::Surface);
      p.drawRect(cardRect);
      p.drawImage(imgRect, rubImg);
      p.setPen(QPen(theme::Edge, 0.8));
      p.setBrush(Qt::NoBrush);
      p.drawRect(imgRect);

      p.setFont(mono);
      p.setPen(theme::Ink);
      const QString rubTitle =
          QStringLiteral("◈ 문양 탁본 (H %1)").arg(num(doc.rubbingZ(), 0));
      p.drawText(QRectF(bx + 3.0, by + 1.0, boxW - 6.0, capH - 2.0),
                 Qt::AlignCenter, rubTitle);
      p.restore();
    }
  }

  if (o.section && o.labels && doc.hasCutMarker() && !horiz) {
    const double cz = doc.cutMarkerZ();
    double cr = doc.outerRadiusAt(cz);
    if (cr <= 0.0)
      cr = 0.5 * doc.dims().belly;
    const double sx = doc.sectionSign() * cr;
    const QPointF cutPt = t.map(QPointF(sx, cz));
    p.save();
    p.setPen(QPen(Qt::white, 2.0));
    p.setBrush(theme::ProfileRed);
    p.drawEllipse(cutPt, 4.5, 4.5);
    labelBox(p, cutPt + QPointF(doc.sectionSign() * 8.0, 0.0),
             QStringLiteral("✂ 절단 H %1 mm").arg(num(cz, 1)), mono,
             (doc.sectionSign() > 0 ? Qt::AlignLeft : Qt::AlignRight) |
                 Qt::AlignVCenter);
    p.restore();
  }
  p.restore();
}

// ---- 용지 ----------------------------------------------------------------
QSizeF paperSize(Paper p) {
  switch (p) {
  case Paper::A4Landscape:
    return {297, 210};
  case Paper::A3Portrait:
    return {297, 420};
  default:
    return {210, 297};
  }
}
QString paperName(Paper p) {
  switch (p) {
  case Paper::A4Landscape:
    return QStringLiteral("A4 가로");
  case Paper::A3Portrait:
    return QStringLiteral("A3 세로");
  default:
    return QStringLiteral("A4 세로");
  }
}
QString scaleName(double s) {
  if (s >= 1.5)
    return QStringLiteral("2:1");
  if (s >= 0.99)
    return QStringLiteral("1:1");
  return QStringLiteral("1:%1").arg(std::lround(1.0 / s));
}

PaperLayout layoutPaper(const ArtifactDoc &doc, int slot, Paper paper,
                        double scale) {
  PaperLayout L;
  L.paperMm = paperSize(paper);
  L.scale = scale;
  const double m = 10.0, titleH = 24.0, gap = 4.0;
  L.drawAreaMm = QRectF(m, m, L.paperMm.width() - 2 * m,
                        L.paperMm.height() - 2 * m - titleH - gap);
  L.titleRectMm = QRectF(m, L.paperMm.height() - m - titleH,
                         L.paperMm.width() - 2 * m, titleH);
  const QRectF E = Plate::extentMm(doc, slot);
  const QSizeF ps(E.width() * scale, E.height() * scale);
  L.plateRectMm = QRectF(QPointF(0, 0), ps);
  L.plateRectMm.moveCenter(L.drawAreaMm.center());
  L.fits = ps.width() <= L.drawAreaMm.width() + 1e-6 &&
           ps.height() <= L.drawAreaMm.height() + 1e-6;
  const auto &d = doc.dims();
  const QString size =
      QStringLiteral("%1 × %2 mm").arg(num(d.belly), num(d.height));
  if (L.fits) {
    L.advice =
        QStringLiteral(
            "✓ %1 축척에서 유물 크기(%2)가 %3 한 장에 여유 있게 들어갑니다.")
            .arg(scaleName(scale), size, paperName(paper));
  } else {
    QString rec;
    for (double s : {1.0, 0.5, 1.0 / 3.0, 0.25}) {
      if (s >= scale)
        continue;
      if (E.width() * s <= L.drawAreaMm.width() &&
          E.height() * s <= L.drawAreaMm.height()) {
        rec = scaleName(s);
        break;
      }
    }
    L.advice = QStringLiteral("▲ %1에서 유물 높이 %2 mm는 %3 한 장을 넘습니다. "
                              "추천: %4로 줄이거나 A3 세로 선택")
                   .arg(scaleName(scale), num(d.height), paperName(paper),
                        rec.isEmpty() ? QStringLiteral("1:4") : rec);
  }
  return L;
}

void paintSheet(QPainter &p, const ArtifactDoc &doc, const ExportRequest &r,
                double ppm) {
  const PaperLayout L = layoutPaper(doc, r.opts.slot, r.paper, r.scale);
  p.save();
  p.setRenderHint(QPainter::Antialiasing, true);
  p.fillRect(QRectF(0, 0, L.paperMm.width() * ppm, L.paperMm.height() * ppm),
             Qt::white);
  const QRectF E = Plate::extentMm(doc, r.opts.slot);
  QTransform t;
  // x_dev = (plate.left + (x - E.left) * s) * ppm ; y_dev = (plate.top +
  // (E.bottom - y) * s) * ppm
  t.setMatrix(r.scale * ppm, 0, 0, 0, -r.scale * ppm, 0,
              (L.plateRectMm.left() - E.left() * r.scale) * ppm,
              (L.plateRectMm.top() + E.bottom() * r.scale) * ppm, 1);
  ShadeImage shade;
  if (r.opts.shade)
    shade = Plate::renderShade(doc, r.opts.shadeMode,
                               std::min(12.0, r.scale * ppm));
  PlateOptions o = r.opts;
  o.paperPxPerMm = ppm;
  Plate::paint(p, doc, t, r.scale * ppm, 2.6 * ppm, o, &shade);

  // 표제란
  const QRectF tr(L.titleRectMm.left() * ppm, L.titleRectMm.top() * ppm,
                  L.titleRectMm.width() * ppm, L.titleRectMm.height() * ppm);
  p.setPen(QPen(theme::Ink, std::max(1.0, 0.3 * ppm)));
  p.drawLine(tr.topLeft(), tr.topRight());
  const QFont small = theme::uiFont(int(2.6 * ppm)),
              mono = theme::monoFont(int(2.4 * ppm));
  p.setFont(small);
  p.setPen(theme::Ink);
  const QString az = azimuthNote(doc, r.opts.slot);
  p.drawText(
      QPointF(tr.left(), tr.top() + 7 * ppm),
      QStringLiteral("축척 %1%2 · 단위 mm%3")
          .arg(scaleName(r.scale),
               r.scale == 1.0 ? QStringLiteral(" (실제 크기)") : QString(),
               az.isEmpty() ? QString() : QStringLiteral(" · ") + az));
  scaleBar(p, QPointF(tr.left(), tr.top() + 15 * ppm), r.scale * ppm, 50.0,
           mono, QStringLiteral("50 mm"));
  const QRectF box(tr.right() - 92 * ppm, tr.top() + 3 * ppm, 92 * ppm,
                   18 * ppm);
  p.setBrush(Qt::NoBrush);
  p.setPen(QPen(theme::Ink, std::max(1.0, 0.25 * ppm)));
  p.drawRect(box);
  p.setFont(theme::uiFont(int(3.0 * ppm), true));
  p.drawText(box.adjusted(3 * ppm, 2 * ppm, -3 * ppm, 0),
             Qt::AlignLeft | Qt::AlignTop, r.title);
  const auto &d = doc.dims();
  p.setFont(mono);
  const QString baseStr =
      (d.base > 0.0 && !doc.isSherdMode()) ? num(d.base) : QStringLiteral("—");
  p.drawText(box.adjusted(3 * ppm, 0, -3 * ppm, -2.5 * ppm),
             Qt::AlignLeft | Qt::AlignBottom,
             QStringLiteral("기고 %1 · 구경 %2 · 저경 %3 mm")
                 .arg(num(d.height), num(d.rim), baseStr));
  if (r.opts.calibration100mm) {
    p.save();
    p.setPen(QPen(theme::Ink, std::max(1.0, 0.13 * ppm)));
    p.setBrush(Qt::NoBrush);
    const QRectF calRect(20.0 * ppm, 182.0 * ppm, 100.0 * ppm, 100.0 * ppm);
    p.drawRect(calRect);
    p.setFont(theme::uiFont(int(2.8 * ppm), false));
    p.drawText(QPointF(22.0 * ppm, 187.0 * ppm),
               QStringLiteral("CALIBRATION 100 × 100 mm"));
    p.restore();
  }
  p.restore();
}

bool exportPdf(const ArtifactDoc &doc, const ExportRequest &r, QString *err) {
  if (refusePreview(doc, err))
    return false;
  QPdfWriter w(r.path);
  const QSizeF ps = paperSize(r.paper);
  w.setPageSize(QPageSize(ps, QPageSize::Millimeter));
  w.setPageMargins(QMarginsF(0, 0, 0, 0));
  w.setResolution(600);
  w.setTitle(r.title);
  QPainter p;
  if (!p.begin(&w)) {
    if (err)
      *err = QStringLiteral("PDF 파일을 만들 수 없습니다.");
    return false;
  }
  paintSheet(p, doc, r, 600.0 / 25.4);
  p.end();
  return true;
}

bool exportPng(const ArtifactDoc &doc, const ExportRequest &r, int dpi,
               QString *err) {
  if (refusePreview(doc, err))
    return false;
  const double ppm = dpi / 25.4;
  const QSizeF ps = paperSize(r.paper);
  QImage img(int(ps.width() * ppm), int(ps.height() * ppm),
             QImage::Format_RGB32);
  img.setDotsPerMeterX(int(ppm * 1000));
  img.setDotsPerMeterY(int(ppm * 1000));
  QPainter p(&img);
  paintSheet(p, doc, r, ppm);
  p.end();
  if (!img.save(r.path, "PNG")) {
    if (err)
      *err = QStringLiteral("PNG 저장 실패");
    return false;
  }
  return true;
}

// ---- 레이어 분리 SVG (일러스트레이터 1:1 mm) --------------------------------
bool exportSvgLayered(const ArtifactDoc &doc, const ExportRequest &r,
                      QString *err) {
  if (r.opts.spec14Layers) {
    return exportSvg14Layers(doc, r, err);
  }
  if (refusePreview(doc, err))
    return false;
  QFile f(r.path);
  if (!f.open(QIODevice::WriteOnly | QIODevice::Truncate | QIODevice::Text)) {
    if (err)
      *err = QStringLiteral("SVG 파일을 만들 수 없습니다.");
    return false;
  }
  const PaperLayout L = layoutPaper(doc, r.opts.slot, r.paper, r.scale);
  const QRectF E = Plate::extentMm(doc, r.opts.slot);
  const double s = r.scale;
  auto X = [&](double x) { return L.plateRectMm.left() + (x - E.left()) * s; };
  auto Y = [&](double y) { return L.plateRectMm.top() + (E.bottom() - y) * s; };
  auto n3 = [](double v) { return QString::number(v, 'f', 3); };
  auto pts = [&](const std::vector<KA::Point2> &v) {
    QString o;
    for (const auto &q : v)
      o += n3(X(q.x)) + ',' + n3(Y(q.y)) + ' ';
    return o.trimmed();
  };
  QTextStream out(&f);
  out.setEncoding(QStringConverter::Utf8);
  const QSizeF ps = L.paperMm;
  out << "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"
      << "<svg xmlns=\"http://www.w3.org/2000/svg\" "
         "xmlns:xlink=\"http://www.w3.org/1999/xlink\" "
         "xmlns:inkscape=\"http://www.inkscape.org/namespaces/inkscape\" "
      << "width=\"" << ps.width() << "mm\" height=\"" << ps.height()
      << "mm\" viewBox=\"0 0 " << ps.width() << ' ' << ps.height() << "\">\n"
      << "<title>" << r.title.toHtmlEscaped() << "</title>\n";
  auto layer = [&](const char *id, const QString &label) {
    out << "<g id=\"" << id
        << "\" inkscape:groupmode=\"layer\" inkscape:label=\"" << label
        << "\">\n";
  };
  const bool horiz = isHorizontal(doc, r.opts.slot);
  const double H = height(doc);

  // (1) 방안 눈금·치수·축척
  if (r.opts.grid) {
    layer("grid_dims_scale", QStringLiteral("방안 눈금·치수·축척"));
    out << "<g fill=\"none\">\n";
    for (int pass = 0; pass < 2; ++pass) {
      const char *col = pass == 0 ? "#EDEBE2" : "#D5D2C6";
      const double w = pass == 0 ? 0.08 : 0.15;
      for (int x = int(std::ceil(E.left())); x <= int(std::floor(E.right()));
           ++x)
        if ((pass == 0) == (x % 10 != 0))
          out << "<line x1=\"" << n3(X(x)) << "\" y1=\"" << n3(Y(E.top()))
              << "\" x2=\"" << n3(X(x)) << "\" y2=\"" << n3(Y(E.bottom()))
              << "\" stroke=\"" << col << "\" stroke-width=\"" << w << "\"/>\n";
      for (int y = int(std::ceil(E.top())); y <= int(std::floor(E.bottom()));
           ++y)
        if ((pass == 0) == (y % 10 != 0))
          out << "<line x1=\"" << n3(X(E.left())) << "\" y1=\"" << n3(Y(y))
              << "\" x2=\"" << n3(X(E.right())) << "\" y2=\"" << n3(Y(y))
              << "\" stroke=\"" << col << "\" stroke-width=\"" << w << "\"/>\n";
    }
    out << "<rect x=\"" << n3(X(E.left())) << "\" y=\"" << n3(Y(E.bottom()))
        << "\" width=\"" << n3(E.width() * s) << "\" height=\""
        << n3(E.height() * s)
        << "\" stroke=\"#C2C0B6\" stroke-width=\"0.2\"/>\n";
    out << "</g>\n";
    const auto &d = doc.dims();
    const QString az = azimuthNote(doc, r.opts.slot);
    const double ty = L.titleRectMm.top();
    out << "<g font-family=\"Malgun Gothic, Noto Sans KR\" font-size=\"2.6\" "
           "fill=\"#141413\">\n"
        << "<line x1=\"" << L.titleRectMm.left() << "\" y1=\"" << ty
        << "\" x2=\"" << L.titleRectMm.right() << "\" y2=\"" << ty
        << "\" stroke=\"#141413\" stroke-width=\"0.3\"/>\n"
        << "<text x=\"" << L.titleRectMm.left() << "\" y=\"" << ty + 7
        << "\">축척 " << scaleName(s) << " · 단위 mm"
        << (az.isEmpty() ? QString() : QStringLiteral(" · ") + az)
        << "</text>\n";
    const double sb = 50.0 * s, sx = L.titleRectMm.left(), sy = ty + 12;
    out << "<rect x=\"" << sx << "\" y=\"" << sy << "\" width=\"" << n3(sb / 2)
        << "\" height=\"1.5\" fill=\"#141413\"/>\n"
        << "<rect x=\"" << n3(sx + sb / 2) << "\" y=\"" << sy << "\" width=\""
        << n3(sb / 2)
        << "\" height=\"1.5\" fill=\"#FFFFFF\" stroke=\"#141413\" "
           "stroke-width=\"0.2\"/>\n"
        << "<text x=\"" << sx << "\" y=\"" << sy + 5
        << "\" font-family=\"Consolas\">0</text>\n"
        << "<text x=\"" << n3(sx + sb - 4) << "\" y=\"" << sy + 5
        << "\" font-family=\"Consolas\">50 mm</text>\n";
    const double bx = L.titleRectMm.right() - 92;
    out << "<rect x=\"" << bx << "\" y=\"" << ty + 3
        << "\" width=\"92\" height=\"18\" fill=\"none\" stroke=\"#141413\" "
           "stroke-width=\"0.25\"/>\n"
        << "<text x=\"" << bx + 3 << "\" y=\"" << ty + 9
        << "\" font-size=\"3\" font-weight=\"700\">" << r.title.toHtmlEscaped()
        << "</text>\n"
        << "<text x=\"" << bx + 3 << "\" y=\"" << ty + 17
        << "\" font-family=\"Consolas\" font-size=\"2.4\">기고 "
        << num(d.height) << " · 구경 " << num(d.rim) << " · 저경 "
        << ((d.base > 0.0 && !doc.isSherdMode()) ? num(d.base)
                                                 : QStringLiteral("—"))
        << " mm</text>\n";
    if (!horiz && d.ok) {
      out << "<text x=\"" << n3(X(0)) << "\" y=\"" << n3(Y(H + 8) - 1)
          << "\" text-anchor=\"middle\" font-family=\"Consolas\">구경 ⌀ "
          << num(d.rim) << " mm</text>\n"
          << "<line x1=\"" << n3(X(-d.rim / 2)) << "\" y1=\"" << n3(Y(H + 8))
          << "\" x2=\"" << n3(X(d.rim / 2)) << "\" y2=\"" << n3(Y(H + 8))
          << "\" stroke=\"#141413\" stroke-width=\"0.18\"/>\n";
    }
    out << "</g>\n</g>\n";
  }
  // (2) 음영·탁본
  if (r.opts.shade && !horiz) {
    const ShadeImage sh = Plate::renderShade(
        doc, r.opts.shadeMode, 300.0 / 25.4 / std::max(0.25, 1.0 / s));
    if (!sh.isNull()) {
      // 외면 쪽 절반만 (Right: 영상 왼쪽 절반을 축 왼쪽에, Left: 오른쪽 절반을
      // 축 오른쪽에)
      const bool right = doc.sectionSide() == SectionSide::Right;
      const int half = sh.image.width() / 2;
      const QImage part = right
                              ? sh.image.copy(0, 0, half, sh.image.height())
                              : sh.image.copy(half, 0, sh.image.width() - half,
                                              sh.image.height());
      QByteArray png;
      QBuffer buf(&png);
      buf.open(QIODevice::WriteOnly);
      part.save(&buf, "PNG");
      layer("shade_dtak", QStringLiteral("음영·탁본"));
      out << "<image x=\"" << n3(X(right ? sh.rectMm.left() : 0.0)) << "\" y=\""
          << n3(Y(sh.rectMm.bottom())) << "\" width=\""
          << n3(sh.rectMm.width() / 2 * s) << "\" height=\""
          << n3(sh.rectMm.height() * s)
          << "\" preserveAspectRatio=\"none\" "
             "xlink:href=\"data:image/png;base64,"
          << png.toBase64() << "\"/>\n</g>\n";
    }
  }
  // (2.5) 기준선 — 선마다 독립 층 (중심선 → 구연선 → 바닥선), 중심선 연장 종이
  // 3 mm
  PlateRefLines refs;
  if (r.opts.outline && !horiz) {
    refs = plateRefLines(doc, 3.0 / s);
    for (const auto &k : kRefStyles)
      if (const auto &l = refs.*k.line) {
        layer(k.id, QString::fromUtf8(k.name));
        out << "<line x1=\"" << n3(X(l->a.x)) << "\" y1=\"" << n3(Y(l->a.y))
            << "\" x2=\"" << n3(X(l->b.x)) << "\" y2=\"" << n3(Y(l->b.y))
            << "\" stroke=\"" << theme::hex(theme::OuterInk)
            << "\" stroke-width=\"" << n3(k.paperMm) << "\"/>\n</g>\n";
      }
  }
  // (3) 외곽선
  if (r.opts.outline && !horiz) {
    layer("outline_axis", QStringLiteral("외곽선"));
    const auto ol = doc.displayOutline();
    out << "<g fill=\"none\" stroke=\"#141413\" stroke-linejoin=\"round\" "
           "stroke-linecap=\"round\">\n";
    if (ol.size() >= 2) {
      out << "<polyline stroke-width=\"" << kSvgOutline << "\" points=\""
          << pts(ol) << "\"/>\n";
      for (const auto &q : closingEnds(ol, refs))
        out << "<line x1=\"" << n3(X(0)) << "\" y1=\"" << n3(Y(q.y))
            << "\" x2=\"" << n3(X(q.x)) << "\" y2=\"" << n3(Y(q.y))
            << "\" stroke-width=\"" << kSvgOutline << "\"/>\n";
    }
    out << "</g>\n</g>\n";
  }
  // (4) 단면선 (맨 위)
  if (r.opts.section) {
    layer("section_profile", QStringLiteral("단면선"));
    out << "<g stroke=\"#FF0000\" stroke-width=\"" << kSvgSection
        << "\" stroke-linejoin=\"round\">\n";
    if (!horiz) {
      if (doc.hasEstimatedInner()) {
        for (const auto &poly : doc.displaySection())
          out << "<polygon fill=\"#FF0000\" fill-opacity=\"0.12\" "
                 "stroke=\"none\" points=\""
              << pts(poly) << "\"/>\n";
        for (const auto &solid : doc.displayScannedSolid())
          out << "<polyline fill=\"none\" stroke=\"#FF0000\" stroke-width=\""
              << kSvgSection << "\" points=\"" << pts(solid) << "\"/>\n";
        for (const auto &est : doc.displayEstimatedInner())
          out << "<polyline fill=\"none\" stroke=\"#FF0000\" stroke-width=\""
              << kSvgSection << "\" stroke-dasharray=\"2.0,1.3\" points=\""
              << pts(est) << "\"/>\n";
      } else {
        for (const auto &poly : doc.displaySection())
          out << "<polygon fill=\"#FF0000\" fill-opacity=\"0.12\" points=\""
              << pts(poly) << "\"/>\n";
      }
      if (r.opts.hatch)
        for (const auto &[a, b] : doc.displayHatch())
          out << "<line x1=\"" << n3(X(a.x)) << "\" y1=\"" << n3(Y(a.y))
              << "\" x2=\"" << n3(X(b.x)) << "\" y2=\"" << n3(Y(b.y))
              << "\" stroke-width=\"0.13\"/>\n";
    } else {
      const auto &sl = doc.slotList()[size_t(r.opts.slot)];
      for (const auto &pl : doc.horizontalSlice(sl.z)) {
        std::vector<KA::Point2> v;
        for (const auto &q : pl.pts)
          v.push_back({q.x, q.y});
        out << "<polygon fill=\"none\" points=\"" << pts(v) << "\"/>\n";
      }
    }
    out << "</g>\n</g>\n";
  }
  out << "</svg>\n";
  return true;
}

bool exportSvg14Layers(const ArtifactDoc &doc, const ExportRequest &r,
                       QString *err) {
  if (refusePreview(doc, err))
    return false;
  QFile f(r.path);
  if (!f.open(QIODevice::WriteOnly | QIODevice::Truncate | QIODevice::Text)) {
    if (err)
      *err = QStringLiteral("SVG 파일을 만들 수 없습니다.");
    return false;
  }
  const PaperLayout L = layoutPaper(doc, r.opts.slot, r.paper, r.scale);
  const QRectF E = Plate::extentMm(doc, r.opts.slot);
  const double s = r.scale;
  auto X = [&](double x) { return L.plateRectMm.left() + (x - E.left()) * s; };
  auto Y = [&](double y) { return L.plateRectMm.top() + (E.bottom() - y) * s; };
  auto n3 = [](double v) { return QString::number(v, 'f', 3); };
  auto pts = [&](const std::vector<KA::Point2> &v) {
    QString o;
    for (const auto &q : v)
      o += n3(X(q.x)) + ',' + n3(Y(q.y)) + ' ';
    return o.trimmed();
  };
  QTextStream out(&f);
  out.setEncoding(QStringConverter::Utf8);
  const QSizeF ps = L.paperMm;
  out << "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"
      << "<svg xmlns=\"http://www.w3.org/2000/svg\" "
         "xmlns:xlink=\"http://www.w3.org/1999/xlink\" "
         "xmlns:inkscape=\"http://www.inkscape.org/namespaces/inkscape\" "
      << "width=\"" << ps.width() << "mm\" height=\"" << ps.height()
      << "mm\" viewBox=\"0 0 " << ps.width() << ' ' << ps.height() << "\">\n"
      << "<title>" << r.title.toHtmlEscaped() << "</title>\n";
  auto layer = [&](const char *id, const QString &label) {
    out << "<g id=\"" << id
        << "\" inkscape:groupmode=\"layer\" inkscape:label=\"" << label
        << "\">\n";
  };
  const bool horiz = isHorizontal(doc, r.opts.slot);
  const double H = height(doc);
  const auto refs = plateRefLines(doc, 3.0 / s);
  const auto &d = doc.dims();

  // (1) grid: 방안 눈금
  layer("grid", QStringLiteral("방안"));
  if (r.opts.grid) {
    out << "<g fill=\"none\">\n";
    for (int pass = 0; pass < 2; ++pass) {
      const char *col = pass == 0 ? "#EDEBE2" : "#D5D2C6";
      const double w = pass == 0 ? 0.08 : 0.15;
      for (int x = int(std::ceil(E.left())); x <= int(std::floor(E.right()));
           ++x)
        if ((pass == 0) == (x % 10 != 0))
          out << "<line x1=\"" << n3(X(x)) << "\" y1=\"" << n3(Y(E.top()))
              << "\" x2=\"" << n3(X(x)) << "\" y2=\"" << n3(Y(E.bottom()))
              << "\" stroke=\"" << col << "\" stroke-width=\"" << w << "\"/>\n";
      for (int y = int(std::ceil(E.top())); y <= int(std::floor(E.bottom()));
           ++y)
        if ((pass == 0) == (y % 10 != 0))
          out << "<line x1=\"" << n3(X(E.left())) << "\" y1=\"" << n3(Y(y))
              << "\" x2=\"" << n3(X(E.right())) << "\" y2=\"" << n3(Y(y))
              << "\" stroke=\"" << col << "\" stroke-width=\"" << w << "\"/>\n";
    }
    out << "<rect x=\"" << n3(X(E.left())) << "\" y=\"" << n3(Y(E.bottom()))
        << "\" width=\"" << n3(E.width() * s) << "\" height=\""
        << n3(E.height() * s)
        << "\" stroke=\"#C2C0B6\" stroke-width=\"0.2\"/>\n";
    out << "</g>\n";
  }
  out << "</g>\n";

  // (2) shade: 외면 음영·탁본
  layer("shade", QStringLiteral("음영"));
  if (r.opts.shade && !horiz) {
    const ShadeImage sh = Plate::renderShade(
        doc, r.opts.shadeMode, 300.0 / 25.4 / std::max(0.25, 1.0 / s));
    if (!sh.isNull()) {
      const bool right = doc.sectionSide() == SectionSide::Right;
      const int half = sh.image.width() / 2;
      const QImage part = right
                              ? sh.image.copy(0, 0, half, sh.image.height())
                              : sh.image.copy(half, 0, sh.image.width() - half,
                                              sh.image.height());
      QByteArray png;
      QBuffer buf(&png);
      buf.open(QIODevice::WriteOnly);
      part.save(&buf, "PNG");
      out << "<image x=\"" << n3(X(right ? sh.rectMm.left() : 0.0)) << "\" y=\""
          << n3(Y(sh.rectMm.bottom())) << "\" width=\""
          << n3(sh.rectMm.width() / 2 * s) << "\" height=\""
          << n3(sh.rectMm.height() * s)
          << "\" preserveAspectRatio=\"none\" "
             "xlink:href=\"data:image/png;base64,"
          << png.toBase64() << "\"/>\n";
    }
  }
  out << "</g>\n";

  // (3) calibration: 보정 100 mm 사각형
  layer("calibration", QStringLiteral("보정 100 mm"));
  if (r.opts.calibration100mm) {
    out << "<g fill=\"none\" stroke=\"#1F1F1F\" stroke-width=\"0.13\">\n"
        << "<rect x=\"20\" y=\"182\" width=\"100\" height=\"100\"/>\n"
        << "<text x=\"22\" y=\"187\" font-family=\"Consolas, monospace\" "
           "font-size=\"3\" fill=\"#1F1F1F\" stroke=\"none\">CALIBRATION 100 × "
           "100 mm</text>\n"
        << "</g>\n";
  }
  out << "</g>\n";

  // (4) reconstruction: 복원선
  layer("reconstruction", QStringLiteral("복원선"));
  if (doc.isSherdMode()) {
    double estDia = doc.estimatedRimDiameter();
    if (estDia > 0.0) {
      out << "<g stroke=\"#1F1F1F\" stroke-width=\"0.13\" "
             "stroke-dasharray=\"7,4\" fill=\"none\">\n"
          << "<line x1=\"" << n3(X(-estDia / 2)) << "\" y1=\"" << n3(Y(H))
          << "\" x2=\"" << n3(X(estDia / 2)) << "\" y2=\"" << n3(Y(H))
          << "\"/>\n"
          << "</g>\n";
    }
  }
  out << "</g>\n";

  // (5) inner: 내벽선
  layer("inner", QStringLiteral("내면선"));
  if (doc.hasEstimatedInner()) {
    out << "<g stroke=\"#FF0000\" stroke-width=\"0.30\" "
           "stroke-dasharray=\"2.0,1.3\" fill=\"none\">\n";
    for (const auto &est : doc.displayEstimatedInner()) {
      out << "<polyline points=\"" << pts(est) << "\"/>\n";
    }
    out << "</g>\n";
  }
  out << "</g>\n";

  // (6) refline.center: 중심선
  layer("refline.center", QStringLiteral("중심선"));
  if (refs.center && !horiz) {
    out << "<line x1=\"" << n3(X(refs.center->a.x)) << "\" y1=\""
        << n3(Y(refs.center->a.y)) << "\" x2=\"" << n3(X(refs.center->b.x))
        << "\" y2=\"" << n3(Y(refs.center->b.y))
        << "\" stroke=\"#141413\" stroke-width=\"0.10\"/>\n";
  }
  out << "</g>\n";

  // (7) refline.rim: 구연선 · 경부선
  layer("refline.rim", QStringLiteral("구연선"));
  if (refs.rim && !horiz) {
    out << "<line x1=\"" << n3(X(refs.rim->a.x)) << "\" y1=\""
        << n3(Y(refs.rim->a.y)) << "\" x2=\"" << n3(X(refs.rim->b.x))
        << "\" y2=\"" << n3(Y(refs.rim->b.y))
        << "\" stroke=\"#141413\" stroke-width=\"0.13\"/>\n";
  }
  if (refs.neck && !horiz) {
    out << "<line x1=\"" << n3(X(refs.neck->a.x)) << "\" y1=\""
        << n3(Y(refs.neck->a.y)) << "\" x2=\"" << n3(X(0.0)) << "\" y2=\""
        << n3(Y(refs.neck->a.y))
        << "\" stroke=\"#141413\" stroke-width=\"0.13\"/>\n"
        << "<line x1=\"" << n3(X(0.0)) << "\" y1=\"" << n3(Y(refs.neck->b.y))
        << "\" x2=\"" << n3(X(refs.neck->b.x)) << "\" y2=\""
        << n3(Y(refs.neck->b.y))
        << "\" stroke=\"#141413\" stroke-width=\"0.13\" "
           "stroke-dasharray=\"12,4\"/>\n";
  }
  out << "</g>\n";

  // (8) refline.base: 바닥선
  layer("refline.base", QStringLiteral("바닥선"));
  if (refs.base && !horiz && !doc.isSherdMode() && d.base > 0) {
    out << "<line x1=\"" << n3(X(refs.base->a.x)) << "\" y1=\""
        << n3(Y(refs.base->a.y)) << "\" x2=\"" << n3(X(refs.base->b.x))
        << "\" y2=\"" << n3(Y(refs.base->b.y))
        << "\" stroke=\"#141413\" stroke-width=\"0.25\"/>\n";
  }
  out << "</g>\n";

  // (9) outline: 외곽선
  layer("outline", QStringLiteral("외곽선"));
  if (r.opts.outline && !horiz) {
    const auto ol = doc.displayOutline();
    out << "<g fill=\"none\" stroke=\"#141413\" stroke-linejoin=\"round\" "
           "stroke-linecap=\"round\">\n";
    if (ol.size() >= 2) {
      out << "<polyline stroke-width=\"0.25\" points=\"" << pts(ol) << "\"/>\n";
      for (const auto &q : closingEnds(ol, refs))
        out << "<line x1=\"" << n3(X(0)) << "\" y1=\"" << n3(Y(q.y))
            << "\" x2=\"" << n3(X(q.x)) << "\" y2=\"" << n3(Y(q.y))
            << "\" stroke-width=\"0.25\"/>\n";
    }
    out << "</g>\n";
  }
  out << "</g>\n";

  // (10) section: 단면선
  layer("section", QStringLiteral("단면선"));
  if (r.opts.section) {
    out << "<g stroke=\"#FF0000\" stroke-width=\"0.30\" "
           "stroke-linejoin=\"round\">\n";
    if (!horiz) {
      if (doc.hasEstimatedInner()) {
        for (const auto &poly : doc.displaySection())
          out << "<polygon fill=\"#FF0000\" fill-opacity=\"0.12\" "
                 "stroke=\"none\" points=\""
              << pts(poly) << "\"/>\n";
        for (const auto &solid : doc.displayScannedSolid())
          out << "<polyline fill=\"none\" stroke=\"#FF0000\" "
                 "stroke-width=\"0.30\" points=\""
              << pts(solid) << "\"/>\n";
      } else {
        for (const auto &poly : doc.displaySection())
          out << "<polygon fill=\"#FF0000\" fill-opacity=\"0.12\" points=\""
              << pts(poly) << "\"/>\n";
      }
      if (r.opts.hatch) {
        for (const auto &[a, b] : doc.displayHatch())
          out << "<line x1=\"" << n3(X(a.x)) << "\" y1=\"" << n3(Y(a.y))
              << "\" x2=\"" << n3(X(b.x)) << "\" y2=\"" << n3(Y(b.y))
              << "\" stroke-width=\"0.13\"/>\n";
      }
    } else {
      const auto &sl = doc.slotList()[size_t(r.opts.slot)];
      for (const auto &pl : doc.horizontalSlice(sl.z)) {
        std::vector<KA::Point2> v;
        for (const auto &q : pl.pts)
          v.push_back({q.x, q.y});
        out << "<polygon fill=\"none\" points=\"" << pts(v) << "\"/>\n";
      }
    }
    out << "</g>\n";
  }
  out << "</g>\n";

  // (11) break: 파단 표시
  layer("break", QStringLiteral("파단 표시"));
  {
    out << "<g stroke=\"#1F1F1F\" stroke-width=\"0.13\" fill=\"none\">\n";
    auto drawBreakMark = [&](double px, double py) {
      const double x1a = px - 0.4, y1a = py - 1.2, x2a = px + 0.4,
                   y2a = py + 1.2;
      const double x1b = px + 0.4, y1b = py - 1.2, x2b = px + 1.2,
                   y2b = py + 1.2;
      out << "<line x1=\"" << n3(X(x1a)) << "\" y1=\"" << n3(Y(y1a))
          << "\" x2=\"" << n3(X(x2a)) << "\" y2=\"" << n3(Y(y2a)) << "\"/>\n";
      out << "<line x1=\"" << n3(X(x1b)) << "\" y1=\"" << n3(Y(y1b))
          << "\" x2=\"" << n3(X(x2b)) << "\" y2=\"" << n3(Y(y2b)) << "\"/>\n";
    };
    if (doc.isSherdMode()) {
      KA::Point2 pt1{d.rim / 2.0, H};
      KA::Point2 pt2{0.0, 0.0};
      bool found = false;
      const auto &dispSec = doc.displaySection();
      if (!dispSec.empty() && !dispSec[0].empty()) {
        const auto &poly = dispSec[0];
        const double distEnds = std::hypot(poly.front().x - poly.back().x,
                                           poly.front().y - poly.back().y);
        if (distEnds > 1.0) {
          if (poly.front().y >= poly.back().y) {
            pt1 = poly.front();
            pt2 = poly.back();
          } else {
            pt1 = poly.back();
            pt2 = poly.front();
          }
          found = true;
        }
      }
      if (!found) {
        const auto &ol = doc.displayOutline();
        if (ol.size() >= 2) {
          const double distEnds = std::hypot(ol.front().x - ol.back().x,
                                             ol.front().y - ol.back().y);
          if (distEnds > 1.0) {
            if (ol.front().y >= ol.back().y) {
              pt1 = ol.front();
              pt2 = ol.back();
            } else {
              pt1 = ol.back();
              pt2 = ol.front();
            }
          }
        }
      }
      drawBreakMark(pt1.x, pt1.y);
      drawBreakMark(pt2.x, pt2.y);
    }
    out << "</g>\n";
  }
  out << "</g>\n";

  // (12) dimension: 치수선·치수
  layer("dimension", QStringLiteral("치수"));
  if (!horiz && d.ok) {
    out << "<g font-family=\"Consolas, monospace\" font-size=\"2.6\" "
           "fill=\"#141413\">\n"
        << "<text x=\"" << n3(X(0)) << "\" y=\"" << n3(Y(H + 8) - 1)
        << "\" text-anchor=\"middle\">구경 ⌀ " << num(d.rim) << " mm</text>\n"
        << "<line x1=\"" << n3(X(-d.rim / 2)) << "\" y1=\"" << n3(Y(H + 8))
        << "\" x2=\"" << n3(X(d.rim / 2)) << "\" y2=\"" << n3(Y(H + 8))
        << "\" stroke=\"#141413\" stroke-width=\"0.18\"/>\n"
        << "</g>\n";
  }
  out << "</g>\n";

  // (13) scalebar: 축척 막대
  layer("scalebar", QStringLiteral("축척 막대"));
  {
    const double ty = L.titleRectMm.top();
    const double sb = 50.0 * s, sx = L.titleRectMm.left(), sy = ty + 12;
    out << "<g>\n"
        << "<rect x=\"" << sx << "\" y=\"" << sy << "\" width=\"" << n3(sb / 2)
        << "\" height=\"1.5\" fill=\"#141413\"/>\n"
        << "<rect x=\"" << n3(sx + sb / 2) << "\" y=\"" << sy << "\" width=\""
        << n3(sb / 2)
        << "\" height=\"1.5\" fill=\"#FFFFFF\" stroke=\"#141413\" "
           "stroke-width=\"0.2\"/>\n"
        << "<text x=\"" << sx << "\" y=\"" << sy + 5
        << "\" font-family=\"Consolas, monospace\" font-size=\"2.6\" "
           "fill=\"#141413\">0</text>\n"
        << "<text x=\"" << n3(sx + sb - 4) << "\" y=\"" << sy + 5
        << "\" font-family=\"Consolas, monospace\" font-size=\"2.6\" "
           "fill=\"#141413\">50 mm</text>\n"
        << "</g>\n";
  }
  out << "</g>\n";

  // (14) label: 표제란·도면명
  layer("label", QStringLiteral("표제란"));
  {
    const double ty = L.titleRectMm.top();
    const double bx = L.titleRectMm.right() - 92;
    const QString az = azimuthNote(doc, r.opts.slot);
    const QString baseStr = (d.base > 0.0 && !doc.isSherdMode())
                                ? num(d.base)
                                : QStringLiteral("—");
    out << "<g font-family=\"Malgun Gothic, Noto Sans KR, sans-serif\" "
           "fill=\"#141413\">\n"
        << "<line x1=\"" << L.titleRectMm.left() << "\" y1=\"" << ty
        << "\" x2=\"" << L.titleRectMm.right() << "\" y2=\"" << ty
        << "\" stroke=\"#141413\" stroke-width=\"0.3\"/>\n"
        << "<text x=\"" << L.titleRectMm.left() << "\" y=\"" << ty + 7
        << "\" font-size=\"2.6\">축척 " << scaleName(s) << " · 단위 mm"
        << (az.isEmpty() ? QString() : QStringLiteral(" · ") + az)
        << "</text>\n"
        << "<rect x=\"" << bx << "\" y=\"" << ty + 3
        << "\" width=\"92\" height=\"18\" fill=\"none\" stroke=\"#141413\" "
           "stroke-width=\"0.25\"/>\n"
        << "<text x=\"" << bx + 3 << "\" y=\"" << ty + 9
        << "\" font-size=\"3\" font-weight=\"700\">" << r.title.toHtmlEscaped()
        << "</text>\n"
        << "<text x=\"" << bx + 3 << "\" y=\"" << ty + 17
        << "\" font-family=\"Consolas, monospace\" font-size=\"2.4\">기고 "
        << num(d.height) << " · 구경 " << num(d.rim) << " · 저경 " << baseStr
        << " mm</text>\n"
        << "</g>\n";
  }
  out << "</g>\n";

  out << "</svg>\n";
  return true;
}

// ---- DXF R12 (K-Arch 11 레이어, 1:1 mm, 화면과 같은 배치(D5 기본 외면
// 왼쪽·단면 오른쪽, R 반전 따름))
// ----------------------------------------------------------
bool exportDxf(const ArtifactDoc &doc, const ExportRequest &r, QString *err) {
  if (refusePreview(doc, err))
    return false;
  KA::DxfWriter w;
  if (!w.Open(r.path.toLocal8Bit().toStdString())) {
    if (err)
      *err = QStringLiteral("DXF 파일을 만들 수 없습니다.");
    return false;
  }
  using LT = KA::DxfLinetype;
  w.DefineLayer("01_AXIS_CENTER", 7, LT::Continuous);
  w.DefineLayer("02_EXTERIOR_OUTLINE", 7, LT::Continuous);
  w.DefineLayer("03_EXTERIOR_FEATURES", 8, LT::Continuous);
  w.DefineLayer("04_SECTION_WALL", 1, LT::Continuous);
  w.DefineLayer("05_SECTION_HATCH", 1, LT::Continuous);
  w.DefineLayer("06_INTERIOR_OUTLINE", 1, LT::Dashed);
  w.DefineLayer("07_PROJECTION_RIM", 7, LT::Continuous);
  w.DefineLayer("08_PROJECTION_BASE", 7, LT::Continuous);
  w.DefineLayer("09_DIMENSION_MEASURE", 3, LT::Continuous);
  w.DefineLayer("10_ANNOTATION_TEXT", 7, LT::Continuous);
  w.DefineLayer("11_SCALE_BAR", 7, LT::Continuous);
  const double H = height(doc);
  // 기준선: 중심선 01 · 구연선 07 · 바닥선 08 (내보내기 칸과 무관하게 늘, 1:1
  // 모델 공간이라 중심선 연장 3 mm)
  const PlateRefLines refs = plateRefLines(doc, 3.0);
  for (const auto &k : kRefStyles)
    if (const auto &l = refs.*k.line)
      w.WriteLine(k.dxfLayer, l->a, l->b);
  const auto ol = doc.displayOutline();
  if (ol.size() >= 2) {
    w.WritePolyline("02_EXTERIOR_OUTLINE", ol, false);
    for (const auto &q : closingEnds(ol, refs))
      w.WriteLine("02_EXTERIOR_OUTLINE", {0, q.y}, q);
  }
  if (doc.hasEstimatedInner()) {
    for (const auto &solid : doc.displayScannedSolid())
      w.WritePolyline("04_SECTION_WALL", solid, false);
    for (const auto &est : doc.displayEstimatedInner())
      w.WritePolyline("06_INTERIOR_OUTLINE", est, false);
  } else {
    for (const auto &poly : doc.displaySection())
      w.WritePolyline("04_SECTION_WALL", poly, true);
  }
  if (r.opts.hatch)
    for (const auto &[a, b] : doc.displayHatch())
      w.WriteLine("05_SECTION_HATCH", a, b);
  const auto &d = doc.dims();
  if (d.ok) {
    w.WriteLine("09_DIMENSION_MEASURE", {-d.rim / 2, H + 8},
                {d.rim / 2, H + 8});
    char buf[96];
    std::snprintf(buf, sizeof buf, "RIM D. %.1fmm", d.rim);
    w.WriteText("10_ANNOTATION_TEXT", {-d.rim / 2, H + 10}, 2.5, buf);
    std::snprintf(buf, sizeof buf, "H %.1fmm  BASE D. %.1fmm  MAX D. %.1fmm",
                  d.height, d.base, d.belly);
    w.WriteText("10_ANNOTATION_TEXT", {-d.rim / 2, -8}, 2.5, buf);
  }
  if (doc.isLoaded() && doc.poseConfirmed()) // DXF 는 늘 종단면(정면각) 도판
    w.WriteText("10_ANNOTATION_TEXT", {-maxR(doc), -12}, 2.5,
                "FRONT AZIMUTH " + azimuthDxf(doc.frontAngle()) +
                    " DEG (CCW, TOP VIEW)");
  const double sx = maxR(doc) - 40, sy = -14;
  w.WriteSolid("11_SCALE_BAR", {sx, sy}, {sx + 25, sy}, {sx, sy + 1.5},
               {sx + 25, sy + 1.5});
  w.WritePolyline(
      "11_SCALE_BAR",
      {{sx, sy}, {sx + 50, sy}, {sx + 50, sy + 1.5}, {sx, sy + 1.5}}, true);
  w.WriteText("11_SCALE_BAR", {sx + 52, sy}, 2.5, "50 mm (1:1)");
  if (!w.Close()) {
    if (err)
      *err = QStringLiteral("DXF 저장 실패");
    return false;
  }
  return true;
}

} // namespace studio
