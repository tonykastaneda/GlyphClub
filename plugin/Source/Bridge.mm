#include "Bridge.h"
#include "GlyphClubID.h"

#import <Foundation/Foundation.h>

#include <arpa/inet.h>
#include <netinet/in.h>
#include <signal.h>
#include <spawn.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <sys/wait.h>
#include <unistd.h>

extern char** environ;

namespace glyphclub::bridge {
namespace {

std::string gHost = "illustrator";
std::string gVersion = GLYPHCLUB_VERSION;

// Activation can mean copying fonts out of iCloud/Dropbox first.
constexpr int kReplyTimeoutSeconds = 25;
// How long a launched GlyphClub gets to open its catalog and start listening.
constexpr double kLaunchWaitSeconds = 20;

struct Session {
	int port = 0;
	std::string token;
};

NSString* SessionPath()
{
	return [NSHomeDirectory() stringByAppendingPathComponent:@"Library/Application Support/glyphclub/bridge.json"];
}

// bridge.json, but only while the GlyphClub that wrote it is still alive: a
// crashed app leaves the file behind.
bool ReadSession(Session& session)
{
	NSData* data = [NSData dataWithContentsOfFile:SessionPath()];
	if (!data) return false;
	NSDictionary* json = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
	if (![json isKindOfClass:[NSDictionary class]]) return false;
	NSNumber* port = json[@"port"];
	NSNumber* pid = json[@"pid"];
	NSString* token = json[@"token"];
	if (![port isKindOfClass:[NSNumber class]] || ![token isKindOfClass:[NSString class]]) return false;
	if ([pid isKindOfClass:[NSNumber class]] && kill(pid.intValue, 0) != 0 && errno == ESRCH) return false;
	session.port = port.intValue;
	session.token = token.UTF8String;
	return session.port > 0 && !session.token.empty();
}

// `open -g`: launch without bringing GlyphClub to the front - the person is
// working in Illustrator. False when there's no GlyphClub to launch.
bool LaunchGlyphClub()
{
	// --background: GlyphClub stays in the menu bar rather than opening its
	// window (a GlyphClub without the menu bar on still opens it).
	const char* argv[] = {"/usr/bin/open", "-g", "-b", kGlyphClubAppBundleID, "--args", "--background", nullptr};
	pid_t pid = 0;
	if (posix_spawn(&pid, argv[0], nullptr, nullptr, (char* const*) argv, environ) != 0) return false;
	int status = 0;
	if (waitpid(pid, &status, 0) != pid) return false;
	return WIFEXITED(status) && WEXITSTATUS(status) == 0;
}

bool WaitForSession(Session& session)
{
	NSDate* deadline = [NSDate dateWithTimeIntervalSinceNow:kLaunchWaitSeconds];
	while ([deadline timeIntervalSinceNow] > 0) {
		if (ReadSession(session)) return true;
		usleep(250 * 1000);
	}
	return false;
}

// One request line out, one reply line back.
NSDictionary* Call(const Session& session, NSDictionary* request, std::string& error)
{
	NSMutableDictionary* body = [request mutableCopy];
	body[@"token"] = [NSString stringWithUTF8String:session.token.c_str()];
	body[@"pluginVersion"] = [NSString stringWithUTF8String:gVersion.c_str()];
	body[@"host"] = [NSString stringWithUTF8String:gHost.c_str()];
	NSMutableData* line = [[NSJSONSerialization dataWithJSONObject:body options:0 error:nil] mutableCopy];
	if (!line) { error = "couldn't encode the request"; return nil; }
	[line appendBytes:"\n" length:1];

	int fd = socket(AF_INET, SOCK_STREAM, 0);
	if (fd < 0) { error = "no socket"; return nil; }
	int one = 1;
	setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &one, sizeof one);
	timeval timeout = {kReplyTimeoutSeconds, 0};
	setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof timeout);
	setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &timeout, sizeof timeout);

	sockaddr_in addr = {};
	addr.sin_family = AF_INET;
	addr.sin_port = htons((uint16_t) session.port);
	addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
	if (connect(fd, (sockaddr*) &addr, sizeof addr) != 0) {
		close(fd);
		error = "GlyphClub isn't answering";
		return nil;
	}

	const char* bytes = (const char*) line.bytes;
	size_t left = line.length;
	while (left > 0) {
		ssize_t sent = send(fd, bytes, left, 0);
		if (sent <= 0) { close(fd); error = "couldn't send to GlyphClub"; return nil; }
		bytes += sent;
		left -= (size_t) sent;
	}

	NSMutableData* reply = [NSMutableData data];
	char buffer[16384];
	for (;;) {
		ssize_t got = recv(fd, buffer, sizeof buffer, 0);
		if (got <= 0) break;
		[reply appendBytes:buffer length:(NSUInteger) got];
		if (memchr(buffer, '\n', (size_t) got)) break;
	}
	close(fd);

	NSDictionary* json = reply.length ? [NSJSONSerialization JSONObjectWithData:reply options:0 error:nil] : nil;
	if (![json isKindOfClass:[NSDictionary class]]) {
		error = reply.length ? "GlyphClub sent an unreadable reply" : "GlyphClub didn't reply in time";
		return nil;
	}
	if (![json[@"ok"] boolValue]) {
		NSString* message = json[@"error"];
		error = [message isKindOfClass:[NSString class]] ? message.UTF8String : "GlyphClub refused the request";
		return nil;
	}
	return json;
}

