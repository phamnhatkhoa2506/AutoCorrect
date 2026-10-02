# AutoCorrect: Kế hoạch & Kiến trúc

> App chạy nền trên Windows, sửa lỗi gõ **realtime** trong mọi ứng dụng (kể cả terminal và IDE), hỗ trợ **tiếng Việt (Telex/VNI)** và **tiếng Anh**, chạy hoàn toàn **offline**.

Ví dụ mục tiêu:

| Gõ (phím thô) | Hiện ra (lỗi) | Sửa thành |
|---|---|---|
| `dunhf` | dunhf | dùng |
| `nhạn` (thiếu `h`) | nhạn | nhanh *(tùy ngữ cảnh)* |
| `tieengs` | tieengs | tiếng |
| `teh` | teh | the |
| `recieve` | recieve | receive |

---

## 1. Nguyên tắc thiết kế

1. **Sửa nhầm còn tệ hơn không sửa.** Ưu tiên *precision* hơn *recall*. Chỉ sửa khi rất chắc chắn.
2. **Độ trễ không được cảm nhận thấy.** Sửa trong khoảng < 20 ms (p99) sau khi người dùng gõ xong một từ.
3. **Offline 100%.** Không gửi phím gõ ra ngoài. Đây là yêu cầu sống còn vì app hoạt động giống keylogger.
4. **Hoàn tác tức thì.** Bấm Backspace ngay sau khi bị sửa thì trả lại nguyên văn, và app ghi nhớ điều này.
5. **Nhận biết ngữ cảnh ứng dụng.** Terminal, IDE, ô mật khẩu và game có chính sách sửa riêng.

---

## 2. Kiến trúc tổng thể

```
┌───────────────────────────── Process: autocorrect.exe (Rust) ─────────────────────────────┐
│                                                                                            │
│  [Hook thread]                [Engine thread]                        [Injector]            │
│  WH_KEYBOARD_LL  ──keys──▶  Input Buffer / Telex Decoder  ──▶  Correction Pipeline  ──▶  SendInput
│  (trả về < 1ms)  (channel)        │                              │                         │
│                                   ▼                              ▼                         │
│                          Context Tracker                 Candidate Gen → Ranker → Gate     │
│                          (app đang focus, ô mật khẩu,    (SymSpell + mô hình lỗi Telex,    │
│                           chuột/phím mũi tên → reset)     n-gram LM → ONNX model)          │
│                                                                                            │
│  [Personal store]  từ điển cá nhân, thống kê undo, whitelist/blacklist theo app (SQLite)   │
└──────────────────────────────────────────────▲─────────────────────────────────────────────┘
                                               │ IPC (Tauri commands)
                               ┌───────────────┴───────────────┐
                               │  UI: Tauri + Svelte           │
                               │  tray icon, settings, thống kê │
                               └───────────────────────────────┘
```

### 2.1 Luồng xử lý một từ

1. **Hook** nhận phím, đẩy vào channel rồi trả về ngay. Windows sẽ tự gỡ hook nếu callback chạy quá `LowLevelHooksTimeout` (khoảng 300 ms), nên trong hook tuyệt đối không xử lý nặng.
2. **Input Buffer** giữ chuỗi phím thô của từ hiện tại, ví dụ `d u n h f`.
3. Gặp **ranh giới từ** (Space, Enter, dấu câu) thì đưa từ vào pipeline.
4. **Pipeline**:
   - *Fast path*: từ đã hợp lệ (âm tiết tiếng Việt đúng chính tả hoặc có trong từ điển) thì bỏ qua. Khoảng 95% trường hợp dừng ở đây, mất dưới 50 µs.
   - *Candidate generation*: sinh các ứng viên sửa.
   - *Ranking*: chấm điểm ứng viên theo ngữ cảnh (2–3 từ trước đó).
   - *Gate*: chỉ sửa khi `score(best) - score(original) > θ_app`, trong đó ngưỡng θ phụ thuộc app đang dùng.
