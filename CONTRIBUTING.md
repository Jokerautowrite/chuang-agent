# Contributing

Thank you for helping improve Chuang Agent.

1. Open an issue before a design-level change.
2. Keep each change focused on one slot or contract.
3. New adapters require a fake implementation, contract tests, documented opt-in, and fail-closed behavior.
4. Never commit credentials, runtime databases, personal identity files, local paths, receipts, or private operational logs.
5. Run the release checks before submitting:

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo audit
```

By contributing, you agree that your contribution is distributed under the repository's custom non-commercial license. Commercial use still requires written authorization from the copyright holder.
