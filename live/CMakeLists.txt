cmake_minimum_required(VERSION 3.25)
project(zcblive_rust)

# Location of the Rust library
set(RUSTLIB_DIR ${CMAKE_CURRENT_SOURCE_DIR})

# Name of the Rust library (not ${PROJECT_NAME})
set(RUSTLIB_NAME zcblive)

file(GLOB_RECURSE RUST_SOURCES
    "${RUSTLIB_DIR}/Cargo.toml"
    "${RUSTLIB_DIR}/Cargo.lock"
    "${RUSTLIB_DIR}/build.rs"
    "${RUSTLIB_DIR}/src/*.rs"
    "${RUSTLIB_DIR}/egui_gl_hook/Cargo.toml"
    "${RUSTLIB_DIR}/egui_gl_hook/src/*.rs"
    "${RUSTLIB_DIR}/gfmod/Cargo.toml"
    "${RUSTLIB_DIR}/gfmod/build.rs"
    "${RUSTLIB_DIR}/gfmod/headers/*.h"
    "${RUSTLIB_DIR}/gfmod/src/*.rs"
)

if (NOT ANDROID)
    message(FATAL_ERROR "This port targets Android64 (arm64-v8a) only.")
endif()

if (NOT (CMAKE_ANDROID_ARCH_ABI STREQUAL "arm64-v8a" OR ANDROID_ABI STREQUAL "arm64-v8a" OR GEODE_TARGET_PLATFORM STREQUAL "Android64"))
    message(FATAL_ERROR "Unsupported Android ABI for Rust build (expected arm64-v8a / Android64): CMAKE_ANDROID_ARCH_ABI='${CMAKE_ANDROID_ARCH_ABI}' ANDROID_ABI='${ANDROID_ABI}'")
endif()

set(RUST_TARGET "aarch64-linux-android")
set(RUST_ENV_TARGET "aarch64_linux_android")
set(RUST_LINKER_VAR "CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER")
set(RUSTLIB_PATH "${RUSTLIB_DIR}/target/${RUST_TARGET}/release/lib${RUSTLIB_NAME}.a")

# Resolve NDK root for bindgen/sysroot/libclang
set(_NDK_ROOT "${CMAKE_ANDROID_NDK}")
if (NOT _NDK_ROOT AND ANDROID_NDK)
    set(_NDK_ROOT "${ANDROID_NDK}")
elseif (NOT _NDK_ROOT AND DEFINED ENV{ANDROID_NDK_HOME})
    set(_NDK_ROOT "$ENV{ANDROID_NDK_HOME}")
elseif (NOT _NDK_ROOT AND DEFINED ENV{ANDROID_NDK_ROOT})
    set(_NDK_ROOT "$ENV{ANDROID_NDK_ROOT}")
endif()

# In NDK r23+, CMAKE_C_COMPILER is generic `clang` with target passed via CMAKE_C_COMPILER_TARGET.
# Generate a thin wrapper script so Cargo and `cc` crate receive the exact NDK target & sysroot.
set(_CLANG_TARGET "${CMAKE_C_COMPILER_TARGET}")
if (NOT _CLANG_TARGET)
    if (CMAKE_SYSTEM_VERSION)
        set(_CLANG_TARGET "${RUST_TARGET}${CMAKE_SYSTEM_VERSION}")
    else()
        set(_CLANG_TARGET "${RUST_TARGET}23")
    endif()
endif()

get_filename_component(_CLANG_BIN_DIR "${CMAKE_C_COMPILER}" DIRECTORY)
get_filename_component(_CLANG_TOOLCHAIN_DIR "${_CLANG_BIN_DIR}" DIRECTORY)

set(_RESOLVED_SYSROOT "")
if (CMAKE_SYSROOT)
    set(_RESOLVED_SYSROOT "${CMAKE_SYSROOT}")
elseif (EXISTS "${_CLANG_TOOLCHAIN_DIR}/sysroot")
    set(_RESOLVED_SYSROOT "${_CLANG_TOOLCHAIN_DIR}/sysroot")
elseif (_NDK_ROOT AND EXISTS "${_NDK_ROOT}/toolchains/llvm/prebuilt/linux-x86_64/sysroot")
    set(_RESOLVED_SYSROOT "${_NDK_ROOT}/toolchains/llvm/prebuilt/linux-x86_64/sysroot")
endif()

set(_SYSROOT_ARG "")
if (_RESOLVED_SYSROOT)
    set(_SYSROOT_ARG "--sysroot=\"${_RESOLVED_SYSROOT}\"")
