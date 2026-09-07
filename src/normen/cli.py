from __future__ import annotations

import argparse

from normen.config import Config
from normen.tui import NormenApp


def main() -> None:
    parser = argparse.ArgumentParser(
        prog="normen",
        description="Deutsche Gesetze lesen und durchsuchen (gesetze-im-internet.de).",
    )
    parser.add_argument(
        "law",
        nargs="?",
        help="Kürzel, z.B. BGB, GG, StGB, VwGO",
    )
    parser.add_argument(
        "norm",
        nargs="?",
        help="Normnummer (433, 31a) oder /Volltextsuche",
    )
    parser.add_argument(
        "--refresh",
        action="store_true",
        help="Gesetzestexte neu von gesetze-im-internet.de laden",
    )
    args = parser.parse_args()
    config = Config()
    config.ensure_file()
    NormenApp(
        initial_law=args.law,
        initial_norm=args.norm,
        refresh=args.refresh,
        config=config,
    ).run()
