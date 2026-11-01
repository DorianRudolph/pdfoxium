#include "form_fixture.h"

#include <array>
#include <cstdint>
#include <numeric>
#include <string>

namespace pdfoxium::integration {
namespace {
// ISO 32000-1 algorithms 2, 3 and 5, revision 3, independently generated with
// Python hashlib. Empty user password, owner "owner", fixed 16-byte file ID.
// Only the test fixture uses RC4 here to vary scripts/values; production uses
// its existing Rust security handler. No native crypto dependency is added.
struct PermissionVector {
  uint32_t allowed;
  const char* user;
  std::array<const char*,7> object_keys;  // Objects 1,4,5,7,8,9,12.
};
const std::array<PermissionVector,6> kPermissions = {{
    {0x0u, "515bafe72f9954ee06e87d303fea612200000000000000000000000000000000", {"df53b730087c0582d26213ee85b3da98","6bbe644073147d7f8c13d80049dcb168","58d31ded7fb4f619116c2ebfa4a9726b","ac7f3d9135e0956a913242ec01e48079","35921cd1bf35d1f62ecab88e3c0227a5","d44e9ebd7e8c81675a45b4e08a20e1e2","2a1d2876c644db42337b9edf3d046280"}},
    {0x100u, "45043f6dfda7f6ebd516552effb6a99000000000000000000000000000000000", {"d98eac339d0453ee88769ab5280bf45b","a44b295be476201d168dde33cb872bfb","bc1309152ae4b75f08ef79dad0ba4abc","6fd716d0405296d6987cf5e4a3ad45e2","867f06e20a3103ce80a46a16d5f1b953","85c73ae462f3af3d1ae6da8b999d1039","07ffff60d1481a8cfa1265e3940a5d86"}},
    {0x20u, "183f6e172dd081f1323f8eda421ee67800000000000000000000000000000000", {"d183d0cdffc12d715e4b5399fabca04d","d82469363a6dff9a427ef528a07f15ce","d220de27b8f752190e6ffac45cd71bbf","fb12bf04666b1c400ceb81da9a257d12","ea9aa09586685980567cbd059506c801","056068fc48dac4e05b1f59191b46869e","aaac379bd1840ed2e2fe161e3238a0b2"}},
    {0x8u, "86c132829f5f87080c16913f4f5ce39400000000000000000000000000000000", {"79a75288247076e7689b92fc4d85b7d9","437da46e171ed1590301fdc1b4490928","e7f2b32b7c94fa3def3d8336096d6f46","01c1c661487c507a9cd2c94b36f22707","67bd4509fd9ecff6bb2b16b63e5509d6","1302f6f01dfc51de0b807bcb28849275","86a882eb6d534b55b3e1d0beb95c9beb"}},
    {0x200u, "9c831c4225ff9d4c6d6d62f67d4823aa00000000000000000000000000000000", {"26172bec1e270049c6d7a92c71fa0ea3","370d6ad0669d0f189867667db360c248","e10ea9ae0c2984286761427d396cc4ea","d923c7e4208893c3ee73dde1f4032fff","84f26d95b22a8fdcc25c642a00fad651","2725b4be5ac6569643282254411d29b3","6629d24e8aefc4e6cbdda66406999e11"}},
    {0xf3cu, "9380c23a3b495e351a98654d255f739f00000000000000000000000000000000", {"49ea6f3bb102013bbe85f193a73ab619","b830900b0c658717f0912e5342bdb90d","27c7e39d6f0f236355564ef7ee62dcea","bb3b1fd9865ad996cc47c8d96eea7667","c4ad9d8b0fb411bf7d257ffd93f67a9f","e4e4038da068627d04c7dbe31207809b","6a092324fc30dbc740259b60f502fc98"}},
}};
std::string Unhex(const std::string& value) {
  std::string bytes;
  for(size_t i=0;i<value.size();i+=2)
    bytes.push_back(static_cast<char>(std::stoul(value.substr(i,2),nullptr,16)));
  return bytes;
}
std::string Hex(const std::string& bytes) {
  constexpr char digits[]="0123456789abcdef";
  std::string result="<";
  for(unsigned char byte:bytes){result+=digits[byte>>4];result+=digits[byte&15];}
  return result+">";
}
std::string Crypt(const std::string& key,const std::string& input) {
  std::array<uint8_t,256> state;std::iota(state.begin(),state.end(),0);
  unsigned j=0;
  for(unsigned i=0;i<256;++i){j=(j+state[i]+static_cast<uint8_t>(key[i%key.size()]))&255;std::swap(state[i],state[j]);}
  std::string result;unsigned i=0;j=0;
  for(unsigned char byte:input){i=(i+1)&255;j=(j+state[i])&255;std::swap(state[i],state[j]);result+=static_cast<char>(byte^state[(state[i]+state[j])&255]);}
  return result;
}
std::vector<uint8_t> PermissionFixture(const PermissionVector& spec,const std::string& script) {
  const auto encrypted=[&](size_t key,const std::string& text){return Crypt(Unhex(spec.object_keys.at(key)),text);};
  const auto string=[&](size_t key,const std::string& text){return Hex(encrypted(key,text));};
  const auto permission=static_cast<int32_t>(0xfffff0c0u|spec.allowed);
  const auto widget=[&](int key,const char* name,const char* value,const char* rect,const std::string& extra){
    return "<< /Type /Annot /Subtype /Widget /P 3 0 R /F 4 /Rect ["+std::string(rect)+"] /T "+string(key,name)+
        " /V "+string(key,value)+" /BS << /W 1 /S /S >> /MK << /BC [0 0 0] /BG [1 1 1] >> "+extra+" >>";
  };
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /Names << /JavaScript << /Names ["+string(0,"main")+" 10 0 R] >> >> >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 320 180] /Resources << /Font << /F 6 0 R >> >> /Contents 5 0 R /Annots [7 0 R 8 0 R 9 0 R] >>",
      "<< /Fields [7 0 R 8 0 R 9 0 R] /NeedAppearances true /DA "+string(1,"/F 12 Tf 0 g")+" /DR << /Font << /F 6 0 R >> >> >>",
      PdfStream("",encrypted(2,"1 0 0 rg 0 0 16 16 re f BT /F 14 Tf 20 155 Td (Page) Tj ET")),
      "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
      widget(3,"input","before","20 100 140 130","/FT /Tx /DV "+string(3,"default")),
      widget(4,"second","Second","170 100 290 130","/FT /Tx /Ff 1"),
      widget(5,"choice","r","20 50 140 80","/FT /Ch /Ff 131072 /Opt [["+string(5,"r")+" "+string(5,"Red")+"] ["+string(5,"g")+" "+string(5,"Green")+"]]"),
      "<< /S /JavaScript /JS 12 0 R >>",
      "<< /Filter /Standard /V 2 /R 3 /Length 128 /P "+std::to_string(permission)+" /O <566fa873ee33c797cd3b904fdadf814afa34df9a38f6ed41b984e2c6da2aa6f5> /U <"+spec.user+"> >>",
      PdfStream("",encrypted(6,script))},
      "/Encrypt 11 0 R /ID [<636f7665726167652d7065726d732d33> <636f7665726167652d7065726d732d33>]");
}
std::string Checks(const std::string& program,bool set,bool modify,bool extract,bool remove) {
  return "var canSet="+std::string(set?"true":"false")+",canModify="+(modify?"true":"false")+
      ",canExtract="+(extract?"true":"false")+",canRemove="+(remove?"true":"false")+";"
      "function eq(a,b){if(JSON.stringify(a)!==JSON.stringify(b))throw 'expected '+JSON.stringify(b)+' got '+JSON.stringify(a);}"
      "function permit(f,allowed){var succeeded=true,why='';try{f();}catch(e){succeeded=false;why=String(e);}if(succeeded!==allowed)throw f.toString()+' allowed='+allowed+' error='+why;}"
      "try{"+program+";app.alert('PASS');}catch(e){app.alert('FAIL: '+String(e));}";
}
void CheckAlerts(const forms::Session& s) {
  ASSERT_EQ(s.host->alerts.size(),1u);
  EXPECT_EQ(std::get<0>(s.host->alerts[0]),u"PASS");
  EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);
}
void CheckLandmark(const Raster& raster) {
  const size_t at=(static_cast<size_t>(172)*raster.width+8)*4;
  ASSERT_LT(at+3,raster.pixels.size());
  EXPECT_EQ(raster.pixels[at],0);EXPECT_EQ(raster.pixels[at+1],0);EXPECT_EQ(raster.pixels[at+2],255);
}
void CheckReload(const Library& writer,forms::Session& s,bool owner,const std::u16string& value,
                 Artifacts& artifacts,const std::string& name) {
  const auto original=s.Draw(320,180);CheckLandmark(original);
  const auto saved=SaveDocument(s.api,s.document,artifacts,writer.name()+name);
  ASSERT_FALSE(saved.empty());
  for(const auto* reader:Implementations()) {
    forms::Session reopened(reader->api(),saved,true,false,2,{},false,owner?"owner":nullptr);
    ASSERT_TRUE(reopened.valid());EXPECT_EQ(reopened.Field(0),value);
    EXPECT_EQ(reader->api().FPDF_GetSecurityHandlerRevision(reopened.document),3);
    const auto image=reopened.Draw(320,180);CheckLandmark(image);
    CompareRaster(original,image,artifacts,writer.name()+reader->name()+name,RasterTolerance{18,0,.15});
  }
}

