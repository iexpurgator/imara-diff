#![cfg(feature = "unified_diff")]

#[path = "../fuzz/validate_unified.rs"]
mod validate_unified;

use imara_diff::{Algorithm, BasicLineDiffPrinter, Diff, InternedInput, UnifiedDiffConfig};

#[test]
fn fuzz_validator_accepts_missing_newline_annotations() {
    for (before, after) in [("!", ""), ("", "!"), ("a", "b"), ("old\nend", "new\nend")] {
        for algorithm in [
            Algorithm::Histogram,
            Algorithm::Myers,
            Algorithm::MyersMinimal,
        ] {
            let input = InternedInput::new(before, after);
            let mut diff = Diff::compute(algorithm, &input);
            diff.postprocess_lines(&input);
            let mut config = UnifiedDiffConfig::default();
            config.context_len(1);
            let output = diff
                .unified_diff(&BasicLineDiffPrinter(&input.interner), config, &input)
                .to_string();
            assert!(output.contains("\\ No newline at end of file"));
            validate_unified::assert_valid_diff_lines(&output);
        }
    }
}

#[test]
fn fuzz_validator_still_rejects_other_backslash_lines() {
    for invalid in [
        r"\ not a valid annotation",
        r"\ No newline at end of file extra",
    ] {
        assert!(
            std::panic::catch_unwind(|| validate_unified::assert_valid_diff_lines(invalid))
                .is_err()
        );
    }
}
