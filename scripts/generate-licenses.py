#!/usr/bin/env python3
"""Generate the complete, linked license inventory for the locked workspace.

All Cargo targets/features and build/dev dependencies are included deliberately.
License texts absent from published crates are fetched from their recorded VCS
revision with --fetch. Later --check runs reuse those immutable texts from the
checked-in notice files; local texts are always reread. Historical crates
without any upstream license file are identified explicitly below.
"""

import argparse
import concurrent.futures
import hashlib
from html.parser import HTMLParser
import json
from pathlib import Path
import re
import subprocess
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
DOCUMENT = ROOT / "THIRD_PARTY_LICENSES.md"
NOTICE_DIR = Path("docs/licenses")
BUNDLE = ROOT / "target/licenses/THIRD_PARTY_LICENSES.md"
MAX_PAGE_BYTES = 250_000
REPO = "https://github.com/DorianRudolph/pdfoxium"
PDFIUM = "f51279699cb59754d1f49aef10eb8e825fe56532"
PDFCRAFT = "3109bb2359759e24ef490e1cdbdba00722b20972"
NAMES = ("LICENSE", "LICENSE-MIT", "LICENSE-APACHE", "LICENSE-ZLIB", "COPYING",
         "COPYRIGHT", "NOTICE", "LICENSE.md", "LICENSE.txt", "UNLICENSE",
         "LICENSE-MIT.txt", "LICENSE-APACHE2.txt", "LICENCE", "COPYING.txt",
         "LICENSE-CC0", "LICENSE-UNLICENSE", "LICENSE-APACHE2")
# Inspected releases without license files. The first two predate Cargo VCS
# metadata; their refs come from release tags. Never extend this exception to
# a new version without inspecting it again.
DECLARED_ONLY_REFS = {
    ("block", "0.1.6"): "47178790cfc9d4a8b092051d8b413b78bd31254a",
    ("malloc_buf", "0.0.6"): "a7811e5f4c6f9685dd57ad9bfe34e9bf6b0ba4f9",
    ("dispatch", "0.2.0"): "82d6c7a5b75dc0c71c3f46f87bb6c16a476f7748",
    ("wasm_sync", "0.1.2"): "cc027b3b27c2e24e3cd5c7a6cc63306eff724af5",
    ("hexf-parse", "0.2.1"): "4225763d744183d720f575ae96d04161b4d08ea0",
}
# Some egui 0.37 archives record a commit that is not reachable in the public
# repository. Use the reachable project-license snapshot recorded by eframe
# 0.37, with the same declared MIT OR Apache-2.0 terms. Disclose this below;
# do not claim that the substitute commit is the source revision of each crate.
LICENSE_REF_OVERRIDES = {
    ("emilk/egui", "7c0b40402271e8d40cda69f87cf3c2617c2e4382"):
        "3c6eb8261c3293ad9c7ea1ed58487e038860ebef",
}


def normalize(text):
    return text.replace("\r\n", "\n").strip() + "\n"


def license_path(path):
    return (any(word in path.name.lower() for word in
                ("license", "licence", "copying", "copyright", "notice"))
            or path.name in ("OFL.txt", "UFL.txt", "Hack-Regular.txt", "egui-icons.txt"))


class NoticeHTML(HTMLParser):
    """Keep all visible text and link destinations from Rust's official notices."""
    def __init__(self):
        super().__init__()
        self.parts = []

    def handle_data(self, data):
        self.parts.append(data)

    def handle_starttag(self, tag, attrs):
        if tag in ("p", "pre", "li", "h1", "h2", "h3", "div", "br"):
            self.parts.append("\n")
        if tag == "a":
            url = dict(attrs).get("href", "")
            if url.startswith("http"):
                self.parts.append(url + " ")

    def text(self):
        return re.sub(r"\n[ \t\n]*\n", "\n\n", "".join(self.parts))


