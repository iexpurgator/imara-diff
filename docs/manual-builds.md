# Build/phát hành thủ công trên GitHub

## Những gì đã thiết lập

Hai workflow đều **chỉ có `workflow_dispatch`**:

- **Manual CI** (`ci.yml`): kiểm thử, MSRV/no_std, Clippy/docs, Miri, fuzz và helper release.
- **Manual multi-platform release** (`release.yml`): build, kiểm tra binary thật,
  đóng gói, tạo checksum/manifest và GitHub Release.

Không có trigger push, pull request, tag hay schedule. Commit/push code và tạo
reservation tag không tự chạy CI/CD. Workflow release mặc định tạo **Draft**,
chưa công bố ra ngoài.

## Tên/version riêng, không đụng version upstream

Ví dụ:

```text
Tag và tên release: imara-diff-26.293.0
CLI --version:     imara-diff 26.293.0

imara-diff-26.293.0-x86_64-pc-windows-msvc.zip
imara-diff-26.293.0-x86_64-pc-windows-msvc.zip.sha256
imara-diff-26.293.0-x86_64-unknown-linux-musl.tar.gz
imara-diff-26.293.0-x86_64-unknown-linux-musl.tar.gz.sha256
```

- Prefix lấy từ tên **repo fork hiện tại**, không đổi tên repository trên GitHub.
- `YY.DAYOFYEAR.N`, theo **UTC** tại lúc cấp số. YY là hai chữ số cuối của năm
  (2026 → 26). Day-of-year bắt đầu từ 1, không đệm số 0; năm nhuận có ngày 366.
  N bắt đầu từ **0** mỗi ngày. Các tag định dạng bốn chữ số cũ được giữ nguyên,
  không tự rename/xóa; bộ đếm mới chỉ xét namespace năm hai chữ số.
- Mỗi lần chạy release sẽ giữ chỗ bằng một lightweight tag trỏ đến commit đang
  build. Số tiếp theo là số lớn nhất của ngày đó + 1. Tạo ref là thao tác atomic;
  nếu hai run tranh cùng số, run thua sẽ lấy số kế tiếp, không ghi đè tag.
- Thứ tự số là thứ tự **giữ chỗ**, không nhất thiết thứ tự bấm nút nếu chạy đồng thời.
- Build lỗi/cancel sau khi giữ chỗ vẫn tiêu thụ số. Giữ nguyên các tag này để
  không tái sử dụng số; tự xóa reservation tag có thể làm số đó được dùng lại.
- **Re-run all jobs** cấp số mới. **Re-run failed jobs** có thể dùng lại version
  từ prepare đã thành công, nếu prepare không bị chạy lại.
- Không tạo tag upstream dạng `v0.2.0`, không sửa `Cargo.toml` version thư viện.
  Version CLI được đưa vào lúc compile qua `IMARA_DIFF_BUILD_VERSION`.
  Build local không đặt biến này vẫn hiện package version phát triển.
- Chạy riêng Manual CI không giữ chỗ version và không tạo release/tag.

## 1. Đưa workflow lên đúng fork

Các file hiện chỉ nằm trong working tree local; GitHub không nhìn thấy chúng
cho đến khi bạn tự commit và push. Cần đưa **toàn bộ source thay đổi** lên,
không chỉ file YAML: CLI, các bugfix/test trước, `.cargo`, `ci`, `Cross.toml`, v.v.

Sau khi xem lại `git diff` và xác nhận muốn lưu/phát hành:

```sh
git remote -v                  # origin phải là fork của bạn
# Ví dụ hiện tại: https://github.com/iexpurgator/imara-diff

git add .cargo .github ci Cross.toml Cargo.toml Cargo.lock README.md src tests docs .gitattributes .gitignore
git commit -m "Add manual multi-platform calendar-version releases"
git push origin master
```

Đây là hướng dẫn để bạn tự chạy khi sẵn sàng, không phải các lệnh đã được agent
thực thi. Không push vào `pascalkuthe/imara-diff`.

Workflow phải có trên **default branch** (hiện là `master`) để GitHub hiện nút
**Run workflow**. Sau đó có thể chọn branch khác khi dispatch.

