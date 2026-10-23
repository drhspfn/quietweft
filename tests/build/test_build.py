import tempfile
import unittest
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'scripts'))
from build import WINDOWS_CRT_FILES, windows_crt_directory

class WindowsRedistributables(unittest.TestCase):
    def test_accepts_existing_and_newer_toolset_directory_names(self):
        for toolset in ('Microsoft.VC143.CRT', 'Microsoft.VC145.CRT'):
            with self.subTest(toolset=toolset), tempfile.TemporaryDirectory() as directory:
                crt = Path(directory) / 'x64' / toolset
                crt.mkdir(parents=True)
                for name in WINDOWS_CRT_FILES:
                    (crt / name).write_bytes(b'test fixture')
                self.assertEqual(windows_crt_directory(directory), crt)

    def test_rejects_incomplete_or_wrong_architecture(self):
        with tempfile.TemporaryDirectory() as directory:
            wrong_arch = Path(directory) / 'arm64' / 'Microsoft.VC145.CRT'
            wrong_arch.mkdir(parents=True)
            for name in WINDOWS_CRT_FILES:
                (wrong_arch / name).write_bytes(b'test fixture')
            partial = Path(directory) / 'x64' / 'Microsoft.VC145.CRT'
            partial.mkdir(parents=True)
            (partial / WINDOWS_CRT_FILES[0]).write_bytes(b'test fixture')
            with self.assertRaisesRegex(RuntimeError, 'Complete x64'):
                windows_crt_directory(directory)

if __name__ == '__main__':
    unittest.main()
