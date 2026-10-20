// Extracts the builtin file-type symbols of the upstream `pierre-icons`
// bundle as white SVGs (colored at paint time) into assets/icons/lang-*.svg.
// usage: node scripts/gen-lang-icons.mjs <pierre-icons-*.js> <token>...
import fs from "node:fs";
import path from "node:path";

const [bundle, ...tokens] = process.argv.slice(2);
const source = fs.readFileSync(bundle, "utf8");
const out = path.join(path.dirname(new URL(import.meta.url).pathname), "..", "assets", "icons");
for (const token of tokens) {
  const match = source.match(
    new RegExp(`<symbol id="file-tree-builtin-${token}" viewBox="([^"]+)">([\\s\\S]*?)</symbol>`),
  );
  if (!match) throw new Error(`no symbol for ${token}`);
  const body = match[2].replaceAll("currentColor", "#ffffff");
  fs.writeFileSync(
    path.join(out, `lang-${token}.svg`),
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${match[1]}">${body}</svg>\n`,
  );
}
console.log(`wrote ${tokens.length} icons`);
