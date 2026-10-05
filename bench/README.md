# Bộ ca kiểm thử thực tế (golden set)

`golden.tsv` gồm các ca có thật hoặc do người dùng báo. Chạy:

    cargo run -p ac-bench --release -- --golden            # bench/golden.tsv
    cargo run -p ac-bench --release -- --golden file.tsv

Mỗi dòng (cột cách nhau bằng Tab; dòng bắt đầu bằng `#` bị bỏ qua):

| Cột | Ý nghĩa |
|---|---|
| `id`, `group` | mã ca, nhóm (en-typo, en-keep, vi-telex, vi-keep, vi-bare, vi-name, code, mixed) |
| `mode` | `vi` (Việt + Anh), `en` (chỉ Anh), `code` (terminal/IDE: chỉ Việt, không thêm dấu) |
| `context` | các từ đã gõ trước đó, cách nhau bằng dấu cách (có thể trống) |
| `typed` | phím thật đã gõ (Telex ở chế độ Việt) |
| `expected` | chữ đúng, hoặc `=` nếu phải **giữ nguyên** |
| `level` | `must`: không bao giờ được hỏng; `goal`: đang theo dõi, được phép chưa qua |
| `source` | `user` (bạn báo), `list`, `synthetic`, sau này `journal` |
| `note` | ghi chú |

Chương trình thoát với mã 1 nếu có ca `must` trượt. Ca `goal` chỉ được in ra.
Thêm ca mới: nối dòng vào cuối file, giữ nguyên số cột.

## Xuất mẫu cho mô hình thầy (`--export`)

    cargo run -p ac-bench --release -- --export out.jsonl --sentences 3000

Mỗi dòng là một vị trí từ trong câu thật của kho công khai (tin tức, web, phụ đề), với lỗi mô phỏng.
Không chứa dữ liệu của người dùng. Trường chính:

| Trường | Ý nghĩa |
|---|---|
| `set` | `vi` (tin tức/web), `vd` (hội thoại phụ đề), `en` |
| `context` | tối đa 3 từ **bên trái** (thứ app nhìn thấy) |
| `right` | tối đa 3 từ **bên phải** (chỉ dành cho thầy, trò không được thấy) |
| `typed`, `shown` | phím đã gõ, và chữ hiện trên màn hình |
| `truth`, `class` | đáp án; `clean`, `one-slip`, `two-slips`, `no-marks` |
| `real` | chữ đã gõ tự nó là một từ hợp lệ |
| `candidates` | tối đa 8 ứng viên của bộ sửa lỗi kèm điểm `s`, cộng một ứng viên `keep` (giữ nguyên) |
| `truth_in` | đáp án có nằm trong danh sách ứng viên hay không |

`--sentences N` chọn số câu mỗi tập; số mẫu khoảng 40 lần số câu cho tiếng Việt.

## Nhật ký học (`journal.tsv`)

Mỗi dòng: `giây_unix<Tab>loại<Tab>...`. Có sáu loại (`FIX`, `LATE`, `UNDO`, `UNDO-LATE`, `EDIT`, `NEAR`); `EDIT` và `NEAR` mặc định **tắt**, bật riêng trong tray hoặc cửa sổ cài đặt.

Các cột sau `loại` (từ 2026-10-05; dòng cũ chỉ có sáu cột đầu, hai cột cuối trống):

| # | Cột | Nội dung |
|---|---|---|
| 1 | `keys` | phím gõ (`EDIT`: chữ trước khi sửa) |
| 2 | `fix` | chữ sửa thành (`EDIT`: chữ bạn sửa thành; `NEAR`: ghi chú điểm các ứng viên gần nhau) |
| 3 | `context` | từ ngay trước |
| 4 | `mode` | `vi` hoặc `en` |
| 5 | `app` | nhóm app (`Normal`, `Code`) |
| 6 | `left` | **mọi** từ trước mà bộ sửa thấy, cũ nhất trước, cách nhau bằng dấu cách (tối đa 4 từ) |
| 7 | `right` | tối đa 3 từ gõ **sau đó**, như đang hiện trên màn hình; để trống nếu `journal_right` tắt hoặc không có từ nào theo sau |

| Loại | Khi nào ghi |
|---|---|
| `FIX` | App tự sửa một từ |
| `LATE` | Sửa muộn: từ trước được sửa lại khi đã có từ sau (`right` bắt đầu bằng từ sau đó) |
| `UNDO`, `UNDO-LATE` | Bạn hoàn tác (Ctrl+Z) lần sửa đó |
| `EDIT` | Bạn xóa lùi vào một từ rồi sửa tay (chỉ khi sửa nhỏ, tối đa 3 ký tự) |
| `NEAR` | App bỏ qua một từ nhưng có ứng viên gần nhau |

**Ngữ cảnh phải (`journal_right`, mặc định tắt):** lúc app sửa một từ thì các từ sau chưa tồn tại, nên dòng đó được giữ lại và chỉ ghi khi đã gõ xong 3 từ tiếp theo. Dòng được ghi sớm hơn khi: có dấu chấm, chấm hỏi hoặc chấm than (câu khác), Enter, click chuột, đổi cửa sổ hoặc vào ô mật khẩu, tạm dừng gõ quá 60 giây (kiểm tra ở phím kế tiếp), có hơn 8 dòng đang chờ, hoặc app thoát. Thời điểm ở đầu dòng là lúc ghi, không phải lúc sửa. Bật nó thì nhật ký chứa nhiều văn bản bạn gõ hơn.

`EDIT` và `NEAR` không ghi trong terminal/IDE và ô mật khẩu; `NEAR` chỉ ghi khi gõ Telex. File dừng tăng ở 8 MB.
`ac-bench --from-journal` biến `FIX`/`UNDO`/`EDIT` thành ca kiểm thử: cột `context` của ca là `left` (đủ các từ trước), thêm cột cuối `right` (`EDIT` thành ca mục tiêu có đáp án là chữ bạn sửa).
