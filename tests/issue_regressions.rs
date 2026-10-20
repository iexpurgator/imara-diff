#![cfg(feature = "unified_diff")]

use imara_diff::{Algorithm, BasicLineDiffPrinter, Diff, InternedInput, UnifiedDiffConfig};

const ALGORITHMS: [Algorithm; 3] = [
    Algorithm::Histogram,
    Algorithm::Myers,
    Algorithm::MyersMinimal,
];

// Original reproducer from https://github.com/pascalkuthe/imara-diff/issues/44.
const ISSUE_44_BEFORE: &str = "scalar user_id = sql\"TEXT\";\nscalar phone_number = sql\"TEXT\";\nscalar preferences = sql\"TEXT\";\nscalar user_name = sql\"TEXT\";\n\ntable RegisteredUser {\n\tuser_id;\n\tphone_number;\n\tpreferences;\n\tuser_name;\n};\n";
const ISSUE_44_AFTER: &str = "scalar user_id = sql\"TEXT\";\nscalar phone_number = sql\"TEXT\";\nscalar preferences = sql\"TEXT\";\nscalar user_name = sql\"TEXT\";\n\ntable RegisteredUser {\n\tuser_id @primary_key;\n\tphone_number @index.unique;\n\tpreferences;\n\tuser_name;\n};\n";

fn patch(before: &str, after: &str, algorithm: Algorithm, context: u32) -> String {
    let input = InternedInput::new(before, after);
    let mut diff = Diff::compute(algorithm, &input);
    diff.postprocess_lines(&input);
    let mut config = UnifiedDiffConfig::default();
    config.context_len(context);
    diff.unified_diff(&BasicLineDiffPrinter(&input.interner), config, &input)
        .to_string()
}

#[test]
fn issue_21_original_reproducer_matches_expected_blank_line_placement() {
    // Preserved from krlvi/imara-diff@f259243, src/tests.rs::correct_new_line.
    for algorithm in ALGORITHMS {
        let actual = patch(
            include_str!("fixtures/issue-21.before"),
            include_str!("fixtures/issue-21.after"),
            algorithm,
            3,
        );
        assert_eq!(
            actual,
            include_str!("fixtures/issue-21.diff"),
            "{algorithm:?}"
        );
    }
}

#[test]
fn issues_44_and_48_context_header_matches_the_lines_actually_printed() {
    for algorithm in ALGORITHMS {
        // Default context starts at line 4, not at the start of the file.
        let actual = patch(ISSUE_44_BEFORE, ISSUE_44_AFTER, algorithm, 3);
        assert_eq!(
            actual,
            concat!(
                "@@ -4,8 +4,8 @@\n",
                " scalar user_name = sql\"TEXT\";\n",
                " \n",
                " table RegisteredUser {\n",
                "-\tuser_id;\n",
                "-\tphone_number;\n",
                "+\tuser_id @primary_key;\n",
                "+\tphone_number @index.unique;\n",
                " \tpreferences;\n",
                " \tuser_name;\n",
                " };\n",
            ),
            "{algorithm:?}"
        );
    }
}

#[test]
fn unified_diff_marks_lines_without_a_final_newline() {
    for algorithm in ALGORITHMS {
        for (before, after, expected) in [
            ("a", "b", "@@ -1,1 +1,1 @@\n-a\n\\ No newline at end of file\n+b\n\\ No newline at end of file\n"),
            ("a\n", "a", "@@ -1,1 +1,1 @@\n-a\n+a\n\\ No newline at end of file\n"),
            ("a", "a\n", "@@ -1,1 +1,1 @@\n-a\n\\ No newline at end of file\n+a\n"),
            ("old\nend", "new\nend", "@@ -1,2 +1,2 @@\n-old\n+new\n end\n\\ No newline at end of file\n"),
        ] {
            assert_eq!(patch(before, after, algorithm, 3), expected, "{algorithm:?}: {before:?} -> {after:?}");
        }
    }
}

#[test]
fn empty_unified_ranges_use_the_preceding_line_number() {
    for algorithm in ALGORITHMS {
        for (before, after, expected) in [
            ("", "a\n", "@@ -0,0 +1,1 @@\n+a\n"),
            ("a\n", "", "@@ -1,1 +0,0 @@\n-a\n"),
            ("a\nb\n", "x\na\nb\n", "@@ -0,0 +1,1 @@\n+x\n"),
            ("a\nb\n", "a\nx\nb\n", "@@ -1,0 +2,1 @@\n+x\n"),
            ("a\nb\n", "a\nb\nx\n", "@@ -2,0 +3,1 @@\n+x\n"),
            ("x\na\nb\n", "a\nb\n", "@@ -1,1 +0,0 @@\n-x\n"),
            ("a\nx\nb\n", "a\nb\n", "@@ -2,1 +1,0 @@\n-x\n"),
            ("a\nb\nx\n", "a\nb\n", "@@ -3,1 +2,0 @@\n-x\n"),
        ] {
            assert_eq!(
                patch(before, after, algorithm, 0),
                expected,
                "{algorithm:?}: {before:?} -> {after:?}"
            );
        }
    }
}

#[cfg(not(miri))]
mod git_interop {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "imara-diff-issue-roundtrip-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn roundtrip(&self, before: &str, after: &str, patch: &str) {
            if patch.is_empty() {
                assert_eq!(before, after);
                return;
            }
            let file = self.0.join("file.txt");
            fs::write(&file, before).unwrap();
            fs::write(
                self.0.join("change.patch"),
                format!("--- a/file.txt\n+++ b/file.txt\n{patch}"),
            )
            .unwrap();
            for reverse in [false, true] {
                let mut command = Command::new("git");
                command
                    .current_dir(&self.0)
                    .env_remove("GIT_DIR")
                    .env_remove("GIT_WORK_TREE")
                    .args([
                        "-c",
                        "core.autocrlf=false",
                        "apply",
                        "--no-index",
                        "--unidiff-zero",
                        "--whitespace=nowarn",
                    ]);
                if reverse {
                    command.arg("--reverse");
                }
                let output = command
                    .arg("change.patch")
                    .output()
                    .expect("roundtrip tests require Git on PATH");
                assert!(
                    output.status.success(),
                    "git apply (reverse={reverse}): {}\n{patch}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert_eq!(
                    fs::read(&file).unwrap(),
                    if reverse { before } else { after }.as_bytes(),
                    "git apply (reverse={reverse}) produced different file contents\n{patch}"
                );
            }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn reported_cases_and_boundaries_roundtrip_through_git_apply() {
        let directory = TestDir::new();
        for algorithm in ALGORITHMS {
            for context in [0, 1, 3, u32::MAX] {
                for (before, after) in [
                    ("", ""),
                    ("same", "same"),
                    ("", "a\n"),
                    ("a\n", ""),
                    ("a\nb\n", "x\na\nb\n"),
                    ("a\nb\n", "a\nx\nb\n"),
                    ("a\nb\n", "a\nb\nx\n"),
                    ("a", "b"),
                    ("a\n", "a"),
                    ("a", "a\n"),
                    ("old\nend", "new\nend"),
                    ("", "🦀"),
                    ("\n", "\n\n"),
                    ("head\r\nold\r\ntail", "head\r\nnew\r\ntail"),
                    (
                        include_str!("fixtures/issue-21.before"),
                        include_str!("fixtures/issue-21.after"),
                    ),
                    (ISSUE_44_BEFORE, ISSUE_44_AFTER),
                ] {
                    directory.roundtrip(before, after, &patch(before, after, algorithm, context));
                }
            }
        }
    }
}
