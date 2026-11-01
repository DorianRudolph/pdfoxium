#include "xfa_form_fixture.h"
#include "pinned_test_fonts.h"

#include <array>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {

// The picture/input/output rows are adapted from the pinned
// CFGASStringFormatterTest tables, exercised through real FormCalc events and
// the document's locale service rather than calling an internal formatter.
struct PictureCase {
  const char* expression;
  const char16_t* expected;
  const char16_t* native_expected = nullptr;
  bool native_default_wrap = false;
  bool check_fitting_single_line = false;
};

bool FocusPicture(forms::Session& session) {
  for (int y = 5; y < 300; y += 10) {
    for (int x = 5; x < 100; x += 10) {
      if (session.api.FPDFPage_HasFormFieldAtPoint(session.form, session.page, x, y) ==
          FPDF_FORMFIELD_XFA_TEXTFIELD)
        return session.Focus(x, y);
    }
  }
  ADD_FAILURE() << "The initialized picture field must remain hit-testable";
  return false;
}

std::vector<uint8_t> PictureFixture(const PictureCase& item,bool embed_test_font=false) {
  // Brackets preserve significant padding and make an empty formatted result
  // observable and renderable. They are ordinary FormCalc string operations.
  return forms::XfaFixture(true, forms::Event(
      std::string("$.rawValue = Concat(\"[\",") + item.expression + ",\"]\")",
      "initialize", "application/x-formcalc"),"",false,embed_test_font);
}

std::u16string PictureValue(const PictureCase& item) {
  return std::u16string(u"[") + item.expected + u"]";
}

void ComparePicture(const Raster& native, const Raster& rust,
                    Artifacts& artifacts, const std::string& name) {
  if(!raster_comparison_settings().enabled)return;
  // Same native FreeType versus pure Rust AGG glyph coverage bounds used by
  // the adjacent FormCalc tests; both assertions include the actual widget.
  CompareRaster(native, rust, artifacts, name, RasterTolerance{24, 0, 0.15});
  CompareRaster(CropLogicalRaster(native, 0, 20, 100, 20),
                CropLogicalRaster(rust, 0, 20, 100, 20), artifacts,
                name + "-widget", RasterTolerance{24, 0, 1.0});
}

void ExpectOriginalSingleLinePicture(const Raster& image, bool native) {
  // The pinned native constructor leaves its breaker at multiline/100pt
  // defaults. Rust synchronizes the requested single-line mode. Keep the
  // original picture value and require its actual visible line placement.
  size_t first_line=0,second_line=0;
  for(int y=20;y<40;++y)for(int x=0;x<100;++x){
    const auto at=(static_cast<size_t>(y)*image.width+x)*4;
    const bool ink=image.pixels[at]<180&&image.pixels[at+1]<180&&image.pixels[at+2]<180;
    if(y<33)first_line+=ink;
    if(y>=35)second_line+=ink;
  }
  EXPECT_GT(first_line,100u);
  if(native) { EXPECT_GT(second_line,10u); }
  else { EXPECT_EQ(second_line,0u); }
}

