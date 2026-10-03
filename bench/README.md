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
