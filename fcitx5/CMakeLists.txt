# fcitx5-uvie — Fcitx5 addon wrapping libuvie.
# Build: cmake -S fcitx5 -B build-fcitx5 && cmake --build build-fcitx5
cmake_minimum_required(VERSION 3.16)
project(fcitx5-uvie VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(Fcitx5Core REQUIRED)
find_package(Gettext REQUIRED)

include("${FCITX_INSTALL_CMAKECONFIG_DIR}/Fcitx5Utils/Fcitx5CompilerSettings.cmake")

# libuvie is built by cargo: ../target/release/libuvie_ffi.{a,so}
set(UVIE_FFI "${CMAKE_CURRENT_SOURCE_DIR}/../target/release"
    CACHE PATH "Directory containing libuvie_ffi")
set(UVIE_INCLUDE "${CMAKE_CURRENT_SOURCE_DIR}/../include"
    CACHE PATH "Directory containing uvie.h")

add_subdirectory(src)
