#include "MeshIO.h"

#include "MeshLoader.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QImage>
#include <QLibrary>
#include <QRegularExpression>
#include <QTextStream>
#include <QtEndian>
#include <algorithm>
#include <charconv>
#include <cmath>
#include <cstring>
#include <unordered_map>

namespace studio {

namespace KA = DongArch::KArch;

namespace {

struct Key {
  long long x, y, z;
  bool operator==(const Key &o) const {
    return x == o.x && y == o.y && z == o.z;
  }
};
struct KeyHash {
  size_t operator()(const Key &k) const {
    return size_t(k.x * 73856093LL ^ k.y * 19349663LL ^ k.z * 83492791LL);
  }
};

int typeSize(const QByteArray &t) {
  if (t == "char" || t == "uchar" || t == "int8" || t == "uint8")
    return 1;
  if (t == "short" || t == "ushort" || t == "int16" || t == "uint16")
    return 2;
  if (t == "int" || t == "uint" || t == "float" || t == "int32" ||
      t == "uint32" || t == "float32")
    return 4;
  if (t == "double" || t == "float64")
    return 8;
  return 0;
}

double readScalar(const char *p, const QByteArray &t, bool be) {
  auto u16 = [&] {
    quint16 v;
    std::memcpy(&v, p, 2);
    return be ? qFromBigEndian(v) : qFromLittleEndian(v);
  };
  auto u32 = [&] {
    quint32 v;
    std::memcpy(&v, p, 4);
    return be ? qFromBigEndian(v) : qFromLittleEndian(v);
  };
  auto u64 = [&] {
    quint64 v;
    std::memcpy(&v, p, 8);
    return be ? qFromBigEndian(v) : qFromLittleEndian(v);
  };
  if (t == "char" || t == "int8")
    return double(*reinterpret_cast<const qint8 *>(p));
  if (t == "uchar" || t == "uint8")
    return double(*reinterpret_cast<const quint8 *>(p));
  if (t == "short" || t == "int16")
    return double(qint16(u16()));
  if (t == "ushort" || t == "uint16")
    return double(u16());
  if (t == "int" || t == "int32")
    return double(qint32(u32()));
  if (t == "uint" || t == "uint32")
    return double(u32());
  if (t == "float" || t == "float32") {
    quint32 v = u32();
    float f;
    std::memcpy(&f, &v, 4);
    return f;
  }
  if (t == "double" || t == "float64") {
    quint64 v = u64();
    double d;
    std::memcpy(&d, &v, 8);
    return d;
  }
  return 0.0;
}

inline int toByteColor(double v, const QByteArray &t) {
  if (t == "float" || t == "float32" || t == "double" || t == "float64") {
    if (v <= 1.001)
      return std::clamp(int(std::lround(v * 255.0)), 0, 255);
  }
  return std::clamp(int(std::lround(v)), 0, 255);
}

inline QRgb sampleTex(const QImage &img, double u, double v) {
  if (img.isNull())
    return qRgb(198, 179, 154);
  u = u - std::floor(u);
  v = 1.0 - (v - std::floor(v)); // OBJ/PLY 표준 UV: 아래가 0 → 이미지 위가 0
  const int x =
      std::clamp(int(u * (img.width() - 1) + 0.5), 0, img.width() - 1);
  const int y =
      std::clamp(int(v * (img.height() - 1) + 0.5), 0, img.height() - 1);
  return img.pixel(x, y);
}

QImage findCompanionTexture(const QString &meshPath,
                            const QString &hintFile = {}) {
  const QFileInfo fi(meshPath);
  const QDir dir = fi.absoluteDir();
  QStringList candidates;
  if (!hintFile.isEmpty())
    candidates << dir.filePath(hintFile)
               << dir.filePath(QFileInfo(hintFile).fileName());
  // MTL 파일에서 map_Kd 탐색
  const QString mtlPath = dir.filePath(fi.completeBaseName() + ".mtl");
  if (QFileInfo::exists(mtlPath)) {
    QFile mf(mtlPath);
    if (mf.open(QIODevice::ReadOnly)) {
      while (!mf.atEnd()) {
        const QByteArray ln = mf.readLine().trimmed();
        if (ln.startsWith("map_Kd ")) {
          const QString tex = QString::fromUtf8(ln.mid(7).trimmed());
          candidates << dir.filePath(tex)
                     << dir.filePath(QFileInfo(tex).fileName());
        }
      }
    }
  }
  for (const char *ext : {".jpg", ".png", ".jpeg", ".bmp", ".tif", "_0.jpg",
                          "_0.png", "_tex.jpg", "_tex.png"}) {
    candidates << dir.filePath(fi.completeBaseName() +
                               QString::fromLatin1(ext));
  }
  for (const QString &c : candidates) {
    if (QFileInfo::exists(c)) {
      QImage img(c);
      if (!img.isNull())
        return img.convertToFormat(QImage::Format_RGB32);
    }
  }
  return {};
}

struct PlyProp {
  QByteArray name, type, countType;
};
struct PlyElem {
  QByteArray name;
  qint64 count = 0;
  std::vector<PlyProp> props;
};

MeshReadResult readPlyAny(const QByteArray &all, const QString &meshPath) {
  MeshReadResult r;
  const qsizetype hEnd = all.indexOf("end_header");
  if (hEnd < 0) {
    r.message = QStringLiteral("PLY 머리(end_header)가 없습니다.");
    return r;
  }
  qsizetype pos = all.indexOf('\n', hEnd) + 1;
  const QList<QByteArray> lines = all.left(hEnd).split('\n');
  bool ascii = false, be = false;
  QString texHint;
  std::vector<PlyElem> elems;
  for (QByteArray ln : lines) {
    ln = ln.trimmed();
    const auto tok = ln.split(' ');
    if (tok.isEmpty())
      continue;
    if (tok[0] == "format" && tok.size() > 1) {
      ascii = (tok[1] == "ascii");
      be = (tok[1] == "binary_big_endian");
    } else if (tok[0] == "comment" && tok.size() >= 3 &&
               (tok[1].compare("TextureFile", Qt::CaseInsensitive) == 0)) {
      texHint = QString::fromUtf8(
          ln.mid(ln.indexOf(tok[1]) + tok[1].size()).trimmed());
    } else if (tok[0] == "element" && tok.size() >= 3) {
      elems.push_back({tok[1], tok[2].toLongLong(), {}});
    } else if (tok[0] == "property" && !elems.empty()) {
      if (tok.size() >= 5 && tok[1] == "list")
        elems.back().props.push_back({tok[4], tok[3], tok[2]});
      else if (tok.size() >= 3)
        elems.back().props.push_back({tok[2], tok[1], {}});
    }
  }

  const QImage texImg = findCompanionTexture(meshPath, texHint);
  const char *d = all.constData();
  const qsizetype N = all.size();
  std::vector<std::pair<double, double>> vUV;
  bool foundVColor = false, foundVUV = false, foundFaceUV = false,
       foundVQuality = false;

  auto nextToken = [&](double &outVal) -> bool {
    while (pos < N && (d[pos] == ' ' || d[pos] == '\t' || d[pos] == '\r' ||
                       d[pos] == '\n'))
      ++pos;
    if (pos >= N)
      return false;
    const char *start = d + pos;
    char *endPtr = nullptr;
    outVal = std::strtod(start, &endPtr);
    if (endPtr == start)
      return false;
    pos = endPtr - d;
    return true;
  };

  for (const auto &e : elems) {
    const bool isV = e.name == "vertex", isF = e.name == "face";
    int ix = -1, iy = -1, iz = -1, ir = -1, ig = -1, ib = -1, iu = -1, iv = -1,
        iq = -1;
    for (int i = 0; i < int(e.props.size()); ++i) {
      const QByteArray &nm = e.props[i].name;
      if (nm == "x")
        ix = i;
      else if (nm == "y")
        iy = i;
      else if (nm == "z")
        iz = i;
      else if (nm == "red" || nm == "r" || nm == "diffuse_red")
        ir = i;
      else if (nm == "green" || nm == "g" || nm == "diffuse_green")
        ig = i;
      else if (nm == "blue" || nm == "b" || nm == "diffuse_blue")
        ib = i;
      else if (nm == "u" || nm == "s" || nm == "texture_u")
        iu = i;
      else if (nm == "v" || nm == "t" || nm == "texture_v")
        iv = i;
      else if (nm == "quality" || nm == "funcval" || nm == "value")
        iq = i;
    }
    if (isV) {
      r.mesh.vertices.reserve(size_t(e.count));
      if (ir >= 0 && ig >= 0 && ib >= 0) {
        r.vertexColors.reserve(size_t(e.count));
        foundVColor = true;
      }
      if (iu >= 0 && iv >= 0 && !texImg.isNull()) {
        vUV.reserve(size_t(e.count));
        foundVUV = true;
      }
      if (iq >= 0) {
        r.vertexQuality.reserve(size_t(e.count));
        foundVQuality = true;
      }
    }
    for (qint64 k = 0; k < e.count; ++k) {
      KA::Point3 p;
      int cr = 198, cg = 179, cb = 154;
      double tu = 0.0, tv = 0.0;
      float qval = 0.0f;
      std::vector<int> faceIdx;
      for (int i = 0; i < int(e.props.size()); ++i) {
        const auto &pr = e.props[i];
        if (pr.countType.isEmpty()) {
          double v = 0.0;
          if (ascii) {
            if (!nextToken(v)) {
              r.message = QStringLiteral("PLY ASCII 자료가 잘렸습니다.");
              return r;
            }
          } else {
            const int sz = typeSize(pr.type);
            if (sz == 0 || pos + sz > N) {
              r.message = QStringLiteral("PLY 자료가 잘렸습니다.");
              return r;
            }
            v = readScalar(d + pos, pr.type, be);
            pos += sz;
          }
          if (isV) {
            if (i == ix)
              p.x = v;
            else if (i == iy)
              p.y = v;
            else if (i == iz)
              p.z = v;
            else if (i == ir)
              cr = toByteColor(v, pr.type);
            else if (i == ig)
              cg = toByteColor(v, pr.type);
            else if (i == ib)
              cb = toByteColor(v, pr.type);
            else if (i == iu)
              tu = v;
            else if (i == iv)
              tv = v;
            else if (i == iq)
              qval = float(v);
          }
        } else {
          int cnt = 0;
          if (ascii) {
            double cv = 0;
            if (!nextToken(cv)) {
              r.message = QStringLiteral("PLY 목록 길이 오류");
              return r;
            }
            cnt = int(cv);
          } else {
            const int cs = typeSize(pr.countType);
            if (cs == 0 || pos + cs > N) {
              r.message = QStringLiteral("PLY 목록 형식 오류");
              return r;
            }
            cnt = int(readScalar(d + pos, pr.countType, be));
            pos += cs;
          }
          const bool isIdx =
              isF && (pr.name == "vertex_indices" || pr.name == "vertex_index");
          const bool isTex = isF && (pr.name == "texcoord") && !texImg.isNull();
          if (ascii) {
            std::vector<double> vals(size_t(std::max(0, cnt)));
            for (int j = 0; j < cnt; ++j)
              nextToken(vals[size_t(j)]);
            if (isIdx && cnt >= 3) {
              faceIdx.assign(vals.begin(), vals.end());
              const int a = int(vals[0]);
              for (int j = 1; j + 1 < cnt; ++j)
                r.mesh.faces.push_back(
                    {a, int(vals[size_t(j)]), int(vals[size_t(j + 1)])});
            } else if (isTex && cnt >= 6 && !faceIdx.empty()) {
              if (r.vertexColors.size() < r.mesh.vertices.size())
                r.vertexColors.resize(r.mesh.vertices.size(),
                                      qRgb(198, 179, 154));
              foundFaceUV = true;
              for (int j = 0; j < int(faceIdx.size()) && 2 * j + 1 < cnt; ++j) {
                const int vi = faceIdx[size_t(j)];
                if (vi >= 0 && size_t(vi) < r.vertexColors.size())
                  r.vertexColors[size_t(vi)] = sampleTex(
                      texImg, vals[size_t(2 * j)], vals[size_t(2 * j + 1)]);
              }
            }
          } else {
            const int is = typeSize(pr.type);
            if (is == 0 || pos + qsizetype(cnt) * is > N) {
              r.message = QStringLiteral("PLY 자료가 잘렸습니다.");
              return r;
            }
            if (isIdx && cnt >= 3) {
              faceIdx.resize(size_t(cnt));
              for (int j = 0; j < cnt; ++j)
                faceIdx[size_t(j)] =
                    int(readScalar(d + pos + j * is, pr.type, be));
              const int a = faceIdx[0];
              for (int j = 1; j + 1 < cnt; ++j)
                r.mesh.faces.push_back(
                    {a, faceIdx[size_t(j)], faceIdx[size_t(j + 1)]});
            } else if (isTex && cnt >= 6 && !faceIdx.empty()) {
              if (r.vertexColors.size() < r.mesh.vertices.size())
                r.vertexColors.resize(r.mesh.vertices.size(),
                                      qRgb(198, 179, 154));
              foundFaceUV = true;
              for (int j = 0; j < int(faceIdx.size()) && 2 * j + 1 < cnt; ++j) {
                const int vi = faceIdx[size_t(j)];
                const double u =
                    readScalar(d + pos + (2 * j) * is, pr.type, be);
                const double v =
                    readScalar(d + pos + (2 * j + 1) * is, pr.type, be);
                if (vi >= 0 && size_t(vi) < r.vertexColors.size())
                  r.vertexColors[size_t(vi)] = sampleTex(texImg, u, v);
              }
            }
            pos += qsizetype(cnt) * is;
          }
        }
      }
      if (isV) {
        r.mesh.vertices.push_back(p);
        if (foundVColor)
          r.vertexColors.push_back(qRgb(cr, cg, cb));
        if (foundVUV)
          vUV.push_back({tu, tv});
        if (foundVQuality)
          r.vertexQuality.push_back(qval);
      }
    }
  }
  if (!foundVColor && foundVUV && !texImg.isNull()) {
    r.vertexColors.resize(r.mesh.vertices.size());
    for (size_t i = 0; i < vUV.size(); ++i)
      r.vertexColors[i] = sampleTex(texImg, vUV[i].first, vUV[i].second);
    r.hasTexture = true;
  }
  if (foundFaceUV)
    r.hasTexture = true;
  r.hasColor = !r.vertexColors.empty();
  r.hasQuality = !r.vertexQuality.empty();
  r.ok = !r.mesh.vertices.empty() && !r.mesh.faces.empty();
  if (!r.ok)
    r.message = QStringLiteral("PLY 에 삼각형이 없습니다.");
  return r;
}

// Artec Studio OBJ + MTL + JPG/PNG 고속 파서
MeshReadResult readObjFast(const QByteArray &all, const QString &meshPath) {
  MeshReadResult r;
  auto &m = r.mesh;
  QString mtlTex;
  std::vector<std::pair<double, double>> uvs;
  std::vector<QRgb> vCols;
  bool hasVCol = false;

  const char *cur = all.constData();
  const char *end = cur + all.size();

  auto skipSpaces = [](const char *&p, const char *e) {
    while (p < e && (*p == ' ' || *p == '\t'))
      ++p;
  };
  auto parseNum = [](const char *&p, const char *e, double &v) -> bool {
    while (p < e && (*p == ' ' || *p == '\t'))
      ++p;
    if (p >= e || *p == '\n' || *p == '\r')
      return false;
    char *ep = nullptr;
    v = std::strtod(p, &ep);
    if (ep == p)
      return false;
    p = ep;
    return true;
  };
  auto parseInt = [](const char *&p, const char *e, int &v) -> bool {
    while (p < e && (*p == ' ' || *p == '\t'))
      ++p;
    if (p >= e || *p == '\n' || *p == '\r')
      return false;
    auto [ptr, ec] = std::from_chars(p, e, v);
    if (ec != std::errc{})
      return false;
    p = ptr;
    return true;
  };

  std::vector<int> vvt; // 정점별 대표 UV 인덱스 (1-based, 없으면 0)

  while (cur < end) {
    const char *lineEnd =
        static_cast<const char *>(std::memchr(cur, '\n', size_t(end - cur)));
    if (!lineEnd)
      lineEnd = end;
    const char *p = cur;
    skipSpaces(p, lineEnd);
    if (p + 1 < lineEnd && p[0] == 'v' && (p[1] == ' ' || p[1] == '\t')) {
      p += 2;
      double x = 0, y = 0, z = 0, cr = -1, cg = -1, cb = -1;
      if (parseNum(p, lineEnd, x) && parseNum(p, lineEnd, y) &&
          parseNum(p, lineEnd, z)) {
        m.vertices.push_back({x, y, z});
        if (parseNum(p, lineEnd, cr) && parseNum(p, lineEnd, cg) &&
            parseNum(p, lineEnd, cb)) {
          hasVCol = true;
          const double scale =
              (cr <= 1.001 && cg <= 1.001 && cb <= 1.001) ? 255.0 : 1.0;
          vCols.push_back(qRgb(std::clamp(int(cr * scale + 0.5), 0, 255),
                               std::clamp(int(cg * scale + 0.5), 0, 255),
                               std::clamp(int(cb * scale + 0.5), 0, 255)));
        } else {
          vCols.push_back(qRgb(198, 179, 154));
        }
      }
    } else if (p + 2 < lineEnd && p[0] == 'v' && p[1] == 't' &&
               (p[2] == ' ' || p[2] == '\t')) {
      p += 3;
      double u = 0, v = 0;
      if (parseNum(p, lineEnd, u) && parseNum(p, lineEnd, v))
        uvs.push_back({u, v});
    } else if (p + 1 < lineEnd && p[0] == 'f' &&
               (p[1] == ' ' || p[1] == '\t')) {
      p += 2;
      if (vvt.size() < m.vertices.size())
        vvt.resize(m.vertices.size(), 0);
      int fv[16], fvt[16], cnt = 0;
      while (cnt < 16) {
        int vi = 0, ti = 0;
        if (!parseInt(p, lineEnd, vi))
          break;
        if (p < lineEnd && *p == '/') {
          ++p;
          if (p < lineEnd && *p != '/')
            parseInt(p, lineEnd, ti);
          while (p < lineEnd && *p != ' ' && *p != '\t' && *p != '\r' &&
                 *p != '\n')
            ++p;
        }
        const int nv = int(m.vertices.size());
        vi = vi > 0 ? vi - 1 : nv + vi;
        if (vi < 0 || vi >= nv)
          continue;
        fv[cnt] = vi;
        fvt[cnt] = ti > 0 ? ti : (ti < 0 ? int(uvs.size()) + 1 + ti : 0);
        if (fvt[cnt] > 0 && vvt[size_t(vi)] == 0)
          vvt[size_t(vi)] = fvt[cnt];
        ++cnt;
      }
      for (int j = 1; j + 1 < cnt; ++j)
        m.faces.push_back({fv[0], fv[j], fv[j + 1]});
    }
    cur = (lineEnd < end) ? lineEnd + 1 : end;
  }

  const QImage texImg = findCompanionTexture(meshPath, mtlTex);
  if (!texImg.isNull() && !uvs.empty() && vvt.size() == m.vertices.size()) {
    r.vertexColors.resize(m.vertices.size(), qRgb(198, 179, 154));
    for (size_t i = 0; i < m.vertices.size(); ++i) {
      const int ti = vvt[i] - 1;
      if (ti >= 0 && size_t(ti) < uvs.size())
        r.vertexColors[i] =
            sampleTex(texImg, uvs[size_t(ti)].first, uvs[size_t(ti)].second);
    }
    r.hasColor = true;
    r.hasTexture = true;
  } else if (hasVCol && vCols.size() == m.vertices.size()) {
    r.vertexColors = std::move(vCols);
    r.hasColor = true;
  }

  r.ok = !m.vertices.empty() && !m.faces.empty();
  if (!r.ok)
    r.message = QStringLiteral("OBJ 에 유효한 정점·삼각형이 없습니다.");
  return r;
}

// Artec Studio VRML 2.0 (.wrl) 파서
MeshReadResult readWrl(const QByteArray &all, const QString &meshPath) {
  MeshReadResult r;
  auto &m = r.mesh;
  if (!all.startsWith("#VRML")) {
    r.message = QStringLiteral("유효한 VRML 2.0 (.wrl) 헤더가 없습니다.");
    return r;
  }
  auto extractBracket = [&](const QByteArray &key,
                            qsizetype startFrom = 0) -> QByteArray {
    const qsizetype k = all.indexOf(key, startFrom);
    if (k < 0)
      return {};
    const qsizetype b0 = all.indexOf('[', k);
    const qsizetype b1 = b0 >= 0 ? all.indexOf(']', b0) : -1;
    return (b0 >= 0 && b1 > b0) ? all.mid(b0 + 1, b1 - b0 - 1) : QByteArray{};
  };
  auto parseDoubles = [](const QByteArray &blk) {
    std::vector<double> out;
    const char *p = blk.constData();
    const char *e = p + blk.size();
    while (p < e) {
      while (p < e &&
             (*p == ' ' || *p == '\t' || *p == '\r' || *p == '\n' || *p == ','))
        ++p;
      if (p >= e)
        break;
      char *ep = nullptr;
      const double v = std::strtod(p, &ep);
      if (ep == p) {
        ++p;
        continue;
      }
      out.push_back(v);
      p = ep;
    }
    return out;
  };
  auto parseInts = [](const QByteArray &blk) {
    std::vector<int> out;
    const char *p = blk.constData();
    const char *e = p + blk.size();
    while (p < e) {
      while (p < e &&
             (*p == ' ' || *p == '\t' || *p == '\r' || *p == '\n' || *p == ','))
        ++p;
      if (p >= e)
        break;
      int v = 0;
      auto [ptr, ec] = std::from_chars(p, e, v);
      if (ec != std::errc{}) {
        ++p;
        continue;
      }
      out.push_back(v);
      p = ptr;
    }
    return out;
  };

  const qsizetype coordPos = all.indexOf("Coordinate");
  const auto pts =
      parseDoubles(extractBracket("point", coordPos >= 0 ? coordPos : 0));
  for (size_t i = 0; i + 2 < pts.size(); i += 3)
    m.vertices.push_back({pts[i], pts[i + 1], pts[i + 2]});

  const qsizetype colPos = all.indexOf("Color");
  if (colPos >= 0) {
    const auto cols = parseDoubles(extractBracket("color", colPos));
    for (size_t i = 0; i + 2 < cols.size(); i += 3) {
      r.vertexColors.push_back(
          qRgb(std::clamp(int(cols[i] * 255 + 0.5), 0, 255),
               std::clamp(int(cols[i + 1] * 255 + 0.5), 0, 255),
               std::clamp(int(cols[i + 2] * 255 + 0.5), 0, 255)));
    }
  }

  const auto idx = parseInts(extractBracket("coordIndex"));
  std::vector<int> poly;
  for (int v : idx) {
    if (v < 0) {
      for (size_t j = 1; j + 1 < poly.size(); ++j)
        if (size_t(poly[0]) < m.vertices.size() &&
            size_t(poly[j]) < m.vertices.size() &&
            size_t(poly[j + 1]) < m.vertices.size())
          m.faces.push_back({poly[0], poly[j], poly[j + 1]});
      poly.clear();
    } else {
      poly.push_back(v);
    }
  }

  const qsizetype texPos = all.indexOf("TextureCoordinate");
  if (texPos >= 0 && r.vertexColors.empty()) {
    const QImage texImg = findCompanionTexture(meshPath);
    const auto uvs = parseDoubles(extractBracket("point", texPos));
    if (!texImg.isNull() && uvs.size() >= m.vertices.size() * 2) {
      r.vertexColors.resize(m.vertices.size());
      for (size_t i = 0; i < m.vertices.size(); ++i)
        r.vertexColors[i] = sampleTex(texImg, uvs[2 * i], uvs[2 * i + 1]);
      r.hasTexture = true;
    }
  }
  if (r.vertexColors.size() != m.vertices.size())
    r.vertexColors.clear();
  r.hasColor = !r.vertexColors.empty();
  r.ok = !m.vertices.empty() && !m.faces.empty();
  if (!r.ok)
    r.message = QStringLiteral("WRL 에 삼각형이 없습니다.");
  return r;
}

MeshReadResult readStl(const QByteArray &all) {
  MeshReadResult r;
  auto &m = r.mesh;
  const bool binary = all.size() >= 84 && [&] {
    quint32 n;
    std::memcpy(&n, all.constData() + 80, 4);
    return qsizetype(84) + qsizetype(qFromLittleEndian(n)) * 50 == all.size();
  }();
  if (binary) {
    quint32 n;
    std::memcpy(&n, all.constData() + 80, 4);
    n = qFromLittleEndian(n);
    const char *p = all.constData() + 84;
    m.vertices.reserve(size_t(n) * 3);
    m.faces.reserve(size_t(n));
    for (quint32 i = 0; i < n; ++i, p += 50) {
      int idx[3];
      for (int k = 0; k < 3; ++k) {
        float v[3];
        std::memcpy(v, p + 12 + k * 12, 12);
        idx[k] = int(m.vertices.size());
        m.vertices.push_back({v[0], v[1], v[2]});
      }
      m.faces.push_back({idx[0], idx[1], idx[2]});
    }
  } else {
    int k = 0;
    int idx[3] = {0, 0, 0};
    for (const QByteArray &raw : all.split('\n')) {
      const QByteArray ln = raw.trimmed();
      if (!ln.startsWith("vertex"))
        continue;
      const auto t = ln.simplified().split(' ');
      if (t.size() < 4)
        continue;
      idx[k] = int(m.vertices.size());
      m.vertices.push_back({t[1].toDouble(), t[2].toDouble(), t[3].toDouble()});
      if (++k == 3) {
        m.faces.push_back({idx[0], idx[1], idx[2]});
        k = 0;
      }
    }
  }
  r.ok = !m.faces.empty();
  if (!r.ok)
    r.message = QStringLiteral("STL 에 삼각형이 없습니다.");
  return r;
}

} // namespace

void weldVertices(KA::MeshData &m, std::vector<QRgb> *colors,
                  std::vector<float> *quality) {
  if (m.vertices.empty())
    return;
  const bool hasCol = colors && colors->size() == m.vertices.size();
  const bool hasQual = quality && quality->size() == m.vertices.size();
  KA::Point3 lo = m.vertices[0], hi = lo;
  for (const auto &p : m.vertices) {
    lo = {std::min(lo.x, p.x), std::min(lo.y, p.y), std::min(lo.z, p.z)};
    hi = {std::max(hi.x, p.x), std::max(hi.y, p.y), std::max(hi.z, p.z)};
  }
  const double diag = std::hypot(hi.x - lo.x, hi.y - lo.y, hi.z - lo.z);
  const double eps = std::max(1e-9, diag * 1e-6);
  std::unordered_map<Key, int, KeyHash> map;
  map.reserve(m.vertices.size());
  std::vector<int> remap(m.vertices.size());
  std::vector<KA::Point3> out;
  std::vector<QRgb> outCol;
  std::vector<float> outQual;
  out.reserve(m.vertices.size());
  if (hasCol)
    outCol.reserve(m.vertices.size());
  if (hasQual)
    outQual.reserve(m.vertices.size());
  for (size_t i = 0; i < m.vertices.size(); ++i) {
    const auto &p = m.vertices[i];
    const Key k{std::llround(p.x / eps), std::llround(p.y / eps),
                std::llround(p.z / eps)};
    auto [it, ins] = map.try_emplace(k, int(out.size()));
    if (ins) {
      out.push_back(p);
      if (hasCol)
        outCol.push_back((*colors)[i]);
      if (hasQual)
        outQual.push_back((*quality)[i]);
    }
    remap[i] = it->second;
  }
  std::vector<std::array<int, 3>> faces;
  faces.reserve(m.faces.size());
  for (auto f : m.faces) {
    if (f[0] < 0 || f[1] < 0 || f[2] < 0 || size_t(f[0]) >= remap.size() ||
        size_t(f[1]) >= remap.size() || size_t(f[2]) >= remap.size())
      continue;
    f = {remap[size_t(f[0])], remap[size_t(f[1])], remap[size_t(f[2])]};
    if (f[0] != f[1] && f[1] != f[2] && f[0] != f[2])
      faces.push_back(f);
  }
  m.vertices = std::move(out);
  m.faces = std::move(faces);
  if (hasCol)
    *colors = std::move(outCol);
  if (hasQual)
    *quality = std::move(outQual);
}

int removeFloatingNoise(KA::MeshData &m, std::vector<QRgb> *colors,
                        std::vector<float> *quality) {
  const size_t nv = m.vertices.size(), nf = m.faces.size();
  if (nv == 0 || nf < 20)
    return 0;
  std::vector<int> parent(nv);
  for (size_t i = 0; i < nv; ++i)
    parent[i] = int(i);
  auto find = [&](int x) {
    while (parent[size_t(x)] != x) {
      parent[size_t(x)] = parent[size_t(parent[size_t(x)])];
      x = parent[size_t(x)];
    }
    return x;
  };
  auto unite = [&](int a, int b) {
    a = find(a);
    b = find(b);
    if (a != b)
      parent[size_t(b)] = a;
  };
  for (const auto &f : m.faces) {
    if (f[0] >= 0 && f[1] >= 0 && f[2] >= 0 && size_t(f[0]) < nv &&
        size_t(f[1]) < nv && size_t(f[2]) < nv) {
      unite(f[0], f[1]);
      unite(f[1], f[2]);
    }
  }
  std::unordered_map<int, int> compFaces;
  int maxFaces = 0;
  for (const auto &f : m.faces) {
    const int r = find(f[0]);
    maxFaces = std::max(maxFaces, ++compFaces[r]);
  }
  if (compFaces.size() <= 1)
    return 0;
  const int minKeep = std::max(
      12, int(maxFaces * 0.02)); // 주 유물 면 수의 2% 미만 부유 조각 제거
  std::vector<int> vmap(nv, -1);
  std::vector<KA::Point3> newV;
  std::vector<QRgb> newC;
  std::vector<float> newQ;
  const bool hasCol = colors && colors->size() == nv;
  const bool hasQual = quality && quality->size() == nv;
  std::vector<std::array<int, 3>> newF;
  newF.reserve(nf);
  int removed = 0;
  for (const auto &f : m.faces) {
    const int r = find(f[0]);
    if (compFaces[r] < minKeep) {
      ++removed;
      continue;
    }
    int nfIdx[3];
    for (int k = 0; k < 3; ++k) {
      const int vi = f[size_t(k)];
      if (vmap[size_t(vi)] < 0) {
        vmap[size_t(vi)] = int(newV.size());
        newV.push_back(m.vertices[size_t(vi)]);
        if (hasCol)
          newC.push_back((*colors)[size_t(vi)]);
        if (hasQual)
          newQ.push_back((*quality)[size_t(vi)]);
      }
      nfIdx[k] = vmap[size_t(vi)];
    }
    newF.push_back({nfIdx[0], nfIdx[1], nfIdx[2]});
  }
  if (removed > 0 && !newF.empty()) {
    m.vertices = std::move(newV);
    m.faces = std::move(newF);
    if (hasCol)
      *colors = std::move(newC);
    if (hasQual)
      *quality = std::move(newQ);
  }
  return removed;
}

int erodeDanglingBorderFaces(KA::MeshData &m) {
  if (m.faces.size() < 16)
    return 0;
  auto edgeKey = [](int a, int b) -> quint64 {
    if (a > b)
      std::swap(a, b);
    return (quint64(quint32(a)) << 32) | quint64(quint32(b));
  };
  std::unordered_map<quint64, int> edgeUse;
  edgeUse.reserve(m.faces.size() * 2);
  for (const auto &f : m.faces) {
    ++edgeUse[edgeKey(f[0], f[1])];
    ++edgeUse[edgeKey(f[1], f[2])];
    ++edgeUse[edgeKey(f[2], f[0])];
  }
  std::vector<std::array<int, 3>> kept;
  kept.reserve(m.faces.size());
  int eroded = 0;
  for (const auto &f : m.faces) {
    const int b = (edgeUse[edgeKey(f[0], f[1])] == 1 ? 1 : 0) +
                  (edgeUse[edgeKey(f[1], f[2])] == 1 ? 1 : 0) +
                  (edgeUse[edgeKey(f[2], f[0])] == 1 ? 1 : 0);
    if (b >= 3) {
      ++eroded;
    } else {
      kept.push_back(f);
    }
  }
  if (eroded > 0 && !kept.empty())
    m.faces = std::move(kept);
  return eroded;
}

std::vector<std::vector<int>> extractBorderLoops(const KA::MeshData &m) {
  std::vector<std::vector<int>> loops;
  if (m.faces.empty())
    return loops;
  auto edgeKey = [](int a, int b) -> quint64 {
    if (a > b)
      std::swap(a, b);
    return (quint64(quint32(a)) << 32) | quint64(quint32(b));
  };
  std::unordered_map<quint64, int> edgeCount;
  edgeCount.reserve(m.faces.size() * 2);
  for (const auto &f : m.faces) {
    ++edgeCount[edgeKey(f[0], f[1])];
    ++edgeCount[edgeKey(f[1], f[2])];
    ++edgeCount[edgeKey(f[2], f[0])];
  }
  std::unordered_map<int, std::vector<int>> nextMap;
  for (const auto &f : m.faces) {
    const int e[3][2] = {{f[0], f[1]}, {f[1], f[2]}, {f[2], f[0]}};
    for (const auto &ed : e) {
      if (edgeCount[edgeKey(ed[0], ed[1])] == 1)
        nextMap[ed[0]].push_back(ed[1]);
    }
  }
  std::unordered_map<int, bool> visited;
  for (const auto &[startV, nbrs] : nextMap) {
    if (visited[startV] || nbrs.empty())
      continue;
    std::vector<int> loop;
    int cur = startV;
    while (!visited[cur]) {
      visited[cur] = true;
      loop.push_back(cur);
      auto it = nextMap.find(cur);
      if (it == nextMap.end() || it->second.empty())
        break;
      int nxt = -1;
      for (int cand : it->second) {
        if (!visited[cand]) {
          nxt = cand;
          break;
        }
      }
      if (nxt < 0)
        break;
      cur = nxt;
    }
    if (loop.size() >= 3)
      loops.push_back(std::move(loop));
  }
  std::sort(loops.begin(), loops.end(),
            [](const auto &a, const auto &b) { return a.size() > b.size(); });
  return loops;
}

ArtecSdkStatus probeArtecSdk() {
  ArtecSdkStatus st;
  const QStringList dirs = {
      QStringLiteral(
          "C:/Program Files/Artec Group/Artec Studio 18 Professional"),
      QStringLiteral(
          "C:/Program Files/Artec Group/Artec Studio 17 Professional"),
      QStringLiteral(
          "C:/Program Files/Artec Group/Artec Studio 16 Professional"),
      QDir::currentPath()};
  for (const QString &d : dirs) {
    const QDir dir(d);
    if (!dir.exists())
      continue;
    if (QFileInfo::exists(dir.filePath(QStringLiteral("cdsdk.dll")))) {
      st.sdkFound = true;
      st.sdkPath = dir.absolutePath();
      QLibrary lib(dir.filePath(QStringLiteral("cdsdk.dll")));
      if (lib.load()) {
        using CdGetDevicesFn = int (*)();
        if (auto fn =
                reinterpret_cast<CdGetDevicesFn>(lib.resolve("cdGetDevices"))) {
          st.connectedDevices = std::max(0, fn());
        }
        lib.unload();
      }
    }
    if (QFileInfo::exists(dir.filePath(QStringLiteral("Snx3Device_x64.dll"))))
      st.snxFound = true;
    if (QFileInfo::exists(dir.filePath(QStringLiteral("RAY2API.dll"))))
      st.rayFound = true;
  }
  if (st.sdkFound) {
    st.koreanSummary =
        st.connectedDevices > 0
            ? QStringLiteral("Artec 스캐너 %1대 연결됨 (%2)")
                  .arg(st.connectedDevices)
                  .arg(st.sdkPath)
            : QStringLiteral("Artec Studio SDK 감지됨 (연결된 스캐너 없음)");
  } else {
    st.koreanSummary =
        QStringLiteral("Artec 감시 폴더 자동 연동 활성 (SDK 미설치)");
  }
  return st;
}

void buildLodMesh(const KA::MeshData &src, const std::vector<QRgb> &srcColors,
                  KA::MeshData &lodOut, std::vector<QRgb> &lodColorsOut,
                  size_t targetFaces) {
  const bool hasCol = srcColors.size() == src.vertices.size();
  if (src.faces.size() <= targetFaces || src.vertices.empty()) {
    lodOut = src;
    lodColorsOut = srcColors;
    return;
  }
  const double dx = src.bboxMax.x - src.bboxMin.x;
  const double dy = src.bboxMax.y - src.bboxMin.y;
  const double dz = src.bboxMax.z - src.bboxMin.z;
  const double diag = std::hypot(dx, dy, dz);
  // 목표 삼각형 수에 맞춰 격자 크기 결정
  const double ratio = std::sqrt(double(src.faces.size()) /
                                 double(std::max<size_t>(1000, targetFaces)));
  const double cell =
      std::max(diag / 450.0, std::max(0.25, src.meanEdgeLength * ratio));

  struct Accum {
    double sx = 0, sy = 0, sz = 0;
    long sr = 0, sg = 0, sb = 0;
    int cnt = 0, idx = 0;
  };
  std::unordered_map<Key, Accum, KeyHash> grid;
  grid.reserve(targetFaces);
  std::vector<int> remap(src.vertices.size());
  int nextIdx = 0;
  for (size_t i = 0; i < src.vertices.size(); ++i) {
    const auto &p = src.vertices[i];
    const Key k{std::llround(p.x / cell), std::llround(p.y / cell),
                std::llround(p.z / cell)};
    auto [it, ins] = grid.try_emplace(k);
    if (ins)
      it->second.idx = nextIdx++;
    auto &a = it->second;
    a.sx += p.x;
    a.sy += p.y;
    a.sz += p.z;
    if (hasCol) {
      const QRgb c = srcColors[i];
      a.sr += qRed(c);
      a.sg += qGreen(c);
      a.sb += qBlue(c);
    }
    ++a.cnt;
    remap[i] = a.idx;
  }
  lodOut = {};
  lodOut.vertices.resize(size_t(nextIdx));
  lodColorsOut.clear();
  if (hasCol)
    lodColorsOut.resize(size_t(nextIdx));
  for (const auto &[k, a] : grid) {
    lodOut.vertices[size_t(a.idx)] = {a.sx / a.cnt, a.sy / a.cnt, a.sz / a.cnt};
    if (hasCol)
      lodColorsOut[size_t(a.idx)] =
          qRgb(int(a.sr / a.cnt), int(a.sg / a.cnt), int(a.sb / a.cnt));
  }
  lodOut.faces.reserve(targetFaces);
  for (const auto &f : src.faces) {
    const int u = remap[size_t(f[0])], v = remap[size_t(f[1])],
              w = remap[size_t(f[2])];
    if (u != v && v != w && u != w)
      lodOut.faces.push_back({u, v, w});
  }
  KA::FillMeshStats(lodOut);
  lodOut.hasBoundary = src.hasBoundary;
}

bool exportBordersPline(const KA::MeshData &m, const QString &path,
                        QString *err) {
  QFile f(path);
  if (!f.open(QIODevice::WriteOnly | QIODevice::Text)) {
    if (err)
      *err = QStringLiteral("경계선 파일(.pline)을 생성할 수 없습니다: %1")
                 .arg(path);
    return false;
  }
  const auto loops = extractBorderLoops(m);
  QTextStream out(&f);
  out << "# DongArch3D Studio — GigaMesh compatible border polylines "
         "(.pline)\n";
  out << "# BorderLoops: " << loops.size() << "\n";
  for (size_t li = 0; li < loops.size(); ++li) {
    const auto &lp = loops[li];
    out << "POLYLINE " << li << " " << lp.size() << "\n";
    for (int vi : lp) {
      if (vi >= 0 && size_t(vi) < m.vertices.size()) {
        const auto &p = m.vertices[size_t(vi)];
        out << vi << " " << p.x << " " << p.y << " " << p.z << "\n";
      }
    }
  }
  return true;
}

bool exportGltf2(const KA::MeshData &m, const std::vector<QRgb> &colors,
                 const QString &path, QString *err) {
  if (m.vertices.empty() || m.faces.empty()) {
    if (err)
      *err = QStringLiteral("내보낼 메시 데이터가 비어 있습니다.");
    return false;
  }
  QFile f(path);
  if (!f.open(QIODevice::WriteOnly | QIODevice::Text)) {
    if (err)
      *err = QStringLiteral("glTF 2.0 파일을 열 수 없습니다: %1").arg(path);
    return false;
  }
  const bool hasCol = colors.size() == m.vertices.size();
  QByteArray bin;
  const int vCount = int(m.vertices.size());
  const int iCount = int(m.faces.size() * 3);
  const int posBytes = vCount * 12;
  const int colBytes = hasCol ? vCount * 12 : 0;
  const int idxBytes = iCount * 4;
  bin.resize(posBytes + colBytes + idxBytes);
  char *ptr = bin.data();
  float minP[3] = {float(m.vertices[0].x), float(m.vertices[0].y),
                   float(m.vertices[0].z)};
  float maxP[3] = {minP[0], minP[1], minP[2]};
  for (int i = 0; i < vCount; ++i) {
    const float xyz[3] = {float(m.vertices[size_t(i)].x),
                          float(m.vertices[size_t(i)].y),
                          float(m.vertices[size_t(i)].z)};
    for (int c = 0; c < 3; ++c) {
      minP[c] = std::min(minP[c], xyz[c]);
      maxP[c] = std::max(maxP[c], xyz[c]);
    }
    std::memcpy(ptr + i * 12, xyz, 12);
  }
  if (hasCol) {
    char *cptr = ptr + posBytes;
    for (int i = 0; i < vCount; ++i) {
      const QRgb c = colors[size_t(i)];
      const float rgb[3] = {qRed(c) / 255.0f, qGreen(c) / 255.0f,
                            qBlue(c) / 255.0f};
      std::memcpy(cptr + i * 12, rgb, 12);
    }
  }
  char *iptr = ptr + posBytes + colBytes;
  for (size_t i = 0; i < m.faces.size(); ++i) {
    const quint32 tri[3] = {quint32(m.faces[i][0]), quint32(m.faces[i][1]),
                            quint32(m.faces[i][2])};
    std::memcpy(iptr + i * 12, tri, 12);
  }
  const QByteArray b64 = bin.toBase64();
  QTextStream out(&f);
  out << "{\n"
      << "  \"asset\": {\"version\": \"2.0\", \"generator\": \"DongArch3D "
         "Studio\"},\n"
      << "  \"scene\": 0,\n"
      << "  \"scenes\": [{\"nodes\": [0]}],\n"
      << "  \"nodes\": [{\"mesh\": 0}],\n"
      << "  \"meshes\": [{\"primitives\": [{\"attributes\": {\"POSITION\": 0"
      << (hasCol ? ", \"COLOR_0\": 1" : "")
      << "}, \"indices\": " << (hasCol ? 2 : 1) << "}]}],\n"
      << "  \"buffers\": [{\"byteLength\": " << bin.size()
      << ", \"uri\": \"data:application/octet-stream;base64," << b64
      << "\"}],\n"
      << "  \"bufferViews\": [\n"
      << "    {\"buffer\": 0, \"byteOffset\": 0, \"byteLength\": " << posBytes
      << ", \"target\": 34962},\n";
  if (hasCol) {
    out << "    {\"buffer\": 0, \"byteOffset\": " << posBytes
        << ", \"byteLength\": " << colBytes << ", \"target\": 34962},\n";
  }
  out << "    {\"buffer\": 0, \"byteOffset\": " << (posBytes + colBytes)
      << ", \"byteLength\": " << idxBytes << ", \"target\": 34963}\n"
      << "  ],\n"
      << "  \"accessors\": [\n"
      << "    {\"bufferView\": 0, \"componentType\": 5126, \"count\": "
      << vCount << ", \"type\": \"VEC3\", \"min\": [" << minP[0] << ", "
      << minP[1] << ", " << minP[2] << "], \"max\": [" << maxP[0] << ", "
      << maxP[1] << ", " << maxP[2] << "]},\n";
  if (hasCol) {
    out << "    {\"bufferView\": 1, \"componentType\": 5126, \"count\": "
        << vCount << ", \"type\": \"VEC3\"},\n";
  }
  out << "    {\"bufferView\": " << (hasCol ? 2 : 1)
      << ", \"componentType\": 5125, \"count\": " << iCount
      << ", \"type\": \"SCALAR\"}\n"
      << "  ]\n}\n";
  return true;
}

bool exportLegacyPly(const KA::MeshData &m, const std::vector<QRgb> &colors,
                     const std::vector<float> &quality, const QString &path,
                     bool binary, QString *err) {
  QFile f(path);
  if (!f.open(QIODevice::WriteOnly)) {
    if (err)
      *err = QStringLiteral("PLY 파일을 생성할 수 없습니다: %1").arg(path);
    return false;
  }
  const bool hasCol = colors.size() == m.vertices.size();
  const bool hasQual = quality.size() == m.vertices.size();
  QByteArray hdr;
  hdr += "ply\n";
  hdr += binary ? "format binary_little_endian 1.0\n" : "format ascii 1.0\n";
  hdr +=
      "comment Generated by DongArch3D Studio (GigaMesh Legacy compatible)\n";
  hdr +=
      "element vertex " + QByteArray::number(qint64(m.vertices.size())) + "\n";
  hdr += "property float x\nproperty float y\nproperty float z\n";
  if (hasCol)
    hdr += "property uchar red\nproperty uchar green\nproperty uchar blue\n";
  if (hasQual)
    hdr += "property float quality\n";
  hdr += "element face " + QByteArray::number(qint64(m.faces.size())) + "\n";
  hdr += "property list uchar int vertex_indices\nend_header\n";
  f.write(hdr);

  if (binary) {
    for (size_t i = 0; i < m.vertices.size(); ++i) {
      const float xyz[3] = {float(m.vertices[i].x), float(m.vertices[i].y),
                            float(m.vertices[i].z)};
      f.write(reinterpret_cast<const char *>(xyz), 12);
      if (hasCol) {
        const quint8 rgb[3] = {quint8(qRed(colors[i])),
                               quint8(qGreen(colors[i])),
                               quint8(qBlue(colors[i]))};
        f.write(reinterpret_cast<const char *>(rgb), 3);
      }
      if (hasQual) {
        const float q = quality[i];
        f.write(reinterpret_cast<const char *>(&q), 4);
      }
    }
    for (const auto &fc : m.faces) {
      const quint8 cnt = 3;
      const qint32 idx[3] = {fc[0], fc[1], fc[2]};
      f.write(reinterpret_cast<const char *>(&cnt), 1);
      f.write(reinterpret_cast<const char *>(idx), 12);
    }
  } else {
    QTextStream out(&f);
    for (size_t i = 0; i < m.vertices.size(); ++i) {
      out << float(m.vertices[i].x) << " " << float(m.vertices[i].y) << " "
          << float(m.vertices[i].z);
      if (hasCol)
        out << " " << qRed(colors[i]) << " " << qGreen(colors[i]) << " "
            << qBlue(colors[i]);
      if (hasQual)
        out << " " << quality[i];
      out << "\n";
    }
    for (const auto &fc : m.faces)
      out << "3 " << fc[0] << " " << fc[1] << " " << fc[2] << "\n";
  }
  return true;
}

MeshReadResult readMeshAny(const QString &path) {
  MeshReadResult r;
  const QString ext = QFileInfo(path).suffix().toLower();
  QFile f(path);
  if (!f.open(QIODevice::ReadOnly)) {
    r.message = QStringLiteral("파일을 열 수 없습니다: %1").arg(path);
    return r;
  }
  const QByteArray all = f.readAll();
  f.close();
  if (ext == QLatin1String("ply"))
    r = readPlyAny(all, path);
  else if (ext == QLatin1String("obj"))
    r = readObjFast(all, path);
  else if (ext == QLatin1String("wrl"))
    r = readWrl(all, path);
  else if (ext == QLatin1String("stl"))
    r = readStl(all);
  else {
    r.message =
        QStringLiteral("지원하지 않는 형식입니다 (PLY · OBJ · WRL · STL).");
    return r;
  }
  if (!r.ok)
    return r;

  weldVertices(r.mesh, r.hasColor ? &r.vertexColors : nullptr,
               r.hasQuality ? &r.vertexQuality : nullptr);
  r.removedNoiseFaces =
      removeFloatingNoise(r.mesh, r.hasColor ? &r.vertexColors : nullptr,
                          r.hasQuality ? &r.vertexQuality : nullptr);
  r.erodedBorderFaces = erodeDanglingBorderFaces(r.mesh);
  KA::FillMeshStats(r.mesh);

  // Artec Studio 단위(m / cm → mm) 자동 판별·보정
  const double dx = r.mesh.bboxMax.x - r.mesh.bboxMin.x;
  const double dy = r.mesh.bboxMax.y - r.mesh.bboxMin.y;
  const double dz = r.mesh.bboxMax.z - r.mesh.bboxMin.z;
  const double maxSpan = std::max({dx, dy, dz});
  if (maxSpan > 1e-5 && maxSpan < 1.8) {
    // 미터(m) 단위로 내보내진 스캔 (예: 0.184 m → 184 mm)
    r.unitScaleApplied = 1000.0;
    r.unitNote = QStringLiteral("m→mm 자동환산(×1000)");
  } else if (maxSpan >= 1.8 && maxSpan < 18.0 && r.mesh.meanEdgeLength < 0.08) {
    // 센티미터(cm) 단위로 내보내진 스캔 (예: 15.2 cm → 152 mm)
    r.unitScaleApplied = 10.0;
    r.unitNote = QStringLiteral("cm→mm 자동환산(×10)");
  }
  if (r.unitScaleApplied != 1.0) {
    for (auto &p : r.mesh.vertices) {
      p.x *= r.unitScaleApplied;
      p.y *= r.unitScaleApplied;
      p.z *= r.unitScaleApplied;
    }
    KA::FillMeshStats(r.mesh);
  }

  // 스캐너 기종 해상도 프로파일 자동 판별 + 기종별 최적 파라미터 설정
  if (r.mesh.meanEdgeLength > 0 && r.mesh.meanEdgeLength <= 0.38) {
    r.scannerProfile = QStringLiteral("Artec Space Spider 급 (%1 mm)")
                           .arg(r.mesh.meanEdgeLength, 0, 'f', 2);
    r.recommendedDpMm = 0.25;
    r.recommendedMsiiMm = 1.0;
  } else if (r.mesh.meanEdgeLength > 0 && r.mesh.meanEdgeLength <= 1.2) {
    r.scannerProfile = QStringLiteral("Artec Eva·Leo 급 (%1 mm)")
                           .arg(r.mesh.meanEdgeLength, 0, 'f', 2);
    r.recommendedDpMm = 0.50;
    r.recommendedMsiiMm = 2.0;
  } else {
    r.scannerProfile =
        QStringLiteral("3D 스캔 (%1 mm)").arg(r.mesh.meanEdgeLength, 0, 'f', 2);
    r.recommendedDpMm = 0.50;
    r.recommendedMsiiMm = 2.0;
  }

  return r;
}

} // namespace studio
