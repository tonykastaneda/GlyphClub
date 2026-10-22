# GlyphClub's Photoshop plug-in (arm64, macOS): activates a document's
# missing fonts through the running GlyphClub app. Command Line Tools only,
# like the Illustrator plug-in in ../plugin, whose bridge to GlyphClub and
# platform layer it shares.
#   make            build + sign build/GlyphClub.plugin (ad hoc; SIGN_ID="Developer ID ..." to sign for real)
#   make clean
# Windows builds with CMakeLists.txt. Version: VERSION, the plug-in's own.

PS_SDK     ?= $(HOME)/Developer/AdobePhotoshopSDK/pluginsdk/photoshopapi
SIGN_ID    ?= -
VERSION    ?= $(shell cat VERSION)

NAME       := GlyphClub
BUILD      := build
OBJDIR     := $(BUILD)/obj
BUNDLE     := $(BUILD)/$(NAME).plugin
EXE        := $(BUNDLE)/Contents/MacOS/$(NAME)
SHARED     := ../plugin/Source
VERSION_STAMP := $(BUILD)/version-$(VERSION).stamp

SOURCES    := Source/PsGlyphClub.cpp
MM_SOURCES := $(SHARED)/Bridge.mm $(SHARED)/PlatformMac.mm
OBJECTS    := $(addprefix $(OBJDIR)/,$(notdir $(SOURCES:.cpp=.o) $(MM_SOURCES:.mm=.o)))
vpath %.cpp Source
vpath %.mm $(SHARED)

CXX      := clang++
ARCH     := -arch arm64 -mmacosx-version-min=12.0
CXXFLAGS := $(ARCH) -std=c++17 -stdlib=libc++ -x objective-c++ -O2 -g \
	-fvisibility=hidden -fvisibility-inlines-hidden -Wno-deprecated-declarations -Wno-multichar \
	-DGLYPHCLUB_PS_VERSION='"$(VERSION)"' -DGLYPHCLUB_VERSION='"$(VERSION)"' \
	-ISource -I$(SHARED) -I$(PS_SDK)/photoshop -I$(PS_SDK)/pica_sp -MMD -MP
LDFLAGS  := $(ARCH) -bundle -stdlib=libc++ -framework Cocoa -framework CoreFoundation

ifeq ($(SIGN_ID),-)
SIGN_FLAGS := --timestamp=none
else
SIGN_FLAGS := --options runtime --timestamp
endif

.PHONY: all clean
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

$(BUILD)/.signed: $(EXE) Resources/Info.plist Resources/PiPLs.json $(VERSION_STAMP)
	mkdir -p $(BUNDLE)/Contents/Resources
	sed 's/__VERSION__/$(VERSION)/g' Resources/Info.plist > $(BUNDLE)/Contents/Info.plist
	printf '8LIZ8BIM' > $(BUNDLE)/Contents/PkgInfo
	cp Resources/PiPLs.json $(BUNDLE)/Contents/Resources/PiPLs.json
	codesign --force --sign "$(SIGN_ID)" $(SIGN_FLAGS) $(BUNDLE)
	codesign --verify --strict $(BUNDLE)
	touch $@

$(VERSION_STAMP):
	mkdir -p $(BUILD)
	rm -f $(BUILD)/version-*.stamp
	touch $@

clean:
	rm -rf $(BUILD)

-include $(OBJECTS:.o=.d)
