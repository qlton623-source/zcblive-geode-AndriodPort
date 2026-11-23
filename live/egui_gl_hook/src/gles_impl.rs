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
            return symbol;
        }
    }

    libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) as *const c_void
}

unsafe fn load_gl_state_api(
    lib_gles: *mut c_void,
    egl_get_proc: Option<EglGetProcAddressFn>,
) -> GlStateApi {
    let get_booleanv = load_gl_symbol(c"glGetBooleanv", lib_gles, egl_get_proc);
    let get_vertex_attribiv = load_gl_symbol(c"glGetVertexAttribiv", lib_gles, egl_get_proc);
    let get_vertex_attrib_pointerv =
        load_gl_symbol(c"glGetVertexAttribPointerv", lib_gles, egl_get_proc);
    let vertex_attrib_pointer =
        load_gl_symbol(c"glVertexAttribPointer", lib_gles, egl_get_proc);

    GlStateApi {
        get_booleanv: if get_booleanv.is_null() {
            None
        } else {
            Some(std::mem::transmute::<*const c_void, GlGetBooleanvFn>(get_booleanv))
        },
        get_vertex_attribiv: if get_vertex_attribiv.is_null() {
            None
        } else {
            Some(std::mem::transmute::<*const c_void, GlGetVertexAttribivFn>(get_vertex_attribiv))
        },
        get_vertex_attrib_pointerv: if get_vertex_attrib_pointerv.is_null() {
            None
        } else {
            Some(std::mem::transmute::<
                *const c_void,
                GlGetVertexAttribPointervFn,
            >(get_vertex_attrib_pointerv))
        },
        vertex_attrib_pointer: if vertex_attrib_pointer.is_null() {
            None
        } else {
            Some(std::mem::transmute::<*const c_void, GlVertexAttribPointerFn>(
                vertex_attrib_pointer,
            ))
        },
    }
}

