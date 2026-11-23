use crate::*;
use std::ffi::{c_char, c_int, c_uint, c_void, CStr};
use std::sync::OnceLock;

static FMOD_LIB: OnceLock<usize> = OnceLock::new();

fn fmod_handle() -> *mut c_void {
    let addr = *FMOD_LIB.get_or_init(|| unsafe {
        let mut h = libc::dlopen(c"libfmod.so".as_ptr(), libc::RTLD_NOLOAD | libc::RTLD_LAZY);
        if h.is_null() {
            h = libc::dlopen(c"libfmod.so".as_ptr(), libc::RTLD_LAZY);
        }
        if h.is_null() {
            h = libc::dlopen(c"libfmodL.so".as_ptr(), libc::RTLD_NOLOAD | libc::RTLD_LAZY);
        }
        h as usize
    });
    addr as *mut c_void
}

unsafe fn resolve_sym<T: Copy>(name: &CStr) -> Option<T> {
    let handle = fmod_handle();
    let ptr = if handle.is_null() {
        libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr())
    } else {
        let p = libc::dlsym(handle, name.as_ptr());
        if p.is_null() {
            libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr())
        } else {
            p
        }
    };
    if ptr.is_null() {
        None
    } else {
        Some(std::mem::transmute_copy(&ptr))
    }
}

pub unsafe fn FMOD_System_Create(
    system: *mut *mut FMOD_SYSTEM,
    headerversion: c_uint,
) -> FMOD_RESULT {
    static FN: OnceLock<Option<unsafe extern "C" fn(*mut *mut FMOD_SYSTEM, c_uint) -> FMOD_RESULT>> =
        OnceLock::new();
    match *FN.get_or_init(|| resolve_sym(c"FMOD_System_Create")) {
        Some(f) => f(system, headerversion),
        None => FMOD_ERR_INITIALIZATION,
    }
}

pub unsafe fn FMOD_System_SetSoftwareFormat(
    system: *mut FMOD_SYSTEM,
    samplerate: c_int,
    speakermode: FMOD_SPEAKERMODE,
    numrawspeakers: c_int,
) -> FMOD_RESULT {
    static FN: OnceLock<
        Option<
            unsafe extern "C" fn(
                *mut FMOD_SYSTEM,
                c_int,
                FMOD_SPEAKERMODE,
                c_int,
            ) -> FMOD_RESULT,
        >,
    > = OnceLock::new();
    match *FN.get_or_init(|| resolve_sym(c"FMOD_System_SetSoftwareFormat")) {
        Some(f) => f(system, samplerate, speakermode, numrawspeakers),
        None => FMOD_ERR_INITIALIZATION,
    }
}

pub unsafe fn FMOD_System_SetStreamBufferSize(
    system: *mut FMOD_SYSTEM,
    filebuffersize: c_uint,
    filebuffersizetype: FMOD_TIMEUNIT,
) -> FMOD_RESULT {
    static FN: OnceLock<
        Option<unsafe extern "C" fn(*mut FMOD_SYSTEM, c_uint, FMOD_TIMEUNIT) -> FMOD_RESULT>,
    > = OnceLock::new();
    match *FN.get_or_init(|| resolve_sym(c"FMOD_System_SetStreamBufferSize")) {
        Some(f) => f(system, filebuffersize, filebuffersizetype),
        None => FMOD_ERR_INITIALIZATION,
    }
}

pub unsafe fn FMOD_System_GetDSPBufferSize(
    system: *mut FMOD_SYSTEM,
    bufferlength: *mut c_uint,
    numbuffers: *mut c_int,
) -> FMOD_RESULT {
    static FN: OnceLock<
        Option<unsafe extern "C" fn(*mut FMOD_SYSTEM, *mut c_uint, *mut c_int) -> FMOD_RESULT>,
    > = OnceLock::new();
    match *FN.get_or_init(|| resolve_sym(c"FMOD_System_GetDSPBufferSize")) {
        Some(f) => f(system, bufferlength, numbuffers),
        None => FMOD_ERR_INITIALIZATION,
    }
}

