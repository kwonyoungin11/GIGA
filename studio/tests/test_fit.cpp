// 헤드리스 검증: 단면·외곽선을 기준선에 맞추기 (JOB-008 수용 기준 1-8·23·24, 판정 꼬리표 [J8-n])
// 그린 단면·외곽선 = 원 선에서 「간격」의 절반 안(편차 제한 곡선 fitCurve — 곡선 점은 원 선의 높이 범위·축 쪽 안),
// 외곽선의 원 선 = 막과 수직인 실루엣의 외면 반쪽(원본 메시·평활 없음 — 시험 쪽 독립 계산 rawHalfOrtho 와 같은지),
// 최대 편차 함수, 출력 길(DXF·래스터)과 파이썬 독립 판정용 파일(SVG·DXF·CSV).
// 허용오차: 같은 식으로 옮긴 도면 mm 1e-9 · DXF 소수 6자리 ±2e-6 · 편차 상한 = 간격/2(새 매개변수 없음) ·
// 「틈 ≤ 0.30 mm」(단면)·「≤ 0.25 mm」(외곽선 = 칸 크기)는 수평 예제 실측 성질의 회귀 판정(끌어 붙임 없음).
// 시험 쪽 도움 함수(거리·편차·반쪽 정사 실루엣)는 제품 코드를 쓰지 않고 이 파일 안에 따로 둔다.
// 출력 폴더 = argv[1] — 파이썬 독립 판정(명세 §5-3)이 여기 남긴 SVG·DXF·CSV 를 다시 읽는다.
#include "ArtifactDoc.h"
#include "MeshLoader.h"
#include "Plate.h"
#include "SyntheticMeshGen.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QGuiApplication>
#include <QImage>
#include <QPainter>
#include <QStringList>
#include <algorithm>
#include <array>
#include <cmath>
#include <cstdio>
#include <functional>
#include <limits>
#include <numbers>
#include <utility>
#include <vector>

using namespace studio;
using P2 = KA::Point2;
using Line = std::vector<P2>;
using Lines = std::vector<Line>;

static int fails = 0;
#define CHECK(c, ...)                         \
    do {                                      \
        if (!(c)) {                           \
            std::printf("FAIL %s: ", #c);     \
            std::printf(__VA_ARGS__);         \
            std::printf("\n");                \
            ++fails;                          \
        }                                     \
    } while (0)

static constexpr double kInf = std::numeric_limits<double>::infinity();
static double meshH(const ArtifactDoc& d) { return d.mesh().bboxMax.z - d.mesh().bboxMin.z; }

// ---- 시험 쪽 거리·편차 (§3.3 과 같은 정의) ------------------------------------------
static double segDist(const P2& p, const P2& a, const P2& b) {
    const double dx = b.x - a.x, dy = b.y - a.y, L2 = dx * dx + dy * dy;
    const double t = L2 > 0 ? std::clamp(((p.x - a.x) * dx + (p.y - a.y) * dy) / L2, 0.0, 1.0) : 0.0;
    return std::hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy));
}
// 점 → 꺾은선 최단 거리 (닫힘이면 끝→처음 변 포함, 빈 선 0, 점 1개면 그 점까지)
static double lineDist(const P2& p, const Line& l, bool closed) {
    if (l.empty()) return 0.0;
    double d = std::hypot(p.x - l[0].x, p.y - l[0].y);
    const size_t n = l.size();
    for (size_t i = 0, s = closed ? n : n - 1; i < s && n > 1; ++i) d = std::min(d, segDist(p, l[i], l[(i + 1) % n]));
    return d;
}
// 꼭짓점 기준 양방향: 그린 꼭짓점 → 원 꺾은선, 원 꼭짓점 → 그린 꺾은선
static double dev2(const Line& raw, const Line& drawn, bool closed) {
    double d = 0.0;
    for (const auto& p : drawn) d = std::max(d, lineDist(p, raw, closed));
    for (const auto& p : raw) d = std::max(d, lineDist(p, drawn, closed));
    return d;
}

