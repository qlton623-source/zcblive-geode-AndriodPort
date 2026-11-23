use crate::Error;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, ViewportId, ViewportInfo};
use egui_glow::glow::{self, HasContext};
use std::{
    ffi::{c_char, c_void, CString},
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

struct EguiGlesState {
    egui_ctx: egui::Context,
    painter: egui_glow::Painter,
    gl: Arc<glow::Context>,
    events: Vec<egui::Event>,
    start_time: Instant,
    screen_size: (u32, u32),
    native_ppp: f32,
    last_total_ppp: f32,
    active_pointer: Option<i32>,
    is_changing_scale: bool,
    prev_scale: f32,
    ime_open: bool,
}

static mut STATE: Option<EguiGlesState> = None;

pub fn set_changing_scale(is_changing_scale: bool, prev_scale: f32) {
    unsafe {
        if let Some(state) = &mut STATE {
            state.is_changing_scale = is_changing_scale;
            if is_changing_scale {
                state.prev_scale = prev_scale;
            }
        }
    }
}

pub fn is_init() -> bool {
    unsafe { &STATE }.is_some()
}

pub fn destroy() -> Result<(), Error> {
    let mut state = unsafe {
        match STATE.take() {
            Some(s) => s,
            None => return Err(Error::NotInit),
        }
    };
    if state.ime_open {
        if let Some(cb) = unsafe { IME_CB } {
            cb(false);
        }
    }
    state.painter.destroy();
    Ok(())
}

#[inline]
fn compute_native_ppp(height: u32) -> f32 {
    (height as f32 / 500.0).clamp(1.25, 3.5)
}

pub unsafe fn init_gles(width: u32, height: u32) -> Result<(), Error> {
    if is_init() {
        return Err(Error::AlreadyInit);
    }

    let lib_gles = libc::dlopen(c"libGLESv2.so".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
    let lib_egl = libc::dlopen(c"libEGL.so".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);

    type EglGetProcAddressFn = unsafe extern "C" fn(*const c_char) -> *const c_void;
    let egl_get_proc: Option<EglGetProcAddressFn> = if !lib_egl.is_null() {
        let sym = libc::dlsym(lib_egl, c"eglGetProcAddress".as_ptr());
        if sym.is_null() {
            None
        } else {
            Some(std::mem::transmute(sym))
        }
    } else {
        None
    };

    let gl = Arc::new(glow::Context::from_loader_function_cstr(|s| {
        if !lib_gles.is_null() {
            let sym = libc::dlsym(lib_gles, s.as_ptr());
            if !sym.is_null() {
                return sym;
            }
        }
        if let Some(get_proc) = egl_get_proc {
            let sym = get_proc(s.as_ptr());
            if !sym.is_null() {
                return sym;
            }
        }
        libc::dlsym(libc::RTLD_DEFAULT, s.as_ptr())
    }));

    let painter = egui_glow::Painter::new(gl.clone(), "", None)?;
    let egui_ctx = egui::Context::default();
    let w = width.max(1);
    let h = height.max(1);
    let native_ppp = compute_native_ppp(h);

    STATE = Some(EguiGlesState {
        egui_ctx,
        painter,
        gl,
        events: Vec::new(),
        start_time: Instant::now(),
        screen_size: (w, h),
        native_ppp,
        last_total_ppp: native_ppp,
        active_pointer: None,
        is_changing_scale: false,
        prev_scale: 1.0,
        ime_open: false,
    });

    Ok(())
}

struct GlStateSnapshot {
    program: i32,
    vertex_array: Option<i32>,
    array_buffer: i32,
    element_array_buffer: i32,
    active_texture: i32,
    texture_2d: i32,
    unpack_alignment: i32,
    viewport: [i32; 4],
    scissor_box: [i32; 4],
    scissor_test: bool,
    blend: bool,
    cull_face: bool,
    depth_test: bool,
    stencil_test: bool,
    blend_src_rgb: i32,
    blend_dst_rgb: i32,
    blend_src_alpha: i32,
    blend_dst_alpha: i32,
    blend_eq_rgb: i32,
    blend_eq_alpha: i32,
}

unsafe fn save_gl_state(gl: &glow::Context) -> GlStateSnapshot {
    let mut viewport = [0i32; 4];
    let mut scissor_box = [0i32; 4];
    gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
    gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut scissor_box);

    let vertex_array = if gl.version().major >= 3 {
        Some(gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING))
    } else {
        None
    };

    GlStateSnapshot {
        program: gl.get_parameter_i32(glow::CURRENT_PROGRAM),
        vertex_array,
        array_buffer: gl.get_parameter_i32(glow::ARRAY_BUFFER_BINDING),
        element_array_buffer: gl.get_parameter_i32(glow::ELEMENT_ARRAY_BUFFER_BINDING),
        active_texture: gl.get_parameter_i32(glow::ACTIVE_TEXTURE),
        texture_2d: gl.get_parameter_i32(glow::TEXTURE_BINDING_2D),
        unpack_alignment: gl.get_parameter_i32(glow::UNPACK_ALIGNMENT),
        viewport,
        scissor_box,
        scissor_test: gl.is_enabled(glow::SCISSOR_TEST),
        blend: gl.is_enabled(glow::BLEND),
        cull_face: gl.is_enabled(glow::CULL_FACE),
        depth_test: gl.is_enabled(glow::DEPTH_TEST),
        stencil_test: gl.is_enabled(glow::STENCIL_TEST),
        blend_src_rgb: gl.get_parameter_i32(glow::BLEND_SRC_RGB),
        blend_dst_rgb: gl.get_parameter_i32(glow::BLEND_DST_RGB),
        blend_src_alpha: gl.get_parameter_i32(glow::BLEND_SRC_ALPHA),
        blend_dst_alpha: gl.get_parameter_i32(glow::BLEND_DST_ALPHA),
        blend_eq_rgb: gl.get_parameter_i32(glow::BLEND_EQUATION_RGB),
        blend_eq_alpha: gl.get_parameter_i32(glow::BLEND_EQUATION_ALPHA),
    }
}

