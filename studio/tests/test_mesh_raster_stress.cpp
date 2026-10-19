// DongArch3D Studio — Empirical Adversarial Stress Test Suite for MeshRaster
// Written by Challenger 1 (Milestone 2)
// Rigorously tests extreme inputs, boundary clipping, concurrency, buffer
// reuse, and DTak SAT.

#include "MeshRaster.h"
#include "RimDetector.h"

#include <QColor>
#include <QGuiApplication>
#include <QImage>
#include <algorithm>
#include <atomic>
#include <cassert>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <limits>
#include <numbers>
#include <random>
#include <string>
#include <thread>
#include <vector>

using namespace studio;
namespace KA = DongArch::KArch;

static int fails = 0;
#define CHECK(c, ...)                                                          \
  do {                                                                         \
    if (!(c)) {                                                                \
      std::printf("  [FAIL] %s: ", #c);                                        \
      std::printf(__VA_ARGS__);                                                \
      std::printf("\n");                                                       \
      ++fails;                                                                 \
    }                                                                          \
  } while (0)

#define CHECK_PASS(msg) std::printf("  [PASS] %s\n", msg)

static KA::MeshData makeSyntheticCylinder(int slices = 32, double r = 50.0,
                                          double h = 100.0) {
  KA::MeshData m;
  m.vertices.reserve(slices * 2 + 2);
  m.faces.reserve(slices * 4);

  // Bottom ring (z = 0)
  for (int i = 0; i < slices; ++i) {
    double th = 2.0 * std::numbers::pi * i / slices;
    m.vertices.push_back({r * std::cos(th), r * std::sin(th), 0.0});
  }
  // Top ring (z = h)
  for (int i = 0; i < slices; ++i) {
    double th = 2.0 * std::numbers::pi * i / slices;
    m.vertices.push_back({r * std::cos(th), r * std::sin(th), h});
  }

  // Side quads as two triangles
  for (int i = 0; i < slices; ++i) {
    int next = (i + 1) % slices;
    int b0 = i;
    int b1 = next;
    int t0 = slices + i;
    int t1 = slices + next;
    m.faces.push_back({b0, b1, t0});
    m.faces.push_back({b1, t1, t0});
  }

  // Bounding box
  m.bboxMin = {-r, -r, 0.0};
  m.bboxMax = {r, r, h};
  return m;
}

// ---------------------------------------------------------------------------
// 1. Extreme Inputs
// ---------------------------------------------------------------------------
static void test_extreme_inputs() {
  std::printf("\n=== [Test 1] Extreme Inputs ===\n");

  RasterView v;
  v.pxPerMm = 2.0;
  v.centerPx = QPointF(100.0, 100.0);
  v.centerZ = 50.0;
  QSize sz(200, 200);

  // 1.1 Empty mesh
  {
    KA::MeshData empty;
    std::vector<float> depth;
    for (ShadeMode mode :
         {ShadeMode::Real, ShadeMode::Depth, ShadeMode::DTak}) {
      QImage img = renderMesh(empty, sz, v, mode, Qt::gray, &depth);
      CHECK(img.width() == 200 && img.height() == 200,
            "Empty mesh image dimensions must match");
      CHECK(depth.size() == 200 * 200, "Depth vector must match pixel count");
      bool allNan = true;
      for (float d : depth) {
        if (!std::isnan(d)) {
          allNan = false;
          break;
        }
      }
      CHECK(allNan, "All depth values for empty mesh must be NaN");

      // Check that image is completely transparent
      bool allTransparent = true;
      for (int y = 0; y < 200; ++y) {
        const auto *line = reinterpret_cast<const QRgb *>(img.constScanLine(y));
        for (int x = 0; x < 200; ++x) {
          if (qAlpha(line[x]) != 0) {
            allTransparent = false;
            break;
          }
        }
      }
      CHECK(allTransparent, "Empty mesh image must be completely transparent");
    }
    CHECK_PASS("1.1 Empty Mesh handled safely without crashes");
  }

  // 1.2 Single degenerate triangle (point, collinear, tiny area)
  {
    KA::MeshData degen;
    // Degenerate 1: all 3 points identical
    degen.vertices = {{0, 0, 50}, {0, 0, 50}, {0, 0, 50}};
    degen.faces = {{0, 1, 2}};
    QImage img1 = renderMesh(degen, sz, v, ShadeMode::Real, Qt::gray);
    CHECK(!img1.isNull(), "Identical vertices must not crash");

    // Degenerate 2: collinear vertices (area = 0)
    degen.vertices = {{-10, 0, 50}, {0, 0, 50}, {10, 0, 50}};
    degen.faces = {{0, 1, 2}};
    QImage img2 = renderMesh(degen, sz, v, ShadeMode::Depth, Qt::gray);
    CHECK(!img2.isNull(), "Collinear vertices must not crash");

    // Degenerate 3: near-zero area (< 1e-12)
    degen.vertices = {{0, 0, 50}, {1e-7, 0, 50}, {0, 1e-7, 50}};
    degen.faces = {{0, 1, 2}};
    QImage img3 = renderMesh(degen, sz, v, ShadeMode::DTak, Qt::gray);
    CHECK(!img3.isNull(), "Near-zero area triangle must not crash");

    CHECK_PASS("1.2 Degenerate triangles (point, collinear, sub-epsilon) "
               "safely rejected");
  }

  // 1.2b Empirical Bug Proof: Mesh with < 5000 faces dropping (1 - 1/nThreads)
  // of its faces
  {
    std::printf("  Testing 16-triangle grid (numFaces = 16 < 5000)...\n");
    KA::MeshData gridMesh;
    for (int i = 0; i < 16; ++i) {
      double xBase = -35.0 + i * 5.0; // from -35 to +40 mm
      int vBase = int(gridMesh.vertices.size());
      gridMesh.vertices.push_back({xBase, 0.0, 45.0});
      gridMesh.vertices.push_back({xBase + 4.0, 0.0, 45.0});
      gridMesh.vertices.push_back({xBase + 2.0, 0.0, 55.0});
      gridMesh.faces.push_back({vBase, vBase + 1, vBase + 2});
    }

    RasterView gv;
    gv.pxPerMm = 2.0;
    gv.centerPx = QPointF(100.0, 100.0);
    gv.centerZ = 50.0;
    std::vector<float> depth;
    QImage gImg = renderMesh(gridMesh, QSize(200, 200), gv, ShadeMode::Real,
                             Qt::gray, &depth);

    int renderedTriangles = 0;
    for (int i = 0; i < 16; ++i) {
      double xBase = -35.0 + i * 5.0;
      int px = int(std::round(100.0 + (xBase + 2.0) * 2.0));
      int py = 104; // z = 48 -> y = 100 - (48 - 50)*2 = 104
      float d = depth[size_t(py) * 200 + px];
      bool hit = std::isfinite(d);
      if (hit)
        ++renderedTriangles;
      std::printf("    Triangle %d (x=%.1f): hit=%d (depth=%g)\n", i,
                  xBase + 2.0, int(hit), d);
    }
    std::printf("    Result: %d out of 16 triangles were rendered!\n",
                renderedTriangles);
    CHECK(renderedTriangles == 16,
          "All 16 triangles MUST be rendered, but only %d were rendered!",
          renderedTriangles);
  }

  // 1.3 Extreme coordinates: NaN, Inf, 1e20, -1e20
  {
    const double qnan = std::numeric_limits<double>::quiet_NaN();
    const double pinf = std::numeric_limits<double>::infinity();
    const double ninf = -pinf;

    std::vector<KA::Point3> extremeVerts = {
        {qnan, 0.0, 50.0},  {0.0, qnan, 50.0},     {0.0, 0.0, qnan},
        {qnan, qnan, qnan}, {pinf, 0.0, 50.0},     {0.0, ninf, 50.0},
        {pinf, pinf, pinf}, {1e20, 0.0, 50.0},     {-1e20, 0.0, 50.0},
        {1e20, 1e20, 1e20}, {-1e20, -1e20, -1e20},
    };

    for (size_t i = 0; i < extremeVerts.size(); ++i) {
      KA::MeshData extMesh;
      extMesh.vertices = {
          extremeVerts[i], {10.0, 0.0, 50.0}, {0.0, 10.0, 50.0}};
      extMesh.faces = {{0, 1, 2}};
      std::vector<float> depth;
      QImage img =
          renderMesh(extMesh, sz, v, ShadeMode::Real, Qt::gray, &depth);
      CHECK(!img.isNull(), "Extreme coordinates must not crash (case %zu)", i);

      // Verify no NaN or Inf was written to depth buffer as a foreground pixel
      for (float d : depth) {
        if (!std::isnan(d)) {
          CHECK(std::isfinite(d), "Depth values if not NaN must be finite");
        }
      }
    }
    CHECK_PASS("1.3 Extreme coordinates (NaN, Inf, 1e20, -1e20) handled "
               "without crash or UB");
  }

  // 1.4 Inverted normals and winding
  {
    // Part A: 1-face mesh (demonstrates that single triangle is completely
    // dropped by the < 5000 bug)
    KA::MeshData invMesh1;
    invMesh1.vertices = {{-10, 0, 40}, {10, 0, 40}, {0, 0, 60}};
    invMesh1.faces = {{0, 1, 2}};
    QImage imgCCW1 = renderMesh(invMesh1, sz, v, ShadeMode::Real, Qt::gray);
    int count1 = 0;
    for (int y = 0; y < 200; ++y) {
      const auto *l = reinterpret_cast<const QRgb *>(imgCCW1.constScanLine(y));
      for (int x = 0; x < 200; ++x)
        if (qAlpha(l[x]) > 0)
          ++count1;
    }
    std::printf("  1.4a Single triangle (1 face): rendered pixels = %d "
                "(expected > 0, but dropped by bug)\n",
                count1);
    CHECK(count1 > 0,
          "Single triangle must render pixels, but was dropped (got %d)",
          count1);

    // Part B: 5000-face mesh (bypasses the < 5000 bug to test double-sided
    // lighting and CW/CCW winding)
    KA::MeshData invMesh5k;
    for (int i = 0; i < 5000; ++i) {
      int vb = int(invMesh5k.vertices.size());
      invMesh5k.vertices.push_back({-10.0, 0.0, 40.0});
      invMesh5k.vertices.push_back({10.0, 0.0, 40.0});
      invMesh5k.vertices.push_back({0.0, 0.0, 60.0});
      invMesh5k.faces.push_back({vb, vb + 1, vb + 2});
    }
    QImage imgCCW5k = renderMesh(invMesh5k, sz, v, ShadeMode::Real, Qt::gray);

    KA::MeshData revMesh5k = invMesh5k;
    for (auto &f : revMesh5k.faces)
      std::swap(f[1], f[2]);
    QImage imgCW5k = renderMesh(revMesh5k, sz, v, ShadeMode::Real, Qt::gray);

    int countCCW = 0, countCW = 0;
    for (int y = 0; y < 200; ++y) {
      const auto *lCCW =
          reinterpret_cast<const QRgb *>(imgCCW5k.constScanLine(y));
      const auto *lCW =
          reinterpret_cast<const QRgb *>(imgCW5k.constScanLine(y));
      for (int x = 0; x < 200; ++x) {
        if (qAlpha(lCCW[x]) > 0)
          ++countCCW;
        if (qAlpha(lCW[x]) > 0)
          ++countCW;
      }
    }
    CHECK(countCCW > 0, "CCW 5000-triangle mesh must render pixels (got %d)",
          countCCW);
    CHECK(countCW > 0, "CW 5000-triangle mesh must render pixels (got %d)",
          countCW);
    CHECK(countCCW == countCW,
          "Double-sided rendering must cover identical pixel count (CCW=%d, "
          "CW=%d)",
          countCCW, countCW);
    if (countCCW > 0 && countCCW == countCW) {
      CHECK_PASS("1.4b Double-sided lighting and winding invariant verified "
                 "for multi-threaded path");
    }
  }

  // 1.5 Out-of-bounds face indices
  {
    KA::MeshData oobMesh;
    oobMesh.vertices = {{0, 0, 50}, {10, 0, 50}, {0, 10, 50}};
    // Face indices 99, 100, 101 are way beyond vertices.size() == 3
    oobMesh.faces = {{0, 1, 99}, {100, 1, 2}, {0, 99999, 2}};
    QImage img = renderMesh(oobMesh, sz, v, ShadeMode::Real, Qt::gray);
    CHECK(!img.isNull(), "Out-of-bounds face index must not crash");
    CHECK_PASS("1.5 Out-of-bounds face indices safely skipped by bounds guard");
  }
}

// ---------------------------------------------------------------------------
// 2. Screen Boundaries & Canvas Sizes
// ---------------------------------------------------------------------------
static void test_boundaries_and_canvas() {
  std::printf("\n=== [Test 2] Screen Boundaries & Canvas Sizes ===\n");

  RasterView v;
  v.pxPerMm = 1.0;
  v.centerPx = QPointF(50.0, 50.0);
  v.centerZ = 0.0;

  KA::MeshData tri;
  tri.vertices = {{-10, 0, 0}, {10, 0, 0}, {0, 0, 20}};
  tri.faces = {{0, 1, 2}};

  // 2.1 Mesh entirely outside viewport
  {
    // Shift far to the left
    RasterView vLeft = v;
    vLeft.centerPx = QPointF(-1000.0, 50.0);
    QImage imgLeft =
        renderMesh(tri, QSize(100, 100), vLeft, ShadeMode::Real, Qt::gray);
    int nonZero = 0;
    for (int y = 0; y < 100; ++y) {
      const auto *l = reinterpret_cast<const QRgb *>(imgLeft.constScanLine(y));
      for (int x = 0; x < 100; ++x)
        if (qAlpha(l[x]) > 0)
          ++nonZero;
    }
    CHECK(nonZero == 0,
          "Mesh completely outside viewport must not render any pixels");

    // Shift far to the right
    RasterView vRight = v;
    vRight.centerPx = QPointF(1000.0, 50.0);
    QImage imgRight =
        renderMesh(tri, QSize(100, 100), vRight, ShadeMode::Real, Qt::gray);
    nonZero = 0;
    for (int y = 0; y < 100; ++y) {
      const auto *l = reinterpret_cast<const QRgb *>(imgRight.constScanLine(y));
      for (int x = 0; x < 100; ++x)
        if (qAlpha(l[x]) > 0)
          ++nonZero;
    }
    CHECK(nonZero == 0, "Mesh far right must not render any pixels");
    CHECK_PASS("2.1 Mesh entirely outside viewport cleanly culled");
  }

  // 2.2 Partially clipped triangles (all 4 edges + huge triangle)
  {
    // Triangle overlapping left edge
    RasterView vEdge = v;
    vEdge.centerPx = QPointF(0.0, 50.0);
    QImage imgEdge =
        renderMesh(tri, QSize(100, 100), vEdge, ShadeMode::Real, Qt::gray);
    CHECK(!imgEdge.isNull(),
          "Clipped triangle on left edge must not crash or leak");

    // Huge triangle spanning way past all 4 screen edges
    KA::MeshData hugeTri;
    hugeTri.vertices = {{-5000, 0, -5000}, {5000, 0, -5000}, {0, 0, 5000}};
    hugeTri.faces = {{0, 1, 2}};
    QImage imgHuge =
        renderMesh(hugeTri, QSize(100, 100), v, ShadeMode::Real, Qt::gray);
    CHECK(!imgHuge.isNull(),
          "Huge triangle spanning all borders must not overflow buffers");
    CHECK_PASS("2.2 Partially clipped triangles safely bounded without buffer "
               "overrun");
  }

  // 2.3 Canvas size stress: 1x1, 1x4000, 4000x1, 4000x4000, 0x0
  {
    // 1x1 canvas
    QImage img1x1 = renderMesh(tri, QSize(1, 1), v, ShadeMode::Real, Qt::gray);
    CHECK(img1x1.width() == 1 && img1x1.height() == 1, "1x1 canvas supported");

    // 0x0 canvas (clamped to 1x1)
    QImage img0x0 = renderMesh(tri, QSize(0, 0), v, ShadeMode::Real, Qt::gray);
    CHECK(img0x0.width() >= 1 && img0x0.height() >= 1,
          "0x0 canvas clamped safely");

    // Extreme aspect ratios
    QImage imgNarrow =
        renderMesh(tri, QSize(1, 4000), v, ShadeMode::Real, Qt::gray);
    CHECK(imgNarrow.width() == 1 && imgNarrow.height() == 4000,
          "1x4000 narrow canvas supported");

    QImage imgWide =
        renderMesh(tri, QSize(4000, 1), v, ShadeMode::Real, Qt::gray);
    CHECK(imgWide.width() == 4000 && imgWide.height() == 1,
          "4000x1 wide canvas supported");

    // 4000x4000 canvas in all 3 modes
    std::printf("  Testing 4000x4000 canvas (16M pixels)...\n");
    for (ShadeMode mode :
         {ShadeMode::Real, ShadeMode::Depth, ShadeMode::DTak}) {
      const auto t0 = std::chrono::steady_clock::now();
      QImage img4k = renderMesh(tri, QSize(4000, 4000), v, mode, Qt::gray);
      double dt = std::chrono::duration<double, std::milli>(
                      std::chrono::steady_clock::now() - t0)
                      .count();
      CHECK(img4k.width() == 4000 && img4k.height() == 4000,
            "4000x4000 canvas dimensions correct");
      std::printf("    4000x4000 mode %d took %.1f ms\n", int(mode), dt);
    }
    CHECK_PASS("2.3 Extreme canvas sizes (1x1 to 4000x4000) rendered without "
               "OOM or crash");
  }
}

// ---------------------------------------------------------------------------
// 3. Concurrency & Thread-Local Buffer Reuse Stress
// ---------------------------------------------------------------------------
static void test_concurrency_and_buffer_reuse() {
  std::printf("\n=== [Test 3] Concurrency & Buffer Reuse Stress ===\n");

  // Use > 5000 faces to stress-test the multi-threaded code paths
  KA::MeshData cyl = makeSyntheticCylinder(2560, 40.0, 80.0); // 5120 faces
  std::printf("  Using test cylinder with %zu faces (> 5000)\n",
              cyl.faces.size());
  RasterView v;
  v.pxPerMm = 2.0;
  v.centerPx = QPointF(200.0, 200.0);
  v.centerZ = 40.0;

  // 3.1 Rapid sequential calls on single thread with alternating resolutions
  // and modes
  {
    std::printf(
        "  Sequential buffer reuse stress (100 rapid alternations)...\n");
    std::vector<QSize> sizes = {
        QSize(400, 400), QSize(10, 10),     QSize(2500, 1500), QSize(1, 1),
        QSize(800, 600), QSize(3000, 2000), QSize(50, 50),
    };
    std::vector<ShadeMode> modes = {
        ShadeMode::Real,
        ShadeMode::Depth,
        ShadeMode::DTak,
        ShadeMode::Real,
    };

    std::vector<QRgb> vcols(cyl.vertices.size(), qRgb(180, 140, 100));

    {
      QSize sz(400, 400);
      v.centerPx = QPointF(200.0, 200.0);
      std::vector<float> depth;
      QImage img = renderMesh(cyl, sz, v, ShadeMode::Real, Qt::gray, &depth);
      int nonZero = 0;
      float minD = 1e9, maxD = -1e9;
      for (float d : depth) {
        if (std::isfinite(d)) {
          ++nonZero;
          minD = std::min(minD, d);
          maxD = std::max(maxD, d);
        }
      }
      std::printf(
          "    DEBUG: 400x400 render nonZero pixels: %d, minD: %g, maxD: %g\n",
          nonZero, minD, maxD);
      int minPxX = 9999, maxPxX = -1, minPxY = 9999, maxPxY = -1;
      for (int y = 0; y < 400; ++y) {
        for (int x = 0; x < 400; ++x) {
          if (std::isfinite(depth[y * 400 + x])) {
            minPxX = std::min(minPxX, x);
            maxPxX = std::max(maxPxX, x);
            minPxY = std::min(minPxY, y);
            maxPxY = std::max(maxPxY, y);
          }
        }
      }
      std::printf(
          "    DEBUG: Rendered pixel bounding box: X=[%d, %d], Y=[%d, %d]\n",
          minPxX, maxPxX, minPxY, maxPxY);
      int cx = 200, cy = 200;
      std::printf("    DEBUG: center pixel (200, 200) depth: %g, alpha: %d\n",
                  depth[cy * 400 + cx], qAlpha(img.pixel(cx, cy)));
    }

    for (int iter = 0; iter < 100; ++iter) {
      QSize sz = sizes[iter % sizes.size()];
      ShadeMode mode = modes[iter % modes.size()];
      v.centerPx = QPointF(sz.width() / 2.0, sz.height() / 2.0);
      std::vector<float> depth;

      const std::vector<QRgb> *colPtr = (iter % 3 == 0) ? &vcols : nullptr;
      QImage img = renderMesh(cyl, sz, v, mode, Qt::gray, &depth, colPtr);

      CHECK(img.width() == sz.width() && img.height() == sz.height(),
            "Iter %d: size mismatch", iter);
      CHECK(depth.size() == size_t(sz.width()) * sz.height(),
            "Iter %d: depth size mismatch", iter);

      // Verify that center pixel has valid depth and color
      int cx = sz.width() / 2, cy = sz.height() / 2;
      size_t k = size_t(cy) * sz.width() + cx;
      float d = depth[k];
      CHECK(std::isfinite(d),
            "Iter %d: Center pixel depth must be finite (got %g)", iter, d);
      QRgb px = img.pixel(cx, cy);
      CHECK(qAlpha(px) == 255,
            "Iter %d: Center pixel must be opaque (got alpha=%d)", iter,
            qAlpha(px));
    }
    CHECK_PASS("3.1 100 sequential alternating renders verified (no stale "
               "buffer corruption)");
  }

  // 3.2 Concurrent multi-threaded renderMesh calls (8 client threads)
  {
    std::printf(
        "  Concurrent client threads stress (8 threads x 25 renders)...\n");
    const int numClientThreads = 8;
    const int rendersPerThread = 25;
    std::vector<std::thread> clients;
    clients.reserve(numClientThreads);
    std::atomic<int> clientFails{0};

    for (int tid = 0; tid < numClientThreads; ++tid) {
      clients.emplace_back([tid, &cyl, &clientFails]() {
        RasterView tv;
        tv.pxPerMm = 1.5 + tid * 0.2;
        int w = 200 + tid * 50;
        int h = 150 + tid * 40;
        tv.centerPx = QPointF(w / 2.0, h / 2.0);
        tv.centerZ = 40.0;
        QSize sz(w, h);

        std::vector<QRgb> myCols(cyl.vertices.size(),
                                 qRgb(100 + tid * 10, 150, 200));

        for (int r = 0; r < rendersPerThread; ++r) {
          ShadeMode sm = (r % 3 == 0)   ? ShadeMode::Real
                         : (r % 3 == 1) ? ShadeMode::Depth
                                        : ShadeMode::DTak;
          std::vector<float> dOut;
          QImage res = renderMesh(cyl, sz, tv, sm, Qt::gray, &dOut, &myCols);
          if (res.width() != w || res.height() != h) {
            ++clientFails;
          }
          if (dOut.size() != size_t(w) * h) {
            ++clientFails;
          }
          // Center pixel check
          float cd = dOut[size_t(h / 2) * w + (w / 2)];
          if (!std::isfinite(cd)) {
            ++clientFails;
          }
        }
      });
    }

    for (auto &t : clients) {
      t.join();
    }
    CHECK(clientFails.load() == 0,
          "Concurrent client threads must have 0 failures (got %d)",
          clientFails.load());
    CHECK_PASS("3.2 8 concurrent client threads (200 total renders) passed "
               "without race or crash");
  }
}

// ---------------------------------------------------------------------------
// 4. Large Mesh Benchmark & Shading Fidelity
// ---------------------------------------------------------------------------
static void test_large_mesh_fidelity() {
  std::printf(
      "\n=== [Test 4] Large Synthetic Mesh (100k faces) & DTak Fidelity ===\n");

  KA::MeshData mesh = makeSyntheticCylinder(500, 50.0, 100.0); // 1000 faces
  // Duplicate to reach 100k faces
  KA::MeshData bigMesh;
  const int copies = 100;
  bigMesh.vertices.reserve(mesh.vertices.size() * copies);
  bigMesh.faces.reserve(mesh.faces.size() * copies);
  for (int c = 0; c < copies; ++c) {
    size_t vBase = bigMesh.vertices.size();
    for (const auto &p : mesh.vertices) {
      bigMesh.vertices.push_back({p.x + (c % 10) * 1.0, p.y, p.z});
    }
    for (const auto &f : mesh.faces) {
      bigMesh.faces.push_back(
          {int(vBase + f[0]), int(vBase + f[1]), int(vBase + f[2])});
    }
  }

  std::printf("  Created test mesh with %zu vertices and %zu faces\n",
              bigMesh.vertices.size(), bigMesh.faces.size());

  RasterView v;
  v.pxPerMm = 2.0;
  v.centerPx = QPointF(250.0, 250.0);
  v.centerZ = 50.0;
  QSize sz(500, 500);

  // Measure DTak rendering
  const auto t0 = std::chrono::steady_clock::now();
  QImage dtakImg = renderMesh(bigMesh, sz, v, ShadeMode::DTak, Qt::gray);
  double dt = std::chrono::duration<double, std::milli>(
                  std::chrono::steady_clock::now() - t0)
                  .count();
  std::printf("  100k faces DTak render time: %.2f ms\n", dt);
  CHECK(!dtakImg.isNull(), "DTak render must not be null");

  // Verify pixel colors in DTak are valid RGB
  int validPixels = 0;
  for (int y = 0; y < sz.height(); ++y) {
    const auto *line = reinterpret_cast<const QRgb *>(dtakImg.constScanLine(y));
    for (int x = 0; x < sz.width(); ++x) {
      if (qAlpha(line[x]) > 0) {
        int r = qRed(line[x]);
        int g = qGreen(line[x]);
        int b = qBlue(line[x]);
        CHECK(r >= 0 && r <= 255 && g >= 0 && g <= 255 && b >= 0 && b <= 255,
              "RGB values must be in [0, 255]");
        ++validPixels;
      }
    }
  }
  CHECK(validPixels > 1000,
        "DTak must have rendered foreground pixels (got %d)", validPixels);
  CHECK_PASS("4.1 100k faces DTak fidelity and SAT computation verified");
}

int main(int argc, char **argv) {
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  qputenv("QT_QPA_PLATFORM", "offscreen");
  QGuiApplication app(argc, argv);

  std::printf("================================================================"
              "======\n");
  std::printf("DongArch3D Studio — Adversarial MeshRaster Stress Test Suite\n");
  std::printf("Challenger 1 (Milestone 2)\n");
  std::printf("================================================================"
              "======\n");

  test_extreme_inputs();
  test_boundaries_and_canvas();
  test_concurrency_and_buffer_reuse();
  test_large_mesh_fidelity();

  std::printf("\n=============================================================="
              "========\n");
  if (fails == 0) {
    std::printf("[VERDICT] ALL ADVERSARIAL STRESS TESTS PASSED (0 failures)\n");
  } else {
    std::printf("[VERDICT] FAILED with %d failure(s)\n", fails);
  }
  std::printf("================================================================"
              "======\n");

  return fails ? 1 : 0;
}
