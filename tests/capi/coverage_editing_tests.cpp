#include "coverage_editing_fixture.h"

namespace pdfoxium::integration {
namespace {
using namespace coverage_editing;
constexpr std::array<uint8_t,4> kWhite{255,255,255,255};

std::vector<uint8_t> SharedContents(int kind) {
  const std::array<std::string,4> paint = {
      "1 0 0 rg 8 8 24 24 re f\n", "0 1 0 rg 40 8 24 24 re f\n",
      "0 0 1 rg 72 8 24 24 re f\n", "1 1 0 rg 8 48 24 24 re f\n"};
  const std::string contents = kind == 0 ? "5 0 R" : kind == 1 ? "[6 0 R 7 0 R 8 0 R 9 0 R]" : "6 0 R";
  const std::string page = "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 128] /Resources 10 0 R /Contents " + contents + " >>";
  return PdfFixture({"<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>", page,page,
      "[6 0 R 7 0 R 8 0 R 9 0 R]", PdfStream("",kind == 2 ? paint[0]+paint[1]+paint[2]+paint[3] : paint[0]),
      PdfStream("",paint[1]),PdfStream("",paint[2]),PdfStream("",paint[3]),"<< >>"});
}

TEST(CoverageEditing, SharedContentsCopyOnWriteRemovalAndAppendAcrossGenerations) {
  Artifacts artifacts;
  for (int kind : {0,1,2}) {
    SCOPED_TRACE(kind);
    std::vector<std::vector<Raster>> results;
    for (const auto* writer : Implementations()) {
      SCOPED_TRACE(writer->name()); const auto& api=writer->api();
      const auto bytes=SharedContents(kind);
      Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument); ASSERT_NE(doc.get(),nullptr);
      {
        Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage); ASSERT_NE(page.get(),nullptr);
        ASSERT_EQ(api.FPDFPage_CountObjects(page.get()),4);
        for(int n=0;n<2;++n) { auto obj=api.FPDFPage_GetObject(page.get(),0); ASSERT_TRUE(api.FPDFPage_RemoveObject(page.get(),obj)); api.FPDFPageObj_Destroy(obj); }
        ASSERT_TRUE(api.FPDFPageObj_SetFillColor(api.FPDFPage_GetObject(page.get(),0),0,0,0,255));
        ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(),editing::Rectangle(api,80,80,24,24,255,0,255)));
        ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get())); ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
      }
      const bool native_shared_array = writer->name()=="native" && kind==0;
      auto inspect=[native_shared_array](const Api& read,FPDF_DOCUMENT,FPDF_PAGE page,int index) {
        EXPECT_EQ(read.FPDFPage_CountObjects(page),index ? 4 : 3);
        const auto image=Render(read,page,128,128);
        EXPECT_EQ(Pixel(image,16,112),index ? (std::array<uint8_t,4>{0,0,255,255}) : kWhite);
        EXPECT_EQ(Pixel(image,48,112),index ? (std::array<uint8_t,4>{0,255,0,255}) : kWhite);
        EXPECT_EQ(Pixel(image,80,112),index && !native_shared_array ? (std::array<uint8_t,4>{255,0,0,255}) : (std::array<uint8_t,4>{0,0,0,255}));
        EXPECT_EQ(Pixel(image,16,64),(std::array<uint8_t,4>{0,255,255,255}));
        EXPECT_EQ(Pixel(image,88,40),index ? kWhite : (std::array<uint8_t,4>{255,0,255,255}));
      };
      results.push_back(RoundTrip(*writer,doc.get(),{{},{}},artifacts,"cow-"+std::to_string(kind),inspect));
      {
        Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);
        while(api.FPDFPage_CountObjects(page.get())) {auto obj=api.FPDFPage_GetObject(page.get(),0); ASSERT_TRUE(api.FPDFPage_RemoveObject(page.get(),obj)); api.FPDFPageObj_Destroy(obj);}
        ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
        ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(),editing::Rectangle(api,48,80,24,24,0,255,255)));
        ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
      }
      RoundTrip(*writer,doc.get(),{{},{}},artifacts,"emptied-"+std::to_string(kind),[](const Api& read,FPDF_DOCUMENT,FPDF_PAGE page,int index){
        EXPECT_EQ(read.FPDFPage_CountObjects(page),index ? 4 : 1);
        const auto image=Render(read,page,128,128);
        EXPECT_EQ(Pixel(image,56,40),index ? kWhite : (std::array<uint8_t,4>{255,255,0,255}));
        EXPECT_EQ(Pixel(image,16,112),index ? (std::array<uint8_t,4>{0,0,255,255}) : kWhite);
      });
    }
    ASSERT_EQ(results.size(),2u); for(size_t p=0;p<(kind==0 ? 1u : 2u);++p) Exact(results[0][p],results[1][p],artifacts,"cow-backends"+std::to_string(kind)+std::to_string(p));
  }
}

