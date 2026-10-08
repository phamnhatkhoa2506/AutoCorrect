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

Các yêu cầu (mục 2) quy về cùng một quyết định, lặp lại ở mỗi ranh giới từ:

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

## 2. Các yêu cầu và cách xếp chúng

| # | Yêu cầu | Loại bài toán | Ghi chú |
|---|---|---|---|
| 1 | Ca khó, nhất là lẫn Anh–Việt (`that`/`thật`) | Học: nhận diện ngôn ngữ theo từ, có ngữ cảnh | Lõi nghiên cứu |
| 2 | Thích nghi theo môi trường (shell, code, terminal, Notepad...) | Học: đặc trưng môi trường + học từ phản ứng (contextual bandit) | Ví dụ thật: sửa muộn đang chỉ bật ở nhóm `Normal` (`hook.rs`, `apply`) |
| 3 | Sửa cả ngữ pháp, theo cả hai chế độ | Học: mô hình chuỗi (chèn, xóa, đổi từ) | Lớn hơn hẳn; làm sau. Phần giao là lỗi "từ hợp lệ nhưng sai" |
| 4 | Chống nhiễu (`vò sao`, `vì saoi`) | Học: mô hình lỗi gõ + ngữ cảnh | `vò` là từ hợp lệ sinh từ lỗi kề phím: cần ngữ cảnh, giống #1 |
| 5 | Dấu câu dính chữ (`saoi)`, `"vif"`) | **Không phải bài toán học**: cách engine tách từ | Đã sửa trong engine (mục 6) |
| 6 | Thiếu dấu cách giữa k từ (`quanheej` → `quan hệ`, k ≥ 2 bất kỳ) | Tách chuỗi phím thành k âm tiết: sinh ứng viên bằng quy hoạch động theo chỗ cắt + **học** chọn cách tách theo ngữ cảnh, đồng thời với sửa lỗi gõ bên trong | Engine phải thay một từ bằng k từ (ngữ cảnh, hoàn tác); Telex ghép từng đoạn sau khi tách. Sim: app sửa 0% ở mọi k |

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

### 3.1 Kết quả đầu tiên (2026-10-04: 300 câu mỗi kho, hạt giống 7, sửa muộn bật)

Đo trước khi có lỗi thiếu dấu cách; chạy lại đúng bảng này bằng `--join 0`.

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
- **Thiếu dấu cách giữa k từ** (thêm sau; `--join` là tỉ lệ câu có một đoạn dính, `--join-max` là k tối đa,
  mặc định 5): app sửa 0% ở mọi k từ 2 đến 5 (lần đo 400 câu mỗi kho, `--join 0.5`: 2 từ 0 trên 326, 3 từ 0 trên
  137, 4 từ 0 trên 85, 5 từ 0 trên 59, ở môi trường normal). Màn hình giữ chữ thô (`giastreen`); một phần
  ca 2 từ bị sửa sai mất chữ (`toanfan` → `toàn`), tiếng Anh dính hai từ bị sửa sai nhiều nhất.

### 3.2 Bộ tách âm tiết v1 (`ac-core/src/smart/split.rs`, 2026-10-04)

Luật, chưa học: quy hoạch động theo chỗ cắt (beam 6), mỗi đoạn phải là âm tiết hợp lệ và đủ phổ biến
(`split_floor`), điểm là tổng ln P (KN) với từ trước làm ngữ cảnh. Chỉ cắt khi mọi âm tiết sau âm tiết đầu
đều khả dĩ hơn khi đứng sau từ trước so với đứng một mình (`split_lift`) và cách cắt tốt nhất hơn cách thứ
hai (`split_margin`). Chạy sau các cách sửa một phím và trước các cách sửa hai phím, chỉ cho chuỗi không
phải từ. Engine thay một từ bằng k từ; từ cuối là ngữ cảnh cho từ sau; Ctrl+Z trả lại chuỗi phím.

Kết quả (400 câu mỗi kho, `--join 0.5`, ba kho tiếng Việt gộp, môi trường normal, ngưỡng mặc định
margin 2, lift 1, floor 5; tắt hẳn bộ tách thì sửa đúng 0% ở mọi k):

| Số từ dính | 2 | 3 | 4 | 5 |
|---|---|---|---|---|
| Sửa đúng | 53% | 41% | 20% | 12% |

Quét `split_lift` (margin 2, floor 5), cùng mẫu (khoảng 250, 150, 90, 60 đoạn dính cho k = 2 đến 5);
"đổi nhầm" là số từ gõ đúng bị app đổi, trên khoảng 21 150 từ:

| lift | 99 (tắt) | 2 | 1 (mặc định) | 0,5 | 0 | -1 |
|---|---|---|---|---|---|---|
| Sửa đúng 2 từ | 0% | 36% | 53% | 58% | 66% | 77% |
| Sửa đúng 3 từ | 0% | 26% | 41% | 47% | 58% | 70% |
| Sửa đúng 4 từ | 0% | 10% | 20% | 27% | 34% | 49% |
| Sửa đúng 5 từ | 0% | 2% | 12% | 21% | 34% | 37% |
| Đổi nhầm từ đúng | 46 | 48 | 53 | 56 | 60 | 70 |

`split_margin` (1 đến 3) và `split_floor` (4 đến 6) gần như không đổi gì (quét trước khi đổi thứ tự thử các cách sửa). Lift thấp hơn sửa nhiều hơn
nhưng đổi nhầm thêm toàn từ nước ngoài và tên riêng tình cờ tách được (`Musiala` → `Mu sia la`,
`learners`, `vincom`, `rotundin`), nên mặc định giữ 1,0 (ưu tiên độ chính xác).

Chi phí trên bộ đo cũ `ac-bench` (không có đoạn dính): golden vẫn đạt đủ; lỗi gõ một phím của tiếng Việt:
sửa đúng 40,7% → 40,5%, sửa sai 6,9% → 7,4% (từ chuỗi "bỏ sót" nay bị tách ra); tiếng Anh: 1 trên 5035 từ
đúng bị đổi thêm.

Chưa làm: lỗi gõ nằm bên trong chuỗi dính (`quanhejf`), cắt khi một ranh giới yếu (min lift) mà ngữ cảnh
phía phải mạnh (sửa muộn), và học cách chọn thay cho luật.

### 3.3 Bộ thử ngoài: Viwiki-Spelling (2026-10-05)

