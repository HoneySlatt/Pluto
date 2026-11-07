# Notices

Pluto is released under the MIT license (`LICENSE`). It ports the client of
[T3 Code](https://t3.codes) (MIT, Copyright (c) 2026 T3
Tools Inc.), whose notice `LICENSE` keeps; the built-in environment themes
(`assets/themes/built-in-dark.json`) come from its `themePalettes.ts`.

Pluto's logo (`assets/logo/pluto.svg`) is not under MIT: its author keeps its rights, and
`assets/logo/README.md` says how it may be shown.

Bundled assets and their licenses:

| Asset | Source | License | Text |
| --- | --- | --- | --- |
| Geist, Geist Mono fonts | Vercel | SIL OFL 1.1 | `assets/fonts/OFL.txt` |
| Lucide icons (`assets/icons`, `assets/lucide`) | Lucide Contributors | ISC (portions MIT, Feather) | `assets/icons/LUCIDE-LICENSE` |
| `diffs-symbol-*` file status icons | @pierre/diffs | Apache-2.0 | `assets/PIERRE-DIFFS-LICENSE` |
| `lang-*` file type icons, recolored white (`scripts/gen-lang-icons.mjs`) | @pierre/trees | Apache-2.0 | `assets/PIERRE-DIFFS-LICENSE` (same text), `assets/PIERRE-TREES-NOTICE.md` |
| pierre-dark, pierre-light code themes | @pierre/theme | MIT | `assets/PIERRE-THEME-LICENSE` |

The provider logos (`assets/icons/provider-*`, `claude.svg`, `openai.svg`,
and their copies in `crates/pluto-ffi/assets/marks`) are trademarks of their owners
(Anthropic, OpenAI, Google, Moonshot AI, Mistral AI, Zhipu AI, xAI, GitHub,
Anysphere, Ollama and the others shown). They only identify each provider;
Pluto is not affiliated with these companies.

The Rust crates the programs link keep their own licenses; their notices are
generated with cargo-about (`android/app/notice.gradle.kts` for the Android app,
shown on its Open source licenses page).
