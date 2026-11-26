//! Video frames from the stream worker to the UI, through a memory-mapped
//! file both processes open. Three slots make a triple buffer: the worker
//! fills its back slot and swaps it into the middle; the UI swaps the middle
//! out when it's newer than what it shows. Neither waits for the other, and
//! neither reads a slot the other is writing.
//!
//! The file lives in the runtime directory, which is memory on Linux, and is
//! removed once the worker has it open, so the memory returns when both
//! processes let go. Windows won't remove a mapped file cleanly while handles
//! are open, so there frames live in a named pagefile-backed section
//! (`Local\yapfast-frames-...`), keeping disk writes at zero until both
//! processes close their handles.

use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
};

use anyhow::{Context, Result, ensure};
use memmap2::MmapMut;

use crate::picture::I420;

const MAGIC: u32 = u32::from_le_bytes(*b"TKFR");
const HEADER: usize = 64;
const SLOTS: u32 = 3;
/// Set beside the middle slot's index when the worker put a frame there that
/// the UI hasn't taken.
const FRESH: u32 = 4;

/// The header at the start of the file; slots follow it.
#[repr(C)]
struct Header {
    magic: u32,
    capacity: u32,
    /// The middle slot's index, with [`FRESH`].
    middle: AtomicU32,
    i420: u32,
    /// Each slot's frame size, as width and height in pixels.
    sizes: [[AtomicU32; 2]; SLOTS as usize],
    /// When each slot's frame shows in a video file, in milliseconds.
    times: [AtomicU64; SLOTS as usize],
}

const _: () = assert!(size_of::<Header>() <= HEADER);

/// The largest frame `width` by `height` RGBA pixels fit in.
fn capacity(width: u32, height: u32, i420: bool) -> usize {
    if i420 {
        I420::packed_len(width, height)
    } else {
        width as usize * height as usize * 4
    }
}

#[cfg(windows)]
mod win_section {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use anyhow::{Context, Result, ensure};
    use windows::Win32::Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows::Win32::System::Memory::{
        CreateFileMappingW, FILE_MAP_ALL_ACCESS, MEMORY_BASIC_INFORMATION,
        MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, OpenFileMappingW, PAGE_READWRITE,
        UnmapViewOfFile, VirtualQuery,
    };
    use windows::core::HSTRING;

    pub(super) const SECTION_PREFIX: &str = "Local\\yapfast-frames-";
    static CREATED: AtomicUsize = AtomicUsize::new(0);

    struct HandleGuard(HANDLE);

    impl Drop for HandleGuard {
        fn drop(&mut self) {
            // SAFETY: owned section handle closed at most once.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    pub(super) struct SectionMap {
        _handle: HandleGuard,
        view: MEMORY_MAPPED_VIEW_ADDRESS,
        len: usize,
    }

    // SAFETY: view and kernel section handle remain valid until SectionMap
    // drops, and concurrent access uses the triple-buffer atomic header.
    unsafe impl Send for SectionMap {}
    unsafe impl Sync for SectionMap {}

    impl Drop for SectionMap {
        fn drop(&mut self) {
            // SAFETY: view came from MapViewOfFile on _handle.
            unsafe {
                let _ = UnmapViewOfFile(self.view);
            }
        }
    }

    impl SectionMap {
        pub(super) fn create(total: usize) -> Result<(Self, String)> {
            ensure!(total > 0, "the video frames mapping cannot be empty");
            let size = u64::try_from(total)?;
            let high = (size >> 32) as u32;
            let low = size as u32;
            loop {
                let name = format!(
                    "{SECTION_PREFIX}{}-{}",
                    std::process::id(),
                    CREATED.fetch_add(1, Ordering::Relaxed)
                );
                let wide = HSTRING::from(&name);
                // SAFETY: pagefile-backed named section sized to `total` bytes.
                let handle = unsafe {
                    CreateFileMappingW(
                        INVALID_HANDLE_VALUE,
                        None,
                        PAGE_READWRITE,
                        high,
                        low,
                        &wide,
                    )
                }
                .context("could not create the video frames mapping")?;
                // SAFETY: immediate inspection after CreateFileMappingW.
                let existed = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
                let guard = HandleGuard(handle);
                if existed {
                    continue;
                }
                let map = Self::map_guard(guard, total)?;
                return Ok((map, name));
            }
        }

