#!/usr/bin/env python3
"""Small coverage-report regressions; no instrumented build required."""
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("capi_coverage", Path(__file__).with_name("capi-coverage.py"))
COVERAGE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(COVERAGE)


class CoverageReportTests(unittest.TestCase):
    def test_real_coverage_sections_are_required_in_elf_and_coff(self):
        self.assertTrue(COVERAGE.has_coverage_sections("Name: __llvm_covmap\nName: __llvm_covfun"))
        self.assertTrue(COVERAGE.has_coverage_sections("Name: .lcovmap$M\nName: .lcovfun$M"))
        self.assertFalse(COVERAGE.has_coverage_sections("Name: .text\nName: .debug_info"))
        self.assertFalse(COVERAGE.has_coverage_sections("Name: __llvm_covmap"))

    def test_ranges_keep_uninstrumented_gaps_out_of_missed_ranges(self):
        self.assertEqual(COVERAGE.ranges({20, 2, 4, 3, 21, 8}), [[2, 4], [8, 8], [20, 21]])
        self.assertEqual(COVERAGE.ranges(set()), [])

    def test_actual_api_records_distinguish_zero_missing_and_name_mentions(self):
        result = COVERAGE.classify_api_functions([
            {"name": "FPDF_LoadDocument", "count": 0},
            {"name": "FORM_OnFocus", "count": 3},
            {"name": "FORM_OnFocus", "count": 2},
            {"name": "FSDK_SetUnSpObjProcessHandler", "count": 1},
            {"name": "_RN_helper_mentions_FPDF_CreateNewDocument", "count": 17},
        ], {"FPDF_LoadDocument", "FORM_OnFocus", "FSDK_SetUnSpObjProcessHandler", "FPDF_CreateNewDocument"})
        self.assertEqual(result["selected_count"], 4)
        self.assertEqual(result["executed_count"], 2)
        self.assertEqual(result["counts"]["FORM_OnFocus"], 5)
        self.assertEqual(result["non_executed"], ["FPDF_LoadDocument"])
        self.assertEqual(result["missing_mapping"], ["FPDF_CreateNewDocument"])


