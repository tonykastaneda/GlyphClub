//! `glyphclub-cli`: GlyphClub from the command line. It works on the same
//! catalog as the app (through the shared core in `src/lib.rs`), and when
//! the app is running it asks it to reload afterwards, so the window shows
//! the change.

use std::collections::{BTreeMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use clap::{Parser, Subcommand};
use glyphclub::catalog::Catalog;
use glyphclub::font::is_system_path;
use glyphclub::store::FontRow;
use serde_json::{json, Value};

#[derive(Parser)]
#[command(name = "glyphclub-cli", version, about = "Manage GlyphClub libraries and fonts from the command line")]
struct Cli {
    /// Print machine-readable JSON instead of text.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// What's catalogued and active, and whether the app is running.
    Status,
    /// List your libraries.
    Libraries,
    /// Add a folder as a library (and scan it).
    Add { folder: PathBuf },
    /// Remove a library (the files stay where they are).
    Remove {
        /// The library's folder, name or id.
        library: String,
    },
    /// Rescan every library for added, changed or removed fonts.
    Sync,
    /// List font families.
    List {
        /// Only this library (folder name or id).
        #[arg(long)]
        library: Option<String>,
        /// Only families with an active style.
        #[arg(long)]
        active: bool,
        /// Only starred families.
        #[arg(long)]
        starred: bool,
        /// Include system fonts.
        #[arg(long)]
        system: bool,
    },
    /// Find families by name or PostScript name.
    Search { text: String },
    /// A font's metadata.
    Info {
        /// A family name or PostScript name.
        font: String,
    },
    /// Activate fonts by family or PostScript name.
    Activate {
        #[arg(required = true)]
        fonts: Vec<String>,
        /// Only until GlyphClub next quits.
        #[arg(long)]
        temp: bool,
    },
    /// Deactivate fonts by family or PostScript name.
    Deactivate {
        #[arg(required = true)]
        fonts: Vec<String>,
    },
    /// Copy every user-installed font into <folder>/User-FontExport.
    ExportUserFonts { folder: PathBuf },
    /// List the bundled icon sets, or the icons in one.
    Icons { set: Option<String> },
    /// Save one icon as an SVG of vector paths (what dragging it out does).
    IconSvg {
        set: String,
        icon: String,
        out: PathBuf,
        /// Height in pixels.
        #[arg(long, default_value_t = 64.0)]
        size: f32,
    },
    /// Drive the running app's UI, e.g. `app view list`, `app select Optima`,
    /// `app tab glyphs`, `app action ToggleDarkMode`, `app state`,
    /// `app screenshot shot.png`. `app help` lists every command.
    App {
        command: String,
        args: Vec<String>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            if cli.json {
                println!("{}", json!({ "ok": false, "error": format!("{e:#}") }));
            } else {
                eprintln!("glyphclub-cli: {e:#}");
            }
            ExitCode::FAILURE
        }
    }
}

fn open_catalog() -> Result<Catalog> {
    Catalog::open(|| {})
}

/// One family in one library, the way the app groups them.
struct Family {
    name: String,
    folder_id: i64,
    faces: Vec<FontRow>,
}

fn families(rows: Vec<FontRow>) -> Vec<Family> {
    let mut map: BTreeMap<(String, i64), Family> = BTreeMap::new();
    for row in rows {
        if row.record.family.starts_with('.') {
            continue;
        }
        let key = (row.record.family.to_lowercase(), row.folder_id);
        map.entry(key)
            .or_insert_with(|| Family { name: row.record.family.clone(), folder_id: row.folder_id, faces: Vec::new() })
            .faces
            .push(row);
    }
    map.into_values().collect()
}

/// Face ids for a name: an exact family name (every style), else an exact
/// PostScript name.
fn resolve(rows: &[FontRow], name: &str) -> Vec<i64> {
    let lower = name.to_lowercase();
    let by_family: Vec<i64> = rows
        .iter()
        .filter(|r| r.record.family.to_lowercase() == lower)
        .map(|r| r.id)
        .collect();
    if !by_family.is_empty() {
        return by_family;
    }
    rows.iter()
        .filter(|r| r.record.postscript_name.to_lowercase() == lower)
        .map(|r| r.id)
        .collect()
}

