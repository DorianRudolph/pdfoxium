#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <array>
#include <string>

namespace pdfoxium::integration {
namespace {

// Expectations below are transcribed from the pinned native constructor
// tables in upstream/pdfium/xfa/fxfa/parser/cxfa_*.cpp, elements.inc and
// attribute_values.inc. No Rust metadata is an oracle for these tests.
std::string CheckedSchema(const std::string& body) {
  return "var rustBackend=xfa.form.root.amount.rawValue==='rust',check=0;"
      "function eq(a,b){++check;if(a===b)return;if(Array.isArray(a)&&Array.isArray(b)&&JSON.stringify(a)===JSON.stringify(b))return;throw 'check '+check+': expected '+String(b)+' got '+String(a);}"
      "function yes(v,label){++check;if(!v)throw 'check '+check+': '+label;}"
      "function throws(f){var caught=false;try{f();}catch(e){caught=true;}yes(caught,'exception required');}"
      "function classes(n){var a=[];for(var i=0;i<n.nodes.length;++i)a.push(n.nodes.item(i).className);return a;}"
      "try{this.caption.value.text.value='Run';"+body+
      ";xfa.form.root.amount.rawValue='PASS';}catch(e){xfa.form.root.amount.rawValue='FAIL';xfa.host.messageBox(String(e));}";
}
std::vector<uint8_t> SchemaFixture(const std::string& body,const std::string& model) {
  const std::string button="<field name=\"run\" w=\"80pt\" h=\"20pt\"><bind match=\"none\"/>"
      "<ui><button/></ui><font typeface=\"Helvetica\" size=\"10pt\"/>"
      "<caption><value><text>Run</text></value></caption>"+
      forms::Event(CheckedSchema(body),"click")+"</field>";
  const std::string xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\">"
      "<subform name=\"root\" layout=\"tb\" locale=\"en_US\" restoreState=\"auto\">"
      "<pageSet><pageArea name=\"page\"><occur min=\"1\" max=\"-1\"/>"
      "<medium short=\"200pt\" long=\"300pt\"/><contentArea w=\"200pt\" h=\"300pt\"/></pageArea></pageSet>"
      "<draw w=\"40pt\" h=\"20pt\"><value><rectangle><fill><color value=\"255,0,0\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw>"
      "<field name=\"amount\" w=\"100pt\" h=\"20pt\"><bind match=\"none\"/><ui><textEdit/></ui>"
      "<font typeface=\"Helvetica\" size=\"12pt\"/><value><text>Ready</text></value></field>"+button+model+"</subform></template>";
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Resources << /Font << /Helv 10 0 R >> >> >>",
      "<< /Fields [] /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 10 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
      PdfStream("","<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),
      PdfStream("","<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
      PdfStream("",xml),
      PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root/></xfa:data></xfa:datasets>"),
      PdfStream("","</xdp:xdp>"),
      "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"});
}
void ExecuteSchema(forms::Session& s,const Library& library) {
  ASSERT_TRUE(s.Focus(8,25));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));
  s.Replace(library.name()=="rust"?u"rust":u"native");s.api.FORM_ForceToKillFocus(s.form);
  s.Click(8,50);ASSERT_TRUE(s.Focus(8,25));
  std::string errors;
  for(const auto& alert:s.host->alerts)
    errors+=std::string(std::get<0>(alert).begin(),std::get<0>(alert).end())+"\n";
  EXPECT_EQ(s.Text(),u"PASS")<<errors;EXPECT_TRUE(s.host->alerts.empty())<<errors;
  s.api.FORM_ForceToKillFocus(s.form);
  EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);
}
void CompareSchema(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label) {
  CompareRaster(a,b,artifacts,label,RasterTolerance{24,0,.2});
  CompareRaster(CropRaster(a,0,20,100,20),CropRaster(b,0,20,100,20),artifacts,
                label+"-report",RasterTolerance{24,0,1});
  CompareRaster(CropRaster(a,0,0,200,20),CropRaster(b,0,0,200,20),artifacts,label+"-landmark");
}
void ExpectSchemaPresentation(const Raster& image) {
  size_t yellow=0,blue=0,red=0;
  for(int y=60;y<100;++y)for(int x=0;x<160;++x) {
    const auto i=(static_cast<size_t>(y)*image.width+x)*4;
    const auto b=image.pixels[i],g=image.pixels[i+1],r=image.pixels[i+2];
    yellow+=r>235&&g>235&&b<30;blue+=b>180&&r<80&&g<80;red+=r>235&&g<220&&b<220;
  }
  EXPECT_GT(yellow,1000u);EXPECT_GT(blue,20u);EXPECT_GT(red,50u);
}
void RunSchema(const std::string& body,const std::string& model="",bool save=false) {
  const auto bytes=SchemaFixture(body,model);Artifacts artifacts;std::array<Raster,2> rendered;size_t index=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());forms::Session s(writer->api(),bytes,true,true);
    ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);const auto before=s.Draw(200,300);
    ExecuteSchema(s,*writer);ASSERT_FALSE(::testing::Test::HasFailure());rendered[index]=s.Draw(200,300);forms::ExpectInk(rendered[index],0,20,100,40);
    EXPECT_NE(CropRaster(before,0,20,100,20).pixels,CropRaster(rendered[index],0,20,100,20).pixels);
    if(model.find("name=\"sample\"")!=std::string::npos)ExpectSchemaPresentation(rendered[index]);
    if(save) {
      const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+"-schema");ASSERT_FALSE(saved.empty());
      for(const auto* reader:Implementations()) {
        SCOPED_TRACE(reader->name());forms::Session reopened(reader->api(),saved,true,true);
        ASSERT_TRUE(reopened.valid());ASSERT_TRUE(reopened.xfa_loaded);ExecuteSchema(reopened,*reader);ASSERT_FALSE(::testing::Test::HasFailure());
        const auto image=reopened.Draw(200,300);
        if(model.find("name=\"sample\"")!=std::string::npos)ExpectSchemaPresentation(image);
        CompareSchema(rendered[index],image,artifacts,writer->name()+"-"+reader->name()+"-reload");
      }
    }
    ++index;
  }
  CompareSchema(rendered[0],rendered[1],artifacts,"schema-backends");
}

