from __future__ import annotations

import argparse
import json
import shutil
import sqlite3
import sys
import textwrap
from collections import Counter
from datetime import datetime
from pathlib import Path

from . import __version__
from .config import DEFAULT_CONFIG, expand_path, initialize_config, load_config
from .database import (
    VALID_STATUSES,
    find_job,
    get_jobs_by_fingerprints,
    list_jobs,
    mark_job,
    save_jobs,
)
from .discovery import deduplicate, discover, import_csv
from .ranking import filter_job, rank_job
from .reporting import write_markdown
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
    print(f"Database: {database}")
    print(f"Report: {report}")
    return 0


def cmd_search(args: argparse.Namespace) -> int:
    config = load_config(args.config)
    return _collect_and_save(discover(config), args, config)


def cmd_import(args: argparse.Namespace) -> int:
    return _collect_and_save(import_csv(args.file.expanduser().resolve()), args)


def cmd_list(args: argparse.Namespace) -> int:
    config = load_config(args.config)
    database, _ = storage_paths(config)
    rows = list_jobs(database, status=args.status, limit=args.limit)
    if not rows:
        print("No jobs found.")
        return 0
    width = shutil.get_terminal_size((120, 24)).columns
    label_width = max(24, width - 32)
    print(f"{'ID':<10}  {'SCORE':>5}  {'STATUS':<9}  ROLE")
    for row in rows:
        label = textwrap.shorten(
            f"{row['title']} - {row['company']}",
            width=label_width,
            placeholder="...",
        )
        print(f"{row['fingerprint'][:10]}  {row['score']:5.1f}  {row['status']:<9}  {label}")
    return 0


def cmd_show(args: argparse.Namespace) -> int:
    config = load_config(args.config)
    database, _ = storage_paths(config)
    row = find_job(database, args.id)
    payload = dict(row)
    for field in ("reasons", "concerns"):
        payload[field] = json.loads(payload[field])
    print(json.dumps(payload, ensure_ascii=False, indent=2))
    return 0


def cmd_mark(args: argparse.Namespace) -> int:
    config = load_config(args.config)
    database, _ = storage_paths(config)
    mark_job(database, args.id, args.status, args.note)
    print(f"Marked {args.id} as {args.status}.")
    return 0


def cmd_report(args: argparse.Namespace) -> int:
    config = load_config(args.config)
    database, report_dir = storage_paths(config)
    output = (
        args.output.expanduser().resolve()
        if args.output
        else report_dir / f"jobs_{datetime.now():%Y-%m-%d_%H%M%S}.md"
    )
    rows = list_jobs(database, status=args.status, limit=args.limit)
    print(f"Report: {write_markdown(rows, output)}")
    return 0


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed < 1:
        raise argparse.ArgumentTypeError("must be at least 1")
    return parsed


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="jobscout",
        description="Find, verify, rank, and track jobs locally.",
        epilog="Run `jobscout COMMAND --help` for command-specific options.",
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
        "list", help="List tracked jobs", description="Show the local review queue."
    )
    add_config_argument(list_parser)
    list_parser.add_argument(
        "--status", choices=sorted(VALID_STATUSES), help="only show this application state"
    )
    list_parser.add_argument(
        "--limit", type=positive_int, default=20, metavar="N", help="maximum rows (default: 20)"
    )
    list_parser.set_defaults(handler=cmd_list)

    show_parser = subparsers.add_parser(
        "show",
        help="Show one tracked job",
        description="Print one complete local record as JSON.",
    )
    add_config_argument(show_parser)
    show_parser.add_argument("id", metavar="ID", help="full or unambiguous short job ID")
    show_parser.set_defaults(handler=cmd_show)

    mark_parser = subparsers.add_parser(
        "mark",
        help="Update application status",
        description="Set the application state and optionally replace its note.",
    )
    add_config_argument(mark_parser)
    mark_parser.add_argument("id", metavar="ID", help="full or unambiguous short job ID")
    mark_parser.add_argument("status", choices=sorted(VALID_STATUSES), help="new application state")
    mark_parser.add_argument("--note", help="replace the note stored with this job")
    mark_parser.set_defaults(handler=cmd_mark)

    report_parser = subparsers.add_parser(
        "report",
        help="Write a Markdown report",
        description="Export a Markdown snapshot from the local tracker.",
    )
    add_config_argument(report_parser)
    report_parser.add_argument(
        "--status", choices=sorted(VALID_STATUSES), help="only include this application state"
    )
    report_parser.add_argument(
        "--limit",
        type=positive_int,
        default=100,
        metavar="N",
        help="maximum rows (default: 100)",
    )
    report_parser.add_argument(
        "--output", type=Path, metavar="PATH", help="write to this Markdown file"
    )
    report_parser.set_defaults(handler=cmd_report)
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
