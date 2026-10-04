# Các vấn đề đang gặp

File này liệt kê những gì app đang làm chưa đúng. Mỗi lần gặp thêm một vấn đề, thêm vào cuối; dưới mỗi vấn đề có thể ghi **giải pháp có thể** (nếu có). Giải pháp ghi ở đây là **giả thuyết**, chưa kiểm chứng, trừ khi có ghi "đã sửa".

**Trạng thái:** `Mở` (chưa làm gì) · `Cần làm rõ` (chưa hiểu đủ để làm) · `Đang điều tra` · `Ý tưởng` (chưa phải lỗi) · `Đã sửa` (có commit)

**Mẫu để thêm vấn đề mới:**

```
## #N. Tiêu đề ngắn
Trạng thái: Mở | Nguồn: bạn báo / log / benchmark | Ngày: YYYY-MM-DD
Mô tả: ...
Bằng chứng: ...
Giải pháp có thể: ...
```

| # | Vấn đề | Trạng thái |
|---|---|---|
| 1 | Gõ chữ có `ff` (ví dụ `off`) trong chế độ tiếng Việt cho kết quả lạ | Cần làm rõ |
| 2 | `ngượi` không thành `người` | Mở |
| 3 | `đok` quay về `ddok`, và `do` thắng `đo` | Mở |
| 4 | `that` (tiếng Anh) được đưa ra thay cho `thật` ở nhiều trường hợp | Mở |
| 5 | `thicsk` không thành `thích` (thêm một ký tự thì không sửa được) | Mở |
| 6 | Từ có chữ hoa lẫn (`ĐIểm`) không được sửa | Mở |
| 7 | Sửa từ vựng, ngữ pháp theo ngữ cảnh | Ý tưởng |
| 8 | Terminal: phân biệt lệnh với văn xuôi | Cần làm rõ |
| 9 | Mất ngữ cảnh giữa hai từ ở terminal | Đang điều tra |
| 10 | Thỉnh thoảng gõ tiếng Việt không được một lúc | Đang điều tra |
| 11 | Cơ chế guard chống sửa sai | Ý tưởng |
| 12 | Giả thuyết: bước sinh ứng viên bỏ sót từ đúng (`that là`, `ngẫy nhiên` đã bị bác bỏ) | Cần làm rõ |
| 13 | Notepad bản mới: Autocorrect/Spell check làm hỏng chữ app chèn | Đã giải quyết (bằng cấu hình) |
| 14 | Chữ viết tắt in hoa và tên riêng bị đổi nhầm (`CPI` → `COI`, `Xavi` → `Xạ`) | Mở |
| 15 | Tiếng Anh gõ ở chế độ Việt không bao giờ được sửa lại (`of` → `ò`) | Mở |
| 16 | Sửa muộn đổi `that` trong câu tiếng Anh thành `thật` | Mở |
| 17 | Thiếu dấu cách giữa hai hoặc nhiều từ (`quanheej` → `quan hệ`) | Đang sửa (bản đầu có, sửa đúng 53% với 2 từ) |

---

## #1. Gõ chữ có `ff` (ví dụ `off`) trong chế độ tiếng Việt cho kết quả lạ
Trạng thái: Cần làm rõ | Nguồn: ghi chú của bạn (mục 1 và 3 trong `notes/điểm cần cải thiện.txt`)

**Mô tả:** Ghi chú gốc chỉ ghi `"off -> fff"`. Mình **chưa hiểu chắc** bạn muốn gõ gì và thấy gì (cần bạn bổ sung).

**Bằng chứng (từ hai đoạn log trong ghi chú):** hai lần cùng một chuỗi thao tác: gõ `r` (hoặc `b` do PowerToys chèn) + `o` + `f` thành `rò`/`bò`, nhấn Space, nhấn Backspace để nối lại từ, gõ `f` lần nữa (phím dấu lặp nên hủy dấu, ra `rof`/`bof`, phím thô `roff`/`boff`), nhấn Space, và app sửa `rof`/`bof` thành `off`.

**Giải pháp có thể:**
- Hỏi lại bạn ý định: nếu bạn muốn gõ `off` thì việc app sửa ra `off` có thể là đúng và vấn đề nằm ở chỗ khác (chữ `f` bị Telex dùng làm dấu huyền nên chữ `ff` bị rút xuống một `f`).
- Telex dùng `f` làm dấu huyền, nên từ tiếng Anh có `ff`, `ss`, `rr`, `jj`, `xx` (off, staff, coffee, address...) rất dễ bị biến dạng ở chế độ tiếng Việt. Có thể thêm nhận diện "từ tiếng Anh phổ biến chứa các cặp này" để không áp Telex lên chúng.

