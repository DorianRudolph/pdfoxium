#include "test_support.h"

#include <algorithm>
#include <cmath>
#include <cstdlib>
#include <exception>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <map>
#include <limits>
#include <sstream>

#include <gtest/gtest.h>

extern "C" double pdfoxium_test_raster_score(const uint8_t*, const uint8_t*, size_t, size_t);

namespace pdfoxium::integration {

Artifacts::~Artifacts() {
  if (!::testing::Test::HasFailure()) return;
  try {
#ifdef _WIN32
    const wchar_t* configured = _wgetenv(L"PDFOXIUM_CAPI_ARTIFACT_DIR");
#else
    const char* configured = std::getenv("PDFOXIUM_CAPI_ARTIFACT_DIR");
#endif
    const auto root = configured ? std::filesystem::path(configured)
                                 : std::filesystem::path("target/capi-artifacts");
    const auto* info = ::testing::UnitTest::GetInstance()->current_test_info();
    const auto directory = root / (info ? info->test_suite_name() : "Lifecycle") /
                           (info ? info->name() : "startup");
    std::filesystem::create_directories(directory);
    for (const auto& file : files_) {
      const auto path = directory / file.first;
      std::ofstream stream(path, std::ios::binary);
      stream.write(reinterpret_cast<const char*>(file.second.data()),
                   static_cast<std::streamsize>(file.second.size()));
      if (!stream) {
        std::cerr << "Could not write failure artifact: " << path.u8string() << '\n';
      }
    }
    std::cerr << "Failure artifacts: " << directory.u8string() << '\n';
  } catch (const std::exception& error) {
    std::cerr << "Could not retain failure artifacts: " << error.what() << '\n';
  }
}

void Artifacts::Pdf(const std::string& name, const std::vector<uint8_t>& bytes) {
  files_.emplace_back(name + ".pdf", bytes);
}

void Artifacts::Text(const std::string& name, const std::string& text) {
  files_.emplace_back(name + ".txt", std::vector<uint8_t>(text.begin(), text.end()));
}

namespace {
std::vector<uint8_t> EncodePpm(const Raster& raster) {
  const std::string header = "P6\n" + std::to_string(raster.width) + " " +
                             std::to_string(raster.height) + "\n255\n";
  std::vector<uint8_t> ppm(header.begin(), header.end());
  ppm.reserve(ppm.size() + raster.pixels.size() / 4 * 3);
  for (size_t index = 0; index < raster.pixels.size(); index += 4) {
    ppm.push_back(raster.pixels[index + 2]);
    ppm.push_back(raster.pixels[index + 1]);
    ppm.push_back(raster.pixels[index]);
  }
  return ppm;
}
}  // namespace

void Artifacts::Image(const std::string& name, const Raster& raster) {
  if (!raster_comparison_settings().enabled) return;
  if (raster.width <= 0 || raster.height <= 0 ||
      raster.pixels.size() != static_cast<size_t>(raster.width) * raster.height * 4) return;
  files_.emplace_back(name + ".ppm", EncodePpm(raster));
}

void CaptureFormRenderScales(const Api& api, const Raster& baseline,
                             const std::function<Raster(int)>& render) {
  if (!raster_comparison_settings().enabled) return;
  static thread_local bool active=false;
  if(active)return;  // Additional Draw calls must not recursively capture.
  const char* root=std::getenv("PDFOXIUM_CAPI_FORM_SCALE_DIR");
  if(!root||!*root)return;
  const auto* test=::testing::UnitTest::GetInstance()->current_test_info();
  if(!test)return;
  std::string backend;
  for(const auto* library:Implementations())if(&library->api()==&api)backend=library->name();
  ASSERT_FALSE(backend.empty());
  const std::string case_name=std::string(test->test_suite_name())+"/"+test->name();
  static thread_local std::map<std::pair<std::string,std::string>,size_t> counts;
  const size_t frame=counts[{case_name,backend}]++;
  struct Guard { bool& active; explicit Guard(bool& value):active(value){active=true;} ~Guard(){active=false;} } guard(active);
  const auto directory=std::filesystem::path(root)/case_name/backend;
  std::filesystem::create_directories(directory);
  std::vector<int> scales{1,2,4,8};
  if(const char* value=std::getenv("PDFOXIUM_CAPI_FORM_SCALES")) {
    std::string list(value);std::replace(list.begin(),list.end(),',',' ');
    std::istringstream input(list);int scale;
    scales.clear();
    while(input>>scale) {
      ASSERT_GE(scale,1);ASSERT_LE(scale,16);
      scales.push_back(scale);
    }
    ASSERT_TRUE(input.eof())<<"Expected comma-separated integer render scales";
    ASSERT_FALSE(scales.empty());
  }
  for(int scale:scales) {
    const Raster image=scale==1?baseline:render(scale);
    ASSERT_GT(image.width,0);ASSERT_GT(image.height,0);
    ASSERT_EQ(image.pixels.size(),static_cast<size_t>(image.width)*image.height*4);
    bool opaque=true;
    for(size_t i=3;i<image.pixels.size();i+=4)opaque&=image.pixels[i]==255;
    EXPECT_TRUE(opaque)<<"Form scale captures on a white background must remain opaque";
    const auto path=directory/("frame-"+std::to_string(frame)+"-scale-"+std::to_string(scale)+".ppm");
    const auto bytes=EncodePpm(image);
    std::ofstream stream(path,std::ios::binary);
    stream.write(reinterpret_cast<const char*>(bytes.data()),static_cast<std::streamsize>(bytes.size()));
    ASSERT_TRUE(stream.good())<<path.u8string();
  }
}

MemoryWriter::MemoryWriter() {
  version = 1;
  WriteBlock = Append;
}

int MemoryWriter::Append(FPDF_FILEWRITE* interface, const void* data,
                         unsigned long size) noexcept {
  auto* self = static_cast<MemoryWriter*>(interface);
  ++self->calls;
  try {
    if (size != 0) {
      const auto* bytes = static_cast<const uint8_t*>(data);
      self->bytes.insert(self->bytes.end(), bytes, bytes + size);
    }
    return 1;
  } catch (...) {
    self->failed = true;
    return 0;
  }
}

std::array<const Library*, 2> Implementations() {
  return {&libraries().native, &libraries().rust};
}

Raster Render(const Api& api, FPDF_PAGE page, int width, int height, int flags,
              int rotation) {
  // Disable platform text rendering in the native oracle on every host.
  return RenderWithExactFlags(api, page, width, height,
                              flags | FPDF_NO_NATIVETEXT, rotation);
}

Raster RenderWithExactFlags(const Api& api, FPDF_PAGE page, int width, int height,
                           int flags, int rotation) {
  if (!page || width <= 0 || height <= 0) {
    ADD_FAILURE() << "Render requires a live page and positive dimensions";
    return {};
  }
  Handle bitmap(api.FPDFBitmap_CreateEx(width, height, FPDFBitmap_BGRA, nullptr, 0),
                api.FPDFBitmap_Destroy);
  if (!bitmap.get()) {
    ADD_FAILURE() << "Could not allocate render bitmap";
    return {};
  }
  EXPECT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, width, height, 0xffffffff));
  api.FPDF_RenderPageBitmap(bitmap.get(), page, 0, 0, width, height, rotation, flags);
  const auto* buffer = static_cast<const uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap.get()));
  const int stride = api.FPDFBitmap_GetStride(bitmap.get());
  if (!buffer || stride < width * 4) {
    ADD_FAILURE() << "Invalid render bitmap storage";
    return {};
  }
  Raster raster{width, height, std::vector<uint8_t>(static_cast<size_t>(width) * height * 4)};
  for (int row = 0; row < height; ++row) {
    std::copy_n(buffer + static_cast<size_t>(row) * stride, width * 4,
                raster.pixels.begin() + static_cast<size_t>(row) * width * 4);
  }
  return raster;
}

