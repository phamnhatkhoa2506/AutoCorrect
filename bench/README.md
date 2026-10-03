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
