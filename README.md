# AutoCorrect

Realtime typo correction for every Windows app. Kế hoạch chi tiết: [PLAN.md](PLAN.md).

## Trạng thái: Phase 0 (spike)

- `crates/ac-core`: engine độc lập nền tảng (buffer từ, sửa khi gõ Space, Backspace để hoàn tác, giữ nguyên kiểu hoa/thường). Có unit test.
- `crates/ac-platform-win`: binary `ac-spike`, gồm hook `WH_KEYBOARD_LL`/`WH_MOUSE_LL`, inject bằng `SendInput` trên luồng riêng, log ra console.

Bộ sửa hiện chỉ là bảng typo nhỏ (`DictCorrector::builtin()`) để kiểm chứng pipeline.

## Chạy

```powershell
cargo test
cargo run --release --bin ac-spike                       # bình thường
cargo run --release --bin ac-spike -- --debug            # in mọi phím đã decode (chỉ ra console)
cargo run --release --bin ac-spike -- --normal-priority  # A/B: tắt ưu tiên cao cho luồng hook
```

**Tắt Unikey/EVKey** (hoặc chuyển sang chế độ E) trước khi test.

## Kịch bản test tay

| Gõ                                         | Kết quả mong đợi                                            |
| ------------------------------------------- | --------------------------------------------------------------- |
| `teh `                                    | `the `                                                        |
| `Recieve `                                | `Receive `                                                    |
| `TEH `                                    | `THE `                                                        |
| `dunhf `                                  | `dùng `                                                      |
| `tieengs vieetj `                         | `tiếng việt `                                               |
| `teh ` rồi Backspace ngay                | quay về`teh`, và từ này không bị sửa lại trong phiên |
| `te`, click chuột chỗ khác, gõ `h ` | không sửa (buffer đã reset)                                 |

Thử trong: Notepad, Windows Terminal (PowerShell), VS Code, Chrome, Zalo/Messenger, Word.

Console in mỗi lần sửa kèm độ trễ (p50/p99) và tên process. Nếu thấy `!! only x/y inputs accepted` thì app đó đã chặn `SendInput` (thường do app chạy quyền Admin).

## Giới hạn đã biết (để giải quyết ở Phase 1)

- Chỉ kích hoạt sửa bằng Space. Enter/Tab/dấu câu chỉ reset buffer.
- Chưa có bộ gõ Telex riêng, nên chưa chạy chung được với Unikey.
- Chưa phân biệt app (terminal/IDE/ô mật khẩu).
- Gõ cực nhanh ngay sau Space có thể chen vào trước khi inject xong (cần đo thực tế).
