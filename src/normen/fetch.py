from __future__ import annotations

import io
import zipfile
from pathlib import Path
from urllib.request import Request, urlopen

from normen.catalog import LawRef
from normen.models import Law
from normen.parser import parse_law_xml

SOURCE_BASE = "https://www.gesetze-im-internet.de"
USER_AGENT = "normen/0.1 (+https://www.gesetze-im-internet.de/)"


def default_cache_dir() -> Path:
    return Path.home() / ".normen"


def xml_zip_url(slug: str) -> str:
    return f"{SOURCE_BASE}/{slug}/xml.zip"


def download_law_xml(slug: str) -> bytes:
    request = Request(xml_zip_url(slug), headers={"User-Agent": USER_AGENT})
    with urlopen(request, timeout=60) as response:
        archive = response.read()
    with zipfile.ZipFile(io.BytesIO(archive)) as zipped:
        name = next(item for item in zipped.namelist() if item.endswith(".xml"))
        return zipped.read(name)


class LawLibrary:
    def __init__(
        self,
        cache_dir: Path | None = None,
        downloader=download_law_xml,
    ) -> None:
        self.cache_dir = cache_dir or default_cache_dir()
        self.downloader = downloader

    def load(self, ref: LawRef, refresh: bool = False) -> Law:
        return parse_law_xml(self.load_xml(ref, refresh=refresh))

    def load_xml(self, ref: LawRef, refresh: bool = False) -> bytes:
        self.cache_dir.mkdir(parents=True, exist_ok=True)
        cache_path = self.cache_dir / f"{ref.slug}.xml"
        if refresh or not cache_path.exists():
            data = self.downloader(ref.slug)
            cache_path.write_bytes(data)
            return data
        return cache_path.read_bytes()
