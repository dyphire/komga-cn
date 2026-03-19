#!/usr/bin/env python3

from __future__ import annotations

import argparse
import sqlite3
import sys
from pathlib import Path


FIXED_TIMESTAMP = "2024-01-02 03:04:05"
FIXED_LIBRARY_ID = "1"
FIXED_LIBRARY_ROOT = "file:/library1"
FIXED_SERIES_ID = "series-1"
FIXED_SERIES_URL = "file:/series"
FIXED_BOOK_ID = "book-1"
FIXED_BOOK_URL = "file:/book.cbr"


REQUIRED_TABLES = (
    "LIBRARY",
    "SERIES",
    "SERIES_METADATA",
    "BOOK",
    "MEDIA",
    "BOOK_METADATA",
    "BOOK_METADATA_AGGREGATION",
)


FIXTURE_ROWS = (
    (
        "LIBRARY",
        ("ID",),
        {
            "ID": FIXED_LIBRARY_ID,
            "CREATED_DATE": FIXED_TIMESTAMP,
            "LAST_MODIFIED_DATE": FIXED_TIMESTAMP,
            "NAME": "default",
            "ROOT": FIXED_LIBRARY_ROOT,
            "IMPORT_COMICINFO_BOOK": 1,
            "IMPORT_COMICINFO_SERIES": 1,
            "IMPORT_COMICINFO_COLLECTION": 1,
            "IMPORT_COMICINFO_READLIST": 1,
            "IMPORT_COMICINFO_SERIES_APPEND_VOLUME": 1,
            "IMPORT_EPUB_BOOK": 1,
            "IMPORT_EPUB_SERIES": 1,
            "IMPORT_MYLAR_SERIES": 1,
            "IMPORT_LOCAL_ARTWORK": 1,
            "IMPORT_BARCODE_ISBN": 1,
            "SCAN_FORCE_MODIFIED_TIME": 0,
            "SCAN_STARTUP": 0,
            "SCAN_CBX": 1,
            "SCAN_PDF": 1,
            "SCAN_EPUB": 1,
            "SCAN_INTERVAL": "EVERY_6H",
            "REPAIR_EXTENSIONS": 0,
            "CONVERT_TO_CBZ": 0,
            "EMPTY_TRASH_AFTER_SCAN": 0,
            "SERIES_COVER": "FIRST",
            "HASH_FILES": 1,
            "HASH_PAGES": 0,
            "HASH_KOREADER": 0,
            "ANALYZE_DIMENSIONS": 1,
            "ONESHOTS_DIRECTORY": None,
            "UNAVAILABLE_DATE": None,
        },
    ),
    (
        "SERIES",
        ("ID",),
        {
            "ID": FIXED_SERIES_ID,
            "CREATED_DATE": FIXED_TIMESTAMP,
            "LAST_MODIFIED_DATE": FIXED_TIMESTAMP,
            "FILE_LAST_MODIFIED": FIXED_TIMESTAMP,
            "NAME": "series",
            "URL": FIXED_SERIES_URL,
            "LIBRARY_ID": FIXED_LIBRARY_ID,
            "BOOK_COUNT": 1,
            "DELETED_DATE": None,
            "ONESHOT": 0,
        },
    ),
    (
        "SERIES_METADATA",
        ("SERIES_ID",),
        {
            "SERIES_ID": FIXED_SERIES_ID,
            "CREATED_DATE": FIXED_TIMESTAMP,
            "LAST_MODIFIED_DATE": FIXED_TIMESTAMP,
            "STATUS": "ONGOING",
            "STATUS_LOCK": 0,
            "TITLE": "series",
            "TITLE_LOCK": 0,
            "TITLE_SORT": "series",
            "TITLE_SORT_LOCK": 0,
            "SUMMARY": "",
            "SUMMARY_LOCK": 0,
            "READING_DIRECTION": None,
            "READING_DIRECTION_LOCK": 0,
            "PUBLISHER": "",
            "PUBLISHER_LOCK": 0,
            "AGE_RATING": None,
            "AGE_RATING_LOCK": 0,
            "LANGUAGE": "",
            "LANGUAGE_LOCK": 0,
            "GENRES_LOCK": 0,
            "TAGS_LOCK": 0,
            "TOTAL_BOOK_COUNT": None,
            "TOTAL_BOOK_COUNT_LOCK": 0,
            "SHARING_LABELS_LOCK": 0,
            "LINKS_LOCK": 0,
            "ALTERNATE_TITLES_LOCK": 0,
        },
    ),
    (
        "BOOK",
        ("ID",),
        {
            "ID": FIXED_BOOK_ID,
            "CREATED_DATE": FIXED_TIMESTAMP,
            "LAST_MODIFIED_DATE": FIXED_TIMESTAMP,
            "FILE_LAST_MODIFIED": FIXED_TIMESTAMP,
            "NAME": "book.cbr",
            "URL": FIXED_BOOK_URL,
            "SERIES_ID": FIXED_SERIES_ID,
            "FILE_SIZE": 0,
            "NUMBER": 1,
            "LIBRARY_ID": FIXED_LIBRARY_ID,
            "FILE_HASH": "",
            "FILE_HASH_KOREADER": "",
            "DELETED_DATE": None,
            "ONESHOT": 0,
        },
    ),
    (
        "MEDIA",
        ("BOOK_ID",),
        {
            "BOOK_ID": FIXED_BOOK_ID,
            "MEDIA_TYPE": "application/zip",
            "STATUS": "READY",
            "THUMBNAIL": None,
            "CREATED_DATE": FIXED_TIMESTAMP,
            "LAST_MODIFIED_DATE": FIXED_TIMESTAMP,
            "COMMENT": "",
            "PAGE_COUNT": 1,
            "EPUB_DIVINA_COMPATIBLE": 0,
            "EPUB_IS_KEPUB": 0,
            "_UNUSED": None,
            "EXTENSION_VALUE_BLOB": None,
        },
    ),
    (
        "MEDIA_PAGE",
        ("BOOK_ID", "NUMBER"),
        {
            "BOOK_ID": FIXED_BOOK_ID,
            "NUMBER": 0,
            "FILE_NAME": "komga.png",
            "MEDIA_TYPE": "image/png",
            "WIDTH": None,
            "HEIGHT": None,
            "FILE_HASH": "",
            "FILE_SIZE": 0,
        },
    ),
    (
        "BOOK_METADATA",
        ("BOOK_ID",),
        {
            "BOOK_ID": FIXED_BOOK_ID,
            "CREATED_DATE": FIXED_TIMESTAMP,
            "LAST_MODIFIED_DATE": FIXED_TIMESTAMP,
            "AGE_RATING": None,
            "AGE_RATING_LOCK": 0,
            "NUMBER": "1",
            "NUMBER_LOCK": 0,
            "NUMBER_SORT": 1.0,
            "NUMBER_SORT_LOCK": 0,
            "PUBLISHER": "",
            "PUBLISHER_LOCK": 0,
            "READING_DIRECTION": None,
            "READING_DIRECTION_LOCK": 0,
            "RELEASE_DATE": None,
            "RELEASE_DATE_LOCK": 0,
            "SUMMARY": "",
            "SUMMARY_LOCK": 0,
            "TITLE": "book.cbr",
            "TITLE_LOCK": 0,
            "AUTHORS_LOCK": 0,
            "TAGS_LOCK": 0,
            "ISBN": "",
            "ISBN_LOCK": 0,
            "LINKS_LOCK": 0,
        },
    ),
    (
        "BOOK_METADATA_AGGREGATION",
        ("SERIES_ID",),
        {
            "SERIES_ID": FIXED_SERIES_ID,
            "CREATED_DATE": FIXED_TIMESTAMP,
            "LAST_MODIFIED_DATE": FIXED_TIMESTAMP,
            "RELEASE_DATE": None,
            "SUMMARY": "",
            "SUMMARY_NUMBER": "",
        },
    ),
)


