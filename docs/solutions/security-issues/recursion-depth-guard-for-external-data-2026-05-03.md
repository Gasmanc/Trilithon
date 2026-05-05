---
name: Recursion depth guard for recursive traversal of operator-supplied data
description: Any recursive walk over operator-supplied or external structures needs a depth cap — adversarial input can trigger unbounded recursion and overflow the stack
type: solution
category: security-issues
phase_id: onboard-git-history
source_commit: 8a5180d
source_date: 2026-05-03
one_sentence_lesson: Any recursive traversal over operator-supplied structures (config files, API responses, JSON trees) needs an explicit depth limit — without one, adversarially crafted deeply-nested input can overflow the stack.
---

## Problem

`collect_module_ids` in `hyper_client.rs` recursively walked every node in a Caddy JSON config to collect `module` IDs. The config is retrieved from a live Caddy process, meaning it's operator-supplied. A crafted config with 10,000 levels of nesting would overflow the stack.

```rust
fn collect_module_ids(value: &serde_json::Value, out: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for v in map.values() {
                collect_module_ids(v, out);  // unbounded recursion
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                collect_module_ids(v, out);  // unbounded recursion
            }
        }
        _ => {}
    }
}
```

## Fix

Add an inner function with a depth counter. Return early when the limit is exceeded:

```rust
const MAX_COLLECT_DEPTH: usize = 128;

fn collect_module_ids(value: &serde_json::Value, out: &mut BTreeSet<String>) {
    collect_module_ids_inner(value, out, 0);
}

fn collect_module_ids_inner(value: &serde_json::Value, out: &mut BTreeSet<String>, depth: usize) {
    if depth >= MAX_COLLECT_DEPTH {
        return;
    }
    match value {
        serde_json::Value::Object(map) => {
            for v in map.values() {
                collect_module_ids_inner(v, out, depth + 1);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                collect_module_ids_inner(v, out, depth + 1);
            }
        }
        _ => {}
    }
}
```

## When to apply

Any recursive function that processes externally-sourced data: JSON/YAML/TOML parsers, config walkers, AST traversals fed by user input, directory trees. 128 is a reasonable default for config structures.

## Category

security-issues
