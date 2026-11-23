#![allow(static_mut_refs)]

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("state was not initialized")]
    NotInit,
    #[error("state was already initialized")]
    AlreadyInit,

    #[error("couldn't lock state")]
    StateLock,
    #[error("couldn't create state")]
    StateCreation,

    #[error("failed to create gl context")]
    CtxCreate,
    #[error("failed to switch gl context")]
    CtxSwitch,
    #[error("failed to delete gl context")]
    CtxDelete,

    #[error("failed to get window size")]
    WindowSize,

    #[error("could not create painter: `{0}`")]
    PainterError(#[from] egui_glow::PainterError),
}

mod gles_impl;
pub use gles_impl::*;
