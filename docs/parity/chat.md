# Chat view parity: T3 Code v0.1.0 web client vs Pluto (egui-client)

Scope: `components/chat/*`, ChatView*, ChatMarkdown, CommandBlock, Composer*/composer*, ContextChip,
contextChipParts, contextPresentation*, ComposerContextLabel, BranchPicker, BranchToolbar*,
PreviousWorktreeItemContent, promptLinks/, media/, ConfirmDialogHost, CustomSnoozeDialog,
NoActiveThreadState, plus the state/lib helpers they use.
Upstream: the v0.1.0 client sources. Port: the repository (paths below are relative to it).
Ignored: Pluto additions (Chat mode, Kimi/Vibe/Z drivers, mobile), ACP registry, multi-environment
selectors (BranchToolbarEnvironmentSelector: the port connects to one environment on purpose),
CommandBlock.tsx (used only by onboarding, outside the chat view).

Since this report: drafts are kept per thread and server (src/ui/drafts.rs), a refused or
lost message comes back with its attachments, the thread error banner is drawn
(src/ui/thread_error.rs), `timestampFormat` and `sendShortcut` apply, and a subset of raw HTML
renders (src/ui/md_html.rs). Still read only by src/ui/settings.rs: `composerCollapseOnScroll`,
`proactivePanelsEnabled` and "Rich text composer"; no revertCheckpoint, minimap or scroll-to-end.

## 1. Done (brief)

Timeline and markdown
- Virtualized rows with anchored scroll; "Load earlier turns"; "Worked for…" turn folds; superseded-attempt fold; compaction dividers; Thinking and live-work rows; Working timer (MessagesTimeline*, WorkLog, AgentElapsed, TimelineSystemDivider): chat.rs, timeline_view.rs, core timeline.rs, live_activity.rs
- Lifecycle dividers (Run interrupted, Context handoff, fork pill, subagent cards, "N subagents") (V2LifecycleRow): lifecycle.rs
- Changed-files card, Open diff, expand/collapse folders (ChangedFilesTree, DiffStatLabel): changed_files.rs
- Proposed plan card: badge, fold, Copy/Download/Save to workspace; Refine/Implement; Implement in new thread (ProposedPlanCard, ComposerPlanFollowUpBanner): plan_card.rs
- html_render rows, MCP app rows (HtmlRenderFrame, McpAppFrame): html_render.rs, mcp_apps.rs
- Citations: Cite/Shorten selection toolbar, chips, comment editor, source pulse (AssistantCitation*, AssistantSelectionToolbar): cite.rs, citations.rs
- Find in thread (threadFind*, ThreadFindBar, useThreadFind*): thread_find.rs
- Lightbox with zoom/pan/arrows/keys, image MediaActions menu, tool output images (ExpandedImageDialog, ZoomableImage, media/MediaActions): lightbox.rs, media.rs, tool_images.rs
- MessageCopyButton, markdown-as-clipboard, table "Copy as Markdown / CSV", file chips, favicons, `$skill` chips, "(empty response)": md_copy.rs, markdown.rs, md_links.rs
- Inspector for commands and dynamic tools (V2ItemInspector, part): inspector.rs
- Fork from a response (`thread.fork`), workspace-preparation Retry: chat.rs

Composer
- Suggestion menu: `/model` `/plan` `/default`, provider commands/skills, `@` files/folders, `$` skills, `#` PRs, ranking, keys (ComposerCommandMenu, composerSlashCommandSearch, composerMenuHighlight, useComposerTriggerState): command_menu.rs
- Mention/skill/citation/terminal-excerpt chips with popovers, PR review-comment chip (ContextChip, contextChipParts, TerminalContextInlineChip): cite.rs, mentions.rs, core context.rs
- Prompt recall ArrowUp/Down (composerPromptHistory): prompt_history.rs
- Approval panel (4 decisions, "More approval options"), question card (multi-select, prev/next, dismiss) (ComposerPendingApproval*, ComposerPendingUserInputPanel): requests_panel.rs, core requests.rs
- Tasks badge and drawer (ComposerTasksBadge): tasks.rs
- Send/Stop/Queue/Steer/Resume/Update-queued, Ctrl flip (ComposerPrimaryActions): composer.rs `send_actions`
- Footer fit: labels drop, then the "More composer controls" menu (CompactComposerControlsMenu, composerFooterLayout): composer.rs
- Model picker: rail, favorites, legacy models, New tag, search, setup notice (ModelPicker*, ModelListRow, ProviderModelPicker): pickers.rs
- Image/file attachments: paste, drop, picker, size/count limits, `attachments.createUploadUrl` (composerAttachmentFiles, attachmentUploadQueue): attachments.rs
- Queued runs: reorder by drag, edit, steer, remove, collapse (QueuedRunsControl, queuedMessageEdit): queue.rs

