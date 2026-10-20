# DongArch3D Studio — E2E 대용량 실측 유물(6183.obj) 벤치마크 및 정밀도 검증 테스트 등록
# 기준 사양: ORIGINAL_REQUEST.md, SCOPE.md, PROJECT.md Milestone 6

add_executable(test_studio_realartifact ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_studio_realartifact.cpp)
target_link_libraries(test_studio_realartifact PRIVATE studio_core Qt6::Widgets)
add_test(NAME studio_bench_6183 COMMAND test_studio_realartifact)
set_tests_properties(studio_bench_6183 PROPERTIES RUN_SERIAL TRUE)

add_executable(test_mesh_raster_stress ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_mesh_raster_stress.cpp)
target_link_libraries(test_mesh_raster_stress PRIVATE studio_core Qt6::Widgets)
add_test(NAME studio_mesh_raster_stress COMMAND test_mesh_raster_stress)
set_tests_properties(studio_mesh_raster_stress PROPERTIES RUN_SERIAL TRUE)

add_executable(test_challenger_m2 ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_challenger_m2.cpp)
target_link_libraries(test_challenger_m2 PRIVATE studio_core Qt6::Widgets)
add_test(NAME studio_challenger_m2 COMMAND test_challenger_m2)
set_tests_properties(studio_challenger_m2 PROPERTIES RUN_SERIAL TRUE)

add_executable(test_milestone3_features ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_milestone3_features.cpp ${STUDIO_GUI_SRC})
set_target_properties(test_milestone3_features PROPERTIES AUTOMOC ON)
target_link_libraries(test_milestone3_features PRIVATE studio_core Qt6::Widgets)
add_test(NAME studio_milestone3 COMMAND test_milestone3_features ${CMAKE_CURRENT_BINARY_DIR})
set_tests_properties(studio_milestone3 PROPERTIES RUN_SERIAL TRUE)

add_executable(test_challenger_m3_2 ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_challenger_m3_2.cpp ${STUDIO_GUI_SRC})
set_target_properties(test_challenger_m3_2 PROPERTIES AUTOMOC ON)
target_link_libraries(test_challenger_m3_2 PRIVATE studio_core Qt6::Widgets)
add_test(NAME studio_challenger_m3_2 COMMAND test_challenger_m3_2 ${CMAKE_CURRENT_BINARY_DIR})
set_tests_properties(studio_challenger_m3_2 PROPERTIES RUN_SERIAL TRUE)
add_executable(test_challenger_m3_1 ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_challenger_m3_1.cpp ${STUDIO_GUI_SRC})
set_target_properties(test_challenger_m3_1 PROPERTIES AUTOMOC ON)
target_link_libraries(test_challenger_m3_1 PRIVATE studio_core Qt6::Widgets)
add_test(NAME studio_challenger_m3_1 COMMAND test_challenger_m3_1 ${CMAKE_CURRENT_BINARY_DIR})
set_tests_properties(studio_challenger_m3_1 PROPERTIES RUN_SERIAL TRUE)

add_executable(test_challenger_m3_3 ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_challenger_m3_3.cpp ${STUDIO_GUI_SRC})
set_target_properties(test_challenger_m3_3 PROPERTIES AUTOMOC ON)
target_link_libraries(test_challenger_m3_3 PRIVATE studio_core Qt6::Widgets)
add_test(NAME studio_challenger_m3_3 COMMAND test_challenger_m3_3 ${CMAKE_CURRENT_BINARY_DIR})
set_tests_properties(studio_challenger_m3_3 PROPERTIES RUN_SERIAL TRUE)

add_executable(test_challenger_m4_1 ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_challenger_m4_1.cpp)
target_link_libraries(test_challenger_m4_1 PRIVATE studio_core Qt6::Widgets)
add_test(NAME studio_challenger_m4_1 COMMAND test_challenger_m4_1)
set_tests_properties(studio_challenger_m4_1 PROPERTIES RUN_SERIAL TRUE)

add_executable(test_challenger_m4_2 ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_challenger_m4_2.cpp)
target_link_libraries(test_challenger_m4_2 PRIVATE studio_core Qt6::Widgets)
add_test(NAME studio_challenger_m4_2 COMMAND test_challenger_m4_2 ${CMAKE_CURRENT_BINARY_DIR})
set_tests_properties(studio_challenger_m4_2 PROPERTIES RUN_SERIAL TRUE)

add_executable(test_easy_workflow ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_easy_workflow.cpp ${STUDIO_GUI_SRC})
set_target_properties(test_easy_workflow PROPERTIES AUTOMOC ON)
target_link_libraries(test_easy_workflow PRIVATE studio_core Qt6::Widgets)
add_test(NAME studio_easy_workflow COMMAND test_easy_workflow ${CMAKE_CURRENT_BINARY_DIR})
set_tests_properties(studio_easy_workflow PROPERTIES RUN_SERIAL TRUE)



