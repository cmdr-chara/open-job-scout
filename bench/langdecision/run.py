from __future__ import annotations

import json
import math
import os
import re
import statistics
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent
GO_BIN = ROOT / "bin" / "go-bench"
RUST_BIN = ROOT / "bin" / "rust-bench"
RUNS = 5
STARTUP_RUNS = 60

# UI/interactive work matters most for the stated product priorities.
WEIGHTS = {
    "startup_ms": 0.10,
    "tui_ms": 0.30,
    "filter_ms": 0.25,
    "json_ms": 0.15,
    "sqlite_ms": 0.15,
    "html_ms": 0.05,
}


def env() -> dict[str, str]:
    values = os.environ.copy()
    values["BENCH_FIXTURES"] = str(ROOT)
    return values


def run_payload(binary: Path) -> dict[str, float]:
    completed = subprocess.run(
        [str(binary)],
        cwd=ROOT.parent.parent,
        env=env(),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=True,
    )
    return {key: float(value) for key, value in json.loads(completed.stdout).items()}


def median_payload(binary: Path) -> dict[str, float]:
    samples = [run_payload(binary) for _ in range(RUNS)]
    keys = samples[0]
    return {key: statistics.median(sample[key] for sample in samples) for key in keys}


def startup_ms(binary: Path) -> float:
    for _ in range(5):
        subprocess.run([str(binary), "noop"], stdout=subprocess.DEVNULL, check=True)
    samples = []
    for _ in range(STARTUP_RUNS):
        started = time.perf_counter_ns()
        subprocess.run([str(binary), "noop"], stdout=subprocess.DEVNULL, check=True)
        samples.append((time.perf_counter_ns() - started) / 1e6)
    return statistics.median(samples)


def memory_kib(binary: Path) -> int | None:
    completed = subprocess.run(
        ["/usr/bin/time", "-v", str(binary)],
        cwd=ROOT.parent.parent,
        env=env(),
        text=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        check=True,
    )
    match = re.search(r"Maximum resident set size \(kbytes\):\s+(\d+)", completed.stderr)
    return int(match.group(1)) if match else None


def geometric_speedup(go: dict[str, float], rust: dict[str, float]) -> float:
    total = 0.0
    for name, weight in WEIGHTS.items():
        speedup = go[name] / rust[name]
        total += weight * math.log(speedup)
    return math.exp(total)


def format_row(label: str, go_value: float, rust_value: float, unit: str = "ms") -> str:
    speedup = go_value / rust_value
    return f"| {label} | {go_value:.3f} {unit} | {rust_value:.3f} {unit} | {speedup:.2f}x |"


def main() -> None:
    go = median_payload(GO_BIN)
    rust = median_payload(RUST_BIN)
    go["startup_ms"] = startup_ms(GO_BIN)
    rust["startup_ms"] = startup_ms(RUST_BIN)
    go_memory = memory_kib(GO_BIN)
    rust_memory = memory_kib(RUST_BIN)
    weighted = geometric_speedup(go, rust)
    decision = "RUST" if weighted >= 1.50 else "GO"

    lines = [
        "# OpenJobScout Go vs Rust decision benchmark",
        "",
        "`Rust speedup` is Go time / Rust time, so values above 1.00x favor Rust.",
        "",
        "| Workload | Go | Rust | Rust speedup |",
        "|---|---:|---:|---:|",
        format_row("Process startup", go["startup_ms"], rust["startup_ms"]),
        format_row("TUI render/update batch", go["tui_ms"], rust["tui_ms"]),
        format_row("10k-job filter/sort", go["filter_ms"], rust["filter_ms"]),
        format_row("10k-job JSON parse", go["json_ms"], rust["json_ms"]),
        format_row("SQLite filtered query", go["sqlite_ms"], rust["sqlite_ms"]),
        format_row("1k-card HTML parse", go["html_ms"], rust["html_ms"]),
        "",
        f"**Weighted meaningful-workload Rust speedup: {weighted:.2f}x**",
        f"**Decision using the agreed 1.50x threshold: {decision}**",
        "",
    ]
    if go_memory is not None and rust_memory is not None:
        lines.extend(
            [
                f"Peak RSS while running the full benchmark: Go {go_memory / 1024:.1f} MiB, "
                f"Rust {rust_memory / 1024:.1f} MiB.",
                "",
            ]
        )
    lines.extend(
        [
            "## Interpretation",
            "",
            "The score weights TUI rendering (30%) and interactive filtering (25%) most heavily, "
            "then JSON parsing and SQLite (15% each), startup (10%), and HTML parsing (5%).",
            "",
            "This intentionally excludes live job-board network latency because upstream variance "
            "would swamp the language comparison. It is a same-runner CPU/runtime decision signal, "
            "not a universal hardware benchmark.",
        ]
    )
    report = "\n".join(lines) + "\n"
    print(report)
    (ROOT / "result.md").write_text(report, encoding="utf-8")
    (ROOT / "result.json").write_text(
        json.dumps(
            {
                "go": go,
                "rust": rust,
                "rust_weighted_speedup": weighted,
                "threshold": 1.5,
                "decision": decision,
                "go_peak_rss_kib": go_memory,
                "rust_peak_rss_kib": rust_memory,
            },
            indent=2,
        ),
        encoding="utf-8",
    )
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as handle:
            handle.write(report)


if __name__ == "__main__":
    main()
