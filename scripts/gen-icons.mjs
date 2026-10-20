// Converts lucide-react icon nodes into white-stroked SVG files that egui can
// tint. Use the lucide-react version pinned by upstream (0.564.0).
//
// usage: node scripts/gen-icons.mjs <lucide-react package dir> <name>...
import fs from "node:fs";
import path from "node:path";

const [pkg, ...names] = process.argv.slice(2);
const out = path.join(path.dirname(new URL(import.meta.url).pathname), "..", "assets", "icons");
for (const name of names) {
  const source = fs.readFileSync(path.join(pkg, "dist", "esm", "icons", `${name}.js`), "utf8");
  const literal = source.match(/const __iconNode = (\[[\s\S]*?\]);/)?.[1];
  if (!literal) throw new Error(`no icon node in ${name}`);
  const nodes = JSON.parse(literal.replace(/(\w+):/g, '"$1":'));
  const body = nodes
    .map(([tag, attrs]) => {
      const pairs = Object.entries(attrs)
        .filter(([key]) => key !== "key")
        .map(([key, value]) => `${key}="${value}"`);
      return `  <${tag} ${pairs.join(" ")} />`;
    })
    .join("\n");
  fs.writeFileSync(
    path.join(out, `${name}.svg`),
    `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#ffffff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">\n${body}\n</svg>\n`,
  );
}
console.log(`wrote ${names.length} icons`);
