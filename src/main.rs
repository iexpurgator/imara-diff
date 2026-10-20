use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::fs;
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use imara_diff::{Algorithm, BasicLineDiffPrinter, Diff, InternedInput, UnifiedDiffConfig};

const VERSION: &str = match option_env!("IMARA_DIFF_BUILD_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

const HELP: &str = "imara-diff - compare two UTF-8 text files without a Git repository

Usage: imara-diff [OPTIONS] BEFORE AFTER

Use - for one input to read it from stdin. Inputs are never modified.

Options:
  -U, --context N       Context lines (default: 3; maximum: 1048576)
      --algorithm NAME histogram (default), myers, or myers-minimal
      --exit-code      Exit 1 when the files differ (otherwise exit 0)
  -q, --quiet          Print nothing; exit 1 when the files differ
  -h, --help           Show this help
  -V, --version        Show the version
      --               Treat remaining arguments as file paths

Output: plain unified diff on stdout; diagnostics on stderr. No color or pager.
Exit codes: 0 = success; 1 = differences with --exit-code/--quiet; 2 = error.
Binary files (NUL bytes) and invalid UTF-8 are rejected.
";

struct Options {
    paths: [PathBuf; 2],
    algorithm: Algorithm,
    context: u32,
    exit_code: bool,
    quiet: bool,
}

enum Invocation {
    Help,
    Version,
    Diff(Options),
}

fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Invocation, String> {
    let mut paths = Vec::new();
    let mut algorithm = Algorithm::Histogram;
    let mut context = 3;
    let mut exit_code = false;
    let mut quiet = false;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--") => {
                paths.extend(args.map(PathBuf::from));
                break;
            }
            Some("--help" | "-h") => return Ok(Invocation::Help),
            Some("--version" | "-V") => return Ok(Invocation::Version),
            Some("--exit-code") => exit_code = true,
            Some("--quiet" | "-q") => quiet = true,
            Some("--context" | "-U") => {
                context = parse_context(&args.next().ok_or("--context requires a number")?)?;
            }
            Some("--algorithm") => {
                algorithm = parse_algorithm(&args.next().ok_or("--algorithm requires a name")?)?;
            }
            Some(flag) if flag.starts_with("--context=") => {
                context = parse_context(OsStr::new(&flag["--context=".len()..]))?;
            }
            Some(flag) if flag.starts_with("-U") => {
                context = parse_context(OsStr::new(&flag[2..]))?;
            }
            Some(flag) if flag.starts_with("--algorithm=") => {
                algorithm = parse_algorithm(OsStr::new(&flag["--algorithm=".len()..]))?;
            }
            Some(flag) if flag.starts_with('-') && flag != "-" => {
                return Err(format!(
                    "unknown option {flag:?}; use -- before option-like paths"
                ));
            }
            _ => paths.push(PathBuf::from(arg)),
        }
    }
    let paths: [PathBuf; 2] = paths
        .try_into()
        .map_err(|_| "expected exactly two file paths; see --help")?;
    if paths.iter().all(|path| path == Path::new("-")) {
        return Err("only one input can read from stdin".into());
    }
    Ok(Invocation::Diff(Options {
        paths,
        algorithm,
        context,
        exit_code,
        quiet,
    }))
}

fn parse_context(value: &OsStr) -> Result<u32, String> {
    value
        .to_str()
        .and_then(|s| s.parse().ok())
        .filter(|&n| n <= 1024 * 1024)
        .ok_or_else(|| "context must be an integer from 0 to 1048576".into())
}

fn parse_algorithm(value: &OsStr) -> Result<Algorithm, String> {
    match value.to_str() {
        Some("histogram") => Ok(Algorithm::Histogram),
        Some("myers") => Ok(Algorithm::Myers),
        Some("myers-minimal") => Ok(Algorithm::MyersMinimal),
        _ => Err("algorithm must be histogram, myers, or myers-minimal".into()),
    }
}

fn read_input(path: &Path) -> Result<String, Box<dyn Error>> {
    let text = if path == Path::new("-") {
        let mut text = String::new();
        io::stdin().read_to_string(&mut text).map(|_| text)
    } else {
        fs::read_to_string(path)
    }
    .map_err(|error| format!("{path:?}: {error}"))?;
    if text.contains('\0') {
        return Err(
            format!("{path:?}: binary files containing NUL bytes are not supported").into(),
        );
    }
    Ok(text)
}

fn label(prefix: &str, path: &Path) -> Result<String, Box<dyn Error>> {
    let path = path
        .to_str()
        .ok_or("diff header paths must be valid UTF-8")?;
    #[cfg(windows)]
    let path = path.replace('\\', "/");
    let mut label = format!("\"{prefix}/");
    // Git's quoted paths use C escapes and octal bytes, not Rust's Unicode escapes.
    for byte in path.bytes() {
        match byte {
            b'"' | b'\\' => {
                label.push('\\');
                label.push(byte as char);
            }
            0x20..=0x7e => label.push(byte as char),
            _ => write!(label, "\\{byte:03o}")?,
        }
    }
    label.push('"');
    Ok(label)
}

fn run() -> Result<u8, Box<dyn Error>> {
    let invocation = parse(std::env::args_os().skip(1))?;
    let stdout = io::stdout();
    let mut output = BufWriter::new(stdout.lock());
    let status = match invocation {
        Invocation::Help => {
            write!(output, "{HELP}")?;
            0
        }
        Invocation::Version => {
            writeln!(output, "imara-diff {VERSION}")?;
            0
        }
        Invocation::Diff(options) => {
            let [before_path, after_path] = &options.paths;
            let before = read_input(before_path)?;
            let after = read_input(after_path)?;
            if before == after {
                return Ok(0);
            }
            if options.quiet {
                return Ok(1);
            }
            let before_label = label("a", before_path)?;
            let after_label = label("b", after_path)?;
            let input = InternedInput::new(before.as_str(), after.as_str());
            let mut diff = Diff::compute(options.algorithm, &input);
            diff.postprocess_lines(&input);
            let mut config = UnifiedDiffConfig::default();
            config.context_len(options.context);
            writeln!(output, "--- {before_label}\n+++ {after_label}")?;
            write!(
                output,
                "{}",
                diff.unified_diff(&BasicLineDiffPrinter(&input.interner), config, &input)
            )?;
            u8::from(options.exit_code)
        }
    };
    output.flush()?;
    Ok(status)
}

fn main() -> ExitCode {
    match run() {
        Ok(status) => ExitCode::from(status),
        Err(error) => {
            eprintln!("imara-diff: {error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_paths_escape_controls_quotes_and_utf8_bytes() {
        assert_eq!(
            label("a", Path::new("name\t\n\"é.txt")).unwrap(),
            "\"a/name\\011\\012\\\"\\303\\251.txt\""
        );
        #[cfg(unix)]
        assert_eq!(
            label("b", Path::new("dir\\file")).unwrap(),
            "\"b/dir\\\\file\""
        );
        #[cfg(windows)]
        assert_eq!(
            label("b", Path::new("dir\\file")).unwrap(),
            "\"b/dir/file\""
        );
    }

    #[test]
    fn header_paths_reject_non_unicode_instead_of_corrupting_the_name() {
        #[cfg(unix)]
        let invalid = {
            use std::os::unix::ffi::OsStringExt;
            OsString::from_vec(vec![0xff])
        };
        #[cfg(windows)]
        let invalid = {
            use std::os::windows::ffi::OsStringExt;
            OsString::from_wide(&[0xd800])
        };
        #[cfg(any(unix, windows))]
        assert!(label("a", Path::new(&invalid)).is_err());
    }
}
