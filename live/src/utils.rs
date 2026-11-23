use std::{
    ops::RangeInclusive,
    path::PathBuf,
    sync::OnceLock,
};

static ZCB_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn set_save_dir(dir: PathBuf) {
    #[cfg(target_os = "android")]
    {
        let _ = std::fs::create_dir_all(&dir);
        if let Err(e) = std::env::set_current_dir(&dir) {
            log::warn!("failed to set current dir to {dir:?}: {e}");
        }
    }
    let _ = ZCB_DIR.set(dir.join(".zcb"));
}

pub fn zcb_dir() -> &'static PathBuf {
    ZCB_DIR.get_or_init(|| PathBuf::from(".zcb"))
}

pub fn zcb_path(rel: &str) -> PathBuf {
    zcb_dir().join(rel)
}

pub fn capitalize_first_letter(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

#[inline]
pub fn f64_range(range: RangeInclusive<f64>) -> f64 {
    fastrand::f64() * (range.end() - range.start()) + range.start()
}