CLEANUP_TABLES = (
    ("SERIES_METADATA_GENRE", "SERIES_ID", FIXED_SERIES_ID),
    ("SERIES_METADATA_TAG", "SERIES_ID", FIXED_SERIES_ID),
    ("SERIES_METADATA_SHARING", "SERIES_ID", FIXED_SERIES_ID),
    ("SERIES_METADATA_LINK", "SERIES_ID", FIXED_SERIES_ID),
    ("SERIES_METADATA_ALTERNATE_TITLE", "SERIES_ID", FIXED_SERIES_ID),
    ("BOOK_METADATA_AGGREGATION_AUTHOR", "SERIES_ID", FIXED_SERIES_ID),
    ("BOOK_METADATA_AGGREGATION_TAG", "SERIES_ID", FIXED_SERIES_ID),
    ("BOOK_METADATA_AUTHOR", "BOOK_ID", FIXED_BOOK_ID),
    ("BOOK_METADATA_TAG", "BOOK_ID", FIXED_BOOK_ID),
    ("BOOK_METADATA_LINK", "BOOK_ID", FIXED_BOOK_ID),
    ("MEDIA_PAGE", "BOOK_ID", FIXED_BOOK_ID),
    ("MEDIA_FILE", "BOOK_ID", FIXED_BOOK_ID),
    ("READ_PROGRESS", "BOOK_ID", FIXED_BOOK_ID),
    ("READ_PROGRESS_SERIES", "SERIES_ID", FIXED_SERIES_ID),
)


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Seed a Komga localdb with the live diff fixture.")
    parser.add_argument(
        "--database",
        type=Path,
        default=Path("localdb/localdb.sqlite"),
        help="path to the SQLite database to seed",
    )
    parser.add_argument(
        "--fixture-root",
        type=Path,
        help="path to a prepared live-http-fixture root",
    )
    return parser.parse_args(argv)


