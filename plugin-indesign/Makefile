# GlyphClub's InDesign plug-in (arm64 + x86_64, macOS): asks the running
# GlyphClub app for fonts InDesign can't find. Command Line Tools only (no
# Xcode): the SDK's Xcode settings (build/mac/prj/_shared_build_settings)
# translated to make, and its ODFRC for the resources. Shares the bridge to
# GlyphClub and the platform layer with the Illustrator plug-in.
#   make            build + sign build/GlyphClub.InDesignPlugin (ad hoc; SIGN_ID="Developer ID ..." to sign for real)
#   make clean
# Windows builds in CI (plugin-indesign/win). Version: VERSION, the plug-in's own.

ID_SDK     ?= $(HOME)/Developer/AdobeInDesignSDK
SIGN_ID    ?= -
VERSION    ?= $(shell cat VERSION)

NAME       := GlyphClub
BUILD      := build
OBJDIR     := $(BUILD)/obj
REZDIR     := $(BUILD)/rez
BUNDLE     := $(BUILD)/$(NAME).InDesignPlugin
VERSIONDIR := $(BUNDLE)/Versions/A
EXE        := $(VERSIONDIR)/$(NAME)
SHARED     := ../plugin/Source
VERSION_STAMP := $(BUILD)/version-$(VERSION).stamp
ID_LIBS    := $(ID_SDK)/build/mac/release_cocoa64/packagefolder/contents

# The plug-in runtime comes from the SDK's libPublicPlugIn.a on macOS
# (Windows compiles PlugInStatics.cpp instead).
ID_SOURCES := Source/GCID.cpp Source/GCNoStrip.cpp Source/GCMissingFontResponder.cpp \
	$(ID_SDK)/source/sdksamples/common/SDKPlugInEntrypoint.cpp
MM_SOURCES := $(SHARED)/Bridge.mm $(SHARED)/PlatformMac.mm
ID_OBJECTS := $(addprefix $(OBJDIR)/,$(notdir $(ID_SOURCES:.cpp=.o)))
MM_OBJECTS := $(addprefix $(OBJDIR)/,$(notdir $(MM_SOURCES:.mm=.o)))
vpath %.cpp Source $(ID_SDK)/source/sdksamples/common
vpath %.mm $(SHARED)

