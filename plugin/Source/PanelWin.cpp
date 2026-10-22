// Windows side of Panel.h (Panel.mm + PanelView.mm are the macOS one): the
// Glyph Club panel's contents, painted into Illustrator's docked panel in
// its theme colors. One window draws everything (no native controls), so
// it matches Illustrator's light and dark themes, and clicks don't take the
// keyboard away from the document.

#include "IllustratorSDK.h"
#include "Panel.h"
#include "GlyphClubSuites.h"
#include "AIUITheme.h"

#include <windows.h>
#include <windowsx.h>

#include <algorithm>
#include <deque>
#include <string>

extern "C" SPBasicSuite* sSPBasic;

namespace {

// ---- state, kept even without a window so one made later starts current

AIPanelRef gPanel = nullptr;
PanelCallbacks gCallbacks;
bool gConnected = false;
std::wstring gStatus = L"Looking for Glyph Club…";
bool gAuto = true;
int gMissing = 0;
bool gBusy = false;
std::wstring gUpdate;

struct LogLine {
	std::wstring text, time;
	PanelLog kind;
};
std::deque<LogLine> gLog;   // newest first
constexpr size_t kMaxLog = 50;

HWND gView = nullptr;
const wchar_t* const kViewClass = L"GlyphClubPanelView";
bool gClassRegistered = false;

// Hit areas from the last paint, in client pixels.
RECT gToggleRect{}, gActivateRect{}, gOpenRect{};
enum class Hot { None, Toggle, Activate, Open } gHot = Hot::None, gPressed = Hot::None;

std::wstring Wide(const std::string& s)
{
	int n = MultiByteToWideChar(CP_UTF8, 0, s.c_str(), -1, nullptr, 0);
	std::wstring out(n > 0 ? n - 1 : 0, L'\0');
	if (n > 1) MultiByteToWideChar(CP_UTF8, 0, s.c_str(), -1, out.data(), n);
	return out;
}

HINSTANCE ThisModule()
{
	HMODULE module = nullptr;
	GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, (LPCWSTR) &ThisModule, &module);
	return module;
}

// ---- colors

AIUIThemeSuite* Theme()
{
	static AIUIThemeSuite* suite = nullptr;
	static bool tried = false;
	if (!tried) {
		tried = true;
		const void* s = nullptr;
		if (sSPBasic && !sSPBasic->AcquireSuite(kAIUIThemeSuite, kAIUIThemeSuiteVersion, &s)) suite = (AIUIThemeSuite*) s;
	}
	return suite;
}

struct Colors {
	COLORREF background, text, dim, faint, accent, warning, control, controlText, line;
	bool dark;
};

COLORREF Mix(COLORREF a, COLORREF b, double t)
{
	auto m = [t](int x, int y) { return (BYTE) std::clamp((int) (x + (y - x) * t + 0.5), 0, 255); };
	return RGB(m(GetRValue(a), GetRValue(b)), m(GetGValue(a), GetGValue(b)), m(GetBValue(a), GetBValue(b)));
}

Colors CurrentColors()
{
	COLORREF background = RGB(0x32, 0x32, 0x32);
	bool dark = true;
	if (AIUIThemeSuite* t = Theme()) {
		AIUIThemeColor c;
		if (!t->GetUIThemeColor(kAIUIThemeSelectorPanel, kAIUIComponentColorBackground, c))
			background = RGB((BYTE) (c.red * 255), (BYTE) (c.green * 255), (BYTE) (c.blue * 255));
		dark = t->IsUIThemeDark();
	}
	Colors c;
	c.dark = dark;
	c.background = background;
	c.text = dark ? RGB(0xEE, 0xEE, 0xEE) : RGB(0x1E, 0x1E, 0x1E);
	c.dim = Mix(c.text, background, 0.40);
	c.faint = Mix(c.text, background, 0.62);
	// Glyph Club's lime: the neon one on dark UI, the deeper olive on light
	// (same pair as the app and the macOS panel).
	c.accent = dark ? RGB(0x9A, 0xFF, 0x00) : RGB(0x5C, 0x8A, 0x00);
	c.warning = dark ? RGB(0xFF, 0x9F, 0x0A) : RGB(0xC9, 0x6A, 0x00);
	c.control = Mix(background, c.text, dark ? 0.14 : 0.10);
	c.controlText = c.text;
	c.line = Mix(background, c.text, 0.12);
	return c;
}

