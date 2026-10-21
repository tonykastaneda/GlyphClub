#ifndef __GLYPHCLUB_SUITES_H__
#define __GLYPHCLUB_SUITES_H__

#include "IllustratorSDK.h"
#include "Suites.hpp"

#include "AIMenu.h"
#include "AIDocument.h"
#include "AIDocumentList.h"
#include "AIFont.h"
#include "AIPanel.h"
#include "AIATETextUtil.h"
#include "AIAssert.hpp"

extern "C" SPBlocksSuite*			sSPBlocks;
extern "C" AIMenuSuite*				sAIMenu;
extern "C" AIUnicodeStringSuite*	sAIUnicodeString;
extern "C" AIDocumentSuite*			sAIDocument;
extern "C" AIDocumentListSuite*		sAIDocumentList;
extern "C" AIFontSuite*				sAIFont;
extern "C" AIPanelSuite*			sAIPanel;

#endif // __GLYPHCLUB_SUITES_H__
