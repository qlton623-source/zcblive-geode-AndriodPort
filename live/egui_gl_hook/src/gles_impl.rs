use crate::Error;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, ViewportId, ViewportInfo};
use egui_glow::glow::{self, HasContext};
use std::{
    ffi::{c_char, c_void, CStr, CString},
    num::NonZeroU32,
    sync::Arc,
    time::Instant,
};

pub type StrCallback = extern "C" fn(*const c_char);
pub type BoolCallback = extern "C" fn(bool);

static mut OPEN_URL_CB: Option<StrCallback> = None;
static mut CLIPBOARD_CB: Option<StrCallback> = None;
static mut IME_CB: Option<BoolCallback> = None;

pub fn set_open_url_callback(cb: StrCallback) {
    unsafe { OPEN_URL_CB = Some(cb) };
}

pub fn set_clipboard_callback(cb: StrCallback) {
    unsafe { CLIPBOARD_CB = Some(cb) };
}

pub fn set_ime_callback(cb: BoolCallback) {
    unsafe { IME_CB = Some(cb) };
}

pub fn set_capture_all_touches(capture: bool) {
    unsafe {
        if let Some(state) = &mut STATE {
            state.capture_all_touches = capture;
        }
    }
}

// These are the GLES entry points needed to save state that glow 0.13.1 does not
// expose through HasContext. GLboolean is an unsigned byte in GLES.
type EglGetProcAddressFn = unsafe extern "C" fn(*const c_char) -> *const c_void;
type GlGetBooleanvFn = unsafe extern "C" fn(u32, *mut u8);
type GlGetVertexAttribivFn = unsafe extern "C" fn(u32, u32, *mut i32);
type GlGetVertexAttribPointervFn = unsafe extern "C" fn(u32, u32, *mut *mut c_void);
type GlVertexAttribPointerFn = unsafe extern "C" fn(u32, i32, u32, u8, i32, *const c_void);

const GL_VERTEX_ATTRIB_ARRAY_ENABLED: u32 = 0x8622;
const GL_VERTEX_ATTRIB_ARRAY_SIZE: u32 = 0x8623;
const GL_VERTEX_ATTRIB_ARRAY_STRIDE: u32 = 0x8624;
const GL_VERTEX_ATTRIB_ARRAY_TYPE: u32 = 0x8625;
const GL_VERTEX_ATTRIB_ARRAY_NORMALIZED: u32 = 0x886A;
const GL_VERTEX_ATTRIB_ARRAY_POINTER: u32 = 0x8645;
const GL_VERTEX_ATTRIB_ARRAY_BUFFER_BINDING: u32 = 0x889F;

#[derive(Clone, Copy)]
struct GlStateApi {
    get_booleanv: Option<GlGetBooleanvFn>,
    get_vertex_attribiv: Option<GlGetVertexAttribivFn>,
    get_vertex_attrib_pointerv: Option<GlGetVertexAttribPointervFn>,
    vertex_attrib_pointer: Option<GlVertexAttribPointerFn>,
}

#[derive(Clone, Copy)]
struct GlStateCaps {
    has_vertex_arrays: bool,
    has_framebuffer_srgb: bool,
    max_vertex_attribs: u32,
}

unsafe fn load_gl_symbol(
    name: &CStr,
    lib_gles: *mut c_void,
    egl_get_proc: Option<EglGetProcAddressFn>,
) -> *const c_void {
    if !lib_gles.is_null() {
        let symbol = libc::dlsym(lib_gles, name.as_ptr());
        if !symbol.is_null() {
            return symbol as *const c_void;
        }
    }

    if let Some(get_proc) = egl_get_proc {
        let symbol = get_proc(name.as_ptr());
        if !symbol.is_null() {
           