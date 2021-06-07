use super::*;
use crate::UserData;
use basis_universal_sys as sys;
use std::marker::PhantomData;

/// A transcoder that can convert compressed basis-universal data to compressed GPU formats or raw
/// color data
pub struct Transcoder<'a> {
    handle: *mut sys::Transcoder,
    data_phantom: PhantomData<&'a [u8]>,
}

/// Lightweight description of a mip level on a single image within basis data
#[derive(Default, Debug, Copy, Clone)]
pub struct ImageLevelDescription {
    pub original_width: u32,
    pub original_height: u32,
    pub block_count: u32,
}

/// Info for an image within basis data
pub type ImageInfo = sys::basist_basisu_image_info;

/// Info for a mip level of a single image within basis data
pub type ImageLevelInfo = sys::basist_basisu_image_level_info;

/// Info for the complete basis file
pub type FileInfo = sys::FileInfo;

/// Extra parameters for transcoding an image
#[derive(Default, Debug, Clone)]
pub struct TranscodeParameters {
    /// The image to transcode
    pub image_index: u32,
    /// The mip level of the image to transcode
    pub level_index: u32,
    /// Optional flags can affect transcoding in various ways
    pub decode_flags: Option<DecodeFlags>,
    /// Optional override for row pitch
    pub output_row_pitch_in_blocks_or_pixels: Option<u32>,
    /// Optional override for number of rows to transcode
    pub output_rows_in_pixels: Option<u32>,
}

/// Error result from trying to transcode an image
#[derive(Debug, Copy, Clone)]
pub enum TranscodeError {
    TranscodeFormatNotSupported,
    ImageLevelNotFound,
    TranscodeFailed,
}

impl<'a> Transcoder<'a> {
    /// Create a transcoder for the given file.
    pub fn new(
        ttype: TranscoderType,
        data: &'a [u8],
    ) -> Transcoder {
        let data_view = sys::MemoryView {
            pData: data.as_ptr() as *const std::ffi::c_void,
            size: data.len() as u32,
        };
        Transcoder {
            handle: unsafe { sys::transcoder_new(ttype, data_view) },
            data_phantom: PhantomData,
        }
    }

    /// Validates the .basis file.
    ///
    /// For .basis files this computes a crc16 over the entire file, so it's slow.
    pub fn validate_file_checksums(
        &self,
        full_validation: bool,
    ) -> bool {
        unsafe { sys::transcoder_validate_file_checksums(self.handle, full_validation) }
    }

    /// Quick header validation - no crc16 checks.
    pub fn validate_header(&self) -> bool {
        unsafe { sys::transcoder_validate_header(self.handle) }
    }

    /// The type of texture represented by the basis data
    pub fn basis_texture_type(&self) -> BasisTextureType {
        unsafe { sys::transcoder_get_texture_type(self.handle).into() }
    }

    /// The basis texture format of the basis data
    pub fn basis_texture_format(&self) -> BasisTextureFormat {
        unsafe { sys::transcoder_get_tex_format(self.handle).into() }
    }

    pub fn user_data(&self) -> Result<UserData, ()> {
        let mut userdata = UserData::default();
        let result = unsafe {
            sys::transcoder_get_userdata(
                self.handle,
                &mut userdata.userdata0,
                &mut userdata.userdata1,
            )
        };

        if result {
            Ok(userdata)
        } else {
            Err(())
        }
    }

    /// Number of images in the basis data
    pub fn image_count(&self) -> u32 {
        unsafe { sys::transcoder_get_total_images(self.handle) }
    }

    /// Number of mipmap levels for the specified image in the basis data
    pub fn image_level_count(
        &self,
        image_index: u32,
    ) -> u32 {
        unsafe { sys::transcoder_get_total_image_levels(self.handle, image_index) }
    }

