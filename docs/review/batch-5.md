# Batch 5: the interface and the apps

The crates `desk-ui`, `desk-writer`, `desk-tables`, `desk-slides`,
`desk-convert` and `desk-lang`, and the Flatpak manifest. Reviewed on 24
November 2026 from commit `12b1973`. `/code-review` ran at level high over
the crates, looking for leaks by reference cycles, signals connected
twice, work that blocks the window, undo and data lost on saving.
`/security-review` (D-029) looked at what a document, the clipboard, a
dropped file or a link can make the interface do.

## Findings

| # | Class | Where | What failed | What was done |
| --- | --- | --- | --- | --- |
| 1 | A | `desk-ui/src/editing.rs`, `links.rs` | *Security.* A plain click on a link of a document launched it, whatever it pointed to: a file of the user, a shared folder (`smb:`), another app's scheme, or an email with a file attached | Decided by the owner (D-030). A link to a bookmark or another document opens at once. A link to the web follows a preference: open at once, ask first, or never open. The first such click asks which and keeps the answer, and the preferences can change it. Any other link shows its address and asks. The address shows while the pointer is over a link. Test |
| 2 | A | `desk-ui/src/window.rs` | Recovering a crash draft removed the draft before anything else held the document; a second crash before an edit lost it | The draft stays the draft of the window until the document is saved or given up |
| 3 | A | `desk-ui/src/recovery.rs` | *Security.* Drafts were named by the process ID, which in Flatpak is the same at each run: the drafts of a crash looked like those of the run after it and were never offered back | Named by a random token of the run; test |
| 4 | A | `desk-ui/src/window.rs` | Autosave wrote over the file even when the copy of it as opened could not be kept | The file is not written over; the document waits for Save, and an alert says why |
| 5 | A | `desk-ui/src/window.rs` | Revert within two seconds of an edit, before the first autosave, reloaded the file and lost the edit, with no version of it | The changes are saved first, so they are a version before the revert |
| 6 | A | `desk-ui/src/window.rs`, `versions.rs` | *Security.* Restore and Revert read the version and wrote it again through Desk, which leaves out what Desk does not read; the version as opened was thinned a day later | The bytes of the version take the place of the file; the newest version as opened is not thinned; test |
| 7 | A | `desk-ui/src/window.rs` | *Security.* Save As added the extension after the dialog: "Report" saved over an existing Report.odt without asking | Asks before replacing it; test |
| 8 | A | `desk-ui/src/tabs.rs` | A tab dragged to another window closed there without asking to save, and its shortcuts did nothing | The window it goes to takes it over; test |
| 9 | A | `desk-ui/src/lib.rs` | *Security.* A file was written through a temporary file of a name anyone could guess, which a link in a shared folder could send elsewhere, and without syncing it | A new file of a random name, made only if absent, synced before it takes the place of the old one; test |
| 10 | B | `desk-ui/src/window.rs`, `fields.rs` | Each autosave could add "Update Table of Contents" to the undo history and clear the redo | Tables of contents are updated in the copy saved; test |
| 11 | B | `desk-ui/src/editing.rs`, `view.rs` | Each document shown again in a view (Revert, Restore, recovery) left the controllers, input method, width callback and blinking of the one before | They go with their editor; test |
| 12 | B | `desk-ui/src/compact.rs` | Rows of the panes that held handlers held themselves, and were never freed | Held weakly; test |
| 13 | C | `desk-ui/src/tables/window.rs` | Desk Tables kept a whole copy of the workbook for each change, without end, copied it twice a change, and kept changes that changed nothing | At most 100 steps, one copy a change, none for no change; test |
| 14 | A | `desk-lang/src/lib.rs` | *Security.* A file naming thousands of language tags loaded the same dictionary again for each, on the thread of the window | Dictionaries are found by what of the tag picks them and loaded once by their file; test |
| 15 | A | `desk-ui/src/editing.rs`, `bullet_pictures.rs` | *Security.* What the clipboard offered, and a picture dropped on the document, were read whole whatever their size | At most 256 MB, read without blocking; pictures pasted larger than 8192 by 8192 pixels are refused; test |
| 16 | B | `desk-model/src/format.rs` | *Security.* A paragraph could have thousands of tab stops, and the Tabs pane built rows for each | At most 100; test |
| 17 | D | `desk-ui/src/versions.rs` | A document renamed to the name of a deleted file showed that file's versions | They are removed; test |
| 18 | B | `desk-ui/src/editing.rs`, `window.rs`, `tables/window.rs` | *Review of the pull request.* Keys and pointer controllers stayed on the window a tab left when it was dragged to another, and a window left with no tab stayed open | They move with the tab; the empty window closes; test |
| 19 | A | `desk-ui/src/lib.rs` | *Review of the pull request.* A document with a name near the longest a file can have could not be saved, as the temporary file beside it had a longer name | The temporary name is cut to fit; test |
| 20 | A | `desk-ui/src/editing.rs` | *Security, review of the pull request.* Plain text pasted from another app was read whole, and a picture was decoded before its size was checked | Both read with the bound; a picture is refused by its header; test |
| 21 | B | `desk-ui/src/fields.rs` | *Review of the pull request.* A table of contents that could not be updated kept the fields of the saved copy from refreshing | Each updated on its own |
| 22 | B | `desk-ui/src/window.rs` | *Review of the pull request.* A document recovered twice was named "(recovered) (recovered)" | The draft keeps the name it had |
| 23 | B | `desk-ui/src/preferences.rs` | *Review of the pull request.* Before links to the web were chosen, Preferences showed Ask First, which then could not be chosen | The row shows that nothing is chosen yet; test |
| 24 | B | `desk-ui/src/tab_stops.rs`, `ruler.rs` | *Review of the pull request.* A stop added past the hundredth was left out without a word | The addition is refused with a beep; test |
| 25 | D | `desk-ui/src/versions.rs` | *Review of the pull request.* The versions set aside by finding 17 piled up with nothing to show them | Removed; test |
| 26 | B | `desk-ui/src/bullet_pictures.rs` | *Review of the pull request.* A picture or movie whose size its place does not give was refused | Read up to the bound; test |
| 27 | A | `desk-ui/src/links.rs` | *Security, review of the pull request.* A link could hide or turn around part of its address in the tooltip and the dialog that asks before opening it | Such characters are shown as a URL writes them, and a long address is cut short; test |
| 28 | A | `desk-ui/src/editing.rs` | *Security, second review of the pull request.* Some decoders (TIFF, GIF) make every pixel of a pasted picture before its size can be checked | Only PNG and JPEG are pasted, kept as they are, sized by their header and checked away from the thread of the window; test |
| 29 | A | `desk-ui/src/outline_view.rs` | *Security, second review of the pull request.* Paste in the outline read the text whole | Read with the bound |
| 30 | A | `desk-ui/src/links.rs` | *Security, second review of the pull request.* A long user name before the host pushed the host out of the address shown, and some characters not seen were not escaped | The host always shows, in DNS letters; an address with a user name always asks; every format character is escaped; test |
| 31 | B | `desk-ui/src/versions.rs` | *Second review of the pull request.* The versions of a file gone were removed before those of the document were moved | Removed only once the move succeeds, put back otherwise |
| 32 | B | `desk-ui/src/tabs.rs`, `window.rs`, `tables/window.rs` | *Second review of the pull request.* After a tab moved, full screen, dialogs, Escape, Shortcuts, the context menu and the title followed the window it had left | They follow the window that holds the tab; test |
| 33 | B | `desk-ui/src/window.rs` | *Second review of the pull request.* A recovered document renamed went back to its old name at the next recovery | The draft keeps the name it has now; test |
| 34 | B | `desk-ui/src/editing.rs` | *Second review of the pull request.* Text offered in upper case types, as `STRING`, or in another character set was not pasted | Types matched in any case, read in their set; test |