struct EguiGlesState {
    egui_ctx: egui::Context,
    painter: egui_glow::Painter,
    gl: Arc<glow::Context>,
    gl_api: GlStateApi,
    gl_caps: GlStateCaps,
    events: Vec<egui::Event>,
    start_time: Instant,
    screen_size: (u32, u32),
    native_ppp: f32,
    last_total_ppp: f32,
    active_pointer: Option<i32>,
    active_pos: Option<Pos2>,
    capture_all_touches: bool,
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

// Match egui_glow 0.27.2's own VAO detection. When GLES2 has no
// OES_vertex_array_object, egui_glow emulates a VAO by changing global vertex
// attribute pointers/enables, so those must be explicitly saved and restored.
fn supports_vertex_arrays(gl: &glow::Context) -> bool {
    const WEBGL_PREFIX: &str = "WebGL ";
    const OPENGL_ES_PREFIX: &str = "OpenGL ES ";

    let version_string = unsafe { gl.get_parameter_string(glow::VERSION) };

    if let Some(pos) = version_string.rfind(WEBGL_PREFIX) {
        let version_str = &version_string[pos + WEBGL_PREFIX.len()..];
        if version_str.contains("1.0") {
            let extensions = gl.supported_extensions();
            extensions.contains("OES_vertex_array_object")
                || extensions.contains("GL_OES_vertex_array_object")
        } else {
            true
        }
    } else if version_string.contains(OPENGL_ES_PREFIX) {
        if version_string.contains("2.0") {
            let extensions = gl.supported_extensions();
            extensions.contains("OES_vertex_array_object")
                || extensions.contains("GL_OES_vertex_array_object")
        } else {
            true
        }
    } else if version_string.starts_with('2') {
        let extensions = gl.supported_extensions();
        extensions.contains("ARB_vertex_array_object")
            || extensions.contains("GL_ARB_vertex_array_object")
    } else {
        true
    }
}

#[derive(Clone, Copy)]
struct VertexAttribSnapshot {
    index: u32,
    enabled: bool,
    size: i32,
    data_type: u32,
    normalized: bool,
    stride: i32,
    buffer_binding: i32,
    pointer: *mut c_void,
}

struct GlStateSnapshot {
    program: i32,
    vertex_array: Option<i32>,
    vertex_attribs: Option<Vec<VertexAttribSnapshot>>,
    array_buffer: i32,
    element_array_buffer: i32,
    active_texture: i32,
    active_texture_binding: i32,
    texture0_binding: i32,
    unpack_alignment: i32,
    viewport: [i32; 4],
    scissor_box: [i32; 4],
    color_mask: [bool; 4],
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
    framebuffer_srgb: Option<bool>,
}

unsafe fn save_vertex_attribs(
    api: &GlStateApi,
    max_vertex_attribs: u32,
) -> Option<Vec<VertexAttribSnapshot>> {
    let get_attrib = api.get_vertex_attribiv?;
    let get_pointer = api.get_vertex_attrib_pointerv?;

    let mut attributes = Vec::with_capacity(max_vertex_attribs as usize);

    for index in 0..max_vertex_attribs {
        let mut enabled = 0;
        let mut size = 4;
        let mut data_type = glow::FLOAT as i32;
        let mut normalized = 0;
        let mut stride = 0;
        let mut buffer_binding = 0;
        let mut pointer: *mut c_void = std::ptr::null_mut();

        get_attrib(index, GL_VERTEX_ATTRIB_ARRAY_ENABLED, &mut enabled);
        get_attrib(index, GL_VERTEX_ATTRIB_ARRAY_SIZE, &mut size);
        get_attrib(index, GL_VERTEX_ATTRIB_ARRAY_TYPE, &mut data_type);
        get_attrib(index, GL_VERTEX_ATTRIB_ARRAY_NORMALIZED, &mut normalized);
        get_attrib(index, GL_VERTEX_ATTRIB_ARRAY_STRIDE, &mut stride);
        get_attrib(index, GL_VERTEX_ATTRIB_ARRAY_BUFFER_BINDING, &mut buffer_binding);
        get_pointer(index, GL_VERTEX_ATTRIB_ARRAY_POINTER, &mut pointer);

        attributes.push(VertexAttribSnapshot {
            index,
            enabled: enabled != 0,
            size,
            data_type: data_type as u32,
            normalized: normalized != 0,
            stride,
            buffer_binding,
            pointer,
        });
    }

    Some(attributes)
}

unsafe fn restore_vertex_attribs(
    gl: &glow::Context,
    api: &GlStateApi,
    attributes: &[VertexAttribSnapshot],
) {
    let Some(vertex_attrib_pointer) = api.vertex_attrib_pointer else {
        return;
    };

    for attribute in attributes {
        gl.bind_buffer(
            glow::ARRAY_BUFFER,
            NonZeroU32::new(attribute.buffer_binding as u32).map(glow::NativeBuffer),
        );
        vertex_attrib_pointer(
            attribute.index,
            attribute.size,
            attribute.data_type,
            attribute.normalized as u8,
            attribute.stride,
            attribute.pointer as *const c_void,
        );
        if attribute.enabled {
            gl.enable_vertex_attrib_array(attribute.index);
        } else {
            gl.disable_vertex_attrib_array(attribute.index);
        }
    }
}

unsafe fn save_gl_state(
    gl: &glow::Context,
    api: &GlStateApi,
    caps: &GlStateCaps,
) -> GlStateSnapshot {
    let mut viewport = [0i32; 4];
    let mut scissor_box = [0i32; 4];
    gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
    gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut scissor_box);

    let vertex_array = if caps.has_vertex_arrays {
        Some(gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING))
    } else {
        None
    };

    let vertex_attribs = if vertex_array.is_none() {
        save_vertex_attribs(api, caps.max_vertex_attribs)
    } else {
        None
    };

    let mut color_mask_bytes = [0u8; 4];
    if let Some(get_booleanv) = api.get_booleanv {
        get_booleanv(glow::COLOR_WRITEMASK, color_mask_bytes.as_mut_ptr());
    }

    let active_texture = gl.get_parameter_i32(glow::ACTIVE_TEXTURE);
    let active_texture_binding = gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);
    gl.active_texture(glow::TEXTURE0);
    let texture0_binding = gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);
    gl.active_texture(active_texture as u32);

    let framebuffer_srgb = if caps.has_framebuffer_srgb {
        Some(gl.is_enabled(glow::FRAMEBUFFER_SRGB))
    } else {
        None
    };

    GlStateSnapshot {
        program: gl.get_parameter_i32(glow::CURRENT_PROGRAM),
        vertex_array,
        vertex_attribs,
        array_buffer: gl.get_parameter_i32(glow::ARRAY_BUFFER_BINDING),
        element_array_buffer: gl.get_parameter_i32(glow::ELEMENT_ARRAY_BUFFER_BINDING),
        active_texture,
        active_texture_binding,
        texture0_binding,
        unpack_alignment: gl.get_parameter_i32(glow::UNPACK_ALIGNMENT),
        viewport,
        scissor_box,
        color_mask: color_mask_bytes.map(|value| value != 0),
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
        framebuffer_srgb,
    }
}