pub unsafe fn FMOD_System_SetDSPBufferSize(
    system: *mut FMOD_SYSTEM,
    bufferlength: c_uint,
    numbuffers: c_int,
) -> FMOD_RESULT {
    static FN: OnceLock<
        Option<unsafe extern "C" fn(*mut FMOD_SYSTEM, c_uint, c_int) -> FMOD_RESULT>,
    > = OnceLock::new();
    match *FN.get_or_init(|| resolve_sym(c"FMOD_System_SetDSPBufferSize")) {
        Some(f) => f(system, bufferlength, numbuffers),
        None => FMOD_ERR_INITIALIZATION,
    }
}

pub unsafe fn FMOD_System_Init(
    system: *mut FMOD_SYSTEM,
    maxchannels: c_int,
    flags: FMOD_INITFLAGS,
    extradriverdata: *mut c_void,
) -> FMOD_RESULT {
    static FN: OnceLock<
        Option<
            unsafe extern "C" fn(*mut FMOD_SYSTEM, c_int, FMOD_INITFLAGS, *mut c_void) -> FMOD_RESULT,
        >,
    > = OnceLock::new();
    match *FN.get_or_init(|| resolve_sym(c"FMOD_System_Init")) {
        Some(f) => f(system, maxchannels, flags, extradriverdata),
        None => FMOD_ERR_INITIALIZATION,
    }
}

pub unsafe fn FMOD_System_Release(system: *mut FMOD_SYSTEM) -> FMOD_RESULT {
    if system.is_null() {
        return FMOD_OK;
    }
    static FN: OnceLock<Option<unsafe extern "C" fn(*mut FMOD_SYSTEM) -> FMOD_RESULT>> =
        OnceLock::new();
    match *FN.get_or_init(|| resolve_sym(c"FMOD_System_Release")) {
        Some(f) => f(system),
        None => FMOD_ERR_INITIALIZATION,
    }
}

pub unsafe fn FMOD_System_Update(system: *mut FMOD_SYSTEM) -> FMOD_RESULT {
    if system.is_null() {
        return FMOD_ERR_INVALID_HANDLE;
    }
    static FN: OnceLock<Option<unsafe extern "C" fn(*mut FMOD_SYSTEM) -> FMOD_RESULT>> =
        OnceLock::new();
    match *FN.get_or_init(|| resolve_sym(c"FMOD_System_Update")) {
        Some(f) => f(system),
        None => FMOD_ERR_INITIALIZATION,
    }
}

pub unsafe fn FMOD_System_CreateSound(
    system: *mut FMOD_SYSTEM,
    name_or_data: *const c_char,
    mode: FMOD_MODE,
    exinfo: *mut FMOD_CREATESOUNDEXINFO,
    sound: *mut *mut FMOD_SOUND,
) -> FMOD_RESULT {
    if system.is_null() {
        return FMOD_ERR_INVALID_HANDLE;
    }
    static FN: OnceLock<
        Option<
            unsafe extern "C" fn(
                *mut FMOD_SYSTEM,
                *const c_char,
                FMOD_MODE,
                *mut FMOD_CREATESOUNDEXINFO,
                *mut *mut FMOD_SOUND,
            ) -> FMOD_RESULT,
        >,
    > = OnceLock::new();
    match *FN.get_or_init(|| resolve_sym(c"FMOD_System_CreateSound")) {
        Some(f) => f(system, name_or_data, mode, exinfo, sound),
        None => FMOD_ERR_INITIALIZATION,
    }
}

pub unsafe fn FMOD_System_PlaySound(
    system: *mut FMOD_SYSTEM,
    sound: *mut FMOD_SOUND,
    channelgroup: *mut FMOD_CHANNELGROUP,
    paused: FMOD_BOOL,
    channel: *mut *mut FMOD_CHANNEL,
) -> FMOD_RESULT {
    if system.is_null() || sound.is_null() {
        return FMOD_ERR_INVALID_HANDLE;
    }
    static FN: OnceLock<
        Option<
            unsafe extern "C" fn(
                *mut FMOD_SYSTEM,
                *mut FMOD_SOUND,
                *mut FMOD_CHANNELGROUP,
                FMOD_BOOL,
                *mut *mut FMOD_CHANNEL,
            ) -> FMOD_RESULT,
        >,
    > = OnceLock::new();
    match *FN.get_or_init(|| resolve_sym(c"FMOD_System_PlaySound")) {
        Some(f) => f(system, sound, channelgroup, paused, channel),
        None => FMOD_ERR_INITIALIZATION,
    }
}

