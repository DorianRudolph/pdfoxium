#include "test_support.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <fstream>
#include <functional>
#include <iterator>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {

// These fixtures exercise PDF structures without public constructors. They are
// selected from the first CAPI coverage report: shading/mesh/sampled/stitching,
// transfer-image translation, JPX, Fax and JBIG2 were wholly or mostly unvisited.
using RGB=std::array<int,3>;
constexpr int kSize=100;
constexpr int kFlags=FPDF_RENDER_NO_SMOOTHIMAGE;

RGB Color(const Raster& raster,int x,int pdf_y) {
  const int y=raster.height-1-pdf_y;
  EXPECT_GE(x,0);EXPECT_LT(x,raster.width);EXPECT_GE(y,0);EXPECT_LT(y,raster.height);
  if(x<0 || y<0 || x>=raster.width || y>=raster.height) return {};
  const auto i=(static_cast<size_t>(y)*raster.width+x)*4;
  return {raster.pixels[i+2],raster.pixels[i+1],raster.pixels[i]};
}
void ExpectColor(const Raster& raster,int x,int y,RGB expected,int delta=0) {
  const auto actual=Color(raster,x,y);
  for(size_t c=0;c<3;++c) EXPECT_NEAR(actual[c],expected[c],delta)<<"at "<<x<<","<<y<<" channel "<<c;
}
std::string ReadBytes(const std::filesystem::path& path) {
  std::ifstream stream(path,std::ios::binary);
  EXPECT_TRUE(stream.good())<<path.u8string();
  return std::string(std::istreambuf_iterator<char>(stream),{});
}
std::string Resource(const std::string& name) {
  return ReadBytes(TestRoot()/"upstream/pdfium/testing/resources"/name);
}

std::vector<uint8_t> Scene(const std::string& content,const std::string& resources,
                            const std::vector<std::string>& extra,int width=kSize,int height=kSize) {
  std::vector<std::string> objects={"<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 "+std::to_string(width)+" "+std::to_string(height)+"] "
      "/Resources << "+resources+" >> /Contents 4 0 R >>",PdfStream("",content)};
  objects.insert(objects.end(),extra.begin(),extra.end());return PdfFixture(objects);
}
std::vector<uint8_t> Shading(const std::string& dictionary,const std::vector<std::string>& functions={},
                              bool pattern=false,const std::string& data="") {
  auto objects=functions;
  objects.insert(objects.begin(),data.empty()?"<< "+dictionary+" >>":PdfStream(dictionary,data));
  return Scene(pattern?"/Pattern cs /P scn 0 0 100 100 re f":"/S sh",
      pattern?"/Pattern << /P << /Type /Pattern /PatternType 2 /Shading 5 0 R >> >>":"/Shading << /S 5 0 R >>",objects);
}

// Preserve input content streams while saving. In particular, native's editing
// content generator omits shading objects; invoking that generator is unrelated
// to saving an existing valid shading document and would obscure the oracle.
void CompareFixture(const std::vector<uint8_t>& bytes,Artifacts& artifacts,const std::string& name,
                    const std::function<void(const Raster&)>& inspect,int delta=0,
                    int width=kSize,int height=kSize,int flags=kFlags,
                    const std::function<void(const Api&,FPDF_DOCUMENT,FPDF_PAGE)>& semantic={},
                    const std::function<void(Raster&,bool)>& oracle_control={}) {
  const auto compare=[&](const Raster& a,bool native_a,const Raster& b,bool native_b,const std::string& label) {
    if(!oracle_control) {CompareRaster(a,b,artifacts,label,delta);return;}
    Raster aa=a,bb=b;
    oracle_control(aa,native_a);oracle_control(bb,native_b);
    // Preserve exact raw same-engine behavior; only cross-engine comparisons
    // use an independently checked native-defect control.
    if(native_a==native_b)CompareRaster(a,b,artifacts,label,delta);
    else CompareRaster(aa,bb,artifacts,label,delta);
  };
  std::array<Raster,2> original;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(name+" writer="+writer->name());const auto& api=writer->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(document.get(),nullptr);ASSERT_EQ(api.FPDF_GetPageCount(document.get()),1);
    Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
    if(semantic) {ASSERT_NO_FATAL_FAILURE(semantic(api,document.get(),page.get()));}
    original[backend]=Render(api,page.get(),width,height,flags);
    ASSERT_NO_FATAL_FAILURE(inspect(original[backend]));
    const auto saved=SaveDocument(api,document.get(),artifacts,name+"-"+writer->name());ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE("reader="+reader->name());const auto& read=reader->api();
      Handle reopened(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);ASSERT_NE(reopened.get(),nullptr);
      Handle loaded(read.FPDF_LoadPage(reopened.get(),0),read.FPDF_ClosePage);ASSERT_NE(loaded.get(),nullptr);
      if(semantic) {ASSERT_NO_FATAL_FAILURE(semantic(read,reopened.get(),loaded.get()));}
      const auto after=Render(read,loaded.get(),width,height,flags);ASSERT_NO_FATAL_FAILURE(inspect(after));
      compare(original[backend],writer->name()=="native",after,reader->name()=="native",name+"-"+writer->name()+"-"+reader->name());
    }
    ++backend;
  }
  compare(original[0],true,original[1],false,name+"-native-rust");
}

class Bits {
 public:
  void Add(uint32_t value,unsigned count) {
    for(unsigned n=count;n>0;--n) {
      if(bit_==0) data_.push_back(0);
      data_.back()=static_cast<char>(static_cast<uint8_t>(data_.back())|(((value>>(n-1))&1)<<(7-bit_)));
      bit_=(bit_+1)%8;
    }
  }
  std::string Take() const {return data_;}
 private:
  unsigned bit_=0;std::string data_;
};
std::string SampleFunction(unsigned bits,const std::vector<std::array<uint32_t,3>>& samples,
                            const std::string& size="[2]",const std::string& domain="[0 1]",
                            const std::string& more="") {
  Bits writer;for(const auto& sample:samples) for(auto component:sample) writer.Add(component,bits);
  return PdfStream("/FunctionType 0 /Domain "+domain+" /Range [0 1 0 1 0 1] /Size "+size+
      " /BitsPerSample "+std::to_string(bits)+" "+more,writer.Take());
}
std::string Exponential(const std::string& c0,const std::string& c1,const std::string& n="1") {
  return "<< /FunctionType 2 /Domain [0 1] /C0 "+c0+" /C1 "+c1+" /N "+n+" >>";
}

