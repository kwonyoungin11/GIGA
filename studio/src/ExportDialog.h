// DongArch3D Studio — 유물 실측 도면 내보내기 창 (UI_RULES R6, 목업 ③).
#pragma once
#include "Plate.h"

#include <QDialog>

class QLabel;
class QLineEdit;
class QComboBox;
class QCheckBox;
class QPushButton;

namespace studio {

enum class ExportFormat { Svg, Pdf, Dxf, Png };

class PaperPreview : public QWidget {
  Q_OBJECT
public:
  explicit PaperPreview(QWidget *parent = nullptr) : QWidget(parent) {
    setMinimumSize(360, 420);
  }
  void set(const ArtifactDoc *d, const ExportRequest &r) {
    doc_ = d;
    req_ = r;
    update();
  }

protected:
  void paintEvent(QPaintEvent *) override;

private:
  const ArtifactDoc *doc_ = nullptr;
  ExportRequest req_;
};

class ExportDialog : public QDialog {
  Q_OBJECT
public:
  ExportDialog(const ArtifactDoc &doc, int slot, const PlateOptions &screenOpts,
               ExportFormat fmt, QWidget *parent = nullptr);
  QString savedPath() const { return saved_; }

private:
  void sync();
  void save();
  void print();
  ExportRequest request() const;
  QWidget *chipBar(const QStringList &items, int sel,
                   std::function<void(int)> on, QList<QPushButton *> *out);

  const ArtifactDoc &doc_;
  int slot_;
  PlateOptions base_;
  ExportFormat fmt_;
  double scale_ = 1.0;
  QLineEdit *name_;
  QComboBox *paper_;
  QCheckBox *cSec_, *cOut_, *cShade_, *cGrid_;
  QCheckBox *c14Layers_ = nullptr, *cCalib100_ = nullptr;
  QLabel *lblVerified_ = nullptr;
  QList<QPushButton *> scaleChips_, fmtChips_;
  PaperPreview *preview_;
  QLabel *previewHead_;
  QLabel *advice_;
  QLabel *note_;
  QPushButton *saveBtn_;
  QString saved_;
};

} // namespace studio
