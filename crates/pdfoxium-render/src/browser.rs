// Browser interfaces copied from pdfium-render 0.9.4; MIT.
use crate::prelude::*;
use js_sys::{Array, ArrayBuffer, Uint8Array};
use wasm_bindgen::{Clamped, JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{window, Blob, ImageData, Response};
impl Pdfium {
    pub async fn load_pdf_from_fetch<'a>(
        &'a self,
        url: impl ToString,
        password: Option<&str>,
    ) -> Result<PdfDocument<'a>, PdfiumError> {
        if let Some(window) = window() {
            let fetch_result = JsFuture::from(window.fetch_with_str(url.to_string().as_str()))
                .await
                .map_err(PdfiumError::WebSysFetchError)?;

            debug_assert!(fetch_result.is_instance_of::<Response>());

            let response: Response = fetch_result
                .dyn_into()
                .map_err(|_| PdfiumError::WebSysInvalidResponseError)?;

            let blob: Blob =
                JsFuture::from(response.blob().map_err(PdfiumError::WebSysFetchError)?)
                    .await
                    .map_err(PdfiumError::WebSysFetchError)?
                    .into();

            self.load_pdf_from_blob(blob, password).await
        } else {
            Err(PdfiumError::WebSysWindowObjectNotAvailable)
        }
    }
    pub async fn load_pdf_from_blob<'a>(
        &'a self,
        blob: Blob,
        password: Option<&str>,
    ) -> Result<PdfDocument<'a>, PdfiumError> {
        let array_buffer: ArrayBuffer = JsFuture::from(blob.array_buffer())
            .await
            .map_err(PdfiumError::WebSysFetchError)?
            .into();

        let u8_array: Uint8Array = Uint8Array::new(&array_buffer);

        let bytes: Vec<u8> = u8_array.to_vec();

        self.load_pdf_from_byte_vec(bytes, password)
    }
}
impl<'a> PdfDocument<'a> {
    pub fn save_to_blob(&self) -> Result<Blob, PdfiumError> {
        let bytes = self.save_to_bytes()?;

        let array = Uint8Array::new_with_length(bytes.len() as u32);

        array.copy_from(bytes.as_slice());

        let blob =
            Blob::new_with_u8_array_sequence(&JsValue::from(Array::of1(&JsValue::from(array))))
                .map_err(|_| PdfiumError::JsSysErrorConstructingBlobFromBytes)?;

        Ok(blob)
    }
}
impl<'a> PdfAttachments<'a> {
    pub async fn create_attachment_from_fetch(
        &'a mut self,
        name: &str,
        url: impl ToString,
    ) -> Result<PdfAttachment<'a>, PdfiumError> {
        if let Some(window) = window() {
            let fetch_result = JsFuture::from(window.fetch_with_str(url.to_string().as_str()))
                .await
                .map_err(PdfiumError::WebSysFetchError)?;

            debug_assert!(fetch_result.is_instance_of::<Response>());

            let response: Response = fetch_result
                .dyn_into()
                .map_err(|_| PdfiumError::WebSysInvalidResponseError)?;

            let blob: Blob =
                JsFuture::from(response.blob().map_err(PdfiumError::WebSysFetchError)?)
                    .await
                    .map_err(PdfiumError::WebSysFetchError)?
                    .into();

            self.create_attachment_from_blob(name, blob).await
        } else {
            Err(PdfiumError::WebSysWindowObjectNotAvailable)
        }
    }
    pub async fn create_attachment_from_blob(
        &'a mut self,
        name: &str,
        blob: Blob,
    ) -> Result<PdfAttachment<'a>, PdfiumError> {
        let array_buffer: ArrayBuffer = JsFuture::from(blob.array_buffer())
            .await
            .map_err(PdfiumError::WebSysFetchError)?
            .into();

        let u8_array: Uint8Array = Uint8Array::new(&array_buffer);

        let bytes: Vec<u8> = u8_array.to_vec();

        self.create_attachment_from_bytes(name, bytes.as_slice())
    }
}
impl<'a> PdfAttachment<'a> {
    pub fn save_to_blob(&self) -> Result<Blob, PdfiumError> {
        let bytes = self.save_to_bytes()?;

        let array = Uint8Array::new_with_length(bytes.len() as u32);

        array.copy_from(bytes.as_slice());

        let blob =
            Blob::new_with_u8_array_sequence(&JsValue::from(Array::of1(&JsValue::from(array))))
                .map_err(|_| PdfiumError::JsSysErrorConstructingBlobFromBytes)?;

        Ok(blob)
    }
}
impl<'a> PdfFonts<'a> {
    pub async fn load_type1_from_fetch(
        &mut self,
        url: impl ToString,
        is_cid_font: bool,
    ) -> Result<PdfFontToken, PdfiumError> {
        if let Some(window) = window() {
            let fetch_result = JsFuture::from(window.fetch_with_str(url.to_string().as_str()))
                .await
                .map_err(PdfiumError::WebSysFetchError)?;

            debug_assert!(fetch_result.is_instance_of::<Response>());

            let response: Response = fetch_result
                .dyn_into()
                .map_err(|_| PdfiumError::WebSysInvalidResponseError)?;

            let blob: Blob =
                JsFuture::from(response.blob().map_err(PdfiumError::WebSysFetchError)?)
                    .await
                    .map_err(PdfiumError::WebSysFetchError)?
                    .into();

            self.load_type1_from_blob(blob, is_cid_font).await
        } else {
            Err(PdfiumError::WebSysWindowObjectNotAvailable)
        }
    }
    pub async fn load_type1_from_blob(
        &mut self,
        blob: Blob,
        is_cid_font: bool,
    ) -> Result<PdfFontToken, PdfiumError> {
        let array_buffer: ArrayBuffer = JsFuture::from(blob.array_buffer())
            .await
            .map_err(PdfiumError::WebSysFetchError)?
            .into();

        let u8_array: Uint8Array = Uint8Array::new(&array_buffer);

        let bytes: Vec<u8> = u8_array.to_vec();

        self.load_type1_from_bytes(bytes.as_slice(), is_cid_font)
    }
    pub async fn load_true_type_from_fetch(
        &mut self,
        url: impl ToString,
        is_cid_font: bool,
    ) -> Result<PdfFontToken, PdfiumError> {
        if let Some(window) = window() {
            let fetch_result = JsFuture::from(window.fetch_with_str(url.to_string().as_str()))
                .await
                .map_err(PdfiumError::WebSysFetchError)?;

            debug_assert!(fetch_result.is_instance_of::<Response>());

            let response: Response = fetch_result
                .dyn_into()
                .map_err(|_| PdfiumError::WebSysInvalidResponseError)?;

            let blob: Blob =
                JsFuture::from(response.blob().map_err(PdfiumError::WebSysFetchError)?)
                    .await
                    .map_err(PdfiumError::WebSysFetchError)?
                    .into();

            self.load_true_type_from_blob(blob, is_cid_font).await
        } else {
            Err(PdfiumError::WebSysWindowObjectNotAvailable)
        }
    }
    pub async fn load_true_type_from_blob(
        &mut self,
        blob: Blob,
        is_cid_font: bool,
    ) -> Result<PdfFontToken, PdfiumError> {
        let array_buffer: ArrayBuffer = JsFuture::from(blob.array_buffer())
            .await
            .map_err(PdfiumError::WebSysFetchError)?
            .into();

        let u8_array: Uint8Array = Uint8Array::new(&array_buffer);

        let bytes: Vec<u8> = u8_array.to_vec();

        self.load_true_type_from_bytes(bytes.as_slice(), is_cid_font)
    }
}
impl<'a> PdfBitmap<'a> {
    pub fn as_array(&self) -> Uint8Array {
        Uint8Array::from(self.as_raw_bytes().as_slice())
    }
    pub fn as_image_data(&self) -> Result<ImageData, JsValue> {
        ImageData::new_with_u8_clamped_array_and_sh(
            Clamped(&self.as_rgba_bytes()),
            self.width() as u32,
            self.height() as u32,
        )
    }
}