// ---- fonts, per DPI

double Scale(HWND window)
{
	UINT dpi = window ? GetDpiForWindow(window) : 96;
	return dpi ? dpi / 96.0 : 1.0;
}

struct Fonts {
	double scale = 0;
	HFONT title = nullptr, body = nullptr, bodyBold = nullptr, note = nullptr, caption = nullptr;
	void Clear()
	{
		for (HFONT f : {title, body, bodyBold, note, caption})
			if (f) DeleteObject(f);
		*this = Fonts();
	}
	void Ensure(double s)
	{
		if (scale == s && title) return;
		Clear();
		scale = s;
		auto make = [s](double px, int weight) {
			return CreateFontW(-(int) (px * s + 0.5), 0, 0, 0, weight, FALSE, FALSE, FALSE, DEFAULT_CHARSET,
				OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, DEFAULT_PITCH, L"Segoe UI");
		};
		title = make(13, FW_SEMIBOLD);
		body = make(12, FW_NORMAL);
		bodyBold = make(12, FW_SEMIBOLD);
		note = make(11, FW_NORMAL);
		caption = make(9.5, FW_SEMIBOLD);
	}
} gFonts;

// ---- painting

void FillRounded(HDC dc, const RECT& r, int radius, COLORREF color)
{
	HBRUSH brush = CreateSolidBrush(color);
	HPEN pen = CreatePen(PS_SOLID, 1, color);
	HGDIOBJ oldBrush = SelectObject(dc, brush), oldPen = SelectObject(dc, pen);
	RoundRect(dc, r.left, r.top, r.right, r.bottom, radius, radius);
	SelectObject(dc, oldBrush);
	SelectObject(dc, oldPen);
	DeleteObject(brush);
	DeleteObject(pen);
}

void Dot(HDC dc, int cx, int cy, int size, COLORREF color)
{
	RECT r = {cx - size / 2, cy - size / 2, cx - size / 2 + size, cy - size / 2 + size};
	FillRounded(dc, r, size, color);
}

int TextHeight(HDC dc, HFONT font)
{
	HGDIOBJ old = SelectObject(dc, font);
	TEXTMETRICW tm;
	GetTextMetricsW(dc, &tm);
	SelectObject(dc, old);
	return tm.tmHeight;
}

// One line, cut with … to fit; returns the rect used.
RECT Text(HDC dc, const std::wstring& text, HFONT font, COLORREF color, int x, int y, int right, UINT align = DT_LEFT)
{
	HGDIOBJ old = SelectObject(dc, font);
	SetTextColor(dc, color);
	RECT r = {x, y, std::max(x, right), y + TextHeight(dc, font)};
	DrawTextW(dc, text.c_str(), (int) text.size(), &r, align | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX | DT_VCENTER);
	SelectObject(dc, old);
	return r;
}

int TextWidth(HDC dc, const std::wstring& text, HFONT font)
{
	HGDIOBJ old = SelectObject(dc, font);
	SIZE size{};
	GetTextExtentPoint32W(dc, text.c_str(), (int) text.size(), &size);
	SelectObject(dc, old);
	return size.cx;
}

RECT Button(HDC dc, const std::wstring& label, int x, int y, double s, bool enabled, bool hot, bool pressed, const Colors& c)
{
	int w = TextWidth(dc, label, gFonts.body) + (int) (20 * s), h = (int) (24 * s);
	RECT r = {x, y, x + w, y + h};
	COLORREF fill = !enabled ? Mix(c.background, c.text, 0.06) : pressed ? Mix(c.control, c.text, 0.18) : hot ? Mix(c.control, c.text, 0.08) : c.control;
	FillRounded(dc, r, (int) (6 * s), fill);
	Text(dc, label, gFonts.body, enabled ? c.controlText : c.faint, r.left, r.top + (h - TextHeight(dc, gFonts.body)) / 2, r.right, DT_CENTER);
	return r;
}

