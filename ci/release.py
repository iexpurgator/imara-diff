"""Manual release helpers; Python standard library only. No API calls on import."""
import argparse
import gzip
import hashlib
import io
import json
import os
import re
import shutil
import struct
import subprocess
import sys
import tarfile
import urllib.error
import urllib.request
import zipfile
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


class APIError(RuntimeError):
    def __init__(self, status, message):
        super().__init__(f'GitHub API HTTP {status}: {message}')
        self.status = status


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None  # Never forward the authorization header to a redirect target.


def github_api(method, path, data=None):
    base = os.environ.get('GITHUB_API_URL', 'https://api.github.com').rstrip('/')
    if not base.startswith('https://'):
        raise ValueError('GitHub API must use HTTPS')
    request = urllib.request.Request(
        f'{base}/{path}', method=method,
        data=None if data is None else json.dumps(data).encode(),
        headers={'Authorization': f'Bearer {os.environ["GITHUB_TOKEN"]}',
                 'Accept': 'application/vnd.github+json', 'User-Agent': 'imara-diff-release',
                 'Content-Type': 'application/json', 'X-GitHub-Api-Version': '2022-11-28'},
    )
    try:
        with urllib.request.build_opener(NoRedirect).open(request, timeout=60) as response:
            return json.load(response)
    except urllib.error.HTTPError as error:
        raise APIError(error.code, error.read().decode(errors='replace')) from error


def validate_context(repository, sha):
    if not re.fullmatch(r'[A-Za-z0-9-]+/[A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]+)*', repository):
        raise ValueError('invalid repository owner/name')
    if not re.fullmatch(r'[0-9a-fA-F]{40}', sha):
        raise ValueError('source SHA must be a full 40-character commit ID')


def validate_version(version):
    if not re.fullmatch(r'[0-9]{2}\.[1-9][0-9]{0,2}\.(?:0|[1-9][0-9]*)', version):
        raise ValueError('version must be YY.DAYOFYEAR.BUILD; day/build must not be padded')
    year, day, _ = map(int, version.split('.'))
    if not 1 <= day <= datetime(2000 + year, 12, 31).timetuple().tm_yday:
        raise ValueError('invalid day of year')


def reserve_version(api, repository, sha, now):
    validate_context(repository, sha)
    repo = repository.split('/')[1]
    day = now.astimezone(timezone.utc)
    prefix = f'{repo}-{day.year % 100:02d}.{day.timetuple().tm_yday}.'
    pattern = re.compile(r'^refs/tags/' + re.escape(prefix) + r'(0|[1-9][0-9]*)$')
    endpoint = f'repos/{repository}/git'
    lower_bound = 0
    for _ in range(32):
        refs = api('GET', f'{endpoint}/matching-refs/tags/{prefix}')
        numbers = [int(match[1]) for ref in refs if (match := pattern.fullmatch(ref['ref']))]
        number = max(max(numbers, default=-1) + 1, lower_bound)
        tag = prefix + str(number)
        try:
            api('POST', f'{endpoint}/refs', {'ref': f'refs/tags/{tag}', 'sha': sha})
            return tag[len(repo) + 1:], tag
        except APIError as error:
            if error.status != 422:
                raise
            # A concurrent creator is the only validation failure worth retrying.
            try:
                api('GET', f'{endpoint}/ref/tags/{tag}')
            except APIError:
                raise error
            lower_bound = number + 1
    raise RuntimeError('too many concurrent tag reservations; try again')


def inventory():
    return json.loads((ROOT / 'ci/targets.json').read_text(encoding='utf-8'))


def matrix(profile):
    targets = inventory()
    if profile not in ('core', 'extended'):
        raise ValueError('profile must be core or extended')
    selected = targets['core'] + (targets['extra'] if profile == 'extended' else [])
    rows = []
    for target in selected:
        runner, tool = 'ubuntu-22.04', 'cross'
        if target.endswith('windows-msvc'):
            runner = 'windows-11-arm' if target.startswith('aarch64-') else 'windows-2022'
            tool = 'cargo'
        elif target.endswith('apple-darwin'):
            runner = 'macos-15' if target.startswith('aarch64-') else 'macos-15-intel'
            tool = 'cargo'
        elif target in targets['core']:
            runner = 'ubuntu-24.04-arm' if target.startswith('aarch64-') else 'ubuntu-22.04'
            tool = 'zig'
        rows.append({'target': target, 'runner': runner, 'tool': tool})
    return {'include': rows}


def build_command(target, tool):
    command = {'cargo': ['cargo', 'build'], 'zig': ['cargo', 'zigbuild'], 'cross': ['cross', 'build']}[tool]
    link_target = target + '.2.17' if tool == 'zig' and target.endswith(('linux-gnu', 'linux-gnueabihf')) else target
    return command + ['--locked', '--release', '--bin', 'imara-diff', '--target', link_target]