Class A: data lost, a panic, a hang or memory exhausted. B: wrong
behaviour. C: performance. D: clean-up. See [README.md](README.md).

Six findings moved to the backlog:
- B-058: autosave writes on the thread of the window, to phase W5.
- B-059: the Thumbnails pane and the Outline view build a row for
  everything, to phase W6.
- B-060: the Flatpak build downloads its crates and builds Poppler's
  unmaintained JPEG 2000 decoder, to batch 6 of this review.
- B-061: a file opened with content Desk does not read is saved over on
  its own. The original stays as the version kept when it opened. Not
  saving over it changes D-001, so it is for the owner. Phase W5.
- B-062: the CI runs the tests that need GTK without a display, so they
  are skipped there, to batch 6 of this review.
- B-063: language tags that name no dictionary each scan the dictionary
  folders again, to batch 6 of this review.

## Security (D-029)

Findings 1, 3, 6, 7, 9, 14, 15, 16, 20, 27, 28, 29 and 30 came from the security review.

These paths were checked and found safe:
- The crates run no command and no shell.
- The look-up of a word builds fixed `https` addresses with the text
  escaped, and runs only when asked.
- Links between documents open only `.odt` and `.pages` files, in Desk.
- Renaming refuses `/` and `..`.
- Versions, recovery drafts, the media cache and the preferences are
  written under the data folders of Desk, with no part of their path from
  a document.
- Movies and sounds are decoded only when inserted or played.
- The Flatpak sandbox has no access to files, the network or other
  services on the session bus.
- Logs hold error messages, not the text of documents.
