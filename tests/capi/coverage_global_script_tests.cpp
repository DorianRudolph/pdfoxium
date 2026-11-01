#include "form_fixture.h"

#include <array>
#include <string>

namespace pdfoxium::integration {
namespace {

std::vector<uint8_t> Program(const std::string& body) {
  forms::Actions actions;
  actions.document =
      "function eq(a,b){if(JSON.stringify(a)!==JSON.stringify(b))throw 'expected '+JSON.stringify(b)+' got '+JSON.stringify(a);}"
      "function yes(v){if(!v)throw 'assertion';}"
      "function throws(f){var caught=false;try{f();}catch(e){caught=true;}if(!caught)throw 'expected exception '+f.toString();}"
      "try{" + body + ";this.getField('second').value='PASS';}"
      "catch(e){this.getField('second').value='FAIL: '+String(e);}";
  return forms::Fixture(actions);
}

void Execute(forms::Session& session) {
  session.api.FORM_DoDocumentJSAction(session.form);
  EXPECT_EQ(session.Field(1), u"PASS");
  EXPECT_TRUE(session.host->callbacks_valid);
  EXPECT_FALSE(session.host->callback_error);
}

Raster Report(forms::Session& session) {
  const auto image = session.Draw();
  forms::ExpectInk(image, 20, 30, 180, 60);
  return CropRaster(image, 20, 30, 160, 30);
}

void CompareReports(const Raster& first, const Raster& second,
                    Artifacts& artifacts, const std::string& label) {
  CompareRaster(first, second, artifacts, label, RasterTolerance{18, 0, 0.25});
}

TEST(CoverageGlobals, TypedValuesIdentityDeletionAndPersistentNameCoercionStayUsable) {
  // Unique names and explicit deletion keep the suite's one initialized
  // library session independent of this scenario's global map.
  const auto program = Program(R"JS(
    var prefix='capiGlobalsTyped_',keys=['yes','no','number','text','nil','object','array','function','reentered'];
    function clean(){keys.forEach(function(k){delete global[prefix+k];});}
    clean();
    try {
      global[prefix+'yes']=true;global[prefix+'no']=false;global[prefix+'number']=-3.125;
      global[prefix+'text']='caf\u00e9 \ud83d\ude00';global[prefix+'nil']=null;
      var object={number:7,nested:{flag:true}},array=[3,'four',null],callable=function(){return 42;};
      global[prefix+'object']=object;global[prefix+'array']=array;global[prefix+'function']=callable;
      eq(global[prefix+'yes'],true);eq(global[prefix+'no'],false);eq(global[prefix+'number'],-3.125);
      eq(global[prefix+'text'],'caf\u00e9 \ud83d\ude00');eq(global[prefix+'nil'],null);
      yes(global[prefix+'object']===object);yes(global[prefix+'array']===array);
      yes(global[prefix+'function']===callable);eq(global[prefix+'function'](),42);
      object.nested.flag=false;eq(global[prefix+'object'].nested.flag,false);
      global.setPersistent(prefix+'yes',true);global.setPersistent(prefix+'yes',false);
      global.setPersistent({toString:function(){global[prefix+'reentered']='name conversion';return prefix+'number';}},
                           {valueOf:function(){throw 'Boolean conversion must not invoke valueOf';}});
      eq(global[prefix+'reentered'],'name conversion');
      throws(function(){global.setPersistent();});throws(function(){global.setPersistent(prefix+'yes');});
      throws(function(){global.setPersistent(prefix+'missing',true);});
      var marker={},caught=false;
      try{global.setPersistent({toString:function(){throw marker;}},true);}catch(e){caught=true;}
      yes(caught);eq(global[prefix+'number'],-3.125);
      global[prefix+'yes']=undefined;eq(typeof global[prefix+'yes'],'undefined');
      throws(function(){global.setPersistent(prefix+'yes',true);});
      delete global[prefix+'object'];eq(typeof global[prefix+'object'],'undefined');
      global[prefix+'object']={restored:9};eq(global[prefix+'object'].restored,9);
      this.getField('input').value='Typed globals';
    } finally {clean();}
  )JS");
  Artifacts artifacts;
  std::array<Raster, 2> original;
  size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name());
    forms::Session session(writer->api(), program, true);
    ASSERT_TRUE(session.valid());Execute(session);
    EXPECT_EQ(session.Field(0), u"Typed globals");
    original[backend] = Report(session);
    const auto saved = SaveDocument(session.api, session.document, artifacts,
                                    writer->name() + "-typed-global-script");
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name());
      forms::Session reopened(reader->api(), saved, true);
      ASSERT_TRUE(reopened.valid());Execute(reopened);
      EXPECT_EQ(reopened.Field(0), u"Typed globals");
      CompareReports(original[backend], Report(reopened), artifacts,
                     writer->name() + "-" + reader->name() + "-global-script-reload");
    }
    ++backend;
  }
  CompareReports(original[0], original[1], artifacts, "typed-global-backends");
}