endif()

set(_RUST_CC_WRAPPER "${CMAKE_CURRENT_BINARY_DIR}/rust-android-cc.sh")
set(_RUST_CXX_WRAPPER "${CMAKE_CURRENT_BINARY_DIR}/rust-android-cxx.sh")
file(WRITE "${_RUST_CC_WRAPPER}"
    "#!/usr/bin/env sh\nexec \"${CMAKE_C_COMPILER}\" --target=${_CLANG_TARGET} ${_SYSROOT_ARG} \"$@\"\n"
)
file(WRITE "${_RUST_CXX_WRAPPER}"
    "#!/usr/bin/env sh\nexec \"${CMAKE_CXX_COMPILER}\" --target=${_CLANG_TARGET} ${_SYSROOT_ARG} \"$@\"\n"
)
file(CHMOD "${_RUST_CC_WRAPPER}" "${_RUST_CXX_WRAPPER}"
    PERMISSIONS OWNER_READ OWNER_WRITE OWNER_EXECUTE GROUP_READ GROUP_EXECUTE WORLD_READ WORLD_EXECUTE
)

set(BINDGEN_CLANG_ARGS "--target=${_CLANG_TARGET}")
if (_RESOLVED_SYSROOT)
    set(BINDGEN_CLANG_ARGS "${BINDGEN_CLANG_ARGS} --sysroot=${_RESOLVED_SYSROOT}")
endif()

set(CARGO_ENV_VARS
    "${RUST_LINKER_VAR}=${_RUST_CC_WRAPPER}"
    "CC_${RUST_ENV_TARGET}=${_RUST_CC_WRAPPER}"
    "CXX_${RUST_ENV_TARGET}=${_RUST_CXX_WRAPPER}"
    "AR_${RUST_ENV_TARGET}=${CMAKE_AR}"
    "RANLIB_${RUST_ENV_TARGET}=${CMAKE_RANLIB}"
    "BINDGEN_EXTRA_CLANG_ARGS_${RUST_ENV_TARGET}=${BINDGEN_CLANG_ARGS}"
)
if (_NDK_ROOT)
    list(APPEND CARGO_ENV_VARS "ANDROID_NDK_HOME=${_NDK_ROOT}")
endif()

if (DEFINED ENV{LIBCLANG_PATH} AND NOT "$ENV{LIBCLANG_PATH}" STREQUAL "")
    list(APPEND CARGO_ENV_VARS "LIBCLANG_PATH=$ENV{LIBCLANG_PATH}")
elseif (EXISTS "${_CLANG_TOOLCHAIN_DIR}/lib/libclang.so")
    list(APPEND CARGO_ENV_VARS "LIBCLANG_PATH=${_CLANG_TOOLCHAIN_DIR}/lib")
elseif (_NDK_ROOT AND EXISTS "${_NDK_ROOT}/toolchains/llvm/prebuilt/linux-x86_64/lib/libclang.so")
    list(APPEND CARGO_ENV_VARS "LIBCLANG_PATH=${_NDK_ROOT}/toolchains/llvm/prebuilt/linux-x86_64/lib")
endif()

set(CARGO_BUILD_CMD
    ${CMAKE_COMMAND} -E env ${CARGO_ENV_VARS}
    cargo build --release --features geode --target ${RUST_TARGET}
)

include(CMakePrintHelpers)
cmake_print_variables(RUSTLIB_PATH)

# Add the Rust static library globally so root CMakeLists.txt can link it
add_library(${PROJECT_NAME} STATIC IMPORTED GLOBAL)
set_target_properties(${PROJECT_NAME} PROPERTIES
    IMPORTED_LOCATION ${RUSTLIB_PATH}
)

# Add a custom command to build the Rust library using cargo
add_custom_command(
    OUTPUT ${RUSTLIB_PATH}
    COMMAND ${CARGO_BUILD_CMD}
    DEPENDS ${RUST_SOURCES}
    WORKING_DIRECTORY ${RUSTLIB_DIR}
    COMMENT "Building Rust library (${RUSTLIB_PATH})"
    USES_TERMINAL
)

# Add a custom target to trigger the custom command
add_custom_target(${PROJECT_NAME}_build ALL DEPENDS ${RUSTLIB_PATH})

# Make sure the Rust library is built before anything else
add_dependencies(${PROJECT_NAME} ${PROJECT_NAME}_build)