5. **Injector** gửi N lần Backspace rồi gõ chuỗi Unicode mới, sau đó gõ lại ký tự ranh giới.
6. Ghi lại `last_correction` để hỗ trợ hoàn tác.

### 2.2 Chạy cùng bộ gõ tiếng Việt: quyết định quan trọng nhất

| Phương án | Ưu | Nhược |
|---|---|---|
| **A. Tự làm bộ gõ Telex/VNI có tích hợp autocorrect** (giống OpenKey/EVKey) | Thấy được phím thô nên sửa được lỗi kiểu `dunhf`; kiểm soát được hoàn toàn | Người dùng phải bỏ Unikey; phải tự làm bộ gõ cho tốt |
| B. Chạy song song Unikey, chỉ đọc kết quả cuối | Người dùng không phải đổi thói quen | Không thấy phím thô, buffer khó đồng bộ, hai hook đánh nhau |

**Chọn A.** Bộ gõ Telex là state machine khá nhỏ (khoảng 1–2 nghìn dòng). Có thể tham khảo mã nguồn mở của **OpenKey** (C++, GPL) và **bamboo-core** (Go, ibus-bamboo) để biết các luật và edge case. Cần lưu ý license nếu copy code; tốt nhất là tự viết lại.

Bộ gõ vẫn có chế độ "English/Off" (Ctrl+Shift) như Unikey.

---

## 3. Stack công nghệ

### 3.1 Core: **Rust**