// ---- 시험 쪽 반쪽 정사 실루엣 --------------------------------------------------------
// 칸 zk = bboxMin.z + (k + 0.5)·0.25 ≤ bboxMax.z 마다 삼각형 세 변 u→v 의 수평면 교점(K-Arch addTriZHits
// 171-189 와 같은 규칙) 중 x ≤ 0 인 것의 최솟값. 맞는 점이 없는 칸은 건너뛴다.
static constexpr double kBin = 0.25;
static double binZ(const KA::MeshData& m, long k) { return m.bboxMin.z + (double(k) + 0.5) * kBin; }
static void edgeHits(const KA::Point3* p, double z, double& mn) {
    for (int e = 0; e < 3; ++e) {
        const KA::Point3& u = p[e];
        const KA::Point3& v = p[(e + 1) % 3];
        if (std::fabs(u.z - z) <= 1e-12 && u.x <= 0.0) mn = std::min(mn, u.x);
        const double dz = v.z - u.z;
        if (std::fabs(dz) < 1e-15) continue;
        const double t = (z - u.z) / dz;
        if (t > 0.0 && t <= 1.0) {
            const double x = u.x + t * (v.x - u.x);
            if (x <= 0.0) mn = std::min(mn, x);
        }
    }
}
static bool face3(const KA::MeshData& m, const std::array<int, 3>& f, KA::Point3 (&p)[3]) {
    if (f[0] < 0 || f[1] < 0 || f[2] < 0) return false;
    for (int i = 0; i < 3; ++i) p[i] = m.vertices[size_t(f[size_t(i)])];
    return true;
}
// 면마다 그 면의 z 범위에 걸치는 칸만 (±1 칸 여유 — 판정은 교점 규칙)
static Line rawHalfOrtho(const KA::MeshData& m) {
    Line out;
    if (m.vertices.empty() || m.faces.empty()) return out;
    long n = 0;
    while (binZ(m, n) <= m.bboxMax.z) ++n;
    std::vector<double> best(size_t(n), kInf);
    KA::Point3 p[3];
    for (const auto& f : m.faces) {
        if (!face3(m, f, p)) continue;
        const double lo = std::min({p[0].z, p[1].z, p[2].z}), hi = std::max({p[0].z, p[1].z, p[2].z});
        const long k0 = std::max(0L, long(std::floor((lo - m.bboxMin.z) / kBin - 0.5)) - 1);
        const long k1 = std::min(n - 1, long(std::ceil((hi - m.bboxMin.z) / kBin - 0.5)) + 1);
        for (long k = k0; k <= k1; ++k) edgeHits(p, binZ(m, k), best[size_t(k)]);
    }
    for (long k = 0; k < n; ++k)
        if (best[size_t(k)] < kInf) out.push_back({best[size_t(k)], binZ(m, k)});
    return out;
}
// 전 칸 × 전 면 (rawHalfOrtho 자체 확인용)
static Line rawHalfOrthoAll(const KA::MeshData& m) {
    Line out;
    KA::Point3 p[3];
    for (long k = 0; binZ(m, k) <= m.bboxMax.z; ++k) {
        const double z = binZ(m, k);
        double mn = kInf;
        for (const auto& f : m.faces)
            if (face3(m, f, p)) edgeHits(p, z, mn);
        if (mn < kInf) out.push_back({mn, z});
    }
    return out;
}
static double maxDiff(const Line& a, const Line& b) {  // 점 수가 다르면 ∞
    if (a.size() != b.size()) return kInf;
    double d = 0.0;
    for (size_t i = 0; i < a.size(); ++i) d = std::max({d, std::fabs(a[i].x - b[i].x), std::fabs(a[i].y - b[i].y)});
    return d;
}
static bool sameLine(const Line& a, const Line& b) {  // 꼭짓점마다 ==
    if (a.size() != b.size()) return false;
    for (size_t i = 0; i < a.size(); ++i)
        if (a[i].x != b[i].x || a[i].y != b[i].y) return false;
    return true;
}

// ---- 단면 짝 (§3.7): 원 선 = 문서가 그린 그 원 선 sectionLines() --------------------------
struct SecPair {
    Line raw, drawn;
    bool closed;
};
static bool blindPath(const SectionLines& s) { return !s.solids.empty() && !s.estInner.empty(); }
// 미스캔 보정이면 (solids[i] ↔ displayScannedSolid()[i]) · (estInner[i] ↔ displayEstimatedInner()[i]) 열림,
// 아니면 (loops[i] ↔ displaySection()[i]) 닫힘. countsOk = 고리·선 수가 같은지
static std::vector<SecPair> sectionPairs(const ArtifactDoc& d, bool& countsOk) {
    const SectionLines& s = d.sectionLines();
    std::vector<SecPair> v;
    countsOk = true;
    auto add = [&](const Lines& raw, const Lines& drawn, bool closed) {
        countsOk = countsOk && raw.size() == drawn.size();
        for (size_t i = 0; i < std::min(raw.size(), drawn.size()); ++i) v.push_back({raw[i], drawn[i], closed});
    };
    if (blindPath(s)) {
        add(s.solids, d.displayScannedSolid(), false);
        add(s.estInner, d.displayEstimatedInner(), false);
    } else {
        add(s.loops, d.displaySection(), true);
    }
    return v;
}
static double sectionDev(const ArtifactDoc& d, bool& countsOk) {
    double m = 0.0;
    for (const auto& p : sectionPairs(d, countsOk)) m = std::max(m, dev2(p.raw, p.drawn, p.closed));
    return m;
}
// 원 단면 선들의 y 범위 [rLo, rHi] 와 그린 단면(displaySection)이 그 안·x ≥ 0 인지
struct SecBand {
    double rLo = kInf, rHi = -kInf, lo = kInf, hi = -kInf, minX = kInf;
    bool inBand = true;
};
static SecBand sectionBand(const ArtifactDoc& d) {
    SecBand b;
    const SectionLines& s = d.sectionLines();
    const std::vector<const Lines*> raws =
        blindPath(s) ? std::vector<const Lines*>{&s.solids, &s.estInner} : std::vector<const Lines*>{&s.loops};
    for (const Lines* ls : raws)
        for (const auto& l : *ls)
            for (const auto& q : l) {
                b.rLo = std::min(b.rLo, q.y);
                b.rHi = std::max(b.rHi, q.y);
            }
    for (const auto& poly : d.displaySection())
        for (const auto& q : poly) {
            b.lo = std::min(b.lo, q.y);
            b.hi = std::max(b.hi, q.y);
            b.minX = std::min(b.minX, q.x);
            b.inBand = b.inBand && q.y >= b.rLo - 1e-9 && q.y <= b.rHi + 1e-9 && q.x >= -1e-9;
        }
    return b;
}

