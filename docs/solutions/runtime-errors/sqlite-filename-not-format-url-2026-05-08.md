---
track: bug
problem_type: integration
root_cause: environment-mismatch
resolution_type: fixed
severity: high
title: "Use SqliteConnectOptions::filename() not a formatted URL string to open SQLite files"
slug: sqlite-filename-not-format-url
date: 2026-05-08
phase_id: "onboard-git-history"
source_commit: 1050f8b
generalizable: true
one_sentence_lesson: "Construct SQLite connection options with SqliteConnectOptions::new().filename(path) instead of format!(\"sqlite://{}\", path) — URL parsing silently mishandles paths that contain spaces, #, or ? characters."
tags: [rust, sqlite, sqlx, path-handling, url-encoding]
---

## Context

`SqliteStorage::open()` constructed the database URL with `format!("sqlite://{}/trilithon.db", data_dir.display())` and then parsed it with `SqliteConnectOptions::from_str`. This is the pattern shown in many early sqlx examples.

## What Happened

`display()` emits the raw path without URL-encoding. Any path segment containing a space (e.g. `My Files/`), a `#` (treated as a fragment), or a `?` (treated as a query separator) causes the URL to parse incorrectly or the connection to silently point at the wrong path. On macOS, home directories under `/Users/First Last/` are common; paths chosen by installers or users can contain any of these characters.

The test `open_with_space_in_path` reproduced the failure by opening a database under `My Files/trilithon/` — the format-based path panicked on connection, while `filename()` succeeded.

The fix replaced the `format!` + `from_str` chain with `SqliteConnectOptions::new().filename(data_dir.join("trilithon.db"))`.

## Lesson

> Construct SQLite connection options with `SqliteConnectOptions::new().filename(path)` instead of `format!("sqlite://{}", path)` — URL parsing silently mishandles paths that contain spaces, `#`, or `?` characters.

## Applies When

- Opening a SQLite database at a user-supplied or installer-chosen path with sqlx
- The path comes from an environment variable, config file, or OS-provided directory (home dir, app support dir, temp dir)

## Does Not Apply When

- The path is a compile-time constant with no special characters (e.g. `:memory:` or a fixed test fixture path)
