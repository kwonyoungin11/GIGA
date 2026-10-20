// DongArch3D Studio — 3D 유물·절단면 보조창 (UI_RULES R1: 약 27 %, 접기 가능).
#pragma once
#include "ArtifactDoc.h"
#include "MeshRaster.h"

#include <QLabel>
#include <QRectF>
#include <QWidget>
#include <array>

class QPushButton;
class QSlider;

namespace studio {

enum class PosePickMode { None, Surface1Click, Floor3Point };
enum class CutPickMode { None, CutPoint1Click, RubbingSpot1Click };

class AngleStripWidget : public QWidget {
  Q_OBJECT
public:
  explicit AngleStripWidget(QWidget *parent = nullptr);
  void setCells(std::vector<AngleStripCell> cells, double currentAngleDeg);

signals:
  void angleClicked(double deg);

protected:
  void paintEvent(QPaintEvent *) override;
  void mousePressEvent(QMouseEvent *) override;

private:
  std::vector<AngleStripCell> cells_;
  double currentAngleDeg_ = 0.0;
};

// [JOB-013] 왼쪽 상단: 위에서 내려다본 실제 유물 평면도 (클릭·드래그로 단면선 A-A' 회전)
class PlanViewWidget : public QWidget {
  Q_OBJECT
public:
  explicit PlanViewWidget(QWidget *parent = nullptr);
  void setDoc(const ArtifactDoc *doc);

signals:
  void anglePreviewed(double absDeg);
  void angleCommitted(double absDeg);

protected:
  void paintEvent(QPaintEvent *) override;
  void mousePressEvent(QMouseEvent *) override;
  void mouseMoveEvent(QMouseEvent *) override;
  void mouseReleaseEvent(QMouseEvent *) override;
  void resizeEvent(QResizeEvent *) override { dirty_ = true; }

private:
  double angleForScreenPt(const QPointF &pt) const;
  void rebuildTopImage();

  const ArtifactDoc *doc_ = nullptr;
  QImage baseTopImg_;
  double pxPerMm_ = 1.0;
  double maxR_ = 50.0;
  QPointF ctrPx_;
  bool dirty_ = true;
  bool dragging_ = false;
  size_t cachedVertCount_ = 0;
  double cachedBboxZ_ = 0.0;
};

class View3DCanvas : public QWidget {
  Q_OBJECT
public:
  explicit View3DCanvas(QWidget *parent = nullptr);
  void setDoc(const ArtifactDoc *d, int slot);
  void setView(double yaw, double pitch);
  void setCutTool(bool on) { cutTool_ = on; }
  void setPosePickMode(PosePickMode m) {
    posePickMode_ = m;
    update();
  }
  PosePickMode posePickMode() const { return posePickMode_; }
  void setCutPickMode(CutPickMode m) {
    cutPickMode_ = m;
    update();
  }
  CutPickMode cutPickMode() const { return cutPickMode_; }
  // 탁본 범위를 끌 때는 정면(수평 시선)에서 — 화면 상자 = 표면 범위가 정확히 맞도록
  void levelView() {
    pitch_ = 0.0;
    dirty_ = true;
    update();
  }

signals:
  void cutAngleDragged(double deltaDeg);
  void cutDragReleased(); // 단면 도구로 끌다가 놓음 → 원본으로 확정
  void cutAngleSelected(double absDeg);
  void poseRotated(PoseAxis axis, double deg);
  void floorCandidateClicked(int index);
  void surfacePointPicked(KA::Point3 worldPt);
  void cutPointPicked(KA::Point3 worldPt);
  void rubbingSpotPicked(KA::Point3 worldPt);
  void rubbingRegionPicked(studio::RubbingRegion region); // [JOB-014] 끌어서 고른 탁본 범위
  void horizontalHeightDragged(double zMm);

protected:
  void paintEvent(QPaintEvent *) override;
  void mousePressEvent(QMouseEvent *) override;
  void mouseMoveEvent(QMouseEvent *) override;
  void mouseReleaseEvent(QMouseEvent *) override;
  void wheelEvent(QWheelEvent *) override;
  void resizeEvent(QResizeEvent *) override { dirty_ = true; }

private:
  RasterView view() const;
  bool pickSurfacePoint(const QPointF &screenPt, KA::Point3 &outWorld) const;
  int hitGizmoAxis(const QPointF &screenPt) const;

  const ArtifactDoc *doc_ = nullptr;
  int slot_ = 0;
  double yaw_ = -32.0, pitch_ = 14.0;
  QImage img_;
  bool dirty_ = true;
  bool drag_ = false, cutTool_ = false;
  bool cutDrag_ =
      false; // 단면 도구 + 왼쪽 단추 끌기 = 회전판 (오른쪽·가운데 = 보는 방향)
  bool poseDrag_ = false;
  int poseDragAxis_ = -1; // -1: 자유 기울이기, 0: X, 1: Y, 2: Z
  bool minimapDrag_ = false;
  bool horizDrag_ = false;
  PosePickMode posePickMode_ = PosePickMode::None;
  CutPickMode cutPickMode_ = CutPickMode::None;
  bool rubBoxDrag_ = false; // [JOB-014] 탁본 범위 상자 끄는 중
  QPointF rubBoxStart_, rubBoxEnd_;
  QPoint last_;
  std::vector<KA::Polyline3> cut_;
  mutable std::array<QRectF, 3> floorBadgeRects_{};
  mutable QRectF minimapRect_{};
  mutable QRectF horizHandleRect_{};
};

class View3DPane : public QWidget {
  Q_OBJECT
public:
  explicit View3DPane(QWidget *parent = nullptr);
  View3DCanvas *canvas() const { return canvas_; }
  PlanViewWidget *planWidget() const { return planView_; }
  void refresh(const ArtifactDoc &doc, int slot);
  void setCutPage(bool on); // ② 막으로 자르기: 머리 제목·3D 끌기 툴팁
  void setEditPage(bool on) {
    editPage_ = on;
  } // ③ 도면 편집: 머리 제목 「3D — 고정된 막」

signals:
  void collapseRequested();
  void actionRequested(const QString &id);
  void poseRotateRequested(PoseAxis axis, double deg);
  void anglePreviewed(double absDeg);
  void angleSelected(double absDeg);
  void horizontalHeightSelected(double zMm);
  void cutPointPicked(KA::Point3 worldPt);
  void rubbingSpotPicked(KA::Point3 worldPt);
  void rubbingRegionPicked(studio::RubbingRegion region);

private:
  bool cutPage_ = false, editPage_ = false;
  PlanViewWidget *planView_ = nullptr;
  View3DCanvas *canvas_ = nullptr;
  QLabel *title_ = nullptr;
  QLabel *info_ = nullptr;
  QLabel *info2_ = nullptr;
  QWidget *poseBar_ = nullptr;
  QPushButton *btnSurfacePick_ = nullptr;
  QPushButton *btn3PtPick_ = nullptr;
  QWidget *pickBar_ = nullptr;
  QPushButton *btnPickCut_ = nullptr;
  QPushButton *btnPickRubbing_ = nullptr;
  QWidget *cutBar_ = nullptr;
  QSlider *angleSlider_ = nullptr;
  QLabel *angleSliderLbl_ = nullptr;
  QWidget *heightRow_ = nullptr;
  QSlider *heightSlider_ = nullptr;
  QLabel *heightSliderLbl_ = nullptr;
  AngleStripWidget *angleStrip_ = nullptr;
};

} // namespace studio