// ---- 기준 1·3 (한 상태, 수평 예제) -----------------------------------------------------
static void checkBand(const char* name, const ArtifactDoc& d) {
    const double H = meshH(d);
    CHECK(std::fabs(d.mesh().bboxMin.z) <= 1e-9, "[J8-1] %s bboxMin.z %.12f", name, d.mesh().bboxMin.z);
    const SecBand b = sectionBand(d);
    CHECK(b.rLo >= -1e-9 && b.rHi <= H + 1e-9, "[J8-1] %s raw y [%.6f, %.6f] H %.6f", name, b.rLo, b.rHi, H);
    CHECK(b.inBand && b.lo <= b.hi, "[J8-1] %s drawn y [%.6f, %.6f] minX %.6f outside raw [%.6f, %.6f]", name, b.lo, b.hi, b.minX,
          b.rLo, b.rHi);
    CHECK(b.lo <= 0.30 && H - b.hi <= 0.30, "[J8-1] %s gap %.4f/%.4f", name, b.lo, H - b.hi);
    std::printf("[J8-1] %s y=[%.3f, %.3f] raw=[%.3f, %.3f] H=%.3f gap %.3f/%.3f\n", name, b.lo, b.hi, b.rLo, b.rHi, H, b.lo, H - b.hi);

    const Line ol = d.displayOutline();
    if (ol.empty()) {
        CHECK(false, "[J8-3] %s outline empty", name);
        return;
    }
    double lo = kInf, hi = -kInf;
    bool band = true;
    for (const auto& q : ol) {
        lo = std::min(lo, q.y);
        hi = std::max(hi, q.y);
        band = band && q.y >= -1e-9 && q.y <= H + 1e-9 && q.x <= 1e-9;
    }
    CHECK(band, "[J8-3] %s outline outside band y [%.6f, %.6f]", name, lo, hi);
    CHECK(ol.front().y == lo && ol.back().y == hi, "[J8-3] %s ends not extreme: front %.6f back %.6f lo %.6f hi %.6f", name,
          ol.front().y, ol.back().y, lo, hi);
    CHECK(ol.front().y <= 0.25 && H - ol.back().y <= 0.25, "[J8-3] %s gap %.4f/%.4f", name, ol.front().y, H - ol.back().y);
    std::printf("[J8-3] %s ends=(%.3f,%.3f)…(%.3f,%.3f) gap %.3f/%.3f · |x|−base/2 %.3f · |x|−rim/2 %.3f\n", name, ol.front().x,
                ol.front().y, ol.back().x, ol.back().y, ol.front().y, H - ol.back().y, std::fabs(ol.front().x) - d.dims().base / 2,
                std::fabs(ol.back().x) - d.dims().rim / 2);
}

// ---- 기준 2·4·6 (한 상태) --------------------------------------------------------------
static void checkDev(const char* name, const ArtifactDoc& d) {
    const double tol = d.simplifyTol();
    bool cnt = false;
    const double sd = sectionDev(d, cnt);
    CHECK(cnt, "[J8-2] %s 단면 짝 수가 다름", name);
    CHECK(sd <= tol / 2 + 1e-9, "[J8-2] %s tol %.2f section dev %.6f", name, tol, sd);
    std::printf("[J8-2] %s tol=%.1f dev=%.3f\n", name, tol, sd);

    const Line& o = d.orthoOutline();
    bool ok = !o.empty();
    for (size_t i = 0; i < o.size(); ++i)
        ok = ok && o[i].x <= 0 && o[i].y >= d.mesh().bboxMin.z && o[i].y <= d.mesh().bboxMax.z && (i == 0 || o[i].y > o[i - 1].y);
    CHECK(ok, "[J8-4] %s orthoOutline n=%zu not x≤0 / in z range / increasing", name, o.size());
    const double od = dev2(o, d.displayOutline(), false);
    CHECK(od <= tol / 2 + 1e-9, "[J8-4] %s tol %.2f outline dev %.6f", name, tol, od);
    std::printf("[J8-4] %s tol=%.1f n=%zu dev=%.4f\n", name, tol, o.size(), od);

    const double md = d.maxDeviationMm();
    CHECK(std::fabs(md - std::max(sd, od)) <= 1e-9 && md <= tol / 2 + 1e-9, "[J8-6] %s maxDeviationMm %.9f vs max(%.9f, %.9f)", name,
          md, sd, od);
    std::printf("[J8-6] %s tol=%.1f maxDeviationMm=%.4f = max(section %.4f, outline %.4f)\n", name, tol, md, sd, od);
}