void CompareExpectedWithNativeException(const std::vector<uint8_t>& bytes,Artifacts& artifacts,
    const std::string& name,const std::function<Raster(bool)>& expected,int delta=0) {
  const auto inspect=[&](const Library& engine,FPDF_PAGE page,const std::string& label) {
    const auto actual=Render(engine.api(),page,kSize,kSize,kFlags);
    CompareRaster(expected(engine.name()=="rust"),actual,artifacts,label,delta);
  };
  for(const auto* writer:Implementations()) {
    const auto& api=writer->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
    Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
    inspect(*writer,page.get(),name+"-original-"+writer->name());
    const auto saved=SaveDocument(api,document.get(),artifacts,name+"-"+writer->name());ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      const auto& read=reader->api();
      Handle reopened(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);ASSERT_NE(reopened.get(),nullptr);
      Handle loaded(read.FPDF_LoadPage(reopened.get(),0),read.FPDF_ClosePage);ASSERT_NE(loaded.get(),nullptr);
      inspect(*reader,loaded.get(),name+"-"+writer->name()+"-"+reader->name());
    }
  }
}
void CompareConstantWithNativeException(const std::vector<uint8_t>& bytes,Artifacts& artifacts,
                                        const std::string& name,RGB native,RGB correct) {
  CompareExpectedWithNativeException(bytes,artifacts,name,[&](bool rust) {
    const auto color=rust?correct:native;
    Raster expected{kSize,kSize,std::vector<uint8_t>(kSize*kSize*4,0)};
    for(size_t pixel=0;pixel<expected.pixels.size();pixel+=4) {
      expected.pixels[pixel]=static_cast<uint8_t>(color[2]);
      expected.pixels[pixel+1]=static_cast<uint8_t>(color[1]);
      expected.pixels[pixel+2]=static_cast<uint8_t>(color[0]);
      expected.pixels[pixel+3]=255;
    }
    return expected;
  });
}

TEST(CoverageRendering, SampledStitchingAndCalculatorFunctionsDriveRealShadings) {
  Artifacts artifacts;
  const std::string axial="/ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 100 0] /Extend [true true] /Function 6 0 R";
  for(unsigned bits:{1u,2u,4u,8u,12u,16u}) {
    const uint32_t maximum=(1u<<bits)-1;
    const auto bytes=Shading(axial,{SampleFunction(bits,{{0,0,maximum},{maximum,0,0}})});
    ASSERT_NO_FATAL_FAILURE(CompareFixture(bytes,artifacts,"sampled-"+std::to_string(bits),[](const Raster& raster) {
      ExpectColor(raster,25,50,{64,0,191},3);ExpectColor(raster,75,50,{191,0,64},3);
    },1));
  }
  // A one-point sampled grid is constant; pinned native incorrectly scales
  // its sole sample by encoded input zero. Correctness takes precedence.
  ASSERT_NO_FATAL_FAILURE(CompareConstantWithNativeException(Shading(axial,
      {SampleFunction(8,{{0,255,0}},"[1]","[0 1]","/Encode [0 0] /Decode [0 1 0 1 0 1]")}),
      artifacts,"sampled-single-grid",{0,0,0},{0,255,0}));
  const auto sampled2d=Shading("/ShadingType 1 /ColorSpace /DeviceRGB /Domain [0 1 0 1] /Matrix [100 0 0 100 0 0] /Function 6 0 R",
      {SampleFunction(8,{{0,0,0},{255,0,0},{0,255,0},{255,255,0}},"[2 2]","[0 1 0 1]")});
  ASSERT_NO_FATAL_FAILURE(CompareFixture(sampled2d,artifacts,"sampled-two-dimensional",[](const Raster& raster) {
    ExpectColor(raster,25,25,{64,64,0},4);ExpectColor(raster,75,75,{191,191,0},4);
  },1));
  const std::string stitching="<< /FunctionType 3 /Domain [0 1] /Functions [7 0 R 8 0 R 9 0 R] /Bounds [0.25 0.75] /Encode [0 1 1 0 0 1] >>";
  ASSERT_NO_FATAL_FAILURE(CompareFixture(Shading(axial,{stitching,Exponential("[1 0 0]","[1 0 0]"),
      Exponential("[0 1 0]","[0 1 0]"),Exponential("[0 0 1]","[0 0 1]")}),artifacts,"stitching-three-domains",[](const Raster& raster) {
    ExpectColor(raster,10,50,{255,0,0});ExpectColor(raster,50,50,{0,255,0});ExpectColor(raster,90,50,{0,0,255});
  },1));
  // Every output below depends on the operator under test. These analytical
  // quarter/midpoint/three-quarter colors distinguish stack corruption, wrong
  // scalar arithmetic, branch predicates and Boolean/integer logical behavior.
  using Values=std::array<double,3>;
  struct Calculator {const char* name;const char* program;std::function<Values(double)> expected;};
  const std::vector<Calculator> calculators={
      {"arithmetic","{ dup 0.25 add 0.5 mul exch 0.25 sub 2 div 0 }",[](double t){return Values{(t+0.25)/2,(t-0.25)/2,0};}},
      {"idiv-mod-conversion","{ 8 mul cvi dup 3 idiv 3 div exch 3 mod 3 div 0 }",[](double t){const int i=static_cast<int>(8*t);return Values{double(i/3)/3,double(i%3)/3,0};}},
      {"neg-abs","{ dup 0.5 sub abs exch neg 1 add 0 }",[](double t){return Values{std::abs(t-0.5),1-t,0};}},
      {"ceiling-floor","{ dup ceiling exch floor 0 }",[](double t){return Values{std::ceil(t),std::floor(t),0};}},
      {"round-truncate","{ 2 mul dup round 2 div exch truncate 2 div 0 }",[](double t){return Values{std::floor(2*t+0.5)/2,std::trunc(2*t)/2,0};}},
      {"sqrt-exp","{ dup sqrt exch 2 exp 0 }",[](double t){return Values{std::sqrt(t),t*t,0};}},
      {"sin-cos","{ 90 mul dup sin exch cos 0 }",[](double t){return Values{std::sin(t*1.5707963267948966),std::cos(t*1.5707963267948966),0};}},
      {"atan","{ 1 atan 90 div dup 1 exch sub 0 }",[](double t){const auto a=std::atan(t)/1.5707963267948966;return Values{a,1-a,0};}},
      {"ln-log","{ dup 1 add ln exch 9 mul 1 add log 0 }",[](double t){return Values{std::log(1+t),std::log10(1+9*t),0};}},
      {"cvi-cvr","{ 3 mul dup cvi cvr 3 div exch 3 div 0 }",[](double t){return Values{std::trunc(3*t)/3,t,0};}},
      {"dup","{ dup 0 }",[](double t){return Values{t,t,0};}},
      {"exch","{ 0.5 exch 0 }",[](double t){return Values{0.5,t,0};}},
      {"pop","{ 0.25 0.5 pop 0 }",[](double t){return Values{t,0.25,0};}},
      {"copy","{ 0.5 2 copy add exch pop 0 }",[](double t){return Values{t,t+0.5,0};}},
      {"index","{ 0.5 1 index 3 -1 roll pop 0 }",[](double t){return Values{0.5,t,0};}},
      {"roll-positive","{ 0.25 0.75 3 1 roll }",[](double t){return Values{0.75,t,0.25};}},
      {"roll-negative","{ 0.25 0.75 3 -1 roll }",[](double t){return Values{0.25,0.75,t};}},
      {"eq","{ 0.24609375 eq { 1 0 0 } { 0 1 0 } ifelse }",[](double t){return t==63./256?Values{1,0,0}:Values{0,1,0};}},
      {"ne","{ 0.24609375 ne { 1 0 0 } { 0 1 0 } ifelse }",[](double t){return t!=63./256?Values{1,0,0}:Values{0,1,0};}},
      {"lt","{ 0.49609375 lt { 1 0 0 } { 0 1 0 } ifelse }",[](double t){return t<127./256?Values{1,0,0}:Values{0,1,0};}},
      {"le","{ 0.49609375 le { 1 0 0 } { 0 1 0 } ifelse }",[](double t){return t<=127./256?Values{1,0,0}:Values{0,1,0};}},
      {"gt","{ 0.49609375 gt { 1 0 0 } { 0 1 0 } ifelse }",[](double t){return t>127./256?Values{1,0,0}:Values{0,1,0};}},
      {"ge","{ 0.49609375 ge { 1 0 0 } { 0 1 0 } ifelse }",[](double t){return t>=127./256?Values{1,0,0}:Values{0,1,0};}},
      {"if","{ dup 0.5 lt { 0.25 add } if dup 1 exch sub 0 }",[](double t){const auto a=t<0.5?t+0.25:t;return Values{a,1-a,0};}},
      {"integer-and-or-xor","{ pop 12 10 and 15 div 12 10 or 15 div 12 10 xor 15 div }",[](double){return Values{8./15,14./15,6./15};}},
      {"bitshift","{ 4 mul cvi dup 1 bitshift 8 div exch -1 bitshift 4 div 0 }",[](double t){const int i=static_cast<int>(4*t);return Values{double(i*2)/8,double(i/2)/4,0};}},
      {"boolean-and-not","{ dup 0.5 lt exch 0.25 ge and not { 1 0 0 } { 0 1 0 } ifelse }",[](double t){return !(t<0.5 && t>=0.25)?Values{1,0,0}:Values{0,1,0};}},
      {"boolean-or","{ dup 0.5 lt exch 0.5 gt or { 1 0 0 } { 0 1 0 } ifelse }",[](double t){return t!=0.5?Values{1,0,0}:Values{0,1,0};}},
      {"boolean-xor","{ dup 0.5 le exch 0.5 ge xor { 1 0 0 } { 0 1 0 } ifelse }",[](double t){return t!=0.5?Values{1,0,0}:Values{0,1,0};}},
      {"true-false-not","{ pop true false or not { 1 0 0 } { 0 1 0 } ifelse }",[](double){return Values{0,1,0};}},
      {"boolean-stack-provenance","{ 0.5 lt dup 1 copy exch 1 index 3 1 roll pop pop not exch pop { 1 0 0 } { 0 1 0 } ifelse }",[](double t){return t>=0.5?Values{1,0,0}:Values{0,1,0};}}};
  for(const auto& calculator:calculators) {
    ASSERT_NO_FATAL_FAILURE(CompareFixture(Shading(axial,{PdfStream("/FunctionType 4 /Domain [0 1] /Range [0 1 0 1 0 1]",calculator.program)}),
        artifacts,"calculator-"+std::string(calculator.name),[&](const Raster& raster) {
          for(int x:{25,50,75}) {
            // Axial shading first selects one of 256 entries using a
            // truncated 255-scaled position; each entry evaluates i/256.
            // Use that exact input for discontinuous integer/Boolean cases.
            const auto input=std::floor((x/100.0)*255)/256;
            const auto values=calculator.expected(input);RGB color{};
            for(size_t channel=0;channel<3;++channel) color[channel]=static_cast<int>(std::lround(std::clamp(values[channel],0.0,1.0)*255));
            ExpectColor(raster,x,50,color,3);
          }
        },1));
  }
  // Numeric not is bitwise complement, unlike logical Boolean not. Native
  // applies logical negation to both, so explicitly record that oracle defect.
  ASSERT_NO_FATAL_FAILURE(CompareConstantWithNativeException(Shading(axial,{PdfStream(
      "/FunctionType 4 /Domain [0 1] /Range [0 1 0 1 0 1]","{ pop 5 not -6 eq { 1 0 0 } { 0 1 0 } ifelse }")}),
      artifacts,"calculator-integer-not",{0,255,0},{255,0,0}));
  ASSERT_NO_FATAL_FAILURE(CompareFixture(Shading("/ShadingType 1 /ColorSpace /DeviceRGB /Domain [0 1 0 1] /Matrix [100 0 0 100 0 0] /Function 6 0 R",
      {PdfStream("/FunctionType 4 /Domain [0 1 0 1] /Range [0 1 0 1 0 1]","{ 0 }")}),artifacts,"calculator-two-inputs",[](const Raster& raster) {
        ExpectColor(raster,25,75,{64,191,0},4);ExpectColor(raster,75,25,{191,64,0},4);
      },1));
  ASSERT_NO_FATAL_FAILURE(CompareFixture(Shading("/ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 100 0] /Extend [true true] /Function [6 0 R 7 0 R 8 0 R]",
      {Exponential("[0]","[1]","2"),Exponential("[0]","[0]"),Exponential("[1]","[0]","0.5")}),artifacts,"independent-channel-functions",[](const Raster& raster) {
        ExpectColor(raster,25,50,{16,0,128},4);ExpectColor(raster,75,50,{143,0,34},4);
      },1));
}

