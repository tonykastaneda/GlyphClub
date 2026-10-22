#ifndef __GLYPHCLUB_PLATFORM_H__
#define __GLYPHCLUB_PLATFORM_H__

// The few things the plug-in needs from the OS beyond the Illustrator SDK:
// PlatformMac.mm (NSUserDefaults, GCD) and PlatformWin.cpp (the registry,
// a message-only window on Illustrator's main thread).

#include <functional>

namespace glyphclub::platform {

// Main thread, from StartupPlugin / ShutdownPlugin.
void Start();
void Stop();

// A saved on/off setting (the panel's auto-activate switch).
bool LoadBool(const char* key, bool fallback);
void SaveBool(const char* key, bool value);

// Runs `work` on Illustrator's main thread soon. Callable from any thread;
// dropped once Stop has run.
void RunOnMain(std::function<void()> work);

// Runs `work` on a background thread.
void RunInBackground(std::function<void()> work);

// Calls `tick` on the main thread every `seconds` until Stop.
void Repeat(double seconds, std::function<void()> tick);

} // namespace glyphclub::platform

#endif // __GLYPHCLUB_PLATFORM_H__
