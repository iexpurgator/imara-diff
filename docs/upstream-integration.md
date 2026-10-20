# Upstream integration

For the subsequent complete-reproducer checks and additional roundtrip fixes,
see [Open upstream issue bug review](issue-bug-review.md).

## Scope and execution

Upstream: <https://github.com/pascalkuthe/imara-diff>.
Starting HEAD: `f02e46a737c43c0d9c65a42e5e08046bd10fd739`, identical to upstream `master` at inspection.
All already-merged upstream PRs were therefore already present.

Approved sequence: import #38, #50, #49, #51; resolve their interactions;
verify the result; review the remaining open issues separately.
The integration was initially left uncommitted in this checkout. The user
subsequently authorized a local commit, but not pushing, tags, or releases.
PRs closed without merging were reviewed as history, not blindly reintroduced.

## Imported open PRs

| PR | Source head | Changes |
| --- | --- | --- |
| [#38](https://github.com/pascalkuthe/imara-diff/pull/38) | `0e61b784e54beadbe8c712975be3ae9b3d05fc45` | `sources::chars`, character token source, `Hunk::char_diff`, custom-tokenizer `Hunk::granular_diff`; word diffs reuse the same implementation. |
| [#50](https://github.com/pascalkuthe/imara-diff/pull/50) | `c039074759fe2437a87e18fbf2382d3129baeee7` | Gate formatter-only tests and the documentation example on `unified_diff`; test without default features in CI. |
| [#49](https://github.com/pascalkuthe/imara-diff/pull/49) | `e34818e55387eb2605759171fcc51e30f089b6fb` | Use `core` and `alloc`, disable memchr's implicit std dependency, add the default `std` feature and embedded CI builds. |
| [#51](https://github.com/pascalkuthe/imara-diff/pull/51) | `2a1b915848dacbefdf02d0b9c516f48b470af22f` | `Diff::compute_cancellable`, `Diff::compute_with_cancellable`, cancellation propagation through Histogram, Myers, recursion, and fallback. |

### Integration decisions

- Return the concrete `core::str::Chars` from `sources::chars`. The original
  opaque return type hid its `TokenSource` implementation, preventing
  `InternedInput::new(chars(before), chars(after))` from compiling.
- Preserve the closing Rust documentation fence that #49 accidentally removed,
  and combine it with #50's feature guard. Format the feature-gated
  `Diff::unified_diff` reference as code so strict rustdoc builds also work
  without that feature.
- Use `core::sync::atomic` for cancellation, including its unit test, so #51
  works with #49 rather than reintroducing a std requirement.
- Keep both legacy compute methods unchanged at the public API boundary.
- Keep cancellation semantics from #51: the result may be incomplete and must
  be discarded after cancellation. Preprocessing and a currently running LCS
  search or middle-snake split are not interrupted. No fixed time bound or
  measured performance improvement is claimed.
- Pin the hand-checked diff fixtures to LF in `.gitattributes`. Git's CRLF
  conversion otherwise caused a pre-existing Windows snapshot failure. Fixture
  contents and expected diff placement are not changed.
- Resolve two existing Clippy errors (`manual_checked_ops` and
  `useless_borrows_in_formatting`) without changing behavior; otherwise the
  existing strict lint job fails on the installed stable toolchain.

## Remaining upstream issues

This is a local assessment, not a claim that upstream issues have been closed.

| Issue | Assessment |
| --- | --- |
| [#21](https://github.com/pascalkuthe/imara-diff/issues/21) blank-line placement | Existing `postprocess_lines` handles the reduced reported pattern, including a preceding edit. A regression test now checks all three algorithms. This does not promise identical output to Git for every ambiguous diff, nor reproduce the complete original GitButler fixture. |
| [#27](https://github.com/pascalkuthe/imara-diff/issues/27) 0.2 release metadata and migration guide | Still pending: the changelog stops at 0.1.8, and release tags/metadata require a maintainer. No historical release information was invented and no tags/releases were created. |
| [#34](https://github.com/pascalkuthe/imara-diff/issues/34) benchmarking infrastructure | Still pending: no open PR implements it. The discussion asks for reproducible measurements before considering bit vectors. No speculative data-structure rewrite or new benchmark dependency was added. |
| [#39](https://github.com/pascalkuthe/imara-diff/issues/39) stable 1.0 release | Maintainer/API-stability decision, not solved by importing patches. Package version remains 0.2.0. |
| [#40](https://github.com/pascalkuthe/imara-diff/issues/40) variable-granularity iterator | Still pending: no accepted implementation or open PR. The maintainer questioned automatic character refinement and preferred byte ranges over string slices. #38 provides low-level refinement, not the proposed automatic wrapper. |
| [#44](https://github.com/pascalkuthe/imara-diff/issues/44) release containing the context-header fix | The fix `8de65af4621b4d83c35b458a73f544cb68842edb` is already in this checkout, with the existing context-boundary regression test. Publishing upstream is outside this local integration. |
| [#45](https://github.com/pascalkuthe/imara-diff/issues/45) GUI consumers | Informational/tool-discovery request; no library code fix. No GUI application was added. |
| [#48](https://github.com/pascalkuthe/imara-diff/issues/48) release containing the context-header fix | Same existing fix as #44; upstream publishing remains a maintainer action. |

## Verification

Regression tests in `tests/upstream_prs.rs` cover Unicode scalar tokenization,
UTF-8 integrity, character edits/insertions/deletions, empty and identical input,
buffer reuse, line-to-word-to-character refinement, a custom tokenizer, complete
edit-script equivalence when cancellation is unset, reuse after cancellation,
and the reduced blank-line placement case from #21. The internal
`preset_cancel_flag_skips_edit_search` test checks that an already-set flag
skips edit searching for all three algorithms. Disabling the cancellation check
made this test fail; restoring the check made it pass.

Completed verification:

- `cargo test --locked --workspace`: 26 passed (including doctests), 1 ignored.
- The all-features, no-default-features, std-only, and unified-diff-only test
  configurations all passed.
- Rust 1.71.0 tests passed with default and no default features.
- `no_std` checks passed for `thumbv7em-none-eabihf`, with and without
  `unified_diff`, on stable and Rust 1.71.0.
- The stable `thumbv6m-none-eabi` check also passed.
- Clippy with `-D warnings` passed for all targets, both with all features and
  without default features.
- Rustdoc with `-D warnings` passed with and without default features.
- `cargo fmt --all --check` and `git diff --check` passed.
- Before commit authorization, HEAD was unchanged and no files were staged.

Logs are local to `.git/upstream-integration-verification/` (not staged).
The final review was a self-review; no independent agent review was requested.
Fuzzing and Miri were not run. The pre-existing `complex_diffs` test is ignored
by default because it asks for visual inspection. Build checks for embedded
targets do not execute on hardware.
No upstream GitHub issue, PR state, release, or published crate was modified.
