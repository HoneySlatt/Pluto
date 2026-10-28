# Pull requests and git: parity inventory (T3 Code v0.1.0 web client vs Pluto egui port)

Scope: `src/components/pullRequest/**`, `GitActionsControl*.ts(x)`, `PullRequestContextDetails.tsx`,
`PullRequestThreadDialog.tsx`, `ProjectScriptsControl.tsx`, `projectScriptEditor.tsx`,
`routes/_chat.pull-requests.tsx`, `state/usePullRequestStack.ts`, `state/vcs.ts`,
`state/pullRequests.ts`, `state/sourceControlActions.ts`, and the client-runtime atoms they wrap
(`packages/client-runtime/src/state/{pullRequests,vcs,git,sourceControl,threadCommands}.ts`).

Port: the repository (branch `egui-client`). Upstream paths are relative to
the v0.1.0 client sources (extracted from the package's source maps). The port talks to one environment, so the upstream multi-server work
(`pullRequests.routing`, `pullRequests.routingIdentity`, `pullRequests.reportState`, the "Server"
filter, `pullRequestProjectAssignment.logic.ts`) does not apply and is left out below.

RPC methods the port calls today in this area: `pullRequests.list`, `pullRequests.summary`,
`git.runStackedAction`, `vcs.pull`, `vcs.init`, `vcs.listRefs`, `vcs.switchRef`, `vcs.createRef`,
`sourceControl.publishRepository`, `subscribeVcsStatus`, `review.getDiffPreview`,
`review.getDiffFileContents`, plus the events `thread.pull-request-link-updated` and
`thread.pull-request-synced`. All the other `pullRequests.*` methods (28 of them) are unused.

## 1. Table

| Feature | Upstream file(s) | Port status | What is missing | RPC methods | Port location | Size |
|---|---|---|---|---|---|---|
| PR list page: listing, involvement grouping, sort tiers | `routes/_chat.pull-requests.tsx`, `pullRequestList.logic.ts` | Done (v0.0.45 shape) | Involvement is not sent to the host (v0.1.0 asks the host and runs separate authored/reviewing partition queries), so the authored/reviewing rows past the first 99 are never seen | `pullRequests.list` | `src/ui/pull_requests.rs` | S |
| Search | `pullRequestList.logic.ts` (`parsePullRequestQuery`, `scorePullRequestMatch`, `rankPullRequestMatches`), route | Partial | Host-side search (`query`, 250 ms debounce), qualifiers (`label:`, `-label:`, `author:`, `draft:`, `review:`, `checks:`/`status:`, namespaced `size:S`), match ranking while searching, "Matched in the description" row marker | `pullRequests.list` (`query`, `filters`) | `pull_requests.rs` `matches_query` | M |
| Filters menu | `PullRequestListFilters.tsx` | Partial | Author facet (searchable), Labels facet, "· Unavailable" project marking from per-project `errors`, filter preferences persisted (`pullRequestListPreferences.ts`) | `pullRequests.list` (`filters.author`, `filters.labels`, `excludedLabels`) | `pull_requests.rs` `pull_requests_menu` | M |
| Host menu | route `CompactFilterMenu`, `PullRequestListFilters.tsx` | Partial | Every host gets the GitHub icon (`sc-github` hard-coded); hosts not signed in are not flagged | `pullRequests.list` (`providers`) | `pull_requests.rs:1076` | S |
| Paging / "Load more" | route (`PAGE_SIZE` 99, `MAX_PAGE_SIZE` 500, cursors), `PullRequestListEmptyState.tsx` | Missing | Cursors, "Load more pull requests", "Loading more" status; port only shows "Narrow your search…" when `truncated` | `pullRequests.list` (`cursors`, `limit`) | — | M |
| Line-count backfill | route stats batching, `pullRequestList.logic.ts` stats helpers, `client-runtime/state/pullRequestStatsPersistence.ts` | Missing | Visible-row `listStats` batches, stats cache; rows show counts only if the listing carried them | `pullRequests.listStats` | — | M |
| Live refresh | `hooks/useLiveRefresh.ts`, route `refreshAfterTurn` | Missing | Re-read on window focus and every minute while visible, refresh after an agent turn | `pullRequests.subscribeRefreshes`, `pullRequests.list` | — | S |
| Refresh button | route `refreshFromHost` | Partial | Re-sends the listing but does not drop the server's cache first | `pullRequests.invalidate` | `pull_requests.rs` toolbar | S |
| List snapshot (instant first paint) | `pullRequestList.logic.ts` `read/writePullRequestListSnapshot` | Missing | Last listing restored from storage while the new one loads | — | — | S |
| Rows | `PullRequestRow.tsx`, `PullRequestListRow.tsx`, `pullRequestPresentation.tsx` | Partial | Avatars (initials drawn), checks popover, stack popover, right-click menu (Copy link / Open on <host>), click opening the detail (shows a notice) | `pullRequests.checks` (popover, via `detail`), `pullRequests.stack` | `pull_requests.rs` `pull_request_row` | M |
| Row speed actions + close sweep | `PullRequestSpeedActions.tsx`, `usePullRequestActions.ts` (`usePullRequestCloseBatch`), route close sweep | Missing | Hover Close / Merge / Ready for review / Reopen, drag-across batch close, optimistic overrides (`applyPullRequestOverrides`) | `pullRequests.runAction` (+ `detail`, `stack` for merge prep) | — | M |
| Condensed topbar / expandable search | route `ExpandableSearch`, `CompactFilterMenu`, `PullRequestsColumn` | Missing | Narrow-width layout | — | — | S |
| Page shortcuts | route `onKeyDown` | Missing | Escape back, `rightPanel.toggle/close/toggleMaximized`, `thread.copyReference` copies the PR link ("PR link copied") | — | — | S |
| PR detail panel (right panel of the page) | `PullRequestDetailPanel.tsx`, `PullRequestDetailLayout.tsx`, `pullRequestDetail.logic.ts`, `PullRequestGhosts.tsx` | Missing | Whole panel: header, branches, file count, tabs, collapse/maximize, back to the thread's PRs | `pullRequests.detail`, `pullRequests.invalidate` | button shows "not implemented" (`pull_requests.rs:454`, `:694`) | L |
| Detail actions | `PullRequestDetailPanel.tsx`, `usePullRequestActions.ts` | Missing | Merge (method resolver, project default), close, reopen, ready, draft, delete-branch, update-branch, auto-merge on/off, revert, approve-workflows, success/failure toasts | `pullRequests.runAction`, `pullRequests.detail`, `pullRequests.stack` | — | M |
| Title / description editing | `PullRequestEditButton.tsx`, `pullRequestEditing.logic.ts`, `PullRequestMarkdownEditor.tsx`, Summary tab | Missing | Edit title, edit description | `pullRequests.update` | — | M |
| Summary tab | `PullRequestSummaryTab.tsx`, `PullRequestMarkdown.tsx`, `pullRequestMarkdown.logic.ts`, `PullRequestCommentBody.tsx`, `PullRequestActivityUnavailableState.tsx` | Missing | Description, checks section, review summaries, comment list with filter and order, "Fix in a thread" | `pullRequests.detail`, `pullRequests.activity`, `pullRequests.updateComment`, `pullRequests.update` | — | L |
| Timeline tab | `PullRequestTimelineTab.tsx` | Missing | Events, commits, comment editing, "Open activity on host" | `pullRequests.activity`, `pullRequests.updateComment` | — | M |
| Code tab + review annotations | `PullRequestCodeTab.tsx`, `pullRequestDiff.logic.ts`, `pullRequestFileOrder.logic.ts`, `PullRequestReviewAnnotation.tsx`, `pullRequestReviewStore.ts`, `client-runtime/state/pullRequestDiffHttp.ts` | Missing | PR diff, file order, inline review threads, reply, resolve/unresolve, draft review comments, comment editing | `pullRequests.detail` (diff), `pullRequests.diffFileContents`, `pullRequests.threadComments`, `pullRequests.replyToThread`, `pullRequests.setThreadResolution`, `pullRequests.updateComment` | — (the Diff panel renderer could be reused) | L |
| Files viewed | `usePullRequestFilesViewed.ts`, `pullRequestFilesViewed.logic.ts` | Missing | Viewed checkboxes synced with the host | `pullRequests.filesViewed`, `pullRequests.setFilesViewed` | — | S |
| Review / comment forms | `PullRequestReviewForm.tsx`, `PullRequestCommentForm.tsx`, `PullRequestComposer.tsx`, `PullRequestCopyableCode.tsx` | Missing | Submit review (comment / approve / request changes), top-level comment, PR composer handoff to a thread | `pullRequests.submitReview`, `pullRequests.comment` | — | M |
| Reactions | `PullRequestReactions.tsx`, `pullRequestReactions.logic.ts` | Missing | Reaction bar and picker | `pullRequests.setReaction` | — | S |
| Labels / reviewers pickers | `PullRequestLabelPicker.tsx`, `PullRequestReviewerPicker.tsx`, `PullRequestCandidatePicker.tsx` | Missing | Candidate search, set labels, request reviewers | `pullRequests.labelCandidates`, `pullRequests.setLabels`, `pullRequests.reviewerCandidates`, `pullRequests.requestReviewers` | — | M |
| Checks popover | `PullRequestChecksPopover.tsx` | Missing | Check list with status, opening details | `pullRequests.detail` / `pullRequests.checks` | — | S |
| Stacks | `PullRequestStackMenu.tsx`, `PullRequestStackPopover.tsx`, `PullRequestStackLayers.tsx`, `PullRequestStackHeader.tsx`, `PullRequestStackLayerContent.tsx`, `pullRequestStackSnapshot.ts`, `state/usePullRequestStack.ts` | Missing (sidebar only) | Stack popover on rows, stack menu in the detail (navigate layers, merge the stack), saved-snapshot fallback from thread links | `pullRequests.stack`, `pullRequests.runAction` | `src/ui/thread_pull_requests.rs` picks the stack top for the sidebar glyph only | M |
| Linked threads in detail | `PullRequestThreadLinks.tsx` | Missing | Threads that reference the PR | `pullRequests.linkedThreads` | — | S |
| Checkout from detail / handoffs | `PullRequestDetailPanel.tsx` (checkout menu), `usePullRequestActions.ts` (`usePullRequestHandoffs`) | Missing | "Check out this pull request" (In a separate worktree / In this repository), "Resolve in a thread", "Ask in a thread", "Fix" | `git.preparePullRequestThread`, `orchestration` new-thread commands | — | M |
| Review guide tab | `PullRequestGuideTab.tsx`, `guide/*` (GuideTour, GuideStop, GuideExcerpt, GuideRail, GuideNotes, useReviewGuideWriter, guideRunStore, PullRequestGuideRunCoordinator) | Missing | AI review guide (gated by a setting). Note: `generateReviewGuide` runs a provider turn, so it cannot be checked automatically | `pullRequests.reviewGuide`, `pullRequests.generateReviewGuide`, `pullRequests.diffFileContents` | — | L |
| Thread "Pull request" surface | `ChatView.tsx` (renders `PullRequestDetailPanel`), `RightPanelTabs.tsx`, `hooks/useOpenPanelPullRequestUrl.ts`, `lib/openPullRequestLink.ts` (`useOpenPrLink`) | Missing | Surface always unavailable; PR links open the browser instead of the panel | `pullRequests.detail` | `src/ui/surfaces.rs:98` | M (after the detail panel) |
| Thread "Linked pull requests" surface | `ThreadPullRequestsPanel.tsx`, `pullRequestListLines.ts` | Missing | Linked PR list grouped by stack chain, source labels, watch / stop watching, unlink / dismiss, copy link, speed actions, "Link pull request" | orchestration commands `thread.pull-request.link`, `thread.pull-request.unlink`, `thread.pull-request.watch`; `pullRequests.runAction` | `src/ui/surfaces.rs:98` (unavailable) | M |
| Link PR dialog + palette items | `LinkPullRequestDialog.tsx`, `hooks/usePullRequestLinking.ts`, `CommandPalette.tsx` ("Link pull request to thread", "Show linked pull requests") | Missing | Dialog (URL or `#123`), both palette entries (absent, not even a "not implemented" item) | `thread.pull-request.link` (or legacy single-link mode) | — | S |
| Thread details PR rows | `chat/ThreadDetailsPrRows.tsx`, `chat/ThreadDetailsPrRow.tsx`, `BranchToolbarBranchSelector.tsx` (panel mode) | Missing | Current PR row with checks, the ranked action slot (Resolve / Ready / Fix / Merge), stop watching, "Show N more" | `pullRequests.detail`, `pullRequests.checks`, `pullRequests.runAction`, `thread.pull-request.watch` | `src/ui/thread_details.rs` (doc lists it as not ported) | M |
| Checkout PR from the ref picker | `PullRequestThreadDialog.tsx`, `BranchToolbarBranchSelector.tsx` | Missing (README lists it) | "Checkout pull request" item, dialog with Local / Worktree | `git.resolvePullRequest`, `git.preparePullRequestThread` | `src/ui/branch_picker.rs` (doc) | M |
| PR link hover preview | `PullRequestLinkPreview.tsx` (used by `ChatMarkdown.tsx`, `contextChipParts.tsx`) | Missing | Hover card on PR links in messages | `pullRequests.preview`, `pullRequests.detail` | — | S |
| PR context chip details | `PullRequestContextDetails.tsx` (via `contextChipParts.tsx`) | Missing | Chip popover: "Pull request #N", state, title, head → base | — (metadata on the context record) | `src/ui/markdown.rs` draws the chip only | S |
| Composer "#" references | `chat/ChatComposer.tsx`, `packages/shared/src/composerPullRequestMatches.ts` | Partial (README lists it) | Exact-number lookup past the 99 listed | `pullRequests.detail` | `src/ui/command_menu.rs` | S |
| Sidebar current-PR glyph | `ThreadStatusIndicators.tsx`, `state/pullRequests.ts` (`useSharedPullRequestSummary`), `packages/shared/src/threadPullRequests.ts` | Done (polling) | Refresh is a 1-minute poll, not `subscribeRefreshes`; no merge of newer list/link observations | `pullRequests.summary`, `pullRequests.subscribeRefreshes` | `src/ui/thread_pull_requests.rs`, `src/ui/sidebar.rs` | S |
| Git quick action + menu | `GitActionsControl.tsx`, `GitActionsControl.logic.ts` (`resolveQuickAction`, `buildMenuItems`) | Done | — | `subscribeVcsStatus` | `src/ui/git_actions.rs` | — |
| Git action progress / result | `GitActionsControl.tsx` (`GitActionProgressButtonContent`, `GitActionSuccessButtonContent`), `.logic.ts` (`resolveGitActionProgressPresentation`, `resolveGitActionResultToastTiming`) | Partial (v0.0.45 behavior) | v0.1.0 shows progress inside the button (status, last hook line, elapsed time) with no loading toast; result toasts stay until seen 10 s (`timeout: 0`, `dismissAfterVisibleMs`); panel mode shows an inline success | `git.runStackedAction` | `git_actions.rs` (`refresh_git_toast`, loading toast) | S |
| Status refresh | `GitActionsControl.tsx` (`requestVcsStatusRefresh`) | Missing | Refresh on menu open and on window focus/visibility (250 ms debounce) | `vcs.refreshStatus` | — | S |
| Commit dialog, default-branch confirm | `GitActionsControl.tsx` | Done | Draft threads do not send `projectId` with the action (upstream links drafts by project) | `git.runStackedAction` | `git_actions.rs` `commit_dialog`, `default_branch_dialog` | S |
| Pull | `GitActionsControl.tsx`, `state/sourceControlActions.ts` | Done | — | `vcs.pull` | `git_actions.rs` `git_pull` | — |
| Initialize Git | `GitActionsControl.tsx` | Done | — | `vcs.init` | header | — |
| Publish repository | `GitActionsControl.tsx` (`PublishRepositoryDialog`) | Done | — | `sourceControl.publishRepository`, `server.discoverSourceControl` | `src/ui/publish.rs` | — |
| Git controls in thread details (v0.1.0 placement) | `chat/ThreadDetailsPanel.tsx` (GitActionsControl `displayMode="panel"`, BranchToolbar panel, "Changes +N −M" row) | Missing (README lists it) | Panel-mode git row, inline success, the Changes row opening the diff | `subscribeVcsStatus`, `git.runStackedAction` | header keeps them | M |
| Source-control write scopes | `GitActionsControl.tsx`, `ProjectScriptsControl.tsx`, `projectScriptEditor.tsx` (`AuthSourceControlWriteScope`, terminal scopes) | Missing | "This connection cannot change source control." and the scripts/editor scope messages for limited pairings | — | — | S |
| Project actions control | `ProjectScriptsControl.tsx` | Partial | Running an action by its shortcut (`script.<id>.run` is not dispatched), `autoOpenPreview` (opening `previewUrl` after running), a new terminal when the active one is busy, `projectScriptRuntimeEnv` variables, the "Pair this client again…" permission note | `terminal.open`/`terminal.write`, `preview.open` | `src/ui/project_actions.rs`, `src/ui/shortcuts.rs` | S–M |
| Add/Edit Action dialog | `projectScriptEditor.tsx` | Done | Scope-based validation messages only | `server.upsertKeybinding` / `server.removeKeybinding`, settings patch | `src/ui/action_editor.rs` | — |
| Import t3.json scripts | `ProjectScriptsControl.tsx`, `settings/ProjectActionsSettings.tsx` | Done | — | `projects.readFile` | `src/ui/script_import.rs` | — |

## 2. Details

### Done (short)

- List core: `pullRequests.list`, involvement groups (Authored / Review requested / Others), all seven
  sorts with the merge-readiness and blocked-on-me tiers, state/involvement/draft/review/checks/project
  filters, rows (state glyph, conflict badge, number, title, checks and review glyphs, line counts when
  present, author initial, repository, labels, update time, provider icon with several hosts), ghost,
  empty and error states.
- Git actions: `resolveQuickAction`, `buildMenuItems`, disabled reasons, detached-HEAD and
  behind-upstream notes, the commit dialog (files, Edit, excluded, Commit on new branch), the
  default-branch confirmation, `git.runStackedAction` streaming with `progress_stages`, the result toast
  and its CTA, `vcs.pull`, `vcs.init`, Publish repository wizard.
- Project actions: preferred action, menu with shortcuts and edit, Add/Edit Action dialog (icons,
  keybinding capture and RPCs, worktree options, preview URL, delete confirmation), t3.json import.
- Sidebar current-PR glyph and the composer "#" reference picker.

### Partial: what exactly is missing

**Search** (`pull_requests.rs` `matches_query`). The port does a substring test over loaded rows.
Upstream sends the free text to the hosts (`query`, debounced 250 ms), so it finds PRs outside the
first page and by description. It parses qualifiers into `filters`: `label:a,b` (OR), `-label:x`,
`author:`, `draft:true|false`, `review:`, `checks:`/`status:`, and unknown `key:v` as namespaced
labels. It ranks results with `scorePullRequestMatch` (number 100, exact title 90, ..., matched
elsewhere 10) and marks the score-10 rows "Matched in the description". The port's placeholder
"Search pull requests, or label:bug" invites the qualifier syntax, but `label:bug` matches nothing.
Its empty state "The hosts were searched for it" is also wrong, because nothing is sent to the hosts.

**Involvement.** The port's `input()` does not send `involvement`. It filters `viewerReviewRequested`
and author on one 99-row page. Upstream sends `involvement` to the hosts and runs separate authored
and reviewing partition queries, plus baseline and facet queries. With many open PRs, the port's
Reviewing and Authored tabs can come up empty.

**Filters menu.** Missing:
- the Author section (search box, facet list, "No authors found");
- the Labels section (facets from the rows, colors, "No labels in this view");
- the project items' "· Unavailable" marking with the per-project error as tooltip (from
  `result.errors`, which the port ignores);
