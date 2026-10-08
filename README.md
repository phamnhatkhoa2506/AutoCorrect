# AutoCorrect

Sửa lỗi gõ tiếng Việt (Telex) và tiếng Anh theo thời gian thực, cho mọi ứng dụng trên Windows. Chạy hoàn toàn offline; không gửi gì đi đâu.

- Gõ sai `tieengs vieetj ` → `tiếng việt `, `teh ` → `the `, `khogn ` → `không `.
- Gõ tiếng Việt không dấu được thêm dấu theo ngữ cảnh: `tôi khong ` → `tôi không `.
- Ctrl+Z ngay sau một lần sửa để hoàn tác lần sửa đó.
- Từ đã đúng (từ phổ biến, tên riêng, lệnh như `git`, `npm`) được giữ nguyên.

## Cài đặt

### Dùng bản đóng gói (khuyến nghị)

1. Tải `AutoCorrect-<phiên bản>-win64.zip` ở trang [Releases](https://github.com/phamnhatkhoa2506/AutoCorrect/releases).
2. Giải nén **cả thư mục**. Ba file phải nằm cạnh nhau:
   - `autocorrect.exe`: chương trình chính, chạy nền ở khay hệ thống.
   - `autocorrect-settings.exe`: cửa sổ cài đặt.
   - `student.acs`: mô hình hỗ trợ sửa muộn (56 MB).
3. Thoát Unikey, EVKey hoặc bộ gõ tiếng Việt khác trước khi chạy. Hai bộ gõ cùng lúc sẽ đánh nhau.
4. Chạy `autocorrect.exe`. Icon xuất hiện ở khay hệ thống.

Yêu cầu: Windows 10/11 64-bit. Cửa sổ cài đặt dùng Microsoft Edge WebView2 (có sẵn trên Windows 11).

Chỉ chạy được một bản `autocorrect.exe` cùng lúc.

### Tự build từ mã nguồn

Cần Rust (stable, MSVC), Node.js (cho cửa sổ cài đặt) và git-lfs (để tải `data/student.acs`).

```powershell
git lfs pull
cargo test
cargo build -p ac-platform-win --release                      # autocorrect.exe
cd app
npm install
npm run build
cd src-tauri
cargo build --release --features tauri/custom-protocol        # autocorrect-settings.exe
```

Sau đó đặt `autocorrect.exe`, `autocorrect-settings.exe` và `student.acs` (lấy từ `data\student.acs`) cạnh nhau. Bản cửa sổ cài đặt phải có `--features tauri/custom-protocol`, nếu không nó sẽ cố kết nối tới máy chủ phát triển và báo lỗi "localhost refused to connect".

## Sử dụng

| Thao tác | Kết quả |
| --- | --- |
| Bấm trái icon khay, hoặc **Alt+Z** | Chuyển Việt / Anh |
| Chuột phải icon khay | Menu: Tiếng Việt, Tự sửa lỗi gõ, Tạm dừng, Khởi động cùng Windows, Cài đặt, Mở từ điển cá nhân, Thoát |
| Ctrl+Z ngay sau một lần sửa | Hoàn tác lần sửa đó (giữ nguyên dấu cách hoặc dấu câu) |
| Thêm `--debug` khi chạy | Hiện console: log từng phím, quyết định sửa, các phương án |
| Thêm `--en` khi chạy | Khởi động ở chế độ tiếng Anh |

Icon khay: `V` đỏ là tiếng Việt, `E` xanh là tiếng Anh, `–` xám là tạm dừng.

Sửa diễn ra khi bạn gõ dấu cách hoặc dấu câu (`. , ; : ! ?`). Enter, Tab và các ký tự khác chỉ reset bộ đệm từ.

### Theo loại ứng dụng

| Ứng dụng | Telex | Sửa lỗi |
| --- | --- | --- |
| Thông thường (trình duyệt, chat, Office, ...) | Có | Tiếng Việt và tiếng Anh |
| Terminal, IDE (Windows Terminal, Git Bash, VS Code, JetBrains, ...) | Có | Chỉ tiếng Việt; bật tiếng Anh trong menu **Sửa lỗi tiếng Anh cả trong IDE/terminal** |
| Ô mật khẩu, Remote Desktop, trình quản lý mật khẩu | Không | Không (phím đi thẳng, không giữ gì) |

Danh sách ứng dụng nằm trong `crates/ac-platform-win/src/policy.rs`. Ô mật khẩu được phát hiện qua UI Automation.

## Cài đặt (cửa sổ Cài đặt)

Mở bằng menu khay → **Cài đặt**. Các tùy chọn lưu trong `%APPDATA%\AutoCorrect\settings.ini`:

- **Mức độ sửa**: Cẩn thận, Cân bằng, Mạnh tay (ảnh hưởng ngưỡng sửa).
- **Sửa muộn** và **Dùng mô hình học + n-gram**: sửa thêm sau khi từ kế tiếp được gõ, dựa trên ngữ cảnh.
- **Mô hình học (nâng cao)**: mức tin cậy (mặc định theo mức độ sửa), chỉ chọn cách sửa hợp lý theo chữ đã gõ, cho n-gram sửa tiếp những gì mô hình bỏ qua, trọng số pha trộn.
- **Tự thêm dấu khi gõ không dấu**: thêm dấu cho từ gõ không dấu (có thể tắt nếu bạn cố ý gõ không dấu, ví dụ URL).
- **Sửa lỗi tiếng Anh cả trong IDE/terminal**.

## Từ điển cá nhân

Menu khay → **Mở từ điển cá nhân** mở `%APPDATA%\AutoCorrect\personal.tsv`. Mỗi dòng một mục, dòng bắt đầu bằng `#` là chú thích:

```text
ignore<TAB>kubectl         không bao giờ tự sửa từ này
fix<TAB>ko<TAB>không       luôn đổi chữ vừa gõ thành chữ bên phải
```

Viết đúng như phím đã gõ (khi gõ tiếng Việt là phím Telex), chữ thường. Lưu file, rồi chuyển sang cửa sổ khác là app nạp lại. Từ điển cá nhân được xét trước mọi luật khác.

Nếu bạn hoàn tác cùng một lần sửa hai lần, app tự thêm dòng `ignore` cho từ đó và không sửa nữa.

## Nhật ký sửa lỗi (tùy chọn)

Menu khay → **Ghi nhật ký sửa lỗi** (mặc định tắt). Khi bật, mỗi lần app sửa hoặc bạn hoàn tác, một dòng được ghi vào `%APPDATA%\AutoCorrect\journal.tsv`. File chỉ nằm trên máy bạn và không bao giờ được gửi đi; xóa bất cứ lúc nào.

## Cách hoạt động

Ba tầng, từ rẻ đến đắt:

1. **Danh sách lỗi phổ biến** (`data/en_misspellings.tsv`).
2. **Mô hình kênh nhiễu**: mỗi phương án được chấm bằng tần suất (có tính từ đứng trước, bảng cặp từ Kneser-Ney) trừ chi phí của kiểu gõ nhầm (đảo phím, lệch phím, nhầm hỏi/ngã, thiếu hoặc thừa phím dấu, chữ gõ đôi).
3. **Hai lỗi cùng lúc** (`khogn` → `không`): chỉ với từ lạ và ngưỡng chặt.

Tùy chọn, **mô hình học** (`student.acs`, 13,9 triệu tham số, chạy trên CPU) quyết định có sửa muộn hay không; cách sửa được chọn trong các phương án n-gram thấy hợp lý. Mô hình được nạp trên một luồng riêng sau lần gõ đầu tiên.

Quy tắc an toàn:
- Từ phổ biến không bao giờ bị sửa.
- Âm tiết tiếng Việt hợp lệ chỉ được đổi dấu, không thêm/bớt thanh và không đổi chữ cái.
- Từ viết hoa đứng giữa câu được coi là tên riêng.
- Sau một từ tiếng Anh, phương án tiếng Việt bị phạt, và ngược lại.
- Các ngưỡng nằm trong `Tuning` (`crates/ac-core/src/smart.rs`).

## Đo chất lượng

```powershell
cargo run -p ac-bench --release -- --sentences 1200             # đo trên corpus, lỗi nhân tạo
cargo run -p ac-bench --release -- --viwiki                      # Viwiki-Spelling, lỗi viết thật
cargo run -p ac-sim --release -- --sentences 2500 --env normal   # gõ phím giả lập qua động cơ thật
cargo run -p ac-e2e --release                                     # kiểm thử tự động có cửa sổ riêng
```

- `ac-bench` gõ từng từ đúng hoặc có một lỗi nhân tạo, rồi đo số từ đúng bị sửa nhầm và số lỗi sửa đúng / sai / bỏ sót.
- `ac-sim` mô phỏng gõ từng phím qua động cơ thật, với bố cục bàn phím đo được và các kiểu trượt tay, đè phím, giữ phím.
- `ac-e2e` mở một cửa sổ nhỏ, gửi phím thật vào đó và so kết quả. Đừng chạm bàn phím khi chạy (khoảng 15 giây). Từ chối chạy khi `autocorrect.exe` đang chạy.

Các con số là để so sánh giữa các phiên bản, không phải độ chính xác ngoài đời. Ghi chú nghiên cứu và kết quả chi tiết nằm trong thư mục `tools/` và các ghi chú nội bộ.

## Huấn luyện lại

Dữ liệu corpus không nằm trong git. Để dựng lại `data/*.tsv` và `data/*_bigrams.bin`, tải corpus vào `data/raw/` (xem đầu `crates/ac-data/src/main.rs`) rồi chạy:

```powershell
cargo run -p ac-data --release
```

Mô hình học được huấn luyện trên Kaggle (xem `tools/student/` và `tools/kaggle/`) và xuất ra `student.acs` bằng `tools/student/export_student.py`.

## Giới hạn đã biết

- Chưa có kiểu gõ VNI.
- Chưa sửa lỗi "sai thành chữ có thật" (`bài` gõ thành `bại`, `git` thành `it`): cần hiểu cả câu.
- Hai lỗi cùng lúc còn sai khoảng một phần tư, nên chỉ chạy với từ lạ và ngưỡng chặt.
- Từ lạ viết thường (tên riêng, thuật ngữ) đôi khi bị sửa nhầm thành từ phổ biến gần giống.
- Mô hình học sửa nhầm từ đúng nhiều hơn khi mức tin cậy thấp (0,9 cho nhiều sửa nhầm hơn 0,999); mặc định đã chọn mức cân bằng.
- Một lần sửa từ lạ mất khoảng 5–10 ms; từ phổ biến gần như tức thì.

## Cấu trúc mã nguồn

| Thư mục | Nội dung |
| --- | --- |
| `crates/ac-telex` | Phân tích âm tiết tiếng Việt và bộ gõ Telex (thuần hàm, có test) |
| `crates/ac-core` | Engine (bộ đệm từ, sửa khi gõ dấu cách, hoàn tác), `SmartCorrector`, mô hình học |
| `crates/ac-data` | Dựng bảng tần suất và cặp từ từ corpus |
| `crates/ac-config` | Đọc và ghi cài đặt, từ điển cá nhân, đường dẫn |
| `crates/ac-platform-win` | App `autocorrect`: hook bàn phím và chuột, `SendInput`, khay hệ thống, chính sách theo ứng dụng |
| `crates/ac-bench`, `ac-sim`, `ac-e2e` | Đo chất lượng và kiểm thử |
| `app/` | Cửa sổ Cài đặt (Tauri v2 + Svelte), là tiến trình riêng, sửa các file cấu hình trong `%APPDATA%\AutoCorrect` |
| `tools/` | Huấn luyện và thí nghiệm (Kaggle, mô hình học, bộ kết hợp) |

`ac-core`, `ac-telex`, `ac-data` và `ac-config` không phụ thuộc Windows; về sau có thể dùng cho nền tảng khác (Fcitx5/IBus trên Linux) với một frontend mới.

## Dữ liệu và ghi công

Bảng tần suất và cặp từ lấy từ các corpus công khai; xem [`data/ATTRIBUTION.md`](data/ATTRIBUTION.md).