Chat view shell
- Send flow (`message.dispatch`/launch, `assets.persistChatAttachments`, mode commands first, base-branch guard); Stop (`run.interrupt` + holdQueue); Resume (ChatView onSend/onInterrupt/onResume): composer.rs, core commands.rs, queue.rs
- Approvals and questions answered through `runtime-request.respond`: requests_panel.rs
- Draft hero, project picker, "or start without a project"; workspace mode menu incl. Previous worktree (DraftHeroHeadline, BranchToolbarEnvModeSelector, PreviousWorktreeItemContent, BranchToolbar.logic): chat.rs `draft_hero`, composer.rs
- Ref picker: search, paging, badges, switch/create ref, Start from origin, copy name (BranchPicker, BranchToolbarBranchSelector): branch_picker.rs
- Panel toggles (PanelLayoutControls): diff_panel.rs; Open in split button + Ctrl+O (OpenInPicker*): open_in.rs
- Thread details: automations, lineage, Merge back, stop subagent/workflow (ThreadDetails*, ThreadRelationshipsControl, ThreadAutomationsPanel): thread_details.rs
- Confirm dialog, custom snooze dialog (ConfirmDialogHost, CustomSnoozeDialog): thread_menu.rs, snooze_dialog.rs
- Shortcuts: chat.find, terminal/rightPanel/diff/threadPanel toggles, settle/pin, steer/edit queued, thread jumps: shortcuts.rs

## 2. Partial and missing

### Timeline, messages and markdown

