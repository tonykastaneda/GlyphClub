#ifndef __GLYPHCLUB_BRIDGE_H__
#define __GLYPHCLUB_BRIDGE_H__

// Talks to the running GlyphClub app over the loopback connection it
// advertises in ~/Library/Application Support/glyphclub/bridge.json (see
// src/illustrator_bridge.rs in the app). Never touches the Illustrator SDK:
// requests run on a background queue and `done` is called on the main queue.

#include <functional>
#include <string>
#include <vector>

namespace glyphclub::bridge {

struct Result {
	bool ok = false;
	std::string error;          // why not, when !ok
	std::string appVersion;     // ping
	std::string latestPluginVersion;   // the plug-in this GlyphClub ships, if it says
	std::vector<std::string> activated, alreadyActive, notFound, failed;
};

// Is GlyphClub running with its bridge up? (Checks bridge.json and the pid.)
void Ping(std::function<void(const Result&)> done);

// Asks GlyphClub to activate fonts by PostScript name. With `launch`, starts
// GlyphClub in the background first when it isn't running.
void Activate(std::vector<std::string> postscriptNames, bool launch, std::function<void(const Result&)> done);

// Brings GlyphClub up on Settings > Integrations, where the plug-in is updated.
// Starts GlyphClub first if it isn't running.
void OpenIntegrations(std::function<void(const Result&)> done);

// Whether `candidate` is a newer dotted version than `current`.
bool IsNewerVersion(const std::string& candidate, const std::string& current);

} // namespace glyphclub::bridge

#endif // __GLYPHCLUB_BRIDGE_H__
