#!/usr/bin/env python3
"""Sign an operator approval ticket with the local private key.

Private key default: ~/.config/chuang-agent/operator-approval.sk (base64 raw 32 bytes).
Public trust anchor (verify only): /etc/chuang-agent/operator-approval.pub

Payload must match src/operator_approval.rs OperatorApprovalPayload field order
(serde_json object key order as defined on the Rust struct).

Usage:
  scripts/chuang-sign-operator-approval-ticket.py \\
    --approval-id ID --call-id CALL --call-fingerprint HEX \\
    --target-fingerprint HEX --workspace-fingerprint HEX \\
    --policy-marker MARKER --operator-ref REF --evidence-ref REF \\
    [--out PATH]
"""
from __future__ import annotations

import argparse
import base64
import json
import os
import sys
from datetime import datetime, timezone
from pathlib import Path

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey


SCHEMA_VERSION = 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--approval-id", required=True)
    parser.add_argument("--call-id", required=True)
    parser.add_argument("--call-fingerprint", required=True)
    parser.add_argument("--target-fingerprint", required=True)
    parser.add_argument("--workspace-fingerprint", required=True)
    parser.add_argument("--policy-marker", required=True)
    parser.add_argument("--operator-ref", default="local-operator")
    parser.add_argument("--evidence-ref", default="manual")
    parser.add_argument(
        "--secret-key-file",
        default=os.path.expanduser("~/.config/chuang-agent/operator-approval.sk"),
    )
    parser.add_argument("--issued-at", default="")
    parser.add_argument("--out", default="")
    args = parser.parse_args()

    sk_path = Path(args.secret_key_file)
    if not sk_path.is_file():
        print(f"missing secret key: {sk_path}", file=sys.stderr)
        return 2
    raw = base64.b64decode(sk_path.read_text(encoding="utf-8").strip())
    if len(raw) != 32:
        print("secret key must be base64 of 32 raw bytes", file=sys.stderr)
        return 2
    signing_key = Ed25519PrivateKey.from_private_bytes(raw)

    issued_at = args.issued_at or datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    # Field order must match Rust OperatorApprovalPayload Serialize order.
    payload = {
        "schema_version": SCHEMA_VERSION,
        "approval_id": args.approval_id,
        "call_id": args.call_id,
        "call_fingerprint": args.call_fingerprint,
        "target_fingerprint": args.target_fingerprint,
        "workspace_fingerprint": args.workspace_fingerprint,
        "policy_marker": args.policy_marker,
        "operator_ref": args.operator_ref,
        "evidence_ref": args.evidence_ref,
        "issued_at": issued_at,
    }
    canonical = json.dumps(payload, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    signature = signing_key.sign(canonical)
    ticket = {
        **payload,
        "signature": base64.b64encode(signature).decode("ascii"),
    }
    body = json.dumps(ticket, indent=2, ensure_ascii=False) + "\n"
    if args.out:
        Path(args.out).write_text(body, encoding="utf-8")
        print(args.out)
    else:
        sys.stdout.write(body)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