TEST(CoverageEditing, CrossStreamTransformsAndNamedPropertiesSurviveRemovalAndReinsertion) {
  const auto bytes=PdfFixture({"<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 128] /Resources << /Properties << /P 5 0 R >> >> /Contents [4 0 R 6 0 R 7 0 R] >>",
    PdfStream("","q 1 0 0 1 16 8 cm\n"),"<< /MCID 17 /Kind (Shared) >>",
    PdfStream("","/Layer /P BDC 1 0 0 rg 8 8 24 24 re f EMC\n"),PdfStream("","Q 0 0 1 rg 64 64 24 24 re f\n")});
  Artifacts artifacts; std::vector<Raster> pictures;
  for(const auto* writer:Implementations()) {
    const auto& api=writer->api(); Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument); ASSERT_NE(doc.get(),nullptr);
    {
      Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage); ASSERT_EQ(api.FPDFPage_CountObjects(page.get()),2);
      auto first=api.FPDFPage_GetObject(page.get(),0); auto second=api.FPDFPage_GetObject(page.get(),1);
      editing::ExpectBounds(api,first,{24,16,48,40}); editing::ExpectBounds(api,second,{64,64,88,88});
      ASSERT_TRUE(api.FPDFPageObj_SetFillColor(first,0,255,0,255)); ASSERT_TRUE(api.FPDFPageObj_SetFillColor(second,255,0,255,255));
      ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
      ASSERT_TRUE(api.FPDFPage_RemoveObject(page.get(),first)); Handle removed(first,api.FPDFPageObj_Destroy);
      ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get())); editing::ExpectBounds(api,second,{64,64,88,88});
      ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(),removed.release())); ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
    }
    const auto observed=RoundTrip(*writer,doc.get(),{{}},artifacts,"cross-stream-properties",[](const Api& read,FPDF_DOCUMENT,FPDF_PAGE page,int){
      ASSERT_EQ(read.FPDFPage_CountObjects(page),2);
      // GenerateContent retains original stream assignments when an object is
      // reinserted. Reparsed order therefore need not match the live list.
      auto marked=read.FPDFPage_GetObject(page,0);
      if(!read.FPDFPageObj_CountMarks(marked)) marked=read.FPDFPage_GetObject(page,1);
      editing::ExpectBounds(read,marked,{24,16,48,40}); EXPECT_EQ(read.FPDFPageObj_CountMarks(marked),1);
      int mcid=0; ASSERT_TRUE(read.FPDFPageObjMark_GetParamIntValue(read.FPDFPageObj_GetMark(marked,0),"MCID",&mcid)); EXPECT_EQ(mcid,17);
      const auto image=Render(read,page,128,128); EXPECT_EQ(Pixel(image,32,100),(std::array<uint8_t,4>{0,255,0,255}));
      EXPECT_EQ(Pixel(image,76,52),(std::array<uint8_t,4>{255,0,255,255})); EXPECT_EQ(Pixel(image,8,120),kWhite);
    }); pictures.push_back(observed[0]);
  }
  Exact(pictures[0],pictures[1],artifacts,"cross-stream-backends");
}

