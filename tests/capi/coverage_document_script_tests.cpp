#include "form_fixture.h"
#include "text_fixture.h"

#include <array>
#include <string>

namespace pdfoxium::integration {
namespace {

std::string Checked(const std::string& body) {
  return "function eq(a,b){if(JSON.stringify(a)!==JSON.stringify(b))throw 'expected '+JSON.stringify(b)+' got '+JSON.stringify(a);}"
      "function yes(v){if(!v)throw 'assertion';}"
      "function throws(f){var caught=false;try{f();}catch(e){caught=true;}if(!caught)throw 'expected exception '+f.toString();}"
      "try{" + body + ";this.getField('second').value='PASS';}"
      "catch(e){this.getField('second').value='FAIL: '+String(e);}";
}

void RunScript(forms::Session& session) {
  // getAnnots traverses all page views, including the second page.
  ASSERT_NE(session.host->Page(1), nullptr);
  session.api.FORM_DoDocumentJSAction(session.form);
  EXPECT_EQ(session.Field(1), u"PASS");
  EXPECT_TRUE(session.host->callbacks_valid);
  EXPECT_FALSE(session.host->callback_error);
}

std::vector<uint8_t> WordFixture(const Library& writer, const std::string& body,
                               bool second_object, Artifacts& artifacts) {
  forms::Actions actions;
  actions.document = Checked(body);
  const auto fixture = forms::Fixture(actions);
  const auto& api = writer.api();
  Handle document(api.FPDF_LoadMemDocument64(fixture.data(), fixture.size(), nullptr),
                  api.FPDF_CloseDocument);
  if (!document.get()) { ADD_FAILURE() << "Document construction failed"; return {}; }
  for (int index = 0; index < 2; ++index) {
    Handle page(api.FPDF_LoadPage(document.get(), index), api.FPDF_ClosePage);
    if (!page.get()) { ADD_FAILURE() << "Page construction failed"; return {}; }
    text_fixture::Add(api, document.get(), page.get(),
                      index ? u"Other page" : u"  Alpha, beta!  Gamma ", 20, 380, 12);
    if (!index && second_object)
      text_fixture::Add(api, document.get(), page.get(), u"Second line", 220, 380, 12);
    EXPECT_TRUE(api.FPDFPage_GenerateContent(page.get()));
  }
  // A third empty page exercises the page-tree traversal and zero result.
  {
    Handle empty(api.FPDFPage_New(document.get(), 2, 400, 400), api.FPDF_ClosePage);
    EXPECT_NE(empty.get(), nullptr);
    if (empty.get()) {
      EXPECT_TRUE(api.FPDFPage_GenerateContent(empty.get()));
    }
  }
  return SaveDocument(api, document.get(), artifacts, writer.name() + "-script-word-fixture");
}

void WordWorkflow(const std::string& body, bool second_object) {
  Artifacts artifacts;
  std::array<Raster, 2> originals;
  size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name());
    auto bytes = WordFixture(*writer, body, second_object, artifacts);
    ASSERT_FALSE(bytes.empty());
    forms::Session session(writer->api(), std::move(bytes), true);
    ASSERT_TRUE(session.valid());
    RunScript(session);
    if (second_object) {
      EXPECT_EQ(session.Field(0), writer->name() == "native" ? u"" : u"Second");
    }
    originals[backend] = CropRaster(session.Draw(), 0, 0, 400, 28);
    forms::ExpectInk(originals[backend], 10, 0, 190, 28);
    const auto saved = SaveDocument(session.api, session.document, artifacts,
                                    writer->name() + "-word-script-results");
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name());
      forms::Session reopened(reader->api(), saved, true);
      ASSERT_TRUE(reopened.valid());
      RunScript(reopened);
      if (second_object) {
        EXPECT_EQ(reopened.Field(0), reader->name() == "native" ? u"" : u"Second");
      }
      const auto after = CropRaster(reopened.Draw(), 0, 0, 400, 28);
      CompareRaster(originals[backend], after, artifacts,
                    writer->name() + "-" + reader->name() + "-word-content",
                    RasterTolerance{24, 0, 0.5});
    }
    ++backend;
  }
  // The report field can differ at the documented native object-boundary bug;
  // the actual page text must render consistently and remain unchanged.
  CompareRaster(originals[0], originals[1], artifacts, "word-content-backends",
                RasterTolerance{24, 0, 0.5});
}

