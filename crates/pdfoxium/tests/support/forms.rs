// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
pub fn fixture() -> Vec<u8> {
    fixture_config(false)
}
pub fn without_default_resources() -> Vec<u8> {
    fixture_config_with_resources(false, false)
}
pub fn advanced() -> Vec<u8> {
    fixture_config(true)
}
pub fn scripted() -> Vec<u8> {
    scripted_with_scripts("var boot = 42;", "throw new Error('boom');")
}
pub fn scripted_with_scripts(boot: &str, bad: &str) -> Vec<u8> {
    let number = js("AFNumber_Format(2,0,0,0,'$',true);");
    let key = js("AFNumber_Keystroke(2,0);");
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /Names << /JavaScript << /Names [(boot) 14 0 R] >> >> /OpenAction 15 0 R /AA << /WS 16 0 R /DS 17 0 R >> >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Resources << /Font << /Helv 5 0 R >> >> /Annots [6 0 R 7 0 R 8 0 R 9 0 R 10 0 R 11 0 R 12 0 R 13 0 R] /AA << /O {} /C {} >> >>", js("this.getField('startup').value += ' open';"), js("this.getField('startup').value += ' close';")),
        "<< /Fields [6 0 R 7 0 R 8 0 R 9 0 R 10 0 R 11 0 R 12 0 R 13 0 R] /CO [8 0 R] /NeedAppearances true /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
    ];
    for field in [
        format!("/FT /Tx /T (amount) /Rect [20 320 180 350] /V (1234.5) /AA << /K {key} /F {number} >>"),
        "/FT /Tx /T (qty) /Rect [20 260 180 290] /V (2)".into(),
        format!("/FT /Tx /T (total) /Rect [20 200 180 230] /V (2469) /Ff 1 /AA << /C {} /F {number} >>", js("event.value = Number(this.getField('amount').value) * Number(this.getField('qty').value);")),
        format!("/FT /Btn /Ff 65536 /T (alert) /Rect [220 320 380 350] /MK << /CA (Alert) >> /A {}", js("this.getField('qty').value = app.alert('Hello',3,1,'Question');")),
        format!("/FT /Btn /Ff 65536 /T (prompt) /Rect [220 260 380 290] /MK << /CA (Prompt) >> /A {}", js("this.getField('qty').value = app.response('How many?','Quantity','2');")),
        format!("/FT /Btn /Ff 65536 /T (bad) /Rect [220 200 380 230] /MK << /CA (Throw) >> /A {}", js(bad)),
        "/FT /Tx /T (startup) /Rect [20 140 180 170] /V ()".into(),
        format!("/FT /Tx /T (validated) /Rect [20 80 180 110] /V (keep) /AA << /V {} >>", js("event.rc = event.value !== 'reject';")),
    ] {
        objects.push(format!("<< /Type /Annot /Subtype /Widget /P 3 0 R /F 4 /BS << /W 1 /S /S >> /MK << /BC [0 0 0] /BG [1 1 1] >> {field} >>"));
    }
    objects.extend([
        js(boot),
        js("this.getField('startup').value = 'ready ' + boot;"),
        js("this.getField('startup').value += ' will-save';"),
        js("this.getField('startup').value += ' did-save';"),
    ]);
    encode_pdf(objects)
}
fn literal(value: &str) -> String {
    format!(
        "({})",
        value
            .replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)")
    )
}
fn js(value: &str) -> String {
    format!("<< /S /JavaScript /JS {} >>", literal(value))
}
pub fn animation() -> Vec<u8> {
    let mark = |name: &str| {
        js(&format!(
            "order+='|{name}';this.getField('log').value=order;"
        ))
    };
    let init = js("order+='|widget-open';this.getField('log').value=order;var doc=this;var ticks=0;var timer;function advance(){ticks++;doc.getField('f0').display=(ticks%2)?display.hidden:display.visible;doc.getField('f1').display=(ticks%2)?display.visible:display.hidden;doc.dirty=false;}");
    let stop = js("try{app.clearInterval(timer);}catch(e){}order+='|widget-invisible';this.getField('log').value=order;");
    let mut objects=vec![
        "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Resources << /Font << /Helv 5 0 R >> >> /Annots [6 0 R 7 0 R 8 0 R 9 0 R 10 0 R] /AA << /O {} /C {} >> >>",js("var order='page-open';this.getField('log').value=order;"),mark("page-close")),
        "<< /Fields [7 0 R 8 0 R 9 0 R 10 0 R] /NeedAppearances false /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        format!("<< /Type /Annot /Subtype /Screen /F 2 /Rect [20 200 25 205] /AA << /PO {} /PV {} /PI {} /PC {} >> >>",mark("screen-open"),mark("screen-visible"),mark("screen-invisible"),mark("screen-close")),
        "<< /Type /Annot /Subtype /Widget /F 4 /FT /Btn /Ff 65536 /T (f0) /Rect [20 200 100 280] /AP << /N 11 0 R >> >>".into(),
        "<< /Type /Annot /Subtype /Widget /F 2 /FT /Btn /Ff 65536 /T (f1) /Rect [20 200 100 280] /AP << /N 12 0 R >> >>".into(),
        format!("<< /Type /Annot /Subtype /Widget /F 4 /FT /Btn /Ff 65536 /T (play) /Rect [120 200 200 230] /MK << /CA (Play) >> /AA << /PO {init} /PV {} /PI {stop} /PC {} /U {} >> >>",mark("widget-visible"),mark("widget-close"),js("timer=app.setInterval('advance()',10);")),
        "<< /Type /Annot /Subtype /Widget /F 4 /FT /Tx /T (log) /Rect [20 100 380 130] /V () >>".into(),
    ];
    for color in ["1 0 0", "0 0 1"] {
        let content = format!("{color} rg 0 0 80 80 re f");
        objects.push(format!("<< /Type /XObject /Subtype /Form /BBox [0 0 80 80] /Length {} >>\nstream\n{content}\nendstream",content.len()));
    }
    encode_pdf(objects)
}
fn fixture_config(advanced: bool) -> Vec<u8> {
    fixture_config_with_resources(advanced, true)
}
fn fixture_config_with_resources(advanced: bool, with_default_resources: bool) -> Vec<u8> {
    let mut objects = vec![String::new(); 5];
    let mut fields = vec![
        "/FT /Tx /T (name) /Rect [20 250 180 280] /V (Initial)",
        "/FT /Tx /T (second) /Rect [20 200 180 230] /V ()",
        "/FT /Btn /T (check) /Rect [20 150 50 180] /V /Off /AS /Off /MK << /CA (4) >>",
        "/FT /Ch /T (choice) /Rect [20 90 180 130] /Ff 393216 /Opt [(Red) (Green) (Blue)] /V (Red)",
        "/FT /Tx /T (readonly) /Rect [20 40 180 70] /Ff 1 /V (Fixed)",
    ];
    if advanced {
        fields.extend([
            "/FT /Tx /T (multiline) /Rect [220 20 380 120] /Ff 4096 /V (First)",
            "/FT /Ch /T (list) /Rect [220 210 380 300] /Opt [(One) (Two) (Three)] /V (One) /I [0]",
            "/Rect [230 145 250 165]",
            "/Rect [290 145 310 165]",
        ]);
    }
    let mut annots = String::new();
    for field in fields {
        let id = objects.len() + 1;
        annots.push_str(&format!("{id} 0 R "));
        objects.push(format!("<< /Type /Annot /Subtype /Widget /P 3 0 R /F 4 /BS << /W 1 /S /S >> /MK << /BC [0 0 0] /BG [1 1 1] >> {field} >>"));
    }
    let off = objects.len() + 1;
    let blank = "1 g 0 0 30 30 re f 0 G 1 w";
    objects.push(format!("<< /Type /XObject /Subtype /Form /BBox [0 0 30 30] /Length {} >>\nstream\n{blank}\nendstream", blank.len()));
    let on = objects.len() + 1;
    let checked = "1 g 0 0 30 30 re f 0 g 7 7 16 16 re f";
    objects.push(format!("<< /Type /XObject /Subtype /Form /BBox [0 0 30 30] /Length {} >>\nstream\n{checked}\nendstream",checked.len()));
    objects[7]=format!("<< /Type /Annot /Subtype /Widget /P 3 0 R /F 4 /FT /Btn /T (check) /Rect [20 150 50 180] /V /Off /AS /Off /AP << /N << /Off {off} 0 R /Yes {on} 0 R >> >> >>");
    let fields_refs = if advanced {
        let parent = objects.len() + 1;
        objects.push(format!(
            "<< /FT /Btn /Ff 32768 /T (radio) /V /A /Kids [13 0 R 14 0 R] >>"
        ));
        for (index, name) in [(12, "A"), (13, "B")] {
            let state = if name == "A" { "A" } else { "Off" };
            let len = objects[index].len();
            objects[index].truncate(len - 3);
            objects[index].push_str(&format!(" /Parent {parent} 0 R /AS /{state} /AP << /N << /Off {off} 0 R /{name} {on} 0 R >> >> >>"));
        }
        format!("6 0 R 7 0 R 8 0 R 9 0 R 10 0 R 11 0 R 12 0 R {parent} 0 R")
    } else {
        annots.clone()
    };
    objects[0] = "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>".into();
    objects[1] = "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into();
    objects[2]=format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Resources << /Font << /Helv 5 0 R >> >> /Annots [{annots}] >>");
    if advanced {
        objects[2] = format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /CropBox [10 10 390 390] /Rotate 90 /Resources << /Font << /Helv 5 0 R >> >> /Annots [{annots}] >>");
    }
    let resources = if with_default_resources {
        "/DR << /Font << /Helv 5 0 R >> >>"
    } else {
        ""
    };
    objects[3] = format!(
        "<< /Fields [{fields_refs}] /NeedAppearances true /DA (/Helv 12 Tf 0 g) {resources} >>"
    );
    objects[4] =
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into();
    encode_pdf(objects)
}
fn encode_pdf(objects: Vec<String>) -> Vec<u8> {
    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{obj}\nendobj\n", i + 1).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}