TEST(CoverageRendering, LargeSourceStrokeCoordinatesTranslatedOntoPageRetainBorders) {
  Artifacts artifacts;
  for (const auto offset : std::array<std::array<int,2>,4>{{{10800,0},{-10800,0},{0,10800},{0,-10800}}}) {
    const std::string content = "q 3 0 0 3 " + std::to_string(-3*offset[0]) + " " +
        std::to_string(-3*offset[1]) + " cm .75 w 0 G " + std::to_string(10+offset[0]) + " " +
        std::to_string(10+offset[1]) + " 20 20 re S Q";
    ASSERT_NO_FATAL_FAILURE(CompareFixture(Scene(content,"",{}),artifacts,
        "translated-stroke-"+std::to_string(offset[0])+"-"+std::to_string(offset[1]),
        [](const Raster& image) {
          ExpectColor(image,30,50,{0,0,0}); ExpectColor(image,90,50,{0,0,0});
          ExpectColor(image,50,30,{0,0,0}); ExpectColor(image,50,90,{0,0,0});
          ExpectColor(image,60,60,{255,255,255}); ExpectColor(image,5,5,{255,255,255});
        },1));
  }
}

TEST(CoverageRendering, RadialRootsExtensionAndShadingPatternBackgroundAreVisible) {
  Artifacts artifacts;
  const std::string function=Exponential("[0 0 1]","[1 0 0]");
  const auto tangent_control=[](int x,bool linear) {
    return [=](Raster& image,bool native) {
      // Native's NaN-to-int index conversion can paint blue outside the
      // unextended common tangent (notably ARM64); Rust must leave it white.
      // Equal-radius shading has real interior pixels on rows30..70.
      for(int y=0;y<kSize;++y) {
        if(!linear && y>=30 && y<=70)continue;
        const auto color=Color(image,x,image.height-1-y);
        if(native)EXPECT_TRUE((color==RGB{255,255,255} || color==RGB{0,0,255}))<<x<<","<<y;
        else EXPECT_EQ(color,(RGB{255,255,255}))<<x<<","<<y;
        const auto at=(static_cast<size_t>(y)*image.width+x)*4;
        std::fill_n(image.pixels.begin()+at,4,255);
      }
    };
  };
  for(bool start:{false,true}) for(bool end:{false,true}) {
    const auto dictionary="/ShadingType 3 /ColorSpace /DeviceRGB /Coords [50 50 10 50 50 40] /BBox [5 5 95 95] /Extend ["+
        std::string(start?"true":"false")+" "+(end?"true":"false")+"] /Function 6 0 R";
    ASSERT_NO_FATAL_FAILURE(CompareFixture(Shading(dictionary,{function}),artifacts,"radial-extend-"+std::to_string(start)+std::to_string(end),[&](const Raster& raster) {
      ExpectColor(raster,50,50,start?RGB{0,0,255}:RGB{255,255,255},2);
      ExpectColor(raster,92,50,end?RGB{254,0,0}:RGB{255,255,255},3);
      ExpectColor(raster,75,50,{128,0,128},5);ExpectColor(raster,1,1,{255,255,255});
    },1));
  }
  ASSERT_NO_FATAL_FAILURE(CompareFixture(Shading("/ShadingType 3 /ColorSpace /DeviceRGB /Coords [50 50 40 50 50 10] /Extend [true true] /Function 6 0 R",{function}),
      artifacts,"radial-decreasing",[](const Raster& raster) {
        ExpectColor(raster,50,50,{254,0,0},3);ExpectColor(raster,92,50,{0,0,255},3);
      },1));
  ASSERT_NO_FATAL_FAILURE(CompareFixture(Shading("/ShadingType 3 /ColorSpace /DeviceRGB /Coords [30 50 0 70 50 40] /Extend [false true] /Function 6 0 R",{function}),
      artifacts,"radial-linear-root",[](const Raster& raster) {ExpectColor(raster,90,50,{191,0,64},6);},1,kSize,kSize,kFlags,{},tangent_control(30,true)));
  ASSERT_NO_FATAL_FAILURE(CompareFixture(Shading("/ShadingType 3 /ColorSpace /DeviceRGB /Coords [20 50 20 80 50 20] /Extend [false false] /Function 6 0 R",{function}),
      artifacts,"radial-equal-radius",[](const Raster& raster) {
        EXPECT_NE(Color(raster,50,50),(RGB{255,255,255}));ExpectColor(raster,1,1,{255,255,255});
      },1,kSize,kSize,kFlags,{},tangent_control(20,false)));
  ASSERT_NO_FATAL_FAILURE(CompareFixture(Shading("/ShadingType 2 /ColorSpace /DeviceRGB /Coords [20 50 80 50] /BBox [10 10 90 90] /Extend [false false] /Background [1 1 0] /Function 6 0 R",{function},true),
      artifacts,"pattern-background-and-bbox",[](const Raster& raster) {
        ExpectColor(raster,12,50,{255,255,0});ExpectColor(raster,50,50,{128,0,128},4);ExpectColor(raster,95,50,{255,255,255});
      },1));
}

