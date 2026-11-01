// Public interfaces copied from pdfium-render 0.9.4; MIT. See LICENSE.pdfium-render.
use crate::backend::document::DocumentState;
use crate::prelude::*;
use pdfoxium_core::fpdfdoc::{
    cpdf_action::{ActionType, CpdfAction},
    cpdf_dest::CpdfDest,
};
use std::{marker::PhantomData, rc::Rc};
const PDFACTION_UNSUPPORTED: isize = 0;
const PDFACTION_GOTO: isize = 1;
const PDFACTION_REMOTEGOTO: isize = 2;
const PDFACTION_URI: isize = 3;
const PDFACTION_LAUNCH: isize = 4;
const PDFACTION_EMBEDDEDGOTO: isize = 5;
#[derive(Debug, Copy, Clone, PartialOrd, PartialEq)]
pub enum PdfActionType {
    GoToDestinationInSameDocument = PDFACTION_GOTO as isize,
    GoToDestinationInRemoteDocument = PDFACTION_REMOTEGOTO as isize,
    GoToDestinationInEmbeddedDocument = PDFACTION_EMBEDDEDGOTO as isize,
    Launch = PDFACTION_LAUNCH as isize,
    Uri = PDFACTION_URI as isize,
    Unsupported = PDFACTION_UNSUPPORTED as isize,
}
pub enum PdfAction<'a> {
    LocalDestination(PdfActionLocalDestination<'a>),
    RemoteDestination(PdfActionRemoteDestination<'a>),
    EmbeddedDestination(PdfActionEmbeddedDestination<'a>),
    Launch(PdfActionLaunch<'a>),
    Uri(PdfActionUri<'a>),
    Unsupported(PdfActionUnsupported<'a>),
}
pub struct PdfActionLocalDestination<'a> {
    document: Rc<DocumentState>,
    core: CpdfAction,
    lifetime: PhantomData<&'a ()>,
}
pub struct PdfActionRemoteDestination<'a> {
    _document: Rc<DocumentState>,
    _core: CpdfAction,
    lifetime: PhantomData<&'a ()>,
}
pub struct PdfActionEmbeddedDestination<'a> {
    _document: Rc<DocumentState>,
    _core: CpdfAction,
    lifetime: PhantomData<&'a ()>,
}
pub struct PdfActionLaunch<'a> {
    _document: Rc<DocumentState>,
    _core: CpdfAction,
    lifetime: PhantomData<&'a ()>,
}
pub struct PdfActionUri<'a> {
    document: Rc<DocumentState>,
    core: CpdfAction,
    lifetime: PhantomData<&'a ()>,
}
pub struct PdfActionUnsupported<'a> {
    _document: Rc<DocumentState>,
    _core: CpdfAction,
    lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfAction<'a> {
    pub(crate) fn from_core(document: Rc<DocumentState>, core: CpdfAction) -> Option<Self> {
        if !core.has_dict() {
            return None;
        }
        Some(match core.action_type() {
            ActionType::GoTo => Self::LocalDestination(PdfActionLocalDestination {
                document,
                core,
                lifetime: PhantomData,
            }),
            ActionType::GoToR => Self::RemoteDestination(PdfActionRemoteDestination {
                _document: document,
                _core: core,
                lifetime: PhantomData,
            }),
            ActionType::GoToE => Self::EmbeddedDestination(PdfActionEmbeddedDestination {
                _document: document,
                _core: core,
                lifetime: PhantomData,
            }),
            ActionType::Launch => Self::Launch(PdfActionLaunch {
                _document: document,
                _core: core,
                lifetime: PhantomData,
            }),
            ActionType::Uri => Self::Uri(PdfActionUri {
                document,
                core,
                lifetime: PhantomData,
            }),
            _ => Self::Unsupported(PdfActionUnsupported {
                _document: document,
                _core: core,
                lifetime: PhantomData,
            }),
        })
    }
}
impl<'a> PdfAction<'a> {
    pub fn action_type(&self) -> PdfActionType {
        match self {
            Self::LocalDestination(_) => PdfActionType::GoToDestinationInSameDocument,
            Self::RemoteDestination(_) => PdfActionType::GoToDestinationInRemoteDocument,
            Self::EmbeddedDestination(_) => PdfActionType::GoToDestinationInEmbeddedDocument,
            Self::Launch(_) => PdfActionType::Launch,
            Self::Uri(_) => PdfActionType::Uri,
            Self::Unsupported(_) => PdfActionType::Unsupported,
        }
    }
    pub fn is_supported(&self) -> bool {
        !self.is_unsupported()
    }
    pub fn is_unsupported(&self) -> bool {
        matches!(self, Self::Unsupported(_))
    }
    pub fn as_local_destination_action(&self) -> Option<&PdfActionLocalDestination<'_>> {
        if let Self::LocalDestination(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_local_destination_action_mut(
        &mut self,
    ) -> Option<&mut PdfActionLocalDestination<'a>> {
        if let Self::LocalDestination(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_remote_destination_action(&self) -> Option<&PdfActionRemoteDestination<'_>> {
        if let Self::RemoteDestination(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_remote_destination_action_mut(
        &mut self,
    ) -> Option<&mut PdfActionRemoteDestination<'a>> {
        if let Self::RemoteDestination(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_embedded_destination_action(&self) -> Option<&PdfActionEmbeddedDestination<'_>> {
        if let Self::EmbeddedDestination(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_embedded_destination_action_mut(
        &mut self,
    ) -> Option<&mut PdfActionEmbeddedDestination<'a>> {
        if let Self::EmbeddedDestination(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_launch_action(&self) -> Option<&PdfActionLaunch<'_>> {
        if let Self::Launch(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_launch_action_mut(&mut self) -> Option<&mut PdfActionLaunch<'a>> {
        if let Self::Launch(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_uri_action(&self) -> Option<&PdfActionUri<'_>> {
        if let Self::Uri(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_uri_action_mut(&mut self) -> Option<&mut PdfActionUri<'a>> {
        if let Self::Uri(value) = self {
            Some(value)
        } else {
            None
        }
    }
}
impl<'a> PdfActionLocalDestination<'a> {
    pub fn destination(&self) -> Result<PdfDestination<'_>, PdfiumError> {
        let dest = self.core.dest(&self.document.core.borrow());
        PdfDestination::from_core(self.document.clone(), dest)
            .ok_or(PdfiumError::DestinationPageIndexNotAvailable)
    }
}
impl<'a> PdfActionUri<'a> {
    pub fn uri(&self) -> Result<String, PdfiumError> {
        let value = self.core.uri(&self.document.core.borrow());
        if value.is_empty() {
            return Err(PdfiumError::NoUriForAction);
        }
        String::from_utf8(value).map_err(|_| PdfiumError::NoUriForAction)
    }
}
#[derive(Debug, Copy, Clone)]
pub enum PdfDestinationViewSettings {
    /// The view settings are unknown.
    Unknown,

    /// Display the target `PdfPage` with the given (x, y) coordinates positioned at the
    /// upper-left corner of the window and the contents of the page magnified by the given
    /// zoom factor. A missing value for any of the parameters indicates that the current value
    /// of that parameter is to be retained unchanged.
    SpecificCoordinatesAndZoom(Option<PdfPoints>, Option<PdfPoints>, Option<f32>),

    /// Display the target `PdfPage` with its contents magnified just enough
    /// to fit the entire page within the window both horizontally and vertically. If
    /// the required horizontal and vertical magnification factors are different, use
    /// the smaller of the two, centering the page within the window in the other
    /// dimension.
    FitPageToWindow,

    /// Display the target `PdfPage` with the given y coordinate positioned at the top edge of
    /// the window and the contents of the page magnified just enough to fit the entire width
    /// of the page within the window. A missing value for the y coordinate indicates
    /// that the current value of that parameter is to be retained unchanged.
    FitPageHorizontallyToWindow(Option<PdfPoints>),

    /// Display the target `PdfPage` with the given x coordinate positioned at the left edge of
    /// the window and the contents of the page magnified just enough to fit the entire height
    /// of the page within the window. A missing value for the x coordinate indicates
    /// that the current value of that parameter is to be retained unchanged.
    FitPageVerticallyToWindow(Option<PdfPoints>),

    /// Display the target `PdfPage` with its contents magnified just enough to fit the
    /// given rectangle entirely within the window both horizontally and vertically.
    /// If the required horizontal and vertical magnification factors are different, then use
    /// the smaller of the two, centering the rectangle within the window in the other dimension.
    FitPageToRectangle(PdfRect),

    /// Display the target `PdfPage` with its contents magnified just enough to fit its bounding box
    /// entirely within the window both horizontally and vertically. If the required horizontal and
    /// vertical magnification factors are different, then use the smaller of the two,
    /// centering the bounding box within the window in the other dimension.
    ///
    /// This variant was added in PDF version 1.1.
    FitBoundsToWindow,

    /// Display the target `PdfPage` with the given y coordinate positioned at the top edge of
    /// the window and the contents of the page magnified just enough to fit the entire width
    /// of its bounding box within the window. A missing value for the y coordinate indicates
    /// that the current value of that parameter is to be retained unchanged.
    ///
    /// This variant was added in PDF version 1.1.
    FitBoundsHorizontallyToWindow(Option<PdfPoints>),

    /// Display the target `PdfPage` with the given x coordinate positioned at the left edge of
    /// the window and the contents of the page magnified just enough to fit the entire height
    /// of its bounding box within the window. A missing value for the x coordinate indicates
    /// that the current value of that parameter is to be retained unchanged.
    ///
    /// This variant was added in PDF version 1.1.
    FitBoundsVerticallyToWindow(Option<PdfPoints>),
}
pub struct PdfDestination<'a> {
    document: Rc<DocumentState>,
    core: CpdfDest,
    lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfDestination<'a> {
    pub(crate) fn from_core(document: Rc<DocumentState>, core: CpdfDest) -> Option<Self> {
        core.array()?;
        Some(Self {
            document,
            core,
            lifetime: PhantomData,
        })
    }
}
impl<'a> PdfDestination<'a> {
    pub fn page_index(&self) -> Result<PdfPageIndex, PdfiumError> {
        let index = self
            .core
            .dest_page_index(&mut self.document.core.borrow_mut());
        if index < 0 {
            Err(PdfiumError::DestinationPageIndexNotAvailable)
        } else {
            Ok(index)
        }
    }
    pub fn view_settings(&self) -> Result<PdfDestinationViewSettings, PdfiumError> {
        let count = self.core.num_params();
        let point = |i| Some(PdfPoints::new(self.core.param(i)));
        use PdfDestinationViewSettings::*;
        match self.core.zoom_mode() {
            0 => Ok(Unknown),
            1 if count == 3 => {
                let xyz = self
                    .core
                    .xyz()
                    .ok_or(PdfiumError::DestinationPageLocationNotAvailable)?;
                Ok(SpecificCoordinatesAndZoom(
                    xyz.x.map(PdfPoints::new),
                    xyz.y.map(PdfPoints::new),
                    xyz.zoom,
                ))
            }
            2 if count == 0 => Ok(FitPageToWindow),
            3 if count == 0 => Ok(FitPageHorizontallyToWindow(None)),
            3 if count == 1 => Ok(FitPageHorizontallyToWindow(point(0))),
            4 if count == 0 => Ok(FitPageVerticallyToWindow(None)),
            4 if count == 1 => Ok(FitPageVerticallyToWindow(point(0))),
            5 if count == 4 => Ok(FitPageToRectangle(PdfRect::new_from_values(
                self.core.param(1),
                self.core.param(0),
                self.core.param(3),
                self.core.param(2),
            ))),
            6 if count == 0 => Ok(FitBoundsToWindow),
            7 if count == 0 => Ok(FitBoundsHorizontallyToWindow(None)),
            7 if count == 1 => Ok(FitBoundsHorizontallyToWindow(point(0))),
            8 if count == 0 => Ok(FitBoundsVerticallyToWindow(None)),
            8 if count == 1 => Ok(FitBoundsVerticallyToWindow(point(0))),
            1..=8 => Err(PdfiumError::PdfDestinationViewInvalidParameters),
            _ => Err(PdfiumError::UnknownPdfDestinationViewType),
        }
    }
}

pub trait PdfActionCommon<'a> {}
impl<'a> PdfActionCommon<'a> for PdfAction<'a> {}
impl<'a> PdfActionCommon<'a> for PdfActionLocalDestination<'a> {}
impl<'a> PdfActionCommon<'a> for PdfActionRemoteDestination<'a> {}
impl<'a> PdfActionCommon<'a> for PdfActionEmbeddedDestination<'a> {}
impl<'a> PdfActionCommon<'a> for PdfActionLaunch<'a> {}
impl<'a> PdfActionCommon<'a> for PdfActionUri<'a> {}
impl<'a> PdfActionCommon<'a> for PdfActionUnsupported<'a> {}