| Feature | Upstream file(s) | Status | What is missing | Port location | Size |
|---|---|---|---|---|---|
| Edit from here (rewind) | MessagesTimeline `RevertUserMessageButton`, ChatView `onRevertToTurnCount` | done | The message's attachments do not return to the composer with its prompt | src/ui/rewind.rs, chat.rs `user_row` | - |
| Checkpoint rollback from inspector | V2ItemInspector, ChatView `onRollbackCheckpoint` | done | Roll back on an expanded checkpoint row, with its confirmation | chat.rs | - |
| Timeline minimap | MessagesTimeline `TimelineMinimap`, timelineMinimapItems.ts, timelineMinimapWheel.ts | done | Turn strip, hover preview, Previous/Next, wheel and scrub (`minimap.rs`) | minimap.rs | - |
| Scroll-to-end pill, page scroll | ChatView (pill), timelineScrollTarget.ts, pageScrollController.ts, ChatView `handleKeyDown` | done | Pill and page keys; position memory per thread is not ported | chat.rs `timeline` | - |
| Worktree setup card | chat/WorktreeSetupCard.tsx, `BackgroundWorktreeSetupChip`, ChatView `onCancelWorktreeSetup`/`onOpenWorktreeSetupTerminal`/`onWorktreeSetupWorkLocally` | partial | Card, stream, row rules, "Setting up worktree…", send block, Details, Open terminal and Cancel done (`worktree_setup.rs`); Work locally and the background chip with its popover missing | worktree_setup.rs | M |
| Secret request card | chat/SecretRequestCard.tsx (`answerSecretRequest`) | done | Masked field, Save securely, Decline, errors (`secret_request.rs`) | secret_request.rs | - |
| Thread status line | chat/ThreadStatusLine.tsx | done | Wake now, Un-settle, Woke · Dismiss under the last message | chat.rs | - |
| Subagent threads | MessagesTimeline `listHeader`, chat/ProviderSubagentBar.tsx, SubagentTooltipContent.tsx | partial | Header and ProviderSubagentBar done; the subagent hover card is missing | lifecycle.rs | S |
| User bubble controls | `UserTimelineRow`, `UserMessageIntentMarker`, `AssistantMessageMeta`, promptLinks/copyPromptLink.ts | partial | Attribution, From queue / Steer and failed pills done; Copy prompt link missing (needs prompt links) | chat.rs `user_row` | S |
| User bubble files/videos | `UserTimelineRow`, `UserVideoAttachment`, SnapShotAttachmentDetails | partial | Files are Pluto chips. Missing: Preview (eye) / "Download <name>" row, unknown-attachment rows, video thumbnail and player, SnapShot details | chat.rs `user_row` | M |
| User bubble context chips | `UserMessage*Chip`, `UserMessageElementDetails`, `UserMessagePreviewAnnotationDetails`, `UserMessageReviewCommentCard`, `UnavailableUserMessageContextChip` | partial | Only terminal chips react to a click. Missing: image expand, file open, thread open, element-details popover, preview-annotation popover with screenshot, review-comment card, PR context details, unavailable state, context-fragment clipboard flavor | markdown.rs `opened_context`, chat.rs:1536 | M |
| Timestamps | `TimelineRowTimestamp`, `formatDayAwareTimestamp`, `formatChatTimestampTooltip` | done | Full-date tooltips, hover times on work rows and folds | widgets.rs | - |
| Work rows: thread actions | `WorkEntryLogRow`, V2LifecycleRow `thread_created` | done | Open chat, Open subagent, created-thread cards under the answer, subagent notifications as cards, thread-read labels | chat.rs, lifecycle.rs | - |
| Work rows: icons | `ToolActivityIconView`, `workEntryIconName`, `resolveWorkEntryToolPresentation` | partial | `workEntryIconName`, T3 tool labels and summaries, group icons done (`tool_presentation.rs`). Missing: favicon / themed logo / native-app image icons, "Used X integration" summaries, workspace-relative read paths | core tool_presentation.rs, timeline.rs | M |
| Work rows: reasoning, answers | `ReasoningTraceContent`, `QuestionAnswerHistory` | partial | Expanded thought is plain mono text clipped at 256 px (upstream: streaming ChatMarkdown); answered questions lose attachments/thumbnails | chat.rs ~2011 | S/M |
| Inspector coverage | V2ItemInspector.tsx | partial | No panels for reasoning, file_change, file/web search, approval/input requests, notifications, todo, checkpoints, fork, handoff, subagent; no ShellCommandBlock or JSON token colours | inspector.rs | M |
| Markdown: raw HTML | shared markdownPipeline (rehype-raw + sanitize), `MarkdownDetails` | partial | The GitHub-sanitized subset renders (`<br>`, `<b>`, `<i>`, `<s>`, `<code>`, `<kbd>`, `<a href>`, `<img>`, `<details>`, `<summary>`); other tags keep their text | md_html.rs | M |
| Markdown: GFM extras | remark-gfm, markdownGithubAlerts | done | Autolinks (URLs and e-mail), footnotes with their section and back-references, GitHub alerts | markdown.rs | - |
| Markdown: links to threads/PRs/headings | chat/MarkdownThreadLink.tsx, ChatMarkdown link renderer, `resolvePullRequestPreviewTarget`, `rehypeHeadingIds` | partial | Thread links and heading anchors (fragment scroll) done; PR preview and Link/Unlink wait for the PR panel (milestone 3) | markdown.rs | M |
| Markdown: Codex directives | shared codexMarkdownDirectives, `CodexArtifactTemplateCard` | missing | `::artifact-template` card ("Use template"), `codex-file-citation`, `codex-followup` | none | M |
| Markdown: code blocks | `MarkdownCodeBlock`, chat/MermaidDiagram.tsx | partial | Copy, Wrap lines, Run in terminal, fence titles done; Mermaid missing | markdown.rs | L |
| Markdown: tables | `MarkdownTable` | done | Expand / Collapse cells, 384px cap, sideways scroll | markdown.rs | - |
| Link menus and target | chat/externalLinkContextMenu.ts, `MarkdownFileLink`, `browserLinkTarget` | partial | External links: only "Copy Link" (no "Open in integrated/system browser"), clicks ignore `browserLinkTarget`. File chips: no "File options" menu (Open in editor, Open HTML in browser, Reveal, Copy relative/full path, Preview media); `file:`/Windows paths not handled | md_links.rs, text_menu.rs | M |
| Video playback | media/MediaVideoPlayer.tsx, media/OpenMediaLink.tsx, `ChatMarkdownVideo` | missing | Video in markdown, user attachments and lightbox (README admits it) | lightbox.rs | L |
| HTML render full screen | chat/HtmlRenderDialog.tsx | partial (deliberate) | No full-screen dialog with zoom controls; replaced by "Open in Design" | html_render.rs | S |
| MCP app extras | chat/McpAppFrame.tsx | partial | No full screen, downloads refused, theme variables not passed (README notes) | mcp_apps.rs | M |
| Citation stale quote | AssistantCitationSource.tsx | partial | No "The quoted text has changed" / "Showing the source response…" notice | cite.rs ~690 | S |

