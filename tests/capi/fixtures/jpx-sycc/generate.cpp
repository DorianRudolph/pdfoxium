// Synthetic JPEG2000 fixtures for public API tests; no production dependency.
// Build from repository root:
// g++ -std=c++17 -I upstream/pdfium/third_party/libopenjpeg \
//   tests/capi/fixtures/jpx-sycc/generate.cpp -l:libopenjp2.so.7 -o /tmp/jpx-sycc
// /tmp/jpx-sycc tests/capi/fixtures/jpx-sycc
#include "openjpeg.h"
#include <cstdio>
#include <cstdlib>
#include <string>

static void Error(const char* message, void*) { std::fprintf(stderr, "%s", message); }
int main(int argc, char** argv) {
  if (argc != 2) return 2;
  for (int vertical : {1, 2}) for (int width : {4, 5}) for (int height : {4, 5})
    for (int precision : {8, 12, 16}) for (bool alpha : {false, true})
      for (bool pattern : {false, true}) {
      // Alpha adds distinct public soft-mask behavior at 8-bit precision.
      if (alpha && precision != 8) continue;
      if (pattern && (alpha || precision != 8)) continue;
      opj_image_cmptparm_t components[4]{};
      for (int c = 0; c < (alpha ? 4 : 3); ++c) {
        auto& p = components[c];
        p.dx = c == 1 || c == 2 ? 2 : 1;
        p.dy = c == 1 || c == 2 ? vertical : 1;
        p.w = (width + p.dx - 1) / p.dx;
        p.h = (height + p.dy - 1) / p.dy;
        p.prec = precision;
      }
      auto* image = opj_image_create(alpha ? 4 : 3, components, OPJ_CLRSPC_SYCC);
      if (!image) return 3;
      image->x1 = width; image->y1 = height;
      for (int c = 0; c < (alpha ? 4 : 3); ++c) {
        auto& component = image->comps[c];
        for (unsigned y = 0; y < component.h; ++y) for (unsigned x = 0; x < component.w; ++x) {
          const int value = pattern ? (c == 0 ? 40 + 20 * x + 10 * y : c == 1 ? 112 + 8 * x : 128 + 8 * y)
                                    : (c == 0 ? 100 : 128);
          component.data[y * component.w + x] = value << (precision - 8);
        }
      }
      if (alpha) image->comps[3].alpha = 1;
      opj_cparameters_t params;
      opj_set_default_encoder_parameters(&params);
      params.tcp_numlayers = 1; params.tcp_rates[0] = 0;
      params.cp_disto_alloc = 1; params.numresolution = 1;
      params.irreversible = 0; params.tcp_mct = 0;
      auto* codec = opj_create_compress(OPJ_CODEC_JP2);
      opj_set_error_handler(codec, Error, nullptr);
      const std::string name = std::string(argv[1]) + "/sycc-42" +
          (vertical == 1 ? "2" : "0") + "-" + std::to_string(width) + "x" +
          std::to_string(height) + "-" + std::to_string(precision) + (alpha ? "-alpha" : "") +
          (pattern ? "-pattern" : "") + ".jp2";
      auto* stream = opj_stream_create_default_file_stream(name.c_str(), OPJ_FALSE);
      const bool ok = stream && opj_setup_encoder(codec, &params, image) &&
          opj_start_compress(codec, image, stream) && opj_encode(codec, stream) &&
          opj_end_compress(codec, stream);
      opj_stream_destroy(stream); opj_destroy_codec(codec); opj_image_destroy(image);
      if (!ok) { std::fprintf(stderr, "Cannot encode %s\n", name.c_str()); return 4; }
    }
}
