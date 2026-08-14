#!/usr/bin/env python3
"""Collect real baseline scores: ask Chuang itself to answer benchmark
statements via channel simulate, then auto-score with benchmark evaluate.

Usage:
  benchmark-baseline.py all [--keep-answers]
  benchmark-baseline.py <benchmark-id> [<benchmark-id> ...] [--keep-answers]

Requires CHUANG_PROXY_API_KEY in the environment (provider.env) and a built
binary. Writes answers to benchmarks/source/<id>.answers-baseline.json unless
--keep-answers is given (then they are kept for audit).
"""

from __future__ import annotations

import json
import subprocess
import sys
import time


ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WORKSPACE = ROOT
SENDER = "u-benchmark-baseline"


def run(cmd, timeout=180):
    try:
        proc = subprocess.run(
            cmd,
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        return proc
    except subprocess.TimeoutExpired:
        return None


def benchmark_cases(bm_id):
    with open(f"{ROOT}/benchmarks/{bm_id}/benchmark.json") as fh:
        data = json.load(fh)
    return data["id"], data["capability"], data["cases"]


def collect_answer(bm_id, case, seq):
    msg_id = f"bm-{bm_id}-{case['id']}-{int(time.time() * 1000)}"
    cmd = [
        "cargo", "run", "-q", "--",
        "channel", "simulate",
        "--workspace-root", WORKSPACE,
        "--message-id", msg_id,
        "--sender-id", SENDER,
        "--text", case["statement"],
        "--json",
    ]
    last = None
    for _ in range(3):
        proc = run(cmd, timeout=180)
        if proc is None:
            last = {"case_id": case["id"], "answer": "<timeout>", "error": True}
            continue
        if proc.returncode != 0:
            last = {
                "case_id": case["id"],
                "answer": f"<collect_error: {proc.stderr.strip()[:300]}>",
                "error": True,
            }
            continue
        try:
            data = json.loads(proc.stdout)
            reply = data.get("outbound", {}).get("text", "")
        except Exception:
            reply = f"<parse_error: {proc.stdout[:300]}>"
        last = {"case_id": case["id"], "answer": reply, "error": False}
        if "PROVIDER_HTTP_ERROR" in reply or "<" in reply[:40]:
            # transient provider failure -> retry the whole turn
            time.sleep(2)
            continue
        break
    return last


def evaluate(bm_id, answers_path, keep_answers):
    cmd = [
        "cargo", "run", "-q", "--",
        "benchmark", "evaluate",
        "--id", bm_id,
        "--answers", answers_path,
        "--record",
        "--json",
    ]
    for _ in range(2):
        proc = run(cmd, timeout=240)
        if proc is None:
            continue
        if proc.returncode == 0:
            break
        if "PROVIDER_HTTP_ERROR" in proc.stderr or "502" in proc.stderr:
            time.sleep(3)
            continue
        break
    if proc is None:
        print("  evaluate TIMEOUT")
        return None
    if proc.returncode != 0:
        print(f"  evaluate FAILED: {proc.stderr.strip()[:400]}")
        return None
    try:
        return json.loads(proc.stdout)
    except Exception:
        print(f"  evaluate output unparseable: {proc.stdout[:400]}")
        return None


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    keep = "--keep-answers" in sys.argv[1:]
    if not args:
        print(__doc__)
        return 2
    ids = [b.strip() for b in args]
    if "all" in ids:
        proc = run(["cargo", "run", "-q", "--", "benchmark", "list", "--json"], timeout=60)
        if proc.returncode != 0:
            print("cannot list benchmarks:", proc.stderr)
            return 1
        ids = json.loads(proc.stdout)

    summary = []
    for bm_id in ids:
        print(f"=== {bm_id} ===")
        _, capability, cases = benchmark_cases(bm_id)
        answers = []
        for i, case in enumerate(cases, 1):
            print(f"  [{i}/{len(cases)}] {case['id']} asking Chuang...", flush=True)
            item = collect_answer(bm_id, case, i)
            answers.append(item)
            if item["error"]:
                print(f"    collect_error: {item['answer'][:160]}")
            else:
                print(f"    reply_len={len(item['answer'])}")
        answers_path = f"{ROOT}/benchmarks/source/{bm_id}.answers-baseline.json"
        with open(answers_path, "w") as fh:
            json.dump(answers, fh, ensure_ascii=False, indent=2)
        print(f"  answers -> {answers_path}")

        receipt = evaluate(bm_id, answers_path, keep)
        if receipt:
            scores = receipt.get("case_scores", [])
            total = sum(s.get("score", 0) for s in scores)
            max_total = sum(s.get("max_score", 0) for s in scores)
            recorded = receipt.get("recorded")
            print(
                f"  SCORE {total}/{max_total} accepted_as_best={bool(recorded and recorded.get('accepted_as_best'))}"
            )
            summary.append(
                {
                    "benchmark": bm_id,
                    "capability": capability,
                    "total": total,
                    "max": max_total,
                    "scores": scores,
                }
            )
        if not keep:
            import os

            os.unlink(answers_path)

    print("\n=== BASELINE SUMMARY ===")
    for item in summary:
        print(f"{item['benchmark']} ({item['capability']}): {item['total']}/{item['max']}")


if __name__ == "__main__":
    sys.exit(main())