def capture(command, **kwargs):
    return subprocess.check_output(command, text=True, encoding='utf-8', **kwargs).strip()


def check_architecture(binary, target):
    data = binary.read_bytes()
    arch = target.split('-')[0]
    if target.endswith('windows-msvc'):
        if len(data) < 64 or data[:2] != b'MZ':
            raise ValueError('truncated or invalid PE executable')
        offset = struct.unpack_from('<I', data, 60)[0]
        if len(data) < offset + 6 or data[offset:offset + 4] != b'PE\0\0':
            raise ValueError('invalid PE signature')
        machine = struct.unpack_from('<H', data, offset + 4)[0]
        expected = {'x86_64': 0x8664, 'i686': 0x14c, 'aarch64': 0xaa64}[arch]
        kind = 'PE'
    elif target.endswith('apple-darwin'):
        if len(data) < 8 or data[:4] != b'\xcf\xfa\xed\xfe':
            raise ValueError('expected a thin 64-bit Mach-O executable')
        machine = struct.unpack_from('<I', data, 4)[0]
        expected = {'x86_64': 0x1000007, 'aarch64': 0x100000c}[arch]
        kind = 'Mach-O'
    else:
        if len(data) < 64 or data[:4] != b'\x7fELF' or data[5] not in (1, 2):
            raise ValueError('truncated or invalid ELF executable')
        bits = 1 if arch in ('i586', 'i686', 'powerpc') or arch.startswith(('arm', 'thumb')) else 2
        endian = 2 if arch in ('powerpc', 'powerpc64', 's390x', 'sparc64', 'sparcv9') else 1
        if data[4] != bits or data[5] != endian:
            raise ValueError(f'ELF class/endianness mismatch for {target}')
        machine = struct.unpack_from('<H' if data[5] == 1 else '>H', data, 18)[0]
        expected = {'x86_64': 62, 'i686': 3, 'i586': 3, 'aarch64': 183,
                    'loongarch64': 258, 'powerpc': 20, 'powerpc64': 21,
                    'powerpc64le': 21, 'riscv64gc': 243, 's390x': 22,
                    'sparc64': 43, 'sparcv9': 43}.get(arch, 40 if arch.startswith(('arm', 'thumb')) else None)
        kind = 'ELF'
    if machine != expected:
        raise ValueError(f'architecture mismatch for {target}: {machine}, expected {expected}')
    return kind


def audit(binary, target, tool):
    details = {'format': check_architecture(binary, target)}
    if target.endswith('windows-msvc'):
        dumpbin = shutil.which('dumpbin')
        if not dumpbin:
            vswhere = Path(os.environ.get('ProgramFiles(x86)', 'C:/Program Files (x86)')) / 'Microsoft Visual Studio/Installer/vswhere.exe'
            install = Path(capture([str(vswhere), '-latest', '-products', '*', '-property', 'installationPath']))
            candidates = sorted(install.glob('VC/Tools/MSVC/*/bin/Host*/*/dumpbin.exe'), reverse=True)
            if not candidates:
                raise ValueError('cannot find MSVC dumpbin for DLL audit')
            dumpbin = str(candidates[0])
        imports = capture([dumpbin, '/DEPENDENTS', str(binary)])
        dlls = re.findall(r'^\s+(\S+\.dll)\s*$', imports, re.M | re.I)
        allowed = {'kernel32.dll', 'ntdll.dll', 'api-ms-win-core-synch-l1-2-0.dll'}
        if not dlls or any(name.lower() not in allowed for name in dlls):
            raise ValueError(f'unexpected Windows DLL dependencies: {dlls}')
        details['dependencies'] = dlls
    elif target.endswith('apple-darwin'):
        output = capture(['otool', '-L', str(binary)])
        libraries = [line.strip().split(' (')[0] for line in output.splitlines()[1:]]
        if not libraries or any(not path.startswith(('/usr/lib/', '/System/Library/')) for path in libraries):
            raise ValueError(f'non-system macOS libraries: {libraries}')
        details['dependencies'] = libraries
    else:
        dynamic = capture(['readelf', '-d', str(binary)])
        libraries = re.findall(r'\(NEEDED\).*?\[(.*?)\]', dynamic)
        program = capture(['readelf', '-l', str(binary)])
        if 'musl' in target and (libraries or 'Requesting program interpreter' in program):
            raise ValueError('musl executable is not fully static')
        versions = capture(['readelf', '--version-info', str(binary)])
        glibc = sorted({tuple(map(int, item.split('.'))) for item in re.findall(r'GLIBC_([0-9]+\.[0-9]+(?:\.[0-9]+)?)', versions)})
        if tool == 'zig' and re.search(r'GLIBC_(?![0-9])[A-Z_]+', versions):
            raise ValueError('GNU artifact requires a private/newer glibc ABI')
        if tool == 'zig' and target.endswith(('linux-gnu', 'linux-gnueabihf')) and glibc and glibc[-1] > (2, 17):
            raise ValueError(f'GNU artifact exceeds declared glibc 2.17 floor: {glibc[-1]}')
        details.update(dependencies=libraries, glibc_max='.'.join(map(str, glibc[-1])) if glibc else None)
    return details


