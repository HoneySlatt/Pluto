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
import zlib from "node:zlib";

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

function thread(projectId, title, minutesAgo, branch = "main", worktreePath = null) {
  const threadId = randomUUID();
  const createdAt = at(minutesAgo);
  emit("thread", threadId, "thread.created", createdAt, {
    threadId, projectId, title, modelSelection: model, runtimeMode: "full-access",
    interactionMode: "default", branch, worktreePath, createdAt, updatedAt: createdAt,
  }, "client");
  return threadId;
}

function message(threadId, role, text, minutesAgo, turnId = null, attachments = []) {
  const createdAt = at(minutesAgo);
  const messageId = randomUUID();
  emit("thread", threadId, "thread.message-sent", createdAt, {
    threadId, messageId, role, text, attachments, turnId,
    streaming: false, createdAt, updatedAt: createdAt,
  }, role === "user" ? "client" : "provider");
  return messageId;
}

function tool(threadId, turnId, title, itemType, detail, minutesAgo, data, toolCallId = randomUUID()) {
  const createdAt = at(minutesAgo);
  emit("thread", threadId, "thread.activity-appended", createdAt, {
    threadId,
    activity: {
      id: randomUUID(), tone: "tool", kind: "tool.completed", summary: title,
      payload: { itemType, toolCallId, status: "completed", title, detail, ...(data ? { data } : {}) },
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
// A package inside web-demo's repository: the sidebar groups it with web-demo.
const webUi = project("web-ui", "web-demo/packages/ui", 570);
thread(webUi, "Composants partagés", 1800);

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

const second = thread(rust, "Brancher l'envoi des messages", 180, "feature/composer",
  path.join(workspaces, "worktrees", "feature-composer"));
// A synced pull request link on a reserved host, so no sync ever reaches a real one.
emit("thread", second, "thread.pull-request-linked", at(170), {
  threadId: second,
  link: {
    host: "git.example.invalid", repository: "example/demo-project", number: 12,
    url: "https://git.example.invalid/example/demo-project/pull/12", source: "manual",
    linkedAt: at(170),
    snapshot: {
      state: "open", title: "Brancher le composer", headBranch: "feature/composer",
      baseBranch: "main", isDraft: false, updatedAt: at(170), syncedAt: at(170),
    },
    stack: null,
  },
  updatedAt: at(170),
}, "client");
message(second, "user", "Branche le composer sur thread.turn.start.", 180);
message(second, "assistant", "C'est fait : le composer envoie maintenant `thread.turn.start` avec le mode et le modèle du thread.", 178);

const nix1 = thread(nixos, "Supprimer Oxi et son thème", 300);
message(nix1, "user", "Retire Oxi de la config et son thème.", 300);
message(nix1, "assistant", "Oxi est retiré du module home et le thème associé n'est plus importé.", 298);
const nix2 = thread(nixos, "Fix Amane Font Patch Build Failure", 1500);
message(nix2, "user", "Le build du patch de police Amane échoue.", 1500);

const web1 = thread(web, "Générer une image de pomme", 90);
message(web1, "user", "Génère une image de pomme en SVG, comme celle-ci.", 90, null, [
  imageAttachment(web1, "pomme.png"),
]);
// Codex's image generation: the server strips the item, and Codex saves the
// PNG as `$CODEX_HOME/generated_images/<thread>/<call id>.png` (the
// fixture's CODEX_HOME is `<baseDir>/codex`).
const web1Turn = randomUUID();
const web1Call = `exec-${randomUUID()}`;
const generatedDir = path.join(baseDir, "codex", "generated_images", randomUUID());
fs.mkdirSync(generatedDir, { recursive: true });
fs.writeFileSync(path.join(generatedDir, `${web1Call}.png`), syntheticPng());
tool(web1, web1Turn, "Image view", "image_view", undefined, 89, {
  item: {
    type: "imageGeneration", id: web1Call, status: "completed", failure: null,
    revisedPrompt: "Une pomme rouge sur un ciel dégradé", transparentBackground: null,
    result: syntheticPng().toString("base64"),
    savedPath: path.join(generatedDir, `${web1Call}.png`),
  },
}, web1Call);
const web1Answer = message(web1, "assistant", "Voici ta pomme rouge 🍎", 88, web1Turn);
// A follow-up citing the answer (`serializeAssistantCitation`).
const environmentId = fs.readFileSync(path.join(baseDir, "userdata", "environment-id"), "utf8").trim();
const citePath = [environmentId, web1, web1Answer].map(encodeURIComponent).join("/");
const citeQuery = new URLSearchParams({
  text: "pomme rouge", start: "9", end: "20", prefix: "Voici ta ", suffix: " 🍎",
});
message(web1, "user", `[Assistant quote](t3-citation://v1/${citePath}?${citeQuery}) Tu peux la rendre plus grosse ?`, 87);
message(web1, "assistant", "Bien sûr, la voici en plus grand.", 86, randomUUID());

// A synthetic 320x240 PNG (a red disc on a sky gradient), stored like an
// uploaded image (`<thread segment>-<uuid>.png` in the attachments dir).
function imageAttachment(threadId, name) {
  const png = syntheticPng();
  const id = `${threadId.toLowerCase()}-${randomUUID()}`;
  const dir = path.join(baseDir, "userdata", "attachments");
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, `${id}.png`), png);
  return { type: "image", id, name, mimeType: "image/png", sizeBytes: png.length };
}

function syntheticPng() {
  const width = 320;
  const height = 240;
  const raw = Buffer.alloc((width * 3 + 1) * height);
  for (let y = 0; y < height; y++) {
    raw[y * (width * 3 + 1)] = 0;
    for (let x = 0; x < width; x++) {
      const inside = (x - 160) ** 2 + (y - 130) ** 2 < 70 ** 2;
      const offset = y * (width * 3 + 1) + 1 + x * 3;
      raw[offset] = inside ? 220 : 120 + (y * 80) / height;
      raw[offset + 1] = inside ? 40 : 170 + (y * 60) / height;
      raw[offset + 2] = inside ? 50 : 230;
    }
  }
  const chunk = (type, data) => {
    const length = Buffer.alloc(4);
    length.writeUInt32BE(data.length);
    const body = Buffer.concat([Buffer.from(type), data]);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(zlib.crc32(body));
    return Buffer.concat([length, body, crc]);
  };
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 8;
  header[9] = 2;
  const png = Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", header),
    chunk("IDAT", zlib.deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
  return png;
}

// Every markdown construct the assistant uses, for style comparisons. Links
// stay on github.com (a drawn mark) and reserved hosts (the globe), so the
// reference never fetches a favicon.
const styles = thread(web, "Styles markdown", 100);
const stylesTurn = randomUUID();
message(styles, "user", "Montre-moi tous les styles.", 100);
message(styles, "assistant", [
  "Le défaut vient de `crates/core/src/overview.rs:58` et touche aussi `src/app.rs`. Voir [main.rs](src/main.rs#L10), l'issue [#3973 sur GitHub](https://github.com/Heroic-Games-Launcher/HeroicGamesLauncher/issues/3973) et [la doc locale](http://docs.example.test/cloud-saves). La commande `cargo test` passe.",
  "",
  "## Titre de niveau 2",
  "",
  "### Titre de niveau 3",
  "",
  "- **Gras**, *italique* et ~~barré~~",
  "- Un élément avec `du code` et un [lien GitHub](https://github.com/t3tools/t3code)",
  "  - Un sous-élément",
  "",
  "1. Premier",
  "2. Second",
  "",
  "> Une citation avec `README.md` dedans.",
  "",
  "| Fichier | Lignes |",
  "| --- | --- |",
  "| `src/ui/chat.rs` | 120 |",
  "",
  "```rust",
  "fn main() {}",
  "```",
  "",
  "---",
  "",
  "![Capture du rendu](docs/capture.png)",
  "",
  "![Image manquante](docs/absente.png)",
  "",
  "Fin.",
].join("\n"), 99, stylesTurn);

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
    // Long descriptions wrap, and the options overflow the banner's scroll area.
    { id: "compositor", header: "Compositeur", question: "Quel compositeur veux-tu utiliser pour la session Wayland, sachant que la configuration actuelle doit rester compatible avec les deux machines ?",
      options: [
        { label: "Niri (Recommandé)", description: "Tiling à défilement : les fenêtres s'ouvrent dans une bande horizontale infinie que l'on fait défiler, sans jamais redimensionner celles qui sont déjà ouvertes. La configuration existante s'y prête déjà en grande partie." },
        { label: "Hyprland", description: "Tiling dynamique avec animations et effets ; la configuration est plus longue à maintenir, mais les plugins couvrent presque tous les besoins et la communauté est très active." },
        { label: "Sway", description: "Compatible i3, stable et sobre, mais sans animations ni coins arrondis : un retour en arrière visuel par rapport à la configuration actuelle." },
        { label: "Garder l'actuel", description: "Ne rien changer pour l'instant." },
      ], multiSelect: false },
    { id: "bar", header: "Barre", question: "Quelle barre d'état ?",
      options: [{ label: "Waybar" }, { label: "Aucune" }], multiSelect: false },
  ],
}, 19);