| Thành phần | Thư viện | Lý do chọn |
|---|---|---|
| Win32 API (hook, SendInput, focus) | [`windows`](https://crates.io/crates/windows) (crate chính thức của Microsoft) | Binding đầy đủ, zero-cost |
| Đa luồng / channel | `crossbeam-channel` | Nhanh, có `select!` |
| Spell candidates | [`symspell`](https://crates.io/crates/symspell) hoặc tự cài đặt | Thuật toán Symmetric Delete, lookup khoảng 1 µs |
| N-gram LM | Tự cài đặt (bảng hash + Kneser-Ney), hoặc đọc file ARPA/KenLM | Nhẹ, xử lý ngữ cảnh 2–3 từ |
| Neural ranker | [`ort`](https://crates.io/crates/ort) (ONNX Runtime) | Chạy model int8 trên CPU, khoảng 2–5 ms |
| Lưu trữ cá nhân | `rusqlite` | Từ điển cá nhân, thống kê |
| Unicode | `unicode-normalization` | Chuẩn hóa NFC (dựng sẵn) để tránh lỗi tổ hợp dấu |
| Log/trace | `tracing` | Đo latency từng bước |

**Vì sao chọn Rust?** Độ trễ ổn định vì không có GC pause trong hook. Binary nhỏ, an toàn bộ nhớ. Bạn cũng đã quen stack Rust/Tauri từ dự án snip-ai.

### 3.2 UI: **Tauri v2 + Svelte**

- Tray icon: bật/tắt, chọn VN/EN, tạm dừng 10 phút.
- Settings: độ "mạnh tay" khi sửa (Conservative/Balanced/Aggressive), whitelist/blacklist app, từ điển cá nhân.
- Lịch sử sửa: xem lại và đánh dấu "sửa sai" để huấn luyện lại.
- Tauri chỉ là lớp vỏ. Core engine chạy độc lập trong cùng process.

### 3.3 Training/research: **Python**

| Công cụ | Dùng để |
|---|---|
| PyTorch | Huấn luyện ranker |
| HuggingFace `transformers` / `datasets` | Lấy model nền byte-level và quản lý dữ liệu |
| KenLM (`lmplz`) | Build n-gram LM từ corpus |
| `optimum` / `onnxruntime` | Export và quantize int8 sang ONNX |
| `underthesea` / `pyvi` | Tách từ, tiền xử lý tiếng Việt |

---

## 4. Các mô hình (theo tầng, từ rẻ đến đắt)

### Tầng 0: Kiểm tra hợp lệ (luật, khoảng 0 ms)
- **Bộ kiểm tra âm tiết tiếng Việt**: âm tiết = phụ âm đầu + vần + thanh. Tập âm tiết hợp lệ chỉ khoảng 7–8 nghìn, nên dùng bảng tra hoặc FSA là đủ.
- **Từ điển tiếng Anh**: khoảng 100 nghìn từ, tần suất lấy từ `wordfreq` hoặc SCOWL.
- **Từ điển kỹ thuật**: lệnh terminal (`git`, `npm`, `cd`, `ls`, `kubectl`...), keyword lập trình, tên file. Lấy tự động thêm từ `PATH` và lịch sử shell.

### Tầng 1: Sinh ứng viên (khoảng 0.1 ms)
- **SymSpell** với edit distance ≤ 2 cho tiếng Anh.
- **Mô hình lỗi Telex (keystroke-level)**. Đây là đóng góp chính của dự án:
  - *Đảo thứ tự phím*: `dunhf` ↔ `dungf`, `nhnah` ↔ `nhanh`
  - *Thiếu hoặc thừa phím*: `nhạn` ← `nhanj` (đúng ra `nhanhj`)
  - *Lỡ tay sang phím bên cạnh* theo layout QWERTY: `g`↔`h`, `f`↔`d`
  - *Đặt dấu sai vị trí hoặc gõ dấu 2 lần*: `tieengss`
  - *Quên bật bộ gõ*: `tieengs vieetj` ở chế độ EN thì chuyển thành `tiếng việt`
  - Mỗi phép biến đổi có xác suất P(lỗi | ý định), ước lượng từ dữ liệu thật thu thập được qua opt-in.

### Tầng 2: Xếp hạng n-gram (khoảng 0.5 ms)
- **KenLM 3-gram mức âm tiết** cho tiếng Việt, 3-gram mức từ cho tiếng Anh.
- Điểm = `log P_LM(candidate | context) + log P_err(typed | candidate)`. Đây là *noisy channel model* kinh điển.
- Corpus: Wikipedia tiếng Việt, [Binhvq News Corpus](https://github.com/binhvq/news-corpus), phần tiếng Việt của CulturaX/OSCAR. Cần prune để model chỉ còn khoảng 50–150 MB.

### Tầng 3: Neural ranker (khoảng 2–5 ms, V2)
- **Model byte-level hoặc char-level nhỏ** (Transformer encoder, 5–20 triệu tham số), int8 ONNX.
  - Input: `[ngữ cảnh 32 ký tự trước] <sep> [từ đã gõ, phím thô] <sep> [ứng viên]`. Output: điểm.
  - Chỉ chấm lại top-5 ứng viên từ tầng 2, **không sinh tự do**, nên không bị hallucinate.
  - Có thể distill từ **ByT5-small** đã fine-tune sửa lỗi, hoặc train từ đầu trên dữ liệu tổng hợp.
- Lý do dùng byte/char-level: lỗi gõ nằm ở mức ký tự và phím, nên tokenizer BPE sẽ làm mất thông tin.

### Tầng 4: Sửa cả câu bằng LLM (on-demand, không realtime)
- Bôi đen rồi nhấn hotkey (vd `Ctrl+Alt+F`) để sửa chính tả và ngữ pháp cả đoạn.
- Mặc định chạy local qua **llama.cpp**, dùng LLM nhỏ khoảng 0.5–3 tỷ tham số, quantize Q4.
- Tùy chọn cloud (Claude Haiku 4.5) nếu người dùng tự bật và nhập API key.
- Tách riêng khỏi pipeline realtime.

---

## 5. Nhận biết ngữ cảnh ứng dụng

| Tín hiệu | API |
|---|---|
| App đang focus | `GetForegroundWindow` → `GetWindowThreadProcessId` → tên process |
| Ô mật khẩu | UI Automation `IsPassword` (gọi async và cache theo HWND) |
| Vị trí con trỏ thay đổi do click hoặc phím mũi tên | Hook chuột `WH_MOUSE_LL` và phím điều hướng → **reset buffer** |

Chính sách mặc định theo app:

| Loại | Ví dụ | Chế độ |
|---|---|---|
| Chat / văn bản | Messenger, Zalo, Word, trình duyệt | Balanced |
| Terminal | Windows Terminal, conhost, PowerShell | Conservative: chỉ sửa lỗi tiếng Việt rõ ràng, không đụng token có `-`, `/`, `.`, `_` |
| IDE | VS Code, JetBrains | Chỉ sửa trong comment/string *(V3, khó)*, mặc định tắt |
| Game, ô mật khẩu, remote desktop | | **Tắt** |

---

## 6. Các chi tiết kỹ thuật dễ sai

- **Bỏ qua phím do chính app gửi**: event có cờ `LLKHF_INJECTED` và `dwExtraInfo` mang một magic number riêng. Thiếu bước này app sẽ tự sửa vòng lặp.
- **Unicode injection**: dùng `KEYEVENTF_UNICODE` và gửi chuỗi đã chuẩn hóa NFC.
- **App không nhận SendInput**, ví dụ app chạy quyền admin trong khi app mình chạy user thường (UIPI): phát hiện được thì tắt sửa trong app đó và báo cho người dùng.
- **Buffer lệch** do autocomplete của IDE, paste, hoặc app tự sửa: chỉ giữ buffer của *từ hiện tại* cộng vài từ ngữ cảnh, reset mạnh tay khi có bất kỳ tín hiệu lạ nào.
- **Antivirus**: ký số binary (code signing), mã nguồn mở, không có network trong core, có trang giải thích quyền riêng tư.

### 6.1 Bài học từ Phase 0 (đã kiểm chứng)

- **Không gọi `SendInput` trong hook callback.** Nó phải chờ chính hook đó xử lý, đo được 20–50 ms. Đã chuyển sang luồng inject riêng.
- **`SendInput` vẫn tốn thời gian theo số phím gửi đi**, vì mỗi phím đi qua mọi hook bàn phím trên máy. Vì vậy chỉ xóa và gõ lại phần khác nhau giữa từ cũ và từ mới (`Recieve`→`Receive` = 4 Backspace + `eive `).
- **`GetKeyState(VK_CAPITAL)` trong hook trả về giá trị cũ** vì nó đọc bảng trạng thái phím của chính luồng mình. App phải tự theo dõi CapsLock qua các lần bấm phím.
- **Autocorrect riêng của từng app sẽ "sửa ngược" kết quả của mình.** Notepad trên Win11 đổi `dùng` thành `dung`, kể cả khi autocorrect của Windows đã tắt. Hướng xử lý:
  - Lập danh sách các app có autocorrect riêng (Notepad, Word, Outlook...) và hướng dẫn người dùng tắt.
  - Phát hiện khi chữ bị đổi lại sau khi mình sửa (đọc lại qua UI Automation), rồi cảnh báo hoặc tự tắt sửa trong app đó.
- **Hoàn tác không được dùng Backspace**: lùi bằng Backspace để sửa chữ cũ là thao tác thường xuyên nhất, và cả "một Backspace" lẫn "hai Backspace" đều đụng vào nó. Hoàn tác là **Ctrl+Z**, chỉ bị app bắt khi vừa có một bản sửa chờ (chưa gõ phím nào khác); lúc khác Ctrl+Z đi thẳng vào app. Phải nhả Ctrl khi gửi phím thay thế, nếu không Backspace thành Ctrl+Backspace. Một từ bị hoàn tác 2 lần thì bỏ qua cả phiên.
- **Backspace lùi qua dấu cách vào từ trước**: phải khôi phục từ trước vào buffer. Nếu lùi vào đoạn chữ không biết thì đánh dấu `Untracked` và không sửa, vì buffer có thể chỉ chứa phần đuôi của từ trên màn hình (`xteh` có thể bị sửa thành `xthe`).
- **Sự kiện foreground của WinEvent có thể đến sai thứ tự** (bấm taskbar báo `explorer.exe` sau app thật): hook tự phân loại lại cửa sổ thật mỗi khi foreground đổi.
- **"Hiện mật khẩu" biến ô mật khẩu thành ô chữ thường**: nhớ các ô từng bị che (process + AutomationId + tên) và nhận diện theo nhãn (password, pwd, mật khẩu…).
- **Phím hủy dấu không liền kề** (`rece` → `rêc` rồi thêm `e`): hiện lại đúng mọi phím đã gõ, không làm mất chữ.
- **Gửi phím thay thế trên luồng riêng làm phím gõ tiếp vượt mặt nó**: lỗi nhân đôi chữ (`toôi`) xuất hiện lúc máy bận ngay sau khi khởi động, rồi tự hết. `SendInput` giờ chạy ngay trong hàm bắt phím, nên phím thật gõ tiếp luôn xếp hàng sau phím của app (đổi lại hàm bắt phím chặn ~15–40 ms mỗi lần thay).
- **Chặn "có thể là ô mật khẩu" quá lâu làm rơi phím đầu của từ**: UI Automation trong Edge hoặc lúc mới khởi động trả lời sau cả trăm ms đến vài giây, và Alt hay bấm taskbar cũng bắn sự kiện focus. Giờ chỉ chặn tối đa 120 ms khi chưa có câu trả lời (đã biết là ô mật khẩu thì vẫn chặn đến khi rời), và câu trả lời của focus cũ bị bỏ.
- **`Untracked` quá hay bị kích hoạt**: mọi Backspace khi bộ đệm rỗng bị coi là xóa chữ lạ. Giờ engine đếm số ký tự đã gõ từ lần reset cuối; xóa đúng bằng ấy (hoặc Ctrl+A rồi Backspace) thì con trỏ về chỗ bắt đầu và từ kế tiếp được coi là mới.
- **Trong lúc chờ kiểm tra mật khẩu, phím phải được theo dõi chứ không bỏ**: engine có chế độ `observing` (phím đi nguyên vẹn, không Telex, không sửa, nhưng vẫn ghi vào bộ đệm), nên từ gõ ngay sau khi chuyển cửa sổ vẫn được sửa ở dấu cách kế tiếp; ô mật khẩu đã biết thì vẫn không ghi gì.
- **Nhóm IDE/terminal mặc định không sửa tiếng Anh** (để khỏi làm hỏng code), nhưng khung chat trong IDE là văn bản thường: có tùy chọn `code_english` ở menu khay.
- **Gợi ý tự điền trong ô tìm kiếm/thanh địa chỉ trình duyệt làm Backspace đầu tiên xóa nhầm phần gợi ý** (`to` + gợi ý bôi đen `ols`: Backspace chỉ xóa `ols`, rồi `ô` thành `toô`). Xuất hiện ngẫu nhiên vì gợi ý hiện sau một thoáng. Cách xử lý như các bộ gõ khác: gõ một ký tự rỗng (U+202F) trước và xóa thêm một Backspace; áp dụng cho trình duyệt và ô tìm kiếm Windows, có công tắc trong menu khay.
- **Không tự động test bằng cách giả lập gõ phím lên desktop thật**, vì phím có thể rơi vào cửa sổ khác. Test E2E phải dùng một cửa sổ test riêng do chính app tạo, và kiểm tra đúng cửa sổ đó đang được focus trước mỗi lần gửi phím.

---

## 7. Dữ liệu & đánh giá

### 7.1 Dữ liệu
1. **Corpus sạch**: tiếng Việt và tiếng Anh như mục 4.
2. **Lỗi tổng hợp**: chuyển văn bản sạch thành chuỗi phím Telex, rồi áp dụng nhiễu theo mô hình lỗi ở tầng 1. Làm vậy sinh được hàng trăm triệu cặp (lỗi, đúng).
3. **Lỗi thật (opt-in)**: log ẩn danh các cặp (phím thô, chữ sau khi người dùng tự sửa bằng backspace). Đây là nguồn dữ liệu quý nhất, chỉ lưu local, người dùng tự chọn xuất ra.

### 7.2 Metrics
| Metric | Mục tiêu MVP |
|---|---|
| **False correction rate** (sửa từ đúng thành sai) | < 0.1% số từ |
| Correction precision | > 95% |
| Correction recall (trên tập lỗi Telex) | > 60% |
| Latency p99 (từ phím ranh giới đến khi inject xong) | < 20 ms |
| Undo rate ngoài thực tế | < 5% số lần sửa |
| RAM | < 150 MB (MVP), < 300 MB (có neural) |

Cần bộ **benchmark offline** (replay chuỗi phím rồi so kết quả) để mọi thay đổi model đều đo được, tránh đánh giá cảm tính.

---

## 8. Lộ trình

### Phase 0: Spike (1 tuần)
- [x] Hook bàn phím, log phím, inject khi gõ sai, đo latency.
- [x] Kiểm chứng trên Notepad: sửa đúng chữ hoa/thường và tiếng Việt có dấu, sau khi tắt autocorrect của Notepad.
- [x] Kiểm chứng SendInput chạy trong Windows Terminal, Antigravity IDE (VS Code fork), Edge.
- [ ] Kiểm chứng thêm Zalo, Word.
- [ ] Latency: hiện 15–45 ms, gần như toàn bộ nằm trong `SendInput` (khoảng 1,5–2 ms mỗi phím gửi đi, do Windows chuyển từng phím qua các hook trên máy). Tăng ưu tiên luồng hook không giúp. Để Phase 1 xử lý bằng cách giảm số phím phải gửi, khi đã có bộ gõ riêng.

### Phase 1: MVP (3–4 tuần)
- [x] 1.1 Bộ kiểm tra âm tiết + bộ gõ Telex dạng hàm thuần (`ac-telex`). VNI làm sau.
- [x] 1.3 Lexicon: Leipzig (tin tức 2022, web 2015) + FrequencyWords, gộp theo tần suất tương đối → 6.795 âm tiết, 46.693 từ tiếng Anh (`ac-data`).
- [x] 1.3 Noisy-channel corrector trên phím thô, cả Việt (qua Telex) lẫn Anh, mô hình lỗi: đảo phím, lệch phím QWERTY, nhầm thanh, thiếu/thừa phím dấu, gõ đôi, nhầm nguyên âm. Thay cho SymSpell.
  - Chữ phổ biến (≥ 1000 lần mỗi tỷ từ) không bao giờ bị sửa; âm tiết hợp lệ chỉ được sửa dấu; tên riêng viết hoa đầu được giữ nguyên.
  - Tốc độ: chữ đúng ~4 µs, lỗi gõ 0,2–1,4 ms.
  - Chưa làm: lỗi cách 2 bước (`khogn`), chỉnh chi phí theo dữ liệu thật.
- [x] 1.2 Gắn bộ gõ Telex vào hook: engine giữ cả phím thô lẫn chữ trên màn hình, chỉ gõ lại khi bộ gõ đổi chữ (phím thường đi thẳng), Backspace tính lại phím thô bằng `to_keys`, Alt+Z chuyển Việt/Anh.
- [x] Gate theo app: terminal/IDE chỉ sửa tiếng Việt; ô mật khẩu (UI Automation, chạy trên luồng riêng, mặc định chặn cho tới khi biết chắc) và app nhạy cảm thì tắt hẳn.
- [x] Hoàn tác bằng Backspace, reset buffer khi click hoặc dùng phím mũi tên.
- [x] Tray icon bằng Win32 thuần (không cần Tauri): icon V/E vẽ bằng GDI, menu, lưu cài đặt, khởi động cùng Windows, chỉ chạy 1 bản.
- [ ] Benchmark replay và các metric ở mục 7.

### Phase 2: Ngữ cảnh (3–4 tuần)
- [x] Ngữ cảnh từ đứng trước: bảng bigram (2,5 MB Việt, 1,6 MB Anh) từ câu Leipzig, trộn với tần suất đơn từ (`Tuning::bigram_weight`), cộng lực nhất quán ngôn ngữ. Dùng bảng cặp từ thay KenLM 3-gram: đủ cho hiệu quả cần có, nhỏ và đọc thẳng trong bộ nhớ.
- [x] Benchmark `ac-bench` (câu giữ lại + lỗi nhân tạo, A/B có/không ngữ cảnh, dò ngưỡng); dùng nó để chọn các ngưỡng mặc định.
- [x] Lỗi hai bước (`khogn` → `không`), danh sách lỗi chính tả phổ biến, tên riêng giữa câu.
- [x] Nhật ký sửa lỗi tùy chọn (local) + báo cáo `ac-bench --journal`, để lấy dữ liệu thật.
- [ ] Sửa "sai thành chữ có thật" bằng ngữ cảnh (43% lỗi gõ tiếng Việt rơi vào đây): cần ngưỡng rất chặt và đo trên benchmark trước khi bật.
- [ ] Khớp chi phí các kiểu gõ nhầm (`crates/ac-core/src/edits.rs`) với nhật ký thật khi đã đủ dữ liệu.
- [ ] Từ điển cá nhân, tự học từ undo (từ bị undo 2 lần thì không sửa nữa).
- [ ] UI settings đầy đủ, lịch sử sửa.

### Phase 3: Neural (4–6 tuần, phần nghiên cứu)
- [ ] Sinh dữ liệu tổng hợp, train ranker byte-level, export sang ONNX int8.
- [ ] So sánh với n-gram trên benchmark, công bố kết quả (blog/paper).
- [ ] Thu thập lỗi thật qua opt-in và fine-tune.

### Phase 4: Mở rộng
- [ ] Sửa cả câu bằng LLM qua hotkey.
- [ ] macOS: `CGEventTap` + Input Method Kit. Core Rust dùng lại được, chỉ thay lớp platform.
- [ ] Linux: ibus/fcitx5 engine.

---

## 9. Cấu trúc repo dự kiến

```
AutoCorrect/
├── crates/
│   ├── ac-core/          # pipeline: buffer, candidates, ranking, gate (platform-agnostic, có unit test)
│   ├── ac-telex/         # bộ gõ Telex/VNI + bộ kiểm tra âm tiết
│   ├── ac-lm/            # n-gram LM + ONNX ranker
│   ├── ac-platform-win/  # hook, SendInput, focus/UIA
│   └── ac-bench/         # replay benchmark
├── app/                  # Tauri + Svelte (tray, settings)
├── research/             # Python: data gen, training, export ONNX, notebooks
├── data/                 # từ điển, LM (Git LFS hoặc tải lúc build)
└── PLAN.md
```

Tách `ac-core` khỏi platform để test toàn bộ logic sửa lỗi mà không cần hook thật, và để port sang macOS/Linux sau này.

---

## 10. Rủi ro chính

| Rủi ro | Giảm thiểu |
|---|---|
| Sửa nhầm khiến người dùng bỏ app | Ngưỡng bảo thủ, hoàn tác 1 phím, tự học từ undo |
| Antivirus coi là keylogger | Ký số, mã nguồn mở, không có network trong core |
| Xung đột với Unikey/EVKey | Phát hiện và cảnh báo, hướng dẫn tắt; app có bộ gõ riêng |
| Buffer lệch gây sửa sai chỗ | Reset mạnh tay, chỉ sửa từ vừa gõ xong |
| Model nặng làm tăng latency | Kiến trúc nhiều tầng, neural chỉ chấm lại top-k và chỉ chạy khi tầng dưới không chắc chắn |