unsafe fn restore_gl_state(gl: &glow::Context, s: &GlStateSnapshot) {
    gl.use_program(NonZeroU32::new(s.program as u32).map(glow::NativeProgram));
    if let Some(vao) = s.vertex_array {
        gl.bind_vertex_array(NonZeroU32::new(vao as u32).map(glow::NativeVertexArray));
    }
    gl.bind_buffer(
        glow::ARRAY_BUFFER,
        NonZeroU32::new(s.array_buffer as u32).map(glow::NativeBuffer),
    );
    gl.bind_buffer(
        glow::ELEMENT_ARRAY_BUFFER,
        NonZeroU32::new(s.element_array_buffer as u32).map(glow::NativeBuffer),
    );
    gl.active_texture(s.active_texture as u32);
    gl.bind_texture(
        glow::TEXTURE_2D,
        NonZeroU32::new(s.texture_2d as u32).map(glow::NativeTexture),
    );
    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, s.unpack_alignment.max(1));
    gl.viewport(s.viewport[0], s.viewport[1], s.viewport[2], s.viewport[3]);
    gl.scissor(
        s.scissor_box[0],
        s.scissor_box[1],
        s.scissor_box[2],
        s.scissor_box[3],
    );
    if s.scissor_test {
        gl.enable(glow::SCISSOR_TEST);
    } else {
        gl.disable(glow::SCISSOR_TEST);
    }
    if s.blend {
        gl.enable(glow::BLEND);
    } else {
        gl.disable(glow::BLEND);
    }
    if s.cull_face {
        gl.enable(glow::CULL_FACE);
    } else {
        gl.disable(glow::CULL_FACE);
    }
    if s.depth_test {
        gl.enable(glow::DEPTH_TEST);
    } else {
        gl.disable(glow::DEPTH_TEST);
    }
    if s.stencil_test {
        gl.enable(glow::STENCIL_TEST);
    } else {
        gl.disable(glow::STENCIL_TEST);
    }
    gl.blend_func_separate(
        s.blend_src_rgb as u32,
        s.blend_dst_rgb as u32,
        s.blend_src_alpha as u32,
        s.blend_dst_alpha as u32,
    );
    gl.blend_equation_separate(s.blend_eq_rgb as u32, s.blend_eq_alpha as u32);
}

