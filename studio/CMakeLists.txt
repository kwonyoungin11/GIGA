cmake_minimum_required(VERSION 3.21)
project(DongArch3DStudio VERSION 4.2.0 LANGUAGES CXX)

# DongArch3D Studio — 「조용한 유물 도판」 UI 셸 (docs/ui/UI_RULES.md 준수)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

option(STUDIO_ENABLE_LTO "Enable Link-Time Optimization (IPO/LTCG) in Release builds" ON)
option(STUDIO_ENABLE_AVX2 "Enable AVX2 SIMD vectorization on x86_64" ON)
option(STUDIO_ENABLE_PCH "Enable Precompiled Headers for faster builds" ON)
option(STUDIO_BUILD_KARCH_TESTS "Build and register 11 standalone K-Arch geometry unit tests" ON)

if(MSVC)
  add_compile_options(/utf-8 /W4 /permissive- /EHsc /Zc:__cplusplus)
  if(NOT CMAKE_GENERATOR MATCHES "Ninja")
    add_compile_options(/MP)
  endif()
  # Release 고성능 최적화: 인라인 확장, 고유 함수, 빠른 부동소수점, 함수 단위 링크
  add_compile_options("$<$<CONFIG:Release>:/O2;/Ob2;/Oi;/Ot;/Gy;/Zc:inline;/fp:fast>")
  if(STUDIO_ENABLE_AVX2 AND CMAKE_SIZEOF_VOID_P EQUAL 8)
    add_compile_options("$<$<CONFIG:Release>:/arch:AVX2>")
  endif()
  add_link_options("$<$<CONFIG:Release>:/OPT:REF;/OPT:ICF>")
  add_compile_definitions(_USE_MATH_DEFINES NOMINMAX _CRT_SECURE_NO_WARNINGS)
else()
  add_compile_options(-Wall -Wextra -Wpedantic)
  add_compile_options("$<$<CONFIG:Release>:-O3;-ffast-math>")
  if(STUDIO_ENABLE_AVX2 AND CMAKE_SYSTEM_PROCESSOR MATCHES "(x86_64|AMD64)")
    add_compile_options("$<$<CONFIG:Release>:-mavx2;-mfma>")
  endif()
endif()

if(STUDIO_ENABLE_LTO)
  include(CheckIPOSupported)
  check_ipo_supported(RESULT ipo_supported OUTPUT ipo_error)
  if(ipo_supported)
    set(CMAKE_INTERPROCEDURAL_OPTIMIZATION_RELEASE ON)
  endif()
endif()

find_package(Qt6 REQUIRED COMPONENTS Core Gui Widgets)

# ---- K-Arch 계산 엔진 (Qt 무의존 — AUTOMOC 비활성화로 빌드 가속) -------------
set(KARCH_DIR ${CMAKE_CURRENT_SOURCE_DIR}/../GigaMesh/gui/src/dongarch/karch)
add_library(karch STATIC
  ${KARCH_DIR}/TaubinFit.cpp ${KARCH_DIR}/AnnulusClassifier.cpp ${KARCH_DIR}/RimDetector.cpp
  ${KARCH_DIR}/AxisPca.cpp ${KARCH_DIR}/MeshSlicer.cpp ${KARCH_DIR}/AxisOdr.cpp
  ${KARCH_DIR}/SyntheticMeshGen.cpp ${KARCH_DIR}/MeshLoader.cpp ${KARCH_DIR}/Silhouette.cpp
  ${KARCH_DIR}/SectionComposer.cpp ${KARCH_DIR}/DimensionAnalyzer.cpp ${KARCH_DIR}/DxfWriter.cpp)
target_include_directories(karch PUBLIC ${KARCH_DIR})

# ---- 도판 엔진 (문서·래스터·도판·내보내기, 헤드리스 테스트 공용) -------------
add_library(studio_core STATIC
  src/ArtifactDoc.cpp src/ArtifactDoc.h
  src/CutGesture.h
  src/CutPageText.h
  src/MeshIO.cpp src/MeshIO.h
  src/MeshRaster.cpp src/MeshRaster.h
  src/Plate.cpp src/Plate.h
  src/Theme.h)
set_target_properties(studio_core PROPERTIES AUTOMOC ON)
target_include_directories(studio_core PUBLIC src)
target_link_libraries(studio_core PUBLIC karch Qt6::Core Qt6::Gui)
if(STUDIO_ENABLE_PCH)
  target_precompile_headers(studio_core PRIVATE
    <vector> <array> <string> <cmath> <algorithm> <unordered_map>
    <QString> <QColor> <QImage> <QPainter> <QPointF> <QPolygonF>)
endif()

# ---- 앱 -------------------------------------------------------------------
# 화면 원본: 앱과 화면 통합 시험(test_studio_cutpage)이 함께 쓴다 (각자 컴파일 — 앱 링크 구조는 그대로)
set(STUDIO_GUI_SRC
  src/MainWindow.cpp src/MainWindow.h
  src/Ribbon.cpp src/Ribbon.h
  src/SheetView.cpp src/SheetView.h
  src/View3D.cpp src/View3D.h
  src/StartPage.cpp src/StartPage.h
  src/ExportDialog.cpp src/ExportDialog.h)