TEST(CoverageXfaSchema, EveryNativeFormConstructorCreatesIndependentDetachedNodesAndClones) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunSchema(R"JS(
    var tags=["appearanceFilter","arc","area","assist","barcode","bind","bindItems","bookend","boolean","border","break","breakAfter","breakBefore","button","calculate","caption","certificate","certificates","checkButton","choiceList","color","comb","connect","contentArea","corner","date","dateTime","dateTimeEdit","decimal","defaultUi","desc","digestMethod","digestMethods","draw","dSigData","edge","encoding","encodings","encrypt","encryptionMethod","encryptionMethods","event","exclGroup","exData","execute","exObject","extras","field","fill","filter","float","font","format","handler","hyphenation","image","imageEdit","integer","issuers","items","keep","keyUsage","line","linear","lockDocument","manifest","margin","mdp","medium","message","numericEdit","occur","oid","oids","overflow","pageArea","pageSet","para","passwordEdit","pattern","picture","proto","radial","reason","reasons","rectangle","ref","script","setProperty","#text","#xHTML","#xml","signature","signatureProperties","signData","signing","solid","speak","stipple","subform","subformSet","subjectDN","subjectDNs","submit","template","text","textEdit","time","timeStamp","toolTip","traversal","traverse","ui","validate","value","variables"];
    tags.forEach(function(tag){try{
      var n=xfa.form.createNode(tag);yes(n!==null,'permitted form class '+tag);eq(n.className,tag);
      // Native constructors for these primitives deliberately use CJX_Object
      // (Boolean specializes it), so XML element ancestry supplies no methods.
      var primitive={boolean:[1,true],decimal:[12.5,12.5],exData:['payload','payload'],
        float:[1.25,1.25],integer:[17,17],text:['sample','sample'],time:['12:34:56','12:34:56']};
      if(primitive[tag]) {
        eq(n.parent,undefined);eq(n.model,undefined);eq(n.nodes,undefined);eq(typeof n.clone,'undefined');
        // A real owning Field supplies the notification/container chain.
        // Native dereferences Value.parent when an orphan Value is mutated.
        var owner=xfa.form.createNode('field'),holder=owner.getElement('value'),v=holder.getElement(tag),data=primitive[tag];
        v.value=data[0];eq(v.value,data[1]);
        var copiedOwner=owner.clone(true),cloned=copiedOwner.getElement('value').getElement(tag);eq(cloned.value,data[1]);
        cloned.value=null;eq(v.value,data[1]);return;
      }
      eq(n.parent,null);
      n.setAttribute('original '+tag,'schemaProbe');eq(n.getAttribute('schemaProbe'),'original '+tag);
      var copy=n.clone(true);yes(copy!==null&&copy!==n,'independent clone '+tag);eq(copy.className,tag);eq(copy.parent,null);
      eq(copy.getAttribute('schemaProbe'),'original '+tag);copy.setAttribute('changed','schemaProbe');
      eq(n.getAttribute('schemaProbe'),'original '+tag);eq(copy.getAttribute('schemaProbe'),'changed');
    }catch(e){throw tag+': '+e;}});
    ['config','recordSet','connectionSet','dataGroup','dataValue','localeSet','xfa','notAnElement'].forEach(function(tag){
      eq(xfa.form.createNode(tag),null);
    });
    throws(function(){xfa.form.createNode();});throws(function(){xfa.form.createNode('field','a','b','c');});
  )JS");
}