pub unsafe fn paint_gles(
    width: u32,
    height: u32,
    run_fn: Box<dyn Fn(&egui::Context)>,
) -> Result<(), Error> {
    let state = match &mut STATE {
        Some(s) => s,
        None => return Err(Error::NotInit),
    };

    state.screen_size = (width.max(1), height.max(1));
    state.native_ppp = compute_native_ppp(state.screen_size.1);
    let zoom_factor = state.egui_ctx.zoom_factor();
    let effective_ppp = (state.native_ppp * zoom_factor).max(0.5);
    state.last_total_ppp = effective_ppp;

    let mut viewports = egui::ViewportIdMap::default();
    viewports.insert(
        ViewportId::ROOT,
        ViewportInfo {
            native_pixels_per_point: Some(state.native_ppp),
            ..Default::default()
        },
    );

    let raw_input = RawInput {
        modifiers: Modifiers::NONE,
        events: std::mem::take(&mut state.events),
        screen_rect: Some(Rect {
            min: Pos2::ZERO,
            max: Pos2::new(
                state.screen_size.0 as f32 / effective_ppp,
                state.screen_size.1 as f32 / effective_ppp,
            ),
        }),
        time: Some(state.start_time.elapsed().as_secs_f64()),
        max_texture_side: None,
        predicted_dt: 1.0 / 60.0,
        hovered_files: vec![],
        dropped_files: vec![],
        focused: true,
        viewports,
        ..Default::default()
    };

    let egui::FullOutput {
        platform_output,
        mut textures_delta,
        shapes,
        pixels_per_point,
        viewport_output: _,
    } = state.egui_ctx.run(raw_input, &*run_fn);

    state.last_total_ppp = pixels_per_point;

    let wants_ime = state.egui_ctx.wants_keyboard_input();
    if wants_ime != state.ime_open {
        state.ime_open = wants_ime;
        if let Some(cb) = IME_CB {
            cb(wants_ime);
        }
    }

    if let Some(open_url) = platform_output.open_url {
        if let Some(cb) = OPEN_URL_CB {
            if let Ok(cstr) = CString::new(open_url.url) {
                cb(cstr.as_ptr());
            }
        }
    }
    if !platform_output.copied_text.is_empty() {
        if let Some(cb) = CLIPBOARD_CB {
            if let Ok(cstr) = CString::new(platform_output.copied_text) {
                cb(cstr.as_ptr());
            }
        }
    }

    let gl_snapshot = save_gl_state(&state.gl);

    for (id, image_delta) in textures_delta.set {
        state.painter.set_texture(id, &image_delta);
    }

    let clipped_primitives = state.egui_ctx.tessellate(shapes, pixels_per_point);
    state.painter.paint_primitives(
        [state.screen_size.0, state.screen_size.1],
        pixels_per_point,
        &clipped_primitives,
    );

    for id in textures_delta.free.drain(..) {
        state.painter.free_texture(id);
    }

    restore_gl_state(&state.gl, &gl_snapshot);

    Ok(())
}

fn map_pos(state: &EguiGlesState, x: f32, y: f32) -> Pos2 {
    let scale = if state.is_changing_scale {
        (state.native_ppp * state.prev_scale).max(0.5)
    } else {
        state.last_total_ppp.max(0.5)
    };
    Pos2::new(x / scale, y / scale)
}

/// action: 0 = BEGAN, 1 = MOVED, 2 = ENDED, 3 = CANCELLED
/// Returns true if egui consumed the touch event (should not pass to GD).
pub fn on_touch(action: u8, pointer_id: i32, x: f32, y: f32) -> bool {
    let state = unsafe {
        match &mut STATE {
            Some(s) => s,
            None => return false,
        }
    };

    let pos = map_pos(state, x, y);

    match action {
        0 => {
            if state.active_pointer.is_some() {
                return false;
            }
            let over_egui = state.egui_ctx.layer_id_at(pos).is_some();
            if !over_egui {
                return false;
            }
            state.active_pointer = Some(pointer_id);
            state.events.push(Event::PointerMoved(pos));
            state.events.push(Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            });
            true
        }
        1 => {
            if state.active_pointer != Some(pointer_id) {
                return false;
            }
            state.events.push(Event::PointerMoved(pos));
            true
        }
        2 | 3 => {
            if state.active_pointer != Some(pointer_id) {
                return false;
            }
            state.active_pointer = None;
            state.events.push(Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            });
            state.events.push(Event::PointerGone);
            true
        }
        _ => false,
    }
}

pub fn on_text(text: &str) -> bool {
    let state = unsafe {
        match &mut STATE {
            Some(s) => s,
            None => return false,
        }
    };
    if !state.egui_ctx.wants_keyboard_input() {
        return false;
    }
    state.events.push(Event::Text(text.to_owned()));
    true
}

pub fn on_key(cocos_key: i32, pressed: bool, repeat: bool) -> bool {
    let state = unsafe {
        match &mut STATE {
            Some(s) => s,
            None => return false,
        }
    };

    if let Some(key) = map_cocos_key(cocos_key) {
        state.events.push(Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat,
            modifiers: Modifiers::NONE,
        });
    }
    state.egui_ctx.wants_keyboard_input()
}

pub fn on_scroll(dx: f32, dy: f32) -> bool {
    let state = unsafe {
        match &mut STATE {
            Some(s) => s,
            None => return false,
        }
    };
    if !state.egui_ctx.is_pointer_over_area() && !state.egui_ctx.wants_pointer_input() {
        return false;
    }
    state.events.push(Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(dx, dy),
        modifiers: Modifiers::NONE,
    });
    true
}

