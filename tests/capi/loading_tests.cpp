#include "runtime_fixture.h"

#include <chrono>
#include <cstring>
#include <limits>
#include <tuple>

namespace pdfoxium::integration {
namespace {

struct ReadSource {
  explicit ReadSource(const std::vector<uint8_t>& bytes) : bytes(bytes) {}
  FPDF_FILEACCESS Descriptor() {
    FPDF_FILEACCESS result{};
    result.m_FileLen = static_cast<unsigned long>(bytes.size());
    result.m_GetBlock = Read;
    result.m_Param = this;
    return result;
  }
  static int Read(void* opaque, unsigned long offset, unsigned char* buffer,
                  unsigned long size) noexcept {
    auto* self = static_cast<ReadSource*>(opaque);
    try {
      self->requests.emplace_back(offset, size);
      const bool valid = buffer && size && offset <= self->bytes.size() &&
                          size <= self->bytes.size() - offset;
      self->valid &= valid;
      if (!valid || self->reject) return 0;
      std::copy_n(self->bytes.data() + offset, size, buffer);
      return 1;
    } catch (...) { self->valid = false; return 0; }
  }
  const std::vector<uint8_t>& bytes;
  bool reject = false;
  bool valid = true;
  std::vector<std::pair<unsigned long, unsigned long>> requests;
};

class TemporaryFile {
 public:
  explicit TemporaryFile(const std::vector<uint8_t>& bytes) {
    directory = std::filesystem::temp_directory_path() /
        ("pdfoxium-capi-load-" + std::to_string(std::chrono::steady_clock::now().time_since_epoch().count()));
    if (!std::filesystem::create_directory(directory)) throw std::runtime_error("Cannot create private fixture directory");
    path = directory / std::filesystem::u8path("document-\xc3\xa9.pdf");
    std::ofstream stream(path, std::ios::binary);
    stream.write(reinterpret_cast<const char*>(bytes.data()), static_cast<std::streamsize>(bytes.size()));
    if (!stream) throw std::runtime_error("Cannot write temporary PDF fixture");
  }
  ~TemporaryFile() { std::error_code error; std::filesystem::remove_all(directory, error); }
  std::filesystem::path directory;
  std::filesystem::path path;
};

TEST(Loading, FileAndBothMemoryLengthsRenderIdenticalPdfWithLiveStorage) {
  Artifacts artifacts;
  const auto bytes = runtime_test::RectangleFixture();
  TemporaryFile file(bytes);
  std::array<std::array<Raster, 4>, 2> observations;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    SCOPED_TRACE(library->name());
    for (const char* password : {static_cast<const char*>(nullptr), ""}) {
      Handle from_file(api.FPDF_LoadDocument(file.path.u8string().c_str(), password), api.FPDF_CloseDocument);
      Handle narrow(api.FPDF_LoadMemDocument(bytes.data(), static_cast<int>(bytes.size()), password), api.FPDF_CloseDocument);
      Handle wide(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), password), api.FPDF_CloseDocument);
      ReadSource source(bytes);
      // The descriptor itself may leave scope; callback state and bytes may not.
      Handle custom([&] { auto descriptor = source.Descriptor(); return api.FPDF_LoadCustomDocument(&descriptor, password); }(), api.FPDF_CloseDocument);
      const std::array<FPDF_DOCUMENT, 4> documents = {from_file.get(), narrow.get(), wide.get(), custom.get()};
      for (size_t method = 0; method < documents.size(); ++method) {
        ASSERT_NE(documents[method], nullptr) << method << " error=" << api.FPDF_GetLastError();
        EXPECT_EQ(api.FPDF_GetPageCount(documents[method]), 1);
        EXPECT_EQ(api.FPDF_GetDocPermissions(documents[method]), 0xffffffffu);
        EXPECT_EQ(api.FPDF_GetDocUserPermissions(documents[method]), 0xffffffffu);
        EXPECT_EQ(api.FPDF_GetSecurityHandlerRevision(documents[method]), -1);
        ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(api, documents[method], &observations[backend][method]));
        runtime_test::ExpectRectangle(observations[backend][method]);
        CompareRaster(observations[backend][0], observations[backend][method], artifacts, library->name() + "-method" + std::to_string(method));
      }
      EXPECT_FALSE(source.requests.empty());
      EXPECT_TRUE(source.valid);
    }
    ++backend;
  }
  for (size_t method = 0; method < 4; ++method) CompareRaster(observations[0][method], observations[1][method], artifacts, "loader" + std::to_string(method));
}

