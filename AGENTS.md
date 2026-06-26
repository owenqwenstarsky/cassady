# AGENTS.md

## Release process: build artifacts and create a draft GitHub release

Use this process when preparing a Cassady (`cass`) release. Always create the GitHub release as a **draft** first; do not publish the final release unless the user explicitly asks.

### 1. Review prior releases for consistency

```sh
git fetch origin --tags
gh release list --repo owenqwenstarsky/cassady --limit 5
gh release view --repo owenqwenstarsky/cassady --json tagName,body --jq '.tagName + "\n\n" + .body'
```

Keep release notes consistent with the existing format:

- Title: `## Cassady vX.Y.Z`
- One short summary paragraph.
- `### Downloads` with four bullets in this order: macOS Apple Silicon, Linux x86_64, Linux ARM64, Windows x86_64.
- Note that each archive contains both `cass` and `cassady`; mention SHA-256 files.
- `### Highlights` with concise user-facing bullets.
- Optional `### Upgrade notes` only when compatibility, config, or migration details matter.
- `### Install from source` with a `cargo install --git ... --tag vX.Y.Z` command.
- `### Verification` listing the exact test/build commands used.

### 2. Confirm the version and starting state

```sh
git status --short
VERSION=$(awk -F\" '/^version = / { print $2; exit }' Cargo.toml)
TAG="v${VERSION}"
echo "$TAG"
git log --oneline --decorate -20
```

If the version is wrong, update `Cargo.toml` and `Cargo.lock` first, then commit that change before tagging. Release tags should point at the intended release commit on `main`.

### 3. Run verification before packaging

```sh
cargo +stable test --locked --all-targets
```

### 4. Build all release binaries

Prerequisites for cross-builds: stable Rust, `cargo-zigbuild`, Zig, and the needed Rust targets.

```sh
rustup target add aarch64-apple-darwin x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-pc-windows-gnu
cargo install cargo-zigbuild --locked
```

Build the same four targets used by previous releases:

```sh
cargo +stable build --release --locked --target aarch64-apple-darwin
cargo +stable zigbuild --release --locked --target x86_64-unknown-linux-gnu
cargo +stable zigbuild --release --locked --target aarch64-unknown-linux-gnu
cargo +stable zigbuild --release --locked --target x86_64-pc-windows-gnu
```

### 5. Rebuild the `dist/` artifacts

This creates the unpacked artifact directories, compressed archives, and checksum files for the current `$TAG`. Generate checksums from inside `dist/` so the `.sha256` files contain archive names without a `dist/` prefix.

```sh
VERSION=$(awk -F\" '/^version = / { print $2; exit }' Cargo.toml)
TAG="v${VERSION}"

rm -rf \
  "dist/cassady-${TAG}-aarch64-apple-darwin"* \
  "dist/cassady-${TAG}-x86_64-unknown-linux-gnu"* \
  "dist/cassady-${TAG}-aarch64-unknown-linux-gnu"* \
  "dist/cassady-${TAG}-x86_64-pc-windows-gnu"*

mkdir -p dist

for target in aarch64-apple-darwin x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu; do
  name="cassady-${TAG}-${target}"
  mkdir -p "dist/${name}"
  cp "target/${target}/release/cass" "dist/${name}/cass"
  cp "target/${target}/release/cassady" "dist/${name}/cassady"
  cp README.md "dist/${name}/README.md"
  tar -C dist -czf "dist/${name}.tar.gz" "${name}"
done

win_target=x86_64-pc-windows-gnu
win_name="cassady-${TAG}-${win_target}"
mkdir -p "dist/${win_name}"
cp "target/${win_target}/release/cass.exe" "dist/${win_name}/cass.exe"
cp "target/${win_target}/release/cassady.exe" "dist/${win_name}/cassady.exe"
cp README.md "dist/${win_name}/README.md"
(cd dist && zip -qr "${win_name}.zip" "${win_name}")

(cd dist && for artifact in cassady-${TAG}-*.tar.gz cassady-${TAG}-*.zip; do
  shasum -a 256 "$artifact" > "${artifact}.sha256"
done)
```

Sanity-check the generated artifacts:

```sh
ls -lh dist/cassady-${TAG}-*.tar.gz dist/cassady-${TAG}-*.zip dist/cassady-${TAG}-*.sha256
for sum in dist/cassady-${TAG}-*.sha256; do (cd dist && shasum -a 256 -c "$(basename "$sum")"); done
tar -tzf "dist/cassady-${TAG}-aarch64-apple-darwin.tar.gz" | head
unzip -l "dist/cassady-${TAG}-x86_64-pc-windows-gnu.zip" | head
```

### 6. Write the release notes

Create `dist/RELEASE_NOTES_${TAG}.md` using the same structure as prior releases. Use this template and replace the summary/highlights with the actual changes:

````md
## Cassady vX.Y.Z

Cassady vX.Y.Z ...

### Downloads

- macOS Apple Silicon: `cassady-vX.Y.Z-aarch64-apple-darwin.tar.gz`
- Linux x86_64: `cassady-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`
- Linux ARM64: `cassady-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz`
- Windows x86_64: `cassady-vX.Y.Z-x86_64-pc-windows-gnu.zip`

Each archive contains both `cass` and `cassady`. SHA-256 checksum files are included for every archive.

### Highlights

- ...

### Upgrade notes

...

### Install from source

```sh
cargo install --git https://github.com/owenqwenstarsky/cassady --tag vX.Y.Z
```

### Verification

Built and tested with:

```sh
cargo +stable test --locked --all-targets
cargo +stable build --release --locked --target aarch64-apple-darwin
cargo +stable zigbuild --release --locked --target x86_64-unknown-linux-gnu
cargo +stable zigbuild --release --locked --target aarch64-unknown-linux-gnu
cargo +stable zigbuild --release --locked --target x86_64-pc-windows-gnu
```
````

Check the notes before uploading:

```sh
sed -n '1,220p' "dist/RELEASE_NOTES_${TAG}.md"
```

### 7. Tag the release commit

Only tag after tests pass, artifacts are built, and release notes are ready.

```sh
git fetch origin --tags
git status --short  # should be clean except generated dist/ files
git tag -a "$TAG" -m "Cassady ${TAG}"
git push origin "$TAG"
```

If the tag already exists, stop and ask before deleting or moving it.

### 8. Create the GitHub release as a draft

Upload only the current version's archives and checksum files. Keep `--draft` and `--verify-tag` in the command.

```sh
gh release create "$TAG" \
  "dist/cassady-${TAG}-aarch64-apple-darwin.tar.gz" \
  "dist/cassady-${TAG}-aarch64-apple-darwin.tar.gz.sha256" \
  "dist/cassady-${TAG}-x86_64-unknown-linux-gnu.tar.gz" \
  "dist/cassady-${TAG}-x86_64-unknown-linux-gnu.tar.gz.sha256" \
  "dist/cassady-${TAG}-aarch64-unknown-linux-gnu.tar.gz" \
  "dist/cassady-${TAG}-aarch64-unknown-linux-gnu.tar.gz.sha256" \
  "dist/cassady-${TAG}-x86_64-pc-windows-gnu.zip" \
  "dist/cassady-${TAG}-x86_64-pc-windows-gnu.zip.sha256" \
  --repo owenqwenstarsky/cassady \
  --title "Cassady ${TAG}" \
  --notes-file "dist/RELEASE_NOTES_${TAG}.md" \
  --draft \
  --verify-tag
```

Verify the draft:

```sh
gh release view "$TAG" --repo owenqwenstarsky/cassady --json tagName,name,isDraft,isPrerelease,assets --jq .
```

Do **not** publish the draft or mark it as the final/latest release unless the user explicitly asks.

## NPM package process: prepare and publish the CLI packages

Use this process when preparing Cassady for npm. The npm distribution is a tiny wrapper package plus platform-specific binary packages. Do not publish to npm unless the user explicitly asks; when they ask for a publish-ready setup, make sure the packages can be published with one command.

### Package layout and names

The committed npm tooling generates packages under `dist/npm/` from the current Rust version in `Cargo.toml`:

- Wrapper package: `cassady`
  - Exposes both npm binaries: `cass` and `cassady`.
  - Depends on the platform packages through `optionalDependencies` at the exact same version.
- Platform packages:
  - `@cassady/cli-darwin-arm64` for `aarch64-apple-darwin`
  - `@cassady/cli-linux-x64` for `x86_64-unknown-linux-gnu`
  - `@cassady/cli-linux-arm64` for `aarch64-unknown-linux-gnu`
  - `@cassady/cli-win32-x64` for `x86_64-pc-windows-gnu`