---

## #2. `ngượi` không thành `người`
Trạng thái: Mở | Nguồn: ghi chú của bạn (mục 2)

**Mô tả:** Trong câu "ngày sửa ngày xưa có một ngượi", từ cuối không được sửa thành `người`.

**Giải pháp có thể:**
- Có thể liên quan đến luật hiện tại: với âm tiết đã hợp lệ, app chỉ cho đổi **loại dấu** trong một số cặp (hỏi/ngã, ô/ơ) và không cho đổi sang dấu thanh khác, vì dấu đã gõ được coi là cố ý. Đổi dấu nặng sang dấu huyền (`ngượi` → `người`) vì thế bị chặn. Cần kiểm tra lại có đúng vậy không.
- Cho phép đổi dấu thanh khi ngữ cảnh bên trái rất mạnh ("có một người"), và đo bằng benchmark (chế độ ứng viên mở rộng).
- Từ này đứng cuối câu nên chưa có từ phía phải; sửa muộn chỉ giúp khi gõ thêm từ sau.

---

## #3. `đok` quay về `ddok`, và `do` thắng `đo`
Trạng thái: Mở | Nguồn: ghi chú của bạn (mục 4)

**Mô tả:** Gõ `d`, `d` ra `đ`, `o` ra `đo`, rồi gõ thêm `k`: app thay cả từ bằng chữ thô `ddok`. Khi nhấn Space, từ `ddok` không có ứng viên đủ chắc (log: `typed -inf; top: do 8.0, đô 7.1, đo 6.6`), tức `do` đứng trên `đo`.

**Giải pháp có thể:**
- Khi từ không còn hợp lệ, **giữ phần đầu đã ghép được** (`đ`) và để phần sau là chữ thô (`đok`) thay vì trả cả từ về phím gõ thô, để người dùng không thấy `đ` biến mất.
- Chi phí bỏ một phím dấu cố ý (`dd`) nên cao hơn chi phí bỏ một chữ lạc (`k`). Hiện `do` thắng `đo`, gợi ý chi phí này chưa đúng. Cần đo trước khi đổi.

---

## #4. `that` (tiếng Anh) được đưa ra thay cho `thật` ở nhiều trường hợp
Trạng thái: Mở | Nguồn: ghi chú của bạn (mục 5); nhật ký có dòng `FIX thata → that` (ngữ cảnh trống, ngày 2026-10-04)

**Mô tả:** Từ tiếng Anh `that` rất phổ biến và chỉ cách nhiều phím gõ tiếng Việt một lỗi (`thata`, `thaat`...). Khi không có ngữ cảnh bên trái (đầu câu), ứng viên tiếng Anh dễ thắng.

**Giải pháp có thể:**
- Khi đang ở chế độ tiếng Việt, giảm điểm ứng viên tiếng Anh nếu không có ngữ cảnh tiếng Anh ở bên trái. Cần đo tác động lên người gõ xen tiếng Anh.
- Cá nhân hóa: học từ nhật ký xem bạn thật sự muốn `that` hay `thật`.
- Sửa muộn đã xử lý một phần ("that là" → "thật là" khi có từ phía sau), nhưng chưa giúp ở thời điểm nhấn Space của chính từ đó.

---

## #5. `thicsk` không thành `thích`
Trạng thái: Mở | Nguồn: ghi chú của bạn (mục 6)

**Mô tả:** `thics` được sửa thành `thích` (đã sửa ở commit riêng), nhưng chỉ cần thêm một ký tự (`thicsk`) thì không sửa nữa.

**Giải pháp có thể:**
- `thicsk` cách `thích` hai lỗi (dấu gõ sớm và một chữ lạc), nên bị tìm kiếm hai lỗi (chặt hơn) xử lý. Có thể thêm loại lỗi "phím dấu gõ trước khi hết từ" với chi phí thấp.
- Cho phép sửa theo hai bước: bỏ chữ thừa (`thicsk` → `thics`) rồi sửa tiếp (`thics` → `thích`), mỗi bước đều qua kiểm tra độ tin cậy.

---

## #6. Từ có chữ hoa lẫn (`ĐIểm`) không được sửa
Trạng thái: Mở | Nguồn: ghi chú của bạn (mục 7)