// ---- 기준 5 · 23(a) · 24: 시험 쪽 독립 반쪽 정사 실루엣과 같은지 --------------------------------
static double checkOrtho(const char* tag, const char* name, const ArtifactDoc& d) {
    const Line r = rawHalfOrtho(d.mesh());
    const double md = maxDiff(r, d.orthoOutline());
    CHECK(md <= 1e-9, "%s %s rawHalfOrtho n=%zu vs orthoOutline n=%zu maxdiff %.3g", tag, name, r.size(), d.orthoOutline().size(), md);
    return md;
}

// ---- DXF ------------------------------------------------------------------------------------
struct DxfRec {
    QString type;
    std::vector<std::pair<int, QString>> kv;
    QString get(int c) const {
        for (const auto& [k, v] : kv)
            if (k == c) return v;
        return {};
    }
};
static std::vector<DxfRec> readDxf(const QString& path) {
    std::vector<DxfRec> out;
    QFile f(path);
    if (!f.open(QIODevice::ReadOnly)) {
        CHECK(false, "[J8-8] open %s", qUtf8Printable(path));
        return out;
    }
    const QStringList lines = QString::fromLatin1(f.readAll()).split(QLatin1Char('\n'));
    for (qsizetype i = 0; i + 1 < lines.size(); i += 2) {
        const int code = lines[i].trimmed().toInt();
        const QString val = lines[i + 1].trimmed();
        if (code == 0) out.push_back({val, {}});
        else if (!out.empty()) out.back().kv.push_back({code, val});
    }
    return out;
}
// 기준 8(a): 단면(04·06)·외곽선(02) POLYLINE 꼭짓점이 바닥선 y ~ 구연선 y 띠 안, 단면 x ≥ 0 · 외곽선 x ≤ 0, 외곽선 끝 틈 ≤ 0.25
static void checkDxfBand(const char* name, const QString& path) {
    const auto rs = readDxf(path);
    double y07 = std::numeric_limits<double>::quiet_NaN(), y08 = y07;
    std::vector<std::pair<QString, Line>> polys;
    long cur = -1;
    for (const auto& r : rs) {
        if (r.type == QLatin1String("LINE") && r.get(8) == QLatin1String("07_PROJECTION_RIM")) y07 = r.get(20).toDouble();
        else if (r.type == QLatin1String("LINE") && r.get(8) == QLatin1String("08_PROJECTION_BASE")) y08 = r.get(20).toDouble();
        else if (r.type == QLatin1String("POLYLINE")) {
            polys.push_back({r.get(8), {}});
            cur = long(polys.size()) - 1;
        } else if (r.type == QLatin1String("VERTEX") && cur >= 0)
            polys[size_t(cur)].second.push_back({r.get(10).toDouble(), r.get(20).toDouble()});
        else if (r.type == QLatin1String("SEQEND"))
            cur = -1;
    }
    CHECK(std::isfinite(y07) && std::isfinite(y08), "[J8-8] %s rim/base LINE missing", name);
    int nSec = 0, nOl = 0;
    bool secOk = true, olOk = true;
    double sLo = kInf, sHi = -kInf, oLo = kInf, oHi = -kInf;
    for (const auto& pl : polys) {
        const bool sec = pl.first == QLatin1String("04_SECTION_WALL") || pl.first == QLatin1String("06_INTERIOR_OUTLINE");
        const bool ol = pl.first == QLatin1String("02_EXTERIOR_OUTLINE");
        if (!sec && !ol) continue;
        (sec ? nSec : nOl) += 1;
        for (const auto& q : pl.second) {
            const bool band = q.y >= y08 - 2e-6 && q.y <= y07 + 2e-6;
            if (sec) {
                secOk = secOk && band && q.x >= -2e-6;
                sLo = std::min(sLo, q.y);
                sHi = std::max(sHi, q.y);
            } else {
                olOk = olOk && band && q.x <= 2e-6;
                oLo = std::min(oLo, q.y);
                oHi = std::max(oHi, q.y);
            }
        }
    }
    CHECK(nSec >= 1 && secOk, "[J8-8] %s section polylines %d y [%.6f, %.6f] base %.6f rim %.6f", name, nSec, sLo, sHi, y08, y07);
    CHECK(nOl == 1 && olOk && oLo - y08 <= 0.25 + 2e-6 && y07 - oHi <= 0.25 + 2e-6,
          "[J8-8] %s outline polylines %d y [%.6f, %.6f] base %.6f rim %.6f", name, nOl, oLo, oHi, y08, y07);
    std::printf("[J8-8] %s dxf base y %.6f rim y %.6f · section %d polyline y [%.6f, %.6f] · outline y [%.6f, %.6f] gap %.3f/%.3f\n",
                name, y08, y07, nSec, sLo, sHi, oLo, oHi, oLo - y08, y07 - oHi);
}

