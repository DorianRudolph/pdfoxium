// Interfaces copied from pdfium-render 0.9.4; MIT.
use crate::{object_store::ObjectHandle, prelude::*};
#[cfg(feature = "image_api")]
use image::DynamicImage;
use pdfoxium_core::{
    fpdfapi::page::cpdf_imageobject::CpdfImageObject, fpdfsdk::fpdf_editimg as sdk,
    fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream,
};
use std::{
    io::{Read, Seek},
    marker::PhantomData,
    ops::{Range, RangeInclusive},
    path::Path,
    rc::Rc,
};
impl PdfPageImageObject<'_> {
    fn metadata(&self) -> Result<sdk::ImageMetadata, PdfiumError> {
        self.handle.read(|o| {
            let image = o
                .as_any()
                .downcast_ref::<CpdfImageObject>()
                .ok_or(PdfiumError::UnknownPdfPageObjectType)?;
            let page = self.handle.owner_page();
            let page = page.as_ref().map(|p| p.state.core.borrow());
            sdk::metadata(image, page.as_deref().map(|p| p.as_ref()))
                .ok_or(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)
        })
    }
    fn get_current_width_and_height_from_metadata(&self) -> Result<(Pixels, Pixels), PdfiumError> {
        let m = self.metadata()?;
        let (x, y) = m.dpi.unwrap_or((0., 0.));
        if x == 0. || y == 0. {
            return Err(PdfiumError::ImageSizeOutOfBounds);
        }
        Ok((
            (m.width as f32 * 72. / x) as Pixels,
            (m.height as f32 * 72. / y) as Pixels,
        ))
    }
}
impl<'a> PdfPageImageObject<'a> {
    pub fn new(document: &PdfDocument<'a>) -> Result<Self, PdfiumError> {
        let core = sdk::new_image_object(&document.inner.inner.core.borrow().page_data_ref());
        Ok(Self {
            handle: ObjectHandle::detached(document.inner.inner.clone(), Box::new(core)),
            lifetime: PhantomData,
        })
    }
    pub fn new_from_jpeg_file(
        document: &PdfDocument<'a>,
        path: &(impl AsRef<Path> + ?Sized),
    ) -> Result<Self, PdfiumError> {
        Self::new_from_jpeg_reader(
            document,
            std::fs::File::open(path).map_err(PdfiumError::IoError)?,
        )
    }
    pub fn new_from_jpeg_reader<R: Read + Seek>(
        document: &PdfDocument<'a>,
        reader: R,
    ) -> Result<Self, PdfiumError> {
        let object = Self::new(document)?;
        let mut reader = reader;
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(PdfiumError::IoError)?;
        object.handle.write(|o| {
            let image = o
                .as_any_mut()
                .downcast_mut::<CpdfImageObject>()
                .and_then(|o| o.image())
                .ok_or(PdfiumError::UnknownPdfPageObjectType)?;
            crate::objects::boolean(
                image.set_jpeg_image(Rc::new(CfxReadOnlyContainerStream::new(bytes)), true),
            )
        })?;
        Ok(object)
    }
    #[cfg(feature = "image_api")]
    pub fn new_with_width(
        document: &PdfDocument<'a>,
        image: &DynamicImage,
        width: PdfPoints,
    ) -> Result<Self, PdfiumError> {
        let aspect_ratio = image.height() as f32 / image.width() as f32;

        let height = width * aspect_ratio;

        Self::new_with_size(document, image, width, height)
    }
    #[cfg(feature = "image_api")]
    pub fn new_with_height(
        document: &'a PdfDocument<'a>,
        image: &DynamicImage,
        height: PdfPoints,
    ) -> Result<Self, PdfiumError> {
        let aspect_ratio = image.height() as f32 / image.width() as f32;

        let width = height / aspect_ratio;

        Self::new_with_size(document, image, width, height)
    }
    #[cfg(feature = "image_api")]
    pub fn new_with_size(
        document: &PdfDocument<'a>,
        image: &DynamicImage,
        width: PdfPoints,
        height: PdfPoints,
    ) -> Result<Self, PdfiumError> {
        let mut result = Self::new(document)?;
        result.set_image(image)?;

        result.scale(width.value, height.value)?;

        Ok(result)
    }
    pub fn get_raw_bitmap(&self) -> Result<PdfBitmap<'_>, PdfiumError> {
        self.handle.read(|o| {
            let image = o
                .as_any()
                .downcast_ref::<CpdfImageObject>()
                .and_then(|o| o.image())
                .ok_or(PdfiumError::UnknownPdfPageObjectType)?;
            let bitmap = sdk::get_bitmap(&image)
                .ok_or(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
            PdfBitmap::from_core(bitmap)
        })
    }
    #[cfg(feature = "image_api")]
    pub fn get_raw_image(&self) -> Result<DynamicImage, PdfiumError> {
        self.get_image_from_bitmap(&self.get_raw_bitmap()?)
    }
    pub fn get_processed_bitmap(
        &self,
        document: &PdfDocument,
    ) -> Result<PdfBitmap<'_>, PdfiumError> {
        let (width, height) = self.get_current_width_and_height_from_metadata()?;
        self.get_processed_bitmap_with_size(document, width, height)
    }
    #[cfg(feature = "image_api")]
    pub fn get_processed_image(&self, document: &PdfDocument) -> Result<DynamicImage, PdfiumError> {
        let (width, height) = self.get_current_width_and_height_from_metadata()?;

        self.get_processed_image_with_size(document, width, height)
    }
    pub fn get_processed_bitmap_with_width(
        &self,
        document: &PdfDocument,
        width: Pixels,
    ) -> Result<PdfBitmap<'_>, PdfiumError> {
        let (current_width, current_height) = self.get_current_width_and_height_from_metadata()?;

        let aspect_ratio = current_width as f32 / current_height as f32;

        self.get_processed_bitmap_with_size(
            document,
            width,
            ((width as f32 / aspect_ratio) as u32)
                .try_into()
                .map_err(|_| PdfiumError::ImageSizeOutOfBounds)?,
        )
    }
    #[cfg(feature = "image_api")]
    pub fn get_processed_image_with_width(
        &self,
        document: &PdfDocument,
        width: Pixels,
    ) -> Result<DynamicImage, PdfiumError> {
        let (current_width, current_height) = self.get_current_width_and_height_from_metadata()?;

        let aspect_ratio = current_width as f32 / current_height as f32;

        self.get_processed_image_with_size(
            document,
            width,
            ((width as f32 / aspect_ratio) as u32)
                .try_into()
                .map_err(|_| PdfiumError::ImageSizeOutOfBounds)?,
        )
    }
    pub fn get_processed_bitmap_with_height(
        &self,
        document: &PdfDocument,
        height: Pixels,
    ) -> Result<PdfBitmap<'_>, PdfiumError> {
        let (current_width, current_height) = self.get_current_width_and_height_from_metadata()?;

        let aspect_ratio = current_width as f32 / current_height as f32;

        self.get_processed_bitmap_with_size(
            document,
            ((height as f32 * aspect_ratio) as u32)
                .try_into()
                .map_err(|_| PdfiumError::ImageSizeOutOfBounds)?,
            height,
        )
    }
    #[cfg(feature = "image_api")]
    pub fn get_processed_image_with_height(
        &self,
        document: &PdfDocument,
        height: Pixels,
    ) -> Result<DynamicImage, PdfiumError> {
        let (current_width, current_height) = self.get_current_width_and_height_from_metadata()?;

        let aspect_ratio = current_width as f32 / current_height as f32;

        self.get_processed_image_with_size(
            document,
            ((height as f32 * aspect_ratio) as u32)
                .try_into()
                .map_err(|_| PdfiumError::ImageSizeOutOfBounds)?,
            height,
        )
    }
    pub fn get_processed_bitmap_with_size(
        &self,
        document: &PdfDocument,
        width: Pixels,
        height: Pixels,
    ) -> Result<PdfBitmap<'_>, PdfiumError> {
        if width <= 0 || height <= 0 {
            return Err(PdfiumError::ImageSizeOutOfBounds);
        }
        if !Rc::ptr_eq(&document.inner.inner, &self.handle.document()) {
            return Err(PdfiumError::CannotMoveObjectAcrossDocuments);
        }
        self.handle.read(|o| {
            let o = o
                .as_any()
                .downcast_ref::<CpdfImageObject>()
                .ok_or(PdfiumError::UnknownPdfPageObjectType)?;
            let data = document.inner.inner.core.borrow().page_data_ref();
            let mut temporary = sdk::new_image_object(&data);
            temporary.set_image(o.image());
            let mut matrix = o.matrix();
            let w = matrix.a.hypot(matrix.c);
            let h = matrix.b.hypot(matrix.d);
            if w == 0. || h == 0. {
                return Err(PdfiumError::ImageSizeOutOfBounds);
            }
            matrix.a *= width as f32 / w;
            matrix.c *= width as f32 / w;
            matrix.b *= height as f32 / h;
            matrix.d *= height as f32 / h;
            temporary.set_image_matrix(matrix);
            let bitmap = sdk::rendered_bitmap(&data, None, &temporary)
                .ok_or(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
            PdfBitmap::from_core(bitmap)
        })
    }
    #[cfg(feature = "image_api")]
    pub fn get_processed_image_with_size(
        &self,
        document: &PdfDocument,
        width: Pixels,
        height: Pixels,
    ) -> Result<DynamicImage, PdfiumError> {
        self.get_processed_bitmap_with_size(document, width, height)
            .and_then(|bitmap| self.get_image_from_bitmap(&bitmap))
    }
    pub fn get_raw_image_data(&self) -> Result<Vec<u8>, PdfiumError> {
        self.handle.read(|o| {
            let image = o
                .as_any()
                .downcast_ref::<CpdfImageObject>()
                .and_then(|o| o.image())
                .ok_or(PdfiumError::UnknownPdfPageObjectType)?;
            Ok(sdk::with_image_data(&image, false, |bytes| bytes.to_vec()).unwrap_or_default())
        })
    }
    pub fn width(&self) -> Result<Pixels, PdfiumError> {
        self.metadata()?
            .width
            .try_into()
            .map_err(|_| PdfiumError::ImageSizeOutOfBounds)
    }
    pub fn height(&self) -> Result<Pixels, PdfiumError> {
        self.metadata()?
            .height
            .try_into()
            .map_err(|_| PdfiumError::ImageSizeOutOfBounds)
    }
    #[cfg(feature = "image_api")]
    pub fn set_image(&mut self, image: &DynamicImage) -> Result<(), PdfiumError> {
        let rgba = image.to_rgba8();
        let mut bitmap = PdfBitmap::empty(
            image
                .width()
                .try_into()
                .map_err(|_| PdfiumError::ImageSizeOutOfBounds)?,
            image
                .height()
                .try_into()
                .map_err(|_| PdfiumError::ImageSizeOutOfBounds)?,
            PdfBitmapFormat::BGRA,
        )?;
        for (dest, src) in bitmap
            .as_raw_bytes_mut()
            .chunks_exact_mut(4)
            .zip(rgba.as_raw().chunks_exact(4))
        {
            dest.copy_from_slice(&[src[2], src[1], src[0], src[3]]);
        }
        self.set_bitmap(&bitmap)
    }
    pub fn set_bitmap(&mut self, bitmap: &PdfBitmap) -> Result<(), PdfiumError> {
        self.handle.write(|o| {
            crate::objects::boolean(sdk::set_bitmap(
                o.as_any_mut()
                    .downcast_mut::<CpdfImageObject>()
                    .ok_or(PdfiumError::UnknownPdfPageObjectType)?,
                bitmap,
            ))
        })
    }
    pub fn horizontal_dpi(&self) -> Result<f32, PdfiumError> {
        Ok(self.metadata()?.dpi.map_or(0., |p| p.0))
    }
    pub fn vertical_dpi(&self) -> Result<f32, PdfiumError> {
        Ok(self.metadata()?.dpi.map_or(0., |p| p.1))
    }
    pub fn bits_per_pixel(&self) -> Result<u8, PdfiumError> {
        self.metadata()?
            .bits_per_pixel
            .try_into()
            .map_err(|_| PdfiumError::ImageSizeOutOfBounds)
    }
    pub fn color_space(&self) -> Result<PdfColorSpace, PdfiumError> {
        PdfColorSpace::from_core(self.metadata()?.colorspace)
    }
    pub fn filters(&self) -> PdfPageImageObjectFilters<'_> {
        PdfPageImageObjectFilters { object: self }
    }
}
pub type PdfPageImageObjectFilterIndex = usize;
pub struct PdfPageImageObjectFilters<'a> {
    object: &'a PdfPageImageObject<'a>,
}
impl<'a> PdfPageImageObjectFilters<'a> {
    pub fn len(&self) -> usize {
        self.object
            .handle
            .read(|o| {
                Ok(o.as_any()
                    .downcast_ref::<CpdfImageObject>()
                    .and_then(|o| o.image())
                    .map_or(0, |i| sdk::filter_count(&i)))
            })
            .unwrap_or(0)
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn as_range(&self) -> Range<PdfPageImageObjectFilterIndex> {
        0..self.len()
    }
    pub fn as_range_inclusive(&self) -> RangeInclusive<PdfPageImageObjectFilterIndex> {
        if self.is_empty() {
            0..=0
        } else {
            0..=(self.len() - 1)
        }
    }
    pub fn get(
        &self,
        index: PdfPageImageObjectFilterIndex,
    ) -> Result<PdfPageImageObjectFilter, PdfiumError> {
        if index >= self.len() {
            return Err(PdfiumError::ImageObjectFilterIndexOutOfBounds);
        }
        self.object.handle.read(|o| {
            let image = o
                .as_any()
                .downcast_ref::<CpdfImageObject>()
                .and_then(|o| o.image())
                .ok_or(PdfiumError::UnknownPdfPageObjectType)?;
            let bytes = sdk::filter(&image, index)
                .ok_or(PdfiumError::ImageObjectFilterIndexInBoundsButFilterUndefined)?;
            Ok(PdfPageImageObjectFilter {
                name: String::from_utf8(bytes)
                    .map(|s| s.trim_end_matches(char::from(0)).to_owned())
                    .unwrap_or_default(),
            })
        })
    }
    pub fn iter(&self) -> PdfPageImageObjectFiltersIterator<'_> {
        PdfPageImageObjectFiltersIterator {
            filters: self,
            next_index: 0,
        }
    }
}
pub struct PdfPageImageObjectFilter {
    name: String,
}
impl PdfPageImageObjectFilter {
    pub fn name(&self) -> &str {
        &self.name
    }
}
pub struct PdfPageImageObjectFiltersIterator<'a> {
    filters: &'a PdfPageImageObjectFilters<'a>,
    next_index: usize,
}
impl<'a> Iterator for PdfPageImageObjectFiltersIterator<'a> {
    type Item = PdfPageImageObjectFilter;
    fn next(&mut self) -> Option<Self::Item> {
        if self.next_index >= self.filters.len() {
            return None;
        }
        let i = self.next_index;
        self.next_index += 1;
        self.filters.get(i).ok()
    }
}
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum PdfColorSpace {
    /// An unknown or unset color space. Color spaces were added to the PDF file format
    /// gradually from versions 1.1 to versions 1.3.
    Unknown = 0 as isize,

    /// Black, white, and intermediate shades of gray are special cases of full color.
    /// A grayscale value is represented by a single number in the range `0.0..=1.0`, where
    /// 0.0 corresponds to black, 1.0 to white, and intermediate values to different gray levels.
    ///
    /// This is a non-calibrated color space; the exact color produced for a particular set of
    /// component values may vary slightly from device to device.
    DeviceGray = 1 as isize,

    /// Colors in this color space are specified according to the additive Red-Green-Blue color
    /// model used by light-emitting displays and projectors. Color values are defined by three
    /// components representing the intensities of the additive primary colorants red, green,
    /// and blue. Each component is specified by a number in the range `0.0..=1.0`, where
    /// 0.0 corresponds to a complete absence of the component and 1.0 corresponds to maximum
    /// intensity of the component. If all three components have equal intensity, the perceived
    /// result theoretically is a pure gray on the scale from black to white. If the intensities
    /// are not all equal, the result is some color other than a pure gray.
    ///
    /// This is a non-calibrated color space; the exact color produced for a particular set of
    /// component values may vary slightly from device to device.
    DeviceRGB = 2 as isize,

    /// Colors in this color space are specified according to the subtractive Cyan-Magenta-Yellow-Black
    /// model typical of printers and other paper-based output devices. In theory, each of the three
    /// standard process colorants used in printing (cyan, magenta, and yellow) absorbs one of the
    /// additive primary colors (red, green, and blue, respectively). Black, a fourth standard process
    /// colorant, absorbs all of the additive primaries in equal amounts. Color values are defined
    /// by four components representing the concentrations of these process colorants. Each component
    /// is specified by a number in the range `0.0..=1.0`, where 0.0 denotes the complete absence of
    /// a process colorant (that is, absorbs none of the corresponding additive primary) and 1.0
    /// denotes maximum concentration (absorbs as much as possible of the additive primary). Note
    /// that the sense of these numbers is opposite to that of RGB color components.
    ///
    /// This is a non-calibrated color space; the exact color produced for a particular set of
    /// component values may vary slightly from device to device.
    DeviceCMYK = 3 as isize,

    /// Colors in this color space are specified by a single component, arbitrarily named A,
    /// that represents the gray component of a calibrated gray color space.
    ///
    /// This is a calibrated color space, based on the tristimulus components of the CIE 1931 XYZ
    /// color space. The three components of the color space are defined in terms of human
    /// color vision and are independent of any particular output device.
    CalibratedCIEGray = 4 as isize,

    /// Colors in this color space are specified by three components, arbitrarily named A, B, and C,
    /// representing calibrated red, green, and blue color values. These three color components must
    /// be in the range `0.0..=1.0`.
    ///
    /// This is a calibrated color space, based on the tristimulus components of the CIE 1931 XYZ
    /// color space. The three components of the color space are defined in terms of human
    /// color vision and are independent of any particular output device.
    CalibratedCIERGB = 5 as isize,

    /// Colors in this color space are specified by three components, named L*, a*, and b*,
    /// of a CIE 1976 L\*a\*b color space. The range of the first (L*) component is always 0 to 100;
    /// the ranges of the second (a*) and third (b*) components are defined by the color space.
    ///
    /// This is a calibrated color space; the three components of the color space are defined in
    /// terms of human color vision and are independent of any particular output device.
    CalibratedCIELab = 6 as isize,

    /// Colors in this color space are based on a cross-platform color profile defined by
    /// the International Color Consortium (ICC).
    ///
    /// This is a calibrated color space; colors are defined by international standards
    /// and are independent of any particular output device.
    CalibratedICCProfile = 7 as isize,

    /// Some output devices, such as image-setters, produce a separate, monochromatic rendition of
    /// a page - a _separation_ - for each colorant. When the separations are later combined - on a
    /// printing press, for example - with proper inks or other colorants added to them, the result
    /// is a full-color page.
    ///
    /// This special color space provides a means for specifying the use of additional colorants,
    /// called a _tint_, or for isolating the control of individual color components of a device
    /// color space for a subtractive device.
    Separation = 8 as isize,

    /// Colors in this special color space can contain an arbitrary number of color components.
    /// This provides greater flexibility than is possible with standard device color spaces
    /// or with individual separation color spaces. DeviceN color spaces are used in applications
    /// such as high-fidelity color (such as the Pantone Hexachrome system), multi-tone
    /// color systems (such as duotone), and spot color systems (using subtractive colorants outside
    /// the standard Cyan-Magenta-Yellow-Black model).
    DeviceN = 9 as isize,

    /// This special color space indicates an object or area should be painted according to
    /// color values stored in a lookup table rather than a color space.
    Indexed = 10 as isize,

    /// This special color space indicates an object or area should be painted using a pattern,
    /// rather than a single color.
    Pattern = 11 as isize,
}
impl PdfColorSpace {
    fn from_core(value: i32) -> Result<Self, PdfiumError> {
        match value {
            0 => Ok(Self::Unknown),
            1 => Ok(Self::DeviceGray),
            2 => Ok(Self::DeviceRGB),
            3 => Ok(Self::DeviceCMYK),
            4 => Ok(Self::CalibratedCIEGray),
            5 => Ok(Self::CalibratedCIERGB),
            6 => Ok(Self::CalibratedCIELab),
            7 => Ok(Self::CalibratedICCProfile),
            8 => Ok(Self::Separation),
            9 => Ok(Self::DeviceN),
            10 => Ok(Self::Indexed),
            11 => Ok(Self::Pattern),
            _ => Err(PdfiumError::UnknownPdfColorSpace),
        }
    }
}

#[cfg(feature = "image_api")]
impl PdfPageImageObject<'_> {
    fn get_image_from_bitmap(&self, bitmap: &PdfBitmap) -> Result<DynamicImage, PdfiumError> {
        bitmap.as_image()
    }
}