TEST(CoverageFormPermissions, EncryptedFieldSettersHonorEachGrantAndOwnerAuthentication) {
  const std::string program=R"JS(
    var f=this.getField('input');eq(f.value,'before');eq(this.getField('choice').value,'r');
    var values={charLimit:12,comb:true,commitOnSelChange:true,defaultValue:'replacement',
      doNotScroll:true,doNotSpellCheck:true,multiline:true,multipleSelection:true,password:true};
    for(var key in values){(function(k){permit(function(){f[k]=values[k];},canSet);})(key);}
    var changed={alignment:'center',borderStyle:'dashed',fillColor:['RGB',.8,.9,1],
      strokeColor:['RGB',.2,.4,.6],textColor:['RGB',.1,.2,.3],textFont:'Courier',textSize:14,
      lineWidth:2,display:0,readonly:false,required:true,value:'after'};
    for(var property in changed){(function(k){permit(function(){f[k]=changed[k];},canSet);})(property);}
    eq(f.value,canSet?'after':'before');eq(f.required,false);
    eq(f.charLimit,0);eq(f.multiline,false);eq(f.password,false);
    // Pinned setItems is unconditional success; it neither checks permissions
    // nor mutates options. Do not manufacture a mutation for that boundary.
    this.getField('choice').setItems([['Blue','b'],['Yellow','y']]);
    eq(this.getField('choice').getItemAt(0,true),'r');eq(this.getField('choice').getItemAt(0,false),'Red');
  )JS";
  Artifacts artifacts;
  for(size_t index=0;index<kPermissions.size();++index)for(bool owner:{false,true}) {
    const auto& spec=kPermissions[index];const bool allowed=owner||(spec.allowed&(0x100|0x20|0x8));
    const auto name="fields-"+std::to_string(index)+std::to_string(owner);
    const auto bytes=PermissionFixture(spec,Checks(program,allowed,false,false,false));
    std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name()+name);
      forms::Session s(library->api(),bytes,true,false,2,{},false,owner?"owner":nullptr);
      ASSERT_TRUE(s.valid());EXPECT_EQ(s.api.FPDF_GetSecurityHandlerRevision(s.document),3);
      EXPECT_EQ(s.api.FPDF_GetDocUserPermissions(s.document),0xfffff0c0u|spec.allowed);
      EXPECT_EQ(s.api.FPDF_GetDocPermissions(s.document),owner?0xfffffffcu:(0xfffff0c0u|spec.allowed));
      EXPECT_EQ(s.Field(0),u"before");const auto before=s.Draw(320,180);CheckLandmark(before);
      s.api.FORM_DoDocumentJSAction(s.form);CheckAlerts(s);
      EXPECT_EQ(s.Field(0),allowed?u"after":u"before");
      images[backend++]=s.Draw(320,180);
      if(allowed)EXPECT_NE(before.pixels,images[backend-1].pixels);
      else EXPECT_EQ(before.pixels,images[backend-1].pixels);
      CheckReload(*library,s,owner,allowed?u"after":u"before",artifacts,name);
    }
    CompareRaster(images[0],images[1],artifacts,name,RasterTolerance{18,0,.15});
  }
}

