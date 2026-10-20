import hashlib
import json
import struct
import tarfile
import tempfile
import unittest
import zipfile
from datetime import datetime, timedelta, timezone
from pathlib import Path

import release

SHA = 'a' * 40
NOW = datetime(2026, 10, 20, 23, 59, tzinfo=timezone.utc)


class FakeGitHub:
    def __init__(self, refs=(), collide=False, forbidden=False):
        self.refs = {ref: {'ref': ref, 'object': {'sha': SHA, 'type': 'commit'}} for ref in refs}
        self.collide = collide
        self.forbidden = forbidden

    def __call__(self, method, path, data=None):
        if self.forbidden:
            raise release.APIError(403, 'permission denied')
        if '/matching-refs/' in path:
            prefix = 'refs/' + path.split('/matching-refs/')[1]
            return [value for key, value in self.refs.items() if key.startswith(prefix)]
        if method == 'POST' and path.endswith('/git/refs'):
            ref = data['ref']
            if self.collide:
                self.collide = False
                self.refs[ref] = {'ref': ref, 'object': {'sha': SHA, 'type': 'commit'}}
                raise release.APIError(422, 'reference already exists')
            if ref in self.refs:
                raise release.APIError(422, 'reference already exists')
            self.refs[ref] = {'ref': ref, 'object': {'sha': data['sha'], 'type': 'commit'}}
            return self.refs[ref]
        if '/git/ref/' in path:
            ref = 'refs/' + path.split('/git/ref/')[1]
            if ref not in self.refs:
                raise release.APIError(404, 'not found')
            return self.refs[ref]
        raise AssertionError((method, path, data))


class VersionTests(unittest.TestCase):
    def test_first_build_starts_at_zero_without_touching_old_tags(self):
        old_tags = ['refs/tags/v0.2.0', 'refs/tags/imara-diff-2026.293.8']
        api = FakeGitHub(old_tags)
        result = release.reserve_version(api, 'acme/imara-diff', SHA, NOW)
        self.assertEqual(result, ('26.293.0', 'imara-diff-26.293.0'))
        self.assertTrue(all(tag in api.refs for tag in old_tags))
        self.assertEqual(api.refs['refs/tags/imara-diff-26.293.0']['object']['sha'], SHA)

    def test_existing_failed_reservations_and_gaps_are_not_reused(self):
        api = FakeGitHub([
            'refs/tags/imara-diff-26.293.0', 'refs/tags/imara-diff-26.293.4',
            'refs/tags/imara-diff-26.292.100', 'refs/tags/imara-diff-26.293.bad',
        ])
        self.assertEqual(release.reserve_version(api, 'acme/imara-diff', SHA, NOW)[0], '26.293.5')

    def test_concurrent_reservation_retries_instead_of_overwriting(self):
        api = FakeGitHub(collide=True)
        self.assertEqual(release.reserve_version(api, 'acme/imara-diff', SHA, NOW)[0], '26.293.1')
        self.assertIn('refs/tags/imara-diff-26.293.0', api.refs)

    def test_collision_progress_does_not_depend_on_a_fresh_listing(self):
        class StaleAPI(FakeGitHub):
            def __call__(self, method, path, data=None):
                if '/matching-refs/' in path:
                    return []
                return super().__call__(method, path, data)
        self.assertEqual(release.reserve_version(StaleAPI(collide=True), 'acme/imara-diff', SHA, NOW)[0], '26.293.1')

    def test_permissions_error_is_not_treated_as_a_collision(self):
        with self.assertRaises(release.APIError):
            release.reserve_version(FakeGitHub(forbidden=True), 'acme/imara-diff', SHA, NOW)

    def test_utc_leap_year_two_digit_year_and_daily_reset(self):
        api = FakeGitHub(['refs/tags/imara-diff-26.293.8'])
        for date, expected in [
            (datetime(2024, 2, 29, tzinfo=timezone.utc), '24.60.0'),
            (datetime(2024, 12, 31, tzinfo=timezone.utc), '24.366.0'),
            (datetime(2027, 1, 1, tzinfo=timezone.utc), '27.1.0'),
            (datetime(2026, 10, 21, tzinfo=timezone.utc), '26.294.0'),
            (datetime(2000, 2, 29, tzinfo=timezone.utc), '00.60.0'),
        ]:
            actual = release.reserve_version(api, 'acme/imara-diff', SHA, date)[0]
            self.assertEqual(actual, expected)
            release.validate_version(actual)

    def test_timezone_is_converted_to_utc_and_repo_name_is_not_hardcoded(self):
        date = datetime(2026, 1, 1, 1, tzinfo=timezone(timedelta(hours=7)))
        self.assertEqual(release.reserve_version(FakeGitHub(), 'acme/agent-diff', SHA, date),
                         ('25.365.0', 'agent-diff-25.365.0'))

    def test_invalid_calendar_versions_are_rejected(self):
        for value in ['25.366.0', '26.0.0', '26.001.0', '26.1.-1', '2026.293.0', '6.293.0', '../outside']:
            with self.assertRaises(ValueError):
                release.validate_version(value)

    def test_invalid_context_is_rejected_before_api_mutation(self):
        def no_network(*args):
            raise AssertionError('invalid context reached API')
        for repo, sha in [('upstream/../repo', SHA), ('acme/imara-diff', 'not-a-sha')]:
            with self.assertRaises(ValueError):
                release.reserve_version(no_network, repo, sha, NOW)