        pub(super) fn open(name: &str) -> Result<Self> {
            let wide = HSTRING::from(name);
            // SAFETY: opens the existing named section created by Frames.
            let handle = unsafe { OpenFileMappingW(FILE_MAP_ALL_ACCESS.0, false, &wide) }
                .context("could not open the video frames mapping")?;
            Self::map_guard(HandleGuard(handle), 0)
        }

        fn map_guard(guard: HandleGuard, requested: usize) -> Result<Self> {
            // SAFETY: maps the validated section handle into this process.
            let view = unsafe { MapViewOfFile(guard.0, FILE_MAP_ALL_ACCESS, 0, 0, requested) };
            if view.Value.is_null() {
                return Err(std::io::Error::last_os_error())
                    .context("could not map the video frames");
            }
            let len = if requested > 0 {
                requested
            } else {
                let mut info = MEMORY_BASIC_INFORMATION::default();
                // SAFETY: queries the mapped view we just created.
                let queried = unsafe {
                    VirtualQuery(
                        Some(view.Value as *const c_void),
                        &mut info,
                        size_of::<MEMORY_BASIC_INFORMATION>(),
                    )
                };
                if queried == 0 {
                    let err = std::io::Error::last_os_error();
                    // SAFETY: unmap before returning error.
                    unsafe {
                        let _ = UnmapViewOfFile(view);
                    }
                    return Err(err).context("could not measure the video frames mapping");
                }
                info.RegionSize
            };
            Ok(Self {
                _handle: guard,
                view,
                len,
            })
        }

        pub(super) fn as_ptr(&self) -> *const u8 {
            self.view.Value.cast()
        }

        pub(super) fn as_mut_ptr(&mut self) -> *mut u8 {
            self.view.Value.cast()
        }

        pub(super) fn len(&self) -> usize {
            self.len
        }
    }
}

enum MapBacking {
    File(MmapMut),
    #[cfg(windows)]
    Section(win_section::SectionMap),
}

impl MapBacking {
    fn as_ptr(&self) -> *const u8 {
        match self {
            Self::File(map) => map.as_ptr(),
            #[cfg(windows)]
            Self::Section(section) => section.as_ptr(),
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::File(map) => map.len(),
            #[cfg(windows)]
            Self::Section(section) => section.len(),
        }
    }
}

struct Mapping {
    map: MapBacking,
    capacity: usize,
}

impl Mapping {
    fn open(file: &File) -> Result<Self> {
        // SAFETY: only Yapfast's own processes open the file, and they touch the
        // slots through the triple buffer, never the same slot at once.
        let map = unsafe { MmapMut::map_mut(file) }.context("could not map the video frames")?;
        Self::from_backing(MapBacking::File(map))
    }

    fn from_backing(map: MapBacking) -> Result<Self> {
        ensure!(map.len() >= HEADER, "the video frames file is too short");
        let mapping = Self { capacity: 0, map };
        ensure!(mapping.header().magic == MAGIC, "not a video frames file");
        let capacity = mapping.header().capacity as usize;
        ensure!(
            mapping.map.len() >= HEADER + SLOTS as usize * capacity,
            "the video frames file is too short"
        );
        Ok(Self {
            capacity,
            ..mapping
        })
    }

    fn header(&self) -> &Header {
        // SAFETY: the map is at least HEADER bytes, page aligned, and Header
        // is plain integers.
        unsafe { &*self.map.as_ptr().cast::<Header>() }
    }

    fn frame_len(&self, width: u32, height: u32) -> usize {
        capacity(width, height, self.header().i420 != 0)
    }

    fn slot(&self, index: u32) -> *mut u8 {
        // SAFETY: Mapping::open checked the file holds every slot.
        unsafe {
            self.map
                .as_ptr()
                .cast_mut()
                .add(HEADER + index as usize * self.capacity)
        }
    }
}

/// The UI's side: it creates the file and shows the newest frame.
pub struct Frames {
    mapping: Mapping,
    /// The slot the UI reads, while it reads it.
    front: Mutex<u32>,
    name: Mutex<Option<Name>>,
}

/// What keeps the file's name until [`Frames::unlink`].
#[cfg(not(windows))]
type Name = tempfile::TempPath;

#[cfg(windows)]
struct Name(PathBuf);

#[cfg(windows)]
impl std::ops::Deref for Name {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

#[cfg(not(windows))]
fn create_file(directory: PathBuf) -> std::io::Result<(File, Name)> {
    let file = tempfile::Builder::new()
        .prefix("yapfast-frames-")
        .tempfile_in(directory)?;
    Ok(file.into_parts())
}

impl std::fmt::Debug for Frames {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Frames")
            .field("capacity", &self.mapping.capacity)
            .finish_non_exhaustive()
    }
}

impl Frames {
    /// Creates a file for frames up to `width` by `height` pixels.
    pub fn create(width: u32, height: u32) -> Result<Self> {
        Self::create_format(width, height, false)
    }

