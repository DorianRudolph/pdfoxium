//! Direct FormCalc lexer, parser and JavaScript emitter. Executing the emitted
//! XFA runtime calls requires the still-open FormCalc/XFA host bindings.
pub mod cxfa_fmexpression;
pub mod cxfa_fmlexer;
pub mod cxfa_fmparser;
pub mod cxfa_fmtojavascriptdepth;

pub fn translate(input: impl Into<std::rc::Rc<[u32]>>) -> Option<Vec<u32>> {
    let mut parser = cxfa_fmparser::CxfaFmParser::new(cxfa_fmlexer::CxfaFmLexer::new(input));
    parser.parse()?.to_javascript()
}
