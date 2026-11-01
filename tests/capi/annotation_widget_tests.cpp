#include "test_support.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {

constexpr int kWidth=480,kHeight=360;
const FPDF_WCHAR* Wide(const std::u16string& value) {return reinterpret_cast<const FPDF_WCHAR*>(value.c_str());}

template <typename Getter>
void ExpectText(Getter get,const std::u16string& expected) {
  const auto needed=static_cast<unsigned long>((expected.size()+1)*2);
  ASSERT_EQ(get(nullptr,0),needed);
  for(unsigned long capacity:{0ul,needed-1,needed,needed+6}) {
    std::vector<FPDF_WCHAR> buffer(expected.size()+8,0x5a5a);const auto initial=buffer;
    ASSERT_EQ(get(buffer.data(),capacity),needed);
    if(capacity<needed) {EXPECT_EQ(buffer,initial);continue;}
    for(size_t i=0;i<expected.size();++i) EXPECT_EQ(buffer[i],expected[i]);
    EXPECT_EQ(buffer[expected.size()],0);
    for(size_t i=expected.size()+1;i<buffer.size();++i) EXPECT_EQ(buffer[i],0x5a5a);
  }
}

struct FormInfo:FPDF_FORMFILLINFO {
  explicit FormInfo(FPDF_PAGE owner):FPDF_FORMFILLINFO{},page(owner) {
    version=2;xfa_disabled=1;
    FFI_GetPage=GetPage;FFI_GetCurrentPage=Current;FFI_GetRotation=Rotation;
    FFI_Invalidate=Invalidate;
  }
  static FPDF_PAGE GetPage(FPDF_FORMFILLINFO* self,FPDF_DOCUMENT,int index) {
    return index==0?static_cast<FormInfo*>(self)->page:nullptr;
  }
  static FPDF_PAGE Current(FPDF_FORMFILLINFO* self,FPDF_DOCUMENT) {return static_cast<FormInfo*>(self)->page;}
  static int Rotation(FPDF_FORMFILLINFO*,FPDF_PAGE) {return 0;}
  static void Invalidate(FPDF_FORMFILLINFO*,FPDF_PAGE,double,double,double,double) {}
  FPDF_PAGE page;
};

class FormOwner {
 public:
  FormOwner(const Api& api,FPDF_DOCUMENT document)
      :api(api),page(api.FPDF_LoadPage(document,0)),info(page),
       handle(api.FPDFDOC_InitFormFillEnvironment(document,&info)) {
    EXPECT_NE(page,nullptr);EXPECT_NE(handle,nullptr);
    if(page&&handle) api.FORM_OnAfterLoadPage(page,handle);
  }
  ~FormOwner() {
    if(page&&handle) api.FORM_OnBeforeClosePage(page,handle);
    if(page) api.FPDF_ClosePage(page);
    if(handle) api.FPDFDOC_ExitFormFillEnvironment(handle);
  }
  FormOwner(const FormOwner&)=delete;FormOwner& operator=(const FormOwner&)=delete;
  const Api& api;FPDF_PAGE page;FormInfo info;FPDF_FORMHANDLE handle;
};

Raster DrawForm(const Api& api,const FormOwner& form) {
  Handle bitmap(api.FPDFBitmap_Create(kWidth,kHeight,1),api.FPDFBitmap_Destroy);
  EXPECT_NE(bitmap.get(),nullptr);if(!bitmap.get()) return {};
  EXPECT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(),0,0,kWidth,kHeight,0xffffffff));
  api.FPDF_RenderPageBitmap(bitmap.get(),form.page,0,0,kWidth,kHeight,0,FPDF_ANNOT | FPDF_NO_NATIVETEXT);
  api.FPDF_FFLDraw(form.handle,bitmap.get(),form.page,0,0,kWidth,kHeight,0,FPDF_ANNOT | FPDF_NO_NATIVETEXT);
  const auto* data=static_cast<const uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap.get()));
  const int stride=api.FPDFBitmap_GetStride(bitmap.get());
  EXPECT_NE(data,nullptr);if(!data) return {};
  Raster result{kWidth,kHeight,std::vector<uint8_t>(kWidth*kHeight*4)};
  for(int row=0;row<kHeight;++row)
    std::copy_n(data+row*stride,kWidth*4,result.pixels.begin()+row*kWidth*4);
  return result;
}

struct ExpectedWidget {
  std::u16string name,alternate,value;
  int type=FPDF_FORMFIELD_TEXTFIELD,flags=0;
  std::vector<std::u16string> labels;
  std::vector<bool> selected;
  int option_count=-1,control_count=1,control_index=0;
  bool checked=false;
  std::u16string export_value;
  bool has_export=false;
  float font_size=12;
  bool registered=true;
};

struct WidgetFixture {
  std::vector<uint8_t> bytes;
  std::vector<ExpectedWidget> expected;
};

