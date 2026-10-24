#!/usr/bin/env python3
"""Small dependency-free documentation consistency check."""

from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]


def markdown_files():
    paths = [ROOT / "README.md", ROOT / "CHANGELOG.md"]
    paths.extend(sorted((ROOT / "docs").glob("*.md")))
    paths.extend(sorted((ROOT / "examples").rglob("*.md")))
    return paths


def main() -> int:
    errors = []
    files = markdown_files()
    for path in files:
        text = path.read_text(encoding="utf-8")
        fence_count = sum(
            line.startswith(("```", "~~~")) for line in text.splitlines()
        )
        if fence_count % 2:
            errors.append(f"{path.relative_to(ROOT)}: unbalanced code fences")
        for target in re.findall(r"\]\(([^)#]+)", text):
            if target.startswith(("http://", "https://", "mailto:")):
                continue
            resolved = (path.parent / target).resolve()
            if not resolved.exists():
                errors.append(f"{path.relative_to(ROOT)}: missing link target {target}")
    if errors:
        print("documentation check failed:")
        print("\n".join(errors))
        return 1
    print(f"documentation check passed ({len(files)} markdown files)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
