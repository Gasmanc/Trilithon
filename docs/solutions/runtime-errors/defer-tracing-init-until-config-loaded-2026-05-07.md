---
track: bug
problem_type: correctness
root_cause: api-misuse
resolution_type: refactored
severity: critical
title: "Defer tracing subscriber init until after config is loaded"
slug: defer-tracing-init-until-config-loaded
date: 2026-05-07
phase_id: "1"
generalizable: true
one_sentence_lesson: "Never install a global subscriber with hardcoded defaults before the config is loaded — defer init until after config load so user-configured log level and format actually take effect."
tags: [rust, tracing, tracing-subscriber, observability, config, global-state]
---

## Context

A daemon loads `[tracing].log_filter` and `[tracing].format` from a TOML config file. The subscriber must be initialized with the user-configured values. Installing it with hardcoded defaults before config load means the config values are silently ignored.

## What Happened

`main()` called `observability::init` with hardcoded `"info,trilithon=info"` filter and `Pretty` format before CLI dispatch. When `run_daemon` later loaded the config and called `init` a second time, `tracing` returned `AlreadyInstalled` — silently ignored. The daemon always ran with hardcoded values regardless of `config.toml`. The fix moved `observability::init` into `run_daemon` and `config_show::run` after `DaemonConfig` was successfully loaded.

## Lesson

> Never install a global subscriber with hardcoded defaults before the config is loaded — defer init until after config load so user-configured log level and format actually take effect.

## Applies When

- Any global tracing/logging initialization that should respect config-file or env-var settings
- Using `tracing_subscriber::registry().init()` or any `set_global_default()` call
- The log level or format is a user-visible configuration option

## Does Not Apply When

- A deliberate pre-config "bootstrap" subscriber is intentional and documented (e.g. printing a startup line before any config is available), as long as it is replaced or supplemented after config load
- The config cannot fail to load (no user-configurable logging settings)
