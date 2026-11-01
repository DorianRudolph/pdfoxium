#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Eq)]
pub enum PdfFormFieldType {
    Unknown,
    PushButton,
    Checkbox,
    RadioButton,
    ComboBox,
    ListBox,
    Text,
    Signature,
}
impl PdfFormFieldType {
    pub(crate) fn as_core(self) -> u32 {
        match self {
            Self::Unknown => 0,
            Self::PushButton => 1,
            Self::Checkbox => 2,
            Self::RadioButton => 3,
            Self::ComboBox => 4,
            Self::ListBox => 5,
            Self::Text => 6,
            Self::Signature => 7,
        }
    }
}

#[cfg(all(test, feature = "image_api"))]
mod flags {
    #[derive(Clone, Copy)]
    pub(crate) struct PdfAnnotationFlags(pub(crate) u32);
    #[allow(non_upper_case_globals)]
    impl PdfAnnotationFlags {
        pub const Invisible: Self = Self(1);
        pub const Hidden: Self = Self(2);
        pub const Print: Self = Self(4);
        pub const ReadOnly: Self = Self(64);
        pub const Locked: Self = Self(128);
        pub const LockedContents: Self = Self(512);
        pub fn contains(self, flag: Self) -> bool {
            self.0 & flag.0 != 0
        }
        pub fn set(&mut self, flag: Self, value: bool) {
            if value {
                self.0 |= flag.0
            } else {
                self.0 &= !flag.0
            }
        }
    }
}
#[cfg(all(test, feature = "image_api"))]
pub(crate) use flags::PdfAnnotationFlags;