void MeshPoint(Bits& bits,float x,float y) {
  bits.Add(static_cast<uint32_t>(std::lround(x*65535/100)),16);
  bits.Add(static_cast<uint32_t>(std::lround(y*65535/100)),16);
}
void MeshColor(Bits& bits,RGB color,bool function=false) {
  if(function) bits.Add(128,8);
  else for(int component:color) bits.Add(static_cast<uint32_t>(component),8);
}
std::string MeshData(int type,bool gradient,bool function=false,bool continuation=false) {
  Bits bits;const RGB green{0,255,0};
  if(type==4) {
    const std::array<std::array<float,2>,5> points={{{10,10},{50,10},{10,50},{50,50},{90,10}}};
    for(size_t i=0;i<points.size();++i) {
      bits.Add(i<3?0:i==3?1:2,8);MeshPoint(bits,points[i][0],points[i][1]);MeshColor(bits,green,function);
    }
  } else if(type==5) {
    for(float y:{10.f,50.f,90.f}) for(float x:{10.f,50.f,90.f}) {
      MeshPoint(bits,x,y);MeshColor(bits,gradient?RGB{static_cast<int>(x/100*255),static_cast<int>(y/100*255),0}:green,function);
    }
  } else {
    const float right=continuation?50.f:90.f,third=(right-10)/3;
    std::vector<std::array<float,2>> points={
        {10,10},{10+third,10},{right-third,10},{right,10},
        {right,10+third},{right,right-third},{right,right},{right-third,right},
        {10+third,right},{10,right},{10,right-third},{10,10+third}};
    if(type==7) points.insert(points.end(),{{10+third,10+third},{right-third,10+third},
        {right-third,right-third},{10+third,right-third}});
    bits.Add(0,8);for(const auto& p:points) MeshPoint(bits,p[0],p[1]);
    const std::array<RGB,4> colors={{{255,0,0},{0,255,0},{0,0,255},{255,255,255}}};
    for(size_t i=0;i<colors.size();++i) MeshColor(bits,gradient?colors[i]:green,function);
    if(continuation) {
      // Flag1 shares the preceding patch's right edge as the new first edge.
      bits.Add(1,8);
      const std::array<std::array<float,2>,8> next={{{63.3333f,50},{76.6667f,50},{90,50},{90,36.6667f},
          {90,23.3333f},{90,10},{76.6667f,10},{63.3333f,10}}};
      for(const auto& p:next) MeshPoint(bits,p[0],p[1]);
      if(type==7) for(const auto& p:std::array<std::array<float,2>,4>{{{63.3333f,23.3333f},{63.3333f,36.6667f},
          {76.6667f,36.6667f},{76.6667f,23.3333f}}}) MeshPoint(bits,p[0],p[1]);
      MeshColor(bits,green,function);MeshColor(bits,green,function);
    }
  }
  return bits.Take();
}

