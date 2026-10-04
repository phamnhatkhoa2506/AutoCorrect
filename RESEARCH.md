# Nghiên cứu: bộ sửa lỗi học được, thích nghi được

Tài liệu này định nghĩa bài toán nghiên cứu sau mốc MVP (tag `mvp-1`). Các vấn đề cụ thể và lịch sử
từng ca nằm ở `ISSUE.md`; kiến trúc app nằm ở `PLAN.md`. Mọi con số ghi ở đây đều phải đo lại được
bằng công cụ trong repo; giả thuyết được ghi rõ là giả thuyết.

## 1. Bài toán

MVP sửa bằng luật và ngưỡng viết tay. Cách đó dùng tạm được, nhưng mỗi ca khó mới (`thật`/`that`,
`ngẫy nhiên`, `thicsk`, terminal khác Notepad) lại cần một luật mới. Mục tiêu nghiên cứu là một
phương pháp **học từ dữ liệu**, **thích nghi theo môi trường và theo người dùng**, thay cho việc vá
từng ca.

### 1.1 Một quyết định chung

Năm yêu cầu (mục 2) quy về cùng một quyết định, lặp lại ở mỗi ranh giới từ:

> Biết **các phím đã gõ** của từ, **ngữ cảnh trái**, **môi trường** (app, loại ô nhập) và, nếu đang
> sửa muộn, **ngữ cảnh phải**: chọn **để nguyên**, **thay bằng ứng viên X**, hoặc **chờ thêm**.

- Sửa tức thì và sửa muộn là cùng một mô hình với lượng thông tin khác nhau.
- Môi trường là đặc trưng đầu vào, không phải một mô hình riêng.
- "Guard" chống sửa sai chính là lựa chọn *để nguyên* khi chưa đủ tự tin (selective prediction).

### 1.2 Ràng buộc (không đổi so với MVP)

- **Chính xác hơn độ phủ**: sửa sai một từ đúng tệ hơn nhiều so với bỏ sót một lỗi.
- **Ngoại tuyến**, chạy trên CPU của máy người dùng, vài ms mỗi từ.
- Dữ liệu cá nhân (nhật ký, lịch sử lệnh) ở lại trên máy; chỉ đưa ra ngoài số liệu tổng hợp, và
  chỉ khi được cho phép.
- Huấn luyện nặng chạy trên Kaggle hoặc máy GPU khác, không chạy trên máy người dùng.
- Chuẩn hóa teencode chỉ là tùy chọn, mặc định tắt.

## 2. Năm yêu cầu và cách xếp chúng

| # | Yêu cầu | Loại bài toán | Ghi chú |
|---|---|---|---|
| 1 | Ca khó, nhất là lẫn Anh–Việt (`that`/`thật`) | Học: nhận diện ngôn ngữ theo từ, có ngữ cảnh | Lõi nghiên cứu |
| 2 | Thích nghi theo môi trường (shell, code, terminal, Notepad...) | Học: đặc trưng môi trường + học từ phản ứng (contextual bandit) | Ví dụ thật: sửa muộn đang chỉ bật ở nhóm `Normal` (`hook.rs`, `apply`) |
| 3 | Sửa cả ngữ pháp, theo cả hai chế độ | Học: mô hình chuỗi (chèn, xóa, đổi từ) | Lớn hơn hẳn; làm sau. Phần giao là lỗi "từ hợp lệ nhưng sai" |
| 4 | Chống nhiễu (`vò sao`, `vì saoi`) | Học: mô hình lỗi gõ + ngữ cảnh | `vò` là từ hợp lệ sinh từ lỗi kề phím: cần ngữ cảnh, giống #1 |
| 5 | Dấu câu dính chữ (`saoi)`, `"vif"`) | **Không phải bài toán học**: cách engine tách từ | Đã sửa trong engine (mục 6) |

## 3. Môi trường mô phỏng (`crates/ac-sim`)

Bài học của `ISSUE.md` #12: benchmark chỉ đo được những kiểu lỗi mình tự nghĩ ra. Môi trường mô phỏng
phải (a) chạy **đúng engine của app** trên **chuỗi phím**, và (b) được **hiệu chỉnh bằng dữ liệu lỗi
thật**, nếu không mô hình chỉ giỏi trên thế giới do mình tưởng tượng.