fn folder_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

fn find_library(catalog: &Catalog, which: &str) -> Result<glyphclub::store::Folder> {
    let folders = catalog.list_folders()?;
    let wanted = which.trim_end_matches('/');
    folders
        .into_iter()
        .find(|f| {
            f.id.to_string() == which
                || f.path.trim_end_matches('/') == wanted
                || folder_name(&f.path).eq_ignore_ascii_case(wanted)
        })
        .ok_or_else(|| anyhow!("no library matches “{which}” (see `glyphclub-cli libraries`)"))
}

fn run(cli: &Cli) -> Result<()> {
    let json_out = cli.json;
    match &cli.command {
        Command::Status => {
            let catalog = open_catalog()?;
            let rows = catalog.list_fonts("")?;
            let active = catalog.active_ids()?;
            let libraries = catalog.list_folders()?.into_iter().filter(|f| !is_system_path(&f.path)).count();
            let running = app_request("ping").is_ok();
            if json_out {
                println!(
                    "{}",
                    json!({ "ok": true, "fonts": rows.len(), "families": families(rows).len(), "active": active.len(), "libraries": libraries, "appRunning": running })
                );
            } else {
                println!("{} fonts in {} families, {} libraries", rows.len(), families(rows).len(), libraries);
                println!("{} active", active.len());
                println!("GlyphClub is {}", if running { "running" } else { "not running" });
            }
        }
        Command::Libraries => {
            let catalog = open_catalog()?;
            let rows = catalog.list_fonts("")?;
            let mut out = Vec::new();
            for f in catalog.list_folders()?.into_iter().filter(|f| !is_system_path(&f.path)) {
                let count = rows.iter().filter(|r| r.folder_id == f.id).count();
                out.push((f.id, folder_name(&f.path), f.path, count));
            }
            if json_out {
                let list: Vec<Value> = out
                    .iter()
                    .map(|(id, name, path, count)| json!({ "id": id, "name": name, "path": path, "fonts": count }))
                    .collect();
                println!("{}", json!({ "ok": true, "libraries": list }));
            } else if out.is_empty() {
                println!("No libraries yet. Add one with `glyphclub-cli add <folder>`.");
            } else {
                for (id, name, path, count) in out {
                    println!("{id:>4}  {name:<24} {count:>6} fonts  {path}");
                }
            }
        }
        Command::Add { folder } => {
            let path = std::fs::canonicalize(folder).map_err(|e| anyhow!("{}: {e}", folder.display()))?;
            if !path.is_dir() {
                bail!("{} isn't a folder", path.display());
            }
            let mut catalog = open_catalog()?;
            let result = catalog.add_folder(&path.to_string_lossy())?;
            tell_app_to_reload();
            done(json_out, &result.status);
        }
        Command::Remove { library } => {
            let mut catalog = open_catalog()?;
            let folder = find_library(&catalog, library)?;
            catalog.remove_folder(folder.id)?;
            tell_app_to_reload();
            done(json_out, &format!("Removed {} (its files are untouched)", folder.path));
        }
        Command::Sync => {
            let mut catalog = open_catalog()?;
            let result = catalog.rescan()?;
            tell_app_to_reload();
            done(json_out, &result.status);
        }
        Command::List { library, active, starred, system } => {
            let catalog = open_catalog()?;
            let folder = library.as_deref().map(|l| find_library(&catalog, l)).transpose()?;
            let active_ids = catalog.active_ids()?;
            let starred_ids = catalog.favorite_ids()?;
            let fams: Vec<Family> = families(catalog.list_fonts("")?)
                .into_iter()
                .filter(|f| folder.as_ref().is_none_or(|lib| lib.id == f.folder_id))
                .filter(|f| *system || folder.is_some() || !f.faces.iter().all(|r| is_system_path(&r.record.path)))
                .filter(|f| !*active || f.faces.iter().any(|r| active_ids.contains(&r.id)))
                .filter(|f| !*starred || f.faces.iter().any(|r| starred_ids.contains(&r.id)))
                .collect();
            print_families(&fams, &active_ids, &starred_ids, json_out);
        }
        Command::Search { text } => {
            let catalog = open_catalog()?;
            let q = text.to_lowercase();
            let active_ids = catalog.active_ids()?;
            let starred_ids = catalog.favorite_ids()?;
            let fams: Vec<Family> = families(catalog.list_fonts("")?)
                .into_iter()
                .filter(|f| {
                    f.name.to_lowercase().contains(&q)
                        || f.faces.iter().any(|r| r.record.postscript_name.to_lowercase().contains(&q))
                })
                .collect();
            print_families(&fams, &active_ids, &starred_ids, json_out);
        }
        Command::Info { font } => {
            let catalog = open_catalog()?;
            let rows = catalog.list_fonts("")?;
            let ids = resolve(&rows, font);
            let Some(row) = rows.iter().find(|r| ids.first() == Some(&r.id)) else {
                bail!("no font named “{font}”");
            };
            let data = catalog.font_bytes(row.id)?;
            let info = glyphclub::font_info::extract(&data, &row.record.path)
                .ok_or_else(|| anyhow!("couldn't read {}", row.record.path))?;
            let active = catalog.active_ids()?.contains(&row.id);
            if json_out {
                println!(
                    "{}",
                    json!({
                        "ok": true, "fullName": info.full_name, "family": info.family, "style": info.style,
                        "postscriptName": info.postscript_name, "version": info.version, "format": info.format,
                        "variable": info.is_variable, "glyphs": info.glyph_count, "characters": info.codepoints.len(),
                        "designer": info.designer, "manufacturer": info.manufacturer, "copyright": info.copyright,
                        "license": info.license_description, "path": row.record.path, "active": active,
                    })
                );
            } else {
                let line = |label: &str, v: Option<&str>| {
                    if let Some(v) = v.filter(|v| !v.trim().is_empty()) {
                        println!("{label:<16}{v}");
                    }
                };
                line("Full name", Some(&info.full_name));
                line("Style", Some(&info.style));
                line("PostScript name", info.postscript_name.as_deref());
                line("Version", info.version.as_deref());
                line("Format", Some(info.format));
                line("Glyphs", Some(&info.glyph_count.to_string()));
                line("Characters", Some(&info.codepoints.len().to_string()));
                line("Variable", Some(if info.is_variable { "yes" } else { "no" }));
                line("Designer", info.designer.as_deref());
                line("Manufacturer", info.manufacturer.as_deref());
                line("Active", Some(if active { "yes" } else { "no" }));
                line("Location", Some(&row.record.path));
            }
        }
        Command::Activate { fonts, temp } => set_active(fonts, true, *temp, json_out)?,
        Command::Deactivate { fonts } => set_active(fonts, false, false, json_out)?,
        Command::Icons { set } => {
            let sets = glyphclub::icons::load_sets();
            match set {
                None => {
                    if json_out {
                        let list: Vec<Value> = sets
                            .iter()
                            .map(|s| json!({ "slug": s.slug, "name": s.name, "icons": s.icons.len(), "license": s.license, "version": s.version }))
                            .collect();
                        println!("{}", json!({ "ok": true, "sets": list }));
                    } else {
                        for s in &sets {
                            println!("{:<26} {:>6} icons  {}", s.slug, s.icons.len(), s.license);
                        }
                    }
                }
                Some(which) => {
                    let s = find_set(&sets, which)?;
                    if json_out {
                        let list: Vec<Value> = s.icons.iter().map(|i| json!({ "name": i.name, "codepoint": i.codepoint })).collect();
                        println!("{}", json!({ "ok": true, "set": s.name, "icons": list }));
                    } else {
                        for i in &s.icons {
                            println!("U+{:04X}  {}", i.codepoint, i.name);
                        }
                    }
                }
            }
        }
        Command::IconSvg { set, icon, out, size } => {
            let sets = glyphclub::icons::load_sets();
            let s = find_set(&sets, set)?;
            let wanted = icon.to_lowercase().replace(['-', '_'], " ");
            let found = s
                .icons
                .iter()
                .find(|i| i.name.to_lowercase() == wanted)
                .ok_or_else(|| anyhow!("no icon “{icon}” in {} (see `glyphclub-cli icons {}`)", s.name, s.slug))?;
            let data = std::fs::read(&s.font_path)?;
            let svg = glyphclub::icons::icon_svg(&data, found.codepoint, *size)
                .ok_or_else(|| anyhow!("“{icon}” has no outline"))?;
            std::fs::write(out, svg)?;
            done(json_out, &format!("Saved {} from {} to {}", found.name, s.name, out.display()));
        }
        Command::App { command, args } => {
            if command == "screenshot" {
                let path = args.first().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("glyphclub.png"));
                let reply = app_ui("state", &[])?;
                let pid = reply["state"]["pid"].as_u64().ok_or_else(|| anyhow!("the app didn't say its pid"))?;
                screenshot(pid, &path)?;
                done(json_out, &format!("Saved {}", path.display()));
                return Ok(());
            }
            let reply = app_ui(command, args)?;
            let state = &reply["state"];
            if json_out || command == "state" || command == "help" {
                println!("{}", serde_json::to_string_pretty(state)?);
            } else {
                println!(
                    "ok: {} view, {} fonts shown, selected: {}",
                    state["view"].as_str().unwrap_or("?"),
                    state["visibleCount"],
                    state["selected"]
                        .as_array()
                        .map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "))
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "nothing".into())
                );
            }
        }
        Command::ExportUserFonts { folder } => {
            let summary = glyphclub::font_export::export_user_fonts(folder)?;
            done(
                json_out,
                &format!(
                    "Exported {} to {} ({} already there, {} failed)",
                    summary.copied,
                    summary.dest.display(),
                    summary.skipped,
                    summary.failed
                ),
            );
        }
    }
    Ok(())
}

