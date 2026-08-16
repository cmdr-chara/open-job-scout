# Background TUI operations

Rust v2 keeps long-running network work off the terminal event loop.

Inside the interactive app:

- `S` starts native first-party provider discovery in a worker thread;
- `V` re-verifies tracked listings and reranks them in a worker thread;
- navigation, job details, search, notes, and pipeline views continue rendering while the operation runs;
- the header shows an animated activity indicator and operation label;
- starting another long-running operation while one is active is rejected with a compact status message;
- when the worker finishes, the TUI reloads the shared SQLite tracker and surfaces a concise completion summary;
- provider warnings and generated report paths are summarized without printing into the alternate terminal screen.

The worker layer opens its own short-lived SQLite connection. WAL mode and the existing manual-status ownership rules keep background discovery/recheck compatible with user-driven tracker edits.
