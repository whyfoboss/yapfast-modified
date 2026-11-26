/*
 * Copyright 2025 LiveKit, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#include "livekit/desktop_capturer.h"

#include "modules/desktop_capture/desktop_capture_options.h"
#ifdef _WIN64
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <dwmapi.h>
#include "modules/desktop_capture/desktop_and_cursor_composer.h"
#include "modules/desktop_capture/mouse_cursor.h"
#include "modules/desktop_capture/mouse_cursor_monitor.h"
#endif

using SourceList = webrtc::DesktopCapturer::SourceList;

namespace livekit_ffi {

bool cursor_info_is_visible(bool get_cursor_ok, uint32_t flags, bool has_cursor) {
  constexpr uint32_t kCursorShowing = 0x00000001u;
  return get_cursor_ok && ((flags & kCursorShowing) != 0) && has_cursor;
}

bool cursor_bgra_has_visible_pixel(rust::Slice<const uint8_t> pixels,
                                    int32_t width,
                                    int32_t height,
                                    int32_t stride) {
  if (width <= 0 || height <= 0 || stride < width * 4) {
    return false;
  }
  const size_t row_bytes = static_cast<size_t>(width) * 4;
  const size_t stride_bytes = static_cast<size_t>(stride);
  const size_t required =
      static_cast<size_t>(height - 1) * stride_bytes + row_bytes;
  if (pixels.size() < required) {
    return false;
  }
  for (int32_t y = 0; y < height; ++y) {
    const uint8_t* row = pixels.data() + static_cast<size_t>(y) * stride_bytes;
    for (int32_t x = 0; x < width; ++x) {
      if (row[static_cast<size_t>(x) * 4 + 3] != 0) {
        return true;
      }
    }
  }
  return false;
}

#ifdef _WIN64
namespace {

// Wraps DesktopAndCursorComposer on Windows so hidden, suppressed, transparent,
// or out-of-window cursors (such as during full-screen gameplay) are cleared
// immediately instead of leaving the last cached cursor visible on stream frames.
class CursorAwareDesktopCapturer final
    : public webrtc::DesktopCapturer,
      public webrtc::MouseCursorMonitor::Callback {
 public:
  CursorAwareDesktopCapturer(std::unique_ptr<webrtc::DesktopCapturer> capturer,
                             const webrtc::DesktopCaptureOptions& options,
                             SourceType source_type)
      : composer_(
            webrtc::DesktopAndCursorComposer::CreateWithoutMouseCursorMonitor(
                std::move(capturer))),
        monitor_(webrtc::MouseCursorMonitor::Create(options)),
        source_type_(source_type) {}

  void Start(DesktopCapturer::Callback* callback) override {
    if (monitor_) {
      monitor_->Init(this, webrtc::MouseCursorMonitor::SHAPE_AND_POSITION);
    }
    composer_->Start(callback);
  }

  void SetMaxFrameRate(uint32_t max_frame_rate) override {
    composer_->SetMaxFrameRate(max_frame_rate);
  }

  webrtc::DelegatedSourceListController* GetDelegatedSourceListController()
      override {
    return composer_->GetDelegatedSourceListController();
  }

  void SetSharedMemoryFactory(
      std::unique_ptr<webrtc::SharedMemoryFactory> shared_memory_factory)
      override {
    composer_->SetSharedMemoryFactory(std::move(shared_memory_factory));
  }

  void CaptureFrame() override {
    CURSORINFO ci{};
    ci.cbSize = sizeof(ci);
    const BOOL ok = GetCursorInfo(&ci);
    const bool visible =
        cursor_info_is_visible(ok != FALSE, ci.flags, ci.hCursor != nullptr) &&
        IsPointInSelectedWindow(ci.ptScreenPos);
    if (!visible) {
      allow_cursor_this_capture_ = false;
      if (!cursor_hidden_) {
        composer_->OnMouseCursor(nullptr);
        cursor_hidden_ = true;
      }
    } else {
      const bool was_hidden = cursor_hidden_;
      cursor_hidden_ = false;
      allow_cursor_this_capture_ = true;
      cursor_updated_this_capture_ = false;
      if (monitor_) {
        monitor_->Capture();
      }
      if (was_hidden && !cursor_updated_this_capture_) {
        if (!cursor_transparent_ && cached_cursor_) {
          composer_->OnMouseCursor(webrtc::MouseCursor::CopyOf(*cached_cursor_));
        } else {
          composer_->OnMouseCursor(nullptr);
        }
      }
    }
    composer_->CaptureFrame();
  }

  void SetExcludedWindow(webrtc::WindowId window) override {
    composer_->SetExcludedWindow(window);
  }

  bool GetSourceList(SourceList* sources) override {
    return composer_->GetSourceList(sources);
  }

  bool SelectSource(SourceId id) override {
    if (source_type_ == SourceType::Window) {
      selected_window_ = reinterpret_cast<HWND>(static_cast<uintptr_t>(id));
    }
    return composer_->SelectSource(id);
  }

  bool FocusOnSelectedSource() override {
    return composer_->FocusOnSelectedSource();
  }

  bool IsOccluded(const webrtc::DesktopVector& pos) override {
    return composer_->IsOccluded(pos);
  }

  void OnMouseCursor(webrtc::MouseCursor* cursor) override {
    std::unique_ptr<webrtc::MouseCursor> owned(cursor);
    cursor_updated_this_capture_ = true;
    const webrtc::DesktopFrame* img = owned ? owned->image() : nullptr;
    const bool has_pixels =
        img && img->data() && img->size().height() > 0 && img->stride() > 0 &&
        cursor_bgra_has_visible_pixel(
            rust::Slice<const uint8_t>(
                img->data(), static_cast<size_t>(img->stride()) *
                                 static_cast<size_t>(img->size().height())),
            img->size().width(), img->size().height(), img->stride());
    if (!has_pixels) {
      cursor_transparent_ = true;
      cached_cursor_.reset();
      composer_->OnMouseCursor(nullptr);
      return;
    }
    cursor_transparent_ = false;
    cached_cursor_.reset(webrtc::MouseCursor::CopyOf(*owned));
    if (allow_cursor_this_capture_) {
      composer_->OnMouseCursor(owned.release());
    } else {
      composer_->OnMouseCursor(nullptr);
    }
  }

  void OnMouseCursorPosition(const webrtc::DesktopVector& position) override {
    if (allow_cursor_this_capture_) {
      composer_->OnMouseCursorPosition(position);
    }
  }

 private:
  bool IsPointInSelectedWindow(POINT pt) const {
    if (source_type_ != SourceType::Window || !selected_window_) {
      return true;
    }
    if (!IsWindowVisible(selected_window_) || IsIconic(selected_window_)) {
      return false;
    }
    RECT rect{};
    if (FAILED(DwmGetWindowAttribute(selected_window_,
                                     DWMWA_EXTENDED_FRAME_BOUNDS, &rect,
                                     sizeof(rect))) &&
        !GetWindowRect(selected_window_, &rect)) {
      return false;
    }
    return PtInRect(&rect, pt) != FALSE;
  }

  std::unique_ptr<webrtc::DesktopAndCursorComposer> composer_;
  std::unique_ptr<webrtc::MouseCursorMonitor> monitor_;
  SourceType source_type_;
  HWND selected_window_ = nullptr;
  std::unique_ptr<webrtc::MouseCursor> cached_cursor_;
  bool cursor_hidden_ = false;
  bool cursor_transparent_ = false;
  bool cursor_updated_this_capture_ = false;
  bool allow_cursor_this_capture_ = false;
};

}  // namespace
#endif  // _WIN64

std::unique_ptr<DesktopCapturer> new_desktop_capturer(
    DesktopCapturerOptions options) {
  webrtc::DesktopCaptureOptions webrtc_options =
      webrtc::DesktopCaptureOptions::CreateDefault();
#if defined(WEBRTC_MAC) && !defined(WEBRTC_IOS)
  webrtc_options.set_allow_sck_capturer(true);
  webrtc_options.set_allow_sck_system_picker(options.allow_sck_system_picker);
#endif /* defined(WEBRTC_MAC) && !defined(WEBRTC_IOS) */
#ifdef _WIN64
  switch (options.source_type) {
    case SourceType::Screen:
      webrtc_options.set_allow_wgc_screen_capturer(true);
      break;
    case SourceType::Window:
      webrtc_options.set_allow_wgc_window_capturer(true);
      // https://github.com/webrtc-sdk/webrtc/blob/m137_release/modules/desktop_capture/desktop_capture_options.h#L133-L142
      webrtc_options.set_enumerate_current_process_windows(false);
      break;
    default:
      break;
  }
  webrtc_options.set_allow_directx_capturer(true);