void RunPictures(const std::vector<PictureCase>& cases,bool embed_test_font=false) {
  Artifacts artifacts;
  size_t index = 0;
  for (const auto& item : cases) {
    SCOPED_TRACE(item.expression);
    std::array<Raster, 2> images;
    std::array<Raster, 2> normalized;
    size_t backend = 0;
    for (const auto* library : Implementations()) {
      SCOPED_TRACE(library->name());
      forms::Session session(library->api(), PictureFixture(item,embed_test_font), true, true);
      ASSERT_TRUE(session.valid());
      ASSERT_TRUE(session.xfa_loaded);
      ASSERT_TRUE(FocusPicture(session));
      const auto expected = backend == 0 && item.native_expected
                                ? std::u16string(u"[") + item.native_expected + u"]"
                                : PictureValue(item);
      EXPECT_EQ(session.Text(), expected);
      session.api.FORM_ForceToKillFocus(session.form);
      auto base=session.Draw(200,300);
      forms::ExpectInk(base,0,20,100,40);
      if(item.native_default_wrap || item.check_fitting_single_line)
        ExpectOriginalSingleLinePicture(base,item.native_default_wrap&&library->name()=="native");
      images[backend]=(embed_test_font && raster_comparison_settings().dpi!=72)
          ?session.DrawForComparison(200,300):std::move(base);
      if (item.native_expected || item.native_default_wrap) {
        // Preserve the original native defect observation and Rust's correct
        // output. Then render equal, independently checked text through both
        // public input paths instead of allowing different digits as noise.
        artifacts.Image(library->name() + "-original-" + std::to_string(index), images[backend]);
        ASSERT_TRUE(FocusPicture(session));
        ASSERT_TRUE(session.api.FORM_SelectAllText(session.form, session.page));
        // Short public input gives both engines the same single-line control
        // state without weakening checks on the original formatted result.
        const auto common=item.native_default_wrap?std::u16string(u"[date]"):PictureValue(item);
        session.Replace(common);
        EXPECT_EQ(session.Text(), common);
        session.api.FORM_ForceToKillFocus(session.form);
        auto base=session.Draw(200,300);
        forms::ExpectInk(base,0,20,100,40);
        normalized[backend]=(embed_test_font && raster_comparison_settings().dpi!=72)
            ?session.DrawForComparison(200,300):std::move(base);
      } else {
        normalized[backend] = images[backend];
      }
      ++backend;
      EXPECT_TRUE(session.host->callbacks_valid);
      EXPECT_FALSE(session.host->callback_error);
    }
    const auto name = "picture-" + std::to_string(index++);
    if (raster_comparison_settings().enabled && (item.native_expected || item.native_default_wrap)) {
      CompareRaster(CropLogicalRaster(images[0], 0, 0, 200, 20),
                    CropLogicalRaster(images[1], 0, 0, 200, 20), artifacts, name + "-header");
      CompareRaster(CropLogicalRaster(images[0], 0, 40, 200, 260),
                    CropLogicalRaster(images[1], 0, 40, 200, 260), artifacts, name + "-below-field");
    }
    ComparePicture(normalized[0], normalized[1], artifacts, name);
  }
}

TEST(CoveragePictures, NumericMasksSignsGroupingExponentsAndExplicitLocalesDrawTheirResults) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunPictures({
      {R"(Format("num{zz9.zzz}","1.234"))", u"1.234"},
      {R"(Format("num{ZZ9.ZZZ}","1.234"))", u"  1.234"},
      {R"(Format("num{ZZ9.ZZZ}","12.345"))", u" 12.345"},
      {R"(Format("num{ZZ9.ZZZ}","123"))", u"123.000"},
      {R"(Format("num{zz9.zzz}","123.000"))", u"123"},
      {R"(Format("num{z 'text'}",1))", u"1 text"},
      {R"(Format("num{'text' z}",1))", u"text 1"},
      {R"(Format("num{zzz,zz9.8888}",123456))", u"123,456"},
      {R"(Format("num{S999v99}",-1.23))", u"-00123"},
      {R"(Format("num{S999V99}",1.23))", u" 001.23"},
      {R"(Format("num{S999V99}",123))", u" 123.00"},
      {R"(Format("num{SZZ9.99}",12.3))", u"  12.30"},
      {R"(Format("num{SZ99.99}",-12.3))", u"- 12.30"},
      {R"(Format("num{szz9.99}",123))", u"123.00"},
      {R"(Format("num{szz9.99}",-123))", u"-123.00"},
      {R"(Format("num{$z,zz9.99DB}",-1234))", u"$1,234.00DB"},
      {R"(Format("num{99.999E}",12345))", u"12.345E+3"},
      // Native ParseNum consumes the mantissa as an exponent without an E
      // marker, producing zero; Rust rejects that parse and formats 12345.
      {R"(Format("num{99999E}",12345))", u"12345E+0", u"00000E+0"},
      {R"(Format("num{99.999E}",0.12345))", u"12.345E-2"},
      {R"(Format("num{99,999}",12345))", u"12,345"},
      {R"(Format("num(fr){z,zzz}",1234))", u"1\u00a0234"},
      {R"(Format("num.percent{}",12.34))", u"1,234%"},
      {R"(Format("num(fr).percent{}",12.34))", u"1\u00a0234%"},
      {R"(Format("num{zzzCR}",-123))", u"123CR"},
      {R"(Format("num{zzzCR}",123))", u"123  "},
  });
}