// The public API cannot construct AcroForm field trees. Every widget has an
// explicit path-only AP so baseline rendering is independent of system fonts;
// actual interactive drawing is still exercised through FPDF_FFLDraw.
WidgetFixture MakeWidgets() {
  std::vector<std::string> objects={"", "<< /Type /Pages /Count 1 /Kids [3 0 R] >>", "", "",
      "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
      PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 64 24]","0.9 0.95 1 rg 0 0 64 24 re f 0 0 1 RG 1 w 0.5 0.5 63 23 re S"),
      PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 24 24]","1 1 1 rg 0 0 24 24 re f 0 0 0 RG 1 w 1 1 22 22 re S"),
      PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 24 24]","0 0 1 rg 0 0 24 24 re f")};
  const auto add=[&](const std::string& object) {objects.push_back(object);return static_cast<int>(objects.size());};
  std::vector<int> annotations,fields;
  WidgetFixture fixture;
  auto widget=[&](std::string entries,ExpectedWidget expected,int parent=0,const std::string& ap="/AP << /N 6 0 R >>") {
    const size_t index=annotations.size();const int x=16+static_cast<int>(index%5)*88;
    const int y=24+static_cast<int>(index/5)*72;
    const auto rect="/Rect ["+std::to_string(x)+" "+std::to_string(y)+" "+std::to_string(x+64)+" "+std::to_string(y+24)+"]";
    const auto parent_entry=parent?" /Parent "+std::to_string(parent)+" 0 R":"";
    const std::string appearance=parent?"":" /DA (/Helv 12 Tf 0 0 1 rg) ";
    const int number=add("<< /Type /Annot /Subtype /Widget /P 3 0 R "+rect+" /F 4 "+appearance+ap+parent_entry+" "+entries+" >>");
    annotations.push_back(number);if(!parent) fields.push_back(number);fixture.expected.push_back(std::move(expected));return number;
  };
  ExpectedWidget simple;simple.name=u"simple";simple.alternate=u"Alternate text";simple.value=u"Hello";
  widget("/FT /Tx /T (simple) /TU (Alternate text) /V (Hello) /AA << /K << /S /JavaScript /JS (event.value='K';) >> /F << /S /JavaScript /JS (event.value='F';) >> /V << /S /JavaScript /JS (event.value='V';) >> /C << /S /JavaScript /JS (event.value='C';) >> >>",simple);
  ExpectedWidget unicode;unicode.name=u"\u6771\u4eac";unicode.alternate=u"\u5225\u540d";unicode.value=u"\u5024\U0001f642";
  widget("/FT /Tx /T <FEFF67714EAC> /TU <FEFF5225540D> /V <FEFF5024D83DDE42>",unicode);
  // A top-level empty fully qualified name is not registered by PDFium's
  // field tree. Exercise its defined query errors, not a fabricated live field.
  ExpectedWidget empty;empty.font_size=0;empty.registered=false;
  widget("/FT /Tx /T () /TU () /V () /DA (/Helv 0 Tf 0 0 1 rg)",empty);
  const int inherited=add("<< /FT /Tx /T (root) /Ff 7 /V (ParentValue) /DA (/Helv 0 Tf 0 0 1 rg) >>");fields.push_back(inherited);
  ExpectedWidget child;child.name=u"root.child";child.value=u"ParentValue";child.flags=7;child.font_size=0;
  const int child_widget=widget("/T (child)",child,inherited);
  objects[inherited-1]="<< /FT /Tx /T (root) /Ff 7 /V (ParentValue) /DA (/Helv 0 Tf 0 0 1 rg) /Kids ["+std::to_string(child_widget)+" 0 R] >>";
  ExpectedWidget check;check.name=u"check";check.value=u"checked-export";check.type=FPDF_FORMFIELD_CHECKBOX;check.checked=true;check.option_count=1;check.labels={u"checked-export"};check.selected={false};check.has_export=true;check.export_value=u"checked-export";
  widget("/FT /Btn /T (check) /V /Yes /AS /Yes /Opt [(checked-export)]",check,0,"/AP << /N << /Off 7 0 R /Yes 8 0 R >> >>");
  const int radio=add("<< /FT /Btn /T (radio) /Ff 32768 /V /Two /Opt [(One-export) (Two-export)] >>");fields.push_back(radio);
  ExpectedWidget one;one.name=u"radio";one.value=u"Two-export";one.type=FPDF_FORMFIELD_RADIOBUTTON;one.flags=32768;one.control_count=2;one.option_count=2;one.labels={u"One-export",u"Two-export"};one.selected={false,false};one.has_export=true;one.export_value=u"One-export";
  const int radio_one=widget("/AS /Off",one,radio,"/AP << /N << /Off 7 0 R /One 8 0 R >> >>");
  ExpectedWidget two=one;two.checked=true;two.control_index=1;two.export_value=u"Two-export";
  const int radio_two=widget("/AS /Two",two,radio,"/AP << /N << /Off 7 0 R /Two 8 0 R >> >>");
  objects[radio-1]="<< /FT /Btn /T (radio) /Ff 32768 /V /Two /Opt [(One-export) (Two-export)] /Kids ["+std::to_string(radio_one)+" 0 R "+std::to_string(radio_two)+" 0 R] >>";
  ExpectedWidget push;push.name=u"push";push.type=FPDF_FORMFIELD_PUSHBUTTON;push.flags=65536;
  widget("/FT /Btn /T (push) /Ff 65536 /H /P",push,0,"/AP << /N 6 0 R /R 7 0 R /D 8 0 R >>");
  ExpectedWidget list;list.name=u"list";list.type=FPDF_FORMFIELD_LISTBOX;list.flags=FPDF_FORMFLAG_CHOICE_MULTI_SELECT;list.option_count=4;
  list.labels={u"A",u"Display B",u"\u6771\u4eac",u""};list.selected={false,true,true,false};list.value=u"export-b";
  widget("/FT /Ch /T (list) /Ff 2097152 /Opt [(A) [(export-b) (Display B)] <FEFF67714EAC> ()] /I [1 2] /V [(export-b) <FEFF67714EAC>]",list);
  ExpectedWidget combo=list;combo.name=u"combo";combo.type=FPDF_FORMFIELD_COMBOBOX;combo.flags=FPDF_FORMFLAG_CHOICE_COMBO;combo.selected={false,true,false,false};
  widget("/FT /Ch /T (combo) /Ff 131072 /Opt [(A) [(export-b) (Display B)] <FEFF67714EAC> ()] /I [1] /V (export-b)",combo);
  ExpectedWidget empty_list;empty_list.name=u"empty-list";empty_list.type=FPDF_FORMFIELD_LISTBOX;empty_list.option_count=0;
  widget("/FT /Ch /T (empty-list) /Opt []",empty_list);
  ExpectedWidget no_options;no_options.name=u"no-options";no_options.type=FPDF_FORMFIELD_LISTBOX;no_options.option_count=0;
  widget("/FT /Ch /T (no-options)",no_options);
  ExpectedWidget signature;signature.name=u"signature";signature.type=FPDF_FORMFIELD_SIGNATURE;
  widget("/FT /Sig /T (signature)",signature);
  ExpectedWidget unicode_check=check;unicode_check.name=u"unicode-check";unicode_check.value=u"\u6771\u4eac";unicode_check.export_value=u"\u6771\u4eac";unicode_check.labels={u"\u6771\u4eac"};
  widget("/FT /Btn /T (unicode-check) /V /On /AS /On /Opt [<FEFF67714EAC>]",unicode_check,0,"/AP << /N << /Off 7 0 R /On 8 0 R >> >>");
  ExpectedWidget single=list;single.name=u"single-list";single.flags=0;single.value=u"A";single.selected={true,false,false,false};
  widget("/FT /Ch /T (single-list) /Opt [(A) [(export-b) (Display B)] <FEFF67714EAC> ()] /I [0] /V (A)",single);
  const int free_text=add("<< /Type /Annot /Subtype /FreeText /Rect [280 240 440 280] /DA (/Helv 10 Tf 1 0 0 rg) /Contents (Color) /AP << /N 6 0 R >> >>");annotations.push_back(free_text);
  const auto references=[](const std::vector<int>& values) {std::string result;for(int number:values) result+=std::to_string(number)+" 0 R ";return result;};
  objects[0]="<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>";
  objects[2]="<< /Type /Page /Parent 2 0 R /MediaBox [0 0 480 360] /Resources << /Font << /Helv 5 0 R >> >> /Annots ["+references(annotations)+"] >>";
  objects[3]="<< /Fields ["+references(fields)+"] /NeedAppearances false /DA (/Helv 12 Tf 0 0 1 rg) /DR << /Font << /Helv 5 0 R >> >> >>";
  fixture.bytes=PdfFixture(objects);return fixture;
}

