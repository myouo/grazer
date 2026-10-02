//! C ABI v1. See the distributed `include/grazer.h` for layouts and ownership.
use crate::{Bullet, Config, DrawSprite, Fixed, Input, Runtime};
use std::panic::{AssertUnwindSafe, catch_unwind};
mod game;
pub use game::*;

pub const ABI_VERSION: u32 = 1;
pub const OK: i32 = 0;
pub const INVALID_ARGUMENT: i32 = 1;
pub const VERSION_MISMATCH: i32 = 2;
pub const BUFFER_TOO_SMALL: i32 = 3;
pub const RUNTIME_ERROR: i32 = 4;
pub const PANIC: i32 = 5;

#[repr(C)]
pub struct GrazerConfig {
    pub abi_version: u32,
    pub struct_size: u32,
    pub width_q16: i32,
    pub height_q16: i32,
    pub capacity: u32,
    pub seed: u64,
}
#[repr(C)]
pub struct GrazerBullet {
    pub x_q16: i32,
    pub y_q16: i32,
    pub vx_q16: i32,
    pub vy_q16: i32,
    pub radius_q16: i32,
    pub rgba: u32,
}
pub struct GrazerRuntime {
    inner: Runtime,
}

fn guard(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(PANIC)
}

#[unsafe(no_mangle)]
pub extern "C" fn grazer_abi_version() -> u32 {
    ABI_VERSION
}

/// # Safety
/// `config` points to a readable GrazerConfig; `out` points to writable pointer
/// storage. Neither may overlap. A successful handle must be destroyed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_create(
    config: *const GrazerConfig,
    out: *mut *mut GrazerRuntime,
) -> i32 {
    guard(|| {
        if out.is_null() {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller supplies valid, writable output storage.
        unsafe {
            *out = std::ptr::null_mut();
        }
        if config.is_null() {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller supplies a readable config with the documented layout.
        let config = unsafe { &*config };
        if config.abi_version != ABI_VERSION
            || config.struct_size as usize != std::mem::size_of::<GrazerConfig>()
        {
            return VERSION_MISMATCH;
        }
        match Runtime::new(
            Config {
                width: Fixed::from_bits(config.width_q16),
                height: Fixed::from_bits(config.height_q16),
                capacity: config.capacity,
            },
            config.seed,
        ) {
            Ok(inner) => {
                // SAFETY: out was checked and its storage is valid by contract.
                unsafe {
                    *out = Box::into_raw(Box::new(GrazerRuntime { inner }));
                }
                OK
            }
            Err(_) => INVALID_ARGUMENT,
        }
    })
}

/// # Safety
/// Handle must be null or a live handle returned by grazer_create, exclusively
/// owned by this call. It must never be accessed or destroyed again afterward.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_destroy(runtime: *mut GrazerRuntime) {
    if !runtime.is_null() {
        // SAFETY: caller transfers sole ownership of a live Box allocation.
        drop(unsafe { Box::from_raw(runtime) });
    }
}

/// # Safety
/// Handle must be live, with exclusive access for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_set_input(runtime: *mut GrazerRuntime, input: Input) -> i32 {
    guard(|| {
        // SAFETY: non-null pointers must identify exclusively borrowed handles.
        let Some(runtime) = (unsafe { runtime.as_mut() }) else {
            return INVALID_ARGUMENT;
        };
        runtime
            .inner
            .set_input(input)
            .map_or(INVALID_ARGUMENT, |_| OK)
    })
}

/// # Safety
/// Handle must be live and exclusively borrowed. `bullets` is readable for
/// count elements and does not alias the handle; it may be null only at count 0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_spawn(
    runtime: *mut GrazerRuntime,
    bullets: *const GrazerBullet,
    count: u32,
) -> i32 {
    guard(|| {
        // SAFETY: pointer validity/exclusivity is required by the ABI.
        let Some(runtime) = (unsafe { runtime.as_mut() }) else {
            return INVALID_ARGUMENT;
        };
        if count == 0 {
            return OK;
        }
        if bullets.is_null() || count > runtime.inner.config().capacity {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller guarantees count readable, aligned elements.
        let source = unsafe { std::slice::from_raw_parts(bullets, count as usize) };
        let values: Vec<_> = source
            .iter()
            .map(|b| Bullet {
                x: Fixed::from_bits(b.x_q16),
                y: Fixed::from_bits(b.y_q16),
                vx: Fixed::from_bits(b.vx_q16),
                vy: Fixed::from_bits(b.vy_q16),
                radius: Fixed::from_bits(b.radius_q16),
                rgba: b.rgba,
            })
            .collect();
        runtime
            .inner
            .spawn_batch(&values)
            .map_or(RUNTIME_ERROR, |_| OK)
    })
}

/// # Safety
/// Handle must be live, with exclusive access for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_step(runtime: *mut GrazerRuntime) -> i32 {
    guard(|| {
        // SAFETY: caller guarantees live exclusive handle or null.
        let Some(runtime) = (unsafe { runtime.as_mut() }) else {
            return INVALID_ARGUMENT;
        };
        runtime.inner.step().map_or(RUNTIME_ERROR, |_| OK)
    })
}

/// # Safety
/// Handle must be live; `required` writable; `out` writable for capacity sprites
/// or null when capacity is zero. All regions must be nonoverlapping. No concurrent
/// mutation is allowed. On insufficient capacity, no sprites are written.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_snapshot(
    runtime: *const GrazerRuntime,
    out: *mut DrawSprite,
    capacity: u32,
    required: *mut u32,
) -> i32 {
    guard(|| {
        if required.is_null() {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller guarantees readable handle or null.
        let Some(runtime) = (unsafe { runtime.as_ref() }) else {
            return INVALID_ARGUMENT;
        };
        let count = runtime.inner.bullet_count() as u32 + 1;
        // SAFETY: caller supplies writable nonoverlapping required storage.
        unsafe {
            *required = count;
        }
        if capacity < count {
            return BUFFER_TOO_SMALL;
        }
        if out.is_null() {
            return INVALID_ARGUMENT;
        }
        for (index, sprite) in runtime.inner.sprites().enumerate() {
            // SAFETY: capacity >= count and caller guarantees writable storage.
            unsafe {
                out.add(index).write(sprite);
            }
        }
        OK
    })
}

/// # Safety
/// Handle must be live and readable; out points to nonoverlapping writable u64
/// storage. No concurrent mutation is allowed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_state_hash(runtime: *const GrazerRuntime, out: *mut u64) -> i32 {
    guard(|| {
        if out.is_null() {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller guarantees readable handle or null.
        let Some(runtime) = (unsafe { runtime.as_ref() }) else {
            return INVALID_ARGUMENT;
        };
        // SAFETY: out is writable and does not alias the runtime by contract.
        unsafe {
            *out = runtime.inner.state_hash();
        }
        OK
    })
}