def make_archive(binary, output, repo, version, target, sha, validation, details):
    validate_context('owner/' + repo, sha)
    validate_version(version)
    if target not in [row['target'] for row in matrix('extended')['include']]:
        raise ValueError('unknown target')
    output.mkdir(parents=True, exist_ok=True)
    windows = target.endswith('windows-msvc')
    name = f'{repo}-{version}-{target}' + ('.zip' if windows else '.tar.gz')
    path = output / name
    metadata = output / (name + '.metadata.json')
    sidecar = output / (name + '.sha256')
    if path.exists() or metadata.exists() or sidecar.exists():
        raise ValueError(f'refusing to overwrite {path}')
    files = [('imara-diff.exe' if windows else 'imara-diff', binary, 0o755),
             ('LICENSE', ROOT / 'LICENSE', 0o644), ('README.md', ROOT / 'README.md', 0o644)]
    if windows:
        with zipfile.ZipFile(path, 'x', compression=zipfile.ZIP_DEFLATED) as archive:
            for entry, source, _ in files:
                archive.writestr(entry, source.read_bytes())
    else:
        with path.open('xb') as raw, gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode='w') as archive:
                for entry, source, mode in files:
                    content = source.read_bytes()
                    info = tarfile.TarInfo(entry)
                    info.size, info.mode = len(content), mode
                    archive.addfile(info, io.BytesIO(content))
    data = {'repo': repo, 'version': version, 'target': target, 'source_sha': sha,
            'asset': name, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
            'size': path.stat().st_size, 'validation': validation, 'details': details}
    with sidecar.open('x', encoding='utf-8', newline='\n') as checksum:
        checksum.write(f'{data["sha256"]}  {name}\n')
    metadata.write_text(json.dumps(data, indent=2) + '\n', encoding='utf-8')
    return data


def write_manifest(output, repo, version, sha, expected_targets):
    validate_context('owner/' + repo, sha)
    validate_version(version)
    records = []
    for path in sorted(output.glob('*.metadata.json')):
        data = json.loads(path.read_text(encoding='utf-8'))
        if not isinstance(data, dict) or any(data.get(key) != value for key, value in [('repo', repo), ('version', version), ('source_sha', sha)]):
            raise ValueError(f'metadata identity mismatch: {path}')
        target = data.get('target')
        if not isinstance(target, str) or target not in expected_targets:
            raise ValueError('unexpected target metadata')
        asset = data.get('asset', '')
        suffix = '.zip' if target.endswith('windows-msvc') else '.tar.gz'
        expected_asset = f'{repo}-{version}-{target}{suffix}'
        if asset != expected_asset or path.name != asset + '.metadata.json':
            raise ValueError('invalid asset name/path')
        archive = output / asset
        if archive.is_symlink() or not archive.is_file() or data.get('size') != archive.stat().st_size or data.get('sha256') != hashlib.sha256(archive.read_bytes()).hexdigest():
            raise ValueError(f'archive checksum/size mismatch: {asset}')
        sidecar = output / (asset + '.sha256')
        expected_checksum = f'{data["sha256"]}  {asset}\n'.encode('utf-8')
        if sidecar.is_symlink() or not sidecar.is_file() or sidecar.read_bytes() != expected_checksum:
            raise ValueError(f'checksum sidecar missing or invalid: {asset}')
        records.append(data)
    targets = [record['target'] for record in records]
    if len(targets) != len(set(targets)) or set(targets) != set(expected_targets):
        raise ValueError('missing, duplicate, or unexpected target artifacts')
    assets = [record['asset'] for record in records]
    allowed = set(assets + [name + suffix for name in assets for suffix in ('.metadata.json', '.sha256')] + ['SHA256SUMS', 'build-manifest.json', 'release-notes.md'])
    if any(path.name not in allowed for path in output.iterdir()):
        raise ValueError('unexpected file in aggregated artifacts')
    (output / 'SHA256SUMS').write_text(''.join(f'{row["sha256"]}  {row["asset"]}\n' for row in records), encoding='utf-8')
    (output / 'build-manifest.json').write_text(json.dumps({'repo': repo, 'version': version, 'source_sha': sha, 'artifacts': records}, indent=2) + '\n', encoding='utf-8')
    notes = [f'# {repo}-{version}', '', f'Source commit: `{sha}`', '', '| Target | Validation |', '| --- | --- |']
    notes.extend(f'| `{row["target"]}` | {row["validation"]} |' for row in records)
    notes += ['', 'Build-only targets have not been executed on their destination OS.',
              'Verify each archive using its .sha256 file or SHA256SUMS. Package/library version is unchanged.', '']
    (output / 'release-notes.md').write_text('\n'.join(notes), encoding='utf-8')
    return records