### Composer

| Feature | Upstream file(s) | Status | What is missing | Port location | Size |
|---|---|---|---|---|---|
| Per-thread composer drafts | composerDraftStore (throughout ChatComposer), lib/discardComposerDraft, lib/composerDraftUploads | done | Per thread and per new-thread session, saved for the next run (`drafts.rs`) | drafts.rs | - |
| Rich-text editor | ComposerPromptEditorTiptap, ComposerCodeBlockNodeView, ComposerCodeBlockLanguagePicker, composerCodeBlockHighlight, composerContextUndo | missing | Plain `TextEdit`: no bold/italic/strike/inline code, no fenced code blocks with "Search languages" picker and highlighting, no list continuation, no Tab indent, no undo grouping, no undo restoring a deleted chip payload. The "Rich text composer" setting is read nowhere | composer.rs; settings.rs:825 | L |
| Prompt stash | ComposerStashBadge, ComposerStashMenu | done | Stash badge, menu, Mod+S (`stash.rs`) | stash.rs | - |
| Send shortcut setting | ChatComposer `onComposerCommandKey` (`sendShortcut`) | partial | Enter and Ctrl+Enter always send; `mod-enter` / `mod-enter-multiline` have no effect | composer.rs ~430 | S |
| Placeholders | ChatComposer ~7458-7473, composerPlaceholder | partial | Default should be "Ask anything, @tag files/folders, $use skills, or / for commands"; the port shows upstream's disconnected text. Missing "Choose an option above", "Choose a project above to start a thread", "Enable a provider in Settings to send a message" | composer.rs | S |
| Resting (collapsed) composer | composerScrollGesture, restingComposerControlsMeasurement, composerFooterLayout `shouldUseRestingComposerLayout`, ChatView `onComposerRestingChange` | missing | Collapse to one line on timeline scroll, lift on interaction; `composerCollapseOnScroll` setting unread | settings.rs:833 only | M |
| Banner stack and notices | ComposerBanner, ComposerBannerStack, ChatView `composerBannerItems`/`systemComposerBannerItems`, ComposerServerUpdateStatus | done | Stack with priority and peek; offline/reconnecting, server update, branch changed, child waiting, goal, background work, project clone, usage limit | composer_notices.rs | - |
| Thread error banner | chat/ThreadErrorBanner.tsx, ChatView `threadError` | done | Drawn by src/ui/thread_error.rs | thread_error.rs | — |
| Provider status banner | chat/ProviderStatusBanner.tsx | done | Above the thread error, dismissible by key | thread_error.rs | - |
| Usage-limit banners | UsageLimitRecoveryBanner, ComposerUsageLimits | done | Resume at reset, Cancel auto-resume, Snooze until reset | composer_notices.rs | - |
| Provider update and feedback banners | useAutoBalanceUpdateBanner, ComposerFeedback | missing | "Update available for … / Update all / Manual update required"; Codex feedback thread-id notice | none | M / S |
| Sync activity row | ComposerActivityStatus | missing | "Loading messages..." / "Syncing messages..." | none | S |
| Send button states | ComposerPrimaryActions | partial | Disabled status tooltips, clone block, Compact chip and Compact and send done; no busy spinner | composer.rs `send_actions` | S |
| Failed send recovery | ChatView `onSend` failure branch | partial | A refused or lost message comes back to its composer with its images, files and contexts (drafts.rs `give_back`); no optimistic user row | drafts.rs | S |
| Multi-model fan-out | ChatView `multipleModelSelections`, `fanoutStateAtom`, ModelPickerContent "Choose models" | missing | One thread per selected model, "Started N threads in background", "Each model starts in its own worktree.", "Select at least one model." | none | L |
| Background send | composer-logic `composer.sendBackground`/`sendAndNewThread`, ChatView `submissionIntent` | done | Send in background and send-and-new-thread | composer.rs | - |
| Prompt length, large paste | composerSubmission, ComposerPromptLengthValidation | done | Limit alert and send block; large pastes fold into a file | composer.rs | - |
| Paste handling | composerInlineTokenPaste, lib/desktopPasteAsText, composer context clipboard | partial | Citation links round-trip. Missing: copied context chips carrying records/files, paste-as-text bypass, foreign-thread drop rules | cite.rs | M |
| Attachments details | lib/imageCompression, attachmentUploadState, ChatComposer thumbnails | partial | Images over 10 MiB refused instead of downscaled (attachments.rs:648); no inline image chips in the prompt, no video first-frame thumbnails, no upload progress/retry, no "needs reattach", no attachments on question answers, no pending-compression send guard | attachments.rs | M |
| Drag and drop into composer | folderDrop, workspaceFileDrop, composerMentionDrag, threadContextDrag, ThreadContextDragGhost, lib/threadContextAttachment, composerThreadImport | missing | Dropped folders become plain files, not `@` links; no file-tree drag-to-mention (files.rs:12); no sidebar thread drag / thread `.jsonl` attachment; no "Drop files to attach" overlay (dropping files works) | attachments.rs, composer.rs:501 | M |
| `@` threads, `#` linked PRs | ChatComposer `matchComposerThreadItems`, `composerPullRequestsFromLinks` | partial | `@` never lists threads (no thread item or thread chip); `#` ignores thread-linked PRs and exact-number lookup | command_menu.rs | S-M |
| Other context kinds | composerContextPresentation, contextPresentationRegistry, lib/composerContextRecords | partial | Only terminal and PR-reference records. Missing: diff review comments, preview annotations / picked elements, image/file context records, thread records | core context.rs | M |
| Traits picker | TraitsPicker, TraitsSpeed, runtimeModeConfig | partial | Generic descriptor menu only. Missing: Ultrathink prompt-controlled effort (`promptInjectedValues`, disabled when typed), paired speed label ("High Fast") with fast/ultrafast icons, `serviceTier`, `contextWindow` and `agent` sections, the "Effort / Reasoning effort / Agent" headings; traits not folded into the compact "…" menu | composer.rs `traits_label`, pickers.rs | M |
| Model picker extras | ModelPickerContent, ModelPickerSidebar, ProviderInstanceIcon, modelPickerKeys | partial | No keyboard navigation (arrows, Enter, Escape, ArrowLeft/Shift+Tab to rail); no instance initials badge / accent colour on rail; no shortcut in trigger tooltip | pickers.rs | M |
| Request-panel details | ComposerPendingApprovalPanel, ComposerPendingUserInputPanel | done | not_resumable text and gating, choice-only editor, 200 ms advance, Stop beside the actions | requests_panel.rs | - |
| Typing guard | composerTypingGuard, useComposerTypingGuard, pendingDraftWork | done | 1.5 s hold (`typing_guard.rs`) | typing_guard.rs | - |
| Context window meter | ContextWindowMeter(.logic) | partial (redesigned) | Missing "Cost" and "Total processed" rows, compaction-disabled reasons, loading placeholder | context_meter.rs | S |
| Queue extras | QueuedRunsControl | partial | No arrow-key reorder, no rows still in flight, no editing of attachments | queue.rs | S |
| Prompt links | promptLinks/* (copyPromptLink, linkedPrompt, NewThreadLinkView, ThreadLinkPrompt, HostedPromptLinkLanding) | partial | On `pluto://` (decision): Copy prompt link (bubble, palette, with repo and model), receiving through `pluto %u` and a running instance, sanitizing, Replace draft. Missing: cloning the linked repository, PR checkout | prompt_links.rs | M |

### Chat view shell, header, branch toolbar

| Feature | Upstream file(s) | Status | What is missing | Port location | Size |
|---|---|---|---|---|---|
| Header breadcrumb | chat/ChatHeader.tsx | partial | Project crumb is static text, not the "New thread in X" button; no parent crumb for subagents ("Open parent thread: …"); title not a button (no title menu/chevron, no double-click rename, "Thread title cannot be empty"); no header right-click menu (thread menu, or "Project settings" on a draft) | chat.rs `header` (~270-300) | M |
| Keyboard shortcuts | ChatView ~7873-8170 | partial | Not dispatched in `run_command`: thread.copyReference, thread.stop, rightPanel.toggleMaximized, modelPicker.toggle, composer.host/cycleHost/effort/mode/workspace/branch/previousWorktree/stash, `script.<id>.run` (labels shown, keys do nothing); type-to-focus composer missing; terminal.split/splitVertical/new/close only as toolbar buttons | shortcuts.rs:196-330 | M |
| Proactive panels | ChatView.logic `shouldOpenProactiveTurnDiff`, `resolveProactiveTurnDiffAction`, `shouldOpenProactivePullRequest` | missing | Auto-open diff after a turn with >=3 files or >=50 lines; PR panel when a PR is linked; `proactivePanelsEnabled` unread | settings.rs:811 only | M |
| Branch switch confirmation | ChatView `branchRestoreConfirmOpen`, `handleSwitchCheckoutToThread` | missing | "Switch to <branch>?" dialog → `vcs.switchRef` + `thread.metadata.update`, error toasts | none | S |
| Workspace control context menu | BranchToolbar `showWorkspaceContextMenu` | missing | "Copy full path" → "Path copied" | composer.rs | S |
| Branch control PR badge | BranchToolbarBranchSelector, chat/ThreadDetailsPrRow(s) | missing (README admits) | PR badge with checks, Ready/Fix/Merge, "Stop watching #N", "Checkout pull request" item / PullRequestThreadDialog | branch_picker.rs | L |
| Thread details card gaps | ThreadDetailsPanel, chatCanvasLayout, ThreadLineageWorkflowRow | partial (README admits) | Workspace and Version Control sections (branch toolbar, Open in, scripts, git actions in the card); inline placement beside a wide canvas (`DETAILS_CARD_CLEARANCE`); workflow phases, "Stop entire workflow" tooltip; "More thread actions" → "Disconnect agent session" | thread_details.rs | M |
| Empty states | NoActiveThreadState, NoProjectsHero, DraftHeroHeadline | partial | No "Add project" / "Start without a project" buttons, no "Add a project to start your first thread."; "No active thread / Pick a thread to continue" absent; draft hero lacks "Choose a project to start" / "Add a project to start" variants | chat.rs `empty_state`, `draft_hero` | S |
| Snooze clock picker | CustomSnoozeDialog trigger | partial | The dialog is ported, but not the clock button's own picker | snooze_dialog.rs | S |

## 3. README claims absent, wrong or incomplete

- Header row (README line 32), "Breadcrumb": it is static text only (chat.rs ~270-300); the "New thread in X" button, parent crumb, title menu, double-click rename and header context menu are missing and not listed as gaps.
- Composer row (line 35), "Prompt editor": reads as parity, but the "Rich text composer" setting is inert, `sendShortcut` is ignored (Enter and Ctrl+Enter always send), and the default placeholder is upstream's disconnected text.
- Composer: README never says that the composer text and attachments are global and not kept per thread (app.rs:30-34).
- Timeline row (line 33), "hover meta (time, copy)": the time has no full-date tooltip.
- Timeline row, "context references": accurate, but only terminal chips react to a click (chat.rs:1536). The other chip kinds are inert.
- Markdown row (line 34): it does not say raw HTML is drawn as literal text (markdown.rs:421). Bare autolinks, footnotes, GitHub alerts, Mermaid, thread links, PR autolinks, Codex directives, wrap/run-in-terminal buttons and table-cell expansion are missing but not listed.
- Secret requests are described as "standalone work rows with their upstream labels". True, but the README doesn't say they cannot be answered.
- Protocol 2 "Not ported" list (lines ~58-85) is stale in both directions:
  - Lists "non-image files" / "file attachments in user messages", but both are ported (attachments.rs uploads via `attachments.createUploadUrl` and draws chips).
  - `/compact` is listed, but compaction works from the meter's "Compact session" (only the slash item is hidden, command_menu.rs:725).
  - Omits: checkpoint revert from the inspector, ThreadErrorBanner, ThreadStatusLine, minimap, scroll-to-end pill, worktree setup card, the ChatView banners (child input, goal, background work, branch mismatch, clone, offline), the branch switch dialog, multi-model fan-out, background send, failed-send restore, user-row controls (intent pill, attribution, prompt link, status pill), the "Subagent of" header and ProviderSubagentBar.
- Settings that do nothing: "Collapse composer on scroll" (`composerCollapseOnScroll`), proactive panels (`proactivePanelsEnabled`), "Rich text composer", "Send shortcut" are all editable in Settings but read only by settings.rs.
- Keybindings: composer.*, modelPicker.toggle, thread.copyReference, thread.stop, rightPanel.toggleMaximized and `script.*.run` show on the Keybindings page but are not dispatched. The shortcuts.rs doc mentions the composer controls only.
- Stale module docs, not README:
  - crates/pluto-core/src/timeline.rs:14-15 says MCP apps are not ported, but they are (`Row::McpApp`, mcp_apps.rs).
  - src/ui/terminal_surface.rs:176-177 says "Add to chat is not ported", but it is.
  - src/ui/surfaces.rs says only the Diff surface is rendered there.
