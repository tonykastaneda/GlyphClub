// Windows side of Bridge.h (Bridge.mm is the macOS one): the same one-line
// JSON requests to the running GlyphClub over 127.0.0.1, found through
// %APPDATA%\glyphclub\bridge.json. Requests run on a background thread and
// `done` is called on Illustrator's main thread (Platform.h).

#include "Bridge.h"
#include "GlyphClubID.h"
#include "Platform.h"

#include <winsock2.h>
#include <ws2tcpip.h>
#include <windows.h>
#include <shlobj.h>

#include <algorithm>
#include <chrono>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <memory>
#include <mutex>
#include <sstream>
#include <thread>

namespace glyphclub::bridge {
namespace {

// Activation can mean copying fonts out of OneDrive/Dropbox first.
constexpr int kReplyTimeoutSeconds = 25;
// How long a launched GlyphClub gets to open its catalog and start listening.
constexpr int kLaunchWaitSeconds = 20;

// ---- a small JSON reader: enough for bridge.json and the bridge's replies

struct Json {
	enum Kind { Null, Bool, Number, String, Array, Object } kind = Null;
	bool boolean = false;
	double number = 0;
	std::string string;
	std::vector<Json> items;          // an array's, or an object's values
	std::vector<std::string> keys;    // an object's keys, matching items

	const Json* Get(const std::string& key) const
	{
		if (kind != Object) return nullptr;
		for (size_t i = 0; i < keys.size(); i++)
			if (keys[i] == key) return &items[i];
		return nullptr;
	}
};

struct Parser {
	const std::string& text;
	size_t at = 0;
	bool ok = true;

