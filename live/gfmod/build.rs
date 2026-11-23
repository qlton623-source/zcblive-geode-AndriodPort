use std::path::PathBuf;

fn main() {
    let out_path =
        PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR enviroment variable not set"));
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    let mut builder = bindgen::Builder::default()
        .header("headers/fmod.h")
        .header("headers/fmod_codec.h")
        .header("headers/fmod_common.h")
        .header("headers/fmod_dsp.h")
        .header("headers/fmod_dsp_effects.h")
        .header("headers/fmod_errors.h")
        .header("headers/fmod_output.h")
        .prepend_enum_name(false)
        .derive_debug(true);

    if target_os == "android" {
        // On Android, Geometry Dash loads libfmod.so at runtime; resolve symbols dynamically
        // via dlopen/dlsym so linking libzeozeozeo.zcblive.so never fails with undefined symbols.
        builder = builder.ignore_functions();
        if target_arch == "aarch64" {
            builder = builder.clang_arg("--target=aarch64-linux-android");
        } else if target_arch == "arm" {
            builder = builder.clang_arg("--target=armv7-linux-androideabi");
        }
    }

    builder
        .generate()
        .expect("unable to generate bindings")
        .write_to_file(out_path.join("bindings.rs"))
        .expect("unable to write bindings");
}
