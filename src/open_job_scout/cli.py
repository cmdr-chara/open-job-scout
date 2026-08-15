from __future__ import annotations

import argparse
import json
import shutil
import sqlite3
import sys
import textwrap
import webbrowser
from collections import Counter
from datetime import datetime
from pathlib import Path

from . import __version__
from .actions import add_note
from .config import DEFAULT_CONFIG, expand_path, initialize_config, load_config
from .database import (
    VALID_STATUSES,
    find_job,
    get_jobs_by_fingerprints,
    list_job_events,
    mark_job,
    mark_stale_jobs,
    refresh_jobs,
    save_jobs,
)
from .diagnostics import run_diagnostics
from .discovery import deduplicate, discover, import_csv
from .exporting import write_export
from .models import job_from_record
from .presentation import format_job_detail, preferred_job_url
from .ranking import filter_job, rank_job
from .reporting import write_markdown
from .review import run_review_session
from .tracker import SORT_ORDERS, VALID_WORK_MODES, query_jobs, tracker_summary
from .verification import verify_jobs


def storage_paths(config: dict) -> tuple[Path, Path]:
    storage = config["storage"]
    return expand_path(storage["database"]), expand_path(storage["report_directory"])


def process_jobs(jobs: list, config: dict, *, verify: bool) -> tuple[list, list[str], int]:
    retained = []
    rejected = []
    unique = deduplicate(jobs)
    for job in unique:
        accepted, reason = filter_job(job, config)
        if accepted:
            retained.append(job)
        else:
            rejected.append(f"{job.title} — {job.company}: {reason}")
    if verify and retained:
        retained = verify_jobs(retained)
    retained = [rank_job(job, config) for job in retained]
    retained.sort(key=lambda job: job.score, reverse=True)
    return retained, rejected, len(unique)


def _tip(message: str) -> None:
    if sys.stdout.isatty():
        print(f"\nTip: {message}")


def _json_job(row: sqlite3.Row) -> dict:
    payload = dict(row)
    for field in ("reasons", "concerns"):
        payload[field] = json.loads(payload[field])
    return payload


def _open_row(row: sqlite3.Row, *, source: bool = False) -> str:
    url = preferred_job_url(row, source=source)
    if not url:
        raise LookupError("This job does not have a usable URL.")
    if not webbrowser.open(url, new=2):
        raise RuntimeError(
            "The browser could not be opened. Copy the URL from `jobscout show ID` instead."
        )
    return url


def cmd_init(args: argparse.Namespace) -> int:
    target = initialize_config(args.output, force=args.force)
    print(f"Created config: {target}")
    print(f"Next: edit {target}")
    print("Then run: jobscout search")
    return 0


def _collect_and_save(jobs: list, args: argparse.Namespace, config: dict | None = None) -> int:
    config = config or load_config(args.config)
    retained, rejected, unique_count = process_jobs(jobs, config, verify=not args.no_verify)
    database, report_dir = storage_paths(config)
    stored = save_jobs(retained, database)
    stale = mark_stale_jobs(database, int(config["storage"].get("stale_after_days", 30)))
    current_rows = get_jobs_by_fingerprints(database, (job.fingerprint for job in retained))
    report = write_markdown(
        current_rows,
        report_dir / f"jobs_{datetime.now():%Y-%m-%d_%H%M%S}.md",
    )
    skipped = len(jobs) - unique_count
    verification = Counter(job.verification_status for job in retained)
    print(f"Received: {len(jobs)}")
    print(f"Unique valid jobs: {unique_count}")
    print(f"Accepted: {len(retained)}")
    print(f"Filtered out: {len(rejected)}")
    if skipped:
        print(f"Duplicates or invalid rows: {skipped}")
    if verification:
        details = ", ".join(f"{status}={count}" for status, count in sorted(verification.items()))
        print(f"Verification: {details}")
    print(f"Stored or refreshed: {stored}")
    if stale:
        print(f"Marked stale: {stale}")
    print(f"Database: {database}")
    print(f"Report: {report}")
    if retained:
        _tip("run `jobscout next` to start with the highest-ranked new job")
    return 0


def cmd_search(args: argparse.Namespace) -> int:
    config = load_config(args.config)
    return _collect_and_save(discover(config), args, config)


def cmd_import(args: argparse.Namespace) -> int:
    return _collect_and_save(import_csv(args.file.expanduser().resolve()), args)


def _filtered_rows(args: argparse.Namespace, database: Path) -> list:
    return query_jobs(
        database,
        status=args.status,
        work_mode=args.work_mode,
        source=args.source,
        min_score=args.min_score,
        query=args.query,
        sort=args.sort,
        limit=args.limit,
    )