TEST(CoverageGlobals, LiveSharedStoreRestoresTypedTreesAcrossDocumentRuntimeTeardown) {
  // Both public implementations configure CFX_GlobalData without a persistence
  // delegate. An anchor runtime keeps the in-memory store alive. This tests
  // runtime sharing, not disk persistence or a PDF serialization contract.
  // Native defines CommitGlobalPersisitentVariables but never calls it; its
  // new runtimes therefore see no committed values. Rust's existing explicit
  // outer-script commit retains the intended shared snapshot semantics.
  const std::string prefix = "capiGlobalsShared_";
  const std::string cleanup =
      "['flag','number','text','nil','object','array','temporary'].forEach(function(k){delete global['" + prefix + "'+k];});";
  const auto producer = Program(cleanup + R"JS(
    global.capiGlobalsShared_flag=true;global.setPersistent('capiGlobalsShared_flag',true);
    global.capiGlobalsShared_number=17.25;global.capiGlobalsShared_text='Shared caf\u00e9';
    global.capiGlobalsShared_nil=null;global.capiGlobalsShared_array=[3,4];
    var item=Object.create({inherited:9});item.number=1.25;item.flag=false;
    item.text='Nested';item.nil=null;item.array=[5,'six'];
    Object.defineProperty(item,'hidden',{value:'excluded'});
    global.capiGlobalsShared_object=item;
    global.capiGlobalsShared_temporary='deleted';global.capiGlobalsShared_temporary=undefined;
    this.getField('input').value='Producer';
  )JS");
  const std::string rust_checks = R"JS(
    eq(global.capiGlobalsShared_flag,true);eq(global.capiGlobalsShared_number,17.25);
    eq(global.capiGlobalsShared_text,'Shared caf\u00e9');eq(global.capiGlobalsShared_nil,null);
    eq(global.capiGlobalsShared_array[0],3);eq(global.capiGlobalsShared_array[1],4);
    var item=global.capiGlobalsShared_object;eq(item.number,1.25);eq(item.flag,false);
    eq(item.text,'Nested');eq(item.nil,null);eq(item.array[0],5);eq(item.array[1],'six');
    eq(item.inherited,9);eq(typeof item.hidden,'undefined');
    eq(typeof global.capiGlobalsShared_temporary,'undefined');
  )JS";
  const std::string native_checks = R"JS(
    ['flag','number','text','nil','object','array','temporary'].forEach(function(k){
      eq(typeof global['capiGlobalsShared_'+k],'undefined');
    });
  )JS";
  Artifacts artifacts;
  std::array<Raster, 2> reports;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    forms::Session anchor(library->api(), Program("this.getField('input').value='Anchor';"), true);
    ASSERT_TRUE(anchor.valid());Execute(anchor);
    {
      forms::Session source(library->api(), producer, true);
      ASSERT_TRUE(source.valid());Execute(source);
      EXPECT_EQ(source.Field(0), u"Producer");
    }
    {
      const auto checks = library->name() == "native" ? native_checks : rust_checks;
      forms::Session consumer(library->api(),
          Program("try{" + checks + ";this.getField('input').value='Shared values';}finally{" + cleanup + "}"), true);
      ASSERT_TRUE(consumer.valid());Execute(consumer);
      EXPECT_EQ(consumer.Field(0), u"Shared values");
      reports[backend++] = Report(consumer);
    }
    // A third fresh runtime verifies the deletion commit independently.
    forms::Session cleared(library->api(), Program(native_checks + cleanup), true);
    ASSERT_TRUE(cleared.valid());Execute(cleared);
  }
  CompareReports(reports[0], reports[1], artifacts, "shared-global-report");
}

}  // namespace
}  // namespace pdfoxium::integration