void InspectWidgets(const Api& api,const FormOwner& form,const std::vector<ExpectedWidget>& expected) {
  ASSERT_EQ(api.FPDFPage_GetAnnotCount(form.page),static_cast<int>(expected.size()+1));
  for(size_t index=0;index<expected.size();++index) {
    SCOPED_TRACE(index);const auto& e=expected[index];
    Handle a(api.FPDFPage_GetAnnot(form.page,static_cast<int>(index)),api.FPDFPage_CloseAnnot);
    ASSERT_NE(a.get(),nullptr);EXPECT_EQ(api.FPDFAnnot_GetSubtype(a.get()),FPDF_ANNOT_WIDGET);
    if(!e.registered) {
      EXPECT_EQ(api.FPDFAnnot_GetFormFieldType(form.handle,a.get()),-1);
      for(auto getter:{api.FPDFAnnot_GetFormFieldName,api.FPDFAnnot_GetFormFieldAlternateName,api.FPDFAnnot_GetFormFieldValue}) {
        std::array<FPDF_WCHAR,4> output;output.fill(0x5a5a);
        EXPECT_EQ(getter(form.handle,a.get(),output.data(),8),0ul);EXPECT_EQ(output[0],0x5a5a);
      }
      continue;
    }
    EXPECT_EQ(api.FPDFAnnot_GetFormFieldType(form.handle,a.get()),e.type);
    EXPECT_EQ(api.FPDFAnnot_GetFormFieldFlags(form.handle,a.get()),e.flags);
    ExpectText([&](FPDF_WCHAR* b,unsigned long n){return api.FPDFAnnot_GetFormFieldName(form.handle,a.get(),b,n);},e.name);
    ExpectText([&](FPDF_WCHAR* b,unsigned long n){return api.FPDFAnnot_GetFormFieldAlternateName(form.handle,a.get(),b,n);},e.alternate);
    ExpectText([&](FPDF_WCHAR* b,unsigned long n){return api.FPDFAnnot_GetFormFieldValue(form.handle,a.get(),b,n);},e.value);
    EXPECT_EQ(api.FPDFAnnot_GetOptionCount(form.handle,a.get()),e.option_count);
    for(int option=0;option<e.option_count;++option) {
      ExpectText([&](FPDF_WCHAR* b,unsigned long n){return api.FPDFAnnot_GetOptionLabel(form.handle,a.get(),option,b,n);},e.labels[option]);
      EXPECT_EQ(!!api.FPDFAnnot_IsOptionSelected(form.handle,a.get(),option),e.selected[option]);
    }
    for(int invalid:{-1,e.option_count,999}) {
      std::array<FPDF_WCHAR,8> output;output.fill(0x5a5a);
      EXPECT_EQ(api.FPDFAnnot_GetOptionLabel(form.handle,a.get(),invalid,output.data(),16),0ul);
      EXPECT_EQ(output[0],0x5a5a);EXPECT_FALSE(api.FPDFAnnot_IsOptionSelected(form.handle,a.get(),invalid));
    }
    EXPECT_EQ(!!api.FPDFAnnot_IsChecked(form.handle,a.get()),e.checked);
    if(e.has_export) {
      EXPECT_EQ(api.FPDFAnnot_GetFormControlCount(form.handle,a.get()),e.control_count);
      EXPECT_EQ(api.FPDFAnnot_GetFormControlIndex(form.handle,a.get()),e.control_index);
      ExpectText([&](FPDF_WCHAR* b,unsigned long n){return api.FPDFAnnot_GetFormFieldExportValue(form.handle,a.get(),b,n);},e.export_value);
    }
    if(e.type==FPDF_FORMFIELD_TEXTFIELD || e.type==FPDF_FORMFIELD_LISTBOX || e.type==FPDF_FORMFIELD_COMBOBOX) {
      float size=-1;ASSERT_TRUE(api.FPDFAnnot_GetFontSize(form.handle,a.get(),&size));EXPECT_FLOAT_EQ(size,e.font_size);
      EXPECT_FALSE(api.FPDFAnnot_GetFontSize(form.handle,a.get(),nullptr));
      unsigned int red=91,green=92,blue=93;ASSERT_TRUE(api.FPDFAnnot_GetFontColor(form.handle,a.get(),&red,&green,&blue));
      EXPECT_EQ(red,0u);EXPECT_EQ(green,0u);EXPECT_EQ(blue,255u);
    }
    FS_RECTF rect{};ASSERT_TRUE(api.FPDFAnnot_GetRect(a.get(),&rect));
    EXPECT_FLOAT_EQ(rect.left,16+static_cast<int>(index%5)*88);
    EXPECT_FLOAT_EQ(rect.bottom,24+static_cast<int>(index/5)*72);
    EXPECT_FLOAT_EQ(rect.right,rect.left+64);EXPECT_FLOAT_EQ(rect.top,rect.bottom+24);
    const FS_POINTF point{(rect.left+rect.right)/2,(rect.bottom+rect.top)/2};
    Handle hit(api.FPDFAnnot_GetFormFieldAtPoint(form.handle,form.page,&point),api.FPDFPage_CloseAnnot);
    ASSERT_NE(hit.get(),nullptr);
    ExpectText([&](FPDF_WCHAR* b,unsigned long n){return api.FPDFAnnot_GetFormFieldName(form.handle,hit.get(),b,n);},e.name);
  }
  const FS_POINTF outside{470,350};EXPECT_EQ(api.FPDFAnnot_GetFormFieldAtPoint(form.handle,form.page,&outside),nullptr);
  Handle unrelated(api.FPDFPage_GetAnnot(form.page,static_cast<int>(expected.size())),api.FPDFPage_CloseAnnot);
  EXPECT_EQ(api.FPDFAnnot_GetFormFieldType(form.handle,unrelated.get()),-1);
  EXPECT_EQ(api.FPDFAnnot_GetFormControlCount(form.handle,unrelated.get()),-1);
  EXPECT_EQ(api.FPDFAnnot_GetFormControlIndex(form.handle,unrelated.get()),-1);
  EXPECT_FALSE(api.FPDFAnnot_IsChecked(form.handle,unrelated.get()));
  EXPECT_FALSE(api.FPDFAnnot_SetFormFieldFlags(form.handle,unrelated.get(),7));
  float size=91;EXPECT_FALSE(api.FPDFAnnot_GetFontSize(form.handle,unrelated.get(),&size));
  for(auto getter:{api.FPDFAnnot_GetFormFieldName,api.FPDFAnnot_GetFormFieldAlternateName,
                   api.FPDFAnnot_GetFormFieldValue,api.FPDFAnnot_GetFormFieldExportValue}) {
    std::array<FPDF_WCHAR,8> output;output.fill(0x5a5a);
    EXPECT_EQ(getter(form.handle,unrelated.get(),output.data(),16),0ul);EXPECT_EQ(output[0],0x5a5a);
  }
}