class Inventory:
    def __init__(self, fetch=False):
        self.fetch = fetch
        self.texts = {}
        self.previous = {}
        # Read both the legacy monolith and the split inventory during migration.
        paths = [DOCUMENT, *(DOCUMENT.parent / NOTICE_DIR).glob("*.md")]
        for path in paths:
            if not path.exists():
                continue
            for block in path.read_text().split("<!-- notice -->")[1:]:
                match = re.search(r"````text\n(.*?)\n````", block, re.S)
                if match:
                    text = normalize(match.group(1))
                else:
                    parts = re.findall(r"\[Part \d+\]\(([^)]+)\)", block)
                    if not parts:
                        continue
                    chunks = []
                    for part in parts:
                        # Generated links are sibling filenames, never arbitrary paths.
                        if not re.fullmatch(r"[0-9a-f]{16}-part-\d+\.md", part):
                            raise ValueError(f"Invalid notice part: {part}")
                        chunk = (path.parent / part).read_text()
                        chunks.append(re.search(r"````text\n(.*?)````", chunk, re.S)[1])
                    text = "".join(chunks)
                digest = re.search(r'notice-([0-9a-f]{16})', block)
                if digest and hashlib.sha256(text.encode()).hexdigest()[:16] != digest[1]:
                    raise ValueError(f"Notice content hash mismatch: {path}")
                for url in re.findall(r"<!-- source: (.*?) -->", block):
                    self.previous[url] = text

    def add(self, text, source):
        text = normalize(text)
        if not text.strip() or "<html" in text[:200].lower():
            raise ValueError(f"Not a license text: {source}")
        digest = hashlib.sha256(text.encode()).hexdigest()[:16]
        entry = self.texts.setdefault(digest, {"text": text, "sources": set()})
        entry["sources"].add(source)
        return digest

    def local(self, path, source=None):
        path = Path(path)
        if source is None:
            source = REPO + "/blob/main/" + path.relative_to(ROOT).as_posix()
        raw = path.read_bytes()
        try:
            text = raw.decode("utf-8-sig")
        except UnicodeDecodeError:
            if path.name != "freetype.txt":
                raise
            text = raw.decode("latin-1")  # Oracle FreeType copyright sign.
        return self.add(text, source)

    def remote(self, url):
        if url in self.previous:
            return self.add(self.previous[url], url)
        cache = ROOT / "target/license-cache" / hashlib.sha256(url.encode()).hexdigest()
        if cache.exists():
            text = cache.read_text()
            return self.add(text, url) if text else None
        if not self.fetch:
            return None
        try:
            request = urllib.request.Request(url, headers={"User-Agent": "pdfoxium-license-inventory"})
            with urllib.request.urlopen(request, timeout=30) as response:
                text = response.read().decode("utf-8-sig")
            if url.endswith("/COPYRIGHT-library.html"):
                parser = NoticeHTML()
                parser.feed(text)
                text = parser.text()
        except urllib.error.HTTPError as error:
            if error.code != 404:
                raise
            text = ""
        cache.parent.mkdir(parents=True, exist_ok=True)
        cache.write_text(text)
        return self.add(text, url) if text else None

    def package(self, package):
        root = Path(package["manifest_path"]).parent
        sources = []
        if root.is_relative_to(ROOT):
            paths = [p for p in root.rglob("*") if p.is_file() and license_path(p)
                     and not any(x in p.parts for x in ("target", ".git"))]
            # Generated package copies are not recursive inventory inputs.
            # Every local crate also retains its own original notices.
            paths = [p for p in paths if p.name != DOCUMENT.name]
        else:
            paths = [p for p in root.rglob("*") if p.is_file() and license_path(p)]
        if package["name"] == "r-efi":
            paths.append(root / "AUTHORS")  # Contains its full copyright/MIT grant.
        for path in sorted(paths):
            if package["source"]:
                source = (f"https://docs.rs/crate/{package['name']}/{package['version']}/source/"
                          + path.relative_to(root).as_posix())
            else:
                source = None
            sources.append(self.local(path, source))
        # Font-only notices do not replace the crate's own software license.
        has_root_license = any(p.parent == root for p in paths)
        if not has_root_license:
            key = (package["name"], package["version"])
            if key in DECLARED_ONLY_REFS:
                # The complete tagged repositories were inspected: neither
                # release contains a license file. Preserve the actual MIT
                # declaration and author metadata, and provide MIT's permission
                # text separately, without inventing a copyright year/notice.
                ref = DECLARED_ONLY_REFS[key]
                repo = package["repository"].replace("http://", "https://")
                vcs_path = root / ".cargo_vcs_info.json"
                vcs = json.loads(vcs_path.read_text()) if vcs_path.exists() else {}
                path_in_vcs = vcs.get("path_in_vcs", "parse" if package["name"] == "hexf-parse" else "")
                relative = Path(path_in_vcs) / "Cargo.toml"
                source = repo + "/blob/" + ref + "/" + relative.as_posix()
                original = root / "Cargo.toml.orig"
                sources.append(self.local(original if original.exists() else root / "Cargo.toml", source))
                if package["name"] == "hexf-parse":
                    assert package["license"] == "CC0-1.0"
                    url = "https://raw.githubusercontent.com/spdx/license-list-data/v3.27.0/text/CC0-1.0.txt"
                    notice = self.remote(url)
                    if not notice:
                        raise ValueError(f"Missing CC0 legal text: {url}; rerun with --fetch")
                    sources.append(notice)
                    return sorted(set(sources))
                assert package["license"] in ("MIT", "MIT OR Apache-2.0")
                text = (ROOT / "LICENSE").read_text()
                text = text[text.index("Permission is hereby granted"):]
                authors = ", ".join(package.get("authors", [])) or "Not supplied in the upstream manifest"
                sources.append(self.add(
                    "MIT License (selected from the original package manifest)\n\n"
                    f"Package: {package['name']} {package['version']}\nAuthors: {authors}\n\n"
                    "The tagged upstream release includes no separate copyright/license file. "
                    "The authors and MIT designation above are from its manifest; "
                    "the standard MIT permission text follows, without an invented copyright notice.\n\n" + text,
                    source + "#declared-mit-license"))
                return sorted(set(sources))
            vcs_path = root / ".cargo_vcs_info.json"
            vcs = json.loads(vcs_path.read_text()) if vcs_path.exists() else {}
            ref = vcs.get("git", {}).get("sha1")
            repo = package.get("repository") or ""
            match = re.match(r"https?://github.com/([^/]+/[^/]+)", repo)
            if not match or not ref:
                raise ValueError(f"Missing original license source: {package['name']}")
            repository = match[1].removesuffix('.git')
            ref = LICENSE_REF_OVERRIDES.get((repository, ref), ref)
            base = f"https://raw.githubusercontent.com/{repository}/{ref}/"
            prefixes = [""]
            parent = Path(vcs.get("path_in_vcs", ""))
            prefixes.extend(str(p) + "/" for p in [parent, *parent.parents] if str(p) != ".")
            for prefix in dict.fromkeys(prefixes):
                urls = [base + prefix + name for name in NAMES]
                with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
                    found = list(pool.map(self.remote, urls))
                found = [item for item in found if item]
                if found:
                    sources.extend(found)
                    break
            else:
                raise ValueError(f"Missing original license text: {package['name']} {ref}; rerun with --fetch")
        return sorted(set(sources))


