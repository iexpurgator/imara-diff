#![no_main]

use imara_diff::{Algorithm, BasicLineDiffPrinter, Diff, InternedInput, UnifiedDiffConfig};
use libfuzzer_sys::arbitrary;
use libfuzzer_sys::fuzz_target;

#[path = "../validate_unified.rs"]
mod validate_unified;

#[derive(arbitrary::Arbitrary, Debug)]
struct Input<'a> {
    before: &'a str,
    after: &'a str,
    context_len: u32,
}

/// Tests unified diff printing with:
/// - Different context lengths (0-10)
/// - Various input combinations
/// - Validates normal diff prefixes and the standard missing-newline annotation
fn do_fuzz(
    Input {
        before,
        after,
        context_len,
    }: Input<'_>,
) {
    let input = InternedInput::new(before, after);

    // Test with different algorithms
    for algorithm in [
        Algorithm::Histogram,
        Algorithm::Myers,
        Algorithm::MyersMinimal,
    ] {
        let mut diff = Diff::compute(algorithm, &input);

        // Postprocess before printing
        diff.postprocess_lines(&input);

        // Create printer and config
        let printer = BasicLineDiffPrinter(&input.interner);
        let mut config = UnifiedDiffConfig::default();
        config.context_len(context_len);

        // Generate unified diff
        let unified = diff.unified_diff(&printer, config, &input);
        let output = unified.to_string();

        validate_unified::assert_valid_diff_lines(&output);
    }
}

fuzz_target!(|input: Input<'_>| {
    do_fuzz(input);
});
