#include "ExportDialog.h"

#include "Theme.h"

#include <QCheckBox>
#include <QComboBox>
#include <QDesktopServices>
#include <QDir>
#include <QFileDialog>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QMessageBox>
#include <QPainter>
#include <QPushButton>
#include <QRegularExpression>
#include <QSettings>
#include <QStandardPaths>
#include <QStyle>
#include <QUrl>
#include <QVBoxLayout>

namespace studio {

void PaperPreview::paintEvent(QPaintEvent *) {
  QPainter p(this);
  p.fillRect(rect(), theme::Surface);
  if (!doc_)
    return;
  const QSizeF ps = paperSize(req_.paper);
  const double ppm =
      std::min((width() - 2.0) / ps.width(), (height() - 2.0) / ps.height());
  const QSizeF dev(ps.width() * ppm, ps.height() * ppm);
  p.translate((width() - dev.width()) / 2, (height() - dev.height()) / 2);
  ExportRequest r = req_;
  paintSheet(p, *doc_, r, ppm);
  p.setPen(QPen(theme::Ink, 1));
  p.setBrush(Qt::NoBrush);
  p.drawRect(QRectF(QPointF(0, 0), dev));
}

ExportDialog::ExportDialog(const ArtifactDoc &doc, int slot,
                           const PlateOptions &screenOpts, ExportFormat fmt,
                           QWidget *parent)
    : QDialog(parent), doc_(doc), slot_(slot), base_(screenOpts), fmt_(fmt) {
  setWindowTitle(QStringLiteral("유물 실측 도면 내보내기"));
  setModal(true);
  resize(940, 680);
  setStyleSheet(QStringLiteral("QDialog{background:%1;}")
                    .arg(theme::hex(theme::Surface)));
  auto *root = new QVBoxLayout(this);
  root->setContentsMargins(0, 0, 0, 0);
  root->setSpacing(0);

  // 머리
  auto *head = new QWidget(this);
  head->setObjectName("Head");
  head->setFixedHeight(44);
  head->setStyleSheet(
      QStringLiteral("QWidget#Head{background:%1; border-bottom:1px solid %2;}")
          .arg(theme::hex(theme::Surface), theme::hex(theme::Line)));
  auto *hh = new QHBoxLayout(head);
  hh->setContentsMargins(20, 0, 20, 0);
  auto *ht = new QLabel(QStringLiteral("유물 실측 도면 내보내기"), head);
  ht->setFont(theme::serifFont(19));
  hh->addWidget(ht);
  const int nslots = int(doc.slotList().size());
  auto *hs = new QLabel(QStringLiteral("%1 · 슬롯 %2개 중 %3번째")
                            .arg(doc.name())
                            .arg(nslots)
                            .arg(slot + 1),
                        head);
  hs->setStyleSheet(QStringLiteral("color:%1;").arg(theme::hex(theme::Muted)));
  hh->addWidget(hs);
  hh->addStretch();
  root->addWidget(head);

  auto *body = new QHBoxLayout;
  body->setContentsMargins(0, 0, 0, 0);
  body->setSpacing(0);
  // 왼쪽: 미리보기
  auto *left = new QWidget(this);
  left->setObjectName("Left");
  left->setStyleSheet(QStringLiteral("QWidget#Left{background:%1;}")
                          .arg(theme::hex(theme::Surface)));
  auto *lv = new QVBoxLayout(left);
  lv->setContentsMargins(20, 16, 20, 16);
  previewHead_ = new QLabel(left);
  previewHead_->setStyleSheet(
      QStringLiteral("color:%1;").arg(theme::hex(theme::Muted)));
  lv->addWidget(previewHead_);
  preview_ = new PaperPreview(left);
  lv->addWidget(preview_, 1);
  advice_ = new QLabel(left);
  advice_->setWordWrap(true);
  advice_->setMinimumHeight(44);
  lv->addWidget(advice_);
  body->addWidget(left, 11);

  // 오른쪽: 설정
  auto *right = new QWidget(this);
  right->setObjectName("Right");
  right->setStyleSheet(
      QStringLiteral("QWidget#Right{background:%1; border-left:1px solid %2;}")
          .arg(theme::hex(theme::Wash), theme::hex(theme::Line)));
  auto *rv = new QVBoxLayout(right);
  rv->setContentsMargins(20, 16, 20, 16);
  rv->setSpacing(8);
  auto section = [&](const QString &t) {
    auto *l = new QLabel(t, right);
    l->setStyleSheet(
        QStringLiteral("color:%1; font-size:11px; border:0; border-bottom:1px "
                       "solid %2; padding-bottom:4px;")
            .arg(theme::hex(theme::Muted), theme::hex(theme::Line)));
    rv->addWidget(l);
  };
  section(QStringLiteral("도면 · 축척"));
  auto *form = new QFormLayout;
  form->setLabelAlignment(Qt::AlignLeft);
  form->setHorizontalSpacing(24);
  name_ = new QLineEdit(QStringLiteral("도면 %1. %2")
                            .arg(slot + 1)
                            .arg(QString(doc.name()).replace('_', ' ')),
                        right);
  form->addRow(QStringLiteral("유물 도면명"), name_);
  paper_ = new QComboBox(right);
  paper_->addItems({QStringLiteral("A4 세로 (210 × 297 mm)"),
                    QStringLiteral("A4 가로 (297 × 210 mm)"),
                    QStringLiteral("A3 세로 (297 × 420 mm)")});
  paper_->setFont(theme::monoFont(12));
  form->addRow(QStringLiteral("용지 규격"), paper_);
  form->addRow(
      QStringLiteral("실측 축척"),
      chipBar(
          {QStringLiteral("2:1"), QStringLiteral("1:1 (실제크기)"),
           QStringLiteral("1:2"), QStringLiteral("1:3"), QStringLiteral("1:4")},
          1,
          [this](int i) {
            static const double s[] = {2.0, 1.0, 0.5, 1.0 / 3.0, 0.25};
            scale_ = s[i];
            sync();
          },
          &scaleChips_));
  rv->addLayout(form);
  rv->addSpacing(6);
  section(QStringLiteral("포함할 레이어 (위에서 아래로 그리는 순서)"));
  cSec_ = new QCheckBox(
      QStringLiteral("단면선 Layer (맨 위 · 순수 빨강 #FF0000 · 0.35 mm)"),
      right);
  cOut_ = new QCheckBox(
      QStringLiteral(
          "외곽선·기준선 Layer (먹색 · 외곽선 0.50 · 기준선 0.10~0.25 mm)"),
      right);
  cShade_ = new QCheckBox(
      QStringLiteral("유물 음영·디지털 탁본 (외면 쪽 300 DPI 이미지)"), right);
  cGrid_ = new QCheckBox(
      QStringLiteral("방안 눈금 (1mm/10mm) · 치수 대장 · 축척 막대"), right);
  c14Layers_ = new QCheckBox(
      QStringLiteral("14개 레이어 분리 SVG (Inkscape/Illustrator 호환 14층)"),
      right);
  cCalib100_ = new QCheckBox(
      QStringLiteral("100 mm 보정 사각형 출력 (1:1 검증용)"), right);
  for (auto *c : {cSec_, cOut_, cShade_, cGrid_, c14Layers_, cCalib100_}) {
    c->setChecked(true);
    c->setStyleSheet(QStringLiteral("border:0;"));
    rv->addWidget(c);
  }
  cShade_->setChecked(base_.shade);
  c14Layers_->setChecked(fmt_ == ExportFormat::Svg);
  cCalib100_->setChecked(true);

  auto *verifyRow = new QHBoxLayout();
  lblVerified_ =
      new QLabel(QStringLiteral("Illustrator 1:1 검증: 미검증"), right);
  lblVerified_->setStyleSheet(QStringLiteral("color:%1; font-size:11px;")
                                  .arg(theme::hex(theme::Caution)));
  auto *btnVerify = new QPushButton(QStringLiteral("검증됨으로 기록"), right);
  btnVerify->setFixedHeight(24);
  btnVerify->setStyleSheet(QStringLiteral("font-size:11px; padding:2px 8px;"));
  connect(btnVerify, &QPushButton::clicked, this, [this] {
    lblVerified_->setText(QStringLiteral("✓ Illustrator 1:1 검증됨"));
    lblVerified_->setStyleSheet(
        QStringLiteral("color:%1; font-size:11px; font-weight:bold;")
            .arg(theme::hex(theme::Done)));
  });
  verifyRow->addWidget(lblVerified_);
  verifyRow->addWidget(btnVerify);
  rv->addLayout(verifyRow);
  rv->addSpacing(6);
  section(QStringLiteral("파일 형식"));
  rv->addWidget(chipBar(
      {QStringLiteral("SVG (일러스트 1:1)"),
       QStringLiteral("PDF (보고서 도판)"), QStringLiteral("DXF (CAD)"),
       QStringLiteral("PNG")},
      int(fmt_),
      [this](int i) {
        fmt_ = ExportFormat(i);
        sync();
      },
      &fmtChips_));
  note_ = new QLabel(right);
  note_->setWordWrap(true);
  note_->setStyleSheet(QStringLiteral("color:%1; font-size:12px; border:0;")
                           .arg(theme::hex(theme::Muted)));
  rv->addWidget(note_);
  rv->addStretch();
  body->addWidget(right, 9);
  root->addLayout(body, 1);

  // 바닥 단추줄
  auto *foot = new QWidget(this);
  foot->setObjectName("Foot");
  foot->setFixedHeight(56);
  foot->setStyleSheet(
      QStringLiteral("QWidget#Foot{background:%1; border-top:1px solid %2;}")
          .arg(theme::hex(theme::Surface), theme::hex(theme::Line)));
  auto *fh = new QHBoxLayout(foot);
  fh->setContentsMargins(20, 0, 20, 0);
  auto *def = new QPushButton(QStringLiteral("이 설정을 기본으로"), foot);
  def->setProperty("link", true);
  def->setCursor(Qt::PointingHandCursor);
  fh->addWidget(def);
  fh->addStretch();
  saveBtn_ = new QPushButton(foot);
  saveBtn_->setProperty("primary", true); // R3: 대화상자의 유일한 Action
  saveBtn_->setCursor(Qt::PointingHandCursor);
  auto *prn = new QPushButton(QStringLiteral("인쇄…"), foot);
  auto *cancel = new QPushButton(QStringLiteral("취소"), foot);
  for (auto *b : {prn, cancel})
    b->setProperty("secondary", true);
  fh->addWidget(saveBtn_);
  fh->addWidget(prn);
  fh->addWidget(cancel);
  root->addWidget(foot);
  connect(saveBtn_, &QPushButton::clicked, this, &ExportDialog::save);
  connect(prn, &QPushButton::clicked, this, &ExportDialog::print);
  connect(cancel, &QPushButton::clicked, this, &QDialog::reject);
  connect(paper_, &QComboBox::currentIndexChanged, this, &ExportDialog::sync);
  connect(name_, &QLineEdit::textChanged, this, &ExportDialog::sync);
  for (auto *c : {cSec_, cOut_, cShade_, cGrid_, c14Layers_, cCalib100_}) {
    if (c)
      connect(c, &QCheckBox::toggled, this, &ExportDialog::sync);
  }
  connect(def, &QPushButton::clicked, this, [this] {
    QSettings s;
    s.setValue("export/paper", paper_->currentIndex());
    s.setValue("export/scale", scale_);
    s.setValue("export/format", int(fmt_));
  });

  QSettings s;
  paper_->setCurrentIndex(s.value("export/paper", 0).toInt());
  scale_ = s.value("export/scale", 1.0).toDouble();
  const double ss[] = {2.0, 1.0, 0.5, 1.0 / 3.0, 0.25};
  for (int i = 0; i < 5; ++i)
    if (std::fabs(ss[i] - scale_) < 1e-6) {
      for (int k = 0; k < scaleChips_.size(); ++k)
        scaleChips_[k]->setProperty("state", k == i ? "on" : "idle");
    }
  sync();
}

QWidget *ExportDialog::chipBar(const QStringList &items, int sel,
                               std::function<void(int)> on,
                               QList<QPushButton *> *out) {
  auto *w = new QWidget(this);
  w->setStyleSheet(QStringLiteral("border:0;"));
  auto *h = new QHBoxLayout(w);
  h->setContentsMargins(0, 0, 0, 0);
  h->setSpacing(0);
  for (int i = 0; i < items.size(); ++i) {
    auto *b = new QPushButton(items[i], w);
    b->setProperty("chip", true);
    b->setProperty("first", i == 0);
    b->setProperty("last", i == items.size() - 1);
    b->setProperty("state", i == sel ? "on" : "idle");
    b->setStyleSheet(QString());
    b->setCursor(Qt::PointingHandCursor);
    h->addWidget(b);
    out->push_back(b);
    connect(b, &QPushButton::clicked, this, [out, i, on] {
      for (int k = 0; k < out->size(); ++k) {
        (*out)[k]->setProperty("state", k == i ? "on" : "idle");
        (*out)[k]->style()->unpolish((*out)[k]);
        (*out)[k]->style()->polish((*out)[k]);
      }
      on(i);
    });
  }
  h->addStretch();
  return w;
}

ExportRequest ExportDialog::request() const {
  ExportRequest r;
  r.title = name_->text();
  r.paper = Paper(paper_->currentIndex());
  r.scale = scale_;
  r.opts = base_;
  r.opts.slot = slot_;
  r.opts.section = cSec_->isChecked();
  r.opts.outline = cOut_->isChecked();
  r.opts.shade = cShade_->isChecked();
  r.opts.grid = cGrid_->isChecked();
  r.opts.labels = cGrid_->isChecked();
  r.opts.spec14Layers = c14Layers_ ? c14Layers_->isChecked() : false;
  r.opts.calibration100mm = cCalib100_ ? cCalib100_->isChecked() : false;
  return r;
}

void ExportDialog::sync() {
  if (!saveBtn_ || !note_ || !previewHead_ || !advice_)
    return;
  const ExportRequest r = request();
  const PaperLayout L = layoutPaper(doc_, slot_, r.paper, r.scale);
  previewHead_->setText(
      QStringLiteral("미리보기 · <b>%1 (보고서 도판 규격) · 축척 %2%3</b>")
          .arg(paperName(r.paper), scaleName(r.scale),
               r.scale == 1.0 ? QStringLiteral(" (실제 크기 mm)") : QString()));
  preview_->set(&doc_, r);
  advice_->setText(
      L.advice +
      (L.fits ? QStringLiteral("<br>팁: 대형 항아리(300 mm 초과)는 1:2 "
                               "축척이나 A3 용지를 자동으로 추천합니다.")
              : QString()));
  advice_->setStyleSheet(
      QStringLiteral("border:1px solid %1; border-radius:6px; padding:8px "
                     "12px; color:%2; background:%3;")
          .arg(theme::hex(L.fits ? theme::Edge : theme::Caution),
               theme::hex(L.fits ? theme::Ink : theme::Caution),
               theme::hex(theme::Surface)));
  static const char *labels[] = {"SVG / 도면 저장", "PDF / 도면 저장",
                                 "DXF / 도면 저장", "PNG / 도면 저장"};
  saveBtn_->setText(QString::fromUtf8(labels[int(fmt_)]));
  static const char *notes[] = {
      "• SVG 선택 시: Adobe Illustrator에서 열었을 때 "
      "단면선·외곽선·기준선(중심선·구연선·바닥선)·눈금이 각각 독립된 레이어로 "
      "분리되며 1:1 mm 치수가 정확히 유지됩니다.",
      "• PDF 선택 시: 보고서 도판 그대로 용지 크기·축척이 고정된 벡터 PDF로 "
      "저장합니다.",
      "• DXF 선택 시: R12 · mm 단위 · K-Arch 11 레이어(01_AXIS_CENTER … "
      "11_SCALE_BAR — 기준선: 중심선 01 · 구연선 07 · 바닥선 08)로 CAD에서 "
      "1:1로 엽니다. 음영은 포함되지 않습니다.",
      "• PNG 선택 시: 300 DPI 래스터 도판(용지 크기)으로 저장합니다.",
  };
  note_->setText(QString::fromUtf8(notes[int(fmt_)]));
}

void ExportDialog::save() {
  static const char *ext[] = {"svg", "pdf", "dxf", "png"};
  static const char *filt[] = {"SVG 일러스트 (*.svg)", "PDF 도판 (*.pdf)",
                               "DXF R12 (*.dxf)", "PNG 300 DPI (*.png)"};
  QSettings s;
  const QString dir =
      s.value("export/dir", QStandardPaths::writableLocation(
                                QStandardPaths::DocumentsLocation))
          .toString();
  QString base = name_->text();
  base.replace(QRegularExpression(QStringLiteral(R"([\\/:*?"<>|])")), "_");
  const QString path = QFileDialog::getSaveFileName(
      this, saveBtn_->text(), QDir(dir).filePath(base + "." + ext[int(fmt_)]),
      QString::fromUtf8(filt[int(fmt_)]));
  if (path.isEmpty())
    return;
  s.setValue("export/dir", QFileInfo(path).absolutePath());
  ExportRequest r = request();
  r.path = path;
  QString err;
  bool ok = false;
  switch (fmt_) {
  case ExportFormat::Svg:
    ok = exportSvgLayered(doc_, r, &err);
    break;
  case ExportFormat::Pdf:
    ok = exportPdf(doc_, r, &err);
    break;
  case ExportFormat::Dxf:
    ok = exportDxf(doc_, r, &err);
    break;
  case ExportFormat::Png:
    ok = exportPng(doc_, r, 300, &err);
    break;
  }
  if (!ok) {
    QMessageBox::warning(this, QStringLiteral("내보내기 실패"), err);
    return;
  }
  saved_ = path;
  accept();
}

void ExportDialog::print() {
  // 인쇄: 용지 그대로의 PDF 를 만들어 기본 뷰어(인쇄 대화상자)로 연다
  ExportRequest r = request();
  r.path = QDir(QStandardPaths::writableLocation(QStandardPaths::TempLocation))
               .filePath("DongArch3D_print.pdf");
  QString err;
  if (exportPdf(doc_, r, &err))
    QDesktopServices::openUrl(QUrl::fromLocalFile(r.path));
  else
    QMessageBox::warning(this, QStringLiteral("인쇄 준비 실패"), err);
}

} // namespace studio
