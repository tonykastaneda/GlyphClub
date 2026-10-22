# GlyphClub's Photoshop plug-in for Windows (x64 and ARM64): GlyphClub.8li.
# macOS builds with the Makefile. Shares the bridge to GlyphClub and the
# platform layer with the Illustrator plug-in (../plugin/Source).
#
#   cmake -S plugin-photoshop -B plugin-photoshop/build-win-x64 -A x64 -DPS_SDK="C:/path/to/AdobePhotoshopSDK/pluginsdk/photoshopapi"
#   cmake --build plugin-photoshop/build-win-x64 --config Release
#   -> plugin-photoshop/build-win-x64/Release/GlyphClub.8li
#
# Requires Visual Studio 2022 (C++ workload). The SDK isn't in this
# repository (Adobe's license); CI decrypts this repo's own copy.

cmake_minimum_required(VERSION 3.21)
file(STRINGS "${CMAKE_CURRENT_SOURCE_DIR}/VERSION" GLYPHCLUB_PS_VERSION LIMIT_COUNT 1)
string(STRIP "${GLYPHCLUB_PS_VERSION}" GLYPHCLUB_PS_VERSION)
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/VERSION")
project(GlyphClubPhotoshop VERSION ${GLYPHCLUB_PS_VERSION} LANGUAGES CXX RC)

if(NOT WIN32)
	message(FATAL_ERROR "CMakeLists.txt is the Windows build. On macOS run `make`.")
endif()

set(PS_SDK "$ENV{PS_SDK}" CACHE PATH "Photoshop SDK's photoshopapi folder (contains photoshop/ and pica_sp/)")
if(NOT EXISTS "${PS_SDK}/photoshop/PIActions.h")
	message(FATAL_ERROR "Set PS_SDK to the Photoshop SDK's pluginsdk/photoshopapi folder. Got '${PS_SDK}'.")
endif()

set(SHARED "${CMAKE_CURRENT_SOURCE_DIR}/../plugin/Source")
string(REPLACE "." "," GLYPHCLUB_PS_VERSION_COMMAS "${PROJECT_VERSION_MAJOR}.${PROJECT_VERSION_MINOR}.${PROJECT_VERSION_PATCH}.0")
configure_file(Resources/Win/GlyphClubPsVersion.h.in "${CMAKE_CURRENT_BINARY_DIR}/GlyphClubPsVersion.h" @ONLY)

add_library(GlyphClubPhotoshop SHARED
	Source/PsGlyphClub.cpp
	${SHARED}/BridgeWin.cpp
	${SHARED}/PlatformWin.cpp
	Resources/Win/GlyphClub.rc
)

set_target_properties(GlyphClubPhotoshop PROPERTIES
	OUTPUT_NAME GlyphClub
	SUFFIX ".8li"
	CXX_STANDARD 17
	CXX_STANDARD_REQUIRED ON
	MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>"	# static CRT: no VC++ redistributable needed
)

target_include_directories(GlyphClubPhotoshop PRIVATE
	Source
	"${SHARED}"
	"${CMAKE_CURRENT_BINARY_DIR}"
	"${CMAKE_CURRENT_SOURCE_DIR}/Resources"
	"${PS_SDK}/photoshop"
	"${PS_SDK}/pica_sp"
)

# As the SDK's own Windows projects define them.
target_compile_definitions(GlyphClubPhotoshop PRIVATE
	WIN32=1 _WINDOWS ISOLATION_AWARE_ENABLED=1 _CRT_SECURE_NO_DEPRECATE NOMINMAX
	_WIN32_WINNT=0x0A00 WINVER=0x0A00
	GLYPHCLUB_PS_VERSION="${GLYPHCLUB_PS_VERSION}" GLYPHCLUB_VERSION="${GLYPHCLUB_PS_VERSION}"
	$<$<CONFIG:Debug>:_DEBUG> $<$<NOT:$<CONFIG:Debug>>:NDEBUG>)
target_compile_options(GlyphClubPhotoshop PRIVATE $<$<COMPILE_LANGUAGE:CXX>:/utf-8 /EHsc /W3 /wd4996 /wd4068>)
target_link_libraries(GlyphClubPhotoshop PRIVATE user32 shell32 ole32 advapi32 ws2_32)
set_source_files_properties(Resources/Win/GlyphClub.rc PROPERTIES OBJECT_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/Resources/PiPLs.json")
