from open_job_scout.verification import page_indicates_closed, visible_text


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
