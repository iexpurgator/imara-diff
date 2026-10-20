# Open upstream issue bug review

Scope: the eight open issues at <https://github.com/pascalkuthe/imara-diff/issues>,
including their discussion and linked reproducers. Starting local commit:
`de93efb`. This follow-up is intentionally uncommitted; no upstream state,
release, tag, or remote branch is changed.

## Classification

| Issue | Classification | Local result |
| --- | --- | --- |
| [#21](https://github.com/pascalkuthe/imara-diff/issues/21) | Diff presentation / ambiguous blank-line placement, not evidence of data corruption | Rebuilt the complete linked GitButler reproducer. Current `postprocess_lines` produces the requested placement for Histogram, Myers, and MyersMinimal. No algorithm fix needed. |
| [#44](https://github.com/pascalkuthe/imara-diff/issues/44), [#48](https://github.com/pascalkuthe/imara-diff/issues/48) | Release requests referencing a real unified-diff context-header bug | The historical bug is already fixed locally by `8de65af`. Added the exact #44 input as a regression and verified that reintroducing the old cursor initialization makes it fail. #48 supplies no separate reproducer. Publishing still requires an upstream maintainer. |
| [#27](https://github.com/pascalkuthe/imara-diff/issues/27) | Missing release documentation / metadata | Not a runtime bug; no release history or migration information invented. |
| [#34](https://github.com/pascalkuthe/imara-diff/issues/34) | Benchmark infrastructure and possible memory optimization | No demonstrated correctness failure or measured performance regression; no speculative bit-vector rewrite. |
| [#39](https://github.com/pascalkuthe/imara-diff/issues/39) | Stable release / API policy | Maintainer decision, not a code defect. |
| [#40](https://github.com/pascalkuthe/imara-diff/issues/40) | Convenience API feature request | Not a bug; no new API added. |
| [#45](https://github.com/pascalkuthe/imara-diff/issues/45) | Question about GUI tools | Not a library bug. |

## Additional roundtrip bugs found and fixed

Boundary checks around #44/#48 exposed two remaining defects in
`BasicLineDiffPrinter`, independent of the already-fixed historical issue:

1. **Empty hunk ranges were one line too high.** The printer unconditionally
   converted both starting positions to 1-based indices. Unified diff instead
   uses the preceding line number when the range length is zero. For example,
   inserting into an empty file must print `-0,0`, not `-1,0`. Only the basic
   printer's header conversion changed; custom printers and diff algorithms
   are untouched.
2. **Missing final newlines were lost.** The printer added a display newline
   without the standard `\ No newline at end of file` marker. Git could not
   apply a replacement such as `"a" -> "b"`. Removed, added, and unchanged
   context lines now retain their missing-newline status in the patch.

Both defects had failing tests before their fixes. The real `git apply`
roundtrip test also failed on the missing-newline case before its fix.
Existing `simple_insert` snapshots were updated only to include the newly
required marker; their input, hunks, and other output remain unchanged.

## Reproducer provenance

`tests/fixtures/issue-21.{before,after,diff}` comes from
[krlvi/imara-diff@f259243, `correct_new_line`](https://github.com/krlvi/imara-diff/blob/f259243ba3873189744b81b7a8d7e0dde58b0700/src/tests.rs#L159).
The original expected patch's final header said `-38,9 +38,5`, despite the
files having only 45 and 41 lines. It is corrected to `-38,8 +38,4`, independently
confirmed with `git diff --no-index`; the expected blank-line placement is
preserved. These fixtures are pinned to LF for Windows checkouts.

## Tests and limits

`tests/issue_regressions.rs` covers the two original reports, zero-length
ranges at the beginning/middle/end, missing final newlines on either side and
in context, Unicode, CRLF, identical/empty inputs, and maximum context length.

The Git interoperability test checks 16 input pairs, three algorithms, and
four context lengths (192 configurations). Each nonempty patch is applied
forward and in reverse, checking exact bytes both ways (336 Git invocations).
It requires Git on PATH, uses a private temporary directory, and is excluded
under Miri. No new crate dependency was added.

Verification completed:

- `cargo test --locked --workspace`: **31 passed, 1 pre-existing ignored**,
  including the Git roundtrips and doctests.
- Tests passed with all features, no default features, std-only, and
  unified-diff-only configurations.
- Rust 1.71.0 tests passed with default and no default features.
- Embedded checks passed on `thumbv7em-none-eabihf`, including the unified
  printer on stable and Rust 1.71.0, and on `thumbv6m-none-eabi` with the printer.
- Strict Clippy and rustdoc passed with and without default features.
- `cargo fmt --all --check` and `git diff --check` passed.
- HEAD remains `de93efb`; no files are staged and no commit or push was made.

Logs and the fetched issue pages are in `.git/issue-review/` (not staged).
Review is local and self-reviewed, not independent review. Fuzzing, Miri, and
publishing are outside this pass. API requests and release-policy issues above
are not claimed to be resolved.