Chạy bằng `cargo run -p ac-bench --release -- --viwiki [--docs N] [--show N]` (khoảng 6 phút cho cả bộ; ngưỡng
chỉnh bằng các cờ `--revise-margin`, `--split-lift`...). Bộ dữ liệu: 107 bài Wikipedia, 458 366 từ, 1511 lỗi
chú thích (Tran và cộng sự, 2021, CC BY 4.0). Lỗi là lỗi **thật** của người viết: 55% chỉ sai dấu hoặc thanh
(`hát` thay `hán`), 41% sai một chữ, 2% về dấu cách. Mỗi từ được đưa cho bộ sửa như app thấy: có các từ trước
trong cụm, và với sửa muộn có từ kế tiếp. "Đúng" là đổi thành một trong các đáp án đã chú thích.

Giới hạn khi đọc số: đây là văn bản đã viết xong, không phải phím đã gõ; chỉ các lỗi được chú thích mới được
tính, nên một từ bị đổi mà không có trong chú thích có thể là lỗi thật bị sót (ví dụ `bòng → bóng đèn` trông
đúng), tức precision dưới đây bị đánh giá thấp. Định nghĩa precision, recall của ta (tính theo từ bị đổi) có
thể khác của bài báo.

| Cách chạy | Từ bị đổi | Precision (đúng) | Recall (đúng) | F1 | Đổi từ không chú thích, trên 1000 từ |
|---|---|---|---|---|---|
| Sửa tức thì | 627 | 21,4% | 8,9% | 12,5 | 1,05 |
| Sửa muộn, ngưỡng mặc định (8 / 12) | 333 | 51,4% | 11,3% | 18,5 | 0,35 |
| Sửa muộn, nới (5 / 8) | 515 | 52,6% | 17,9% | 26,8 | 0,53 |
| Sửa muộn, nới nhiều (3 / 5) | 882 | 44,0% | 25,7% | 32,4 | 1,06 |
| Cả hai như app (mặc định) | 831 | 27,4% | 15,1% | 19,5 | 1,29 |
| Cả hai, nới nhiều | 1365 | 31,9% | 28,8% | 30,3 | 1,99 |

Tham chiếu: bài báo báo precision 67%, recall 71%, F1 69% trên cùng loại bộ này, bằng một mô hình lớn có ngữ
cảnh hai phía và được huấn luyện cho đúng việc đó. Ta thấp hơn nhiều ở recall, đúng như thiết kế: bộ sửa
tức thì cố ý để nguyên từ đã hợp lệ. Sửa muộn là phần có cơ hội, và nới ngưỡng cho thêm recall nhưng precision
tụt (đường đánh đổi ở mục 1.2).

Theo loại lỗi (cả hai, mặc định): sai dấu hoặc thanh sửa đúng 21,8%, sai một chữ 6,9%, dấu cách 3,4%.

Bộ tách âm tiết (mục 3.2) trên bộ này: tắt thì sửa dấu cách đúng 0%, bật thì 3,4%; nhưng số từ bị đổi mà không
có chú thích tăng từ 1,12 lên 1,29 trên 1000 và precision cả hai giảm từ 30,5% xuống 27,4%. Phần tăng là các
từ mượn khoa học bị cắt nhầm (`canxi → can xi`, `magiê → ma giê`, `halua → ha lua`, `natri → na tri`): hai âm
tiết đều hợp lệ và đứng cạnh nhau, nhưng không phải hai từ. Xem `ISSUE.md` #18.

### 3.4 Nguồn lỗi thật khác: VSEC (2026-10-05)

`data/raw/vsec/VSEC.jsonl`: 9341 câu, 282 459 âm tiết, 11 202 lỗi được sửa tay, nhãn ở mức âm tiết, khoảng
5000 cặp (lỗi, sửa) khác nhau. Mọi câu đều có ít nhất một lỗi (nên tỉ lệ 39,7 lỗi trên 1000 âm tiết cao hơn
nhiều so với văn bản thường). Theo loại: sai dấu hoặc thanh 64,7%, sai một chữ 29,5%, một âm tiết thành nhiều
(dính từ) 1,8%, khác 4,0%. Nhiều lỗi trông như trượt phím thật (`tranhh`, `cachh`, `hê` thay `hệ`, `iên` thay
`nhiên`). Repo không ghi giấy phép, cách chia tập hay nguồn câu; một số cặp là biến thể đặt dấu (`thoả` và
`thỏa`) chứ không hẳn lỗi, nên nhãn có nhiễu. Chưa có chia train/dev/test chính thức: ta sẽ tự chia theo câu.
Dùng để: (a) hiệu chỉnh mô hình người gõ của `ac-sim` bằng tần suất kiểu lỗi thật (mốc R2); (b) huấn luyện
và đo trên lỗi thật; (c) đo chéo: huấn luyện trên VSEC, đo trên Viwiki (mục 3.3). Không có chuỗi phím Telex,
nên phần "phím nào bị gõ trượt" vẫn phải suy ra.

`binhvq/news-corpus` (14,9 triệu bài báo): kho lưu trữ đã bị đóng từ tháng 8/2026, **không còn phát hành** và
các liên kết tải đã bị gỡ; giấy phép MIT chỉ áp dụng cho mã, không cho bài báo. Không dùng được.

## 4. Dữ liệu thật: hiện có gì

Nhật ký trên máy (đếm ngày 2026-10-04, chỉ số tổng hợp): 97 dòng, gồm 92 `FIX` và 5 `LATE`;
50 ở nhóm `Normal`, 42 ở nhóm `Code`; không có dòng hoàn tác hay sửa tay.

**Định dạng mới (2026-10-05):** mỗi dòng nay có thêm cột `left` (mọi từ trước, tối đa 4) và `right` (tối đa 3 từ
gõ sau đó, cần bật `journal_right`, mặc định tắt). Dòng nhật ký chờ các từ sau rồi mới ghi; chi tiết điều kiện chờ
ở `bench/README.md`. Dữ liệu này là đầu vào cho các đặc trưng ngữ cảnh phải của mô hình quyết định, và cho việc
hiệu chỉnh sim. Dòng cũ vẫn đọc được (hai cột cuối trống).

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

## 5. Kế hoạch huấn luyện: thầy rồi trò (viết 2026-10-05)

Mục tiêu: thay các ngưỡng viết tay bằng một mô hình học được, theo hướng "huấn luyện một mô hình lớn (thầy) rồi
chưng cất ra mô hình nhỏ (trò) chạy trong app". Thầy chỉ chạy trên Kaggle; máy bạn chỉ chạy đo và suy luận nhẹ.

