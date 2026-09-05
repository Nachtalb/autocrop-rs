# Releasing

A release is one tag. Pushing `vX.Y.Z` runs `.github/workflows/release.yml`,
which checks that the tag matches the versions in `Cargo.toml`,
`python/Cargo.toml` and `python/pyproject.toml`, then builds and publishes:

| artefact | targets | destination |
|---|---|---|
| `autocrop` and `autocrop-eval` binaries | Linux x86_64 and aarch64 (glibc and musl), macOS x86_64 and aarch64, Windows x86_64 and aarch64 | GitHub release, with `SHA256SUMS` |
| `autocrop-rs` wheels (abi3, CPython 3.11+) and sdist | manylinux and musllinux x86_64 and aarch64, macOS x86_64 and aarch64, Windows x64 | PyPI |
| `autocrop` crate | | crates.io |

## Cutting a release

1. Bump the version in all three files (same value), run `cargo update -w`
   and `cd python && uv lock`, commit.
2. `git tag vX.Y.Z && git push origin main vX.Y.Z`.
3. Watch the Release workflow. The GitHub release is created with generated
   notes; edit them afterwards if wanted.

The `aarch64` Linux jobs use the `ubuntu-24.04-arm` runner, which GitHub
provides free for public repositories. On a private repository those two
matrix entries need a paid runner or must be switched to cross compilation.

## One-time setup

Both registries are configured for trusted publishing: the workflow proves
its identity to the registry through GitHub's OIDC token, so no API token
is stored as a secret.

### PyPI

PyPI accepts a "pending publisher" for a project that does not exist yet,
so the first release can go through the workflow directly.

1. On pypi.org: account → Publishing → "Add a new pending publisher":
   PyPI project name `autocrop-rs`, owner `Nachtalb`, repository
   `autocrop-rs`, workflow `release.yml`, environment `pypi`.
2. On GitHub: repository Settings → Environments → create `pypi`.
   Optionally require a reviewer so a publish needs a click.

### crates.io

crates.io only lets you configure trusted publishing for a crate that
already exists, so the very first version is published by hand:

1. Log in to crates.io with the GitHub account, verify the e-mail address
   under Account Settings.
2. Account Settings → API Tokens → new token with the `publish-new` scope.
3. Locally: `cargo login` (paste the token), then `cargo publish -p autocrop`
   from the repository root. `cargo publish --dry-run -p autocrop` first shows
   what would be uploaded.
4. On crates.io, crate page → Settings → Trusted Publishing → add GitHub:
   owner `Nachtalb`, repository `autocrop-rs`, workflow `release.yml`,
   environment `crates-io`.
5. On GitHub: Settings → Environments → create `crates-io`.
6. Revoke the API token; from now on the workflow publishes.

The Python bindings crate (`python/`) has `publish = false` and never goes
to crates.io; it only ships as wheels.

## What crates.io checks

`cargo publish` refuses to upload when `Cargo.toml` lacks `license`,
`description` or `repository`, when the package is larger than 10 MB, or when
the version already exists. Versions are permanent: a published version can
be yanked but never replaced, so bump instead of re-tagging.