TEST(Loading, ImmediateErrorsDistinguishFilesFormatsPasswordsAndUnsupportedSecurity) {
  const std::vector<uint8_t> malformed{'n', 'o', 't', ' ', 'p', 'd', 'f'};
  TemporaryFile malformed_file(malformed);
  const auto encrypted = runtime_test::Resource("encrypted.pdf");
  const auto unsupported = PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 64 64] >>",
      "<< /Filter /UnsupportedHandler /V 99 /R 99 /O (owner) /U (user) /P -4 >>"}, "/Encrypt 4 0 R");
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    SCOPED_TRACE(library->name());
    EXPECT_EQ(api.FPDF_LoadDocument((malformed_file.directory / "missing.pdf").u8string().c_str(), nullptr), nullptr);
    EXPECT_EQ(api.FPDF_GetLastError(), static_cast<unsigned long>(FPDF_ERR_FILE));
    // A directory cannot provide PDF file bytes, even when privileged test
    // runners can bypass ordinary file permission restrictions.
    EXPECT_EQ(api.FPDF_LoadDocument(malformed_file.directory.u8string().c_str(), nullptr), nullptr);
    EXPECT_NE(api.FPDF_GetLastError(), static_cast<unsigned long>(FPDF_ERR_SUCCESS));
    EXPECT_EQ(api.FPDF_LoadDocument(malformed_file.path.u8string().c_str(), nullptr), nullptr);
    EXPECT_EQ(api.FPDF_GetLastError(), static_cast<unsigned long>(FPDF_ERR_FORMAT));
    for (int length : {0, -1, static_cast<int>(malformed.size())}) {
      EXPECT_EQ(api.FPDF_LoadMemDocument(malformed.data(), length, nullptr), nullptr);
      EXPECT_NE(api.FPDF_GetLastError(), static_cast<unsigned long>(FPDF_ERR_SUCCESS));
    }
    EXPECT_EQ(api.FPDF_LoadMemDocument64(malformed.data(), malformed.size(), nullptr), nullptr);
    EXPECT_EQ(api.FPDF_GetLastError(), static_cast<unsigned long>(FPDF_ERR_FORMAT));
    EXPECT_EQ(api.FPDF_LoadMemDocument64(malformed.data(), 0, nullptr), nullptr);
    EXPECT_NE(api.FPDF_GetLastError(), static_cast<unsigned long>(FPDF_ERR_SUCCESS));
    for (const char* password : {static_cast<const char*>(nullptr), "", "incorrect"}) {
      EXPECT_EQ(api.FPDF_LoadMemDocument64(encrypted.data(), encrypted.size(), password), nullptr);
      EXPECT_EQ(api.FPDF_GetLastError(), static_cast<unsigned long>(FPDF_ERR_PASSWORD));
      ReadSource source(encrypted);
      auto access = source.Descriptor();
      EXPECT_EQ(api.FPDF_LoadCustomDocument(&access, password), nullptr);
      EXPECT_EQ(api.FPDF_GetLastError(), static_cast<unsigned long>(FPDF_ERR_PASSWORD));
    }
    EXPECT_EQ(api.FPDF_LoadMemDocument64(unsupported.data(), unsupported.size(), nullptr), nullptr);
    EXPECT_EQ(api.FPDF_GetLastError(), static_cast<unsigned long>(FPDF_ERR_SECURITY));
    // Own the bytes separately: ReadSource only borrows stable storage.
    const auto valid = runtime_test::RectangleFixture();
    ReadSource rejected(valid);
    rejected.reject = true;
    auto access = rejected.Descriptor();
    EXPECT_EQ(api.FPDF_LoadCustomDocument(&access, nullptr), nullptr);
    EXPECT_TRUE(rejected.valid);
    EXPECT_FALSE(rejected.requests.empty());
  }
}