pub unsafe fn FMOD_Sound_Release(sound: *mut FMOD_SOUND) -> FMOD_RESULT {
    if sound.is_null() {
        return FMOD_OK;
    }
    static FN: OnceLock<Option<unsafe extern "C" fn(*mut FMOD_SOUND) -> FMOD_RESULT>> =
        OnceLock::new();
    match *FN.get_or_init(|| resolve_sym(c"FMOD_Sound_Release")) {
        Some(f) => f(sound),
        None => FMOD_ERR_INITIALIZATION,
    }
}

pub unsafe fn FMOD_Sound_SetLoopCount(sound: *mut FMOD_SOUND, loopcount: c_int) -> FMOD_RESULT {
    if sound.is_null() {
        return FMOD_ERR_INVALID_HANDLE;
    }
    static FN: OnceLock<Option<unsafe extern "C" fn(*mut FMOD_SOUND, c_int) -> FMOD_RESULT>> =
        OnceLock::new();
    match *FN.get_or_init(|| resolve_sym(c"FMOD_Sound_SetLoopCount")) {
        Some(f) => f(sound, loopcount),
        None => FMOD_ERR_INITIALIZATION,
    }
}

pub unsafe fn FMOD_Channel_Stop(channel: *mut FMOD_CHANNEL) -> FMOD_RESULT {
    if channel.is_null() {
        return FMOD_ERR_INVALID_HANDLE;
    }
    static FN: OnceLock<Option<unsafe extern "C" fn(*mut FMOD_CHANNEL) -> FMOD_RESULT>> =
        OnceLock::new();
    match *FN.get_or_init(|| resolve_sym(c"FMOD_Channel_Stop")) {
        Some(f) => f(channel),
        None => FMOD_ERR_INITIALIZATION,
    }
}

pub unsafe fn FMOD_Channel_SetPitch(channel: *mut FMOD_CHANNEL, pitch: f32) -> FMOD_RESULT {
    if channel.is_null() {
        return FMOD_ERR_INVALID_HANDLE;
    }
    static FN: OnceLock<Option<unsafe extern "C" fn(*mut FMOD_CHANNEL, f32) -> FMOD_RESULT>> =
        OnceLock::new();
    match *FN.get_or_init(|| resolve_sym(c"FMOD_Channel_SetPitch")) {
        Some(f) => f(channel, pitch),
        None => FMOD_ERR_INITIALIZATION,
    }
}

pub unsafe fn FMOD_Channel_SetVolume(channel: *mut FMOD_CHANNEL, volume: f32) -> FMOD_RESULT {
    if channel.is_null() {
        return FMOD_ERR_INVALID_HANDLE;
    }
    static FN: OnceLock<Option<unsafe extern "C" fn(*mut FMOD_CHANNEL, f32) -> FMOD_RESULT>> =
        OnceLock::new();
    match *FN.get_or_init(|| resolve_sym(c"FMOD_Channel_SetVolume")) {
        Some(f) => f(channel, volume),
        None => FMOD_ERR_INITIALIZATION,
    }
}

pub unsafe fn FMOD_Channel_SetLoopCount(
    channel: *mut FMOD_CHANNEL,
    loopcount: c_int,
) -> FMOD_RESULT {
    if channel.is_null() {
        return FMOD_ERR_INVALID_HANDLE;
    }
    static FN: OnceLock<Option<unsafe extern "C" fn(*mut FMOD_CHANNEL, c_int) -> FMOD_RESULT>> =
        OnceLock::new();
    match *FN.get_or_init(|| resolve_sym(c"FMOD_Channel_SetLoopCount")) {
        Some(f) => f(channel, loopcount),
        None => FMOD_ERR_INITIALIZATION,
    }
}

pub unsafe fn FMOD_Channel_IsPlaying(
    channel: *mut FMOD_CHANNEL,
    isplaying: *mut FMOD_BOOL,
) -> FMOD_RESULT {
    if channel.is_null() {
        return FMOD_ERR_INVALID_HANDLE;
    }
    static FN: OnceLock<
        Option<unsafe extern "C" fn(*mut FMOD_CHANNEL, *mut FMOD_BOOL) -> FMOD_RESULT>,
    > = OnceLock::new();
    match *FN.get_or_init(|| resolve_sym(c"FMOD_Channel_IsPlaying")) {
        Some(f) => f(channel, isplaying),
        None => FMOD_ERR_INITIALIZATION,
    }
}

