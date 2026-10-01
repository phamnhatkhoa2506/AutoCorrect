# AutoCorrect

Realtime typo correction for every Windows app. Kế hoạch chi tiết: [PLAN.md](PLAN.md).

## Trạng thái: Phase 1 (đang làm)

- `crates/ac-telex`: phân tích âm tiết tiếng Việt và bộ gõ Telex (thuần hàm, có test).
- `crates/ac-core`: engine (buffer từ, sửa khi gõ Space, Backspace để hoàn tác) và `SmartCorrector`, bộ sửa theo noisy channel trên phím thô cho cả tiếng Việt (qua Telex) và tiếng Anh.
- `crates/ac-data`: build bảng tần suất `data/*.tsv` từ corpus (xem `data/ATTRIBUTION.md`).
- `crates/ac-platform-win`: binary `ac-spike`, gồm hook bàn phím/chuột, inject bằng `SendInput` trên luồng riêng, log ra console.

## Chạy

```powershell
cargo test
cargo run --release --bin ac-spike                       # bình thường
cargo run --release --bin ac-spike -- --debug            # in mọi phím, quyết định và top phương án (chỉ ra console)
cargo run --release --bin ac-spike -- --normal-priority  # A/B: tắt ưu tiên cao cho luồng hook
```

**Tắt Unikey/EVKey** (hoặc chuyển sang chế độ E) trước khi test. Bộ gõ Telex riêng chưa được gắn vào hook (Phase 1.2), nên chữ Telex gõ đúng như `tieengs` vẫn hiện nguyên phím thô; chỉ chữ gõ **sai** mới được sửa.

Build lại dữ liệu (cần tải corpus vào `data/raw/` trước, xem đầu file `crates/ac-data/src/main.rs`):

```powershell
cargo run -p ac-data --release
```

## Kịch bản test tay

| Gõ | Kết quả mong đợi | Loại lỗi |
| --- | --- | --- |
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
- Chưa gắn bộ gõ Telex riêng vào hook (Phase 1.2), nên chưa thay được Unikey.
- Chưa phân biệt app (terminal/IDE/ô mật khẩu).
- Chưa sửa lỗi "sai thành chữ có thật" (`git` vs `it`), việc này cần ngữ cảnh (Phase 2).
- Lỗi cách 2 bước chưa sửa được (`khogn` → `không`). Một số lỗi chính tả tiếng Anh như `seperate`, `thier` vẫn chưa được sửa (xem test `known_misses`).