void CompareEditedWidgets(const Raster& expected,const Raster& actual,Artifacts& artifacts,
                          const std::string& name) {
  // Measured native/Rust differences after editing are confined to glyph edges:
  // text 258 pixels/max11, list164/max13, combo23/max3. Check each control with
  // a bounded mean, and require exact output everywhere outside these controls.
  struct Region {int x,y,width,height;RasterTolerance tolerance;};
  const std::array<Region,3> regions={{{104,312,64,24,{12,0,0.25}},
      {280,240,64,24,{14,0,0.15}},{368,240,64,24,{4,0,0.02}}}};
  auto expected_rest=expected,actual_rest=actual;
  for(size_t index=0;index<regions.size();++index) {
    const auto& box=regions[index];
    CompareRaster(CropRaster(expected,box.x,box.y,box.width,box.height),
        CropRaster(actual,box.x,box.y,box.width,box.height),artifacts,
        name+"-control-"+std::to_string(index),box.tolerance);
    for(int y=box.y;y<box.y+box.height;++y) for(int x=box.x;x<box.x+box.width;++x) {
      const auto offset=(static_cast<size_t>(y)*expected.width+x)*4;
      std::fill_n(expected_rest.pixels.begin()+offset,4,255);
      std::fill_n(actual_rest.pixels.begin()+offset,4,255);
    }
  }
  CompareRaster(expected_rest,actual_rest,artifacts,name+"-outside-edited-controls");
}

void RoundTripWidgets(const Library& writer,FPDF_DOCUMENT document,FormOwner& original,
                      Artifacts& artifacts,const std::string& name,
                      const std::function<void(const Api&,const FormOwner&)>& inspect,
                      RasterTolerance tolerance = {},bool edited_text=false) {
  ASSERT_NO_FATAL_FAILURE(inspect(writer.api(),original));
  const auto before=DrawForm(writer.api(),original);
  const auto bytes=SaveDocument(writer.api(),document,artifacts,name+"-"+writer.name());
  ASSERT_FALSE(bytes.empty());std::array<Raster,2> after;size_t index=0;
  for(const auto* reader:Implementations()) {
    SCOPED_TRACE(writer.name()+"->"+reader->name());const auto& api=reader->api();
    Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    FormOwner form(api,doc.get());ASSERT_NO_FATAL_FAILURE(inspect(api,form));
    after[index]=DrawForm(api,form);
    const auto comparison=name+"-"+writer.name()+"-"+reader->name();
    if(edited_text) CompareEditedWidgets(before,after[index],artifacts,comparison);
    else CompareRaster(before,after[index],artifacts,comparison,tolerance);
    ++index;
  }
  if(edited_text) CompareEditedWidgets(after[0],after[1],artifacts,name+"-readers-"+writer.name());
  else CompareRaster(after[0],after[1],artifacts,name+"-readers-"+writer.name(),tolerance);
}

TEST(AnnotationWidgets, AllFieldKindsNamesValuesOptionsInheritedFlagsAndHitTesting) {
  Artifacts artifacts;const auto fixture=MakeWidgets();std::array<Raster,2> scenes;size_t index=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());const auto& api=writer->api();
    Handle document(api.FPDF_LoadMemDocument64(fixture.bytes.data(),fixture.bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
    FormOwner form(api,document.get());ASSERT_NO_FATAL_FAILURE(InspectWidgets(api,form,fixture.expected));
    scenes[index++]=DrawForm(api,form);
    ASSERT_NO_FATAL_FAILURE(RoundTripWidgets(*writer,document.get(),form,artifacts,"widget-queries",
        [&](const Api& read,const FormOwner& loaded){InspectWidgets(read,loaded,fixture.expected);}));
  }
  CompareRaster(scenes[0],scenes[1],artifacts,"widget-query-backends");
}