TEST(CoverageXfaSchema, NativeConstructorDefaultsAndInvalidEnumsRemainStableAfterTypedRoundTrips) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunSchema(R"JS(
    var defaults=[["appearanceFilter","type","optional","Enum"],
["arc","startAngle","0","Integer"],
["arc","sweepAngle","360","Integer"],
["arc","circular","0","Boolean"],
["arc","hand","even","Enum"],
["area","x","0in","Measure"],
["area","y","0in","Measure"],
["area","level","0","Integer"],
["area","colSpan","1","Integer"],
["area","lock","0","Integer"],
["barcode","dataPrep","none","Enum"],
["barcode","textLocation","below","Enum"],
["barcode","moduleWidth","0.25mm","Measure"],
["barcode","printCheckDigit","0","Boolean"],
["barcode","moduleHeight","5mm","Measure"],
["barcode","truncate","0","Boolean"],
["barcode","wideNarrowRatio","3:1","CData"],
["barcode","upsMode","usCarrier","Enum"],
["barcode","checksum","none","Enum"],
["bind","transferEncoding","none","Enum"],
["bind","match","once","Enum"],
["border","break","close","Enum"],
["border","presence","visible","Enum"],
["border","hand","even","Enum"],
["break","startNew","0","Boolean"],
["break","after","auto","Enum"],
["break","before","auto","Enum"],
["breakAfter","startNew","0","Boolean"],
["breakAfter","targetType","auto","Enum"],
["breakBefore","startNew","0","Boolean"],
["breakBefore","targetType","auto","Enum"],
["button","highlight","inverted","Enum"],
["calculate","override","error","Enum"],
["caption","reserve","-1","Measure"],
["caption","presence","visible","Enum"],
["caption","placement","left","Enum"],
["certificates","credentialServerPolicy","optional","Enum"],
["checkButton","allowNeutral","0","Boolean"],
["checkButton","mark","default","Enum"],
["checkButton","shape","square","Enum"],
["checkButton","size","10pt","Measure"],
["choiceList","open","userControl","Enum"],
["choiceList","commitOn","select","Enum"],
["choiceList","textEntry","0","Boolean"],
["color","cSpace","SRGB","CData"],
["comb","numberOfCells","0","Integer"],
["connect","timeout","15","Integer"],
["connect","usage","exportAndImport","Enum"],
["contentArea","h","0in","Measure"],
["contentArea","w","0in","Measure"],
["contentArea","x","0in","Measure"],
["contentArea","y","0in","Measure"],
["corner","stroke","solid","Enum"],
["corner","presence","visible","Enum"],
["corner","inverted","0","Boolean"],
["corner","thickness","0.5pt","Measure"],
["corner","join","square","Enum"],
["corner","radius","0in","Measure"],
["dateTimeEdit","picker","host","Enum"],
["dateTimeEdit","hScrollPolicy","auto","Enum"],
["decimal","fracDigits","2","Integer"],
["decimal","leadDigits","-1","Integer"],
["digestMethods","type","optional","Enum"],
["draw","h","0in","Measure"],
["draw","w","0in","Measure"],
["draw","x","0in","Measure"],
["draw","y","0in","Measure"],
["draw","hAlign","left","Enum"],
["draw","rotate","0","Integer"],
["draw","presence","visible","Enum"],
["draw","vAlign","top","Enum"],
["draw","maxH","0in","Measure"],
["draw","maxW","0in","Measure"],
["draw","minH","0in","Measure"],
["draw","minW","0in","Measure"],
["draw","colSpan","1","Integer"],
["draw","anchorType","topLeft","Enum"],
["edge","cap","square","Enum"],
["edge","stroke","solid","Enum"],
["edge","presence","visible","Enum"],
["edge","thickness","0.5pt","Measure"],
["encodings","type","optional","Enum"],
["encrypt","lock","0","Integer"],
["encryptionMethods","type","optional","Enum"],
["event","listen","refOnly","Enum"],
["event","activity","click","Enum"],
["exclGroup","h","0in","Measure"],
["exclGroup","w","0in","Measure"],
["exclGroup","x","0in","Measure"],
["exclGroup","y","0in","Measure"],
["exclGroup","hAlign","left","Enum"],
["exclGroup","access","open","Enum"],
["exclGroup","presence","visible","Enum"],
["exclGroup","vAlign","top","Enum"],
["exclGroup","maxH","0in","Measure"],
["exclGroup","maxW","0in","Measure"],
["exclGroup","minH","0in","Measure"],
["exclGroup","minW","0in","Measure"],
["exclGroup","layout","position","Enum"],
["exclGroup","colSpan","1","Integer"],
["exclGroup","anchorType","topLeft","Enum"],
["exData","transferEncoding","none","Enum"],
["exData","maxLength","-1","Integer"],
["execute","runAt","client","Enum"],
["execute","executeType","import","Enum"],
["field","h","0in","Measure"],
["field","w","0in","Measure"],
["field","x","0in","Measure"],
["field","y","0in","Measure"],
["field","hAlign","left","Enum"],
["field","access","open","Enum"],
["field","rotate","0","Integer"],
["field","presence","visible","Enum"],
["field","vAlign","top","Enum"],
["field","maxH","0in","Measure"],
["field","maxW","0in","Measure"],
["field","minH","0in","Measure"],
["field","minW","0in","Measure"],
["field","colSpan","1","Integer"],
["field","anchorType","topLeft","Enum"],
["fill","presence","visible","Enum"],
["font","lineThrough","0","Integer"],
["font","typeface","Courier","CData"],
["font","fontHorizontalScale","100%","CData"],
["font","kerningMode","none","Enum"],
["font","underline","0","Integer"],
["font","baselineShift","0in","Measure"],
["font","overlinePeriod","all","Enum"],
["font","lineThroughPeriod","all","Enum"],
["font","fontVerticalScale","100%","CData"],
["font","size","10pt","Measure"],
["font","posture","normal","Enum"],
["font","weight","normal","Enum"],
["font","underlinePeriod","all","Enum"],
["font","overline","0","Integer"],
["font","genericFamily","serif","Enum"],
["handler","type","optional","Enum"],
["hyphenation","wordCharacterCount","7","Integer"],
["hyphenation","hyphenate","0","Boolean"],
["hyphenation","excludeInitialCap","0","Boolean"],
["hyphenation","pushCharacterCount","3","Integer"],
["hyphenation","remainCharacterCount","3","Integer"],
["hyphenation","excludeAllCaps","0","Boolean"],
["image","transferEncoding","base64","Enum"],
["image","aspect","fit","Enum"],
["imageEdit","data","link","Enum"],
["issuers","type","optional","Enum"],
["items","presence","visible","Enum"],
["items","save","0","Boolean"],
["keep","next","none","Enum"],
["keep","previous","none","Enum"],
["keep","intact","none","Enum"],
["keyUsage","type","optional","Enum"],
["line","slope","\\","Enum"],
["line","hand","even","Enum"],
["linear","type","toRight","Enum"],
["lockDocument","type","optional","Enum"],
["manifest","action","include","Enum"],
["margin","leftInset","0in","Measure"],
["margin","bottomInset","0in","Measure"],
["margin","topInset","0in","Measure"],
["margin","rightInset","0in","Measure"],
["mdp","signatureType","filter","Enum"],
["mdp","permissions","2","Integer"],
["medium","trayOut","auto","Enum"],
["medium","orientation","portrait","Enum"],
["medium","imagingBBox","none","CData"],
["medium","short","0in","Measure"],
["medium","trayIn","auto","Enum"],
["medium","long","0in","Measure"],
["message","lock","0","Integer"],
["numericEdit","hScrollPolicy","auto","Enum"],
["occur","max","1","Integer"],
["occur","min","1","Integer"],
["occur","initial","1","Integer"],
["oids","type","optional","Enum"],
["pageArea","pagePosition","any","Enum"],
["pageArea","oddOrEven","any","Enum"],
["pageArea","initialNumber","1","Integer"],
["pageArea","numbered","1","Integer"],
["pageArea","blankOrNotBlank","any","Enum"],
["pageSet","relation","orderedOccurrence","Enum"],
["pageSet","duplexImposition","longEdge","Enum"],
["para","hAlign","left","Enum"],
["para","textIndent","0in","Measure"],
["para","widows","0","Integer"],
["para","marginRight","0in","Measure"],
["para","marginLeft","0in","Measure"],
["para","radixOffset","0in","Measure"],
["para","spaceBelow","0in","Measure"],
["para","vAlign","top","Enum"],
["para","orphans","0","Integer"],
["para","lineHeight","0pt","Measure"],
["para","spaceAbove","0in","Measure"],
["passwordEdit","passwordChar","*","CData"],
["passwordEdit","hScrollPolicy","auto","Enum"],
["pattern","type","crossHatch","Enum"],
["picture","lock","0","Integer"],
["radial","type","toEdge","Enum"],
["reasons","type","optional","Enum"],
["rectangle","hand","even","Enum"],
["script","runAt","client","Enum"],
["script","lock","0","Integer"],
["signature","type","PDF1.3","Enum"],
["signData","operation","sign","Enum"],
["signing","type","optional","Enum"],
["speak","priority","custom","Enum"],
["speak","disable","0","Boolean"],
["stipple","rate","50","Integer"],
["subform","h","0in","Measure"],
["subform","w","0in","Measure"],
["subform","x","0in","Measure"],
["subform","y","0in","Measure"],
["subform","hAlign","left","Enum"],
["subform","allowMacro","0","Boolean"],
["subform","access","open","Enum"],
["subform","presence","visible","Enum"],
["subform","vAlign","top","Enum"],
["subform","maxH","0in","Measure"],
["subform","maxW","0in","Measure"],
["subform","minH","0in","Measure"],
["subform","minW","0in","Measure"],
["subform","layout","position","Enum"],
["subform","mergeMode","consumeData","Enum"],
["subform","colSpan","1","Integer"],
["subform","anchorType","topLeft","Enum"],
["subform","restoreState","manual","Enum"],
["subform","scope","name","Enum"],
["subformSet","relation","ordered","Enum"],
["subjectDNs","type","optional","Enum"],
["submit","format","xdp","Enum"],
["submit","embedPDF","0","Boolean"],
["template","baseProfile","full","Enum"],
["template","lock","0","Integer"],
["text","maxChars","0","Integer"],
["textEdit","vScrollPolicy","auto","Enum"],
["textEdit","allowRichText","0","Boolean"],
["textEdit","multiLine","0","Boolean"],
["textEdit","hScrollPolicy","auto","Enum"],
["timeStamp","type","optional","Enum"],
["traverse","operation","next","Enum"],
["validate","scriptTest","error","Enum"],
["validate","nullTest","disabled","Enum"],
["validate","formatTest","warning","Enum"],
["validate","lock","0","Integer"]];
    defaults.forEach(function(row){try{
      var n=xfa.form.createNode(row[0]);yes(n!==null,row[0]);
      var primitive=['decimal','exData','text'].indexOf(row[0])>=0,holder,owner=xfa.form.createNode('field');
      if(primitive){holder=owner.getElement('value');n=holder.getElement(row[0]);}
      else if(['font','para','margin','caption'].indexOf(row[0])>=0)n=owner.getElement(row[0]);
      else if(row[0]==='comb')n=owner.getElement('ui').getElement('textEdit').getElement('comb');
      function read(v){return primitive?v[row[1]]:v.getAttribute(row[1]);}
      function write(v,value){if(primitive)v[row[1]]=value;else v.setAttribute(value,row[1]);}
      eq(read(n),row[2]);write(n,row[2]);eq(read(n),row[2]);
      var copiedOwner=primitive?owner.clone(true):null;
      var copy=primitive?copiedOwner.getElement('value').getElement(row[0]):n.clone(false);eq(read(copy),row[2]);
      if(row[3]==='Enum'){write(n,'not-a-schema-enum');eq(read(n),row[2]);}
    }catch(e){throw row[0]+'.'+row[1]+': '+e;}});
  )JS");
}

