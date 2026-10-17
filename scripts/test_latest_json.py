"""Tests of latest_json.py: python3 -m unittest scripts/test_latest_json.py"""
import json
import subprocess
import sys
import tempfile
import unittest
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import latest_json  # noqa: E402

NOW = datetime(2026, 10, 17, 12, 30, 5, tzinfo=timezone.utc)


def asset(directory: Path, name: str, signature: str) -> Path:
    file = directory / name
    file.write_bytes(b"binary")
    Path(f"{file}.sig").write_text(signature + "\n", encoding="utf-8")
    return file


class ManifestTest(unittest.TestCase):
    def test_it_lists_both_platforms_with_the_release_urls_and_signatures(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            linux = asset(folder, "AnkiMining_0.1.0_amd64.AppImage", "LINUX-SIG")
            windows = asset(folder, "AnkiMining_0.1.0_x64-setup.exe", "WINDOWS-SIG")

            manifest = latest_json.build_manifest("0.1.0", "owner/repo", "  First version\n", linux, windows, NOW)

        self.assertEqual(
            manifest,
            {
                "version": "0.1.0",
                "notes": "First version",
                "pub_date": "2026-10-17T12:30:05Z",
                "platforms": {
                    "linux-x86_64": {
                        "signature": "LINUX-SIG",
                        "url": "https://github.com/owner/repo/releases/download/v0.1.0/AnkiMining_0.1.0_amd64.AppImage",
                    },
                    "windows-x86_64": {
                        "signature": "WINDOWS-SIG",
                        "url": "https://github.com/owner/repo/releases/download/v0.1.0/AnkiMining_0.1.0_x64-setup.exe",
                    },
                },
            },
        )

    def test_a_missing_signature_is_an_error(self):
        with tempfile.TemporaryDirectory() as directory:
            file = Path(directory) / "app.AppImage"
            file.write_bytes(b"x")
            with self.assertRaises(FileNotFoundError):
                latest_json.platform_entry("owner/repo", "v1", file)

    def test_an_empty_signature_is_an_error(self):
        with tempfile.TemporaryDirectory() as directory:
            file = asset(Path(directory), "app.AppImage", "")
            with self.assertRaises(ValueError):
                latest_json.platform_entry("owner/repo", "v1", file)

    def test_the_command_prints_json_and_refuses_wrong_arguments(self):
        script = Path(__file__).parent / "latest_json.py"
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            linux = asset(folder, "a.AppImage", "L")
            windows = asset(folder, "b-setup.exe", "W")
            notes = folder / "notes.md"
            notes.write_text("Notes", encoding="utf-8")

            done = subprocess.run(
                [sys.executable, str(script), "1.2.3", "o/r", str(notes), str(linux), str(windows)],
                capture_output=True,
                text=True,
                check=True,
            )
            wrong = subprocess.run([sys.executable, str(script)], capture_output=True, text=True)

        printed = json.loads(done.stdout)
        self.assertEqual(printed["version"], "1.2.3")
        self.assertEqual(sorted(printed["platforms"]), ["linux-x86_64", "windows-x86_64"])
        self.assertEqual(wrong.returncode, 2)


if __name__ == "__main__":
    unittest.main()
