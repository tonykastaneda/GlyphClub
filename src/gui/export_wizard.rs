//! File ▸ Export ▸ User Installed Fonts…: a few pages in one dialog.
//!
//! 1. Where to: a folder, or one of your libraries.
//! 2. Only when that folder already has different files by the same names:
//!    Overwrite (recommended), Let Me Choose, or Append -1, -2, -3.
//! 3. Let Me Choose: each conflict side by side, your copy against the
//!    folder's; picking one moves on to the next.
//! 4. Done, and: should GlyphClub manage them? Yes adds the folder as a
//!    library (when it isn't one), activates the exported fonts from it and
//!    uninstalls the originals from the Fonts folder
//!    (`GlyphClub::manage_exported`).
//!
//! The copying itself is `glyphclub::font_export`. Every page can also be
//! driven with `glyphclub-cli app export …` (see `drive`).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use glyphclub::font_export::{self, ExportSummary, FontFile, Plan, Resolution};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Disableable, Sizable, WindowExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::app::GlyphClub;
use crate::palette::Palette;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Policy {
    Overwrite,
    Choose,
    Append,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Step {
    Destination,
    Working,
    Conflicts,
    Choose(usize),
    Manage,
    Empty,
}

#[derive(Clone)]
struct Dest {
    path: PathBuf,
    name: SharedString,
    /// One of the libraries already (or inside one).
    library: bool,
}

pub struct ExportWizard {
    app: WeakEntity<GlyphClub>,
    libraries: Vec<(PathBuf, SharedString)>,
    /// A folder picked with Choose Folder…, listed above the libraries.
    picked: Option<Dest>,
    dest: Option<Dest>,
    step: Step,
    working: SharedString,
    error: Option<SharedString>,
    plan: Option<Arc<Plan>>,
    /// Indices into the plan's items.
    conflicts: Vec<usize>,
    policy: Policy,
    /// Per conflict (same order as `conflicts`): Overwrite (yours) or
    /// KeepTheirs (the folder's).
    picks: Vec<Option<Resolution>>,
    summary: Option<ExportSummary>,
}

pub fn open(app: &mut GlyphClub, window: &mut Window, cx: &mut Context<GlyphClub>) {
    let libraries = app.lib.folders.iter().map(|f| (PathBuf::from(&f.path), f.name.clone())).collect();
    let weak = cx.entity().downgrade();
    let wizard = cx.new(|_| ExportWizard {
        app: weak,
        libraries,
        picked: None,
        dest: None,
        step: Step::Destination,
        working: "".into(),
        error: None,
        plan: None,
        conflicts: Vec::new(),
        policy: Policy::Overwrite,
        picks: Vec::new(),
        summary: None,
    });
    app.export_wizard = Some(wizard.downgrade());
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .title("Export User Installed Fonts")
            .w(px(560.))
            .overlay_closable(false)
            .child(wizard.clone())
    });
}

impl ExportWizard {
    fn is_library(&self, path: &PathBuf) -> bool {
        self.libraries.iter().any(|(lib, _)| path.starts_with(lib))
    }