**Quyết định hướng (2026-10-06): n-gram kết hợp mô hình học sâu do ta huấn luyện.** Không chọn giữa hai bên: n-gram
là nền luôn bật, mạng nơ-ron học phần n-gram không làm được, và một bộ kết hợp học được thay cho các ngưỡng đặt tay.
Căn cứ: bài báo Gupta (2019, mục 8) cho thấy hệ thống thời gian thực, thích ứng ngôn ngữ, chỉ dùng n-gram là khả thi
(phát hiện khoảng 7 micro giây mỗi từ, sinh ứng viên và xếp hạng vài mili giây) nhưng chỉ xử lý lỗi "từ không có trong
từ điển" và phải dò tay ba trọng số unigram, bigram, trigram, còn bài Tran và cộng sự (2021) cho thấy mô hình học sâu hai
chiều làm tốt lỗi từ hợp lệ nhưng nặng và chạy trên văn bản đã gõ xong. Ta cần phần giữa của hai bên, trên chuỗi phím.

| Tầng | Vai trò | Độ trễ mục tiêu | Trạng thái |
|---|---|---|---|
| 0. n-gram và sinh ứng viên bằng luật | Xác suất từ, cặp, bộ ba (Kneser-Ney), ngữ cảnh phải khi sửa muộn; luôn bật | micro giây đến mili giây | Có (bộ sửa hiện tại) |
| 1. Bộ kết hợp học được | Chấm mọi ứng viên trên một thang: đầu vào là điểm n-gram, nhãn nguồn, môi trường, độ dài, chữ hoa; đầu ra là ứng viên thắng và độ tự tin; thay các ngưỡng đặt tay | dưới 1 ms | Chưa làm (bước đầu tiên) |
| 2. Mạng trò nhỏ | Chỉ gọi khi tình huống mơ hồ (dưới ngưỡng tự tin của tầng 1); học phần dư so với n-gram (đầu vào có sẵn điểm n-gram), chưng cất từ thầy | vài mili giây | Chưa làm |
| 3. Thầy lớn | Chỉ chạy ngoại tuyến trên Kaggle để dạy trò và gắn nhãn | không giới hạn | Mã đã viết (T1, T2), chưa chạy được vì GPU |
| 4. Sửa muộn chạy nền | Dùng tầng 1 và 2 với ngữ cảnh phải | vài chục mili giây, không chặn phím | Có bản luật (mặc định tắt) |

**Thứ tự làm, mỗi bước có phép đo quyết định bước sau:**
1. **Tầng 1 với đặc trưng n-gram thuần** (hồi quy logistic hoặc cây quyết định nhỏ), huấn luyện trên lỗi sinh ở `ac-sim` và lỗi thật VSEC, đo trên đường cong Viwiki. Rẻ, không cần GPU. *Điều kiện xem lại:* nếu nó không hơn rõ bảng ngưỡng ở cùng mức đổi nhầm, thì phần "học" chưa đáng, và ta dồn sức vào dữ liệu thay vì mô hình.
2. **Thầy** khi GPU Kaggle dùng lại được (T1, tùy chọn T2).
3. **Trò** từ xác suất mềm của thầy, đo khoảng cách thầy–trò trên cùng đường cong.
4. **Tích hợp** vào app: tầng 1 trong hook, tầng 2 và sửa muộn chạy nền.

**Kết quả bước 1 (tầng 1, 2026-10-06): bộ học không hơn rõ bảng ngưỡng.** `tools/combiner/` (ac-bench `--evidence`, `make_cases.py`, `train_eval.py`): hồi quy logistic và gradient boosting trên 21 đặc trưng n-gram, huấn luyện trên VSEC train + lỗi sinh, đo ở cùng số lần đổi nhầm trên 1000 từ. Sửa đúng so với bảng ngưỡng: VSEC dev 15,0% → 15,2–15,6%; Viwiki 9,6% → 9,9–10,0%; lỗi sinh 27,7% → 29,5–29,7%. Chênh 0,2–2 điểm, trong sai số của 1.077 lỗi thật. Ở mức đổi nhầm rất thấp bộ học giữ độ chính xác cao hơn (95,7% so với 84–89% trên VSEC dev), nhưng thu hồi vẫn chỉ 10–15% trên lỗi thật: phần lớn lỗi không có ứng viên nào đủ điều kiện, nên trần nằm ở nguồn ứng viên và ngữ cảnh, không ở bộ quyết định. Theo điều kiện trên, dồn sức vào dữ liệu. Chưa thử: ngữ cảnh phải, mô hình lớn hơn 21 đặc trưng.

**Kết quả bước 2, lần chạy thử thầy (2026-10-07):** GPU Kaggle chạy lại được. T2 mới (v2.1) cho 36.000 lỗi từ 18.000 câu (1 giờ, 2 T4). Thầy `xlm-roberta-base` chỉ **1.500 bước, 7 phút trên một T4**, trên Viwiki-Spelling (chưa từng huấn luyện), cùng mức đổi nhầm khoảng 1 trên 1000 từ: sửa đúng **16,2% (không có từ bên phải, tau 0,95, 1,14 đổi nhầm) và 23,0% (một từ bên phải, tau 0,9, 1,39)**, so với **9,6% của bộ n-gram ở 1,05** (mục 3.3) và 9,9–10,0% của bộ học tầng 1. Tức thầy mới sau vài phút đã gấp 1,7 đến 2,4 lần thu hồi ở mức đổi nhầm tương đương, và nó vẫn đang học (F1 dev tăng 0,16 → 0,42 → 0,55 qua ba lần đo). Lưu ý khi so: bộ đo của thầy chia Viwiki thành 530.150 từ và 1.514 lỗi, ac-bench 457.560 từ và 1.453 lỗi (khác cách tách từ), nên đây là so sánh gần đúng, chưa cùng một bộ đo; một lần chạy, chưa lặp. Kết quả lần chạy đầy đủ ở ngay dưới.

**Kết quả bước 2, thầy đầy đủ (2026-10-07):** 40.000 bước, 188,6 phút trên một T4 (kernel `autocorrect-t1-teacher`); đường cong dev phẳng từ khoảng bước 38.000. Trên Viwiki (chưa huấn luyện, 530.150 từ, 1.514 lỗi):

| Hệ | Sửa đúng (thu hồi) | Độ chính xác khi sửa | Đổi nhầm /1000 từ |
|---|---|---|---|
| n-gram, sửa muộn (mục 3.3) | 9,6% | 22% | 1,05 |
| Thầy, **1 từ bên phải**, tau 0,9 | **39,1%** | 49,8% | 1,01 |
| Thầy, 1 từ bên phải, tau 0,95 | 36,6% | 58,4% | 0,66 |
| Thầy, 1 từ bên phải, tau 0,99 | 30,6% | 77,0% | 0,22 |
| Thầy, **0 từ bên phải**, tau 0,99 (cao nhất đo) | 22,5% | 11,4% | 4,95 |
| Thầy 1.500 bước, 0 từ bên phải, tau 0,95 (lần thử) | 16,2% | 24,9% | 1,14 |

