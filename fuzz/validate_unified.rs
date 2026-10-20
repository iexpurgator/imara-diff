/// Check the line-prefix contract shared by the fuzzer and regression tests.
pub fn assert_valid_diff_lines(output: &str) {
    const VALID_DIFF_LINE_PREFIXES: [char; 4] = [' ', '+', '-', '@'];
    for line in output.lines() {
        if !line.is_empty() && line != r"\ No newline at end of file" {
            let first_char = line.chars().next().unwrap();
            assert!(
                VALID_DIFF_LINE_PREFIXES.contains(&first_char),
                "Invalid diff line prefix: '{}' in line: '{}'",
                first_char,
                line
            );
        }
    }
}
