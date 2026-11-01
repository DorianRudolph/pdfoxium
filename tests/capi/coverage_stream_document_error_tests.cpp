#include "document_fixture.h"
#include "runtime_fixture.h"
#include "form_fixture.h"
#include "pinned_test_fonts.h"

#include <array>
#include <climits>
#include <functional>
#include <limits>
#include <regex>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using Bytes = std::vector<uint8_t>;
using forms::Session;
using namespace document_test;

struct RecordedUnsupported : UNSUPPORT_INFO {
  RecordedUnsupported() : UNSUPPORT_INFO{} {
    version = 1;
    FSDK_UnSupport_Handler = [](UNSUPPORT_INFO* p, int type) {
      auto& self = *static_cast<RecordedUnsupported*>(p);
      try { self.types.push_back(type); } catch (...) { self.valid = false; }
    };
  }
  std::vector<int> types;
  bool valid = true;
};
RecordedUnsupported& UnsupportedOwner(size_t backend) {
  // PDFium exposes process-wide installation but no uninstall operation.
  // Keep descriptors alive through process exit, including later test groups.
  static std::array<RecordedUnsupported, 2> owners;
  return owners.at(backend);
}
void MetadataWorkflow(const std::string& xml, const std::vector<int>& expected) {
  const auto input = runtime_test::RectangleFixture("/Metadata 5 0 R", "", {PdfStream("/Type /Metadata /Subtype /XML", xml)});
  Artifacts artifacts; std::array<Raster, 2> originals; size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name()); const auto& api = writer->api();
    auto& host = UnsupportedOwner(backend); host.types.clear();
    ASSERT_TRUE(api.FSDK_SetUnSpObjProcessHandler(&host));
    Handle doc(api.FPDF_LoadMemDocument64(input.data(), input.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(doc.get(), nullptr); EXPECT_EQ(host.types, expected); EXPECT_TRUE(host.valid);
    originals[backend] = {};
    runtime_test::RenderDocument(api, doc.get(), &originals[backend]); runtime_test::ExpectRectangle(originals[backend]);
    const auto saved = SaveDocument(api, doc.get(), artifacts, writer->name() + "metadata-workflow"); ASSERT_FALSE(saved.empty());
    for (size_t read_index = 0; read_index < Implementations().size(); ++read_index) {
      const auto* reader = Implementations()[read_index]; const auto& read = reader->api();
      auto& receiver = UnsupportedOwner(read_index); receiver.types.clear();
      ASSERT_TRUE(read.FSDK_SetUnSpObjProcessHandler(&receiver));
      Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
      ASSERT_NE(reopened.get(), nullptr); EXPECT_EQ(receiver.types, expected); EXPECT_TRUE(receiver.valid);
      Raster image; runtime_test::RenderDocument(read, reopened.get(), &image);
      runtime_test::ExpectRectangle(image); CompareRaster(originals[backend], image, artifacts, writer->name() + "metadata-" + reader->name());
    }
    ++backend;
  }
  CompareRaster(originals[0], originals[1], artifacts, "metadata-original-engines");
}
std::string Workflow(const std::string& children, const std::string& ns = "http://ns.adobe.com/AcrobatAdhocWorkflow/1.0/") {
  return "<root xmlns:adhocwf=\"" + ns + "\">" + children + "</root>";
}
TEST(CoverageStreamDocumentErrors, MetadataEmailWorkflowReportsItsSpecificUnsupportedFeature) {
  MetadataWorkflow(Workflow("<adhocwf:workflowType>0</adhocwf:workflowType>"), {FPDF_UNSP_DOC_SHAREDFORM_EMAIL});
}
TEST(CoverageStreamDocumentErrors, MetadataAcrobatWorkflowReportsItsSpecificUnsupportedFeature) {
  MetadataWorkflow(Workflow("<adhocwf:workflowType>1</adhocwf:workflowType>"), {FPDF_UNSP_DOC_SHAREDFORM_ACROBAT});
}
TEST(CoverageStreamDocumentErrors, MetadataFilesystemWorkflowReportsItsSpecificUnsupportedFeature) {
  MetadataWorkflow(Workflow("<adhocwf:workflowType>2</adhocwf:workflowType>"), {FPDF_UNSP_DOC_SHAREDFORM_FILESYSTEM});
}
TEST(CoverageStreamDocumentErrors, MetadataIgnoresNonElementsAndUsesTheFirstWorkflowType) {
  MetadataWorkflow(Workflow("text<!-- comment --><?ignored value?><other>9</other><adhocwf:workflowType>2</adhocwf:workflowType><adhocwf:workflowType>0</adhocwf:workflowType>"), {FPDF_UNSP_DOC_SHAREDFORM_FILESYSTEM});
}
TEST(CoverageStreamDocumentErrors, MetadataNamespaceMustMatchBeforeWorkflowChildrenAreInterpreted) {
  MetadataWorkflow(Workflow("<adhocwf:workflowType>0</adhocwf:workflowType>", "https://example.invalid/other"), {});
}
TEST(CoverageStreamDocumentErrors, MetadataWalksNestedSiblingWorkflowDeclarationsInDocumentOrder) {
  MetadataWorkflow("<root><first>" + Workflow("<adhocwf:workflowType>0</adhocwf:workflowType>") + "</first><second>" + Workflow("<adhocwf:workflowType>1</adhocwf:workflowType>") + "</second></root>",
      {FPDF_UNSP_DOC_SHAREDFORM_EMAIL, FPDF_UNSP_DOC_SHAREDFORM_ACROBAT});
}
TEST(CoverageStreamDocumentErrors, MetadataUnknownWorkflowNumbersProduceNoInventedReport) {
  for (const std::string value : {"-1", "3", "99"}) MetadataWorkflow(Workflow("<adhocwf:workflowType>" + value + "</adhocwf:workflowType>"), {});
}
TEST(CoverageStreamDocumentErrors, MetadataDepthLimitTerminatesBeforeAnOtherwiseRecognizedWorkflow) {
  std::string deep; for (int i = 0; i < 140; ++i) deep += "<nested>";
  deep += Workflow("<adhocwf:workflowType>1</adhocwf:workflowType>");
  for (int i = 0; i < 140; ++i) deep += "</nested>";
  MetadataWorkflow(deep, {});
}

