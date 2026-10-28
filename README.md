# VidSaver

App desktop (Windows + macOS) tải **video chất lượng cao** và **MP3 320 kbps** từ YouTube, TikTok, Facebook.
Dùng [Tauri 2](https://tauri.app) + Svelte cho giao diện, [yt-dlp](https://github.com/yt-dlp/yt-dlp) + ffmpeg để tải và ghép.

## Tính năng

- Dán link (hoặc copy link rồi quay lại app, app tự dán), xem trước tiêu đề, ảnh bìa, độ phân giải tối đa.
- 3 chế độ:
  - **Video · Cao nhất**: bản tốt nhất có thể (YouTube đến 4K/8K), tự ghép hình + tiếng thành MP4.
  - **Video · Tương thích**: H.264 ≤ 1080p, mở được trên mọi máy và điện thoại.
  - **MP3 · 320 kbps**: chỉ lấy âm thanh, kèm ảnh bìa và thông tin bài.
- Hàng đợi tải (2 video cùng lúc), tiến độ, tốc độ, thời gian còn lại, huỷ, thử lại, mở file.
- Cookie đăng nhập (Firefox / Chrome / Edge / Brave / Safari / file cookies.txt) cho video cần đăng nhập.
- yt-dlp tự cập nhật mỗi ngày (các trang thay đổi liên tục, cập nhật giúp app không bị hỏng).

## 1. Cài công cụ để build (chỉ làm một lần)

### Windows

1. **Visual Studio C++ Build Tools**: tải tại <https://visualstudio.microsoft.com/visual-cpp-build-tools/>,
   khi cài chọn **"Desktop development with C++"**. Hoặc chạy trong PowerShell:
   ```powershell
   winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
   ```
2. **Rust**:
   ```powershell
   winget install Rustlang.Rustup
   ```
   Cài xong **đóng và mở lại** VS Code / terminal, kiểm tra bằng `cargo --version`.
3. **Node.js** 18+ (máy bạn đã có).
4. WebView2 có sẵn trên Windows 10/11.

### macOS

```bash
xcode-select --install
curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh
brew install node   # hoặc tải Node.js từ nodejs.org
```

## 2. Chạy thử khi phát triển

```bash
npm install
npm run tauri dev
```

Lần đầu Rust sẽ biên dịch khá lâu (3–10 phút), các lần sau nhanh hơn nhiều.

## 3. Đóng gói ra file cài đặt

```bash
npm run tauri build
```

Kết quả nằm trong `src-tauri/target/release/bundle/`:

| Hệ điều hành | File                                        |
| ------------ | ------------------------------------------- |
| Windows      | `nsis/VidSaver_0.1.0_x64-setup.exe`, `msi/` |
| macOS        | `dmg/VidSaver_0.1.0_aarch64.dmg`, `macos/`  |

**Bản macOS phải build trên máy Mac** (Tauri không build chéo từ Windows sang macOS). Nếu không có Mac, có thể
dùng GitHub Actions với runner `macos-latest` và [tauri-action](https://github.com/tauri-apps/tauri-action).
Để người khác mở được app mà macOS không chặn, cần tài khoản Apple Developer để ký và notarize; nếu chưa ký,
người dùng phải chuột phải vào app → **Open** ở lần mở đầu tiên.

## 4. Sử dụng

1. Lần đầu mở app, bấm **Cài đặt ngay** để tải yt-dlp, ffmpeg, deno (khoảng 150–250 MB, chỉ một lần).
   Các file này nằm trong thư mục dữ liệu của app:
   - Windows: `%APPDATA%\com.msduyen.vidsaver\bin`
   - macOS: `~/Library/Application Support/com.msduyen.vidsaver/bin`
2. Copy link video, quay lại app (tự dán) hoặc bấm **Dán**, rồi bấm **Lấy video**.
3. Chọn chế độ, bấm **Tải về**. File lưu vào thư mục Downloads (đổi trong **Cài đặt**).

### Khi nào cần đăng nhập?

| Trang    | Video công khai                                     | Cần cookie khi                                   |
| -------- | --------------------------------------------------- | ------------------------------------------------ |
| YouTube  | Không cần                                           | Video giới hạn tuổi, thành viên, hoặc báo "not a bot" |
| TikTok   | Không cần                                           | Video riêng tư / chỉ bạn bè                       |
| Facebook | Thường không cần (reel, video trang công khai)      | Video bạn bè, nhóm kín, hoặc Facebook chặn người chưa đăng nhập |

Cách dùng cookie: **Cài đặt → Cookie đăng nhập**, chọn trình duyệt bạn đã đăng nhập trang đó.

- **Firefox** ổn định nhất.
- **Chrome / Edge trên Windows** mã hoá cookie kiểu mới nên yt-dlp hay không đọc được. Khi đó dùng
  **File cookies.txt**: cài extension xuất cookie dạng Netscape (ví dụ "Get cookies.txt LOCALLY"),
  mở facebook.com khi đã đăng nhập, xuất file, rồi chọn file đó trong app.
- File cookies.txt chứa phiên đăng nhập của bạn, **không gửi file này cho ai**.

### Lỗi thường gặp

| Hiện tượng                                  | Cách xử lý                                              |
| ------------------------------------------- | ------------------------------------------------------- |
| "Trang web có thể vừa thay đổi", HTTP 403   | Cài đặt → **Cập nhật** yt-dlp rồi tải lại               |
| "Video cần đăng nhập…"                      | Chọn cookie như ở trên                                  |
| "Không đọc được cookie từ trình duyệt"      | Tắt hẳn trình duyệt rồi thử lại, hoặc dùng Firefox / cookies.txt |
| Lỗi ghép / chuyển đổi                       | Cài đặt → **Cài lại thành phần**                        |
| File "Cao nhất" không mở được trên máy cũ   | Chọn chế độ **Tương thích** (H.264), hoặc mở bằng VLC   |

## Cấu trúc code

```
src/                     Giao diện Svelte
  App.svelte             Màn hình chính: dán link, xem trước, chọn chế độ, hàng đợi, cài đặt
  lib/api.ts             Gọi lệnh Rust + nhận event tiến độ
  lib/format.ts          Định dạng dung lượng, thời lượng, nhận diện trang
src-tauri/
  src/deps.rs            Tải/cài yt-dlp, ffmpeg, deno; cập nhật yt-dlp
  src/download.rs        Lấy thông tin video, chạy yt-dlp, đọc tiến độ, huỷ, thông báo lỗi dễ hiểu
  tauri.conf.json        Cấu hình cửa sổ, đóng gói
  capabilities/          Quyền của giao diện (mở file, chọn thư mục, đọc clipboard)
```

## Lưu ý

Chỉ tải nội dung bạn có quyền: video của chính bạn, nội dung cho phép tải, hoặc dùng cá nhân.
Điều khoản của YouTube, TikTok, Facebook thường hạn chế việc tải video về.