TEST(CoverageDocumentScript, WordExtractionTraversesPagesAndPreservesPunctuationAndWhitespace) {
  WordWorkflow(R"JS(
    eq(this.numPages,3);eq(this.getPageNumWords(),3);
    eq(this.getPageNumWords(1),2);eq(this.getPageNumWords(2),0);
    eq(this.getPageNthWord(),'Alpha,');eq(this.getPageNthWord(0,1),'beta!');
    eq(this.getPageNthWord(0,2),'Gamma');
    eq(this.getPageNthWord(0,0,false),'Alpha, ');
    eq(this.getPageNthWord(0,1,false),'beta!  ');
    eq(this.getPageNthWord(0,2,false),'Gamma ');
    eq(this.getPageNthWord(1,0),'Other');eq(this.getPageNthWord(1,1),'page');
    eq(this.getPageNthWord(2,0),'');eq(this.getPageNthWord(0,-1),'');
    eq(this.getPageNthWord(0,999),'');
    var order=[];
    eq(this.getPageNthWord({valueOf:function(){order.push('page');return 0;}},
                         {valueOf:function(){order.push('word');return 1;}},false),'beta!  ');
    eq(order,['page','word']);
    eq(this.getPageNumWords('1'),2);eq(this.getPageNthWord('1','1'), 'page');
    throws(function(){getPageNumWords(-1);});throws(function(){getPageNumWords(3);});
    throws(function(){getPageNthWord(-1,0);});throws(function(){getPageNthWord(3,0);});
  )JS", false);
}

TEST(CoverageDocumentScript, WordExtractionCrossesContentObjectsWithoutLosingTheBoundaryWord) {
  // Native CJS_Document tests >= at the end of the preceding object's words,
  // selecting an empty result there. Rust's existing corrected < traversal
  // must return the first word in the next object. Assert each observation;
  // do not teach the Rust implementation to reproduce that defect.
  WordWorkflow(R"JS(
    eq(this.getPageNumWords(0),5);eq(this.getPageNthWord(0,2),'Gamma');
    eq(this.getPageNthWord(0,4),'line');eq(this.getPageNthWord(0,5),'');
    this.getField('input').value=this.getPageNthWord(0,3);
  )JS", true);
}

std::vector<uint8_t> AnnotationFixture(const Library& writer, Artifacts& artifacts) {
  forms::Actions actions;
  actions.document = Checked(R"JS(
    var annotations=this.getAnnots({nPage:99});
    eq(annotations.length,2);
    eq(annotations.map(function(a){return a.type;}),['Square','Circle']);
    var note;try{note=this.getAnnot(0,'note');}catch(e){note=this.getAnnot(0,'changed');}
    eq(note.type,'Square');eq(Object.create(note).type,'Square');
    eq(Object.create(note).name,note.name);
    var old=note.name;
    var caught=false;
    try{note.name={toString:function(){throw 'conversion failed';}};}catch(e){caught=e==='conversion failed';}
    this.getField('input').value=caught?'throw':'converted';
    // Native's V8 ToString helper suppresses the exception and returns empty.
    // Assert both engine observations in C++, then restore the valid name.
    if(caught)eq(note.name,old);else eq(note.name,'');
    note.name={toString:function(){return 'changed';}};eq(note.name,'changed');
    throws(function(){note.type='Link';});eq(note.type,'Square');
    note.hidden=!note.hidden;
    eq(this.getAnnot({valueOf:function(){return 1;}},{toString:function(){return 'other';}}).type,'Circle');
    throws(function(){getAnnot();});throws(function(){getAnnot(9,'absent');});
    throws(function(){getAnnot(0,'absent');});
  )JS");
  const auto bytes = forms::Fixture(actions);
  const auto& api = writer.api();
  Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
  if (!document.get()) { ADD_FAILURE() << "Annotation construction load failed"; return {}; }
  for (int page_index = 0; page_index < 2; ++page_index) {
    Handle page(api.FPDF_LoadPage(document.get(), page_index), api.FPDF_ClosePage);
    Handle annotation(api.FPDFPage_CreateAnnot(page.get(), page_index ? FPDF_ANNOT_CIRCLE : FPDF_ANNOT_SQUARE),
                      api.FPDFPage_CloseAnnot);
    if (!annotation.get()) { ADD_FAILURE() << "Public annotation construction failed"; return {}; }
    const FS_RECTF rect{184, 370, 216, 340};
    EXPECT_TRUE(api.FPDFAnnot_SetRect(annotation.get(), &rect));
    EXPECT_TRUE(api.FPDFAnnot_SetFlags(annotation.get(), FPDF_ANNOT_FLAG_PRINT | FPDF_ANNOT_FLAG_READONLY));
    const auto name = forms::Terminated(page_index ? u"other" : u"note");
    EXPECT_TRUE(api.FPDFAnnot_SetStringValue(annotation.get(), "NM", name.data()));
    const auto paint = forms::Terminated(u"0 1 0 rg -1000 -1000 3000 3000 re f");
    EXPECT_TRUE(api.FPDFAnnot_SetAP(annotation.get(), FPDF_ANNOT_APPEARANCEMODE_NORMAL, paint.data()));
  }
  return SaveDocument(api, document.get(), artifacts, writer.name() + "-script-annotation-fixture");
}