- persisting the list preferences across restarts (`pullRequestListPreferences.ts`; the port keeps
  them in memory only).

**Host menu.** Every host item gets the `sc-github` icon (`pull_requests.rs:1081`), so GitLab,
Bitbucket, Azure and Forgejo hosts show GitHub's mark. Hosts that need sign-in are not flagged.

**Rows.** Missing:
- avatars (initials are drawn);
- `PullRequestChecksPopover` on the checks glyph;
- `PullRequestStackPopover` (stack badge);
- the right-click menu (`pullRequestLinkContextMenu.ts`: Copy link, Open on <host>);
- selection, since clicking only sets a "not implemented" status;
- hover speed actions.

**Refresh.** The button re-sends `pullRequests.list` but never calls `pullRequests.invalidate`, so
the server cache can answer with the same stale data.

**Git action progress (v0.1.0 change).** The port shows a loading toast ("Waiting for Git...") with
its elapsed time. That is v0.0.45 behavior. v0.1.0 has no loading toast. The quick-action button
itself turns into a status row: spinner, phase label, last hook output line with tooltip, elapsed
`Ns` / `Nm Ns`. Result toasts use `timeout: 0` with `dismissAfterVisibleMs: 10000`, and in panel mode
an inline success row replaces the toast. Upstream also calls `vcs.refreshStatus` when the menu opens
and on window focus or visibility change, debounced 250 ms. The port never calls it.