def build(target, repo, version, sha):
    from smoke import smoke
    row = next((row for row in matrix('extended')['include'] if row['target'] == target), None)
    if row is None:
        raise ValueError('unknown build target')
    tool = row['tool']
    env = dict(os.environ, IMARA_DIFF_BUILD_VERSION=version)
    details = {'tool': tool, 'rustc': capture(['rustc', '-vV'])}
    if tool == 'cross':
        image = f'ghcr.io/cross-rs/{target}:main'
        subprocess.run(['docker', 'pull', image], check=True)
        digest = json.loads(capture(['docker', 'image', 'inspect', image, '--format', '{{json .RepoDigests}}']))[0]
        env['CROSS_TARGET_' + target.upper().replace('-', '_') + '_IMAGE'] = digest
        details['cross_image'] = digest
    subprocess.run(build_command(target, tool), env=env, check=True)
    binary = (ROOT / 'target' / target / 'release' / ('imara-diff.exe' if target.endswith('windows-msvc') else 'imara-diff')).resolve()
    validation = 'build-only (not executed)'
    if tool != 'cross':
        prefix = []
        if target.startswith('armv7-'):
            prefix = ['qemu-arm'] + (['-L', '/usr/arm-linux-gnueabihf'] if target.endswith('gnueabihf') else [])
        elif target.startswith('i686-unknown-linux-'):
            prefix = ['qemu-i386'] + (['-L', '/'] if target.endswith('gnu') else [])
        smoke(binary, version, prefix)
        validation = 'CLI smoke via QEMU' if prefix else 'native/OS-compatibility CLI smoke'
        if tool == 'cargo':
            subprocess.run(['cargo', 'test', '--locked', '--release', '--workspace', '--target', target], env=env, check=True)
            validation += ' + workspace tests'
    if version.encode('ascii') not in binary.read_bytes():
        raise ValueError('build version is missing from the executable')
    details.update(audit(binary, target, tool))
    make_archive(binary, ROOT / 'dist', repo, version, target, sha, validation, details)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    version = sub.add_parser('version')
    version.add_argument('--profile', choices=['core', 'extended'], default='core')
    for command in ['build', 'manifest']:
        item = sub.add_parser(command)
        item.add_argument('--repo', required=True)
        item.add_argument('--version', required=True)
        item.add_argument('--sha', required=True)
        if command == 'build':
            item.add_argument('--target', required=True)
        else:
            item.add_argument('--profile', choices=['core', 'extended'], default='core')
            item.add_argument('--directory', type=Path, default=ROOT / 'dist')
    args = parser.parse_args()
    if args.command == 'version':
        selection = matrix(args.profile)
        repository, sha = os.environ['GITHUB_REPOSITORY'], os.environ['GITHUB_SHA']
        number, tag = reserve_version(github_api, repository, sha, datetime.now(timezone.utc))
        values = {'version': number, 'tag': tag, 'repo': repository.split('/')[1], 'matrix': json.dumps(selection, separators=(',', ':'))}
        with open(os.environ['GITHUB_OUTPUT'], 'a', encoding='utf-8') as output:
            output.write(''.join(f'{key}={value}\n' for key, value in values.items()))
        summary = f'## {tag}\n\nSource: `{sha}`\n\n{len(selection["include"])} targets; UTC version reserved by tag.\n'
        if path := os.environ.get('GITHUB_STEP_SUMMARY'):
            Path(path).write_text(summary, encoding='utf-8')
        print(summary)
    elif args.command == 'build':
        validate_context('owner/' + args.repo, args.sha)
        validate_version(args.version)
        build(args.target, args.repo, args.version, args.sha)
    else:
        targets = [row['target'] for row in matrix(args.profile)['include']]
        write_manifest(args.directory, args.repo, args.version, args.sha, targets)


if __name__ == '__main__':
    try:
        main()
    except (RuntimeError, ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f'release: {error}', file=sys.stderr)
        sys.exit(1)