// ---- 래스터 (PDF·PNG 공용 paintSheet), A4 세로 1:1, 8 px/mm -------------------------------------
static constexpr double kPpm = 8.0;
struct Sheet {
    QImage img;
    double X0 = 0, Yb = 0;
    QRectF E;  // 도판 칸 (도면 mm, y 위로 +: [E.top(), E.bottom()] = [−8, H + 18])
};
static Sheet drawSheet(const ArtifactDoc& d, bool section, bool outline) {
    ExportRequest r;
    r.opts.grid = r.opts.shade = r.opts.labels = false;
    r.opts.section = section;
    r.opts.outline = outline;
    const PaperLayout L = layoutPaper(d, 0, r.paper, r.scale);
    Sheet sh;
    sh.E = Plate::extentMm(d, 0);
    sh.img = QImage(int(L.paperMm.width() * kPpm), int(L.paperMm.height() * kPpm), QImage::Format_RGB32);
    sh.img.fill(Qt::white);
    {
        QPainter p(&sh.img);
        paintSheet(p, d, r, kPpm);
    }
    sh.X0 = L.plateRectMm.left() - sh.E.left() * r.scale;
    sh.Yb = L.plateRectMm.top() + sh.E.bottom() * r.scale;
    return sh;
}
// 도판 칸 E 안의 화소(화소 중심의 도면 좌표 x, y) 중 in(x, y) 이고 pred(색) 인 것의 수. 도면점 (x,y) → 화소 ((X0 + x)·8, (Yb − y)·8)
static int countPx(const Sheet& sh, const std::function<bool(double, double)>& in, const std::function<bool(QRgb)>& pred) {
    int n = 0;
    for (int py = 0; py < sh.img.height(); ++py) {
        const double y = sh.Yb - (py + 0.5) / kPpm;
        if (y < sh.E.top() || y > sh.E.bottom()) continue;
        const QRgb* row = reinterpret_cast<const QRgb*>(sh.img.constScanLine(py));
        for (int px = 0; px < sh.img.width(); ++px) {
            const double x = (px + 0.5) / kPpm - sh.X0;
            if (x < sh.E.left() || x > sh.E.right() || !in(x, y)) continue;
            if (pred(row[px])) ++n;
        }
    }
    return n;
}

// ---- 파이썬 판정용 원 자료 CSV: kind,idx,x,y (sec = loops, solid, est, ortho = rawHalfOrtho) ---------------------
static void writeRawCsv(const QString& path, const ArtifactDoc& d) {
    QByteArray b("kind,idx,x,y\n");
    char buf[128];
    auto put = [&](const char* kind, const Lines& ls) {
        for (size_t i = 0; i < ls.size(); ++i)
            for (const auto& q : ls[i]) {
                std::snprintf(buf, sizeof buf, "%s,%zu,%.9f,%.9f\n", kind, i, q.x, q.y);
                b += buf;
            }
    };
    const SectionLines& s = d.sectionLines();
    put("sec", s.loops);
    put("solid", s.solids);
    put("est", s.estInner);
    put("ortho", Lines{rawHalfOrtho(d.mesh())});
    QFile f(path);
    CHECK(f.open(QIODevice::WriteOnly | QIODevice::Truncate) && f.write(b) == b.size(), "[J8-8] write %s", qUtf8Printable(path));
}

