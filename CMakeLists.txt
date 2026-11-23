cmake_minimum_required(VERSION 3.25)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_VISIBILITY_PRESET hidden)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON) # generate compile_commands.json for clangd

project(zcblive VERSION 1.0.0)

# Set up the mod binary
add_library(${PROJECT_NAME} SHARED src/main.cpp)

# Build and statically link the Rust library (defines IMPORTED target `zcblive_rust`)
add_subdirectory(${CMAKE_CURRENT_SOURCE_DIR}/live)
target_link_libraries(${PROJECT_NAME} zcblive_rust)

if (ANDROID)
    # System libraries required by egui_gl_hook (GLES2/EGL), Android logging, and dlopen/dlsym
    target_link_libraries(${PROJECT_NAME} GLESv2 EGL log android dl)
endif()

if (NOT DEFINED ENV{GEODE_SDK})
    message(FATAL_ERROR "Unable to find Geode SDK! Please define GEODE_SDK environment variable to point to Geode")
else()
    message(STATUS "Found Geode: $ENV{GEODE_SDK}")
endif()

add_subdirectory($ENV{GEODE_SDK} ${CMAKE_CURRENT_BINARY_DIR}/geode)

# Set up dependencies, resources, link Geode
setup_geode_mod(${PROJECT_NAME})
