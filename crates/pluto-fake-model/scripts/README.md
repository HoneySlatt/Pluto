# The fake model's scripts

Their format is in [`src/script.rs`](../src/script.rs).

`workload.json` is the workload of M1.6 ([ADR-0026](../../../docs/adr/0026-the-rivals-measured-how.md)).
Turns 0 to 2 are a short coding turn each: a shell command, a file read, then a Markdown answer of
about 1 KB with a list, a code block and a table, as the fixture's long thread
(`scripts/seed-fixture.mjs`). Turn 3 is a long answer, about a minute at the script's pace, for 15
threads streaming at once. A client starts a turn by sending a prompt that holds `[turn N]`.