TEST(CoverageRendering, GouraudCoonsAndTensorMeshesInterpolateAndContinuePatches) {
  Artifacts artifacts;
  for(int type:{4,5,6,7}) for(bool function:{false,true}) {
    const auto dictionary="/ShadingType "+std::to_string(type)+" /ColorSpace /DeviceRGB /BitsPerCoordinate 16 /BitsPerComponent 8 /BitsPerFlag 8 "+
        (type==5?std::string("/VerticesPerRow 3 "):"")+"/Decode [0 100 0 100 0 1 "+(function?std::string(""):"0 1 0 1 ")+"] "+(function?"/Function 6 0 R":"");
    ASSERT_NO_FATAL_FAILURE(CompareFixture(Shading(dictionary,function?std::vector<std::string>{Exponential("[0 1 0]","[0 1 0]")}:std::vector<std::string>{},
        false,MeshData(type,false,function)),artifacts,"mesh-"+std::to_string(type)+"-function"+std::to_string(function),[&](const Raster& raster) {
          ExpectColor(raster,25,25,{0,255,0},1);ExpectColor(raster,1,1,{255,255,255});
          if(type!=4) ExpectColor(raster,75,75,{0,255,0},1);
        },1));
  }
  for(int type:{6,7}) {
    const auto dictionary="/ShadingType "+std::to_string(type)+" /ColorSpace /DeviceRGB /BitsPerCoordinate 16 /BitsPerComponent 8 /BitsPerFlag 8 /Decode [0 100 0 100 0 1 0 1 0 1]";
    ASSERT_NO_FATAL_FAILURE(CompareFixture(Shading(dictionary,{},false,MeshData(type,true)),artifacts,"patch-gradient-"+std::to_string(type),[](const Raster& raster) {
      ExpectColor(raster,50,50,{128,128,128},20);
      EXPECT_NE(Color(raster,25,25),Color(raster,75,75));ExpectColor(raster,1,1,{255,255,255});
    },1));
    ASSERT_NO_FATAL_FAILURE(CompareFixture(Shading(dictionary,{},false,MeshData(type,false,false,true)),artifacts,"patch-continuation-"+std::to_string(type),[](const Raster& raster) {
      ExpectColor(raster,25,25,{0,255,0},1);ExpectColor(raster,75,25,{0,255,0},1);
      ExpectColor(raster,75,75,{255,255,255});
    },1));
  }
}

TEST(CoverageRendering, CalibratedSpotDeviceNAndIndexedImagesRetainColorMeaning) {
  Artifacts artifacts;
  struct Space {const char* name;std::string color_space,operands;RGB expected;int bound;std::vector<std::string> extra;};
  const std::vector<Space> cases={
      {"calgray","[/CalGray << /WhitePoint [0.9642 1 0.8249] /BlackPoint [0.01 0.01 0.01] /Gamma 2.2 >>]","0.5",{128,128,128},4,{}},
      {"calrgb","[/CalRGB << /WhitePoint [0.9505 1 1.089] /BlackPoint [0 0 0] /Gamma [2.2 2.2 2.2] /Matrix [0.4124 0.2126 0.0193 0.3576 0.7152 0.1192 0.1805 0.0722 0.9505] >>]","1 0 0",{255,0,0},8,{}},
      {"lab","[/Lab << /WhitePoint [0.9505 1 1.089] /Range [-128 127 -128 127] >>]","50 0 0",{119,119,119},5,{}},
      {"separation","[/Separation /Spot /DeviceRGB 5 0 R]","0.5",{128,128,255},2,{Exponential("[1 1 1]","[0 0 1]")}},
      {"devicen","[/DeviceN [/First /Second] /DeviceRGB 5 0 R]","0.25 0.75",{191,64,0},2,
       {PdfStream("/FunctionType 4 /Domain [0 1 0 1] /Range [0 1 0 1 0 1]","{ exch 0 }")}}};
  for(const auto& item:cases) {
    ASSERT_NO_FATAL_FAILURE(CompareFixture(Scene("/C cs "+item.operands+" scn 10 10 80 80 re f",
        "/ColorSpace << /C "+item.color_space+" >>",item.extra),artifacts,"color-"+std::string(item.name),[&](const Raster& raster) {
          ExpectColor(raster,50,50,item.expected,item.bound);ExpectColor(raster,1,1,{255,255,255});
        },2));
  }
  for(bool reverse:{false,true}) {
    const auto bytes=Scene("q 80 0 0 20 10 40 cm /I Do Q","/XObject << /I 5 0 R >>",
        {PdfStream("/Type /XObject /Subtype /Image /Width 4 /Height 1 /BitsPerComponent 2 /ColorSpace [/Indexed /DeviceRGB 3 <ff000000ff000000ffffff00>] "+
            std::string(reverse?"/Decode [3 0]":""),std::string(1,'\x1b'))});
    ASSERT_NO_FATAL_FAILURE(CompareFixture(bytes,artifacts,"indexed-two-bit-"+std::to_string(reverse),[&](const Raster& raster) {
      const std::array<RGB,4> expected={{{255,0,0},{0,255,0},{0,0,255},{255,255,0}}};
      for(int i=0;i<4;++i) ExpectColor(raster,20+i*20,50,expected[reverse?3-i:i]);
    }));
  }
}

TEST(CoverageRendering, AlphaLuminosityBackgroundAndTransferSoftMasksCompositeActualPixels) {
  Artifacts artifacts;
  for(bool luminosity:{false,true}) for(bool invert:{false,true}) for(bool backdrop:{false,true}) {
    const std::string mask=luminosity?"0.25 g 10 10 40 80 re f 0.75 g 50 10 40 80 re f":
        "/A gs 1 g 10 10 40 80 re f /B gs 1 g 50 10 40 80 re f";
    const auto bytes=Scene("/GS gs 1 0 0 rg 0 0 100 100 re f","/ExtGState << /GS 5 0 R >>",{
        "<< /SMask << /S /"+std::string(luminosity?"Luminosity":"Alpha")+" /G 6 0 R "+
          (invert?"/TR 9 0 R ":"")+(backdrop?"/BC [0.5 0.5 0.5] ":"")+">> >>",
        PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 100 100] /Group << /S /Transparency /CS /DeviceRGB /I true >> /Resources << /ExtGState << /A 7 0 R /B 8 0 R >> >>",mask),
        "<< /ca 0.25 >>","<< /ca 0.75 >>",Exponential("[1]","[0]")});
    ASSERT_NO_FATAL_FAILURE(CompareFixture(bytes,artifacts,"softmask-"+std::to_string(luminosity)+std::to_string(invert)+std::to_string(backdrop),[&](const Raster& raster) {
      ExpectColor(raster,25,50,{255,invert?64:191,invert?64:191},3);
      ExpectColor(raster,75,50,{255,invert?191:64,invert?191:64},3);
      const int outside=luminosity && backdrop?128:invert?0:255;
      ExpectColor(raster,3,3,{255,outside,outside},3);
    },1));
  }
  for(bool matte:{false,true}) {
    const auto bytes=Scene("q 80 0 0 80 10 10 cm /I Do Q","/XObject << /I 5 0 R >>",{
        PdfStream("/Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceRGB /SMask 6 0 R",std::string("\xff\x80\x80",3)),
        PdfStream("/Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceGray "+std::string(matte?"/Matte [1 1 1]":""),std::string(1,'\x80'))});
    ASSERT_NO_FATAL_FAILURE(CompareFixture(bytes,artifacts,"softmask-matte-"+std::to_string(matte),[&](const Raster& raster) {
      ExpectColor(raster,50,50,{255,matte?128:191,matte?128:191},3);ExpectColor(raster,1,1,{255,255,255});
    },1));
  }
}

