// Seeds a disposable T3 Code data directory with synthetic projects, threads,
// messages and tool activity, so the upstream web client and this client can
// be compared on identical data without running provider turns.
//
// usage: node scripts/seed-fixture.mjs <base-dir> <workspace-parent>
// The server must be stopped; it projects the appended events on next start.
import { DatabaseSync } from "node:sqlite";
import { randomUUID } from "node:crypto";
import path from "node:path";
import fs from "node:fs";
import { execFileSync } from "node:child_process";

const [baseDir, workspaces] = process.argv.slice(2);
if (!baseDir || !workspaces) {
  console.error("usage: node scripts/seed-fixture.mjs <base-dir> <workspace-parent>");
  process.exit(1);
}
const db = new DatabaseSync(path.join(baseDir, "userdata", "state.sqlite"));
const now = Date.now();
const at = (minutesAgo) => new Date(now - minutesAgo * 60_000).toISOString();
const versions = new Map();
const insert = db.prepare(`insert into orchestration_events
  (event_id, aggregate_kind, stream_id, stream_version, event_type, occurred_at,
   command_id, causation_event_id, correlation_id, actor_kind, payload_json, metadata_json)
  values (?, ?, ?, ?, ?, ?, ?, null, ?, ?, ?, '{}')`);

function emit(kind, streamId, type, occurredAt, payload, actor = "server") {
  const version = versions.get(streamId) ?? nextVersion(kind, streamId);
  versions.set(streamId, version + 1);
  const commandId = randomUUID();
  insert.run(randomUUID(), kind, streamId, version, type, occurredAt, commandId, commandId,
    actor, JSON.stringify(payload));
}

function nextVersion(kind, streamId) {
  const row = db
    .prepare("select max(stream_version) as v from orchestration_events where aggregate_kind = ? and stream_id = ?")
    .get(kind, streamId);
  return row.v == null ? 0 : row.v + 1;
}

const model = { instanceId: "codex", model: "gpt-6-astra" };

function project(title, dir, minutesAgo) {
  const projectId = randomUUID();
  const createdAt = at(minutesAgo);
  emit("project", projectId, "project.created", createdAt, {
    projectId, title, workspaceRoot: path.join(workspaces, dir), defaultModelSelection: null,
    faviconPath: null, projectIcon: null, scripts: [], createdAt, updatedAt: createdAt,
  }, "client");
  return projectId;
}

function thread(projectId, title, minutesAgo, branch = "main") {
  const threadId = randomUUID();
  const createdAt = at(minutesAgo);
  emit("thread", threadId, "thread.created", createdAt, {
    threadId, projectId, title, modelSelection: model, runtimeMode: "full-access",
    interactionMode: "default", branch, worktreePath: null, createdAt, updatedAt: createdAt,
  }, "client");
  return threadId;
}

function message(threadId, role, text, minutesAgo, turnId = null) {
  const createdAt = at(minutesAgo);
  const messageId = randomUUID();
  emit("thread", threadId, "thread.message-sent", createdAt, {
    threadId, messageId, role, text, attachments: [], turnId,
    streaming: false, createdAt, updatedAt: createdAt,
  }, role === "user" ? "client" : "provider");
  return messageId;
}

function tool(threadId, turnId, title, itemType, detail, minutesAgo, data) {
  const createdAt = at(minutesAgo);
  emit("thread", threadId, "thread.activity-appended", createdAt, {
    threadId,
    activity: {
      id: randomUUID(), tone: "tool", kind: "tool.completed", summary: title,
      payload: { itemType, toolCallId: randomUUID(), status: "completed", title, detail, ...(data ? { data } : {}) },
      turnId, createdAt,
    },
  }, "provider");
}

// Real checkpoint refs so the server can compute turn diffs: turn 0 is the
// workspace before the turn, turn N the workspace after it.
function checkpointRef(threadId, count) {
  return `refs/t3/checkpoints/${Buffer.from(threadId).toString("base64url")}/turn/${count}`;
}

function gitCheckpoints(repo, threadId, before, after) {
  const git = (args, input) =>
    execFileSync("git", ["-C", repo, ...args], { input, encoding: "utf8" }).trim();
  const commit = (files, message) => {
    for (const [file, content] of Object.entries(files)) {
      fs.mkdirSync(path.dirname(path.join(repo, file)), { recursive: true });
      fs.writeFileSync(path.join(repo, file), content);
    }
    git(["add", "-A"]);
    git(["-c", "user.name=fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", message]);
    return git(["rev-parse", "HEAD"]);
  };
  const base = commit(before, "before turn");
  git(["update-ref", checkpointRef(threadId, 0), base]);
  const turned = commit(after, "after turn");
  git(["update-ref", checkpointRef(threadId, 1), turned]);
  git(["reset", "-q", "--hard", base]);
}

const lines = (prefix, count) =>
  Array.from({ length: count }, (_, i) => `${prefix} ${i + 1}`).join("\n") + "\n";

