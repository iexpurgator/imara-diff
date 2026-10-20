#![cfg(all(feature = "std", feature = "unified_diff", not(miri)))]

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

#[path = "support/temp_dir.rs"]
mod temp_dir;

struct Files(PathBuf);

impl Files {
    fn new(before: &[u8], after: &[u8]) -> Self {
        let base = std::env::temp_dir().join(format!("imara-diff-cli-{}", std::process::id()));
        let path = temp_dir::create_temp_dir(&base);
        fs::write(path.join("before.txt"), before).unwrap();
        fs::write(path.join("after.txt"), after).unwrap();
        Self(path)
    }

    fn run(&self, args: &[&str], stdin: Option<&[u8]>) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_imara-diff"))
            .current_dir(&self.0)
            .args(args)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(input) = stdin {
            child.stdin.take().unwrap().write_all(input).unwrap();
        }
        child.wait_with_output().unwrap()
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn prints_a_plain_unified_patch_without_modifying_files() {
    let files = Files::new(b"old\nkeep\n", b"new\nkeep\n");
    let output = files.run(&["before.txt", "after.txt"], None);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        concat!(
            "--- \"a/before.txt\"\n+++ \"b/after.txt\"\n",
            "@@ -1,2 +1,2 @@\n-old\n+new\n keep\n",
        )
    );
    assert_eq!(
        fs::read(files.0.join("before.txt")).unwrap(),
        b"old\nkeep\n"
    );
    assert_eq!(fs::read(files.0.join("after.txt")).unwrap(), b"new\nkeep\n");
}

#[test]
fn identical_files_produce_no_output_for_all_status_modes() {
    let files = Files::new(b"same", b"same");
    for flags in [vec![], vec!["--exit-code"], vec!["--quiet"]] {
        let mut args = flags;
        args.extend(["before.txt", "after.txt"]);
        let output = files.run(&args, None);
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn exit_code_and_quiet_distinguish_differences_from_errors() {
    let files = Files::new(b"a\n", b"b\n");
    let output = files.run(&["--exit-code", "before.txt", "after.txt"], None);
    assert_eq!(output.status.code(), Some(1));
    assert!(!output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    let output = files.run(&["--quiet", "before.txt", "after.txt"], None);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn supports_algorithms_and_zero_context_option_forms() {
    let files = Files::new(b"keep\na\nend\n", b"keep\nb\nend\n");
    for algorithm in ["histogram", "myers", "myers-minimal"] {
        for context in [
            vec!["-U", "0"],
            vec!["-U0"],
            vec!["--context", "0"],
            vec!["--context=0"],
        ] {
            let mut args = vec!["--algorithm", algorithm];
            args.extend(context);
            args.extend(["before.txt", "after.txt"]);
            let output = files.run(&args, None);
            assert_eq!(output.status.code(), Some(0));
            assert!(output.stderr.is_empty());
            assert!(String::from_utf8(output.stdout)
                .unwrap()
                .ends_with("@@ -2,1 +2,1 @@\n-a\n+b\n"));
        }
    }
    assert_eq!(
        files
            .run(&["--algorithm=myers", "before.txt", "after.txt"], None)
            .status
            .code(),
        Some(0)
    );
}

#[test]
fn accepts_stdin_on_either_side_and_preserves_missing_newlines() {
    let files = Files::new(b"a", b"b");
    for (args, input) in [
        (vec!["-", "after.txt"], b"a".as_slice()),
        (vec!["before.txt", "-"], b"b".as_slice()),
    ] {
        let output = files.run(&args, Some(input));
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        assert!(String::from_utf8(output.stdout).unwrap().ends_with(
            "@@ -1,1 +1,1 @@\n-a\n\\ No newline at end of file\n+b\n\\ No newline at end of file\n"
        ));
    }
}

#[test]
fn reports_invalid_arguments_and_io_errors_only_on_stderr() {
    let files = Files::new(b"a\n", b"b\n");
    for args in [
        vec![],
        vec!["before.txt"],
        vec!["before.txt", "after.txt", "extra"],
        vec!["--unknown"],
        vec!["--context"],
        vec!["--algorithm"],
        vec!["--context", "-1", "before.txt", "after.txt"],
        vec!["--context=4294967296", "before.txt", "after.txt"],
        vec!["--context=1048577", "before.txt", "after.txt"],
        vec!["--algorithm=unknown", "before.txt", "after.txt"],
        vec!["-", "-"],
        vec!["missing.txt", "after.txt"],
        vec!["before.txt", "missing.txt"],
        vec![".", "after.txt"],
    ] {
        let output = files.run(&args, None);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        assert!(!output.stderr.is_empty(), "{args:?}");
    }
}

#[test]
fn rejects_binary_and_invalid_utf8_inputs_including_quiet_mode() {
    for content in [b"a\0b".as_slice(), b"\xff".as_slice()] {
        let files = Files::new(content, b"text\n");
        for args in [
            vec!["before.txt", "after.txt"],
            vec!["--quiet", "before.txt", "after.txt"],
            vec!["after.txt", "before.txt"],
        ] {
            let output = files.run(&args, None);
            assert_eq!(output.status.code(), Some(2));
            assert!(output.stdout.is_empty());
            assert!(!output.stderr.is_empty());
        }
        let output = files.run(&["-", "after.txt"], Some(content));
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn supports_spaces_unicode_paths_and_option_like_file_names() {
    let files = Files::new(b"a\n", b"b\n");
    for (before, after, args) in [
        (
            "old file.txt",
            "new file.txt",
            vec!["old file.txt", "new file.txt"],
        ),
        ("-old", "-new", vec!["--", "-old", "-new"]),
        ("café.txt", "mới.txt", vec!["café.txt", "mới.txt"]),
    ] {
        fs::copy(files.0.join("before.txt"), files.0.join(before)).unwrap();
        fs::copy(files.0.join("after.txt"), files.0.join(after)).unwrap();
        let output = files.run(&args, None);
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        assert!(String::from_utf8(output.stdout)
            .unwrap()
            .ends_with("@@ -1,1 +1,1 @@\n-a\n+b\n"));
    }
}

#[test]
fn help_and_version_do_not_need_inputs() {
    let files = Files::new(b"", b"");
    for flag in ["--help", "-h", "--version", "-V"] {
        let output = files.run(&[flag], None);
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        if flag.contains('h') {
            assert!(text.contains("--exit-code"));
            assert!(text.contains("--context"));
        } else {
            assert_eq!(
                text.trim(),
                format!(
                    "imara-diff {}",
                    option_env!("IMARA_DIFF_BUILD_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
                )
            );
        }
    }
}
