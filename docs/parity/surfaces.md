# Surfaces and previews: parity inventory (T3 Code v0.1.0 web client vs Pluto egui port)

Scope: right panel layout and surface switching, Diff, Files (tree, editing, media
previews), Browser/preview, Device, Terminal. Upstream read from
the v0.1.0 client sources (extracted from the package's source maps) (+ `packages/{contracts,client-runtime,shared}`);
port read from the repository (branch `egui-client`).
Pluto-only additions (Design surface, design sessions, html_render pages) are ignored
except where they offer reusable machinery.

Sizes: S = under a day / a few hundred lines, M = a few days, L = a subsystem.

## 1. Table

| Feature | Upstream file(s) | Port status | What is missing | Port location | Size |
|---|---|---|---|---|---|
| **Layout / surface switching** | | | | | |
| "Open a surface" launcher + "+" menu, letter shortcuts, arrows/Enter | `RightPanelTabs.tsx` (`SurfaceLauncher`, `SURFACE_UNAVAILABLE_HINTS`) | Partial | Lists **Agents (A)**, which v0.1.0 removed (see §3); Device is shown *available* (`_ => true`) but opens a "not implemented" status; Files/Terminal availability is not tied to an open project; Browser "profiles" submenu; Device row description ("Watch an iOS Simulator or Android Emulator.") | `src/ui/surfaces.rs` | S |
| Tab bar: one tab per surface, hover close glyph | `RightPanelTabs.tsx` | Partial | Rename for device, Mute/Unmute for browser tabs; drag-to-reorder; overflow arrows; page favicon and audio button on browser tabs (the menu with Copy path / Close / Close others / Close to the right / Close all and middle-click close are ported) | `src/ui/surfaces.rs` `panel_tab_bar`, `panel_tab_menu` | M |
| Panel state per thread, persisted | `rightPanelStore.ts` (`t3code:right-panel-state:v2`, v14, `maximized`) | Done | Browser tabs are not restored (their pages live in the run's browser) | `src/app.rs` (`panels_to_json`, `ui-state.json` `panels`) | — |
| Reopen closed tab/view (Mod+Shift+T) | `closedViewStore.ts`, `reopenClosedView.ts`, `view.reopenClosed` | Done | — | `src/ui/surfaces.rs` | — |
| `rightPanel.new` (Mod+T while panel open) | `packages/shared/src/keybindings.ts` | Done | — | `src/ui/shortcuts.rs` | — |
| Panel resize, min chat width 360, maximize/restore | `PreviewPanelShell.tsx`, `RightPanelResizeHandle.tsx`, `RightPanelMaximizeControl` | Done | — | `src/app.rs`, `src/ui/diff_panel.rs` (`panel_controls`, `resize_handle`) | — |
| Narrow window: panel as right sheet (≤980px, popover presentation) | `rightPanelLayout.ts`, `RightPanelSheet.tsx`, `toggleThreadPanel` | Missing | Port only clamps the inline panel; no sheet presentation | `src/app.rs` layout | M |
| Proactive panels (diff opens on Changes after a completed turn; linked PR panel) | `ChatView.tsx` ~5670-5780, `rightPanelStore.openProactive` | Done | The user-choice check compares the panel at turn start and end | `src/ui/proactive_panels.rs` | — |
| Back/forward navigation (Mod+[ / Mod+]) | `AppSidebarLayout.tsx` `NavigationHistoryShortcuts` | Missing | Bindings listed, not dispatched; no route history stack | `src/ui/shortcuts.rs` | M |
| Resizable, persisted sidebar width | `AppSidebarLayout.tsx`, `threadSidebarWidth.ts` | Done | (Code labels it a Pluto addition; it is upstream behaviour) | `src/ui/sidebar.rs` `sidebar_width` | — |
| Pull request / Linked pull requests surfaces | `components/pullRequest/*`, `RightPanelTabs.tsx` | Missing | Always disabled in the launcher (covered by the pull-request inventory) | `src/ui/surfaces.rs` | L |
| **Diff panel** | | | | | |
| Scope picker | `DiffPanel.tsx` 703-790, `diffPanelStore.ts` | Done | — | `src/ui/diff_panel.rs` | — |
| Unified/split, word-alt emphasis, expand unchanged context, base-ref picker (Branch/Remote, Automatic, search), HEAD → base label, file tree, collapse all, wrap, whitespace, refresh, copy path, empty/error notes | `DiffPanel.tsx`, `DiffPanelShell.tsx`, `DiffFileTree.tsx`, `diffFileTree.logic.ts`, `StyledDiffCodeView.tsx`, `DiffFilePathCopyButton.tsx` | Done | Minor: file-tree open state not persisted (`t3code.diffFileTreeOpen`) | `src/ui/diff_panel.rs` | — |
| Highlighting off the UI thread | `DiffWorkerPoolProvider.tsx` (Shiki worker pool) | Done (native) | syntect on a worker thread | `src/highlight.rs` | — |
| Filename opens the file; header right-click file menu | `diffFileActions.ts` `openDiffFilePrimaryAction`, `fileContextMenu.ts`, `DiffPanel.tsx` 1100-1150 | Done | The repository root is taken to be the workspace (worktrees inside a repository resolve from the workspace) | `src/ui/diff_panel.rs` `diff_file_header`, `src/ui/file_menu.rs` | — |
| Review comments on diff lines | `diffs/AnnotatableCodeView.tsx`, `diffs/DiffCommentAnnotation.tsx`, `diffs/commentSubmitShortcut.ts`, `reviewCommentContext.ts` | Done | Only the patch's lines can be commented (expanded context cannot); comments live in memory with the composer draft | `src/ui/diff_review.rs` | — |
| Large diffs: per-file lazy patches, truncated files | `diffs/useReviewFilePatches.ts`, `diffs/DiffFileStatus.tsx`, `diffs/DiffFileLoadingBoundary.tsx` | Done | A file's patch loads when it opens (upstream also loads the first four ahead); the notes are rows, not header icons | `src/ui/diff_panel.rs` `lazy_file_body` | — |
| Refresh after workspace mutations | `hooks/useWorkspaceMutationRefresh.ts` | Done | — | `src/ui/diff_panel.rs`, `src/ui/files.rs` (`workspace_mutated`) | — |
| Find in diff | Pierre CodeView search, `onRevealSearchMatch` (unfolds the collapsed file) | Missing | No find in the code view | — | M |
| **Files surface** | | | | | |
| Tree (`projects.listEntries`), search, refresh, expand/collapse all, ignored dimmed, explorer toggle | `FileBrowserPanel.tsx`, `useDirectoryEntries.ts`, `fileTreeExpansion.ts`, `projectFilesQueryState.ts` | Done | Keyboard navigation of the tree | `src/ui/files.rs` | — |
| Tree context menu | `FileBrowserPanel.tsx` 170-235, `fileContextMenu.ts` | Done | — | `src/ui/files.rs` `file_tree_menu` | — |
| Drag a tree entry into the composer | `fileTreeDragMention.ts` | Missing | — | — | S |
| Breadcrumbs | `FileBreadcrumbs.tsx` | Partial | Port draws static text; upstream crumbs are buttons ("Browse <folder>") opening a folder listing ("This folder is empty.") and scroll the current crumb into view | `src/ui/files.rs` 326-385 | S |
| Read-only source view: highlighting, line numbers, wrap, reveal line, 1 MB notice, binary notice, "Open in" picker, file tabs replacing the bare Files tab | `FilePreviewPanel.tsx`, `ReadOnlySourcePreview.tsx`, `fileLineReveal.ts`, `fileSurfaceChrome.tsx`, `rightPanelStore.openFile` | Done | — | `src/ui/files.rs` | — |
| **File editing and saving** | `FilePreviewPanel.tsx` `EditableFileSurface` (@pierre/diffs `Editor`), `fileSaveCoordinator.ts`, `useFileSaveCoordinator.ts`, `fileEditorDismissal.ts`, `fileContentRevision.ts` | Done | A click in the read-only view starts the editor (upstream's view is always an editor); truncated files stay read-only; no write-scope message | `src/ui/file_editor.rs` | — |
| File line comments | `fileCommentAnnotations.ts` | Missing | Comment ranges in a file → review_comment context | — | M |
| Re-read open file / tree on workspace mutation | `useWorkspaceMutationRefresh` in `FilePreviewPanel.tsx` | Done | — | `src/ui/files.rs` | — |
| Markdown rendered/source toggle | `FileMarkdownPreview.tsx`, `filePreviewMode.ts` | Done | Task markers are the ☐/☑ glyphs, not inputs | `src/ui/file_previews.rs` | — |
| CSV/TSV table | `DelimitedTablePreview.tsx` | Done | — | `src/ui/file_previews.rs` | — |
| Image preview + media menu | `FilePreviewPanel.tsx` `WorkspaceImagePreview` | Done (AVIF partial) | `.avif` is routed to the image preview but the `image` crate is built without AVIF, so it fails with "Unable to load workspace image." | `src/ui/file_previews.rs` 23-35, `Cargo.toml` | S |
| HTML rendered in place ("Show rendered page" / "Show HTML source") | `FilePreviewPanel.tsx` `WorkspaceBrowserPreview`, `BrowserDocumentFrame.tsx` | Missing | Port only offers "Open file in preview browser" (new Browser tab) | `src/ui/files.rs` 418-425 | M |
| PDF in place | `BrowserDocumentFrame.tsx` (`pdf`) | Missing | Same as HTML; only the Browser-tab hand-off | `src/ui/files.rs` | M |
| Video preview | `FilePreviewPanel.tsx` `WorkspaceVideoPreview` | Done | Media menu | `src/ui/file_previews.rs` `workspace_video` | — |
| Audio preview | `AudioPreview.tsx`, `WorkspaceAudioPreview` | Missing | `<audio>` player, "Unable to load audio." | — | M |
| Attachment surfaces (chat attachments opened in the panel) | `AttachmentFilePreview.tsx`, `rightPanelStore.openAttachment` | Missing | Copy contents/preview, Save file, Remove from draft, word wrap, media modes (blocked on file attachments, not ported) | — | M |
| Go to file | `ProjectFilePicker.tsx`, `ProjectFilePicker.logic.ts` | Done | Lives in the palette with upstream labels | `src/ui/palette.rs` | — |
| **Browser / preview** | | | | | |
| Chrome row: back, forward, refresh/stop, address field (`normalizePreviewUrl`), "Open in system browser", loading bar, tab title | `PreviewChromeRow.tsx`, `PreviewView.tsx` | Done | Annotate, capture/record buttons, more menu (rows below) | `src/ui/preview.rs` | — |
| Empty state: "Local servers" | `PreviewEmptyState.tsx`, `PreviewLocalServerCard.tsx`, `useDiscoveredLocalServers.ts`, `openDiscoveredPort.ts` | Done | — | `src/ui/preview.rs`, `src/app.rs` | — |
| Empty state: "Recently used" | `PreviewRecentUrlCard.tsx`, `browserHistoryStore.ts` | Missing | Recent URLs per thread with remove | — | S |
| **Server-managed preview sessions** | `packages/client-runtime/src/state/preview.ts` (`preview.open/list/navigate/resize/adjust/refresh/close/reportStatus/reportProfiles/clearProfile`, `subscribePreviewEvents`), `previewStateStore.ts`, `usePreviewSession.ts`, `openPreviewSession.ts`, `closePreviewSession.ts` | Missing | The port never calls a `preview.*` RPC. Tabs agents open (`preview_open` + `reveal`) never appear, agents cannot see or drive the user's tabs, nothing is reported back (`reportStatus`), tabs do not survive a restart (`preview.list`) | `src/preview.rs` (local only) | L |
| Server-browser tabs (stream, control, dialogs, uploads, downloads) | `browser/ServerBrowserSurface.tsx`, `client-runtime/src/preview/serverBrowserStream.ts` (`/api/preview-stream` WebSocket, JPEG frames, JSON input, `takeControl`, `dialog`) | Missing | "You have control / Agent has control / Watching" status, Take control, dialog response, file chooser upload, downloads | — | L (shares the item above) |
| Agent cursor | `AgentBrowserCursor.tsx`, `agentBrowserCursorLogic.ts`, `browserPointerStore.ts` | Missing | Depends on server sessions | — | S |
| Floating mini player | `ThreadPreviewMiniPlayer.tsx`, `previewMiniPlayerLayout.ts`, `previewMiniPlayerStore.ts`, `showPreviewPopup.ts` | Missing | Floating preview over the chat, "Open in right panel", "Close floating preview", "Auto-show floating preview" | — | M |
| More menu | `PreviewMoreMenu.tsx` | Missing | Hard reload, Open DevTools, separate preview window (PiP, `serverPictureInPicture.ts`), Show/Hide device toolbar, Appearance (System/Light/Dark), Zoom row, Profile, Clear cookies, Clear cache | — | M |
| Zoom + indicator + shortcuts | `ZoomIndicator.tsx`, `preview.zoomIn/zoomOut/resetZoom` | Missing | — | — | S |
| Preview shortcuts | `preview.toggle`, `preview.refresh`, `preview.focusUrl` | Missing | Listed in defaults, not dispatched | `src/ui/shortcuts.rs` | S |
| Device toolbar / responsive viewport | `browser/BrowserDeviceToolbar.tsx`, `browserViewportLayout.ts`, `BrowserViewportResizeHandles.tsx`, `useBrowserViewportResize.ts`, `browserViewportActions.ts`, `browserDeviceToolbarState.ts` | Missing | Presets, width/height fields, rotate, edge/corner resize handles, default viewport setting | — | M |
| Annotations | `PreviewView.tsx` (annotation mode), `PreviewChromeRow.tsx` ("Annotate preview"), `annotationTheme.ts` | Missing | Element pick + cropped screenshot sent as a preview-annotation context | — | M |
| Screenshot capture and recording | `PreviewChromeRow.tsx` ("Capture screenshot" / "Stop recording"), `browser/browserRecording.ts`, `recordingCompositor.ts` | Missing | Screenshot to composer; recording with key/mouse press overlays and fps setting | — | S (screenshot) / L (recording) |
| Browser profiles | `BrowserProfileReporter.tsx`, `browserDefaults.ts`, `preview.reportProfiles` | Missing | Port uses a throwaway profile per run (logins lost) | `src/preview.rs` | M |
| JS dialogs | `ServerBrowserSurface.tsx` ("Browser dialog", "Dialog response") | Missing | No `Page.javascriptDialogOpening` handling: a page calling `alert()` blocks the tab | `src/preview.rs` | S |
| File chooser and downloads | `ServerBrowserSurface.tsx` ("Choose files for the page"), `PreviewStreamDownload` | Missing | `<input type=file>` does nothing; downloads go nowhere | `src/preview.rs` | S/M |
| Load-failure page | `PreviewUnreachable.tsx`, `errorCodeMessages.ts` | Partial | Port shows a generic centered note ("Could not open the page"); upstream shows the error code message, the checklist ("Checking your connection", "Confirming the dev server is running", "Checking the proxy and the firewall") and retry; `PreviewFileNotShown` | `src/ui/preview.rs` 199 | S |
| Popups / `target=_blank` from a Browser tab | `showPreviewPopup.ts`, `ServerBrowserSurface.tsx` `onPopup` | Missing | Port only intercepts popups of html_render tabs; a Browser tab's new window appears to be dropped | `src/preview.rs` 1110-1147 | S |
| Link routing ("Open links in" app vs system; Ctrl-click escape) | `browserLinkTarget.ts`, `useOpenLink.ts`, `openTerminalLinkInPreview.ts` | Missing | Chat and terminal links always go to the system browser; the setting row is marked "Only available in the desktop app" | `src/ui/settings.rs` 1530-1600 | S |
| Page favicons, per-tab mute | `browserFaviconStore.ts`, `PreviewFaviconIcon.tsx`, `RightPanelTabs.tsx` mute | Missing | Browser runs with `--mute-audio` | `src/preview.rs` 435 | S |
| Crash recovery | `webviewCrashRecovery.ts` | Partial | "The page crashed" note, no automatic reload | `src/preview.rs` 1167 | S |
| Cursor shapes, page clipboard | `HostedBrowserWebview.tsx` | Missing | (README lists both) | — | S |
| Open workspace HTML/PDF in a Browser tab | `browser/openFileInPreview.ts` | Done | — | `src/ui/file_previews.rs` 469-510 | — |
| **Device** | | | | | |
| Device surface | `components/device/*` (`DevicePanel`, `DeviceSetup`, `DeviceStreamView`, `DeviceControlsRail`, `DeviceToolsPanel`, `DevicePhoneViewport`, `DeviceDuoViewport`, `DeviceAndroidFoldControls`, `deviceHubApi.ts`, GLB models), `client-runtime/src/device/*`, `device.*` RPCs, `rightPanelStore.openDevice/renameDevice` | Missing | Everything: device list per host, setup ("Enable the device hub"), stream view with touch/keyboard, controls rail (rotate, fold, appearance, text size, location, Liquid Glass…), tools/events panel, agent control toggle, rename. Only the Settings rows and `subscribeDeviceState` exist | `src/ui/surfaces.rs` (stub), `src/ui/settings.rs` | L |
| **Terminal** | | | | | |
| Terminal surface + bottom drawer: attach stream, VT grid, scrollback, alt screen, SGR 16/256/truecolor, bracketed paste, resize, new/close, splits (≤4, side by side / stacked), groups sidebar, drawer resize, Add to chat / Copy / Paste, font family/size from Typography | `ThreadTerminalDrawer.tsx`, `terminal/ghostty/{surface,renderer}.ts` | Done | — | `src/ui/terminal_surface.rs`, `src/ui/terminal_drawer.rs`, `src/terminal.rs` | — |
| Context menu | `ThreadTerminalDrawer.tsx` `terminalContextMenuItems` | Done | — | `src/ui/terminal_surface.rs` | — |
| Links | `terminal-links.ts`, `ghostty/surface.ts` (link hit-testing, `hyperlinkAt`), `openTerminalLinkInPreview.ts` | Done (no preview routing) | URLs open in the system browser, not the preview browser | `src/ui/terminal_surface.rs` | S |
| VT completeness | `terminal/ghostty/core.ts` (ghostty-vt wasm) | Done (`alacritty_terminal`) | Kitty keyboard protocol (off; programs fall back to legacy keys) | `src/terminal.rs` | M |
| Cursor blinking | `ghostty/surface.ts` `shouldBlinkTerminalCursor` | Done | Reduced motion is not read | `src/ui/terminal_surface.rs` | — |
| Close confirmation | `lib/terminalCloseConfirm.ts` | Done | — | `src/ui/terminal_surface.rs`, `src/ui/terminal_drawer.rs` | — |
| Persisted groups + reattach after restart | `terminalUiStateStore.ts` (`t3code:terminal-state:v1`) | Done (drawer) | Panel terminal tabs are not persisted (panel state is in-memory) | `src/ui/terminal_drawer.rs` (`ui-state.json` `terminalDrawers`) | S |
| Terminal shortcuts | `terminal.split`, `terminal.splitVertical`, `terminal.new`, `terminal.close` (terminalFocus) | Missing | Buttons exist; bindings not dispatched | `src/ui/shortcuts.rs` | S |
| Server labels, read-only mode, clear/restart | `@t3tools/shared/terminalLabels` `resolveTerminalSessionLabel`, `AuthTerminalOperateScope`, `terminal.clear`/`terminal.restart` | Missing | Port names tabs "Terminal N" only; no read-only state | `src/ui/terminal_surface.rs` | S |

## 2. Details

### Done (brief)

- Right panel: per-thread tabs, launcher with letter shortcuts, "+" menu, resize with the
  360px chat minimum, maximize/restore.
- Diff: working tree, branch and turn scopes with the right RPCs, unified/split, word-alt
  emphasis, expandable unchanged context (`review.getDiffFileContents`, 100-line steps),
  base-ref combobox, file tree, collapse/expand all, wrap, whitespace, refresh, copy path,
  syntax highlighting on a worker thread (the native counterpart of `DiffWorkerPoolProvider`).
- Files: directory-by-directory tree with search, read-only highlighted source with line
  numbers, wrap and reveal, Markdown and CSV/TSV previews with their toggles kept in the
  UI state file, image previews with the media menu, "Open in" picker, "Open file in
  preview browser" for HTML/PDF, Ctrl+P file picker.
- Browser: local headless Chromium over `--remote-debugging-pipe`, `Page.startScreencast`
  frames drawn into the panel, pointer/wheel/keys sent with `Input.*`, navigation row,
  local servers empty state, discovered-port globe.
- Terminal: right-panel surface and bottom drawer, splits, groups sidebar, VT grid on
  `vte`, selection with Add to chat / Copy / Paste, terminal font settings.

### Partial and missing, in detail

**Server-managed preview sessions (the biggest gap).** In v0.1.0 a Browser tab is a
thread-scoped session that lives on the server (`PreviewSessionSnapshot`, `runtime:
"desktop" | "server"`). The client lists them (`preview.list`), follows
`subscribePreviewEvents` (`opened`, `navigated`, `resized` with hard reload / clear
requests, `failed` with code and download, `closed`) and reports the desktop tab's status.
This is what makes the agent `preview_*` tools and the user share one browser, and why an
agent's `preview_open` with `reveal` floats a mini player. The port never calls any
`preview.*` method: its tabs are invisible to agents, agent-opened tabs never appear, and
nothing survives a restart. Native approach: the Pluto server advertises
`serverBrowser: true` (`apps/server/src/environment/ServerEnvironment.ts`) and serves
`/api/preview-stream` (`apps/server/src/preview/ServerBrowserStream.ts`). A server tab
streams complete JPEG frames over a WebSocket and takes JSON input (mouse, wheel, key,
navigate, viewport, takeControl/releaseControl, dialog), plus HTTP upload and download
routes. The port already decodes JPEG screencast frames into egui textures and maps the
pointer and keys to page CSS pixels, so a `ServerBrowserStream` client could reuse
`src/ui/preview.rs`'s drawing and input code almost unchanged. That would also bring
control status, dialogs, uploads, downloads, agent cursor, profiles and zoom
(`previewAdjust`), and would remove the need for a local Chromium. Keep the local CDP
browser for `html_render` and Design, where it works well.

**Local browser shortcomings, if it stays.** No `Page.javascriptDialogOpening` handler,
so `alert()`/`confirm()` stall the page. Fix: answer with `Page.handleJavaScriptDialog`
behind an egui modal. No `Page.setInterceptFileChooserDialog`: file inputs are dead. Fix:
`DOM.setFileInputFiles` with the portal file chooser the composer already uses. No
`Browser.setDownloadBehavior`. Popups from Browser tabs are not attached; upstream opens
them as new tabs (`showPreviewPopup.ts`). The profile is throwaway and the browser runs
`--mute-audio`, so there are no persistent logins and no per-tab mute. The more menu maps
directly onto CDP: `Page.reload {ignoreCache}`, `Emulation.setEmulatedMedia`
(prefers-color-scheme, already used for render tabs), zoom via
`Emulation.setPageScaleFactor` or CSS zoom, `Network.clearBrowserCookies` and
`clearBrowserCache`. DevTools can open the `devtools://devtools/bundled/inspector.html?ws=…`
frontend in a separate Chromium window. The device toolbar needs only
`Emulation.setDeviceMetricsOverride` (already used) plus the toolbar UI. Screenshot: use
`Page.captureScreenshot` (already used internally) and attach the image to the composer.
Recording: collect screencast frames and encode them with an external `ffmpeg`, or as a
GIF/WebP with a crate. Annotations: Pluto's Design comment mode already picks elements
and builds `element` context records, so it can be adapted to `PreviewAnnotationPayload`.
Error page: port `errorCodeMessages.ts` and `PreviewUnreachable.tsx` from the
`Page.navigate` `errorText` and network failures.

**Diff.** (1) Scope labels and default are from the older client: v0.1.0's menu is
Changes / Uncommitted / Latest turn / Turn ▸, defaulting to Changes (`DEFAULT_SELECTION =
{kind:"branch"}`). (2) Clicking a filename should open it in a file tab
(`openDiffFilePrimaryAction` → `openFile`, with `resolveDiffPathForWorkspace` for
worktrees). Right click should open the shared file context menu (`fileContextMenu.ts`:
reveal through `shell.openInEditor {reveal:true}` when the server allows it, Open, Open with
▸ detected editors). (3) Review comments: gutter range selection, an inline comment editor
with its submit shortcut, entries stored as composer-draft review comments and sent as
`review_comment` context. This needs the composer's context-chip machinery extended
beyond terminal excerpts. (4) Large diffs: upstream requests patches per file (four at
first, more as files scroll into view or are revealed), marks truncated files with an info
icon and offers a per-file retry. The port renders the whole preview payload. (5) No
refresh on workspace mutations and no find-in-diff.

**Files.** Editing is the main gap. Upstream edits in place with Pierre's `Editor` and
saves through a coordinator: debounced `projects.writeFile`, echo suppression with content
revisions, a read-only notice that keeps unsaved edits, and dismissal handling. This is
web-only (Pierre's editor), but the native path is straightforward: an egui
`TextEdit::multiline` with a layouter fed by the existing syntect worker
(`src/highlight.rs`), the line-number gutter the read-only view already draws, and the
`projects.writeFile` call the plan card already uses. A crate such as `egui_code_editor`
is an alternative. Once writes exist, Markdown task toggling is small. Also missing: the
tree context menu, drag-to-mention, browsable breadcrumbs, and re-reading the open file
and the tree when the workspace changes. The last one matters most in practice: an
agent's edits do not show until the tab is reopened.

**Media previews.** Upstream uses the browser for all of them: `<video>`, `<audio>`, and an
iframe for HTML and PDF (`BrowserDocumentFrame.tsx`). Native options:
- HTML: reuse the offscreen rendering that `src/ui/html_render.rs` and `src/ui/design.rs`
  already do with the preview browser (signed `workspace-file` URL, frames drawn in the
  surface, input forwarded). Add the "Show rendered page" / "Show HTML source" toggle.
- PDF: try the same offscreen tab with Chromium's viewer, which needs checking under
  `--headless=new`. Otherwise rasterize pages with `pdfium-render` or MuPDF into textures.
- Video: an offscreen Chromium tab on the `media-file` URL works visually; for sound,
  the tab must not get `--mute-audio`. A native player would need GStreamer
  (`gstreamer-rs`) or ffmpeg bindings.
- Audio: `rodio` + `symphonia` (pure Rust) fetching the asset URL, with a small
  play/pause/seek bar.
- AVIF: enable the `image` crate's `avif` feature (or `avif-native`), or route `.avif`
  through the browser.

**Device.** Fully missing. Upstream drives `expo-device-hub` through `device.*` RPCs and
a proxy. Android (`serve-emu`) streams H.264 access units over a WebSocket with a 16-byte
"SEMU" header. iOS (`serve-sim`) sends length-prefixed avcC/H.264 envelopes decoded with
WebCodecs, or MJPEG as a fallback. Input goes back as JSON gestures. The frame is a
three.js GLB device model. Web-only parts: WebCodecs and three.js. Native approach: decode
H.264 with the `openh264` crate (or ffmpeg) into an egui texture, map touches to the
JSON gesture protocol (`phoneInteraction.ts`, `phoneTrackpad.ts`), and draw a flat 2D
bezel instead of the 3D models (record that difference). On Linux only Android emulators
apply unless a remote macOS host is configured. Until then, the launcher should show Device
as unavailable rather than enabled-but-unimplemented.

**Terminal.** The hand-written `vte` grid covers shells and common TUIs but lacks mouse
reporting, real cell widths for wide characters and graphemes, OSC 8, the Kitty keyboard
protocol and replies to DA/DSR queries. Upstream gets all of these from ghostty-vt
(wasm). Native approach: switch the model to `alacritty_terminal`. It provides the grid,
the modes, wide characters, hyperlinks and an event proxy that writes replies back
(here through `terminal.write`), and the current renderer could draw its cells. Smaller
items: "Select all" and "Jump to latest" in the context menu, path links and wrapped-line
links (port `terminal-links.ts`), link routing to the preview browser, cursor blinking,
the close confirmation, persisted groups and drawer height with reattach after restart,
and dispatching the `terminal.split/splitVertical/new/close` bindings.

**Layout.** Per-thread panel state is not persisted. Also missing: the tab context menu,
drag reorder, overflow arrows, reopen closed (Mod+Shift+T), `rightPanel.new` (Mod+T), the
≤980px sheet presentation, proactive panels and back/forward navigation.

## 3. README and module-doc claims found absent or wrong

1. **Agents surface.** README "Right panel" lists "the Agents surface" as ported, and the
   launcher has an "Agents (A)" row. v0.1.0 removed that surface: `rightPanelStore.ts`
   says "v14 removes the agents surface; lineage lives in the thread title bar".
   `AgentsPanel.tsx` no longer exists, and the launcher has 7 entries (Browser, Terminal,
   Files, Diff, Pull request, Linked pull requests, Device). The port's Agents surface is
   a v0.0.45 leftover, not v0.1.0 parity. Lineage is already in the port's thread details
   card.
2. **Diff scopes.** README "Diff panel" ("Working tree, branch and per-turn scopes") and
   the code use the old labels, order and default. v0.1.0 is Changes (default) /
   Uncommitted / Latest turn / Turn ▸ (`DiffPanel.tsx` 303-310, 746-760).
3. **`src/ui/surfaces.rs` module doc** says "Only the Diff surface is rendered here; the
   others report that they are not implemented". Out of date: Files, Terminal, Browser,
   Agents and Design all open. Only Device (and the PR surfaces) still give the
   not-implemented notice.
4. **`src/ui/terminal_surface.rs` module doc** says "Not ported: font settings". Out of
   date: cells use `theme::terminal_font`, i.e. the terminal family and size from
   Settings → Typography (README Settings row).
5. **Device in the launcher** is shown as available (`surface_available` falls through to
   `true`) but only sets a status message. README does say device is not ported; the
   launcher should disable it with its hint.
6. **Proactive panels.** The setting row exists and saves `proactivePanelsEnabled`, but
   nothing opens a panel when a turn completes. README does not list this gap.
7. **Browser settings** are shown under "Only available in the desktop app" even though the
   port has a browser. Default viewport, zoom, appearance, "Open links in" and "Auto-show
   floating preview" have no effect. README does not mention this.
8. **AVIF.** README says AVIF is not ported, which is true. But `.avif` is in
   `IMAGE_EXTENSIONS`, so such files open the image preview and fail with "Unable to load
   workspace image." instead of falling back.
9. **Sidebar width** is commented as a Pluto addition (`src/ui/sidebar.rs` 297). Upstream
   has a resizable, persisted thread sidebar (`AppSidebarLayout.tsx`,
   `threadSidebarWidth.ts`).
10. **Gaps missing from README's not-ported list:**
    - layout: per-thread panel persistence, the tab context menu, drag reorder and overflow
      arrows, reopen closed, Mod+T, the ≤980px sheet;
    - Diff: filename click and context menu, review comments, lazy and truncated
      large-diff handling, auto refresh, find;
    - Files: tree context menu, browsable breadcrumbs, stale open files after agent edits;
    - Browser: the whole server-side session model (`preview.*`, `subscribePreviewEvents`,
      server-browser stream, agent sharing, mini player), JS dialogs, file chooser,
      downloads, popups, device toolbar;
    - Terminal: mouse reporting, wide characters, OSC 8, DA/DSR replies, "Select all" and
      "Jump to latest", the close confirmation;
    - shortcuts that are not dispatched: `preview.*`, `terminal.split/new/close`,
      `navigation.back/forward`.
