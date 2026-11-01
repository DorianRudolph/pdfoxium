#!/usr/bin/env python3
"""Regressions for API-scope drift and deterministic table generation."""

from pathlib import Path
import os
import shutil
import tempfile
import sys
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True

import generate_api


class GenerateApiTests(unittest.TestCase):
    def test_complete_selected_scope_and_public_headers(self):
        inventory = generate_api.INVENTORY
        rendered = generate_api.render()
        functions = inventory.public_functions()
        selected = [name for name in functions if name not in inventory.EXCLUDED]
        self.assertEqual(len(functions), 478)
        self.assertEqual(len(selected), 470)
        self.assertEqual(
            rendered["api_functions.inc"].splitlines()[1:],
            [f"PDFIUM_API({name})" for name in selected],
        )
        self.assertEqual(
            rendered["api_headers.inc"].splitlines()[1:],
            [f'#include "public/{path.name}"'
             for path in sorted(inventory.PUBLIC.glob("*.h"))],
        )
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            generate_api.generate(output)
            first = {path.name: path.read_bytes() for path in output.iterdir()}
            for path in output.iterdir():
                os.utime(path, ns=(1_000_000_000, 1_000_000_000))
            generate_api.generate(output)
            self.assertEqual(first, {path.name: path.read_bytes()
                                     for path in output.iterdir()})
            self.assertTrue(all(path.stat().st_mtime_ns == 1_000_000_000
                                for path in output.iterdir()))

    def test_new_declaration_requires_inventory_review(self):
        inventory = generate_api.INVENTORY
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            public = root / "public"
            shutil.copytree(inventory.PUBLIC, public)
            header = public / "fpdfview.h"
            with header.open("a") as stream:
                stream.write("\nFPDF_EXPORT void FPDF_CALLCONV FPDF_NewUnreviewedApi();\n")
            output = root / "generated"
            with patch.object(inventory, "PUBLIC", public):
                with self.assertRaisesRegex(ValueError, "Pinned public declarations changed"):
                    generate_api.generate(output)
            self.assertFalse(output.exists())

    def test_changed_exclusion_requires_review(self):
        inventory = generate_api.INVENTORY
        exclusions = dict(inventory.EXCLUDED)
        exclusions["FPDF_RenderPage"] = "fpdf_edit.h"
        with patch.object(inventory, "EXCLUDED", exclusions):
            with self.assertRaisesRegex(ValueError, "C API exclusions differ"):
                generate_api.render()

    def test_exclusion_reasons_and_duplicates_are_checked(self):
        inventory = generate_api.INVENTORY
        original = inventory.EXCLUSIONS.read_text()
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "exclusions.tsv"
            lines = original.splitlines()
            path.write_text("\n".join(lines + [lines[1]]) + "\n")
            with patch.object(inventory, "EXCLUSIONS", path):
                with self.assertRaisesRegex(ValueError, "Duplicate C API exclusion"):
                    generate_api.render()
            columns = lines[1].split("\t")
            columns[2] = ""
            lines[1] = "\t".join(columns)
            path.write_text("\n".join(lines) + "\n")
            with patch.object(inventory, "EXCLUSIONS", path):
                with self.assertRaisesRegex(ValueError, "Invalid exclusion"):
                    generate_api.render()

    def test_upstream_unit_exclusions_reject_unknown_duplicate_and_unexplained(self):
        import importlib.util
        spec = importlib.util.spec_from_file_location(
            "unit_audit", generate_api.ROOT / "scripts/audit-unit-tests.py")
        audit = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(audit)
        native = audit.native_cases()
        rows = audit.read_manifest()
        audit.validate(rows, native)
        with self.assertRaisesRegex(ValueError, "Duplicate upstream exclusion"):
            audit.validate(rows + [rows[0]], native)
        with self.assertRaisesRegex(ValueError, "Unknown upstream exclusion"):
            audit.validate([dict(rows[0], case="Absent.Case")], native)
        with self.assertRaisesRegex(ValueError, "specific reason"):
            audit.validate([dict(rows[0], reason="")], native)


if __name__ == "__main__":
    unittest.main()