Với một từ bên phải, ở cùng mức đổi nhầm thầy gấp khoảng 4 lần thu hồi của n-gram và độ chính xác cũng cao hơn. Với **không có từ bên phải thì thầy đầy đủ đổi nhầm 5 đến 29 trên 1000 từ ở mọi tau đã đo**, tệ hơn cả lần thử 1.500 bước (1,14 ở tau 0,95): huấn luyện lâu làm bản có từ bên phải tốt lên nhưng bản không có từ bên phải tự tin quá mức trên văn bản ngoài miền. *Giả thuyết tỉ lệ lỗi huấn luyện cao hơn văn bản thật (xác suất lệch phía "có lỗi") đã thử bằng cách nâng tau (kernel `autocorrect-t1-recal`, 2026-10-07):* bản 0 từ bên phải chỉ về **ngang n-gram, không hơn**: tau 0,9998 cho 9,2% sửa đúng ở 1,32 đổi nhầm (n-gram 9,6% ở 1,05), tau 0,9999 cho 6,9% ở 0,30. Nâng tau chỉ giảm đổi nhầm bằng cách giảm thu hồi, nên ở lúc đang gõ thầy chưa có lợi thế. Bản 1 từ bên phải giữ lợi thế ở mọi tau: 30,6% ở 0,22 đổi nhầm (tau 0,99), 20,4% với độ chính xác 89% ở 0,06 (tau 0,999). Hệ quả thiết kế: **lúc đang gõ (0 từ bên phải) chưa dùng được thầy; sửa muộn sau một từ thì dùng được.** Điểm còn thiếu: một lần chạy; Viwiki của thầy và ac-bench khác cách tách từ; thầy 283 triệu tham số, không đưa vào app được, cần trò (đã có 200.000 câu nhãn mềm, 478 MB).

**Kết quả bước 3, trò lần 1 (2026-10-07):** `tools/student/student_train.py`, 13,9 triệu tham số (n-gram chữ cái băm + Transformer 3 tầng), học từ 200.000 câu nhãn mềm của thầy, 6 epoch; kernel `autocorrect-student`. Đồng thuận top-1 với thầy trên phần giữ riêng 97,7%. Trên Viwiki (1 từ bên phải): **15,7% sửa đúng ở 1,25 đổi nhầm/1000 (tau 0,9, độ chính xác 23,4%); 10,3% ở 0,25 (tau 0,99)**, so với thầy 39,0% ở 1,01 và 30,6% ở 0,22, n-gram 9,6% ở 1,05. Tức trò gấp khoảng 1,6 lần n-gram nhưng chỉ giữ khoảng 40% thu hồi của thầy. Trên VSEC dev F1 (tau 0,9, 1 từ bên phải) 0,42 so với 0,77 của thầy. Đường cong dev vẫn tăng nhẹ ở epoch 6. Điểm lạ: bản 0 từ bên phải của trò (12,6% ở 0,95 đổi nhầm, tau 0,95) tốt hơn của thầy và nhỉnh hơn n-gram. Nghi ngờ nguyên nhân khoảng cách (chưa kiểm): nhãn mềm chỉ có lỗi tổng hợp trên câu sạch, không có lỗi thật VSEC hay lỗi do LLM chọn, và trò học từ đầu nên không có hiểu biết tiếng Việt có sẵn của thầy. Chưa tích hợp vào app.

**Trò không chưng cất (2026-10-07):** cùng mạng 13,9 triệu tham số, cùng 4.638 bước, nhưng học từ nhãn cứng trên dòng dữ liệu của thầy (75% lỗi tổng hợp, 15% cửa sổ lỗi thật VSEC, 10% lỗi LLM chọn, lỗi sinh tươi mỗi lần); kernel `autocorrect-student-hard`. Viwiki, 1 từ bên phải: **19,1% sửa đúng ở 1,14 đổi nhầm/1000 (tau 0,9, độ chính xác 29,1%); 11,8% ở 0,15 (tau 0,99)**, so với trò chưng cất 15,7% ở 1,25 và 10,3% ở 0,25, thầy 39,0% ở 1,01, n-gram 9,6% ở 1,05. VSEC dev F1 (tau 0,9, 1 từ bên phải) 0,52 so với 0,42 của trò chưng cất. Tức bản không chưng cất **tốt hơn** bản chưng cất ở mọi mức đổi nhầm đã đo, gấp khoảng 2 lần n-gram. **Không tách được nguyên nhân:** bản này có thêm dữ liệu lỗi thật VSEC và lỗi LLM chọn mà nhãn mềm không có, và dữ liệu sinh tươi vô hạn thay vì 200.000 câu lặp 6 lần. Đường cong dev vẫn tăng chậm (0,507 → 0,516 ở epoch 5 → 6). Vẫn còn cách xa thầy.

**Trò chưng cất trực tiếp (2026-10-07):** cùng dòng dữ liệu, cùng 4.638 bước và cùng cấu hình với bản không chưng cất, nhưng nhãn là xác suất top-8 của thầy chấm từng lô (70%) cộng nhãn cứng (30%); kernel `autocorrect-student-online`. Viwiki, 1 từ bên phải, sửa đúng / độ chính xác / đổi nhầm trên 1000 từ:

| tau | Chưng cất ngoại tuyến (200.000 câu) | Nhãn cứng | Chưng cất trực tiếp |
|---|---|---|---|
| 0,9 | 15,7% / 23,4% / 1,25 | 19,1% / 29,1% / 1,14 | **20,7% / 30,1% / 1,19** |
| 0,99 | 10,3% / 42,8% / 0,25 | 11,8% / 58,7% / 0,15 | **13,2% / 61,9% / 0,14** |
| 0,999 | 5,3% / 59,7% / 0,05 | 4,9% / 74,7% / 0,02 | **6,4% / 86,5% / 0,01** |

