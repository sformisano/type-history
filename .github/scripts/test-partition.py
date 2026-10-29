#!/usr/bin/env python3
"""Run one CI partition of `cargo test --workspace --all-targets --all-features`.

`rest` runs every target except the `standalone` test binary.
`standalone INDEX TOTAL` runs share INDEX (0-based) of TOTAL disjoint shares of
that binary's tests. Each test belongs to exactly one share, so `rest` and every
share together run each test once. Every partition selects the whole workspace,
so Cargo resolves the same features and builds the same test binaries as an
unpartitioned run.
"""

from pathlib import Path
import json
import statistics
import subprocess
import sys

CARGO_TEST = ["cargo", "test", "--workspace", "--all-features", "--locked"]
STANDALONE = "standalone"
SECONDS = Path(__file__).with_name("standalone-test-seconds.tsv")


def fail(message):
    print(f"test-partition: {message}", file=sys.stderr)
    sys.exit(2)


def show(command):
    print("+ " + " ".join(command), file=sys.stderr, flush=True)


def output(command):
    show(command)
    return subprocess.run(
        command, check=True, stdout=subprocess.PIPE, text=True
    ).stdout


def run(command):
    show(command)
    sys.exit(subprocess.run(command).returncode)


def rest():
    command = ["cargo", "metadata", "--no-deps", "--format-version", "1"]
    metadata = json.loads(output(command + ["--locked"]))
    members = set(metadata["workspace_members"])
    tests = {
        target["name"]
        for package in metadata["packages"]
        if package["id"] in members
        for target in package["targets"]
        if "test" in target["kind"]
    }
    if STANDALONE not in tests:
        fail(f"no `{STANDALONE}` test target to exclude")
    # `--all-targets` with `--tests` narrowed to every other integration test.
    selection = ["--lib", "--bins", "--examples", "--benches"]
    selection += [f"--test={name}" for name in sorted(tests - {STANDALONE})]
    run(CARGO_TEST + selection)


def listed(filters):
    command = CARGO_TEST + ["--test", STANDALONE, "--", "--list"]
    command += ["--format", "terse"] + filters
    return [
        line.removesuffix(": test")
        for line in output(command).splitlines()
        if line.endswith(": test")
    ]


def module(name):
    return name.split("::", 1)[0]


def weigher():
    # Weights only balance the shares. An unrecorded test weighs its module's
    # mean, or the median of all recorded tests in an unrecorded module.
    seconds = {}
    for line in SECONDS.read_text().splitlines():
        if line and not line.startswith("#"):
            name, value = line.split("\t")
            seconds[name] = int(value)
    modules = {}
    for name, value in seconds.items():
        modules.setdefault(module(name), []).append(value)
    means = {name: statistics.mean(values) for name, values in modules.items()}
    median = statistics.median(seconds.values())
    return lambda name: seconds.get(name, means.get(module(name), median))


def shares(names, total, weight):
    # Longest-first greedy assignment. Ties break by name and share index, so
    # every job computes the same shares from the same test list.
    loads = [0] * total
    result = [[] for _ in range(total)]
    for name in sorted(names, key=lambda name: (-weight(name), name)):
        index = min(range(total), key=lambda index: (loads[index], index))
        result[index].append(name)
        loads[index] += weight(name)
    return result


def standalone(index, total):
    if index >= total:
        fail(f"share index {index} must be below {total}")
    names = listed([])
    if not names or len(names) != len(set(names)):
        fail(f"expected unique `{STANDALONE}` test names, got {len(names)}")
    partition = shares(names, total, weigher())
    if sorted(sum(partition, [])) != sorted(names):
        fail("shares do not cover each test exactly once")
    share = sorted(partition[index])
    # libtest runs every test when it receives no filter.
    if not share:
        print(f"test-partition: share {index} has no tests", file=sys.stderr)
        return
    filters = ["--exact"] + share
    if sorted(listed(filters)) != share:
        fail("exact filters select different tests than this share")
    print(
        f"test-partition: share {index} of {total} runs "
        f"{len(share)} of {len(names)} tests",
        file=sys.stderr,
    )
    run(CARGO_TEST + ["--test", STANDALONE, "--"] + filters)


def main(arguments):
    match arguments:
        case ["rest"]:
            rest()
        case ["standalone", index, total] if (
            index.isdigit() and total.isdigit() and int(total) > 0
        ):
            standalone(int(index), int(total))
        case _:
            fail("usage: test-partition.py rest | standalone INDEX TOTAL")


if __name__ == "__main__":
    main(sys.argv[1:])
