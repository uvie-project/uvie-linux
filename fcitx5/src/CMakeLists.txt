set(fcitx_uvie_sources uvie-im.cpp)

# add_fcitx5_addon() lives in fcitx5's own source tree; an addon is just a
# shared module named lib<name>.so.
add_library(uvie MODULE ${fcitx_uvie_sources})
set_target_properties(uvie PROPERTIES PREFIX "lib")
target_link_libraries(uvie Fcitx5::Core uvie_ffi dl pthread m)
target_include_directories(uvie PRIVATE ${UVIE_INCLUDE}
    ${CMAKE_CURRENT_SOURCE_DIR})
target_link_directories(uvie PRIVATE ${UVIE_FFI})
install(TARGETS uvie DESTINATION "${CMAKE_INSTALL_LIBDIR}/fcitx5")
fcitx5_translate_desktop_file(uvie.conf.in uvie.conf)
install(FILES "${CMAKE_CURRENT_BINARY_DIR}/uvie.conf"
        DESTINATION "${CMAKE_INSTALL_DATADIR}/fcitx5/inputmethod" COMPONENT config)
fcitx5_translate_desktop_file(uvie-addon.conf.in uvie-addon.conf)
install(FILES "${CMAKE_CURRENT_BINARY_DIR}/uvie-addon.conf"
        RENAME uvie.conf
        DESTINATION "${FCITX_INSTALL_PKGDATADIR}/addon" COMPONENT config)
