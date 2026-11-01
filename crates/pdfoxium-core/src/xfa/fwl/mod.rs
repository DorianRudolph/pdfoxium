//! Portable FWL widgets use the existing FGAS/FDE graphics and text adapters.
pub mod cfwl_app;
pub mod cfwl_barcode;
pub mod cfwl_caret;
pub mod cfwl_checkbox;
pub mod cfwl_combobox;
pub mod cfwl_comboedit;
pub mod cfwl_combolist;
pub mod cfwl_datetimeedit;
pub mod cfwl_datetimepicker;
pub mod cfwl_edit;
pub mod cfwl_event;
pub mod cfwl_listbox;
pub mod cfwl_message;
pub mod cfwl_monthcalendar;
pub mod cfwl_notedriver;
pub mod cfwl_picturebox;
pub mod cfwl_pushbutton;
pub mod cfwl_scrollbar;
pub mod cfwl_themebackground;
pub mod cfwl_themepart;
pub mod cfwl_themetext;
pub mod cfwl_widget;
pub mod cfwl_widgetmgr;
#[cfg(test)]
mod composite_widget_tests;
pub mod fwl_widgetdef;
pub mod fwl_widgethit;
pub mod ifwl_themeprovider;
pub mod ifwl_widgetdelegate;
#[cfg(test)]
pub(crate) mod test_support;
pub mod theme;
#[cfg(test)]
mod widget_integration_tests;