**Mô tả:** Từ có chữ hoa bất kỳ thì không được sửa lại, ví dụ `ĐIểm` (lẽ ra là `Điểm`).

**Giải pháp có thể:**
- App hiện coi từ viết hoa là có thể là tên riêng nên bỏ qua. Có thể thêm một bước **sửa chữ hoa nhầm** riêng, tách khỏi sửa chính tả: mẫu hai chữ hoa đầu rồi chữ thường (`ĐIểm`, `THis`) và Caps Lock đảo (`tHIS`) được đưa về dạng đúng khi dạng chữ thường là từ đã biết.

---

## #7. Sửa từ vựng, ngữ pháp theo ngữ cảnh
Trạng thái: Ý tưởng | Nguồn: ghi chú của bạn (mục 8)

**Mô tả:** Muốn app sửa cả từ vựng (dùng sai từ) và ngữ pháp dựa vào ngữ cảnh, không chỉ lỗi gõ.

**Giải pháp có thể:**
- Đây là hướng nghiên cứu lớn hơn MVP. Hướng gần nhất là sửa lỗi "từ hợp lệ nhưng sai" bằng ngữ cảnh hai phía (sửa muộn với ứng viên mở rộng, đã có bản thử nghiệm, mặc định tắt).
- Các thí nghiệm ngoại tuyến trên Kaggle cho thấy n-gram kết hợp ngữ cảnh phải đã hơn mô hình thầy LLM ở nhóm từ tiếng Việt khó. Đi xa hơn cần hướng huấn luyện tổng quát (xem `tools/kaggle`).

---

## #8. Terminal: phân biệt lệnh với văn xuôi
Trạng thái: Cần làm rõ | Nguồn: bạn báo, 2026-10-04 | Liên quan: #9

**Mô tả:** Bạn dùng terminal cho nhiều việc: chat với Claude Code, Codex (văn xuôi, trộn tiếng Việt và thuật ngữ tiếng Anh) và gõ lệnh shell. App phân loại theo **tiến trình** (`WindowsTerminal.exe` là nhóm `Code`) nên không phân biệt được hai tình huống đó. Ở nhóm `Code`, app chỉ cho sửa sang từ **có dấu** và chặn sửa sang từ không dấu.

**Đã xảy ra:** `namk` trong câu "...hay nhất việt namk" bị sửa thành `năm` thay vì `nam`: các ứng viên không dấu bị loại trước khi chấm điểm nên `năm` thắng dù kém hơn. **Đã sửa** ở commit `b480389`: ứng viên không dấu vẫn tranh, nếu nó thắng thì app bỏ qua. Hệ quả: ở terminal `namk` giờ giữ nguyên, không ra `nam`.

