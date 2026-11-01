// Interfaces copied from pdfium-render 0.9.4; MIT. See LICENSE.pdfium-render.
use crate::object_store::ObjectHandle;
use crate::{create_transform_getters, create_transform_setters, prelude::*};
use pdfoxium_core::{
    fpdfapi::page::cpdf_pageobject::PageObjectType, fpdfsdk::fpdf_editstate as sdk,
};
use std::marker::PhantomData;
#[derive(Debug, Copy, Clone, PartialOrd, PartialEq, Eq, Hash)]
pub enum PdfPageObjectType {
    /// Any External Object ("XObject") page object type not directly supported by Pdfium.
    Unsupported = 0 as isize,

    /// A page object containing renderable text.
    Text = 1 as isize,

    /// A page object containing a renderable vector path.
    Path = 2 as isize,

    /// A page object containing a renderable bitmapped image.
    Image = 3 as isize,

    /// A page object containing a renderable geometric shape whose color is an arbitrary
    /// function of position within the shape.
    Shading = 4 as isize,

    /// A page object containing a content stream that itself may consist of multiple other page
    /// objects. When this page object is rendered, it renders all its constituent page objects,
    /// effectively serving as a template or stamping object.
    ///
    /// Despite the page object name including "form", this page object type bears no relation
    /// to an interactive form containing form fields.
    XObjectForm = 5 as isize,
}
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum PdfPageObjectBlendMode {
    /// Selects the source color, ignoring the backdrop.
    Normal,

    /// Multiplies the backdrop and source color values. The resulting color is always at least
    /// as dark as either of the two constituent colors. Multiplying any color with black
    /// produces black; multiplying with white leaves the original color unchanged.
    /// Painting successive overlapping objects with a color other than black or white
    /// produces progressively darker colors.
    Multiply,

    /// Multiplies the complements of the backdrop and source color values, then complements
    /// the result.

    /// The result color is always at least as light as either of the two constituent colors.
    /// Screening any color with white produces white; screening with black leaves the original
    /// color unchanged. The effect is similar to projecting multiple photographic slides
    /// simultaneously onto a single screen.
    Screen,

    /// Multiplies or screens the colors, depending on the backdrop color value. Source colors
    /// overlay the backdrop while preserving its highlights and shadows. The backdrop color is
    /// not replaced but is mixed with the source color to reflect the lightness or darkness of
    /// the backdrop.
    Overlay,

    /// Selects the darker of the backdrop and source colors. The backdrop is replaced with the
    /// source where the source is darker; otherwise, it is left unchanged.
    Darken,

    /// Selects the lighter of the backdrop and source colors. The backdrop is replaced with the
    /// source where the source is lighter; otherwise, it is left unchanged.
    Lighten,

    /// Brightens the backdrop color to reflect the source color. Painting with black produces no
    /// changes.
    ColorDodge,

    /// Darkens the backdrop color to reflect the source color. Painting with white produces no
    /// change.
    ColorBurn,

    /// Multiplies or screens the colors, depending on the source color value. The effect is similar
    /// to shining a harsh spotlight on the backdrop.
    HardLight,

    /// Darkens or lightens the colors, depending on the source color value. The effect is similar
    /// to shining a diffused spotlight on the backdrop.
    SoftLight,

    /// Subtracts the darker of the two constituent colors from the lighter color.
    /// Painting with white inverts the backdrop color; painting with black produces no change.
    Difference,

    /// Produces an effect similar to that of the Difference mode but lower in contrast.
    /// Painting with white inverts the backdrop color; painting with black produces no change.
    Exclusion,

    /// Preserves the luminosity of the backdrop color while adopting the hue and saturation
    /// of the source color.
    HSLColor,

    /// Preserves the luminosity and saturation of the backdrop color while adopting the hue
    /// of the source color.
    HSLHue,

    /// Preserves the hue and saturation of the backdrop color while adopting the luminosity
    /// of the source color.
    HSLLuminosity,

    /// Preserves the luminosity and hue of the backdrop color while adopting the saturation
    /// of the source color.
    HSLSaturation,
}
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum PdfPageObjectLineJoin {
    /// The outer edges of the strokes for the two path segments are extended
    /// until they meet at an angle, as in a picture frame. If the segments meet at too
    /// sharp an angle, a bevel join is used instead.
    Miter = 0 as isize,

    /// An arc of a circle with a diameter equal to the line width is drawn
    /// around the point where the two path segments meet, connecting the outer edges of
    /// the strokes for the two segments. This pie-slice-shaped figure is filled in,
    /// producing a rounded corner.
    Round = 1 as isize,

    /// The two path segments are finished with butt caps and the resulting notch
    /// beyond the ends of the segments is filled with a triangle.
    Bevel = 2 as isize,
}
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum PdfPageObjectLineCap {
    /// The stroke is squared off at the endpoint of the path. There is no
    /// projection beyond the end of the path.
    Butt = 0 as isize,

    /// A semicircular arc with a diameter equal to the line width is
    /// drawn around the endpoint and filled in.
    Round = 1 as isize,

    /// The stroke continues beyond the endpoint of the path
    /// for a distance equal to half the line width and is squared off.
    Square = 2 as isize,
}
impl PdfPageObjectBlendMode {
    pub(crate) fn as_pdfium(&self) -> &str {
        match self {
            PdfPageObjectBlendMode::HSLColor => "Color",
            PdfPageObjectBlendMode::ColorBurn => "ColorBurn",
            PdfPageObjectBlendMode::ColorDodge => "ColorDodge",
            PdfPageObjectBlendMode::Darken => "Darken",
            PdfPageObjectBlendMode::Difference => "Difference",
            PdfPageObjectBlendMode::Exclusion => "Exclusion",
            PdfPageObjectBlendMode::HardLight => "HardLight",
            PdfPageObjectBlendMode::HSLHue => "Hue",
            PdfPageObjectBlendMode::Lighten => "Lighten",
            PdfPageObjectBlendMode::HSLLuminosity => "Luminosity",
            PdfPageObjectBlendMode::Multiply => "Multiply",
            PdfPageObjectBlendMode::Normal => "Normal",
            PdfPageObjectBlendMode::Overlay => "Overlay",
            PdfPageObjectBlendMode::HSLSaturation => "Saturation",
            PdfPageObjectBlendMode::Screen => "Screen",
            PdfPageObjectBlendMode::SoftLight => "SoftLight",
        }
    }
}
pub enum PdfPageObject<'a> {
    /// A page object containing renderable text.
    Text(PdfPageTextObject<'a>),

    /// A page object containing a renderable vector path.
    Path(PdfPagePathObject<'a>),

    /// A page object containing a renderable bitmapped image.
    Image(PdfPageImageObject<'a>),

    /// A page object containing a renderable geometric shape whose color is an arbitrary
    /// function of position within the shape.
    Shading(PdfPageShadingObject<'a>),

    /// A page object containing a content stream that itself may consist of multiple other page
    /// objects. When this page object is rendered, it renders all its constituent page objects,
    /// effectively serving as a template or stamping object.
    ///
    /// Despite the page object name including "form", this page object type bears no relation
    /// to an interactive form containing form fields.
    XObjectForm(PdfPageXObjectFormObject<'a>),

    /// Any External Object ("XObject") page object type not directly supported by Pdfium.
    ///
    /// Common properties shared by all [PdfPageObject] types can still be accessed for
    /// page objects not recognized by Pdfium, but object-specific functionality
    /// will be unavailable.
    Unsupported(PdfPageUnsupportedObject<'a>),
}
pub struct PdfPageTextObject<'a> {
    pub(crate) handle: ObjectHandle,
    pub(crate) lifetime: PhantomData<&'a ()>,
}
pub struct PdfPagePathObject<'a> {
    pub(crate) handle: ObjectHandle,
    pub(crate) lifetime: PhantomData<&'a ()>,
    pub(crate) current_point_x: PdfPoints,
    pub(crate) current_point_y: PdfPoints,
}
pub struct PdfPageImageObject<'a> {
    pub(crate) handle: ObjectHandle,
    pub(crate) lifetime: PhantomData<&'a ()>,
}
pub struct PdfPageShadingObject<'a> {
    pub(crate) handle: ObjectHandle,
    pub(crate) lifetime: PhantomData<&'a ()>,
}
pub struct PdfPageXObjectFormObject<'a> {
    pub(crate) collection: crate::object_collections::ObjectCollection,
    pub(crate) handle: ObjectHandle,
    pub(crate) lifetime: PhantomData<&'a ()>,
}
pub struct PdfPageUnsupportedObject<'a> {
    pub(crate) handle: ObjectHandle,
    pub(crate) lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfPageObject<'a> {
    pub(crate) fn from_handle(handle: ObjectHandle) -> Result<Self, PdfiumError> {
        let kind = handle.read(|o| Ok(o.object_type()))?;
        Ok(match kind {
            PageObjectType::Text => Self::Text(PdfPageTextObject {
                handle,
                lifetime: PhantomData,
            }),
            PageObjectType::Path => Self::Path(PdfPagePathObject {
                handle,
                lifetime: PhantomData,
                current_point_x: PdfPoints::ZERO,
                current_point_y: PdfPoints::ZERO,
            }),
            PageObjectType::Image => Self::Image(PdfPageImageObject {
                handle,
                lifetime: PhantomData,
            }),
            PageObjectType::Shading => Self::Shading(PdfPageShadingObject {
                handle,
                lifetime: PhantomData,
            }),
            PageObjectType::Form => Self::XObjectForm(PdfPageXObjectFormObject {
                collection: crate::object_collections::ObjectCollection::Form(handle.clone()),
                handle,
                lifetime: PhantomData,
            }),
        })
    }
    pub(crate) fn handle(&self) -> &ObjectHandle {
        match self {
            Self::Text(o) => &o.handle,
            Self::Path(o) => &o.handle,
            Self::Image(o) => &o.handle,
            Self::Shading(o) => &o.handle,
            Self::XObjectForm(o) => &o.handle,
            Self::Unsupported(o) => &o.handle,
        }
    }
}
impl<'a> PdfPageObject<'a> {
    pub fn object_type(&self) -> PdfPageObjectType {
        match self {
            Self::Text(_) => PdfPageObjectType::Text,
            Self::Path(_) => PdfPageObjectType::Path,
            Self::Image(_) => PdfPageObjectType::Image,
            Self::Shading(_) => PdfPageObjectType::Shading,
            Self::XObjectForm(_) => PdfPageObjectType::XObjectForm,
            Self::Unsupported(_) => PdfPageObjectType::Unsupported,
        }
    }
    pub fn is_supported(&self) -> bool {
        !self.is_unsupported()
    }
    pub fn is_unsupported(&self) -> bool {
        matches!(self, Self::Unsupported(_))
    }
    pub fn as_text_object(&self) -> Option<&PdfPageTextObject<'_>> {
        if let Self::Text(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_text_object_mut(&mut self) -> Option<&mut PdfPageTextObject<'a>> {
        if let Self::Text(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_path_object(&self) -> Option<&PdfPagePathObject<'_>> {
        if let Self::Path(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_path_object_mut(&mut self) -> Option<&mut PdfPagePathObject<'a>> {
        if let Self::Path(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_image_object(&self) -> Option<&PdfPageImageObject<'_>> {
        if let Self::Image(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_image_object_mut(&mut self) -> Option<&mut PdfPageImageObject<'a>> {
        if let Self::Image(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_shading_object(&self) -> Option<&PdfPageShadingObject<'_>> {
        if let Self::Shading(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_shading_object_mut(&mut self) -> Option<&mut PdfPageShadingObject<'a>> {
        if let Self::Shading(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_x_object_form_object(&self) -> Option<&PdfPageXObjectFormObject<'_>> {
        if let Self::XObjectForm(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_x_object_form_object_mut(&mut self) -> Option<&mut PdfPageXObjectFormObject<'a>> {
        if let Self::XObjectForm(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn get_clip_path(&self) -> Option<PdfClipPath<'_>> {
        self.handle()
            .read(|o| Ok(o.base().clip_path().has_ref()))
            .ok()
            .filter(|v| *v)
            .map(|_| PdfClipPath::from_handle(self.handle().clone()))
    }
    pub fn set_active(&mut self) -> Result<(), PdfiumError> {
        self.handle().write(|o| {
            o.base_mut().set_is_active(true);
            Ok(())
        })
    }
    pub fn is_active(&self) -> Result<bool, PdfiumError> {
        self.handle().read(|o| Ok(o.base().is_active()))
    }
    pub fn set_inactive(&mut self) -> Result<(), PdfiumError> {
        self.handle().write(|o| {
            o.base_mut().set_is_active(false);
            Ok(())
        })
    }
    pub fn is_inactive(&self) -> Result<bool, PdfiumError> {
        self.is_active().map(|v| !v)
    }
}
pub trait PdfPageObjectCommon<'a> {
    /// Returns `true` if this [PdfPageObject] contains transparency.
    fn has_transparency(&self) -> bool;

    /// Returns the bounding box of this [PdfPageObject] as a quadrilateral.
    ///
    /// For text objects, the bottom of the bounding box is set to the font baseline. Any characters
    /// in the text object that have glyph shapes that descends below the font baseline will extend
    /// beneath the bottom of this bounding box. To measure the distance of the maximum descent of
    /// any glyphs, use the [PdfPageTextObject::descent()] function.
    fn bounds(&self) -> Result<PdfQuadPoints, PdfiumError>;

    /// Returns the width of this [PdfPageObject].
    #[inline]
    fn width(&self) -> Result<PdfPoints, PdfiumError> {
        Ok(self.bounds()?.width())
    }

    /// Returns the height of this [PdfPageObject].
    #[inline]
    fn height(&self) -> Result<PdfPoints, PdfiumError> {
        Ok(self.bounds()?.height())
    }

    /// Returns `true` if the bounds of this [PdfPageObject] lie entirely within the given rectangle.
    #[inline]
    fn is_inside_rect(&self, rect: &PdfRect) -> bool {
        self.bounds()
            .map(|bounds| bounds.to_rect().is_inside(rect))
            .unwrap_or(false)
    }

    /// Returns `true` if the bounds of this [PdfPageObject] lie at least partially within
    /// the given rectangle.
    #[inline]
    fn does_overlap_rect(&self, rect: &PdfRect) -> bool {
        self.bounds()
            .map(|bounds| bounds.to_rect().does_overlap(rect))
            .unwrap_or(false)
    }

    /// Transforms this [PdfPageObject] by applying the transformation matrix read from the given [PdfPageObject].
    ///
    /// Any translation, rotation, scaling, or skewing transformations currently applied to the
    /// given [PdfPageObject] will be immediately applied to this [PdfPageObject].
    fn transform_from(&mut self, other: &PdfPageObject) -> Result<(), PdfiumError>;

    /// Sets the blend mode that will be applied when painting this [PdfPageObject].
    ///
    /// Note that Pdfium does not currently expose a function to read the currently set blend mode.
    fn set_blend_mode(&mut self, blend_mode: PdfPageObjectBlendMode) -> Result<(), PdfiumError>;

    /// Returns the color of any filled paths in this [PdfPageObject].
    fn fill_color(&self) -> Result<PdfColor, PdfiumError>;

    /// Sets the color of any filled paths in this [PdfPageObject].
    fn set_fill_color(&mut self, fill_color: PdfColor) -> Result<(), PdfiumError>;

    /// Returns the color of any stroked paths in this [PdfPageObject].
    fn stroke_color(&self) -> Result<PdfColor, PdfiumError>;

    /// Sets the color of any stroked paths in this [PdfPageObject].
    ///
    /// Even if this object's path is set with a visible color and a non-zero stroke width,
    /// the object's stroke mode must be set in order for strokes to actually be visible.
    fn set_stroke_color(&mut self, stroke_color: PdfColor) -> Result<(), PdfiumError>;

    /// Returns the width of any stroked lines in this [PdfPageObject].
    fn stroke_width(&self) -> Result<PdfPoints, PdfiumError>;

    /// Sets the width of any stroked lines in this [PdfPageObject].
    ///
    /// A line width of 0 denotes the thinnest line that can be rendered at device resolution:
    /// 1 device pixel wide. However, some devices cannot reproduce 1-pixel lines,
    /// and on high-resolution devices, they are nearly invisible. Since the results of rendering
    /// such zero-width lines are device-dependent, their use is not recommended.
    ///
    /// Even if this object's path is set with a visible color and a non-zero stroke width,
    /// the object's stroke mode must be set in order for strokes to actually be visible.
    fn set_stroke_width(&mut self, stroke_width: PdfPoints) -> Result<(), PdfiumError>;

    /// Returns the line join style that will be used when painting stroked path segments
    /// in this [PdfPageObject].
    fn line_join(&self) -> Result<PdfPageObjectLineJoin, PdfiumError>;

    /// Sets the line join style that will be used when painting stroked path segments
    /// in this [PdfPageObject].
    fn set_line_join(&mut self, line_join: PdfPageObjectLineJoin) -> Result<(), PdfiumError>;

    /// Returns the line cap style that will be used when painting stroked path segments
    /// in this [PdfPageObject].
    fn line_cap(&self) -> Result<PdfPageObjectLineCap, PdfiumError>;

    /// Sets the line cap style that will be used when painting stroked path segments
    /// in this [PdfPageObject].
    fn set_line_cap(&mut self, line_cap: PdfPageObjectLineCap) -> Result<(), PdfiumError>;

    /// Returns the line dash phase that will be used when painting stroked path segments
    /// in this [PdfPageObject].
    ///
    /// A page object's line dash pattern controls the pattern of dashes and gaps used to stroke
    /// paths, as specified by a _dash array_ and a _dash phase_. The dash array's elements are
    /// [PdfPoints] values that specify the lengths of alternating dashes and gaps; all values
    /// must be non-zero and non-negative. The dash phase specifies the distance into the dash pattern
    /// at which to start the dash.
    ///
    /// For more information on stroked dash patterns, refer to the PDF Reference Manual,
    /// version 1.7, pages 217 - 218.
    ///
    /// Note that dash pattern save support in Pdfium was not fully stabilized until release
    /// `chromium/5772` (May 2023). Versions of Pdfium older than this can load and render
    /// dash patterns, but will not save dash patterns to PDF files.
    fn dash_phase(&self) -> Result<PdfPoints, PdfiumError>;

    /// Sets the line dash phase that will be used when painting stroked path segments
    /// in this [PdfPageObject].
    ///
    /// A page object's line dash pattern controls the pattern of dashes and gaps used to stroke
    /// paths, as specified by a _dash array_ and a _dash phase_. The dash array's elements are
    /// [PdfPoints] values that specify the lengths of alternating dashes and gaps; all values
    /// must be non-zero and non-negative. The dash phase specifies the distance into the dash pattern
    /// at which to start the dash.
    ///
    /// For more information on stroked dash patterns, refer to the PDF Reference Manual,
    /// version 1.7, pages 217 - 218.
    ///
    /// Note that dash pattern save support in Pdfium was not fully stabilized until release
    /// `chromium/5772` (May 2023). Versions of Pdfium older than this can load and render
    /// dash patterns, but will not save dash patterns to PDF files.
    fn set_dash_phase(&mut self, dash_phase: PdfPoints) -> Result<(), PdfiumError>;

    /// Returns the line dash array that will be used when painting stroked path segments
    /// in this [PdfPageObject].
    ///
    /// A page object's line dash pattern controls the pattern of dashes and gaps used to stroke
    /// paths, as specified by a _dash array_ and a _dash phase_. The dash array's elements are
    /// [PdfPoints] values that specify the lengths of alternating dashes and gaps; all values
    /// must be non-zero and non-negative. The dash phase specifies the distance into the dash pattern
    /// at which to start the dash.
    ///
    /// For more information on stroked dash patterns, refer to the PDF Reference Manual,
    /// version 1.7, pages 217 - 218.
    ///
    /// Note that dash pattern save support in Pdfium was not fully stabilized until release
    /// `chromium/5772` (May 2023). Versions of Pdfium older than this can load and render
    /// dash patterns, but will not save dash patterns to PDF files.
    fn dash_array(&self) -> Result<Vec<PdfPoints>, PdfiumError>;

    /// Sets the line dash array that will be used when painting stroked path segments
    /// in this [PdfPageObject].
    ///
    /// A page object's line dash pattern controls the pattern of dashes and gaps used to stroke
    /// paths, as specified by a _dash array_ and a _dash phase_. The dash array's elements are
    /// [PdfPoints] values that specify the lengths of alternating dashes and gaps; all values
    /// must be non-zero and non-negative. The dash phase specifies the distance into the dash pattern
    /// at which to start the dash.
    ///
    /// For more information on stroked dash patterns, refer to the PDF Reference Manual,
    /// version 1.7, pages 217 - 218.
    ///
    /// Note that dash pattern save support in Pdfium was not fully stabilized until release
    /// `chromium/5772` (May 2023). Versions of Pdfium older than this can load and render
    /// dash patterns, but will not save dash patterns to PDF files.
    fn set_dash_array(&mut self, array: &[PdfPoints], phase: PdfPoints) -> Result<(), PdfiumError>;

    /// Copies this [PdfPageObject] object into a new [PdfPageXObjectFormObject], then adds
    /// the new form object to the page objects collection of the given [PdfPage],
    /// returning the new form object.
    fn copy_to_page<'b>(
        &mut self,
        page: &mut PdfPage<'b>,
    ) -> Result<PdfPageObject<'b>, PdfiumError>;

    /// Moves the ownership of this [PdfPageObject] to the given [PdfPage], regenerating
    /// page content as necessary.
    ///
    /// An error will be returned if the destination page is in a different [PdfDocument]
    /// than this object. Pdfium only supports safely moving objects within the
    /// same document, not across documents.
    fn move_to_page(&mut self, page: &mut PdfPage) -> Result<(), PdfiumError>;

    /// Moves the ownership of this [PdfPageObject] to the given [PdfPageAnnotation],
    /// regenerating page content as necessary.
    ///
    /// An error will be returned if the destination annotation is in a different [PdfDocument]
    /// than this object. Pdfium only supports safely moving objects within the
    /// same document, not across documents.
    fn move_to_annotation(&mut self, annotation: &mut PdfPageAnnotation)
        -> Result<(), PdfiumError>;
}
mod private {
    pub(crate) trait ObjectOwner {
        fn core_handle(&self) -> &super::ObjectHandle;
    }
}
impl private::ObjectOwner for PdfPageObject<'_> {
    fn core_handle(&self) -> &ObjectHandle {
        self.handle()
    }
}
impl private::ObjectOwner for PdfPageTextObject<'_> {
    fn core_handle(&self) -> &ObjectHandle {
        &self.handle
    }
}
impl private::ObjectOwner for PdfPagePathObject<'_> {
    fn core_handle(&self) -> &ObjectHandle {
        &self.handle
    }
}
impl private::ObjectOwner for PdfPageImageObject<'_> {
    fn core_handle(&self) -> &ObjectHandle {
        &self.handle
    }
}
impl private::ObjectOwner for PdfPageShadingObject<'_> {
    fn core_handle(&self) -> &ObjectHandle {
        &self.handle
    }
}
impl private::ObjectOwner for PdfPageXObjectFormObject<'_> {
    fn core_handle(&self) -> &ObjectHandle {
        &self.handle
    }
}
impl private::ObjectOwner for PdfPageUnsupportedObject<'_> {
    fn core_handle(&self) -> &ObjectHandle {
        &self.handle
    }
}
impl<'a, T: private::ObjectOwner> PdfPageObjectCommon<'a> for T {
    fn has_transparency(&self) -> bool {
        self.core_handle()
            .read(|o| Ok(sdk::has_transparency(o)))
            .unwrap_or(false)
    }
    fn bounds(&self) -> Result<PdfQuadPoints, PdfiumError> {
        self.core_handle().read(|o| {
            if let Some(p) = sdk::rotated_bounds(o) {
                Ok(PdfQuadPoints::new_from_values(
                    p[0].x, p[0].y, p[1].x, p[1].y, p[2].x, p[2].y, p[3].x, p[3].y,
                ))
            } else {
                Ok(PdfRect::from_core(o.base().rect()).to_quad_points())
            }
        })
    }
    fn transform_from(&mut self, other: &PdfPageObject) -> Result<(), PdfiumError> {
        let matrix = other.matrix()?;
        self.core_handle().write(|o| {
            if pdfoxium_core::fpdfsdk::fpdf_editpage::set_object_matrix(o, matrix.as_core()) {
                Ok(())
            } else {
                Err(PdfiumError::InvalidTransformationMatrix)
            }
        })
    }
    fn set_blend_mode(&mut self, blend_mode: PdfPageObjectBlendMode) -> Result<(), PdfiumError> {
        self.core_handle().write(|o| {
            sdk::set_blend_mode(o, blend_mode.as_pdfium().as_bytes());
            Ok(())
        })
    }
    fn fill_color(&self) -> Result<PdfColor, PdfiumError> {
        self.core_handle().read(|o| {
            let c =
                sdk::fill_color(o).ok_or(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
            Ok(PdfColor::new(
                c[0] as u8, c[1] as u8, c[2] as u8, c[3] as u8,
            ))
        })
    }
    fn set_fill_color(&mut self, fill_color: PdfColor) -> Result<(), PdfiumError> {
        self.core_handle()
            .write(|o| boolean(sdk::set_fill_color(o, rgba(fill_color))))
    }
    fn stroke_color(&self) -> Result<PdfColor, PdfiumError> {
        self.core_handle().read(|o| {
            let c = sdk::stroke_color(o)
                .ok_or(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
            Ok(PdfColor::new(
                c[0] as u8, c[1] as u8, c[2] as u8, c[3] as u8,
            ))
        })
    }
    fn set_stroke_color(&mut self, stroke_color: PdfColor) -> Result<(), PdfiumError> {
        self.core_handle()
            .write(|o| boolean(sdk::set_stroke_color(o, rgba(stroke_color))))
    }
    fn stroke_width(&self) -> Result<PdfPoints, PdfiumError> {
        self.core_handle()
            .read(|o| Ok(PdfPoints::new(o.base().graph_state().get_line_width())))
    }
    fn set_stroke_width(&mut self, stroke_width: PdfPoints) -> Result<(), PdfiumError> {
        self.core_handle()
            .write(|o| boolean(sdk::set_stroke_width(o, stroke_width.value)))
    }
    fn line_join(&self) -> Result<PdfPageObjectLineJoin, PdfiumError> {
        self.core_handle()
            .read(|o| match o.base().graph_state().get_line_join().0 as i32 {
                0 => Ok(PdfPageObjectLineJoin::Miter),
                1 => Ok(PdfPageObjectLineJoin::Round),
                2 => Ok(PdfPageObjectLineJoin::Bevel),
                _ => Err(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure),
            })
    }
    fn set_line_join(&mut self, line_join: PdfPageObjectLineJoin) -> Result<(), PdfiumError> {
        self.core_handle()
            .write(|o| boolean(sdk::set_line_join(o, line_join as i32)))
    }
    fn line_cap(&self) -> Result<PdfPageObjectLineCap, PdfiumError> {
        self.core_handle()
            .read(|o| match o.base().graph_state().get_line_cap().0 as i32 {
                0 => Ok(PdfPageObjectLineCap::Butt),
                1 => Ok(PdfPageObjectLineCap::Round),
                2 => Ok(PdfPageObjectLineCap::Square),
                _ => Err(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure),
            })
    }
    fn set_line_cap(&mut self, line_cap: PdfPageObjectLineCap) -> Result<(), PdfiumError> {
        self.core_handle()
            .write(|o| boolean(sdk::set_line_cap(o, line_cap as i32)))
    }
    fn dash_phase(&self) -> Result<PdfPoints, PdfiumError> {
        self.core_handle()
            .read(|o| Ok(PdfPoints::new(o.base().graph_state().get_line_dash_phase())))
    }
    fn set_dash_phase(&mut self, dash_phase: PdfPoints) -> Result<(), PdfiumError> {
        self.core_handle().write(|o| {
            sdk::set_dash_phase(o, dash_phase.value);
            Ok(())
        })
    }
    fn dash_array(&self) -> Result<Vec<PdfPoints>, PdfiumError> {
        self.core_handle().read(|o| {
            Ok(sdk::dash_array(o)
                .iter()
                .map(|&v| PdfPoints::new(v))
                .collect())
        })
    }
    fn set_dash_array(&mut self, array: &[PdfPoints], phase: PdfPoints) -> Result<(), PdfiumError> {
        self.core_handle().write(|o| {
            sdk::set_dash_array(o, array.iter().map(|v| v.value).collect(), phase.value);
            Ok(())
        })
    }
    fn copy_to_page<'b>(
        &mut self,
        page: &mut PdfPage<'b>,
    ) -> Result<PdfPageObject<'b>, PdfiumError> {
        let handle = self.core_handle().copy_to(page.inner.document.clone())?;
        handle.attach_to_page(&page.inner, None)?;
        PdfPageObject::from_handle(handle)
    }
    fn move_to_page(&mut self, page: &mut PdfPage) -> Result<(), PdfiumError> {
        self.core_handle().attach_to_page(&page.inner, None)
    }
    fn move_to_annotation(
        &mut self,
        annotation: &mut PdfPageAnnotation,
    ) -> Result<(), PdfiumError> {
        let (page, dict, context) = annotation.core_object_target()?;
        self.core_handle()
            .attach_to_annotation(&page, dict, context)
    }
}
impl<'a> PdfPageObject<'a> {
    create_transform_setters!(&mut Self,Result<(),PdfiumError>,"this object","this object.","this object,");
    create_transform_getters!("this object", "this object.", "this object,");
    fn transform_impl(
        &mut self,
        a: PdfMatrixValue,
        b: PdfMatrixValue,
        c: PdfMatrixValue,
        d: PdfMatrixValue,
        e: PdfMatrixValue,
        f: PdfMatrixValue,
    ) -> Result<(), PdfiumError> {
        let matrix = PdfMatrix::new(a, b, c, d, e, f);
        if ![a, b, c, d, e, f].iter().all(|v| v.is_finite()) {
            return Err(PdfiumError::InvalidTransformationMatrix);
        }
        (self.handle()).write(|o| {
            o.transform(matrix.as_core());
            Ok(())
        })
    }
    fn reset_matrix_impl(&mut self, matrix: PdfMatrix) -> Result<(), PdfiumError> {
        (self.handle()).write(|o| {
            boolean(pdfoxium_core::fpdfsdk::fpdf_editpage::set_object_matrix(
                o,
                matrix.as_core(),
            ))
        })
    }
    fn get_matrix_impl(&self) -> Result<PdfMatrix, PdfiumError> {
        (self.handle()).read(|o| {
            let m = sdk::object_matrix(o).ok_or(PdfiumError::InvalidTransformationMatrix)?;
            Ok(PdfMatrix::new(m.a, m.b, m.c, m.d, m.e, m.f))
        })
    }
}
impl<'a> PdfPageTextObject<'a> {
    create_transform_setters!(&mut Self,Result<(),PdfiumError>,"this object","this object.","this object,");
    create_transform_getters!("this object", "this object.", "this object,");
    fn transform_impl(
        &mut self,
        a: PdfMatrixValue,
        b: PdfMatrixValue,
        c: PdfMatrixValue,
        d: PdfMatrixValue,
        e: PdfMatrixValue,
        f: PdfMatrixValue,
    ) -> Result<(), PdfiumError> {
        let matrix = PdfMatrix::new(a, b, c, d, e, f);
        if ![a, b, c, d, e, f].iter().all(|v| v.is_finite()) {
            return Err(PdfiumError::InvalidTransformationMatrix);
        }
        (&self.handle).write(|o| {
            o.transform(matrix.as_core());
            Ok(())
        })
    }
    fn reset_matrix_impl(&mut self, matrix: PdfMatrix) -> Result<(), PdfiumError> {
        (&self.handle).write(|o| {
            boolean(pdfoxium_core::fpdfsdk::fpdf_editpage::set_object_matrix(
                o,
                matrix.as_core(),
            ))
        })
    }
    fn get_matrix_impl(&self) -> Result<PdfMatrix, PdfiumError> {
        (&self.handle).read(|o| {
            let m = sdk::object_matrix(o).ok_or(PdfiumError::InvalidTransformationMatrix)?;
            Ok(PdfMatrix::new(m.a, m.b, m.c, m.d, m.e, m.f))
        })
    }
}
impl<'a> PdfPagePathObject<'a> {
    create_transform_setters!(&mut Self,Result<(),PdfiumError>,"this object","this object.","this object,");
    create_transform_getters!("this object", "this object.", "this object,");
    fn transform_impl(
        &mut self,
        a: PdfMatrixValue,
        b: PdfMatrixValue,
        c: PdfMatrixValue,
        d: PdfMatrixValue,
        e: PdfMatrixValue,
        f: PdfMatrixValue,
    ) -> Result<(), PdfiumError> {
        let matrix = PdfMatrix::new(a, b, c, d, e, f);
        if ![a, b, c, d, e, f].iter().all(|v| v.is_finite()) {
            return Err(PdfiumError::InvalidTransformationMatrix);
        }
        (&self.handle).write(|o| {
            o.transform(matrix.as_core());
            Ok(())
        })
    }
    fn reset_matrix_impl(&mut self, matrix: PdfMatrix) -> Result<(), PdfiumError> {
        (&self.handle).write(|o| {
            boolean(pdfoxium_core::fpdfsdk::fpdf_editpage::set_object_matrix(
                o,
                matrix.as_core(),
            ))
        })
    }
    fn get_matrix_impl(&self) -> Result<PdfMatrix, PdfiumError> {
        (&self.handle).read(|o| {
            let m = sdk::object_matrix(o).ok_or(PdfiumError::InvalidTransformationMatrix)?;
            Ok(PdfMatrix::new(m.a, m.b, m.c, m.d, m.e, m.f))
        })
    }
}
impl<'a> PdfPageImageObject<'a> {
    create_transform_setters!(&mut Self,Result<(),PdfiumError>,"this object","this object.","this object,");
    create_transform_getters!("this object", "this object.", "this object,");
    fn transform_impl(
        &mut self,
        a: PdfMatrixValue,
        b: PdfMatrixValue,
        c: PdfMatrixValue,
        d: PdfMatrixValue,
        e: PdfMatrixValue,
        f: PdfMatrixValue,
    ) -> Result<(), PdfiumError> {
        let matrix = PdfMatrix::new(a, b, c, d, e, f);
        if ![a, b, c, d, e, f].iter().all(|v| v.is_finite()) {
            return Err(PdfiumError::InvalidTransformationMatrix);
        }
        (&self.handle).write(|o| {
            o.transform(matrix.as_core());
            Ok(())
        })
    }
    fn reset_matrix_impl(&mut self, matrix: PdfMatrix) -> Result<(), PdfiumError> {
        (&self.handle).write(|o| {
            boolean(pdfoxium_core::fpdfsdk::fpdf_editpage::set_object_matrix(
                o,
                matrix.as_core(),
            ))
        })
    }
    fn get_matrix_impl(&self) -> Result<PdfMatrix, PdfiumError> {
        (&self.handle).read(|o| {
            let m = sdk::object_matrix(o).ok_or(PdfiumError::InvalidTransformationMatrix)?;
            Ok(PdfMatrix::new(m.a, m.b, m.c, m.d, m.e, m.f))
        })
    }
}
impl<'a> PdfPageShadingObject<'a> {
    create_transform_setters!(&mut Self,Result<(),PdfiumError>,"this object","this object.","this object,");
    create_transform_getters!("this object", "this object.", "this object,");
    fn transform_impl(
        &mut self,
        a: PdfMatrixValue,
        b: PdfMatrixValue,
        c: PdfMatrixValue,
        d: PdfMatrixValue,
        e: PdfMatrixValue,
        f: PdfMatrixValue,
    ) -> Result<(), PdfiumError> {
        let matrix = PdfMatrix::new(a, b, c, d, e, f);
        if ![a, b, c, d, e, f].iter().all(|v| v.is_finite()) {
            return Err(PdfiumError::InvalidTransformationMatrix);
        }
        (&self.handle).write(|o| {
            o.transform(matrix.as_core());
            Ok(())
        })
    }
    fn reset_matrix_impl(&mut self, matrix: PdfMatrix) -> Result<(), PdfiumError> {
        (&self.handle).write(|o| {
            boolean(pdfoxium_core::fpdfsdk::fpdf_editpage::set_object_matrix(
                o,
                matrix.as_core(),
            ))
        })
    }
    fn get_matrix_impl(&self) -> Result<PdfMatrix, PdfiumError> {
        (&self.handle).read(|o| {
            let m = sdk::object_matrix(o).ok_or(PdfiumError::InvalidTransformationMatrix)?;
            Ok(PdfMatrix::new(m.a, m.b, m.c, m.d, m.e, m.f))
        })
    }
}
impl<'a> PdfPageXObjectFormObject<'a> {
    create_transform_setters!(&mut Self,Result<(),PdfiumError>,"this object","this object.","this object,");
    create_transform_getters!("this object", "this object.", "this object,");
    fn transform_impl(
        &mut self,
        a: PdfMatrixValue,
        b: PdfMatrixValue,
        c: PdfMatrixValue,
        d: PdfMatrixValue,
        e: PdfMatrixValue,
        f: PdfMatrixValue,
    ) -> Result<(), PdfiumError> {
        let matrix = PdfMatrix::new(a, b, c, d, e, f);
        if ![a, b, c, d, e, f].iter().all(|v| v.is_finite()) {
            return Err(PdfiumError::InvalidTransformationMatrix);
        }
        (&self.handle).write(|o| {
            o.transform(matrix.as_core());
            Ok(())
        })
    }
    fn reset_matrix_impl(&mut self, matrix: PdfMatrix) -> Result<(), PdfiumError> {
        (&self.handle).write(|o| {
            boolean(pdfoxium_core::fpdfsdk::fpdf_editpage::set_object_matrix(
                o,
                matrix.as_core(),
            ))
        })
    }
    fn get_matrix_impl(&self) -> Result<PdfMatrix, PdfiumError> {
        (&self.handle).read(|o| {
            let m = sdk::object_matrix(o).ok_or(PdfiumError::InvalidTransformationMatrix)?;
            Ok(PdfMatrix::new(m.a, m.b, m.c, m.d, m.e, m.f))
        })
    }
}
impl<'a> PdfPageUnsupportedObject<'a> {
    create_transform_setters!(&mut Self,Result<(),PdfiumError>,"this object","this object.","this object,");
    create_transform_getters!("this object", "this object.", "this object,");
    fn transform_impl(
        &mut self,
        a: PdfMatrixValue,
        b: PdfMatrixValue,
        c: PdfMatrixValue,
        d: PdfMatrixValue,
        e: PdfMatrixValue,
        f: PdfMatrixValue,
    ) -> Result<(), PdfiumError> {
        let matrix = PdfMatrix::new(a, b, c, d, e, f);
        if ![a, b, c, d, e, f].iter().all(|v| v.is_finite()) {
            return Err(PdfiumError::InvalidTransformationMatrix);
        }
        (&self.handle).write(|o| {
            o.transform(matrix.as_core());
            Ok(())
        })
    }
    fn reset_matrix_impl(&mut self, matrix: PdfMatrix) -> Result<(), PdfiumError> {
        (&self.handle).write(|o| {
            boolean(pdfoxium_core::fpdfsdk::fpdf_editpage::set_object_matrix(
                o,
                matrix.as_core(),
            ))
        })
    }
    fn get_matrix_impl(&self) -> Result<PdfMatrix, PdfiumError> {
        (&self.handle).read(|o| {
            let m = sdk::object_matrix(o).ok_or(PdfiumError::InvalidTransformationMatrix)?;
            Ok(PdfMatrix::new(m.a, m.b, m.c, m.d, m.e, m.f))
        })
    }
}
impl<'a> From<PdfPageTextObject<'a>> for PdfPageObject<'a> {
    fn from(value: PdfPageTextObject<'a>) -> Self {
        Self::Text(value)
    }
}
impl<'a> From<PdfPagePathObject<'a>> for PdfPageObject<'a> {
    fn from(value: PdfPagePathObject<'a>) -> Self {
        Self::Path(value)
    }
}
impl<'a> From<PdfPageImageObject<'a>> for PdfPageObject<'a> {
    fn from(value: PdfPageImageObject<'a>) -> Self {
        Self::Image(value)
    }
}
impl<'a> From<PdfPageShadingObject<'a>> for PdfPageObject<'a> {
    fn from(value: PdfPageShadingObject<'a>) -> Self {
        Self::Shading(value)
    }
}
impl<'a> From<PdfPageXObjectFormObject<'a>> for PdfPageObject<'a> {
    fn from(value: PdfPageXObjectFormObject<'a>) -> Self {
        Self::XObjectForm(value)
    }
}
impl<'a> From<PdfPageUnsupportedObject<'a>> for PdfPageObject<'a> {
    fn from(value: PdfPageUnsupportedObject<'a>) -> Self {
        Self::Unsupported(value)
    }
}
pub(crate) fn rgba(c: PdfColor) -> [u32; 4] {
    [
        c.red().into(),
        c.green().into(),
        c.blue().into(),
        c.alpha().into(),
    ]
}
pub(crate) fn boolean(value: bool) -> Result<(), PdfiumError> {
    if value {
        Ok(())
    } else {
        Err(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)
    }
}

/// A retained marked-content item. Upstream exposes no public constructor or operations.
pub struct PdfPageObjectMark<'a> {
    _mark: pdfoxium_core::fpdfapi::page::cpdf_contentmarks::MarkItemRef,
    _lifetime: std::marker::PhantomData<&'a ()>,
}
