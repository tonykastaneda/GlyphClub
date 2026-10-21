#include "IllustratorSDK.h"
#include "GlyphClubPlugin.h"
#include "Bridge.h"
#include "Panel.h"
#include "AppContext.hpp"

#import <Foundation/Foundation.h>

namespace {

GlyphClubPlugin* gPlugin = nullptr;   // main thread; null once shutting down
SPPluginRef gPluginRef = nullptr;
dispatch_source_t gPingTimer = nullptr;

AINotifierHandle gDocumentOpened = nullptr;
AINotifierHandle gDocumentClosed = nullptr;
AINotifierHandle gFontListChanged = nullptr;

NSString* const kAutoKey = @"GlyphClubAutoActivate";

// PostScript names from an XMP packet's font list: <stFnt:fontName>X</...>,
// or the attribute form stFnt:fontName="X" some writers use.
std::vector<std::string> XmpFontNames(const char* xmp)
{
	std::vector<std::string> names;
	if (!xmp) return names;
	std::string text(xmp);
	auto unescape = [](std::string s) {
		const std::pair<const char*, const char*> entities[] = {{"&lt;", "<"}, {"&gt;", ">"}, {"&quot;", "\""}, {"&apos;", "'"}, {"&amp;", "&"}};
		for (const auto& [from, to] : entities)
			for (size_t at = s.find(from); at != std::string::npos; at = s.find(from, at + strlen(to)))
				s.replace(at, strlen(from), to);
		return s;
	};
	std::set<std::string> seen;
	auto add = [&](std::string name) {
		name = unescape(name);
		if (!name.empty() && seen.insert(name).second) names.push_back(name);
	};
	for (size_t at = text.find("<stFnt:fontName>"); at != std::string::npos; at = text.find("<stFnt:fontName>", at)) {
		at += strlen("<stFnt:fontName>");
		size_t end = text.find("</stFnt:fontName>", at);
		if (end == std::string::npos) break;
		add(text.substr(at, end - at));
	}
	for (size_t at = text.find("stFnt:fontName=\""); at != std::string::npos; at = text.find("stFnt:fontName=\"", at)) {
		at += strlen("stFnt:fontName=\"");
		size_t end = text.find('"', at);
		if (end == std::string::npos) break;
		add(text.substr(at, end - at));
	}
	return names;
}

// Installed for real - not missing, and not only a fauxed stand-in.
bool FontInstalled(const std::string& postscriptName)
{
	AIFontKey key = nullptr;
	if (sAIFont->FindFont(postscriptName.c_str(), kAIAnyFontTechnology, kNativeAIScript, false, &key) || !key) return false;
	AIFontStyle style = {};
	if (sAIFont->GetFontInfo(key, &style)) return true;
	return !style.fauxedInstalled;
}

std::string Join(const std::vector<std::string>& names, size_t most = 3)
{
	std::string out;
	for (size_t i = 0; i < names.size() && i < most; i++) {
		if (i) out += ", ";
		out += names[i];
	}
	if (names.size() > most) out += " and " + std::to_string(names.size() - most) + " more";
	return out;
}

std::string Fonts(size_t n) { return n == 1 ? "1 font" : std::to_string(n) + " fonts"; }

} // namespace

Plugin* AllocatePlugin(SPPluginRef pluginRef)
{
	return new GlyphClubPlugin(pluginRef);
}

void FixupReload(Plugin* plugin)
{
	GlyphClubPlugin::FixupVTable((GlyphClubPlugin*) plugin);
}

GlyphClubPlugin::GlyphClubPlugin(SPPluginRef pluginRef)
	: Plugin(pluginRef)
{
	strncpy(fPluginName, kGlyphClubPluginName, kMaxStringLength);
}

ASErr GlyphClubPlugin::StartupPlugin(SPInterfaceMessage* message)
{
	ASErr error = Plugin::StartupPlugin(message);
	if (error) return error;

	NSNumber* saved = [[NSUserDefaults standardUserDefaults] objectForKey:kAutoKey];
	fAuto = saved ? saved.boolValue : true;

	AIPlatformAddMenuItemDataUS menuData;
	menuData.groupName = kGlyphClubMenuGroup;
	menuData.itemText = ai::UnicodeString::FromUTF8(kGlyphClubMenuItemText);
	if (sAIMenu->AddMenuItem(fPluginRef, kGlyphClubMenuItemName, &menuData, 0, &fPanelItem)) fPanelItem = nullptr;

	sAINotifier->AddNotifier(fPluginRef, kGlyphClubPluginName, kAIDocumentOpenedNotifier, &gDocumentOpened);
	sAINotifier->AddNotifier(fPluginRef, kGlyphClubPluginName, kAIDocumentClosedNotifier, &gDocumentClosed);
	sAINotifier->AddNotifier(fPluginRef, kGlyphClubPluginName, kAIFontListChangedNotifier, &gFontListChanged);

	AddPanel();
	return kNoErr;
}