unsafe fn restore_gl_state(gl: &glow::Context, api: &GlStateApi, state: &GlStateSnapshot) {
    gl.use_program(NonZeroU32::new(state.program as u32).map(glow::NativeProgram));

    if let Some(vao) = state.vertex_array {
        gl.bind_vertex_array(NonZeroU32::new(vao as u32).map(glow::NativeVertexArray));
    }

    if let Some(attributes) = &state.vertex_attribs {
        restore_vertex_attribs(gl, api, attributes);
    }

    gl.bind_buffer(
        glow::ARRAY_BUFFER,
        NonZeroU32::new(state.array_buffer as u32).map(glow::NativeBuffer),
    );
    gl.bind_buffer(
        glow::ELEMENT_ARRAY_BUFFER,
        NonZeroU32::new(state.element_array_buffer as u32).map(glow::NativeBuffer),
    );

    gl.active_texture(glow::TEXTURE0);
    gl.bind_texture(
        glow::TEXTURE_2D,
        NonZeroU32::new(state.texture0_binding as u32).map(glow::NativeTexture),
    );
    if state.active_texture as u32 != glow::TEXTURE0 {
        gl.active_texture(state.active_texture as u32);
        gl.bind_texture(
            glow::TEXTURE_2D,
            NonZeroU32::new(state.active_texture_binding as u32).map(glow::NativeTexture),
        );
    }
    gl.active_texture(state.active_texture as u32);

    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, state.unpack_alignment.max(1));
    gl.viewport(
        state.viewport[0],
        state.viewport[1],
        state.viewport[2],
        state.viewport[3],
    );
    gl.scissor(
        state.scissor_box[0],
        state.scissor_box[1],
        state.scissor_box[2],
        state.scissor_box[3],
    );
    gl.color_mask(
        state.color_mask[0],
        state.color_mask[1],
        state.color_mask[2],
        state.color_mask[3],
    );

    if state.scissor_test {
        gl.enable(glow::SCISSOR_TEST);
    } else {
        gl.disable(glow::SCISSOR_TEST);
    }
    if state.blend {
        gl.enable(glow::BLEND);
    } else {
        gl.disable(glow::BLEND);
    }
    if state.cull_face {
        gl.enable(glow::CULL_FACE);
    } else {
        gl.disable(glow::CULL_FACE);
    }
    if state.depth_test {
        gl.enable(glow::DEPTH_TEST);
    } else {
        gl.disable(glow::DEPTH_TEST);
    }
    if state.stencil_test {
        gl.enable(glow::STENCIL_TEST);
    } else {
        gl.disable(glow::STENCIL_TEST);
    }

    gl.blend_func_separate(
        state.blend_src_rgb as u32,
        state.blend_dst_rgb as u32,
        state.blend_src_alpha as u32,
        state.blend_dst_alpha as u32,
    );
    gl.blend_equation_separate(
        state.blend_eq_rgb as u32,
        state.blend_eq_alpha as u32,
    );

    if let Some(enabled) = state.framebuffer_srgb {
        if enabled {
            gl.enable(glow::FRAMEBUFFER_SRGB);
        } else {
            gl.disable(glow::FRAMEBUFFER_SRGB);
        }
    }
}

