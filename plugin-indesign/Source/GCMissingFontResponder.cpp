// GlyphClub for InDesign: asks the running GlyphClub app for the fonts a
// document uses that InDesign can't find, the way the Illustrator plug-in
// (plugin/) does.
//
// After a document opens (kAfterOpenDocSignalResponderService, registered
// with InDesign's own service provider in GlyphClub.fr) it lists the
// document's missing fonts (IUsedFontList) by PostScript name and asks
// GlyphClub for them, through the bridge shared with the other plug-ins
// (Bridge.h), which starts GlyphClub in the background if it isn't running.
// Each font is asked for once per session. When GlyphClub
// activates something, InDesign's font manager is told to look again, so
// the document picks the fonts up straight away. The responder never
// supplies a font itself, so it never stands in the way of InDesign's own
// substitution.
#include "VCPlugInHeaders.h"
#include "ISignalMgr.h"
#include "IDocumentSignalData.h"
#include "IDocument.h"
#include "IUsedFontList.h"
#include "IFontFamily.h"
#include "IFontMgr.h"
#include "ISession.h"
#include "DocumentID.h"
#include "FontMgrID.h"
#include "CResponder.h"
#include "GCID.h"

#include "Bridge.h"
#include "Platform.h"

#include <chrono>
#include <thread>
#include <set>
#include <string>
#include <vector>

namespace {

bool gStarted = false;
bool gFlushQueued = false;
std::set<std::string> gAsked;
std::vector<std::string> gPending;

void RefreshFonts()
{
	InterfacePtr<IFontMgr> fontMgr(GetExecutionContextSession(), UseDefaultIID());
	if (!fontMgr) return;
	// A full check makes CoolType look at the system's fonts again, which
	// is what picks up fonts activated while InDesign runs.
	fontMgr->CurrentFontSystemSeed(kTrue);
}

// macOS registers a newly activated font a moment after GlyphClub replies,
// so InDesign is told to look again a little later too.
void RefreshFontsSoon()
{
	RefreshFonts();
	glyphclub::platform::RunInBackground([] {
		for (int ms : {1500, 2500}) {
			std::this_thread::sleep_for(std::chrono::milliseconds(ms));
			glyphclub::platform::RunOnMain(RefreshFonts);
		}
	});
}

void Flush()
{
	gFlushQueued = false;
	std::vector<std::string> names;
	names.swap(gPending);
	if (names.empty()) return;
	glyphclub::bridge::Activate(names, true, [names](const glyphclub::bridge::Result& result) {
		// Never reached GlyphClub: let a later open ask again.
		if (!result.ok)
			for (const auto& name : names) gAsked.erase(name);
		else if (!result.activated.empty() || !result.alreadyActive.empty())
			RefreshFontsSoon();
	});
}

void Ask(const std::string& name)
{
	if (name.empty() || !gAsked.insert(name).second) return;
	gPending.push_back(name);
	// The rest of a burst arrives before this runs.
	if (!gFlushQueued) {
		gFlushQueued = true;
		glyphclub::platform::RunOnMain(Flush);
	}
}

// The document's missing fonts, by PostScript name.
void AskForMissingFonts(const UIDRef& docRef)
{
	InterfacePtr<IDocument> document(docRef, UseDefaultIID());
	if (!document) return;
	InterfacePtr<IUsedFontList> used(document, UseDefaultIID());
	if (!used) return;
	int32 count = used->CountMissingFonts(kTrue);
	for (int32 i = 0; i < count; i++) {
		UID familyUID = kInvalidUID;
		PMString style;
		used->GetMissingFont(i, &familyUID, style, nil, nil, kFalse);
		InterfacePtr<IFontFamily> family(::GetDataBase(document), familyUID, UseDefaultIID());
		if (!family) continue;
		std::string name = family->GetFacePostscriptName(style).GetUTF8String();
		Ask(name);
	}
}

} // namespace

class GCMissingFontResponder : public CResponder
{
public:
	GCMissingFontResponder(IPMUnknown* boss) : CResponder(boss) {}
	virtual void Respond(ISignalMgr* signalMgr);
};

CREATE_PMINTERFACE(GCMissingFontResponder, kGCMissingFontResponderImpl)

void GCMissingFontResponder::Respond(ISignalMgr* signalMgr)
{
	if (!gStarted) {
		gStarted = true;
		glyphclub::bridge::SetHost("indesign", GLYPHCLUB_ID_VERSION);
		glyphclub::platform::Start();
	}
	InterfacePtr<IDocumentSignalData> docData(signalMgr, UseDefaultIID());
	if (docData) AskForMissingFonts(docData->GetDocument());
}