/// Maps Cocos2d-x `enumKeyCodes` (from CCKeyboardDelegate.h) to egui::Key.
fn map_cocos_key(code: i32) -> Option<Key> {
    match code {
        0x08 => Some(Key::Backspace),
        0x09 => Some(Key::Tab),
        0x0D | 0x1009 => Some(Key::Enter),
        0x1B => Some(Key::Escape),
        0x20 => Some(Key::Space),
        0x21 => Some(Key::PageUp),
        0x22 => Some(Key::PageDown),
        0x23 => Some(Key::End),
        0x24 => Some(Key::Home),
        0x25 | 0x11D => Some(Key::ArrowLeft),
        0x26 | 0x11B => Some(Key::ArrowUp),
        0x27 | 0x11E => Some(Key::ArrowRight),
        0x28 | 0x11C => Some(Key::ArrowDown),
        0x2D => Some(Key::Insert),
        0x2E => Some(Key::Delete),
        0x30 | 0x60 => Some(Key::Num0),
        0x31 | 0x61 => Some(Key::Num1),
        0x32 | 0x62 => Some(Key::Num2),
        0x33 | 0x63 => Some(Key::Num3),
        0x34 | 0x64 => Some(Key::Num4),
        0x35 | 0x65 => Some(Key::Num5),
        0x36 | 0x66 => Some(Key::Num6),
        0x37 | 0x67 => Some(Key::Num7),
        0x38 | 0x68 => Some(Key::Num8),
        0x39 | 0x69 => Some(Key::Num9),
        0x41 => Some(Key::A),
        0x42 => Some(Key::B),
        0x43 => Some(Key::C),
        0x44 => Some(Key::D),
        0x45 => Some(Key::E),
        0x46 => Some(Key::F),
        0x47 => Some(Key::G),
        0x48 => Some(Key::H),
        0x49 => Some(Key::I),
        0x4A => Some(Key::J),
        0x4B => Some(Key::K),
        0x4C => Some(Key::L),
        0x4D => Some(Key::M),
        0x4E => Some(Key::N),
        0x4F => Some(Key::O),
        0x50 => Some(Key::P),
        0x51 => Some(Key::Q),
        0x52 => Some(Key::R),
        0x53 => Some(Key::S),
        0x54 => Some(Key::T),
        0x55 => Some(Key::U),
        0x56 => Some(Key::V),
        0x57 => Some(Key::W),
        0x58 => Some(Key::X),
        0x59 => Some(Key::Y),
        0x5A => Some(Key::Z),
        0x70 => Some(Key::F1),
        0x71 => Some(Key::F2),
        0x72 => Some(Key::F3),
        0x73 => Some(Key::F4),
        0x74 => Some(Key::F5),
        0x75 => Some(Key::F6),
        0x76 => Some(Key::F7),
        0x77 => Some(Key::F8),
        0x78 => Some(Key::F9),
        0x79 => Some(Key::F10),
        0x7A => Some(Key::F11),
        0x7B => Some(Key::F12),
        0xB8 => Some(Key::Plus),
        0xBC => Some(Key::Comma),
        0xBD => Some(Key::Minus),
        0xBE => Some(Key::Period),
        0x1000 => Some(Key::Backtick),
        0x1001 | 0x1008 => Some(Key::Equals),
        0x1002 => Some(Key::OpenBracket),
        0x1003 => Some(Key::CloseBracket),
        0x1004 => Some(Key::Backslash),
        0x1005 => Some(Key::Semicolon),
        0x1007 => Some(Key::Slash),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_ppp_clamps_across_mobile_resolutions() {
        assert!((compute_native_ppp(480) - 1.25).abs() < 1e-6);
        assert!((compute_native_ppp(1080) - 2.16).abs() < 1e-5);
        assert!((compute_native_ppp(2400) - 3.5).abs() < 1e-6);
    }

    #[test]
    fn maps_geode_5_11_2_cocos_keycodes() {
        assert_eq!(map_cocos_key(0x11B), Some(Key::ArrowUp));
        assert_eq!(map_cocos_key(0x11C), Some(Key::ArrowDown));
        assert_eq!(map_cocos_key(0x11D), Some(Key::ArrowLeft));
        assert_eq!(map_cocos_key(0x11E), Some(Key::ArrowRight));
        assert_eq!(map_cocos_key(0x61), Some(Key::Num1));
        assert_eq!(map_cocos_key(0x1009), Some(Key::Enter));
        assert_eq!(map_cocos_key(0x1000), Some(Key::Backtick));
        assert_eq!(map_cocos_key(0x08), Some(Key::Backspace));
    }
}
