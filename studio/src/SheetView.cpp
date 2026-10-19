#include "SheetView.h"

#include "CutPageText.h"
#include "Theme.h"

#include <QHBoxLayout>
#include <QMouseEvent>
#include <QPainter>
#include <QStyle>
#include <QVBoxLayout>
#include <QWheelEvent>
#include <cmath>

namespace studio {

// ---- SheetCanvas ----------------------------------------------------------
SheetCanvas::SheetCanvas(QWidget *parent) : QWidget(parent) {
  setMouseTracking(true);
  setMinimumSize(320, 240);
  setAttribute(Qt::WA_OpaquePaintEvent);
  setCursor(Qt::CrossCursor);
}

void SheetCanvas::setDoc(const ArtifactDoc *d) {
  doc_ = d;
  fitted_ = false;
  docChanged();
}

void SheetCanvas::setOptions(const PlateOptions &o) {
  const bool reshade =
      o.shadeMode != opts_.shadeMode || (o.shade && !opts_.shade);
  const bool refit = o.slot != opts_.slot;
  opts_ = o;
  if (reshade)
    shadeDirty_ = true;
  if (refit)
    fitView();
  update();
}

void SheetCanvas::docChanged() {
  shadeDirty_ = true;
  if (!fitted_)
    fitView();
  update();
}

void SheetCanvas::fitView() {
  if (!doc_ || !doc_->isLoaded() || !doc_->poseConfirmed() || width() < 50)
    return; // 확정 전에는 도판이 없다
  const QRectF E = Plate::extentMm(*doc_, opts_.slot);
  const double mx = 64, my = 40;
  ppm_ = std::max(0.2, std::min((width() - 2 * mx) / E.width(),
                                (height() - 2 * my) / E.height()));
  centerMm_ = E.center();
  fitted_ = true;
  update();
}

QTransform SheetCanvas::xf() const {
  return QTransform(ppm_, 0, 0, -ppm_, width() / 2.0 - centerMm_.x() * ppm_,
                    height() / 2.0 + centerMm_.y() * ppm_);
}

double SheetCanvas::outerRadiusAt(double h) const {
  if (!doc_)
    return -1;
  return doc_->outerRadiusAt(h);
}

void SheetCanvas::paintEvent(QPaintEvent *) {
  QPainter p(this);
  p.fillRect(rect(), theme::Surface);
  if (!doc_ || !doc_->isLoaded()) {
    p.setPen(theme::Muted);
    p.setFont(theme::uiFont(13));
    p.drawText(
        rect(), Qt::AlignCenter,
        QStringLiteral("유물을 열면 실측 도판이 여기에 그려집니다  ·  Ctrl+O"));
    return;
  }
  if (!doc_->poseConfirmed()) {
    p.setRenderHint(QPainter::Antialiasing, true);
    const QRect r = rect();
    if (!doc_->isUnitConfirmed()) {
      const QRectF box(r.center().x() - 180, r.center().y() - 40, 360, 80);
      p.setPen(QPen(theme::Edge, 1));
      p.setBrush(theme::Surface);
      p.drawRoundedRect(box, 8, 8);
      p.setFont(theme::uiFont(13, true));
      p.setPen(theme::Ink);
      p.drawText(box.adjusted(10, 15, -10, -35), Qt::AlignCenter,
                 QStringLiteral("단위를 확정하면 미리보기가 나옵니다"));
      p.setFont(theme::uiFont(11));
      p.setPen(theme::Muted);
      p.drawText(box.adjusted(10, 42, -10, -10), Qt::AlignCenter,
                 QStringLiteral(
                     "(바닥 후보 · 기준선 · 치수는 단위 확정 전까지 잠김)"));
      return;
    }

    const int wCard = std::clamp((r.width() - 40) / 2, 160, 280);
    const int hCard = std::clamp(r.height() - 60, 160, 320);
    const int yTop = (r.height() - hCard) / 2;
    const int xCardA = r.center().x() - wCard - 10;
    const int xCardB = r.center().x() + 10;
    const QRectF cardA(xCardA, yTop, wCard, hCard);
    const QRectF cardB(xCardB, yTop, wCard, hCard);

    if (doc_->isSherdMode()) {
      p.setPen(QPen(theme::Edge, 1));
      p.setBrush(theme::Surface);
      p.drawRoundedRect(cardA, 6, 6);
      p.drawRoundedRect(cardB, 6, 6);

      p.setFont(theme::uiFont(12, true));
      p.setPen(theme::Ink);
      p.drawText(cardA.adjusted(12, 12, -12, 0), Qt::AlignTop | Qt::AlignLeft,
                 QStringLiteral("틈 보기 — 구연 접지면 이격"));
      p.drawText(cardB.adjusted(12, 12, -12, 0), Qt::AlignTop | Qt::AlignLeft,
                 QStringLiteral("✓ 도판에서는 구연을 위로"));

      const auto gap = doc_->rimGapChart();
      const double x0 = cardA.left() + 20, x1 = cardA.right() - 20;
      const double y0 = cardA.bottom() - 40;
      p.setPen(QPen(theme::Line, 1));
      p.drawLine(QPointF(x0, y0), QPointF(x1, y0));
      p.setPen(QPen(QColor(0x2f, 0x7a, 0x4f), 1, Qt::DashLine));
      p.drawLine(QPointF(x0, y0 - 15), QPointF(x1, y0 - 15));
      p.setFont(theme::monoFont(9));
      p.setPen(QColor(0x2f, 0x7a, 0x4f));
      p.drawText(QPointF(x0, y0 - 18), QStringLiteral("닿음 기준 0.3 mm"));

      if (!gap.gapMm.empty()) {
        const double barW = (x1 - x0) / double(gap.gapMm.size());
        for (size_t i = 0; i < gap.gapMm.size(); ++i) {
          const double gVal = gap.gapMm[i];
          const double bh = std::min(45.0, gVal * 50.0);
          const QRectF br(x0 + i * barW, y0 - bh, std::max(1.0, barW - 1.0),
                          bh);
          p.fillRect(br, gVal > 0.3 ? QColor(0xe0, 0x8a, 0x1a)
                                    : QColor(0x2f, 0x7a, 0x4f));
        }
      }
      p.setFont(theme::uiFont(10));
      p.setPen(theme::Muted);
      p.drawText(cardA.adjusted(12, 0, -12, -10),
                 Qt::AlignBottom | Qt::AlignLeft,
                 QStringLiteral("초록 = 닿음 · 주황 = 틈 (최대 %1 mm)")
                     .arg(gap.maxGapMm, 0, 'f', 2));

      p.drawText(
          cardB.adjusted(12, 40, -12, 0), Qt::AlignTop | Qt::AlignLeft,
          QStringLiteral("추정 구경 ⌀ %1 mm (±%2 mm)\n호 각도 %3° (잔존율 "
                         "%4%)\n저경은 공란(\"—\") 원칙")
              .arg(doc_->estimatedRimDiameter(), 0, 'f', 1)
              .arg(doc_->estimatedRimError(), 0, 'f', 2)
              .arg(doc_->rimArcDegrees(), 0, 'f', 0)
              .arg(int(std::round(doc_->rimArcDegrees() / 360.0 * 100))));
      return;
    }

    if (!doc_->isGrounded() || doc_->isLying()) {
      p.setPen(QPen(QColor(0xd8, 0x35, 0x2a, 140), 1.5));
      p.setBrush(theme::Surface);
      p.drawRoundedRect(cardA, 6, 6);
      p.setFont(theme::uiFont(12, true));
      p.setPen(QColor(0xd8, 0x35, 0x2a));
      p.drawText(cardA.adjusted(12, 12, -12, 0), Qt::AlignTop | Qt::AlignLeft,
                 QStringLiteral("✗ 지금 자세 그대로 자르면"));
      p.setFont(theme::uiFont(10.5));
      p.setPen(theme::Ink);
      p.drawText(
          cardA.adjusted(12, 40, -12, -10), Qt::AlignTop | Qt::AlignLeft,
          QStringLiteral("• 바닥선 없음 · 중심선 없음\n• 높이가 가로 폭으로 "
                         "잘못 잼\n\n누운 채로 자르면 바닥·중심을\n정할 수 "
                         "없어 기고·구경이 틀립니다."));

      p.setPen(QPen(QColor(0x2f, 0x7a, 0x4f, 160), 1.5));
      p.drawRoundedRect(cardB, 6, 6);
      p.setFont(theme::uiFont(12, true));
      p.setPen(QColor(0x2f, 0x7a, 0x4f));
      p.drawText(cardB.adjusted(12, 12, -12, 0), Qt::AlignTop | Qt::AlignLeft,
                 QStringLiteral("✓ ① 저부 접지면을 바닥으로"));
      p.setFont(theme::uiFont(10.5));
      p.setPen(theme::Ink);
      p.drawText(cardB.adjusted(12, 40, -12, -10), Qt::AlignTop | Qt::AlignLeft,
                 QStringLiteral("• 바닥선 = 바닥판\n• 중심선 = 3D 중심축\n• "
                                "구연선 = 구연 평면\n\n후보 평면 또는 자동 "
                                "세우기를\n누르면 바닥판에 정위치합니다."));
      return;
    }

    const bool isRim = doc_->baseTiltMode() == BaseTiltMode::RimHorizontal;
    p.setPen(QPen(isRim ? theme::Line : theme::Edge, isRim ? 1.0 : 2.0));
    p.setBrush(isRim ? theme::Surface : theme::Oat);
    p.drawRoundedRect(cardA, 6, 6);
    p.setFont(theme::uiFont(12, true));
    p.setPen(theme::Ink);
    p.drawText(cardA.adjusted(12, 12, -12, 0), Qt::AlignTop | Qt::AlignLeft,
               QStringLiteral("바닥 수평 (B) — 바닥판 그대로"));
    p.setFont(theme::uiFont(10.5));
    p.setPen(theme::Muted);
    p.drawText(cardA.adjusted(12, 38, -12, -10), Qt::AlignTop | Qt::AlignLeft,
               QStringLiteral(
                   "바닥이 고른 토기에 맞음.\n구연선이 기운 채로 유지됩니다."));

    p.setPen(QPen(isRim ? theme::Edge : theme::Line, isRim ? 2.0 : 1.0));
    p.setBrush(isRim ? theme::Oat : theme::Surface);
    p.drawRoundedRect(cardB, 6, 6);
    p.setFont(theme::uiFont(12, true));
    p.setPen(theme::Ink);
    p.drawText(cardB.adjusted(12, 12, -12, 0), Qt::AlignTop | Qt::AlignLeft,
               QStringLiteral("✓ 구연 수평 (T) — 보정 · 권고"));
    p.setFont(theme::uiFont(10.5));
    p.setPen(theme::Muted);
    p.drawText(cardB.adjusted(12, 38, -12, -10), Qt::AlignTop | Qt::AlignLeft,
               QStringLiteral("입 둘레를 수평으로 맞춤 (보고서 관행).\n바닥 "
                              "미세 기울기를 자동 보정합니다."));
    return;
  }
  if (shadeDirty_ && opts_.shade) {
    shade_ = Plate::renderShade(*doc_, opts_.shadeMode, 4.0);
    shadeDirty_ = false;
  }
  const QTransform t = xf();
  Plate::paint(p, *doc_, t, ppm_, 12.0, opts_, &shade_);

  // Feature 22: 짝 표시 보라 점 (#6b4fc0) — H=95 mm 단면점 동기화
  if (opts_.section) {
    const double rMatch = outerRadiusAt(95.0);
    if (rMatch > 0.0) {
      const double s = doc_->sectionSign();
      const QPointF pMatch = t.map(QPointF(s * rMatch, 95.0));
      p.setRenderHint(QPainter::Antialiasing, true);
      p.setPen(QPen(Qt::white, 1.5));
      p.setBrush(QColor(0x6b, 0x4f, 0xc0));
      p.drawEllipse(pMatch, 5.0, 5.0);
      p.setFont(theme::monoFont(9, true));
      p.setPen(QColor(0x6b, 0x4f, 0xc0));
      p.drawText(pMatch + QPointF(8, 3), QStringLiteral("H 95 짝"));
    }
  }

  // Feature 21: 실시간 단면 상태 검사선 (Cut Checks 상태 배지)
  if (opts_.section) {
    const auto cs = doc_->checkCutStatus();
    const QRectF badge(12, 12, 320, 24);
    p.setPen(QPen(theme::Line, 1));
    p.setBrush(QColor(255, 255, 255, 220));
    p.drawRoundedRect(badge, 4, 4);
    p.setFont(theme::uiFont(10.5));
    p.setPen(cs.closedLoop ? QColor(0x2f, 0x7a, 0x4f)
                           : QColor(0xd8, 0x35, 0x2a));
    p.drawText(badge.adjusted(10, 0, -10, 0), Qt::AlignVCenter | Qt::AlignLeft,
               cs.statusText);
  }

  // 커서 풍선 (기벽 두께) — 단면 쪽(D5 기본 오른쪽, R 반전 시 왼쪽)에 마우스를
  // 대면
  const bool vertical =
      opts_.slot < 0 || opts_.slot >= int(doc_->slotList().size()) ||
      doc_->slotList()[size_t(opts_.slot)].kind == SlotKind::Vertical;
  if (hover_ && vertical && opts_.section) {
    const double h = hoverMm_.y();
    const double wall = doc_->wallThicknessAt(h);
    const double rOut = outerRadiusAt(h);
    const double s = doc_->sectionSign(); // 단면 쪽 x 부호
    if (wall > 0 && rOut > 0 && s * hoverMm_.x() > 0 &&
        s * hoverMm_.x() < rOut + 15) {
      p.setRenderHint(QPainter::Antialiasing);
      const QPointF dot = t.map(QPointF(s * rOut, h));
      p.setPen(QPen(theme::ProfileRed, 1));
      p.setBrush(theme::ProfileRed);
      p.drawEllipse(dot, 3.5, 3.5);
      const bool est = doc_->isWallEstimatedAt(h);
      const QString l1 =
          est ? QStringLiteral("추정 기벽 %1 mm (미스캔 보정)")
                    .arg(wall, 0, 'f', 1)
              : QStringLiteral("기벽 두께 %1 mm").arg(wall, 0, 'f', 1);
      const QString l2 = QStringLiteral("높이 H %1 · 외경 %2")
                             .arg(h, 0, 'f', 1)
                             .arg(2 * rOut, 0, 'f', 1);
      const QFont f1 = theme::monoFont(13, true), f2 = theme::monoFont(11);
      const double w = std::max(QFontMetricsF(f1).horizontalAdvance(l1),
                                QFontMetricsF(f2).horizontalAdvance(l2)) +
                       22;
      // 풍선 상자는 축에서 먼 쪽 (화면 밖이면 반대쪽)
      QRectF box(dot.x() - w - 14, dot.y() - 22, w, 44);
      if (s > 0) {
        box.moveLeft(dot.x() + 14);
        if (box.right() > width() - 4)
          box.moveRight(dot.x() - 14);
      } else if (box.left() < 4) {
        box.moveLeft(dot.x() + 14);
      }
      p.setPen(QPen(theme::Edge, 1));
      p.setBrush(theme::Surface);
      p.drawRoundedRect(box, 4, 4);
      p.setFont(f1);
      p.setPen(est ? theme::ProfileRed : theme::Ink);
      p.drawText(box.adjusted(11, 5, 0, 0), Qt::AlignLeft | Qt::AlignTop, l1);
      p.setFont(f2);
      p.setPen(theme::Muted);
      p.drawText(box.adjusted(11, 0, 0, -6), Qt::AlignLeft | Qt::AlignBottom,
                 l2);
    }
  }
}

void SheetCanvas::resizeEvent(QResizeEvent *) {
  if (!fitted_)
    fitView();
}

void SheetCanvas::wheelEvent(QWheelEvent *e) {
  const QPointF pos = e->position();
  const QPointF before = xf().inverted().map(pos);
  ppm_ = std::clamp(ppm_ * std::pow(1.0015, e->angleDelta().y()), 0.2, 80.0);
  const QPointF after = xf().inverted().map(pos);
  centerMm_ += before - after;
  update();
}

void SheetCanvas::mousePressEvent(QMouseEvent *e) {
  if (e->button() == Qt::MiddleButton || e->button() == Qt::RightButton ||
      e->button() == Qt::LeftButton) {
    panning_ = true;
    lastPos_ = e->pos();
    setCursor(Qt::ClosedHandCursor);
  }
}

void SheetCanvas::mouseMoveEvent(QMouseEvent *e) {
  if (panning_) {
    const QPoint d = e->pos() - lastPos_;
    lastPos_ = e->pos();
    centerMm_ -= QPointF(d.x() / ppm_, -d.y() / ppm_);
    update();
    return;
  }
  hoverMm_ = xf().inverted().map(QPointF(e->pos()));
  hover_ = true;
  const double wall =
      doc_ && doc_->isLoaded() ? doc_->wallThicknessAt(hoverMm_.y()) : -1;
  emit cursorMm(hoverMm_.x(), hoverMm_.y(), wall, true);
  update();
}

void SheetCanvas::mouseReleaseEvent(QMouseEvent *) {
  panning_ = false;
  setCursor(Qt::CrossCursor);
}

void SheetCanvas::mouseDoubleClickEvent(QMouseEvent *) { fitView(); }

void SheetCanvas::leaveEvent(QEvent *) {
  hover_ = false;
  emit cursorMm(0, 0, -1, false);
  update();
}

// ---- SheetPane ------------------------------------------------------------
SheetPane::SheetPane(QWidget *parent) : QWidget(parent) {
  setObjectName("SheetPane");
  setAttribute(Qt::WA_StyledBackground, true);
  auto *v = new QVBoxLayout(this);
  v->setContentsMargins(0, 0, 0, 0);
  v->setSpacing(0);

  auto *head = new QWidget(this);
  head->setObjectName("SheetHead");
  head->setAttribute(Qt::WA_StyledBackground, true);
  head->setFixedHeight(32);
  auto *h = new QHBoxLayout(head);
  h->setContentsMargins(12, 0, 8, 0);
  h->setSpacing(8);
  title_ = new QLabel(head);
  title_->setObjectName("SheetTitle");
  title_->setFont(theme::serifFont(19));
  sub_ = new QLabel(head);
  sub_->setObjectName("SheetSub");
  sub_->setStyleSheet(QStringLiteral("color:%1; font-size:12px;")
                          .arg(theme::hex(theme::Muted)));
  badge_ = new QLabel(head);
  badge_->setFixedHeight(22);
  scale_ = new QLabel(QStringLiteral("1:1"), head);
  scale_->setFont(theme::monoFont(12, true));
  scale_->setToolTip(QStringLiteral(
      "화면은 실제 mm 좌표 · 휠로 확대, 끌어서 이동, 두 번 눌러 맞춤"));
  maxBtn_ = new QPushButton(QStringLiteral("□"), head);
  maxBtn_->setProperty("icon-btn", true);
  maxBtn_->setToolTip(QStringLiteral("도면 100% 최대화 (Tab)"));
  maxBtn_->setFocusPolicy(Qt::NoFocus);
  connect(maxBtn_, &QPushButton::clicked, this, &SheetPane::maximizeToggled);
  h->addWidget(title_);
  h->addWidget(sub_);
  h->addStretch();
  h->addWidget(badge_);
  h->addWidget(scale_);
  h->addWidget(maxBtn_);
  v->addWidget(head);

  auto *strip = new QWidget(this);
  strip->setObjectName("InfoStrip");
  strip->setAttribute(Qt::WA_StyledBackground, true);
  strip->setFixedHeight(28);
  auto *sh = new QHBoxLayout(strip);
  sh->setContentsMargins(12, 0, 12, 0);
  dims_ = new QLabel(strip);
  dims_->setTextFormat(Qt::RichText);
  order_ = new QLabel(QStringLiteral("그리는 순서: 방안눈금 → 탁본음영 → "
                                     "기준선(먹) → 외곽선(먹) → 단면선(빨강)"),
                      strip);
  order_->setStyleSheet(
      QStringLiteral("color:%1;").arg(theme::hex(theme::Muted)));
  // 그리는 순서 글이 창 최소 폭·3D 보조창 폭(R1 27 %, ③ 330 px)을 밀지 않게 —
  // 좁으면 글 끝이 잘린다. (0 은 Qt 에서 「정하지 않음」이라 글 폭 그대로
  // 밀었다 — 오프스크린 □ 글꼴에서 3D 가 최소 폭으로 눌림)
  order_->setMinimumWidth(1);
  sh->addWidget(dims_);
  sh->addStretch();
  sh->addWidget(order_);
  v->addWidget(strip);

  canvas_ = new SheetCanvas(this);
  v->addWidget(canvas_, 1);
}

void SheetPane::refresh(const ArtifactDoc &doc, int slot) {
  const auto &sl = doc.slotList();
  if (slot >= 0 && slot < int(sl.size())) {
    const auto &s = sl[size_t(slot)];
    const bool live =
        cutPage_ &&
        s.kind == SlotKind::Vertical; // ② 종단면: 끄는 동안 각도를 따라간다
    title_->setText(live ? cutpage::plateTitle(s.name.at(0), doc.previewAngle())
                         : s.name);
    QString subText =
        live ? cutpage::plateSub(doc.sectionSide())
        : s.kind != SlotKind::Vertical
            ? QStringLiteral("H %1 mm 수평 절단 평면").arg(s.z, 0, 'f', 1)
        : doc.sectionSide() == SectionSide::Right
            ? QStringLiteral("외면 왼쪽 · 단면 오른쪽 (국내 보고서 표준)")
            : QStringLiteral("단면 왼쪽 · 외면 오른쪽 (R 반전)");
    if (s.kind == SlotKind::Vertical && doc.hasEstimatedInner())
      subText += QStringLiteral(" · 내벽 미스캔 파선 보정(%1 mm)")
                     .arg(doc.estimatedWallMm(), 0, 'f', 1);
    sub_->setText(subText);
  } else {
    title_->setText(QStringLiteral("실측 도판"));
    sub_->clear();
  }
  if (doc.isLoaded() && !doc.poseConfirmed()) {
    badge_->setObjectName("BadgeWarn");
    badge_->setText(QStringLiteral("▲ 바닥 미정"));
  } else if (doc.aligned()) {
    badge_->setObjectName("BadgeOk");
    badge_->setText(QStringLiteral("✓ 기준: %1").arg(doc.alignNote()));
  } else {
    badge_->setObjectName("BadgeWarn");
    badge_->setText(QStringLiteral("▲ %1").arg(
        doc.alignNote().isEmpty() ? QStringLiteral("정위치 필요")
                                  : doc.alignNote()));
  }
  badge_->style()->unpolish(badge_);
  badge_->style()->polish(badge_);

  const auto &d = doc.dims();
  const QString mu = theme::hex(theme::Muted), fa = theme::hex(theme::Faint);
  auto cell = [&](const QString &k, double v) {
    return QStringLiteral(
               "<span style='color:%1'>%2</span>&nbsp;<span "
               "style='font-family:Consolas; font-weight:700'>%3</span>"
               "<span style='color:%4; font-family:Consolas'>&nbsp;mm</span>")
        .arg(mu, k, v > 0 ? QString::number(v, 'f', 1) : QStringLiteral("—"),
             fa);
  };
  const QString sep = QStringLiteral("&nbsp;&nbsp;&nbsp;&nbsp;");
  dims_->setText(cell(QStringLiteral("기고"), d.height) + sep +
                 cell(QStringLiteral("구경"), d.rim) + sep +
                 cell(QStringLiteral("최대복경"), d.belly) + sep +
                 cell(QStringLiteral("저경"), d.base) + sep +
                 cell(QStringLiteral("평균 기벽두께"), d.meanWall));
  dims_->setToolTip(
      QStringLiteral("최대복경 높이 H %1 mm").arg(d.bellyZ, 0, 'f', 1));
}

void SheetPane::setMaximized(bool on) {
  maxBtn_->setProperty("state", on ? "on" : "idle");
  maxBtn_->setText(on ? QStringLiteral("▣") : QStringLiteral("□"));
  maxBtn_->style()->unpolish(maxBtn_);
  maxBtn_->style()->polish(maxBtn_);
}

} // namespace studio