def supplemental(inv):
    entries = []

    def entry(name, project, purpose, paths=(), urls=()):
        ids = [inv.local(ROOT / path) for path in paths]
        for url in urls:
            notice = inv.remote(url)
            if not notice:
                raise ValueError(f"Missing supplemental notice: {url}; rerun with --fetch")
            ids.append(notice)
        entries.append((name, project, purpose, sorted(set(ids))))

    entry("PDFium", "https://pdfium.googlesource.com/pdfium/",
          f"Engine translation, Foxit substitute fonts and CMap tables; baseline `{PDFIUM}`. "
          "Original per-file copyright notices remain in the source.",
          ["upstream/pdfium/LICENSE", "crates/pdfoxium-core/assets/chromefontdata/README.md"])
    entry("Chromium", "https://chromium.googlesource.com/chromium/src/",
          "Viewer selection, search and form behavior derived from Chromium's PDF viewer; BSD-3-Clause.",
          urls=["https://raw.githubusercontent.com/chromium/chromium/refs/tags/142.0.7444.0/LICENSE"])
    entry("PdfCraft / ArtCraft", "https://github.com/storytold/pdfcraft",
          f"Reference for the egui viewer architecture and programmatic control approach; "
          f"license snapshot at `{PDFCRAFT}`. MIT OR Apache-2.0. "
          "PdfCraft branding and its unrelated assets are not included.",
          urls=[f"https://raw.githubusercontent.com/storytold/pdfcraft/{PDFCRAFT}/LICENSE-{kind}"
                for kind in ("MIT", "APACHE")])
    entry("Anti-Grain Geometry 2.3", "https://sourceforge.net/projects/agg/",
          "Rust translation of PDFium's AGG renderer; original permissive AGG 2.3 notice.",
          ["upstream/pdfium/third_party/agg23/copying"])
    entry("OpenJPEG", "https://github.com/uclouvain/openjpeg",
          "Translated JP2 parsing and codec extensions. Further component notices appear with pdfoxium-jpeg2000.",
          ["crates/pdfoxium-core/src/fxcodec/jpx/OPENJPEG-JP2-LICENSE"])
    entry("Hayro codecs", "https://github.com/LaurenzV/hayro",
          "Maintained snapshots of hayro-ccitt 0.4.0, hayro-jbig2 0.3.1 and hayro-jpeg2000 0.4.1, "
          "with PDFium-derived extensions; packaged under distinct pdfoxium codec names. "
          "Their original licenses and OpenJPEG notices appear in the Rust package table.")
    entry("libjpeg-turbo-rs", "https://github.com/developer0hye/libjpeg-turbo-rs",
          "Unmodified production source snapshot at b614c77936e56e6409c06af87fb104a5e75e64ee, "
          "packaged as pdfoxium-jpeg. Its MIT/Apache and test-image notices appear in the package table.")
    entry("egui default fonts", "https://github.com/emilk/egui/tree/main/crates/epaint_default_fonts",
          "Viewer fonts: Hack (MIT/Bitstream Vera), Noto Emoji (OFL), Ubuntu Light (Ubuntu Font Licence), "
          "emoji-icon-font (MIT). Full font notices are included with epaint_default_fonts below.")
    for name, project, path in [
        ("Hack", "https://github.com/source-foundry/Hack", None),
        ("Noto Emoji", "https://github.com/googlefonts/noto-emoji", None),
        ("Ubuntu fonts", "https://design.ubuntu.com/font", None),
        ("emoji-icon-font", "https://github.com/jslegers/emoji-icon-font", None),
        ("Arimo / Chromium test fonts", "https://chromium.googlesource.com/chromium/src/+/main/third_party/test_fonts/",
         "tests/capi/resources/test_fonts/LICENSE.chromium"),
        ("Cantarell", "https://gitlab.gnome.org/GNOME/cantarell-fonts", "tests/capi/fixtures/Cantarell-OFL.txt"),
        ("Roboto", "https://github.com/googlefonts/roboto", "crates/pdfoxium-core/testdata/harfbuzz/LICENSE-Roboto.txt"),
        ("Ahem", "https://www.w3.org/Style/CSS/Test/Fonts/Ahem/", "tests/capi/resources/OFL.txt"),
    ]:
        entry(name, project, "Bundled font or test fixture; Arimo is also embedded in the XFA demo.", [path] if path else [])
    entry("GoogleTest", "https://github.com/google/googletest", "C++ test harness only; version 1.17.0.",
          urls=["https://raw.githubusercontent.com/google/googletest/v1.17.0/LICENSE"])
    rust_version = subprocess.check_output(["rustc", "--version"], text=True).split()[1]
    for version in sorted({"1.95.0", rust_version}):
        source = f"https://github.com/rust-lang/rust/releases/tag/{version}#standard-library-notices"
        if source in inv.previous:
            notice = inv.add(inv.previous[source], source)
        else:
            # COPYRIGHT-library.html is shipped in official rustc distributions,
            # but is not hosted in the online API docs. Read that actual file.
            command = ["rustc"] if version == rust_version else ["rustc", "+" + version]
            sysroot = subprocess.check_output([*command, "--print", "sysroot"], text=True).strip()
            path = Path(sysroot) / "share/doc/rust/COPYRIGHT-library.html"
            parser = NoticeHTML()
            parser.feed(path.read_text())
            notice = inv.add(parser.text(), source)
        entries.append(("Rust standard library " + version, "https://github.com/rust-lang/rust",
                        "Complete COPYRIGHT-library.html from the official rustc distribution, "
                        "converted to plain text with link destinations retained. "
                        "Regenerate when distributing builds made with another toolchain.", [notice]))
    entry("bblanchon PDFium binaries", "https://github.com/bblanchon/pdfium-binaries",
          "Native test oracles only, chromium/8100. These libraries are never production fallbacks.",
          ["vendor/pdfium/chromium-8100/LICENSE"])
    # Retain every notice shipped beside both downloadable native oracle binaries.
    oracle_projects = {
        "abseil": "https://github.com/abseil/abseil-cpp", "agg23": "https://sourceforge.net/projects/agg/",
        "bigint": "https://github.com/mattmccutchen/bigint", "dragonbox": "https://github.com/jk-jeon/dragonbox",
        "fast_float": "https://github.com/fastfloat/fast_float", "fdlibm": "https://netlib.org/fdlibm/",
        "fp16": "https://github.com/Maratyszcza/FP16", "freetype": "https://freetype.org/",
        "harfbuzz": "https://github.com/harfbuzz/harfbuzz", "highway": "https://github.com/google/highway",
        "icu": "https://icu.unicode.org/", "inspector_protocol": "https://chromium.googlesource.com/deps/inspector_protocol/",
        "lcms": "https://www.littlecms.com/", "libjpeg_turbo": "https://github.com/libjpeg-turbo/libjpeg-turbo",
        "libopenjpeg": "https://github.com/uclouvain/openjpeg", "libpng": "https://libpng.org/",
        "libtiff": "https://libtiff.gitlab.io/libtiff/", "llvm-libc": "https://libc.llvm.org/",
        "pdfium": "https://pdfium.googlesource.com/pdfium/", "simdutf": "https://github.com/simdutf/simdutf",
        "strongtalk": "http://www.strongtalk.org/", "v8": "https://v8.dev/", "zlib": "https://zlib.net/",
    }
    grouped = {}
    for path in sorted((ROOT / "vendor/pdfium/chromium-8100").glob("*/licenses/*")):
        name = path.name.split(".")[0].split("-Apache")[0].split("-Boost")[0]
        if name not in oracle_projects:
            raise ValueError(f"Missing oracle project link: {name}")
        grouped.setdefault(name, []).append(path.relative_to(ROOT).as_posix())
    for name, paths in grouped.items():
        entry("Oracle: " + name, oracle_projects[name], "Native test-oracle dependency only.", paths)
    # Small redistributed PDFs and externally fetched benchmark documents retain
    # their own rights statements, independently of the software license.
    for name, project, path in [
        ("Hamiltonian mechanics", "https://tobybartels.name/notes/", "tests/render_corpus/books/hamiltonian.NOTICE.md"),
        ("The Right to Privacy", "https://commons.wikimedia.org/wiki/File:Right_to_Privacy.djvu", "tests/render_corpus/pdfs/right_to_privacy_first_two_pages.NOTICE.md"),
        ("Hayro corpus", "https://github.com/LaurenzV/hayro", "tests/render_corpus/hayro.NOTICE.md"),
    ]:
        entry(name, project, "Test/corpus document provenance and rights; not the engine's software license.", [path])
    return entries


