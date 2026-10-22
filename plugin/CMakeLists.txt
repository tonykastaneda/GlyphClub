# GlyphClub's Illustrator plug-in for Windows (x64 and ARM64): GlyphClub.aip.
# macOS builds with the Makefile. Same recipe as Slippy's Windows build.
#
#   cmake -S plugin -B plugin/build-win-x64 -A x64 -DAI_SDK="C:/path/to/AdobeIllustratorSDK"
#   cmake --build plugin/build-win-x64 --config Release
#   -> plugin/build-win-x64/Release/GlyphClub.aip
#
# Requires Visual Studio 2022 (C++ workload, Windows 10 SDK) and Python 3 on
# PATH (the SDK's PiPL tool). The SDK isn't in this repository (Adobe's
# license); CI decrypts a copy (see .github/workflows/release.yml).

cmake_minimum_required(VERSION 3.21)
# The plug-in's version lives in plugin/VERSION, separate from the app's.
file(STRINGS "${CMAKE_CURRENT_SOURCE_DIR}/VERSION" GLYPHCLUB_PLUGIN_VERSION LIMIT_COUNT 1)
string(STRIP "${GLYPHCLUB_PLUGIN_VERSION}" GLYPHCLUB_PLUGIN_VERSION)
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/VERSION")
project(GlyphClubPlugin VERSION ${GLYPHCLUB_PLUGIN_VERSION} LANGUAGES CXX RC)

if(NOT WIN32)
	message(FATAL_ERROR "CMakeLists.txt is the Windows build. On macOS run `make`.")
endif()

set(AI_SDK "$ENV{AI_SDK}" CACHE PATH "Adobe Illustrator SDK root (contains illustratorapi/ and samplecode/)")
if(NOT EXISTS "${AI_SDK}/illustratorapi/illustrator/AITypes.h")
	message(FATAL_ERROR "Set AI_SDK to the Illustrator SDK's root. Got '${AI_SDK}'.")
endif()
find_package(Python3 REQUIRED COMPONENTS Interpreter)

set(API "${AI_SDK}/illustratorapi")
set(COMMON "${AI_SDK}/samplecode/common")

# "0.2.0" -> 0,2,0,0 for the version resource.
string(REPLACE "." "," GLYPHCLUB_VERSION_COMMAS "${PROJECT_VERSION_MAJOR}.${PROJECT_VERSION_MINOR}.${PROJECT_VERSION_PATCH}.0")
configure_file(Resources/Win/GlyphClubVersion.h.in "${CMAKE_CURRENT_BINARY_DIR}/GlyphClubVersion.h" @ONLY)

add_library(GlyphClubPlugin SHARED
	Source/GlyphClubPlugin.cpp
	Source/GlyphClubSuites.cpp
	Source/BridgeWin.cpp
	Source/PanelWin.cpp
	Source/PlatformWin.cpp
	${COMMON}/source/Main.cpp
	${COMMON}/source/Plugin.cpp
	${COMMON}/source/Suites.cpp
	${COMMON}/source/AppContext.cpp
	${COMMON}/source/IllustratorSDK.cpp
	${API}/illustrator/IAIUnicodeString.cpp
	${API}/illustrator/IAIFilePath.cpp
	${API}/illustrator/AIAssert.cpp
	Resources/Win/GlyphClub.rc
	${COMMON}/win/PluginMain.def
)

set_target_properties(GlyphClubPlugin PROPERTIES
	OUTPUT_NAME GlyphClub
	SUFFIX ".aip"
	CXX_STANDARD 17
	CXX_STANDARD_REQUIRED ON
	MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>"	# static CRT: no VC++ redistributable needed
)

target_include_directories(GlyphClubPlugin PRIVATE
	Source
	"${CMAKE_CURRENT_BINARY_DIR}"
	${COMMON}/includes
	${COMMON}/win
	${API}/illustrator
	${API}/illustrator/actions
	${API}/illustrator/legacy
	${API}/pica_sp
	${API}/ate
	${API}/adm
)

target_compile_definitions(GlyphClubPlugin PRIVATE
	WIN_ENV _WINDOWS NOMINMAX _USE_MATH_DEFINES _WIN32_WINNT=0x0A00 WINVER=0x0A00
	GLYPHCLUB_VERSION="${GLYPHCLUB_PLUGIN_VERSION}"
	$<$<CONFIG:Debug>:_DEBUG> $<$<NOT:$<CONFIG:Debug>>:NDEBUG>)

# pragma.h silences SDK warnings (as in the SDK's own projects).
target_compile_options(GlyphClubPlugin PRIVATE $<$<COMPILE_LANGUAGE:CXX>:/utf-8 /EHsc /W3 /FI${COMMON}/win/pragma.h /wd4996>)

target_link_libraries(GlyphClubPlugin PRIVATE user32 gdi32 shell32 ole32 advapi32 ws2_32)

# The PiPL: create_pipl.py writes a platform-specific binary, so it runs here
# rather than being checked in. The .rc embeds it as resource "plugin".
set(PIPL_DIR "${CMAKE_CURRENT_BINARY_DIR}/pipl")
set(PIPL "${PIPL_DIR}/plugin.pipl")
file(MAKE_DIRECTORY "${PIPL_DIR}")
add_custom_command(
	OUTPUT "${PIPL}"
	COMMAND "${Python3_EXECUTABLE}" "${AI_SDK}/tools/pipl/create_pipl.py" -input "[{\"name\":\"GlyphClub\"}]"
	WORKING_DIRECTORY "${PIPL_DIR}"
	COMMENT "Generating the Windows PiPL"
	VERBATIM
)
add_custom_target(GlyphClubPipl DEPENDS "${PIPL}")
add_dependencies(GlyphClubPlugin GlyphClubPipl)
target_include_directories(GlyphClubPlugin PRIVATE "${PIPL_DIR}")
set_source_files_properties(Resources/Win/GlyphClub.rc PROPERTIES OBJECT_DEPENDS "${PIPL}")