TEST(Loading, EncryptedRevisionsPasswordsAndPermissionMasksMatchKnownFixtures) {
  Artifacts artifacts;
  struct Case { const char* file; int revision; };
  for (const auto& fixture : {Case{"encrypted_hello_world_r2.pdf", 2}, {"encrypted_hello_world_r3.pdf", 3}, {"encrypted_hello_world_r5.pdf", 5}, {"encrypted_hello_world_r6.pdf", 6}}) {
    const auto bytes = runtime_test::Resource(fixture.file);
    TemporaryFile file(bytes);
    std::array<Raster, 2> images;
    size_t backend = 0;
    for (const auto* library : Implementations()) {
      SCOPED_TRACE(std::string(fixture.file) + "/" + library->name());
      const auto& api = library->api();
      for (const char* password : {"\xc3\xa2ge", "\xe2ge", "h\xc3\xb4tel", "h\xf4tel"}) {
        for (int method = 0; method < 4; ++method) {
          ReadSource source(bytes);
          auto descriptor = source.Descriptor();
          Handle document(method == 0 ? api.FPDF_LoadDocument(file.path.u8string().c_str(), password)
              : method == 1 ? api.FPDF_LoadMemDocument(bytes.data(), static_cast<int>(bytes.size()), password)
              : method == 2 ? api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), password)
                            : api.FPDF_LoadCustomDocument(&descriptor, password), api.FPDF_CloseDocument);
          ASSERT_NE(document.get(), nullptr) << "error=" << api.FPDF_GetLastError();
          EXPECT_EQ(api.FPDF_GetSecurityHandlerRevision(document.get()), fixture.revision);
          ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(api, document.get(), &images[backend]));
        }
      }
      ++backend;
    }
    CompareRaster(images[0], images[1], artifacts, std::string(fixture.file) + "-readers", RasterTolerance{24, 0, 0.1});
  }
  const auto bytes = runtime_test::Resource("encrypted.pdf");
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    for (const char* password : {"1234", "5678"}) {
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), password), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      EXPECT_EQ(api.FPDF_GetDocPermissions(document.get()), std::string(password) == "1234" ? 0xfffff2c0u : 0xfffffffcu);
      EXPECT_EQ(api.FPDF_GetDocUserPermissions(document.get()), 0xfffff2c0u);
    }
  }
}

std::vector<uint8_t> XrefStreamFixture() {
  const auto ordinary = runtime_test::RectangleFixture();
  std::string body(ordinary.begin(), ordinary.end());
  const size_t xref = body.find("xref\n");
  if (xref == std::string::npos) throw std::runtime_error("Fixture has no classic xref");
  body.resize(xref);
  const size_t stream_offset = body.size();
  std::string entries;
  for (unsigned int object = 0; object < 6; ++object) {
    const uint32_t offset = object == 0 ? 0 : object == 5 ? static_cast<uint32_t>(stream_offset)
        : static_cast<uint32_t>(body.find(std::to_string(object) + " 0 obj"));
    entries.push_back(object ? 1 : 0);
    for (int shift : {24, 16, 8, 0}) entries.push_back(static_cast<char>(offset >> shift));
    entries.push_back(object ? 0 : '\xff'); entries.push_back(object ? 0 : '\xff');
  }
  body += "5 0 obj\n" + PdfStream("/Type /XRef /Size 6 /Root 1 0 R /W [1 4 2]", entries) +
          "\nendobj\nstartxref\n" + std::to_string(stream_offset) + "\n%%EOF\n";
  return {body.begin(), body.end()};
}

