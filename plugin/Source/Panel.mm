#include "IllustratorSDK.h"
#include "Panel.h"
#include "GlyphClubSuites.h"

#import "PanelView.h"

namespace {
AIPanelRef gPanel = nullptr;
GCPanelView* gView = nil;
PanelCallbacks gCallbacks;

// Last known state, so a view (re)created later starts out current.
BOOL gConnected = NO;
NSString* gStatus = @"Looking for Glyph Club…";
BOOL gAuto = YES;
NSInteger gMissing = 0;
BOOL gBusy = NO;
NSString* gUpdate = @"";

NSString* NS(const std::string& s) { return [NSString stringWithUTF8String:s.c_str()] ?: @""; }

// Illustrator hands out the panel's host view lazily, so every update
// makes sure our view is in it.
void Install()
{
	if (!gPanel || !sAIPanel) return;
	__autoreleasing AIPanelPlatformWindow host = nil;
	if (sAIPanel->GetPlatformWindow(gPanel, host) || !host) return;
	if (gView && gView.superview == host) return;
	if (!gView) {
		gView = [[GCPanelView alloc] initWithFrame:host.bounds];
		gView.onAutoChanged = ^(BOOL on) { if (gCallbacks.autoChanged) gCallbacks.autoChanged(on); };
		gView.onActivateNow = ^{ if (gCallbacks.activateNow) gCallbacks.activateNow(); };
		gView.onOpenGlyphClub = ^{ if (gCallbacks.openGlyphClub) gCallbacks.openGlyphClub(); };
		[gView setConnected:gConnected text:gStatus];
		[gView setAutoActivate:gAuto];
		[gView setMissingCount:gMissing];
		[gView setBusy:gBusy];
		[gView setUpdateVersion:gUpdate];
	}
	[gView removeFromSuperview];
	gView.frame = host.bounds;
	[host addSubview:gView];
}
} // namespace

void PanelAttach(AIPanelRef panel, PanelCallbacks callbacks)
{
	@autoreleasepool {
		gPanel = panel;
		gCallbacks = std::move(callbacks);
		AISize minSize = {220, 240}, pref = {260, 320}, maxSize = {600, 2000};
		sAIPanel->SetSizes(gPanel, minSize, pref, pref, maxSize);
		Install();
	}
}

void PanelDetach()
{
	@autoreleasepool {
		[gView removeFromSuperview];
		gView = nil;
		gPanel = nullptr;
		gCallbacks = {};
	}
}

void PanelSetConnected(bool connected, const std::string& text)
{
	@autoreleasepool {
		gConnected = connected;
		gStatus = NS(text);
		Install();
		[gView setConnected:gConnected text:gStatus];
	}
}

void PanelSetAutoActivate(bool on)
{
	@autoreleasepool {
		gAuto = on;
		Install();
		[gView setAutoActivate:on];
	}
}

void PanelSetMissingCount(int count)
{
	@autoreleasepool {
		gMissing = count;
		Install();
		[gView setMissingCount:count];
	}
}

void PanelSetBusy(bool busy)
{
	@autoreleasepool {
		gBusy = busy;
		Install();
		[gView setBusy:busy];
	}
}

void PanelLogLine(const std::string& line, PanelLog kind)
{
	@autoreleasepool {
		Install();
		[gView log:NS(line) kind:kind == PanelLog::Activated ? GCLogActivated : kind == PanelLog::Problem ? GCLogProblem : GCLogInfo];
	}
}

void PanelSetUpdate(const std::string& version)
{
	@autoreleasepool {
		gUpdate = NS(version);
		Install();
		[gView setUpdateVersion:gUpdate];
	}
}
