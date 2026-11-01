//! XML DOM and parser translated from `core/fxcrt/xml`.
pub mod cfx_xmlchardata;
pub mod cfx_xmldocument;
pub mod cfx_xmlelement;
pub mod cfx_xmlinstruction;
pub mod cfx_xmlnode;
pub mod cfx_xmlparser;
pub mod cfx_xmltext;

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod regression_tests;

pub mod xml_string;
