from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime
from pathlib import Path

from .config import DEFAULT_CONFIG, expand_path, initialize_config, load_config
from .database import VALID_STATUSES, find_job, list_jobs, mark_job, save_jobs
from .discovery import deduplicate, discover, import_csv
from .ranking import filter_job, rank_job
from .reporting import write_markdown
from .verification import verify_jobs


def storage_paths(config: dict) -> tuple[Path, Path]:
    storage = config["storage"]
    return expand_path(storage["database"]), expand_path(storage["report_directory"])


def process_jobs(jobs: list, config: dict, *, verify: bool) -> tuple[list, list[str]]:
    retained = []
    rejected = []
    for job in deduplicate(jobs):
        accepted, reason = filter_job(job, config)
        if accepted:
            retained.append(job)
        else:
            rejected.append(f"{job.title} — {job.company}: {reason}")
    if verify and retained:
        retained = verify_jobs(retained)
    retained = [rank_job(job, config) for job in retained]
    retained.sort(key=lambda job: job.score, reverse=True)
    return retained, rejected


def cmd_init(args: argparse.Namespace) -> int:
    target = initialize_config(args.output, force=args.force)
    print(f"Created config: {target}")
    return 0


def _collect_and_save(jobs: list, args: argparse.Namespace) -> int:
    config = load_config(args.config)
    retained, rejected = process_jobs(jobs, config, verify=not args.no_verify)
    database, report_dir = storage_paths(config)
    saved = save_jobs(retained, database)
    report = write_markdown(
        list_jobs(database, limit=max(saved, 1)),
        report_dir / f"jobs_{datetime.now():%Y-%m-%d_%H%M%S}.md",
    )
    print(f"Accepted: {saved}")
    print(f"Rejected: {len(rejected)}")
    print(f"Database: {database}")
    print(f"Report: {report}")
    return 0


def cmd_search(args: argparse.Namespace) -> int:
    return _collect_and_save(discover(load_config(args.config)), args)


def cmd_import(args: argparse.Namespace) -> int:
    return _collect_and_save(import_csv(args.file), args)


def cmd_list(args: argparse.Namespace) -> int:
    config = load_config(args.config)
    database, _ = storage_paths(config)
    rows = list_jobs(database, status=args.status, limit=args.limit)
    if not rows:
        print("No jobs found.")
        return 0
    for row in rows:
        print(
            f"{row['fingerprint'][:10]}  {row['score']:5.1f}  "
            f"{row['status']:<9}  {row['title']} - {row['company']}"
        )
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
    output = args.output or report_dir / f"jobs_{datetime.now():%Y-%m-%d_%H%M%S}.md"
    rows = list_jobs(database, status=args.status, limit=args.limit)
    print(f"Report: {write_markdown(rows, output)}")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="jobscout", description="Find, verify, rank, and track jobs locally."
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    def add_config_argument(command_parser: argparse.ArgumentParser) -> None:
        command_parser.add_argument("--config", type=Path, default=DEFAULT_CONFIG)

    init_parser = subparsers.add_parser("init", help="Create a local config")
    init_parser.add_argument("--output", type=Path)
    init_parser.add_argument("--force", action="store_true")
    init_parser.set_defaults(handler=cmd_init)

    search_parser = subparsers.add_parser("search", help="Discover and store jobs")
    add_config_argument(search_parser)
    search_parser.add_argument("--no-verify", action="store_true")
    search_parser.set_defaults(handler=cmd_search)

    import_parser = subparsers.add_parser("import-csv", help="Import a JobSpy-compatible CSV")
    add_config_argument(import_parser)
    import_parser.add_argument("file", type=Path)
    import_parser.add_argument("--no-verify", action="store_true")
    import_parser.set_defaults(handler=cmd_import)

    list_parser = subparsers.add_parser("list", help="List tracked jobs")
    add_config_argument(list_parser)
    list_parser.add_argument("--status", choices=sorted(VALID_STATUSES))
    list_parser.add_argument("--limit", type=int, default=20)
    list_parser.set_defaults(handler=cmd_list)

    show_parser = subparsers.add_parser("show", help="Show one tracked job")
    add_config_argument(show_parser)
    show_parser.add_argument("id")
    show_parser.set_defaults(handler=cmd_show)

    mark_parser = subparsers.add_parser("mark", help="Update application status")
    add_config_argument(mark_parser)
    mark_parser.add_argument("id")
    mark_parser.add_argument("status", choices=sorted(VALID_STATUSES))
    mark_parser.add_argument("--note")
    mark_parser.set_defaults(handler=cmd_mark)

    report_parser = subparsers.add_parser("report", help="Write a Markdown report")
    add_config_argument(report_parser)
    report_parser.add_argument("--status", choices=sorted(VALID_STATUSES))
    report_parser.add_argument("--limit", type=int, default=100)
    report_parser.add_argument("--output", type=Path)
    report_parser.set_defaults(handler=cmd_report)
    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    try:
        return int(args.handler(args))
    except (FileNotFoundError, FileExistsError, LookupError, RuntimeError, ValueError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