class CoverageCollectionTests(unittest.TestCase):
    def test_cli_defaults_and_raster_opt_in(self):
        for extra, dpi, raster_diff in (([], 72, False),
                                        (["--comparison-dpi", "288", "--raster-diff"], 288, True)):
            with self.subTest(extra=extra), mock.patch.object(sys, "argv", [
                    "capi-coverage.py", "collect", "--run-dir", "/unused", *extra]), \
                    mock.patch.object(COVERAGE, "collect") as collect:
                self.assertEqual(COVERAGE.main(), 0)
                args = collect.call_args.args[0]
                self.assertEqual(args.comparison_dpi, dpi)
                self.assertEqual(args.raster_diff, raster_diff)

    def test_invalid_cli_dpi_is_rejected_before_collection(self):
        for value in ("17", "1153", "0", "72.5", "nan"):
            with self.subTest(value=value), mock.patch.object(sys, "argv", [
                    "capi-coverage.py", "collect", "--run-dir", "/unused",
                    "--comparison-dpi", value]), mock.patch.object(COVERAGE, "collect") as collect, \
                    mock.patch("sys.stderr", new_callable=io.StringIO):
                with self.assertRaises(SystemExit) as error:
                    COVERAGE.main()
                self.assertEqual(error.exception.code, 2)
                collect.assert_not_called()
        self.assertEqual(COVERAGE.comparison_dpi("18"), 18)
        self.assertEqual(COVERAGE.comparison_dpi("1152"), 1152)

    def test_collection_records_settings_for_suite_and_lifecycles(self):
        for dpi, raster_diff in ((72, False), (288, True)):
            with self.subTest(dpi=dpi, raster_diff=raster_diff), tempfile.TemporaryDirectory() as directory:
                run = Path(directory).resolve()
                (run / "profiles").mkdir()
                data = {"binary": "instrumented.dylib", "binary_sha256": "exact-build-hash",
                        "source_sha256": {"engine.rs": "exact-source-hash"}, "runs": []}
                args = SimpleNamespace(run_dir=run, runner=run / "runner",
                                       native_library=run / "native.dylib", gtest_filter="Forms.*",
                                       comparison_dpi=dpi, raster_diff=raster_diff)
                inherited = {"PDFOXIUM_CAPI_FORM_SCALE_DIR": "/expensive/diagnostics",
                             "PDFOXIUM_CAPI_FORM_SCALES": "1,2,4,8",
                             "LLVM_PROFILE_FILE": "/unrelated.profraw",
                             "PDFOXIUM_CAPI_FONT_PATH": "/custom/fixture-fonts"}

                def execute(argv, **kwargs):
                    # Settings must already be durable when collection starts.
                    self.assertEqual(json.loads((run / "manifest.json").read_text())["collection"]
                                     ["comparison_dpi"], dpi)
                    return SimpleNamespace(returncode=0)

                with mock.patch.object(COVERAGE, "verify", return_value=data) as verify, \
                        mock.patch.object(COVERAGE.subprocess, "run", side_effect=execute) as execute_mock, \
                        mock.patch.dict(COVERAGE.os.environ, inherited), \
                        mock.patch("sys.stdout", new_callable=io.StringIO):
                    COVERAGE.collect(args)
                self.assertEqual(verify.call_count, 2)
                self.assertEqual(execute_mock.call_count, 12)
                recorded = json.loads((run / "manifest.json").read_text())
                self.assertEqual(recorded["binary_sha256"], "exact-build-hash")
                self.assertEqual(recorded["source_sha256"], {"engine.rs": "exact-source-hash"})
                self.assertEqual(recorded["collection"], {
                    "comparison_dpi": dpi, "raster_diff": raster_diff,
                    "form_scale_environment": {"PDFOXIUM_CAPI_FORM_SCALE_DIR": None,
                                               "PDFOXIUM_CAPI_FORM_SCALES": None}})
                labels = {"suite", "system-fonts", *("lifecycle-" + config for config in
                           ("default", "1", "2", "3", "4", "5", "6", "7")),
                          "lifecycle-brotli", "lifecycle-isolation"}
                self.assertEqual({item["label"] for item in recorded["runs"]}, labels)
                for call, item in zip(execute_mock.call_args_list, recorded["runs"]):
                    argv, environment = call.args[0], call.kwargs["env"]
                    self.assertEqual(argv[argv.index("--comparison-dpi") + 1], str(dpi))
                    self.assertEqual("--skip-raster-diff" in argv, not raster_diff)
                    self.assertEqual(argv[argv.index("--rust-library") + 1], str(run / data["binary"]))
                    self.assertEqual(item["host_fonts"], item["label"] == "system-fonts")
                    self.assertEqual("--host-fonts" in argv, item["host_fonts"])
                    if item["host_fonts"]:
                        self.assertNotIn("PDFOXIUM_CAPI_FONT_PATH", environment)
                        self.assertIn("--gtest_filter=SystemFonts.*", argv)
                    else:
                        self.assertEqual(environment["PDFOXIUM_CAPI_FONT_PATH"], "/custom/fixture-fonts")
                    self.assertEqual(item["argv"], argv)
                    self.assertEqual(item["comparison_settings"], recorded["collection"])
                    self.assertEqual(item["llvm_profile_file"], str(
                        run / "profiles" / (item["label"] + "-%p-%m.profraw")))
                    self.assertEqual(environment["LLVM_PROFILE_FILE"], item["llvm_profile_file"])
                    self.assertNotIn("PDFOXIUM_CAPI_FORM_SCALE_DIR", environment)
                    self.assertNotIn("PDFOXIUM_CAPI_FORM_SCALES", environment)


if __name__ == "__main__":
    unittest.main()
