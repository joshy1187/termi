# Releasing Termi

Termi releases are tagged from a clean `main` commit and built on GitHub's
Ubuntu 24.04 runner. The release workflow publishes an amd64 Debian package, an
x86_64 AppImage, an x86_64 Linux tarball, SHA-256 checksums, and build-provenance
attestations.

## 1. Prepare the version

Update the version in `Cargo.toml`, then refresh the lockfile with the pinned
toolchain. Confirm `README.md`, `product.md`, package examples, and the planned
tag all use the same version. Move relevant Unreleased changelog entries into a
dated version section only when the release commit is final.

```bash
cargo update --workspace
cargo metadata --locked --no-deps --format-version 1 \
  | jq -r '.packages[0].version'
```

Review every dependency change rather than accepting a broad lockfile rewrite
without explanation.

## 2. Run release gates

Install the tools and Ubuntu packages listed in `README.md`, then run:

```bash
./scripts/check.sh
cargo deny --locked check advisories sources
cargo about generate --locked --fail \
  --output-file THIRD_PARTY_NOTICES.html about.hbs
./packaging/build-packages.sh "$(cargo metadata --locked --no-deps \
  --format-version 1 | jq -r '.packages[0].version')"
./scripts/verify-packages.sh
git diff --check
git status --short
```

Perform the manual matrix in `docs/TESTING.md`. Generated
`THIRD_PARTY_NOTICES.html` and `dist/` are intentionally ignored and must not be
committed.

For a reproducibility spot check, save the first `SHA256SUMS`, rebuild with the
same clean commit and `SOURCE_DATE_EPOCH`, and compare the results.
The AppImage build downloads only the linuxdeploy, output-plugin, and runtime
files whose SHA-256 values are pinned in `packaging/build-packages.sh`; an
upstream `continuous` asset update must fail closed until those pins are
reviewed and updated deliberately.

## 3. Review the release commit

The release commit must contain no credentials, private terminal data, local
paths in public docs, or generated artifacts. Check the complete staged diff,
not only its summary. Confirm CI is green on the exact commit and that the
license notice generator succeeds from `Cargo.lock`.

## 4. Tag and publish

Create an annotated tag whose value exactly matches `v` plus the Cargo version:

```bash
VERSION=$(cargo metadata --locked --no-deps --format-version 1 \
  | jq -r '.packages[0].version')
git tag -a "v$VERSION" -m "Termi $VERSION"
git push origin main
git push origin "v$VERSION"
```

The release workflow rejects a mismatched tag. It validates source, regenerates
full notices, builds and verifies packages, uploads workflow artifacts, creates
provenance, and creates or updates the GitHub Release.

## 5. Verify publication

Download the release artifacts into an empty directory and verify:

```bash
sha256sum --check SHA256SUMS
gh attestation verify termi_*.deb --repo OWNER/REPOSITORY
gh attestation verify termi-*.AppImage --repo OWNER/REPOSITORY
gh attestation verify termi-*.tar.gz --repo OWNER/REPOSITORY
```

Install the Debian package on a clean Ubuntu 24.04 x86_64 system, launch it from
the desktop entry, and repeat the CLI smoke tests. Separately mark the AppImage
executable, run its `--version` command, and launch its GUI. Confirm release
notes describe known limitations and the supported target accurately.

If publication fails after artifacts were created, fix the workflow on a new
commit and tag rather than silently moving a public release tag, unless the tag
was never pushed or announced.
