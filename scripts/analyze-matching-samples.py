#!/usr/bin/env python3
# Copyright (c) 2026 DorianRudolph
# SPDX-License-Identifier: MIT
"""Classify macOS sample stacks beneath the matching-only worker root."""
import argparse
import json
import re
from collections import Counter
from pathlib import Path


def analyze(path):
    graph = path.read_text().split("Call graph:", 1)[1].split("Total number in stack", 1)[0]
    nodes, stack = [], []
    for line in graph.splitlines():
        match = re.match(r"^([ +!|:.`\t-]*)(\d+) (.+)", line)
        if not match:
            continue
        prefix, count, symbol = match.groups()
        depth = len(prefix)
        if depth == 4 and symbol.startswith("Thread_"):
            if nodes:
                break
            if "com.apple.main-thread" not in symbol:
                continue
        if not nodes and "com.apple.main-thread" not in symbol:
            continue
        while stack and depth <= stack[-1]["depth"]:
            stack.pop()
        node = dict(depth=depth, count=int(count), symbol=symbol, parent=stack[-1] if stack else None, children=[])
        if stack:
            stack[-1]["children"].append(node)
        stack.append(node)
        nodes.append(node)
    if not nodes:
        raise ValueError(f"missing main-thread graph: {path}")
    stages, inclusive = Counter(), Counter()
    matched = 0
    for node in nodes:
        weight = node["count"] - sum(child["count"] for child in node["children"])
        if weight < 0:
            raise ValueError("inconsistent sampling tree")
        symbols = []
        ancestor = node
        while ancestor:
            symbols.append(ancestor["symbol"])
            ancestor = ancestor["parent"]
        trace = "\n".join(symbols)
        if "::MatchingPass(" not in trace:
            continue
        matched += weight
        stage = next((name for api, name in [("FPDFText_FindStart", "setup"), ("FPDFText_FindNext", "scan"),
                     ("FPDFText_GetSch", "range_getters"), ("FPDFText_FindClose", "close")] if api in trace), "harness")
        stages[stage] += weight
        predicates = {
            "case_folding": any(x in trace for x in ["cpdf_textpagefind5lower", "MakeLower", "ucase_tolower"]),
            "scalar_decoding_and_position_map": "15unicode_scalars" in trace,
            "page_text_copy": "13get_page_text" in trace or "CPDF_TextPage::GetPageText" in trace,
            "memcmp_in_scan": stage == "scan" and "memcmp" in trace,
            "allocator": any(x in trace for x in ["malloc", "_free", "zone_free", "___rdl_", "___rust_alloc"]),
        }
        for name, applies in predicates.items():
            if applies:
                inclusive[name] += weight
    if not matched:
        raise ValueError("no matching samples")
    return dict(file=path.name, directory=str(path.parent), main_thread_samples=nodes[0]["count"], matching_samples=matched,
                stages=dict(stages), stage_percent={k: 100*v/matched for k, v in stages.items()},
                inclusive=dict(inclusive), inclusive_percent={k: 100*v/matched for k, v in inclusive.items()})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directories", nargs="+", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    results = [analyze(p) for directory in args.directories for p in sorted(directory.glob("*.sample.txt"))]
    if not results:
        parser.error("no samples")
    args.output.write_text(json.dumps(results, indent=2) + "\n")
    for row in results:
        print(row["file"], {k: round(v, 1) for k, v in row["stage_percent"].items()},
              {k: round(v, 1) for k, v in row["inclusive_percent"].items()})


if __name__ == "__main__":
    main()
