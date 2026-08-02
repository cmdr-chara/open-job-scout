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

from .models import Job, normalize_text

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
    if host.endswith(".recruitee.com") and len(segments) >= 2 and segments[-2] == "o":
        company = host.removesuffix(".recruitee.com")
        if company:
            return "recruitee", (company, segments[-1])
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


def _set_salary(
    job: Job,
    minimum: object,
    maximum: object,
    currency: object,
    source: str,
    *,
    cents: bool = False,
) -> None:
    try:
        low = float(minimum) if minimum is not None else None
        high = float(maximum) if maximum is not None else None
    except (TypeError, ValueError):
        return
    divisor = 100 if cents else 1
    job.salary_min = low / divisor if low is not None and low >= 0 else None
    job.salary_max = high / divisor if high is not None and high >= 0 else None
    job.currency = str(currency) if currency else job.currency
    job.salary_source = source


def _enrich_greenhouse(job: Job, payload: dict) -> None:
    ranges = payload.get("pay_input_ranges")
    if isinstance(ranges, list) and ranges and isinstance(ranges[0], dict):
        value = ranges[0]
        context = normalize_text(f"{value.get('title', '')} {value.get('blurb', '')}")
        if any(signal in context for signal in ("annual", "per year", "yearly", "/year")):
            _set_salary(
                job,
                value.get("min_cents"),
                value.get("max_cents"),
                value.get("currency_type"),
                "greenhouse",
                cents=True,
            )


def _enrich_lever(job: Job, payload: dict) -> None:
    salary = payload.get("salaryRange")
    if isinstance(salary, dict) and salary.get("interval") == "per-year-salary":
        _set_salary(
            job,
            salary.get("min"),
            salary.get("max"),
            salary.get("currency"),
            "lever",
        )
    workplace = normalize_text(payload.get("workplaceType"))
    if workplace in {"remote", "hybrid", "onsite", "on site"}:
        job.work_mode = "onsite" if workplace in {"onsite", "on site"} else workplace
        job.remote = job.work_mode == "remote"


def _enrich_ashby(job: Job, item: dict) -> None:
    workplace = normalize_text(item.get("workplaceType"))
    if workplace in {"remote", "hybrid", "onsite", "on site"}:
        job.work_mode = "onsite" if workplace in {"onsite", "on site"} else workplace
    if isinstance(item.get("isRemote"), bool):
        job.remote = item["isRemote"]
        if job.work_mode == "unknown":
            job.work_mode = "remote" if job.remote else "onsite"
    description = item.get("descriptionPlain")
    if isinstance(description, str) and len(description) > len(job.description):
        job.description = description
    if item.get("publishedAt"):
        job.posted_at = str(item["publishedAt"])


def _suggest_ashby_replacement(job: Job, payload: dict, board: str, posting: str) -> None:
    candidates = [
        item
        for item in payload.get("jobs", [])
        if isinstance(item, dict)
        and item.get("isListed", True)
        and normalize_text(item.get("title")) == normalize_text(job.title)
        and not _ashby_job_matches(item, board, posting)
    ]
    if len(candidates) != 1:
        return
    candidate = candidates[0]
    replacement = candidate.get("jobUrl") or candidate.get("applyUrl")
    if isinstance(replacement, str):
        job.replacement_url = replacement
        job.replacement_title = str(candidate.get("title") or job.title)


def _recruitee_offer(payload: dict, slug: str) -> dict | None:
    offers = payload.get("offers", [])
    return next(
        (
            offer
            for offer in offers
            if isinstance(offer, dict) and str(offer.get("slug") or "") == slug
        ),
        None,
    )


def _suggest_recruitee_replacement(job: Job, payload: dict, slug: str) -> None:
    candidates = [
        offer
        for offer in payload.get("offers", [])
        if isinstance(offer, dict)
        and str(offer.get("slug") or "") != slug
        and normalize_text(offer.get("title")) == normalize_text(job.title)
    ]
    if len(candidates) != 1:
        return
    candidate = candidates[0]
    replacement = candidate.get("careers_url") or candidate.get("url")
    if isinstance(replacement, str):
        job.replacement_url = replacement
        job.replacement_title = str(candidate.get("title") or job.title)


def verify_job(job: Job) -> Job:
    targets = list(dict.fromkeys(value for value in (job.canonical_url, job.source_url) if value))
    status, resolved = "unreachable", job.source_url
    for target in targets:
        status, resolved, _ = resolve_url(target)
        if status != "unreachable":
            break

    job.verification_status = status
    if status == "reachable":
        job.canonical_url = resolved
    detected = detect_ats(resolved) or next(
        (candidate for target in targets if (candidate := detect_ats(target))),
        None,
    )
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
            _enrich_greenhouse(job, payload)
        elif provider == "lever":
            region, site, posting = values
            host = "api.eu.lever.co" if region == "eu" else "api.lever.co"
            payload = request_json(
                f"https://{host}/v0/postings/{urllib.parse.quote(site)}/"
                f"{urllib.parse.quote(posting)}"
            )
            job.canonical_url = str(payload.get("applyUrl") or payload.get("hostedUrl") or resolved)
            _enrich_lever(job, payload)
        elif provider == "recruitee":
            company, slug = values
            payload = request_json(
                f"https://{urllib.parse.quote(company)}.recruitee.com/api/offers/"
            )
            matched = _recruitee_offer(payload, slug)
            if matched is None:
                _suggest_recruitee_replacement(job, payload, slug)
                raise LookupError("posting is absent from the Recruitee careers site")
            job.canonical_url = str(matched.get("careers_url") or matched.get("url") or resolved)
            if isinstance(matched.get("remote"), bool):
                job.remote = matched["remote"]
                job.work_mode = "remote" if job.remote else job.work_mode
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
                _suggest_ashby_replacement(job, payload, board, posting)
                raise LookupError("posting is absent from the Ashby board")
            job.canonical_url = str(matched.get("applyUrl") or matched.get("jobUrl") or resolved)
            _enrich_ashby(job, matched)
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