Bytes FieldFixture(const std::string& field, const std::string& action = "", const std::vector<std::string>& extra = {}) {
  std::vector<std::string> objects = {
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Resources << >> /Contents 7 0 R /Annots [5 0 R 6 0 R] >>",
      "<< /Fields [5 0 R 6 0 R] /NeedAppearances " + std::string(field.find("/AP") == std::string::npos ? "true" : "false") + " /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 8 0 R >> >> >>",
      "<< /Type /Annot /Subtype /Widget /T (w) /P 3 0 R /Rect [20 300 190 370] /F 4 /MK << /BC [0 0 0] /BG [1 1 1] >> " + field + " >>",
      "<< /Type /Annot /Subtype /Widget /T (trigger) /P 3 0 R /Rect [220 300 380 370] /F 4 /FT /Btn /Ff 65536 /MK << /CA (GO) /BG [1 1 1] >> /A " + (action.empty() ? forms::Js("") : action) + " >>",
      PdfStream("", "1 g 0 0 400 400 re f 0.25 0.5 0.75 rg 16 16 64 48 re f"),
      "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"};
  objects.insert(objects.end(), extra.begin(), extra.end()); return PdfFixture(objects);
}
void FieldPaint(const Raster& image) {
  ASSERT_EQ(image.width, 400); ASSERT_EQ(image.height, 400);
  const size_t marker = (352u * 400 + 48) * 4;
  EXPECT_NEAR(image.pixels[marker], 191, 1); EXPECT_NEAR(image.pixels[marker + 1], 128, 1); EXPECT_NEAR(image.pixels[marker + 2], 64, 1);
  size_t ink = 0;
  for (int y = 30; y < 100; ++y) for (int x = 20; x < 190; ++x) {
    const size_t at = (size_t(y) * 400 + x) * 4;
    ink += std::min({image.pixels[at], image.pixels[at + 1], image.pixels[at + 2]}) < 128;
  }
  EXPECT_GT(ink, 10u);
}
void FieldWorkflow(const Bytes& input, const std::function<void(Session&)>& act,
                   const std::function<void(Session&)>& inspect, const std::string& label) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts; std::array<Raster, 2> originals; size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name() + label); Session s(writer->api(), input, true); ASSERT_TRUE(s.valid());
    ASSERT_NO_FATAL_FAILURE(act(s));
     ASSERT_NO_FATAL_FAILURE(inspect(s));
    s.api.FORM_ForceToKillFocus(s.form); s.api.FORM_OnMouseMove(s.form, s.page, 0, 390, 390);
    originals[backend] = s.Draw(); FieldPaint(originals[backend]);
    const auto saved = SaveDocument(s.api, s.document, artifacts, writer->name() + label); ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name()); Session reopened(reader->api(), saved, true); ASSERT_TRUE(reopened.valid());
      ASSERT_NO_FATAL_FAILURE(inspect(reopened)); const auto image = reopened.Draw(); FieldPaint(image);
      CompareRaster(originals[backend], image, artifacts, writer->name() + reader->name() + label, {24, 0, .3});
    }
    ++backend;
  }
  CompareRaster(originals[0], originals[1], artifacts, label + "-engines", {24, 0, .3});
}
void SelectionIndices(const std::string& indices, const std::string& value, const std::vector<int>& selected,
                       const std::string& options = "[(a) (b) (c)]") {
  SCOPED_TRACE(indices + value + options);
  const auto bytes = FieldFixture("/FT /Ch /Ff 2097152 /Opt " + options + " " + indices + " " + value);
  FieldWorkflow(bytes, [](Session&) {}, [=](Session& s) {
    Handle annot(s.api.FPDFPage_GetAnnot(s.page, 0), s.api.FPDFPage_CloseAnnot); ASSERT_NE(annot.get(), nullptr);
    ASSERT_EQ(s.api.FPDFAnnot_GetOptionCount(s.form, annot.get()), 3);
    for (int i = 0; i < 3; ++i) {
      EXPECT_EQ(!!s.api.FPDFAnnot_IsOptionSelected(s.form, annot.get(), i), std::find(selected.begin(), selected.end(), i) != selected.end()) << i;
    }
  }, "selection-indices");
}
TEST(CoverageStreamDocumentErrors, ScalarChoiceIndexAgreesWithItsScalarExportValue) { SelectionIndices("/I 2", "/V (c)", {2}); }
TEST(CoverageStreamDocumentErrors, NonNumericChoiceIndexFallsBackToItsExportValue) { SelectionIndices("/I (bad)", "/V (c)", {2}); }
TEST(CoverageStreamDocumentErrors, MismatchedChoiceIndexCountFallsBackToAllExportValues) { SelectionIndices("/I [0]", "/V [(a) (c)]", {0, 2}); }
TEST(CoverageStreamDocumentErrors, NonNumericChoiceArrayEntryFallsBackToTheValues) { SelectionIndices("/I [0 (bad)]", "/V [(a) (c)]", {0, 2}); }
TEST(CoverageStreamDocumentErrors, NegativeChoiceIndexFallsBackToTheValues) { SelectionIndices("/I [-1 2]", "/V [(a) (c)]", {0, 2}); }
TEST(CoverageStreamDocumentErrors, OutOfRangeChoiceIndexFallsBackToTheValues) { SelectionIndices("/I [0 99]", "/V [(a) (c)]", {0, 2}); }
TEST(CoverageStreamDocumentErrors, ChoiceIndexMustIdentifyTheSameExportValueMultiset) { SelectionIndices("/I [0 1]", "/V [(a) (c)]", {0, 2}); }
TEST(CoverageStreamDocumentErrors, DuplicateChoiceExportsUseBothMatchingIndicesWhenTheMultisetAgrees) { SelectionIndices("/I [0 1]", "/V [(a) (a)]", {0, 1}, "[(a) (a) (c)]"); }
TEST(CoverageStreamDocumentErrors, DuplicateValueWithoutDuplicateOptionsFallsBackToOneMatchingOption) { SelectionIndices("/I [0 1]", "/V [(a) (a)]", {0}); }
TEST(CoverageStreamDocumentErrors, ScalarChoiceIndexMustMatchTheSelectedExportValue) { SelectionIndices("/I 1", "/V (c)", {2}); }
TEST(CoverageStreamDocumentErrors, ChoiceBooleanValueAndIndexDoNotSelectAnInventedOption) { SelectionIndices("/I true", "/V true", {}); }
TEST(CoverageStreamDocumentErrors, ChoiceEmptyIndicesWithNoValueRetainAnEmptySelection) { SelectionIndices("/I []", "", {}); }

