# ibus-uvie — IBus engine wrapping libuvie.
# Build: cmake -S ibus -B build-ibus && cmake --build build-ibus
cmake_minimum_required(VERSION 3.16)
project(ibus-uvie VERSION 0.1.0 LANGUAGES C)

find_package(PkgConfig REQUIRED)
pkg_check_modules(IBUS REQUIRED ibus-1.0)

include(GNUInstallDirs)

# libuvie is built by cargo: ../../target/release/libuvie_ffi.{a,so}
# Point UVIE_FFI at the directory containing it.
set(UVIE_FFI "${CMAKE_CURRENT_SOURCE_DIR}/../target/release"
    CACHE PATH "Directory containing libuvie_ffi")
set(UVIE_INCLUDE "${CMAKE_CURRENT_SOURCE_DIR}/../include"
    CACHE PATH "Directory containing uvie.h")

add_executable(ibus-engine-uvie ibus-uvie.c)
target_include_directories(ibus-engine-uvie PRIVATE
    ${IBUS_INCLUDE_DIRS} ${UVIE_INCLUDE} ${CMAKE_CURRENT_SOURCE_DIR})
target_link_directories(ibus-engine-uvie PRIVATE ${UVIE_FFI})
target_link_libraries(ibus-engine-uvie PRIVATE ${IBUS_LIBRARIES} uvie_ffi
    dl pthread m)
target_compile_options(ibus-engine-uvie PRIVATE ${IBUS_CFLAGS_OTHER})

configure_file(uvie.xml "${CMAKE_CURRENT_BINARY_DIR}/uvie.xml" @ONLY)

install(TARGETS ibus-engine-uvie
        RUNTIME DESTINATION ${CMAKE_INSTALL_FULL_LIBEXECDIR})
install(FILES "${CMAKE_CURRENT_BINARY_DIR}/uvie.xml"
        DESTINATION ${CMAKE_INSTALL_FULL_DATADIR}/ibus/component)