Before the first publish, make sure the npm account owns or has publish access to the `cassady` package name and the `@cassady` scope. If the npm package name or scope changes, update `npm/scripts/lib/release-config.mjs` and regenerate the packages. Scoped packages are published with `--access public`.

### 1. Build the release binaries first

The npm packages copy binaries from `target/<target>/release/`, so run the same verification and target builds used for the GitHub release first:

```sh
cargo +stable test --locked --all-targets
cargo +stable build --release --locked --target aarch64-apple-darwin
cargo +stable zigbuild --release --locked --target x86_64-unknown-linux-gnu
cargo +stable zigbuild --release --locked --target aarch64-unknown-linux-gnu
cargo +stable zigbuild --release --locked --target x86_64-pc-windows-gnu
```

### 2. Generate and verify the npm package directories

```sh
npm run npm:prepare
npm run npm:verify
```

`npm:prepare` rebuilds `dist/npm/` for the current `Cargo.toml` version. `npm:verify` runs `npm publish --dry-run` for each generated package and does not publish anything.

### 3. Publish to npm when explicitly requested

The publish command prepares packages, checks npm auth, runs `npm login` if needed, checks whether each package version already exists, publishes platform packages first, publishes the wrapper last, and verifies that the published versions can be packed from npm. The existence/verification checks intentionally use `npm pack --dry-run` instead of `npm view` because npm registry metadata for newly-created scoped packages can briefly return 404 even after publish succeeds.

```sh
npm run npm:publish
```

Optional checks and controls:

```sh
npm run npm:publish -- --dry-run   # full publish flow without publishing
NPM_TAG=next npm run npm:publish    # publish under a non-latest dist-tag
```

If a version already exists, npm cannot overwrite it. Stop and ask before changing versions, deleting packages, or moving dist-tags.

## Roadmap process: write a new release entry in `ROADMAP.md`

Use this process when planning a future Cassady release. Follow the existing newest-first format in `ROADMAP.md` so new entries look like prior releases.

### 1. Review the existing roadmap format

```sh
sed -n '1,220p' ROADMAP.md
ls plans
```

Current conventions:

- Keep the `# Cassady (Cass) Roadmap` title at the top.
- Add the newest release immediately below the title, above older releases.
- Use headings like `## vX.Y.Z — Short Theme` for planned releases.
- Add `✅ Completed` to the heading only after the release is actually complete.
- Start with a short paragraph: `This release focuses on ...`.
- If there is a detailed plan, reference it with a sentence like: See `plans/PLAN_FILE.md`.
- Group work under `###` area headings such as `Interactive Setup`, `Agent Control`, or `Safety and Reviewability`.
- Use checklist items: `- [ ]` for planned work, `- [x]` for completed work.
- Write main tasks as bold, user-facing outcomes: `**Add a first-run setup wizard.** ...`.
- Use indented bullets for scope details, constraints, and explicit deferrals.

### 2. Decide the release scope

Before editing `ROADMAP.md`, identify:

- Version number: `vX.Y.Z`.
- Short theme: a concise release name after the em dash.
- One-paragraph goal: what the release changes for users.
- 2-4 major areas to group the work.
- Concrete checklist tasks under each area.
- Any intentionally deferred work, especially broad integrations or risky scope.
- Optional plan file under `plans/` if the release needs deeper implementation detail.

Prefer roadmap items that describe outcomes and acceptance criteria, not implementation minutiae. Keep them concise enough to scan.

### 3. Insert the new entry

Place the new release section directly under the top-level title:

```md
# Cassady (Cass) Roadmap

## vX.Y.Z — Short Release Theme

This release focuses on ... See `plans/VX_Y_Z_SHORT_PLAN.md`.

### Area Name

- [ ] **User-facing task title.** Describe the outcome in one sentence.
  - Add key behavior or acceptance criteria.
  - Note constraints or non-goals.

- [ ] **Second task title.** Describe the next outcome.
  - Include compatibility, docs, tests, or safety requirements when relevant.

### Another Area

- [ ] **Another task title.** Describe the outcome.
  - Keep details specific and checkable.

## vPrevious — Existing Theme ✅ Completed
```

If there is no detailed plan file yet, omit the `See ...` sentence rather than linking to a nonexistent file.

### 4. Keep old entries stable