**Project actions.**
- Saved shortcuts are written (`server.upsertKeybinding`), but `src/ui/shortcuts.rs::run_command`
  has no `script.<id>.run` branch, so pressing an action's shortcut does nothing.
- `autoOpenPreview` / `previewUrl` are edited and saved but never used when running. Upstream opens
  the preview after writing the command (`ChatView.tsx` `runProjectScript`).
- No new terminal when the active one is busy, and no `projectScriptRuntimeEnv` (README lists both).

**Composer "#".** The `pullRequests.detail` lookup for a number outside the 99 listed is missing
(README lists it).

**Sidebar glyph.** It polls `pullRequests.summary` every minute instead of following
`pullRequests.subscribeRefreshes`. It also does not merge newer observations from the list and the
links (`newestPullRequestObservation`). The visible result is equivalent.

### Missing

**PR detail panel**, the largest gap (about 9k upstream lines with its tabs). It needs
`pullRequests.detail` and then, tab by tab:
- **Header:** state, number, editable title (`pullRequests.update`), head ← base branches, file
  count, stack menu (`pullRequests.stack`), checkout menu (`git.preparePullRequestThread`, local or
  worktree), and the actions (`pullRequests.runAction`: merge with a method resolver from project
  settings, close, reopen, ready, draft, delete-branch, update-branch, enable/disable auto-merge,
  revert, approve-workflows). Each action has its success/failure toast and hint.