// A long thread (60 turns of mixed markdown and tool work), for measuring
// frame times while scrolling and switching threads.
const long = thread(web, "Long fil de discussion", 2000);
let firstLongAnswer;
for (let i = 0; i < 60; i++) {
  const minutes = 1990 - i * 3;
  const longTurn = randomUUID();
  message(long, "user", `Étape ${i + 1} : peux-tu revoir le module \`step_${i + 1}.rs\` et expliquer ce qui change ?`, minutes);
  tool(long, longTurn, "Ran command", "command_execution", `rg -n "fn " src/step_${i + 1}.rs`, minutes - 1, { command: `rg -n "fn " src/step_${i + 1}.rs` });
  tool(long, longTurn, "Read file", "file_read", `src/step_${i + 1}.rs`, minutes - 1);
  tool(long, longTurn, "File change", "file_change", `src/step_${i + 1}.rs`, minutes - 1);
  const longAnswer = message(long, "assistant", [
    `## Étape ${i + 1}`,
    "",
    `Le module \`src/step_${i + 1}.rs\` regroupe maintenant la **lecture**, la *validation* et l'écriture des données. Chaque fonction garde une seule responsabilité, et les erreurs remontent avec leur contexte au lieu d'être avalées.`,
    "",
    "- La lecture passe par un tampon unique, réutilisé d'un appel à l'autre.",
    "- La validation renvoie la liste complète des problèmes, pas seulement le premier.",
    "- L'écriture est atomique : un fichier temporaire, puis un renommage.",
    "",
    "```rust",
    `pub fn step_${i + 1}(input: &str) -> Result<Vec<Entry>, Error> {`,
    "    let mut entries = Vec::new();",
    "    for (line, text) in input.lines().enumerate() {",
    "        let entry = Entry::parse(text).map_err(|e| e.at(line + 1))?;",
    "        if entry.is_valid() {",
    "            entries.push(entry);",
    "        }",
    "    }",
    "    Ok(entries)",
    "}",
    "```",
    "",
    "| Fonction | Avant | Après |",
    "| --- | --- | --- |",
    `| \`read\` | ${40 + i} lignes | ${20 + i} lignes |`,
    `| \`validate\` | ${30 + i} lignes | ${25 + i} lignes |`,
    "",
    "> Les tests existants passent sans modification.",
  ].join("\n"), minutes - 2, longTurn);
  firstLongAnswer ??= longAnswer;
}
// A follow-up citing the first answer, far up the thread.
const longCite = new URLSearchParams({
  text: "regroupe maintenant", start: "30", end: "49", prefix: "Le module step_1.rs ", suffix: " la lecture",
});
message(long, "user", `[Assistant quote](t3-citation://v1/${[environmentId, long, firstLongAnswer].map(encodeURIComponent).join("/")}?${longCite}) Et pour la toute première étape ?`, 1800);
message(long, "assistant", "La première étape suit le même découpage.", 1799, randomUUID());

