---
track: bug
problem_type: correctness
root_cause: wrong-type-assumption
resolution_type: algorithm-replaced
severity: medium
title: "Use chars().count() not byte subtraction for UTF-8 column numbers"
slug: utf8-column-chars-not-bytes
date: 2026-05-07
phase_id: "1"
generalizable: true
one_sentence_lesson: "When computing column numbers in user-visible diagnostics, always count Unicode scalar values (chars()) not bytes — byte offsets produce wrong column numbers for any non-ASCII input."
tags: [rust, utf8, unicode, diagnostics, error-reporting, column-number]
---

## Context

A TOML config loader reports parse errors with line and column numbers. The column was computed from the byte offset returned by `toml::de::Error::span()`. TOML files can contain UTF-8 characters (e.g. in string values, comments, or file paths on non-English systems).

## What Happened

Column was computed as `safe_offset - last_newline_byte_pos` — a byte distance from the last `\n`. For an ASCII-only file this is coincidentally correct, but for any file containing multi-byte UTF-8 characters (e.g. a `data_dir` path like `/Users/tëst/data`) the column would be larger than the visual character position. The fix replaced the subtraction with `text[last_newline..safe_offset].chars().count()`, which counts Unicode scalar values from the last newline to the error byte offset — the correct visual column for most text.

## Lesson

> When computing column numbers in user-visible diagnostics, always count Unicode scalar values (chars()) not bytes — byte offsets produce wrong column numbers for any non-ASCII input.

## Applies When

- Computing line/column positions in any parser error, linter diagnostic, or source-location report
- Parsing or processing user-supplied text files that may contain non-ASCII content
- Implementing `byte_offset_to_line_col()` or equivalent utility functions

## Does Not Apply When

- The output explicitly documents that column numbers are byte offsets (acceptable for binary formats or internal tooling where users won't interpret them visually)
- Processing pure-ASCII guaranteed input (e.g., fixed protocol fields)
