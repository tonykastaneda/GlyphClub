// References every implementation so the linker doesn't dead-strip them.
#include "VCPlugInHeaders.h"
#include "InterfaceFactory.h"

extern bool16 gFalse;
void DontDeadStrip();
void DontDeadStrip()
{
	if (gFalse) {
#include "GCFactoryList.h"
	}
}