TEST(CoverageRendering, TransferFunctionsTranslateGrayPaletteRgbAlphaAndStencilImages) {
  Artifacts artifacts;
  struct Image {const char* name;std::string dictionary,data;int width;};
  const std::vector<Image> images={
      {"one-bit","/ColorSpace /DeviceGray /BitsPerComponent 1",std::string(1,'\x55'),8},
      {"gray","/ColorSpace /DeviceGray /BitsPerComponent 8",std::string("\0\x55\xaa\xff",4),4},
      {"palette","/ColorSpace [/Indexed /DeviceRGB 3 <000000555555aaaaaaffffff>] /BitsPerComponent 8",std::string("\0\1\2\3",4),4},
      {"rgb","/ColorSpace /DeviceRGB /BitsPerComponent 8",std::string("\xff\0\0\0\0\xff",6),2},
      {"alpha","/ColorSpace /DeviceRGB /BitsPerComponent 8 /SMask 8 0 R",std::string("\xff\0\0\0\0\xff",6),2},
      {"stencil","/ImageMask true /BitsPerComponent 1",std::string(1,'\x55'),8}};
  // Transfer functions have one output, unlike the RGB sampled helpers above.
  const auto transfer=PdfStream("/FunctionType 0 /Domain [0 1] /Range [0 1] /Size [2] /BitsPerSample 8",std::string("\xff\0",2));
  for(const auto& image:images) {
    const auto bytes=Scene("/GS gs 0 0.5 1 rg q 80 0 0 80 10 10 cm /I Do Q",
        "/XObject << /I 5 0 R >> /ExtGState << /GS 6 0 R >>",{
          PdfStream("/Type /XObject /Subtype /Image /Width "+std::to_string(image.width)+" /Height 1 "+image.dictionary,image.data),
          "<< /TR 7 0 R >>",transfer,
          PdfStream("/Type /XObject /Subtype /Image /Width 2 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8",std::string("\xff\x80",2))});
    ASSERT_NO_FATAL_FAILURE(CompareFixture(bytes,artifacts,"transfer-"+std::string(image.name),[&](const Raster& raster) {
      ExpectColor(raster,1,1,{255,255,255});
      if(image.name==std::string("rgb") || image.name==std::string("alpha")) {
        ExpectColor(raster,30,50,{0,255,255},2);
        ExpectColor(raster,70,50,image.name==std::string("alpha")?RGB{255,255,127}:RGB{255,255,0},2);
      } else if(image.name==std::string("one-bit")) {
        ExpectColor(raster,15,50,{255,255,255});ExpectColor(raster,25,50,{0,0,0});
      } else if(image.name==std::string("stencil")) {
        EXPECT_NE(Color(raster,15,50),Color(raster,25,50));
        const auto a=Color(raster,15,50),b=Color(raster,25,50);
        const auto painted=a==RGB{255,255,255}?b:a;
        EXPECT_GT(painted[0],250);EXPECT_NEAR(painted[1],128,2);EXPECT_LT(painted[2],3);
      } else {
        for(int i=0;i<4;++i) ExpectColor(raster,20+i*20,50,{255-i*85,255-i*85,255-i*85},1);
      }
    },1));
  }
  const auto channelwise=Scene("/GS gs 0.25 0.5 0.75 rg 10 10 80 80 re f","/ExtGState << /GS 5 0 R >>",{
      "<< /TR [6 0 R 7 0 R 8 0 R 6 0 R] >>",Exponential("[0]","[1]"),
      Exponential("[0]","[1]","2"),Exponential("[1]","[0]")});
  ASSERT_NO_FATAL_FAILURE(CompareFixture(channelwise,artifacts,"transfer-independent-channels",[](const Raster& raster) {
    ExpectColor(raster,50,50,{64,64,64},3);
  },1));
}

// Valid stored DEFLATE blocks avoid introducing a test-only compression library.
// This is fixture encoding, not a production decoder or alternate PDF writer.
std::string StoredZlib(const std::string& data) {
  EXPECT_LT(data.size(),65536u);const auto size=static_cast<uint16_t>(data.size());
  std::string out("\x78\x01\x01",3);
  out.push_back(static_cast<char>(size));out.push_back(static_cast<char>(size>>8));
  const auto inverse=static_cast<uint16_t>(~size);out.push_back(static_cast<char>(inverse));out.push_back(static_cast<char>(inverse>>8));
  out+=data;uint32_t a=1,b=0;for(uint8_t byte:data) {a=(a+byte)%65521;b=(b+a)%65521;}
  const uint32_t adler=(b<<16)|a;for(int shift:{24,16,8,0}) out.push_back(static_cast<char>(adler>>shift));return out;
}
int Paeth(int left,int up,int upper_left) {
  const int prediction=left+up-upper_left;
  const int a=std::abs(prediction-left),b=std::abs(prediction-up),c=std::abs(prediction-upper_left);
  return a<=b && a<=c?left:b<=c?up:upper_left;
}
std::string PngPredict(const std::string& raw,int row_size,int bpp,int tag) {
  std::string encoded;
  for(size_t row=0;row<raw.size()/row_size;++row) {
    const int type=tag==5?static_cast<int>(row%5):tag;encoded.push_back(static_cast<char>(type));
    for(int i=0;i<row_size;++i) {
      const auto index=row*row_size+i;
      const int left=i>=bpp?static_cast<uint8_t>(raw[index-bpp]):0;
      const int up=row?static_cast<uint8_t>(raw[index-row_size]):0;
      const int upper_left=row && i>=bpp?static_cast<uint8_t>(raw[index-row_size-bpp]):0;
      const int prediction=type==1?left:type==2?up:type==3?(left+up)/2:type==4?Paeth(left,up,upper_left):0;
      encoded.push_back(static_cast<char>(static_cast<uint8_t>(raw[index])-prediction));
    }
  }
  return encoded;
}
std::string TiffPredict(const std::string& raw,int columns,int colors,int bpc) {
  if(bpc==1) {
    std::string out;
    for(uint8_t row:raw) {
      Bits bits;unsigned previous=0;
      for(int i=0;i<columns;++i) {const unsigned value=(row>>(7-i))&1;bits.Add(value^previous,1);previous=value;}
      out+=bits.Take();
    }
    return out;
  }
  const int bytes=bpc/8,step=colors*bytes,row_size=columns*step;auto out=raw;
  for(size_t row=0;row<raw.size()/row_size;++row) for(int i=step;i<row_size;i+=bytes) {
    const auto at=row*row_size+i;
    if(bytes==2) {
      const auto current=(static_cast<uint8_t>(raw[at])<<8)|static_cast<uint8_t>(raw[at+1]);
      const auto previous=(static_cast<uint8_t>(raw[at-step])<<8)|static_cast<uint8_t>(raw[at-step+1]);
      const uint16_t difference=static_cast<uint16_t>(current-previous);
      out[at]=static_cast<char>(difference>>8);out[at+1]=static_cast<char>(difference);
    } else out[at]=static_cast<char>(static_cast<uint8_t>(raw[at])-static_cast<uint8_t>(raw[at-step]));
  }
  return out;
}

