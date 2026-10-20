#include "MainWindow.h"

#include "CutPageText.h"
#include "ExportDialog.h"
#include "Ribbon.h"
#include "SheetView.h"
#include "StartPage.h"
#include "Theme.h"
#include "View3D.h"

#include <QApplication>
#include <QCloseEvent>
#include <QDateTime>
#include <QDir>
#include <QDragEnterEvent>
#include <QElapsedTimer>
#include <QFileDialog>
#include <QFileInfo>
#include <QFileSystemWatcher>
#include <QHBoxLayout>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QMenu>
#include <QMessageBox>
#include <QMimeData>
#include <QPushButton>
#include <QSettings>
#include <QStackedWidget>
#include <QTimer>
#include <QVBoxLayout>
#include <algorithm>
#include <cmath>

namespace studio {

namespace {
QLabel *kbdLabel(const QString &t, QWidget *p) {
  auto *l = new QLabel(t, p);
  l->setProperty("kbd", true);
  return l;
}
QLabel *textLabel(const QString &t, QWidget *p, const QColor &c = theme::Ink) {
  auto *l = new QLabel(t, p);
  l->setStyleSheet(
      QStringLiteral("color:%1; font-size:12px;").arg(theme::hex(c)));
  return l;
}
QFrame *vsep(QWidget *p) {
  auto *f = new QFrame(p);
  f->setFixedSize(1, 18);
  f->setStyleSheet(
      QStringLiteral("background:%1;").arg(theme::hex(theme::Line)));
  return f;
}
const char *kShadeNames[] = {"음영: 실사", "음영: 깊이 음영",
                             "음영: 디지털 탁본 (D-Tak)"};
// 리본 「단면 위치」 단추 글자 (D5 기본 / R 반전)
QString sideText(SectionSide s) {
  return s == SectionSide::Right
             ? QStringLiteral("외면 왼쪽·단면 오른쪽")
             : QStringLiteral("단면 왼쪽·외면 오른쪽 (반전)");
}
// ② 막 각도 키: , [ ← = −1, . ] → = +1, 그 밖 0
int cutKeySign(int key) {
  switch (key) {
  case Qt::Key_Comma:
  case Qt::Key_BracketLeft:
  case Qt::Key_Left:
    return -1;
  case Qt::Key_Period:
  case Qt::Key_BracketRight:
  case Qt::Key_Right:
    return 1;
  default:
    return 0;
  }
}
bool isModifierKey(int key) {
  return key == Qt::Key_Shift || key == Qt::Key_Control || key == Qt::Key_Alt ||
         key == Qt::Key_Meta;
}
} // namespace

MainWindow::MainWindow() {
  setWindowTitle(QStringLiteral("동국유물실측 (DongArch3D) 1.0"));
  resize(1560, 920);
  setAcceptDrops(
      true); // 작업 화면 어디에나 Artec 파일을 끌어다 놓으면 즉시 열림
  pages_ = new QStackedWidget(this);
  setCentralWidget(pages_);

  start_ = new StartPage(pages_);
  pages_->addWidget(start_);
  connect(start_, &StartPage::openFileRequested, this,
          &MainWindow::openFileDialog);
  connect(start_, &StartPage::openPathRequested, this,
          [this](const QString &p) { openPath(p); });
  connect(start_, &StartPage::sampleRequested, this, [this] { openSample(); });
  connect(start_, &StartPage::artecBlindSampleRequested, this,
          [this] { openArtecBlindSample(); });
  connect(start_, &StartPage::watchFolderRequested, this,
          &MainWindow::toggleWatchFolder);

  watcher_ = new QFileSystemWatcher(this);
  watchTimer_ = new QTimer(this);
  watchTimer_->setSingleShot(true);
  connect(watcher_, &QFileSystemWatcher::directoryChanged, this,
          [this](const QString &) {
            pendingWatchSize_ = -1;
            watchTimer_->start(450);
          });
  connect(watchTimer_, &QTimer::timeout, this, &MainWindow::checkWatchFolder);

  auto *work = new QWidget(pages_);
  auto *v = new QVBoxLayout(work);
  v->setContentsMargins(0, 0, 0, 0);
  v->setSpacing(0);
  ribbon_ = new Ribbon(work);
  v->addWidget(ribbon_->tabsBar());
  v->addWidget(ribbon_);
  ctx_ = buildCtxBar();
  v->addWidget(ctx_);
  auto *ws = new QWidget(work);
  auto *wh = new QHBoxLayout(ws);
  wh->setContentsMargins(0, 0, 0, 0);
  wh->setSpacing(0);
  slotPanel_ = buildSlotPanel();
  view3d_ = new View3DPane(ws);
  view3d_->setMinimumWidth(240);
  sheet_ = new SheetPane(ws);
  wh->addWidget(slotPanel_);
  wh->addWidget(view3d_, 27);
  wh->addWidget(sheet_, 73);
  v->addWidget(ws, 1);
  v->addWidget(buildStatusBar());
  pages_->addWidget(work);

  buildRibbon();
  sheet_->canvas()->setDoc(&doc_);
  connect(sheet_, &SheetPane::maximizeToggled, this,
          &MainWindow::toggleMaximize);
  connect(view3d_, &View3DPane::collapseRequested, this,
          &MainWindow::toggleMaximize);
  cutTimer_ = new QTimer(this);
  cutTimer_->setSingleShot(true);
  cutTimer_->setTimerType(Qt::PreciseTimer);
  connect(cutTimer_, &QTimer::timeout, this, &MainWindow::onCutTimer);
  cutClock_.start();
  connect(view3d_->canvas(), &View3DCanvas::cutAngleDragged, this,
          [this](double d) {
            if (!doc_.isLoaded() || !doc_.poseConfirmed())
              return;
            beginCutInput();
            cut_.drag(doc_.frontAngle(), d, cutClock_.elapsed());
            scheduleCut();
          });
  connect(view3d_->canvas(), &View3DCanvas::cutDragReleased, this, [this] {
    cut_.release(cutClock_.elapsed());
    onCutTimer();
  });
  connect(view3d_, &View3DPane::actionRequested, this, &MainWindow::onAction);
  connect(view3d_, &View3DPane::poseRotateRequested, this,
          [this](PoseAxis ax, double deg) {
            if (!doc_.isLoaded())
              return;
            doc_.rotatePose(ax, deg);
            slot_ = 0;
            refreshAll(true);
          });
  connect(view3d_->canvas(), &View3DCanvas::surfacePointPicked, this,
          [this](KA::Point3 pt) {
            if (!doc_.isLoaded() || doc_.poseConfirmed())
              return;
            if (view3d_->canvas()->posePickMode() ==
                PosePickMode::Surface1Click) {
              doc_.alignSurfacePointToFloor(pt);
              view3d_->canvas()->setPosePickMode(PosePickMode::None);
              slot_ = 0;
              refreshAll(true);
              setHint(QStringLiteral(
                  "클릭한 면을 바닥판(Z=0)에 수평으로 맞춰 세웠습니다."));
            } else if (view3d_->canvas()->posePickMode() ==
                       PosePickMode::Floor3Point) {
              const int n = doc_.addFloorPickPoint(pt);
              slot_ = 0;
              if (n >= 3) {
                view3d_->canvas()->setPosePickMode(PosePickMode::None);
                refreshAll(true);
                setHint(QStringLiteral(
                    "바닥 둘레 3점으로 수평 바닥면을 맞춰 세웠습니다."));
              } else {
                refreshAll(true);
                setHint(QStringLiteral(
                            "바닥 점 %1/3 찍음 — 다음 바닥 둘레 점을 클릭하세요.")
                            .arg(n));
              }
            }
          });
  connect(view3d_, &View3DPane::angleSelected, this, [this](double absDeg) {
    if (!doc_.isLoaded() || !doc_.poseConfirmed())
      return;
    flushCut();
    pushUndo();
    fixedAngle_.reset();
    const auto &sl = doc_.slotList();
    if (slot_ >= 0 && slot_ < int(sl.size()) &&
        sl[size_t(slot_)].kind == SlotKind::Horizontal &&
        firstVerticalSlot() >= 0) {
      slot_ = firstVerticalSlot();
    }
    doc_.setFrontAngle(doc_.snapAngle(absDeg, 2.0));
    refreshAll(true);
  });
  connect(view3d_, &View3DPane::cutPointPicked, this,
          [this](KA::Point3 worldPt) {
            if (!doc_.isLoaded() || !doc_.poseConfirmed())
              return;
            flushCut();
            pushUndo();
            fixedAngle_.reset();
            if (firstVerticalSlot() >= 0)
              slot_ = firstVerticalSlot();
            if (doc_.alignCutToSurfacePoint(worldPt)) {
              refreshAll(true);
              setHint(QStringLiteral(
                          "✂ 클릭한 지점(H %1 mm, %2°)으로 단면을 절단하여 도면에 표시했습니다.")
                          .arg(doc_.cutMarkerZ(), 0, 'f', 1)
                          .arg(doc_.frontAngle(), 0, 'f', 1));
            }
          });
  connect(view3d_, &View3DPane::rubbingSpotPicked, this,
          [this](KA::Point3 worldPt) {
            if (!doc_.isLoaded() || !doc_.poseConfirmed())
              return;
            if (doc_.selectRubbingSpot(worldPt)) {
              opts_.shadeMode = ShadeMode::DTak;
              ribbon_->setText(
                  "shadeMode",
                  QString::fromUtf8(kShadeNames[int(opts_.shadeMode)]));
              ribbon_->setState("shade", ToolState::On);
              applyOptions();
              refreshAll(true);
              setHint(QStringLiteral(
                          "◈ 선택한 문양 영역(H %1 mm, 방위 %2°)의 정밀 먹탁을 우측 도면에 출력했습니다.")
                          .arg(doc_.rubbingZ(), 0, 'f', 1)
                          .arg(doc_.rubbingAzimuthDeg(), 0, 'f', 0));
            }
          });
  connect(view3d_, &View3DPane::horizontalHeightSelected, this,
          [this](double zMm) {
            if (!doc_.isLoaded() || !doc_.poseConfirmed())
              return;
            auto &sl = doc_.slotList();
            if (slot_ < 0 || slot_ >= int(sl.size()) ||
                sl[size_t(slot_)].kind != SlotKind::Horizontal) {
              int hSlot = -1;
              for (size_t i = 0; i < sl.size(); ++i) {
                if (sl[i].kind == SlotKind::Horizontal) {
                  hSlot = int(i);
                  break;
                }
              }
              if (hSlot < 0)
                hSlot = doc_.addHorizontalSlotAt(zMm);
              slot_ = hSlot;
            }
            doc_.setSlotHorizontalZ(slot_, zMm);
            refreshSlots();
            sheet_->refresh(doc_, slot_);
            view3d_->refresh(doc_, slot_);
            setHint(QStringLiteral("횡단면 절단 높이: H %1 mm")
                        .arg(doc_.slotList()[size_t(slot_)].z, 0, 'f', 1));
          });
  connect(
      sheet_->canvas(), &SheetCanvas::cursorMm, this,
      [this](double r, double h, double wall, bool inside) {
        if (!inside || !doc_.poseConfirmed()) { // 확정 전에는 도판 좌표가 없다
          stCursor_->clear();
          return;
        }
        lastHoverH_ = h;
        const QString fa = theme::hex(theme::Faint);
        stCursor_->setText(
            QStringLiteral("반경 <b>R %1</b><span style='color:%4'> "
                           "mm</span>&nbsp;&nbsp;높이 <b>H %2</b><span "
                           "style='color:%4'> mm</span>&nbsp;&nbsp;두께 "
                           "<b>%3</b><span style='color:%4'> mm</span>")
                .arg(r, 0, 'f', 1)
                .arg(doc_.heightRef() == HeightRef::Rim ? h - doc_.dims().height
                                                        : h,
                     0, 'f', 1)
                .arg(wall > 0 ? QString::number(wall, 'f', 1)
                              : QStringLiteral("—"),
                     fa));
      });
  const QString savedWatch = QSettings().value("artec/watchDir").toString();
  if (!savedWatch.isEmpty() && QDir(savedWatch).exists())
    applyWatchFolder(savedWatch);
  qApp->installEventFilter(this);
  showHome();
}

// ---- 리본 구성 (목업 ① 그대로)
// ------------------------------------------------
void MainWindow::buildRibbon() {
  auto fileGroup = [this](int pg) {
    auto *g = ribbon_->addGroup(pg, QStringLiteral("파일"));
    g->tile("open", QStringLiteral("열기"), "open",
            QStringLiteral(
                "Artec 3D 유물 열기 — PLY · OBJ(+JPG) · WRL · STL (Ctrl+O)"));
    g->tile("recent", QStringLiteral("최근 ▾"), "recent",
            QStringLiteral("최근 실측 유물 및 Artec 감시 폴더 설정"));
  };
  auto alignGroup = [this](int pg) {
    auto *g = ribbon_->addGroup(pg, QStringLiteral("유물 정위치 · 중심축"));
    g->tile("axis", QStringLiteral("중심축 맞춤"), "axis",
            QStringLiteral(
                "구연부 PCA + 회전 중심축 자동 정렬 · 부유 노이즈 제거"));
    g->column({{"refBase", {QStringLiteral("바닥 수평 맞춤"), "B"}},
               {"refRim", {QStringLiteral("구연부 수평 맞춤"), "T"}}});
  };
  auto lineGroup = [this](int pg) {
    auto *g = ribbon_->addGroup(pg, QStringLiteral("실측선 추출"));
    g->tile(
        "cut", QStringLiteral("단면선"), "cut",
        QStringLiteral("② 막으로 자르기 (S) — 막(절단면 A–A′)을 돌려 단면을 "
                       "봅니다 · Artec 미스캔 내벽 파선 자동 보정(W)"));
    g->tile("outline", QStringLiteral("외곽선"), "outline",
            QStringLiteral("외곽선 도구 (O)"));
    g->column({{"combined", {sideText(doc_.sectionSide()), "R"}},
               {"addSlot", {QStringLiteral("단면 추가 (횡단면)"), "N"}}});
  };
  auto simplifyGroup = [this](int pg) {
    auto *g = ribbon_->addGroup(pg, QStringLiteral("선 다듬기 · 간략화"));
    g->chipRows(
        {{"simplify",
          QStringLiteral("간격"),
          {"0.1", "0.3", "0.5 mm", "1.0"},
          2},
         {"curve",
          QStringLiteral("곡선"),
          {QStringLiteral("원본"), QStringLiteral("매끄럽게 (Catmull)")},
          1}});
  };
  auto showGroup = [this](int pg) {
    auto *g = ribbon_->addGroup(pg, QStringLiteral("도판 표시 (겹침)"));
    g->tile("shade", QStringLiteral("유물 음영"), "shade",
            QStringLiteral("유물 음영 · Artec 컬러 / 디지털 탁본 (I)"));
    g->tile("lines", QStringLiteral("단면·외곽"), "profile",
            QStringLiteral("단면·외곽선 표시 (L)"));
    g->tile("grid", QStringLiteral("방안 눈금"), "grid",
            QStringLiteral("방안 눈금 (V)"));
    g->column({{"shadeMode", {QString::fromUtf8(kShadeNames[2]), ""}},
               {"gridInfo", {QStringLiteral("눈금 1 mm / 숫자 50 mm"), ""}}});
  };
  auto exportGroup = [this](int pg) {
    auto *g = ribbon_->addGroup(pg, QStringLiteral("내보내기"));
    g->tile("export", QStringLiteral("도면"), "doc",
            QStringLiteral("보고서 도면 (Ctrl+P)"));
    g->tile("svg", QStringLiteral("SVG 일러스트"), "pen",
            QStringLiteral("레이어 분리 SVG (1:1 mm)"));
    g->tile("dxf", QStringLiteral("DXF"), "dxf", QStringLiteral("CAD DXF R12"));
  };
  const int pFile = ribbon_->addPage(QStringLiteral("파일"));
  pHome_ = ribbon_->addPage(QStringLiteral("홈"));
  pBase_ = ribbon_->addPage(QStringLiteral("① 바닥 정하기"));
  pCut_ = ribbon_->addPage(cutpage::kTabName);
  pEdit_ = ribbon_->addPage(cutpage::kEditTab);
  const int pExport = ribbon_->addPage(QStringLiteral("내보내기"));
  {
    auto *g = ribbon_->addGroup(pFile, QStringLiteral("파일 · Artec 연동"));
    g->tile("open", QStringLiteral("열기"), "open",
            QStringLiteral("유물 열기 (Ctrl+O)"));
    g->tile("watch", QStringLiteral("Artec 연동"), "recent",
            QStringLiteral("Artec Studio 내보내기 폴더 실시간 자동 감지"));
    g->tile("sample", QStringLiteral("예제 유물"), "lithic",
            QStringLiteral("내장 합성 토기 (Artec 컬러)"));
    g->tile("blindSample", QStringLiteral("미스캔 예제"), "cut",
            QStringLiteral("좁은 아가리 토기 내벽 미스캔 파선 자동보정 예제"));
    g->tile("home", QStringLiteral("시작 화면"), "doc");
  }
  fileGroup(pHome_);
  alignGroup(pHome_);
  lineGroup(pHome_);
  simplifyGroup(pHome_);
  showGroup(pHome_);
  exportGroup(pHome_);
  // ① 바닥 정하기: 파일 · 바닥판에 올려놓기 · 확정 (디자인 v2, 손 실측: 실측판
  // 위에 올려놓기)
  fileGroup(pBase_);
  {
    auto *g = ribbon_->addGroup(pBase_, QStringLiteral("바닥판에 올려놓기"));
    g->tile("poseAuto", QStringLiteral("자동 세우기"), "axis",
            QStringLiteral(
                "자동 세우기 — 구연부를 찾아 회전 중심축을 수직으로 세우고 "
                "바닥판에 놓습니다 (입이 대략 위로 오게 돌린 뒤 누르세요)"));
    g->column({{"poseX", {QStringLiteral("X 90°"), ""}},
               {"poseY", {QStringLiteral("Y 90°"), ""}},
               {"poseZ", {QStringLiteral("Z 90°"), ""}}});
    g->column({{"poseFlip", {QStringLiteral("뒤집기"), ""}},
               {"poseReset", {QStringLiteral("처음 자세로"), ""}},
               {"poseDrop", {QStringLiteral("바닥에 내려놓기"), ""}}});
    g->column({{"poseModeTB", {QStringLiteral("T/B 토글"), ""}},
               {"sherdModeToggle", {QStringLiteral("편 모드"), ""}},
               {"poseCenter", {QStringLiteral("바닥 중앙 맞춤"), ""}}});
  }
  {
    auto *g = ribbon_->addGroup(pBase_, QStringLiteral("확정"));
    g->tile("poseConfirm", QStringLiteral("이 자세로 확정"), "check",
            QStringLiteral("이 자세로 확정 (Enter) → ② 막으로 자르기"));
  }
  // ② 막으로 자르기 (디자인 v2 phase=cut): 파일 · 막 방향 · 막 움직이기
  // (회전판) · 단면 위치
  fileGroup(pCut_);
  cutAxisGroup_ = ribbon_->addGroup(pCut_, cutpage::axisGroupName(0));
  cutAxisGroup_->tile(
      "axX", cutpage::kTileX, "axX",
      QStringLiteral(
          "X 막 옆에서 — 회전판을 90° 로 맞춰 옆에서 본 종단면(절단면 A–A′)"));
  cutAxisGroup_->tile("axY", cutpage::kTileY, "axY",
                      QStringLiteral("Y 막 정면에서 — 회전판을 0° 로 맞춰 "
                                     "정면에서 본 종단면(절단면 A–A′)"));
  cutAxisGroup_
      ->tile("axZ", cutpage::kTileZ, "axZ",
             QStringLiteral("🔒 Z 막 높이로 — 높이별 횡단면 막은 다음 판에서 "
                            "엽니다 · 지금은 홈 「단면 추가 (횡단면)」 (N)"))
      ->setEnabled(false);
  {
    auto *g = ribbon_->addGroup(pCut_, cutpage::kGrpMove);
    auto *pinned = textLabel(cutpage::kPinned, g);
    pinned->setToolTip(
        QStringLiteral("종단면은 중심선(회전 중심축)에 고정해 돌리기만 합니다 "
                       "— 기고·구경은 중심축 기준 · 풀기(막 밀기)는 다음 판"));
    auto *minus = g->button("cutMinus", QStringLiteral("−"));
    minus->setToolTip(QStringLiteral(
        "회전판(정면 방위각) −1° — 키 ← (Shift 15°) · , [ 도 같음"));
    auto *plus = g->button("cutPlus", QStringLiteral("+"));
    plus->setToolTip(
        QStringLiteral("회전판(정면 방위각) +1° — 위에서 보아 반시계(가까운 "
                       "면이 오른쪽으로) · 키 → (Shift 15°) · . ] 도 같음"));
    cutAngle_ = new QLabel(g);
    cutAngle_->setObjectName("CutAngle");
    cutAngle_->setStyleSheet(
        QStringLiteral("font-family: Consolas; font-size: 12px;"));
    cutAngle_->setAlignment(Qt::AlignCenter);
    auto *rec1 = g->button("cutRec1", QStringLiteral("★ 추천 1"));
    rec1->setToolTip(QStringLiteral("결함 점수 최적 추천 1 각도"));
    auto *rec2 = g->button("cutRec2", QStringLiteral("추천 2"));
    auto *rec3 = g->button("cutRec3", QStringLiteral("추천 3"));
    auto *sweep = g->button("cutSweep", QStringLiteral("훑기"));
    sweep->setToolTip(QStringLiteral("3.2초 360° 부드러운 자동 회전"));
    g->rows(
        {{pinned, rec1, rec2, rec3, sweep},
         {textLabel(cutpage::kAngle, g), minus, cutAngle_, plus,
          kbdLabel("← →", g), textLabel(cutpage::kStep1, g, theme::Muted),
          kbdLabel("Shift", g), textLabel(cutpage::kStep15, g, theme::Muted)},
         {textLabel(cutpage::kLive, g, theme::Muted)}});
    // 끄는 동안 글자 폭이 바뀌어도 리본이 옆으로 밀리지 않게: 각도 칸·막 방향
    // 이름 칸을 가장 긴 문구 폭으로
    cutAngle_->ensurePolished();
    cutAngle_->setFixedWidth(
        cutAngle_->fontMetrics().horizontalAdvance(QStringLiteral("−179.5°")) +
        6);
    QLabel *axisName = cutAxisGroup_->titleLabel();
    axisName->ensurePolished();
    axisName->setMinimumWidth(axisName->fontMetrics().horizontalAdvance(
        cutpage::axisGroupName(-179.5)));
  }
  {
    auto *g = ribbon_->addGroup(pCut_, cutpage::kGrpSide);
    g->rows({{g->button("combined", sideText(doc_.sectionSide()), "R")},
             {textLabel(cutpage::kStandard, g, theme::Done)},
             {textLabel(cutpage::kCheckUser, g, theme::Muted)}});
  }
  {
    auto *g = ribbon_->addGroup(pCut_, cutpage::kGrpFix);
    g->tile("fixAngle", cutpage::kTileFix, "lock",
            QStringLiteral("지금 각도 고정 (Enter) — 지금 막 각도로 종단면 "
                           "도면을 정합니다 · 막을 다시 움직이면 풀립니다"));
    g->tile("toEdit", cutpage::kTileEdit, "next",
            QStringLiteral(
                "③ 도면 편집으로 — 고정 전이면 지금 각도로 고정하고 넘어갑니다 "
                "· 중심선 = 3D 중심축 · 바닥선 = 바닥판 · 구연선 = 구연 평면"));
  }
  const QList<QPair<QString, QString>> poseTips = {
      {"poseX",
       QStringLiteral("X 축(바닥판 가로, 왼→오른쪽)을 축으로 +90° 돌립니다 "
                      "(오른손 법칙) · Shift 를 누른 채 누르면 −90°")},
      {"poseY",
       QStringLiteral("Y 축(바닥판 세로, 앞→뒤)을 축으로 +90° 돌립니다 (오른손 "
                      "법칙) · Shift 를 누른 채 누르면 −90°")},
      {"poseZ",
       QStringLiteral("Z 축(바닥판에 수직, 아래→위)을 축으로 +90° 돌립니다 "
                      "(위에서 보면 반시계) · Shift 를 누른 채 누르면 −90°")},
      {"poseFlip",
       QStringLiteral(
           "위아래 뒤집기 — X 축으로 180° (정면에서 보면 좌우는 그대로)")},
      {"poseReset",
       QStringLiteral("처음 자세로 — 스캔이 들어온 자세 그대로 되돌립니다")},
      {"poseDrop",
       QStringLiteral(
           "바닥에 내려놓기 — 가장 낮은 점이 바닥판(Z 0 mm)에 닿게 내립니다")}};
  for (const auto &[id, tip] : poseTips)
    for (auto *b : ribbon_->findChildren<QPushButton *>(id))
      b->setToolTip(tip);
  for (auto *b : ribbon_->findChildren<QPushButton *>("combined"))
    b->setToolTip(QStringLiteral("국내 보고서 표준 (D5) · R 로 좌/우 반전 — "
                                 "같은 단면을 거울상으로 그립니다"));
  // ③ 도면 편집 (디자인 v2 phase=edit): 파일 · 고정된 막 · 선 다듬기 · 도판
  // 표시 · 내보내기
  fileGroup(pEdit_);
  {
    auto *g = ribbon_->addGroup(pEdit_, cutpage::kGrpFixed);
    fixedLbl_ = textLabel(cutpage::fixText(std::nullopt), g);
    fixedLbl_->setObjectName("FixedAngle");
    fixedLbl_->ensurePolished(); // 각이 바뀌어도 리본이 옆으로 밀리지 않게 가장
                                 // 긴 글 폭으로
    fixedLbl_->setMinimumWidth(
        fixedLbl_->fontMetrics().horizontalAdvance(cutpage::fixText(-179.5)));
    auto *back = g->button("backToCut", cutpage::kBackToCut, "S");
    back->setToolTip(
        QStringLiteral("② 막으로 자르기로 돌아가 막을 다시 움직입니다 (S) — "
                       "막을 움직이면 고정이 풀립니다"));
    g->rows({{fixedLbl_}, {back}});
  }
  simplifyGroup(pEdit_);
  showGroup(pEdit_);
  exportGroup(pEdit_);
  exportGroup(pExport);
  ribbon_->setCurrentPage(pHome_);

  for (const char *id : {"export"})
    ribbon_->setState(id, ToolState::Primary); // R3: 화면당 하나
  ribbon_->setState("toEdit",
                    ToolState::Primary); // R3: ② 막으로 자르기의 주 단추
  ribbon_->setState(
      "poseConfirm",
      ToolState::Primary); // R3: ① 바닥 정하기 페이지의 주 단추 (확정 뒤 Idle)
  for (const char *id : {"shade", "lines", "grid", "combined", "refBase"})
    ribbon_->setState(id, ToolState::On);
  connect(ribbon_, &Ribbon::pageChanged, this, &MainWindow::onPage);
  connect(ribbon_, &Ribbon::triggered, this, &MainWindow::onAction);
  connect(ribbon_, &Ribbon::chipChosen, this, &MainWindow::onChip);
  connect(ribbon_, &Ribbon::undoRequested, this, [this] {
    flushCut();
    if (!requirePose() || undo_.isEmpty())
      return;
    redo_.push_back({doc_.frontAngle(), doc_.heightRef(), doc_.simplifyTol(),
                     doc_.smooth()});
    restore(undo_.takeLast());
  });
  connect(ribbon_, &Ribbon::redoRequested, this, [this] {
    flushCut();
    if (!requirePose() || redo_.isEmpty())
      return;
    undo_.push_back({doc_.frontAngle(), doc_.heightRef(), doc_.simplifyTol(),
                     doc_.smooth()});
    restore(redo_.takeLast());
  });
}

QWidget *MainWindow::buildCtxBar() {
  auto *w = new QWidget(this);
  w->setObjectName("CtxBar");
  w->setAttribute(Qt::WA_StyledBackground, true);
  w->setFixedHeight(36);
  auto *h = new QHBoxLayout(w);
  h->setContentsMargins(12, 0, 12, 0);
  h->setSpacing(10);
  ctxBadge_ = new QLabel(w);
  ctxBadge_->setObjectName("ToolBadge");
  ctxBadge_->setFixedHeight(24);
  ctxHint_ = new QLabel(w);
  ctxHint_->setObjectName("CtxHint");
  ctxHint_->setMinimumWidth(
      1); // 긴 안내가 창 최소 폭을 밀지 않게 (잘리면 툴팁으로 전체)
  ctxRight_ = new QLabel(w);
  ctxRight_->setObjectName("CtxRight");
  ctxRight_->setFont(theme::monoFont(12));
  h->addWidget(ctxBadge_);
  h->addWidget(ctxHint_);
  h->addWidget(vsep(w));
  // , . [ ] 는 ← → 의 별칭이라 배지를 따로 두지 않는다 (R8, ② [−][+] 툴팁에
  // 적음)
  ctxCutKeys_ = {
      kbdLabel("← →", w), new QLabel(QStringLiteral("막 1° · Shift 15°"), w),
      kbdLabel("W", w),   new QLabel(QStringLiteral("미스캔 내벽 파선보정"), w),
      kbdLabel("- +", w), new QLabel(QStringLiteral("추정 두께 ±0.5mm"), w),
      kbdLabel("1~5", w), new QLabel(QStringLiteral("슬롯 저장"), w),
      kbdLabel("R", w),   new QLabel(QStringLiteral("좌/우 반전"), w),
      kbdLabel("Esc", w), new QLabel(QStringLiteral("내려놓기"), w)};
  ctxDupKeys_ = {ctxCutKeys_[0], ctxCutKeys_[1], ctxCutKeys_[8],
                 ctxCutKeys_[9]}; // ← → · R (② 리본에 같은 배지)
  ctxBaseKeys_ = {kbdLabel("Enter", w),
                  new QLabel(QStringLiteral("확정"), w), // R8: ① 바닥 정하기
                  kbdLabel("Shift", w), new QLabel(QStringLiteral("−90°"), w)};
  ctxFixKeys_ = {kbdLabel("Enter", w),
                 new QLabel(cutpage::kTileFix, w)}; // ② 에서만 (확정 뒤)
  for (auto *x : ctxCutKeys_)
    h->addWidget(x);
  for (auto *x : ctxFixKeys_ + ctxBaseKeys_) {
    h->addWidget(x);
    x->hide();
  }
  h->addStretch();
  h->addWidget(ctxRight_);
  w->hide(); // R1: 도구를 손에 들었을 때만
  return w;
}

QWidget *MainWindow::buildSlotPanel() {
  auto *w = new QWidget(this);
  w->setObjectName("SlotPanel");
  w->setAttribute(Qt::WA_StyledBackground, true);
  w->setFixedWidth(176);
  auto *v = new QVBoxLayout(w);
  v->setContentsMargins(0, 0, 0, 0);
  v->setSpacing(0);
  auto *head = new QWidget(w);
  head->setObjectName("PaneHead");
  head->setAttribute(Qt::WA_StyledBackground, true);
  head->setFixedHeight(32);
  auto *h = new QHBoxLayout(head);
  h->setContentsMargins(10, 0, 8, 0);
  auto *t = new QLabel(QStringLiteral("실측 도면 목록"), head);
  t->setProperty("ph", true);
  slotCount_ = new QLabel(head);
  slotCount_->setFont(theme::monoFont(12));
  slotCount_->setStyleSheet(
      QStringLiteral("color:%1;").arg(theme::hex(theme::Muted)));
  auto *add = new QPushButton(QStringLiteral("+ 추가"), head);
  add->setProperty("sbtn", true);
  add->setFocusPolicy(Qt::NoFocus);
  add->setToolTip(QStringLiteral("커서 높이에 횡단면 슬롯 추가 (N)"));
  connect(add, &QPushButton::clicked, this, [this] { onAction("addSlot"); });
  h->addWidget(t);
  h->addWidget(slotCount_);
  h->addStretch();
  h->addWidget(add);
  v->addWidget(head);
  slotList_ = new QListWidget(w);
  slotList_->setObjectName("SlotList");
  slotList_->setDragDropMode(QAbstractItemView::InternalMove);
  slotList_->setFocusPolicy(Qt::NoFocus);
  slotList_->setVerticalScrollMode(QAbstractItemView::ScrollPerPixel);
  v->addWidget(slotList_, 1);
  auto *foot = new QLabel(
      QStringLiteral(
          "누르면 해당 도판으로 전환 · 끌어서 보고서 도판 순서 변경"),
      w);
  foot->setWordWrap(true);
  foot->setStyleSheet(
      QStringLiteral("color:%1; font-size:11px; padding:8px 10px; "
                     "border-top:1px solid %2;")
          .arg(theme::hex(theme::Muted), theme::hex(theme::Line)));
  v->addWidget(foot);
  connect(slotList_, &QListWidget::currentRowChanged, this, [this](int r) {
    if (r >= 0 && r != slot_)
      selectSlot(r);
  });
  connect(slotList_->model(), &QAbstractItemModel::rowsMoved, this, [this] {
    std::vector<SectionSlot> reordered;
    for (int i = 0; i < slotList_->count(); ++i) {
      const int src = slotList_->item(i)->data(Qt::UserRole).toInt();
      reordered.push_back(doc_.slotList()[size_t(src)]);
    }
    doc_.slotList() = std::move(reordered);
    refreshSlots();
  });
  return w;
}

QWidget *MainWindow::buildStatusBar() {
  auto *w = new QWidget(this);
  w->setObjectName("StatusBar");
  w->setAttribute(Qt::WA_StyledBackground, true);
  w->setFixedHeight(30);
  auto *h = new QHBoxLayout(w);
  h->setContentsMargins(12, 0, 12, 0);
  h->setSpacing(14);
  stHint_ = new QLabel(w);
  stHint_->setObjectName("StHint");
  stCursor_ = new QLabel(w);
  stCursor_->setFont(theme::monoFont(12));
  stScale_ = new QLabel(
      QStringLiteral(
          "축척 <span style='font-family:Consolas'>1:1 (실제크기)</span>"),
      w);
  stRef_ = new QLabel(w);
  stDirty_ = new QLabel(w);
  stFix_ = new QLabel(w);
  stFix_->setObjectName("StFix");
  stFix_->hide(); // ②·③ 확정 뒤에만
  h->addWidget(stHint_, 1);
  h->addWidget(stCursor_);
  h->addWidget(vsep(w));
  h->addWidget(stScale_);
  h->addWidget(vsep(w));
  h->addWidget(stRef_);
  h->addWidget(stDirty_);
  h->addWidget(stFix_);
  return w;
}

// ---- 열기 & Artec Studio 감시 폴더
// --------------------------------------------
void MainWindow::openFileDialog() {
  QSettings s;
  const QString p = QFileDialog::getOpenFileName(
      this, QStringLiteral("Artec 3D 유물 열기"),
      s.value("open/dir").toString(),
      QStringLiteral(
          "Artec / 3D 유물 (*.ply *.obj *.wrl *.stl);;모든 파일 (*.*)"));
  if (p.isEmpty())
    return;
  s.setValue("open/dir", QFileInfo(p).absolutePath());
  openPath(p);
}

void MainWindow::toggleWatchFolder() {
  if (!watchDir_.isEmpty()) {
    QMenu m(this);
    m.addAction(QStringLiteral("Artec 감시 폴더 변경…"), this, [this] {
      const QString d = QFileDialog::getExistingDirectory(
          this, QStringLiteral("Artec Studio 내보내기 폴더 선택"), watchDir_);
      if (!d.isEmpty())
        applyWatchFolder(d);
    });
    m.addAction(QStringLiteral("감시 중지 (%1)")
                    .arg(QDir::toNativeSeparators(watchDir_)),
                this, [this] { applyWatchFolder(QString()); });
    m.exec(QCursor::pos());
    return;
  }
  QSettings s;
  const QString initDir = s.value("open/dir").toString();
  const QString d = QFileDialog::getExistingDirectory(
      this,
      QStringLiteral("Artec Studio 내보내기(Export) 폴더 선택 — 파일 저장 시 "
                     "자동으로 열립니다"),
      initDir);
  if (!d.isEmpty())
    applyWatchFolder(d);
}

void MainWindow::applyWatchFolder(const QString &dir) {
  if (!watcher_->directories().isEmpty())
    watcher_->removePaths(watcher_->directories());
  watchDir_ = dir;
  QSettings().setValue("artec/watchDir", dir);
  if (!dir.isEmpty() && QDir(dir).exists()) {
    watcher_->addPath(dir);
    ribbon_->setState("watch", ToolState::On);
    start_->setWatchFolderStatus(dir, true);
    setHint(QStringLiteral("● Artec Studio 감시 폴더 연결됨: <span "
                           "style='font-family:Consolas'>%1</span> — Artec에서 "
                           "내보내면 자동으로 열립니다.")
                .arg(QDir::toNativeSeparators(dir)));
  } else {
    ribbon_->setState("watch", ToolState::Idle);
    start_->setWatchFolderStatus(QString(), false);
    setHint(QStringLiteral("Artec Studio 감시 폴더를 해제했습니다."));
  }
}

void MainWindow::checkWatchFolder() {
  if (watchDir_.isEmpty())
    return;
  QDir d(watchDir_);
  const QFileInfoList files = d.entryInfoList(
      {"*.ply", "*.obj", "*.wrl", "*.stl"}, QDir::Files, QDir::Time);
  if (files.isEmpty())
    return;
  const QFileInfo newest = files.first();
  if (newest.lastModified().secsTo(QDateTime::currentDateTime()) > 20)
    return;
  if (newest.absoluteFilePath() == doc_.path() && pendingWatchFile_.isEmpty())
    return;
  // 파일 쓰기가 진행 중이면 크기가 멈출 때까지 대기
  if (pendingWatchFile_ != newest.absoluteFilePath() ||
      pendingWatchSize_ != newest.size() || newest.size() == 0) {
    pendingWatchFile_ = newest.absoluteFilePath();
    pendingWatchSize_ = newest.size();
    watchTimer_->start(450);
    return;
  }
  const QString target = pendingWatchFile_;
  pendingWatchFile_.clear();
  pendingWatchSize_ = -1;
  if (openPath(target)) {
    setHint(QStringLiteral("✓ Artec Studio 연동: <b>%1</b> 자동 로드 — ① 바닥 "
                           "정하기에서 자세를 확정하세요")
                .arg(QFileInfo(target).fileName()));
  }
}

bool MainWindow::openPath(const QString &path) {
  cancelCut(); // 문서가 바뀌므로 막 각도는 계산하지 않는다
  QApplication::setOverrideCursor(Qt::WaitCursor);
  QString err;
  const bool ok = doc_.loadFile(path, &err); // 들어온 자세 그대로 (확정 전)
  QApplication::restoreOverrideCursor();
  if (!ok) {
    if (doc_.isPreviewing()) { // 열지 못해 문서가 그대로면 움직이던 막을 놓은
                               // 것으로
      doc_.commitPreview();
      refreshAll(true);
    }
    QMessageBox::warning(this, QStringLiteral("유물을 열 수 없습니다"), err);
    return false;
  }
  recordRecent();
  setWindowTitle(QStringLiteral("%1 — 동국유물실측 (DongArch3D) 1.0")
                     .arg(QFileInfo(path).fileName()));
  undo_.clear();
  redo_.clear();
  slot_ = 0;
  fixedAngle_.reset(); // 새 문서 = 고정 풀림
  pages_->setCurrentIndex(1);
  ribbon_->setCurrentPage(pBase_); // ① 바닥 정하기
  sheet_->canvas()->setDoc(&doc_);
  refreshAll(true);
  return true;
}

void MainWindow::recordRecent() {
  if (doc_.path().isEmpty())
    return;
  const bool done = doc_.poseConfirmed();
  const auto &d = doc_.dims();
  const QString state = !done
                            ? QStringLiteral("바닥 미정")
                            : (doc_.aligned() ? QStringLiteral("✓ 중심축 완료")
                                              : QStringLiteral("정위치 필요"));
  pushRecent({doc_.path(), doc_.name(),
              QStringLiteral("%1 · %2").arg(state, doc_.scannerProfile()),
              done ? QStringLiteral("기고 %1 · 구경 %2 mm")
                         .arg(d.height, 0, 'f', 1)
                         .arg(d.rim, 0, 'f', 1)
                   : QStringLiteral("치수 — (바닥 확정 전)"),
              QDateTime::currentDateTime().toString("MM-dd HH:mm"),
              int(doc_.slotList().size())});
}

void MainWindow::openSample(bool poseStage) {
  cancelCut();
  QApplication::setOverrideCursor(Qt::WaitCursor);
  doc_.loadSample(!poseStage);
  QApplication::restoreOverrideCursor();
  setWindowTitle(
      QStringLiteral("%1 — 동국유물실측 (DongArch3D) 1.0").arg(doc_.name()));
  undo_.clear();
  redo_.clear();
  slot_ = 0;
  fixedAngle_.reset(); // 새 문서 = 고정 풀림
  pages_->setCurrentIndex(1);
  ribbon_->setCurrentPage(poseStage ? pBase_ : pHome_);
  sheet_->canvas()->setDoc(&doc_);
  refreshAll(true);
}

void MainWindow::openArtecBlindSample(bool poseStage) {
  cancelCut();
  QApplication::setOverrideCursor(Qt::WaitCursor);
  doc_.loadArtecBlindSample(!poseStage);
  QApplication::restoreOverrideCursor();
  setWindowTitle(
      QStringLiteral("%1 — 동국유물실측 (DongArch3D) 1.0").arg(doc_.name()));
  undo_.clear();
  redo_.clear();
  slot_ = 0;
  fixedAngle_.reset(); // 새 문서 = 고정 풀림
  pages_->setCurrentIndex(1);
  ribbon_->setCurrentPage(poseStage ? pBase_ : pHome_);
  sheet_->canvas()->setDoc(&doc_);
  refreshAll(true);
  if (poseStage)
    return;
  setHint(
      QStringLiteral("Artec 내벽 미스캔 자동 보정: 구연부 실측 두께(%1 mm)를 "
                     "기준으로 미스캔 동체부·저부 내벽을 빨강 파선으로 "
                     "복원했습니다 (W: 켜기/끄기, -/+: 두께 조절).")
          .arg(doc_.estimatedWallMm(), 0, 'f', 1));
}

void MainWindow::showHome() {
  start_->reload();
  pages_->setCurrentIndex(0);
}

// ---- 상태 갱신 --------------------------------------------------------------
void MainWindow::refreshAll(bool geometryChanged) {
  // ③ 에서는 지금 각이 곧 고정 각 (들어올 때 자동 고정 · 되돌리기·슬롯 고르기로
  // 각이 바뀌면 그 각으로)
  if (ribbon_->currentPage() == pEdit_ && doc_.isLoaded() &&
      doc_.poseConfirmed() && !angleFixed())
    fixAngle();
  if (geometryChanged)
    dirtyExport_ = true;
  applyOptions();
  refreshSlots();
  sheet_->refresh(doc_, slot_);
  view3d_->refresh(doc_, slot_);
  if (geometryChanged)
    sheet_->canvas()->docChanged();
  ribbon_->setState("refBase", doc_.heightRef() == HeightRef::Base
                                   ? ToolState::On
                                   : ToolState::Idle);
  ribbon_->setState("refRim", doc_.heightRef() == HeightRef::Rim
                                  ? ToolState::On
                                  : ToolState::Idle);
  const bool base = doc_.isLoaded() && !doc_.poseConfirmed();
  ribbon_->setState("poseConfirm",
                    base
                        ? ToolState::Primary
                        : ToolState::Idle); // R3: 확정 전 주 단추 = poseConfirm
  ribbon_->setPageLocked(
      pCut_, base,
      cutpage::kLockTip); // 숨기지 않고 잠김 표시 (누르면 ① 로 안내)
  ribbon_->setPageLocked(pEdit_, base, cutpage::kLockTip);
  const bool onCut = ribbon_->currentPage() == pCut_,
             onEdit = ribbon_->currentPage() == pEdit_;
  for (auto *w : ctxCutKeys_) // ③ 에서는 막이 돌지 않는다 → ← → 묶음 숨김
    w->setVisible(!base && !(onCut && ctxDupKeys_.contains(w)) &&
                  !(onEdit && (w == ctxCutKeys_[0] || w == ctxCutKeys_[1])));
  for (auto *w : ctxBaseKeys_)
    w->setVisible(base);
  for (auto *w : ctxFixKeys_)
    w->setVisible(!base && onCut);
  stFix_->setVisible(!base && (onCut || onEdit));
  if (base) {
    // ① 바닥 정하기 = 「▣ 바닥 정하기」를 손에 든 상태 (R1: ctx 줄 보임)
    if (!hand_.isEmpty())
      setHandTool(QString());
    const bool grounded = doc_.isGrounded();
    stRef_->setText(grounded ? QStringLiteral("단위 mm · 놓음 (확정 전)")
                             : QStringLiteral("단위 mm · 바닥 미정"));
    stDirty_->clear();
    setHint(grounded ? QStringLiteral("④ 수평·기준선 — 3D 바닥판에서 자세를 "
                                      "보고 「이 자세로 확정」 (Enter)")
                     : QStringLiteral(
                           "③ 바닥 놓기 — 입이 위로 오게 X·Y·Z 90°·뒤집기로 "
                           "돌리거나 「자동 세우기」를 누르세요"));
    ctxBadge_->setText(QStringLiteral("▣ 바닥 정하기"));
    ctxHint_->setText(
        QStringLiteral("스캔은 위아래 없이 들어옵니다 · X·Y·Z 90° 단추나 "
                       "「자동 세우기」로 세우세요 · Enter 확정"));
    ctxHint_->setToolTip(ctxHint_->text());
    ctxRight_->setText(doc_.groundNote());
    ctx_->show();
    return;
  }
  if (hand_.isEmpty())
    ctx_->hide(); // 확정하면 바닥 정하기 ctx 를 내려놓는다
  stRef_->setText(QStringLiteral("단위 mm · 정위치 %1")
                      .arg(doc_.aligned()
                               ? (doc_.heightRef() == HeightRef::Base
                                      ? QStringLiteral("바닥수평·중심축")
                                      : QStringLiteral("구연부 0 mm·중심축"))
                               : QStringLiteral("미정렬")));
  stDirty_->setText(
      dirtyExport_
          ? QStringLiteral("<span style='color:%1'>●</span> 도면 저장 전")
                .arg(theme::hex(theme::Caution))
          : QString());
  if (!doc_.aligned())
    setHint(QStringLiteral("다음: [중심축 맞춤] 으로 유물을 바로 세우세요 — "
                           "단면·치수는 정위치 후에 정확합니다."));
  else if (dirtyExport_)
    setHint(QStringLiteral(
        "실측선이 완성되었습니다 — 다음: <span "
        "style='font-family:Consolas'>Ctrl+P</span> 「도면」으로 보고서 도판을 "
        "만들거나 「SVG 일러스트」로 내보내세요."));
  else
    setHint(QStringLiteral("저장했습니다. 다른 슬롯을 고르거나 [단면 추가] 로 "
                           "횡단면을 더하세요."));
  if (onCut)
    setHint(cutpage::kStatus);
  if (onEdit)
    setHint(cutpage::kEditStatus);
  updateCutInfo();
}

void MainWindow::updateCutInfo() {
  const double a = doc_.previewAngle();
  if (ribbon_->currentPage() == pCut_) {
    const size_t full = doc_.mesh().faces.size(),
                 lod = doc_.lodMesh().faces.size();
    ctxRight_->setText(
        cutpage::ctxRight(a, lod ? lod : full, full, doc_.isPreviewing()));
  } else {
    ctxRight_->setText(
        QStringLiteral("점 %1 → <b>%2개</b> (%3 mm 간략화) · 최대 편차 %4 mm")
            .arg(QLocale(QLocale::English).toString(doc_.rawPointCount()))
            .arg(doc_.pointCount())
            .arg(doc_.simplifyTol(), 0, 'f', 1)
            .arg(doc_.maxDeviationMm(), 0, 'f', 2));
  }
  cutAngle_->setText(cutpage::angleText(a));
  cutAxisGroup_->setTitle(cutpage::axisGroupName(a));
  ribbon_->setState("axX", cutpage::atAxis(a, CutAxis::X) ? ToolState::On
                                                          : ToolState::Idle);
  ribbon_->setState("axY", cutpage::atAxis(a, CutAxis::Y) ? ToolState::On
                                                          : ToolState::Idle);
  const QString fx =
      cutpage::fixText(angleFixed() ? fixedAngle_ : std::nullopt);
  stFix_->setText(fx);
  fixedLbl_->setText(fx);
  ribbon_->setState("fixAngle", angleFixed() ? ToolState::On : ToolState::Idle);
}

bool MainWindow::angleFixed() const {
  return fixedAngle_ && doc_.poseConfirmed() && !doc_.isPreviewing() &&
         !cut_.active() &&
         std::fabs(std::remainder(*fixedAngle_ - doc_.frontAngle(), 360.0)) <=
             1e-9;
}

int MainWindow::firstVerticalSlot() const {
  const auto &sl = doc_.slotList();
  const auto v = std::find_if(sl.begin(), sl.end(), [](const SectionSlot &s) {
    return s.kind == SlotKind::Vertical;
  });
  return v == sl.end() ? -1 : int(v - sl.begin());
}

void MainWindow::fixAngle() {
  fixedAngle_ = doc_.frontAngle();
  auto &sl = doc_.slotList(); // 고정할 종단면: 지금 슬롯이 종단면이면 그것,
                              // 아니면 첫 종단면 (③ 에 들어갈 때 고를 슬롯)
  const int k =
      slot_ < int(sl.size()) && sl[size_t(slot_)].kind == SlotKind::Vertical
          ? slot_
          : firstVerticalSlot();
  if (k < 0)
    return;
  sl[size_t(k)].angleDeg =
      *fixedAngle_; // 슬롯을 다시 골라도 고정 각으로 돌아온다
  sl[size_t(k)].note = cutpage::slotMemo(*fixedAngle_);
}

void MainWindow::refreshSlots() {
  QSignalBlocker b(slotList_);
  slotList_->clear();
  const auto &sl = doc_.slotList();
  for (int i = 0; i < int(sl.size()); ++i) {
    const auto &s = sl[size_t(i)];
    auto *it = new QListWidgetItem(slotList_);
    it->setData(Qt::UserRole, i);
    auto *w = new QWidget;
    w->setAttribute(Qt::WA_TranslucentBackground);
    auto *v = new QVBoxLayout(w);
    v->setContentsMargins(10, 8, 10, 8);
    v->setSpacing(2);
    auto *n = new QLabel(s.name, w);
    n->setStyleSheet(QStringLiteral(
        "font-weight:700; font-size:12px; background:transparent;"));
    const QString meta =
        s.kind == SlotKind::Vertical
            ? QStringLiteral("%1 · %2 mm")
                  .arg(s.meta)
                  .arg(doc_.dims().height, 0, 'f', 1)
            : QStringLiteral("높이 H %1 mm %2").arg(s.z, 0, 'f', 1).arg(s.meta);
    auto *m = new QLabel(meta, w);
    m->setFont(theme::monoFont(11));
    m->setStyleSheet(QStringLiteral("color:%1; background:transparent;")
                         .arg(theme::hex(theme::Muted)));
    const bool fx =
        s.kind == SlotKind::Vertical &&
        angleFixed() && // 고정 표시는 살아 있는 상태로 (저장하지 않음)
        std::fabs(std::remainder(s.angleDeg - *fixedAngle_, 360.0)) <= 1e-9;
    auto *nt = new QLabel(fx ? cutpage::fixText(fixedAngle_) : s.note, w);
    nt->setStyleSheet(
        QStringLiteral("color:%1; font-size:11px; background:transparent;")
            .arg(theme::hex(theme::Muted)));
    v->addWidget(n);
    v->addWidget(m);
    v->addWidget(nt);
    it->setSizeHint(QSize(170, 74));
    slotList_->setItemWidget(it, w);
  }
  slotList_->setCurrentRow(slot_);
  slotCount_->setText(QString::number(sl.size()));
}

void MainWindow::selectSlot(int i) {
  flushCut();
  if (i < 0 || i >= int(doc_.slotList().size()))
    return;
  slot_ = i;
  const auto &s = doc_.slotList()[size_t(i)];
  if (s.kind == SlotKind::Vertical &&
      std::fabs(s.angleDeg - doc_.frontAngle()) > 1e-6) {
    doc_.setFrontAngle(s.angleDeg);
    refreshAll(true);
  } else {
    applyOptions();
    sheet_->refresh(doc_, slot_);
    view3d_->refresh(doc_, slot_);
    slotList_->setCurrentRow(slot_);
  }
}

void MainWindow::applyOptions() {
  opts_.slot = slot_;
  opts_.shade = ribbon_->state("shade") == ToolState::On;
  opts_.grid = ribbon_->state("grid") == ToolState::On;
  const bool lines = ribbon_->state("lines") == ToolState::On;
  opts_.section = lines;
  opts_.outline = lines;
  sheet_->canvas()->setOptions(opts_);
}

void MainWindow::setHint(const QString &s) { stHint_->setText(s); }

bool MainWindow::requirePose() {
  if (!doc_.isLoaded() || doc_.poseConfirmed())
    return true;
  ribbon_->setCurrentPage(pBase_); // 페이지를 먼저 옮기고 안내는 나중에 (페이지
                                   // 전환이 안내를 덮지 않게)
  setHint(QStringLiteral("먼저 ① 바닥 정하기에서 「이 자세로 확정」 (Enter)"));
  return false;
}

void MainWindow::confirmPose() {
  if (!doc_.isLoaded())
    return;
  if (doc_.poseConfirmed()) {
    setHint(
        QStringLiteral("이미 확정했습니다 — 자세를 바꾸면 다시 확정하세요."));
    return;
  }
  QApplication::setOverrideCursor(Qt::WaitCursor);
  doc_.confirmPose();
  QApplication::restoreOverrideCursor();
  slot_ = 0;
  fixedAngle_.reset();
  ribbon_->setCurrentPage(pCut_); // 확정하면 곧바로 ② 막으로 자르기
  refreshAll(true);
  sheet_->canvas()->fitView();
  recordRecent();
}

// ---- 도구·동작
// ----------------------------------------------------------------
void MainWindow::onPage(int p) {
  flushCut(); // 탭 이동도 "다른 동작" — 막 각도를 먼저 확정
  const bool
      cut = p == pCut_,
      edit = p ==
             pEdit_; // ② = 「✂ 막으로 자르기」, ③ = 「✎ 도면 편집」을 든 페이지
  if ((cut || edit) && (!doc_.isLoaded() || !requirePose()))
    return; // 확정 전 → ① 로 안내
  if ((!cut && hand_ == "cut") || (!edit && hand_ == "edit"))
    setHandTool(QString());
  sheet_->setCutPage(cut);
  view3d_->setCutPage(cut);
  view3d_->setEditPage(edit);
  view3d_->setMinimumWidth(edit ? 330 : 240); // ③ 3D 330 px 고정 (디자인 가이드
                                              // 「편집 단계는 도판을 넓게」)
  view3d_->setMaximumWidth(edit ? 330 : QWIDGETSIZE_MAX);
  if (cut || edit) {
    const auto &sl = doc_.slotList(); // 가로 슬롯을 보고 있었으면 첫 종단면으로
                                      // (각도는 그대로)
    if (slot_ < int(sl.size()) &&
        sl[size_t(slot_)].kind == SlotKind::Horizontal &&
        firstVerticalSlot() >= 0)
      slot_ = firstVerticalSlot();
    setHandTool(cut ? "cut" : "edit");
  }
  if (doc_.poseConfirmed())
    refreshAll(false); // 확정 전이면 requirePose 안내를 지우지 않게 부르지
                       // 않는다 (③ 은 여기서 자동 고정)
}

void MainWindow::setHandTool(const QString &id) {
  for (const char *t : {"cut", "outline"})
    ribbon_->setState(t, ToolState::Idle);
  hand_ = id;
  if (id.isEmpty()) {
    ctx_->hide();
    view3d_->canvas()->setCutTool(false);
    return;
  }
  ribbon_->setState(id, ToolState::Hand); // R3: 손에 든 도구는 하나
  ctxBadge_->setText(id == "cut"    ? cutpage::kCtxBadge
                     : id == "edit" ? cutpage::kEditBadge
                                    : QStringLiteral("◠ 외곽선 편집 중"));
  ctxHint_->setText(id == "cut" ? cutpage::kCtxHint
                    : id == "edit"
                        ? cutpage::kEditHint
                        : QStringLiteral("간격 칩으로 외곽선 점 수를, 곡선 "
                                         "칩으로 매끄러움을 고릅니다."));
  ctxHint_->setToolTip(ctxHint_->text());
  view3d_->canvas()->setCutTool(id == "cut");
  ctx_->show();
}

void MainWindow::pushUndo() {
  undo_.push_back(
      {doc_.frontAngle(), doc_.heightRef(), doc_.simplifyTol(), doc_.smooth()});
  if (undo_.size() > 100)
    undo_.removeFirst();
  redo_.clear();
}

void MainWindow::restore(const ViewState &s) {
  doc_.setHeightRef(s.ref);
  doc_.setSimplifyTol(s.simplify);
  doc_.setSmooth(s.smooth);
  doc_.setFrontAngle(s.angle);
  fixedAngle_.reset(); // 되돌리기·다시 = 막 각도가 바뀜 (③ 에서는 refreshAll 이
                       // 그 각으로 다시 고정)
  static const double tol[] = {0.1, 0.3, 0.5, 1.0};
  for (int i = 0; i < 4; ++i)
    if (std::fabs(tol[i] - s.simplify) < 1e-9)
      ribbon_->setChip("simplify", i);
  ribbon_->setChip("curve", s.smooth ? 1 : 0);
  refreshAll(true);
}

// ---- ② 막 각도 입력 합치기
// ----------------------------------------------------
void MainWindow::scheduleCut() {
  const long long due = cut_.nextDueMs();
  if (due < 0) {
    cutTimer_->stop();
    return;
  }
  cutTimer_->start(int(std::max<long long>(0, due - cutClock_.elapsed())));
}

void MainWindow::onCutTimer() {
  for (auto due = cut_.poll(cutClock_.elapsed()); due != CutGesture::Due::None;
       due = cut_.poll(cutClock_.elapsed())) {
    if (due == CutGesture::Due::Preview) {
      const double snapped = doc_.snapAngle(cut_.target(), 2.0);
      doc_.previewFrontAngle(
          snapped); // LOD 단면 1장 (치수·두께·외곽은 놓은 뒤)
      refreshPreview();
    } else {
      const double snapped = doc_.snapAngle(cut_.target(), 2.0);
      doc_.setFrontAngle(snapped); // 원본으로 단면·외곽·치수·두께 다시 계산
      refreshAll(true);
    }
  }
  scheduleCut();
}

void MainWindow::refreshPreview() {
  sheet_->refresh(doc_, slot_);
  sheet_->canvas()->update();
  view3d_->refresh(doc_, slot_);
  updateCutInfo();
}

void MainWindow::beginCutInput() {
  if (cut_.active())
    return;
  pushUndo();
  if (fixedAngle_) { // 막을 움직이면 고정이 풀린다 — 끄는 동안은 refreshAll 이
                     // 오지 않아 슬롯 목록을 여기서
    fixedAngle_.reset();
    refreshSlots();
  }
}

void MainWindow::flushCut() {
  if (!cut_.active())
    return;
  cut_.release(cutClock_.elapsed());
  onCutTimer();
}

void MainWindow::cancelCut() {
  cut_.cancel();
  cutTimer_->stop();
  if (sweepTimer_) {
    sweepTimer_->stop();
    sweepTimer_->deleteLater();
    sweepTimer_ = nullptr;
  }
}

void MainWindow::saveSlotAngle(int k) {
  auto &sl = doc_.slotList();
  const QString memo = cutpage::slotMemo(doc_.frontAngle()); // 예: 막 12°
  int seen = 0;
  for (int i = 0; i < int(sl.size()); ++i) {
    if (sl[size_t(i)].kind != SlotKind::Vertical)
      continue;
    if (++seen == k) {
      sl[size_t(i)].angleDeg = doc_.frontAngle();
      sl[size_t(i)].note = memo;
      slot_ = i;
      refreshAll(false);
      setHint(QStringLiteral("슬롯 %1 에 %2 를 저장했습니다.")
                  .arg(QString::number(k), memo));
      return;
    }
  }
  const QChar c = QChar('A' + std::min(25, seen));
  sl.push_back({QStringLiteral("%1–%1′ 정면 실측도").arg(c),
                QStringLiteral("종단면"), memo, SlotKind::Vertical,
                doc_.frontAngle(), 0.0});
  slot_ = int(sl.size()) - 1;
  refreshAll(false);
}

void MainWindow::openExport(ExportFormat f) {
  if (!doc_.isLoaded())
    return;
  ExportDialog dlg(doc_, slot_, opts_, f, this);
  if (dlg.exec() == QDialog::Accepted && !dlg.savedPath().isEmpty()) {
    dirtyExport_ = false;
    refreshAll(false);
    setHint(QStringLiteral(
                "저장했습니다: <span style='font-family:Consolas'>%1</span>")
                .arg(QDir::toNativeSeparators(dlg.savedPath())));
  }
}

void MainWindow::onAction(const QString &id) {
  static const QStringList cutInput = {"cutMinus", "cutPlus", "axX", "axY"};
  if (!cutInput.contains(id))
    flushCut(); // 다른 동작(내보내기 포함) 전에 막 각도를 먼저 확정
  if (id == "open")
    return openFileDialog();
  if (id == "watch")
    return toggleWatchFolder();
  if (id == "sample")
    return openSample();
  if (id == "blindSample")
    return openArtecBlindSample();
  if (id == "home")
    return showHome();
  if (id == "recent") {
    QMenu m(this);
    for (const auto &r : loadRecent())
      m.addAction(r.name, this, [this, p = r.path] { openPath(p); });
    m.addSeparator();
    m.addAction(QStringLiteral("내장 예제 1: 합성 토기 (Artec 컬러)"), this,
                [this] { openSample(); });
    m.addAction(
        QStringLiteral("내장 예제 2: Artec 내벽 미스캔 단경호 (파선 보정)"),
        this, [this] { openArtecBlindSample(); });
    m.addSeparator();
    m.addAction(watchDir_.isEmpty()
                    ? QStringLiteral("Artec Studio 연동 감시 폴더 지정…")
                    : QStringLiteral("● Artec 감시 중 (%1)…")
                          .arg(QFileInfo(watchDir_).fileName()),
                this, &MainWindow::toggleWatchFolder);
    m.exec(QCursor::pos());
    return;
  }
  if (!doc_.isLoaded())
    return;
  // 확정 전에는 단면·치수·내보내기 동작을 하지 않는다 (표시 토글
  // shade·lines·grid·shadeMode 는 허용)
  static const QStringList needPose = {
      "cut",      "outline",         "cutMinus", "cutPlus",  "axX", "axY",
      "axZ",      "addSlot",         "addVerticalSlot",      "refBase",
      "refRim",   "export",          "svg",      "dxf",      "combined",
      "fixAngle", "toEdit",          "backToCut"};
  if (needPose.contains(id) && !requirePose())
    return;
  if (id == "poseX" || id == "poseY" || id == "poseZ") {
    // 고정 월드축 +90° (오른손 법칙), Shift 를 누른 채면 −90°. 자세 조작은
    // 되돌리기에 쌓지 않는다.
    const bool neg =
        QGuiApplication::keyboardModifiers().testFlag(Qt::ShiftModifier);
    doc_.rotatePose(id == "poseX"   ? PoseAxis::X
                    : id == "poseY" ? PoseAxis::Y
                                    : PoseAxis::Z,
                    neg ? -90.0 : 90.0);
    slot_ = 0;
    refreshAll(true);
  } else if (id == "poseFlip" || id == "poseReset" || id == "poseDrop" ||
             id == "poseCenter") {
    if (id == "poseFlip")
      doc_.flipPose();
    else if (id == "poseReset")
      doc_.resetPose();
    else if (id == "poseCenter")
      doc_.centerOnFloor();
    else
      doc_.dropToFloor();
    slot_ = 0;
    refreshAll(true);
  } else if (id == "pose3PtClear") {
    doc_.clearFloorPickPoints();
    view3d_->canvas()->update();
  } else if (id == "poseAuto") {
    QApplication::setOverrideCursor(Qt::WaitCursor);
    doc_.autoAlign(); // 자동 세우기 — 확정은 사람이 (Enter)
    QApplication::restoreOverrideCursor();
    slot_ = 0;
    refreshAll(true);
  } else if (id == "poseConfirm") {
    confirmPose();
  } else if (id == "poseModeTB") {
    pushUndo();
    doc_.setBaseTiltMode(doc_.baseTiltMode() == BaseTiltMode::RimHorizontal
                             ? BaseTiltMode::BaseHorizontal
                             : BaseTiltMode::RimHorizontal);
    refreshAll(false);
    sheet_->canvas()->update();
    setHint(doc_.baseTiltMode() == BaseTiltMode::RimHorizontal
                ? QStringLiteral("수평 기준: 구연 수평 (T · 권고)")
                : QStringLiteral("수평 기준: 바닥 수평 (B, 바닥판 그대로)"));
  } else if (id == "sherdModeToggle") {
    pushUndo();
    doc_.setSherdMode(!doc_.isSherdMode());
    refreshAll(false);
    sheet_->canvas()->update();
    setHint(
        doc_.isSherdMode()
            ? QStringLiteral("구연부편 모드: 저경 공란, 원호판 가상 축 산출")
            : QStringLiteral("완형 모드"));
  } else if (id == "floorCand1" || id == "floorCand2" || id == "floorCand3") {
    int cIdx = (id == "floorCand1" ? 0 : (id == "floorCand2" ? 1 : 2));
    pushUndo();
    doc_.applyFloorCandidate(cIdx);
    refreshAll(true);
    setHint(QStringLiteral("바닥 후보 %1 적용됨").arg(cIdx + 1));
  } else if (id == "axis") {
    QApplication::setOverrideCursor(Qt::WaitCursor);
    pushUndo();
    doc_.autoAlign(); // 중심축 맞춤 = 자동 세우기 → 확정 (한 번 누르기)
    doc_.confirmPose();
    QApplication::restoreOverrideCursor();
    slot_ = 0;
    fixedAngle_.reset();
    refreshAll(true);
  } else if (id == "refBase" || id == "refRim") {
    pushUndo();
    doc_.setHeightRef(id == "refBase" ? HeightRef::Base : HeightRef::Rim);
    refreshAll(false);
    sheet_->canvas()->update();
  } else if (id == "cutMinus" || id == "cutPlus") {
    beginCutInput(); // 단추: 마지막 누름 300 ms 뒤 원본으로 확정
    cut_.click(doc_.frontAngle(), id == "cutMinus" ? -1.0 : 1.0,
               cutClock_.elapsed());
    scheduleCut();
  } else if (id == "axX" || id == "axY") {
    // 막 방향 맞춤 (X 막 90° · Y 막 0°): 절대각이라 델타 합치기를 거치지 않는다
    const double goal = id == "axX" ? 90.0 : 0.0;
    const bool pending = cut_.active(); // 끄는·누르는 중 막이 있으면 확정하지
                                        // 않고 취소 (원본 계산 한 번)
    if (!pending &&
        std::fabs(std::remainder(goal - doc_.frontAngle(), 360.0)) <= 1e-9)
      return; // 이미 그 각
    if (pending)
      cancelCut(); // 되돌리기 한 칸은 beginCutInput 이 이미 쌓았다
    else
      pushUndo();
    fixedAngle_.reset();
    QApplication::setOverrideCursor(Qt::WaitCursor); // 원본 다시 계산 ≈ 1.4 s
    doc_.alignCut(id == "axX" ? CutAxis::X
                              : CutAxis::Y); // 미리보기도 함께 지운다
    QApplication::restoreOverrideCursor();
    refreshAll(true);
  } else if (id == "axZ") {
    pushUndo();
    int hSlot = -1;
    for (size_t i = 0; i < doc_.slotList().size(); ++i) {
      if (doc_.slotList()[i].kind == SlotKind::Horizontal) {
        hSlot = int(i);
        break;
      }
    }
    if (hSlot < 0) {
      const double z = doc_.dims().bellyZ > 0.0 ? doc_.dims().bellyZ : 95.0;
      doc_.slotList().push_back(
          {QStringLiteral("C–C′ 횡단면"), QStringLiteral("수평절단"),
           QStringLiteral("Z 막 횡단면"), SlotKind::Horizontal, 0.0, z});
      hSlot = int(doc_.slotList().size()) - 1;
    }
    slot_ = hSlot;
    refreshAll(false);
  } else if (id == "cutRec1" || id == "cutRec2" || id == "cutRec3") {
    const auto recs = doc_.recommendAngles();
    size_t recIdx = (id == "cutRec1" ? 0 : (id == "cutRec2" ? 1 : 2));
    if (recIdx < recs.size()) {
      pushUndo();
      doc_.setFrontAngle(recs[recIdx].angleDeg);
      refreshAll(true);
      setHint(QStringLiteral("추천 각도 적용: %1° (%2)")
                  .arg(recs[recIdx].angleDeg, 0, 'f', 1)
                  .arg(recs[recIdx].why));
    }
  } else if (id == "cutSweep") {
    if (sweepTimer_) {
      sweepTimer_->stop();
      sweepTimer_->deleteLater();
      sweepTimer_ = nullptr;
    }
    sweepTimer_ = new QTimer(this);
    QTimer *timer = sweepTimer_;
    QElapsedTimer elapsed;
    elapsed.start();
    const double startAngle = doc_.frontAngle();
    connect(timer, &QTimer::timeout, this,
            [this, timer, elapsed, startAngle]() mutable {
              qint64 ms = elapsed.elapsed();
              if (ms >= 3200) {
                timer->stop();
                timer->deleteLater();
                if (sweepTimer_ == timer)
                  sweepTimer_ = nullptr;
                doc_.setFrontAngle(startAngle);
                refreshAll(true);
                return;
              }
              double frac = double(ms) / 3200.0;
              double ang = std::remainder(startAngle + frac * 360.0, 360.0);
              doc_.previewFrontAngle(ang);
              refreshPreview();
            });
    timer->start(16);
  } else if (id == "cut") {
    ribbon_->setCurrentPage(
        hand_ == "cut" ? pHome_ : pCut_); // ② 막으로 자르기 = 도구를 든 페이지
                                          // (③ 에서 S = 다시 자르기)
  } else if (id == "fixAngle") {
    fixAngle();
    refreshAll(false);
  } else if (id == "toEdit" || id == "backToCut") {
    ribbon_->setCurrentPage(
        id == "toEdit" ? pEdit_
                       : pCut_); // ③ 은 들어갈 때 지금 각으로 고정 (refreshAll)
  } else if (id == "outline") {
    if (ribbon_->currentPage() == pCut_ || ribbon_->currentPage() == pEdit_)
      ribbon_->setCurrentPage(pHome_);
    setHandTool(hand_ == id ? QString() : id);
  } else if (id == "combined") {
    // D5 좌/우 반전 (R): 같은 단면의 거울상. 다시 계산하지 않고, 되돌리기에
    // 쌓지 않는다.
    doc_.toggleSectionSide();
    ribbon_->setText("combined", sideText(doc_.sectionSide()));
    refreshAll(true); // 도판·3D·음영(보는 방향)을 다시 그린다
    setHint(doc_.sectionSide() == SectionSide::Right
                ? QStringLiteral(
                      "단면 위치: 외면 왼쪽 · 단면 오른쪽 (국내 보고서 표준)")
                : QStringLiteral("단면 위치: 단면 왼쪽 · 외면 오른쪽 (R 반전) "
                                 "— R 을 다시 누르면 되돌립니다"));
  } else if (id == "addSlot") {
    const double h = doc_.dims().height;
    auto &sl = doc_.slotList();
    const double curHorizZ =
        (slot_ >= 0 && slot_ < int(sl.size()) &&
         sl[size_t(slot_)].kind == SlotKind::Horizontal)
            ? sl[size_t(slot_)].z
            : doc_.dims().bellyZ;
    const double z = (lastHoverH_ > 0.5 && lastHoverH_ < h - 0.5) ? lastHoverH_
                                                                  : curHorizZ;
    int nh = 0;
    for (const auto &s : sl)
      nh += s.kind == SlotKind::Horizontal;
    const QChar c = QChar('B' + std::min(24, nh));
    sl.push_back(
        {QStringLiteral("%1–%1′ 횡단면").arg(c), QStringLiteral("수평절단"),
         QStringLiteral("높이 H %1 mm 절단").arg(z, 0, 'f', 1),
         SlotKind::Horizontal, 0.0, z});
    slot_ = int(sl.size()) - 1;
    refreshAll(false);
  } else if (id == "addVerticalSlot") {
    pushUndo();
    slot_ = doc_.addVerticalSlotAt(doc_.frontAngle());
    refreshAll(false);
    setHint(QStringLiteral("지금 자리(%1°)의 종단면을 도면 목록에 추가했습니다.")
                .arg(doc_.frontAngle(), 0, 'f', 0));
  } else if (id == "shade" || id == "lines" || id == "grid") {
    ribbon_->setState(id, ribbon_->state(id) == ToolState::On ? ToolState::Idle
                                                              : ToolState::On);
    applyOptions();
  } else if (id == "shadeMode") {
    opts_.shadeMode = ShadeMode((int(opts_.shadeMode) + 1) % 3);
    ribbon_->setText("shadeMode",
                     QString::fromUtf8(kShadeNames[int(opts_.shadeMode)]));
    ribbon_->setState("shade", ToolState::On);
    applyOptions();
  } else if (id == "export") {
    openExport(ExportFormat::Pdf);
  } else if (id == "svg") {
    openExport(ExportFormat::Svg);
  } else if (id == "dxf") {
    openExport(ExportFormat::Dxf);
  }
}

void MainWindow::onChip(const QString &group, int idx) {
  flushCut();
  if (!doc_.isLoaded())
    return;
  pushUndo();
  if (group == "simplify") {
    static const double tol[] = {0.1, 0.3, 0.5, 1.0};
    doc_.setSimplifyTol(tol[idx]);
  } else if (group == "curve") {
    doc_.setSmooth(idx == 1);
  }
  refreshAll(true);
}

void MainWindow::toggleMaximize() {
  maximized_ = !maximized_;
  slotPanel_->setVisible(!maximized_);
  view3d_->setVisible(!maximized_);
  sheet_->setMaximized(maximized_);
  sheet_->canvas()->fitView();
}

bool MainWindow::eventFilter(QObject *o, QEvent *e) {
  if (e->type() == QEvent::KeyPress && pages_->currentIndex() == 1 &&
      isActiveWindow()) {
    auto *k = static_cast<QKeyEvent *>(e);
    if (k->key() == Qt::Key_Tab && k->modifiers() == Qt::NoModifier &&
        !qobject_cast<QLineEdit *>(QApplication::focusWidget())) {
      toggleMaximize();
      return true;
    }
  }
  return QMainWindow::eventFilter(o, e);
}

void MainWindow::keyPressEvent(QKeyEvent *e) {
  const auto mod = e->modifiers();
  const int cutSign = (mod & Qt::ControlModifier) ? 0 : cutKeySign(e->key());
  if (cutSign == 0 && !isModifierKey(e->key()))
    flushCut(); // 각도 키가 아닌 동작 전에 먼저 확정
  if (mod & Qt::ControlModifier) {
    switch (e->key()) {
    case Qt::Key_O:
      return openFileDialog();
    case Qt::Key_P:
      return onAction("export");
    case Qt::Key_Z:
      return emit ribbon_->undoRequested();
    case Qt::Key_Y:
      return emit ribbon_->redoRequested();
    default:
      break;
    }
  }
  if (pages_->currentIndex() != 1 || !doc_.isLoaded())
    return QMainWindow::keyPressEvent(e);
  if (!doc_.poseConfirmed()) { // ① 바닥 정하기
    switch (e->key()) {
    case Qt::Key_Return:
    case Qt::Key_Enter:
      return confirmPose();
    case Qt::Key_Escape:
      return; // 바닥 정하기 ctx 유지
    case Qt::Key_S:
    case Qt::Key_O:
    case Qt::Key_B:
    case Qt::Key_T:
    case Qt::Key_N:
    case Qt::Key_R:
    case Qt::Key_Comma:
    case Qt::Key_Period:
    case Qt::Key_BracketLeft:
    case Qt::Key_BracketRight:
    case Qt::Key_Left:
    case Qt::Key_Right:
    case Qt::Key_1:
    case Qt::Key_2:
    case Qt::Key_3:
    case Qt::Key_4:
    case Qt::Key_5:
    case Qt::Key_W:
    case Qt::Key_Minus:
    case Qt::Key_Underscore:
    case Qt::Key_Equal:
    case Qt::Key_Plus:
      requirePose();
      return;
    default:
      break; // I · L · V 표시 토글 등은 그대로
    }
  }
  if (cutSign != 0 && ribbon_->currentPage() == pEdit_)
    return; // ③ 에서는 막이 돌지 않는다 (고정된 막)
  if (cutSign != 0) {
    // ② 막 1° (← → 만 Shift 15°): 누르는 동안 LOD 미리보기, 뗀 뒤 300 ms 에
    // 원본 확정 (자동 반복 포함)
    const bool arrow = e->key() == Qt::Key_Left || e->key() == Qt::Key_Right;
    const double step = arrow && (mod & Qt::ShiftModifier) ? 15.0 : 1.0;
    beginCutInput();
    cut_.keyDown(doc_.frontAngle(), cutSign * step, cutClock_.elapsed());
    scheduleCut();
    return;
  }
  switch (e->key()) {
  case Qt::Key_Escape: // ②·③ 에서는 홈으로 (= 손에 든 도구 내려놓기)
    if (ribbon_->currentPage() == pCut_ || ribbon_->currentPage() == pEdit_)
      ribbon_->setCurrentPage(pHome_);
    else
      setHandTool(QString());
    break;
  case Qt::Key_Return:
  case Qt::Key_Enter: // ② 지금 각도 고정
    if (ribbon_->currentPage() == pCut_)
      onAction("fixAngle");
    break;
  case Qt::Key_S:
    onAction("cut");
    break;
  case Qt::Key_O:
    onAction("outline");
    break;
  case Qt::Key_B:
    onAction("refBase");
    break;
  case Qt::Key_T:
    onAction("refRim");
    break;
  case Qt::Key_N:
    onAction("addSlot");
    break;
  case Qt::Key_I:
    onAction("shade");
    break;
  case Qt::Key_L:
    onAction("lines");
    break;
  case Qt::Key_V:
    onAction("grid");
    break;
  case Qt::Key_W:
    doc_.setInferInnerWall(!doc_.inferInnerWall());
    refreshAll(true);
    setHint(doc_.inferInnerWall()
                ? QStringLiteral("Artec 내벽 미스캔 파선 자동 보정을 켰습니다 "
                                 "(추정 두께 %1 mm · -/+ 로 조절).")
                      .arg(doc_.estimatedWallMm(), 0, 'f', 1)
                : QStringLiteral("Artec 내벽 미스캔 자동 보정을 껐습니다 "
                                 "(스캔된 원본 단면만 표시)."));
    break;
  case Qt::Key_Minus:
  case Qt::Key_Underscore:
    doc_.setEstimatedWallMm(doc_.estimatedWallMm() - 0.5);
    refreshAll(true);
    setHint(QStringLiteral("미스캔 추정 기벽 두께: %1 mm")
                .arg(doc_.estimatedWallMm(), 0, 'f', 1));
    break;
  case Qt::Key_Equal:
  case Qt::Key_Plus:
    doc_.setEstimatedWallMm(doc_.estimatedWallMm() + 0.5);
    refreshAll(true);
    setHint(QStringLiteral("미스캔 추정 기벽 두께: %1 mm")
                .arg(doc_.estimatedWallMm(), 0, 'f', 1));
    break;
  case Qt::Key_R:
    onAction("combined");
    break; // D5 좌/우 반전
  case Qt::Key_1:
  case Qt::Key_2:
  case Qt::Key_3:
  case Qt::Key_4:
  case Qt::Key_5:
    saveSlotAngle(e->key() - Qt::Key_0);
    break;
  default:
    QMainWindow::keyPressEvent(e);
  }
}

void MainWindow::keyReleaseEvent(QKeyEvent *e) {
  if (!e->isAutoRepeat() &&
      cutKeySign(e->key()) != 0) { // 자동 반복이 아닌 뗌만 → 300 ms 뒤 확정
    cut_.keyUp(cutClock_.elapsed());
    scheduleCut();
    return;
  }
  QMainWindow::keyReleaseEvent(e);
}

void MainWindow::dragEnterEvent(QDragEnterEvent *e) {
  if (e->mimeData()->hasUrls())
    e->acceptProposedAction();
}

void MainWindow::dropEvent(QDropEvent *e) {
  const auto urls = e->mimeData()->urls();
  if (!urls.isEmpty())
    openPath(urls.first().toLocalFile());
}

void MainWindow::closeEvent(QCloseEvent *e) {
  QSettings().setValue("window/geometry", saveGeometry());
  e->accept();
}

} // namespace studio