std::vector<std::string> Strings(id value)
{
	std::vector<std::string> out;
	if (![value isKindOfClass:[NSArray class]]) return out;
	for (id item in (NSArray*) value)
		if ([item isKindOfClass:[NSString class]]) out.push_back([(NSString*) item UTF8String]);
	return out;
}

void ReadLatest(NSDictionary* reply, Result& result)
{
	NSString* latest = reply[@"latestPluginVersion"];
	if ([latest isKindOfClass:[NSString class]]) result.latestPluginVersion = latest.UTF8String;
}

// GlyphClub's session, starting GlyphClub in the background if needed.
bool Connect(Session& session, bool launch, std::string& error)
{
	if (ReadSession(session)) return true;
	if (!launch) { error = "GlyphClub isn't running"; return false; }
	if (!LaunchGlyphClub()) { error = "GlyphClub isn't running"; return false; }
	if (!WaitForSession(session)) { error = "GlyphClub didn't start in time"; return false; }
	return true;
}

void Finish(std::function<void(const Result&)> done, Result result)
{
	dispatch_async(dispatch_get_main_queue(), ^{ done(result); });
}

} // namespace

void SetHost(const std::string& host, const std::string& version)
{
	gHost = host;
	gVersion = version;
}

void Ping(std::function<void(const Result&)> done)
{
	dispatch_async(dispatch_get_global_queue(QOS_CLASS_UTILITY, 0), ^{
		@autoreleasepool {
			Result result;
			Session session;
			if (!ReadSession(session)) {
				result.error = "GlyphClub isn't running";
				return Finish(done, result);
			}
			NSDictionary* reply = Call(session, @{@"method": @"ping"}, result.error);
			if (reply) {
				result.ok = true;
				ReadLatest(reply, result);
				NSString* version = reply[@"version"];
				if ([version isKindOfClass:[NSString class]]) result.appVersion = version.UTF8String;
			}
			Finish(done, result);
		}
	});
}

void Activate(std::vector<std::string> postscriptNames, bool launch, std::function<void(const Result&)> done)
{
	dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
		@autoreleasepool {
			Result result;
			Session session;
			if (!Connect(session, launch, result.error)) return Finish(done, result);
			NSMutableArray* names = [NSMutableArray array];
			for (const auto& name : postscriptNames) {
				NSString* s = [NSString stringWithUTF8String:name.c_str()];
				if (s) [names addObject:s];
			}
			NSDictionary* reply = Call(session, @{@"method": @"activate", @"fonts": names}, result.error);
			if (reply) {
				result.ok = true;
				ReadLatest(reply, result);
				result.activated = Strings(reply[@"activated"]);
				result.alreadyActive = Strings(reply[@"alreadyActive"]);
				result.notFound = Strings(reply[@"notFound"]);
				result.failed = Strings(reply[@"failed"]);
			}
			Finish(done, result);
		}
	});
}

void OpenIntegrations(std::function<void(const Result&)> done)
{
	dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
		@autoreleasepool {
			Result result;
			Session session;
			if (!Connect(session, true, result.error)) return Finish(done, result);
			NSDictionary* reply = Call(session, @{@"method": @"openIntegrations"}, result.error);
			result.ok = reply != nil;
			// GlyphClub may be a menu bar app with no window up: it shows
			// its window itself, but only the frontmost app can pull focus.
			if (result.ok) {
				const char* argv[] = {"/usr/bin/open", "-b", kGlyphClubAppBundleID, nullptr};
				pid_t pid = 0;
				if (posix_spawn(&pid, argv[0], nullptr, nullptr, (char* const*) argv, environ) == 0) {
					int status = 0;
					waitpid(pid, &status, 0);
				}
			}
			Finish(done, result);
		}
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