TEST(CoverageRendering, PngAndTiffPredictorsDecodeEveryFilterPackedAndSixteenBitSamples) {
  Artifacts artifacts;
  const std::string rgb("\xff\0\0\0\xff\0\0\0\xff\xff\xff\xff"
                        "\0\xff\xff\xff\0\xff\xff\xff\0\0\0\0",24);
  const auto inspect=[](const Raster& raster) {
    const std::array<RGB,8> colors={{{255,0,0},{0,255,0},{0,0,255},{255,255,255},
        {0,255,255},{255,0,255},{255,255,0},{0,0,0}}};
    for(int row=0;row<2;++row) for(int column=0;column<4;++column)
      ExpectColor(raster,20+column*20,row==0?70:30,colors[row*4+column]);
  };
  for(int tag=0;tag<=5;++tag) {
    const auto data=StoredZlib(PngPredict(rgb,12,3,tag));
    const auto bytes=Scene("q 80 0 0 80 10 10 cm /I Do Q","/XObject << /I 5 0 R >>",{
        PdfStream("/Type /XObject /Subtype /Image /Width 4 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8 "
                  "/Filter /FlateDecode /DecodeParms << /Predictor "+std::to_string(tag==5?15:10+tag)+" /Colors 3 /Columns 4 /BitsPerComponent 8 >>",data)});
    ASSERT_NO_FATAL_FAILURE(CompareFixture(bytes,artifacts,"png-predictor-"+std::to_string(tag),inspect));
  }
  const auto inline_bytes=Scene("q 80 0 0 80 10 10 cm BI /W 4 /H 2 /CS /RGB /BPC 8 /F /Fl "
      "/DP << /Predictor 15 /Colors 3 /Columns 4 /BitsPerComponent 8 >> ID\n"+StoredZlib(PngPredict(rgb,12,3,4))+"\nEI Q","",{});
  ASSERT_NO_FATAL_FAILURE(CompareFixture(inline_bytes,artifacts,"inline-png-paeth",inspect));
  const auto tiff8=Scene("q 80 0 0 80 10 10 cm /I Do Q","/XObject << /I 5 0 R >>",{
      PdfStream("/Type /XObject /Subtype /Image /Width 4 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode "
          "/DecodeParms << /Predictor 2 /Colors 3 /Columns 4 /BitsPerComponent 8 >>",StoredZlib(TiffPredict(rgb,4,3,8)))});
  ASSERT_NO_FATAL_FAILURE(CompareFixture(tiff8,artifacts,"tiff-rgb-eight-bit",inspect));
  const std::string rgb16("\xff\xff\0\0\0\0\0\0\xff\xff\0\0\0\0\0\0\xff\xff",18);
  for(bool tiff:{false,true}) {
    const auto encoded=tiff?TiffPredict(rgb16,3,3,16):PngPredict(rgb16,18,6,4);
    const auto bytes=Scene("q 90 0 0 80 5 10 cm /I Do Q","/XObject << /I 5 0 R >>",{
        PdfStream("/Type /XObject /Subtype /Image /Width 3 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 16 /Filter /FlateDecode "
            "/DecodeParms << /Predictor "+std::string(tiff?"2":"15")+" /Colors 3 /Columns 3 /BitsPerComponent 16 >>",StoredZlib(encoded))});
    ASSERT_NO_FATAL_FAILURE(CompareFixture(bytes,artifacts,"sixteen-bit-"+std::to_string(tiff),[](const Raster& raster) {
      ExpectColor(raster,20,50,{255,0,0});ExpectColor(raster,50,50,{0,255,0});ExpectColor(raster,80,50,{0,0,255});
    }));
  }
  const std::string packed("\xb2\x4d",2);
  const auto onebit=Scene("q 80 0 0 80 10 10 cm /I Do Q","/XObject << /I 5 0 R >>",{
      PdfStream("/Type /XObject /Subtype /Image /Width 8 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /FlateDecode "
          "/DecodeParms << /Predictor 2 /Colors 1 /Columns 8 /BitsPerComponent 1 >>",StoredZlib(TiffPredict(packed,8,1,1)))});
  ASSERT_NO_FATAL_FAILURE(CompareFixture(onebit,artifacts,"tiff-packed-bit",[&](const Raster& raster) {
    for(int row=0;row<2;++row) for(int column=0;column<8;++column) {
      const int value=(static_cast<uint8_t>(packed[row])&(0x80>>column))?255:0;
      ExpectColor(raster,15+column*10,row==0?70:30,{value,value,value});
    }
  }));
}

