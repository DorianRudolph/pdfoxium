#pragma once

#include "harness.h"

#include <array>
#include <cstdint>
#include <functional>
#include <string>
#include <utility>
#include <vector>

namespace pdfoxium::integration {

// Capture the deleter from the creating API table, including its calling
// convention. Ownership is never transferred between implementations.
template <typename T, typename Deleter>
class Handle {
 public:
  Handle(T value, Deleter deleter) : value_(value), deleter_(deleter) {}
  ~Handle() { reset(); }
  Handle(const Handle&) = delete;
  Handle& operator=(const Handle&) = delete;
  Handle(Handle&& other) noexcept
      : value_(other.release()), deleter_(other.deleter_) {}
  Handle& operator=(Handle&& other) noexcept {
    if (this != &other) {
      reset();
      value_ = other.release();
      deleter_ = other.deleter_;
    }
    return *this;
  }
  T get() const { return value_; }
  T release() { return std::exchange(value_, nullptr); }
  void reset() {
    if (value_) deleter_(std::exchange(value_, nullptr));
  }

 private:
  T value_;
  Deleter deleter_;
};

struct Raster {
  int width = 0;
  int height = 0;
  // Packed top-down BGRA, copied from the owning implementation's live bitmap.
  std::vector<uint8_t> pixels;
  // Pixels per logical comparison coordinate (72 DPI).
  double render_scale = 1.0;
};

class Artifacts {
 public:
  ~Artifacts();
  void Pdf(const std::string& name, const std::vector<uint8_t>& bytes);
  void Image(const std::string& name, const Raster& raster);
  void Text(const std::string& name, const std::string& text);

 private:
  std::vector<std::pair<std::string, std::vector<uint8_t>>> files_;
};

struct MemoryWriter : FPDF_FILEWRITE {
  MemoryWriter();
  static int Append(FPDF_FILEWRITE* interface, const void* data,
                    unsigned long size) noexcept;
  size_t calls = 0;
  bool failed = false;
  std::vector<uint8_t> bytes;
};

std::array<const Library*, 2> Implementations();
Raster Render(const Api& api, FPDF_PAGE page, int width, int height,
              int flags = 0, int rotation = 0);
// Only flag-contract tests bypass the portable comparison rendering policy.
Raster RenderWithExactFlags(const Api& api, FPDF_PAGE page, int width, int height,
                           int flags, int rotation = 0);
// Complex font edges can differ between rendering backends. Bound both the
// number of exceptional pixels and total error; crop to the affected element
// rather than hiding a small missing widget in a large white page.
struct RasterTolerance {
  // All-zero requests an exact raster contract. Nonzero legacy tolerances
  // select the shared stroke-tolerant comparison (connected error mass <= 1).
  // The old channel/fraction values no longer set differential pass criteria.
  int channel_delta = 0;
  double max_outlier_pixel_fraction = 0;
  double max_mean_channel_delta = 0;
  // Explicit metric limit; zero keeps the legacy selection/default of 1.0.
  double max_connected_error_mass = 0;
  static RasterTolerance ConnectedError(double limit) {
    RasterTolerance tolerance;
    tolerance.max_connected_error_mass = limit;
    return tolerance;
  }
};
// Optional sidecar captures (default 1/2/4/8x; PDFOXIUM_CAPI_FORM_SCALES overrides).
// Ordinary assertions keep their raster; enabled by PDFOXIUM_CAPI_FORM_SCALE_DIR.
void CaptureFormRenderScales(const Api& api, const Raster& baseline,
                             const std::function<Raster(int)>& render);
Raster CropRaster(const Raster& raster, int left, int top, int width, int height);
Raster CropLogicalRaster(const Raster& raster, int left, int top, int width, int height);
void CompareRaster(const Raster& expected, const Raster& actual,
                   Artifacts& artifacts, const std::string& name,
                   int max_channel_delta = 0);
void CompareRaster(const Raster& expected, const Raster& actual,
                   Artifacts& artifacts, const std::string& name,
                   RasterTolerance tolerance);
std::vector<uint8_t> SaveDocument(const Api& api, FPDF_DOCUMENT document,
                                 Artifacts& artifacts, const std::string& name,
                                 FPDF_DWORD flags = FPDF_NO_INCREMENTAL,
                                 int version = 0);

std::filesystem::path TestRoot();
// Fixture-only serialization for structures without public constructors. Object
// bodies are numbered from 1, with object 1 as the catalog. This is intentionally
// a small classic-xref fixture builder, not a second document implementation.
std::vector<uint8_t> PdfFixture(const std::vector<std::string>& objects,
                                const std::string& trailer_entries = "",
                                int version = 17);
std::string PdfStream(const std::string& dictionary, const std::string& data);

}  // namespace pdfoxium::integration
