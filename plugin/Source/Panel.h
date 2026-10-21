#ifndef __GLYPHCLUB_PANEL_H__
#define __GLYPHCLUB_PANEL_H__

// Hosts GCPanelView (PanelView.mm) in Illustrator's docked panel
// (Window > Glyph Club). Main thread only. Every call is a no-op until
// PanelAttach, so the plug-in works the same without a panel.

#include "AIPanel.h"

#include <functional>
#include <string>

struct PanelCallbacks {
	std::function<void(bool on)> autoChanged;
	std::function<void()> activateNow;
	std::function<void()> openGlyphClub;   // the update notice's button
};

enum class PanelLog { Info, Activated, Problem };

void PanelAttach(AIPanelRef panel, PanelCallbacks callbacks);
void PanelDetach();
void PanelSetConnected(bool connected, const std::string& text);
void PanelSetAutoActivate(bool on);
void PanelSetMissingCount(int count);
void PanelSetBusy(bool busy);
void PanelLogLine(const std::string& line, PanelLog kind);
// Shows "Plug-in update <version> available" with an Open Glyph Club button;
// an empty version hides it.
void PanelSetUpdate(const std::string& version);

#endif // __GLYPHCLUB_PANEL_H__