void CompareRaster(const Raster& expected, const Raster& actual,
                   Artifacts& artifacts, const std::string& name,
                   int max_channel_delta) {
  CompareRaster(expected, actual, artifacts, name,
                RasterTolerance{max_channel_delta, 0,
                                static_cast<double>(max_channel_delta)});
}

Raster CropRaster(const Raster& raster, int left, int top, int width, int height) {
  if (left < 0 || top < 0 || width <= 0 || height <= 0 ||
      left > raster.width || width > raster.width - left ||
      top > raster.height || height > raster.height - top ||
      raster.pixels.size() != static_cast<size_t>(raster.width) * raster.height * 4) {
    ADD_FAILURE() << "Invalid raster crop";
    return {};
  }
  Raster result{width, height, std::vector<uint8_t>(static_cast<size_t>(width) * height * 4)};
  result.render_scale = raster.render_scale;
  for (int row = 0; row < height; ++row) {
    const auto offset = (static_cast<size_t>(top + row) * raster.width + left) * 4;
    std::copy_n(raster.pixels.begin() + offset, width * 4,
                result.pixels.begin() + static_cast<size_t>(row) * width * 4);
  }
  return result;
}

Raster CropLogicalRaster(const Raster& raster, int left, int top, int width, int height) {
  const double scale = raster.render_scale;
  const double right = (static_cast<double>(left) + width) * scale;
  const double bottom = (static_cast<double>(top) + height) * scale;
  if (left < 0 || top < 0 || width <= 0 || height <= 0 ||
      !std::isfinite(scale) || scale <= 0 ||
      !std::isfinite(right) || !std::isfinite(bottom) ||
      right > std::numeric_limits<int>::max() || bottom > std::numeric_limits<int>::max()) {
    ADD_FAILURE() << "Invalid logical raster crop";
    return {};
  }
  // Round shared edges consistently so adjacent logical crops stay adjacent.
  const int pixel_left = static_cast<int>(std::round(left * scale));
  const int pixel_top = static_cast<int>(std::round(top * scale));
  return CropRaster(raster, pixel_left, pixel_top,
                    static_cast<int>(std::round(right)) - pixel_left,
                    static_cast<int>(std::round(bottom)) - pixel_top);
}

