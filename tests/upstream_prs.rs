use std::sync::atomic::{AtomicBool, Ordering};

use imara_diff::sources::chars;
use imara_diff::{Algorithm, Diff, Hunk, InternedInput};

const ALGORITHMS: [Algorithm; 3] = [
    Algorithm::Histogram,
    Algorithm::Myers,
    Algorithm::MyersMinimal,
];

#[test]
fn character_source_can_be_interned_without_splitting_utf8() {
    let input = InternedInput::new(chars("é🦀e\u{301}"), chars("é🐙e\u{301}"));
    let before: String = input.before.iter().map(|&id| input.interner[id]).collect();
    assert_eq!(before, "é🦀e\u{301}");
    assert_eq!(input.before.len(), 4);
    for algorithm in ALGORITHMS {
        let diff = Diff::compute(algorithm, &input);
        assert_eq!(
            diff.hunks().collect::<Vec<_>>(),
            vec![Hunk {
                before: 1..2,
                after: 1..2
            }]
        );
    }
}

#[test]
fn character_hunks_handle_edits_empty_inputs_and_reused_buffers() {
    let mut tokens = InternedInput::default();
    let mut diff = Diff::default();
    for (before, after, expected) in [
        (
            "café\n",
            "cafe\n",
            vec![Hunk {
                before: 3..4,
                after: 3..4,
            }],
        ),
        (
            "",
            "🦀\n",
            vec![Hunk {
                before: 0..0,
                after: 0..2,
            }],
        ),
        (
            "é\n",
            "",
            vec![Hunk {
                before: 0..2,
                after: 0..0,
            }],
        ),
        ("", "", vec![]),
        ("same", "same", vec![]),
    ] {
        let input = InternedInput::new(before, after);
        let hunk = Hunk {
            before: 0..input.before.len() as u32,
            after: 0..input.after.len() as u32,
        };
        hunk.char_diff(&input, &mut tokens, &mut diff);
        assert_eq!(diff.hunks().collect::<Vec<_>>(), expected);
        assert_eq!(
            tokens
                .before
                .iter()
                .map(|&id| tokens.interner[id])
                .collect::<String>(),
            before
        );
        assert_eq!(
            tokens
                .after
                .iter()
                .map(|&id| tokens.interner[id])
                .collect::<String>(),
            after
        );
    }
}

#[test]
fn character_diff_can_refine_a_word_diff() {
    let input = InternedInput::new("hello cat\n", "hello cut\n");
    let line_diff = Diff::compute(Algorithm::Histogram, &input);
    let mut words = InternedInput::default();
    let mut word_diff = Diff::default();
    line_diff
        .hunks()
        .next()
        .unwrap()
        .latin_word_diff(&input, &mut words, &mut word_diff);
    let mut characters = InternedInput::default();
    let mut character_diff = Diff::default();
    word_diff
        .hunks()
        .next()
        .unwrap()
        .char_diff(&words, &mut characters, &mut character_diff);
    assert_eq!(
        character_diff.hunks().collect::<Vec<_>>(),
        vec![Hunk {
            before: 1..2,
            after: 1..2
        }]
    );
}

#[test]
fn postprocessing_keeps_blank_lines_with_the_preceding_deletion() {
    let input = InternedInput::new("old\nkeep\n\na\n\nfoo\n\nb\n", "new\nkeep\n\na\n\nb\n");
    for algorithm in ALGORITHMS {
        let mut diff = Diff::compute(algorithm, &input);
        diff.postprocess_lines(&input);
        assert_eq!(
            diff.hunks().collect::<Vec<_>>(),
            vec![
                Hunk {
                    before: 0..1,
                    after: 0..1
                },
                Hunk {
                    before: 5..7,
                    after: 5..5
                },
            ],
            "{algorithm:?}"
        );
    }
}

#[test]
fn cancellation_unset_preserves_the_entire_edit_script() {
    let cancel = AtomicBool::new(false);
    let repeated_before = "a\nb\n".repeat(100);
    let repeated_after = "b\na\n".repeat(100);
    for (before, after) in [
        ("", ""),
        ("", "added\n"),
        ("removed\n", ""),
        ("same\n", "same\n"),
        ("a\nb\nc\nd\ne\nf\n", "a\nX\nc\nd\nY\nf\nZ\n"),
        (repeated_before.as_str(), repeated_after.as_str()),
    ] {
        let input = InternedInput::new(before, after);
        for algorithm in ALGORITHMS {
            let plain = Diff::compute(algorithm, &input);
            let cancellable = Diff::compute_cancellable(algorithm, &input, &cancel);
            assert_eq!(
                plain.hunks().collect::<Vec<_>>(),
                cancellable.hunks().collect::<Vec<_>>(),
                "{algorithm:?}: {before:?} -> {after:?}"
            );
        }
    }
}

#[test]
fn cancelled_diff_buffers_can_be_reused() {
    let cancel = AtomicBool::new(true);
    let input = InternedInput::new("a\nb\nc\nd\n", "b\na\nd\nc\n");
    let mut diff = Diff::default();
    for algorithm in ALGORITHMS {
        diff.compute_with_cancellable(
            algorithm,
            &input.before,
            &input.after,
            input.interner.num_tokens(),
            &cancel,
        );
        cancel.store(false, Ordering::Relaxed);
        diff.compute_with_cancellable(
            algorithm,
            &input.before,
            &input.after,
            input.interner.num_tokens(),
            &cancel,
        );
        let plain = Diff::compute(algorithm, &input);
        assert_eq!(
            diff.hunks().collect::<Vec<_>>(),
            plain.hunks().collect::<Vec<_>>()
        );
        cancel.store(true, Ordering::Relaxed);
        let _ = Diff::compute_cancellable(algorithm, &input, &cancel);
        diff.compute_with(
            algorithm,
            &input.before,
            &input.after,
            input.interner.num_tokens(),
        );
        assert_eq!(
            diff.hunks().collect::<Vec<_>>(),
            plain.hunks().collect::<Vec<_>>()
        );
    }
}

#[test]
fn granular_diff_uses_the_supplied_tokenizer() {
    let input = InternedInput::new("one two", "one three");
    let mut tokens = InternedInput::default();
    let mut diff = Diff::default();
    Hunk {
        before: 0..1,
        after: 0..1,
    }
    .granular_diff(str::split_whitespace, &input, &mut tokens, &mut diff);
    assert_eq!(
        diff.hunks().collect::<Vec<_>>(),
        vec![Hunk {
            before: 1..2,
            after: 1..2
        }]
    );
    assert_eq!(tokens.interner[tokens.before[1]], "two");
    assert_eq!(tokens.interner[tokens.after[1]], "three");
}
