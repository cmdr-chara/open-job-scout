from __future__ import annotations

import json
import re
import urllib.error
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from html.parser import HTMLParser

from .models import Job

USER_AGENT = "OpenJobScout/0.1 (+https://github.com/cmdr-chara/open-job-scout)"

CLOSED_MARKERS = (
    "job not found",
    "job is no longer available",
    "job no longer available",
    "position is no longer available",
    "position no longer available",
    "this job has expired",
    "this position has been filled",
    "no longer accepting applications",
    "vacancy has been filled",
    "offerta non disponibile",
    "posizione non è più disponibile",
    "annuncio non è più disponibile",
    "non accetta più candidature",
)


class VisibleTextParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self._ignored_depth = 0
        self.parts: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag in {"script", "style", "noscript", "svg"}:
            self._ignored_depth += 1

    def handle_endtag(self, tag: str) -> None:
        if tag in {"script", "style", "noscript", "svg"} and self._ignored_depth:
            self._ignored_depth -= 1

    def handle_data(self, data: str) -> None:
        if not self._ignored_depth:
            self.parts.append(data)


def visible_text(html: str) -> str:
    parser = VisibleTextParser()
    parser.feed(html)
    return re.sub(r"\s+", " ", " ".join(parser.parts)).strip().lower()


def page_indicates_closed(html: str) -> bool:
    text = visible_text(html)
    return any(marker in text for marker in CLOSED_MARKERS)


def resolve_url(url: str, timeout: int = 8) -> tuple[str, str, str]:
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            content_type = response.headers.get_content_type()
            body = ""
            if content_type in {"text/html", "application/xhtml+xml"}:
                body = response.read(1_000_000).decode(
                    response.headers.get_content_charset() or "utf-8", errors="replace"
                )
            status = "closed" if page_indicates_closed(body) else "reachable"
            return status, response.geturl(), body
    except urllib.error.HTTPError as exc:
        if exc.code in {404, 410}:
            return "closed", url, ""
        return "unreachable", url, ""
    except (urllib.error.URLError, TimeoutError, ValueError):
        return "unreachable", url, ""


def detect_ats(url: str) -> tuple[str, tuple[str, ...]] | None:
    parts = urllib.parse.urlsplit(url)
    host = parts.netloc.lower()
    segments = [urllib.parse.unquote(value) for value in parts.path.split("/") if value]
    if "greenhouse.io" in host:
        query_id = urllib.parse.parse_qs(parts.query).get("gh_jid", [None])[0]
        if query_id and segments:
            return "greenhouse", (segments[0], str(query_id))
        if len(segments) >= 3 and segments[-2] == "jobs":
            return "greenhouse", (segments[-3], segments[-1])
    if host in {"jobs.lever.co", "jobs.eu.lever.co"} and len(segments) >= 2:
        region = "eu" if host.startswith("jobs.eu") else "global"
        return "lever", (region, segments[0], segments[1])
    if host == "jobs.ashbyhq.com" and len(segments) >= 2:
        return "ashby", (segments[0], segments[1])
    return None


def request_json(url: str, timeout: int = 10) -> dict:
    request = urllib.request.Request(
        url, headers={"Accept": "application/json", "User-Agent": USER_AGENT}
    )
    with urllib.request.urlopen(request, timeout=timeout) as response:
        return json.load(response)


def verify_job(job: Job) -> Job:
    status, resolved, _ = resolve_url(job.canonical_url or job.source_url)
    if status != "reachable":
        job.verification_status = status
        return job
    job.canonical_url = resolved
    job.verification_status = "reachable"
    detected = detect_ats(resolved)
    if not detected:
        return job
    provider, values = detected
    try:
        if provider == "greenhouse":
            board, job_id = values
            payload = request_json(
                f"https://boards-api.greenhouse.io/v1/boards/"
                f"{urllib.parse.quote(board)}/jobs/{urllib.parse.quote(job_id)}"
                "?pay_transparency=true"
            )
            job.canonical_url = str(payload.get("absolute_url") or resolved)
        elif provider == "lever":
            region, site, posting = values
            host = "api.eu.lever.co" if region == "eu" else "api.lever.co"
            payload = request_json(
                f"https://{host}/v0/postings/{urllib.parse.quote(site)}/"
                f"{urllib.parse.quote(posting)}"
            )
            job.canonical_url = str(payload.get("applyUrl") or payload.get("hostedUrl") or resolved)
        else:
            board, posting = values
            payload = request_json(
                f"https://api.ashbyhq.com/posting-api/job-board/"
                f"{urllib.parse.quote(board)}?includeCompensation=true"
            )
            matched = next(
                (
                    item
                    for item in payload.get("jobs", [])
                    if posting in str(item.get("jobUrl", ""))
                ),
                None,
            )
            if matched is None:
                raise LookupError("posting is absent from the Ashby board")
            job.canonical_url = str(matched.get("applyUrl") or matched.get("jobUrl") or resolved)
        job.verification_status = "verified"
        job.verification_source = provider
    except urllib.error.HTTPError as exc:
        if exc.code in {404, 410}:
            job.verification_status = "closed"
            job.verification_source = provider
    except (urllib.error.URLError, TimeoutError, ValueError, KeyError):
        pass
    except LookupError:
        job.verification_status = "closed"
        job.verification_source = provider
    return job


def verify_jobs(jobs: list[Job], workers: int = 6) -> list[Job]:
    with ThreadPoolExecutor(max_workers=workers) as pool:
        return list(pool.map(verify_job, jobs))
