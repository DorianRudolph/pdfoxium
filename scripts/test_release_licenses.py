#!/usr/bin/env python3
"""Focused regression tests for the license inventory's completeness rules."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import re
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).with_name("generate-licenses.py")
spec = importlib.util.spec_from_file_location("licenses", SCRIPT)
licenses = importlib.util.module_from_spec(spec)
spec.loader.exec_module(licenses)
warning_spec = importlib.util.spec_from_file_location(
    "warnings_gate", SCRIPT.with_name("check-compiler-warnings.py"))
warnings_gate = importlib.util.module_from_spec(warning_spec)
warning_spec.loader.exec_module(warnings_gate)
release_spec = importlib.util.spec_from_file_location(
    "release_checks", SCRIPT.with_name("check-release.py"))
release_checks = importlib.util.module_from_spec(release_spec)
release_spec.loader.exec_module(release_checks)


class LicenseInventoryTests(unittest.TestCase):
    def test_identical_text_keeps_every_source(self):
        inventory = licenses.Inventory()
        first = inventory.add("Copyright Example\r\nPermission.\r\n", "https://a.example/LICENSE")
        second = inventory.add("Copyright Example\nPermission.\n", "https://b.example/LICENSE")
        self.assertEqual(first, second)
        self.assertEqual(len(inventory.texts[first]["sources"]), 2)

    def notice_document(self, text):
        digest = hashlib.sha256(text.encode()).hexdigest()[:16]
        return (f"# Inventory\n\n[text](#notice-{digest})\n\n"
                f'<!-- notice -->\n<a id="notice-{digest}"></a>\n'
                f"### Notice {digest}\n\n"
                "<!-- source: https://example.org/commit/LICENSE -->\n"
                "Source: <https://example.org/commit/LICENSE>\n\n"
                f"````text\n{text}````\n")

    def test_split_round_trip_preserves_complete_text_sources_and_order(self):
        # Include non-ASCII, indentation and blank lines across component boundaries.
        text = "Collection preface\n\n" + "".join(
            f"📦 package-{number}\n\nCopyright © Author {number}\n"
            + "  Full license grant.\n" * 100 + "\n"
            for number in range(4))
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            with patch.object(licenses, "ROOT", root), \
                 patch.object(licenses, "DOCUMENT", root / "THIRD_PARTY_LICENSES.md"), \
                 patch.object(licenses, "MAX_PAGE_BYTES", 5000):
                files = licenses.split_document(self.notice_document(text))
                licenses.sync_inventory(files)
                inventory = licenses.Inventory()
                self.assertEqual(inventory.previous,
                                 {"https://example.org/commit/LICENSE": text})
                self.assertTrue(list((root / licenses.NOTICE_DIR).glob("*-part-*.md")))
                for path, content in files.items():
                    self.assertLessEqual(len(content.encode()), 5000)
                    for target in re.findall(r"\]\(([^)]+)\)", content):
                        self.assertTrue((root / path.parent / target).is_file(), target)
                licenses.sync_inventory(files, check=True)
                # Missing parts fail instead of silently shortening a notice.
                part = next((root / licenses.NOTICE_DIR).glob("*-part-*.md"))
                original = part.read_text()
                part.write_text(original.replace("Full license grant.", "Changed grant.", 1))
                with self.assertRaisesRegex(ValueError, "hash mismatch"):
                    licenses.Inventory()
                part.unlink()
                with self.assertRaises(FileNotFoundError):
                    licenses.Inventory()

    def test_split_rejects_an_oversized_indivisible_component(self):
        with patch.object(licenses, "MAX_PAGE_BYTES", 5000):
            with self.assertRaisesRegex(ValueError, "component boundary"):
                licenses.split_document(self.notice_document("Copyright Author\n" * 500))

    def test_check_detects_changed_missing_and_obsolete_notice_pages(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            with patch.object(licenses, "ROOT", root):
                files = licenses.split_document(self.notice_document("Copyright Author\nGrant.\n"))
                licenses.sync_inventory(files)
                notice = next(path for path in files if path.parent == licenses.NOTICE_DIR)
                (root / notice).write_text("Incomplete grant.\n")
                with self.assertRaises(SystemExit):
                    licenses.sync_inventory(files, check=True)
                (root / notice).unlink()
                with self.assertRaises(SystemExit):
                    licenses.sync_inventory(files, check=True)
                licenses.sync_inventory(files)
                obsolete = root / licenses.NOTICE_DIR / "obsolete.md"
                obsolete.write_text("Stale notice.\n")
                with self.assertRaises(SystemExit):
                    licenses.sync_inventory(files, check=True)
                licenses.sync_inventory(files)
                self.assertFalse(obsolete.exists())
                licenses.sync_inventory(files, check=True)

    def test_full_fonts_are_discovered(self):
        for name in ("fonts/OFL.txt", "fonts/UFL.txt", "fonts/Hack-Regular.txt",
                     "fonts/emoji-icon-font-mit-license.txt", "LICENSE-OPENJPEG-DWT"):
            self.assertTrue(licenses.license_path(Path(name)), name)
        self.assertFalse(licenses.license_path(Path("fonts/Ubuntu-Light.ttf")))

    def test_missing_new_license_fails_instead_of_substituting_spdx(self):
        with tempfile.TemporaryDirectory() as folder:
            package = {"name": "new-dependency", "version": "1.0.0", "license": "MIT",
                       "manifest_path": str(Path(folder) / "Cargo.toml"),
                       "repository": "https://github.com/example/new-dependency", "source": "registry"}
            with self.assertRaisesRegex(ValueError, "Missing original license source"):
                licenses.Inventory().package(package)

    def test_previous_document_reuses_exact_immutable_text(self):
        with tempfile.TemporaryDirectory() as folder:
            document = Path(folder) / "licenses.md"
            document.write_text("<!-- notice -->\n<!-- source: https://example.org/commit/LICENSE -->\n"
                                "````text\nCopyright Example\nFull grant.\n````\n")
            with patch.object(licenses, "DOCUMENT", document):
                inventory = licenses.Inventory()
                digest = inventory.remote("https://example.org/commit/LICENSE")
            self.assertEqual(inventory.texts[digest]["text"], "Copyright Example\nFull grant.\n")

    def test_runtime_html_keeps_notice_and_project_links(self):
        parser = licenses.NoticeHTML()
        parser.feed('<p>Copyright Authors</p><pre>Full permission\n  notice.</pre>'
                    '<a href="https://example.org/project">Dependency</a>')
        text = parser.text()
        self.assertIn("Copyright Authors", text)
        self.assertIn("Full permission\n  notice.", text)
        self.assertIn("https://example.org/project", text)

    def test_html_error_pages_are_rejected(self):
        with self.assertRaisesRegex(ValueError, "Not a license text"):
            licenses.Inventory().add("<html>Not found</html>", "https://example.org/LICENSE")

    def test_package_staging_preserves_notices_and_skips_private_tools(self):
        with tempfile.TemporaryDirectory() as folder:
            package = Path(folder) / "public"
            package.mkdir()
            metadata = {"packages": [
                {"manifest_path": str(package / "Cargo.toml"), "publish": None},
                {"manifest_path": str(Path(folder) / "private/Cargo.toml"), "publish": []},
            ]}
            text = "Copyright Example — original notice\nFull grant.\n"
            with patch.object(licenses.subprocess, "check_output", return_value=json.dumps(metadata).encode()):
                licenses.sync_packages(text)
            self.assertEqual((package / licenses.DOCUMENT.name).read_bytes(), text.encode())
            self.assertFalse((Path(folder) / "private" / licenses.DOCUMENT.name).exists())


class ManifestWarningTests(unittest.TestCase):
    def status(self, stderr, returncode=0):
        result = warnings_gate.subprocess.CompletedProcess([], returncode, "{}", stderr)
        with patch.object(warnings_gate.subprocess, "run", return_value=result), \
             patch.object(warnings_gate.sys, "stderr", io.StringIO()):
            return warnings_gate.check_manifests()

    def test_manifest_warning_fails_even_when_cargo_succeeds(self):
        self.assertEqual(self.status("warning: only one of `license` or `license-file` is necessary\n"), 1)

    def test_clean_metadata_passes(self):
        self.assertEqual(self.status(""), 0)

    def test_cargo_failure_is_preserved(self):
        self.assertEqual(self.status("error: invalid manifest\n", 101), 101)


class PackageSourceTests(unittest.TestCase):
    def test_generated_notices_are_allowed_but_source_changes_are_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)

            def git(*args):
                release_checks.subprocess.run(
                    ["git", "-c", "user.name=Release Test", "-c", "user.email=test@example.org",
                     "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false", *args],
                    cwd=root, check=True, capture_output=True)

            git("init", "--quiet")
            (root / ".gitignore").write_text("THIRD_PARTY_LICENSES.md\n")
            source = root / "source.rs"
            source.write_text("original\n")
            git("add", ".")
            git("commit", "--quiet", "-m", "Source fixture")
            (root / "THIRD_PARTY_LICENSES.md").write_text("Generated notice\n")
            with patch.object(release_checks, "ROOT", root):
                release_checks.require_clean_package_sources()
                source.write_text("modified\n")
                with self.assertRaisesRegex(SystemExit, "source.rs"):
                    release_checks.require_clean_package_sources()
                git("add", "source.rs")
                with self.assertRaisesRegex(SystemExit, "source.rs"):
                    release_checks.require_clean_package_sources()
                git("commit", "--quiet", "-m", "Updated fixture")
                (root / "untracked.rs").write_text("new source\n")
                with self.assertRaisesRegex(SystemExit, "untracked.rs"):
                    release_checks.require_clean_package_sources()


if __name__ == "__main__":
    unittest.main()