std::wstring MissingText()
{
	if (gMissing == 0) return L"No missing fonts in open documents";
	if (gMissing == 1) return L"1 missing font in open documents";
	return std::to_wstring(gMissing) + L" missing fonts in open documents";
}

bool ActivateEnabled() { return !gBusy && gMissing > 0; }

void Paint(HWND window, HDC target)
{
	RECT client;
	GetClientRect(window, &client);
	int width = client.right - client.left, height = client.bottom - client.top;
	if (width <= 0 || height <= 0) return;

	// Drawn off-screen, then copied: no flicker while the log updates.
	HDC dc = CreateCompatibleDC(target);
	HBITMAP bitmap = CreateCompatibleBitmap(target, width, height);
	HGDIOBJ oldBitmap = SelectObject(dc, bitmap);
	SetBkMode(dc, TRANSPARENT);

	double s = Scale(window);
	gFonts.Ensure(s);
	Colors c = CurrentColors();
	HBRUSH background = CreateSolidBrush(c.background);
	FillRect(dc, &client, background);
	DeleteObject(background);

	int pad = (int) (12 * s), x = pad, right = width - pad, y = pad;
	auto gap = [&](double px) { y += (int) (px * s); };

	RECT r = Text(dc, L"Glyph Club", gFonts.title, c.text, x, y, right);
	y = r.bottom;
	gap(2);

	// Status: a dot that lights up when connected.
	int lineH = TextHeight(dc, gFonts.note);
	Dot(dc, x + (int) (4 * s), y + lineH / 2, (int) (8 * s), gConnected ? c.accent : c.faint);
	r = Text(dc, gStatus, gFonts.note, c.dim, x + (int) (14 * s), y, right);
	y = r.bottom;
	gap(14);

	// Only when GlyphClub ships a newer plug-in: only GlyphClub can install it.
	gOpenRect = {};
	if (!gUpdate.empty()) {
		r = Text(dc, L"Plug-in update " + gUpdate + L" available", gFonts.bodyBold, c.warning, x, y, right);
		y = r.bottom;
		gap(2);
		r = Text(dc, L"Update it from Glyph Club's Settings.", gFonts.note, c.dim, x, y, right);
		y = r.bottom;
		gap(6);
		gOpenRect = Button(dc, L"Open Glyph Club", x, y, s, true, gHot == Hot::Open, gPressed == Hot::Open, c);
		y = gOpenRect.bottom;
		gap(14);
	}

	// Auto-activate, with a switch at the right.
	int switchW = (int) (30 * s), switchH = (int) (16 * s);
	int rowH = std::max(TextHeight(dc, gFonts.body), switchH);
	Text(dc, L"Auto-activate missing fonts", gFonts.body, c.text, x, y + (rowH - TextHeight(dc, gFonts.body)) / 2, right - switchW - (int) (8 * s));
	gToggleRect = {right - switchW, y + (rowH - switchH) / 2, right, y + (rowH - switchH) / 2 + switchH};
	COLORREF track = gAuto ? c.accent : Mix(c.background, c.text, gHot == Hot::Toggle ? 0.32 : 0.25);
	FillRounded(dc, gToggleRect, switchH, track);
	int knob = switchH - (int) (4 * s);
	int knobX = gAuto ? gToggleRect.right - (int) (2 * s) - knob : gToggleRect.left + (int) (2 * s);
	RECT knobRect = {knobX, gToggleRect.top + (int) (2 * s), knobX + knob, gToggleRect.top + (int) (2 * s) + knob};
	FillRounded(dc, knobRect, knob, gAuto && !c.dark ? RGB(0xFF, 0xFF, 0xFF) : gAuto ? RGB(0x1E, 0x1E, 0x1E) : RGB(0xFF, 0xFF, 0xFF));
	y += rowH;
	gap(4);

	r = Text(dc, MissingText(), gFonts.note, gMissing ? c.text : c.dim, x, y, right);
	y = r.bottom;
	gap(8);

	gActivateRect = Button(dc, L"Activate Missing Fonts", x, y, s, ActivateEnabled(), gHot == Hot::Activate, gPressed == Hot::Activate, c);
	if (gBusy) Text(dc, L"Working…", gFonts.note, c.dim, gActivateRect.right + (int) (8 * s), gActivateRect.top, right);
	y = gActivateRect.bottom;
	gap(16);

	r = Text(dc, L"RECENT", gFonts.caption, c.faint, x, y, right);
	y = r.bottom;
	gap(4);

	// Newest first; whatever fits.
	int logH = TextHeight(dc, gFonts.note) + (int) (4 * s);
	for (const auto& line : gLog) {
		if (y + logH > height - pad / 2) break;
		COLORREF mark = line.kind == PanelLog::Activated ? c.accent : line.kind == PanelLog::Problem ? c.warning : c.faint;
		Dot(dc, x + (int) (3 * s), y + logH / 2, (int) (6 * s), mark);
		int timeW = TextWidth(dc, line.time, gFonts.note);
		Text(dc, line.time, gFonts.note, c.faint, right - timeW, y + (int) (2 * s), right);
		Text(dc, line.text, gFonts.note, c.text, x + (int) (12 * s), y + (int) (2 * s), right - timeW - (int) (8 * s));
		y += logH;
	}

	BitBlt(target, 0, 0, width, height, dc, 0, 0, SRCCOPY);
	SelectObject(dc, oldBitmap);
	DeleteObject(bitmap);
	DeleteDC(dc);
}