// A turn rewriting one large file, for measuring the diff panel.
const bigDiff = thread(web, "Grand diff", 2100);
const bigTurn = randomUUID();
message(bigDiff, "user", "Réécris le module de rendu.", 2100);
gitCheckpoints(path.join(workspaces, "web-demo"), bigDiff, {
  "src/render.ts": lines("export const oldValue =", 800),
}, {
  "src/render.ts": lines("export const oldValue =", 200) + lines("export const newValue =", 1400),
});
const bigAnswer = message(bigDiff, "assistant", "Le module de rendu est réécrit.", 2098, bigTurn);
completeTurn(bigDiff, bigTurn, 1, [
  { path: "src/render.ts", kind: "modified", additions: 1400, deletions: 600 },
], 2098, bigAnswer);

// A turn that kept a task list (`turn.plan.updated`), shown while it runs.
const tasksThread = thread(rust, "Porter le thème", 2300);
const tasksTurn = randomUUID();
message(tasksThread, "user", "Porte le thème sombre dans le client.", 2300);
const taskSteps = ["Lire les jetons du thème", "Porter les couleurs", "Vérifier les captures", "Mettre à jour le README"];
[[1, 0], [2, 1], [2, 2]].forEach(([done, minute]) => {
  const createdAt = at(2299 - minute);
  emit("thread", tasksThread, "thread.activity-appended", createdAt, {
    threadId: tasksThread,
    activity: {
      id: randomUUID(), tone: "info", kind: "turn.plan.updated", summary: "Plan updated",
      payload: {
        explanation: null,
        plan: taskSteps.map((step, index) => ({
          step,
          status: index < done ? "completed" : index === done ? "inProgress" : "pending",
        })),
      },
      turnId: tasksTurn, createdAt,
    },
  }, "provider");
});
const tasksAnswer = message(tasksThread, "assistant", "Les couleurs sont portées.", 2297, tasksTurn);
completeTurn(tasksThread, tasksTurn, 1, [], 2297, tasksAnswer);