def require_file(path: Path) -> Path:
    if not path.is_file():
        raise SystemExit(f"database not found: {path}")
    return path.resolve()


def table_columns(conn: sqlite3.Connection, table: str) -> set[str]:
    rows = conn.execute(f'PRAGMA table_info("{table}")').fetchall()
    return {row[1] for row in rows}


def ensure_required_tables(conn: sqlite3.Connection) -> None:
    missing = [table for table in REQUIRED_TABLES if not table_columns(conn, table)]
    if missing:
        raise SystemExit(f"missing required tables: {', '.join(missing)}")


def delete_rows(conn: sqlite3.Connection, table: str, column: str, value: str) -> None:
    if not table_columns(conn, table):
        return
    conn.execute(f'DELETE FROM "{table}" WHERE "{column}" = ?', (value,))


def file_url(path: Path) -> str:
    return path.resolve().as_uri()


def fixture_urls(fixture_root: Path | None) -> tuple[str, str, str]:
    if fixture_root is None:
        return FIXED_LIBRARY_ROOT, FIXED_SERIES_URL, FIXED_BOOK_URL

    book_path = (fixture_root / "library1/series/book.cbr").resolve()
    if not book_path.is_file():
        raise SystemExit(f"prepared fixture book not found: {book_path}")

    return file_url(book_path.parent.parent), file_url(book_path.parent), file_url(book_path)


def upsert_row(
    conn: sqlite3.Connection,
    table: str,
    key_columns: tuple[str, ...],
    row: dict[str, object],
) -> None:
    columns = table_columns(conn, table)
    values = {key: value for key, value in row.items() if key in columns}
    if not values:
        return

    ordered_columns = [column for column in row.keys() if column in values]
    placeholders = ", ".join("?" for _ in ordered_columns)
    quoted_columns = ", ".join(f'"{column}"' for column in ordered_columns)
    params = [values[column] for column in ordered_columns]

    mutable_columns = [column for column in ordered_columns if column not in key_columns]
    if mutable_columns:
        updates = ", ".join(f'"{column}" = excluded."{column}"' for column in mutable_columns)
        conflict = ", ".join(f'"{column}"' for column in key_columns)
        sql = (
            f'INSERT INTO "{table}" ({quoted_columns}) VALUES ({placeholders}) '
            f'ON CONFLICT ({conflict}) DO UPDATE SET {updates}'
        )
    else:
        conflict = ", ".join(f'"{column}"' for column in key_columns)
        sql = f'INSERT INTO "{table}" ({quoted_columns}) VALUES ({placeholders}) ON CONFLICT ({conflict}) DO NOTHING'

    conn.execute(sql, params)


def seed_database(database: Path, fixture_root: Path | None) -> None:
    library_root_url, series_url, book_url = fixture_urls(fixture_root)

    with sqlite3.connect(database, timeout=10.0) as conn:
        conn.execute("PRAGMA foreign_keys = ON")
        ensure_required_tables(conn)

        for table, column, value in CLEANUP_TABLES:
            delete_rows(conn, table, column, value)

        fixture_rows = (
            (
                "LIBRARY",
                ("ID",),
                {
                    **FIXTURE_ROWS[0][2],
                    "ROOT": library_root_url,
                },
            ),
            (
                "SERIES",
                ("ID",),
                {
                    **FIXTURE_ROWS[1][2],
                    "URL": series_url,
                },
            ),
            FIXTURE_ROWS[2],
            (
                "BOOK",
                ("ID",),
                {
                    **FIXTURE_ROWS[3][2],
                    "URL": book_url,
                },
            ),
            FIXTURE_ROWS[4],
            FIXTURE_ROWS[5],
            FIXTURE_ROWS[6],
        )

        for table, key_columns, row in fixture_rows:
            upsert_row(conn, table, key_columns, row)

        conn.commit()


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    database = require_file(args.database)
    seed_database(database, args.fixture_root)
    print(f"seeded fixture into {database}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
