//! Safe engine-facing portions of the PDFium SDK; raw public handles live in
//! pdfoxium-capi.
pub mod cpdfsdk_annot;
pub mod cpdfsdk_annotiteration;
pub mod cpdfsdk_annotiterator;
pub mod cpdfsdk_appstream;
pub mod cpdfsdk_baannot;
pub mod cpdfsdk_formfillenvironment;
pub mod cpdfsdk_helpers;
pub mod cpdfsdk_interactiveform;
pub mod cpdfsdk_pageview;
pub mod cpdfsdk_widget;
pub mod formfiller;
pub mod fpdf_annot;
pub mod fpdf_attachment;
pub mod fpdf_editimg;
pub mod fpdf_editmark;
pub mod fpdf_editpage;
pub mod fpdf_editpath;
pub mod fpdf_editpattern;
pub mod fpdf_editstate;
pub mod fpdf_edittext;
pub mod fpdf_flatten;
pub mod fpdf_fwlevent;
pub mod fpdf_ppo;
pub mod fpdf_transformpage;
pub mod fpdf_view;
pub mod pwl;

pub mod fpdf_catalog;
pub mod fpdf_signature;
pub mod fpdf_thumbnail;

pub mod cpdfsdk_unsupported;
#[cfg(feature = "xfa")]
pub mod fpdfxfa;

pub mod cpdfsdk_pauseadapter;
pub mod cpdfsdk_renderpage;

#[cfg(feature = "xfa")]
pub mod fpdf_xfa_save;

#[cfg(feature = "xfa")]
pub mod cpdfsdk_widget_xfa;