TEST(CoveragePictures, FixedCalendarAndTimePicturesUseQuotedLiteralsAndEmbeddedLocales) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  // Time rows avoid zone-bearing input and output: no process timezone or
  // clock override, and no expected value derived from the current date.
  RunPictures({
      {R"(Format("date{MMMM DD, YYYY}","2002-10-25"))", u"October 25, 2002", nullptr, false, true},
      {R"(Format("date{'Week of the month is' w}","20040722"))", u"Week of the month is 3", nullptr, true},
      {R"(Format("date{e 'days after Sunday'}","20040722"))", u"4 days after Sunday", nullptr, true},
      {R"(Format("date{YYYY-'W'WW-e}","20040722"))", u"2004-W30-4"},
      {R"(Format("date{EEE, 'the' D 'of' MMMM, YYYY}","2000-01-01"))", u"Sat, the 1 of January, 2000", nullptr, true},
      {R"(Format("date{MM/D/YY}","19991202"))", u"12/2/99"},
      {R"(Format("date{J}","19990202"))", u"33"},
      {R"(Format("date{JJJ}","19990202"))", u"033"},
      {R"(Format("date{J}","19991231"))", u"365"},
      {R"(Format("date{J}","20001231"))", u"366"},
      {R"(Format("date{J}","20000229"))", u"60"},
      {R"(Format("date{YYYY G}","19990102"))", u"1999 AD"},
      {R"(Format("date(fr){DD MMMM, YYYY}","2002-10-25"))", u"25 octobre, 2002"},
      {R"(Format("date(es){EEEE, D 'de' MMMM 'de' YYYY}","2002-10-25"))", u"viernes, 25 de octubre de 2002", nullptr, true},
      {R"(Format("time{h:M A}","01:01:11"))", u"1:1 AM"},
      {R"(Format("time{h:M A}","13:01:11"))", u"1:1 PM"},
      {R"(Format("time{hh:MM:SS A}","13:01:11"))", u"01:01:11 PM"},
      {R"(Format("time{k:MM:SS}","14:01:01"))", u"2:01:01"},
      // Native SetTime appends malformed "%:03d" and loses the fraction.
      {R"(Format("time{HH:M:S.FFF}","00:01:11.001"))", u"00:1:11.001", u"00:1:11.000"},
      {R"(Format("time{K:M}","00:01:11"))", u"24:1"},
      {R"(Format("time{h:MM A}","14:30:59"))", u"2:30 PM"},
  },true);
  fonts.ExpectMapped();
}

TEST(CoveragePictures, NumericAndTextParsingRemoveOnlyTheirPictureDecoration) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunPictures({
      {R"(Parse("99.999E","12.345e3"))", u"12345"},
      {R"(Parse("99.999E","12.345E-2"))", u"0.12345"},
      {R"(Parse("99E","12e2"))", u"1200"},
      {R"(Parse("99E","12e-2"))", u"0.12"},
      {R"(Parse("z999","0150"))", u"150"},
      {R"(Parse("999cr","123CR"))", u"-123"},
      {R"(Parse("999cr","123"))", u"123"},
      {R"(Parse("999DB","123DB"))", u"-123"},
      {R"(Parse("999.9CR","123.5CR"))", u"-123.5"},
      {R"(Parse("z,zz9.99","3,125.00"))", u"3125"},
      {R"(Parse("$z,zz9.99DB","$1,234.00"))", u"1234"},
      {R"(Parse("z.zzz 'text'","1.234 text"))", u"1.234"},
      {R"(Parse("text{AAA-9999-X}","ABC-1234-5"))", u"ABC12345"},
      {R"(Parse("text{AAA-9999-X}","ABC-1234-D"))", u"ABC1234D"},
      {R"(Parse("text{OOO-9999-X}","A1C-1234-D"))", u"A1C1234D"},
      {R"(Parse("text{000-9999-X 'text'}","A1C-1234-D text"))", u"A1C1234D"},
  });
}

