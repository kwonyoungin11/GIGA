#include "View3D.h"

#include "CutPageText.h"
#include "Theme.h"

#include <QHBoxLayout>
#include <QLocale>
#include <QMouseEvent>
#include <QPainter>
#include <QPushButton>
#include <QVBoxLayout>
#include <cmath>

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
} // namespace

View3DCanvas::View3DCanvas(QWidget *parent) : QWidget(parent) {
  setMinimumSize(200, 200);
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
    const FloorPlate f = floorPlate(doc_->mesh());
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

    // Feature 14: 바닥 후보 평면 배지 ①②③
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
              .arg(c.recommended ? QStringLiteral(" [추천]") : QString());
      p.setFont(theme::uiFont(10, true));
      const QFontMetricsF fm(p.font());
      const double bw = fm.horizontalAdvance(label) + 12;
      const QRectF br(pt.x() - bw / 2, pt.y() - 11, bw, 22);
      p.setPen(QPen(c.recommended ? QColor(0x2f, 0x7a, 0x4f) : theme::Line,
                    c.recommended ? 1.5 : 1.0));
      p.setBrush(c.recommended ? QColor(240, 250, 244, 230)
                               : QColor(255, 255, 255, 220));
      p.drawRoundedRect(br, 4, 4);
      p.setPen(c.recommended ? QColor(0x2f, 0x7a, 0x4f) : theme::Ink);
      p.drawText(br, Qt::AlignCenter, label);
    }
    return;
  }
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
  p.setBrush(QColor(255, 0, 0, 22));
  p.drawPolygon(quad);
  p.drawImage(0, 0, img_);
  p.setPen(QPen(theme::ProfileRed, 1, Qt::DashLine));
  p.setBrush(Qt::NoBrush);
  p.drawPolygon(quad);

  // 중심축
  p.setPen(QPen(theme::Ink, 1, Qt::DashDotLine));
  p.drawLine(projectPoint({0, 0, z0}, v), projectPoint({0, 0, z1}, v));

  // 절단 단면선. 종단면(수직 슬롯)은 선분 중점의 월드 x 로 나눠 단면 쪽 반(x ≥
  // 0 = 도판의 단면)만 흰 3 px 테 + 빨강으로 굵게, 반대쪽 반(x < 0 = 도판의
  // 외면)은 가는 빨강 파선. 수평 슬롯은 전부 굵게.
  std::vector<QPolygonF> thick, thin;
  for (const auto &pl : cut_) {
    for (size_t i = 0; i + 1 < pl.pts.size(); ++i) {
      const bool sec = horiz || pl.pts[i].x + pl.pts[i + 1].x >= 0.0;
      auto &runs = sec ? thick : thin;
      const bool cont =
          i > 0 && (horiz || (pl.pts[i - 1].x + pl.pts[i].x >= 0.0) == sec);
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

  // Feature 18: 3D 회전판 및 빨간 손잡이 (#d8352a)
  if (!horiz) {
    const double ringR = R * 1.12;
    QPolygonF ringPoly;
    for (int deg = 0; deg <= 360; deg += 6) {
      const double rad = deg * M_PI / 180.0;
      ringPoly << projectPoint(
          {ringR * std::cos(rad), ringR * std::sin(rad), z0}, v);
    }
    p.setPen(QPen(theme::Line, 1.2, Qt::SolidLine));
    p.setBrush(Qt::NoBrush);
    p.drawPolyline(ringPoly);

    // 눈금 (30° 간격)
    p.setPen(QPen(theme::Muted, 1.0));
    for (int deg = 0; deg < 360; deg += 30) {
      const double rad = deg * M_PI / 180.0;
      const QPointF p0 = projectPoint(
          {ringR * 0.96 * std::cos(rad), ringR * 0.96 * std::sin(rad), z0}, v);
      const QPointF p1 = projectPoint(
          {ringR * 1.04 * std::cos(rad), ringR * 1.04 * std::sin(rad), z0}, v);
      p.drawLine(p0, p1);
    }

    // 현재 각도 빨간 손잡이 (Red Knob #d8352a)
    const double fa = doc_->frontAngle() * M_PI / 180.0;
    const QPointF knobPt =
        projectPoint({ringR * std::cos(fa), ringR * std::sin(fa), z0}, v);
    p.setPen(QPen(Qt::white, 2.0));
    p.setBrush(QColor(0xd8, 0x35, 0x2a));
    p.drawEllipse(knobPt, 7.0, 7.0);

    const QString angStr =
        QStringLiteral("%1°").arg(doc_->frontAngle(), 0, 'f', 0);
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
}

void View3DCanvas::mousePressEvent(QMouseEvent *e) {
  drag_ = true;
  cutDrag_ = cutTool_ && e->button() == Qt::LeftButton;
  last_ = e->pos();
  setCursor(Qt::ClosedHandCursor);
}

void View3DCanvas::mouseMoveEvent(QMouseEvent *e) {
  if (!drag_)
    return;
  const QPoint d = e->pos() - last_;
  last_ = e->pos();
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
  const bool cutDrag = drag_ && cutDrag_;
  drag_ = false;
  setCursor(Qt::OpenHandCursor);
  if (cutDrag)
    emit cutDragReleased();
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

  canvas_ = new View3DCanvas(this);
  v->addWidget(canvas_, 1);
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
  const QString mu = theme::hex(theme::Muted);
  const bool base = doc.isLoaded() && !doc.poseConfirmed();
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
