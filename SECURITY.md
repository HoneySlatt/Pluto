# Security

Pluto has no release yet: only the `main` branch is maintained, and fixes land there.

## Reporting a vulnerability

Report it privately, through GitHub's private vulnerability reporting: the repository's
**Security** tab, then **Report a vulnerability**
(<https://github.com/HoneySlatt/Pluto/security/advisories/new>). Do not open a public issue for
it.

Say what is affected, how to reproduce it, and what it lets an attacker do. The report is
answered as soon as the owner can; a fix and its advisory are published together.

## What counts

Pluto drives coding agents on the owner's machine, so these matter most: a way for a model's
output, a web page, a repository or a server to run code, read files or secrets, or send data off
the machine without the owner's action; a crash on input from outside; and anything that breaks
the rules of [AGENTS.md](AGENTS.md#rules-the-code-depends-on).
