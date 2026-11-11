// The harness's own test configuration, passed to the fork's runner with `--config`
// (scripts/fork-replay.sh). It replaces the fork's: nothing of the fork's configuration runs.
//
// - `cacheDir` keeps every file the runner writes in Pluto's target folder, never in the fork.
// - `@fork/` stands for the fork's checkout, so that the harness imports the fork's modules by
//   their absolute path without writing the owner's home folder in a committed file.
// - A package the harness names (`effect`, `@t3tools/contracts`) resolves as if the fork's server
//   had named it: from the fork's own `node_modules`, the same files the fork's modules load.
import { fileURLToPath } from "node:url";

const fork = process.env.PLUTO_FORK;
const cacheDir = process.env.PLUTO_HARNESS_CACHE;
if (!fork || !cacheDir) {
  throw new Error(
    "fork-harness: PLUTO_FORK and PLUTO_HARNESS_CACHE must be set; run scripts/fork-replay.sh",
  );
}

const forkDir = fork.replace(/\/+$/, "");
// A path, not a URL's pathname, which would keep a space as `%20`.
const harnessDir = fileURLToPath(new URL(".", import.meta.url));
const isBare = (source) => !/^(\.|\/|@fork\/|node:|virtual:|\0)/.test(source);

const resolveFromFork = {
  name: "pluto-fork-harness:resolve-from-fork",
  enforce: "pre",
  resolveId(source, importer, options) {
    if (importer === undefined || !importer.startsWith(harnessDir) || !isBare(source)) {
      return null;
    }
    return this.resolve(source, `${forkDir}/apps/server/package.json`, {
      ...options,
      skipSelf: true,
    });
  },
};

export default {
  cacheDir,
  plugins: [resolveFromFork],
  resolve: {
    alias: [{ find: /^@fork\//, replacement: `${forkDir}/` }],
  },
  test: {
    include: ["*.test.ts"],
    environment: "node",
    // The scenarios start real processes; one file at a time keeps them apart.
    fileParallelism: false,
    testTimeout: 300_000,
    hookTimeout: 60_000,
  },
};
