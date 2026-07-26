from __future__ import annotations

import ipaddress
import json
import re
import socket
import urllib.error
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from html.parser import HTMLParser

from .models import Job

USER_AGENT = "OpenJobScout/0.1 (+https://github.com/cmdr-chara/open-job-scout)"
MAX_HTML_BYTES = 1_000_000
MAX_JSON_BYTES = 5_000_000

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
        self._heading_depth = 0
        self.parts: list[str] = []
        self.heading_parts: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag in {"script", "style", "noscript", "svg"}:
            self._ignored_depth += 1
        elif tag in {"title", "h1", "h2"}:
            self._heading_depth += 1

    def handle_endtag(self, tag: str) -> None:
        if tag in {"script", "style", "noscript", "svg"} and self._ignored_depth:
            self._ignored_depth -= 1
        elif tag in {"title", "h1", "h2"} and self._heading_depth:
            self._heading_depth -= 1

    def handle_data(self, data: str) -> None:
        if not self._ignored_depth:
            self.parts.append(data)
            if self._heading_depth:
                self.heading_parts.append(data)


def visible_text(html: str) -> str:
    parser = VisibleTextParser()
    parser.feed(html)
    return re.sub(r"\s+", " ", " ".join(parser.parts)).strip().lower()


def page_indicates_closed(html: str) -> bool:
    parser = VisibleTextParser()
    parser.feed(html)
    text = re.sub(r"\s+", " ", " ".join(parser.parts)).strip().lower()
    headings = re.sub(r"\s+", " ", " ".join(parser.heading_parts)).strip().lower()
    # An incidental FAQ or description can mention an expired job. Only close a
    # non-ATS listing when the message is page-level, rather than guessing from
    # arbitrary body copy.
    if headings:
        return any(marker in headings for marker in CLOSED_MARKERS)
    return len(text) <= 280 and any(marker in text for marker in CLOSED_MARKERS)


def is_safe_public_url(url: str) -> bool:
    """Accept only HTTP(S) targets that resolve outside private/reserved networks."""
    try:
        parts = urllib.parse.urlsplit(url)
        if parts.scheme.lower() not in {"http", "https"} or not parts.hostname:
            return False
        if parts.username or parts.password:
            return False
        host = parts.hostname.rstrip(".")
        if host.lower() == "localhost":
            return False
        try:
            return ipaddress.ip_address(host).is_global
        except ValueError:
            addresses = socket.getaddrinfo(host, parts.port or 443, type=socket.SOCK_STREAM)
            return bool(addresses) and all(
                ipaddress.ip_address(address[4][0]).is_global for address in addresses
            )
    except (OSError, ValueError):
        return False


class SafeRedirectHandler(urllib.request.HTTPRedirectHandler):
    """Do not follow a redirect to localhost or a private/reserved IP range."""

    def redirect_request(
        self,
        request: urllib.request.Request,
        file_pointer: object,
        code: int,
        message: str,
        headers: object,
        new_url: str,
    ) -> urllib.request.Request | None:
        destination = urllib.parse.urljoin(request.full_url, new_url)
        if not is_safe_public_url(destination):
            raise urllib.error.URLError("redirect target is not a public HTTP(S) URL")
        return super().redirect_request(request, file_pointer, code, message, headers, destination)


def _open_public(request: urllib.request.Request, timeout: int):
    if not is_safe_public_url(request.full_url):
        raise urllib.error.URLError("target is not a public HTTP(S) URL")
    return urllib.request.build_opener(SafeRedirectHandler()).open(request, timeout=timeout)


def _read_limited(response: object, maximum: int) -> bytes:
    headers = getattr(response, "headers", None)
    length = headers.get("Content-Length") if headers else None
    if length and int(length) > maximum:
        raise ValueError("response exceeds the allowed size")
    body = response.read(maximum + 1)
    if len(body) > maximum:
        raise ValueError("response exceeds the allowed size")
    return body


def resolve_url(url: str, timeout: int = 8) -> tuple[str, str, str]:
    try:
        request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
        with _open_public(request, timeout=timeout) as response:
            content_type = response.headers.get_content_type()
            body = ""
            if content_type in {"text/html", "application/xhtml+xml"}:
                body = _read_limited(response, MAX_HTML_BYTES).decode(
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
    host = (parts.hostname or "").lower()
    segments = [urllib.parse.unquote(value) for value in parts.path.split("/") if value]
    if host == "greenhouse.io" or host.endswith(".greenhouse.io"):
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
    with _open_public(request, timeout=timeout) as response:
        body = _read_limited(response, MAX_JSON_BYTES)
        return json.loads(body.decode(response.headers.get_content_charset() or "utf-8"))


def _ashby_job_matches(item: object, board: str, posting: str) -> bool:
    """Match the board and posting segments exactly; substrings are not identities."""
    if not isinstance(item, dict):
        return False
    for field in ("jobUrl", "applyUrl"):
        value = item.get(field)
        if not isinstance(value, str):
            continue
        parts = urllib.parse.urlsplit(value)
        segments = [urllib.parse.unquote(segment) for segment in parts.path.split("/") if segment]
        if (
            (parts.hostname or "").lower() == "jobs.ashbyhq.com"
            and len(segments) in {2, 3}
            and segments[0] == board
            and segments[1] == posting
            and (len(segments) == 2 or segments[2] == "application")
        ):
            return True
    return False


def verify_job(job: Job) -> Job:
    targets = list(dict.fromkeys(value for value in (job.canonical_url, job.source_url) if value))
    status, resolved = "unreachable", job.source_url
    for target in targets:
        status, resolved, _ = resolve_url(target)
        if status != "unreachable":
            break
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
                    if _ashby_job_matches(item, board, posting)
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