def cmd_list(args: argparse.Namespace) -> int:
    config = load_config(args.config)
    database, _ = storage_paths(config)
    rows = _filtered_rows(args, database)
    if not rows:
        print("No jobs matched this view.")
        _tip("relax a filter, run `jobscout list`, or discover jobs with `jobscout search`")
        return 0
    width = shutil.get_terminal_size((120, 24)).columns
    label_width = max(24, width - 41)
    print(f"{'ID':<10}  {'SCORE':>5}  {'STATUS':<9}  {'MODE':<7}  ROLE")
    for row in rows:
        label = textwrap.shorten(
            f"{row['title']} - {row['company']}",
            width=label_width,
            placeholder="...",
        )
        mode = row["work_mode"] or "unknown"
        print(
            f"{row['fingerprint'][:10]}  {row['score']:5.1f}  "
            f"{row['status']:<9}  {mode:<7}  {label}"
        )
    _tip("inspect with `jobscout show ID`, or jump straight to `jobscout next`")
    return 0


def cmd_show(args: argparse.Namespace) -> int:
    config = load_config(args.config)
    database, _ = storage_paths(config)
    row = find_job(database, args.id)
    if args.json:
        print(json.dumps(_json_job(row), ensure_ascii=False, indent=2))
    else:
        print(format_job_detail(row, full=args.full))
    return 0


def cmd_open(args: argparse.Namespace) -> int:
    config = load_config(args.config)
    database, _ = storage_paths(config)
    row = find_job(database, args.id)
    url = _open_row(row, source=args.source)
    print(f"Opened: {url}")
    return 0


def cmd_note(args: argparse.Namespace) -> int:
    config = load_config(args.config)
    database, _ = storage_paths(config)
    created = add_note(database, args.id, args.text)
    if created:
        print(f"Added note to {args.id}.")
    else:
        print("That note is already the latest matching note; nothing changed.")
    return 0


def cmd_next(args: argparse.Namespace) -> int:
    config = load_config(args.config)
    database, _ = storage_paths(config)
    rows = query_jobs(
        database,
        status="new",
        work_mode=args.work_mode,
        source=args.source,
        min_score=args.min_score,
        query=args.query,
        sort=args.sort,
        limit=1,
    )
    if not rows:
        print("No new jobs matched your next-job filters.")
        _tip("run `jobscout search` or inspect other states with `jobscout list`")
        return 0
    row = rows[0]
    print("Next job in your review queue:\n")
    print(format_job_detail(row, full=args.full))
    if args.open:
        print(f"\nOpened: {_open_row(row)}")
    return 0


def cmd_review(args: argparse.Namespace) -> int:
    config = load_config(args.config)
    database, _ = storage_paths(config)
    rows = query_jobs(
        database,
        status="new",
        work_mode=args.work_mode,
        source=args.source,
        min_score=args.min_score,
        query=args.query,
        sort=args.sort,
        limit=args.limit,
    )
    if not rows:
        print("No new jobs matched your review filters.")
        _tip("run `jobscout search` or relax the review filters")
        return 0
    print(f"Starting guided review with {len(rows)} new job(s).")
    run_review_session(rows, database, open_job=_open_row)
    return 0


def cmd_mark(args: argparse.Namespace) -> int:
    config = load_config(args.config)
    database, _ = storage_paths(config)
    mark_job(database, args.id, args.status, args.note)
    print(f"Marked {args.id} as {args.status}.")
    if args.status in {"reviewed", "closed", "rejected"}:
        _tip("run `jobscout next` for the next new job")
    return 0


def cmd_history(args: argparse.Namespace) -> int:
    config = load_config(args.config)
    database, _ = storage_paths(config)
    events = list_job_events(database, args.id, limit=args.limit)
    if args.json:
        print(json.dumps([dict(event) for event in events], ensure_ascii=False, indent=2))
        return 0
    if not events:
        print("No history recorded for this job.")
        return 0
    print(f"History for {args.id} (newest first):")
    for event in events:
        timestamp = str(event["created_at"]).replace("T", " ")[:19]
        change = ""
        if event["old_value"] is not None or event["new_value"] is not None:
            change = f" {event['old_value'] or '-'} -> {event['new_value'] or '-'}"
        note = f" — {event['note']}" if event["note"] else ""
        print(f"{timestamp}  {event['event_type']:<12}{change}{note}")
    return 0