void CompareRaster(const Raster& expected, const Raster& actual,
                   Artifacts& artifacts, const std::string& name,
                   RasterTolerance tolerance) {
  ASSERT_GT(expected.width, 0) << name;
  ASSERT_GT(expected.height, 0) << name;
  ASSERT_EQ(expected.width, actual.width) << name;
  ASSERT_EQ(expected.height, actual.height) << name;
  ASSERT_EQ(expected.pixels.size(), static_cast<size_t>(expected.width) * expected.height * 4);
  ASSERT_EQ(expected.pixels.size(), actual.pixels.size()) << name;
  ASSERT_GE(tolerance.channel_delta, 0);
  ASSERT_LE(tolerance.channel_delta, 255);
  ASSERT_GE(tolerance.max_outlier_pixel_fraction, 0);
  ASSERT_LE(tolerance.max_outlier_pixel_fraction, 1);
  ASSERT_GE(tolerance.max_mean_channel_delta, 0);
  ASSERT_LE(tolerance.max_mean_channel_delta, 255);
  ASSERT_TRUE(std::isfinite(tolerance.max_connected_error_mass));
  ASSERT_GE(tolerance.max_connected_error_mass, 0);
  if (!raster_comparison_settings().enabled) return;
  if (expected.pixels == actual.pixels) return;
  // Exact raster contracts remain exact. Differential comparisons with an
  // explicit tolerance use radius-2 transport, 10% stroke contrast leeway and
  // strongest connected residual mass above .01. One unit is one fully wrong
  // pixel's worth of error; this candidate separates the book/defect controls.
  if (tolerance.channel_delta || tolerance.max_outlier_pixel_fraction ||
      tolerance.max_mean_channel_delta || tolerance.max_connected_error_mass) {
    const double limit = tolerance.max_connected_error_mass > 0
        ? tolerance.max_connected_error_mass : 1.0;
    const double score = pdfoxium_test_raster_score(expected.pixels.data(), actual.pixels.data(),
                                                  expected.width, expected.height);
    if (score <= limit) return;
    artifacts.Image(name + "-expected", expected);
    artifacts.Image(name + "-actual", actual);
    artifacts.Text(name + "-comparison", "strongest connected residual mass: " + std::to_string(score)
        + "\nallowed mass: " + std::to_string(limit));
    EXPECT_LE(score, limit) << name << ": strongest connected residual mass (radius 2, stroke leeway .10)";
    return;
  }
  artifacts.Image(name + "-expected", expected);
  artifacts.Image(name + "-actual", actual);
  Raster difference = actual;
  size_t outlier_pixels = 0;
  uint64_t total_delta = 0;
  int maximum_delta = 0;
  for (size_t pixel = 0; pixel < expected.pixels.size(); pixel += 4) {
    bool outlier = false;
    for (size_t channel = 0; channel < 4; ++channel) {
      const size_t index = pixel + channel;
      const int delta = std::abs(static_cast<int>(expected.pixels[index]) - actual.pixels[index]);
      outlier |= delta > tolerance.channel_delta;
      total_delta += delta;
      maximum_delta = std::max(maximum_delta, delta);
      difference.pixels[index] = static_cast<uint8_t>(delta);
    }
    outlier_pixels += outlier;
  }
  const double outlier_fraction = static_cast<double>(outlier_pixels) /
                                 (expected.pixels.size() / 4);
  const double mean_delta = static_cast<double>(total_delta) / expected.pixels.size();
  std::ostringstream summary;
  summary << "allowed channel delta: " << tolerance.channel_delta << '\n'
          << "allowed outlier fraction: " << tolerance.max_outlier_pixel_fraction << '\n'
          << "allowed mean channel delta: " << tolerance.max_mean_channel_delta << '\n'
          << "maximum channel delta: " << maximum_delta << '\n'
          << "outlier pixels: " << outlier_pixels << '\n'
          << "outlier fraction: " << outlier_fraction << '\n'
          << "mean channel delta: " << mean_delta << '\n';
  artifacts.Text(name + "-comparison", summary.str());
  EXPECT_LE(outlier_fraction, tolerance.max_outlier_pixel_fraction) << name << '\n' << summary.str();
  EXPECT_LE(mean_delta, tolerance.max_mean_channel_delta) << name << '\n' << summary.str();
  if (outlier_pixels) artifacts.Image(name + "-difference", difference);
}