- Do not reorder completed releases except to insert the new release at the top.
- Do not rewrite old completed scopes unless correcting a clear error.
- Use `[x]` only for work that has landed.
- Add `✅ Completed` only when the release has shipped or the user explicitly asks to mark it complete.
- Keep wording consistent with earlier entries: concise headings, bold task names, and nested details.

### 5. Check the edit

```sh
sed -n '1,180p' ROADMAP.md
git diff -- ROADMAP.md
```

## Plan process: write implementation plans in `plans/`

Use this process when creating or updating an implementation plan. All project plans belong in the `plans/` directory; do not put new plan documents at the repository root.

### 1. Review previous plans first

```sh
ls plans
sed -n '1,240p' plans/V0_2_2_ONBOARDING_SETUP_WIZARD_PLAN.md
sed -n '1,220p' plans/V0_2_1_MESSAGE_RENDERING_POLISH_PLAN.md
sed -n '1,220p' plans/V0_2_1_COLLAPSED_TOOL_DENSITY_PLAN.md
```

Also read any plan that is directly related to the new work. For example, read `plans/SECURITY_ACCESS_MODES_PLAN.md` before planning access-control work, or `plans/PROVIDER_MODEL_CONFIG_PLAN.md` before planning provider/config changes.

### 2. Choose the plan filename

Write new plans under `plans/` using uppercase snake-case names:

- Release-scoped plan: `plans/VX_Y_Z_SHORT_THEME_PLAN.md`, e.g. `plans/V0_2_3_CONTEXT_MANAGEMENT_PLAN.md`.
- Feature/follow-up plan: `plans/FEATURE_OR_AREA_PLAN.md`, e.g. `plans/SECURITY_ACCESS_MODES_PLAN.md`.
- Follow-up to an existing release plan: include the version and specific topic, e.g. `plans/V0_2_1_COLLAPSED_TOOL_DENSITY_PLAN.md`.

Prefer one focused plan per coherent feature or release theme. Do not edit `plans/PLAN.md` for new release work; it is the historical MVP plan.

### 3. Match the existing plan structure

Most plans should use this shape, adapted to the task size:

````md
# vX.Y.Z Short Theme Implementation Plan

## Goal

State the user-facing outcome in a short paragraph. If helpful, add a success statement.

## Scope

### In scope

- Concrete included work.
- Supported behavior and user-visible changes.

### Out of scope

- Explicit non-goals and deferred work.
- Integrations or risky scope that should not be pulled into this plan.

## Context or Current State

Describe the relevant files, modules, existing behavior, and constraints.

## Design Principles

1. Principle that guides tradeoffs.
2. Safety, compatibility, or UX constraint.
3. Simplicity or deferral rule.

## Design

Describe the proposed behavior and architecture. Include tables, examples, CLI output, JSON shapes, or module/type sketches when they make implementation clearer.

## Implementation Steps

1. First concrete code/docs step.
2. Next step.
3. Final integration step.

## Tests

- Specific unit/integration tests to add or update.
- Manual checks when automated tests are not enough.

## Documentation

- README, bundled docs, release notes, or roadmap updates required.

## Acceptance Criteria

- Checkable condition that proves the plan is done.
- `cargo fmt` and `cargo test --locked --all-targets` pass.
````

For small follow-ups, it is okay to use the shorter style from `V0_2_1_COLLAPSED_TOOL_DENSITY_PLAN.md`: `Context`, `Goal`, `Scope`, `Design`, `Implementation Steps`, and `Acceptance Criteria`.

### 4. Writing guidelines

- Keep plans implementation-oriented but readable by a future agent.
- Be explicit about in-scope vs out-of-scope work to prevent scope creep.
- Mention exact files/modules when known, such as `src/ui/render.rs` or `src/config.rs`.
- Include examples of expected CLI output, JSON, or UI text when behavior matters.
- Add compatibility and migration notes when config, storage, provider behavior, or public commands change.
- Add docs and tests sections for any user-facing change.
- Prefer ordered implementation steps over vague tasks.
- Do not mark roadmap items complete just because a plan was written.

### 5. Check the plan

```sh
sed -n '1,260p' plans/YOUR_PLAN_FILE.md
git diff -- plans/YOUR_PLAN_FILE.md ROADMAP.md
```

## General project notes

- This is a Rust project. Use `cargo test --locked --all-targets` before handing off code changes when practical.
- Keep release notes user-facing and concise; avoid dumping raw commit logs.
- `dist/` contains generated release artifacts. Rebuild current-version files rather than editing archives by hand.