TEST(CoverageEditing, SparsePageImportsAndNupConcatenateContentArrays) {
  const auto bytes=PdfFixture({"<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 >>",
    "<< /Type /Page /Parent 2 0 R /CropBox [0 0 128 64] /Contents [6 0 R 7 0 R] >>",
    "<< /Type /Page /Parent 2 0 R /Contents 8 0 R >>","<< /Type /Page /Parent 2 0 R /MediaBox [0 0 64 128] /Contents [] >>",
    PdfStream("","1 0 0 rg 0 0 64 64 re f\n"),PdfStream("","0 1 0 rg 64 0 64 64 re f\n"),PdfStream("","0 0 1 rg 8 8 24 24 re f\n")});
  Artifacts artifacts; std::vector<Raster> sheets;
  for(const auto* writer:Implementations()) {
    const auto& api=writer->api(); Handle source(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
    Handle target(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument); ASSERT_NE(source.get(),nullptr); ASSERT_NE(target.get(),nullptr);
    const int indices[]={0,1,2,0}; ASSERT_TRUE(api.FPDF_ImportPagesByIndex(target.get(),source.get(),indices,4,0)); source.reset();
    RoundTrip(*writer,target.get(),{{128,64},{612,792},{64,128},{128,64}},artifacts,"sparse-import",[](const Api& read,FPDF_DOCUMENT,FPDF_PAGE page,int index){
      EXPECT_DOUBLE_EQ(read.FPDF_GetPageWidth(page),index==1 ? 612 : index==2 ? 64 : 128);
      EXPECT_DOUBLE_EQ(read.FPDF_GetPageHeight(page),index==1 ? 792 : index==2 ? 128 : 64);
      EXPECT_EQ(read.FPDFPage_CountObjects(page),index==1 ? 1 : index==2 ? 0 : 2);
      if(index==0 || index==3) {const auto image=Render(read,page,128,64); EXPECT_EQ(Pixel(image,24,32),(std::array<uint8_t,4>{0,0,255,255})); EXPECT_EQ(Pixel(image,100,32),(std::array<uint8_t,4>{0,255,0,255}));}
    });
    Handle nup(api.FPDF_ImportNPagesToOne(target.get(),384,256,2,2),api.FPDF_CloseDocument); ASSERT_NE(nup.get(),nullptr);
    const auto observed=RoundTrip(*writer,nup.get(),{{384,256}},artifacts,"sparse-array-nup",[](const Api& read,FPDF_DOCUMENT,FPDF_PAGE page,int){
      EXPECT_DOUBLE_EQ(read.FPDF_GetPageWidth(page),384); EXPECT_DOUBLE_EQ(read.FPDF_GetPageHeight(page),256);
      EXPECT_EQ(read.FPDFPage_CountObjects(page),4);
      const auto image=Render(read,page,384,256);
      EXPECT_EQ(Pixel(image,32,64),(std::array<uint8_t,4>{0,0,255,255})); EXPECT_EQ(Pixel(image,160,64),(std::array<uint8_t,4>{0,255,0,255}));
      EXPECT_EQ(Pixel(image,32,192),kWhite); EXPECT_EQ(Pixel(image,224,192),(std::array<uint8_t,4>{0,0,255,255}));
    }); sheets.push_back(observed[0]);
  }
  Exact(sheets[0],sheets[1],artifacts,"sparse-nup-backends");
}

TEST(CoverageEditing, MissingCircleAndMarkupAppearancesAreGeneratedAndPersist) {
  const auto bytes=PdfFixture({"<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 256 256] /Resources << >> /Annots [4 0 R 5 0 R 6 0 R 7 0 R] >>",
    "<< /Type /Annot /Subtype /Circle /Rect [20 170 110 235] /C [0 0 0] /IC [1 .8 .2] /BS << /S /S /W 3 >> /F 4 >>",
    "<< /Type /Annot /Subtype /Underline /Rect [20 125 230 150] /QuadPoints [20 150 110 150 20 130 110 130 130 150 230 150 130 130 230 130] /C [0 0 0] /F 4 >>",
    "<< /Type /Annot /Subtype /StrikeOut /Rect [20 75 230 100] /QuadPoints [20 100 110 100 20 80 110 80 130 100 230 100 130 80 230 80] /C [0 0 0] /F 4 >>",
    "<< /Type /Annot /Subtype /Squiggly /Rect [20 25 230 50] /QuadPoints [20 50 110 50 20 30 110 30 130 50 230 50 130 30 230 30] /C [0 0 0] /F 4 >>"});
  Artifacts artifacts; std::vector<Raster> scenes;
  for(const auto* writer:Implementations()) {
    const auto& api=writer->api(); Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument); ASSERT_NE(doc.get(),nullptr);
    {Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage); Render(api,page.get(),256,256,FPDF_ANNOT);}
    const auto pictures=RoundTrip(*writer,doc.get(),{{256,256,FPDF_ANNOT}},artifacts,"generated-markups",[](const Api& read,FPDF_DOCUMENT,FPDF_PAGE page,int){
      ASSERT_EQ(read.FPDFPage_GetAnnotCount(page),4);
      const std::array<int,4> types={FPDF_ANNOT_CIRCLE,FPDF_ANNOT_UNDERLINE,FPDF_ANNOT_STRIKEOUT,FPDF_ANNOT_SQUIGGLY};
      for(int i=0;i<4;++i) {Handle annot(read.FPDFPage_GetAnnot(page,i),read.FPDFPage_CloseAnnot); EXPECT_EQ(read.FPDFAnnot_GetSubtype(annot.get()),types[i]); EXPECT_FALSE(Appearance(read,annot.get()).empty()); if(i) { EXPECT_EQ(read.FPDFAnnot_CountAttachmentPoints(annot.get()),2u); }}
      const auto image=Render(read,page,256,256,FPDF_ANNOT);
      EXPECT_EQ(Pixel(image,65,54),(std::array<uint8_t,4>{51,204,255,255}));
      Ink(image,18,104,214,30); Ink(image,18,154,214,30); Ink(image,18,204,214,30); EXPECT_EQ(Pixel(image,245,245),kWhite);
    }); scenes.push_back(pictures[0]);
  }
  Exact(scenes[0],scenes[1],artifacts,"generated-markup-backends");
}

