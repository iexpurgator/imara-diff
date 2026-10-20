"""Exercise an actual release binary, optionally through an emulator."""
import argparse
import subprocess
import tempfile
from pathlib import Path


def smoke(binary, version, prefix=()):
    binary = Path(binary).resolve()
    with tempfile.TemporaryDirectory(prefix='imara-diff-smoke-') as directory:
        root = Path(directory)
        before, after = b'old\nkeep\n', b'new\nkeep\n'
        (root / 'before.txt').write_bytes(before)
        (root / 'after.txt').write_bytes(after)

        def run(args, code=0, stdin=None):
            result = subprocess.run([*prefix, str(binary), *args], cwd=root, input=stdin,
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
            if result.returncode != code:
                raise ValueError(f'CLI smoke {args}: expected exit {code}, got {result.returncode}: {result.stderr!r}')
            if code != 2 and result.stderr:
                raise ValueError(f'CLI smoke {args}: unexpected stderr: {result.stderr!r}')
            return result.stdout

        if run(['--version']).decode().strip() != f'imara-diff {version}':
            raise ValueError('artifact reports the wrong build version')
        patch = run(['before.txt', 'after.txt'])
        if patch != b'--- "a/before.txt"\n+++ "b/after.txt"\n@@ -1,2 +1,2 @@\n-old\n+new\n keep\n':
            raise ValueError(f'ordinary unified diff is incorrect: {patch!r}')
        if run(['--exit-code', 'before.txt', 'after.txt'], code=1) != patch:
            raise ValueError('--exit-code changed the diff output')
        if run(['--quiet', 'before.txt', 'after.txt'], code=1):
            raise ValueError('--quiet produced output')
        if run(['before.txt', 'before.txt']):
            raise ValueError('identical input produced output')
        if run(['--quiet', 'before.txt', 'before.txt']):
            raise ValueError('identical quiet comparison produced output')
        if not run(['-', 'after.txt'], stdin=before).endswith(b'-old\n+new\n keep\n'):
            raise ValueError('stdin comparison is incorrect')
        if run(['missing.txt', 'after.txt'], code=2):
            raise ValueError('read error leaked data to stdout')
        if (root / 'before.txt').read_bytes() != before or (root / 'after.txt').read_bytes() != after:
            raise ValueError('CLI modified an input file')
    print(f'PASS artifact smoke: {binary.name} {version}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    parser.add_argument('--version', required=True)
    parser.add_argument('--runner', nargs=argparse.REMAINDER, default=[])
    args = parser.parse_args()
    smoke(args.binary, args.version, args.runner)
