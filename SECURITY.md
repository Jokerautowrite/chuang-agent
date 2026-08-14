# Security Policy

## Supported versions

Security fixes currently target the latest commit on `main`. Chuang Agent is Linux-first; Windows native execution is not currently part of the supported security boundary.

## Reporting a vulnerability

Do not open a public issue for suspected credentials, authentication bypasses, unsafe command execution, memory disclosure, or plugin boundary violations. Contact the maintainer using the address listed in the README and include only the minimum reproduction data needed. Never include live tokens, private memory, or personal data.

## Security defaults

- Real actuators, control adapters, external workers, providers, and proactive sends are opt-in.
- Provider credentials are referenced by environment-variable name; they must not be committed to config files.
- Governance cannot be disabled. Destructive or external actions require explicit approval and an exact target.
- Subagents may propose memory writes but cannot write core memory directly.

Please run `cargo audit` and a full-history Gitleaks scan before publishing a release.