// ---- 기운 예제 (§3.7): 아래 30 mm(바닥)·위 30 mm(구연)만 s = x(앞쪽 높음이면 s = −y) 쪽으로 기울인다 ---------------
static void tiltMesh(KA::MeshData& m, double baseDeg, double rimDeg, bool alongY) {
    const double H = m.bboxMax.z;
    for (auto& p : m.vertices) {
        const double s = alongY ? -p.y : p.x;
        p.z += std::tan(baseDeg * std::numbers::pi / 180) * s * std::max(0.0, 1 - p.z / 30.0);
        p.z += std::tan(rimDeg * std::numbers::pi / 180) * s * std::max(0.0, (p.z - (H - 30)) / 30.0);
    }
}
// 기준 23: 외곽선이 중심선을 넘지 않고 원 실루엣의 첫·끝 칸에서 끝남(끝 틈은 실측대로 — 판정 안 함)
static void checkTilt(const char* name, const KA::MeshData& m0, double baseDeg, double rimDeg, bool alongY) {
    KA::MeshData m = m0;
    tiltMesh(m, baseDeg, rimDeg, alongY);
    ArtifactDoc t;
    t.loadMesh(std::move(m), {}, QStringLiteral("기운 예제"));
    t.confirmPose();
    const double tol = t.simplifyTol(), H = meshH(t);
    const double md = checkOrtho("[J8-23]", name, t);  // (a)
    const Line& o = t.orthoOutline();
    const Line ol = t.displayOutline();
    if (o.empty() || ol.empty()) {
        CHECK(false, "[J8-23] %s outline empty", name);
        return;
    }
    double maxX = -kInf;
    bool inY = true;
    for (const auto& q : ol) {
        maxX = std::max(maxX, q.x);
        inY = inY && q.y >= ol.front().y && q.y <= ol.back().y;
    }
    CHECK(maxX <= 1e-9 && inY && ol.front().x == o.front().x && ol.front().y == o.front().y && ol.back().x == o.back().x &&
              ol.back().y == o.back().y,
          "[J8-23] %s (b) maxX %.6f inY %d ends (%.6f,%.6f)…(%.6f,%.6f) vs raw (%.6f,%.6f)…(%.6f,%.6f)", name, maxX, int(inY), ol.front().x,
          ol.front().y, ol.back().x, ol.back().y, o.front().x, o.front().y, o.back().x, o.back().y);  // (b)
    const double od = dev2(o, ol, false);
    CHECK(od <= tol / 2 + 1e-9, "[J8-23] %s (c) outline dev %.6f", name, od);  // (c)
    const SecBand b = sectionBand(t);
    bool cnt = false;
    const double sd = sectionDev(t, cnt);
    CHECK(b.inBand && cnt && sd <= tol / 2 + 1e-9, "[J8-23] %s (d) section y [%.6f, %.6f] raw [%.6f, %.6f] minX %.6f pairs %d dev %.6f",
          name, b.lo, b.hi, b.rLo, b.rHi, b.minX, int(cnt), sd);  // (d)
    const double mx = t.maxDeviationMm();
    CHECK(mx <= tol / 2 + 1e-9, "[J8-23] %s (e) maxDeviationMm %.6f", name, mx);  // (e)
    std::printf("[J8-23] %s H=%.3f n=%zu ends=(%.3f,%.3f)…(%.3f,%.3f) gap %.3f/%.3f maxX=%.3f olDev=%.3f secDev=%.3f maxDev=%.3f "
                "(section y [%.3f, %.3f], maxdiff %.3g)\n",
                name, H, o.size(), ol.front().x, ol.front().y, ol.back().x, ol.back().y, ol.front().y - t.mesh().bboxMin.z,
                t.mesh().bboxMax.z - ol.back().y, maxX, od, sd, mx, b.lo, b.hi, md);
}