TEST(CoverageXfaSchema, AttributeAndScriptConversionsDistinguishBooleanIntegerEnumAndMeasures) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunSchema(R"JS(
    var owner=xfa.form.createNode('field'),font=owner.getElement('font'),edit=owner.getElement('ui').getElement('textEdit'),
        corner=xfa.form.createNode('corner'),value=owner.getElement('value'),comb=edit.getElement('comb');
    [['0','0'],['1','1'],['false','1'],['','1'],['00','1']].forEach(function(row){
      edit.setAttribute(row[0],'multiLine');eq(edit.getAttribute('multiLine'),row[1]);
    });
    [[false,'0'],[true,'1'],[0,'0'],[7,'1'],['0','1'],['','0'],[null,'0'],[{},'1']].forEach(function(row){
      value.override=row[0];eq(value.override,row[1]);eq(value.getAttribute('override'),row[1]);
    });
    [['2.4','2'],['2.6','3'],['-2.4','-2'],['-2.6','-3'],['garbage','0']].forEach(function(row){
      font.setAttribute(row[0],'underline');eq(font.getAttribute('underline'),row[1]);
    });
    [[2.9,2],[-2.9,-2],[4294967297,1],['7.8',7],[null,0]].forEach(function(row){
      comb.numberOfCells=row[0];eq(comb.numberOfCells,row[1]);eq(comb.getAttribute('numberOfCells'),String(row[1]));
    });
    ['bold','normal'].forEach(function(value){font.weight=value;eq(font.weight,value);});
    font.weight='not-valid';eq(font.weight,'normal');
    ['0pt','1.5in','2.25cm','3mm','6pc','9pt'].forEach(function(value){
      font.setAttribute(value,'size');eq(font.getAttribute('size'),value);
      corner.setAttribute(value,'radius');eq(corner.getAttribute('radius'),value);
    });
    var order=[];font.setAttribute({toString:function(){order.push('value');return 'Helvetica';}},
      {toString:function(){order.push('name');return 'typeface';}});
    eq(order,['value','name']);eq(font.typeface,'Helvetica');
    throws(function(){font.setAttribute('a');});throws(function(){font.getAttribute();});
  )JS");
}