void RichReset(bool default_present, bool indirect_default) {
  const std::string default_entry = default_present ? indirect_default ? "/DV 9 0 R" : "/DV (Default)" : "";
  const auto bytes = FieldFixture("/FT /Tx /Ff 33554432 /V (Current) /RV (Rich current) " + default_entry,
      "<< /S /ResetForm >>", default_present && indirect_default ? std::vector<std::string>{"(Default)"} : std::vector<std::string>{});
  FieldWorkflow(bytes, [](Session& s) { s.Click(250, 340); }, [=](Session& s) {
    EXPECT_EQ(s.Field(0), default_present ? u"Default" : u"");
    Handle annot(s.api.FPDFPage_GetAnnot(s.page, 0), s.api.FPDFPage_CloseAnnot); ASSERT_NE(annot.get(), nullptr);
    if (default_present) {
      ExpectBytes([&](void* b, unsigned long n) { return s.api.FPDFAnnot_GetStringValue(annot.get(), "RV", static_cast<FPDF_WCHAR*>(b), n); }, Utf16(u"Default"));
    } else {
      EXPECT_FALSE(s.api.FPDFAnnot_HasKey(annot.get(), "RV")); EXPECT_FALSE(s.api.FPDFAnnot_HasKey(annot.get(), "V"));
    }
  }, "rich-reset-" + std::to_string(default_present) + std::to_string(indirect_default));
}
TEST(CoverageStreamDocumentErrors, RichTextResetCopiesItsDefaultToBothPlainAndRichValues) { RichReset(true, false); }
TEST(CoverageStreamDocumentErrors, RichTextResetClonesAnIndirectDefaultString) { RichReset(true, true); }
TEST(CoverageStreamDocumentErrors, RichTextResetRemovesBothValuesWhenNoDefaultExists) { RichReset(false, false); }

