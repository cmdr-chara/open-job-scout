from __future__ import annotations

from collections.abc import Callable, Iterable, Mapping
from pathlib import Path

from .actions import add_note
from .database import mark_job
from .presentation import format_job_detail

_HELP = """Actions:
  o  open the job in your browser
  n  add a note without changing status
  r  mark reviewed and move on
  a  mark applied and move on
  x  mark rejected and move on
  c  mark closed and move on
  s  skip this job for this session
  q  quit the review session
  ?  show this help
"""


def run_review_session(
    rows: Iterable[Mapping],
    database: Path,
    *,
    open_job: Callable[[Mapping], str],
    input_func: Callable[[str], str] = input,
    output: Callable[[str], None] = print,
) -> int:
    queue = list(rows)
    decisions = 0
    for index, row in enumerate(queue, 1):
        identifier = str(row["fingerprint"])
        while True:
            output(f"\nReview {index}/{len(queue)}")
            output(format_job_detail(row))
            try:
                action = input_func(
                    "\nAction [o/n/r/a/x/c/s/q/?]: "
                ).strip().lower()
            except EOFError:
                output("\nInput ended; leaving the review session.")
                return decisions

            if action in {"?", "help"}:
                output(_HELP.rstrip())
                continue
            if action in {"q", "quit"}:
                output("Review session ended.")
                return decisions
            if action in {"s", "skip", ""}:
                break
            if action in {"o", "open"}:
                output(f"Opened: {open_job(row)}")
                continue
            if action in {"n", "note"}:
                note = input_func("Note: ").strip()
                if not note:
                    output("No note added.")
                    continue
                if add_note(database, identifier, note):
                    output("Note added.")
                else:
                    output("That note is already recorded.")
                continue

            states = {
                "r": "reviewed",
                "reviewed": "reviewed",
                "a": "applied",
                "applied": "applied",
                "x": "rejected",
                "rejected": "rejected",
                "c": "closed",
                "closed": "closed",
            }
            status = states.get(action)
            if status is None:
                output("Unknown action. Enter ? for help.")
                continue

            note = None
            if status == "applied":
                entered = input_func("Application note (optional): ").strip()
                note = entered or None
            mark_job(database, identifier, status, note)
            output(f"Marked {identifier[:10]} as {status}.")
            decisions += 1
            break

    output(f"Review queue finished. Decisions recorded: {decisions}.")
    return decisions