void Redraw()
{
	if (gView) InvalidateRect(gView, nullptr, FALSE);
}

Hot HitTest(int x, int y)
{
	POINT p = {x, y};
	if (PtInRect(&gToggleRect, p)) return Hot::Toggle;
	if (ActivateEnabled() && PtInRect(&gActivateRect, p)) return Hot::Activate;
	if (!gUpdate.empty() && PtInRect(&gOpenRect, p)) return Hot::Open;
	return Hot::None;
}

void Click(Hot target)
{
	switch (target) {
	case Hot::Toggle:
		gAuto = !gAuto;
		Redraw();
		if (gCallbacks.autoChanged) gCallbacks.autoChanged(gAuto);
		break;
	case Hot::Activate:
		if (gCallbacks.activateNow) gCallbacks.activateNow();
		break;
	case Hot::Open:
		if (gCallbacks.openGlyphClub) gCallbacks.openGlyphClub();
		break;
	case Hot::None:
		break;
	}
}

LRESULT CALLBACK ViewProc(HWND window, UINT message, WPARAM wParam, LPARAM lParam)
{
	switch (message) {
	case WM_ERASEBKGND:
		return 1;
	case WM_PAINT: {
		PAINTSTRUCT ps;
		HDC dc = BeginPaint(window, &ps);
		Paint(window, dc);
		EndPaint(window, &ps);
		return 0;
	}
	case WM_MOUSEACTIVATE:
		return MA_NOACTIVATE;   // clicks work without taking focus from the document
	case WM_MOUSEMOVE: {
		Hot hot = HitTest(GET_X_LPARAM(lParam), GET_Y_LPARAM(lParam));
		if (hot != gHot) { gHot = hot; Redraw(); }
		TRACKMOUSEEVENT track = {sizeof track, TME_LEAVE, window, 0};
		TrackMouseEvent(&track);
		return 0;
	}
	case WM_MOUSELEAVE:
		if (gHot != Hot::None || gPressed != Hot::None) { gHot = gPressed = Hot::None; Redraw(); }
		return 0;
	case WM_LBUTTONDOWN:
		gPressed = HitTest(GET_X_LPARAM(lParam), GET_Y_LPARAM(lParam));
		if (gPressed != Hot::None) { SetCapture(window); Redraw(); }
		return 0;
	case WM_LBUTTONUP: {
		Hot pressed = gPressed;
		gPressed = Hot::None;
		if (GetCapture() == window) ReleaseCapture();
		Redraw();
		if (pressed != Hot::None && HitTest(GET_X_LPARAM(lParam), GET_Y_LPARAM(lParam)) == pressed) Click(pressed);
		return 0;
	}
	case WM_SETCURSOR:
		if (LOWORD(lParam) == HTCLIENT) {
			POINT p;
			GetCursorPos(&p);
			ScreenToClient(window, &p);
			SetCursor(LoadCursor(nullptr, HitTest(p.x, p.y) != Hot::None ? IDC_HAND : IDC_ARROW));
			return TRUE;
		}
		break;
	case WM_DPICHANGED_AFTERPARENT:
		Redraw();
		return 0;
	}
	return DefWindowProcW(window, message, wParam, lParam);
}