std::vector<uint8_t> SaveDocument(const Api& api, FPDF_DOCUMENT document,
                                 Artifacts& artifacts, const std::string& name,
                                 FPDF_DWORD flags, int version) {
  MemoryWriter sink;
  const auto result = version ? api.FPDF_SaveWithVersion(document, &sink, flags, version)
                              : api.FPDF_SaveAsCopy(document, &sink, flags);
  artifacts.Pdf(name, sink.bytes);
  EXPECT_TRUE(result) << name;
  EXPECT_GT(sink.calls, 0u) << name;
  EXPECT_FALSE(sink.failed) << name;
  EXPECT_FALSE(sink.bytes.empty()) << name;
  return std::move(sink.bytes);
}

std::filesystem::path TestRoot() { return std::filesystem::u8path(PDFOXIUM_TEST_ROOT); }

std::string PdfStream(const std::string& dictionary, const std::string& data) {
  return "<< " + dictionary + " /Length " + std::to_string(data.size()) +
         " >>\nstream\n" + data + "\nendstream";
}

std::vector<uint8_t> PdfFixture(const std::vector<std::string>& objects,
                                const std::string& trailer_entries, int version) {
  std::ostringstream pdf;
  pdf << "%PDF-" << version / 10 << '.' << version % 10 << "\n";
  std::vector<std::streamoff> offsets{0};
  for (size_t index = 0; index < objects.size(); ++index) {
    offsets.push_back(pdf.tellp());
    pdf << index + 1 << " 0 obj\n" << objects[index] << "\nendobj\n";
  }
  const auto xref = pdf.tellp();
  pdf << "xref\n0 " << offsets.size() << "\n0000000000 65535 f \n";
  for (size_t index = 1; index < offsets.size(); ++index) {
    pdf << std::setfill('0') << std::setw(10) << offsets[index] << " 00000 n \n";
  }
  pdf << "trailer\n<< /Size " << offsets.size() << " /Root 1 0 R "
      << trailer_entries << " >>\nstartxref\n" << xref << "\n%%EOF\n";
  const auto bytes = pdf.str();
  return {bytes.begin(), bytes.end()};
}

}  // namespace pdfoxium::integration
