# Conventions and testing

## Running tooling
Never install Rust or system packages on the host. Run everything through the container: `scripts/dev.sh image|install|typecheck|lint|test|appimage|shell|exec`. The commands in "Definition of done" below are executed this way.

## OCR assets and display tests
`scripts/fetch-ocr-models.sh` must have been run before the Rust tests: the OCR tests load the models and `libonnxruntime.so` from `src-tauri/resources/ocr/` (git-ignored). Tests that need an X11 display are `#[ignore]`d and run by `scripts/dev.sh test` under xvfb. `scripts/e2e-hover.sh` is the end-to-end check of the hover lookup (see PROGRESS).

## Language
- Everything is written in English: docs, code, commits, `describe`/`it` names and test comments.

## TypeScript code
- Correct typing; no unnecessary `any`. Shared interfaces live in their own files (`src/types/`), never inline when an interface fits.
- Arrow functions always use a block body with an explicit `return` (no implicit returns).
- Avoid inline conditionals (`if (x) return y;`); prefer blocks with `{}`.
- Small functions with a single responsibility; extract repeated logic.

## Rust code
- `cargo clippy -- -D warnings` and `cargo fmt` clean.
- Typed errors (`thiserror`); no `unwrap()` outside tests.
- Platform modules only expose traits; no X11 in the core.

## Tests
Every test, unit or e2e, must be complete: module definition, all branches (if/else), loops, calls with exact parameters, success, failure and edge cases.
- When changing production code, always check for related unit **and** e2e tests and create/update both.
- Assertions verify concrete content (payload sent, response received, exact arguments). No `any`, `expect.anything()` or partial matchers when the full value is known. Exception: non-deterministic values (use `expect.any(Date)` for dates).
- E2E: verify status, relevant headers and the full body.
- AnkiConnect in tests: HTTP mock that validates the exact JSON of each action (`addNote`, `storeMediaFile`, `canAddNotes`, `createModel`).
- Tokenization/deinflection: table of real sentences with the full expected output.

## Definition of done (per task)
1. Code implemented per the roadmap task, without touching unrelated files.
2. Unit and e2e tests created or updated and passing.
3. `npx tsc --noEmit --project tsconfig.json` with no errors.
4. `npx eslint src/ test/ --max-warnings=0` with no warnings (whole codebase, including tests).
5. `cargo clippy -- -D warnings` and `cargo test` with no errors.
6. Roadmap and `PROGRESS.md` updated; ADR recorded if a decision was made.

## Git
- Commit messages in English, no AI attribution.
- Commit and push only when the user asks.

## Confirmations
Require explicit confirmation ("yes" in the current message): deploy/push, migrations, external calls, deleting files or dependencies, irreversible actions.