pub unsafe fn init_gles(width: u32, height: u32) -> Result<(), Error> {
    if is_init() {
        return Err(Error::AlreadyInit);
    }

    let lib_gles = libc::dlopen(c"libGLESv2.so".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
    let lib_egl = libc::dlopen(c"libEGL.so".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);

    let egl_get_proc: Option<EglGetProcAddressFn> = if !lib_egl.is_null() {
        let symbol = libc::dlsym(lib_egl, c"eglGetProcAddress".as_ptr());
        if symbol.is_null() {
            None
        } else {
            Some(std::mem::transmute::<*mut c_void, EglGetProcAddressFn>(symbol))
        }
    } else {
        None
    };

    let gl = Arc::new(glow::Context::from_loader_function_cstr(|name| unsafe {
        load_gl_symbol(name, lib_gles, egl_get_proc)
    }));
    let gl_api = load_gl_state_api(lib_gles, egl_get_proc);

    if gl_api.get_booleanv.is_none() {
        return Err(Error::CtxCreate);
    }

    let has_vertex_arrays = supports_vertex_arrays(&gl);
    if !has_vertex_arrays
        && (gl_api.get_vertex_attribiv.is_none()
            || gl_api.get_vertex_attrib_pointerv.is_none()
            || gl_api.vertex_attrib_pointer.is_none())
    {
        return Err(Error::CtxCreate);
    }

    let extensions = gl.supported_extensions();
    let has_framebuffer_srgb = extensions
        .iter()
        .any(|extension| extension.ends_with("ARB_framebuffer_sRGB"));
    let max_vertex_attribs = if has_vertex_arrays {
        0
    } else {
        gl.get_parameter_i32(glow::MAX_VERTEX_ATTRIBS).max(0) as u32
    };
    let gl_caps = GlStateCaps {
        has_vertex_arrays,
        has_framebuffer_srgb,
        max_vertex_attribs,
    };

    // Painter::new creates buffers and configures its VAO (or emulated vertex
    // attributes on GLES2). Preserve the game's state around construction too.
    let init_gl_state = save_gl_state(&gl, &gl_api, &gl_caps);
    let painter_result = egui_glow::Painter::new(gl.clone(), "", None);
    restore_gl_state(&gl, &gl_api, &init_gl_state);
    let painter = painter_result?;

    let egui_ctx = egui::Context::default();
    let w = width.max(1);
    let h = height.max(1);
    let native_ppp = compute_native_ppp(h);

    STATE = Some(EguiGlesState {
        egui_ctx,
        painter,
        gl,
        gl_api,
        gl_caps,
        events: Vec::new(),
        start_time: Instant::now(),
        screen_size: (w, h),
        native_ppp,
        last_total_ppp: native_ppp,
        active_pointer: None,
        active_pos: None,
        capture_all_touches: false,
        is_changing_scale: false,
        prev_scale: 1.0,
        ime_open: false,
    });

    Ok(())
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

    let gl_snapshot = save_gl_state(&state.gl, &state.gl_api, &state.gl_caps);

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

    restore_gl_state(&state.gl, &state.gl_api, &gl_snapshot);

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
            // Recover from a previous touch whose release was missed.
            if let Some(previous_pos) = state.active_pos.take() {
                state.events.push(Event::PointerButton {
                    pos: previous_pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                });

                state.events.push(Event::PointerGone);
            }

            state.active_pointer = None;

            // egui registers a full-screen background layer every frame.
            // Therefore `layer_id_at(pos).is_some()` is NOT enough to know
            // that the touch is over our UI: outside every window it returns
            // `LayerId::background()`. Treat that layer exactly like no hit,
            // otherwise every Android touch is consumed and Geometry Dash's
            // own buttons never receive it.
            let hit_layer = state.egui_ctx.layer_id_at(pos);
            let is_over_egui_ui = hit_layer
                .is_some_and(|layer| layer != egui::LayerId::background());

            if !state.capture_all_touches && !is_over_egui_ui {
                return false;
            }

            state.active_pointer = Some(pointer_id);
            state.active_pos = Some(pos);

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

            state.active_pos = Some(pos);
            state.events.push(Event::PointerMoved(pos));
            true
        }

        2 | 3 => {
            if state.active_pointer != Some(pointer_id) {
                return false;
            }

            state.active_pointer = None;
            state.active_pos = None;

            state.events.push(Event::PointerMoved(pos));
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