TEST(CoveragePictures, TextMasksAndRejectedPicturesLeaveUsableVisibleFields) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunPictures({
      {R"(Format("text{A9A 9A9}","K1S5K2"))", u"K1S 5K2"},
      {R"(Format("text(fr){A9A 9A9}","K1S5K2"))", u"K1S 5K2"},
      {R"(Format("text{999.999.9999}","6135551212"))", u"613.555.1212"},
      {R"(Format("text{999#999*9999}","6135551212"))", u"613#555*1212"},
      {R"(Format("text{00X OO9}","K1#5K2"))", u"K1# 5K2"},
      {R"(Parse("text{AAA-9999-X}","123-4567-8"))", u""},
      {R"(Parse("num{999}","abc"))", u""},
      {R"(Parse("date{YYYY-MM-DD}","not-a-date"))", u""},
      {R"(HasValue(Parse("text{AAA}",null)))", u"0"},
      {R"(Format("text{XXX}","ABC"))", u"ABC"},
  });
  // Failed parsing must not poison later events, input or form rendering.
  Artifacts artifacts;
  std::array<Raster, 2> images;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    const PictureCase item{R"(Parse("num{999}","abc"))", u""};
    forms::Session session(library->api(), PictureFixture(item), true, true);
    ASSERT_TRUE(session.valid());
    ASSERT_TRUE(session.xfa_loaded);
    ASSERT_TRUE(FocusPicture(session));
    EXPECT_EQ(session.Text(), u"[]");
    ASSERT_TRUE(session.api.FORM_SelectAllText(session.form, session.page));
    session.Replace(u"recovered");
    EXPECT_EQ(session.Text(), u"recovered");
    session.api.FORM_ForceToKillFocus(session.form);
    images[backend] = session.Draw(200, 300);
    forms::ExpectInk(images[backend++], 0, 20, 100, 40);
    EXPECT_TRUE(session.host->callbacks_valid);
  }
  ComparePicture(images[0], images[1], artifacts, "picture-recovery");
}

TEST(CoveragePictures, PictureProgramsAndLocalesSurviveBothWritersAndReaders) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  size_t case_index = 0;
  for (const PictureCase& item : std::vector<PictureCase>{
           {R"(Format("num{99.999E}",12345))", u"12.345E+3"},
           {R"(Format("num(fr){z,zzz}",1234))", u"1\u00a0234"},
           {R"(Format("date(fr){DD MMMM, YYYY}","2002-10-25"))", u"25 octobre, 2002"},
           {R"(Format("time{hh:MM:SS A}","13:01:11"))", u"01:01:11 PM"},
           {R"(Format("text{999.999.9999}","6135551212"))", u"613.555.1212"}}) {
    SCOPED_TRACE(item.expression);
    const auto name = "picture-" + std::to_string(case_index++);
    for (const auto* writer : Implementations()) {
      SCOPED_TRACE(writer->name());
      forms::Session session(writer->api(), PictureFixture(item), true, true);
      ASSERT_TRUE(session.valid());
      ASSERT_TRUE(session.xfa_loaded);
      ASSERT_TRUE(FocusPicture(session));
      EXPECT_EQ(session.Text(), PictureValue(item));
      session.api.FORM_ForceToKillFocus(session.form);
      const auto before = session.Draw(200, 300);
      const auto saved = SaveDocument(session.api, session.document, artifacts,
                                      writer->name() + "-" + name);
      std::array<Raster, 2> images;
      size_t backend = 0;
      for (const auto* reader : Implementations()) {
        SCOPED_TRACE(reader->name());
        forms::Session reopened(reader->api(), saved, true, true);
        ASSERT_TRUE(reopened.valid());
        ASSERT_TRUE(reopened.xfa_loaded);
        ASSERT_TRUE(FocusPicture(reopened));
        EXPECT_EQ(reopened.Text(), PictureValue(item));
        reopened.api.FORM_ForceToKillFocus(reopened.form);
        images[backend] = reopened.Draw(200, 300);
        forms::ExpectInk(images[backend], 0, 20, 100, 40);
        ComparePicture(before, images[backend++], artifacts,
                       writer->name() + "-" + name + "-to-" + reader->name());
        EXPECT_TRUE(reopened.host->callbacks_valid);
      }
      ComparePicture(images[0], images[1], artifacts, writer->name() + "-" + name + "-readers");
    }
  }
}

}  // namespace
}  // namespace pdfoxium::integration