#endif /* _WIN64 */
#ifdef WEBRTC_USE_PIPEWIRE
  webrtc_options.set_allow_pipewire(true);
#endif /* WEBRTC_USE_PIPEWIRE */

  // prefer_cursor_embedded indicate that the capturer should try to include the
  // cursor in the frame
  webrtc_options.set_prefer_cursor_embedded(options.include_cursor);

  std::unique_ptr<webrtc::DesktopCapturer> capturer = nullptr;
  switch (options.source_type) {
    case SourceType::Window:
      capturer = webrtc::DesktopCapturer::CreateWindowCapturer(webrtc_options);
      break;
    case SourceType::Screen:
      capturer = webrtc::DesktopCapturer::CreateScreenCapturer(webrtc_options);
      break;
    case SourceType::Generic:
      capturer = webrtc::DesktopCapturer::CreateGenericCapturer(webrtc_options);
      break;
    default:
      return nullptr;
  }

  if (!capturer) {
    return nullptr;
  }
#ifdef _WIN64
  if (options.include_cursor) {
    // Keep the native cursor where available and compose it on fallback frames
    // only while the OS cursor is visible (and inside a selected window).
    capturer = std::make_unique<CursorAwareDesktopCapturer>(
        std::move(capturer), webrtc_options, options.source_type);
  }