// A user message carrying a terminal excerpt (inline context) and a file mention.
const contextThread = thread(rust, "Analyser l'erreur", 2400);
{
  const createdAt = at(2400);
  const contextId = "terminal_0f6e3c2a-1b7d-4c55-9a0e-5d2f8c1b9e47";
  emit("thread", contextThread, "thread.message-sent", createdAt, {
    threadId: contextThread, messageId: randomUUID(), role: "user",
    text: `Pourquoi [Terminal 1 lines 3-4](t3-context://v1/terminal/${contextId}) échoue dans [app.rs](src/app.rs) ?`,
    attachments: [], turnId: null, streaming: false, createdAt, updatedAt: createdAt,
    context: {
      version: 1,
      records: [{
        version: 1, contextId, kind: "terminal", label: "Terminal 1 lines 3-4",
        terminalId: "default", terminalLabel: "Terminal 1", lineStart: 3, lineEnd: 4,
        text: "$ cargo build\nerror[E0425]: cannot find value `x` in this scope",
      }],
    },
  }, "client");
}

// A plan-mode thread whose settled turn proposed a plan.
const planThreadId = randomUUID();
{
  const createdAt = at(2200);
  emit("thread", planThreadId, "thread.created", createdAt, {
    threadId: planThreadId, projectId: web, title: "Planifier la migration", modelSelection: model,
    runtimeMode: "full-access", interactionMode: "plan", branch: "main", worktreePath: null,
    createdAt, updatedAt: createdAt,
  }, "client");
}
const planTurn = randomUUID();
message(planThreadId, "user", "Prépare un plan pour migrer le rendu vers le nouveau moteur.", 2200);
const planAnswer = message(planThreadId, "assistant", "Voici le plan proposé.", 2199, planTurn);
{
  const createdAt = at(2199);
  emit("thread", planThreadId, "thread.proposed-plan-upserted", createdAt, {
    threadId: planThreadId,
    proposedPlan: {
      id: randomUUID(),
      turnId: planTurn,
      planMarkdown: [
        "# Migrer le rendu vers le nouveau moteur",
        "",
        "## Summary",
        "",
        "Remplacer l'ancien moteur de rendu par le nouveau, module par module.",
        "",
        "## Étapes",
        "",
        ...Array.from({ length: 14 }, (_, i) => `${i + 1}. Porter le module \`render/part${i + 1}.ts\` et vérifier sa capture.`),
        "",
        "## Vérification",
        "",
        "- Comparer les captures avant et après.",
        "- Lancer `cargo test`.",
      ].join("\n"),
      implementedAt: null,
      implementationThreadId: null,
      createdAt,
      updatedAt: createdAt,
    },
  }, "provider");
}
completeTurn(planThreadId, planTurn, 1, [], 2198, planAnswer);

// The image the "Styles markdown" answer shows, written last so no turn
// checkpoint of web-demo picks it up.
fs.mkdirSync(path.join(workspaces, "web-demo", "docs"), { recursive: true });
fs.writeFileSync(path.join(workspaces, "web-demo", "docs", "capture.png"), syntheticPng());
// A document and a table for the Files surface's rendered previews.
fs.writeFileSync(
  path.join(workspaces, "web-demo", "docs", "guide.md"),
  [
    "# Guide de rendu",
    "",
    "Le rendu suit **trois étapes** et lit `src/render.ts`.",
    "",
    "- Préparer les données",
    "- Dessiner les lignes",
    "- Vérifier la capture",
    "",
    "![La capture](capture.png)",
    "",
  ].join("\n"),
);
fs.writeFileSync(
  path.join(workspaces, "web-demo", "docs", "long.md"),
  Array.from({ length: 120 }, (_, i) =>
    [
      `## Section ${i + 1}`,
      "",
      `Le paragraphe ${i + 1} décrit **le rendu** et cite \`src/render.ts\` avec un peu de texte pour remplir la ligne.`,
      "",
      "- Un point",
      "- Un autre point",
      "",
      "```ts",
      `export const value${i} = ${i};`,
      "```",
      "",
    ].join("\n"),
  ).join("\n"),
);
fs.writeFileSync(
  path.join(workspaces, "web-demo", "docs", "ventes.csv"),
  [
    "Mois,Région,Ventes,Commentaire",
    "Janvier,Nord,1200,Stable",
    'Février,Sud,980,"Baisse, puis reprise"',
    'Mars,Est,1430,"Une note ""citée"""',
    "Avril,Ouest,1610,Record trimestriel après la campagne de printemps",
    "",
  ].join("\n"),
);

console.log("seeded fixture events");
