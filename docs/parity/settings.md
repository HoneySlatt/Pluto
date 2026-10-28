# Settings and account surfaces: parity inventory

Upstream: T3 Code v0.1.0 web client (the v0.1.0 client sources (extracted from the package's source maps)). Port: Pluto
egui client (the repository, branch `egui-client`).
Audit date 2026-10-29. Read-only audit. Pluto's own additions (Chat mode,
Personalization, Kimi/Vibe/Z Code) and the ACP registry are excluded.

"Electron" in the "What is missing" column marks a feature whose upstream
depends on the desktop shell; a Linux-native equivalent is suggested where one
makes sense.

## 1. Feature table

### Settings shell and infrastructure

| Feature | Upstream file(s) | Port status | What is missing | Port location | Size |
|---|---|---|---|---|---|
| Sidebar nav, breadcrumb, page list | SettingsSidebarNav.tsx, SettingsBreadcrumb.tsx, routes/settings.tsx | done | Diagnostics and Licenses have no pages, so they have no breadcrumb either | src/ui/settings.rs `settings_sidebar`, `settings_page_view` | – |
| Restore device defaults | routes/settings.tsx, SettingsPanels.tsx `useSettingsRestore` | partial | No "Restore default settings? This will reset: …" confirmation listing the changed labels; it resets right away | settings.rs `settings_page_view` | S |
| Settings search | settingsSearch.ts | partial | Catalog lacks the v0.1.0 entries: storage-delete-now, storage-worktrees-location, storage-worktree-keep-when, composer-context, project-order, snooze/auto-resume-limited-threads, app-version, update-track, cli-command, privacy-policy, telemetry-export, project-actions. It also lacks keybinding commands rightPanel.new, view.reopenClosed, composer.sendAlternate/sendBackground/sendAndNewThread/cycleHost. A legacy-row hit does not unfold "Legacy features" (`LEGACY_FEATURE_TARGET_IDS`), so it never pulses. The quit-confirmation hit points at a row that does not exist | src/ui/settings_search.rs | S |
| Scope sentence, pickers, project overrides, inheritance popover | SettingsScopeSentence.tsx, settingsScope*.ts, scopedSettings.ts, SettingInheritance.tsx, SettingsScopeNotice.tsx, ScopedSwitch.tsx | done | "Mixed" values are N/A with one environment | src/ui/settings_scope.rs, src/ui/setting_inheritance.rs | – |

### General, Diagnostics, Licenses

| Feature | Upstream file(s) | Port status | What is missing | Port location | Size |
|---|---|---|---|---|---|
| New threads (model, permissions, workspace, submodules) | ProjectDefaultsSettings.tsx | done | – | settings.rs `general_sections` | – |
| Organization | SettingsPanels.tsx | partial | Rows **Project order** (`sidebarProjectSortOrder`, only in the legacy sidebar menu today), **Auto-resume limited threads**, **Snooze limited threads**, and the **Keep threads with a PR** sub-row of auto-settle | settings.rs `general_sections` | S |
| Thread notifications | NotificationSettings.tsx | partial | "Sound" modes can be picked but no sound plays (thread_notifications.rs). On Linux, `canberra-gtk-play`/`paplay` would do | settings.rs, thread_notifications.rs | S |
| Response streaming | SettingsPanels.tsx | partial (bug) | Offers a third option, `token` ("Token by token (legacy)", from v0.0.45). The v0.1.0 contract, which the Pluto server shares, is `Schema.Literals(["turn","paragraph"])`, so choosing it sends an invalid patch | settings.rs `RESPONSE_STREAMING` | S |
| Background activity | SettingsPanels.tsx `BackgroundActivityAdvancedDialog` | partial | Presets only. No "Advanced…" entry or Background Activity dialog (shared policy, Git fetch, provider health, active/idle host power intervals). A custom profile reads "Uses custom intervals." and cannot be edited | settings.rs | M |
| Behavior, Projects & threads, Text generation model, Confirmations (unpin/archive/delete), Legacy fold | SettingsPanels.tsx | done | – | settings.rs | – |
| Confirmations: quit shortcut | SettingsPanels.tsx (`isElectron`) | missing | `confirmQuit` row and hold/double-press Ctrl+Q handling. Electron-only upstream; natively a Ctrl+Q handler in egui | – | S |
| About: version | SettingsPanels.tsx `AboutVersionTitle` | done | – | settings.rs `about-version` | – |
| About: check for updates and update track | SettingsPanels.tsx `AboutVersionSection`, desktopUpdate.logic.ts, desktopUpdate.toast.tsx | missing | Check, download, install, restart, release notes, stable/nightly track, "Update downloaded" toast. **Electron** autoUpdater. Linux: Nix flake updates, at most a "new release" check against HoneySlatt/Pluto releases | – | S–M |
| About: `t3` CLI install | CliCommandSettingsRow.tsx | missing | Install or remove the CLI on PATH. **Electron** bridge. Linux: symlink into `~/.local/bin` (or Nix provides it) | – | S |
| About: privacy policy link | SettingsPanels.tsx | missing | "View policy" row (may be skipped on purpose; not recorded) | – | S |
| Nightly mobile beta (About row and toast) | NightlyMobileBeta.tsx | N/A | Nightly-only, points at T3's TestFlight/Play beta; Pluto has its own Android app | – | – |
| Diagnostics page | DiagnosticsSettings.tsx, ResourceTelemetryDiagnostics*.ts(x), ProcessSignalActions.tsx, routes/settings.diagnostics.tsx | missing | Whole page: live processes with SIGINT/SIGKILL (`signalProcess`); resource history, monitor, timeline and process tree (`processResourceHistory`, `processDiagnostics`); trace diagnostics (failures, slowest spans, span logs, top names; `traceDiagnostics`). "View diagnostics" only shows the notice | settings.rs row `diagnostics` | L |
| OpenTelemetry export | TelemetryExportSettings.tsx | missing | Traces/metrics/logs endpoints with live status (`checkOtlpEndpoint`); it lives on the Diagnostics page | – | M |
| Open source licenses page | OpenSourceLicenses.tsx, routes/settings.open-source-licenses.tsx | missing | Searchable, collapsible license list. Pluto would need its own Rust crate manifest (e.g. cargo-about output) rather than upstream's JSON. "View licenses" shows the notice | settings.rs row `open-source-licenses` | M |

### Appearance

| Feature | Upstream file(s) | Port status | What is missing | Port location | Size |
|---|---|---|---|---|---|
| Colors & themes: scheme and library | ThemeSettings.tsx, ThemePreviewCircles.tsx, ThemeWireframe.tsx | missing | Light/Dark/System selector, theme cards and variants, duplicate/edit/export/delete. The port is dark-only and follows the environment theme. "System" on Linux needs the xdg-desktop-portal `org.freedesktop.appearance color-scheme` | – | L |
| Theme editor, import, OpenVSX search | ThemeEditorPanel.tsx, ThemeEditorHost.tsx, themeEditorStore.ts, themeInspector.ts, ThemeColorPicker.tsx, ThemeImportDialog.tsx, ThemeSearchSection.tsx | missing | Token editor, inspector, VS Code theme import, OpenVSX search and update (`themeEditor.toggle` is bound but has nothing to open) | – | L |
| Interface: Contrast, Glass opacity | SettingsPanels.tsx, appearanceContrast.ts | missing | Both sliders and the token mixing behind them | – | M |
| Interface: Environment identification | SettingsPanels.tsx | missing | Dev/Nightly stage label only; low value | – | S |
| Interface: Composer context | SettingsPanels.tsx (`persistComposerContextStrip`) | missing | v0.1.0 row "Keep branch and worktree controls below the composer after a thread starts". The key is absent from the port and from the README | – | S |
| Interface: Diff colors, Chat width | SettingsPanels.tsx | done | – | settings.rs `appearance_sections` | – |
| Motion | SettingsPanels.tsx, PanelAnimationsPreview.tsx | missing | Panel animation duration slider with live preview (`panelAnimationDurationMs`) | – | M |
| Typography | FontFamilyPicker.tsx, SettingsFontPreviews.tsx, appearanceFonts.ts | partial | Family names are not drawn in their own face; the prompt preview is not editable; the Advanced toggle is not persisted. Font smoothing is macOS-only (N/A). `queryLocalFonts` is replaced by `fc-list` and fontconfig, a sound native equivalent | src/ui/typography.rs | S |

### Keybindings, Archive, Storage, SnapShots

| Feature | Upstream file(s) | Port status | What is missing | Port location | Size |
|---|---|---|---|---|---|
| Keybindings page | KeybindingsSettings.tsx, KeybindingsSettings.logic.ts | partial | "Open keybindings.json" shows the notice (`shell.openInEditor` with `keybindingsConfigPath` needs no Electron). No "Unknown condition(s)" warning. The `DEFAULTS` table lacks five v0.1.0 defaults (`composer.sendAlternate`, `composer.sendAndNewThread`, `composer.sendBackground`, `rightPanel.new`, `view.reopenClosed`). Those bindings therefore show as Custom with Remove instead of Reset to default. No `mergeWithDefaultKeybindings` | src/ui/keybindings.rs `DEFAULTS`, when_builder.rs, settings.rs `keybindings_list` | S |
| Keybindings reload toast and invalid-config warning | KeybindingsUpdateToast.logic.ts, KeybindingsConfigWarning.tsx | missing | No "Keybindings updated" toast (2 s cooldown). No warning toast for `keybindings.*` issues with its "Open keybindings.json" action; `keybindingsUpdated` ignores `issues` | crates/pluto-core/src/model.rs | S |
| Archive page | SettingsPanels.tsx `ArchivedThreadsPanel`, routes/settings.archived.tsx | done | – | settings.rs `archive_sections`, `archive_context_menu` | – |
| Storage | StorageSettings.tsx, FoldedSettingsSection.tsx | partial | Missing rows: **Worktree location** (`worktreesDirectory`), **Delete settled worktrees** (`worktreeSettledAfterDays`), **Keep worktrees with local changes** (`worktreeKeepWhen`), all capability-gated. Also missing the project-scope Inherit/Off/Custom cleanup mode, the only one the README lists | settings.rs `storage_sections`, `cleanup_sections` | S each, M for project mode |
| SnapShots page and capture | SnapShotSettings*.ts(x), SnapShotSetupDialog*.ts(x), CaptureShortcutConfig.tsx, useSnapShotShortcutRecorder.tsx, desktop/SnapShotCoordinator.tsx, desktop/SnapShotShortcutKeys.tsx | stub | Only a disabled switch reading "Only available in the desktop app.". Missing: include app text, shortcut recorder, sound, flash, animations, setup wizard, and the capture-to-draft pipeline. **Electron** (Hyprland/niri helpers on Linux). Natively: `grim` with `hyprctl`/`niri msg`, or the portal Screenshot API, attaching to the draft | settings.rs `snap_shot_sections` | L |

### Providers

| Feature | Upstream file(s) | Port status | What is missing | Port location | Size |
|---|---|---|---|---|---|
| Instance list | ProviderSettingsPanel.tsx, ProviderInstanceCard.tsx (list), providerStatus.ts | partial | No "Add provider" row at the end of the list. The list does not scroll inside its fixed card. Refresh sends `{}` instead of `{refreshModels:true}`. No error toast when a save fails | src/ui/providers.rs `providers_page`, `provider_list_row` | S |
| Header actions | ProviderSettingsPanel.tsx, ProviderUpdatesAction.tsx | partial | No "Update all" / "Updating…" | providers.rs | S |
| Instance writes | hooks/useSettings.ts, `server.mutateProviderInstance` | partial (differs) | The port rewrites the whole `providerInstances` map through `updateSettings`, which can clobber concurrent edits. It also does not clear `textGenerationModelSelection` when the text-generation instance is disabled | providers.rs `save_instance`, `replace_provider_instances` | S |
| Editor title | ProviderInstanceCard.tsx | partial | No "Reset <driver> provider settings" for an edited default instance. An unknown driver shows an empty Runtime section instead of "Driver … not available in this build" | providers.rs `provider_editor` | S |
| Status line | ProviderInstanceCard.tsx `editorStatusNode`, providerStatus.ts | partial | `summary()`'s detail is discarded (`let (headline, _)` at providers.rs:1109 and :1572): no "· CLI not detected on PATH." and the like. Also missing: the warning/error dot, the diagnostic tooltip, the Incompatible/Unsupported/Limited labels, and "Not authenticated · <label>" | providers.rs | S |
| Version advisory and update popover | ProviderInstanceCard.tsx, `getProviderVersionAdvisoryPresentation` | missing | The marker only shows "Provider updates are not implemented yet". Missing: popover, Update now / Install vX (`server.updateProvider`), `updateState` progress and problems, the manual command with Copy, the compatibility triangle. The server runs updates, so no Electron is needed | providers.rs:1278 | M |
| Provider update launch notification | ProviderUpdatePrimaryNotification.tsx, ProviderUpdateLaunchNotification*.ts(x), ProviderUpdateEnvironmentRows.tsx, providerUpdateDismissal.ts | missing | Launch toast (Update / Settings, loading and success), dismissal persistence, per-environment rows (the WSL split is Windows-only) | – | M |
| Runtime fields | ProviderSettingsForm.tsx, contracts provider settings schemas | partial | Fields are hard-coded rather than schema-driven, and have drifted. Antigravity lacks `authMethod` and `apiKey`. OpenCode lacks `serverPassword`. Cursor shows Binary path and API endpoint, which v0.1.0 hides | providers.rs `driver_fields`, `driver_selects`, `driver_secrets` | S |
| Muse and Pi drivers | providerDriverMeta.ts | missing | Not in `DRIVERS`: no label, mark or fields, and they cannot be added | providers.rs `DRIVERS`, add_instance.rs | S |
| Secret inputs | SecretInput.tsx | partial | Only Z Code's key is write-only editable; Antigravity's API key and OpenCode's server password are not | providers.rs `Control::Secret` | S |
| Environment variables | ProviderInstanceCard.tsx `ProviderEnvironmentSection` | done | – | src/ui/providers/environment.rs | – |
| Models section | ProviderModelsSection.tsx | partial | "Filter models" field (above `FILTER_THRESHOLD`) and "No models match."; "· N hidden" missing from the count | providers.rs `provider_models` | S |
| Custom models | ProviderModelsSection.tsx, CustomModelEditor.tsx, customModelEditor.logic.ts | done | – | providers.rs, src/ui/custom_model_editor.rs | – |
| Accent color picker | ProviderAccentColorPicker.tsx | done | – | src/ui/accent_picker.rs | – |
| Add provider instance wizard | AddProviderInstanceDialog.tsx/.logic.ts, AddProviderInstanceWizardSteps.tsx | partial | The driver step still shows v0.0.45's `COMING_SOON` tiles (Github Copilot, Gemini, ACP Registry, Pi Agent; Pi is a real v0.1.0 driver). "Continue with ChatGPT" is a notice | src/ui/providers/add_instance.rs | S (M with ChatGPT) |
| Provider sign-in section | ProviderAuthenticationSection.tsx, ProviderAuthTerminal.tsx, ProviderWizardAuthenticationStep.tsx | missing | Account row, sign-in method, Sign in / Retry / Change account, copy or open link, Cancel, embedded sign-in PTY. Server-driven; could reuse terminal_surface.rs and xdg-open | – | L |
| Antigravity managed runtime | ProviderSetupSection.tsx | missing | Enable, install/update/reinstall/retry/cancel, remove downloaded runtime, "Update required" (`provider.setup`) | – | M |
| Cursor setup rows | provider-cursor client `environmentFields` | missing | Dedicated `CURSOR_API_KEY` field and the "Cursor account" row; the key can only go in as a generic variable | – | S |
| Codex managed setup and ChatGPT accounts | CodexSetupSection*.ts(x), AddCodexAccountDialog.tsx, ChatGpt*.tsx, ProviderAuthCallbackCoordinator.tsx, providerAuthDelivery.ts | missing | Managed install, account row and picker, Continue with ChatGPT / Reconnect, paste-redirect fallback, welcome dialog, `#codex-auth=` callback (`completeProviderAuth`). **Electron** loopback OAuth receiver (`desktopBridge.receiveProviderAuthCallback`). Linux: a short-lived loopback listener in the client, an `x-scheme-handler`, or only the paste fallback | add_instance.rs:310 | L |
| Project scope on Providers | routes/settings.providers.tsx, ProviderInstanceCard.tsx `ProjectOverrideInheritance` | missing | The scope sentence is painted with chevrons but is inert. The enable switch never writes `projectProviderInstanceOverrides`. No Project/Device inheritance popover | providers.rs `providers_scope_sentence` | M |
| Read-only and loading states | ProviderSettingsPanel.tsx | missing | "Limited permissions" row, inert editor without `providers:manage`, "Loading provider settings" / "Could not connect" placeholders | – | S |
| Usage limit hubs | UsageProviderSettings.tsx, AddUsageLimitSourceDialog.tsx | partial | "Add hub" is a notice; only "N hubs configured." shows. No dialog (URL, management key, label), hub rows, or Remove with confirmation. "Cursor account usage" is macOS Keychain only (N/A) | providers.rs:942 | M |
| Health check interval | ProviderSettingsPanel.tsx | partial | No reset back to the background activity preset | providers.rs | S |

### Usage

| Feature | Upstream file(s) | Port status | What is missing | Port location | Size |
|---|---|---|---|---|---|
| Header | UsagePage.tsx, usagePagePreferences.ts | partial | Missing: provider filter crumb (`UsageProviderFilter`, `hiddenProviders`), window label ("Sep 1 to Sep 30"), scan status in the environments menu (Ready / Scanning… / Update required), `UsageCoverageNotice`, persisted preferences, compact select for narrow windows | src/ui/usage.rs | S–M |
| Cost and Tokens views | UsagePage.tsx, UsageProviderChart.tsx, usageBreakdown.ts, UsageShareBar.tsx | partial | `UsageShareBar` sections (Tokens by type, Cost by type, Cost by speed) missing; `categoryCost`/`speedCost` are not merged. "ChatGPT shared usage" / Manage usage row missing | src/ui/usage_summary.rs | M |
| Model detail dialog | UsageModelDialog.tsx | missing | Breakdown rows are not clickable. Missing dialog: cost, tokens, per-1M, cache hit, share bars, "Set price" | – | M |
| Model prices dialog | UsagePriceOverrides.tsx, usagePrice*.ts | partial | Model suggestions; opening it on a single model (needs the model dialog) | src/ui/usage_prices.rs | S |
| Limits view | UsageLimits.tsx, UsageLimitsPooled.tsx | partial | Reset credits ("Use reset" and its confirmation), hub accounts (`usageLimitSources`), external "Manage usage" cards, hidden-provider filtering | src/ui/usage.rs | M |
| Usage shortcuts | usageShortcuts.ts | done | – | keybindings.rs, shortcuts.rs | – |

### Projects, Scheduled Tasks, Integrations, Source Control

| Feature | Upstream file(s) | Port status | What is missing | Port location | Size |
|---|---|---|---|---|---|
| Project page | ProjectsSettings.tsx, ProjectSettingsPanel*.ts(x), ProjectIconPickerDialog.tsx, ProjectFaviconPickerDialog.tsx | partial | Edits one project, not a group. Missing: group-wide rename, the Checkouts section (per-member Remove), and the Danger variants "Remove checkout" / "Remove this project everywhere". No external image pick (`onPickExternal`; Linux: the FileChooser portal already used by the composer). No "This connection cannot change this project." gate | src/ui/projects_page.rs, icon_picker.rs | M |
| Project defaults and actions | ProjectDefaultsSettings.tsx, ProjectActionsSettings.tsx, ProjectActionsList.tsx, useProjectScriptSettings.ts | done | "Different actions across environments" is N/A with one environment | projects_page.rs, action_editor.rs, script_import.rs | – |
| Scheduled tasks list | ScheduledTasksSettings.tsx, scheduledTasksSettings.logic.ts | partial | Webhook "Deliveries" item and `WebhookDeliveriesDialog` | src/ui/scheduled_tasks.rs | M |
| Scheduled task editor | ScheduledTasksSettings.tsx `ScheduledTaskEditorDialog` | partial | Webhook URL with Copy and Rotate (and its confirmation); "Require signature" (header, prefix, encoding, secret); base branch picker (typed); "Runs on" selector (N/A, one environment) | scheduled_tasks.rs | M |
| Integrations: agent browser access | ProjectDefaultsSettings `category="integrations"` | done | – | settings.rs `integrations_sections` | – |
| Integrations: browser defaults | IntegrationsSettings.tsx | inert | Shown as "Only available in the desktop app" with hard-coded values; profiles is a static "Default" row. Upstream gates these on the **Electron** preview bridge, but Pluto has its own Browser surface, so they could be made live | settings.rs:1530 | M |
| Integrations: "Open pull requests in T3 Code" | IntegrationsSettings.tsx `PullRequestLinksOpenInAppSetting` | missing | The whole row, which upstream does not gate on desktop | – | S |
| Browser profile import wizard | BrowserImportWizard.tsx, browserImportWizard.logic.ts | missing | Cookie and login import. **Electron**, plus a macOS keychain checklist. Linux: read Chromium/Firefox profile stores for the Pluto browser | – | L |
| Devices: hub and agent device access | IntegrationsSettings.tsx `DeviceIntegrationSettings` | partial | Read-only: switches show "Device setup is not implemented yet". iOS/Android platform support row missing | settings.rs:1610, :1658 | M |
| Devices: device hosts | DeviceHostsSettings.tsx, DeviceHostEditor.tsx, deviceHostConnectionChecks.ts, useHostConnectionChecks.ts | missing | "No device hosts." is a hard-coded string (settings.rs:1457), so configured hosts never show. Missing: host editor (SSH host and identity), connection checks; "Add host" is a notice | settings.rs | M |
| Source Control: repositories, discovery, Git fetch interval, Bitbucket | SourceControlSettings.tsx, SourceControlHostSettings.tsx, RedactedSensitiveText.tsx | done | Scan button tooltip only | src/ui/source_control.rs, bitbucket.rs | – |
| GitHub details panel | GitHubTokenSettings.tsx, GitHubAccountSettings*.ts(x) | missing | Saved GitHub token (works without `gh`), per-host "Use GitHub on <host>", account picker, reveal/hide | source_control.rs (`has_details` covers git and bitbucket only) | M |
| Other hosts' settings panels | SourceControlHostSettings.tsx | partial | Only Bitbucket's definition is ported | – | S |
| Text generation: writing | SourceControlWritingSettings.tsx | partial | The "Custom instructions" style textarea and its "Write custom instructions for all" action | settings.rs:1280 | S |
| Text generation: branch naming | BranchNamingSettings.tsx | missing | Naming mode, branch prefix, instructions | – | S |
| Text generation: review guides | SourceControlWritingSettings.tsx | missing | Switch, model, instructions | – | S |

### Connections, auth, cloud, onboarding, desktop shell

| Feature | Upstream file(s) | Port status | What is missing | Port location | Size |
|---|---|---|---|---|---|
| Connections: this environment | ConnectionsSettings.tsx, EnvironmentIconPicker.tsx, LocalEnvironmentSetting.tsx, SessionPermissions.tsx | static mock | `connections_sections()` takes no `self`: it always draws "This machine" plus the "Administrative access … access:write" notice, even for admin sessions. Missing: label and icon menu, Version row with server self-update (`ServerUpdateAction`), Permissions disclosure (scopes) | settings.rs:4446 | M |
| Connections: authorized clients | ConnectionsSettings.tsx, pairingUrls.ts, environments/primary/auth.ts | missing | Create a pairing link (label, scopes, QR, copy), list and revoke links and client sessions, revoke all others; all over the server's `auth.*` HTTP API, self-hosted | – | L |
| Connections: network access, endpoints, Tailscale, WSL | ConnectionsSettings.tsx | missing | **Electron** launches the backend. The web build's disabled network row is missing too. Linux: server launch flags or the systemd unit, `tailscale serve` | – | M |
| Connections: environments | ConnectionsSettings.tsx, EnvironmentRow.tsx, EnvironmentRoutesList.tsx, src/connection/*, environments/primary/target.ts | missing | Multi-environment catalog, saved remotes, bearer refresh, SSH provisioning, routes. Shows an empty list; "Add environment" is a notice | settings.rs:1671, :1714 | L |
| Load balancing, GitHub routing | LoadBalancingSettings.tsx, GitHubRoutingSettings.tsx | blocked | Only render with 2+ environments | – | M |
| T3 Connect in Connections | ConnectionsSettings.tsx, src/cloud/* | missing (hosted) | Cloud link row, Publish agent activity, Hold webhooks while offline | – | L |
| Pairing screen | routes/pair.tsx, auth/PairingRouteSurface.tsx, AuthSurfaceShell.tsx | partial (different flow) | The port takes a server URL and a long-lived bearer token (`t3 auth session issue`). Missing: one-time pairing token/link exchange (`exchangeBootstrapCredential`), pairing-URL parsing, auth-mode descriptions. Card eyebrow still says "T3 CODE (ALPHA)" | src/ui/connect.rs:30 | M |
| Agent access approval | routes/connect-agent.tsx, auth/ConnectAgentSurface.tsx | missing | Approve or deny an agent's request with a scope and a one-time code (self-hosted) | – | M |
| T3 Connect CLI auth | routes/connect.tsx, cloud/ConnectCliAuthSurface.tsx, connectCliAuth.ts, dpop.ts | missing (hosted) | – | – | M |
| T3 Connect account (Clerk) | components/clerk/*, cloud/ConnectOnboardingDialog.tsx, RelayClientInstallDialog.tsx, CloudEnvironmentConnectList.tsx, hostedPairing.ts | missing (hosted) | The whole Clerk/T3 Connect layer; probably out of scope for Pluto | – | L |
| First-run gate and Welcome wizard | onboarding/FirstRunGate.tsx, WelcomeWizard.tsx, src/onboarding/* | missing | `onboardingCompletedAt` gate; connection step; "Connect your agents" (readiness, install/login in a setup terminal); "Import your projects" (`agentSessionScan`/`agentSessionImport` of Claude Code and Codex history, upstream's only UI for that import). All self-hosted | – | L |
| Permission update notice | PermissionUpdateNotice.tsx | missing | "Permissions have changed for <env>" toast with Open Connections, from `/api/auth/session` (the port never reads the session) | – | S |
| Native permission checklist | permissions/PermissionChecklist.tsx, usePermissionStatus.ts | missing | **Electron**/macOS (screen recording, accessibility, keychain) for SnapShots and browser import. Linux: portal consent dialogs | – | M |
| App activation (open a folder from the OS or CLI) | desktop/DesktopAppActivationCoordinator.tsx, desktopAppActivation.ts | missing | **Electron**. `main.rs` takes only `--server` and `--token-file`. Linux: a `pluto <path>` argument plus single-instance D-Bus activation | src/main.rs | M |
| OS links (`t3code://`, default-browser links) | desktop/DesktopWebLinkCoordinator.tsx | missing | **Electron**. Linux: `.desktop` `MimeType=x-scheme-handler/pluto;` and forwarding to the running instance | – | M |
| Running-thread keep-alive | desktop/RunningThreadKeepAlive.tsx | partial | Upstream keeps every running thread subscribed (not a power blocker). The port only retains left threads for 5 minutes (`threadRetention`) | crates/pluto-core/src/model.rs | S |
| SSH password prompt | desktop/SshPasswordPromptDialog.tsx | missing | **Electron**, tied to SSH environments. Linux: `SSH_ASKPASS` back into the app, or ssh-agent | – | S |

## 2. Done items

- **Shell:** settings sidebar with icons and the Project, Scheduled Tasks and Personalization entries; breadcrumb header; search field with "/", ranking, page open, center and pulse; Escape leaves.
- **Scope:** scope sentence with project and environment pickers; project overrides with reset; inheritance layers popover and tooltip; environment-wide rows inert at a project scope; t3.json defaults.
- **General:**
  - new-thread defaults with model and traits pickers;
  - project grouping, Working section, auto-settle merged/inactive with days;
  - notification mode (notify-send), in-app notifications, time format;
  - diff whitespace, file state, layout; proactive panels; skills in slash menu; rich text; collapse composer; send shortcut; follow-up mode;
  - provider update checks; continue after restarts; background profile presets;
  - start from origin; add-project start dir;
  - unpin/archive/delete confirmations; text generation model; version; legacy plan mode, context indicator, sidebar.
- **Appearance:** Diff colors, Chat width, Typography (simple and Advanced, fontconfig families, sizes, live previews, word wrap).
- **Keybindings:** recording, Save, Custom badge, Reset to default, Remove, conflict icon and tooltip, when-clause builder, new-binding row, search, groups.
- **Archive:** grouped by project, Unarchive, context menu with Delete, loading/empty/error rows.
- **Storage:** delete-with-thread, inactive, merged and unchanged worktree rules; artifact and log retention; Delete now; Last run report; capability gating.
- **Providers:**
  - list and editor with enable switch; display name with redacted account;
  - accent picker; environment variables (redacted, draft dropped on echo);
  - models: favorites, visibility, order, Enable all / Disable all, groups;
  - custom models: slug checks, editor, presets, "Copy from…", remove;
  - delete custom instances; model-picker deep link; health interval with tooltip;
  - Add-instance wizard steps and validation.
- **Usage:**
  - metric and period segments, refresh;
  - Cost/Tokens summary, chart with hover readout, totals, model and period breakdowns;
  - Limits pooled per driver with pace, reset countdown and segment popovers, limit notices;
  - environments menu with Model prices; shortcuts.
- **Projects:** scope notice, rename, icon (Lucide, emoji, monogram, favicon from project files), new-thread defaults, actions with the editor and t3.json import, remove with confirmation.
- **Scheduled Tasks:** live list, schedule and next run, outcomes, pause, Edit / Run now / Delete; editor (name, project, workspace mode, typed base branch, prompt, model, three schedule kinds, age limit, enabled).
- **Source Control:** repository defaults, discovery with redacted accounts, Git fetch interval panel, Bitbucket credentials, writing style, follow templates, writer model.
- **Integrations:** agent browser access (with project override); hub and agent tool versions from the local host.

## 3. Detail on the main partial and missing items

1. **Provider onboarding and maintenance is server-driven, yet absent.** v0.1.0 moved provider sign-in (`ProviderAuthenticationSection`, sign-in PTY), managed runtimes (Antigravity install, Codex managed setup) and provider updates (version advisory popover, `server.updateProvider`, launch toast, Update all) into the client. The port has none of it. Only the ChatGPT OAuth callback needs the Electron shell; everything else is server RPCs plus a terminal (reuse `terminal_surface.rs`) and `xdg-open`.
2. **Connections is a hard-coded placeholder.** It never reads the session's scopes, so admin tokens also see the "requires access:write" notice. Pairing links (with QR), client session management, environment identity and version, and server self-update are all self-hosted and missing. Without pairing-token exchange, a user can only connect by pasting a long-lived bearer token.
3. **Single environment is the architectural limit.** Saved remotes, SSH environments (and their password prompt), routes, load balancing, GitHub routing, the "Runs on" selector and Usage's environment filter all wait on a multi-environment catalog (`src/connection/*`).
4. **Onboarding (FirstRunGate, WelcomeWizard) is missing.** It is also the only upstream UI for importing Claude Code and Codex history (`agentSessionScan`/`agentSessionImport`).
5. **Diagnostics and Open source licenses** are rows whose buttons show the notice. The Diagnostics page (processes with signals, resource history, traces, OTLP export) is large.
6. **Theme system:** library, light and system schemes, editor, import, OpenVSX search, contrast, glass and motion. The port is dark-only and follows the environment theme.
7. **Settings drifted from v0.1.0 schemas:**
   - the response streaming `token` option (invalid against the shared contract);
   - provider runtime fields (Antigravity `authMethod`/`apiKey`, OpenCode `serverPassword`, Cursor hidden fields shown);
   - Muse and Pi drivers absent, while the wizard shows v0.0.45 "Coming Soon" tiles;
   - five keybinding defaults missing from `DEFAULTS`;
   - new rows absent: Organization ×4, composer context, Storage ×3, branch naming, review guides, custom writing instructions, "Open pull requests in T3 Code".
8. **Provider instance writes** go through whole-map `updateSettings` instead of `server.mutateProviderInstance`, which risks overwriting concurrent edits.
9. **Electron-shell features without a Linux-native counterpart yet:**

   | Feature | Linux-native equivalent |
   |---|---|
   | Desktop auto-update | Nix, or a release check |
   | `t3` CLI install | `~/.local/bin` symlink |
   | Quit confirmation | egui Ctrl+Q |
   | SnapShots | grim, or the portal Screenshot API |
   | Native permissions | portal consent |
   | App activation | CLI argument + D-Bus single instance |
   | `t3code://` deep links | `x-scheme-handler` .desktop entry |
   | SSH password prompt | `SSH_ASKPASS` |
   | Codex OAuth loopback | client loopback listener |
   | Browser profile import | read Chromium/Firefox stores |
   | System color scheme | portal `color-scheme` |

   The browser defaults are gated on the Electron preview bridge upstream, but Pluto's own Browser surface could make them live.

## 4. README and module-doc claims found absent or wrong

- **README, Settings row:**
  - "the General page (… about, diagnostics, …)": only the row exists, and its button shows "not implemented".
  - "device hub and hosts from `subscribeDeviceState`, read-only": hosts are a hard-coded "No device hosts.", and only local tool versions are read.
  - "the Connections page as a session without `access:write` sees it": the page is static and shown to every session, including admin ones.
  - "runtime fields": incomplete against v0.1.0 (see §3.7).
- **README, Keybindings page "Reset to default":** wrong for the five v0.1.0 defaults missing from `DEFAULTS`. They show as Custom and offer Remove.
- **README "Not ported yet" omissions:**
  - **Pages:** Diagnostics, Open source licenses, onboarding.
  - **General:** Organization rows, quit confirmation, About updates/track, CLI install, privacy policy, background activity dialog.
  - **Appearance:** composer context.
  - **Storage:** worktree location, settled worktrees, keep-when.
  - **Keybindings:** reload toast, invalid-config warning, unknown-condition warning.
  - **Settings:** restore-defaults confirmation.
  - **Providers:**
    - launch toast and Update all, sign-in section;
    - status detail line, Filter models;
    - hub list and remove, health interval reset;
    - default-instance reset, limited-permissions state, project override;
    - Muse/Pi, `mutateProviderInstance`.
  - **Usage:** provider filter, share bars, model dialog, Manage usage cards, window label.
  - **Source Control:** GitHub panel, branch naming, review guides, custom writing instructions.
  - **Integrations:** "Open pull requests in T3 Code", device platform row.
  - **Scheduled Tasks:** webhook URL and Deliveries (only partly listed).
  - **Connections and auth:** pairing-token exchange, connect-agent, permission-update notice.
  - **Projects:** group checkouts.
- **Stale module docs:**
  - `src/ui/settings.rs`: says search, scope pickers, model pickers and "the other pages" are not ported, and that client settings live in memory. All are ported, and client settings persist to `$XDG_CONFIG_HOME/pluto/client-settings.json`. It also cites a "token-streaming warning" that v0.1.0 no longer has.
  - `src/ui/providers.rs`: lists adding/deleting instances, accent picker, env vars and custom models as not ported. All are ported, in add_instance.rs, accent_picker.rs, providers/environment.rs and custom_model_editor.rs.
  - `src/ui/usage.rs`: lists segment popovers and keyboard shortcuts as not ported. Both exist.
  - `src/ui/usage_summary.rs`: lists "price overrides" as not ported. usage_prices.rs ports them; only the per-model "Set price" entry is missing.
  - `src/ui/source_control.rs`: says the Git fetch interval and Bitbucket panels are not ported. Both are. The GitHub panel is the one missing.
  - `src/ui/settings_search.rs`: drops "the project overview (no project scope here)". A project scope now exists.
  - `src/ui/keybindings.rs`: "Conflicts show their warning icon only". The conflict tooltip text is shown too.
  - `src/ui/projects_page.rs` (~line 141): "actions (read-only here)". Rows open the action editor.
