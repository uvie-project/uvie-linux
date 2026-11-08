# fcitx5-uvie — Fcitx5 addon wrapping libuvie.
# Build: cmake -S fcitx5 -B build-fcitx5 && cmake --build build-fcitx5
cmake_minimum_required(VERSION 3.16)
project(fcitx5-uvie VERSION 0.1.0 LANGUAGES CXX)

include(GNUInstallDirs)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Fcitx only searches its own compile-time addon directory (FCITX_INSTALL_ADDONDIR),
# not /usr/local/lib/fcitx5. When our prefix (/usr/local) differs from Fcitx's
# (/usr), Fcitx5Utils defaults FCITX_INSTALL_USE_FCITX_SYS_PATHS to Off, which
# would install the addon where fcitx5 never looks. Force the system paths so
# FCITX_INSTALL_ADDONDIR / FCITX_INSTALL_PKGDATADIR point at the real Fcitx dirs.
# This MUST be set before find_package(Fcitx5Core): the Utils config reads it
# while computing the install dirs.
set(FCITX_INSTALL_USE_FCITX_SYS_PATHS On)

find_package(Fcitx5Core REQUIRED)
find_package(Gettext REQUIRED)

include("${FCITX_INSTALL_CMAKECONFIG_DIR}/Fcitx5Utils/Fcitx5CompilerSettings.cmake")

# Keep the install RPATH to the libdir that receives libuvie_ffi.so; do not
# bake the build-tree link path (/target/release) into the shipped module.
set(CMAKE_INSTALL_RPATH_USE_LINK_PATH FALSE)

# libuvie is built by cargo: ../target/release/libuvie_ffi.{a,so}
set(UVIE_FFI "${CMAKE_CURRENT_SOURCE_DIR}/../target/release"
    CACHE PATH "Directory containing libuvie_ffi")
set(UVIE_INCLUDE "${CMAKE_CURRENT_SOURCE_DIR}/../include"
    CACHE PATH "Directory containing uvie.h")

add_subdirectory(src)
