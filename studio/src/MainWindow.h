// DongArch3D Studio — 메인 창 (UI_RULES R1 고정 구조 + Artec Studio 실시간
// 연동).
#pragma once
#include "ArtifactDoc.h"
#include "CutGesture.h"
#include "Plate.h"

#include <QElapsedTimer>
#include <QMainWindow>
#include <QVector>
#include <optional>

class QFileSystemWatcher;
class QLabel;
class QListWidget;
class QStackedWidget;
class QTimer;
class QWidget;

namespace studio {

class Ribbon;
class RibbonGroup;
class SheetPane;
class View3DPane;
class StartPage;
enum class ExportFormat;

class MainWindow : public QMainWindow {
  Q_OBJECT
public:
  MainWindow();
  bool
  openPath(const QString &path); // 들어온 자세 그대로 ① 바닥 정하기 단계로 연다
  // poseStage=false: 예전처럼 자동 세우기 → 확정 후 홈, true: 들어온 자세
  // 그대로 ① 바닥 정하기
  void openSample(bool poseStage = false);
  void openArtecBlindSample(bool poseStage = false);
  void showHome();
  void triggerActionForTest(const QString &id) { onAction(id); }
  const ArtifactDoc &doc() const { return doc_; }

protected:
  void keyPressEvent(QKeyEvent *) override;
  void keyReleaseEvent(QKeyEvent *) override;
  bool eventFilter(QObject *, QEvent *) override;
  void dragEnterEvent(QDragEnterEvent *) override;
  void dropEvent(QDropEvent *) override;
  void closeEvent(QCloseEvent *) override;

private:
  struct ViewState {
    double angle;
    HeightRef ref;
    double simplify;
    bool smooth;
  };
  void buildRibbon();
  QWidget *buildSlotPanel();
  QWidget *buildCtxBar();
  QWidget *buildStatusBar();
  void onAction(const QString &id);
  void onPage(int p); // 리본 탭 바뀜: ② 막으로 자르기에 있는 동안만 「✂ 막으로
                      // 자르기」를 든다
  void onChip(const QString &group, int idx);
  void setHandTool(const QString &id); // "" = 내려놓기
  void toggleMaximize();
  void refreshAll(bool geometryChanged);
  void refreshSlots();
  void selectSlot(int i);
  void applyOptions();
  void pushUndo();
  void restore(const ViewState &s);
  // ② 막 각도 입력 합치기: 끄는 동안 LOD 미리보기(16 ms), 놓으면·키 300 ms 뒤
  // 원본 확정
  void scheduleCut();    // 다음 마감에 맞춰 단발 타이머
  void onCutTimer();     // 마감이 지난 미리보기·확정 처리
  void refreshPreview(); // 도판 단면·3D 만 다시 그림 (음영·치수·슬롯 그대로)
  void updateCutInfo();  // ctx 오른쪽 각도·점 수
  void beginCutInput();  // 제스처 하나 = 되돌리기 한 칸
  void flushCut();       // 다른 동작 전에 먼저 확정
  void cancelCut();      // 문서가 바뀜 → 계산하지 않고 버림
  void saveSlotAngle(int k);
  void openExport(ExportFormat f);
  void openFileDialog();
  void toggleWatchFolder();
  void applyWatchFolder(const QString &dir);
  void checkWatchFolder();
  void setHint(const QString &s);
  bool requirePose();  // 확정 전이면 안내하고 ① 페이지로 → false
  void confirmPose();  // 이 자세로 확정 (Enter)
  void recordRecent(); // 최근 목록에 현재 유물 기록 (경로가 있을 때)
  // ② 지금 각도 고정 (Enter): 지금 막 각도를 종단면 슬롯에 남기고 고정 표시 —
  // 막·문서·자세가 바뀌면 풀림
  void fixAngle();
  bool angleFixed()
      const; // 고정 각이 지금 막 각도와 같은지 (확정 뒤 · 끄는 중 아님)
  int firstVerticalSlot() const; // 첫 종단면 슬롯 번호 (없으면 −1)

  ArtifactDoc doc_;
  PlateOptions opts_;
  int slot_ = 0;
  QString hand_;
  bool maximized_ = false;
  bool dirtyExport_ = false;
  double lastHoverH_ = -1;
  QVector<ViewState> undo_, redo_;
  CutGesture cut_;
  QTimer *cutTimer_ = nullptr;
  QTimer *sweepTimer_ = nullptr;
  QElapsedTimer cutClock_;

  // Artec Studio 실시간 감시 폴더
  QFileSystemWatcher *watcher_ = nullptr;
  QTimer *watchTimer_ = nullptr;
  QString watchDir_;
  QString pendingWatchFile_;
  qint64 pendingWatchSize_ = -1;

  QStackedWidget *pages_ = nullptr;
  StartPage *start_ = nullptr;
  Ribbon *ribbon_ = nullptr;
  QWidget *ctx_ = nullptr;
  QLabel *ctxBadge_ = nullptr;
  QLabel *ctxHint_ = nullptr;
  QLabel *ctxRight_ = nullptr;
  QVector<QWidget *>
      ctxCutKeys_; // 단면 도구 단축키 묶음 (← → · W · - + · 1~5 · R · Esc)
  QVector<QWidget *>
      ctxDupKeys_; // 그중 ② 리본과 겹쳐 ② 에서 숨기는 것 (← → · R)
  QVector<QWidget *>
      ctxBaseKeys_; // ① 바닥 정하기 단축키 묶음 (Enter 확정 · Shift −90°)
  int pHome_ = -1, pBase_ = -1, pCut_ = -1,
      pEdit_ =
          -1; // 리본 페이지: 홈, ① 바닥 정하기, ② 막으로 자르기, ③ 도면 편집
  std::optional<double> fixedAngle_; // ② 「지금 각도 고정」 한 막 각도 (화면
                                     // 상태 — 파일·되돌리기에 남지 않음)
  QVector<QWidget *> ctxFixKeys_;       // ② ctx: Enter 지금 각도 고정
  QLabel *fixedLbl_ = nullptr;          // ③ 리본 「고정된 막」: 막 12° 고정
  RibbonGroup *cutAxisGroup_ = nullptr; // ② 막 방향 (그룹 이름에 지금 각)
  QLabel *cutAngle_ = nullptr;          // ② 각도 칸
  QWidget *slotPanel_ = nullptr;
  QListWidget *slotList_ = nullptr;
  QLabel *slotCount_ = nullptr;
  View3DPane *view3d_ = nullptr;
  SheetPane *sheet_ = nullptr;
  QLabel *stHint_ = nullptr;
  QLabel *stCursor_ = nullptr;
  QLabel *stScale_ = nullptr;
  QLabel *stRef_ = nullptr;
  QLabel *stDirty_ = nullptr;
  QLabel *stFix_ = nullptr; // ②·③ 상태줄 오른쪽: 막 고정 전 / 막 12° 고정
};

} // namespace studio
