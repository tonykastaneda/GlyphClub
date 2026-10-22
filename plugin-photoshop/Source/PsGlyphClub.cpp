// GlyphClub for Photoshop: activates a document's missing fonts through the
// running GlyphClub app, the way the Illustrator plug-in (plugin/) does.
//
// An automation plug-in that stays loaded (Persistent) and starts with
// Photoshop. When a document opens it reads the fonts its text layers use
// (by PostScript name), compares them with the fonts Photoshop has, and asks
// GlyphClub for the missing ones (Bridge.h, shared with the Illustrator
// plug-in), starting GlyphClub in the background if it isn't running.
// GlyphClub activates whatever its libraries have; Photoshop notices the new
// fonts itself. File > Automate > Glyph Club asks again for the current
// document.
//
// Everything here runs on Photoshop's main thread: the open notification,
// the menu command, and the bridge's replies (Platform.h).

#include "SPBasic.h"
#include "SPInterf.h"
#include "PIActions.h"
#include "PIActionsPlugin.h"
#include "PITerminology.h"

#include "Bridge.h"
#include "Platform.h"

#include <set>
#include <string>
#include <vector>

#ifndef GLYPHCLUB_PS_VERSION
#define GLYPHCLUB_PS_VERSION "dev"
#endif

namespace {

SPBasicSuite* sBasic = nullptr;
SPPluginRef sSelf = nullptr;
PSActionControlProcs* sControl = nullptr;
PSActionDescriptorProcs* sDesc = nullptr;
PSActionReferenceProcs* sRef = nullptr;
PSActionListProcs* sList = nullptr;

bool gStarted = false;
bool gBusy = false;
std::set<std::string> gAsked;   // asked automatically once per session, found or not

template <typename T>
bool Acquire(const char* name, int32 version, T*& suite)
{
	const void* p = nullptr;
	if (sBasic->AcquireSuite(name, version, &p) || !p) {
		suite = nullptr;
		return false;
	}
	suite = (T*) p;
	return true;
}

void ReleaseSuites()
{
	if (!sBasic) return;
	if (sControl) sBasic->ReleaseSuite(kPSActionControlSuite, kPSActionControlSuiteVersion);
	if (sDesc) sBasic->ReleaseSuite(kPSActionDescriptorSuite, kPSActionDescriptorSuiteVersion);
	if (sRef) sBasic->ReleaseSuite(kPSActionReferenceSuite, kPSActionReferenceSuiteVersion);
	if (sList) sBasic->ReleaseSuite(kPSActionListSuite, kPSActionListSuiteVersion);
	sControl = nullptr;
	sDesc = nullptr;
	sRef = nullptr;
	sList = nullptr;
}

DescriptorTypeID ID(const char* stringID)
{
	DescriptorTypeID id = 0;
	sControl->StringIDToTypeID(stringID, &id);
	return id;
}

// A property of the application, the active document, or the active
// document's layer at `layerIndex` (> 0). Null when it doesn't have one
// (e.g. textKey on a layer that isn't text). The caller frees it.
PIActionDescriptor GetProperty(const char* property, DescriptorClassID of, int32 layerIndex = 0)
{
	PIActionReference ref = nullptr;
	if (sRef->Make(&ref) || !ref) return nullptr;
	sRef->PutProperty(ref, classProperty, ID(property));
	if (layerIndex > 0) sRef->PutIndex(ref, classLayer, layerIndex);
	else sRef->PutEnumerated(ref, of, typeOrdinal, enumTarget);
	PIActionDescriptor result = nullptr;
	OSErr error = sControl->Get(&result, ref);
	sRef->Free(ref);
	if (error && result) {
		sDesc->Free(result);
		result = nullptr;
	}
	return result;
}

std::string ListString(PIActionList list, uint32 index)
{
	char buffer[512] = {};
	return sList->GetString(list, index, buffer, sizeof buffer) ? std::string() : std::string(buffer);
}

// PostScript names of every font Photoshop has.
std::set<std::string> InstalledFonts()
{
	std::set<std::string> names;
	PIActionDescriptor app = GetProperty("fontList", classApplication);
	if (!app) return names;
	DescriptorClassID type = 0;
	PIActionDescriptor fontList = nullptr;
	if (!sDesc->GetObject(app, ID("fontList"), &type, &fontList) && fontList) {
		PIActionList list = nullptr;
		if (!sDesc->GetList(fontList, ID("fontPostScriptName"), &list) && list) {
			uint32 count = 0;
			sList->GetCount(list, &count);
			for (uint32 i = 0; i < count; i++) {
				std::string name = ListString(list, i);
				if (!name.empty()) names.insert(name);
			}
			sList->Free(list);
		}
		sDesc->Free(fontList);
	}
	sDesc->Free(app);
	return names;
}

// PostScript names the active document's text layers use, in layer order.
std::vector<std::string> DocumentFonts()
{
	std::vector<std::string> fonts;
	std::set<std::string> seen;
	PIActionDescriptor doc = GetProperty("numberOfLayers", classDocument);
	if (!doc) return fonts;
	int32 layers = 0;
	sDesc->GetInteger(doc, ID("numberOfLayers"), &layers);
	sDesc->Free(doc);

	DescriptorKeyID textKey = ID("textKey"), styleRanges = ID("textStyleRange"), textStyle = ID("textStyle"),
		postscript = ID("fontPostScriptName");
	for (int32 i = 1; i <= layers; i++) {
		PIActionDescriptor layer = GetProperty("textKey", classLayer, i);
		if (!layer) continue;
		DescriptorClassID type = 0;
		PIActionDescriptor text = nullptr;
		if (!sDesc->GetObject(layer, textKey, &type, &text) && text) {
			PIActionList ranges = nullptr;
			if (!sDesc->GetList(text, styleRanges, &ranges) && ranges) {
				uint32 count = 0;
				sList->GetCount(ranges, &count);
				for (uint32 r = 0; r < count; r++) {
					PIActionDescriptor range = nullptr, style = nullptr;
					if (sList->GetObject(ranges, r, &type, &range) || !range) continue;
					if (!sDesc->GetObject(range, textStyle, &type, &style) && style) {
						char name[512] = {};
						if (!sDesc->GetString(style, postscript, name, sizeof name) && name[0] && seen.insert(name).second)
							fonts.push_back(name);
						sDesc->Free(style);
					}
					sDesc->Free(range);
				}
				sList->Free(ranges);
			}
			sDesc->Free(text);
		}
		sDesc->Free(layer);
	}
	return fonts;
}

// Asks GlyphClub for the current document's missing fonts. Automatic checks
// ask for each font once per session, so a font GlyphClub doesn't have isn't
// asked about on every open; File > Automate > Glyph Club always asks.
void Check(bool automatic)
{
	if (!gStarted || gBusy) return;
	std::vector<std::string> used = DocumentFonts();
	if (used.empty()) return;
	std::set<std::string> installed = InstalledFonts();
	std::vector<std::string> ask;
	for (const auto& name : used) {
		if (installed.count(name)) continue;
		if (automatic && gAsked.count(name)) continue;
		ask.push_back(name);
	}
	if (ask.empty()) return;
	for (const auto& name : ask) gAsked.insert(name);
	gBusy = true;
	glyphclub::bridge::Activate(ask, true, [ask](const glyphclub::bridge::Result& result) {
		gBusy = false;
		// Never reached GlyphClub: these weren't really asked.
		if (!result.ok)
			for (const auto& name : ask) gAsked.erase(name);
	});
}

void OnOpen(DescriptorEventID, PIActionDescriptor, PIDialogRecordOptions, void*)
{
	Check(true);
}

void Startup()
{
	glyphclub::bridge::SetHost("photoshop", GLYPHCLUB_PS_VERSION);
	glyphclub::platform::Start();
	sControl->AddNotify(sSelf, eventOpen, OnOpen, nullptr);
	gStarted = true;
}

void Shutdown()
{
	if (gStarted && sControl) sControl->RemoveNotify(sSelf, eventOpen);
	gStarted = false;
	glyphclub::platform::Stop();
	ReleaseSuites();
}

} // namespace

