# Copy a directory, or do nothing if it is not there.
#
# `cmake -E copy_directory` fails when the source is missing, and
# `copy_directory_if_different` needs CMake 3.26 - newer than the 3.22 that
# ships on the oldest Linux this project builds on.
#
# Used for artefacts that are generated rather than committed: the icon PNGs
# come from the master SVGs via assets/icon/build-icons.sh, and a fresh clone
# does not have them. A missing picture is a missing picture; it is not a
# reason for the build to stop.
if(EXISTS "${SRC}")
    file(MAKE_DIRECTORY "${DST}")
    file(GLOB _files "${SRC}/*")
    if(_files)
        file(COPY ${_files} DESTINATION "${DST}")
    endif()
endif()