## 2. Chạy kiểm tra khi cần

1. Mở fork của bạn → **Actions**.
2. Nếu fork đang tắt Actions, chọn bật workflows trên fork.
3. Chọn **Manual CI** → **Run workflow**.
4. Chọn branch chứa source muốn kiểm tra → **Run workflow**.
5. Xem từng job; Miri/fuzz dùng nightly, độc lập với toolchain release.

Không có gì chạy tự động ở lần push kế tiếp.

## 3. Build và tạo bản phát hành

1. Trong **Actions**, chọn **Manual multi-platform release**.
2. Bấm **Run workflow**, chọn branch/commit source qua branch selector.
3. Chọn `profile`:
   - **core** (nên chạy đầu tiên): 13 target Windows/macOS/Linux GNU/musl.
   - **extended**: cả 46 target ứng viên, gồm BSD/illumos/Solaris/Android và các
     kiến trúc ít phổ biến. Nhiều target chỉ được build/audit, chưa được chạy.
4. Giữ **draft = true** cho lần đầu. Nếu bỏ chọn, workflow tạo release công khai
   sau khi mọi target bắt buộc và manifest đều thành công.
5. Bấm **Run workflow**. Job **Reserve UTC build version** hiển thị tag/version
   trong summary; mỗi build job ghi rõ target và công cụ.
6. Khi thành công, mở **Releases** → bản Draft vừa tạo, kiểm tra asset/manifest,
   sau đó bấm **Publish release** nếu muốn công bố.

Mỗi profile là một tập bắt buộc: **một target lỗi thì không tạo release tổng**.
`fail-fast: false` cho phép các target khác chạy tiếp; các archive thành công
vẫn tải được ở phần **Artifacts** của run, giữ 14 ngày. Đây là cách xem kết quả
probe extended trước khi quyết định hỗ trợ/phát hành chính thức.

## 4. Build được gì và kiểm tra đến đâu

`ci/targets.json` là danh sách target chính xác. Core gồm:

- Windows MSVC static CRT: x64, x86, ARM64.
- macOS: Intel x64, Apple Silicon ARM64.
- Linux GNU và musl: x64, i686, ARM64, ARMv7 hard-float.

Release compiler được pin **Rust 1.98.1**, riêng MSRV thư viện vẫn kiểm tra 1.71.
Windows/macOS chạy workspace tests và smoke test CLI. Linux core dùng Zig,
GNU target đặt floor 2.17 và kiểm tra symbol GLIBC; ARMv7/i686 smoke qua QEMU,
ARM64/x64 trên runner native. Musl phải không có dynamic interpreter/NEEDED libs.
Windows phải chỉ import các system DLL đã cho phép; macOS chỉ dùng system libs.

Extended dùng cross source pin theo commit. Image `:main` được resolve sang
**digest cụ thể trước khi build**, và digest được ghi trong metadata; đầu vào
image giữa hai run vẫn có thể thay đổi. Đây là luồng thử nghiệm, không phải
cam kết tái lập bit-for-bit. Availability của image, SDK, target và ABI còn phải
được kiểm chứng bằng run GitHub thật. Target không chạy được được ghi rõ
`build-only (not executed)` trong manifest/release notes.

Không có ký Developer-ID/notarization cho macOS hay chứng chỉ ký Windows.
Windows build không cần Visual C++ Redistributable nhưng vẫn cần phiên bản
Windows được toolchain hỗ trợ. Universal2 chưa được thêm; có hai bản macOS riêng.

## 5. Tải và kiểm tra

GitHub Release chứa:

- `.zip` Windows hoặc `.tar.gz` Unix, tên ngoài chứa repo/version/target.
- Mỗi archive có file `<tên-archive>.sha256` đi kèm riêng, như cách ripgrep phân
  phối asset. Nội dung dùng định dạng `sha256sum`: `<hash>  <tên-archive>`.
- `SHA256SUMS`: checksum tổng của các archive cuối cùng.
- `build-manifest.json`: source SHA, target, compiler, ABI/dependency audit,
  digest image nếu có, và mức kiểm tra runtime.