TEST(CoverageFormPermissions, DocumentOperationsUseTheirDistinctPermissionBits) {
  const std::string program=R"JS(
    eq(this.getField('input').value,'before');
    permit(function(){this.calculateNow();}.bind(this),canSet);
    permit(function(){this.resetForm(['input']);}.bind(this),canSet);
    eq(this.getField('input').value,canSet?'default':'before');
    permit(function(){this.delay=true;this.delay=false;}.bind(this),canModify);
    permit(function(){eq(this.getPageNumWords(0),1);eq(this.getPageNthWord(0,0),'Page');}.bind(this),canExtract);
    // This method checks extract permission, then reports unsupported even
    // for an authorized caller in the pinned source.
    permit(function(){this.getPageNthWordQuads(0,0);}.bind(this),false);
    permit(function(){this.removeField('second');}.bind(this),canRemove);
    // Pinned removeField refreshes widgets; it does not erase the field tree.
    eq(this.getField('second').value,'Second');
  )JS";
  Artifacts artifacts;
  for(size_t index=0;index<kPermissions.size();++index)for(bool owner:{false,true}) {
    const auto& spec=kPermissions[index];const bool set=owner||(spec.allowed&(0x100|0x20|0x8));
    const bool modify=owner||(spec.allowed&8),extract=owner||(spec.allowed&0x200),remove=owner||(spec.allowed&(8|0x20));
    const auto name="document-"+std::to_string(index)+std::to_string(owner);
    const auto bytes=PermissionFixture(spec,Checks(program,set,modify,extract,remove));
    std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name()+name);
      forms::Session s(library->api(),bytes,true,false,2,{},false,owner?"owner":nullptr);
      ASSERT_TRUE(s.valid());EXPECT_EQ(s.api.FPDF_GetSecurityHandlerRevision(s.document),3);
      s.api.FORM_DoDocumentJSAction(s.form);CheckAlerts(s);
      EXPECT_EQ(s.Field(0),set?u"default":u"before");
      EXPECT_EQ(s.api.FPDFPage_GetAnnotCount(s.page),3);
      images[backend++]=s.Draw(320,180);CheckLandmark(images[backend-1]);
      CheckReload(*library,s,owner,set?u"default":u"before",artifacts,name);
    }
    CompareRaster(images[0],images[1],artifacts,name,RasterTolerance{18,0,.15});
  }
}
}  // namespace
}  // namespace pdfoxium::integration