#ifdef _WIN32
#define GLYPHCLUB_EXPORT extern "C" __declspec(dllexport)
#else
#define GLYPHCLUB_EXPORT extern "C" __attribute__((visibility("default")))
#endif

GLYPHCLUB_EXPORT SPAPI SPErr AutoPluginMain(const char* caller, const char* selector, void* message)
{
	SPMessageData* data = (SPMessageData*) message;
	SPBasicSuite* basic = data->basic;
	if (basic->IsEqual(caller, kSPInterfaceCaller)) {
		if (basic->IsEqual(selector, kSPInterfaceStartupSelector)) {
			sBasic = basic;
			sSelf = data->self;
			bool ok = Acquire(kPSActionControlSuite, kPSActionControlSuiteVersion, sControl)
				&& Acquire(kPSActionDescriptorSuite, kPSActionDescriptorSuiteVersion, sDesc)
				&& Acquire(kPSActionReferenceSuite, kPSActionReferenceSuiteVersion, sRef)
				&& Acquire(kPSActionListSuite, kPSActionListSuiteVersion, sList);
			if (!ok) {
				ReleaseSuites();   // stays quiet rather than half-working
				return kSPNoError;
			}
			try { Startup(); } catch (...) {}
		}
		else if (basic->IsEqual(selector, kSPInterfaceShutdownSelector)) {
			Shutdown();
		}
		return kSPNoError;
	}
	// File > Automate > Glyph Club: ask now for the current document.
	if (basic->IsEqual(caller, kPSPhotoshopCaller) && basic->IsEqual(selector, kPSDoIt)) {
		try { Check(false); } catch (...) {}
	}
	return kSPNoError;
}