// Each fixture deliberately omits AP; ordinary rendering invokes the core
// FreeText generator rather than the SDK's editable annotation app stream.
std::vector<uint8_t> FreeTextStyles(bool existing_form) {
  std::vector<std::string> objects={
    std::string("<< /Type /Catalog /Pages 2 0 R")+(existing_form?" /AcroForm 9 0 R":"")+" >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Resources << >> /Annots [4 0 R 5 0 R 6 0 R 7 0 R 8 0 R] >>"};
  const std::array<const char*,5> styles={"S","D","B","I","U"};
  for(int i=0;i<5;++i) {
    const int y=320-i*70;
    objects.push_back("<< /Type /Annot /Subtype /FreeText /Rect [20 "+std::to_string(y)+" 380 "+std::to_string(y+55)+"] /Contents (First line of words\\rSecond line) /DA (/"+
      std::string(existing_form?"Missing":"Helv")+" "+(i==0?"0":"12")+" Tf 0 g) /Q "+std::to_string(i%3)+
      " /C [1 1 .9] /BS << /S /"+styles[i]+" /W "+(i==0?"0":"2")+" /D [3 2 1] >> /F 4 >>");
  }
  objects.push_back("<< /Fields [] /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 10 0 R >> >> >>");
  objects.push_back("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
  return PdfFixture(objects);
}

TEST(CoverageEditing, FreeTextFiveBorderStylesAutoSizeAlignmentAndFontFallback) {
  Artifacts artifacts;
  const std::vector<std::array<int,4>> regions={{23,28,354,49},{23,98,354,49},{23,168,354,49},{23,238,354,49},{23,308,354,49}};
  const auto comparator=[&](const Raster& a,const Raster& b,Artifacts& out,const std::string& name){TextRegions(a,b,out,name,regions);};
  for(bool existing_form:{false,true}) {
    std::vector<Raster> scenes;
    for(const auto* writer:Implementations()) {
      SCOPED_TRACE(writer->name());
      SCOPED_TRACE(existing_form); const auto& api=writer->api(); const auto bytes=FreeTextStyles(existing_form);
      Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument); ASSERT_NE(doc.get(),nullptr);
      {Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage); Render(api,page.get(),400,400,FPDF_ANNOT);}
      const auto pictures=RoundTrip(*writer,doc.get(),{{400,400,FPDF_ANNOT}},artifacts,"free-text-"+std::to_string(existing_form),[](const Api& read,FPDF_DOCUMENT,FPDF_PAGE page,int){
        ASSERT_EQ(read.FPDFPage_GetAnnotCount(page),5);
        for(int i=0;i<5;++i) {Handle annot(read.FPDFPage_GetAnnot(page,i),read.FPDFPage_CloseAnnot); EXPECT_EQ(read.FPDFAnnot_GetSubtype(annot.get()),FPDF_ANNOT_FREETEXT); const auto ap=Appearance(read,annot.get()); EXPECT_NE(ap.find(u"Tj"),std::u16string::npos);}
        const auto image=Render(read,page,400,400,FPDF_ANNOT); for(int i=0;i<5;++i) Ink(image,24,29+i*70,352,47); EXPECT_EQ(Pixel(image,5,5),kWhite);
      },comparator); scenes.push_back(pictures[0]);
    }
    comparator(scenes[0],scenes[1],artifacts,"free-text-backends-"+std::to_string(existing_form));
  }
}