    /// Returns basic information about an image. Note that orig_width/orig_height may not be a multiple of 4.
    pub fn image_level_description(
        &self,
        image_index: u32,
        level_index: u32,
    ) -> Option<ImageLevelDescription> {
        let mut description = ImageLevelDescription::default();
        unsafe {
            if sys::transcoder_get_image_level_desc(
                self.handle,
                image_index,
                level_index,
                &mut description.original_width,
                &mut description.original_height,
                &mut description.block_count,
            ) {
                Some(description)
            } else {
                None
            }
        }
    }

    /// Returns information about the specified image.
    pub fn image_info(
        &self,
        image_index: u32,
    ) -> Option<ImageInfo> {
        let mut image_info = unsafe { std::mem::zeroed::<ImageInfo>() };
        unsafe {
            if sys::transcoder_get_image_info(self.handle, &mut image_info, image_index) {
                Some(image_info)
            } else {
                None
            }
        }
    }

    /// Returns information about the specified image's mipmap level.
    pub fn image_level_info(
        &self,
        image_index: u32,
        level_index: u32,
    ) -> Option<ImageLevelInfo> {
        let mut image_level_info = unsafe { std::mem::zeroed::<ImageLevelInfo>() };
        unsafe {
            if sys::transcoder_get_image_level_info(
                self.handle,
                &mut image_level_info,
                image_index,
                level_index,
            ) {
                Some(image_level_info)
            } else {
                None
            }
        }
    }

    /// Get a description of the basis file and low-level information about each slice.
    pub fn file_info(&self) -> Option<FileInfo> {
        let mut file_info = unsafe { std::mem::zeroed::<FileInfo>() };
        unsafe {
            if sys::transcoder_get_file_info(self.handle, &mut file_info) {
                Some(file_info)
            } else {
                None
            }
        }
    }

    /// transcode_image_level() decodes a single mipmap level from the .basis file to any of the supported output texture formats.
    /// It'll first find the slice(s) to transcode, then call transcode_slice() one or two times to decode both the color and alpha texture data (or RG texture data from two slices for BC5).
    /// If the .basis file doesn't have alpha slices, the output alpha blocks will be set to fully opaque (all 255's).
    /// Currently, to decode to PVRTC1 the basis texture's dimensions in pixels must be a power of 2, due to PVRTC1 format requirements.
    /// output_blocks_buf_size_in_blocks_or_pixels should be at least the image level's total_blocks (num_blocks_x * num_blocks_y), or the total number of output pixels if fmt==cTFRGBA32.
    /// output_row_pitch_in_blocks_or_pixels: Number of blocks or pixels per row. If 0, the transcoder uses the slice's num_blocks_x or orig_width (NOT num_blocks_x * 4). Ignored for PVRTC1 (due to texture swizzling).
    /// output_rows_in_pixels: Ignored unless fmt is cRGBA32. The total number of output rows in the output buffer. If 0, the transcoder assumes the slice's orig_height (NOT num_blocks_y * 4).
    /// Notes:
    /// - basisu_transcoder_init() must have been called first to initialize the transcoder lookup tables before calling this function.
    /// - This method assumes the output texture buffer is readable. In some cases to handle alpha, the transcoder will write temporary data to the output texture in
    /// a first pass, which will be read in a second pass.
    pub fn transcode_image_level(
        &self,
        transcode_format: TranscoderTextureFormat,
        transcode_parameters: TranscodeParameters,
    ) -> Result<Vec<u8>, TranscodeError> {
        let image_index = transcode_parameters.image_index;
        let level_index = transcode_parameters.level_index;

        //
        // Check that the transcode format is supported for the stored texture's basis format
        //
        let basis_format = self.basis_texture_format();
        if !basis_format.can_transcode_to_format(transcode_format) {
            return Err(TranscodeError::TranscodeFormatNotSupported);
        }

        //
        // Determine required size for the buffer
        //
        let description = self
            .image_level_description(image_index, level_index)
            .ok_or(TranscodeError::ImageLevelNotFound)?;
        let required_buffer_bytes = transcode_format.calculate_minimum_output_buffer_bytes(
            description.original_width,
            description.original_height,
            description.block_count,
            transcode_parameters.output_row_pitch_in_blocks_or_pixels,
            transcode_parameters.output_rows_in_pixels,
        ) as usize;

        //
        // unwrap_or() the optional parameters
        //
        let decode_flags = transcode_parameters
            .decode_flags
            .unwrap_or_else(DecodeFlags::empty);
        let output_row_pitch_in_blocks_or_pixels = transcode_parameters
            .output_row_pitch_in_blocks_or_pixels
            .unwrap_or(0);
        let output_rows_in_pixels = transcode_parameters.output_rows_in_pixels.unwrap_or(0);
        let transcoder_state = std::ptr::null_mut();

        //
        // Transcode
        //
        let mut output = vec![0_u8; required_buffer_bytes];
        let success = unsafe {
            sys::transcoder_transcode_image_level(
                self.handle,
                image_index,
                level_index,
                output.as_mut_ptr() as _,
                output.len() as u32,
                transcode_format.into(),
                decode_flags.bits(),
                output_row_pitch_in_blocks_or_pixels,
                transcoder_state,
                output_rows_in_pixels,
            )
        };

        if success {
            Ok(output)
        } else {
            Err(TranscodeError::TranscodeFailed)
        }
    }