void GlyphClubPlugin::AddPanel()
{
	if (!sAIPanel) return;
	AISize minSize = {220, 240};
	if (sAIPanel->Create(fPluginRef, ai::UnicodeString::FromUTF8("Glyph Club"), ai::UnicodeString::FromUTF8("Glyph Club"), 1, minSize, true, nullptr, this, fPanel)) {
		fPanel = nullptr;
		return;
	}
	PanelCallbacks callbacks;
	callbacks.autoChanged = [this](bool on) {
		fAuto = on;
		[[NSUserDefaults standardUserDefaults] setBool:on forKey:kAutoKey];
		PanelLogLine(on ? "Auto-activation on" : "Auto-activation off", PanelLog::Info);
		if (on) {
			AppContext context(gPluginRef);
			Check(true);
		}
	};
	callbacks.openGlyphClub = [] {
		glyphclub::bridge::OpenIntegrations([](const glyphclub::bridge::Result& result) {
			if (!result.ok) PanelLogLine(result.error, PanelLog::Problem);
		});
	};
	callbacks.activateNow = [this] {
		AppContext context(gPluginRef);
		RememberCurrentDocument();
		Check(false);
	};
	PanelAttach(fPanel, callbacks);
	PanelSetAutoActivate(fAuto);
}

ASErr GlyphClubPlugin::PostStartupPlugin()
{
	ASErr error = Plugin::PostStartupPlugin();
	gPlugin = this;
	gPluginRef = fPluginRef;
	RefreshConnection();
	// Keeps the panel's status honest when GlyphClub starts or quits.
	gPingTimer = dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER, 0, 0, dispatch_get_main_queue());
	dispatch_source_set_timer(gPingTimer, dispatch_time(DISPATCH_TIME_NOW, 10 * NSEC_PER_SEC), 10 * NSEC_PER_SEC, 2 * NSEC_PER_SEC);
	dispatch_source_set_event_handler(gPingTimer, ^{ if (gPlugin) gPlugin->RefreshConnection(); });
	dispatch_resume(gPingTimer);
	return error;
}

ASErr GlyphClubPlugin::ShutdownPlugin(SPInterfaceMessage* message)
{
	gPlugin = nullptr;
	if (gPingTimer) { dispatch_source_cancel(gPingTimer); gPingTimer = nullptr; }
	PanelDetach();
	if (fPanel && sAIPanel) { sAIPanel->Destroy(fPanel); fPanel = nullptr; }
	return Plugin::ShutdownPlugin(message);
}

ASErr GlyphClubPlugin::GoMenuItem(AIMenuMessage* message)
{
	if (message->menuItem != fPanelItem) return kNoErr;
	if (fPanel) {
		sAIPanel->Show(fPanel, true);
		RefreshConnection();
		RememberCurrentDocument();
		UpdateMissingCount();
		return kNoErr;
	}
	if (sAIUser) {
		auto missing = MissingFontNames();
		sAIUser->MessageAlert(ai::UnicodeString::FromUTF8(missing.empty()
			? std::string("No missing fonts in open documents.")
			: "Missing: " + Join(missing, 10)));
	}
	return kNoErr;
}

ASErr GlyphClubPlugin::Notify(AINotifierMessage* message)
{
	if (message->notifier == gDocumentOpened) {
		RememberCurrentDocument();
		Check(true);
	}
	else if (message->notifier == gFontListChanged) Check(true);
	else if (message->notifier == gDocumentClosed) {
		ForgetClosedDocuments();
		UpdateMissingCount();
	}
	return kNoErr;
}

// Plug-in context required. The current document's fonts, read now: the
// XMP is only reachable for the current document, and a document that's
// just opened is current.
void GlyphClubPlugin::RememberCurrentDocument()
{
	AIDocumentHandle document = nullptr;
	if (sAIDocument->GetDocument(&document) || !document) return;
	const char* xmp = nullptr;
	if (sAIDocument->GetDocumentXAP(&xmp) || !xmp) return;
	fDocumentFonts[document] = XmpFontNames(xmp);
}

void GlyphClubPlugin::ForgetClosedDocuments()
{
	std::set<AIDocumentHandle> open;
	ai::int32 count = 0;
	if (!sAIDocumentList->Count(&count))
		for (ai::int32 i = 0; i < count; i++) {
			AIDocumentHandle document = nullptr;
			if (!sAIDocumentList->GetNthDocument(&document, i) && document) open.insert(document);
		}
	for (auto it = fDocumentFonts.begin(); it != fDocumentFonts.end();)
		it = open.count(it->first) ? std::next(it) : fDocumentFonts.erase(it);
}

