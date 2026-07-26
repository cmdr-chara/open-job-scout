from open_job_scout.models import Job
from open_job_scout.verification import (
    MAX_JSON_BYTES,
    _ashby_job_matches,
    _read_limited,
    detect_ats,
    is_safe_public_url,
    page_indicates_closed,
    resolve_url,
    verify_job,
    visible_text,
)


def test_visible_job_not_found_is_closed() -> None:
    html = """
    <html><body><main><h1>Job not found</h1>
    <p>This position is no longer available.</p></main></body></html>
    """
    assert page_indicates_closed(html) is True


def test_script_marker_does_not_create_false_positive() -> None:
    html = """
    <html><body><h1>Backend Engineer</h1>
    <script>const fallback = "job not found";</script></body></html>
    """
    assert visible_text(html) == "backend engineer"
    assert page_indicates_closed(html) is False


def test_incidental_body_marker_does_not_close_live_job() -> None:
    html = """
    <html><body><h1>Backend Engineer</h1><p>Apply now.</p>
    <section><h3>FAQ</h3><p>What happens if a job is no longer available?</p></section>
    <p>We are looking for an engineer to join our team and build reliable software.</p>
    </body></html>
    """
    assert page_indicates_closed(html) is False


def test_private_and_non_http_targets_are_not_safe(monkeypatch) -> None:
    assert is_safe_public_url("file:///etc/passwd") is False
    assert is_safe_public_url("http://localhost:8080") is False
    assert is_safe_public_url("http://127.0.0.1") is False
    assert is_safe_public_url("http://169.254.169.254") is False
    assert resolve_url("file:///etc/passwd")[0] == "unreachable"

    monkeypatch.setattr(
        "open_job_scout.verification.socket.getaddrinfo",
        lambda *_args, **_kwargs: [(None, None, None, None, ("8.8.8.8", 443))],
    )
    assert is_safe_public_url("https://public.example") is True

    monkeypatch.setattr(
        "open_job_scout.verification.socket.getaddrinfo",
        lambda *_args, **_kwargs: [(None, None, None, None, ("10.0.0.1", 443))],
    )
    assert is_safe_public_url("https://private.example") is False


def test_limited_response_read_rejects_oversized_body() -> None:
    class Response:
        headers: dict[str, str] = {}

        @staticmethod
        def read(_size: int) -> bytes:
            return b"x" * (MAX_JSON_BYTES + 1)

    try:
        _read_limited(Response(), MAX_JSON_BYTES)
    except ValueError as exc:
        assert "allowed size" in str(exc)
    else:
        raise AssertionError("oversized response was accepted")


def test_greenhouse_host_requires_a_real_hostname_boundary() -> None:
    assert detect_ats("https://notgreenhouse.io/jobs/123") is None
    assert detect_ats("https://boards.greenhouse.io/example/jobs/123") == (
        "greenhouse",
        ("example", "123"),
    )


def test_ashby_match_requires_exact_board_and_posting() -> None:
    item = {"jobUrl": "https://jobs.ashbyhq.com/acme/1234"}
    assert _ashby_job_matches(item, "acme", "1234") is True
    assert (
        _ashby_job_matches(
            {"applyUrl": "https://jobs.ashbyhq.com/acme/1234/application"},
            "acme",
            "1234",
        )
        is True
    )
    assert _ashby_job_matches(item, "acme", "234") is False
    assert _ashby_job_matches(item, "other", "1234") is False


def test_verification_falls_back_to_source_listing(monkeypatch) -> None:
    calls: list[str] = []

    def fake_resolve(url: str) -> tuple[str, str, str]:
        calls.append(url)
        if "careers.example" in url:
            return "unreachable", url, ""
        return "reachable", url, "<h1>Backend Engineer</h1>"

    monkeypatch.setattr("open_job_scout.verification.resolve_url", fake_resolve)
    job = Job(
        title="Backend Engineer",
        company="Example",
        source_url="https://board.example/jobs/1",
        canonical_url="https://careers.example/jobs/1",
    )

    verified = verify_job(job)
    assert calls == [
        "https://careers.example/jobs/1",
        "https://board.example/jobs/1",
    ]
    assert verified.verification_status == "reachable"
    assert verified.canonical_url == "https://board.example/jobs/1"