int main(int argc, char** argv) {
    std::setvbuf(stdout, nullptr, _IONBF, 0);
    qputenv("QT_QPA_PLATFORM", "offscreen");
    QGuiApplication app(argc, argv);
    const QString out = argc > 1 ? QString::fromLocal8Bit(argv[1]) : QDir::currentPath();
    auto P = [&](const char* n) { return out + QLatin1Char('/') + QLatin1String(n); };
    QString err;

    // 기준 6: 미로드 · 확정 전 = 0
    {
        ArtifactDoc e;
        ArtifactDoc u;
        u.loadSample(false);
        CHECK(e.maxDeviationMm() == 0.0 && u.maxDeviationMm() == 0.0, "[J8-6] empty %.6f unconfirmed %.6f", e.maxDeviationMm(),
              u.maxDeviationMm());
        std::printf("[J8-6] empty=%.1f unconfirmed=%.1f\n", e.maxDeviationMm(), u.maxDeviationMm());
    }

    ArtifactDoc d;
    d.loadSample();
    const KA::MeshData m0 = d.mesh();  // 기운 예제의 바탕 (예제 0°)
    checkBand("sample 0°", d);
    checkDev("sample 0°", d);
    {  // 기준 5 (0°): 제품 ↔ 시험 쪽, 시험 쪽 면마다 ↔ 전 칸 × 전 면
        const double md = checkOrtho("[J8-5]", "sample 0°", d);
        const Line a = rawHalfOrtho(d.mesh()), all = rawHalfOrthoAll(d.mesh());
        CHECK(sameLine(a, all), "[J8-5] rawHalfOrtho n=%zu vs brute n=%zu maxdiff %.3g", a.size(), all.size(), maxDiff(a, all));
        std::printf("[J8-5] sample 0° n=%zu maxdiff=%.3g · brute n=%zu same=%d\n", d.orthoOutline().size(), md, all.size(),
                    int(sameLine(a, all)));
    }
    for (const double tol : {0.1, 0.3, 1.0}) {  // 기준 2·4·6 간격별
        d.setSimplifyTol(tol);
        checkDev("sample 0°", d);
    }
    d.setSimplifyTol(0.5);

    // 기준 7: 곡선 끔 = DP 그대로 · 점 수
    {
        d.setSmooth(false);
        const auto sec = d.displaySection();
        const SectionLines& s = d.sectionLines();
        CHECK(sec.size() == 1 && !s.loops.empty() && sameLine(sec[0], simplifyDP(s.loops[0], 0.25, true)),
              "[J8-7] smooth off section %zu loops, %zu pts", sec.size(), sec.empty() ? size_t(0) : sec[0].size());
        CHECK(sameLine(d.displayOutline(), simplifyDP(d.orthoOutline(), 0.25, false)), "[J8-7] smooth off outline %zu pts",
              d.displayOutline().size());
        const int offPts = d.pointCount();
        CHECK(offPts == 30, "[J8-7] smooth off pointCount %d", offPts);
        d.setSmooth(true);
        const auto sec1 = d.displaySection();
        const size_t s1 = sec1.empty() ? 0 : sec1[0].size(), o1 = d.displayOutline().size();
        CHECK(s1 == 48 && o1 == 34 && d.pointCount() == 82 && d.rawPointCount() == 1343,
              "[J8-7] smooth on section %zu outline %zu points %d raw %d", s1, o1, d.pointCount(), d.rawPointCount());
        std::printf("[J8-7] smooth off: section %zu · outline %zu · points %d | on: section %zu · outline %zu · points %d/%d\n",
                    sec.empty() ? size_t(0) : sec[0].size(), simplifyDP(d.orthoOutline(), 0.25, false).size(), offPts, s1, o1,
                    d.pointCount(), d.rawPointCount());
    }

    // 기준 8: 파일 (예제 0°) · 래스터
    ExportRequest R0;  // A4 세로 · 1:1 · opts 기본
    R0.title = QStringLiteral("단면 맞춤 시험");
    auto exportBoth = [&](const ArtifactDoc& doc, const char* svg, const char* dxf, const char* csv) {
        ExportRequest r = R0;
        r.path = P(svg);
        CHECK(exportSvgLayered(doc, r, &err), "[J8-8] export %s: %s", svg, qUtf8Printable(err));
        r.path = P(dxf);
        CHECK(exportDxf(doc, r, &err), "[J8-8] export %s: %s", dxf, qUtf8Printable(err));
        writeRawCsv(P(csv), doc);
        checkDxfBand(dxf, P(dxf));
    };
    exportBoth(d, "studio_fit_s.svg", "studio_fit_s.dxf", "studio_fit_s_raw.csv");
    {
        const double H = meshH(d);
        const auto red = [](QRgb c) { return qRed(c) - qGreen(c) > 100; };
        const auto ink = [](QRgb c) { return std::min({qRed(c), qGreen(c), qBlue(c)}) < 245; };
        const Sheet a = drawSheet(d, true, false);  // 단면만
        const int sAbove = countPx(a, [&](double, double y) { return y > H + 0.5; }, red);
        const int sBelow = countPx(a, [&](double, double y) { return y < -0.5; }, red);
        const int sTop = countPx(a, [&](double, double y) { return y >= H - 1 && y <= H; }, red);
        const int sBot = countPx(a, [&](double, double y) { return y >= 0 && y <= 1; }, red);
        CHECK(sAbove == 0 && sBelow == 0 && sTop > 0 && sBot > 0, "[J8-8] section raster above %d below %d top %d bottom %d", sAbove,
              sBelow, sTop, sBot);
        const Sheet b = drawSheet(d, false, true);  // 외곽선(·기준선)만, 열 x ≤ −2 (중심선 제외)
        const int oAbove = countPx(b, [&](double x, double y) { return x <= -2 && y > H + 0.5; }, ink);
        const int oBelow = countPx(b, [&](double x, double y) { return x <= -2 && y < -0.5; }, ink);
        const int oBot = countPx(b, [&](double x, double y) { return x <= -2 && y >= 0.3 && y <= 1.0; }, ink);
        const int oTop = countPx(b, [&](double x, double y) { return x <= -2 && y >= H - 1.0 && y <= H - 0.3; }, ink);
        CHECK(oAbove == 0 && oBelow == 0 && oBot > 0 && oTop > 0, "[J8-8] outline raster above %d below %d [0.3,1] %d [H-1,H-0.3] %d",
              oAbove, oBelow, oBot, oTop);
        std::printf("[J8-8] raster E=[%.1f, %.1f]×[%.1f, %.1f] section above %d below %d top %d bottom %d · outline above %d below %d "
                    "[0.3,1] %d [H-1,H-0.3] %d\n",
                    a.E.left(), a.E.right(), a.E.top(), a.E.bottom(), sAbove, sBelow, sTop, sBot, oAbove, oBelow, oBot, oTop);
    }

    // 12° · −20° · 105°
    d.setFrontAngle(12);
    checkBand("sample 12°", d);
    checkDev("sample 12°", d);
    std::printf("[J8-5] sample 12° n=%zu maxdiff=%.3g\n", d.orthoOutline().size(), checkOrtho("[J8-5]", "sample 12°", d));
    d.setFrontAngle(-20);
    checkBand("sample −20°", d);
    checkDev("sample −20°", d);
    std::printf("[J8-5] sample −20° n=%zu maxdiff=%.3g\n", d.orthoOutline().size(), checkOrtho("[J8-5]", "sample −20°", d));
    d.setFrontAngle(105);
    checkBand("sample 105°", d);
    checkDev("sample 105°", d);

    // 미스캔 예제 0° · 12°
    {
        ArtifactDoc b;
        b.loadArtecBlindSample();
        checkBand("blind 0°", b);
        checkDev("blind 0°", b);
        std::printf("[J8-5] blind 0° n=%zu maxdiff=%.3g\n", b.orthoOutline().size(), checkOrtho("[J8-5]", "blind 0°", b));
        exportBoth(b, "studio_fit_b.svg", "studio_fit_b.dxf", "studio_fit_b_raw.csv");
        b.setFrontAngle(12);
        checkBand("blind 12°", b);
        checkDev("blind 12°", b);
    }

    // 기준 23: 기운 바닥·구연 (예제 0° 메시를 기울여 들어온 자세 그대로 확정)
    checkTilt("base 0.3°", m0, -0.3, 0, false);
    checkTilt("base 0.5°", m0, -0.5, 0, false);
    checkTilt("rim 0.5°", m0, 0, 0.5, false);
    checkTilt("rim 1.0°", m0, 0, 1.0, false);
    checkTilt("rim 1.0° front", m0, 0, 1.0, true);

    // 기준 24: 25만 면 초과 메시도 외곽선은 원본에서 (자동 세우기 없이 확정)
    {
        KA::Testing::SyntheticParams sp;
        sp.radialSeg = 360;
        sp.heightSeg = 240;
        sp.capTop = false;
        sp.capBottom = true;
        sp.tiltDeg = 0;
        sp.translation = {0, 0, 0};
        sp.noiseSigma = 0.05;
        KA::MeshData m = KA::Testing::MakeTorusProfileVessel(82.6, 71.0, 184.5, 6.8, sp).toMesh();
        KA::FillMeshStats(m);
        ArtifactDoc h;
        h.loadMesh(std::move(m), {}, QStringLiteral("25만 면 초과 예제"));
        h.confirmPose();
        const double tol = h.simplifyTol(), H = meshH(h);
        const size_t faces = h.mesh().faces.size(), lod = h.lodMesh().faces.size();
        CHECK(faces > 250000 && lod < faces, "[J8-24] faces %zu lod %zu", faces, lod);
        const double md = checkOrtho("[J8-24]", "hi", h);
        const Line ol = h.displayOutline();
        if (ol.empty()) {
            CHECK(false, "[J8-24] outline empty");
        } else {
            bool xOk = true;
            for (const auto& q : ol) xOk = xOk && q.x <= 1e-9;
            const double od = dev2(h.orthoOutline(), ol, false), mx = h.maxDeviationMm();
            const double gLo = ol.front().y - h.mesh().bboxMin.z, gHi = h.mesh().bboxMax.z - ol.back().y;
            CHECK(xOk && ol.front().y - h.mesh().bboxMin.z <= 0.25 && H - (ol.back().y - h.mesh().bboxMin.z) <= 0.25,
                  "[J8-24] outline x %d gap %.4f/%.4f", int(xOk), gLo, gHi);
            CHECK(od <= tol / 2 + 1e-9 && mx <= tol / 2 + 1e-9, "[J8-24] olDev %.6f maxDev %.6f", od, mx);
            std::printf("[J8-24] faces=%zu lod=%zu n=%zu ends=(%.3f,%.3f)…(%.3f,%.3f) gap %.3f/%.3f olDev=%.3f maxDev=%.3f maxdiff=%.3g · "
                        "dims base=%.2f wall=%.2f (원본 치수 — 판정은 [J9-1])\n",
                        faces, lod, h.orthoOutline().size(), ol.front().x, ol.front().y, ol.back().x, ol.back().y, gLo, gHi, od, mx, md,
                        h.dims().base, h.dims().meanWall);
        }
    }

    // 기준 8(c): 파이썬 판정용 파일 6개
    for (const char* n : {"studio_fit_s.svg", "studio_fit_s.dxf", "studio_fit_b.svg", "studio_fit_b.dxf", "studio_fit_s_raw.csv",
                          "studio_fit_b_raw.csv"})
        CHECK(QFileInfo(P(n)).isFile() && QFileInfo(P(n)).size() > 0, "[J8-8] file %s", n);
    std::printf("[J8-8] files: studio_fit_s/b .svg .dxf _raw.csv in %s\n", qUtf8Printable(out));

    std::puts(fails ? "FAILED" : "PASS");
    return fails ? 1 : 0;
}
