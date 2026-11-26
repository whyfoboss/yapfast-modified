// Copyright 2025 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use cxx::UniquePtr;
use ffi::CaptureResult;

use crate::{desktop_capturer::ffi::DesktopFrame, impl_thread_safety};

#[cxx::bridge(namespace = "livekit_ffi")]
pub mod ffi {
    #[derive(Clone)]
    struct Source {
        id: u64,
        title: String,
        display_id: i64,
    }

    #[derive(Debug, PartialEq)]
    enum SourceType {
        Screen,
        Window,
        Generic,
    }

    #[derive(Clone, Debug)]
    struct DesktopCapturerOptions {
        source_type: SourceType,
        include_cursor: bool,
        allow_sck_system_picker: bool,
    }

    enum CaptureResult {
        Success,
        ErrorTemporary,
        ErrorPermanent,
    }

    unsafe extern "C++" {
        include!("livekit/desktop_capturer.h");

        type DesktopCapturer;
        type DesktopFrame;

        fn new_desktop_capturer(options: DesktopCapturerOptions) -> UniquePtr<DesktopCapturer>;
        fn cursor_info_is_visible(get_cursor_ok: bool, flags: u32, has_cursor: bool) -> bool;
        fn cursor_bgra_has_visible_pixel(
            pixels: &[u8],
            width: i32,
            height: i32,
            stride: i32,
        ) -> bool;
        fn capture_frame(self: &DesktopCapturer);
        fn get_source_list(self: &DesktopCapturer) -> Vec<Source>;
        fn select_source(self: &DesktopCapturer, id: u64) -> bool;
        fn start(self: Pin<&mut DesktopCapturer>, callback: Box<DesktopCapturerCallbackWrapper>);

        fn width(self: &DesktopFrame) -> i32;
        fn height(self: &DesktopFrame) -> i32;
        fn stride(self: &DesktopFrame) -> i32;
        fn left(self: &DesktopFrame) -> i32;
        fn top(self: &DesktopFrame) -> i32;
        fn data(self: &DesktopFrame) -> *const u8;
    }

    extern "Rust" {
        type DesktopCapturerCallbackWrapper;

        fn on_capture_result(
            self: &mut DesktopCapturerCallbackWrapper,
            result: CaptureResult,
            frame: UniquePtr<DesktopFrame>,
        );
    }
}

impl_thread_safety!(ffi::DesktopCapturer, Send + Sync);

#[derive(Debug, PartialEq)]
pub enum CaptureError {
    Temporary,
    Permanent,
}

pub trait DesktopCapturerCallback: Send {
    fn on_capture_result(&mut self, result: Result<UniquePtr<DesktopFrame>, CaptureError>);
}

pub struct DesktopCapturerCallbackWrapper {
    callback: Box<dyn DesktopCapturerCallback>,
}

impl DesktopCapturerCallbackWrapper {
    pub fn new(callback: Box<dyn DesktopCapturerCallback>) -> Self {
        Self { callback }
    }

    fn on_capture_result(&mut self, result: CaptureResult, frame: UniquePtr<DesktopFrame>) {
        match result {
            CaptureResult::Success => self.callback.on_capture_result(Ok(frame)),
            CaptureResult::ErrorTemporary => {
                self.callback.on_capture_result(Err(CaptureError::Temporary))
            }
            CaptureResult::ErrorPermanent => {
                self.callback.on_capture_result(Err(CaptureError::Permanent))
            }
            _ => self.callback.on_capture_result(Err(CaptureError::Permanent)),
        }
    }
}

pub fn cursor_info_is_visible(get_cursor_ok: bool, flags: u32, has_cursor: bool) -> bool {
    ffi::cursor_info_is_visible(get_cursor_ok, flags, has_cursor)
}

pub fn cursor_bgra_has_visible_pixel(pixels: &[u8], width: i32, height: i32, stride: i32) -> bool {
    ffi::cursor_bgra_has_visible_pixel(pixels, width, height, stride)
}

#[cfg(test)]
mod tests {
    use super::ffi::{cursor_bgra_has_visible_pixel, cursor_info_is_visible};

    const CURSOR_SHOWING: u32 = 0x0000_0001;
    const CURSOR_SUPPRESSED: u32 = 0x0000_0002;

    #[test]
    fn cursor_visibility_respects_flags_and_handle() {
        assert!(cursor_info_is_visible(true, CURSOR_SHOWING, true));
        assert!(!cursor_info_is_visible(false, CURSOR_SHOWING, true));
        assert!(!cursor_info_is_visible(true, 0, true));
        assert!(!cursor_info_is_visible(true, CURSOR_SUPPRESSED, true));
        assert!(!cursor_info_is_visible(true, CURSOR_SHOWING, false));
    }

    #[test]
    fn transparent_cursor_bitmap_is_treated_as_hidden() {
        // 2x2 BGRA cursor with non-zero RGB but alpha == 0 everywhere (Unreal/game hidden cursor).
        let transparent = [
            255, 255, 255, 0, 10, 20, 30, 0,
            40, 50, 60, 0, 70, 80, 90, 0,
        ];
        assert!(!cursor_bgra_has_visible_pixel(&transparent, 2, 2, 8));

        let mut visible = transparent;
        visible[15] = 255;
        assert!(cursor_bgra_has_visible_pixel(&visible, 2, 2, 8));

        // Invalid dimensions, stride, or truncated buffer fail closed.
        assert!(!cursor_bgra_has_visible_pixel(&visible, 0, 2, 8));
        assert!(!cursor_bgra_has_visible_pixel(&visible, 2, 0, 8));
        assert!(!cursor_bgra_has_visible_pixel(&visible, 2, 2, 4));
        assert!(!cursor_bgra_has_visible_pixel(&visible[..12], 2, 2, 8));
    }
}