void CheckboxSubmission(bool option, bool checked, bool radio, bool no_export) {
  // CPDF_FileSpec::GetFileName preserves URL file specifications. A bare
  // string instead follows DecodeFileName's platform file-path conversion.
  const auto action = "<< /S /SubmitForm /F << /Type /Filespec /FS /URL /F (https://example.invalid/record-only) >> >>";
  const std::string flags = std::to_string((radio ? 32768 : 0) | (no_export ? 4 : 0));
  const auto input = FieldFixture("/FT /Btn /Ff " + flags + " /V /" + (checked ? "Yes" : "Off") + " /AS /" + (checked ? "Yes" : "Off") +
      (option ? " /Opt [(custom-export)]" : "") + " /AP << /N << /Off 9 0 R /Yes 10 0 R >> >>", action,
      {PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 170 70]", "1 g 0 0 170 70 re f 0 G 1 w .5 .5 169 69 re S"),
       PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 170 70]", "1 g 0 0 170 70 re f 0 G 1 w .5 .5 169 69 re S 0 g 20 20 130 30 re f")});
  FieldWorkflow(input, [=](Session& s) {
    s.Click(250, 340); ASSERT_EQ(s.host->submissions.size(), 1u);
    EXPECT_EQ(s.host->submit_urls, (std::vector<std::u16string>{u"https://example.invalid/record-only"}));
    const auto& packet = s.host->submissions[0]; const std::string text(packet.begin(), packet.end());
    EXPECT_EQ(text.substr(0, 5), "%FDF-");
    const std::regex field_name("/T\\s*\\(w\\)");
    EXPECT_EQ(std::regex_search(text, field_name), !no_export);
    if (!no_export) {
      const std::string value = checked ? option ? "custom-export" : "Yes" : "Off";
      const std::regex encoded("/V\\s*" + std::string(option ? "\\(" + value + "\\)" : "/" + value + "(?:\\s|/|>)"));
      EXPECT_TRUE(std::regex_search(text, encoded)) << text;
    }
  }, [=](Session& s) { EXPECT_EQ(s.Field(0), checked ? option ? u"custom-export" : u"Yes" : u"Off"); },
      "checkbox-submit-" + std::to_string(option) + std::to_string(checked) + std::to_string(radio) + std::to_string(no_export));
}
TEST(CoverageStreamDocumentErrors, CheckboxFdfWithoutOptionsSerializesAnExportName) { CheckboxSubmission(false, true, false, false); }
TEST(CoverageStreamDocumentErrors, CheckboxFdfWithOptionsSerializesAnExportString) { CheckboxSubmission(true, true, false, false); }
TEST(CoverageStreamDocumentErrors, RadioFdfWithOptionsSerializesItsCheckedExportString) { CheckboxSubmission(true, true, true, false); }
TEST(CoverageStreamDocumentErrors, UncheckedCheckboxFdfUsesTheOffExportValue) { CheckboxSubmission(true, false, false, false); }
TEST(CoverageStreamDocumentErrors, NoExportCheckboxIsExcludedBeforeFdfSerialization) { CheckboxSubmission(false, true, false, true); }

