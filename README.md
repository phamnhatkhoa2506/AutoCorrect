# AutoCorrect

Realtime typo correction for every Windows app. Kế hoạch chi tiết: [PLAN.md](PLAN.md).

## Trạng thái: Phase 1 xong, đang nâng chất lượng (Phase 2)

- `crates/ac-telex`: phân tích âm tiết tiếng Việt và bộ gõ Telex (thuần hàm, có test).
- `crates/ac-core`: engine (buffer từ, sửa khi gõ Space, Backspace để hoàn tác) và `SmartCorrector`, bộ sửa theo noisy channel trên phím thô cho cả tiếng Việt (qua Telex) và tiếng Anh.
- `crates/ac-data`: build bảng tần suất `data/*.tsv` và bảng cặp từ `data/*_bigrams.bin` từ corpus (xem `data/ATTRIBUTION.md`).
- `crates/ac-bench`: benchmark có số liệu (xem bên dưới).
- `crates/ac-platform-win`: app `autocorrect` (khay hệ thống), gồm hook bàn phím/chuột, inject bằng `SendInput` trên luồng riêng, chính sách theo app, phát hiện ô mật khẩu.

## Chạy

```powershell
cargo test
cargo build --release
.	arget
eleaseutocorrect.exe            # chạy nền, icon ở khay hệ thống
.	arget
eleaseutocorrect.exe --debug    # kèm console: log từng phím, quyết định, top phương án
.	arget
eleaseutocorrect.exe --en       # khởi động ở chế độ Anh
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
| `definately ` / `seperate ` / `thier ` | `definitely ` / `separate ` / `their ` | danh sách lỗi chính tả phổ biến |
| `khogn ` | `không ` | hai lỗi cùng lúc (đảo phím + thiếu `o`) |
| `dunhf ` | `dùng ` | lệch sang phím bên cạnh (h ↔ g) |
| `nhnah ` | `nhanh ` | đảo phím |
| `gruwi ` | `gửi ` | đảo phím (gõ thanh `r` quá sớm) |
| `mooir ` | `mỗi ` | nhầm hỏi/ngã |
| `git `, `npm `, `cargo `, `Tuan ` | giữ nguyên | chữ có thật / tên riêng / lệnh |
| `teh ` rồi Backspace ngay | quay về `teh` | hoàn tác |
| `te`, click chuột chỗ khác, gõ `h ` | không sửa | buffer đã reset |

Thử trong: Notepad, Windows Terminal (PowerShell), VS Code, Chrome, Zalo/Messenger, Word.

Console in mỗi lần sửa kèm độ trễ (p50/p99) và tên process. Nếu thấy `!! only x/y inputs accepted` thì app đó đã chặn `SendInput` (thường do app chạy quyền Admin).

## Chất lượng sửa lỗi

Ba tầng, từ rẻ đến đắt: (1) danh sách lỗi chính tả phổ biến `data/en_misspellings.tsv`; (2) noisy channel: mỗi phương án = tần suất (có tính từ đứng trước, bảng bigram) trừ chi phí của kiểu gõ nhầm (đảo phím, lệch phím, nhầm hỏi/ngã, thiếu/thừa phím dấu hoặc chữ gõ đôi, nhầm nguyên âm); (3) nếu từ lạ và không có phương án một lỗi nào đủ chắc thì thử hai lỗi (`khogn` → `không`). Mọi ngưỡng nằm trong `Tuning` (`crates/ac-core/src/smart.rs`).

Quy tắc an toàn: từ phổ biến không bao giờ bị sửa; âm tiết tiếng Việt hợp lệ chỉ được đổi dấu (không thêm/bớt thanh, không đổi chữ cái); từ viết hoa đứng giữa câu coi là tên riêng; sau một từ tiếng Anh thì phương án tiếng Việt bị phạt và ngược lại.

### Benchmark

```powershell
cargo run -p ac-bench --release -- --sentences 1200
cargo run -p ac-bench --release -- --sentences 600 --margin 2 --known 5.5 --show 10   # dò ngưỡng, in ví dụ sai
```

Lấy 20.000 câu cuối của mỗi corpus (không dùng để huấn luyện), gõ từng từ đúng hoặc (15%) với một lỗi nhân tạo, rồi đo: bao nhiêu từ đúng bị sửa nhầm, bao nhiêu lỗi sửa đúng / sửa sai / bỏ sót. Lỗi nhân tạo chỉ là ước chừng: dùng để so sánh các phiên bản, không phải cam kết độ chính xác ngoài đời.

Kết quả (1200 câu mỗi ngôn ngữ), trước và sau Phase 3:

| | Việt: sửa đúng / sai, nhầm từ đúng | Anh: sửa đúng / sai, nhầm từ đúng |
| --- | --- | --- |
| 1 lỗi, trước | 39,0% / 6,2%, 0,37‰ | 47,2% / 7,4%, 2,27‰ |
| 1 lỗi, sau | 44,8% / 4,5%, 0,24‰ | 55,3% / 5,6%, 1,88‰ |
| 2 lỗi, trước | 9,2% / 24,8% | 0,0% / 19,3% |
| 2 lỗi, sau | 28,0% / 25,6% | 14,4% / 18,9% |

Ở tiếng Việt khoảng 43% lỗi gõ tạo ra một âm tiết hợp lệ khác, nên sửa an toàn không thể sửa chúng nếu không hiểu cả câu.

### Nhật ký để tinh chỉnh bằng dữ liệu thật

Menu khay có mục **Ghi nhật ký sửa lỗi** (mặc định tắt). Khi bật, mỗi lần app sửa một từ hoặc bạn hoàn tác, một dòng được ghi vào `%APPDATA%\AutoCorrect\journal.tsv` (`giây\tFIX|UNDO\tphím đã gõ\ttừ sửa\ttừ trước đó`). File chỉ nằm trên máy bạn, không bao giờ gửi đi; xóa tùy ý. Xem báo cáo: `cargo run -p ac-bench --release -- --journal`.

## Giới hạn đã biết

- Chỉ kích hoạt sửa bằng Space. Enter/Tab/dấu câu chỉ reset buffer.
- Chưa có VNI; danh sách app theo từng loại chưa sửa được qua giao diện.
- Chưa sửa lỗi "sai thành chữ có thật" (`git` vs `it`, `bài` gõ thành `bại`): cần hiểu cả câu.
- Hai lỗi cùng lúc vẫn hay sai (khoảng một phần tư), nên nhánh này chỉ chạy với từ lạ và ngưỡng chặt.
- Từ lạ viết thường (tên riêng, thuật ngữ) đôi khi bị sửa nhầm thành từ phổ biến gần giống (khoảng 2 trên 1000 từ tiếng Anh).
- Một lần sửa từ lạ tốn 5–10 ms (chữ đúng phổ biến chỉ vài µs).
