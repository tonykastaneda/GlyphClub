#ifndef __GLYPHCLUB_PLUGIN_H__
#define __GLYPHCLUB_PLUGIN_H__

#include "GlyphClubSuites.h"
#include "GlyphClubID.h"
#include "Plugin.hpp"

#include <map>
#include <set>
#include <string>
#include <vector>

Plugin* AllocatePlugin(SPPluginRef pluginRef);
void FixupReload(Plugin* plugin);

/**	Auto-activates a document's missing fonts through GlyphClub.

	When a document opens, the plug-in reads the fonts it uses from its XMP
	metadata (xmpTPg:Fonts, by PostScript name), asks Illustrator which of
	those aren't installed, and sends those names to the running GlyphClub
	app (Bridge.h), starting it in the background if it isn't running.
	(Illustrator's own fauxed fonts can't be used for this: a fauxed font
	reports the stand-in's name, e.g. Myriad-Roman, not the missing one.)
	GlyphClub temporarily activates whatever its libraries have; Illustrator
	notices the new fonts and resolves the text itself, including while its
	Missing Fonts dialog is up.

	Window > Glyph Club opens a panel: connection status, the auto-activate
	switch, a button to ask again, and what happened recently.
*/
class GlyphClubPlugin : public Plugin
{
public:
	GlyphClubPlugin(SPPluginRef pluginRef);
	virtual ~GlyphClubPlugin() {}

	FIXUP_VTABLE_EX(GlyphClubPlugin, Plugin);

protected:
	virtual ASErr StartupPlugin(SPInterfaceMessage* message);
	virtual ASErr PostStartupPlugin();
	virtual ASErr ShutdownPlugin(SPInterfaceMessage* message);
	virtual ASErr GoMenuItem(AIMenuMessage* message);
	virtual ASErr Notify(AINotifierMessage* message);

private:
	AIMenuItemHandle fPanelItem = nullptr;
	AIPanelRef fPanel = nullptr;
	bool fAuto = true;
	bool fBusy = false;
	bool fCheckAgain = false;             // something changed while a request was out
	bool fConnected = false;              // last ping: GlyphClub up with its bridge
	std::string fUpdateShown;             // the newer plug-in version the panel is announcing
	std::set<std::string> fAskedAutomatically;   // asked once per session, found or not
	std::map<AIDocumentHandle, std::vector<std::string>> fDocumentFonts;   // from each open document's XMP

	void AddPanel();
	void RememberCurrentDocument();
	void ForgetClosedDocuments();
	std::vector<std::string> MissingFontNames() const;
	void Check(bool automatic);
	void Request(std::vector<std::string> names, bool automatic);
	void UpdateMissingCount();
	void RefreshConnection();
	void NoteLatestPluginVersion(const std::string& latest);
};

#endif // __GLYPHCLUB_PLUGIN_H__
