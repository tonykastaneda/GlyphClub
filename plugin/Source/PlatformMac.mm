#include "Platform.h"

#import <Foundation/Foundation.h>

namespace glyphclub::platform {
namespace {
bool gRunning = false;
dispatch_source_t gTimer = nullptr;
}

void Start() { gRunning = true; }

void Stop()
{
	gRunning = false;
	if (gTimer) { dispatch_source_cancel(gTimer); gTimer = nullptr; }
}

bool LoadBool(const char* key, bool fallback)
{
	NSNumber* saved = [[NSUserDefaults standardUserDefaults] objectForKey:@(key)];
	return saved ? saved.boolValue : fallback;
}

void SaveBool(const char* key, bool value)
{
	[[NSUserDefaults standardUserDefaults] setBool:value forKey:@(key)];
}

void RunOnMain(std::function<void()> work)
{
	dispatch_async(dispatch_get_main_queue(), ^{ if (gRunning) work(); });
}

void RunInBackground(std::function<void()> work)
{
	dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{ @autoreleasepool { work(); } });
}

void Repeat(double seconds, std::function<void()> tick)
{
	if (gTimer) dispatch_source_cancel(gTimer);
	gTimer = dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER, 0, 0, dispatch_get_main_queue());
	uint64_t interval = (uint64_t) (seconds * NSEC_PER_SEC);
	dispatch_source_set_timer(gTimer, dispatch_time(DISPATCH_TIME_NOW, interval), interval, interval / 5);
	dispatch_source_set_event_handler(gTimer, ^{ if (gRunning) tick(); });
	dispatch_resume(gTimer);
}

} // namespace glyphclub::platform