TEST(CoverageXfaSchema, BoundedNestedPropertiesAndOneOfChoicesObeyTheNativeSchema) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunSchema(R"JS(
    var graphs=[['field','border',1],['field','margin',1],['field','font',1],['field','caption',1],
      ['field','para',1],['field','validate',1],['field','calculate',1],['field','assist',1],
      ['border','edge',4],['border','corner',4],['border','fill',1],['border','margin',1],
      ['rectangle','edge',4],['rectangle','corner',4],['rectangle','fill',1],
      ['arc','edge',1],['arc','fill',1],['line','edge',1],['font','fill',1],
      ['textEdit','comb',1],['textEdit','border',1],['textEdit','margin',1],
      ['caption','value',1],['caption','font',1],['caption','para',1],
      ['para','hyphenation',1],['fill','color',1],['edge','color',1],['corner','color',1],
      ['subform','occur',1],['subform','pageSet',1],['subform','bind',1]];
    graphs.forEach(function(row){var n=xfa.form.createNode(row[0]);
      for(var i=0;i<row[2];++i){var p=n.getElement(row[1],i);yes(p!==null,row[0]+'.'+row[1]);eq(p.className,row[1]);
        eq(p.parent.className,row[0]);p.setAttribute(String(i),'schemaIndex');eq(n.getElement(row[1],i).getAttribute('schemaIndex'),String(i));}
      eq(n.getElement(row[1],row[2]),null);eq(n.getElement(row[1],-1),null);
      eq(n.isPropertySpecified(row[1],false,0),true);
    });
    [['ui','textEdit','numericEdit'],['value','integer','text'],['fill','linear','pattern']].forEach(function(row){
      var n=xfa.form.createNode(row[0]),first=n.getElement(row[1]);yes(first!==null,'first one-of');
      eq(n.oneOfChild.className,row[1]);eq(n.getElement(row[2]),null);eq(n.oneOfChild.className,row[1]);
    });
    var n=xfa.form.createNode('field');eq(n.getElement('notAnElement'),null);eq(n.getElement('field'),null);
    throws(function(){n.getElement();});throws(function(){n.getElement('font',0,0);});
    throws(function(){n.isPropertySpecified();});throws(function(){n.isPropertySpecified('font',false,0,0);});
  )JS");
}

TEST(CoverageXfaSchema, DeepAndShallowClonesPreserveAttributesWithoutSharingPropertyTrees) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunSchema(R"JS(
    var original=xfa.form.createNode('field','source');original.setAttribute('en_GB','locale');
    var font=original.getElement('font');font.typeface='Helvetica';font.size='13pt';
    var text=original.getElement('value').getElement('text');text.value='Original';
    var border=original.getElement('border');for(var i=0;i<4;++i)border.getElement('edge',i).thickness='2pt';
    var shallow=original.clone(false),deep=original.clone(true);
    eq(shallow.name,'source');eq(deep.name,'source');eq(shallow.getAttribute('locale'),'en_GB');
    eq(shallow.nodes.length,0);eq(deep.getElement('font').typeface,'Helvetica');eq(deep.getElement('font').size,'13pt');
    eq(deep.getElement('value').getElement('text').value,'Original');
    deep.getElement('font').size='20pt';deep.getElement('value').getElement('text').value='Clone';
    eq(font.size,'13pt');eq(text.value,'Original');eq(deep.getElement('font').size,'20pt');
    var truthy=original.clone({valueOf:function(){throw 'boolean coercion must not call';}});
    eq(truthy.getElement('font').size,'13pt');
    throws(function(){original.clone();});throws(function(){original.clone(true,false);});
  )JS");
}

