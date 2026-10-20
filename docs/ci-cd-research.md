# Multi-platform CLI CI/CD research

## Status and recommendation

This document records the research preceding implementation. Manual-only
workflows now exist locally; see [the run guide](manual-builds.md) for their
actual behavior. The later manual-only/calendar-version requirement overrides
this document's earlier scheduling suggestions. No GitHub workflow has been
triggered and no commit, remote tag, push, or release has been created.

Recommendation: keep normal CI small; start release qualification with **13
native target triples**, plus an optional macOS universal2 archive. Separately
probe the **46 candidate triples** inventoried below. Promote a target to the
supported release matrix only after its pinned toolchain/image actually builds
this source and its validation policy passes.

**46 is a candidate inventory, not 46 successful builds, and not 46 different
OS/CPU combinations.** GNU/musl and ARM ABI variants account for some entries.
The inventory intersects the current `cross` support table with the installed
Rust 1.98.1 rustup manifest, then adds the three MSVC and two Darwin targets.
A listed cross image may still be unavailable for a particular pinned release,
and a rustup target entry alone does not supply a linker, SDK, or runtime.

## What is already verified locally

All these Windows executables have static CRT linkage. Their direct DLL imports
are exactly `KERNEL32.dll`, `ntdll.dll`, and
`api-ms-win-core-synch-l1-2-0.dll`; their delayed-import directories are empty.
They do not import VCRUNTIME, MSVCP, UCRTBASE, or api-ms-win-crt DLLs.

| Target | Local artifact | Evidence |
| --- | --- | --- |
| `x86_64-pc-windows-msvc` | `target/release/imara-diff.exe` | Release workspace: 42 passed, 1 pre-existing ignored; CLI runs. |
| `i686-pc-windows-msvc` | `target/i686-pc-windows-msvc/release/imara-diff.exe` | 9 release CLI integration tests pass on this x64 Windows host via 32-bit compatibility. |
| `aarch64-pc-windows-msvc` | `target/aarch64-pc-windows-msvc/release/imara-diff.exe` | Cross-build succeeds; PE machine is ARM64; imports checked. **Not executed locally.** |

Logs and target inventory are local under `.git/ci-research/` and
`.git/static-build-verification/`. Current Rust MSVC documentation requires
Windows 10+ / Windows Server 2016+; static CRT does not restore Windows 7 support.
No clean Windows VM compatibility test has been performed.

## Current repository constraints

- Existing CI checks Rust 1.71 and stable, no-default-features and embedded
  library builds, tests, Miri, Clippy/docs, and fuzzing, but only uses Ubuntu.
- The CLI needs `std` and `unified_diff`. Bare-metal targets tested for the
  library must not be advertised as native CLI release targets.
- The CLI and its library dependencies do not introduce C build scripts or
  third-party runtime DLLs. This makes cross-compilation relatively simple,
  but it does not remove the need for target runtime/system libraries.
- `tests/cli.rs` launches the built CLI as a child process. The issue regression
  suite also invokes Git. Native/VM test jobs need Git; full cross test runs
  must handle child-process execution, not just the test harness executable.
- `.cargo/config.toml` scopes static CRT flags to Windows MSVC. Global
  `RUSTFLAGS` or `CARGO_ENCODED_RUSTFLAGS` can override them: do not set common
  Linux linker flags globally across the Windows jobs.
- Keep MSRV checks separate from release compiler selection. Newly supported
  target triples need not exist in Rust 1.71. Pin a qualified release compiler;
  1.98.1 is the compiler used for the local artifacts above.

## First release matrix: 13 targets

Use explicit runner labels rather than relying on the moving meaning of
`*-latest`. GitHub currently documents the native labels shown below [1].