INTERFACES := $(wildcard $(ID_SDK)/source/public/interfaces/*)
ID_INCLUDES := -ISource -I$(SHARED) \
	-I$(ID_SDK)/source/precomp/common -I$(ID_SDK)/source/precomp/msvc -I$(ID_SDK)/source/precomp/gcc \
	-I$(ID_SDK)/source/public/includes $(addprefix -I,$(INTERFACES)) -I$(ID_SDK)/source/public/libs/widgetbin/includes \
	-I$(ID_SDK)/source/sdksamples/common -I$(ID_SDK)/external \
	-I$(ID_SDK)/external/dva/third_party/boost_libraries -I$(ID_SDK)/external/intel_tbb/mac/include \
	-I$(ID_SDK)/external/dva/third_party/adobe_source_libraries -I$(ID_SDK)/external/afl/includes -I$(ID_SDK)/external/dva/adobe/dvacore/sys/mac-xcode
ID_DEFINES := -DMACINTOSH -DNDEBUG -DNS_BUILD_32_LIKE_64 -DSUPPRESS_USING_BIB_NAMESPACE_IN_HEADER -DID_MODEL_PRECOMPILE \
	-DID_COCOA_ENABLE=1 -DID_DROVER_ENABLE=1 -DDVA_ASL_SUPPORT=1 -DBOOST_NO_CXX98_FUNCTION_BASE=1 -DBOOST_BIND_GLOBAL_PLACEHOLDERS=1 \
	-DBOOST_REGEX_USE_CPP_LOCALE -DBOOST_REGEX_NON_RECURSIVE -DBOOST_HAS_ICU=1 -DBOOST_SPIRIT_THREADSAFE \
	-DGLYPHCLUB_ID_VERSION='"$(VERSION)"' -DGLYPHCLUB_VERSION='"$(VERSION)"'

CXX      := clang++
ARCH     := -arch arm64 -arch x86_64 -mmacosx-version-min=13.0
COMMON   := $(ARCH) -std=gnu++17 -stdlib=libc++ -O2 -g -fvisibility=hidden -fvisibility-inlines-hidden \
	-Wno-deprecated-declarations -Wno-multichar -Wno-unknown-pragmas -Wno-ignored-attributes -Wno-ignored-pragmas
ID_FLAGS := $(COMMON) -include $(ID_SDK)/source/precomp/gcc/PluginPrefix.pch $(ID_DEFINES) $(ID_INCLUDES) -MMD -MP
MM_FLAGS := $(COMMON) -x objective-c++ -fobjc-arc -DGLYPHCLUB_VERSION='"$(VERSION)"' -ISource -I$(SHARED) -MMD -MP
LDFLAGS  := $(ARCH) -dynamiclib -stdlib=libc++ -install_name "@rpath/$(NAME).InDesignPlugin/Versions/A/$(NAME)" \
	-F$(ID_LIBS)/macos -F$(ID_LIBS)/Frameworks -framework InDesignModel $(ID_LIBS)/macos/libs/libPublicPlugIn.a \
	$(addprefix $(ID_LIBS)/macos/,PMRuntime.dylib PublicLib.dylib ObjectModelLib.dylib DataBaseLib.dylib ASLSupportLib.dylib) \
	-framework Cocoa -framework CoreFoundation -Xlinker -no_data_in_code_info

ifeq ($(SIGN_ID),-)
SIGN_FLAGS := --timestamp=none
else
SIGN_FLAGS := --options runtime --timestamp
endif

ODFRC_INCLUDES := -i Source $(foreach d,precomp/common precomp/msvc public/includes public/libs/widgetbin/includes sdksamples/common,-i $(ID_SDK)/source/$(d)) \
	$(foreach d,$(INTERFACES),-i $(d))

.PHONY: all clean
all: $(BUILD)/.signed

$(ID_OBJECTS): $(OBJDIR)/%.o: %.cpp $(VERSION_STAMP) | $(OBJDIR)
	$(CXX) $(ID_FLAGS) -c $< -o $@

$(MM_OBJECTS): $(OBJDIR)/%.o: %.mm $(VERSION_STAMP) | $(OBJDIR)
	$(CXX) $(MM_FLAGS) -c $< -o $@

$(OBJDIR):
	mkdir -p $@

$(EXE): $(ID_OBJECTS) $(MM_OBJECTS)
	mkdir -p $(dir $@)
	$(CXX) $(LDFLAGS) $^ -o $@

$(REZDIR)/.done: Source/GlyphClub.fr Source/GCID.h Source/GCFactoryList.h $(VERSION_STAMP)
	rm -rf $(REZDIR) && mkdir -p $(REZDIR)
	$(ID_SDK)/devtools/bin/odfrc-cmd -d RELEASE -d MACINTOSH -d __GNUC__ -d ID_DROVER_ENABLE=1 -d ID_COCOA_ENABLE=1 \
		-d DVAUI_ENABLE_CSNEXT_SKIN=1 -d __APPLE__ $(ODFRC_INCLUDES) -o $(REZDIR)/$(NAME).rsrc Source/GlyphClub.fr
	touch $@

# Laid out like InDesign's own plug-ins: a framework-style bundle with the
# ODFRC resources (idrc_*) beside Info.plist in Versions/A/Resources.
$(BUILD)/.signed: $(EXE) $(REZDIR)/.done Resources/Info.plist $(VERSION_STAMP)
	mkdir -p $(VERSIONDIR)/Resources
	rm -rf $(VERSIONDIR)/Resources/idrc_*
	cp -R $(REZDIR)/idrc_* $(VERSIONDIR)/Resources/
	cp $(REZDIR)/$(NAME).rsrc $(VERSIONDIR)/Resources/$(NAME).rsrc
	sed 's/__VERSION__/$(VERSION)/g' Resources/Info.plist > $(VERSIONDIR)/Resources/Info.plist
	printf 'InD3InDn' > $(VERSIONDIR)/Resources/PkgInfo
	ln -sfn A $(BUNDLE)/Versions/Current
	ln -sfn Versions/Current/$(NAME) $(BUNDLE)/$(NAME)
	ln -sfn Versions/Current/Resources $(BUNDLE)/Resources
	xattr -cr $(BUNDLE)
	codesign --force --sign "$(SIGN_ID)" $(SIGN_FLAGS) $(BUNDLE)
	codesign --verify --strict $(BUNDLE)
	touch $@

$(VERSION_STAMP):
	mkdir -p $(BUILD)
	rm -f $(BUILD)/version-*.stamp
	touch $@

clean:
	rm -rf $(BUILD)

-include $(ID_OBJECTS:.o=.d) $(MM_OBJECTS:.o=.d)