def cmd_recheck(args: argparse.Namespace) -> int:
    config = load_config(args.config)
    database, _ = storage_paths(config)
    if args.ids:
        unique = {}
        for identifier in args.ids:
            row = find_job(database, identifier)
            unique[row["fingerprint"]] = row
        rows = list(unique.values())
    else:
        rows = _filtered_rows(args, database)
    if not rows:
        print("No jobs matched the recheck selection.")
        return 0

    jobs = [job_from_record(row) for row in rows]
    checked = verify_jobs(jobs, workers=args.workers)
    checked = [rank_job(job, config) for job in checked]
    refreshed = refresh_jobs(checked, database)
    verification = Counter(job.verification_status for job in checked)
    details = ", ".join(f"{status}={count}" for status, count in sorted(verification.items()))
    print(f"Rechecked: {refreshed}")
    print(f"Verification: {details}")
    print("Discovery timestamps were not changed.")
    return 0


def cmd_report(args: argparse.Namespace) -> int:
    config = load_config(args.config)
    database, report_dir = storage_paths(config)
    output = (
        args.output.expanduser().resolve()
        if args.output
        else report_dir / f"jobs_{datetime.now():%Y-%m-%d_%H%M%S}.md"
    )
    rows = _filtered_rows(args, database)
    print(f"Report: {write_markdown(rows, output)}")
    return 0


def _format_counts(values: dict[str, int]) -> str:
    return ", ".join(f"{label}={count}" for label, count in values.items()) or "none"


def cmd_stats(args: argparse.Namespace) -> int:
    config = load_config(args.config)
    database, _ = storage_paths(config)
    summary = tracker_summary(database)
    total = int(summary["total"])
    print(f"Tracked jobs: {total}")
    if not total:
        _tip("run `jobscout search` or `jobscout import-csv FILE`")
        return 0
    print(f"Average score: {summary['average_score']:.1f}")
    print(f"Salary published: {summary['salary_published']}/{total}")
    print(f"Status: {_format_counts(summary['statuses'])}")
    print(f"Work mode: {_format_counts(summary['work_modes'])}")
    print(f"Sources: {_format_counts(summary['sources'])}")
    top_new = summary["top_new"]
    if top_new:
        print("Top new:")
        for row in top_new:
            print(
                f"  {row['fingerprint'][:10]}  {row['score']:5.1f}  "
                f"{row['title']} - {row['company']}"
            )
        _tip("run `jobscout next` to inspect the first one")
    return 0


def cmd_export(args: argparse.Namespace) -> int:
    config = load_config(args.config)
    database, report_dir = storage_paths(config)
    rows = _filtered_rows(args, database)
    output = (
        args.output.expanduser().resolve()
        if args.output
        else report_dir / f"jobs_export_{datetime.now():%Y-%m-%d_%H%M%S}.{args.format}"
    )
    path = write_export(rows, output, args.format)
    print(f"Exported {len(rows)} jobs: {path}")
    return 0


def cmd_doctor(args: argparse.Namespace) -> int:
    checks = run_diagnostics(args.config)
    if args.json:
        print(json.dumps([check.as_dict() for check in checks], ensure_ascii=False, indent=2))
    else:
        for check in checks:
            print(f"{check.level.upper():<5} {check.check}: {check.message}")
    return 1 if any(check.level == "error" for check in checks) else 0


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed < 1:
        raise argparse.ArgumentTypeError("must be at least 1")
    return parsed


def score_value(value: str) -> float:
    try:
        parsed = float(value)
    except ValueError as exc:
        raise argparse.ArgumentTypeError("must be a number between 0 and 100") from exc
    if not 0 <= parsed <= 100:
        raise argparse.ArgumentTypeError("must be between 0 and 100")
    return parsed


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="jobscout",
        description="Find, verify, rank, and track jobs locally.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""Common workflow:
  jobscout search
  jobscout next
  jobscout open ID
  jobscout mark ID applied --note "Applied on the employer site"
  jobscout next

Or review several jobs in one guided session:
  jobscout review