fn set_active(names: &[String], on: bool, temp: bool, json_out: bool) -> Result<()> {
    let mut catalog = open_catalog()?;
    let rows = catalog.list_fonts("")?;
    let active = catalog.active_ids()?;
    let (mut changed, mut not_found, mut failed) = (Vec::new(), Vec::new(), Vec::new());
    for name in names {
        let ids = resolve(&rows, name);
        if ids.is_empty() {
            not_found.push(name.clone());
            continue;
        }
        let mut ok = true;
        for id in ids {
            if active.contains(&id) == on {
                continue;
            }
            let result = match (on, temp) {
                (true, true) => catalog.activate_temporarily(id).map(|_| ()),
                _ => catalog.toggle_activation(id).map(|_| ()),
            };
            ok &= result.is_ok();
        }
        if ok { changed.push(name.clone()) } else { failed.push(name.clone()) }
    }
    tell_app_to_reload();
    let verb = if on { "Activated" } else { "Deactivated" };
    if json_out {
        println!("{}", json!({ "ok": failed.is_empty() && not_found.is_empty(), "changed": changed, "notFound": not_found, "failed": failed }));
    } else {
        for n in &changed {
            println!("{verb} {n}");
        }
        for n in &not_found {
            eprintln!("No font named “{n}”");
        }
        for n in &failed {
            eprintln!("Couldn't change {n}");
        }
    }
    if !not_found.is_empty() || !failed.is_empty() {
        bail!("{} of {} not changed", not_found.len() + failed.len(), names.len());
    }
    Ok(())
}