std::vector<uint8_t> WidgetAppearanceFixture() {
  const std::array<std::string,9> fields={
    "/FT /Tx /V (Aligned left) /Q 0 /DA (/Helv 0 Tf 0 g) /BS << /S /S /W 1 >>",
    "/FT /Tx /V (First line\\rSecond line) /Ff 4096 /Q 1 /DA (/Helv 10 Tf 0 g) /BS << /S /D /W 2 /D [3 2 1] >>",
    "/FT /Tx /V (secret) /Ff 8192 /Q 2 /MaxLen 12 /DA (/Helv 12 Tf 0 g) /BS << /S /B /W 2 >>",
    "/FT /Tx /V (12345) /Ff 16777216 /Q 1 /MaxLen 6 /DA (/Helv 0 Tf 0 g) /BS << /S /I /W 2 >>",
    "/FT /Ch /V (g) /Ff 131072 /Opt [(Red) [(g) (Green)] (Blue)] /DA (/Helv 12 Tf 0 g) /BS << /S /U /W 2 >>",
    "/FT /Ch /V [(Green) (Gold)] /Ff 2097152 /Opt [(Red) [(g) (Green)] (Blue) (Gold)] /I [1 3] /TI 1 /DA (/Helv 10 Tf 0 g) /BS << /S /S /W 1 >>",
    "/FT /Tx /V (Rotate ninety) /DA (/Helv 10 Tf 0 g) /BS << /S /S /W 1 >>",
    "/FT /Tx /V (Rotate half) /DA (/Helv 10 Tf 0 g) /BS << /S /S /W 1 >>",
    "/FT /Tx /V (Rotate three) /DA (/Helv 10 Tf 0 g) /BS << /S /S /W 1 >>"};
  std::vector<std::string> objects={"<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>","<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 420 400] /Resources << >> /Annots [6 0 R 7 0 R 8 0 R 9 0 R 10 0 R 11 0 R 12 0 R 13 0 R 14 0 R] >>",
    "<< /NeedAppearances true /Fields [6 0 R 7 0 R 8 0 R 9 0 R 10 0 R 11 0 R 12 0 R 13 0 R 14 0 R] /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"};
  for(int i=0;i<9;++i) {const int x=20+(i%3)*135,y=270-(i/3)*125; objects.push_back(
    "<< /Type /Annot /Subtype /Widget /P 3 0 R /T (field"+std::to_string(i)+") /Rect ["+std::to_string(x)+" "+std::to_string(y)+" "+std::to_string(x+115)+" "+std::to_string(y+105)+"] /MK << /BC [0 0 0] /BG [1 1 1] /R "+std::to_string(i<6?0:(i-5)*90)+" >> /F 4 "+fields[i]+" >>");}
  return PdfFixture(objects);
}

TEST(CoverageEditing, CoreWidgetAppearanceGenerationTextChoiceStylesRotationThenFlatten) {
  Artifacts artifacts; std::vector<Raster> scenes; std::vector<std::array<int,4>> regions;
  for(int i=0;i<9;++i) regions.push_back({21+(i%3)*135,26+(i/3)*125,113,103});
  const auto comparator=[&](const Raster&a,const Raster&b,Artifacts&out,const std::string&name){TextRegions(a,b,out,name,regions);};
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name()); const auto& api=writer->api(); const auto bytes=WidgetAppearanceFixture();
    Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument); ASSERT_NE(doc.get(),nullptr);
    {
      // No form environment: this specifically reaches the core NeedAppearances
      // path, before an SDK widget controller can supply an editable AP.
      Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage); ASSERT_NE(page.get(),nullptr);
      const auto plain=Render(api,page.get(),420,400,FPDF_ANNOT);
      EXPECT_TRUE(std::all_of(plain.pixels.begin(),plain.pixels.end(),[](uint8_t b){return b==255;}));
      for(int i=0;i<9;++i) {Handle annot(api.FPDFPage_GetAnnot(page.get(),i),api.FPDFPage_CloseAnnot); const auto ap=Appearance(api,annot.get()); EXPECT_FALSE(ap.empty())<<i; EXPECT_NE(ap.find(u"Tj"),std::u16string::npos)<<i;}
      EXPECT_EQ(api.FPDFPage_Flatten(page.get(),FLAT_NORMALDISPLAY),FLATTEN_SUCCESS);
    }
    const auto pictures=RoundTrip(*writer,doc.get(),{{420,400}},artifacts,"core-widget-flatten",[](const Api& read,FPDF_DOCUMENT,FPDF_PAGE page,int){
      EXPECT_EQ(read.FPDFPage_GetAnnotCount(page),0); EXPECT_GT(read.FPDFPage_CountObjects(page),0);
      const auto image=Render(read,page,420,400); for(int i=0;i<9;++i) Ink(image,23+(i%3)*135,28+(i/3)*125,109,99); EXPECT_EQ(Pixel(image,5,5),kWhite);
    },comparator); scenes.push_back(pictures[0]);
  }
  comparator(scenes[0],scenes[1],artifacts,"core-widget-backends");
}