VSEC dev F1 (tau 0,9, 1 từ bên phải): 0,42 / 0,52 / **0,56**. Kết luận: thầy **có giúp, nhưng ít** (khoảng 10% thu hồi tương đối trên Viwiki, nhiều hơn trên VSEC dev vốn gần miền huấn luyện), và việc bản ngoại tuyến thua là do dữ liệu (không có lỗi thật VSEC và lỗi LLM chọn, 200.000 câu lặp lại), không phải do chưng cất. Mỗi cấu hình mới chạy một lần, chênh 1 đến 2 điểm trên 1.511 lỗi nên chưa chắc chắn. **Trần của trò 14 triệu tham số khoảng 20% thu hồi ở 1,2 đổi nhầm, thầy 39%:** khoảng cách chủ yếu do dung lượng và hiểu biết tiếng Việt có sẵn của thầy, không do nhãn. Đường nhanh khác để thử là chạy chính thầy (lượng tử hóa int8, khoảng 280 MB) ở tầng sửa muộn chạy nền, cần đo độ trễ trên CPU của máy bạn.

**Đo trên máy thật, CPU 4 luồng (2026-10-07, `tools/student/bench_cpu.py`, log trong `models/bench_cpu.log` và `models/bench_cpu_student.log`):** cửa sổ Viwiki thật 21 từ, một từ bên phải, mỗi lần một cửa sổ, gồm tách từ và chạy mạng bằng PyTorch.

| Bản | Dung lượng | Độ trễ, 1 luồng / 4 luồng (trung bình, p95 4 luồng) | Thu hồi, tau 0,9 (mẫu 300 lỗi) | Quyết định giống bản fp32 |
|---|---|---|---|---|
| Thầy fp32 | 1.133 MB | 300 ms / 220 ms (p95 349) | 38,7% | |
| Thầy int8 (nén động lớp Linear) | 861 MB | 253 ms / 206 ms (p95 285) | **22,0%** (tau 0,99: 10,0% so với 32,7%) | 96,4% |
| Trò (chưng cất trực tiếp) fp32 | 56 MB | **14,7 ms** / 14,7 ms (p95 21) | 21,3% | |
| Trò int8 | 47 MB | 22 ms / 25 ms (đã tắt đường nhanh của PyTorch) | 21,7% | 99,6% |

Kết luận: (1) **nén int8 đơn giản làm thầy mất gần nửa thu hồi** mà chỉ nhanh hơn khoảng 15% và nhẹ hơn 24%, vì phần lớn dung lượng là bảng nhúng 250 nghìn từ (khoảng 770 MB) không được nén; nên không dùng được cách này. Thầy fp32 mất 220 đến 300 ms mỗi từ trên CPU, đủ cho sửa muộn chạy nền nhưng chiếm CPU liên tục khi gõ và cần 1,1 GB. (2) **Trò 56 MB chạy khoảng 15 ms mỗi cửa sổ, đủ nhanh cả trong lúc gõ**, và nén int8 giữ nguyên chất lượng (mất 0,0026 xác suất trung bình); int8 trong PyTorch chậm hơn vì mất đường nhanh, tốc độ thật sẽ phụ thuộc bản cài bằng Rust. Thu hồi trên mẫu khớp với toàn bộ Viwiki (thầy 38,7% so với 39,0%, trò 21,3% so với 20,7%). Mẫu chỉ có 1.200 từ sạch nên **không đo được tỉ lệ đổi nhầm dưới khoảng 1 trên 1000 và độ chính xác ở đây bị thổi phồng** (lỗi chiếm 20% mẫu so với khoảng 0,3% thật): dùng bảng Viwiki đầy đủ cho các số đó.

**Đo đầu-cuối trong điều kiện app và nhật ký thật (2026-10-07).** Đo bằng Rust đúng dạng đầu vào của app (`ac-bench --viwiki`: lịch sử tối đa 4 từ trong cụm, dấu câu cắt cụm, một từ bên phải) cho kết quả **kém hơn** đo Python trên cửa sổ cả câu: trò chưa học dạng đầu vào này (trò cũ, chọn sửa tự do, toàn bộ Viwiki: mức 0,99 thu hồi 8,6%, độ chính xác 23,6%, 0,82 đổi nhầm/1000; mức 0,9: 16,1%, 13,1%, 3,31). Mốc n-gram sửa muộn cũ (25 tài liệu): thu hồi 10,8%, độ chính xác 35,8%, 0,55 đổi nhầm. Trò mới (có phím giữ) với n-gram dự phòng và chọn ứng viên trong tập hợp lý: mức 0,99: 18,5%, 22,5%, 1,77; mức 0,9: 26,6%, 15,9%, 3,90. Nên mỗi lần sửa đúng thêm phải đánh đổi khoảng 5 đến 6 lần đổi nhầm: chưa đạt ưu tiên chính xác. Nhật ký thật của người dùng (194 lần tự sửa) cho thấy 21 lần là sửa nhầm từ tiếng Anh gõ bằng cách bấm đôi phím dấu Telex (`tesst` hiện `test` bị đổi thành `tết`, `json` thành `son`); đã sửa lớp lỗi này trong `SmartCorrector::typed` (chữ hiện ra là từ tiếng Anh đã biết thì giữ). 41 trong 55 ca "gần" là từ không dấu cần từ kế tiếp quyết định; 35 lần tự sửa có phím dấu gõ đôi, 14 lần có chữ lặp 3 lần.

**Hướng mới (người dùng, 2026-10-07): bảo vệ (guard) song song với độ chính xác, và trò kết hợp với n-gram, không chạy riêng.** Mục tiêu đo: số lần sửa nhầm trên 1000 từ ở mức rất thấp, rồi mới tối đa thu hồi. Kế hoạch: (A) LLM 3B Instruct (7B nếu thiếu đa dạng) viết câu sạch cùng miền người dùng (trò chuyện kỹ thuật, máy học, lẫn thuật ngữ tiếng Anh) bằng chủ đề và phong cách chung, không dùng dòng nhật ký nào (`t3_domain_text.py`); (B) chạy động cơ (n-gram + trò) trên văn bản sạch đó: mỗi lần nó đổi chữ là một sửa nhầm có nhãn, dùng để huấn luyện bộ bảo vệ (bộ kết hợp học được trên đặc trưng của trò và n-gram), tinh chỉnh trên VSEC dev và chỉ đo trên Viwiki; (C) huấn luyện lại trò trên dữ liệu cùng miền, đúng dạng đầu vào của app.

**Dữ liệu T2 sau bước 1:** chọn lỗi nay đòi từ hợp lệ thay thế thua ít nhất 3 nat (từ hợp lệ ở chỗ đó có thể là đúng), và cân các loại lỗi theo tỉ lệ đo được (`KIND_WEIGHTS`) thay vì để loại dễ lấn át; xem `AUGMENT_RULES.md`.