| Family | Targets | Build / test strategy |
| --- | --- | --- |
| Windows x64 | `x86_64-pc-windows-msvc` | `windows-2022`, native build/tests; static CRT and PE import audit. |
| Windows x86 | `i686-pc-windows-msvc` | `windows-2022`, cross-build with MSVC x86 tools; execute tests through Windows compatibility. |
| Windows ARM64 | `aarch64-pc-windows-msvc` | `windows-11-arm`, native build/tests; static CRT and PE import audit. |
| macOS Intel | `x86_64-apple-darwin` | `macos-15-intel`, native build/tests. |
| macOS Apple Silicon | `aarch64-apple-darwin` | `macos-15`, native build/tests. |
| Linux GNU | `x86_64-unknown-linux-gnu`, `i686-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `armv7-unknown-linux-gnueabihf` | Native CI for x64/ARM64 (`ubuntu-22.04`, `ubuntu-24.04-arm`); qualified cross builds for release artifacts and non-native architectures. |
| Linux musl | `x86_64-unknown-linux-musl`, `i686-unknown-linux-musl`, `aarch64-unknown-linux-musl`, `armv7-unknown-linux-musleabihf` | Static binaries; inspect ELF dependencies and execute natively or under QEMU. |

Native CI tests and distribution-artifact tests are different checks: if the
release binary was linked by another toolchain, run smoke tests against that
exact artifact too. Do not treat a successful native Cargo test as proof that
an unexecuted cross-built binary works.

### Linux linkers and ABI compatibility

Use `cargo-zigbuild` for the first Linux candidate builds and explicit glibc
floors where supported, for example:

```sh
rustup target add x86_64-unknown-linux-gnu
cargo zigbuild --locked --release --bin imara-diff --target x86_64-unknown-linux-gnu.2.17
```

A glibc suffix is not a substitute for checking ELF symbol versions and running
on the minimum supported environment. Choose floors per target, never blindly
apply 2.17 to RISC-V or LoongArch. Those architectures have different minimum
runtime requirements [2]. ARM CPU-feature/linker quirks also need a real probe.

`cargo-zigbuild` documents Linux and macOS support, not general BSD/Windows
support. It does not support fully static glibc linkage: use musl for that [3].
Use `cross` with pinned images as the fallback for Linux and the primary probe
mechanism for less common targets [4]. Check the selected image's real glibc
version: the current `cross` main table includes GNU images using glibc 2.39.
A generic `cross build` therefore must not be advertised as a glibc-2.17 build.

For musl artifacts, verify there is no dynamic interpreter / unexpected NEEDED
library; do not infer static linkage merely from the target's name. Never use
`target-cpu=native` in distributable builds.

### macOS universal2

Optionally merge the qualified Intel and ARM64 artifacts with `lipo` on macOS.
This produces one additional archive, not a third native architecture or a
new upstream Rust target. Check both slices and run each on its native runner.
Use Apple's SDK on hosted macOS runners rather than introducing redistributed
SDK images. Developer-ID signing/notarization is a separate, credentialed
step; an unsigned download is not promised to pass Gatekeeper without approval.

## Extended probe inventory

Probe these only on manual/weekly runs initially, not on every pull request.
Use `fail-fast: false`, bounded parallelism/timeouts, and per-target results.
An experimental failure must remain visible, even if it does not block a core
release. Successful compile-only artifacts must be labelled as such.

### Linux: 28 candidates total, including the 8 core Linux targets

```text
aarch64-unknown-linux-gnu
aarch64-unknown-linux-musl
arm-unknown-linux-gnueabi
arm-unknown-linux-gnueabihf
arm-unknown-linux-musleabi
arm-unknown-linux-musleabihf
armv5te-unknown-linux-gnueabi
armv5te-unknown-linux-musleabi
armv7-unknown-linux-gnueabi
armv7-unknown-linux-gnueabihf
armv7-unknown-linux-musleabi
armv7-unknown-linux-musleabihf
i586-unknown-linux-gnu
i586-unknown-linux-musl
i686-unknown-linux-gnu
i686-unknown-linux-musl
loongarch64-unknown-linux-gnu
loongarch64-unknown-linux-musl
powerpc-unknown-linux-gnu
powerpc64-unknown-linux-gnu
powerpc64le-unknown-linux-gnu
riscv64gc-unknown-linux-gnu
riscv64gc-unknown-linux-musl
s390x-unknown-linux-gnu
sparc64-unknown-linux-gnu
thumbv7neon-unknown-linux-gnueabihf
x86_64-unknown-linux-gnu
x86_64-unknown-linux-musl
```

`cross` documents QEMU test support for these families, but also explicitly
warns that emulator failures are not necessarily crate failures [4]. Require
at least a core test pass and CLI smoke tests before promoting each target.

### Additional OS families: 7 candidates

```text
aarch64-unknown-freebsd
i686-unknown-freebsd
x86_64-unknown-freebsd
x86_64-unknown-netbsd
x86_64-unknown-illumos
x86_64-pc-solaris
sparcv9-sun-solaris
```

They appear in the current `cross` support table and the local rustup manifest,
but the table does not promise runtime testing for them. FreeBSD can use a VM
job to run real OS tests; `vmactions/freebsd-vm` documents x86_64 and aarch64
support [5]. Pin the VM release/image, install Rust and Git inside it, and copy
artifacts back explicitly. Other BSD/illumos/Solaris artifacts remain
compile-only until appropriate VM/native tests exist.

### Android: 6 optional candidates

```text
aarch64-linux-android
arm-linux-androideabi
armv7-linux-androideabi
i686-linux-android
thumbv7neon-linux-androideabi
x86_64-linux-android
```

These are candidates from `cross`, not a claim that the CLI has been tested
on a phone. Pin NDK/API level per target and execute CLI smoke tests through
ADB or a suitable emulator before promotion. Native libc test support is not
evidence of compatibility with every Android application/Termux environment.

### Deliberate exclusions from the first stable matrix

- OpenBSD, DragonFly, and removed MIPS rustup targets: no blanket stable
  std-component promise from the inventory used here. A VM/ports toolchain or
  nightly `build-std` would be a separate experimental path, not a hidden fallback.
- iOS/tvOS/watchOS: compile targets do not make a normal desktop-style agent CLI
  deployable; signing, sandboxing, and invocation differ.
- WebAssembly/WASI: possible future runtime-specific artifacts, not native OS
  executables. They require a WASI runtime and a different execution/test setup.
- Windows GNU duplicates: unnecessary for this MSVC-static distribution goal.
- Bare-metal targets: continue library checks, not CLI packaging.

## Workflow layout to implement next

1. **`ci.yml`**: retain existing checks and add native Windows/macOS/Linux CLI
   coverage. PR jobs have read-only permissions, no signing/publishing secrets.
   Do not multiply nightly fuzzing or Miri across the release matrix.
2. **`release.yml`**: manual builds for dry runs; version-tag builds for release.
   Build/validate the qualified core targets, package once per target, upload
   intermediate Actions artifacts, then aggregate into a draft GitHub Release.
   Publishing requires all mandatory targets/checksums to pass. Do not hide a
   missing core asset with `continue-on-error`.
3. **`cross-experimental.yml`**: manual/weekly candidate probes; upload successful
   outputs and a build/test/ABI-status manifest. Promotion is a reviewed matrix
   change, not automatic publication of anything that happened to compile.

Prefer a small explicit matrix plus `cargo-zigbuild`/`cross` over introducing
an installer-generation framework for this one binary. `cargo-dist` becomes
worth evaluating if managed installers/updaters/signing become requirements.
This is a project-size judgment, not a limitation of those tools.

### Commands and validation

```sh
cargo test --locked --workspace
cargo build --locked --release --bin imara-diff --target TARGET
```

Native jobs run the workspace and CLI tests. Cross jobs use the qualified
linker/image and run the artifact itself via a native runner, compatibility
layer, or QEMU when possible. Required smoke cases include `--version`, an
ordinary diff, identical inputs, `--exit-code`/`--quiet`, stdin, and error code 2.
Verify PE/Mach-O/ELF architecture matches the asset name, not only that a file
exists. Windows jobs audit external CRT imports; Linux jobs check static musl
or the declared dynamic glibc floor; macOS jobs audit system-library imports.

### Packaging and publishing

- Asset names: `imara-diff-VERSION-TARGET.zip` for Windows and `.tar.gz` for Unix.
  Include the executable, LICENSE, and CLI usage documentation.
- Create Unix tar archives **before** `upload-artifact`. The action documents
  executable-permission loss when files are uploaded through its zip packaging [6].
- Attach a sorted `SHA256SUMS` file covering final archives, plus a manifest
  recording target, compiler, source commit, linker/image, ABI floor and
  validation status. Keep debug symbols separate if distributing them.
- Pin action dependencies (full verified commit SHAs, or GitHub workflow
  dependency lockfiles where available), Rust/Zig/cross versions and container
  digests; key caches by target/compiler/lockfile/build flags [7].
- Keep normal jobs at `contents: read`; give only the publishing job
  `contents: write`. A repository-scoped `GITHUB_TOKEN` is sufficient for the
  ordinary GitHub Release operation; do not add a PAT by default.
- Optional provenance attestations need `id-token: write` and
  `attestations: write` in the attestation job, with the relevant subject path
  and source association [8]. Do not pass these privileges to untrusted PR jobs.
- Do not combine untrusted PR checkout with privileged `pull_request_target`
  or publishing workflows. Pass tag/version values through quoted arguments,
  not untrusted shell-expression interpolation [7].
- For this fork, enable Actions if needed, push the approved source/workflows,
  dry-run manually, inspect actual results, then create an approved version tag.
  No current local-only change can be built by a remote checkout until pushed.

## Unverified items before rollout

No Linux/macOS/BSD/Android artifact has been built or run in this local Windows
pass. The 46 candidates have not been probed on GitHub. No assumptions are made
about signing credentials, protected environments, clean-machine behavior,
minimum-version runtime compatibility, or the fork's Actions policy settings.
The manual workflow files are now written locally, but have not been enabled
or exercised on GitHub; consult the run guide rather than the earlier rollout
proposal for trigger/version semantics.

## Sources

1. [GitHub-hosted runner labels and architectures](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
2. [Rust platform support and target-specific requirements](https://doc.rust-lang.org/rustc/platform-support.html), [MSVC Windows requirements](https://doc.rust-lang.org/rustc/platform-support/windows-msvc.html)
3. [cargo-zigbuild supported platforms, glibc caveats and universal2](https://github.com/rust-cross/cargo-zigbuild)
4. [cross supported targets and testing guarantees](https://github.com/cross-rs/cross), [image target definitions](https://github.com/cross-rs/cross/blob/main/targets.toml)
5. [FreeBSD VM action: releases, architectures, source sharing](https://github.com/vmactions/freebsd-vm)
6. [upload-artifact: permission preservation and archive behavior](https://github.com/actions/upload-artifact#permission-loss)
7. [GitHub Actions secure-use reference](https://docs.github.com/en/actions/reference/security/secure-use)
8. [Build provenance attestations](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations)
