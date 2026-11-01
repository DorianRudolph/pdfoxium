cmake_minimum_required(VERSION 3.24)
project(pdfoxium_capi_tests LANGUAGES CXX)

include(CTest)
include(FetchContent)
find_package(Python3 3.8 REQUIRED COMPONENTS Interpreter)

get_filename_component(PDFOXIUM_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../.." ABSOLUTE)
set(PDFOXIUM_PUBLIC "${PDFOXIUM_ROOT}/upstream/pdfium/public")
if(NOT EXISTS "${PDFOXIUM_PUBLIC}/fpdfview.h")
  message(FATAL_ERROR "Initialize the pinned upstream/pdfium submodule first")
endif()

# Use an installed GTest, or a checksum-pinned upstream release. An offline
# checkout can use FETCHCONTENT_SOURCE_DIR_GOOGLETEST instead of downloading.
find_package(GTest 1.14 CONFIG QUIET)
if(NOT GTest_FOUND)
  set(INSTALL_GTEST OFF CACHE BOOL "" FORCE)
  set(BUILD_GMOCK OFF CACHE BOOL "" FORCE)
  set(gtest_force_shared_crt ON CACHE BOOL "" FORCE)
  FetchContent_Declare(googletest
    URL https://github.com/google/googletest/archive/refs/tags/v1.17.0.tar.gz
    URL_HASH SHA256=65fab701d9829d38cb77c14acdc431d2108bfdbf8979e40eb8ae567edf10b27c
    DOWNLOAD_EXTRACT_TIMESTAMP TRUE)
  FetchContent_MakeAvailable(googletest)
endif()

file(GLOB public_headers CONFIGURE_DEPENDS "${PDFOXIUM_PUBLIC}/*.h")
set(generated_dir "${CMAKE_CURRENT_BINARY_DIR}/generated")
add_custom_command(
  OUTPUT "${generated_dir}/api_headers.inc" "${generated_dir}/api_functions.inc"
  COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/generate_api.py"
          --output-dir "${generated_dir}"
  DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/generate_api.py"
          "${PDFOXIUM_ROOT}/scripts/audit-capi-tests.py"
          "${PDFOXIUM_ROOT}/tests/capi/exclusions.tsv" ${public_headers}
  VERBATIM)

# Keep generated PDFs in the build tree; the pinned templates and JPX includes
# remain read-only. The expected RGBA fixtures are already checked in.
set(pinned_pixel_dir "${generated_dir}/pixel-fixtures")
set(pinned_pixel_stems
  type3 type3_xobject shade shade-tensor image_transformer_other
  icc_profile_bad_component icc_profile_bad_value
  jpxdecode_without_colorspace jpxdecode_with_mismatch_colorspace
  jpxdecode_smask_in_data_2)
set(pinned_pixel_sources)
set(pinned_pixel_outputs)
foreach(stem IN LISTS pinned_pixel_stems)
  list(APPEND pinned_pixel_sources "${PDFOXIUM_ROOT}/upstream/pdfium/testing/resources/pixel/${stem}.in")
  list(APPEND pinned_pixel_outputs "${pinned_pixel_dir}/${stem}.pdf")
endforeach()
set(pinned_pixel_includes)
foreach(name gray.jp2 gray-alpha.jp2 RGB.jp2 RGB-alpha.jp2 CMYK.jpf CMYK-alpha.jpf)
  list(APPEND pinned_pixel_includes "${PDFOXIUM_ROOT}/upstream/pdfium/testing/resources/${name}")
endforeach()
foreach(name black-red black-green black-blue green-white red-yellow blue-white)
  list(APPEND pinned_pixel_includes "${PDFOXIUM_ROOT}/upstream/pdfium/testing/resources/pixel/SMaskInData2/${name}.jp2")
endforeach()
add_custom_command(
  OUTPUT ${pinned_pixel_outputs}
  COMMAND "${CMAKE_COMMAND}" -E make_directory "${pinned_pixel_dir}"
  COMMAND "${Python3_EXECUTABLE}" "${PDFOXIUM_ROOT}/upstream/pdfium/testing/tools/fixup_pdf_template.py"
          --output-dir "${pinned_pixel_dir}" ${pinned_pixel_sources}
  DEPENDS "${PDFOXIUM_ROOT}/upstream/pdfium/testing/tools/fixup_pdf_template.py"
          ${pinned_pixel_sources} ${pinned_pixel_includes}
  VERBATIM)
add_custom_target(pdfoxium_pinned_pixel_fixtures DEPENDS ${pinned_pixel_outputs})

add_executable(pdfoxium_capi_tests
  main.cpp harness.cpp harness_tests.cpp test_support.cpp smoke_tests.cpp
  pinned_font_test_environment.cpp
  pinned_font_styles_tests.cpp
  bitmap_tests.cpp page_tests.cpp path_tests.cpp save_tests.cpp
  view_tests.cpp javascript_tests.cpp render_tests.cpp
  text_tests.cpp form_tests.cpp signature_tests.cpp attachment_tests.cpp
  loading_tests.cpp progressive_tests.cpp availability_tests.cpp thumbnail_tests.cpp
  flatten_tests.cpp xfa_form_tests.cpp
  font_tests.cpp appearance_font_tests.cpp annotation_tests.cpp annotation_widget_tests.cpp
  document_tests.cpp import_tests.cpp structure_tests.cpp
  object_tests.cpp image_tests.cpp clip_tests.cpp mark_tests.cpp
  system_font_tests.cpp lifecycle_tests.cpp save_extra_tests.cpp
  coverage_document_tests.cpp coverage_text_font_tests.cpp coverage_javascript_tests.cpp
  coverage_form_tests.cpp coverage_formcalc_tests.cpp
  coverage_editing_tests.cpp
  coverage_render_tests.cpp
  coverage_picture_tests.cpp coverage_compositing_tests.cpp
  coverage_inline_image_tests.cpp
  coverage_image_drawn_size_tests.cpp
  coverage_xfa_layout_tests.cpp
  coverage_document_script_tests.cpp
  coverage_global_script_tests.cpp
  coverage_codec_variant_tests.cpp coverage_font_table_tests.cpp
  coverage_graphics_state_tests.cpp coverage_validation_script_tests.cpp
  coverage_xfa_flow_tests.cpp coverage_xfa_text_input_tests.cpp
  coverage_form_permissions_tests.cpp coverage_image_pipeline_tests.cpp
  coverage_richtext_break_tests.cpp coverage_xfa_binding_layout_tests.cpp
  coverage_som_xml_tests.cpp
  coverage_barcode_variant_tests.cpp
  coverage_codec_deep_tests.cpp coverage_locale_service_tests.cpp
  coverage_widget_appearance_variant_tests.cpp
  coverage_alpha_render_tests.cpp coverage_xfa_traversal_bidi_tests.cpp
  coverage_xfa_schema_tests.cpp coverage_xfa_layout_variants_tests.cpp
  coverage_xfa_paint_tests.cpp
  coverage_loading_boundaries_tests.cpp coverage_barcode_boundary_tests.cpp
  coverage_pwl_history_scroll_tests.cpp coverage_indexed_widget_script_tests.cpp
  coverage_truetype_mapping_tests.cpp coverage_xfa_host_callbacks_tests.cpp
  coverage_formcalc_boundary_tests.cpp coverage_xfa_validation_binding_tests.cpp
  coverage_xfa_deep_pagination_tests.cpp coverage_backdrop_resampling_tests.cpp
  coverage_pinned_font_parser_tests.cpp coverage_pinned_edit_image_tests.cpp
  coverage_pinned_pixel_tests.cpp
  coverage_script_app_event_tests.cpp coverage_document_action_tree_tests.cpp
  coverage_xfa_arabic_tests.cpp coverage_xfa_script_context_tests.cpp
  coverage_legacy_security_availability_tests.cpp
  coverage_parser_recovery_boundaries_tests.cpp
  coverage_borrowed_object_identity_tests.cpp coverage_bitmap_device_paths_tests.cpp
  coverage_xfa_widget_boundaries_tests.cpp coverage_font_face_paths_tests.cpp
  coverage_xfa_data_merge_tests.cpp
  coverage_xfa_pagination_boundaries_tests.cpp
  coverage_parser_availability_recovery_tests.cpp
  coverage_xfa_layout_recovery_tests.cpp
  coverage_xfa_event_recovery_tests.cpp
  coverage_richtext_codec_recovery_tests.cpp
  coverage_jpx_subsampling_boundaries_tests.cpp
  coverage_render_font_recovery_tests.cpp
  coverage_javascript_service_recovery_tests.cpp
  coverage_widget_scroll_focus_tests.cpp
  coverage_stream_document_error_tests.cpp
  coverage_widget_capture_navigation_tests.cpp
  coverage_render_transparency_boundaries_tests.cpp
  coverage_script_field_actions_tests.cpp
  coverage_xfa_merge_page_recovery_tests.cpp
  coverage_xfa_picture_model_boundaries_tests.cpp
  coverage_font_provider_alias_boundaries_tests.cpp
  coverage_xfa_timezone_boundary_tests.cpp
  coverage_text_search_link_boundaries_tests.cpp
  coverage_formcalc_scalar_constraints_tests.cpp
  coverage_widgetless_field_constraints_tests.cpp
  coverage_progressive_page_tree_fallback_tests.cpp
  coverage_image_cache_limits_tests.cpp
  coverage_text_semantic_constraints_tests.cpp
  coverage_parser_range_security_tests.cpp
  coverage_xfa_layout_constraints_tests.cpp
  coverage_form_appearance_constraints_tests.cpp
  coverage_xfa_text_widget_constraints_tests.cpp
  coverage_script_resolution_constraints_tests.cpp
  coverage_render_bitmap_constraints_tests.cpp
  coverage_optional_host_services_tests.cpp
  coverage_xfa_packet_prototype_tests.cpp
  coverage_xfa_custom_locale_tests.cpp
  coverage_crypto_filter_constraints_tests.cpp
  coverage_progressive_retained_images_tests.cpp
  coverage_xfa_event_payload_tests.cpp
  coverage_progressive_image_variants_tests.cpp
  coverage_font_outline_variants_tests.cpp
  coverage_jpeg_sampling_variants_tests.cpp
  coverage_xfa_history_ring_tests.cpp
  coverage_xfa_named_images_tests.cpp
  coverage_xfa_plain_paragraph_tests.cpp
  coverage_xfa_pagination_interactions_tests.cpp
  coverage_xref_recovery_interactions_tests.cpp
  coverage_alpha_clip_blend_tests.cpp
  coverage_text_hit_testing_tests.cpp
  coverage_xfa_relayout_sequences_tests.cpp
  coverage_scroll_selection_sequences_tests.cpp
  coverage_encrypted_root_recovery_tests.cpp
  coverage_availability_resource_sequences_tests.cpp
  coverage_appearance_font_resource_tests.cpp
  coverage_edit_operation_sequences_tests.cpp
  coverage_form_window_recreation_tests.cpp
  coverage_onebit_image_roundtrip_tests.cpp
  coverage_font_charmap_enumeration_tests.cpp
  coverage_nested_widget_scroll_tests.cpp
  coverage_xfa_clone_rebinding_tests.cpp
  "${generated_dir}/api_headers.inc" "${generated_dir}/api_functions.inc")
add_dependencies(pdfoxium_capi_tests pdfoxium_pinned_pixel_fixtures)
set_source_files_properties(coverage_pinned_pixel_tests.cpp PROPERTIES
  COMPILE_DEFINITIONS "PDFOXIUM_PINNED_PIXEL_DIR=\"${pinned_pixel_dir}\"")
target_compile_features(pdfoxium_capi_tests PRIVATE cxx_std_17)
set_target_properties(pdfoxium_capi_tests PROPERTIES CXX_EXTENSIONS OFF)
target_include_directories(pdfoxium_capi_tests PRIVATE
  "${generated_dir}" "${PDFOXIUM_ROOT}/upstream/pdfium")
# Match the callback-structure layout of the XFA-enabled native oracle. The
# portable API omits the native V8/Skia integration exports by reviewed policy.
target_compile_definitions(pdfoxium_capi_tests PRIVATE
  PDF_ENABLE_XFA PDFOXIUM_TEST_ROOT="${PDFOXIUM_ROOT}")
if(WIN32)
  target_compile_definitions(pdfoxium_capi_tests PRIVATE FPDFSDK_EXPORTS WIN32)
  if(MINGW)
    target_link_options(pdfoxium_capi_tests PRIVATE -municode)
  endif()
endif()
target_link_libraries(pdfoxium_capi_tests PRIVATE GTest::gtest ${CMAKE_DL_LIBS})

# Small test-only Rust metric, called in memory (no image files or subprocesses).
find_program(PDFOXIUM_RUSTC rustc REQUIRED)
set(raster_metric "${CMAKE_CURRENT_BINARY_DIR}/${CMAKE_STATIC_LIBRARY_PREFIX}pdfoxium_test_raster${CMAKE_STATIC_LIBRARY_SUFFIX}")
add_custom_command(OUTPUT "${raster_metric}"
  COMMAND "${PDFOXIUM_RUSTC}" --edition=2021 --crate-type=staticlib -O
          "${CMAKE_CURRENT_SOURCE_DIR}/raster_metric.rs" -o "${raster_metric}"
  DEPENDS raster_metric.rs "${PDFOXIUM_ROOT}/crates/pdfoxium-diff/src/raster/mod.rs"
          "${PDFOXIUM_ROOT}/crates/pdfoxium-diff/src/raster/transport.rs"
  VERBATIM)
add_custom_target(pdfoxium_test_raster DEPENDS "${raster_metric}")
add_dependencies(pdfoxium_capi_tests pdfoxium_test_raster)
target_link_libraries(pdfoxium_capi_tests PRIVATE "${raster_metric}")
if(WIN32)
  target_link_libraries(pdfoxium_capi_tests PRIVATE ws2_32 userenv bcrypt ntdll)
elseif(NOT APPLE)
  target_link_libraries(pdfoxium_capi_tests PRIVATE pthread m)
endif()
if(MSVC)
  target_compile_options(pdfoxium_capi_tests PRIVATE /W4 /permissive- /utf-8)
else()
  target_compile_options(pdfoxium_capi_tests PRIVATE -Wall -Wextra -Wpedantic)
endif()

set(native_default "")
if(CMAKE_SYSTEM_NAME STREQUAL "Linux" AND CMAKE_SYSTEM_PROCESSOR MATCHES "^(x86_64|AMD64)$")
  set(native_default "${PDFOXIUM_ROOT}/vendor/pdfium/chromium-8100/linux-x64/libpdfium.so")
elseif(APPLE AND CMAKE_SYSTEM_PROCESSOR MATCHES "^(arm64|aarch64)$")
  set(native_default "${PDFOXIUM_ROOT}/vendor/pdfium/chromium-8100/mac-arm64/libpdfium.dylib")
endif()
if(WIN32)
  set(rust_name "pdfoxium_capi.dll")
elseif(APPLE)
  set(rust_name "libpdfoxium_capi.dylib")
else()
  set(rust_name "libpdfoxium_capi.so")
endif()
set(PDFOXIUM_NATIVE_LIBRARY "${native_default}" CACHE FILEPATH "Pinned native PDFium shared library")
set(PDFOXIUM_RUST_LIBRARY "${PDFOXIUM_ROOT}/target/debug/${rust_name}" CACHE FILEPATH "Rust C ABI shared library")

if(BUILD_TESTING)
  set(raster_metric_tests "${CMAKE_CURRENT_BINARY_DIR}/raster_metric_tests${CMAKE_EXECUTABLE_SUFFIX}")
  add_custom_command(OUTPUT "${raster_metric_tests}"
    COMMAND "${PDFOXIUM_RUSTC}" --edition=2021 --test -O
            "${CMAKE_CURRENT_SOURCE_DIR}/raster_metric.rs" -o "${raster_metric_tests}"
    DEPENDS raster_metric.rs "${PDFOXIUM_ROOT}/crates/pdfoxium-diff/src/raster/mod.rs"
          "${PDFOXIUM_ROOT}/crates/pdfoxium-diff/src/raster/transport.rs"
    VERBATIM)
  add_custom_target(pdfoxium_raster_metric_tests ALL DEPENDS "${raster_metric_tests}")
  add_test(NAME capi.raster_metric COMMAND "${raster_metric_tests}")
  # One CTest invocation owns the complete GTest suite. gtest_discover_tests
  # would launch a fresh process/lifecycle for each ordinary scenario.
  add_test(NAME capi.integration COMMAND pdfoxium_capi_tests
    --native-library "${PDFOXIUM_NATIVE_LIBRARY}"
    --rust-library "${PDFOXIUM_RUST_LIBRARY}")
  set_tests_properties(capi.integration PROPERTIES TIMEOUT 2700
    WORKING_DIRECTORY "${PDFOXIUM_ROOT}"
    ENVIRONMENT "PDFOXIUM_CAPI_ARTIFACT_DIR=${CMAKE_CURRENT_BINARY_DIR}/artifacts")
  # Host discovery/provider contracts keep a real host inventory in their own
  # process; ordinary scenarios use the checked-in font inventory by default.
  add_test(NAME capi.system_fonts COMMAND "${CMAKE_COMMAND}" -E env
    --unset=PDFOXIUM_CAPI_FONT_PATH $<TARGET_FILE:pdfoxium_capi_tests>
    --native-library "${PDFOXIUM_NATIVE_LIBRARY}"
    --rust-library "${PDFOXIUM_RUST_LIBRARY}" --host-fonts --gtest_filter=SystemFonts.*)
  set_tests_properties(capi.system_fonts PROPERTIES TIMEOUT 120
    WORKING_DIRECTORY "${PDFOXIUM_ROOT}"
    ENVIRONMENT "PDFOXIUM_CAPI_ARTIFACT_DIR=${CMAKE_CURRENT_BINARY_DIR}/artifacts")
  add_test(NAME capi.generator COMMAND "${Python3_EXECUTABLE}"
    "${CMAKE_CURRENT_SOURCE_DIR}/test_generate_api.py")
  add_test(NAME capi.coverage_report COMMAND "${Python3_EXECUTABLE}"
    "${PDFOXIUM_ROOT}/scripts/test_capi_coverage.py")
  # These cases explicitly exercise independent global-library lifecycles.
  # Ordinary scenarios above continue sharing one initialized process.
  foreach(config IN ITEMS default 1 2 3 4 5 6 7)
    add_test(NAME "capi.lifecycle.${config}" COMMAND pdfoxium_capi_tests
      --native-library "${PDFOXIUM_NATIVE_LIBRARY}"
      --rust-library "${PDFOXIUM_RUST_LIBRARY}" --lifecycle-config "${config}")
    set_tests_properties("capi.lifecycle.${config}" PROPERTIES TIMEOUT 120)
  endforeach()
  add_test(NAME capi.lifecycle.brotli COMMAND pdfoxium_capi_tests
    --native-library "${PDFOXIUM_NATIVE_LIBRARY}" --rust-library "${PDFOXIUM_RUST_LIBRARY}"
    --lifecycle-config 6 --brotli)
  add_test(NAME capi.lifecycle.isolation COMMAND pdfoxium_capi_tests
    --native-library "${PDFOXIUM_NATIVE_LIBRARY}" --rust-library "${PDFOXIUM_RUST_LIBRARY}"
    --lifecycle-config 7 --isolate)
  set_tests_properties(capi.lifecycle.brotli capi.lifecycle.isolation PROPERTIES TIMEOUT 120)
endif()