add_executable(DongArch3DStudio WIN32 src/main.cpp ${STUDIO_GUI_SRC})
set_target_properties(DongArch3DStudio PROPERTIES AUTOMOC ON)
target_link_libraries(DongArch3DStudio PRIVATE studio_core Qt6::Widgets)
if(STUDIO_ENABLE_PCH)
  target_precompile_headers(DongArch3DStudio PRIVATE
    <QWidget> <QMainWindow> <QPainter> <QPushButton> <QLabel> <QBoxLayout>)
endif()

# ---- 헤드리스 테스트 & K-Arch 단위 테스트 (CTest) --------------------------
enable_testing()
add_executable(test_studio_pipeline tests/test_pipeline.cpp)
target_link_libraries(test_studio_pipeline PRIVATE studio_core)
add_test(NAME studio_pipeline COMMAND test_studio_pipeline ${CMAKE_CURRENT_BINARY_DIR})
add_executable(test_studio_pose tests/test_pose.cpp)
target_link_libraries(test_studio_pose PRIVATE studio_core)
add_test(NAME studio_pose COMMAND test_studio_pose ${CMAKE_CURRENT_BINARY_DIR})
add_executable(test_studio_side tests/test_side.cpp)
target_link_libraries(test_studio_side PRIVATE studio_core)
add_test(NAME studio_side COMMAND test_studio_side ${CMAKE_CURRENT_BINARY_DIR})
# 기준선 레이어(중심선·구연선·바닥선)·정면 방위각 — 도판·SVG·DXF·래스터 (JOB-007)
add_executable(test_studio_reflines tests/test_reflines.cpp)
target_link_libraries(test_studio_reflines PRIVATE studio_core)
add_test(NAME studio_reflines COMMAND test_studio_reflines ${CMAKE_CURRENT_BINARY_DIR})
# 단면·외곽선을 기준선에 맞추기 — 편차 제한 곡선·반쪽 정사 실루엣·최대 편차 (JOB-008)
add_executable(test_studio_fit tests/test_fit.cpp)
target_link_libraries(test_studio_fit PRIVATE studio_core)
add_test(NAME studio_fit COMMAND test_studio_fit ${CMAKE_CURRENT_BINARY_DIR})
# 큰 스캔(25만 면 초과) 치수를 원본에서 — 7.7만 면 예제와 같은 값(열린·닫힌 입술, 기운 채 자동 세우기)·원본 독립 계산·정면각 불변·시간 기록 (JOB-009)
add_executable(test_studio_bigscan tests/test_bigscan.cpp)
target_link_libraries(test_studio_bigscan PRIVATE studio_core)
add_test(NAME studio_bigscan COMMAND test_studio_bigscan ${CMAKE_CURRENT_BINARY_DIR})
# 시간 기록(확정·놓기)이 다른 시험과 겹치지 않게, 100만 면 메모리 (CI 는 ctest --parallel 4)
set_tests_properties(studio_bigscan PROPERTIES RUN_SERIAL TRUE)
add_executable(test_studio_cut tests/test_cut.cpp)
target_link_libraries(test_studio_cut PRIVATE studio_core)
add_test(NAME studio_cut COMMAND test_studio_cut ${CMAKE_CURRENT_BINARY_DIR})
# 시간 측정(미리보기 16 ms)이 다른 시험과 겹치지 않게 (CI 는 ctest --parallel 4)
set_tests_properties(studio_cut PROPERTIES RUN_SERIAL TRUE)
# ② 막으로 자르기 페이지 화면 통합 (오프스크린 MainWindow — 원본 계산 ≈ 11번이라 혼자 돈다)
add_executable(test_studio_cutpage tests/test_cutpage.cpp ${STUDIO_GUI_SRC})
set_target_properties(test_studio_cutpage PROPERTIES AUTOMOC ON)
target_link_libraries(test_studio_cutpage PRIVATE studio_core Qt6::Widgets)
add_test(NAME studio_cutpage COMMAND test_studio_cutpage ${CMAKE_CURRENT_BINARY_DIR})
set_tests_properties(studio_cutpage PROPERTIES RUN_SERIAL TRUE)

if(STUDIO_BUILD_KARCH_TESTS)
  set(KARCH_TESTS
    test_taubin_annulus
    test_rim_detector
    test_axis_pca
    test_axis_odr
    test_section
    test_silhouette
    test_dimensions
    test_dxf
    test_dxf_units
    test_mesh_loader
    test_synthetic)
  foreach(t IN LISTS KARCH_TESTS)
    add_executable(${t} ${KARCH_DIR}/${t}.cpp)
    target_link_libraries(${t} PRIVATE karch)
    add_test(NAME karch_${t} COMMAND ${t})
  endforeach()
endif()