// An AP state's nonzero BBox and quarter-turn Matrix force flattening to map
// appearance coordinates rather than treating its stream as page coordinates.
std::vector<uint8_t> FlattenContentsFixture(int kind) {
  const std::string contents=kind==0 ? "" : kind==1 ? " /Contents 8 0 R" : " /Contents [8 0 R 9 0 R]";
  return PdfFixture({"<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 128] /Resources << /XObject << /FFT0 10 0 R >> >> /Annots [4 0 R 5 0 R 6 0 R]"+contents+" >>",
    "<< /Type /Annot /Subtype /Square /Rect [24 24 64 64] /F 4 /AS /On /AP << /N << /Off 10 0 R /On 7 0 R >> >> >>",
    "<< /Type /Annot /Subtype /Square /Rect [80 80 112 112] /F 4 /AP << /N << /Only 10 0 R >> >> >>",
    "<< /Type /Annot /Subtype /Square /Rect [4 80 16 100] /F 4 /AP << /N 11 0 R >> >>",
    PdfStream("/Type /XObject /Subtype /Form /BBox [10 20 50 60] /Matrix [0 1 -1 0 80 0] /Resources << /Font << /Unused 12 0 R >> >>","1 0 0 rg 10 20 40 40 re f"),
    PdfStream("","0 0 1 rg 8 8 8 8 re f\n"),PdfStream("","0 1 0 rg 112 8 8 8 re f\n"),
    PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 32 32] /Resources << >>","0 1 1 rg 0 0 32 32 re f"),
    PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 0 0]","0 0 0 rg 0 0 12 20 re f"),
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding << /BaseEncoding /Unsupported /Differences [] >> >>"});
}

TEST(CoverageEditing, FlattenAppearanceStatesMatricesResourceCollisionAndContentShapes) {
  Artifacts artifacts;
  for(int kind:{0,1,2}) {
    std::vector<Raster> scenes;
    for(const auto* writer:Implementations()) {
      SCOPED_TRACE(writer->name());
      SCOPED_TRACE(kind); const auto& api=writer->api(); const auto bytes=FlattenContentsFixture(kind);
      Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument); ASSERT_NE(doc.get(),nullptr);
      Raster before;
      {Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage); before=Render(api,page.get(),128,128,FPDF_ANNOT); EXPECT_EQ(api.FPDFPage_Flatten(page.get(),FLAT_NORMALDISPLAY),FLATTEN_SUCCESS);}
      const auto pictures=RoundTrip(*writer,doc.get(),{{}},artifacts,"flatten-content-"+std::to_string(kind),[kind](const Api& read,FPDF_DOCUMENT,FPDF_PAGE page,int){
        EXPECT_EQ(read.FPDFPage_GetAnnotCount(page),0); const auto image=Render(read,page,128,128);
        EXPECT_EQ(Pixel(image,40,88),(std::array<uint8_t,4>{0,0,255,255}));
        EXPECT_EQ(Pixel(image,96,32),(std::array<uint8_t,4>{255,255,0,255}));
        EXPECT_EQ(Pixel(image,10,38),kWhite);
        EXPECT_EQ(Pixel(image,12,116),kind ? (std::array<uint8_t,4>{255,0,0,255}) : kWhite);
        EXPECT_EQ(Pixel(image,116,116),kind==2 ? (std::array<uint8_t,4>{0,255,0,255}) : kWhite);
      });
      // With no AS, display resolves Off (absent here), while flatten's
      // documented fallback chooses the sole appearance state.
      EXPECT_EQ(Pixel(before,96,32),kWhite);
      Raster after=pictures[0];
      for(int y=16;y<48;++y) for(int x=80;x<112;++x) for(int c=0;c<4;++c) {
        before.pixels[(y*128+x)*4+c]=255; after.pixels[(y*128+x)*4+c]=255;
      }
      Exact(before,after,artifacts,writer->name()+"-flatten-before-after-"+std::to_string(kind)); scenes.push_back(pictures[0]);
    }
    Exact(scenes[0],scenes[1],artifacts,"flatten-content-backends-"+std::to_string(kind));
  }
}

}  // namespace
}  // namespace pdfoxium::integration
