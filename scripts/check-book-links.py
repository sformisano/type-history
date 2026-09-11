#!/usr/bin/env python3
"""Check local links and fragments in the generated mdBook, without network access."""

from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit
import tomllib


class Page(HTMLParser):
    def __init__(self, source):
        super().__init__(convert_charrefs=True)
        self.ids = set()
        self.links = []
        self.feed(source)

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if "id" in attrs:
            self.ids.add(attrs["id"])
        for name in ("href", "src"):
            if name in attrs:
                self.links.append(attrs[name])


def main():
    repo = Path(__file__).resolve().parent.parent
    config = tomllib.loads((repo / "book/book.toml").read_text(encoding="utf-8"))
    output = (repo / "book" / config["build"]["build-dir"]).resolve()
    site = config["output"]["html"]["site-url"]
    pages = {p: Page(p.read_text(encoding="utf-8")) for p in output.rglob("*.html")}
    if output / "index.html" not in pages:
        raise SystemExit("Build the book first: mdbook build book")

    failures = []
    checked = 0
    for path, page in pages.items():
        for link in page.links:
            url = urlsplit(link)
            if url.scheme or url.netloc:
                continue
            target = unquote(url.path)
            if target.startswith(site):
                destination = output / target[len(site):]
            elif target.startswith("/"):
                failures.append(f"{path.relative_to(output)}: unexpected absolute link {link}")
                continue
            else:
                destination = path.parent / target if target else path
            destination = destination.resolve()
            if destination.is_dir():
                destination /= "index.html"
            checked += 1
            if not destination.is_relative_to(output) or not destination.is_file():
                failures.append(f"{path.relative_to(output)}: missing local target {link}")
            elif url.fragment and destination in pages:
                if unquote(url.fragment) not in pages[destination].ids:
                    failures.append(f"{path.relative_to(output)}: missing fragment {link}")

    if failures:
        raise SystemExit("\n".join(failures))
    print(f"Checked {checked} local links across {len(pages)} generated pages.")


if __name__ == "__main__":
    main()
