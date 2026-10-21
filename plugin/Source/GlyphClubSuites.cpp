#include "IllustratorSDK.h"
#include "GlyphClubSuites.h"

extern "C" {
	SPBlocksSuite*			sSPBlocks = nullptr;
	AIMenuSuite*			sAIMenu = nullptr;
	AIUnicodeStringSuite*	sAIUnicodeString = nullptr;
	AIDocumentSuite*		sAIDocument = nullptr;
	AIDocumentListSuite*	sAIDocumentList = nullptr;
	AIFontSuite*			sAIFont = nullptr;
	AIPanelSuite*			sAIPanel = nullptr;
	AIAssertionSuite*		sAIAssertion = nullptr;
};

// The font and document suites are the whole point, so they're required; the
// panel is optional (without it, Window > Glyph Club reports in an alert).
ImportSuite gImportSuites[] =
{
	kSPBlocksSuite, kSPBlocksSuiteVersion, &sSPBlocks,
	kAIMenuSuite, kAIMenuSuiteVersion, &sAIMenu,
	kAIUnicodeStringSuite, kAIUnicodeStringVersion, &sAIUnicodeString,
	kAIFontSuite, kAIFontSuiteVersion, &sAIFont,
	kAIDocumentSuite, kAIDocumentSuiteVersion, &sAIDocument,
	kAIDocumentListSuite, kAIDocumentListSuiteVersion, &sAIDocumentList,
	nullptr, kStartOptionalSuites, nullptr,
	kAIPanelSuite, kAIPanelSuiteVersion, &sAIPanel,
	kAIAssertionSuite, kAIAssertionSuiteVersion, &sAIAssertion,
	nullptr, 0, nullptr
};
