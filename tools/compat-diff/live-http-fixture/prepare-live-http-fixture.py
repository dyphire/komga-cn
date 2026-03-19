#!/usr/bin/env python3

from __future__ import annotations

import argparse
import filecmp
import os
import shutil
import sys
from pathlib import Path
from zipfile import ZipFile, is_zipfile


FIXED_MTIME = 1704067200  # 2024-01-01T00:00:00Z
EXPECTED_RELATIVE_PATHS = (
    Path("library1"),
    Path("library1/series"),
    Path("library1/series/book.cbr"),
)


def repo_root() -> Path:
    return Path(__file__).resolve().parents[3]


def source_archive() -> Path:
    return repo_root() / "komga/src/test/resources/archives/zip.zip"


def touch_fixed(path: Path) -> None:
    os.utime(path, (FIXED_MTIME, FIXED_MTIME))


def normalize_permissions(path: Path, is_directory: bool) -> None:
    path.chmod(0o755 if is_directory else 0o644)


def ensure_directory(path: Path) -> None:
    if path.exists() and not path.is_dir():
        raise SystemExit(f"expected directory, found file: {path}")
    path.mkdir(parents=True, exist_ok=True)
    normalize_permissions(path, True)


def copy_archive(source: Path, destination: Path) -> None:
    if destination.exists():
        if destination.is_dir():
            raise SystemExit(f"expected file, found directory: {destination}")
        if not filecmp.cmp(source, destination, shallow=False):
            raise SystemExit(f"refusing to overwrite different file: {destination}")
    else:
        shutil.copyfile(source, destination)

    normalize_permissions(destination, False)
    touch_fixed(destination)


def prepare(output_root: Path, source: Path) -> Path:
    if not source.is_file():
        raise SystemExit(f"source archive not found: {source}")

    library_dir = output_root / "library1"
    series_dir = library_dir / "series"
    book_path = series_dir / "book.cbr"

    ensure_directory(output_root)
    ensure_directory(library_dir)
    ensure_directory(series_dir)
    copy_archive(source, book_path)

    for path, is_directory in (
        (book_path, False),
        (series_dir, True),
        (library_dir, True),
        (output_root, True),
    ):
        normalize_permissions(path, is_directory)
        touch_fixed(path)

    return book_path


def collect_relative_entries(root: Path) -> list[Path]:
    entries: list[Path] = []
    for current_root, dirnames, filenames in os.walk(root):
        current_path = Path(current_root)
        for dirname in dirnames:
            entries.append((current_path / dirname).relative_to(root))
        for filename in filenames:
            entries.append((current_path / filename).relative_to(root))
    return sorted(entries)


def verify(output_root: Path, source: Path) -> None:
    if not output_root.is_dir():
        raise SystemExit(f"output directory not found: {output_root}")

    actual_entries = collect_relative_entries(output_root)
    expected_entries = sorted(EXPECTED_RELATIVE_PATHS)
    if actual_entries != expected_entries:
        raise SystemExit(f"unexpected output structure: {actual_entries}")

    book_path = output_root / "library1/series/book.cbr"
    if not book_path.is_file():
        raise SystemExit(f"missing book archive: {book_path}")
    if not is_zipfile(book_path):
        raise SystemExit(f"book archive is not a zip-compatible file: {book_path}")

    with ZipFile(book_path) as archive:
        contents = archive.namelist()
    if contents != ["komga.png"]:
        raise SystemExit(f"unexpected archive contents: {contents}")

    for relative_path in EXPECTED_RELATIVE_PATHS:
        stat = (output_root / relative_path).stat()
        if int(stat.st_mtime) != FIXED_MTIME:
            raise SystemExit(f"unexpected mtime for {relative_path}: {int(stat.st_mtime)}")


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Prepare and verify a live HTTP fixture.")
    parser.add_argument("command", choices=("prepare", "verify"), help="fixture action")
    parser.add_argument("output", type=Path, help="fixture output directory")
    parser.add_argument(
        "--source",
        type=Path,
        default=source_archive(),
        help="archive to copy into the fixture",
    )
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    if args.command == "prepare":
        prepare(args.output, args.source)
        return 0

    verify(args.output, args.source)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