TEST(CoverageXfaSchema, DescriptionUseCopiesTemplatePrototypeChildrenAndSupportsIdAndSomRecovery) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const std::string model="<proto><desc id=\"descriptionOne\" name=\"descriptionOne\"><text>first</text><integer>42</integer></desc>"
      "<desc id=\"descriptionTwo\" name=\"descriptionTwo\"><text>second</text><boolean>1</boolean></desc></proto>";
  RunSchema(R"JS(
    var d=xfa.form.createNode('desc');d.getElement('text').value='old';
    d.use='#descriptionOne';eq(d.getElement('text').value,'first');eq(d.getElement('integer').value,42);
    eq(classes(d),['text','integer']);d.getElement('text').value='local';
    var children=xfa.template.root.nodes,prototype=null;
    for(var i=0;i<children.length;++i)if(children.item(i).className==='proto')prototype=children.item(i);
    yes(prototype!==null,'physical template prototype');
    eq(String(prototype.resolveNode('#desc[0].text').value).replace(/^\n/,''),'first');
    d.use='$.#desc[1]';eq(classes(d),['text','boolean']);eq(d.getElement('text').value,'second');
    eq(d.getElement('boolean').value,true);d.use='#missing';eq(d.getElement('text').value,'second');
    d.use='';eq(d.getElement('text').value,'second');d.use='#descriptionOne';eq(d.getElement('integer').value,42);
    eq(d.metadata('anything'),'');eq(d.metadata(),'');throws(function(){d.metadata('a','b');});
  )JS",model,true);
}

TEST(CoverageXfaSchema, TemplateXmlImportBuildsTypedPropertiesFiltersUnknownChildrenAndClonesSafely) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunSchema(R"JS(
    var n=xfa.form.createNode('field');
    n.loadXML('<field><font typeface="Helvetica" size="14pt" weight="bold"/><margin leftInset="3pt" rightInset="4pt"/>'+
      '<value><integer>17</integer></value><notAnElement/><ui><textEdit multiLine="1"/></ui></field>',true,true);
    eq(n.getElement('font').typeface,'Helvetica');eq(n.getElement('font').size,'14pt');eq(n.getElement('font').weight,'bold');
    eq(n.getElement('margin').leftInset,'3pt');eq(n.getElement('margin').rightInset,'4pt');
    eq(n.getElement('value').getElement('integer').value,17);eq(n.resolveNode('notAnElement'),null);
    eq(n.getElement('ui').getElement('textEdit').multiLine,'1');
    var copy=n.clone(true);copy.getElement('value').getElement('integer').value=23;
    eq(n.getElement('value').getElement('integer').value,17);eq(copy.getElement('value').getElement('integer').value,23);
    n.loadXML('<field><value><text>replacement</text></value></field>',true,true);
    eq(n.getElement('value').getElement('text').value,'replacement');
    // resolveNode('font') can create this schema property. Inspect actual
    // children before the explicit append instead of invoking that getter.
    eq(classes(n).indexOf('font'),-1);eq(classes(n).indexOf('margin'),-1);
    n.loadXML('<field><font size="11pt"/></field>',true,false);eq(n.getElement('font').size,'11pt');
    n.loadXML('<field attr=bad/>',true,true);eq(n.getElement('font').size,'11pt');
    eq(n.saveFilteredXML('ignored'),undefined);eq(n.applyXSL('ignored'),undefined);eq(n.assignNode('ignored'),undefined);
    eq(n.saveFilteredXML(),undefined);throws(function(){n.applyXSL();});throws(function(){n.assignNode();});
  )JS");
}

TEST(CoverageXfaSchema, AttachedPresentationPropertiesMutateRealTextMarginsColorsAndBordersAcrossReload) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const std::string model="<field name=\"sample\" w=\"160pt\" h=\"40pt\"><bind match=\"none\"/><ui><textEdit/></ui>"
      "<font typeface=\"Helvetica\" size=\"12pt\"/><value><text>Before</text></value>"
      "<border><edge thickness=\"1pt\"/><edge thickness=\"1pt\"/><edge thickness=\"1pt\"/><edge thickness=\"1pt\"/></border></field>";
  RunSchema(R"JS(
    var s=xfa.form.root.sample;s.rawValue='Schema';s.fontColor='0,0,255';s.fillColor='255,255,0';s.borderColor='255,0,0';
    eq(s.fontColor,'0,0,255');eq(s.fillColor,'255,255,0');eq(s.borderColor,'255,0,0');
    var font=s.getElement('font');font.size='16pt';font.weight='bold';font.posture='italic';
    eq(font.size,'16pt');eq(font.weight,'bold');eq(font.posture,'italic');
    var margin=s.getElement('margin');margin.leftInset='5pt';margin.rightInset='6pt';margin.topInset='2pt';margin.bottomInset='3pt';
    eq(margin.leftInset,'5pt');eq(margin.rightInset,'6pt');eq(margin.topInset,'2pt');eq(margin.bottomInset,'3pt');
    var border=s.getElement('border');for(var i=0;i<4;++i){var edge=border.getElement('edge',i);edge.thickness='2pt';eq(edge.thickness,'2pt');}
    s.mandatory='error';s.validationMessage='Bad value';s.mandatoryMessage='Required';
    eq(s.mandatory,'error');eq(s.validationMessage,'Bad value');eq(s.mandatoryMessage,'Required');
    s.mandatory='disabled';eq(s.rawValue,'Schema');
  )JS",model,true);
}