| Khối | Vai trò | Trạng thái |
|---|---|---|
| Nguồn câu | Câu giữ riêng (không dùng khi huấn luyện n-gram) của các kho: tin tức, web, phụ đề, mạng xã hội, tiếng Anh | Có (dùng lại `data/raw`) |
| Người gõ | Sinh chuỗi phím từ câu đúng: lỗi kề phím, thiếu, thừa, đảo, gấp đôi phím, sai hoặc thiếu phím thanh, thiếu nửa dấu (`aa`→`a`), chữ hoa dính (`ĐIểm`), tự nhận ra rồi Backspace sửa | Có, tỉ lệ **chưa hiệu chỉnh** |
| Engine | Chính `ac_core::Engine` và bộ sửa nạp giống app (`hook.rs`, `corrector()`) | Có |
| Màn hình | Áp đúng hành động engine trả về (Pass, Replace, ReplaceThenPass) lên một bộ đệm chữ | Có |
| Làn đối chứng | Cùng chuỗi phím, tắt sửa lỗi: cho biết từ "như đã gõ" để phân loại kết quả | Có |
| Người dùng phản ứng | Ctrl+Z khi thấy sửa sai (theo xác suất); sau này: sửa tay | Có (chỉ Ctrl+Z) |
| Môi trường | `normal`, `code` (cấu hình engine như `hook.rs`, `apply`) | Có; nguồn câu riêng cho lệnh shell và prompt: chưa |
| Xuất dữ liệu | Mỗi từ một dòng JSONL (ngữ cảnh, phím, như-đã-gõ, kết quả, đáp án) để huấn luyện | Có |

**Phân loại kết quả mỗi từ** (I = từ đúng, B = làn đối chứng, F = màn hình cuối):

- B = I (gõ đúng): F = I là *đúng*, F ≠ I là **sửa nhầm** (tệ nhất).
- B ≠ I (có lỗi): F = I là *sửa đúng*, F = B là *bỏ sót*, còn lại là **sửa sai**.
- B ≠ I mà người gõ không mắc lỗi nào: Telex làm hỏng từ (thường là từ tiếng Anh gõ ở chế độ Việt).
  Đây chính là yêu cầu #1.

Chạy: `cargo run -p ac-sim --release -- --help`.

Một lần sửa bị người gõ hoàn tác vẫn được chấm theo những gì app đã viết, không theo màn hình sau khi
người gõ dọn lại: chỉ số đo quyết định của app.

### 3.1 Kết quả đầu tiên (2026-10-04, mặc định: 300 câu mỗi kho, hạt giống 7, sửa muộn bật)

Tỉ lệ lỗi gõ của người gõ là đoán, nên chỉ dùng để so phiên bản và so môi trường, không phải độ chính
xác ngoài đời.

| Môi trường / kho | Đổi nhầm từ đúng (trên 1000) | Lỗi: sửa đúng | bỏ sót | sửa sai |
|---|---|---|---|---|
| normal / vi-news | 2,64 | 54,3% | 43,7% | 2,0% |
| normal / vi-dialogue | 1,26 | 54,0% | 41,7% | 4,3% |
| normal / vi-social | 2,01 | 50,2% | 44,2% | 5,5% |
| normal / en-news (gõ ở chế độ Việt) | 1,46 | 13,9% | 85,1% | 1,0% |
| code / vi-news | 0,42 | 42,5% | 56,5% | 1,0% |
| code / vi-dialogue | 0,00 | 36,1% | 60,9% | 3,0% |
| code / vi-social | 0,29 | 46,1% | 50,8% | 3,1% |
| code / en-news (gõ ở chế độ Việt) | 0,21 | 0,0% | 99,1% | 0,9% |

Theo loại lỗi (ba kho tiếng Việt gộp lại, môi trường normal → code, tỉ lệ sửa đúng): gấp đôi phím 90% →
69%, thừa phím kề 71% → 70%, kề phím 58% → 48%, đảo 59% → 53%, thiếu nửa dấu 44% → 14%, thiếu thanh
40% → 18%, sai thanh 34% → 25%, thiếu phím 28% → 19%, giữ Shift (`ĐIểm`) 0% → 0%, Telex làm hỏng 0%.