def generate(fetch):
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--all-features", "--format-version", "1"], cwd=ROOT))
    inventory = Inventory(fetch)
    rows = []
    errors = []
    for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
        project = package.get("repository") or package.get("homepage")
        if not project:
            raise ValueError(f"Missing project link: {package['name']}")
        try:
            notices = inventory.package(package)
        except ValueError as error:
            errors.append(str(error))
            continue
        rows.append((package, project, notices))
    if errors:
        raise ValueError("\n".join(errors))
    extra = supplemental(inventory)
    lock_hash = hashlib.sha256((ROOT / "Cargo.lock").read_bytes()).hexdigest()
    declared_only = ", ".join(
        f"`{p['name']}`" for p, _, _ in rows
        if (p["name"], p["version"]) in DECLARED_ONLY_REFS)
    lines = ["# Third-party licenses and attributions", "",
             "Generated by `python3 scripts/generate-licenses.py --fetch`; verify with `--check`.",
             "Every project is linked below. Original license/copyright texts are reproduced in full; "
             "identical texts are shared by reference. Existing per-file notices remain applicable.", "",
             "This inventory covers the locked Rust workspace across **all Cargo targets and features**, "
             "including transitive, build and development dependencies. It is deliberately broader than "
             "any one executable. Native PDFium/GoogleTest entries are test-only. External development "
             "tools (Rust/Python, CMake, Clang, FFmpeg and optional Python packages) are installed separately "
             "and are not redistributed by the engine or viewer. Their own distributions carry their licenses.", "",
             "The translated engine crates declare BSD-3-Clause; repository tooling declares MIT. "
             "Dependencies retain their stated licenses. An `OR` expression offers alternatives; reproducing "
             "both texts here does not require selecting both. Font and document rights are separate.", "",
             declared_only + " do not include "
             "license files in their inspected source revisions. Their original manifest declarations "
             "and available author metadata are preserved below, together with the selected standard "
             "MIT or CC0 legal text; no missing copyright notices have been invented.", "",
             "Some egui 0.37 archives record an unavailable upstream VCS commit "
             "(`7c0b40402271e8d40cda69f87cf3c2617c2e4382`). Their project-license texts "
             "are reproduced from the reachable egui revision recorded by eframe 0.37 "
             "(`3c6eb8261c3293ad9c7ea1ed58487e038860ebef`), under the same declared "
             "MIT OR Apache-2.0 terms; this does not identify their exact source revision.", "",
             f"Cargo.lock SHA-256: `{lock_hash}`. Rust package entries: **{len(rows)}**.", "",
             "## Rust packages", "", "| Package | Declared license | Original notices |",
             "|---|---|---|"]

    def refs(ids):
        return ", ".join(f"[text](#notice-{digest})" for digest in ids) or "See referenced font notices above."

    for package, project, ids in rows:
        lines.append(f"| [{package['name']} {package['version']}]({project}) | "
                     f"{package['license'] or 'See original notices'} | {refs(ids)} |")
    lines += ["", "## Derived code, bundled assets and test-only components", ""]
    for name, project, purpose, ids in extra:
        lines += [f"### [{name}]({project})", "", purpose, "", refs(ids), ""]
    lines += ["## Full license and notice texts", ""]
    for digest, entry in sorted(inventory.texts.items()):
        lines += ["<!-- notice -->", f'<a id="notice-{digest}"></a>', f"### Notice {digest}", ""]
        for source in sorted(entry["sources"]):
            lines += [f"<!-- source: {source} -->", f"Source: <{source}>"]
        lines += ["", "````text", entry["text"].rstrip(), "````", ""]
    return "\n".join(lines)