function completeTurn(threadId, turnId, count, files, minutesAgo, assistantMessageId = null) {
  const completedAt = at(minutesAgo);
  emit("thread", threadId, "thread.turn-diff-completed", completedAt, {
    threadId, turnId, checkpointTurnCount: count, checkpointRef: checkpointRef(threadId, count),
    status: "ready", files, assistantMessageId, completedAt,
  });
}

const rust = project("T3 Code Rust", "demo-project", 600);
const nixos = project("NixOS", "nixos-demo", 590);
const web = project("web-demo", "web-demo", 580);

const main = thread(rust, "Reproduire T3 Code en Rust avec egui", 42);
const turn = randomUUID();
message(main, "user", "Est-ce qu'on peut reproduire l'interface de T3 Code en Rust avec egui, à l'identique ?", 42);
tool(main, turn, "Ran command", "command_execution", "rg --files src | head -20", 41, { command: "rg --files src | head -20" });
tool(main, turn, "Ran command", "command_execution", "cargo build", 40, { command: "cargo build" });
tool(main, turn, "File change", "file_change", "src/app.rs", 39);
const answer = message(main, "assistant", [
  "Oui. T3 Code est déjà découpé en **serveur + client** : le client egui se connecte au serveur existant par WebSocket.",
  "",
  "## Ce qui est en place",
  "",
  "- `theme.rs` : les tokens de couleur du thème sombre",
  "- `protocol.rs` : les enveloppes Effect RPC",
  "- `model.rs` : la projection shell et thread",
  "",
  "```rust",
  "fn main() -> eframe::Result<()> {",
  "    eframe::run_native(\"T3 Code\", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))",
  "}",
  "```",
  "",
  "| Atteignable | Plus difficile |",
  "|---|---|",
  "| Layout, géométrie | Rendu exact du texte |",
  "| Couleurs du thème | Animations CSS |",
].join("\n"), 38, turn);
gitCheckpoints(path.join(workspaces, "demo-project"), main, {
  "src/app.rs": "// app\n" + lines("let old_line =", 30),
  "src/theme.rs": "// theme\n" + lines("const OLD: u8 =", 10),
}, {
  "src/app.rs": "// app\n" + lines("let old_line =", 16) + lines("let new_line =", 120),
  "src/theme.rs": "// theme\n" + lines("const OLD: u8 =", 7) + lines("const NEW: u8 =", 18),
});
completeTurn(main, turn, 1, [
  { path: "src/app.rs", kind: "modified", additions: 120, deletions: 14 },
  { path: "src/theme.rs", kind: "modified", additions: 18, deletions: 3 },
], 38, answer);

const second = thread(rust, "Brancher l'envoi des messages", 180, "feature/composer");
message(second, "user", "Branche le composer sur thread.turn.start.", 180);
message(second, "assistant", "C'est fait : le composer envoie maintenant `thread.turn.start` avec le mode et le modèle du thread.", 178);

const nix1 = thread(nixos, "Supprimer Oxi et son thème", 300);
message(nix1, "user", "Retire Oxi de la config et son thème.", 300);
message(nix1, "assistant", "Oxi est retiré du module home et le thème associé n'est plus importé.", 298);
const nix2 = thread(nixos, "Fix Amane Font Patch Build Failure", 1500);
message(nix2, "user", "Le build du patch de police Amane échoue.", 1500);

const web1 = thread(web, "Générer une image de pomme", 90);
message(web1, "user", "Génère une image de pomme en SVG.", 90);

function request(threadId, turnId, kind, summary, payload, minutesAgo) {
  const createdAt = at(minutesAgo);
  emit("thread", threadId, "thread.activity-appended", createdAt, {
    threadId,
    activity: { id: randomUUID(), tone: "approval", kind, summary, payload, turnId, createdAt },
  }, "provider");
}

const approval = thread(rust, "Lancer le build release", 12);
const approvalTurn = randomUUID();
message(approval, "user", "Lance un build release et dis-moi s'il passe.", 12);
request(approval, approvalTurn, "approval.requested", "Command approval requested", {
  requestId: randomUUID(), requestKind: "command", requestType: "command_execution_approval",
  detail: "cargo build --release",
}, 11);

const question = thread(nixos, "Choisir le compositeur", 20);
const questionTurn = randomUUID();
message(question, "user", "Configure le compositeur Wayland.", 20);
request(question, questionTurn, "user-input.requested", "User input requested", {
  requestId: randomUUID(),
  questions: [
    { id: "compositor", header: "Compositeur", question: "Quel compositeur veux-tu utiliser ?",
      options: [
        { label: "Niri", description: "Tiling à défilement" },
        { label: "Hyprland", description: "Tiling dynamique" },
      ], multiSelect: false },
    { id: "bar", header: "Barre", question: "Quelle barre d'état ?",
      options: [{ label: "Waybar" }, { label: "Aucune" }], multiSelect: false },
  ],
}, 19);

console.log("seeded fixture events");