**Chưa kiểm chứng (giả thuyết, cần đo):** trò học phần dư so với n-gram sẽ nhỏ hơn trò độc lập mà vẫn tốt; tầng 1 một mình đã nâng đường cong; ngân sách vài mili giây cho tầng 2 đủ trên CPU phổ thông. **Câu hỏi mở:** cỡ trò; mạng chạy bằng gì trong app (ONNX, hay tự cài bằng Rust); hạn mức GPU Kaggle.

**Bài toán của thầy v1.** Với mỗi âm tiết của một cụm câu như đang hiện trên màn hình: *giữ nguyên* hoặc *thay
bằng một âm tiết của từ vựng* (6 795 âm tiết, `data/vi_syllables.tsv`), cộng *loại lỗi* (đầu phụ, 11 loại). Hai đầu
phân loại như bài báo Tran và cộng sự, nhưng ở mức âm tiết. Kiểm tra trên dữ liệu thật: 95% lỗi của VSEC và 92%
lỗi của Viwiki có chữ đúng nằm trong từ vựng, nên cách "chọn một âm tiết" bao được phần lớn lỗi thật.

**Ngữ cảnh mô phỏng lúc gõ.** Mỗi mẫu huấn luyện là một cửa sổ kết thúc ở chỗ người gõ đang đứng: 4 vị trí cuối có
3, 2, 1, 0 từ phía sau (đúng như khi một từ vừa gõ xong và các từ sau lần lượt xuất hiện). Xen vào là mẫu cả câu
(sửa muộn thấy được). Một mô hình, hai chế độ, không cần hai bộ trọng số.

**Dữ liệu.** Lỗi sinh ngay lúc đọc từ 1,4 triệu câu sạch (tin tức, web, phụ đề), theo tỉ lệ loại lỗi của VSEC
(65% chỉ sai dấu hoặc thanh, 30% sai một chữ, 5% khác), mỗi câu một tỉ lệ lỗi ngẫu nhiên từ 1% đến 15%;
15% cửa sổ lấy từ lỗi **thật** của VSEC (90% VSEC để huấn luyện, 10% để kiểm tra giữa chừng). Viwiki chỉ để thử
cuối cùng, không bao giờ vào huấn luyện: đo chéo giữa hai nguồn lỗi thật khác nhau.

**Đo.** Cùng đường "sửa đúng so với đổi nhầm trên 1000 từ" với `ac-sim` và `ac-bench --viwiki`, ở 0 và 1 từ phía
sau; so với bộ sửa luật hiện tại ở cùng mức đổi nhầm (mục 3.3). Chọn checkpoint theo F1 của "sửa đúng" ở ngưỡng
tự tin 0,9 trên VSEC dev.

**Tăng dữ liệu bằng LLM mở (T2, 2026-10-05).** Danh sách quy tắc đầy đủ và mở: `tools/kaggle/AUGMENT_RULES.md`. Mục đích: thêm lỗi "từ hợp lệ nhưng sai theo ngữ cảnh", loại khó
nhất và chỉ có 1520 ca thật (Viwiki) để đo. Nguyên tắc: **LLM chỉ chọn lỗi nào để tạo, không bao giờ quyết định
cái gì đúng**; đáp án luôn là câu gốc, nên nhãn không bị nhiễu. Cách làm: với một âm tiết, bộ sinh ứng viên liệt kê
các âm tiết hợp lệ dễ nhầm (cùng chữ khác dấu, hoặc lệch một chữ), một mô hình ngôn ngữ nhân quả cỡ 2B đến 3B
(mặc định `Qwen/Qwen2.5-3B`, chưa kiểm tra có sẵn) chấm xác suất câu đã thay, và giữ một lỗi mà câu vẫn đọc được
nhưng kém câu gốc ít nhất 1 nat (càng gần câu gốc càng dễ được chọn: ca khó). Câu mà LLM thích bản thay hơn bản
gốc thì bỏ. Chạy hai tiến trình độc lập, mỗi tiến trình một GPU T4, mỗi tiến trình một nửa số câu (gần gấp đôi
tốc độ, không cần chia một mô hình qua hai GPU). Tập đo vẫn chỉ là lỗi thật (VSEC dev, Viwiki); lợi ích phải được
chứng minh bằng đối chứng có và không có dữ liệu LLM trên đường cong Viwiki. Chưa làm: cho LLM tự *viết* lỗi
(mô hình nhỏ dễ bịa), văn bản terminal và code, từ chuyên ngành.

**Lần chạy thử đầu của T2 (2026-10-06, Kaggle 2 × Tesla T4, `Qwen/Qwen2.5-3B`, 5 000 câu).** Chạy được, hai tiến trình
thoát mã 0, khoảng 10 phút kể cả 2 phút nạp mô hình; đo được 6 đến 7 câu mỗi giây mỗi GPU (cho 4 962 lỗi, tức 99% số
câu thử). Các lỗi sinh ra phần lớn đọc như lỗi người thật có thể mắc (`dự kiện thi` thay `dự kiến thi`, `Tất ca` thay
`Tất cả`, `đàm phám` thay `đàm phán`, `Thanh thinh` thay `Thanh Thịnh`). Hai vấn đề:
(1) **Quá dễ:** độ lệch trung vị 11,0 nat; chỉ 13% lỗi dưới 4 nat (khoảng 640 mẫu) và 3,8% dưới 2 nat. Tham số
`min_margin = 1` hầu như không lọc gì (giữ 99%): mỗi câu chỉ thử một vị trí, và đa số âm tiết thay vào đều sai rõ.
(2) **Cơ cấu loại lỗi lệch so với lỗi thật:** 75% là lệch một chữ, 25% chỉ khác dấu hoặc thanh, trong khi lỗi thật
(VSEC) khoảng 65% là chỉ sai dấu hoặc thanh. Một phần vì ứng viên chỉ lấy từ âm tiết hợp lệ trong từ vựng.
Cách sửa dự kiến: thử nhiều vị trí mỗi câu và lấy ca có độ lệch nhỏ nhất, hạ nhiệt độ chọn, phân tầng ứng viên theo
tỉ lệ loại lỗi của VSEC. Dữ liệu và kernel nằm trong tài khoản Kaggle (riêng tư): dataset `autocorrect-train`,
kernel `autocorrect-t2-probe`.

**Trò (chưa viết).** Huấn luyện trên *xác suất mềm* của thầy (top 8 âm tiết và xác suất loại lỗi, xuất sẵn trong
`soft_labels.jsonl`), cùng định dạng đầu vào và đầu ra với thầy. Đo khoảng cách thầy–trò trên cùng đường cong.