fn print_families(fams: &[Family], active: &HashSet<i64>, starred: &HashSet<i64>, json_out: bool) {
    if json_out {
        let list: Vec<Value> = fams
            .iter()
            .map(|f| {
                json!({
                    "family": f.name,
                    "libraryId": f.folder_id,
                    "styles": f.faces.iter().map(|r| json!({
                        "style": r.record.subfamily,
                        "postscriptName": r.record.postscript_name,
                        "path": r.record.path,
                        "active": active.contains(&r.id),
                    })).collect::<Vec<_>>(),
                    "active": f.faces.iter().any(|r| active.contains(&r.id)),
                    "starred": f.faces.iter().any(|r| starred.contains(&r.id)),
                })
            })
            .collect();
        println!("{}", json!({ "ok": true, "families": list }));
        return;
    }
    for f in fams {
        let on = f.faces.iter().any(|r| active.contains(&r.id));
        let star = f.faces.iter().any(|r| starred.contains(&r.id));
        let styles = f.faces.len();
        println!(
            "{} {}{}  ({} style{})",
            if on { "●" } else { "○" },
            f.name,
            if star { " ★" } else { "" },
            styles,
            if styles == 1 { "" } else { "s" }
        );
    }
    eprintln!("{} famil{}", fams.len(), if fams.len() == 1 { "y" } else { "ies" });
}