def split_document(text):
    """Keep full notices locally while making each repository page browsable."""
    heading, *blocks = text.split("<!-- notice -->")
    files = {}
    for block in blocks:
        digest = re.search(r'notice-([0-9a-f]{16})', block)[1]
        path = NOTICE_DIR / f"{digest}.md"
        page = "<!-- notice -->" + block
        if len(page.encode()) > MAX_PAGE_BYTES:
            match = re.search(r"````text\n(.*?)\n````", block, re.S)
            body = match[1] + "\n"
            # Rust's collection marks each dependency with a package heading.
            # Keep every component intact and preserve concatenation byte-for-byte.
            units = re.split(r"(?m)(?=^📦 )", body)
            chunks = []
            current = ""
            for unit in units:
                if len(unit.encode()) > MAX_PAGE_BYTES - 2000:
                    raise ValueError(f"Notice {digest} needs a component boundary for splitting")
                if current and len((current + unit).encode()) > MAX_PAGE_BYTES - 2000:
                    chunks.append(current)
                    current = ""
                current += unit
            if current:
                chunks.append(current)
            page = "<!-- notice -->" + block[:match.start()]
            page += "Full notice, in original order:\n\n"
            for number, chunk in enumerate(chunks, 1):
                part = f"{digest}-part-{number}.md"
                page += f"- [Part {number}]({part})\n"
                files[NOTICE_DIR / part] = (
                    f"# Notice {digest} — part {number}\n\n"
                    f"[Sources and all parts]({digest}.md)\n\n````text\n{chunk}````\n")
        files[path] = page
        heading = heading.replace(f"(#notice-{digest})", f"({path.as_posix()})")
    heading += ("Full texts are stored in the linked notice files, with identical texts shared by reference.\n"
                "For a standalone distribution copy, run "
                "`python3 scripts/generate-licenses.py --sync-packages`; "
                "the complete document is written to `target/licenses/THIRD_PARTY_LICENSES.md`.\n")
    files[Path(DOCUMENT.name)] = heading
    for path, content in files.items():
        if len(content.encode()) > MAX_PAGE_BYTES:
            raise ValueError(f"License page exceeds {MAX_PAGE_BYTES} bytes: {path}")
    return files


