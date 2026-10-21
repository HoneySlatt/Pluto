// Bundles every Lucide icon (with the alias names `lucide-react/dynamic`
// exposes as `iconNames`) into one text file the project icon picker reads.
// Use the lucide-react version pinned by upstream (0.564.0).
//
// usage: node scripts/gen-lucide-catalog.mjs <lucide-react package dir>
//
// Format: "name<TAB>file" lines, a blank line, then "file<TAB><svg elements>".
import fs from "node:fs";
import path from "node:path";

const [pkg] = process.argv.slice(2);
const esm = path.join(pkg, "dist", "esm");
const imports = fs.readFileSync(path.join(esm, "dynamicIconImports.js"), "utf8");
const entries = [...imports.matchAll(/"([a-z0-9-]+)": \(\) => import\('\.\/icons\/([a-z0-9-]+)\.js'\)/g)];
const files = new Map();
for (const [, , file] of entries) {
  if (files.has(file)) continue;
  const source = fs.readFileSync(path.join(esm, "icons", `${file}.js`), "utf8");
  const literal = source.match(/const __iconNode = (\[[\s\S]*?\]);/)?.[1];
  if (!literal) throw new Error(`no icon node in ${file}`);
  const nodes = JSON.parse(literal.replace(/(\w+):/g, '"$1":'));
  const body = nodes
    .map(([tag, attrs]) => {
      const pairs = Object.entries(attrs)
        .filter(([key]) => key !== "key")
        .map(([key, value]) => `${key}="${value}"`);
      return `<${tag} ${pairs.join(" ")}/>`;
    })
    .join("");
  files.set(file, body);
}
const out = path.join(path.dirname(new URL(import.meta.url).pathname), "..", "assets", "lucide", "catalog.txt");
fs.writeFileSync(
  out,
  entries.map(([, name, file]) => `${name}\t${file}`).join("\n") +
    "\n\n" +
    [...files].map(([file, body]) => `${file}\t${body}`).join("\n") +
    "\n",
);
console.log(`wrote ${entries.length} names, ${files.size} icons`);
