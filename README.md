# AutoCorrect

Realtime typo correction for every Windows app. Kế hoạch chi tiết: [PLAN.md](PLAN.md).

## Trạng thái: Phase 1 (đang làm)

- `crates/ac-telex`: phân tích âm tiết tiếng Việt và bộ gõ Telex (thuần hàm, có test).
- `crates/ac-core`: engine (buffer từ, sửa khi gõ Space, Backspace để hoàn tác) và `SmartCorrector`, bộ sửa theo noisy channel trên phím thô cho cả tiếng Việt (qua Telex) và tiếng Anh.
- `crates/ac-data`: build bảng tần suất `data/*.tsv` từ corpus (xem `data/ATTRIBUTION.md`).
- `crates/ac-platform-win`: app `autocorrect` (khay hệ thống), gồm hook bàn phím/chuột, inject bằng `SendInput` trên luồng riêng, chính sách theo app, phát hiện ô mật khẩu.

## Chạy

```powershell
cargo test
cargo build --release
.	argeteleaseutocorrect.exe            # chạy nền, icon ở khay hệ thống
.	argeteleaseutocorrect.exe --debug    # kèm console: log từng phím, quyết định, top phương án
.	argeteleaseutocorrect.exe --en       # khởi động ở chế độ Anh
```

`autocorrect` **là bộ gõ Telex luôn**: thoát hẳn Unikey/EVKey trước khi chạy (hai bộ gõ cùng lúc sẽ đánh nhau). Chỉ chạy được 1 bản cùng lúc.

- **Icon khay**: `V` đỏ = tiếng Việt, `E` xanh = tiếng Anh, `–` xám = tạm dừng. Bấm trái để chuyển Việt/Anh (hoặc **Alt+Z**).
- **Menu chuột phải**: Tiếng Việt, Tự sửa lỗi gõ, Tạm dừng, Khởi động cùng Windows, Thoát.
- Cài đặt được lưu ở `%APPDATA%\AutoCorrect\settings.ini`.

### Theo từng app

| App | Telex | Sửa lỗi |
| --- | --- | --- |
| Bình thường (trình duyệt, chat, Office…) | ✓ | tiếng Việt + tiếng Anh |
| Terminal, IDE (Windows Terminal, Git Bash, VS Code, Antigravity, JetBrains…) | ✓ | chỉ tiếng Việt |
| Ô mật khẩu, Remote Desktop, KeePass/1Password/Bitwarden | ✗ | ✗ (phím đi thẳng, không giữ gì) |

Danh sách app nằm trong `crates/ac-platform-win/src/policy.rs`. Ô mật khẩu được phát hiện qua UI Automation.

Build lại dữ liệu (cần tải corpus vào `data/raw/` trước, xem đầu file `crates/ac-data/src/main.rs`):

```powershell
cargo run -p ac-data --release
```

## Kịch bản test tay

| Gõ | Kết quả mong đợi | Loại lỗi |
| --- | --- | --- |
| `tieengs vieetj ` | `tiếng việt ` | gõ Telex bình thường |
| `vieetj`, Backspace, `n` | `viện` | sửa giữa chừng |
| `teh ` / `TEH ` / `hte ` | `the ` / `THE ` / `the ` | đảo phím |
| `Recieve ` | `Receive ` | đảo phím, giữ chữ hoa |
| `untill ` / `occured ` | `until ` / `occurred ` | thừa/thiếu chữ gõ đôi |
| `definately ` | `definitely ` | nhầm nguyên âm |
| `dunhf ` | `dùng ` | lệch sang phím bên cạnh (h ↔ g) |
| `nhnah ` | `nhanh ` | đảo phím |
| `gruwi ` | `gửi ` | đảo phím (gõ thanh `r` quá sớm) |
| `mooir ` | `mỗi ` | nhầm hỏi/ngã |
| `git `, `npm `, `cargo `, `Tuan ` | giữ nguyên | chữ có thật / tên riêng / lệnh |
| `teh ` rồi Backspace ngay | quay về `teh` | hoàn tác |
| `te`, click chuột chỗ khác, gõ `h ` | không sửa | buffer đã reset |

Thử trong: Notepad, Windows Terminal (PowerShell), VS Code, Chrome, Zalo/Messenger, Word.

Console in mỗi lần sửa kèm độ trễ (p50/p99) và tên process. Nếu thấy `!! only x/y inputs accepted` thì app đó đã chặn `SendInput` (thường do app chạy quyền Admin).

## Giới hạn đã biết

- Chỉ kích hoạt sửa bằng Space. Enter/Tab/dấu câu chỉ reset buffer.
- Chưa có VNI; danh sách app theo từng loại chưa sửa được qua giao diện.
- Chưa sửa lỗi "sai thành chữ có thật" (`git` vs `it`), việc này cần ngữ cảnh (Phase 2).
- Lỗi cách 2 bước chưa sửa được (`khogn` → `không`). Một số lỗi chính tả tiếng Anh như `seperate`, `thier` vẫn chưa được sửa (xem test `known_misses`).