class MatrixTests(unittest.TestCase):
    def test_core_is_a_unique_subset_and_extended_excludes_bare_metal(self):
        core = release.matrix('core')['include']
        extended = release.matrix('extended')['include']
        self.assertEqual(len(core), 13)
        self.assertEqual(len(extended), 46)
        self.assertEqual(len({row['target'] for row in extended}), 46)
        self.assertLessEqual({row['target'] for row in core}, {row['target'] for row in extended})
        self.assertFalse(any('none-eabi' in row['target'] for row in extended))
        self.assertEqual(next(row for row in core if row['target'] == 'aarch64-pc-windows-msvc')['runner'], 'windows-11-arm')
        self.assertEqual(next(row for row in core if row['target'] == 'i686-pc-windows-msvc')['tool'], 'cargo')
        self.assertEqual(next(row for row in extended if row['target'] == 's390x-unknown-linux-gnu')['tool'], 'cross')
        with self.assertRaises(ValueError):
            release.matrix('unknown')
        with self.assertRaises(ValueError):
            release.build('unknown-target', 'imara-diff', '26.293.0', SHA)

    def test_build_commands_keep_target_and_gnu_floor_explicit(self):
        self.assertEqual(release.build_command('x86_64-unknown-linux-gnu', 'zig')[-1], 'x86_64-unknown-linux-gnu.2.17')
        self.assertEqual(release.build_command('armv7-unknown-linux-musleabihf', 'zig')[-1], 'armv7-unknown-linux-musleabihf')
        self.assertEqual(release.build_command('aarch64-pc-windows-msvc', 'cargo')[:2], ['cargo', 'build'])


class ArtifactTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.binary = self.root / 'imara-diff'
        self.binary.write_bytes(b'hello')
        self.dist = self.root / 'dist'

    def archive(self, target):
        return release.make_archive(self.binary, self.dist, 'imara-diff', '26.293.0', target, SHA, 'test-only', {})

    def assert_checksum(self, data):
        archive = self.dist / data['asset']
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        self.assertEqual(data['sha256'], digest)
        self.assertEqual((self.dist / (data['asset'] + '.sha256')).read_bytes(),
                         f'{digest}  {data["asset"]}\n'.encode())

    def test_archives_have_plain_binary_names_and_separate_checksums(self):
        win = self.archive('x86_64-pc-windows-msvc')
        self.assertEqual(win['asset'], 'imara-diff-26.293.0-x86_64-pc-windows-msvc.zip')
        with zipfile.ZipFile(self.dist / win['asset']) as archive:
            self.assertEqual(set(archive.namelist()), {'imara-diff.exe', 'LICENSE', 'README.md'})
            self.assertEqual(archive.read('imara-diff.exe'), b'hello')
        self.assert_checksum(win)
        with self.assertRaises(ValueError):
            self.archive('x86_64-pc-windows-msvc')
        unix = self.archive('x86_64-unknown-linux-musl')
        self.assertEqual(unix['asset'], 'imara-diff-26.293.0-x86_64-unknown-linux-musl.tar.gz')
        with tarfile.open(self.dist / unix['asset']) as archive:
            self.assertEqual(set(archive.getnames()), {'imara-diff', 'LICENSE', 'README.md'})
            self.assertEqual(archive.extractfile('imara-diff').read(), b'hello')
            self.assertEqual(archive.getmember('imara-diff').mode & 0o777, 0o755)
        self.assert_checksum(unix)

    def test_manifest_checks_actual_archives_and_refuses_missing_or_tampered_assets(self):
        target = 'x86_64-unknown-linux-musl'
        data = self.archive(target)
        release.write_manifest(self.dist, 'imara-diff', '26.293.0', SHA, [target])
        self.assertIn(data['asset'], (self.dist / 'SHA256SUMS').read_text())
        with self.assertRaises(ValueError):
            release.write_manifest(self.dist, 'imara-diff', '26.293.0', SHA, [target, 'i686-pc-windows-msvc'])
        (self.dist / data['asset']).write_bytes(b'tampered')
        with self.assertRaises(ValueError):
            release.write_manifest(self.dist, 'imara-diff', '26.293.0', SHA, [target])

    def test_manifest_requires_correct_sidecar_for_each_archive(self):
        target = 'x86_64-unknown-linux-musl'
        data = self.archive(target)
        sidecar = self.dist / (data['asset'] + '.sha256')
        self.assert_checksum(data)
        sidecar.unlink()
        with self.assertRaises(ValueError):
            release.write_manifest(self.dist, 'imara-diff', '26.293.0', SHA, [target])
        sidecar.write_text('0' * 64 + '  ' + data['asset'] + '\n', encoding='utf-8')
        with self.assertRaises(ValueError):
            release.write_manifest(self.dist, 'imara-diff', '26.293.0', SHA, [target])

    def test_manifest_rejects_wrong_source_and_path_traversal(self):
        target = 'x86_64-unknown-linux-musl'
        data = self.archive(target)
        with self.assertRaises(ValueError):
            release.write_manifest(self.dist, 'imara-diff', '26.293.0', 'b' * 40, [target])
        metadata = self.dist / (data['asset'] + '.metadata.json')
        data['asset'] = '../outside.tar.gz'
        metadata.write_text(json.dumps(data))
        with self.assertRaises(ValueError):
            release.write_manifest(self.dist, 'imara-diff', '26.293.0', SHA, [target])

    def test_architecture_mismatch_and_truncated_files_are_rejected(self):
        elf = bytearray(64)
        elf[:6] = b'\x7fELF\x02\x01'
        struct.pack_into('<H', elf, 18, 62)
        self.binary.write_bytes(elf)
        self.assertEqual(release.check_architecture(self.binary, 'x86_64-unknown-linux-musl'), 'ELF')
        with self.assertRaises(ValueError):
            release.check_architecture(self.binary, 'aarch64-unknown-linux-musl')
        elf[4] = 1
        self.binary.write_bytes(elf)
        with self.assertRaises(ValueError):
            release.check_architecture(self.binary, 'x86_64-unknown-linux-musl')
        self.binary.write_bytes(b'MZ')
        with self.assertRaises(ValueError):
            release.check_architecture(self.binary, 'x86_64-pc-windows-msvc')


if __name__ == '__main__':
    unittest.main()