- **Summary:** description (rendered markdown, editable), checks section, review summaries, comments
  with filter and order (`pullRequests.activity`, `pullRequests.updateComment`), reactions
  (`pullRequests.setReaction`), "Fix in a thread" handoffs, the comment form
  (`pullRequests.comment`) and the review form (`pullRequests.submitReview`).
- **Timeline:** `pullRequests.activity` events and commits.
- **Code:** the PR diff (`pullRequests.diffFileContents`, `client-runtime/state/pullRequestDiffHttp.ts`),
  file ordering, viewed files (`pullRequests.filesViewed` / `setFilesViewed`), and inline review
  threads (`pullRequests.threadComments`, `replyToThread`, `setThreadResolution`).
- **Guide:** `pullRequests.reviewGuide` / `generateReviewGuide`. Generating runs a provider turn, so
  it must stay out of automated checks.
- **Pickers:** labels (`labelCandidates` / `setLabels`) and reviewers (`reviewerCandidates` /
  `requestReviewers`).
- **Linked threads:** `pullRequests.linkedThreads`.

The same panel is also the thread's "Pull request" surface, and `useOpenPrLink` routes PR links
into it. In the port, `surfaces.rs` always reports both PR surfaces unavailable.

**Linked pull requests surface** (`ThreadPullRequestsPanel.tsx`). It lists the thread's links,
grouped into stack chains, with source labels ("Linked by you", "Created from this thread", "Linked
by the agent", "Found in the stack", "Dismissed"), last-synced time, and speed actions. Per link it
offers "Watch for changes" / "Stop watching", "Unlink from thread" / "Dismiss from thread" and Copy
link, plus "Link pull request". It needs the orchestration commands `thread.pull-request.link`,
`thread.pull-request.unlink` and `thread.pull-request.watch`. The port already ingests the
`thread.pull-request-link-updated` and `thread.pull-request-synced` events, so the data is there.

**Link PR dialog and palette.** "Link pull request to thread" and "Show linked pull requests" are
absent from the port's command palette. They are not even placeholder items.

**Stacks.** `pullRequests.stack` plus the saved-snapshot fallback (`pullRequestStackSnapshot.ts`,
`usePullRequestStack.ts`) feed the row stack popover and the detail stack menu. Merging is refused
outside the stack menu when a stack exists (client-runtime `runAction` prep).

**Speed actions and close sweep.** Row-hover Close / Merge / Ready for review / Reopen, a
pointer-drag "close sweep" across rows, and optimistic list overrides while the host catches up.

**Paging, stats, live refresh, snapshot.** "Load more pull requests" (cursors, 99 → 500), visible-row
`pullRequests.listStats` batches, refresh on focus, every minute and after turns
(`pullRequests.subscribeRefreshes`), and the stored list snapshot.

**Thread details PR rows and git panel mode.** v0.1.0 moved branch, git and scripts into the thread
details card. That brings the PR row (checks, ranked action slot Resolve / Ready / Fix / Merge, stop
watching, "Show N more"), the panel-mode git control with inline progress and success, and the
"Changes +N −M" row. The README lists the placement as not ported.

**Checkout PR** (`PullRequestThreadDialog.tsx`: `git.resolvePullRequest`, then
`git.preparePullRequestThread` with Local or Worktree), from the ref picker's typed reference and the
detail panel.

**PR link hover preview** (`pullRequests.preview`) and the **PR context chip popover**
(`PullRequestContextDetails`).

**Page shortcuts.** Escape back, the right-panel toggle/close/maximize, and `thread.copyReference`
("PR link copied").

## 3. README and module-doc claims checked

- **README "Header" row** says git actions run as "a `git.runStackedAction` stream whose progress
  events … drive a loading toast". That is v0.0.45 behavior. v0.1.0 has no loading toast and shows
  progress in the button. The row reads as v0.1.0 parity but is not.
- **README "Settings" row**, project actions "shortcut written with the keybinding RPCs": true, but
  the shortcut is never dispatched (`shortcuts.rs` has no `script.*` handling). The "preview URL"
  field is saved but not honored when running (`autoOpenPreview`). Neither gap appears in the README
  gap list, which only names the busy-terminal and env-variable gaps.
- **README "Pull requests" row**, "search": the search only filters loaded rows (the gap list says
  so). But the UI text overclaims: the placeholder suggests `label:bug`, which does not work, and the
  empty state says "The hosts were searched for it", which is false.
- **README "Pull requests" row**, "provider menus": the host menu shows the GitHub icon for every host.
- **README "Pull requests" row**, "grouped by involvement": the grouping is right, but involvement is
  not sent to the hosts as v0.1.0 does, so the tabs work on one page of rows. Not mentioned.
- **The README never mentions** the Linked pull requests surface, the Link PR dialog and palette
  entries, PR link previews, the PR context chip popover, speed actions, stacks on the list page, or
  `vcs.refreshStatus`. They fall loosely under "the pull request detail and right panels" and "the
  surfaces other than … (pull requests …)", but the palette items and link previews are outside both.
- **`src/ui/git_actions.rs` module doc** still says "Publish repository … is not ported". It is
  ported in `src/ui/publish.rs`, and the README says so. The doc is stale.
- **`src/ui/pull_requests.rs` module doc** is accurate. It lists the detail and right panels, host
  search, author/label filters, `listStats`, paging, stacks, avatars and the condensed topbar.