Bên trong archive là file phẳng, **không có thư mục/version/arch trong tên binary**:

```text
Windows .zip:       imara-diff.exe, LICENSE, README.md
Linux/macOS .tar.gz: imara-diff, LICENSE, README.md
```

Checksum nằm cạnh archive, không nằm bên trong. File `.metadata.json` là dữ
liệu nội bộ của CI; không upload từng file này lên Release. Actions Artifacts
`binary-<target>` là bundle trung gian; tải ở **Releases** để lấy trực tiếp các
archive/`.sha256` có tên như trên.

Ví dụ kiểm tra archive tải về:

```powershell
Get-FileHash .\imara-diff-26.293.0-x86_64-pc-windows-msvc.zip -Algorithm SHA256
Get-Content .\imara-diff-26.293.0-x86_64-pc-windows-msvc.zip.sha256
```

```sh
sha256sum -c imara-diff-26.293.0-x86_64-unknown-linux-musl.tar.gz.sha256
# macOS: shasum -a 256 -c ten-archive.tar.gz.sha256
# Hoặc đối chiếu danh sách tổng:
sha256sum -c SHA256SUMS --ignore-missing
```

Đối chiếu với checksum đúng tên file. TAR giữ executable bit của binary Unix.
Plugin phải phân biệt x64/ARM64/x86 và GNU/musl khi chọn asset.

## 6. Quyền, lỗi và chạy lại

- Không cần PAT để release vào chính fork. `GITHUB_TOKEN` được cấp
  `contents: write` chỉ ở prepare/publish; build jobs chỉ đọc.
- Nếu nhận HTTP 403, kiểm tra Actions policy của repo/org và ruleset bảo vệ
  prefix tag `<repo>-*`; không tự tắt mọi cơ chế bảo vệ hoặc thêm PAT quyền rộng.
- Nếu đã có tag nhưng không có release: đó có thể là build lỗi đã giữ chỗ.
  Sửa lỗi rồi trigger mới; số tăng lên, không đè bản cũ.
- Nếu publish lỗi giữa chừng và để lại Draft: không có `--clobber` hay tự xóa
  release. Cách an toàn là chạy lại **all jobs** để có version mới. Bạn có thể
  tự xem/xóa Draft dang dở, nhưng nên giữ tag giữ chỗ.
- Runner/tool/image lỗi khác với bug của thư viện: mở log target tương ứng.
  Extended thất bại không khiến target đó trở thành 'đã hỗ trợ'.

## Đã kiểm tra local và giới hạn hiện tại

- Workflow qua `actionlint`; cả hai chỉ có trigger `workflow_dispatch`.
- 16 test Python đạt: cấp số/năm hai chữ số, tranh chấp/listing cũ, UTC/năm nhuận,
  tên binary phẳng, checksum riêng, thiếu/sai artifact và chống ghi đè.
- 42 test Rust đạt (1 test cũ bỏ qua); CLI nhận version build lịch và chạy được
  với MSRV 1.71. Build/đóng gói thử Windows x64 static đã qua smoke/DLL audit.
- Không có GitHub API mutation/dispatch thật ở bước kiểm tra local: tag allocator
  được thử với API giả lập có trạng thái. Chưa có run/release thật trên GitHub.
- Lần kiểm tra cross-build Linux local bị chặn do ổ C: thiếu dung lượng; rustup
  rollback target cài dở. Linux/macOS/extended cần lần chạy GitHub để xác nhận.

## Dùng GitHub CLI thay giao diện web (tùy chọn)

Sau khi đã push và đăng nhập `gh` vào đúng tài khoản:

```sh
gh workflow run ci.yml --repo iexpurgator/imara-diff --ref master
gh workflow run release.yml --repo iexpurgator/imara-diff --ref master -f profile=core -F draft=true
gh run list --repo iexpurgator/imara-diff --workflow release.yml
gh run view RUN_ID --repo iexpurgator/imara-diff --log-failed
gh run download RUN_ID --repo iexpurgator/imara-diff --dir downloads
```

Thay `RUN_ID` bằng run cần xem. Các lệnh dispatch/release chỉ do bạn chủ động chạy.
