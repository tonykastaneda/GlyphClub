//========================================================================================
// GlyphClub for InDesign: object model IDs.
//========================================================================================
#ifndef __GCID_h__
#define __GCID_h__

#include "SDKDef.h"

#define kGCCompanyKey		"GlyphClub"
#define kGCCompanyValue		"GlyphClub"
#define kGCPluginName		"GlyphClub"
// Unique prefix for this plug-in's IDs. Adobe hands these out to commercial
// plug-ins; this one is picked from a range no SDK sample or Adobe plug-in uses.
#define kGCPrefixNumber		0x1F3A00
#define kGCAuthor			"GlyphClub"
#define kGCPrefix			RezLong(kGCPrefixNumber)
#define kGCStringPrefix		SDK_DEF_STRINGIZE(kGCPrefixNumber)

#ifndef GLYPHCLUB_ID_VERSION
#define GLYPHCLUB_ID_VERSION "dev"
#endif
#define kGCVersion			GLYPHCLUB_ID_VERSION

// Plug-in
DECLARE_PMID(kPlugInIDSpace, kGCPluginID, kGCPrefix + 0)

// Boss: answers InDesign's missing font signal.
DECLARE_PMID(kClassIDSpace, kGCMissingFontResponderBoss, kGCPrefix + 0)

// Implementations
DECLARE_PMID(kImplementationIDSpace, kGCMissingFontResponderImpl, kGCPrefix + 0)

// Persistent data format: none, but InDesign wants the numbers.
#define kGCFirstMajorFormatNumber	kSDKDef_30_PersistMajorVersionNumber
#define kGCFirstMinorFormatNumber	kSDKDef_30_PersistMinorVersionNumber
#define kGCCurrentMajorFormatNumber	kGCFirstMajorFormatNumber
#define kGCCurrentMinorFormatNumber	kGCFirstMinorFormatNumber

#endif // __GCID_h__