**Giới hạn của v1, nói thẳng:**
- Lỗi sinh ở mức văn bản (dấu, thanh, chữ), **chưa** ở mức phím Telex, Backspace hay dính từ: `ac-sim` làm được
  phần đó nhưng chưa có đường xuất cho huấn luyện. Dính từ vẫn do bộ tách luật lo.
- Chỉ âm tiết tiếng Việt; từ tiếng Anh, số, tên riêng được để nguyên (nhãn "giữ").
- Môi trường luôn là `normal`; chưa có dữ liệu cho terminal, code.
- Thầy học từ phân phối lỗi sinh, nên thừa hưởng độ lệch của nó (mục 3.4: cần hiệu chỉnh bằng lỗi thật).
- Mã chưa chạy trên GPU; chỉ có kiểm tra khói trên CPU (ghép nhãn, loss, dự đoán).

### 5.x Cải thiện mô hình trò (bạn chốt 2026-10-08: tạm giữ cấu hình hiện tại, tiếp tục cải thiện)

**Điều đã đo (bộ giả lập gõ, lỗi bấm phím, 140.059 từ đúng và 13.535 từ sai):** mô hình trò giúp ở nhóm lỗi mà n-gram bỏ qua (từ gõ sai thành từ hợp lệ khác: sửa đúng 9,9% → 18,0%), nhưng cái giá là đổi nhầm từ đúng. Quét `tau`: ở 0,99 mỗi lần sửa đúng thêm đi kèm 1,5 lần nhầm; 0,999 còn 0,7 nhưng chỉ thêm 111 lần đúng; 0,9999 gần như không có mô hình trò. Đường cong phẳng quanh 0,7 đến 0,8: nâng `tau` chỉ bớt cả hai, nên cần **mô hình tốt hơn**, không phải ngưỡng khác. **Cập nhật (2026-10-08, sau khi đo lại với mô hình v2 đang chạy trong app):** v2 (huấn luyện với phím giữ lặp) tệ hơn mô hình đầu (v1) trên phép đo này: ở cùng `tau`, số lần nhầm gần như nhau nhưng sửa đúng thêm chỉ bằng nửa hoặc ít hơn (tau 0,99: 115 so với 230; 0,999: 91 so với 111). Nhầm thêm trên mỗi lần đúng thêm của v2: 3,4 (0,99), 6,2 (0,995), 1,5 (0,999), 3,0 (0,9999); của v1: 1,5, 1,9, 0,7, 0,8. Các phép đo trước trong mục này dùng v1. **Mặc định `tau` đã đổi lên 0,999** (Cân bằng 0,999, Cẩn thận 0,9999, Mạnh tay 0,99). v2 còn sửa được ca giữ nhiều phím (`nguuu`) mà phép đo này không nhắm tới.

**Hướng, theo thứ tự (chưa làm):**
1. **Dữ liệu phủ định cứng.** Chạy động cơ cùng mô hình trò trên câu sạch trong miền (T3) và trên câu gõ giả lập, thu mọi từ đúng bị đổi nhầm, làm mẫu huấn luyện với nhãn GIỮ. Hiện mô hình trò chưa từng thấy chính các lỗi của nó.
2. **Cửa sổ giống lúc dùng thật.** Lịch sử ≤ 4 từ, không dấu câu, 1 từ bên phải (đã đo: cửa sổ cả câu lạc quan hơn trong app); thêm lỗi mức phím từ `ac-sim`, không chỉ lỗi mức chữ.
3. **Nhắm vào nhóm bị bỏ qua** (từ gõ sai thành từ hợp lệ): đây là chỗ mô hình trò đem lại nhiều nhất; đo riêng nhóm này khi so sánh các phiên bản.
4. **Đo bằng cùng thước:** quét `tau` trên bộ giả lập (`ac-sim --student`), số lần nhầm thêm trên mỗi lần đúng thêm, Viwiki, và nhật ký thật; mô hình mới chỉ thay khi tỉ lệ này thấp hơn rõ rệt ở cùng `tau`.
5. Về sau: thầy lớn hơn hoặc chưng cất nhiều vòng nếu 1 đến 3 chưa đủ.

### 5.y Hướng bạn đề xuất (2026-10-08): cỡ mô hình vừa đủ, học liên tục, thích ứng cá nhân hóa

Mục tiêu: không phải huấn luyện lại từ đầu mỗi lần mà vẫn học tiếp từ cách người dùng gõ. Ghi ở đây các điểm cần nghiên cứu, **chưa có thí nghiệm nào**:
1. **Cỡ mô hình vừa đủ.** Mô hình trò hiện 13,9 triệu tham số (56 MB fp32, ~15 ms/cửa sổ trên CPU 4 luồng). Phần đọc ngữ cảnh (3 tầng, d=256) chỉ khoảng 2 triệu tham số theo ước tính; phần lớn còn lại là bảng nhúng chữ cái băm, nhúng từ và đầu ra 6.796 lớp. Cần quét cỡ (nhúng, số tầng, d, số lớp) trên cùng đường cong, và thử nén (fp16, int8 chỉ cho trò; với thầy int8 đã làm mất một nửa độ thu hồi), để biết cỡ nhỏ nhất không mất chất lượng.
2. **Cá nhân hóa theo thứ tự rẻ đến đắt:** (a) đếm n-gram riêng của người dùng cộng vào bảng Kneser-Ney (không cần gradient); (b) mô hình lỗi riêng: ma trận nhầm phím và chi phí lỗi học từ nhật ký (hiện chi phí `slip.cost` đặt tay); (c) hiệu chỉnh xác suất của mô hình trò theo người dùng (vài tham số, ví dụ chỉnh `tau` hoặc độ lệch của lớp GIỮ); (d) bộ chỉnh nhỏ (adapter) trên mô hình trò đóng băng, chỉ khi (a) đến (c) chưa đủ.
3. **Tín hiệu từ người dùng, ngầm:** Ctrl+Z ngay sau lần sửa (nhãn xấu), lần sửa mà người dùng đi tiếp (nhãn tốt yếu), sửa tay (EDIT: từ đúng), bỏ sót mà sau đó người dùng sửa tay. Hiện đã có dạng sơ khai: hoàn tác hai lần thì bỏ qua từ đó mãi.
4. **Rủi ro cần đo:** quên cái cũ khi học tiếp (cần bộ đệm phát lại hoặc đóng băng phần lõi và giới hạn mức ảnh hưởng của phần học theo người dùng); tín hiệu nhiễu (nhật ký có cả lần bạn thử nghiệm với app); độ lệch chọn lọc (chỉ thấy những ca đã được sửa); ít dữ liệu (nhật ký hiện chỉ khoảng 200 lần sửa) nên phương pháp cần ít tham số; dữ liệu cá nhân chỉ ở máy.
5. **Cách đo:** dùng `ac-sim` làm người dùng giả với thói quen khác nhau (hồ sơ gõ khác, ví dụ phím lân cận khác), vẽ đường học: sửa đúng và đổi nhầm theo số sự kiện đã thấy, rồi mới thử trên nhật ký thật.
Điều kiện chung: chỉ bật khi đường học cho thấy lợi ích ở cùng mức đổi nhầm; không đổi mặc định khi chưa đo.

