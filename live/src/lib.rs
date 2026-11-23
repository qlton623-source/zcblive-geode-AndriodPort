#![allow(static_mut_refs)]

mod bot;
mod clickpack;
mod utils;

use bot::{Bot, BOT};
use clickpack::Button;
use once_cell::sync::Lazy;
use std::{
    ffi::{c_char, c_void, CStr},
    path::PathBuf,
    sync::Once,
};

#[no_mangle]
unsafe extern "C" fn zcblive_on_swap_buffers(width: u32, height: u32) {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        log::info!("GLES drawScene overlay active ({width}x{height})");
    });

    if !egui_gl_hook::is_init() {
        if let Err(e) = egui_gl_hook::init_gles(width, height) {
            log::error!("init_gles() failed: {e}");
            return;
        }
    }

    if let Err(e) = egui_gl_hook::paint_gles(
        width,
        height,
        Box::new(|ctx| {
            BOT.draw_ui(ctx);
        }),
    ) {
        log::error!("paint_gles() failed: {e}");
    }

    // Keep touch capture in sync with the menu visibility after egui
    // has processed this frame.
    egui_gl_hook::set_capture_all_touches(!BOT.conf.hidden);
}

#[no_mangle]
unsafe extern "C" fn zcblive_on_touch(action: u8, pointer_id: i32, x: f32, y: f32) -> bool {
    egui_gl_hook::on_touch(action, pointer_id, x, y)
}

#[no_mangle]
unsafe extern "C" fn zcblive_on_key(cocos_key: i32, pressed: bool, repeat: bool) -> bool {
    egui_gl_hook::on_key(cocos_key, pressed, repeat)
}

#[no_mangle]
unsafe extern "C" fn zcblive_on_text(text: *const c_char) -> bool {
    if text.is_null() {
        return false;
    }
    let Ok(s) = CStr::from_ptr(text).to_str() else {
        return false;
    };
    egui_gl_hook::on_text(s)
}

#[no_mangle]
unsafe extern "C" fn zcblive_on_scroll(dx: f32, dy: f32) -> bool {
    egui_gl_hook::on_scroll(dx, dy)
}

#[no_mangle]
unsafe extern "C" fn zcblive_set_open_url_cb(cb: extern "C" fn(*const c_char)) {
    egui_gl_hook::set_open_url_callback(cb);
}

#[no_mangle]
unsafe extern "C" fn zcblive_set_clipboard_cb(cb: extern "C" fn(*const c_char)) {
    egui_gl_hook::set_clipboard_callback(cb);
}

#[no_mangle]
unsafe extern "C" fn zcblive_set_open_folder_cb(cb: extern "C" fn(*const c_char)) {
    BOT.open_folder_cb = Some(cb);
}

#[no_mangle]
unsafe extern "C" fn zcblive_set_ime_cb(cb: extern "C" fn(bool)) {
    egui_gl_hook::set_ime_callback(cb);
}

#[no_mangle]
unsafe extern "C" fn zcblive_set_save_dir(dir: *const c_char) {
    if dir.is_null() {
        return;
    }
    if let Ok(s) = CStr::from_ptr(dir).to_str() {
        utils::set_save_dir(PathBuf::from(s));
    }
}

#[no_mangle]
unsafe extern "C" fn zcblive_on_fmod_ready(system: *mut c_void) {
    BOT.on_fmod_ready(system as *mut gfmod::FMOD_SYSTEM);
}

#[no_mangle]
unsafe extern "C" fn zcblive_initialize() {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("zcblive"),
    );

    let panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("ZCB Live panic: {info}");
        panic_hook(info);
    }));

    BOT.init();
}

#[no_mangle]
unsafe extern "C" fn zcblive_uninitialize() {
    log::info!("saving config & env before detach...");
    BOT.conf.save();
    BOT.env.save();
    let _ = egui_gl_hook::destroy();
    BOT.release_fmod();
    BOT = Lazy::new(Box::<Bot>::default);
}

#[no_mangle]
unsafe extern "C" fn zcblive_on_action(button: u8, player2: bool, push: bool) {
    BOT.on_action(Button::from_u8(button), player2, push);
}

#[no_mangle]
unsafe extern "C" fn zcblive_on_reset() {
    BOT.on_reset();
}

#[no_mangle]
unsafe extern "C" fn zcblive_set_is_in_level(is_in_level: bool) {
    BOT.is_in_level = is_in_level;
}

#[no_mangle]
unsafe extern "C" fn zcblive_set_playlayer_time(playlayer_time: f64) {
    BOT.playlayer_time = playlayer_time;
}

#[no_mangle]
unsafe extern "C" fn zcblive_on_init(playlayer: usize) {
    BOT.on_init(playlayer);
}

#[no_mangle]
unsafe extern "C" fn zcblive_on_quit() {
    BOT.on_exit();
}

#[no_mangle]
unsafe extern "C" fn zcblive_on_death() {
    BOT.on_death();
}

#[no_mangle]
unsafe extern "C" fn zcblive_do_force_player2_sounds() -> bool {
    BOT.conf.force_player2_sounds
}

#[no_mangle]
unsafe extern "C" fn zcblive_do_use_alternate_hook() -> bool {
    BOT.conf.use_alternate_hook
}

#[no_mangle]
unsafe extern "C" fn zcblive_on_update(dt: f32) {
    BOT.on_update(dt);
}