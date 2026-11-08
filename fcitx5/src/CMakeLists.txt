set(fcitx_uvie_sources uvie-im.cpp)

# add_fcitx5_addon() lives in fcitx5's own source tree; an addon is just a
# shared module named lib<name>.so.
add_library(uvie MODULE ${fcitx_uvie_sources})
set_target_properties(uvie PROPERTIES PREFIX "lib")
set_target_properties(uvie PROPERTIES INSTALL_RPATH "${CMAKE_INSTALL_FULL_LIBDIR}")
target_link_libraries(uvie Fcitx5::Core uvie_ffi dl pthread m)
target_include_directories(uvie PRIVATE ${UVIE_INCLUDE}
    ${CMAKE_CURRENT_SOURCE_DIR})
target_link_directories(uvie PRIVATE ${UVIE_FFI})
# Install into Fcitx's addon directory (see the FCITX_INSTALL_USE_FCITX_SYS_PATHS
# comment in the top-level CMakeLists.txt). The config files go into Fcitx's
# package data dir, which is always searched via PkgData.
install(TARGETS uvie DESTINATION "${FCITX_INSTALL_ADDONDIR}")
fcitx5_translate_desktop_file(uvie.conf.in uvie.conf)
install(FILES "${CMAKE_CURRENT_BINARY_DIR}/uvie.conf"
        DESTINATION "${FCITX_INSTALL_PKGDATADIR}/inputmethod" COMPONENT config)
fcitx5_translate_desktop_file(uvie-addon.conf.in uvie-addon.conf)
# The addon name is the config file stem, so it must be installed as
# `uvie.conf` to match `Addon=uvie` in the input method definition.
install(FILES "${CMAKE_CURRENT_BINARY_DIR}/uvie-addon.conf"
        RENAME uvie.conf
        DESTINATION "${FCITX_INSTALL_PKGDATADIR}/addon" COMPONENT config)
