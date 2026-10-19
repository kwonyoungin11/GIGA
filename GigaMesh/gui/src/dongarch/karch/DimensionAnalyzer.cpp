/* DongArch3D / K-ArchProfile T6 dimensions (GPLv3+) */

#include "DimensionAnalyzer.h"
#include "AnnulusClassifier.h"
#include "MeshSlicer.h"
#include "RimDetector.h"

#include <algorithm>
#include <chrono>
#include <cmath>
#include <fstream>
#include <iomanip>
#include <locale>
#include <numbers>
#include <string>
#include <vector>

namespace DongArch {
namespace KArch {
namespace {

constexpr double kGapDeg = 20.0;

std::vector<Point2> keepOuter(const std::vector<Point2> &xy,
                              double bandMm = 1.5) {
  if (xy.empty()) {
    return {};
  }
  double rMax = 0.0;
  std::vector<double> rs;
  rs.reserve(xy.size());
  for (const Point2 &p : xy) {
    const double r = std::hypot(p.x, p.y);
    rs.push_back(r);
    rMax = std::max(rMax, r);
  }
  std::vector<Point2> out;
  for (std::size_t i = 0; i < xy.size(); ++i) {
    if (rs[i] >= rMax - bandMm) {
      out.push_back(xy[i]);
    }
  }
  return out;
}

void fillArcFromFit(ArcSpec &arc, bool &isFragment, const Circle2D &c,
                    const std::vector<Point2> &xy, double centerZ) {
  arc.radius = c.r;
  arc.center = Point2{0.0, centerZ};
  arc.startAngleDeg = 0.0;
  arc.endAngleDeg = 360.0;
  arc.isRestored = false;
  isFragment = false;
  if (!c.ok || xy.size() < 3) {
    return;
  }

  std::vector<double> ang;
  ang.reserve(xy.size());
  for (const Point2 &p : xy) {
    ang.push_back(std::atan2(p.y - c.yc, p.x - c.xc));
  }
  std::sort(ang.begin(), ang.end());
  double maxGap = (ang.front() + 2.0 * std::numbers::pi) - ang.back();
  std::size_t gapAfter = ang.size() - 1;
  for (std::size_t i = 0; i + 1 < ang.size(); ++i) {
    const double g = ang[i + 1] - ang[i];
    if (g > maxGap) {
      maxGap = g;
      gapAfter = i;
    }
  }
  const double gapDeg = maxGap * 180.0 / std::numbers::pi;
  if (gapDeg > kGapDeg) {
    isFragment = true;
    arc.isRestored = true;
    const double a0 = ang[(gapAfter + 1) % ang.size()];
    const double a1 = ang[gapAfter];
    arc.startAngleDeg = a0 * 180.0 / std::numbers::pi;
    arc.endAngleDeg = a1 * 180.0 / std::numbers::pi;
    if (arc.endAngleDeg < arc.startAngleDeg) {
      arc.endAngleDeg += 360.0;
    }
  }
}

std::vector<Point2> rimCandidateXy(const MeshData &mesh, double zMax) {
  const auto rim = DetectRim(mesh, RimMode::Auto);
  if (rim.error == RimError::Ok && rim.rimPoints.size() >= 20) {
    double rzMin = rim.rimPoints[0].z;
    double rzMax = rzMin;
    for (const Point3 &p : rim.rimPoints) {
      rzMin = std::min(rzMin, p.z);
      rzMax = std::max(rzMax, p.z);
    }
    if (rzMax - rzMin <= 3.0 && rim.meanZ >= zMax - 2.0) {
      std::vector<Point2> xy;
      xy.reserve(rim.rimPoints.size());
      for (const Point3 &p : rim.rimPoints) {
        xy.push_back(Point2{p.x, p.y});
      }
      return keepOuter(xy);
    }
  }

  std::vector<Point2> xy;
  for (const Point3 &p : mesh.vertices) {
    if (p.z >= zMax - 0.5) {
      xy.push_back(Point2{p.x, p.y});
    }
  }
  return keepOuter(xy);
}

std::vector<Point2> allSliceXy(const std::vector<Polyline2> &loops) {
  std::vector<Point2> xy;
  for (const auto &l : loops) {
    xy.insert(xy.end(), l.pts.begin(), l.pts.end());
  }
  return keepOuter(xy);
}

} // namespace

double RoundTenths(double mm) {
  const double t = mm * 10.0;
  const double bump = (t >= 0.0) ? 1e-9 : -1e-9;
  return std::round(t + bump) / 10.0;
}

Dimensions AnalyzeDimensions(const MeshData &alignedMesh,
                             const SilhouetteResult &silhouette) {
  Dimensions d;
  if (alignedMesh.vertices.empty()) {
    d.warning = "empty mesh";
    return d;
  }

  double zMin = alignedMesh.vertices[0].z;
  double zMax = zMin;
  for (const Point3 &p : alignedMesh.vertices) {
    zMin = std::min(zMin, p.z);
    zMax = std::max(zMax, p.z);
  }
  const double heightRaw = zMax - zMin;

  const auto rimXy = rimCandidateXy(alignedMesh, zMax);
  if (rimXy.size() >= 20) {
    const Circle2D c = FitCircleTaubin(rimXy);
    if (c.ok) {
      d.rimDiameter = 2.0 * c.r;
      fillArcFromFit(d.rimArc, d.rimIsFragment, c, rimXy, zMax + c.r + 15.0);
    }
  }

  std::vector<Point2> baseBand;
  for (const Point3 &p : alignedMesh.vertices) {
    if (p.z <= zMin + 0.5) {
      baseBand.push_back(Point2{p.x, p.y});
    }
  }
  std::vector<Point2> baseXy = keepOuter(baseBand);
  Circle2D baseC;
  if (baseXy.size() >= 20) {
    baseC = FitCircleTaubin(baseXy);
  }
  if (!baseC.ok) {
    const double zBase = zMin + 0.5;
    const auto loops3 = Slice(alignedMesh, Plane{{0.0, 0.0, 1.0}, -zBase});
    std::vector<Polyline2> loops2;
    for (const auto &pl : loops3) {
      Polyline2 l;
      for (const Point3 &p : pl.pts) {
        l.pts.push_back(Point2{p.x, p.y});
      }
      if (!l.pts.empty()) {
        loops2.push_back(std::move(l));
      }
    }
    const auto ann = ClassifyAndFitOuter(loops2);
    if (ann.ok) {
      baseC = ann.outerCircle;
      if (ann.outerIndex >= 0) {
        baseXy =
            keepOuter(loops2[static_cast<std::size_t>(ann.outerIndex)].pts);
      }
    } else {
      baseXy = allSliceXy(loops2);
      if (baseXy.size() >= 20) {
        baseC = FitCircleTaubin(baseXy);
      }
    }
  }
  if (baseC.ok) {
    d.baseDiameter = 2.0 * baseC.r;
    fillArcFromFit(d.baseArc, d.baseIsFragment, baseC, baseXy,
                   zMin - baseC.r - 15.0);
  }

  double rMax = 0.0;
  for (const Point3 &p : alignedMesh.vertices) {
    rMax = std::max(rMax, std::hypot(p.x, p.y));
  }
  double zSum = 0.0;
  int zCount = 0;
  for (const Point3 &p : alignedMesh.vertices) {
    if (std::hypot(p.x, p.y) >= rMax - 1e-9) {
      zSum += p.z;
      ++zCount;
    }
  }
  const double maxBodyRaw = 2.0 * rMax;
  const double maxBodyZRaw = zCount > 0
                                 ? zSum / static_cast<double>(zCount)
                                 : (silhouette.ok ? silhouette.maxBodyZ : 0.0);

  if (d.rimIsFragment) {
    d.warning = "RIM_FRAGMENT";
  }

  d.zMin = zMin;
  d.zMax = zMax;
  d.vesselHeight = RoundTenths(heightRaw);
  d.rimDiameter = RoundTenths(d.rimDiameter);
  d.baseDiameter = RoundTenths(d.baseDiameter);
  d.maxBodyDiameter = RoundTenths(maxBodyRaw);
  d.maxBodyZ = maxBodyZRaw;

  d.ok = d.rimDiameter > 0.0 && d.baseDiameter > 0.0 && d.vesselHeight > 0.0 &&
         d.maxBodyDiameter > 0.0;
  if (!d.ok && d.warning.empty()) {
    d.warning = "dimension fit failed";
  }
  return d;
}

static void svgPoly(std::ostream &out, const char *tag,
                    const std::vector<Point2> &pts, const char *extra) {
  out << '<' << tag << ' ' << extra << " points=\"";
  for (const Point2 &q : pts) {
    out << q.x << ',' << q.y << ' ';
  }
  out << "\"/>\n";
}

bool WritePlateSvg(const std::string &path,
                   const std::vector<Point2> &leftOutline,
                   const SectionResult &section, const Dimensions &dims) {
  std::ofstream out(path, std::ios::out | std::ios::binary);
  if (!out) {
    return false;
  }
  out.imbue(std::locale::classic());
  out << std::setprecision(6);
  const double top = dims.rimArc.center.y + dims.rimArc.radius + 12.0;
  const double bot = dims.baseArc.center.y - dims.baseArc.radius - 12.0;
  const double h = std::max(top - bot, 1.0);
  out << "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";
  out << "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"-90 " << (-top)
      << " 180 " << h << "\">\n";
  out << "<g transform=\"scale(1,-1)\">\n";

  if (!leftOutline.empty()) {
    svgPoly(out, "polyline", leftOutline,
            "fill=\"none\" stroke=\"#111\" stroke-width=\"0.25\"");
  }
  for (const auto &poly : section.wallPolygons) {
    svgPoly(out, "polygon", poly,
            "fill=\"#f4e6c3\" stroke=\"#111\" stroke-width=\"0.25\"");
  }
  for (const auto &ln : section.hatchLines) {
    out << "<line x1=\"" << ln.first.x << "\" y1=\"" << ln.first.y << "\" x2=\""
        << ln.second.x << "\" y2=\"" << ln.second.y
        << "\" stroke=\"#444\" stroke-width=\"0.15\"/>\n";
  }

  auto circle = [&](const ArcSpec &a, const char *stroke) {
    if (a.radius <= 0.0) {
      return;
    }
    out << "<circle cx=\"" << a.center.x << "\" cy=\"" << a.center.y
        << "\" r=\"" << a.radius << "\" fill=\"none\" stroke=\"" << stroke
        << "\" stroke-width=\"0.25\"";
    if (a.isRestored) {
      out << " stroke-dasharray=\"1.2 0.8\"";
    }
    out << "/>\n";
  };
  circle(dims.rimArc, "#1a5");
  circle(dims.baseArc, "#1a5");

  auto label = [&](double x, double z, const char *name, double valueMm) {
    out << "<text transform=\"translate(" << x << "," << z << ") scale(1,-1)\" "
        << "font-size=\"3.2\" font-family=\"sans-serif\" fill=\"#111\">" << name
        << ' ' << std::fixed << std::setprecision(1) << RoundTenths(valueMm)
        << "</text>\n";
  };
  label(-78.0, dims.zMin + dims.vesselHeight * 0.55, "기고", dims.vesselHeight);
  label(-78.0, dims.maxBodyZ, "최대동체경", dims.maxBodyDiameter);
  label(8.0, dims.rimArc.center.y, "구연경", dims.rimDiameter);
  label(8.0, dims.baseArc.center.y, "저경", dims.baseDiameter);

  out << "<line x1=\"-62\" y1=\"" << dims.zMin << "\" x2=\"-62\" y2=\""
      << dims.zMax << "\" stroke=\"#c00\" stroke-width=\"0.2\"/>\n";
  out << "</g></svg>\n";
  return static_cast<bool>(out);
}

} // namespace KArch
} // namespace DongArch
