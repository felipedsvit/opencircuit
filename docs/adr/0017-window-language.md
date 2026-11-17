# ADR-0017: The language of the window

Status: Accepted (2026-11-17). Tracking issues: #103 (Brazilian Portuguese),
#107 (M8).

## Context

The people who draw industrial control circuits in Brazil read their drawings,
their standards and their manuals in Portuguese; the window said everything in
English. The window is already used and known (AGENTS.md, "Conservative
interface"), so the translation must not change it: with the default language
the window is what it was.

## Decision

- **English is the source and the default.** The texts of the program are the keys
  of a catalog; a text with no translation shows as it is written, so a missing
  entry is a visible English text and never a blank or a crash. `DesktopApp::default()`
  is English whatever the environment says, so every window test runs in English
  and passes unchanged.
- **The first translation is Brazilian Portuguese** (`Language::PortugueseBrazil`,
  `apps/desktop/src/i18n/pt_br.rs`): a table sorted by the English text, searched
  with a binary search. A test keeps it sorted, without repeats, without empty
  entries, with the same `{0}`, `{1}` placeholders as the key, and checks both
  ways against the sources: every text written with `tr`, `trf` or `trn` has an
  entry and every entry is used.
- **Texts with numbers or names** are templates with numbered placeholders
  (`trf("Undo: {0}", &[&label])`), so a language can put them in another order.
  Counts use `trn(one, other, n)`; the singular is `n == 1` in English and
  `n <= 1` in Brazilian Portuguese.
- **The language is a state of the window.** It is chosen from the environment
  when the program starts (`Language::from_environment`: the first of `LC_ALL`,
  `LC_MESSAGES` and `LANG` that is set and not empty decides, as in POSIX; a
  language the window lacks gives English) and from "Language" in the Help menu
  while it runs. It is read in `main`, not in the window, and it is not stored,
  because the application has no settings storage. The browser build starts in
  English and uses the Help menu.
- **The window sets the language at the start of every frame** into a
  thread-local that `tr` reads, so that two windows on two threads (the tests)
  never see each other's language, and no text needs the window passed to it.
- **What is translated**: the menus and their items, the toolbar and its hints, the
  status bar, the palette (tabs, search, headings), the inspector (sections and
  fields), the problems list and its buttons, the dialogs (the unsaved changes
  window, the file filters of the file dialogs), the Help windows and the keyboard
  list. The accessible labels follow the visible texts, so a window in Portuguese
  is driven by Portuguese labels.
- **What is not translated, by design**: diagnostic codes and messages, the errors
  of the core, the command line (its words, its answers and its label), every file
  format and JSON field, the names of parts, parameters and terminals (they are
  data of the catalog and of the project), the group titles of the project tree
  (they come from the core), and the output of the command-line programs
  ("diagnostics are English", AGENTS.md).

## Adding a language

Add a variant to `Language`, a catalog file next to `pt_br.rs` with the same
shape, the name of the language in itself to `Language::name`, its locale names to
`Language::from_locale`, and its entry to `Language::translate`. The same tests
then check the new catalog against the sources.

## Not decided here

Other languages, right-to-left scripts, a stored setting, translating the command
line, and translated documentation.
