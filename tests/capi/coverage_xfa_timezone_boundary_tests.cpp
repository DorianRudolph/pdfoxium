#include "xfa_form_fixture.h"
#include "pinned_test_fonts.h"

#include <array>
#include <string>

namespace pdfoxium::integration {
namespace {

// These are existing Rust correctness fixes, not a replacement clock or date
// library. A host callback records each reader's original scalar result; the
// same visible PASS field and red landmark remain comparable in both engines.
// Named-zone pairs cancel each implementation's local offset without assuming
// the machine uses UTC. Comparing a broken named zone with GMT would not.
struct ZoneBoundary {
  const char* expression;
  const char16_t* rust;
  const char16_t* native;
};
void ObserveBoundary(forms::Session& session, const Library& reader, const ZoneBoundary& item) {
  // Initial layout intentionally suppresses host message boxes in both readers.
  // A real focus event runs the formula after the document has finished loading.
  session.host->alerts.clear();
  ASSERT_TRUE(session.Focus(8, 25));
  const std::u16string expected = std::u16string(u"observed:") +
      (reader.name() == "rust" ? item.rust : item.native);
  bool observed = false;
  for (const auto& alert : session.host->alerts) {
    const auto& value = std::get<0>(alert);
    if (value.substr(0, 9) == u"observed:") { observed = true; EXPECT_EQ(value, expected); }
    else {
      ADD_FAILURE() << "Unexpected host message: " << testing::PrintToString(value);
    }
  }
  EXPECT_TRUE(observed) << "Host message count: " << session.host->alerts.size();
  // Give both readers the same visible control through the public editing API;
  // the independently checked callback retains the original formula result.
  ASSERT_TRUE(session.api.FORM_SelectAllText(session.form, session.page));
  session.Replace(u"PASS");
  EXPECT_EQ(session.Text(), u"PASS");
  session.api.FORM_ForceToKillFocus(session.form);
  EXPECT_TRUE(session.host->callbacks_valid); EXPECT_FALSE(session.host->callback_error);
}
void ExerciseZoneBoundary(const ZoneBoundary& item) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  // Fixed length keeps the source oracle's known CDATA delimiter plane issue
  // away from the three formulas without changing their execution.
  std::string program = "var observation = " + std::string(item.expression) +
      "\n$host.messageBox(Concat(\"observed:\",observation))\n";
  if (program.size() < 512) program.append(512 - program.size(), ' ');
  const auto bytes = forms::XfaFixture(true, forms::Event(program, "enter", "application/x-formcalc"), "", false, true);
  Artifacts artifacts; std::array<Raster, 2> originals; size_t index = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name()); forms::Session session(writer->api(), bytes, true, true);
    ASSERT_TRUE(session.valid()); ASSERT_TRUE(session.xfa_loaded);
    ASSERT_NO_FATAL_FAILURE(ObserveBoundary(session, *writer, item)); originals[index] = session.DrawForComparison(200, 300);
    forms::ExpectInk(originals[index], 0, 20, 100, 40);
    const auto saved = SaveDocument(session.api, session.document, artifacts, writer->name() + "-timezone-boundary");
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name()); forms::Session loaded(reader->api(), saved, true, true);
      ASSERT_TRUE(loaded.valid()); ASSERT_TRUE(loaded.xfa_loaded);
      ASSERT_NO_FATAL_FAILURE(ObserveBoundary(loaded, *reader, item));
      CompareRaster(originals[index], loaded.DrawForComparison(200, 300), artifacts,
          writer->name() + "-" + reader->name(), RasterTolerance::ConnectedError(1.0));
    }
    ++index;
  }
  CompareRaster(originals[0], originals[1], artifacts, "timezone-boundary-engines", RasterTolerance::ConnectedError(1.0));
  CompareRaster(CropLogicalRaster(originals[0], 0, 0, 40, 20), CropLogicalRaster(originals[1], 0, 0, 40, 20), artifacts, "landmark");
}

TEST(CoverageXfaTimezoneBoundaries, PacificStandardAndDaylightZonesDifferByOneHourWithTheCorrectSign) {
  ExerciseZoneBoundary({R"(Mod(Time2Num("13:00:00 PST","HH:MM:SS Z")-Time2Num("13:00:00 PDT","HH:MM:SS Z")+86400000,86400000))",
      u"3600000", u"82800000"});
}
TEST(CoverageXfaTimezoneBoundaries, PacificAndEasternStandardZonesDifferByThreeHoursWithTheCorrectSign) {
  ExerciseZoneBoundary({R"(Mod(Time2Num("13:00:00 PST","HH:MM:SS Z")-Time2Num("13:00:00 EST","HH:MM:SS Z")+86400000,86400000))",
      u"10800000", u"75600000"});
}
TEST(CoverageXfaTimezoneBoundaries, DateBeforeTimePicturesParseTheirCalendarAndClockInTheDeclaredOrder) {
  ExerciseZoneBoundary({R"(Concat("",Parse("date{YYYY-MM-DD} time{HH:MM:SS}","2000-02-29 13:04:05")))",
      u"2000-02-29T13:04:05", u""});
}

TEST(CoverageXfaTimezoneBoundaries, UnrecognizedEraIsRejectedInsteadOfAcceptingAnUnconsumedSuffix) {
  ExerciseZoneBoundary({R"(Concat("",Parse("date{YYYY-MM-DD G}","2000-02-29 XX")))",
      u"", u"2000-02-29"});
}

}  // namespace
}  // namespace pdfoxium::integration