TEST(CoverageXfaSchema, ConfigPropertyGraphPreservesRenderingPolicyWhileClonesKeepIndependentMetadata) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunSchema(R"JS(
    // The existing <present><pdf> packet is part of this minimal fixture;
    // @pdf traverses that existing child, not an invented schema property.
    // All other steps/counts/defaults come from native Property/AttributeData.
    var rows=[[[],"config",[["lock","0"]]],
[["present","@pdf"],"pdf",[["lock","0"]]],
[["present"],"present",[["lock","0"]]],
[["acrobat"],"acrobat",[["lock","0"]]],
[["trace"],"trace",[["lock","0"]]],
[["present","@pdf","adobeExtensionLevel"],"adobeExtensionLevel",[["lock","0"]]],
[["present","@pdf","fontInfo"],"fontInfo",[["lock","0"]]],
[["present","@pdf","xdc"],"xdc",[["lock","0"]]],
[["present","@pdf","pdfa"],"pdfa",[["lock","0"]]],
[["present","@pdf","batchOutput"],"batchOutput",[["format","none"],["lock","0"]]],
[["present","@pdf","viewerPreferences"],"viewerPreferences",[["lock","0"]]],
[["present","@pdf","scriptModel"],"scriptModel",[["lock","0"]]],
[["present","@pdf","version"],"version",[["lock","0"]]],
[["present","@pdf","submitFormat"],"submitFormat",[["lock","0"]]],
[["present","@pdf","silentPrint"],"silentPrint",[["lock","0"]]],
[["present","@pdf","producer"],"producer",[["lock","0"]]],
[["present","@pdf","compression"],"compression",[["lock","0"]]],
[["present","@pdf","interactive"],"interactive",[["lock","0"]]],
[["present","@pdf","encryption"],"encryption",[["lock","0"]]],
[["present","@pdf","renderPolicy"],"renderPolicy",[["lock","0"]]],
[["present","@pdf","openAction"],"openAction",[["lock","0"]]],
[["present","@pdf","creator"],"creator",[["lock","0"]]],
[["present","@pdf","linearized"],"linearized",[["lock","0"]]],
[["present","@pdf","tagged"],"tagged",[["lock","0"]]],
[["present","xdp"],"xdp",[["lock","0"]]],
[["present","cache"],"cache",[["lock","0"]]],
[["present","pagination"],"pagination",[["lock","0"]]],
[["present","overprint"],"overprint",[["lock","0"]]],
[["present","behaviorOverride"],"behaviorOverride",[["lock","0"]]],
[["present","copies"],"copies",[["lock","0"]]],
[["present","output"],"output",[["lock","0"]]],
[["present","validate"],"validate",[["scriptTest","error"],["nullTest","disabled"],["formatTest","warning"],["lock","0"]]],
[["present","layout"],"layout",[["lock","0"]]],
[["present","script"],"script",[["runAt","client"],["lock","0"]]],
[["present","common"],"common",[["lock","0"]]],
[["present","paginationOverride"],"paginationOverride",[["lock","0"]]],
[["present","destination"],"destination",[["lock","0"]]],
[["present","incrementalMerge"],"incrementalMerge",[["lock","0"]]],
[["acrobat","autoSave"],"autoSave",[["lock","0"]]],
[["acrobat","validateApprovalSignatures"],"validateApprovalSignatures",[["lock","0"]]],
[["acrobat","acrobat7"],"acrobat7",[["lock","0"]]],
[["present","@pdf","fontInfo","subsetBelow"],"subsetBelow",[["lock","0"]]],
[["present","@pdf","fontInfo","map"],"map",[["lock","0"]]],
[["present","@pdf","fontInfo","embed"],"embed",[["lock","0"]]],
[["present","@pdf","xdc","uri"],"uri",[["lock","0"]]],
[["present","@pdf","xdc","xsl"],"xsl",[["lock","0"]]],
[["present","@pdf","pdfa","amd"],"amd",[["lock","0"]]],
[["present","@pdf","pdfa","part"],"part",[["lock","0"]]],
[["present","@pdf","pdfa","includeXDPContent"],"includeXDPContent",[["lock","0"]]],
[["present","@pdf","pdfa","conformance"],"conformance",[["lock","0"]]],
[["present","@pdf","viewerPreferences","printScaling"],"printScaling",[["lock","0"]]],
[["present","@pdf","viewerPreferences","enforce"],"enforce",[["lock","0"]]],
[["present","@pdf","viewerPreferences","numberOfCopies"],"numberOfCopies",[["lock","0"]]],
[["present","@pdf","viewerPreferences","pageRange"],"pageRange",[["lock","0"]]],
[["present","@pdf","viewerPreferences","addViewerPreferences"],"addViewerPreferences",[["lock","0"]]],
[["present","@pdf","viewerPreferences","ADBE_JSConsole"],"ADBE_JSConsole",[["lock","0"]]],
[["present","@pdf","viewerPreferences","duplexOption"],"duplexOption",[["lock","0"]]],
[["present","@pdf","viewerPreferences","ADBE_JSDebugger"],"ADBE_JSDebugger",[["lock","0"]]],
[["present","@pdf","viewerPreferences","pickTrayByPDFSize"],"pickTrayByPDFSize",[["lock","0"]]],
[["present","@pdf","silentPrint","addSilentPrint"],"addSilentPrint",[["lock","0"]]],
[["present","@pdf","silentPrint","printerName"],"printerName",[["lock","0"]]],
[["present","@pdf","compression","level"],"level",[["lock","0"]]],
[["present","@pdf","compression","type"],"type",[["lock","0"]]],
[["present","@pdf","compression","compressObjectStream"],"compressObjectStream",[["lock","0"]]],
[["present","@pdf","compression","compressLogicalStructure"],"compressLogicalStructure",[["lock","0"]]],
[["present","@pdf","encryption","encryptionLevel"],"encryptionLevel",[["lock","0"]]],
[["present","@pdf","encryption","encrypt"],"encrypt",[["lock","0"]]],
[["present","@pdf","encryption","permissions"],"permissions",[["lock","0"]]],
[["present","xdp","packets"],"packets",[["lock","0"]]],
[["present","cache","templateCache"],"templateCache",[["maxEntries","5"],["lock","0"]]],
[["present","output","to"],"to",[["lock","0"]]],
[["present","validate","message"],"message",[["lock","0"]]],
[["present","validate","picture"],"picture",[["lock","0"]]],
[["present","script","exclude"],"exclude",[["lock","0"]]],
[["present","script","currentPage"],"currentPage",[["lock","0"]]],
[["present","script","runScripts"],"runScripts",[["lock","0"]]],
[["present","common","suppressBanner"],"suppressBanner",[["lock","0"]]],
[["present","common","versionControl"],"versionControl",[["sourceBelow","update"],["outputBelow","warn"],["sourceAbove","warn"],["lock","0"]]],
[["present","common","localeSet"],"localeSet",[["lock","0"]]],
[["present","common","template"],"template",[["baseProfile","full"],["lock","0"]]],
[["present","common","validationMessaging"],"validationMessaging",[["lock","0"]]],
[["present","common","locale"],"locale",[["lock","0"]]],
[["present","common","data"],"data",[["lock","0"]]],
[["present","common","messaging"],"messaging",[["lock","0"]]],
[["acrobat","acrobat7","dynamicRender"],"dynamicRender",[["lock","0"]]],
[["present","@pdf","xdc","xsl","debug"],"debug",[["lock","0"]]],
[["present","@pdf","encryption","permissions","modifyAnnots"],"modifyAnnots",[["lock","0"]]],
[["present","@pdf","encryption","permissions","contentCopy"],"contentCopy",[["lock","0"]]],
[["present","@pdf","encryption","permissions","formFieldFilling"],"formFieldFilling",[["lock","0"]]],
[["present","@pdf","encryption","permissions","change"],"change",[["lock","0"]]],
[["present","@pdf","encryption","permissions","accessibleContent"],"accessibleContent",[["lock","0"]]],
[["present","@pdf","encryption","permissions","print"],"print",[["lock","0"]]],
[["present","@pdf","encryption","permissions","plaintextMetadata"],"plaintextMetadata",[["lock","0"]]],
[["present","@pdf","encryption","permissions","printHighQuality"],"printHighQuality",[["lock","0"]]],
[["present","@pdf","encryption","permissions","documentAssembly"],"documentAssembly",[["lock","0"]]],
[["present","validate","message","msgId"],"msgId",[["lock","0"]]],
[["present","validate","message","severity"],"severity",[["lock","0"]]],
[["present","common","template","startPage"],"startPage",[["lock","0"]]],
[["present","common","template","relevant"],"relevant",[["lock","0"]]],
[["present","common","template","base"],"base",[["lock","0"]]],
[["present","common","data","startNode"],"startNode",[["lock","0"]]],
[["present","common","data","outputXSL"],"outputXSL",[["lock","0"]]],
[["present","common","data","adjustData"],"adjustData",[["lock","0"]]],
[["present","common","data","attributes"],"attributes",[["lock","0"]]],
[["present","common","data","window"],"window",[["lock","0"]]],
[["present","common","data","record"],"record",[["lock","0"]]],
[["present","common","data","range"],"range",[["lock","0"]]],
[["present","common","data","incrementalLoad"],"incrementalLoad",[["lock","0"]]]];
    rows.forEach(function(row){try{
      var n=xfa.config;
      row[0].forEach(function(step){n=step.charAt(0)==='@'?n.resolveNode(step.substring(1)):n.getElement(step);yes(n!==null,'property '+step);});
      eq(n.className,row[1]);row[2].forEach(function(attr){eq(n.getAttribute(attr[0]),attr[1]);});
      n.setAttribute('source '+row[1],'schemaProbe');eq(n.getAttribute('schemaProbe'),'source '+row[1]);
      var copy=n.clone(false);eq(copy.className,row[1]);eq(copy.parent,null);
      eq(copy.getAttribute('schemaProbe'),'source '+row[1]);copy.setAttribute('copy only','schemaProbe');
      eq(n.getAttribute('schemaProbe'),'source '+row[1]);eq(copy.getAttribute('schemaProbe'),'copy only');
    }catch(e){throw 'config '+row[0].join('.')+': '+e;}});
    eq(xfa.config.resolveNode('acrobat.acrobat7.dynamicRender').value,'required');
    var interactive=xfa.config.resolveNode('present.pdf.interactive');
    eq(interactive.value,undefined);eq(interactive.nodes.length,1);
    eq(interactive.nodes.item(0).className,'#text');eq(interactive.nodes.item(0).getAttribute('value'),'1');
  )JS");
}

}  // namespace
}  // namespace pdfoxium::integration