Run `jobscout COMMAND --help` for command-specific options.""",
    )
    parser.add_argument("--version", action="version", version=f"%(prog)s {__version__}")
    subparsers = parser.add_subparsers(dest="command", required=True)

    def add_config_argument(command_parser: argparse.ArgumentParser) -> None:
        command_parser.add_argument(
            "--config",
            type=Path,
            default=DEFAULT_CONFIG,
            metavar="PATH",
            help=f"configuration file (default: {DEFAULT_CONFIG})",
        )

    def add_queue_arguments(
        command_parser: argparse.ArgumentParser,
        *,
        default_limit: int | None,
    ) -> None:
        command_parser.add_argument(
            "--status", choices=sorted(VALID_STATUSES), help="only include this application state"
        )
        command_parser.add_argument(
            "--work-mode",
            choices=sorted(VALID_WORK_MODES),
            help="only include this work arrangement",
        )
        command_parser.add_argument(
            "--source", help="only include this source, for example linkedin or google"
        )
        command_parser.add_argument(
            "--min-score", type=score_value, metavar="N", help="minimum ranking score (0-100)"
        )
        command_parser.add_argument(
            "--query",
            help="search title, company, location, description, and notes",
        )
        command_parser.add_argument(
            "--sort",
            choices=sorted(SORT_ORDERS),
            default="score",
            help="sort by score or most recently seen (default: score)",
        )
        limit_help = (
            f"maximum rows (default: {default_limit})"
            if default_limit is not None
            else "maximum rows (default: all)"
        )
        command_parser.add_argument(
            "--limit",
            type=positive_int,
            default=default_limit,
            metavar="N",
            help=limit_help,
        )

    def add_review_filters(command_parser: argparse.ArgumentParser, *, limit: int | None) -> None:
        command_parser.add_argument(
            "--work-mode",
            choices=sorted(VALID_WORK_MODES),
            help="only consider this work arrangement",
        )
        command_parser.add_argument(
            "--source", help="only consider this source, for example linkedin or google"
        )
        command_parser.add_argument(
            "--min-score", type=score_value, metavar="N", help="minimum ranking score (0-100)"
        )
        command_parser.add_argument(
            "--query", help="search title, company, location, description, and notes"
        )
        command_parser.add_argument(
            "--sort",
            choices=sorted(SORT_ORDERS),
            default="score",
            help="order by score or most recently seen (default: score)",
        )
        if limit is not None:
            command_parser.add_argument(
                "--limit",
                type=positive_int,
                default=limit,
                metavar="N",
                help=f"maximum jobs in the session (default: {limit})",
            )

    init_parser = subparsers.add_parser(
        "init",
        help="Create a local config",
        description="Create a documented configuration from the bundled template.",
    )
    init_parser.add_argument(
        "--output", type=Path, metavar="PATH", help="write the config to this path"
    )
    init_parser.add_argument("--force", action="store_true", help="replace an existing config")
    init_parser.set_defaults(handler=cmd_init)

    search_parser = subparsers.add_parser(
        "search",
        help="Discover and store jobs",
        description="Search configured sources, filter results, verify links, and save a snapshot.",
    )
    add_config_argument(search_parser)
    search_parser.add_argument(
        "--no-verify",
        action="store_true",
        help="skip requests to result and public ATS URLs",
    )
    search_parser.set_defaults(handler=cmd_search)

    import_parser = subparsers.add_parser(
        "import-csv",
        help="Import a JobSpy-compatible CSV",
        description="Filter, optionally verify, and store jobs from a local CSV.",
    )
    add_config_argument(import_parser)
    import_parser.add_argument("file", type=Path, metavar="CSV", help="CSV file to import")
    import_parser.add_argument(
        "--no-verify",
        action="store_true",
        help="skip requests to URLs contained in the CSV",
    )
    import_parser.set_defaults(handler=cmd_import)

    list_parser = subparsers.add_parser(
        "list",
        aliases=["ls"],
        help="List tracked jobs",
        description="Show and filter the local review queue.",
    )
    add_config_argument(list_parser)
    add_queue_arguments(list_parser, default_limit=20)
    list_parser.set_defaults(handler=cmd_list)

    show_parser = subparsers.add_parser(
        "show",
        aliases=["view"],
        help="Show one tracked job",
        description="Show a readable job summary, or the complete record as JSON.",
    )
    add_config_argument(show_parser)
    show_parser.add_argument("id", metavar="ID", help="full or unambiguous short job ID")
    show_parser.add_argument("--json", action="store_true", help="print the complete JSON record")
    show_parser.add_argument(
        "--full", action="store_true", help="show the full description instead of a preview"
    )
    show_parser.set_defaults(handler=cmd_show)

    open_parser = subparsers.add_parser(
        "open",
        help="Open a tracked job in the browser",
        description="Open the employer/canonical URL, falling back to the source listing.",
    )
    add_config_argument(open_parser)
    open_parser.add_argument("id", metavar="ID", help="full or unambiguous short job ID")
    open_parser.add_argument(
        "--source",
        action="store_true",
        help="open the original source listing instead of the canonical URL",
    )
    open_parser.set_defaults(handler=cmd_open)

    note_parser = subparsers.add_parser(
        "note",
        help="Add a note without changing status",
        description="Append a note and history event without changing application state.",
    )
    add_config_argument(note_parser)
    note_parser.add_argument("id", metavar="ID", help="full or unambiguous short job ID")
    note_parser.add_argument("text", metavar="TEXT", help="note text")
    note_parser.set_defaults(handler=cmd_note)

    next_parser = subparsers.add_parser(
        "next",
        help="Show the next new job to review",
        description="Show the highest-priority new job, optionally filtered or opened.",
    )
    add_config_argument(next_parser)
    add_review_filters(next_parser, limit=None)
    next_parser.add_argument("--open", action="store_true", help="also open the job in a browser")
    next_parser.add_argument(
        "--full", action="store_true", help="show the full description instead of a preview"
    )
    next_parser.set_defaults(handler=cmd_next)

    review_parser = subparsers.add_parser(
        "review",
        help="Review several new jobs interactively",
        description="Work through a filtered batch with simple open/note/status actions.",
    )
    add_config_argument(review_parser)
    add_review_filters(review_parser, limit=20)
    review_parser.set_defaults(handler=cmd_review)

    mark_parser = subparsers.add_parser(
        "mark",
        help="Update application status",
        description="Set the application state and optionally append a note.",
    )
    add_config_argument(mark_parser)
    mark_parser.add_argument("id", metavar="ID", help="full or unambiguous short job ID")
    mark_parser.add_argument("status", choices=sorted(VALID_STATUSES), help="new application state")
    mark_parser.add_argument("--note", help="append a note to this job's history")
    mark_parser.set_defaults(handler=cmd_mark)

    history_parser = subparsers.add_parser(
        "history",
        aliases=["log"],
        help="Show one job's event history",
        description="Show status, verification, note, and migration events for a tracked job.",
    )
    add_config_argument(history_parser)
    history_parser.add_argument("id", metavar="ID", help="full or unambiguous short job ID")
    history_parser.add_argument(
        "--limit", type=positive_int, default=50, metavar="N", help="maximum events (default: 50)"
    )
    history_parser.add_argument("--json", action="store_true", help="print structured JSON")
    history_parser.set_defaults(handler=cmd_history)

    recheck_parser = subparsers.add_parser(
        "recheck",
        help="Re-verify tracked jobs",
        description=(
            "Re-verify and re-rank existing tracker rows without updating discovery timestamps."
        ),
    )
    add_config_argument(recheck_parser)
    add_queue_arguments(recheck_parser, default_limit=50)
    recheck_parser.add_argument(
        "ids",
        nargs="*",
        metavar="ID",
        help="specific job IDs; when supplied, queue filters are ignored",
    )
    recheck_parser.add_argument(
        "--workers",
        type=positive_int,
        default=6,
        metavar="N",
        help="parallel verification workers (default: 6)",
    )
    recheck_parser.set_defaults(handler=cmd_recheck)

    report_parser = subparsers.add_parser(
        "report",
        help="Write a Markdown report",
        description="Export a Markdown snapshot from the local tracker.",
    )
    add_config_argument(report_parser)
    add_queue_arguments(report_parser, default_limit=100)
    report_parser.add_argument(
        "--output", type=Path, metavar="PATH", help="write to this Markdown file"
    )
    report_parser.set_defaults(handler=cmd_report)

    stats_parser = subparsers.add_parser(
        "stats",
        help="Summarize the local tracker",
        description="Show pipeline counts, work modes, sources, and top new jobs.",
    )
    add_config_argument(stats_parser)
    stats_parser.set_defaults(handler=cmd_stats)

    export_parser = subparsers.add_parser(
        "export",
        help="Export tracked jobs as CSV or JSON",
        description="Export a filtered tracker view for spreadsheets or local analysis.",
    )
    add_config_argument(export_parser)
    add_queue_arguments(export_parser, default_limit=None)
    export_parser.add_argument(
        "--format",
        choices=("csv", "json"),
        default="csv",
        help="export format (default: csv)",
    )
    export_parser.add_argument(
        "--output", type=Path, metavar="PATH", help="write to this file"
    )
    export_parser.set_defaults(handler=cmd_export)

    doctor_parser = subparsers.add_parser(
        "doctor",
        help="Check the local installation",
        description="Validate config, storage health, permissions, and discovery dependencies.",
    )
    add_config_argument(doctor_parser)
    doctor_parser.add_argument("--json", action="store_true", help="print structured JSON")
    doctor_parser.set_defaults(handler=cmd_doctor)
    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    try:
        return int(args.handler(args))
    except KeyboardInterrupt:
        print("Interrupted.", file=sys.stderr)
        return 130
    except (OSError, LookupError, RuntimeError, sqlite3.Error, ValueError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