    fn set_folder(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let name: SharedString =
            path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string()).into();
        let dest = Dest { library: self.is_library(&path), path, name };
        self.picked = Some(dest.clone());
        self.dest = Some(dest);
        self.error = None;
        cx.notify();
    }

    fn set_library(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let Some((path, name)) = self.libraries.get(ix) {
            self.dest = Some(Dest { path: path.clone(), name: name.clone(), library: true });
            self.error = None;
            cx.notify();
        }
    }

    fn choose_folder(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else { return };
            let Some(path) = paths.into_iter().next() else { return };
            let _ = this.update(cx, |this, cx| this.set_folder(path, cx));
        })
        .detach();
    }

    fn next(&mut self, cx: &mut Context<Self>) {
        match self.step {
            Step::Destination => self.make_plan(cx),
            Step::Conflicts if self.policy == Policy::Choose => {
                self.step = Step::Choose(0);
                cx.notify();
            }
            Step::Conflicts => self.export(cx),
            Step::Choose(i) => {
                if self.picks.get(i).copied().flatten().is_some() {
                    self.advance_from(i, cx);
                }
            }
            _ => {}
        }
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        self.step = match self.step {
            Step::Conflicts => Step::Destination,
            Step::Choose(0) => Step::Conflicts,
            Step::Choose(i) => Step::Choose(i - 1),
            other => other,
        };
        cx.notify();
    }

    fn make_plan(&mut self, cx: &mut Context<Self>) {
        let Some(dest) = self.dest.clone() else { return };
        self.step = Step::Working;
        self.working = "Looking at your fonts…".into();
        self.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let path = dest.path.clone();
            let plan = cx.background_executor().spawn(async move { font_export::plan_user_export(&path) }).await;
            let _ = this.update(cx, |this, cx| {
                match plan {
                    Err(e) => {
                        this.error = Some(format!("{e:#}").into());
                        this.step = Step::Destination;
                    }
                    Ok(plan) if plan.items.is_empty() => this.step = Step::Empty,
                    Ok(plan) => {
                        this.conflicts = plan.conflicts();
                        this.picks = vec![None; this.conflicts.len()];
                        this.plan = Some(Arc::new(plan));
                        if this.conflicts.is_empty() {
                            this.export(cx);
                        } else {
                            this.step = Step::Conflicts;
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn pick(&mut self, i: usize, resolution: Resolution, cx: &mut Context<Self>) {
        if let Some(slot) = self.picks.get_mut(i) {
            *slot = Some(resolution);
        }
        self.advance_from(i, cx);
    }

    fn advance_from(&mut self, i: usize, cx: &mut Context<Self>) {
        if i + 1 < self.conflicts.len() {
            self.step = Step::Choose(i + 1);
            cx.notify();
        } else {
            self.export(cx);
        }
    }

    fn export(&mut self, cx: &mut Context<Self>) {
        let Some(plan) = self.plan.clone() else { return };
        self.step = Step::Working;
        self.working = format!("Exporting {} fonts…", plan.items.len()).into();
        cx.notify();
        let policy = self.policy;
        let picks: std::collections::HashMap<usize, Resolution> = self
            .conflicts
            .iter()
            .zip(&self.picks)
            .filter_map(|(&item, pick)| pick.map(|p| (item, p)))
            .collect();
        let library = self.dest.as_ref().is_some_and(|d| d.library);
        cx.spawn(async move |this, cx| {
            let summary = cx
                .background_executor()
                .spawn(async move {
                    font_export::run(&plan, |i| match policy {
                        Policy::Overwrite => Resolution::Overwrite,
                        Policy::Append => Resolution::Append,
                        Policy::Choose => picks.get(&i).copied().unwrap_or(Resolution::KeepTheirs),
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.summary = Some(summary);
                this.step = Step::Manage;
                // A library shows its new fonts right away.
                if library {
                    let _ = this.app.update(cx, |app, cx| app.rescan(cx));
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn finish(&mut self, manage: bool, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(summary), Some(dest)) = (self.summary.take(), self.dest.clone()) else {
            window.close_dialog(cx);
            return;
        };
        let _ = self.app.update(cx, |app, cx| {
            app.export_wizard = None;
            if manage {
                app.manage_exported(dest.path.clone(), dest.library, summary.exported.clone(), cx);
            } else {
                app.notify(summary_line(&summary, &dest.name));
                cx.notify();
            }
        });
        window.close_dialog(cx);
    }

    /// `glyphclub-cli app export …`: the same steps as the clicks.
    pub fn drive(&mut self, args: &[String], window: &mut Window, cx: &mut Context<Self>) -> Result<(), String> {
        let arg = |i: usize| args.get(i).map(|s| s.as_str()).unwrap_or("");
        match arg(0) {
            "folder" => self.set_folder(PathBuf::from(args[1..].join(" ")), cx),
            "library" => {
                let name = args[1..].join(" ").to_lowercase();
                let ix = self
                    .libraries
                    .iter()
                    .position(|(_, n)| n.to_lowercase() == name)
                    .ok_or_else(|| format!("no library “{name}”"))?;
                self.set_library(ix, cx);
            }
            "next" => self.next(cx),
            "back" => self.back(cx),
            "policy" => {
                self.policy = match arg(1) {
                    "overwrite" => Policy::Overwrite,
                    "choose" => Policy::Choose,
                    "append" => Policy::Append,
                    other => return Err(format!("policy: expected overwrite, choose or append, got “{other}”")),
                };
                cx.notify();
            }
            "pick" => {
                let Step::Choose(i) = self.step else { return Err("not on a conflict".into()) };
                let r = match arg(1) {
                    "mine" | "yours" => Resolution::Overwrite,
                    "theirs" | "folder" => Resolution::KeepTheirs,
                    other => return Err(format!("pick: expected mine or theirs, got “{other}”")),
                };
                self.pick(i, r, cx);
            }
            "manage" => self.finish(arg(1) == "yes", window, cx),
            "close" => {
                let _ = self.app.update(cx, |app, _| app.export_wizard = None);
                window.close_dialog(cx);
            }
            "" | "state" => {}
            other => return Err(format!("export: unknown step “{other}”")),
        }
        Ok(())
    }

    pub fn state(&self) -> serde_json::Value {
        serde_json::json!({
            "step": format!("{:?}", self.step),
            "dest": self.dest.as_ref().map(|d| d.path.display().to_string()),
            "policy": format!("{:?}", self.policy),
            "conflicts": self.conflicts.len(),
            "error": self.error.as_ref().map(|e| e.to_string()),
            "exported": self.summary.as_ref().map(|s| s.total()),
        })
    }
}

fn summary_line(s: &ExportSummary, dest: &str) -> String {
    let mut msg = format!("Exported {} font{} to {dest}", s.total(), if s.total() == 1 { "" } else { "s" });
    let mut parts = Vec::new();
    if s.overwritten > 0 {
        parts.push(format!("{} replaced", s.overwritten));
    }
    if s.renamed > 0 {
        parts.push(format!("{} renamed", s.renamed));
    }
    if s.kept > 0 {
        parts.push(format!("{} kept the folder's copy", s.kept));
    }
    if s.identical > 0 {
        parts.push(format!("{} already there", s.identical));
    }
    if s.failed > 0 {
        parts.push(format!("{} couldn't be copied", s.failed));
    }
    if !parts.is_empty() {
        msg.push_str(&format!(" ({})", parts.join(", ")));
    }
    msg + "."
}

fn when(t: Option<SystemTime>) -> String {
    t.map(|t| {
        chrono::DateTime::<chrono::Local>::from(t).format("%b %-d, %Y at %-I:%M %p").to_string()
    })
    .unwrap_or_else(|| "—".into())
}

fn size_label(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024. * 1024.))
    } else {
        format!("{} KB", (bytes + 1023) / 1024)
    }
}

/// A radio-style choice card.
fn option_card(
    id: impl Into<ElementId>,
    pal: &Palette,
    selected: bool,
    title: &str,
    body: impl IntoElement,
) -> Stateful<Div> {
    div()
        .id(id)
        .w_full()
        .p(px(12.))
        .rounded(px(8.))
        .border_1()
        .border_color(if selected { pal.select } else { pal.border })
        .when(selected, |d| d.bg(pal.select.opacity(0.08)))
        .hover(|s| s.bg(pal.hover))
        .cursor_pointer()
        .flex()
        .gap(px(10.))
        .child(
            div()
                .mt(px(2.))
                .size(px(14.))
                .flex_none()
                .rounded_full()
                .border_1()
                .border_color(if selected { pal.select } else { pal.text_dim })
                .flex()
                .items_center()
                .justify_center()
                .when(selected, |d| d.child(div().size(px(8.)).rounded_full().bg(pal.select))),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .min_w_0()
                .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child(title.to_string()))
                .child(body),
        )
}

fn dim(pal: &Palette, text: impl Into<SharedString>) -> Div {
    div().text_size(px(12.)).text_color(pal.text_dim).child(text.into())
}

impl Render for ExportWizard {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Palette::get(cx);
        let body = match self.step {
            Step::Destination => self.destination(&pal, cx).into_any_element(),
            Step::Working => div()
                .h(px(120.))
                .flex()
                .items_center()
                .justify_center()
                .child(dim(&pal, self.working.clone()))
                .into_any_element(),
            Step::Conflicts => self.conflicts_page(&pal, cx).into_any_element(),
            Step::Choose(i) => self.choose_page(i, &pal, cx).into_any_element(),
            Step::Manage => self.manage_page(&pal).into_any_element(),
            Step::Empty => div()
                .py(px(16.))
                .child(div().text_size(px(13.)).child("You don't have any fonts installed in your Fonts folder."))
                .child(dim(&pal, "Nothing to export.").mt(px(4.)))
                .into_any_element(),
        };

        // The footer's buttons, per page.
        let cancel = Button::new("export-cancel").label("Cancel").on_click(cx.listener(|this, _, window, cx| {
            let _ = this.app.update(cx, |app, _| app.export_wizard = None);
            window.close_dialog(cx);
        }));
        let back = Button::new("export-back").label("Back").on_click(cx.listener(|this, _, _, cx| this.back(cx)));
        let footer = div().flex().justify_end().gap(px(8.)).pt(px(16.));
        let footer = match self.step {
            Step::Destination => footer.child(cancel).child(
                Button::new("export-next")
                    .primary()
                    .label("Next")
                    .disabled(self.dest.is_none())
                    .on_click(cx.listener(|this, _, _, cx| this.next(cx))),
            ),
            Step::Working => footer,
            Step::Conflicts => footer.child(back).child(
                Button::new("export-next")
                    .primary()
                    .label(if self.policy == Policy::Choose { "Next" } else { "Export" })
                    .on_click(cx.listener(|this, _, _, cx| this.next(cx))),
            ),
            Step::Choose(i) => {
                let last = i + 1 == self.conflicts.len();
                footer.child(back).child(
                    Button::new("export-next")
                        .primary()
                        .label(if last { "Export" } else { "Next" })
                        .disabled(self.picks.get(i).copied().flatten().is_none())
                        .on_click(cx.listener(|this, _, _, cx| this.next(cx))),
                )
            }
            Step::Manage if self.summary.as_ref().is_some_and(|s| s.total() == 0) => footer.child(
                Button::new("export-done").primary().label("Close").on_click(cx.listener(|this, _, window, cx| {
                    this.finish(false, window, cx)
                })),
            ),
            Step::Manage => footer
                .child(
                    Button::new("export-no")
                        .label("Not Now")
                        .on_click(cx.listener(|this, _, window, cx| this.finish(false, window, cx))),
                )
                .child(
                    Button::new("export-yes")
                        .primary()
                        .label("Yes, Manage Them")
                        .on_click(cx.listener(|this, _, window, cx| this.finish(true, window, cx))),
                ),
            Step::Empty => footer.child(
                Button::new("export-done").primary().label("Close").on_click(cx.listener(|this, _, window, cx| {
                    let _ = this.app.update(cx, |app, _| app.export_wizard = None);
                    window.close_dialog(cx);
                })),
            ),
        };

        div().flex().flex_col().child(body).child(footer)
    }
}

impl ExportWizard {
    fn destination(&self, pal: &Palette, cx: &mut Context<Self>) -> Div {
        let row = |id: ElementId, icon, name: SharedString, detail: String, selected: bool| {
            div()
                .id(id)
                .h(px(44.))
                .px(px(10.))
                .rounded(px(6.))
                .border_1()
                .border_color(if selected { pal.select } else { gpui_kit::transparent_black() })
                .when(selected, |d| d.bg(pal.select.opacity(0.08)))
                .flex()
                .items_center()
                .gap(px(10.))
                .hover(|s| s.bg(pal.hover))
                .cursor_pointer()
                .child(gpui_kit::component::Icon::new(icon).size(px(18.)).text_color(pal.text_dim))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .child(div().text_size(px(13.)).child(name))
                        .child(div().text_size(px(11.)).text_color(pal.text_dim).truncate().child(detail)),
                )
        };
        let selected_path = self.dest.as_ref().map(|d| d.path.clone());
        let mut list = div().flex().flex_col().gap(px(2.));
        list = list.child(
            row(
                "export-choose-folder".into(),
                gpui_kit::assets::IconName::FolderOpen,
                "Choose Folder…".into(),
                "Any folder on this computer".into(),
                false,
            )
            .on_click(cx.listener(|this, _, _, cx| this.choose_folder(cx))),
        );
        if let Some(picked) = &self.picked {
            let p = picked.path.clone();
            list = list.child(
                row(
                    "export-picked".into(),
                    gpui_kit::assets::IconName::Folder,
                    picked.name.clone(),
                    picked.path.display().to_string(),
                    selected_path.as_ref() == Some(&picked.path),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.set_folder(p.clone(), cx))),
            );
        }
        if !self.libraries.is_empty() {
            list = list.child(
                div()
                    .pt(px(10.))
                    .pb(px(4.))
                    .px(px(10.))
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(pal.text_dim)
                    .child("YOUR LIBRARIES"),
            );
            for (k, (path, name)) in self.libraries.iter().enumerate() {
                let selected = selected_path.as_ref() == Some(path) && self.picked.as_ref().map(|p| &p.path) != Some(path);
                list = list.child(
                    row(
                        ("export-lib", k).into(),
                        gpui_kit::assets::IconName::Folder,
                        name.clone(),
                        path.display().to_string(),
                        selected,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.set_library(k, cx))),
                );
            }
        }
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(div().text_size(px(13.)).child(
                "Copies every font you installed in your system's Fonts folder into a folder or library you choose.",
            ))
            .child(
                div()
                    .id("export-dest-list")
                    .max_h(px(300.))
                    .overflow_y_scroll()
                    .child(list),
            )
            .when_some(self.error.clone(), |d, e| d.child(div().text_size(px(12.)).text_color(gpui_kit::red()).child(e)))
    }

    fn conflicts_page(&self, pal: &Palette, cx: &mut Context<Self>) -> Div {
        let n = self.conflicts.len();
        let dest = self.dest.as_ref().map(|d| d.name.clone()).unwrap_or_default();
        // The append examples use a real conflicting file's name.
        let example = self
            .plan
            .as_ref()
            .and_then(|p| self.conflicts.first().map(|&i| p.dest.join(&p.items[i].rel)))
            .unwrap_or_else(|| PathBuf::from("Font.ttf"));
        let examples = (1..=3)
            .map(|k| {
                font_export::appended_name(&example, k)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        let card = |policy: Policy, id: &'static str, title: &str, body: Div| {
            option_card(id, pal, self.policy == policy, title, body).on_click(cx.listener(move |this, _, _, cx| {
                this.policy = policy;
                cx.notify();
            }))
        };
        let mut chips = div().flex().flex_wrap().gap(px(6.));
        for name in examples {
            chips = chips.child(
                div()
                    .px(px(6.))
                    .py(px(2.))
                    .rounded(px(4.))
                    .bg(pal.chip)
                    .text_size(px(11.))
                    .font_family("Menlo")
                    .child(name),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(div().text_size(px(13.)).child(format!(
                "{n} of your fonts {} a file by the same name in {dest}. How should these be handled?",
                if n == 1 { "has" } else { "have" }
            )))
            .child(card(
                Policy::Overwrite,
                "export-overwrite",
                "Overwrite (Recommended)",
                dim(pal, "Your copies replace the folder's."),
            ))
            .child(card(
                Policy::Choose,
                "export-choose",
                "Let Me Choose",
                dim(pal, "Go through each one and pick your copy or the folder's."),
            ))
            .child(card(
                Policy::Append,
                "export-append",
                "Append File Name",
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(dim(pal, "Keep both: yours are saved with a number added."))
                    .child(chips),
            ))
    }

    fn choose_page(&self, i: usize, pal: &Palette, cx: &mut Context<Self>) -> Div {
        let Some(plan) = self.plan.as_ref() else { return div() };
        let item = &plan.items[self.conflicts[i]];
        let dest = self.dest.as_ref().map(|d| d.name.clone()).unwrap_or_default();
        let picked = self.picks.get(i).copied().flatten();
        let side = |id: &'static str, title: String, file: &FontFile, which: Resolution| {
            let fact = |label: &str, value: String| {
                div()
                    .flex()
                    .gap(px(6.))
                    .text_size(px(12.))
                    .child(div().w(px(62.)).flex_none().text_color(pal.text_dim).child(label.to_string()))
                    .child(div().min_w_0().truncate().child(value))
            };
            let selected = picked == Some(which);
            div()
                .id(id)
                .flex_1()
                .min_w_0()
                .p(px(12.))
                .rounded(px(8.))
                .border_1()
                .border_color(if selected { pal.select } else { pal.border })
                .when(selected, |d| d.bg(pal.select.opacity(0.08)))
                .hover(|s| s.bg(pal.hover))
                .cursor_pointer()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(pal.text_dim)
                        .child(title.to_uppercase()),
                )
                .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).truncate().child(file.full_name.clone()))
                .child(fact("File", file.file_name.clone()))
                .child(fact("Created", when(file.created)))
                .child(fact("Modified", when(file.modified)))
                .child(fact("Size", size_label(file.size)))
                .child(
                    div().pt(px(4.)).child(
                        Button::new((id, 1usize))
                            .label(if selected { "Selected" } else { "Use This One" })
                            .when(selected, |b| b.primary())
                            .xsmall()
                            .on_click(cx.listener(move |this, _, _, cx| this.pick(i, which, cx))),
                    ),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.pick(i, which, cx)))
        };
        let theirs = item.theirs.clone().unwrap_or_else(|| item.mine.clone());
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(div().text_size(px(13.)).truncate().child(item.rel.display().to_string()))
                    .child(dim(pal, format!("{} of {}", i + 1, self.conflicts.len()))),
            )
            .child(
                div()
                    .flex()
                    .gap(px(10.))
                    .child(side("export-mine", "Your copy".into(), &item.mine, Resolution::Overwrite))
                    .child(side("export-theirs", format!("Copy in {dest}"), &theirs, Resolution::KeepTheirs)),
            )
    }

    fn manage_page(&self, pal: &Palette) -> Div {
        let Some(s) = self.summary.as_ref() else { return div() };
        let dest = self.dest.as_ref().map(|d| d.name.to_string()).unwrap_or_default();
        if s.total() == 0 {
            return div()
                .py(px(8.))
                .child(div().text_size(px(13.)).child(format!("None of your fonts could be copied to {dest}.")))
                .child(dim(pal, "Check that you can write to that folder, then try again.").mt(px(4.)));
        }
        let library = self.dest.as_ref().is_some_and(|d| d.library);
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        gpui_kit::component::Icon::new(gpui_kit::assets::IconName::CircleCheck)
                            .size(px(18.))
                            .text_color(pal.select),
                    )
                    .child(div().text_size(px(13.)).child(summary_line(s, &dest))),
            )
            .child(
                div()
                    .p(px(14.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(pal.select.opacity(0.6))
                    .bg(pal.select.opacity(0.06))
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Would you like GlyphClub to manage all these fonts?"),
                    )
                    .child(dim(
                        pal,
                        format!(
                            "This uninstalls them from your system's Fonts folder so GlyphClub can manage them: {}they're activated from {dest} instead, so your apps keep working.",
                            if library { "" } else { "the folder becomes a library, and " }
                        ),
                    )),
            )
    }
}