// ---- hosting in Illustrator's panel

HWND Host()
{
	AIPanelPlatformWindow host = nullptr;
	if (!gPanel || !sAIPanel || sAIPanel->GetPlatformWindow(gPanel, host)) return nullptr;
	return (HWND) host;
}

void Fit()
{
	HWND host = Host();
	if (!gView || !host) return;
	if (GetParent(gView) != host) SetParent(gView, host);
	RECT r;
	GetClientRect(host, &r);
	SetWindowPos(gView, nullptr, 0, 0, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
	Redraw();
}

void SizeChanged(AIPanelRef) { Fit(); }

// Illustrator hands out the panel's host window lazily, so every update
// makes sure our view is in it.
void Install()
{
	HWND host = Host();
	if (!host) return;
	if (gView) {
		if (GetParent(gView) != host) Fit();
		return;
	}
	if (!gClassRegistered) {
		WNDCLASSEXW wc = {sizeof wc};
		wc.style = CS_HREDRAW | CS_VREDRAW;
		wc.lpfnWndProc = ViewProc;
		wc.hInstance = ThisModule();
		wc.hCursor = LoadCursor(nullptr, IDC_ARROW);
		wc.lpszClassName = kViewClass;
		gClassRegistered = RegisterClassExW(&wc) != 0;
	}
	RECT r;
	GetClientRect(host, &r);
	gView = CreateWindowExW(0, kViewClass, L"Glyph Club", WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
		0, 0, r.right - r.left, r.bottom - r.top, host, nullptr, ThisModule(), nullptr);
	if (gView) sAIPanel->SetSizeChangedNotifyProc(gPanel, SizeChanged);
}

std::wstring Now()
{
	SYSTEMTIME t;
	GetLocalTime(&t);
	wchar_t buffer[32] = {};
	GetTimeFormatEx(LOCALE_NAME_USER_DEFAULT, TIME_NOSECONDS, &t, nullptr, buffer, 32);
	return buffer;
}

} // namespace

void PanelAttach(AIPanelRef panel, PanelCallbacks callbacks)
{
	gPanel = panel;
	gCallbacks = std::move(callbacks);
	AISize minSize = {220, 240}, pref = {260, 320}, maxSize = {600, 2000};
	sAIPanel->SetSizes(gPanel, minSize, pref, pref, maxSize);
	Install();
}

void PanelDetach()
{
	if (gView) DestroyWindow(gView);
	gView = nullptr;
	gFonts.Clear();
	if (gClassRegistered) { UnregisterClassW(kViewClass, ThisModule()); gClassRegistered = false; }
	gPanel = nullptr;
	gCallbacks = {};
}

void PanelSetConnected(bool connected, const std::string& text)
{
	gConnected = connected;
	gStatus = Wide(text);
	Install();
	Redraw();
}

void PanelSetAutoActivate(bool on)
{
	gAuto = on;
	Install();
	Redraw();
}

void PanelSetMissingCount(int count)
{
	gMissing = count;
	Install();
	Redraw();
}

void PanelSetBusy(bool busy)
{
	gBusy = busy;
	Install();
	Redraw();
}

void PanelLogLine(const std::string& line, PanelLog kind)
{
	gLog.push_front({Wide(line), Now(), kind});
	if (gLog.size() > kMaxLog) gLog.pop_back();
	Install();
	Redraw();
}

void PanelSetUpdate(const std::string& version)
{
	gUpdate = Wide(version);
	Install();
	Redraw();
}