void CheckAnnotation(forms::Session& session, bool hidden) {
  const auto& api = session.api;
  Handle annotation(api.FPDFPage_GetAnnot(session.page, api.FPDFPage_GetAnnotCount(session.page) - 1),
                    api.FPDFPage_CloseAnnot);
  ASSERT_NE(annotation.get(), nullptr);
  EXPECT_EQ(api.FPDFAnnot_GetSubtype(annotation.get()), FPDF_ANNOT_SQUARE);
  const int expected = FPDF_ANNOT_FLAG_READONLY |
      (hidden ? FPDF_ANNOT_FLAG_HIDDEN | FPDF_ANNOT_FLAG_INVISIBLE | FPDF_ANNOT_FLAG_NOVIEW : FPDF_ANNOT_FLAG_PRINT);
  EXPECT_EQ(api.FPDFAnnot_GetFlags(annotation.get()), expected);
  std::array<FPDF_WCHAR, 32> name{};
  EXPECT_EQ(api.FPDFAnnot_GetStringValue(annotation.get(), "NM", name.data(), sizeof(name)), 16u);
  EXPECT_EQ(std::u16string(name.begin(), name.begin() + 7), u"changed");
}

TEST(CoverageDocumentScript, AnnotationWrappersMutatePublicFlagsNamesAndRenderedVisibility) {
  Artifacts artifacts;
  std::array<std::array<Raster, 2>, 2> frames;
  size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name());
    auto fixture = AnnotationFixture(*writer, artifacts);
    ASSERT_FALSE(fixture.empty());
    forms::Session session(writer->api(), std::move(fixture), true);
    ASSERT_TRUE(session.valid());
    for (int pass = 0; pass < 2; ++pass) {
      RunScript(session);
      EXPECT_EQ(session.Field(0), writer->name()=="native" ? u"converted" : u"throw");
      const bool hidden = pass == 0;
      CheckAnnotation(session, hidden);
      const auto raster = session.Draw(400, 400, 0, FPDF_ANNOT);
      const size_t at = (45 * 400 + 200) * 4;
      EXPECT_EQ(raster.pixels[at], hidden ? 240 : 0);
      EXPECT_EQ(raster.pixels[at + 1], hidden ? 240 : 255);
      EXPECT_EQ(raster.pixels[at + 2], hidden ? 240 : 0);
      frames[backend][pass] = CropRaster(raster, 180, 26, 40, 40);
    }
    const auto saved = SaveDocument(session.api, session.document, artifacts,
                                    writer->name() + "-script-annotation-results");
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name());
      forms::Session reopened(reader->api(), saved, true);
      ASSERT_TRUE(reopened.valid());
      CheckAnnotation(reopened, false);
      for (int pass = 0; pass < 2; ++pass) {
        RunScript(reopened);
        EXPECT_EQ(reopened.Field(0), reader->name()=="native" ? u"converted" : u"throw");
        CheckAnnotation(reopened, pass == 0);
        const auto raster = reopened.Draw(400, 400, 0, FPDF_ANNOT);
        CompareRaster(frames[backend][pass], CropRaster(raster, 180, 26, 40, 40), artifacts,
                      writer->name() + "-" + reader->name() + "-annotation-" + std::to_string(pass));
      }
    }
    ++backend;
  }
  for (int pass = 0; pass < 2; ++pass)
    CompareRaster(frames[0][pass], frames[1][pass], artifacts, "annotation-backends-" + std::to_string(pass));
}

}  // namespace
}  // namespace pdfoxium::integration