TEST(Loading, ClassicStreamAndReconstructedCrossReferencesKeepUsableContent) {
  Artifacts artifacts;
  auto classic = runtime_test::RectangleFixture();
  std::string recovery(classic.begin(), classic.end());
  recovery.resize(recovery.find("xref\n"));
  recovery += "trailer\n<< /Root 1 0 R /Size 5 >>\n%%EOF\n";
  const std::array<std::vector<uint8_t>, 3> inputs = {classic, XrefStreamFixture(), std::vector<uint8_t>(recovery.begin(), recovery.end())};
  for (size_t kind = 0; kind < inputs.size(); ++kind) {
    std::array<Raster, 2> images;
    size_t backend = 0;
    for (const auto* library : Implementations()) {
      const auto& api = library->api();
      const auto& bytes = inputs[kind];
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      EXPECT_EQ(!!api.FPDF_DocumentHasValidCrossReferenceTable(document.get()), kind != 2);
      ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(api, document.get(), &images[backend++]));
    }
    CompareRaster(images[0], images[1], artifacts, "xref" + std::to_string(kind));
  }
}

TEST(Loading, TrailerEndOffsetsAndBufferUnitsMatchKnownIncrementalFixtures) {
  struct Case { const char* file; std::vector<unsigned int> offsets; };
  for (const auto& fixture : {Case{"hello_world.pdf", {840}}, {"two_signatures.pdf", {633, 1703, 2781}}}) {
    const auto bytes = runtime_test::Resource(fixture.file);
    for (const auto* library : Implementations()) {
      SCOPED_TRACE(std::string(fixture.file) + "/" + library->name());
      const auto& api = library->api();
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      ASSERT_EQ(api.FPDF_GetTrailerEnds(document.get(), nullptr, 0), fixture.offsets.size());
      for (size_t capacity : {size_t{0}, fixture.offsets.size() - 1, fixture.offsets.size(), fixture.offsets.size() + 2}) {
        std::vector<unsigned int> values(capacity + 2, 0xa5a5a5a5);
        EXPECT_EQ(api.FPDF_GetTrailerEnds(document.get(), values.data() + 1, capacity), fixture.offsets.size());
        EXPECT_EQ(values.front(), 0xa5a5a5a5u); EXPECT_EQ(values.back(), 0xa5a5a5a5u);
        if (capacity >= fixture.offsets.size()) { EXPECT_TRUE(std::equal(fixture.offsets.begin(), fixture.offsets.end(), values.begin() + 1)); }
        else { EXPECT_TRUE(std::all_of(values.begin(), values.end(), [](unsigned int v) { return v == 0xa5a5a5a5u; })); }
      }
      EXPECT_EQ(api.FPDF_GetTrailerEnds(nullptr, nullptr, 0), 0u);
    }
  }
}