TEST(AnnotationWidgets, AdditionalActionSourcesUseCompleteUtf16ByteBuffersWithoutExecution) {
  Artifacts artifacts;const auto fixture=MakeWidgets();
  for(const auto* writer:Implementations()) {
    const auto& api=writer->api();
    Handle document(api.FPDF_LoadMemDocument64(fixture.bytes.data(),fixture.bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
    FormOwner form(api,document.get());
    const auto inspect=[](const Api& read,const FormOwner& loaded) {
      Handle with_actions(read.FPDFPage_GetAnnot(loaded.page,0),read.FPDFPage_CloseAnnot);
      Handle absent(read.FPDFPage_GetAnnot(loaded.page,1),read.FPDFPage_CloseAnnot);
      const std::array<std::u16string,4> scripts={u"event.value='K';",u"event.value='F';",u"event.value='V';",u"event.value='C';"};
      for(int event=12;event<=15;++event) {
        ExpectText([&](FPDF_WCHAR* b,unsigned long n){return read.FPDFAnnot_GetFormAdditionalActionJavaScript(loaded.handle,with_actions.get(),event,b,n);},scripts[event-12]);
        ExpectText([&](FPDF_WCHAR* b,unsigned long n){return read.FPDFAnnot_GetFormAdditionalActionJavaScript(loaded.handle,absent.get(),event,b,n);},u"");
      }
      for(int invalid:{-1,11,16,999}) {
        std::array<FPDF_WCHAR,8> output;output.fill(0x5a5a);
        EXPECT_EQ(read.FPDFAnnot_GetFormAdditionalActionJavaScript(loaded.handle,with_actions.get(),invalid,output.data(),16),0ul);EXPECT_EQ(output[0],0x5a5a);
      }
      ExpectText([&](FPDF_WCHAR* b,unsigned long n){return read.FPDFAnnot_GetFormFieldValue(loaded.handle,with_actions.get(),b,n);},u"Hello");
    };
    ASSERT_NO_FATAL_FAILURE(RoundTripWidgets(*writer,document.get(),form,artifacts,"widget-actions",inspect));
  }
}

TEST(AnnotationWidgets, FieldFlagReplacementKeepsAnnotationFlagsAndPersistsAllKinds) {
  Artifacts artifacts;const auto fixture=MakeWidgets();
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());const auto& api=writer->api();
    Handle document(api.FPDF_LoadMemDocument64(fixture.bytes.data(),fixture.bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
    FormOwner form(api,document.get());
    for(size_t index=0;index<fixture.expected.size();++index) {
      if(!fixture.expected[index].registered) continue;
      Handle a(api.FPDFPage_GetAnnot(form.page,static_cast<int>(index)),api.FPDFPage_CloseAnnot);
      const int annotation_flags=api.FPDFAnnot_GetFlags(a.get());
      const int original=fixture.expected[index].flags;
      for(int common:{FPDF_FORMFLAG_READONLY,FPDF_FORMFLAG_REQUIRED,FPDF_FORMFLAG_NOEXPORT,0}) {
        ASSERT_TRUE(api.FPDFAnnot_SetFormFieldFlags(form.handle,a.get(),original|common));
        EXPECT_EQ(api.FPDFAnnot_GetFormFieldFlags(form.handle,a.get()),original|common);
        EXPECT_EQ(api.FPDFAnnot_GetFlags(a.get()),annotation_flags);
      }
      int specialized=original;
      if(fixture.expected[index].type==FPDF_FORMFIELD_TEXTFIELD) specialized|=FPDF_FORMFLAG_TEXT_MULTILINE|FPDF_FORMFLAG_TEXT_PASSWORD;
      if(fixture.expected[index].type==FPDF_FORMFIELD_COMBOBOX) specialized|=FPDF_FORMFLAG_CHOICE_EDIT;
      ASSERT_TRUE(api.FPDFAnnot_SetFormFieldFlags(form.handle,a.get(),specialized));
      EXPECT_EQ(api.FPDFAnnot_GetFormFieldFlags(form.handle,a.get()),specialized);
      ASSERT_TRUE(api.FPDFAnnot_SetFormFieldFlags(form.handle,a.get(),original));
    }
    auto expected=fixture.expected;
    // Persist changes rather than merely toggling back before the save.
    Handle text(api.FPDFPage_GetAnnot(form.page,0),api.FPDFPage_CloseAnnot);
    expected[0].flags=FPDF_FORMFLAG_READONLY|FPDF_FORMFLAG_REQUIRED|FPDF_FORMFLAG_NOEXPORT;
    ASSERT_TRUE(api.FPDFAnnot_SetFormFieldFlags(form.handle,text.get(),expected[0].flags));
    ASSERT_TRUE(api.FORM_SetFocusedAnnot(form.handle,text.get()));
    api.FORM_SelectAllText(form.handle,form.page);
    // Readonly controls reject user input. Programmatic replacement APIs do
    // not promise to enforce that user-interaction restriction.
    api.FORM_OnChar(form.handle,form.page,static_cast<unsigned int>('Z'),0);
    ASSERT_TRUE(api.FORM_ForceToKillFocus(form.handle));
    ExpectText([&](FPDF_WCHAR* b,unsigned long n){return api.FPDFAnnot_GetFormFieldValue(form.handle,text.get(),b,n);},expected[0].value);
    Handle radio(api.FPDFPage_GetAnnot(form.page,5),api.FPDFPage_CloseAnnot);
    expected[5].flags|=FPDF_FORMFLAG_NOEXPORT;expected[6].flags=expected[5].flags;
    ASSERT_TRUE(api.FPDFAnnot_SetFormFieldFlags(form.handle,radio.get(),expected[5].flags));
    Handle other_control(api.FPDFPage_GetAnnot(form.page,6),api.FPDFPage_CloseAnnot);
    EXPECT_EQ(api.FPDFAnnot_GetFormFieldFlags(form.handle,other_control.get()),expected[6].flags);
    ASSERT_NO_FATAL_FAILURE(RoundTripWidgets(*writer,document.get(),form,artifacts,"widget-flags",
        [&](const Api& read,const FormOwner& loaded){InspectWidgets(read,loaded,expected);}));
  }
}

TEST(AnnotationWidgets, FreeTextColorChangesRegenerateVisibleAppearanceAndPersist) {
  Artifacts artifacts;const auto fixture=MakeWidgets();std::array<Raster,2> scenes;size_t index=0;
  for(const auto* writer:Implementations()) {
    const auto& api=writer->api();
    Handle document(api.FPDF_LoadMemDocument64(fixture.bytes.data(),fixture.bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
    FormOwner form(api,document.get());
    Handle a(api.FPDFPage_GetAnnot(form.page,static_cast<int>(fixture.expected.size())),api.FPDFPage_CloseAnnot);
    Handle unsupported(api.FPDFPage_GetAnnot(form.page,0),api.FPDFPage_CloseAnnot);
    EXPECT_FALSE(api.FPDFAnnot_SetFontColor(form.handle,unsupported.get(),1,2,3));
    const auto before=DrawForm(api,form);
    for(const auto& color:{std::array<unsigned int,3>{0,0,0},{255,255,255},{17,129,254},{32,96,224}}) {
      ASSERT_TRUE(api.FPDFAnnot_SetFontColor(form.handle,a.get(),color[0],color[1],color[2]));
      unsigned int r=91,g=92,b=93;ASSERT_TRUE(api.FPDFAnnot_GetFontColor(form.handle,a.get(),&r,&g,&b));
      EXPECT_NEAR(r,color[0],1);EXPECT_NEAR(g,color[1],1);EXPECT_NEAR(b,color[2],1);
    }
    unsigned int output=91;
    EXPECT_FALSE(api.FPDFAnnot_GetFontColor(form.handle,a.get(),nullptr,&output,&output));
    EXPECT_FALSE(api.FPDFAnnot_GetFontColor(form.handle,a.get(),&output,nullptr,&output));
    EXPECT_FALSE(api.FPDFAnnot_GetFontColor(form.handle,a.get(),&output,&output,nullptr));
    scenes[index++]=DrawForm(api,form);
    EXPECT_NE(scenes[index-1].pixels,before.pixels);
    ASSERT_NO_FATAL_FAILURE(RoundTripWidgets(*writer,document.get(),form,artifacts,"free-text-color",
        [&](const Api& read,const FormOwner& loaded) {
          Handle free_text(read.FPDFPage_GetAnnot(loaded.page,static_cast<int>(fixture.expected.size())),read.FPDFPage_CloseAnnot);
          unsigned int r=0,g=0,b=0;ASSERT_TRUE(read.FPDFAnnot_GetFontColor(loaded.handle,free_text.get(),&r,&g,&b));
          EXPECT_NEAR(r,32,1);EXPECT_NEAR(g,96,1);EXPECT_NEAR(b,224,1);
        },RasterTolerance{12,0,0.002}));
  }
  // Generated free-text uses the same Helvetica outlines. Measured differences
  // are glyph edge antialiasing only (99 pixels, max10, mean0.000748), confined
  // to x281..304/y96..104; no missing text or changed geometry is tolerated.
  CompareRaster(scenes[0],scenes[1],artifacts,"free-text-color-backends",RasterTolerance{12,0,0.002});
}

TEST(AnnotationWidgets, FocusableSubtypeListsReplaceAndStayEnvironmentLocal) {
  Artifacts artifacts;const auto fixture=MakeWidgets();
  for(const auto* writer:Implementations()) {
    const auto& api=writer->api();
    Handle first(api.FPDF_LoadMemDocument64(fixture.bytes.data(),fixture.bytes.size(),nullptr),api.FPDF_CloseDocument);
    Handle second(api.FPDF_LoadMemDocument64(fixture.bytes.data(),fixture.bytes.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(first.get(),nullptr);ASSERT_NE(second.get(),nullptr);
    FormOwner one(api,first.get()),two(api,second.get());
    const auto check=[&](FPDF_FORMHANDLE handle,const std::vector<FPDF_ANNOTATION_SUBTYPE>& expected) {
      ASSERT_EQ(api.FPDFAnnot_GetFocusableSubtypesCount(handle),static_cast<int>(expected.size()));
      for(size_t extra:{size_t{0},size_t{3}}) {
        std::vector<FPDF_ANNOTATION_SUBTYPE> output(expected.size()+extra+1,999);
        ASSERT_TRUE(api.FPDFAnnot_GetFocusableSubtypes(handle,output.data(),expected.size()+extra));
        for(size_t i=0;i<expected.size();++i) EXPECT_EQ(output[i],expected[i]);
        for(size_t i=expected.size();i<output.size();++i) EXPECT_EQ(output[i],999);
      }
    };
    ASSERT_NO_FATAL_FAILURE(check(one.handle,{FPDF_ANNOT_WIDGET}));
    ASSERT_NO_FATAL_FAILURE(check(two.handle,{FPDF_ANNOT_WIDGET}));
    const std::vector<FPDF_ANNOTATION_SUBTYPE> types={FPDF_ANNOT_WIDGET,FPDF_ANNOT_LINK,FPDF_ANNOT_TEXT};
    ASSERT_TRUE(api.FPDFAnnot_SetFocusableSubtypes(one.handle,types.data(),types.size()));
    ASSERT_NO_FATAL_FAILURE(check(one.handle,types));
    ASSERT_NO_FATAL_FAILURE(check(two.handle,{FPDF_ANNOT_WIDGET}));
    const FPDF_ANNOTATION_SUBTYPE single=FPDF_ANNOT_WIDGET;
    ASSERT_TRUE(api.FPDFAnnot_SetFocusableSubtypes(one.handle,&single,1));
    ASSERT_NO_FATAL_FAILURE(check(one.handle,{FPDF_ANNOT_WIDGET}));
    Handle annotation(api.FPDFPage_GetAnnot(one.page,0),api.FPDFPage_CloseAnnot);
    ASSERT_TRUE(api.FORM_SetFocusedAnnot(one.handle,annotation.get()));
    ASSERT_TRUE(api.FORM_ForceToKillFocus(one.handle));
    ASSERT_TRUE(api.FPDFAnnot_SetFocusableSubtypes(one.handle,nullptr,0));
    ASSERT_NO_FATAL_FAILURE(check(one.handle,{}));
    EXPECT_FALSE(api.FORM_SetFocusedAnnot(one.handle,annotation.get()));
    api.FORM_OnKeyDown(one.handle,one.page,FWL_VKEY_Tab,0);
    int focused_page=91;FPDF_ANNOTATION focused=nullptr;
    ASSERT_TRUE(api.FORM_GetFocusedAnnot(one.handle,&focused_page,&focused));
    EXPECT_EQ(focused_page,-1);EXPECT_EQ(focused,nullptr);
    if(focused) api.FPDFPage_CloseAnnot(focused);
    EXPECT_EQ(api.FPDFAnnot_GetFocusableSubtypesCount(nullptr),-1);
    EXPECT_FALSE(api.FPDFAnnot_SetFocusableSubtypes(nullptr,&single,1));
    std::array<FPDF_ANNOTATION_SUBTYPE,1> output{};
    EXPECT_FALSE(api.FPDFAnnot_GetFocusableSubtypes(nullptr,output.data(),output.size()));
    const auto raster=DrawForm(api,one);artifacts.Image("focus-empty-"+writer->name(),raster);
  }
}

TEST(AnnotationWidgets, RealTextChoiceAndButtonInputUpdatesQueriesAppearanceAndSavedState) {
  Artifacts artifacts;const auto fixture=MakeWidgets();std::array<Raster,2> scenes;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());const auto& api=writer->api();
    Handle document(api.FPDF_LoadMemDocument64(fixture.bytes.data(),fixture.bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
    FormOwner form(api,document.get());auto expected=fixture.expected;
    const auto before=DrawForm(api,form);
    {
      Handle text(api.FPDFPage_GetAnnot(form.page,1),api.FPDFPage_CloseAnnot);
      ASSERT_TRUE(api.FORM_SetFocusedAnnot(form.handle,text.get()));
      ASSERT_TRUE(api.FORM_SelectAllText(form.handle,form.page));
      api.FORM_ReplaceSelection(form.handle,form.page,Wide(u"Changed"));
      ASSERT_TRUE(api.FORM_ForceToKillFocus(form.handle));expected[1].value=u"Changed";
    }
    {
      Handle list(api.FPDFPage_GetAnnot(form.page,8),api.FPDFPage_CloseAnnot);
      ASSERT_TRUE(api.FORM_SetFocusedAnnot(form.handle,list.get()));
      ASSERT_TRUE(api.FORM_SetIndexSelected(form.handle,form.page,0,1));
      ASSERT_TRUE(api.FORM_SetIndexSelected(form.handle,form.page,2,0));
      ASSERT_TRUE(api.FORM_ForceToKillFocus(form.handle));
      expected[8].selected={true,true,false,false};expected[8].value=u"A";
    }
    {
      Handle combo(api.FPDFPage_GetAnnot(form.page,9),api.FPDFPage_CloseAnnot);
      ASSERT_TRUE(api.FORM_SetFocusedAnnot(form.handle,combo.get()));
      ASSERT_TRUE(api.FORM_SetIndexSelected(form.handle,form.page,0,1));
      ASSERT_TRUE(api.FORM_ForceToKillFocus(form.handle));
      expected[9].selected={true,false,false,false};expected[9].value=u"A";
    }
    // Controls are separated; each click targets its unambiguous interior.
    ASSERT_TRUE(api.FORM_OnMouseMove(form.handle,form.page,0,400,36));
    ASSERT_TRUE(api.FORM_OnLButtonDown(form.handle,form.page,0,400,36));
    ASSERT_TRUE(api.FORM_OnLButtonUp(form.handle,form.page,0,400,36));
    ASSERT_TRUE(api.FORM_ForceToKillFocus(form.handle));
    expected[4].checked=false;expected[4].value=u"Off";
    ASSERT_TRUE(api.FORM_OnMouseMove(form.handle,form.page,0,48,108));
    ASSERT_TRUE(api.FORM_OnLButtonDown(form.handle,form.page,0,48,108));
    ASSERT_TRUE(api.FORM_OnLButtonUp(form.handle,form.page,0,48,108));
    ASSERT_TRUE(api.FORM_ForceToKillFocus(form.handle));
    expected[5].checked=true;expected[6].checked=false;
    expected[5].value=u"One-export";expected[6].value=u"One-export";
    ASSERT_NO_FATAL_FAILURE(InspectWidgets(api,form,expected));
    scenes[backend]=DrawForm(api,form);
    EXPECT_NE(scenes[backend].pixels,before.pixels);
    artifacts.Image(writer->name()+"-before-interaction",before);
    ASSERT_NO_FATAL_FAILURE(RoundTripWidgets(*writer,document.get(),form,artifacts,"widget-input",
        [&](const Api& read,const FormOwner& loaded){InspectWidgets(read,loaded,expected);},{},true));
    ++backend;
  }
  CompareEditedWidgets(scenes[0],scenes[1],artifacts,"widget-input-backends");
}

TEST(AnnotationWidgets, PushbuttonNormalRolloverAndDownAppearancesAreActuallyDrawn) {
  Artifacts artifacts;const auto fixture=MakeWidgets();
  std::array<std::array<Raster,3>,2> scenes;size_t backend=0;
  for(const auto* writer:Implementations()) {
    const auto& api=writer->api();
    Handle document(api.FPDF_LoadMemDocument64(fixture.bytes.data(),fixture.bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
    FormOwner form(api,document.get());
    scenes[backend][0]=DrawForm(api,form);
    ASSERT_TRUE(api.FORM_OnMouseMove(form.handle,form.page,0,224,108));
    scenes[backend][1]=DrawForm(api,form);
    ASSERT_TRUE(api.FORM_OnLButtonDown(form.handle,form.page,0,224,108));
    scenes[backend][2]=DrawForm(api,form);
    // Compare the actual widget interior independently of neighboring controls
    // and the platform's focus border. Paths use exact opaque color values.
    const auto normal=CropRaster(scenes[backend][0],200,244,48,16);
    const auto rollover=CropRaster(scenes[backend][1],200,244,48,16);
    const auto down=CropRaster(scenes[backend][2],200,244,48,16);
    EXPECT_NE(normal.pixels,rollover.pixels);EXPECT_NE(rollover.pixels,down.pixels);
    ASSERT_TRUE(api.FORM_OnLButtonUp(form.handle,form.page,0,224,108));
    api.FORM_ForceToKillFocus(form.handle);
    // Hover is transient UI state, not part of the PDF. Restore normal before
    // comparing a fresh reader, which has never received a pointer event.
    api.FORM_OnMouseMove(form.handle,form.page,0,470,350);
    ASSERT_NO_FATAL_FAILURE(RoundTripWidgets(*writer,document.get(),form,artifacts,"pushbutton-modes",
        [](const Api& read,const FormOwner& loaded) {
          Handle annot(read.FPDFPage_GetAnnot(loaded.page,7),read.FPDFPage_CloseAnnot);
          const std::array<std::u16string,3> expected={
            u"0.9 0.95 1 rg 0 0 64 24 re f 0 0 1 RG 1 w 0.5 0.5 63 23 re S",
            u"1 1 1 rg 0 0 24 24 re f 0 0 0 RG 1 w 1 1 22 22 re S",
            u"0 0 1 rg 0 0 24 24 re f"};
          for(int mode=0;mode<3;++mode)
            ExpectText([&](FPDF_WCHAR* b,unsigned long n){return read.FPDFAnnot_GetAP(annot.get(),mode,b,n);},expected[mode]);
        }));
    ++backend;
  }
  for(size_t mode=0;mode<3;++mode)
    CompareRaster(scenes[0][mode],scenes[1][mode],artifacts,"pushbutton-mode-"+std::to_string(mode));
}

TEST(AnnotationWidgets, FontColorsReadGrayRgbCmykFallbackAndMissingDefaultAppearance) {
  Artifacts artifacts;
  // These DA forms cannot be assigned through public constructors. Explicit
  // AP streams keep query semantics separate from appearance precedence.
  const auto bytes=PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 480 360] /Annots [5 0 R 6 0 R 7 0 R 8 0 R 9 0 R] >>",
      "<< /Fields [5 0 R 6 0 R 7 0 R 8 0 R] /DA (/Helv 12 Tf 0 1 0 rg) /DR << /Font << /Helv 10 0 R >> >> >>",
      "<< /Type /Annot /Subtype /Widget /FT /Tx /T (gray) /V (gray) /Rect [16 24 80 48] /DA (/Helv 12 Tf 0.5 g) /AP << /N 11 0 R >> >>",
      "<< /Type /Annot /Subtype /Widget /FT /Tx /T (rgb) /V (rgb) /Rect [104 24 168 48] /DA (/Helv 12 Tf 0.1 0.3 0.7 rg) /AP << /N 11 0 R >> >>",
      "<< /Type /Annot /Subtype /Widget /FT /Tx /T (cmyk) /V (cmyk) /Rect [192 24 256 48] /DA (/Helv 12 Tf 0.1 0.2 0.3 0.25 k) /AP << /N 11 0 R >> >>",
      "<< /Type /Annot /Subtype /Widget /FT /Tx /T (fallback) /V (fallback) /Rect [280 24 344 48] /AP << /N 11 0 R >> >>",
      "<< /Type /Annot /Subtype /FreeText /Rect [24 200 200 232] /Contents (missing) /DA (/Helv 10 Tf) /AP << /N 11 0 R >> >>",
      "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
      PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 64 24]","0.125 0.5 0.875 rg 0 0 64 24 re f")});
  const std::array<std::array<unsigned int,3>,4> expected={{{128,128,128},{26,77,179},{166,140,115},{0,255,0}}};
  const auto inspect=[&](const Api& api,const FormOwner& form) {
    for(int index=0;index<5;++index) {
      Handle annotation(api.FPDFPage_GetAnnot(form.page,index),api.FPDFPage_CloseAnnot);
      ASSERT_NE(annotation.get(),nullptr);
      unsigned int red=91,green=92,blue=93;
      if(index==4) {EXPECT_FALSE(api.FPDFAnnot_GetFontColor(form.handle,annotation.get(),&red,&green,&blue));continue;}
      ASSERT_TRUE(api.FPDFAnnot_GetFontColor(form.handle,annotation.get(),&red,&green,&blue));
      EXPECT_NEAR(red,expected[index][0],1);EXPECT_NEAR(green,expected[index][1],1);EXPECT_NEAR(blue,expected[index][2],1);
    }
  };
  std::array<Raster,2> scenes;size_t backend=0;
  for(const auto* writer:Implementations()) {
    const auto& api=writer->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
    FormOwner form(api,document.get());ASSERT_NO_FATAL_FAILURE(inspect(api,form));
    scenes[backend++]=DrawForm(api,form);
    ASSERT_NO_FATAL_FAILURE(RoundTripWidgets(*writer,document.get(),form,artifacts,"font-colors",inspect));
  }
  CompareRaster(scenes[0],scenes[1],artifacts,"font-color-representations");
}

}  // namespace
}  // namespace pdfoxium::integration
