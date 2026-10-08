# AGENTS.md

## Project Goals

- This repository builds a standalone Topic Desk desktop application without depending on the DeepSeek Harness runtime.
- The production desktop targets are Windows, macOS, and Linux. The shared core must not prevent future iOS adaptation.
- The desktop shell uses Tauri 2, the UI uses React/Vite, and the collection and SQLite business core use Rust.

## Source Code Comments

- Every source file must include a description of its module or responsibility.
- Public types, public functions, Tauri commands, database transactions, and non-obvious business rules must be documented.
- Comments should explain constraints and rationale rather than restating the code line by line.

## Repository Hygiene

- `.DS_Store` files must not exist in the repository. Delete them immediately when found.
- Do not commit API keys, credentials, SQLite runtime data, logs, build artifacts, or signing materials.
- Preserve the user's existing uncommitted changes. Do not reset or overwrite unrelated files.

## Architectural Boundaries

- React pages may access local capabilities only through typed Tauri commands and events.
- Network collection, SQLite, proxy handling, timers, and credential access must reside exclusively in the Rust backend.
- Treat all content returned by sources as untrusted input. Validate and normalize it before persistence or display.
- Open original topic links in the dedicated article-reading pane on the right side of the main window, allowing only HTTP(S) navigation. External pages must never receive the main UI WebView's privileges.
- Place platform-specific capabilities in the `desktop` adapter or a future `mobile` adapter; they must not contaminate the shared core.

## Data Invariants

- Topic identity priority is: platform-stable ID, normalized URL, then normalized title.
- Keep the SHA-256 input format as `v1\0<platform_code>\0<identity_kind>\0<normalized_identity>`.
- Rank, popularity, and collection time must not participate in identity computation.
- Duplicate topics retain the first observed title, URL, publication time, and creation time.
- A failure in one source must not prevent other sources from committing successfully.

## Model Credential Invariants

- Model API keys must be encrypted with AES-256-GCM and an independently generated random nonce for every encryption operation. SQLite may persist only the algorithm identifier, nonce, and authenticated ciphertext; plaintext key columns or plaintext settings are prohibited.
- The master key used to encrypt model credentials must be stored in a separate random key file inside the application data directory. On supported platforms, access must be restricted to the current user. The key must never be written to SQLite, logs, or frontend state.
- API keys may be decrypted only on demand in the native Rust layer. Plaintext must never be returned to the WebView, must use zeroizable memory, and must be released as soon as possible after constructing the model request headers.
- Reject model requests when the ciphertext algorithm is unsupported, the master key is missing, the nonce is invalid, or authentication fails. Never fall back to plaintext reads or ignore integrity failures.
- Database migrations must successfully encrypt legacy credentials before removing the plaintext structure within the same transaction. If migration fails, preserve the old data and schema version; partial migration states are prohibited.
- The application must not access the operating system Keychain, Credential Manager, or Secret Service, and must not provide browser password saving, importing, or autofill capabilities.
- The built-in article-reading WebView must use temporary data storage and must not create persistent WebCrypto master keys or website password material.
- Version 5 model ciphertext encrypted with a legacy system credential-store master key must no longer be read. During upgrade, clear that ciphertext and require the user to enter the API key again.

## Documentation Boundaries

- The README should describe only product capabilities, user workflows, supported platforms, usage, and user-visible privacy behavior.
- Implementation details such as architectural boundaries, database schemas, encryption algorithms, key management, migration steps, and code-validation commands belong in `AGENTS.md`, source comments, or developer documentation—not in the README.
- The README may state that “API keys are encrypted locally and are never exposed to pages,” but must not describe algorithms, fields, master-key locations, or version-migration procedures.

## Testing Requirements

- Every new or changed feature must include unit tests at the lowest practical layer. A feature is incomplete when its tests are missing, skipped, or disabled.
- Test the successful path, input and boundary validation, and important failure behavior. Every bug fix must add a regression test that reproduces the previous failure.
- Keep frontend business and presentation rules in pure testable modules instead of burying them inside React components. Test Rust network, storage, and native boundaries with deterministic fixtures, in-memory databases, or fakes; unit tests must not require live external services.
- Changes pushed to GitHub and pull requests must pass the repository's GitHub Actions workflow on every supported desktop operating system before release.

## Completion Checks

- For TypeScript changes, run at minimum `pnpm typecheck`, `pnpm test`, and the relevant focused tests.
- For Rust changes, run at minimum `cargo fmt --check`, `cargo test`, and `cargo clippy -- -D warnings`.
- Before delivering a desktop build, run a smoke test using a real installer package on the corresponding operating system.