TEST(ViewerPreferences, NamesRangesScalingCopiesAndDuplexPersist) {
  Artifacts artifacts;
  const std::array<std::string, 5> duplex = {"", "/Duplex /Simplex", "/Duplex /DuplexFlipShortEdge", "/Duplex /DuplexFlipLongEdge", "/Duplex /Unexpected"};
  const std::array<FPDF_DUPLEXTYPE, 5> expected = {DuplexUndefined, Simplex, DuplexFlipShortEdge, DuplexFlipLongEdge, DuplexUndefined};
  for (size_t choice = 0; choice < duplex.size(); ++choice) {
    const auto bytes = runtime_test::RectangleFixture("/ViewerPreferences << /PrintScaling /None /NumCopies 3 /PrintPageRange [0 1 3 5] /Direction /R2L /WrongType (R2L) " + duplex[choice] + " >>");
    for (const auto* writer : Implementations()) {
      const auto& api = writer->api();
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      const auto inspect = [&](const Api& read, FPDF_DOCUMENT doc) {
        EXPECT_FALSE(read.FPDF_VIEWERREF_GetPrintScaling(doc));
        EXPECT_EQ(read.FPDF_VIEWERREF_GetNumCopies(doc), 3);
        EXPECT_EQ(read.FPDF_VIEWERREF_GetDuplex(doc), expected[choice]);
        const auto range = read.FPDF_VIEWERREF_GetPrintPageRange(doc);
        EXPECT_NE(range, nullptr);
        EXPECT_EQ(read.FPDF_VIEWERREF_GetPrintPageRangeCount(range), 4u);
        for (size_t index = 0; index < 4; ++index) EXPECT_EQ(read.FPDF_VIEWERREF_GetPrintPageRangeElement(range, index), (std::array<int, 4>{0, 1, 3, 5})[index]);
        EXPECT_EQ(read.FPDF_VIEWERREF_GetPrintPageRangeElement(range, 4), -1);
        for (const char* key : {"Missing", "WrongType"}) EXPECT_EQ(read.FPDF_VIEWERREF_GetName(doc, key, nullptr, 0), 0u);
        EXPECT_EQ(read.FPDF_VIEWERREF_GetName(doc, "Direction", nullptr, 0), 4u);
        for (size_t capacity : {size_t{0}, size_t{3}, size_t{4}, size_t{6}}) {
          std::vector<char> name(capacity + 2, '#');
          EXPECT_EQ(read.FPDF_VIEWERREF_GetName(doc, "Direction", name.data() + 1, capacity), 4u);
          EXPECT_EQ(name.front(), '#'); EXPECT_EQ(name.back(), '#');
          if (capacity >= 4) { EXPECT_EQ(std::string(name.data() + 1), "R2L"); }
          else { EXPECT_TRUE(std::all_of(name.begin(), name.end(), [](char ch) { return ch == '#'; })); }
        }
      };
      inspect(api, document.get());
      Raster before;
      ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(api, document.get(), &before));
      runtime_test::ExpectRectangle(before);
      const auto saved = SaveDocument(api, document.get(), artifacts, writer->name() + "-preferences" + std::to_string(choice));
      for (const auto* reader : Implementations()) {
        Handle reopened(reader->api().FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), reader->api().FPDF_CloseDocument);
        ASSERT_NE(reopened.get(), nullptr);
        inspect(reader->api(), reopened.get());
        Raster image;
        ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(reader->api(), reopened.get(), &image));
        CompareRaster(before, image, artifacts, writer->name() + "-preferences" + std::to_string(choice) + "-to-" + reader->name());
      }
    }
  }
  const auto absent = runtime_test::RectangleFixture();
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    Handle document(api.FPDF_LoadMemDocument64(absent.data(), absent.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    EXPECT_TRUE(api.FPDF_VIEWERREF_GetPrintScaling(document.get()));
    EXPECT_EQ(api.FPDF_VIEWERREF_GetNumCopies(document.get()), 1);
    EXPECT_EQ(api.FPDF_VIEWERREF_GetDuplex(document.get()), DuplexUndefined);
    EXPECT_EQ(api.FPDF_VIEWERREF_GetPrintPageRange(document.get()), nullptr);
    EXPECT_EQ(api.FPDF_VIEWERREF_GetPrintPageRangeCount(nullptr), 0u);
    EXPECT_EQ(api.FPDF_VIEWERREF_GetPrintPageRangeElement(nullptr, 0), -1);
  }
}

TEST(Loading, SandboxMachineTimePolicyAffectsOnlyNewDocumentMetadata) {
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    struct Restore { const Api& api; ~Restore() { api.FPDF_SetSandBoxPolicy(FPDF_POLICY_MACHINETIME_ACCESS, 1); } } restore{api};
    for (bool enabled : {false, true}) {
      api.FPDF_SetSandBoxPolicy(FPDF_POLICY_MACHINETIME_ACCESS, enabled);
      api.FPDF_SetSandBoxPolicy(0xffffffffu, !enabled);
      Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      const unsigned long bytes = api.FPDF_GetMetaText(document.get(), "CreationDate", nullptr, 0);
      EXPECT_EQ(bytes > 2, enabled);
    }
  }
}

}  // namespace
}  // namespace pdfoxium::integration