Đọc nhanh:
- **Môi trường code sửa ít hơn hẳn**, nhất là lỗi về thanh và dấu: sửa muộn chỉ bật ở `normal`.
  Đây là "terminal không sửa như Notepad", giờ đo được (yêu cầu #2).
- **Tiếng Anh gõ ở chế độ Việt** (`of` → `ò`, `this` → `thí`, `was` → `ứa`) không bao giờ được sửa lại:
  10,5% số từ tiếng Anh (600 trên 5732) bị Telex đổi, và app bỏ sót tất cả (yêu cầu #1).
- **Đổi nhầm** chủ yếu là chữ viết tắt in hoa (`CPI` → `COI`, `BYD` → `BY`), tên riêng (`Xavi` → `Xạ`),
  từ hiếm hoặc từ láy (`rưng rưng` → `rung`), và sửa muộn đổi `that` trong câu tiếng Anh thành `thật`.
  Một phần là câu cố ý gõ không dấu trong kho (`sap` → `sắp`): tính năng thêm dấu làm đúng việc của nó.
- **Lỗi thiếu phím và lỗi thanh** bị bỏ sót nhiều nhất (60–80%).

## 4. Dữ liệu thật: hiện có gì

Nhật ký trên máy (đếm ngày 2026-10-04, chỉ số tổng hợp): 97 dòng, gồm 92 `FIX` và 5 `LATE`;
50 ở nhóm `Normal`, 42 ở nhóm `Code`; không có dòng hoàn tác hay sửa tay.

**Hệ quả:** nhật ký hiện chỉ ghi những lỗi app **đã sửa được**. Lỗi app bỏ sót không để lại dấu vết,
nên dùng nó để hiệu chỉnh người gõ sẽ lặp lại đúng thiên lệch của #12. Nguồn lỗi bỏ sót là việc bạn
tự sửa tay (Backspace vào từ rồi gõ lại), đã được engine bắt nhưng chỉ ghi khi bật `journal_edits`.
**Đề xuất:** bật "ghi sửa tay" (`journal_edits=1`) để dữ liệu tích lũy trong lúc dựng phần còn lại.

## 5. Học

- **Ngoại tuyến:** sim sinh dữ liệu có đáp án; thầy LLM (Kaggle) gắn nhãn hoặc chấm các ca khó;
  chưng cất ra một mô hình nhỏ (xếp hạng ứng viên + quyết định để nguyên) chạy CPU.
- **Trực tuyến:** không huấn luyện lại cả mô hình trên máy. Chỉ cập nhật một lớp cá nhân nhỏ (từ vựng
  riêng, độ lệch theo môi trường, hiệu chỉnh độ tin cậy) từ phản ứng của người dùng, có giới hạn biên
  độ và đặt lại được. Phản hồi ngầm là nhiễu và lệch (chỉ thấy phản ứng với những gì app đã làm), nên
  đánh giá chính sách mới phải dùng đánh giá ngoài chính sách (off-policy), không đọc thẳng số liệu.
- **Trọng tài cuối cùng:** tập lỗi thật giữ riêng (từ nhật ký, nếu được phép), không phải sim.

## 6. Mốc

| Mốc | Nội dung | Trạng thái |
|---|---|---|
| R0 | Dấu đóng `) ] } "` kết thúc từ như dấu phẩy (yêu cầu #5) | Xong trong mã (sim xác nhận `saoi)` → `sao`), chờ thử thật |
| R1 | Sim v1: người gõ, engine thật, màn hình, làn đối chứng, Ctrl+Z, xuất JSONL | Xong, kết quả ở mục 3.1 |
| R2 | Hiệu chỉnh người gõ bằng lỗi thật (cần `journal_edits`) | Chờ dữ liệu |
| R3 | Mô hình học cho yêu cầu #1 và #4 (lỗi từ hợp lệ theo ngữ cảnh, có để nguyên) | |
| R4 | Thích nghi môi trường và học trực tuyến (#2) | |
| R5 | Ngữ pháp (#3) | |

## 7. Câu hỏi mở

- Môi trường chia mịn đến đâu: theo app, theo loại ô nhập, hay theo bằng chứng của từng dòng
  (terminal vừa gõ lệnh vừa chat với Claude Code)?
- Nguồn câu cho prompt và lệnh shell: tự sinh, hay dùng lịch sử lệnh của bạn (chỉ khi được phép)?
- Ngân sách độ trễ cho mô hình học: bao nhiêu ms mỗi từ là chấp nhận được?