    pub(crate) fn create_i420(width: u32, height: u32) -> Result<Self> {
        Self::create_format(width, height, true)
    }

    #[cfg(not(windows))]
    fn create_format(width: u32, height: u32, i420: bool) -> Result<Self> {
        let directory = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .filter(|directory| directory.is_dir())
            .unwrap_or_else(std::env::temp_dir);
        let (file, name) =
            create_file(directory).context("could not create the video frames file")?;
        let capacity = capacity(width, height, i420);
        file.set_len((HEADER + SLOTS as usize * capacity) as u64)
            .context("could not size the video frames file")?;
        {
            // SAFETY: the file was just created and no one else has it yet.
            let mut map =
                unsafe { MmapMut::map_mut(&file) }.context("could not map the video frames")?;
            map[..4].copy_from_slice(&MAGIC.to_ne_bytes());
            map[4..8].copy_from_slice(&u32::try_from(capacity)?.to_ne_bytes());
            // The worker starts with slot 0 and the UI with slot 2.
            map[8..12].copy_from_slice(&1u32.to_ne_bytes());
            map[12..16].copy_from_slice(&u32::from(i420).to_ne_bytes());
        }
        Ok(Self {
            mapping: Mapping::open(&file)?,
            front: Mutex::new(2),
            name: Mutex::new(Some(name)),
        })
    }

    #[cfg(windows)]
    fn create_format(width: u32, height: u32, i420: bool) -> Result<Self> {
        let capacity = capacity(width, height, i420);
        let capacity_u32 = u32::try_from(capacity)?;
        let total = HEADER + SLOTS as usize * capacity;
        let (mut section, name) = win_section::SectionMap::create(total)?;
        // SAFETY: section was just created and sized to at least HEADER bytes.
        let header_bytes = unsafe { std::slice::from_raw_parts_mut(section.as_mut_ptr(), 16) };
        header_bytes[..4].copy_from_slice(&MAGIC.to_ne_bytes());
        header_bytes[4..8].copy_from_slice(&capacity_u32.to_ne_bytes());
        // The worker starts with slot 0 and the UI with slot 2.
        header_bytes[8..12].copy_from_slice(&1u32.to_ne_bytes());
        header_bytes[12..16].copy_from_slice(&u32::from(i420).to_ne_bytes());
        Ok(Self {
            mapping: Mapping::from_backing(MapBacking::Section(section))?,
            front: Mutex::new(2),
            name: Mutex::new(Some(Name(PathBuf::from(name)))),
        })
    }

    /// Where the worker opens the file, until [`Self::unlink`].
    pub fn path(&self) -> Option<PathBuf> {
        self.name
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_deref()
            .map(Path::to_owned)
    }

    /// Removes the file's name once the worker has it open. On Windows the
    /// name stays until both processes close the file.
    pub fn unlink(&self) {
        self.name
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
    }

    /// Calls `show` with the newest frame's width, height and RGBA pixels,
    /// if one arrived since the last call.
    pub fn take<T>(&self, show: impl FnOnce(u32, u32, &[u8]) -> T) -> Option<T> {
        self.take_timed(show).map(|(shown, _)| shown)
    }