**Giải pháp có thể (chưa làm):**
- Quyết định theo **bằng chứng của từng dòng**, không theo ứng dụng:
  - Dòng có dấu hiệu lệnh (từ có `-` đầu, `/`, `\`, `.`, `$`, `=`, dấu nháy, `|`; dòng bắt đầu bằng `/`, `!`, `@`; từ đầu dòng khớp tên chương trình trong `PATH`) thì không động vào.
  - Dòng có bằng chứng văn xuôi (các từ trước là tiếng Việt có dấu; tiêu đề cửa sổ chứa `claude`/`codex`) thì cho phép sửa sang từ không dấu.
  - Mơ hồ thì bỏ qua.
- Tiêu đề tab của Windows Terminal có thể là tín hiệu rẻ và mạnh; chưa biết tiêu đề thực tế khi chạy Claude Code/Codex trên máy bạn.
- Đo bằng lịch sử lệnh thật của bạn (PowerShell `ConsoleHost_history.txt`, Git Bash `~/.bash_history`), chỉ chạy cục bộ và chỉ báo số tổng hợp. **Cần bạn đồng ý trước.**
- Ca khó còn lại: lệnh pha văn (`git commit -m "sửa lỗi nam"`), chữ trong dấu nháy là văn nhưng nằm trong lệnh.

---

## #9. Mất ngữ cảnh giữa hai từ ở terminal
Trạng thái: Đang điều tra | Nguồn: nhật ký (2026-10-04 18:53:48)

**Mô tả:** Dòng `FIX namk → năm` có cột ngữ cảnh **trống**, trong khi từ `việt` ngay trước nó vừa được sửa lúc 18:53:46. Nghĩa là app đã mất từ trước đó. Chưa rõ nguyên nhân.

**Giải pháp có thể:**
- Ứng viên nghi ngờ: tay cầm cửa sổ (foreground) nhấp nháy giữa hai phím làm app tự đặt lại trạng thái; một phím được coi là "đặt lại" (Enter, mũi tên, nhấp chuột); cờ ô mật khẩu chớp tắt.
- Ghi lý do mỗi lần đặt lại vào `events.log` (hiện chưa ghi) để lần sau nhìn thấy nguyên nhân.

---

## #10. Thỉnh thoảng gõ tiếng Việt không được một lúc
Trạng thái: Đang điều tra | Nguồn: bạn báo, 2026-10-04

**Mô tả:** Đang ở chế độ tiếng Việt nhưng đôi lúc gõ không ra tiếng Việt; đợi một lúc thì gõ lại được. Chưa rõ nguyên nhân.

**Đã làm (commit `8fc8b08`):** thêm `events.log` (ghi bỏ qua phím khi nghi là ô mật khẩu, bộ kiểm tra tiêu điểm trả lời chậm, bật/tắt tiếng Việt và phím tắt, hook bị gỡ rồi cài lại); sửa việc sự kiện tiêu điểm giữa chừng một từ làm phần còn lại của từ ra chữ thô (`vieetj`); nạp lại file cài đặt không còn ghi đè trạng thái bật/tắt tiếng Việt; phím tắt không còn bị kẹt phím bổ trợ khi đổi cửa sổ.

**Ứng viên nghi ngờ chưa kiểm chứng:**
- Cờ "ô mật khẩu" lỗi thời: app bỏ qua mọi phím cho tới khi bộ kiểm tra trả lời xong (có thể mất vài giây ở trang lớn).
- Windows gỡ hook vì xử lý quá chậm; bộ canh sẽ cài lại sau vài giây.

**Cần từ bạn khi lỗi tái diễn:** biểu tượng khay là V hay E, chữ hiện ra thế nào (chữ thô như `vieetj`, không ra gì, hay chữ tiếng Anh), ở ứng dụng nào và kéo dài bao lâu, kèm nội dung `%APPDATA%\AutoCorrect\events.log` quanh thời điểm đó.

---

## #11. Cơ chế guard chống sửa sai
Trạng thái: Ý tưởng | Nguồn: bạn đề xuất, 2026-10-04

**Mô tả:** Hiện chỉ có cơ chế tìm cách sửa cho đúng; chưa có cơ chế riêng phát hiện và chặn sửa sai. Hai cơ chế nên chạy song song và bù trừ nhau.

**Giải pháp có thể:**
- Guard dùng các tín hiệu **độc lập** với bộ đề xuất sửa (cùng một điểm số thì chỉ lặp lại cùng một lỗi). Ba lớp theo thời điểm:
  1. **Trước khi sửa:** chặn theo hình dạng (URL, email, đường dẫn, từ có số, CamelCase, từ viết hoa hết, `#thẻ`, `@tên`) và "từ vựng của bạn".
  2. **Sau khi sửa, khi có từ kế tiếp:** kiểm tra lại bằng ngữ cảnh phía phải, nếu từ gốc hợp hơn thì khôi phục.
  3. **Theo phản ứng của bạn:** sau mỗi lần hoàn tác hoặc tự sửa tay thì nâng ngưỡng cho từ đó và tạm siết cả hệ thống nếu tỉ lệ hoàn tác tăng.
- Bản thử lớp 2 (hàm `audit`) mới đo một lần ở biên 4,0 và hầu như không có tác dụng. Chưa commit, chưa kết luận. Nên viết rõ phân loại các kiểu sửa sai thật và tín hiệu độc lập cho từng kiểu trước khi viết thêm mã.

---

## #12. Giả thuyết: bước sinh ứng viên bỏ sót từ đúng
Trạng thái: Cần làm rõ | Nguồn: bạn báo `mọi thứ chỉ là ngẫy nhiên` và `that là`, 2026-10-04 | Liên quan: #2, #4, #5

**Mô tả:** Sửa muộn (`delayed=1`, đang bật) vẫn không sửa được. Phần chấm điểm ngữ cảnh chỉ chọn trong danh sách ứng viên do bước sinh đưa ra; từ đúng không có trong danh sách thì ngữ cảnh mạnh đến đâu cũng không cứu được.

**Bằng chứng:**
- `that` + `là`: khi nhấn Space, `that` được 16,5 điểm; ứng viên cao nhất là `thay` 8,3, `thả` 7,2, `tha` 6,6. `thật` **không có trong danh sách** (thiếu phím thanh `j`). Sau `là` không có dòng nào cho thấy sửa muộn thử sửa.
- `ngẫy nhiên`: `ngẫy` là âm tiết hợp lệ về cấu tạo nên coi là cố ý; `ngẫu` có thể không được sinh ra. **Chưa kiểm chứng.**

**Nhận xét:** Benchmark dùng lỗi gõ mô phỏng theo các kiểu mình tự nghĩ ra, nên không đo được các kiểu chưa nghĩ tới. Điểm số tốt chỉ cho biết app sửa tốt các lỗi đã lường trước. Vá từng ca (`thật`, `ngẫu`, `thicsk`) sẽ lặp lại việc sửa riêng lẻ, không tổng quát.

**Giải pháp có thể:**
- Xác nhận trước: chạy `ac-bench --delayed` với `that là` và `ngẫy nhiên` để xem ứng viên thật sự thiếu hay chỉ thiếu điểm.
- Đo độ bỏ sót của bước sinh trên lỗi thật: dùng cặp "gõ → sửa tay" trong nhật ký (chỉ lấy số tổng hợp, cần bạn cho phép) để thống kê các kiểu lỗi hay gặp, thay vì đoán theo từng ca.
- Thêm ứng viên "thiếu phím thanh" khi từ không có dấu thanh.
- Khi sửa muộn có từ phía phải, cho phép đổi dấu thanh hoặc nguyên âm kề phím ở âm tiết hợp lệ.
- Về lâu dài: học bộ sinh ứng viên từ dữ liệu lỗi gõ thật.

**Cập nhật (2026-10-04): giả thuyết trên bị bác bỏ ở hai ca.** Sau khi tắt Autocorrect/Spell check của Notepad (xem #13), `that laf ` ra `thật là` và `ngẫy nhiên` ra `ngẫu nhiên`: sửa muộn sinh được cả `thật` lẫn `ngẫu`. Danh sách `top: thay, thả, tha` là của bước sửa tức thì, không phải của sửa muộn. Phần "bước sinh bỏ sót" còn lại cần đo trên lỗi thật mới kết luận được; #2 (`ngượi`) và #5 (`thicsk`) chưa được thử lại.

---

## #13. Notepad bản mới: Autocorrect/Spell check làm hỏng chữ app chèn
Trạng thái: Đã giải quyết (bằng cấu hình) | Nguồn: bạn báo, 2026-10-04

**Mô tả:** Ở Notepad (Windows 11), cùng một lần sửa `that laf ` cho ra `that àà ` thay vì `thật là`, lặp lại mãi, trong khi log của app ghi đúng (`FIX là -5 +"ật là "`).

**Nguyên nhân (xác nhận bằng thử nghiệm):** tính năng Autocorrect/Spell check của chính Notepad. Tắt hai mục này trong cài đặt Notepad thì hết lỗi.

**Giải pháp có thể:**
- Ghi chú trong README/cài đặt: tắt Autocorrect và Spell check của Notepad khi dùng app.
- Chưa có cách để app tự tránh; nếu muốn, có thể phát hiện Notepad và cảnh báo một lần.
- Khi gặp ứng dụng khác ra chữ sai dù log đúng, kiểm tra trước xem ứng dụng đó có tự sửa chữ không.

---

## #14. Chữ viết tắt in hoa và tên riêng bị đổi nhầm
Trạng thái: Mở | Nguồn: sim `ac-sim`, 2026-10-04 (RESEARCH.md mục 3.1)

**Mô tả:** Trong các ca app đổi nhầm từ gõ đúng ở môi trường normal, nhóm lớn nhất là chữ viết tắt in hoa (`AMG` → `AM`, `ENDF` → `END`, `CPI` → `COI`, `BYD` → `BY`) và tên riêng (`Xavi` → `Xạ`, `Tieran` → `Tiên`, `Zachor` → `Chỏ`).

**Giải pháp có thể:**
- Guard theo hình dạng: không sửa từ in hoa toàn bộ dài từ 2 ký tự (lớp 1 của #11). Đo lại bằng sim trước và sau.
- Tên riêng viết hoa chữ đầu ở giữa câu: chỉ sửa khi rất tự tin; về lâu dài để mô hình học quyết định.

---

## #15. Tiếng Anh gõ ở chế độ Việt không bao giờ được sửa lại
Trạng thái: Mở | Nguồn: sim `ac-sim`, 2026-10-04 | Liên quan: yêu cầu #1 trong RESEARCH.md

**Mô tả:** Gõ tiếng Anh khi đang ở chế độ Việt, Telex đổi từ thành chữ Việt hợp lệ: `of` → `ò`, `this` → `thí`, `was` → `ứa`, `there` → `thể`, `down` → `dơn`. App bỏ sót tất cả (10,5% số từ tiếng Anh trong sim, 600 trên 5732): bộ sửa thấy phím `of` là một từ đúng nên không động vào, trong khi màn hình hiện `ò`.

**Giải pháp có thể:**
- Khi chuỗi phím là một từ tiếng Anh phổ biến mà kết quả Telex là chữ Việt hiếm trong ngữ cảnh (bên trái là tiếng Anh), trả lại chữ thô. Cần nhận diện ngôn ngữ theo ngữ cảnh: đây chính là bài toán học của yêu cầu #1.
- Đo: nhóm `(Telex)` trong bảng của sim.

---

## #16. Sửa muộn đổi `that` trong câu tiếng Anh thành `thật`
Trạng thái: Mở | Nguồn: sim `ac-sim`, 2026-10-04 | Liên quan: #4

**Mô tả:** "the impact that may" → `thật`: sửa muộn đổi một từ tiếng Anh đúng sang tiếng Việt dù ngữ cảnh hai bên đều là tiếng Anh. Mặt trái của chính tính năng đã sửa được `that là` → `thật là`.

**Giải pháp có thể:**
- Không đổi sang tiếng Việt khi cả từ trước lẫn từ sau là tiếng Anh.
- Về lâu dài: cùng bài toán nhận diện ngôn ngữ theo ngữ cảnh như #15.

---

## #17. Thiếu dấu cách giữa hai hoặc nhiều từ
Trạng thái: Đang sửa | Nguồn: bạn đề xuất, 2026-10-04; đo bằng sim `ac-sim` (`--join`) | Liên quan: RESEARCH.md yêu cầu #6

**Mô tả:** Gõ `quanheej` (quên Space giữa `quan` và `hệ`) thì app không sửa thành `quan hệ`. Thực tế có thể dính 3, 4 hoặc nhiều từ. Trong sim: 0 ca được sửa ở mọi độ dài từ 2 đến 5 từ (bảng theo độ dài do `ac-sim` in). Màn hình giữ chữ thô (`giastreen` cho "giá trên", `ddungscachs` cho "đúng cách") vì Telex không ghép được cả chuỗi; vài ca còn bị sửa sai làm mất chữ (`toanfan` → `toàn`, `Xarcos` → `Xác`).

**Giải pháp có thể:**
- Sinh ứng viên tách k âm tiết: quy hoạch động theo vị trí cắt, mỗi đoạn phải là âm tiết hợp lệ, giữ lại vài cách tách tốt nhất (như tách từ). Không liệt kê hết nên chi phí vẫn nhỏ. Mơ hồ ở ranh giới (`an|h` hay `a|nh`, `ng` hay `n|g`) cần ngữ cảnh.
- Lỗi gõ nằm bên trong một chuỗi dính (`quanhejf` thừa phím) trộn bài toán tách với bài toán sửa chính tả: điểm cặp từ và ngữ cảnh mới đủ phân biệt.
- Chọn cách tách và quyết định có sửa hay không: điểm cặp từ (KN) + ngữ cảnh làm mốc ban đầu; về sau để mô hình học (cùng khung với các yêu cầu khác).
- Engine: một lần sửa thay một từ bằng k từ, ghép Telex từng đoạn sau khi tách (ghép cả chuỗi dài một lần sẽ sai ở các âm tiết sau); ngữ cảnh cho từ sau là từ cuối của đoạn; Ctrl+Z trả lại chuỗi phím nguyên.
- Chặn ngay: không để bộ sửa hiện tại "sửa" chuỗi dính thành một từ làm mất chữ (`toanfan` → `toàn`).

**Cập nhật (2026-10-04): bản đầu của bộ tách (`smart/split.rs`).** Sửa đúng 53% với 2 từ dính, 41% với 3, 20% với 4, 12% với 5 (trước đó 0%). Giữ ngưỡng chặt nên chưa tìm được hết; chi tiết và đánh đổi ở RESEARCH.md mục 3.2. Còn lại: lỗi gõ bên trong chuỗi dính, sửa muộn khi ranh giới yếu, và học cách chọn; từ nước ngoài hoặc tên riêng tình cờ tách được vẫn là rủi ro khi hạ ngưỡng.