struct FailingDescriptor {
  FPDF_FILEACCESS access{};
  bool valid = true;
  size_t reads = 0;
  unsigned long length = 0;
  explicit FailingDescriptor(unsigned long size) : length(size) {
    access.m_FileLen = size; access.m_Param = this; access.m_GetBlock = Read;
  }
  static int Read(void* opaque, unsigned long offset, unsigned char* bytes, unsigned long count) noexcept {
    auto& self = *static_cast<FailingDescriptor*>(opaque); ++self.reads;
    self.valid &= bytes && count && offset <= self.length && count <= self.length - offset;
    // Model a bounded host I/O failure. In particular, no allocation is
    // proportional to a claimed multi-gigabyte file length.
    return 0;
  }
};
void FailingCustomFile(unsigned long length) {
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
     SCOPED_TRACE(length);
    const auto& api = library->api(); FailingDescriptor source(length);
    Handle doc(api.FPDF_LoadCustomDocument(&source.access, nullptr), api.FPDF_CloseDocument);
    EXPECT_EQ(doc.get(), nullptr); EXPECT_NE(api.FPDF_GetLastError(), static_cast<unsigned long>(FPDF_ERR_SUCCESS));
    EXPECT_TRUE(source.valid); EXPECT_LT(source.reads, 32u);
    if (length >= 9 && length <= static_cast<unsigned long>(std::numeric_limits<int64_t>::max())) { EXPECT_GT(source.reads, 0u); }
  }
}
TEST(CoverageStreamDocumentErrors, ZeroLengthCustomFileFailsWithoutAHostRead) { FailingCustomFile(0); }
TEST(CoverageStreamDocumentErrors, ShortCustomFileCannotSupplyACompletePdfHeader) { for (unsigned long size : {1ul, 5ul, 8ul}) FailingCustomFile(size); }
TEST(CoverageStreamDocumentErrors, HeaderSizedCustomFileReportsTheHostIoFailure) { FailingCustomFile(9); }
TEST(CoverageStreamDocumentErrors, MultiGigabyteAdvertisedFileFailsThroughBoundedHostReads) { FailingCustomFile(static_cast<unsigned long>(UINT32_MAX)); }
TEST(CoverageStreamDocumentErrors, BeyondFourGigabytesDoesNotTruncateTheCustomFileLength) {
  if (std::numeric_limits<unsigned long>::max() > UINT32_MAX) { FailingCustomFile(static_cast<unsigned long>(UINT32_MAX) + 1ul); }
}
TEST(CoverageStreamDocumentErrors, SignedFileSizeBoundaryDoesNotAllocateTheAdvertisedLength) {
  FailingCustomFile(std::numeric_limits<unsigned long>::max());
}

}  // namespace
}  // namespace pdfoxium::integration