pub unsafe fn FMOD_Channel_SetPaused(channel: *mut FMOD_CHANNEL, paused: FMOD_BOOL) -> FMOD_RESULT {
    if channel.is_null() {
        return FMOD_ERR_INVALID_HANDLE;
    }
    static FN: OnceLock<Option<unsafe extern "C" fn(*mut FMOD_CHANNEL, FMOD_BOOL) -> FMOD_RESULT>> =
        OnceLock::new();
    match *FN.get_or_init(|| resolve_sym(c"FMOD_Channel_SetPaused")) {
        Some(f) => f(channel, paused),
        None => FMOD_ERR_INITIALIZATION,
    }
}

pub unsafe fn FMOD_Channel_SetMode(channel: *mut FMOD_CHANNEL, mode: FMOD_MODE) -> FMOD_RESULT {
    if channel.is_null() {
        return FMOD_ERR_INVALID_HANDLE;
    }
    static FN: OnceLock<Option<unsafe extern "C" fn(*mut FMOD_CHANNEL, FMOD_MODE) -> FMOD_RESULT>> =
        OnceLock::new();
    match *FN.get_or_init(|| resolve_sym(c"FMOD_Channel_SetMode")) {
        Some(f) => f(channel, mode),
        None => FMOD_ERR_INITIALIZATION,
    }
}

pub unsafe fn FMOD_Sound_SetMode(sound: *mut FMOD_SOUND, mode: FMOD_MODE) -> FMOD_RESULT {
    if sound.is_null() {
        return FMOD_ERR_INVALID_HANDLE;
    }
    static FN: OnceLock<Option<unsafe extern "C" fn(*mut FMOD_SOUND, FMOD_MODE) -> FMOD_RESULT>> =
        OnceLock::new();
    match *FN.get_or_init(|| resolve_sym(c"FMOD_Sound_SetMode")) {
        Some(f) => f(sound, mode),
        None => FMOD_ERR_INITIALIZATION,
    }
}

pub unsafe fn FMOD_System_CreateChannelGroup(
    system: *mut FMOD_SYSTEM,
    name: *const c_char,
    channelgroup: *mut *mut FMOD_CHANNELGROUP,
) -> FMOD_RESULT {
    if system.is_null() {
        return FMOD_ERR_INVALID_HANDLE;
    }
    static FN: OnceLock<
        Option<
            unsafe extern "C" fn(
                *mut FMOD_SYSTEM,
                *const c_char,
                *mut *mut FMOD_CHANNELGROUP,
            ) -> FMOD_RESULT,
        >,
    > = OnceLock::new();
    match *FN.get_or_init(|| resolve_sym(c"FMOD_System_CreateChannelGroup")) {
        Some(f) => f(system, name, channelgroup),
        None => FMOD_ERR_INITIALIZATION,
    }
}

pub unsafe fn FMOD_ChannelGroup_Stop(channelgroup: *mut FMOD_CHANNELGROUP) -> FMOD_RESULT {
    if channelgroup.is_null() {
        return FMOD_ERR_INVALID_HANDLE;
    }
    static FN: OnceLock<Option<unsafe extern "C" fn(*mut FMOD_CHANNELGROUP) -> FMOD_RESULT>> =
        OnceLock::new();
    match *FN.get_or_init(|| resolve_sym(c"FMOD_ChannelGroup_Stop")) {
        Some(f) => f(channelgroup),
        None => FMOD_ERR_INITIALIZATION,
    }
}

pub unsafe fn FMOD_ChannelGroup_Release(channelgroup: *mut FMOD_CHANNELGROUP) -> FMOD_RESULT {
    if channelgroup.is_null() {
        return FMOD_OK;
    }
    static FN: OnceLock<Option<unsafe extern "C" fn(*mut FMOD_CHANNELGROUP) -> FMOD_RESULT>> =
        OnceLock::new();
    match *FN.get_or_init(|| resolve_sym(c"FMOD_ChannelGroup_Release")) {
        Some(f) => f(channelgroup),
        None => FMOD_ERR_INITIALIZATION,
    }
}