// Plug-in context required. Fonts open documents use that Illustrator
// doesn't have, in document order.
std::vector<std::string> GlyphClubPlugin::MissingFontNames() const
{
	std::vector<std::string> missing;
	std::set<std::string> seen;
	for (const auto& [document, fonts] : fDocumentFonts)
		for (const auto& name : fonts)
			if (seen.insert(name).second && !FontInstalled(name)) missing.push_back(name);
	return missing;
}

// Plug-in context required (a notifier, a menu item, or an AppContext).
void GlyphClubPlugin::UpdateMissingCount()
{
	PanelSetMissingCount((int) MissingFontNames().size());
}

// Plug-in context required. Automatic checks ask for each font once per
// session, so a font GlyphClub doesn't have isn't asked about on every font
// list change; the panel's button (automatic = false) always asks.
void GlyphClubPlugin::Check(bool automatic)
{
	auto missing = MissingFontNames();
	PanelSetMissingCount((int) missing.size());
	if (automatic && !fAuto) return;

	std::vector<std::string> ask;
	for (const auto& name : missing) {
		if (automatic && fAskedAutomatically.count(name)) continue;
		ask.push_back(name);
	}
	if (ask.empty()) return;
	if (fBusy) {
		fCheckAgain = true;
		return;
	}
	for (const auto& name : ask) fAskedAutomatically.insert(name);
	Request(std::move(ask), automatic);
}

void GlyphClubPlugin::Request(std::vector<std::string> names, bool automatic)
{
	fBusy = true;
	PanelSetBusy(true);
	PanelLogLine("Asking Glyph Club for " + Join(names), PanelLog::Info);
	glyphclub::bridge::Activate(names, true, [names](const glyphclub::bridge::Result& result) {
		GlyphClubPlugin* self = gPlugin;
		if (!self) return;
		self->NoteLatestPluginVersion(result.latestPluginVersion);
		self->fBusy = false;
		PanelSetBusy(false);
		if (!result.ok) {
			// Never reached GlyphClub, so these weren't really asked: let
			// RefreshConnection ask again as soon as GlyphClub is up.
			for (const auto& name : names) self->fAskedAutomatically.erase(name);
			self->fConnected = false;
			PanelLogLine(result.error + " - fonts activate as soon as it's open", PanelLog::Problem);
			PanelSetConnected(false, result.error);
		}
		else {
			self->fConnected = true;
			PanelSetConnected(true, "Connected to Glyph Club");
			if (!result.activated.empty())
				PanelLogLine("Activated " + (result.activated.size() <= 3 ? Join(result.activated) : Fonts(result.activated.size())), PanelLog::Activated);
			if (!result.alreadyActive.empty())
				PanelLogLine("Already active: " + Join(result.alreadyActive) + " (Illustrator may still be loading it)", PanelLog::Info);
			if (!result.notFound.empty())
				PanelLogLine("Not in your Glyph Club libraries: " + Join(result.notFound), PanelLog::Problem);
			if (!result.failed.empty())
				PanelLogLine("Couldn't activate " + Join(result.failed), PanelLog::Problem);
		}
		if (self->fCheckAgain) {
			self->fCheckAgain = false;
			AppContext context(gPluginRef);
			self->Check(true);
		}
	});
}

// Also what catches up when GlyphClub wasn't running: the moment a ping
// finds it (it's checked every 10 s), missing fonts are asked for again.
void GlyphClubPlugin::RefreshConnection()
{
	glyphclub::bridge::Ping([](const glyphclub::bridge::Result& result) {
		GlyphClubPlugin* self = gPlugin;
		if (!self) return;
		self->NoteLatestPluginVersion(result.latestPluginVersion);
		bool cameUp = result.ok && !self->fConnected;
		self->fConnected = result.ok;
		if (result.ok)
			PanelSetConnected(true, "Connected to Glyph Club" + (result.appVersion.empty() ? std::string() : " " + result.appVersion));
		else
			PanelSetConnected(false, result.error + (self->fAuto ? " - it opens when a font is missing" : ""));
		if (cameUp && !self->fBusy) {
			AppContext context(gPluginRef);
			self->Check(true);
		}
	});
}

// GlyphClub says which plug-in it ships; when that's newer than this one,
// the panel says so (once per version) and offers to open GlyphClub, since
// installing a plug-in into Illustrator is GlyphClub's job.
void GlyphClubPlugin::NoteLatestPluginVersion(const std::string& latest)
{
	if (latest.empty() || latest == fUpdateShown) return;
	if (!glyphclub::bridge::IsNewerVersion(latest, GLYPHCLUB_VERSION)) return;
	fUpdateShown = latest;
	PanelSetUpdate(latest);
	PanelLogLine("Plug-in " + latest + " is available (this is " GLYPHCLUB_VERSION ")", PanelLog::Problem);
}
