# GlyphClub's Illustrator plug-in (arm64, macOS): activates a document's
# missing fonts through the running GlyphClub app. Command Line Tools only -
# no Xcode, no Visual Studio. Same recipe as Slippy / RAGE.
#   make            build + sign build/GlyphClub.aip (ad hoc; SIGN_ID="Developer ID ..." to sign for real)
#   make install    copy into Illustrator's Plug-ins folder (quit Illustrator first;
#                   the folder is root-owned, so this may need sudo) - normally
#                   GlyphClub's Settings > Integrations does this instead
#   make clean
#
# The plug-in has its own version (plugin/VERSION), separate from the app's:
# bump it whenever the plug-in changes, and only then. GlyphClub compares it
# with the installed copy's to offer Settings > Integrations > Update, so an
# app release that doesn't touch the plug-in doesn't ask anyone to reinstall.

SDK        ?= $(HOME)/Developer/AdobeIllustratorSDK
AI_APP     ?= /Applications/Adobe Illustrator 2026
SIGN_ID    ?= -
VERSION    ?= $(shell cat VERSION)

NAME       := GlyphClub
BUILD      := build
OBJDIR     := $(BUILD)/obj
BUNDLE     := $(BUILD)/$(NAME).aip
EXE        := $(BUNDLE)/Contents/MacOS/$(NAME)
VERSION_STAMP := $(BUILD)/version-$(VERSION).stamp

API        := $(SDK)/illustratorapi
COMMON     := $(SDK)/samplecode/common

SOURCES := \
	Source/GlyphClubPlugin.cpp \
	Source/GlyphClubSuites.cpp \
	$(COMMON)/source/Main.cpp \
	$(COMMON)/source/Plugin.cpp \
	$(COMMON)/source/Suites.cpp \
	$(COMMON)/source/AppContext.cpp \
	$(COMMON)/source/IllustratorSDK.cpp \
	$(API)/illustrator/IAIUnicodeString.cpp \
	$(API)/illustrator/IAIFilePath.cpp \
	$(API)/illustrator/AIAssert.cpp

MM_SOURCES := \
	Source/Bridge.mm \
	Source/PlatformMac.mm \
	Source/Panel.mm \
	Source/PanelView.mm

OBJECTS := $(addprefix $(OBJDIR)/,$(notdir $(SOURCES:.cpp=.o) $(MM_SOURCES:.mm=.o)))
vpath %.cpp $(sort $(dir $(SOURCES)))
vpath %.mm $(sort $(dir $(MM_SOURCES)))

CXX      := clang++
ARCH     := -arch arm64 -mmacosx-version-min=12.0
INCLUDES := -ISource -I$(COMMON)/includes \
	-I$(API)/illustrator -I$(API)/illustrator/actions -I$(API)/pica_sp -I$(API)/ate
CXXFLAGS := $(ARCH) -std=c++17 -stdlib=libc++ -x objective-c++ -O2 -g \
	-fvisibility=hidden -fvisibility-inlines-hidden \
	-Wno-deprecated-declarations -Wno-unknown-pragmas \
	-DGLYPHCLUB_VERSION='"$(VERSION)"' \
	-include $(COMMON)/includes/IllustratorSDKRelease.pch \
	$(INCLUDES) -MMD -MP
LDFLAGS  := $(ARCH) -bundle -stdlib=libc++ -framework Cocoa -framework CoreFoundation

# A real identity also gets the hardened runtime and a secure timestamp,
# which notarizing GlyphClub.app (this bundle ships inside it) requires.
ifeq ($(SIGN_ID),-)
SIGN_FLAGS := --timestamp=none
else
SIGN_FLAGS := --options runtime --timestamp
endif

.PHONY: all clean install
all: $(BUILD)/.signed

$(OBJDIR)/%.o: %.cpp $(VERSION_STAMP) | $(OBJDIR)
	$(CXX) $(CXXFLAGS) -c $< -o $@

$(OBJDIR)/%.o: %.mm $(VERSION_STAMP) | $(OBJDIR)
	$(CXX) $(CXXFLAGS) -fobjc-arc -c $< -o $@

$(OBJDIR):
	mkdir -p $@

$(EXE): $(OBJECTS)
	mkdir -p $(dir $@)
	$(CXX) $(LDFLAGS) $^ -o $@

$(BUILD)/plugin.pipl:
	mkdir -p $(BUILD)
	cd $(BUILD) && python3 $(SDK)/tools/pipl/create_pipl.py -input '[{"name":"$(NAME)"}]'

# Illustrator opens <Executable>.rsrc when loading a plug-in and fails
# ("Plugin issues detected") if it is missing, even when it is empty.
$(BUILD)/$(NAME).rsrc: Resources/$(NAME).r
	mkdir -p $(BUILD)
	Rez -useDF -o $@ $<

$(BUILD)/.signed: $(EXE) Resources/Info.plist $(BUILD)/plugin.pipl $(BUILD)/$(NAME).rsrc $(VERSION_STAMP)
	mkdir -p $(BUNDLE)/Contents/Resources/pipl
	cp $(BUILD)/$(NAME).rsrc $(BUNDLE)/Contents/Resources/$(NAME).rsrc
	sed 's/__VERSION__/$(VERSION)/g' Resources/Info.plist > $(BUNDLE)/Contents/Info.plist
	printf 'ARPIART5' > $(BUNDLE)/Contents/PkgInfo
	cp $(BUILD)/plugin.pipl $(BUNDLE)/Contents/Resources/pipl/plugin.pipl
	codesign --force --sign "$(SIGN_ID)" $(SIGN_FLAGS) $(BUNDLE)
	codesign --verify --strict $(BUNDLE)
	touch $@

# VERSION is baked into the objects and Info.plist: a new one rebuilds.
$(VERSION_STAMP):
	mkdir -p $(BUILD)
	rm -f $(BUILD)/version-*.stamp
	touch $@

install: all
	ditto $(BUNDLE) "$(AI_APP)/Plug-ins.localized/$(NAME).aip"

clean:
	rm -rf $(BUILD)

-include $(OBJECTS:.o=.d)
