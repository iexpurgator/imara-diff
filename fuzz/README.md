# Fuzz Testing

This directory contains fuzz tests for imara-diff using [cargo-fuzz](https://rust-fuzz.github.io/book/cargo-fuzz.html).

## Running Fuzz Tests

### Prerequisites
- Nightly Rust toolchain: `rustup install nightly`
- cargo-fuzz: `cargo install cargo-fuzz`

### Running a specific target
```bash
# Run for a specific time (e.g., 60 seconds)
cargo +nightly fuzz run comprehensive_diff -- -max_total_time=60

# Run with a specific number of runs
cargo +nightly fuzz run comprehensive_diff -- -runs=1000000
```

### Running all targets
```bash
for target in comprehensive_diff diff_compute_with postprocess_heuristics unified_diff_printer; do
    cargo +nightly fuzz run --release $target -- -max_total_time=60
done
```

### Analyzing coverage
```bash
cargo +nightly fuzz coverage comprehensive_diff
```

## CI Integration

The **Manual CI** workflow runs fuzz tests when explicitly dispatched: 45 seconds per target × 4 targets. The **Release** workflow runs regular workspace tests on Windows/macOS but does not launch fuzzing.

The unified diff fuzzer and `tests/fuzz_regressions.rs` share the same line validator. It accepts the exact standard `\ No newline at end of file` annotation, not arbitrary lines beginning with `\`. The minimized `before = "!", after = "", context_len = 1` case is covered without requiring nightly:

```bash
cargo test --test fuzz_regressions
```

## Adding New Fuzz Targets

1. Create a new file in `fuzz_targets/` directory
2. Add a new `[[bin]]` entry in `Cargo.toml`
3. Update the CI workflow to run the new target
4. Update this README with a description of the new target
