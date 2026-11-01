// Interfaces and shape algorithms copied from pdfium-render 0.9.4; MIT.
use crate::object_store::ObjectHandle;
use crate::prelude::*;
use pdfoxium_core::fpdfsdk::fpdf_editpath as sdk;
use pdfoxium_core::fxcrt::fx_coordinates::CfxPointF;
use std::marker::PhantomData;
impl<'a> PdfPagePathObject<'a> {
    pub fn new(
        document: &PdfDocument<'a>,
        x: PdfPoints,
        y: PdfPoints,
        stroke_color: Option<PdfColor>,
        stroke_width: Option<PdfPoints>,
        fill_color: Option<PdfColor>,
    ) -> Result<Self, PdfiumError> {
        let core = sdk::create_new_path(x.value, y.value);
        let mut result = Self {
            handle: ObjectHandle::detached(document.inner.inner.clone(), Box::new(core)),
            lifetime: PhantomData,
            current_point_x: x,
            current_point_y: y,
        };
        result.move_to(x, y)?;
        let stroke = stroke_color.is_some() && stroke_width.is_some();
        if stroke {
            result.set_stroke_color(stroke_color.unwrap())?;
            result.set_stroke_width(stroke_width.unwrap())?;
        }
        if let Some(color) = fill_color {
            result.set_fill_color(color)?;
        }
        result.set_fill_and_stroke_mode(
            if fill_color.is_some() {
                PdfPathFillMode::default()
            } else {
                PdfPathFillMode::None
            },
            stroke,
        )?;
        Ok(result)
    }
    pub fn new_line(
        document: &PdfDocument<'a>,
        x1: PdfPoints,
        y1: PdfPoints,
        x2: PdfPoints,
        y2: PdfPoints,
        stroke_color: PdfColor,
        stroke_width: PdfPoints,
    ) -> Result<Self, PdfiumError> {
        let mut result = Self::new(
            document,
            x1,
            y1,
            Some(stroke_color),
            Some(stroke_width),
            None,
        )?;

        result.line_to(x2, y2)?;

        Ok(result)
    }
    pub fn new_bezier(
        document: &PdfDocument<'a>,
        x1: PdfPoints,
        y1: PdfPoints,
        x2: PdfPoints,
        y2: PdfPoints,
        control1_x: PdfPoints,
        control1_y: PdfPoints,
        control2_x: PdfPoints,
        control2_y: PdfPoints,
        stroke_color: PdfColor,
        stroke_width: PdfPoints,
    ) -> Result<Self, PdfiumError> {
        let mut result = Self::new(
            document,
            x1,
            y1,
            Some(stroke_color),
            Some(stroke_width),
            None,
        )?;

        result.bezier_to(x2, y2, control1_x, control1_y, control2_x, control2_y)?;

        Ok(result)
    }
    pub fn new_rect(
        document: &PdfDocument<'a>,
        rect: PdfRect,
        stroke_color: Option<PdfColor>,
        stroke_width: Option<PdfPoints>,
        fill_color: Option<PdfColor>,
    ) -> Result<Self, PdfiumError> {
        let mut result = Self::new(
            document,
            rect.left(),
            rect.bottom(),
            stroke_color,
            stroke_width,
            fill_color,
        )?;

        result.rect_to(rect.right(), rect.top())?;

        Ok(result)
    }
    pub fn new_circle(
        document: &PdfDocument<'a>,
        rect: PdfRect,
        stroke_color: Option<PdfColor>,
        stroke_width: Option<PdfPoints>,
        fill_color: Option<PdfColor>,
    ) -> Result<Self, PdfiumError> {
        let mut result = Self::new(
            document,
            rect.left(),
            rect.bottom(),
            stroke_color,
            stroke_width,
            fill_color,
        )?;

        result.circle_to(rect.right(), rect.top())?;

        Ok(result)
    }
    pub fn new_circle_at(
        document: &PdfDocument<'a>,
        center_x: PdfPoints,
        center_y: PdfPoints,
        radius: PdfPoints,
        stroke_color: Option<PdfColor>,
        stroke_width: Option<PdfPoints>,
        fill_color: Option<PdfColor>,
    ) -> Result<Self, PdfiumError> {
        Self::new_circle(
            document,
            PdfRect::new(
                center_y - radius,
                center_x - radius,
                center_y + radius,
                center_x + radius,
            ),
            stroke_color,
            stroke_width,
            fill_color,
        )
    }
    pub fn new_ellipse(
        document: &PdfDocument<'a>,
        rect: PdfRect,
        stroke_color: Option<PdfColor>,
        stroke_width: Option<PdfPoints>,
        fill_color: Option<PdfColor>,
    ) -> Result<Self, PdfiumError> {
        let mut result = Self::new(
            document,
            rect.left(),
            rect.bottom(),
            stroke_color,
            stroke_width,
            fill_color,
        )?;

        result.ellipse_to(rect.right(), rect.top())?;

        Ok(result)
    }
    pub fn new_ellipse_at(
        document: &PdfDocument<'a>,
        center_x: PdfPoints,
        center_y: PdfPoints,
        x_radius: PdfPoints,
        y_radius: PdfPoints,
        stroke_color: Option<PdfColor>,
        stroke_width: Option<PdfPoints>,
        fill_color: Option<PdfColor>,
    ) -> Result<Self, PdfiumError> {
        Self::new_ellipse(
            document,
            PdfRect::new(
                center_y - y_radius,
                center_x - x_radius,
                center_y + y_radius,
                center_x + x_radius,
            ),
            stroke_color,
            stroke_width,
            fill_color,
        )
    }
    pub fn move_to(&mut self, x: PdfPoints, y: PdfPoints) -> Result<(), PdfiumError> {
        self.handle.write(|o| {
            let p = o
                .as_any_mut()
                .downcast_mut::<pdfoxium_core::fpdfapi::page::cpdf_pathobject::CpdfPathObject>()
                .ok_or(PdfiumError::UnknownPdfPagePathFillMode)?;
            sdk::move_to(p, x.value, y.value);
            Ok(())
        })?;
        self.current_point_x = x;
        self.current_point_y = y;
        Ok(())
    }
    pub fn line_to(&mut self, x: PdfPoints, y: PdfPoints) -> Result<(), PdfiumError> {
        self.handle.write(|o| {
            let p = o
                .as_any_mut()
                .downcast_mut::<pdfoxium_core::fpdfapi::page::cpdf_pathobject::CpdfPathObject>()
                .ok_or(PdfiumError::UnknownPdfPagePathFillMode)?;
            sdk::line_to(p, x.value, y.value);
            Ok(())
        })?;
        self.current_point_x = x;
        self.current_point_y = y;
        Ok(())
    }
    pub fn bezier_to(
        &mut self,
        x: PdfPoints,
        y: PdfPoints,
        control1_x: PdfPoints,
        control1_y: PdfPoints,
        control2_x: PdfPoints,
        control2_y: PdfPoints,
    ) -> Result<(), PdfiumError> {
        self.handle.write(|o| {
            let p = o
                .as_any_mut()
                .downcast_mut::<pdfoxium_core::fpdfapi::page::cpdf_pathobject::CpdfPathObject>()
                .ok_or(PdfiumError::UnknownPdfPagePathFillMode)?;
            sdk::bezier_to(
                p,
                CfxPointF::new(control1_x.value, control1_y.value),
                CfxPointF::new(control2_x.value, control2_y.value),
                CfxPointF::new(x.value, y.value),
            );
            Ok(())
        })?;
        self.current_point_x = x;
        self.current_point_y = y;
        Ok(())
    }
    pub fn rect_to(&mut self, x: PdfPoints, y: PdfPoints) -> Result<(), PdfiumError> {
        let orig_x = self.current_point_x;

        let orig_y = self.current_point_y;

        self.close_path()?;
        self.line_to(orig_x, y)?;
        self.line_to(x, y)?;
        self.line_to(x, orig_y)?;
        self.close_path()?;
        self.move_to(x, y)
    }
    pub fn ellipse_to(&mut self, x: PdfPoints, y: PdfPoints) -> Result<(), PdfiumError> {
        let x_radius = (x - self.current_point_x) / 2.0;

        let y_radius = (y - self.current_point_y) / 2.0;

        self.close_path()?;
        self.move_to(
            self.current_point_x + x_radius,
            self.current_point_y + y_radius,
        )?;
        self.ellipse(x_radius, y_radius)?;
        self.move_to(x, y)
    }
    pub fn circle_to(&mut self, x: PdfPoints, y: PdfPoints) -> Result<(), PdfiumError> {
        let radius = (x - self.current_point_x) / 2.0;

        self.move_to(self.current_point_x + radius, self.current_point_y + radius)?;
        self.ellipse(radius, radius)?;
        self.move_to(x, y)
    }
    pub fn close_path(&mut self) -> Result<(), PdfiumError> {
        self.handle.write(|o| {
            crate::objects::boolean(sdk::close(
                o.as_any_mut()
                    .downcast_mut::<pdfoxium_core::fpdfapi::page::cpdf_pathobject::CpdfPathObject>()
                    .ok_or(PdfiumError::UnknownPdfPagePathFillMode)?,
            ))
        })
    }
    pub fn fill_mode(&self) -> Result<PdfPathFillMode, PdfiumError> {
        self.handle.read(|o| {
            let (mode, _) = sdk::draw_mode(
                o.as_any()
                    .downcast_ref::<pdfoxium_core::fpdfapi::page::cpdf_pathobject::CpdfPathObject>()
                    .ok_or(PdfiumError::UnknownPdfPagePathFillMode)?,
            );
            match mode {
                0 => Ok(PdfPathFillMode::None),
                1 => Ok(PdfPathFillMode::EvenOdd),
                2 => Ok(PdfPathFillMode::Winding),
                _ => Err(PdfiumError::UnknownPdfPagePathFillMode),
            }
        })
    }
    pub fn is_stroked(&self) -> Result<bool, PdfiumError> {
        self.handle.read(|o| {
            Ok(sdk::draw_mode(
                o.as_any()
                    .downcast_ref::<pdfoxium_core::fpdfapi::page::cpdf_pathobject::CpdfPathObject>()
                    .ok_or(PdfiumError::UnknownPdfPagePathFillMode)?,
            )
            .1)
        })
    }
    pub fn set_fill_and_stroke_mode(
        &mut self,
        fill_mode: PdfPathFillMode,
        do_stroke: bool,
    ) -> Result<(), PdfiumError> {
        self.handle.write(|o| {
            sdk::set_draw_mode(
                o.as_any_mut()
                    .downcast_mut::<pdfoxium_core::fpdfapi::page::cpdf_pathobject::CpdfPathObject>()
                    .ok_or(PdfiumError::UnknownPdfPagePathFillMode)?,
                fill_mode as i32,
                do_stroke,
            );
            Ok(())
        })
    }
    pub fn segments(&self) -> PdfPagePathObjectSegments<'_> {
        PdfPagePathObjectSegments {
            handle: self.handle.clone(),
            matrix: None,
            lifetime: PhantomData,
        }
    }
}
impl PdfPagePathObject<'_> {
    fn ellipse(&mut self, x_radius: PdfPoints, y_radius: PdfPoints) -> Result<(), PdfiumError> {
        // Ellipse approximation method: https://spencermortensen.com/articles/bezier-circle/
        // Implementation based on: https://stackoverflow.com/a/2007782

        const C: f32 = 0.551915;

        let x_c = x_radius * C;
        let y_c = y_radius * C;
        let orig_x = self.current_point_x;
        let orig_y = self.current_point_y;

        self.move_to(orig_x - x_radius, orig_y)?;
        self.bezier_to(
            orig_x,
            orig_y + y_radius,
            orig_x - x_radius,
            orig_y + y_c,
            orig_x - x_c,
            orig_y + y_radius,
        )?;
        self.bezier_to(
            orig_x + x_radius,
            orig_y,
            orig_x + x_c,
            orig_y + y_radius,
            orig_x + x_radius,
            orig_y + y_c,
        )?;
        self.bezier_to(
            orig_x,
            orig_y - y_radius,
            orig_x + x_radius,
            orig_y - y_c,
            orig_x + x_c,
            orig_y - y_radius,
        )?;
        self.bezier_to(
            orig_x - x_radius,
            orig_y,
            orig_x - x_c,
            orig_y - y_radius,
            orig_x - x_radius,
            orig_y - y_c,
        )?;
        self.close_path()
    }
}
pub struct PdfPagePathObjectSegments<'a> {
    handle: ObjectHandle,
    matrix: Option<PdfMatrix>,
    lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfPagePathObjectSegments<'a> {
    pub fn transform(&self, matrix: PdfMatrix) -> PdfPagePathObjectSegments<'a> {
        Self {
            handle: self.handle.clone(),
            matrix: Some(matrix),
            lifetime: PhantomData,
        }
    }
    pub fn raw(&self) -> PdfPagePathObjectSegments<'a> {
        Self {
            handle: self.handle.clone(),
            matrix: None,
            lifetime: PhantomData,
        }
    }
}
impl<'a> PdfPathSegments<'a> for PdfPagePathObjectSegments<'a> {
    fn len(&self) -> PdfPathSegmentIndex {
        self.handle.read(|o|Ok(sdk::segments(o.as_any().downcast_ref::<pdfoxium_core::fpdfapi::page::cpdf_pathobject::CpdfPathObject>().ok_or(PdfiumError::UnknownPdfPagePathFillMode)?).len() as u32)).unwrap_or(0)
    }
    fn get(&self, index: PdfPathSegmentIndex) -> Result<PdfPathSegment<'a>, PdfiumError> {
        self.handle.read(|o| {
            let point = *sdk::segments(
                o.as_any()
                    .downcast_ref::<pdfoxium_core::fpdfapi::page::cpdf_pathobject::CpdfPathObject>()
                    .ok_or(PdfiumError::UnknownPdfPagePathFillMode)?,
            )
            .get(index as usize)
            .ok_or(PdfiumError::PdfClipPathSegmentIndexOutOfBounds)?;
            Ok(PdfPathSegment::from_point(point, self.matrix))
        })
    }
    fn iter(&'a self) -> PdfPathSegmentsIterator<'a> {
        PdfPathSegmentsIterator::new(self)
    }
}
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum PdfPathFillMode {
    /// The path will not be filled.
    None = 0 as isize,

    /// The even-odd rule will be used to determine the path region to fill.
    ///
    /// The even-odd rule determines whether a point is inside a path by drawing a ray from that
    /// point in any direction and simply counting the number of path segments that cross the
    /// ray, regardless of direction. If this number is odd, the point is inside; if even, the
    /// point is outside. This yields the same results as the nonzero winding number rule
    /// for paths with simple shapes, but produces different results for more complex shapes.
    ///
    /// More information, including visual examples, can be found in Section 4.4.2 of
    /// the PDF Reference Manual, version 1.7, on page 233.
    EvenOdd = 1 as isize,

    /// The non-zero winding number rule will be used to determine the path region to fill.
    ///
    /// The nonzero winding number rule determines whether a given point is inside a
    /// path by conceptually drawing a ray from that point to infinity in any direction
    /// and then examining the places where a segment of the path crosses the ray. Start-
    /// ing with a count of 0, the rule adds 1 each time a path segment crosses the ray
    /// from left to right and subtracts 1 each time a segment crosses from right to left.
    /// After counting all the crossings, if the result is 0, the point is outside the path;
    /// otherwise, it is inside.
    ///
    /// This is the default fill mode used by `pdfium-render` when creating new [PdfPagePathObject]
    /// instances. The fill mode can be changed on an object-by-object basis by calling the
    /// [PdfPagePathObject::set_fill_and_stroke_mode()] function.
    ///
    /// More information, including visual examples, can be found in Section 4.4.2 of
    /// the PDF Reference Manual, version 1.7, on page 232.
    Winding = 2 as isize,
}
impl Default for PdfPathFillMode {
    fn default() -> Self {
        Self::Winding
    }
}