def sync_inventory(files, check=False):
    existing = set((ROOT / NOTICE_DIR).glob("*.md"))
    expected = {ROOT / path for path in files if path.parent == NOTICE_DIR}
    stale = existing - expected
    changed = [path for path, content in files.items()
               if not (ROOT / path).exists() or (ROOT / path).read_text() != content]
    if check:
        if stale or changed:
            raise SystemExit("License inventory is stale; run scripts/generate-licenses.py --fetch")
        print("License inventory is current.")
        return
    for path in stale:
        path.unlink()
    for path, content in files.items():
        destination = ROOT / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(content)
    print(f"Wrote license index and {len(files) - 1} notice pages.")


def sync_packages(text):
    """Stage the standalone inventory as ordinary files included by Cargo."""
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], cwd=ROOT))
    count = 0
    for package in metadata["packages"]:
        if package["publish"] == []:
            continue
        destination = Path(package["manifest_path"]).parent / DOCUMENT.name
        if not destination.exists() or destination.read_text() != text:
            destination.write_text(text)
        count += 1
    print(f"Staged license inventory in {count} packages.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fetch", action="store_true", help="Fetch missing original licenses at recorded revisions")
    parser.add_argument("--check", action="store_true", help="Fail if the checked-in document is stale")
    parser.add_argument("--sync-packages", action="store_true",
                        help="Stage package copies of the checked-in inventory, failing if it is stale")
    args = parser.parse_args()
    text = generate(args.fetch)
    sync_inventory(split_document(text), check=args.check or args.sync_packages)
    if not args.check or args.sync_packages:
        BUNDLE.parent.mkdir(parents=True, exist_ok=True)
        BUNDLE.write_text(text)
        sync_packages(text)


if __name__ == "__main__":
    main()
