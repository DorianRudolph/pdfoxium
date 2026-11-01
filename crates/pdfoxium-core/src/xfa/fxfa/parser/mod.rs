//! XFA schema, XML packet construction and node data-model foundations.
//! Includes prototype/data merging, bound values, executable model bindings and
//! XML export. Layout and the remaining document-host integration stay open.
pub mod cxfa_localemgr;
pub mod cxfa_localevalue;
pub mod cxfa_xmllocale;
mod locale_data;

pub mod cxfa_arraynodelist;
pub mod cxfa_attachnodelist;
pub mod cxfa_data_binding;
pub mod cxfa_dataexporter;
pub mod cxfa_document;
pub mod cxfa_document_builder;
pub mod cxfa_document_datamerge;
pub mod cxfa_document_merge;
pub mod cxfa_list;
pub mod cxfa_measurement;
pub mod cxfa_model_properties;
#[cfg(test)]
mod cxfa_model_tests;
pub mod cxfa_node;
pub mod cxfa_node_locale_access;
pub mod cxfa_nodeiteratortemplate;
pub mod cxfa_nodelocale;
pub mod cxfa_nodeowner;
pub mod cxfa_traversestrategy_xfacontainernode;
pub mod cxfa_traversestrategy_xfanode;
pub mod cxfa_treelist;
pub mod node_schema;
pub mod xfa_basic_data;
pub mod xfa_document_datamerger_imp;
pub mod xfa_utils;

pub mod cxfa_field_properties;
pub(crate) mod cxfa_geometry;
pub mod cxfa_instancemanager;
pub mod cxfa_presentation;

mod cxfa_leaf_properties;

pub mod cxfa_node_check;
mod cxfa_node_widget;