TEST(CoverageRendering, PublishedJpeg2000RgbGrayEmbeddedAlphaAndSyccDecodeAndSave) {
  Artifacts artifacts;
  for(const std::string file:{"RGB.jp2","RGB-alpha.jp2","gray.jp2","gray-alpha.jp2"}) for(bool declared:{false,true}) {
    const bool gray=file.find("gray")==0,alpha=file.find("alpha")!=std::string::npos;
    const auto payload=Resource(file);ASSERT_FALSE(payload.empty());
    const auto bytes=Scene("0.5 g 0 0 100 100 re f q 80 0 0 80 10 10 cm /I Do Q","/XObject << /I 5 0 R >>",{
        PdfStream("/Type /XObject /Subtype /Image /Width 4 /Height 4 /BitsPerComponent 8 /Filter /JPXDecode "+
            std::string(declared?(gray?"/ColorSpace /DeviceGray ":"/ColorSpace /DeviceRGB "):"")+(alpha?"/SMaskInData 1":""),payload)});
    if(alpha) {
      // Independent lossless 4x4 samples: gray has black/white diagonal
      // quadrants; RGB has red/green above green/blue. Four central alpha
      // samples are 128, all others 255. Native drops gray opacity and
      // premattes RGB onto white before applying opacity again. Rust must
      // preserve the straight colors and composite with opacity once.
      ASSERT_NO_FATAL_FAILURE(CompareExpectedWithNativeException(bytes,artifacts,
          "jpx-"+file+"-declared"+std::to_string(declared),[gray](bool rust) {
            Raster expected{kSize,kSize,std::vector<uint8_t>(kSize*kSize*4,255)};
            for(int y=0;y<kSize;++y) for(int x=0;x<kSize;++x) {
              RGB color={128,128,128};
              if(x>=10 && x<90 && y>=10 && y<90) {
                const int column=(x-10)/20,row=(y-10)/20;
                if(gray) {
                  const int value=((column<2)==(row<2))?0:255;
                  color={value,value,value};
                } else {
                  color=row<2&&column<2?RGB{255,0,0}:
                      row>=2&&column>=2?RGB{0,0,255}:RGB{0,255,0};
                }
                if(column>=1 && column<=2 && row>=1 && row<=2) {
                  if(!gray && !rust)
                    for(auto& value:color)value=(value*128+255*127)/255;
                  if(!gray || rust)
                    for(auto& value:color)value=(value*128+128*127)/255;
                }
              }
              const auto pixel=(static_cast<size_t>(y)*kSize+x)*4;
              for(size_t channel=0;channel<3;++channel)
                expected.pixels[pixel+channel]=static_cast<uint8_t>(color[2-channel]);
            }
            return expected;
          },1));
      continue;
    }
    ASSERT_NO_FATAL_FAILURE(CompareFixture(bytes,artifacts,"jpx-"+file+"-declared"+std::to_string(declared),[&](const Raster& raster) {
      ExpectColor(raster,1,1,{128,128,128},1);
      if(!alpha && !gray) {
        ExpectColor(raster,30,70,{255,0,0},1);ExpectColor(raster,70,70,{0,255,0},1);
        ExpectColor(raster,30,30,{0,255,0},1);ExpectColor(raster,70,30,{0,0,255},1);
      } else if(!alpha) {
        ExpectColor(raster,30,70,{0,0,0},1);ExpectColor(raster,70,70,{255,255,255},1);
        ExpectColor(raster,30,30,{255,255,255},1);ExpectColor(raster,70,30,{0,0,0},1);
      }
    },1,kSize,kSize,kFlags,[](const Api& api,FPDF_DOCUMENT,FPDF_PAGE page) {
      ASSERT_EQ(api.FPDFPage_CountObjects(page),2);
      const auto image=api.FPDFPage_GetObject(page,1);ASSERT_EQ(api.FPDFPageObj_GetType(image),FPDF_PAGEOBJ_IMAGE);
      unsigned int width=0,height=0;ASSERT_TRUE(api.FPDFImageObj_GetImagePixelSize(image,&width,&height));
      EXPECT_EQ(width,4u);EXPECT_EQ(height,4u);
      Handle bitmap(api.FPDFImageObj_GetBitmap(image),api.FPDFBitmap_Destroy);ASSERT_NE(bitmap.get(),nullptr);
      EXPECT_EQ(api.FPDFBitmap_GetWidth(bitmap.get()),4);EXPECT_EQ(api.FPDFBitmap_GetHeight(bitmap.get()),4);
      EXPECT_GT(api.FPDFBitmap_GetStride(bitmap.get()),0);
    }));
  }
  // A JP2 enumerated colorspace is independent of its lossless component
  // samples. Reinterpret the known RGB samples as full-resolution Y/Cb/Cr to
  // reach the sYCC conversion without adding an encoder or external decoder.
  auto sycc=Resource("RGB.jp2");const auto colr=sycc.find("colr");ASSERT_NE(colr,std::string::npos);
  ASSERT_GT(sycc.size(),colr+10);sycc[colr+10]=18;
  ASSERT_NO_FATAL_FAILURE(CompareFixture(Scene("q 80 0 0 80 10 10 cm /I Do Q","/XObject << /I 5 0 R >>",{
      PdfStream("/Type /XObject /Subtype /Image /Width 4 /Height 4 /BitsPerComponent 8 /Filter /JPXDecode",sycc)}),
      artifacts,"jpx-sycc-444",[](const Raster& raster) {
        ExpectColor(raster,30,70,{76,255,28},3);ExpectColor(raster,1,1,{255,255,255});
      },1));
}

TEST(CoverageRendering, FaxGroupThreeGroupFourStencilPolarityAndJbig2GlobalsAreRealImages) {
  Artifacts artifacts;
  for(int k:{0,-1}) for(bool black:{false,true}) {
    const auto payload=ReadBytes(TestRoot()/"crates/pdfoxium-core/tests/fixtures"/(k==0?"fax_g3.bin":"fax_g4.bin"));
    ASSERT_FALSE(payload.empty());
    const auto bytes=Scene("0 0.5 1 rg q 80 0 0 80 10 10 cm /I Do Q","/XObject << /I 5 0 R >>",{
        PdfStream("/Type /XObject /Subtype /Image /Width 16 /Height 2 /ImageMask true /BitsPerComponent 1 /Filter /CCITTFaxDecode "
            "/DecodeParms << /K "+std::to_string(k)+" /Columns 16 /Rows 2 /EndOfBlock false /BlackIs1 "+(black?"true":"false")+" >>",payload)});
    ASSERT_NO_FATAL_FAILURE(CompareFixture(bytes,artifacts,"fax-"+std::to_string(k)+"-black"+std::to_string(black),[&](const Raster& raster) {
      const RGB paint{0,128,255},white{255,255,255};
      ExpectColor(raster,12,70,black?paint:white,1);ExpectColor(raster,87,70,black?white:paint,1);
      ExpectColor(raster,12,30,black?white:paint,1);ExpectColor(raster,87,30,black?paint:white,1);
      ExpectColor(raster,1,1,white);
    },1));
  }
  // This pinned file contains a genuine shared JBIG2 global symbol dictionary
  // and a 1152x720 page image, not a decoder-only synthetic byte stream.
  const auto data=Resource("bug_631912.pdf");ASSERT_FALSE(data.empty());
  const std::vector<uint8_t> fixture(data.begin(),data.end());
  ASSERT_NO_FATAL_FAILURE(CompareFixture(fixture,artifacts,"jbig2-global-symbols",[](const Raster& raster) {
    size_t marks=0;
    for(int y=30;y<120;++y) for(int x=30;x<220;++x) if(Color(raster,x,raster.height-1-y)!=RGB{255,255,255}) ++marks;
    EXPECT_GT(marks,100u);EXPECT_LT(marks,2000u);
    ExpectColor(raster,300,200,{255,255,255});ExpectColor(raster,600,20,{255,255,255});
  },0,691,432,kFlags,[](const Api& api,FPDF_DOCUMENT,FPDF_PAGE page) {
    EXPECT_NEAR(api.FPDF_GetPageWidthF(page),691.2f,0.001);EXPECT_FLOAT_EQ(api.FPDF_GetPageHeightF(page),432);
    ASSERT_EQ(api.FPDFPage_CountObjects(page),1);
    const auto image=api.FPDFPage_GetObject(page,0);ASSERT_EQ(api.FPDFPageObj_GetType(image),FPDF_PAGEOBJ_IMAGE);
    unsigned int width=0,height=0;ASSERT_TRUE(api.FPDFImageObj_GetImagePixelSize(image,&width,&height));
    EXPECT_EQ(width,1152u);EXPECT_EQ(height,720u);
    Handle bitmap(api.FPDFImageObj_GetBitmap(image),api.FPDFBitmap_Destroy);ASSERT_NE(bitmap.get(),nullptr);
    EXPECT_EQ(api.FPDFBitmap_GetWidth(bitmap.get()),1152);EXPECT_EQ(api.FPDFBitmap_GetHeight(bitmap.get()),720);
  }));
}

}  // namespace
}  // namespace pdfoxium::integration