fn find_set<'a>(sets: &'a [glyphclub::icons::IconSet], which: &str) -> Result<&'a glyphclub::icons::IconSet> {
    let w = which.to_lowercase();
    sets.iter()
        .find(|s| s.slug == w || s.name.to_lowercase() == w)
        .ok_or_else(|| anyhow!("no icon set “{which}” (see `glyphclub-cli icons`)"))
}

fn done(json_out: bool, message: &str) {
    if json_out {
        println!("{}", json!({ "ok": true, "message": message }));
    } else {
        println!("{message}");
    }
}

/// A UI command for the running app; errors if it isn't running.
fn app_ui(command: &str, args: &[String]) -> Result<Value> {
    app_call(json!({ "method": "ui", "command": command, "args": args }))
        .map_err(|e| anyhow!("{e:#} (is GlyphClub running?)"))
}

/// Saves the app's main window (just that window, wherever it is) to
/// `path`, finding it by process id.
fn screenshot(pid: u64, path: &Path) -> Result<()> {
    let script = format!(
        "ObjC.import('CoreGraphics'); \
         var l = ObjC.castRefToObject($.CGWindowListCopyWindowInfo($.kCGWindowListOptionOnScreenOnly, 0)).js; \
         var best = null; \
         for (var i = 0; i < l.length; i++) {{ var w = l[i].js; \
           if (w.kCGWindowOwnerPID.js == {pid} && w.kCGWindowLayer.js == 0) {{ \
             var a = w.kCGWindowBounds.js.Width.js * w.kCGWindowBounds.js.Height.js; \
             if (!best || a > best.a) best = {{ id: w.kCGWindowNumber.js, a: a }}; }} }} \
         best ? best.id : ''"
    );
    let out = std::process::Command::new("osascript").args(["-l", "JavaScript", "-e", &script]).output()?;
    let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if id.is_empty() {
        bail!("couldn't find GlyphClub's window (is it hidden or minimized?)");
    }
    let status = std::process::Command::new("screencapture")
        .args(["-x", "-o", &format!("-l{id}")])
        .arg(path)
        .status()?;
    if !status.success() {
        bail!("screencapture failed");
    }
    Ok(())
}

/// Asks a running GlyphClub to reload its catalog. Quietly does nothing if
/// it isn't running.
fn tell_app_to_reload() {
    let _ = app_request("reload");
}

fn app_request(method: &str) -> Result<Value> {
    app_call(json!({ "method": method }))
}

/// One request over the app's local bridge (see `src/next/bridge.rs`).
fn app_call(mut request: Value) -> Result<Value> {
    let session = dirs::data_dir()
        .or_else(dirs::data_local_dir)
        .ok_or_else(|| anyhow!("no data folder"))?
        .join("glyphclub")
        .join("bridge.json");
    let raw = std::fs::read(&session)?;
    let session: Value = serde_json::from_slice(&raw)?;
    let port = session.get("port").and_then(Value::as_u64).ok_or_else(|| anyhow!("bad session"))?;
    let token = session.get("token").and_then(Value::as_str).ok_or_else(|| anyhow!("bad session"))?;
    let stream = TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port as u16)),
        Duration::from_millis(500),
    )?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    request["token"] = Value::String(token.to_string());
    let request = request.to_string() + "\n";
    (&stream).write_all(request.as_bytes())?;
    let mut reply = String::new();
    BufReader::new(&stream).read_line(&mut reply)?;
    let reply: Value = serde_json::from_str(&reply)?;
    if reply.get("ok").and_then(Value::as_bool) != Some(true) {
        bail!("{}", reply.get("error").and_then(Value::as_str).unwrap_or("the app said no"));
    }
    Ok(reply)
}
