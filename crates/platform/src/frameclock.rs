//! A frame tick aligned with the compositor (`DCompositionWaitForCompositorClock`, or
//! `DwmFlush` on Windows 10, which does not have the compositor clock).
//!
//! Compositor-side animations (opacity, offset, clip…) never need this. It exists for the few
//! values that are rasterised *inside* a Direct2D surface each frame — a hover highlight
//! fading, the scroll offset gliding — where the app itself has to redraw. `SetTimer` ticks
//! at 15.6 ms granularity and drifts against vsync; the compositor clock does not.
//!
//! One thread waits on the clock and calls `on_frame` (typically a `PostMessage`) once per
//! vsync **while armed**. Consumers call [`FrameClock::request`] whenever they still have
//! work for the next frame; the clock disarms itself after each delivered frame, so an idle
//! app costs nothing.

use crate::bindings::*;
use crate::wide::to_wide;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use windows_core::{PCSTR, PCWSTR};

/// A missing compositor tick must not hold a short client animation past its endpoint.
const COMPOSITOR_TIMEOUT_MS: u32 = 16;
/// Some Windows builds occasionally return from the zero-handle wait without waiting at all.
const IMMEDIATE_WAIT: Duration = Duration::from_millis(1);
/// Pace that fast-return path without limiting a real high-refresh compositor signal.
const FALLBACK_FRAME: Duration = Duration::from_micros(8_333);
/// Safety net: if the clock has been armed for this long without a fresh `request`, force it
/// to disarm. Catches cases where the consumer forgot to request the next frame after an
/// animation finished — the compositor would otherwise keep waking the idle app forever.
const ARMED_FORCE_DISARM_MS: u64 = 500;

/// `DCompositionWaitForCompositorClock(count, handles, timeout_ms)`.
type WaitForCompositorClock = unsafe extern "system" fn(u32, *const HANDLE, u32) -> u32;

/// The compositor clock exists from Windows 11 on. It is looked up at run time so the
/// executable still loads on Windows 10.
fn compositor_clock() -> Option<WaitForCompositorClock> {
    let name = to_wide("dcomp.dll");
    // SAFETY: loading a system DLL by name from System32 and looking up an export; the
    // transmute matches the documented signature of that export.
    unsafe {
        let dll = LoadLibraryExW(
            PCWSTR(name.as_ptr()),
            None,
            LOAD_LIBRARY_SEARCH_SYSTEM32 as u32,
        );
        if dll.0.is_null() {
            return None;
        }
        let proc = GetProcAddress(
            dll,
            PCSTR(c"DCompositionWaitForCompositorClock".as_ptr().cast()),
        )?;
        Some(std::mem::transmute::<
            unsafe extern "system" fn() -> isize,
            WaitForCompositorClock,
        >(proc))
    }
}

struct Shared {
    armed: AtomicBool,
    quit: AtomicBool,
    wake: Mutex<()>,
    cv: Condvar,
    /// When the last `request()` armed the clock; reset each time a new request arrives.
    /// If armed but no new request came in for `ARMED_FORCE_DISARM_MS`, the worker thread
    /// forces `armed = false` to kill an idle wakeup loop.
    armed_since: Mutex<Option<Instant>>,
}

pub struct FrameClock {
    shared: Arc<Shared>,
}

impl FrameClock {
    /// Starts the clock thread. `on_frame` runs on that thread once per requested frame and
    /// must be cheap and thread-safe (post a message; never touch UI state).
    pub fn start(on_frame: impl Fn() + Send + 'static) -> Self {
        let shared = Arc::new(Shared {
            armed: AtomicBool::new(false),
            quit: AtomicBool::new(false),
            wake: Mutex::new(()),
            cv: Condvar::new(),
            armed_since: Mutex::new(None),
        });
        let worker = shared.clone();
        std::thread::Builder::new()
            .name("frame-clock".into())
            .spawn(move || {
                let clock = compositor_clock();
                if clock.is_none() {
                    tracing::info!("compositor clock unavailable; pacing frames with DwmFlush");
                }
                loop {
                    // 自愈：如果 clock 已经 armed 超过 500ms 且没有新的 request() 重入，
                    // 说明调用方在动画结束后忘记解除，强制 disarm 防止 idle wakeup。
                    {
                        let mut since = worker.armed_since.lock().unwrap();
                        if let Some(start) = *since {
                            if start.elapsed().as_millis() as u64 > ARMED_FORCE_DISARM_MS {
                                if worker.armed.swap(false, Ordering::AcqRel) {
                                    tracing::warn!(
                                        armed_for_ms = start.elapsed().as_millis() as u64,
                                        "frame clock force-disarmed after idle wakeup loop"
                                    );
                                }
                                *since = None;
                                continue; // 回到 Condvar wait
                            }
                        }
                    }
                    {
                        let mut guard = worker.wake.lock().unwrap();
                        while !worker.armed.load(Ordering::Acquire)
                            && !worker.quit.load(Ordering::Acquire)
                        {
                            guard = worker.cv.wait(guard).unwrap();
                        }
                    }
                    if worker.quit.load(Ordering::Acquire) {
                        return;
                    }
                    let wait_started = Instant::now();
                    let waited = match clock {
                        // SAFETY: no handles, plain timeout wait. A real compositor signal
                        // keeps the display's native cadence; timeout bounds a missing signal
                        // to 60 Hz.
                        Some(wait) => unsafe { wait(0, std::ptr::null(), COMPOSITOR_TIMEOUT_MS) },
                        // Windows 10: block until the next desktop composition pass.
                        // SAFETY: plain FFI call without arguments.
                        None => {
                            if unsafe { DwmFlush() }.is_ok() {
                                0
                            } else {
                                WAIT_FAILED
                            }
                        }
                    };
                    let waited_for = wait_started.elapsed();
                    if waited == WAIT_FAILED || waited_for < IMMEDIATE_WAIT {
                        // On unsupported or fast-returning implementations, prevent a busy loop
                        // while still giving short client tweens several samples.
                        if let Some(left) = FALLBACK_FRAME.checked_sub(waited_for) {
                            std::thread::sleep(left);
                        }
                    }
                    if worker.armed.swap(false, Ordering::AcqRel) {
                        // 正常帧递送：清空 armed_since，等待下一个 request()。
                        *worker.armed_since.lock().unwrap() = None;
                        on_frame();
                    }
                }
            })
            .expect("frame-clock thread");
        Self { shared }
    }

    /// Asks for one more frame callback. Idempotent within a frame.
    pub fn request(&self) {
        let first = !self.shared.armed.swap(true, Ordering::AcqRel);
        // 每次 request() 都刷新 armed_since —— 这样 500ms 自愈从最近一次重入开始计时。
        *self.shared.armed_since.lock().unwrap() = Some(Instant::now());
        if first {
            let _guard = self.shared.wake.lock().unwrap();
            self.shared.cv.notify_one();
        }
    }
}

impl Drop for FrameClock {
    fn drop(&mut self) {
        self.shared.quit.store(true, Ordering::Release);
        let _guard = self.shared.wake.lock().unwrap();
        self.shared.cv.notify_one();
    }
}