    // Not implemented
    //
    //    // Finds the basis slice corresponding to the specified image/level/alpha params, or -1 if the slice can't be found.
    //    int find_slice(const void *pData, uint32_t data_size, uint32_t image_index, uint32_t level_index, bool alpha_data) const;
    //
    //    // transcode_slice() decodes a single slice from the .basis file. It's a low-level API - most likely you want to use transcode_image_level().
    //    // This is a low-level API, and will be needed to be called multiple times to decode some texture formats (like BC3, BC5, or ETC2).
    //    // output_blocks_buf_size_in_blocks_or_pixels is just used for verification to make sure the output buffer is large enough.
    //    // output_blocks_buf_size_in_blocks_or_pixels should be at least the image level's total_blocks (num_blocks_x * num_blocks_y), or the total number of output pixels if fmt==cTFRGBA32.
    //    // output_block_stride_in_bytes: Number of bytes between each output block.
    //    // output_row_pitch_in_blocks_or_pixels: Number of blocks or pixels per row. If 0, the transcoder uses the slice's num_blocks_x or orig_width (NOT num_blocks_x * 4). Ignored for PVRTC1 (due to texture swizzling).
    //    // output_rows_in_pixels: Ignored unless fmt is cRGBA32. The total number of output rows in the output buffer. If 0, the transcoder assumes the slice's orig_height (NOT num_blocks_y * 4).
    //    // Notes:
    //    // - basisu_transcoder_init() must have been called first to initialize the transcoder lookup tables before calling this function.
    //    bool transcode_slice(const void *pData, uint32_t data_size, uint32_t slice_index,
    //                         void *pOutput_blocks, uint32_t output_blocks_buf_size_in_blocks_or_pixels,
    //                         block_format fmt, uint32_t output_block_stride_in_bytes, uint32_t decode_flags = 0, uint32_t output_row_pitch_in_blocks_or_pixels = 0, basisu_transcoder_state * pState = nullptr, void* pAlpha_blocks = nullptr,
    //                         uint32_t output_rows_in_pixels = 0, int channel0 = -1, int channel1 = -1) const;
    //
    //    static void write_opaque_alpha_blocks(
    //            uint32_t num_blocks_x, uint32_t num_blocks_y,
    //            void* pOutput_blocks, block_format fmt,
    //            uint32_t block_stride_in_bytes, uint32_t output_row_pitch_in_blocks_or_pixels);
}

impl<'a> Drop for Transcoder<'a> {
    fn drop(&mut self) {
        unsafe {
            sys::transcoder_delete(self.handle);
        }
    }
}