	void Space() { while (at < text.size() && (text[at] == ' ' || text[at] == '\t' || text[at] == '\r' || text[at] == '\n')) at++; }
	bool Eat(char c)
	{
		Space();
		if (at < text.size() && text[at] == c) { at++; return true; }
		return false;
	}
	static void AppendUtf8(std::string& out, unsigned code)
	{
		if (code < 0x80) out += (char) code;
		else if (code < 0x800) { out += (char) (0xC0 | (code >> 6)); out += (char) (0x80 | (code & 0x3F)); }
		else if (code < 0x10000) { out += (char) (0xE0 | (code >> 12)); out += (char) (0x80 | ((code >> 6) & 0x3F)); out += (char) (0x80 | (code & 0x3F)); }
		else { out += (char) (0xF0 | (code >> 18)); out += (char) (0x80 | ((code >> 12) & 0x3F)); out += (char) (0x80 | ((code >> 6) & 0x3F)); out += (char) (0x80 | (code & 0x3F)); }
	}
	unsigned Hex4()
	{
		if (at + 4 > text.size()) { ok = false; return 0; }
		unsigned v = (unsigned) strtoul(text.substr(at, 4).c_str(), nullptr, 16);
		at += 4;
		return v;
	}
	std::string Str()
	{
		std::string out;
		if (!Eat('"')) { ok = false; return out; }
		while (at < text.size() && text[at] != '"') {
			char c = text[at++];
			if (c != '\\') { out += c; continue; }
			if (at >= text.size()) break;
			char e = text[at++];
			switch (e) {
			case 'n': out += '\n'; break;
			case 't': out += '\t'; break;
			case 'r': out += '\r'; break;
			case 'b': out += '\b'; break;
			case 'f': out += '\f'; break;
			case 'u': {
				unsigned code = Hex4();
				if (code >= 0xD800 && code < 0xDC00 && at + 1 < text.size() && text[at] == '\\' && text[at + 1] == 'u') {
					at += 2;
					unsigned low = Hex4();
					code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
				}
				AppendUtf8(out, code);
				break;
			}
			default: out += e;
			}
		}
		if (at >= text.size()) ok = false;
		else at++;
		return out;
	}
	Json Value(int depth = 0)
	{
		Json v;
		Space();
		if (at >= text.size() || depth > 32) { ok = false; return v; }
		char c = text[at];
		if (c == '{') {
			at++;
			v.kind = Json::Object;
			if (Eat('}')) return v;
			do {
				std::string key = Str();
				if (!Eat(':')) { ok = false; return v; }
				v.keys.push_back(key);
				v.items.push_back(Value(depth + 1));
			} while (ok && Eat(','));
			if (!Eat('}')) ok = false;
		}
		else if (c == '[') {
			at++;
			v.kind = Json::Array;
			if (Eat(']')) return v;
			do v.items.push_back(Value(depth + 1)); while (ok && Eat(','));
			if (!Eat(']')) ok = false;
		}
		else if (c == '"') { v.kind = Json::String; v.string = Str(); }
		else if (text.compare(at, 4, "true") == 0) { v.kind = Json::Bool; v.boolean = true; at += 4; }
		else if (text.compare(at, 5, "false") == 0) { v.kind = Json::Bool; at += 5; }
		else if (text.compare(at, 4, "null") == 0) { at += 4; }
		else {
			char* end = nullptr;
			v.number = strtod(text.c_str() + at, &end);
			if (end == text.c_str() + at) ok = false;
			v.kind = Json::Number;
			at = end - text.c_str();
		}
		return v;
	}
};

bool Parse(const std::string& text, Json& out)
{
	Parser p{text};
	out = p.Value();
	return p.ok;
}

std::string Quote(const std::string& s)
{
	std::string out = "\"";
	for (unsigned char c : s) {
		if (c == '"') out += "\\\"";
		else if (c == '\\') out += "\\\\";
		else if (c < 0x20) { char buf[8]; snprintf(buf, sizeof buf, "\\u%04x", c); out += buf; }
		else out += (char) c;
	}
	return out + "\"";
}

std::wstring Wide(const std::string& s)
{
	int n = MultiByteToWideChar(CP_UTF8, 0, s.c_str(), -1, nullptr, 0);
	std::wstring out(n > 0 ? n - 1 : 0, L'\0');
	if (n > 1) MultiByteToWideChar(CP_UTF8, 0, s.c_str(), -1, out.data(), n);
	return out;
}

// ---- GlyphClub's session

struct Session {
	int port = 0;
	std::string token;
	DWORD pid = 0;
};

// %APPDATA%\glyphclub, where GlyphClub keeps bridge.json (dirs::data_dir).
std::wstring GlyphClubDataDir()
{
	PWSTR path = nullptr;
	std::wstring dir;
	if (SUCCEEDED(SHGetKnownFolderPath(FOLDERID_RoamingAppData, 0, nullptr, &path))) dir = std::wstring(path) + L"\\glyphclub";
	CoTaskMemFree(path);
	return dir;
}

bool ReadFile(const std::wstring& path, std::string& out)
{
	std::ifstream file(path, std::ios::binary);
	if (!file) return false;
	std::stringstream buffer;
	buffer << file.rdbuf();
	out = buffer.str();
	return true;
}

bool ProcessAlive(DWORD pid)
{
	HANDLE process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
	if (!process) return GetLastError() == ERROR_ACCESS_DENIED;   // exists, just not ours to open
	DWORD code = 0;
	bool alive = GetExitCodeProcess(process, &code) && code == STILL_ACTIVE;
	CloseHandle(process);
	return alive;
}

// bridge.json, but only while the GlyphClub that wrote it is still alive: a
// crashed app leaves the file behind.
bool ReadSession(Session& session)
{
	std::string text;
	Json json;
	if (!ReadFile(GlyphClubDataDir() + L"\\bridge.json", text) || !Parse(text, json)) return false;
	const Json* port = json.Get("port");
	const Json* token = json.Get("token");
	const Json* pid = json.Get("pid");
	if (!port || port->kind != Json::Number || !token || token->kind != Json::String) return false;
	if (pid && pid->kind == Json::Number) {
		session.pid = (DWORD) pid->number;
		if (!ProcessAlive(session.pid)) return false;
	}
	session.port = (int) port->number;
	session.token = token->string;
	return session.port > 0 && !session.token.empty();
}

// GlyphClub.exe, which GlyphClub records in %APPDATA%\glyphclub\app-path.txt
// every time it starts (a zip install can live anywhere).
std::wstring GlyphClubExe()
{
	std::string path;
	if (!ReadFile(GlyphClubDataDir() + L"\\app-path.txt", path)) return {};
	while (!path.empty() && (path.back() == ' ' || path.back() == '\t' || path.back() == '\r' || path.back() == '\n')) path.pop_back();
	std::wstring exe = Wide(path);
	return GetFileAttributesW(exe.c_str()) == INVALID_FILE_ATTRIBUTES ? std::wstring() : exe;
}

// Starts GlyphClub without taking the focus from Illustrator (--background).
// False when there's no GlyphClub to launch.
bool LaunchGlyphClub(bool background)
{
	std::wstring exe = GlyphClubExe();
	if (exe.empty()) return false;
	std::wstring command = L"\"" + exe + L"\"" + (background ? L" --background" : L"");
	STARTUPINFOW startup = {sizeof startup};
	startup.dwFlags = STARTF_USESHOWWINDOW;
	startup.wShowWindow = background ? SW_SHOWNOACTIVATE : SW_SHOWNORMAL;
	PROCESS_INFORMATION process = {};
	std::wstring dir = exe.substr(0, exe.find_last_of(L"\\/"));
	if (!CreateProcessW(exe.c_str(), command.data(), nullptr, nullptr, FALSE, 0, nullptr, dir.c_str(), &startup, &process)) return false;
	CloseHandle(process.hThread);
	CloseHandle(process.hProcess);
	return true;
}

bool WaitForSession(Session& session)
{
	auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(kLaunchWaitSeconds);
	while (std::chrono::steady_clock::now() < deadline) {
		if (ReadSession(session)) return true;
		Sleep(250);
	}
	return false;
}

bool StartWinsock()
{
	static std::once_flag once;
	static bool ok = false;
	std::call_once(once, [] {
		WSADATA data;
		ok = WSAStartup(MAKEWORD(2, 2), &data) == 0;
	});
	return ok;
}

// One request line out, one reply line back. `fields` is the request's
// JSON members after the token, e.g. "\"method\":\"ping\"".
bool Call(const Session& session, const std::string& fields, Json& reply, std::string& error)
{
	if (!StartWinsock()) { error = "no network"; return false; }
	std::string line = "{\"token\":" + Quote(session.token) + ",\"pluginVersion\":" + Quote(GLYPHCLUB_VERSION) + "," + fields + "}\n";

	SOCKET s = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
	if (s == INVALID_SOCKET) { error = "no socket"; return false; }
	DWORD timeout = kReplyTimeoutSeconds * 1000;
	setsockopt(s, SOL_SOCKET, SO_RCVTIMEO, (const char*) &timeout, sizeof timeout);
	setsockopt(s, SOL_SOCKET, SO_SNDTIMEO, (const char*) &timeout, sizeof timeout);

	sockaddr_in addr = {};
	addr.sin_family = AF_INET;
	addr.sin_port = htons((u_short) session.port);
	addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
	if (connect(s, (sockaddr*) &addr, sizeof addr) != 0) {
		closesocket(s);
		error = "GlyphClub isn't answering";
		return false;
	}

	const char* bytes = line.data();
	int left = (int) line.size();
	while (left > 0) {
		int sent = send(s, bytes, left, 0);
		if (sent <= 0) { closesocket(s); error = "couldn't send to GlyphClub"; return false; }
		bytes += sent;
		left -= sent;
	}

	std::string text;
	char buffer[16384];
	for (;;) {
		int got = recv(s, buffer, sizeof buffer, 0);
		if (got <= 0) break;
		text.append(buffer, got);
		if (memchr(buffer, '\n', got)) break;
	}
	closesocket(s);

	if (text.empty() || !Parse(text, reply) || reply.kind != Json::Object) {
		error = text.empty() ? "GlyphClub didn't reply in time" : "GlyphClub sent an unreadable reply";
		return false;
	}
	const Json* ok = reply.Get("ok");
	if (!ok || ok->kind != Json::Bool || !ok->boolean) {
		const Json* message = reply.Get("error");
		error = message && message->kind == Json::String ? message->string : "GlyphClub refused the request";
		return false;
	}
	return true;
}

std::vector<std::string> Strings(const Json* value)
{
	std::vector<std::string> out;
	if (!value || value->kind != Json::Array) return out;
	for (const auto& item : value->items)
		if (item.kind == Json::String) out.push_back(item.string);
	return out;
}

void ReadLatest(const Json& reply, Result& result)
{
	const Json* latest = reply.Get("latestPluginVersion");
	if (latest && latest->kind == Json::String) result.latestPluginVersion = latest->string;
}

// GlyphClub's session, starting GlyphClub in the background if needed.
bool Connect(Session& session, bool launch, bool background, std::string& error)
{
	if (ReadSession(session)) return true;
	if (!launch) { error = "GlyphClub isn't running"; return false; }
	if (!LaunchGlyphClub(background)) { error = "GlyphClub isn't running"; return false; }
	if (!WaitForSession(session)) { error = "GlyphClub didn't start in time"; return false; }
	return true;
}

void Finish(std::function<void(const Result&)> done, Result result)
{
	platform::RunOnMain([done = std::move(done), result = std::move(result)] { done(result); });
}

} // namespace

void Ping(std::function<void(const Result&)> done)
{
	platform::RunInBackground([done = std::move(done)] {
		Result result;
		Session session;
		if (!ReadSession(session)) {
			result.error = "GlyphClub isn't running";
			return Finish(done, result);
		}
		Json reply;
		if (Call(session, "\"method\":\"ping\"", reply, result.error)) {
			result.ok = true;
			ReadLatest(reply, result);
			const Json* version = reply.Get("version");
			if (version && version->kind == Json::String) result.appVersion = version->string;
		}
		Finish(done, result);
	});
}

void Activate(std::vector<std::string> postscriptNames, bool launch, std::function<void(const Result&)> done)
{
	platform::RunInBackground([names = std::move(postscriptNames), launch, done = std::move(done)] {
		Result result;
		Session session;
		if (!Connect(session, launch, true, result.error)) return Finish(done, result);
		std::string fonts = "[";
		for (size_t i = 0; i < names.size(); i++) fonts += (i ? "," : "") + Quote(names[i]);
		fonts += "]";
		Json reply;
		if (Call(session, "\"method\":\"activate\",\"fonts\":" + fonts, reply, result.error)) {
			result.ok = true;
			ReadLatest(reply, result);
			result.activated = Strings(reply.Get("activated"));
			result.alreadyActive = Strings(reply.Get("alreadyActive"));
			result.notFound = Strings(reply.Get("notFound"));
			result.failed = Strings(reply.Get("failed"));
		}
		Finish(done, result);
	});
}

void OpenIntegrations(std::function<void(const Result&)> done)
{
	// Illustrator is in front, so it may hand the foreground on: without
	// this Windows only flashes GlyphClub's taskbar button.
	AllowSetForegroundWindow(ASFW_ANY);
	platform::RunInBackground([done = std::move(done)] {
		Result result;
		Session session;
		if (!Connect(session, true, false, result.error)) return Finish(done, result);
		if (session.pid) AllowSetForegroundWindow(session.pid);
		Json reply;
		result.ok = Call(session, "\"method\":\"openIntegrations\"", reply, result.error);
		Finish(done, result);
	});
}

bool IsNewerVersion(const std::string& candidate, const std::string& current)
{
	auto parts = [](const std::string& v) {
		std::vector<long> out;
		size_t start = 0;
		while (start <= v.size()) {
			size_t dot = v.find('.', start);
			std::string piece = v.substr(start, dot == std::string::npos ? std::string::npos : dot - start);
			out.push_back(piece.empty() ? 0 : strtol(piece.c_str(), nullptr, 10));
			if (dot == std::string::npos) break;
			start = dot + 1;
		}
		return out;
	};
	auto a = parts(candidate), b = parts(current);
	size_t n = std::max(a.size(), b.size());
	a.resize(n, 0);
	b.resize(n, 0);
	return a > b;
}

} // namespace glyphclub::bridge
