#include "Platform.h"

#include <windows.h>

#include <memory>
#include <mutex>
#include <string>
#include <thread>

namespace glyphclub::platform {
namespace {

constexpr UINT kRunMessage = WM_APP + 1;
constexpr UINT_PTR kRepeatTimer = 1;
const wchar_t* const kWindowClass = L"GlyphClubPluginMessages";
const wchar_t* const kSettingsKey = L"Software\\GlyphClub\\Illustrator";

// A message-only window owned by Illustrator's main thread: other threads
// post work to it, and its timer drives Repeat.
HWND gWindow = nullptr;
std::mutex gWindowLock;   // guards gWindow for posting threads
std::function<void()> gTick;

HINSTANCE ThisModule()
{
	HMODULE module = nullptr;
	GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
		(LPCWSTR) &ThisModule, &module);
	return module;
}

LRESULT CALLBACK WindowProc(HWND window, UINT message, WPARAM wParam, LPARAM lParam)
{
	if (message == kRunMessage) {
		std::unique_ptr<std::function<void()>> work((std::function<void()>*) lParam);
		if (work && *work) (*work)();
		return 0;
	}
	if (message == WM_TIMER && wParam == kRepeatTimer) {
		if (gTick) gTick();
		return 0;
	}
	return DefWindowProcW(window, message, wParam, lParam);
}

std::wstring Wide(const char* s)
{
	int n = MultiByteToWideChar(CP_UTF8, 0, s, -1, nullptr, 0);
	std::wstring out(n > 0 ? n - 1 : 0, L'\0');
	if (n > 1) MultiByteToWideChar(CP_UTF8, 0, s, -1, out.data(), n);
	return out;
}

} // namespace

void Start()
{
	WNDCLASSEXW wc = {sizeof wc};
	wc.lpfnWndProc = WindowProc;
	wc.hInstance = ThisModule();
	wc.lpszClassName = kWindowClass;
	RegisterClassExW(&wc);   // fails harmlessly if a reload already registered it
	HWND window = CreateWindowExW(0, kWindowClass, L"", 0, 0, 0, 0, 0, HWND_MESSAGE, nullptr, wc.hInstance, nullptr);
	std::lock_guard<std::mutex> lock(gWindowLock);
	gWindow = window;
}

void Stop()
{
	HWND window;
	{
		std::lock_guard<std::mutex> lock(gWindowLock);
		window = gWindow;
		gWindow = nullptr;
	}
	if (!window) return;
	KillTimer(window, kRepeatTimer);
	gTick = nullptr;
	// Work already posted is freed unrun.
	MSG message;
	while (PeekMessageW(&message, window, kRunMessage, kRunMessage, PM_REMOVE))
		delete (std::function<void()>*) message.lParam;
	DestroyWindow(window);
	UnregisterClassW(kWindowClass, ThisModule());
}

bool LoadBool(const char* key, bool fallback)
{
	DWORD value = 0, size = sizeof value;
	if (RegGetValueW(HKEY_CURRENT_USER, kSettingsKey, Wide(key).c_str(), RRF_RT_REG_DWORD, nullptr, &value, &size) != ERROR_SUCCESS)
		return fallback;
	return value != 0;
}

void SaveBool(const char* key, bool value)
{
	HKEY handle = nullptr;
	if (RegCreateKeyExW(HKEY_CURRENT_USER, kSettingsKey, 0, nullptr, 0, KEY_SET_VALUE, nullptr, &handle, nullptr) != ERROR_SUCCESS) return;
	DWORD data = value ? 1 : 0;
	RegSetValueExW(handle, Wide(key).c_str(), 0, REG_DWORD, (const BYTE*) &data, sizeof data);
	RegCloseKey(handle);
}

void RunOnMain(std::function<void()> work)
{
	auto boxed = new std::function<void()>(std::move(work));
	std::lock_guard<std::mutex> lock(gWindowLock);
	if (!gWindow || !PostMessageW(gWindow, kRunMessage, 0, (LPARAM) boxed)) delete boxed;
}

void RunInBackground(std::function<void()> work)
{
	std::thread(std::move(work)).detach();
}

void Repeat(double seconds, std::function<void()> tick)
{
	gTick = std::move(tick);
	if (gWindow) SetTimer(gWindow, kRepeatTimer, (UINT) (seconds * 1000), nullptr);
}

} // namespace glyphclub::platform