    /// Reads a stream frame in either the packed or legacy RGBA format.
    pub fn take_stream<T>(
        &self,
        show: impl FnOnce(u32, u32, &[u8], Option<I420<'_>>) -> T,
    ) -> Option<T> {
        self.take(|w, h, pixels| {
            let picture =
                (self.mapping.header().i420 != 0).then(|| I420::from_packed(w, h, pixels));
            show(w, h, pixels, picture)
        })
    }

    /// Re-reads the last front stream frame without waiting for a new one,
    /// returning `None` if no frame has been taken yet.
    pub fn current_stream<T>(
        &self,
        show: impl FnOnce(u32, u32, &[u8], Option<I420<'_>>) -> T,
    ) -> Option<T> {
        let front = self
            .front
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let header = self.mapping.header();
        let [width, height] = &header.sizes[*front as usize];
        let (width, height) = (
            width.load(Ordering::Relaxed),
            height.load(Ordering::Relaxed),
        );
        if width == 0 || height == 0 {
            return None;
        }
        let length = self
            .mapping
            .frame_len(width, height)
            .min(self.mapping.capacity);
        // SAFETY: `*front` stays the UI's front slot until `take_timed` swaps
        // it into `middle`; the worker never writes to `*front`.
        let pixels = unsafe { std::slice::from_raw_parts(self.mapping.slot(*front), length) };
        let picture = (header.i420 != 0).then(|| I420::from_packed(width, height, pixels));
        Some(show(width, height, pixels, picture))
    }

    /// As [`Self::take`], with when the frame shows in a video file.
    pub fn take_timed<T>(&self, show: impl FnOnce(u32, u32, &[u8]) -> T) -> Option<(T, u64)> {
        let header = self.mapping.header();
        if header.middle.load(Ordering::Acquire) & FRESH == 0 {
            return None;
        }
        let mut front = self
            .front
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if header.middle.load(Ordering::Acquire) & FRESH == 0 {
            return None;
        }
        *front = header.middle.swap(*front, Ordering::AcqRel) & !FRESH;
        let [width, height] = &header.sizes[*front as usize];
        let (width, height) = (
            width.load(Ordering::Relaxed),
            height.load(Ordering::Relaxed),
        );
        let length = self
            .mapping
            .frame_len(width, height)
            .min(self.mapping.capacity);
        // SAFETY: the worker no longer writes this slot: it's the UI's front
        // until the UI swaps it back.
        let pixels = unsafe { std::slice::from_raw_parts(self.mapping.slot(*front), length) };
        let time = header.times[*front as usize].load(Ordering::Relaxed);
        Some((show(width, height, pixels), time))
    }
}

/// The worker's side: it fills frames.
pub struct Writer {
    mapping: Mapping,
    back: u32,
}

impl Writer {
    pub fn open(path: &Path) -> Result<Self> {
        #[cfg(windows)]
        if let Some(name) = path
            .to_str()
            .filter(|name| name.starts_with(win_section::SECTION_PREFIX))
        {
            let section = win_section::SectionMap::open(name)?;
            return Ok(Self {
                mapping: Mapping::from_backing(MapBacking::Section(section))?,
                back: 0,
            });
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .context("could not open the video frames file")?;
        Ok(Self {
            mapping: Mapping::open(&file)?,
            back: 0,
        })
    }

    /// The largest frame, in pixels, that fits.
    pub fn fits(&self, width: u32, height: u32) -> bool {
        self.mapping.frame_len(width, height) <= self.mapping.capacity
    }

    /// Writes a frame with `fill`, which gets its RGBA pixels, then shows it.
    pub fn write(&mut self, width: u32, height: u32, fill: impl FnOnce(&mut [u8])) {
        self.write_timed(width, height, 0, fill);
    }

    pub(crate) fn write_video(
        &mut self,
        picture: I420<'_>,
        width: u32,
        height: u32,
        scratch: &mut Vec<u8>,
    ) {
        let packed = self.mapping.header().i420 != 0;
        self.write(width, height, |pixels| {
            if packed {
                picture.pack(pixels, width, height);
            } else {
                picture.to_rgba(scratch, pixels, width, height);
            }
        });
    }

    /// As [`Self::write`], for a frame of a video file showing at `time_ms`.
    pub fn write_timed(
        &mut self,
        width: u32,
        height: u32,
        time_ms: u64,
        fill: impl FnOnce(&mut [u8]),
    ) {
        assert!(self.fits(width, height), "a frame larger than its slot");
        // SAFETY: the back slot is the worker's alone until it's swapped in.
        let pixels = unsafe {
            std::slice::from_raw_parts_mut(
                self.mapping.slot(self.back),
                self.mapping.frame_len(width, height),
            )
        };
        fill(pixels);
        let header = self.mapping.header();
        let [w, h] = &header.sizes[self.back as usize];
        w.store(width, Ordering::Relaxed);
        h.store(height, Ordering::Relaxed);
        header.times[self.back as usize].store(time_ms, Ordering::Relaxed);
        self.back = header.middle.swap(self.back | FRESH, Ordering::AcqRel) & !FRESH;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_stream_frames_match_the_rgba_path() {
        let (width, height) = (9, 7);
        let bytes: Vec<_> = (0..I420::packed_len(width, height))
            .map(|n| (n * 17) as u8)
            .collect();
        let source = I420::from_packed(width, height, &bytes);
        let frames = Frames::create_i420(width, height).unwrap();
        let mut writer = Writer::open(&frames.path().unwrap()).unwrap();
        for (w, h) in [(width, height), (6, 4)] {
            let mut scratch = Vec::new();
            let mut expected = vec![0; (w * h * 4) as usize];
            source.to_rgba(&mut scratch, &mut expected, w, h);
            writer.write_video(source, w, h, &mut scratch);
            let shown = frames
                .take_stream(|w, h, pixels, picture| {
                    assert_eq!(pixels.len(), I420::packed_len(w, h));
                    let mut rgba = vec![0; (w * h * 4) as usize];
                    picture.unwrap().to_rgba(&mut scratch, &mut rgba, w, h);
                    rgba
                })
                .unwrap();
            assert_eq!(shown, expected);
        }
    }

    #[test]
    fn frames_pass_from_writer_to_reader() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        #[cfg(windows)]
        assert!(
            path.to_str()
                .unwrap()
                .starts_with(win_section::SECTION_PREFIX)
        );
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();
        assert!(frames.path().is_none());
        assert!(!path.exists());
        assert!(frames.take(|_, _, _| ()).is_none(), "nothing yet");
        assert!(writer.fits(2, 4) && !writer.fits(5, 2));

        writer.write(2, 1, |pixels| pixels.fill(1));
        writer.write(4, 2, |pixels| pixels.fill(2));
        let newest = frames.take(|width, height, pixels| (width, height, pixels.to_vec()));
        assert_eq!(newest, Some((4, 2, vec![2; 32])), "only the newest");
        assert!(frames.take(|_, _, _| ()).is_none(), "taken once");

        // Many frames while the UI holds one: it never sees a partial frame.
        for value in 3..10 {
            writer.write(1, 1, |pixels| pixels.fill(value));
        }
        assert_eq!(
            frames.take(|_, _, pixels| pixels.to_vec()),
            Some(vec![9; 4])
        );

        // A video's frames carry their time.
        writer.write_timed(1, 1, 1234, |pixels| pixels.fill(0));
        assert_eq!(frames.take_timed(|_, _, _| ()), Some(((), 1234)));

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    #[cfg(windows)]
    #[test]
    fn windows_file_fallback_round_trips() {
        let cap = capacity(2, 2, false);
        let temp = tempfile::NamedTempFile::new().unwrap();
        temp.as_file()
            .set_len((HEADER + SLOTS as usize * cap) as u64)
            .unwrap();
        {
            let mut map = unsafe { MmapMut::map_mut(temp.as_file()) }.unwrap();
            map[..4].copy_from_slice(&MAGIC.to_ne_bytes());
            map[4..8].copy_from_slice(&(cap as u32).to_ne_bytes());
            map[8..12].copy_from_slice(&1u32.to_ne_bytes());
            map[12..16].copy_from_slice(&0u32.to_ne_bytes());
        }
        let frames = Frames {
            mapping: Mapping::open(temp.as_file()).unwrap(),
            front: Mutex::new(2),
            name: Mutex::new(None),
        };
        let mut writer = Writer::open(temp.path()).unwrap();
        writer.write(2, 2, |pixels| pixels.fill(7));
        assert_eq!(
            frames.take(|width, height, pixels| (width, height, pixels.to_vec())),
            Some((2, 2, vec![7; 16]))
        );
    }

    #[test]
    fn current_stream_rereads_last_front_frame_and_is_none_before_first() {
        let frames = Frames::create(2, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();

        assert!(frames.current_stream(|w, h, _, _| (w, h)).is_none());
        writer.write(2, 2, |pixels| pixels.fill(5));
        assert!(
            frames.current_stream(|w, h, _, _| (w, h)).is_none(),
            "unread middle frame is not in front yet"
        );
        assert_eq!(
            frames.take_stream(|w, h, pixels, pic| (w, h, pixels[0], pic.is_some())),
            Some((2, 2, 5, false))
        );
        assert_eq!(
            frames.current_stream(|w, h, pixels, pic| (w, h, pixels[0], pic.is_some())),
            Some((2, 2, 5, false))
        );
        assert_eq!(
            frames.current_stream(|w, h, pixels, _| (w, h, pixels.len())),
            Some((2, 2, 16))
        );
    }
}
