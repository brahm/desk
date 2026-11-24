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
| 17 | D | `desk-ui/src/versions.rs` | A document renamed to the name of a deleted file showed that file's versions | They are set aside; test |

Class A: data lost, a panic, a hang or memory exhausted. B: wrong
behaviour. C: performance. D: clean-up. See [README.md](README.md).

Four findings moved to the backlog:
- B-058: autosave writes on the thread of the window, to phase W5.
- B-059: the Thumbnails pane and the Outline view build a row for
  everything, to phase W6.
- B-060: the Flatpak build downloads its crates and builds Poppler's
  unmaintained JPEG 2000 decoder, to batch 6 of this review.
- B-061: a file opened with content Desk does not read is saved over on
  its own. The original stays as the version kept when it opened. Not
  saving over it changes D-001, so it is for the owner. Phase W5.

## Security (D-029)

Findings 1, 3, 6, 7, 9, 14, 15 and 16 came from the security review.

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
