#!/usr/bin/env python3
"""
Commit subject lint 工具 — R132 接力 2+3 結構性發現落工具化

R132 commit history 結構性品質審計留下 2 條結構性發現給 owner M 簽收:
  接力 2: commit subject 偏長 (> 72 字元)
  接力 3: `chore:` / `fix:` / `feat:` / `refactor:` / `docs:` 等無 scope
          (e.g. `chore: rotate log` 應為 `chore(engineering-log): ...`)

本工具:
  - 掃 git log 所有 (或 --since window 內) commit subject
  - 列出 (a) subject > MAX_LEN 字元的 (b) 已知 type 無 scope 的
  - 純 audit 報告, 退出碼永遠 0 (不 fail-closed, 不入 K42 chain 護衛,
    留 owner M 簽收時決定要不要落 fail-closed)

對齊 k41_chore_treadmill.py / k0_measure.py 風格:
  - 純 stdlib (不引外部依賴)
  - bytes → utf-8 errors=replace 避 Windows cp950 解碼雷
  - 退出碼 0 (純 audit 工具, 非護衛)

不擴 K42 chain 20 (R97 飽和契約守住): 本工具不進 chain 護衛,
留 owner M 簽收時決定是否併入 commit-msg 護衛。

R167 落地。
"""
import argparse
import json
import subprocess
import sys
from pathlib import Path

# Conventional commit 標準 type (對齊 k41_chore_treadmill.py GOVERNANCE_PREFIXES
# 以外的 type 全集)。 owner M 簽收時可調。
KNOWN_TYPES = (
    "feat", "fix", "refactor", "perf", "test", "docs",
    "chore", "ci", "style", "build", "revert",
)

# 預設上限 72 字元, 對齊 git 慣例 (git log --oneline 預設截 72)。
DEFAULT_MAX_LEN = 72


def git_log_subjects(since: str | None, limit: int | None) -> list[tuple[str, str]]:
    """回傳 [(short_sha, subject), ...] (新→舊)。

    Args:
        since: e.g. "7d", "30d", "2026-01-01"; None = 全部歷史
        limit: 最大筆數; None = 無上限
    """
    cmd = ["git", "log", "--pretty=format:%h\t%s"]
    if since:
        cmd.append(f"--since={since}")
    if limit:
        cmd.append(f"-n{limit}")
    raw = subprocess.run(
        cmd,
        capture_output=True,
        check=True,
        cwd=Path(__file__).resolve().parent.parent,
    ).stdout
    rows = []
    for line in raw.decode("utf-8", errors="replace").splitlines():
        if not line or "\t" not in line:
            continue
        sha, subject = line.split("\t", 1)
        rows.append((sha, subject))
    return rows


def parse_type_scope(subject: str) -> tuple[str | None, str | None, str | None, str | None]:
    """解析 conventional commit prefix。

    Returns:
        (type, scope, rest, full) — 任一欄位缺失回 (None, ...)。
        e.g. `chore(scripts): add X` → ("chore", "scripts", "add X", full)
             `chore: rotate log`    → ("chore", None,    "rotate log", full)
             `WIP on foo`           → (None, None, None, "WIP on foo")
    """
    if ":" not in subject:
        return (None, None, None, subject)
    head, rest = subject.split(":", 1)
    rest = rest.lstrip()
    if "(" in head and head.endswith(")"):
        ctype, _, scope = head.partition("(")
        ctype = ctype.strip()
        scope = scope[:-1].strip()  # 去尾巴 ')'
    else:
        ctype = head.strip()
        scope = None
    if ctype not in KNOWN_TYPES:
        return (None, None, None, subject)
    return (ctype, scope, rest, subject)


def lint(subjects: list[tuple[str, str]], max_len: int) -> dict:
    """掃 subjects, 回傳 {long_subjects, no_scope, total}。

    Args:
        subjects: [(sha, subject), ...]
        max_len: subject 字元上限 (含 type 前綴)
    """
    long_subjects = []
    no_scope = []
    for sha, subject in subjects:
        if len(subject) > max_len:
            long_subjects.append({"sha": sha, "subject": subject, "length": len(subject)})
        ctype, scope, _rest, full = parse_type_scope(subject)
        if ctype is not None and scope is None:
            no_scope.append({"sha": sha, "subject": subject, "type": ctype})
    return {
        "long_subjects": long_subjects,
        "no_scope": no_scope,
        "total": len(subjects),
    }


def format_report(result: dict, max_len: int) -> str:
    lines = []
    lines.append(f"Commit subject lint report (max_len={max_len})")
    lines.append(f"  total commits scanned: {result['total']}")
    lines.append(f"  long subjects (> {max_len} chars): {len(result['long_subjects'])}")
    lines.append(f"  known-type without scope: {len(result['no_scope'])}")
    lines.append("")
    if result["long_subjects"]:
        lines.append(f"--- Long subjects (top 10 by length) ---")
        for row in sorted(result["long_subjects"], key=lambda r: r["length"], reverse=True)[:10]:
            lines.append(f"  [{row['length']:>4}] {row['sha']} {row['subject'][:120]}{'...' if len(row['subject']) > 120 else ''}")
        lines.append("")
    if result["no_scope"]:
        lines.append(f"--- No-scope subjects (top 10 by recency) ---")
        for row in result["no_scope"][:10]:
            lines.append(f"  [{row['type']:>8}] {row['sha']} {row['subject'][:120]}{'...' if len(row['subject']) > 120 else ''}")
        lines.append("")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Lint commit subjects for length + scope.")
    parser.add_argument("--since", help="git --since value (e.g. 7d, 30d, 2026-01-01)", default=None)
    parser.add_argument("--limit", type=int, help="max commits to scan", default=None)
    parser.add_argument("--max-len", type=int, default=DEFAULT_MAX_LEN,
                        help=f"subject max length (default {DEFAULT_MAX_LEN})")
    parser.add_argument("--json", action="store_true", help="machine-readable JSON to stdout")
    args = parser.parse_args(argv)

    subjects = git_log_subjects(args.since, args.limit)
    result = lint(subjects, args.max_len)

    if args.json:
        print(json.dumps(result, ensure_ascii=False, indent=2))
    else:
        print(format_report(result, args.max_len))

    # 純 audit 工具, 退出碼永遠 0 (R167 決策: 不入 chain 護衛)
    return 0


if __name__ == "__main__":
    sys.exit(main())