#endif
  return std::make_unique<DesktopCapturer>(std::move(capturer));
}

void DesktopCapturer::start(
    rust::Box<DesktopCapturerCallbackWrapper> callback) {
  this->callback = std::move(callback);
  capturer->Start(this);
}

void DesktopCapturer::OnCaptureResult(
    webrtc::DesktopCapturer::Result result,
    std::unique_ptr<webrtc::DesktopFrame> frame) {
  CaptureResult ret_result = CaptureResult::ErrorPermanent;
  switch (result) {
    case webrtc::DesktopCapturer::Result::SUCCESS:
      ret_result = CaptureResult::Success;
      break;
    case webrtc::DesktopCapturer::Result::ERROR_PERMANENT:
      ret_result = CaptureResult::ErrorPermanent;
      break;
    case webrtc::DesktopCapturer::Result::ERROR_TEMPORARY:
      ret_result = CaptureResult::ErrorTemporary;
      break;
    default:
      break;
  }
  if (callback) {
    (*callback)->on_capture_result(
        ret_result, std::make_unique<DesktopFrame>(std::move(frame)));
  }
}

rust::Vec<Source> DesktopCapturer::get_source_list() const {
  SourceList list{};
  bool res = capturer->GetSourceList(&list);
  rust::Vec<Source> source_list{};
  if (res) {
    for (auto& source : list) {
      source_list.push_back(Source{static_cast<uint64_t>(source.id),
                                   source.title, source.display_id});
    }
  }
  return source_list;
}
}  // namespace livekit_ffi
