# App shell parity: T3 Code v0.1.0 web client vs Pluto (egui)

Upstream: the v0.1.0 client sources (extracted from the package's source maps) (U). Port: the repository, branch `egui-client` (P).
Read-only audit, 2026-10-29. Nothing was built or run. Port line numbers are approximate.
Pluto-only additions are ignored: Chat mode, the Code|Chat switch, design sessions, the Kimi/Vibe/Z drivers, mobile, and the ACP registry.

Size means the effort to finish: S < 1 day, M = a few days, L = a week or more, or blocked on a larger subsystem.
"N/A" marks Electron, macOS, Windows or hosted-web features with no native counterpart.

---

## 1. Parity table

### 1.1 Layout, routes and root coordinators

| Feature | Upstream file(s) | Port status | What is missing | Port location | Size |
|---|---|---|---|---|---|
| Sidebar collapse, floating trigger, sheet below 768px | AppSidebarLayout.tsx `SidebarControl`, ui/sidebar.tsx | done | The tooltip lacks the shortcut ("Toggle main sidebar (Ctrl+B)"). The sheet keeps the docked width (documented). | src/ui/sidebar.rs:362-409 | S |
| Sidebar resize and saved width | threadSidebarWidth.ts, `SidebarRail` | partial | Fixed limits (220-420). Upstream's minimum is 208 or the brand width, and its maximum is the viewport minus 640. A plain click on the rail does not toggle. The legacy sidebar cannot be resized. | sidebar.rs:297-357 | S |
| Settings nav replaces the sidebar on settings routes | AppSidebarLayout.tsx | done | (Settings area) | src/ui/settings.rs | - |
| Back from utility pages to the last main location | sidebar/mainAppLocation.ts | done (equivalent) | Utility pages are overlays, so the selection is kept | sidebar.rs:586 | - |
| Navigation history (`navigation.back` / `navigation.forward`, Mod+[ / Mod+]) | AppSidebarLayout.tsx `NavigationHistoryShortcuts` | **missing** | No history of visited threads or pages. The bindings exist but nothing handles them. | shortcuts.rs:185 (no arm) | M |
| Index landing: a draft in the most recent project, the NoProjectsHero otherwise | routes/_chat.index.tsx | done (equivalent) | No "Couldn't start a new thread / Try again" error state. The hosted-static onboarding is N/A. | src/app.rs:1029 `draft_project` | S |
| NoProjectsHero ("What should we work on?") | NoProjectsHero.tsx | partial | No "Add project" button and no "Start without a project" button. The description never switches to the no-scratch variant. | src/ui/chat.rs:367-388 | S |
| NoActiveThreadState ("Pick a thread to continue") | NoActiveThreadState.tsx | missing | None of its strings are present | - | S |
| Server welcome bootstrap: open `bootstrapThreadId`, expand its project | __root.tsx `EventRouter.handleWelcome` | **missing** | No `bootstrapThreadId` / `bootstrapProjectId` handling | - | S |
| Window title from the server version | __root.tsx `DocumentTitleSync` | missing | Fixed title "Pluto" | src/main.rs:32 | S |
| Route error view ("Something went wrong.", Copy error) / "Page not found" | __root.tsx | N/A | No router. A panic has no in-app report. | - | - |
| Prompt links: `/new?prompt=&repo=&model=`, ThreadLinkPrompt, DesktopWebLinkCoordinator | routes/_chat.new.tsx, promptLinks/* | **missing** | No deep links ("Could not open the link", "Could not open the linked prompt"), no repository-clone landing, and no "Copy prompt link" in the palette | - | L |
| `/projects/$projectKey` redirect to project settings | routes/projects.$projectKey.tsx | N/A (no URLs) | Project settings themselves are ported (projects_page.rs) | - | - |
| `/_chat` global shortcuts (undo, Escape clears selection, new thread variants, preview toggle) | routes/_chat.tsx `ChatRouteGlobalShortcuts` | partial | See 1.6: `thread.undo`, `preview.toggle` ("Preview is desktop-only" toast), the `chat.new` group rule, and `chat.newLocal` context. Escape clears only the legacy selection. | shortcuts.rs:185-334 | M |
| ConfirmDialogHost | ConfirmDialogHost.tsx, confirmDialog.ts | done | - | src/ui/thread_menu.rs:403 | - |
| CustomSnoozeDialogHost | CustomSnoozeDialog.tsx | done | The clock button's own picker (documented) | src/ui/snooze_dialog.rs | - |
| Keybindings reload toast ("Keybindings updated" / "Invalid keybindings configuration" with "Open keybindings.json") | __root.tsx, KeybindingsUpdateToast.logic.ts, KeybindingsConfigWarning.tsx | **missing** | The `keybindingsUpdated` event and its issues are not surfaced. No 2s cooldown. | - | S |
| Legacy thread migration toast ("Restoring your threads…", loading) | LegacyThreadMigrationToast.tsx | **missing** | Relevant: the server imports the v0.0.45 history on first launch | - | S |
| Project clone toasts (Cloning / Cloned / Failed / Cancelled, Cancel, Retry, Remove) | ProjectCloneToastCoordinator.tsx, hooks/useRemoveClonedProject.ts | missing | Cloning itself is not ported (the palette's "Git URL" is a notice) | - | M (after clone) |
| Slow RPC toast ("Some requests are slow", "Show requests") | SlowRpcRequestToastCoordinator.tsx | **missing** | No request-ack timing, no expandable toast | - | M |
| Permission update notice ("Permissions have changed for …", Open Connections / Dismiss) | PermissionUpdateNotice.tsx | missing | No session scope tracking at all (see 1.7) | - | M |
| Reopen closed view (`view.reopenClosed`, Mod+Shift+T, "Could not reopen view") | ReopenClosedViewShortcut.tsx, closedViewStore.ts, reopenClosedView.ts | **missing** | No closed-view stack for panel tabs or browser tabs. The command is absent from the port's DEFAULTS table. | - | M |
| Version skew banner (server older than the client, dismissal per env and version, update guidance) | versionSkew.ts, ChatView.tsx:3145, ServerUpdateAction.tsx | **missing** | Only a hard protocol-2 gate exists (protocol.rs:667, "Unsupported orchestration protocol"). No `serverVersion` comparison or banner. | crates/pluto-core/src/protocol.rs:667 | M |
| Provider update launch notification, desktop update notice | ProviderUpdateLaunchNotification*.tsx, desktopUpdate.toast.tsx | missing / N/A | Provider updates are listed as "not implemented" in the README. Desktop auto-update is N/A. | - | M / N/A |
| Quit hold overlay ("Hold Ctrl+Q to quit") | QuitHoldOverlay.tsx | missing | The settings row "Quit shortcut" exists (settings_search.rs:839), but there is no quit interception or overlay | - | S |
| Theme / contrast / glass / font sync | __root.tsx `*AppearanceSync` | partial | Out of scope (Settings/Appearance). Fonts and the environment theme are ported. Contrast, glass and light appearance are not. | src/theme.rs | - |
| RunningThreadKeepAlive, SnapShotCoordinator, NightlyMobileBetaNotice, RelayClientInstallDialog, ConnectOnboardingDialog, SshPasswordPromptDialog, FirstRunGate/welcome | __root.tsx | N/A or missing | Electron, hosted or onboarding features. The welcome wizard is not ported. | - | - |

### 1.2 Default sidebar (`Sidebar.tsx`)

| Feature | Upstream file(s) | Port status | What is missing | Port location | Size |
|---|---|---|---|---|---|
| Sections pinned / active / snoozed / settled, subagents hidden | Sidebar.tsx 2830-2917, Sidebar.logic `resolveSidebarThreadSection` | done | - | sidebar.rs:113 | - |
| Active order | Sidebar.logic `sortActiveThreadsByOrderKey` | deviates | Pluto orders working threads first, then by activity. The server's `activeOrderKey` is ignored. | sidebar.rs:69 | S |
| Settled order and age label | threadSort `resolveSettledThreadTimestamp` | partial | Uses `completed_at` then `updated_at`, not the latest of user message, run requested, started and completed | sidebar.rs:~96 | S |
| Card time label | Sidebar.tsx `threadTimeLabel` | deviates | Upstream uses `latestUserMessageAt ?? updatedAt` | sidebar.rs:38, 1063 | S |
| Working shelf (beta) | Sidebar.tsx 2841-3030 | **missing** | The setting `sidebarWorkingShelfEnabled` is shown but has no effect | settings.rs:661 | M |
| Shelf headers "Snoozed (n)" / "Settled (n)" | Sidebar.tsx `SidebarSectionHeader` | done | Expanded state is in memory only (upstream: local storage) | sidebar.rs:830, app.rs:50 | S |
| Collapsed shelf keeps the open thread | Sidebar.tsx | partial | Snoozed only. A collapsed Settled shelf hides the open thread entirely. | sidebar.rs:670 | S |
| Settled paging (10, then "Show N more" by 25) | Sidebar.tsx 2960-2985 | missing | All settled rows are rendered | - | S |
| Status labels (Approval, Input, Working, Goal, Waiting, Limited, Failed, Woke, Done) | Sidebar.tsx 1294-1360 | done | v0.1.0 colours for Waiting and Limited (README admits it) | sidebar.rs:164, 927 | S |
| Ticking working / waiting duration | `WorkingDuration`, `formatWorkingDurationLabel` | missing (documented) | - | - | S |
| Hover actions: Settle, Snooze menu, Woke | Sidebar.tsx `SnoozeMenuButton` | partial | Settle is not gated on the `threadSettlement` capability, there is no read-only gating, and the actions don't appear on keyboard focus | sidebar.rs:956-1060 | S |
| Slim rows (settled/snoozed: countdown, age, Un-settle, Wake) | Sidebar.tsx 1795-1960 | partial | No capability gating. No pin, terminal, PR or regenerating indicators. | sidebar.rs:1207 | S |
| Pin glyph and "Unpin thread" hover button | Sidebar.tsx 1760-1795 | missing | Pinned rows are only distinguishable by position | - | S |
| Row 3 indicators: worktree, terminal, PR badge, remote env, provider stack | Sidebar.tsx 2165-2225, ThreadStatusIndicators.tsx | partial | Only the branch and one provider glyph. The worktree, terminal, PR, handed-off stack and instance accent are missing, although the legacy sidebar has the first three. | sidebar.rs:1118-1150 | M |
| Thread hover card | ThreadHoverCard.tsx, `SidebarThreadTooltip` | missing | Project, env, branch-mismatch line, model, handoff, terminals, error, PR list | - | M |
| Regenerating-title state | Sidebar.tsx 1700, threadActionMenu.logic | missing | No dimmed "Regenerating title", and the menu item never shows "Regenerating…" | thread_menu.rs:131 | S |
| Unsent draft rows and block, "Discard draft", draft menu | Sidebar.tsx `SidebarDraftRow` / `SidebarDraftBlock` | **missing** | The port has a single draft slot and no draft store | app.rs:24 | L |
| Thread context menu | threadActionMenu.logic.ts, hooks/useThreadActionMenu.ts | partial | Items match. Missing: read-only gating, the Archive and Unpin confirmations, the move to a draft after archiving the open thread, the "delete worktree too?" prompt, the "New thread on <branch>" carry-over, and the copy toasts | src/ui/thread_menu.rs:64-320 | M |
| Inline rename | Sidebar.tsx 1520-1560 | partial | No "Thread title cannot be empty" warning and no failure toast | thread_menu.rs:325 | S |
| Multi-select and selection menu (Settle/Snooze/Unpin/Regenerate/Mark unread/Delete (n)) | Sidebar.tsx 4290-4560, threadSelectionStore.ts | **missing** | Present in the legacy sidebar only | - | M |
| Move to the next thread after settling or snoozing the open one | Sidebar.tsx `planForwardNavigation`, settleAndAdvance.ts | **missing** | Also missing from the `thread.settle` shortcut | shortcuts.rs:284 | S |
| Undo notice and `thread.undo` ("Settled 2 threads, Ctrl+Z to undo") | hooks/showThreadUndoNotice.ts, threadUndo.ts, sidebar/SidebarThreadUndoNotice.tsx | **missing** | Mod+Z is bound but unhandled | shortcuts.rs:185 | M |
| Drag and drop: reorder pins/active, move between sections, action sweep, drag onto composer | Sidebar.drag.ts, Sidebar.pointer.ts, Sidebar.motion.ts | **missing** | - | - | L |
| File drops on a row | Sidebar.tsx `makeWorkspaceFileDropHandlers` | missing | Exists in the legacy sidebar | - | S |
| Row keyboard (focus, Enter/Space, ContextMenu / Shift+F10) | Sidebar.tsx `handleKeyDown` | missing | - | - | M |
| Search: titles then message matches with "You:" / "Agent:" | Sidebar.logic:1081, Sidebar.tsx 2240-2420 | partial | It filters the shelves instead of showing upstream's flat result list. Settled matches stay hidden while that shelf is collapsed. Missing: PR number and branch terms, arrow/Enter/Escape navigation, "Searching thread messages…", and "No threads found" (the port shows "No threads yet"). | sidebar.rs:634-668, 763 | M |
| Header row (SidebarThreadHeader: Search with clear ×, scope, Add project, New thread) | sidebar/SidebarThreadHeader.tsx | deviates | Pluto has separate rows. The New thread tooltip, Shift+click, the palette when there are several projects, and the disabled state with no projects are missing. | sidebar.rs:426-584 | M |
| Project scope picker | Sidebar.tsx 4995-5100 | partial | Scopes single projects, not groups. Not persisted (documented). No right-click to project settings. | src/ui/project_scope.rs | S |
| Empty states | Sidebar.tsx 5550-5580 | done | "No threads yet" is shown for an empty search result | sidebar.rs:763 | S |
| Footer utility menu | sidebar/SidebarChrome.tsx | partial | Pull Requests is not gated on `pullRequestsSupported`. The label is "Pull requests" (upstream: "Pull Requests"). | sidebar.rs:586 | S |
| Thread jumps with hint chips | Sidebar.tsx 4880-4935 | partial | Chips exist only in the legacy sidebar. `ordered_sidebar_threads` ignores chat filtering and message matches, so the jump order can differ from the rendered rows. | shortcuts.rs:337 | S |
| Stage backdrop and environment pill | SidebarStageBackdrop.tsx, SidebarChrome.tsx | missing | Listed as "not implemented" (environment identification) | - | M |
| Provider update pill | sidebar/SidebarProviderUpdatePill.tsx | missing | - | - | M |
| Desktop update pill / capsule / release notes / Update Everything | sidebar/SidebarUpdate*.tsx, updateEverything*.ts | N/A (mostly) | Updating remote servers would apply only with multiple environments | - | L |

### 1.3 Legacy sidebar and project identity

| Feature | Upstream file(s) | Port status | What is missing | Port location | Size |
|---|---|---|---|---|---|
| Search row, "Projects" header, Sidebar options, Add project, "No projects yet" | LegacySidebar.tsx 3062-3206 | done | The visible-thread count can't be typed, only stepped with −/+ | src/ui/legacy_sidebar.rs:305-1421, 2106-2249 | S |
| Project and thread sort, project grouping with overrides, "N projects" | Sidebar.logic, sidebarProjectGrouping.ts | done (one environment) | No cross-environment groups or `environmentPresence` | src/ui/project_grouping.rs | L (multi-env) |
| Collapse memory, status dot when collapsed, open thread kept | LegacySidebar.tsx 1285-1484, 2450-2477 | done | - | legacy_sidebar.rs:223, 1228, 1670 | - |
| Show more / Show less, "No threads yet" | LegacySidebar.tsx 1066-1141 | done | - | legacy_sidebar.rs:1532 | - |
| Manual drag reorder of projects | LegacySidebar.tsx 3129-3175 | done | No auto-scroll near the list edges | legacy_sidebar.rs:1423-1576 | S |
| List animations (autoAnimate 180ms) | LegacySidebar.tsx 3480 | missing | - | - | M |
| Project menu (Rename, Group into…, Copy Path, Project settings, Remove) and both dialogs | LegacySidebar.tsx 1700-1824, 2581-2705 | done | Copy Path shows a plain status message instead of a "Path copied" success toast. The "This connection cannot change projects." line is missing. | legacy_sidebar.rs:563-1193 | S |
| Remove project flow | LegacySidebar.tsx 1533-1698 | partial | No "Project is not empty" / "Delete anyway" step. `force` is sent only when there are threads (upstream always sends it). Drafts are not cleared. No failure toast. | legacy_sidebar.rs:689-729 | S |
| Thread status pills (7) | Sidebar.logic 1180-1270 | done | - | legacy_sidebar.rs:174-221 | - |
| PR glyph | LegacySidebar.tsx 761-799 | partial | It always opens the browser. Upstream opens the in-app PR panel (`pullRequestLinksOpenInApp`). | legacy_sidebar.rs:1797 | M |
| Terminal glyph | ThreadStatusIndicators.tsx 791-838 | partial | Clicking does nothing (upstream opens that terminal). The tooltip lacks the "N terminal processes running. Open terminal" wording. | legacy_sidebar.rs:1861 | S |
| Worktree glyph, port globe | ThreadStatusIndicators.tsx, LegacySidebar.tsx 829 | done | No "Unable to open preview" toast | legacy_sidebar.rs:1880-1944 | - |
| Archive on hover (with Confirm) | LegacySidebar.tsx 860-910 | partial | No "Archived" / Undo notice, no move to a draft when the open thread is archived, no failure toast | legacy_sidebar.rs:1990 | M |
| Thread menu | LegacySidebar.tsx 2296-2435 | partial | "New thread on {branch}" drops the branch, worktree and env mode. No "Path copied" / "Thread ID copied" / "Path unavailable" toasts. | thread_menu.rs:222-333 | S-M |
| Multi-select, selection menu, file drop on a row, jumps | LegacySidebar.tsx 1842-2061, 3581-3721 | done | Failure and undo toasts as above | legacy_sidebar.rs:441-560, 1949 | - |
| Row and header keyboard (Enter/Space) | LegacySidebar.tsx 558, 1486 | missing | - | - | S |
| Permission scopes ("Action unavailable", disabled items) | LegacySidebar.tsx 367-377 | missing | No auth scope model | - | M |
| Desktop-local / WSL backend alerts, Intel-on-Apple-Silicon warning | LegacySidebar.tsx 2718-2786, 3074 | N/A | - | - | - |
| Project icon: Lucide, emoji, monogram, favicon | ProjectFavicon.tsx, ProjectMonogram.tsx, projectIdentity.ts, projectIconColors.ts | done | NFKC normalisation; favicon 25% corner rounding | src/ui/project_icon.rs, src/ui/favicon.rs, theme.rs:540 | S |
| EnvironmentMachineIcon on project headers and thread rows ("Remote environment: …") | EnvironmentMachineIcon.tsx, LegacySidebar.tsx 2498, 913 | missing | A private helper exists but nothing in the sidebar uses it | setting_inheritance.rs:150 | L (multi-env) |
| ProjectEnvironmentBadge | ProjectEnvironmentBadge.tsx | missing | Only meaningful with several environments | - | L (multi-env) |
| ConnectionStatusDot | ConnectionStatusDot.tsx | missing | No connection-phase dot anywhere | - | S |
| New project from a name (`useNewProject`) | hooks/useNewProject.ts | missing | The palette shows a "not implemented" notice | palette.rs:351 | M |

### 1.4 Command palette

| Feature | Upstream file(s) | Port status | What is missing | Port location | Size |
|---|---|---|---|---|---|
| Three modes (command / files / content), toggled by their shortcuts | CommandPalette.logic `reduceCommandPaletteUiState` | partial | Ctrl+K inside a submenu reopens the root (upstream closes the palette) | src/ui/palette.rs:207, shortcuts.rs:185 | S |
| Esc in Files / Content returns to the command palette | CommandPalette.tsx 408-419 | **missing (bug)** | The port closes everything although its footer says "Esc Back" | palette.rs:785, 1116 | S |
| Root groups Actions / Recent Threads (12), typed search groups and ranking, settings results | CommandPalette.logic `buildRootGroups`, `filterCommandPaletteGroups` | done | Recent threads ignore the `created_at` sort setting | palette.rs:411-471, 691-720 | S |
| "New thread in X" | CommandPalette.tsx 1810 | partial | Wrong shortcut label: hard-coded Ctrl+Shift+O instead of the `chat.newLocal` binding | palette.rs:312 | S |
| "New thread in..." submenu | CommandPalette.tsx 1836, 1291 | partial | The current project isn't listed first and "No project" isn't second. No Ctrl+1..9 rows. Sidebar sort and grouping are ignored. | palette.rs:474-502 | M |
| "New thread without a project", "Go to file", "Search project contents", "Add project", "Open usage", "Open settings" | CommandPalette.tsx 1847-2245 | done | - | palette.rs:328-401 | - |
| "Project settings" | CommandPalette.tsx 2247 | **wrong** | Runs `Action::Missing` although `open_project_settings` exists | palette.rs:402-410 | S |
| "Open pull requests" | CommandPalette.tsx 2206 | partial | Not gated on `capabilities.pullRequests` | palette.rs:378 | S |
| "Find in current thread" | CommandPalette.tsx 1859 | missing (silent) | `open_thread_find` already exists | - | S |
| "Copy thread ID" / "Copy PR link" (`thread.copyReference`) | CommandPalette.tsx 1924 | missing (silent) | The shortcut Mod+Shift+C is unhandled too | - | S |
| "Copy prompt link" (+ repo and model) | CommandPalette.tsx 1873 | missing (silent) | Needs prompt links (1.1) | - | M |
| "Link pull request to thread" / "Show linked pull requests", linked-threads intent | CommandPalette.tsx 1941, commandPaletteBus.ts | missing (silent) | Thread PR linking is not ported | - | L |
| "Restart agent session" | CommandPalette.tsx 1968 | missing (silent) | The stop and refresh primitives exist | - | S |
| "New project" + its flow (name, preview, private repo on host) | CommandPalette.tsx 2036, 2840-3015 | missing (notice) | Not gated on `newProjectsRoot` | palette.rs:348 | L |
| Add project → Sources (GitHub, GitLab, Bitbucket, Azure DevOps, Forgejo/Gitea, GitCafe, "Setup Required") | CommandPalette.tsx 1537-1645 | partial | Local folder works. Git URL is a notice. Only GitHub is listed, as a notice. | palette.rs:504-526 | M |
| Clone flow (Repository step, Lookup/Continue, "Select where to clone", Clone / Create & Clone) | CommandPalette.tsx 2541-2760 | **missing** | - | - | L |
| Local folder browse | CommandPalette.tsx 2740-2810, logic `buildBrowseGroups` | partial | No ".." row and no "Loading folders...". No "Create & Add" or "Press Enter to create this folder…", no Ctrl+Enter. No relative `./` paths. Ignores `addProjectBaseDirectory` and always starts at `~/`. | palette.rs:528-581, 1245 | M |
| Root browse mode (typing `~/` or `/` at root) | `getFilesystemBrowsePath` | missing | - | - | M |
| Project search result / adding an existing path opens its latest thread | CommandPalette.tsx 1141, 2390 | partial | Always opens a new draft | palette.rs:428, 1251 | S |
| "Change theme", "Change appearance", "Toggle theme editor" + `theme.select` / `appearance.cycle` / `themeEditor.toggle` keys | CommandPalette.tsx 2096-2204, 433-466 | missing (notice) | The port is dark-only, and the bound keys do nothing | palette.rs:358-377 | L |
| `>` actions-only filter ("No matching actions.") | logic 437-452 | missing | `>` only disables the message search | palette.rs:745 | S |
| Empty state "No matching commands, projects, or threads." | CommandPaletteResults.tsx 36 | missing | - | palette.rs:793 | S |
| Disabled rows | logic `buildCommandPaletteRows` | missing | - | palette.rs:107 | S |
| Highlight behaviour | CommandPalette.tsx 611, 3186 | **bug** | After typing, the first ArrowDown selects the second row. Submenus pre-highlight a row. | palette.rs:770-776 | S |
| Ctrl+1..9, Ctrl+Shift+C, Ctrl+F inside the palette | CommandPalette.tsx 3112-3141 | missing | - | shortcuts.rs:186 | S |
| Shortcut labels | Results `shortcutLabelForCommand` | partial | Hard-coded, so remapped keys are not shown. `effective_shortcut_label` exists but is unused here. | palette.rs:318-376 | S |
| Thread subtitle | ThreadCommandSubtitle.tsx | partial | No "Current thread" segment, no FolderGit2 icon for worktrees, "Local" shown without a project | palette.rs:1450 | S |
| Thread row status | ThreadStatusIndicators.tsx 932-1180 | partial | 3 of 7 pills. No PR or terminal icons, no `#N` | palette.rs:233 | S/M |
| Message matches (You: / Agent:) | ThreadSearchMatch.tsx | partial | The query isn't highlighted in the excerpt. No "Searching thread messages…". | src/ui/thread_search.rs:165 | S |
| Go to file / content search from a draft | ProjectFilePicker, ProjectContentSearchDialog | partial (documented) | No target without a thread | palette.rs:634, content_search.rs:139 | S |

### 1.5 Toast system

| Feature | Upstream file(s) | Port status | What is missing | Port location | Size |
|---|---|---|---|---|---|
| Stack at top right, type icons, spinner, title, description, action, corner dismiss, 2 peeking, spread on hover, 5s timeout paused on hover | ui/toast.tsx, toast.logic.ts | done | - | src/ui/toasts.rs | - |
| Typed toasts at call sites | ui/toastHelpers.ts `stackedThreadToast` | partial | About 56 call sites still set `self.status`, which becomes an untyped Plain toast with no icon and no title/description split. Copy, failure and warning feedback across the sidebar and menus is flattened. | grep `self.status =` | M |
| "Copy error" button on error toasts (with `hideCopyButton`) | ui/toast.tsx `CopyErrorButton` | **missing** | - | toasts.rs | S |
| Clamp long error descriptions (line-clamp-4 past 180 characters) | ui/toast.tsx | missing | - | - | S |
| Thread-scoped toasts (`data.threadRef` / `threadId`: shown only while that thread is open) | toast.logic `shouldRenderThreadScopedToast` | **missing** | Git action and ChatView toasts show on every thread | - | S |
| Secondary / additional actions, action variants | ui/toast.tsx `ThreadToastData` | missing | One action only (git, URL, thread). Needed by the permission notice, provider updates, clone and preview. | toasts.rs:33 | S |
| Expandable content ("Show details" / "Show requests") | ui/toast.tsx | missing | Needed by the slow RPC toast | - | S |
| `dismissAfterVisibleMs` (counts down only while the window is focused) | ui/toast.tsx `ThreadToastVisibleAutoDismiss` | missing | The port's timers run while unfocused | - | S |
| Swipe to dismiss, enter and exit animations | ui/toast.tsx | missing | Styling and gesture only | - | S |
| Anchored copy toast ("Copied!") | ui/anchoredCopyToast.ts | done | - | (markdown, chat) | - |

### 1.6 Keybindings handling

| Feature | Upstream file(s) | Port status | What is missing | Port location | Size |
|---|---|---|---|---|---|
| `resolveShortcutCommand` (last match wins, when AST), `shortcutLabelForCommand` | keybindings.ts, shared/keybindings.ts | done | - | src/ui/shortcuts.rs:17-110 | - |
| When-clause context | keybindings.ts `ShortcutMatchContext` | partial | `terminalOpen`, `previewOpen` and `previewFocus` are hard-coded false. `rightPanelOpen`, `turnRunning` and `draftThreadRoute` are absent from the global context, so bindings gated on them never match (e.g. `rightPanel.new`). | shortcuts.rs:139 | S |
| Handled commands: palette modes, usage keys, `chat.new*`, `chat.find`, `sidebar.toggle`, `threadPanel.toggle`, `editor.openFavorite`, `terminal.toggle`, `rightPanel.toggle` / `close`, `diff.toggle`, `thread.previous` / `next` / `jump.N`, `thread.settle` / `pin`, `modelPicker.*`, queue steer / edit | shortcuts.rs | done | - | shortcuts.rs:185-334 | - |
| `chat.new` / `chat.newLocal` semantics | routes/_chat.tsx | partial | The palette opens when there are more than one *project* (upstream: more than one project *group*, and only in the default sidebar). `chat.newLocal` is treated like `chat.new` with no context carry-over (`startNewThreadFromContext`: branch and worktree of the active thread). | shortcuts.rs:204-205 | S |
| Unhandled bound commands | various | **missing** | `thread.undo`, `thread.copyReference`, `navigation.back` / `forward`, `view.reopenClosed`, `rightPanel.new`, `preview.toggle` / `refresh` / `focusUrl` / `zoom*`, `theme.select`, `appearance.cycle`, `themeEditor.toggle`, `composer.stash` / `host` / `effort` / `mode` / `workspace` / `branch` / `previousWorktree`, `modelPicker.toggle`, `pullRequest.copyNumber`, `composer.sendBackground` / `sendAndNewThread`. They are swallowed or ignored. | shortcuts.rs:331 | M |
| Terminal-scoped bindings (`terminal.split` / `splitVertical` / `new` / `close`) through the binding table | keybindings.ts `isTerminal*Shortcut` | partial | The terminal surface has its own menu actions. The bindings in the table are not consulted, so a remapped key is not honoured. | src/ui/terminal_surface.rs:520 | S |
| Default table used by Settings (Reset to default, Custom badge) | shared/keybindings.ts `DEFAULT_KEYBINDINGS` | **stale** | The port's DEFAULTS is the v0.0.45 list. It lacks `rightPanel.new` (Mod+T), `view.reopenClosed` (Mod+Shift+T), `composer.sendAlternate` (Mod+Enter while running), `composer.sendBackground` and `composer.sendAndNewThread`. | src/ui/keybindings.rs:1033 | S |
| Modifier-held hint chips (`useShortcutModifierState`) | shortcutModifierState.ts | partial | Legacy sidebar only | legacy_sidebar.rs:1282 | S |
| Keybindings reload toasts | __root.tsx | missing | See 1.1 | - | S |

### 1.7 App-wide state and behaviours (`src/state`, `src/lib`, top-level stores)

| Feature | Upstream file(s) | Port status | What is missing | Port location | Size |
|---|---|---|---|---|---|
| Thread status resolution (sidebar status, pills) | Sidebar.logic `resolveSidebarThreadStatus`, `resolveThreadStatusPill` | done | v0.1.0 colours for Waiting and Limited | sidebar.rs:164, legacy_sidebar.rs:174 | S |
| Visited / unread (server `thread.visit`, `thread.mark-unread`; local fallback; never rewinds) | uiStateStore `markThreadVisited` / `markThreadUnread`, ChatView | done | - | src/app.rs:791-905 | - |
| Visited migration (push local watermarks to a server that gained tracking) | hooks/useThreadVisitedMigration.ts | missing | `visited.json` entries are never pushed with `thread.visit` | - | S |
| Woke dismissal and the open thread's Woke banner | client-runtime thread-settled `threadWokeAt`, `useAcknowledgeThreadWoke`, ChatView 7024-7065 | partial | `woke_at` and the badge's dismiss click use the local `last_visited` map only. Upstream resolves server `lastVisitedAt` ?? local and acknowledges through the server, so a dismissal on another device hides it. The chat view's Woke banner for the open thread is absent (no "woke" in chat.rs or composer.rs). | app.rs:866, sidebar.rs:1054, 1330 | S |
| Snooze rules and presets | Sidebar.snooze.ts, thread-settled | done | Preset times are always 12-hour | src/snooze.rs | S |
| Settle / un-settle, settle-and-advance | settleAndAdvance.ts | partial | No advance (see 1.2) | shortcuts.rs:284 | S |
| Archive (sidebar, menu; archived list) | lib/archivedThreadsState.ts, hooks/useThreadActions.ts | partial | Archiving works and the Settings Archive page exists. No undo notice, no move to a draft. | thread_menu.rs, settings.rs | M |
| Thread selection store (toggle, range, anchor, clear on Escape / click / scope change, prune after delete) | threadSelectionStore.ts | partial | Legacy sidebar only | legacy_sidebar.rs:441 | M |
| Timestamp format (`timestampFormat`: locale / 12-hour / 24-hour) | timestampFormat.ts | **missing (inert setting)** | The setting is saved (settings.rs:745) but never read. Times are always `%-I:%M %p`. | widgets.rs:587, snooze.rs:28, usage.rs:906 | S |
| Day-aware chat timestamp | timestampFormat.ts `formatDayAwareTimestamp` | deviates | The port prints "Oct 5 3:04 PM". Upstream prints "yesterday at 3:04 PM", or "10/5 3:04 PM", adding the year when it differs. | widgets.rs:587 (used by chat.rs:1596, diff_panel.rs:758) | S |
| Chat timestamp tooltip ("12:04, 4th June 2026") | timestampFormat.ts `formatChatTimestampTooltip` | missing | - | chat.rs:1596 | S |
| Relative time ("now", "5m", "3h", "2d") | timestampFormat.ts `formatRelativeTime` | done | - | widgets.rs:567 | - |
| UI state persistence (`t3code:ui-state:v1`) | uiStateStore.ts | partial | Persisted: `projectExpandedById`, `projectOrder`, plus Pluto keys. Not persisted: `sidebarProjectScopeKey` (documented), `threadChangedFilesExpandedById` (changed-files folder expansion), `pullRequestMergeMethod`, `defaultAdvertisedEndpointKey`. | src/app.rs:1772-1815 | S |
| Sidebar shelf expansion and Usage metric persistence | Sidebar.tsx `useLocalStorage`, usage | partial | In memory (the Usage part is documented) | app.rs:50 | S |
| Multi-environment (catalog, primary plus remote, per-env shells, env labels, load balancing) | state/environments.ts, primaryEnvironment.ts, connection/*, hooks/useLoadBalancedEnvironment.ts | **missing (by design)** | One server and token. Every env-aware UI is absent: machine icons, env badges, cross-env groups, env filter, per-env notifications. | crates/pluto-core/src/protocol.rs | L |
| Connection resilience (automatic reconnect, disconnect delay, connection phase) | state/session.ts, hooks/useEnvironmentDisconnectDelay.ts, ConnectionStatusDot.tsx | **missing** | Any socket error sends the desktop app back to the Connect screen (app.rs:739). No automatic retry, no "reconnecting" state, no status dot. | src/app.rs:739, protocol.rs:144 | M |
| Auth scopes / read-only sessions (`useEnvironmentScope`, "This connection cannot change threads.") | state/session.ts, auth.ts | missing | No scope model, so nothing is disabled for limited sessions | - | M |
| Version skew | versionSkew.ts | missing | See 1.1 | - | M |
| Thread notifications (completion, input, approval, failed, limited; in-app toast when focused on another thread; system notification) | ThreadNotificationCoordinator.tsx, threadNotifications.ts | partial | Sounds (`notification-completion.mp3` / `-input.mp3`, modes "Sound only" / "with sound") are missing. So is the badge count, closing pending notifications on focus, and one notification per thread (tag replacement). Pluto also notifies while focused on another thread (deliberate). | src/ui/thread_notifications.rs | S-M |
| Thread routes / draft promotion keeps one view mounted | threadRoutes.ts, routes/_chat.*.tsx | done (equivalent) | - | app.rs `select_thread` | - |
| Thread detail prewarm on hover | LegacySidebar.tsx 250 | missing | Performance only. Threads left within 5 minutes reopen at once (README). | - | S |

---

## 2. Done items (brief)

- Sidebar inbox core: the sections and shelves, the status labels, the Settle/Snooze/Wake/Un-settle actions, the context menu, the custom snooze dialog, the project scope picker, collapse and the mobile sheet, resize with a saved width, and the empty states.
- Legacy sidebar: nearly complete. It has sort, grouping, collapse, pills, project and thread menus, both dialogs, manual drag reorder, multi-select, file drops, the worktree, port and PR glyphs, and jumps with hint chips.
- Project identity: Lucide and emoji icons, monograms with the 18 colours, and signed-URL favicons.
- Palette: ranking, settings results, message search, recent threads, "Go to file", content search, local folder add, and "New thread without a project".
- Toast stack visuals and timing. The anchored "Copied!" toast.
- Keybinding resolution and labels. Most navigation, panel, thread and usage commands.
- Visited and unread tracking (server and local), snooze rules, relative times, the confirm dialog host.
- System and in-app thread notifications through `notify-send`.
- The protocol-2 gate on connect.

## 3. Detail on the main partial and missing items

1. **Default sidebar interaction model.** Drag and drop, multi-select, the undo notice with Mod+Z, draft rows, the hover card, row indicators and settle-and-advance are all absent from the default sidebar, which is the one v0.1.0 ships. Several of these already exist in `legacy_sidebar.rs`: the selection model, file drops, terminal metadata, PR and worktree glyphs, and jump chips. Lifting them into `sidebar.rs` would make those S-M. Drag and drop (Sidebar.drag/pointer/motion) and drafts (they need a draft store) are L.
2. **Undo infrastructure.** `showThreadUndoNotice` / `threadUndo` back the footer notice for Settled, Snoozed, Unpinned, Archived and Discarded, and the `thread.undo` binding. Nothing equivalent exists, so Mod+Z is bound and silently ignored.
3. **Root coordinators.** The keybindings reload/invalid toasts, the legacy migration toast, the slow-RPC toast, the permission notice, reopen-closed-view, the version-skew banner and the quit-hold overlay are all missing. The migration toast and version skew matter most for this fork (a v0.0.45 history import, and a client and server versioned separately).
4. **Toast data model.** `toasts::Toast` has one action and no `threadRef` scoping, copy-error button, secondary actions, expandable content or focus-based timers. On top of that, about 56 call sites still use `self.status`, so typed success, warning and error feedback (e.g. "Path copied", "Thread title cannot be empty", "Failed to archive thread") is flattened into untyped plain toasts.
5. **Keybinding dispatch gaps.** About 20 bound v0.1.0 commands have no handler. The when-clause context fakes `terminalOpen`, `previewOpen` and `previewFocus`, and omits `rightPanelOpen`, `turnRunning` and `draftThreadRoute`. The port's DEFAULTS table (Settings → Keybindings) is still the v0.0.45 list.
6. **Timestamps.** The `timestampFormat` setting is inert. The chat time format and the hover tooltip differ from `formatDayAwareTimestamp` / `formatChatTimestampTooltip`.
7. **Connection model.** There is one environment and no automatic reconnect: any socket error drops to the Connect screen. Multi-environment UI (machine icons, env badges, cross-env groups) is blocked by design. Auth scopes are not modelled, so read-only sessions see enabled actions.
8. **Palette.** Six thread actions are silently absent: Find in current thread, Copy thread ID / PR link, Copy prompt link, Link PR, Show linked PRs, Restart agent session. New project and clone flows are missing. Project settings wrongly shows "not implemented". Two bugs: Esc in Files/Content closes everything, and the first ArrowDown after typing skips a row.
9. **Prompt links and deep links** (`/new?prompt=…`, ThreadLinkPrompt, Copy prompt link) are not ported at all.

## 4. README and module-doc claims that are absent, wrong or stale

1. **README "Sidebar" row** lists the status labels "Pending Approval, Awaiting Input, Working, Connecting, Plan Ready, **Monitoring**, Completed". The label is "Waiting"; "Monitoring" exists in neither upstream nor the port (legacy_sidebar.rs:198, Sidebar.logic.ts:1225).
2. **README "Sidebar" row** lists multi-select, file drops, PR/worktree/terminal glyphs and jump chips in one sentence with the default sidebar's features. Those exist only in the legacy sidebar. The row reads as if the default sidebar has them.
3. **README "Sidebar" row**, "New thread on branch": the branch, worktree and env mode are not carried over (thread_menu.rs:223-225). The README only says so in the trailing "Not ported yet" note.
4. **README "Sidebar" row**, terminal glyph: it is not clickable and its tooltip differs. The PR glyph always opens the browser rather than the in-app panel.
5. **README "Not ported from v0.1.0 yet"** mentions only the working duration and the Waiting/Limited colours for the sidebar. It omits the Working shelf (whose setting is shown but inert), drag and drop, multi-select and the undo notices / Mod+Z in the default sidebar, the hover card, draft rows, the pin glyph and Unpin button, row indicators, the regenerating state, the header layout, the search result list, settled paging, settle-and-advance, the archive and unpin confirmations, the update pills and the stage backdrop.
6. **README "Not ported yet" palette list** names only the themes, git clone, and file/content search from a draft. It omits: New project, the non-GitHub sources, Project settings showing "not implemented" although the page exists, the six silent thread actions, the `>` filter, root browse, ".." and "Create & Add", the empty state and disabled rows.
7. **README "Keybindings" row** ("The server's resolved bindings drive the global shortcuts…") omits that `thread.undo`, `thread.copyReference`, `navigation.*`, `view.reopenClosed`, `rightPanel.new`, `preview.*`, `theme.*`, `composer.*` (except via the composer's own handling) and `pullRequest.copyNumber` are unhandled. It also omits that the Settings defaults table is v0.0.45's.
8. **README "Header" row (toasts)** is accurate for what it lists but does not mention the missing copy-error button, thread scoping, secondary actions or expandable details.
9. Nothing in the README records the root coordinators (keybindings reload toast, legacy migration toast, slow-RPC toast, permission notice, reopen closed view, version skew banner, quit hold overlay), the inert `timestampFormat` setting, or the absence of automatic reconnection.
10. **Stale module docs:**
    - `src/ui/project_icon.rs:3` says "Favicon images are not fetched yet", but `favicon.rs` fetches them and `project_icon.rs:132` paints them.
    - `src/ui/thread_menu.rs:3` says "Project settings are not ported yet", but the menu opens them (`projects_page.rs:55`).
    - `src/ui/legacy_sidebar.rs:7` says the globe's preview panel is "Not ported", but it opens a Browser tab (legacy_sidebar.rs:1934).
    - `src/ui/shortcuts.rs:6-10` lists "the terminal drawer" both as handled and as not ported.
    - `src/ui/palette.rs` has no "Not ported" list, and its `View::Files` doc ("Esc closes it", line 37) contradicts the footer's "Esc Back".
    - `src/ui/surfaces.rs:3` says "Only the Diff surface is rendered here; the others report that they are not implemented". The README lists Files, Terminal, Agents and Browser as ported, so this doc should be re-checked against the surfaces audit.
