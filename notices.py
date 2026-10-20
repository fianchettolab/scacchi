#!/usr/bin/env python3
"""Writes THIRD_PARTY_LICENSES.txt: every crate linked into the iOS library (Rust's standard
library aside), with its licence files as published in the crate. Apps that ship Scacchi
bundle this file. Re-run after changing dependencies."""

import json
import pathlib
import subprocess

here = pathlib.Path(__file__).resolve().parent
target = "aarch64-apple-ios"


def cargo(*args):
    return subprocess.run(["cargo", *args], cwd=here, capture_output=True, text=True, check=True).stdout


metadata = json.loads(cargo("metadata", "--format-version", "1", "--filter-platform", target))
tree = cargo("tree", "-e", "normal,no-proc-macro", "--target", target, "--prefix", "none")
linked = {line.split()[0]: line.split()[1].lstrip("v") for line in tree.splitlines() if line.strip()}

# Full texts for licences named by crates that ship no licence file.
named_texts = {
    "GPL-3.0-or-later": here / "LICENSE",
    "MPL-2.0": here / "licenses" / "MPL-2.0.txt",
}

out = ["Third-party components linked into Scacchi (https://github.com/fianchettolab/scacchi).", ""]
count = 0
seen = {}  # licence text -> the crate it was first printed under
needed = set()
for package in sorted(metadata["packages"], key=lambda p: p["name"]):
    name, version, license = package["name"], package["version"], package["license"]
    if name == "scacchi" or linked.get(name) != version:
        continue
    count += 1
    root = pathlib.Path(package["manifest_path"]).parent
    prefixes = ("LICENSE", "LICENCE", "COPYING", "UNLICENSE")
    files = sorted(f for f in root.iterdir() if f.is_file() and f.name.upper().startswith(prefixes))
    out += ["=" * 78, f"{name} {version}: {license}", package.get("repository") or "", ""]
    if not files:
        assert license in named_texts, f"{name}: no licence file and no text for {license}"
        needed.add(license)
        out += [f"Licensed under {license}; its text is at the end of this file.", ""]
    for f in files:
        text = f.read_text(errors="replace").rstrip()
        if text in seen:
            out += [f"--- {f.name}: same text as in {seen[text]}", ""]
        else:
            seen[text] = f"{name} {version}"
            out += [f"--- {f.name}", text, ""]

for license in sorted(needed):
    out += ["=" * 78, license, "", named_texts[license].read_text().rstrip(), ""]

(here / "THIRD_PARTY_LICENSES.txt").write_text("\n".join(out) + "\n")
print(f"Wrote THIRD_PARTY_LICENSES.txt ({count} crates)")
