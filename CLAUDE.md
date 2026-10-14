# anki_mining

Desktop app (Tauri/Rust + TypeScript) that captures Japanese text from any source (YouTube, players, games, apps), segments and highlights words, and creates Anki cards in specific decks.

## Start of every session (mandatory)
1. Read `docs/PROGRESS.md` — current state, next task and blockers.
2. Read the matching phase in `docs/03-roadmap.md`.
3. Check `docs/02-architecture.md` and `docs/04-decisions.md` before any design decision.
4. Follow `docs/05-conventions-and-testing.md`.

## End of every session (mandatory)
1. Tick completed tasks in `docs/03-roadmap.md` (`[x]`).
2. Update `docs/PROGRESS.md`: phase status, "Next step", session log entry, blockers.
3. Record new decisions in `docs/04-decisions.md`.
4. Never mark a task as done without passing tests and clean `tsc`, eslint and `cargo clippy`.

## Tooling
Nothing but Docker is installed on the host. Run all build, lint and test commands through `scripts/dev.sh` (see `docs/05-conventions-and-testing.md`). Never install Rust or system packages on the host. Before running the Rust tests, run `scripts/fetch-ocr-models.sh` once (OCR models and ONNX Runtime, git-ignored). The project overview is in `README.md`; the app is already published as version 0.1.0 (see `docs/PROGRESS.md`).

## Project rules
- Do not implement anything outside the current roadmap task; note ideas in the "Backlog" of `PROGRESS.md`.
- If in doubt about a business rule or technical fact, ask before implementing.
- Language: everything (docs, code, commits, test names, comments) is written in English.
- No AI attribution in any file or commit.