## 6. Mốc

| Mốc | Nội dung | Trạng thái |
|---|---|---|
| R0 | Dấu đóng `) ] } "` kết thúc từ như dấu phẩy (yêu cầu #5) | Xong trong mã (sim xác nhận `saoi)` → `sao`), chờ thử thật |
| R1 | Sim v1: người gõ, engine thật, màn hình, làn đối chứng, Ctrl+Z, xuất JSONL | Xong, kết quả ở mục 3.1 |
| R2 | Hiệu chỉnh người gõ bằng lỗi thật (cần `journal_edits`) | Chờ dữ liệu |
| R2b | Thiếu dấu cách (#6): tách k âm tiết (quy hoạch động), engine thay một từ bằng k từ, đo theo k bằng sim | Bản đầu xong (mục 3.2); thiếu: lỗi gõ bên trong chuỗi dính, học cách chọn |
| R3 | Mô hình học cho yêu cầu #1 và #4 (lỗi từ hợp lệ theo ngữ cảnh, có để nguyên): hướng đã chốt là n-gram + bộ kết hợp học được + mạng trò (mục 5) | Bước 1 (bộ kết hợp n-gram) là việc tiếp theo; thầy v1 đã viết, chưa chạy |
| R4 | Thích nghi môi trường và học trực tuyến (#2) | |
| R5 | Ngữ pháp (#3) | |

## 7. Câu hỏi mở

- Môi trường chia mịn đến đâu: theo app, theo loại ô nhập, hay theo bằng chứng của từng dòng
  (terminal vừa gõ lệnh vừa chat với Claude Code)?
- Nguồn câu cho prompt và lệnh shell: tự sinh, hay dùng lịch sử lệnh của bạn (chỉ khi được phép)?
- Ngân sách độ trễ cho mô hình học: bao nhiêu ms mỗi từ là chấp nhận được?

## 8. Tài liệu tham khảo

- **Tran, Dinh, Phan, Nguyen (2021), "Hierarchical Transformer Encoders for Vietnamese Spelling Correction"**
  (arXiv 2105.13578). Mô hình: bộ mã hóa Transformer hai tầng (ký tự: 4 lớp, ẩn 256; từ: 12 lớp, ẩn 768),
  hai đầu phân loại chung một mô hình (phát hiện lỗi có hay không cho từng token, và gợi ý sửa từ từ vựng),
  loss là tổng hai cross-entropy, từ đúng không tính vào loss sửa. Dữ liệu huấn luyện sinh bằng luật từ khoảng
  3 GB văn bản (tin tức, Wikipedia, phụ đề), chia lỗi làm ba nhóm: gõ sai (thêm, thiếu, thay), lỗi chính tả
  theo vùng miền, thiếu dấu. Dùng ngữ cảnh hai phía. Số liệu họ báo cho bộ thử lỗi thật từ bản nháp Wikipedia
  (1500 lỗi trong 14 000 câu): precision 66,96%, recall 70,92%, F1 68,88%; thêm dấu cho phụ đề: F1 99,75%.
  Hạn chế họ nêu: không xử lý được các từ dính nhau hoặc từ viết tắt vì mỗi token chỉ ra một token. Họ không
  nói gì về thích nghi theo người dùng. (Mình đọc qua bản tóm tắt tự động của trang HTML, chưa đối chiếu
  từng con số với bản PDF.)
  Liên hệ với ta: (1) khớp với ý "đầu phân loại lỗi là đầu phụ của cùng một mô hình"; (2) cùng cách sinh lỗi
  bằng luật rồi học, nhưng họ làm trên văn bản đã gõ xong, không có chuỗi phím Telex, Backspace, Ctrl+Z;
  (3) ngay cả mô hình lớn cũng chỉ đạt F1 khoảng 69% trên lỗi thật, cho thấy đánh đổi chính xác/độ phủ ở mục
  1.2 là có thật; (4) mô hình 12 lớp 768 ẩn quá nặng cho chạy theo từng từ trên CPU, nên chỉ hợp làm "thầy"
  để chưng cất; (5) lỗi từ dính nhau (#17) là chỗ họ cũng bó tay, ta đã có bộ tách bằng luật.
- **Gupta (2019), "A context sensitive real-time Spell Checker with language adaptability"** (arXiv 1910.11242, Amazon).
  Gần với ta nhất về tinh thần: thời gian thực, dựa trên n-gram, thích ứng ngôn ngữ (dựng bảng từ Wikipedia và phụ đề,
  24 ngôn ngữ). Sinh ứng viên bằng thuật toán xóa đối xứng (khoảng cách sửa tối đa 2), xếp hạng bằng
  `S = W1·P(w) + W2·P(w | trước) + W3·P(w | hai từ trước)`; **không có mô hình học sâu**, chỉ xử lý lỗi "từ không có
  trong từ điển" (từ dài hơn 2 ký tự), tác giả nêu lỗi từ hợp lệ và từ ghép là việc tương lai. Độ trễ: phát hiện khoảng
  7 micro giây mỗi từ, sinh ứng viên 0,4 đến 50 ms, xếp hạng 1 đến 3 ms. Ba cách sinh lỗi mô phỏng: ký tự ngẫu nhiên,
  đảo hai ký tự kề nhau, và thay ký tự theo xác suất bigram ký tự (gần lỗi kề phím nhất). Trên lỗi thật tiếng Anh: hạng 1
  đạt 68,99% so với Aspell 60,82% và Hunspell 61,34%; trên dữ liệu mô phỏng hạng 1 từ 80% trở lên cho cả 24 ngôn ngữ.
  Nhận xét: trọng số n-gram phải cân bằng (đẩy một bậc lên quá cao thì độ chính xác tụt), đúng chỗ bộ kết hợp học được
  của ta thay cho việc dò tay. Không có Telex, không có phím lúc gõ, không có hoàn tác. (Mình đọc nội dung các trang
  1 đến 6 của bản PDF; chưa đọc phần tài liệu tham khảo của họ.)
