# Quy tắc tăng dữ liệu bằng LLM (T2)

Đây là **danh sách mở**. Nó ghi mọi quy tắc quyết định *lỗi nào được sinh ra và giữ lại* để dạy thầy (T1).
Không có quy tắc nào ở đây là "giới hạn cuối": thêm quy tắc mới bằng cách thêm một mục có số thứ tự, trạng thái và nguồn.
Mã tương ứng: `t2_lm_negatives.py` (chọn lỗi bằng LLM), `t1_data.py` (bộ sinh lỗi và đo thao tác).

**Trạng thái:** `Đã cài` (có trong mã và có test) · `Đề xuất` (bạn yêu cầu hoặc mình đề xuất, chưa viết mã) · `Chưa kiểm chứng` (có giả thuyết, chưa đo trên dữ liệu thật)

**Nguồn:** `Dữ liệu` (đo từ lỗi thật) · `Bạn` (bạn nêu từ hiểu biết hoặc kinh nghiệm gõ) · `Mình` (mình đặt, chưa có bằng chứng)

**Nguyên tắc bất di bất dịch (không phải quy tắc để thêm bớt):**
- Đáp án luôn là **câu gốc** trong kho văn bản. LLM không quyết định cái gì đúng. Với T2 nó chỉ **chấm điểm** các câu ta đưa vào. *Cập nhật 2026-10-08, theo yêu cầu của bạn:* LLM còn được **viết câu sạch** (T3, `t3_domain_text.py`) và **đề xuất lỗi theo từng luật** (T4, `t4_llm_errors.py`, xem `AUGMENT_PROMPT.md`), nhưng đáp án vẫn là câu gốc và mọi lỗi phải qua bộ kiểm tra riêng của luật; cái chương trình không kiểm chứng được thì bị bỏ.
- **Tập đo chỉ gồm lỗi thật** (VSEC dev, Viwiki, sau này nhật ký và dữ liệu gõ của bạn). Không bao giờ đo trên lỗi do LLM hay luật sinh.
- Lỗi thật của Viwiki **không bao giờ** dùng để huấn luyện hay rút quy tắc.

---

## A. Quy tắc đã cài

### A1. Chọn vị trí
- Mỗi câu thử tối đa **4 âm tiết**, chọn ngẫu nhiên trong những âm tiết thuộc từ vựng 6 795 âm tiết (`data/vi_syllables.tsv`) và dài từ 2 ký tự trở lên.
- Nguồn: Mình. Tham số: `--positions 4`.

### A2. Nguồn ứng viên lỗi cho một âm tiết (tối đa 8, theo thứ tự ưu tiên)
1. **Lỗi đã thấy thật:** tối đa 4 cặp (đúng → sai) có trong phần huấn luyện của VSEC cho chính âm tiết đó, rút theo số lần xuất hiện. Ví dụ `của` ← `cuả` (24 lần), `cùa` (12), `cua` (8). Gắn cờ `seen`. Nguồn: Dữ liệu. Phủ 1 382 âm tiết, 3 481 cặp.
2. **Thao tác sinh theo tần suất đo thật** (xem A3). Nguồn: Dữ liệu.
3. Tối đa 2 **âm tiết hợp lệ cùng chữ cái nhưng khác dấu** (ví dụ `ốm` ↔ `ôm`), để có ca khó. Nguồn: Mình.

### A3. Các thao tác sinh lỗi và tần suất (đo trên 8 645 lỗi của phần huấn luyện VSEC, 2026-10-06)

| Thao tác | Số ca | Ví dụ | Mô tả |
|---|---|---|---|
| `tone_drop` | 2 294 | `việt` → `viêt` | thiếu dấu thanh |
| `tone` | 2 081 | `quý` → `quỷ` | nhầm sang thanh khác |
| `omit` | 1 285 | `nhiên` → `nhiê` | thiếu một chữ |
| `mark_drop` | 855 | `được` → `dược` | thiếu dấu mũ, móc hay nét của `đ` |
| `tone_add` | 521 | `viêt` → `viết` | thêm thanh vào chữ không có thanh |
| `insert` | 298 | | thừa một chữ lạ |
| `mark_add` | 294 | `của` → `cửa` | thêm dấu mũ, móc |
| `substitute` | 267 | | thay chữ bằng chữ xa trên bàn phím |
| `tone_place` | 236 | `của` → `cuả` | đặt thanh sai nguyên âm |
| `neighbour` | 212 | | thay bằng phím kề |
| `extra` | 176 | | thừa một phím kề |
| `double` | 108 | `tranh` → `tranhh` | gấp đôi chữ |
| `swap` | 18 | | đảo hai chữ liền nhau |

Bộ sinh lỗi dùng đúng tỉ lệ này (`KIND_WEIGHTS`). Ghi chú: các con số này lẫn thói quen **vùng miền** (đọc nhầm `t`/`c`, `n`/`l`, `hỏi`/`ngã`) chứ không chỉ trượt tay.

### A4. Những thứ bị loại
- Ứng viên trùng nhau, hoặc trùng với từ gốc.
- **Quy ước đặt dấu** ở vần `oa`, `oe`, `uy` (`thoả` ↔ `thỏa`, `hoà` ↔ `hòa`, `thuỷ` ↔ `thủy`): bộ sửa của ta chấp nhận cả hai, nên không dạy sửa và không tính là lỗi (`tone_convention`). Nguồn: Dữ liệu (423 ca, 4,7%) và quyết định thiết kế.
- Đặt thanh sai ở các vần khác (`cuả`, `qủa`) là **lỗi thật**, vẫn được dạy (`tone_place`).

### A5. Chấm điểm và chọn
- Thay ứng viên vào đúng chỗ trong câu gốc (giữ dấu câu, khoảng trắng, chữ hoa). LLM (mặc định `Qwen/Qwen2.5-3B`) chấm log xác suất cả câu gốc và từng câu đã thay. `margin = log P(gốc) − log P(thay)`, tính bằng nat.
- **Bỏ** ứng viên có `margin < 0,5` (LLM thích bản thay hơn hoặc ngang bản gốc: không ai khẳng định được bản gốc đúng hơn). Tham số `--min-margin 0.5`.
- Mỗi câu giữ **tối đa 2 lỗi, ở hai vị trí khác nhau** (`--per-sentence 2`).
- Xác suất được chọn tỉ lệ với `ưu tiên × exp(−margin / 1,5)` (`--temperature 1.5`): margin càng nhỏ (ca càng khó) càng dễ được chọn. `ưu tiên` = 3 cho cặp đã thấy thật, 1 cho cặp khác (`SEEN_PRIOR`). Nguồn: Mình.

### A6. Lọc câu gốc (`prepare_corpus.py`)
- Từ 4 đến 40 từ, ít nhất 90% là âm tiết thuộc từ vựng.
- Bỏ 20 000 dòng cuối của mỗi kho (tập đo của `ac-bench`).
- Phụ đề là tỉ lệ nhỏ vì có lỗi OCR (`Khá Iắm`).

### A7. Nhãn của mẫu
- Nhãn sửa: âm tiết gốc. Loại lỗi: thao tác (A3). Cờ `seen`. `margin` và `lp_orig` được ghi lại để phân tích.

---

## B. Quy tắc bạn yêu cầu thêm (2026-10-06) — **chưa cài**

Bạn nêu các ví dụ sau và nói rõ đây chỉ là **một vài trường hợp**, không phải toàn bộ. Mình ghi nguyên ý của bạn, kèm trạng thái và bằng chứng.

### B1. Nhầm phím theo logic của bàn phím
- **Nội dung (nguyên ý bạn):** gõ sai không phải bừa, mà phải có logic bàn phím. Ví dụ phím `t` hay lẫn với các phím lân cận `r`, `y`, `g`, và có **ngoại lệ nặng hơn** là `e`, `u`, `f`, `h`.
- **Trạng thái:** **Đã cài ở dữ liệu T4 (luật 10, `AUGMENT_PROMPT.md`)** ở mức chữ trên màn hình, với bộ kiểm tra riêng (một chữ thành phím kề QWERTY; riêng `t` thêm `r`, `y`, `g`, `e`, `u`, `f`, `h`). **Chưa ở bộ gây lỗi bằng luật `Corruptor`**, và chưa ở mức phím. **Nguồn:** Bạn.
- **Cách áp dụng dự kiến:** ma trận "phím định gõ → phím đã gõ" có trọng số: tiên nghiệm theo hình học (khoảng cách, cùng tay, cùng ngón), cộng các trọng số bạn đặt riêng cho từng phím (ví dụ `t`: `r`,`y`,`g` có trọng số, `e`,`u`,`f`,`h` trọng số **cao hơn**). Sau này cập nhật bằng dữ liệu gõ thật của bạn.
- **Bằng chứng từ dữ liệu:** trong VSEC, nhầm chữ cái thuần chỉ có **354 ca (khoảng 4% tổng lỗi)**; bỏ các cặp đọc nhầm (`t↔c`, ...) còn 276 ca, trong đó **37,7% là phím kề, so với 9,8% nếu ngẫu nhiên**; cùng tay 70% (ngẫu nhiên 49%); cùng ngón 18% (ngẫu nhiên 12,6%). Riêng phím `t`: chỉ thấy `c` (45 lần, là đọc nhầm) và `i`, `n`, `p`, `h`, `v`, `y`, `k` mỗi phím không quá 4 lần. **VSEC không đủ để xác nhận hay bác bỏ mẫu `r, y, g, e, u, f, h` của bạn**, nên coi đây là tiên nghiệm của bạn, cần dữ liệu gõ thật để đo.

### B2. Trượt tay dài, đè nhiều phím liền nhau
- **Nội dung (nguyên ý bạn):** ngoài nhầm một phím, tay có thể bị **trượt dài** khiến luôn mấy phím đi liền nhau; hoặc **một ngón đè một lần 2 đến 3 phím** cũng là lỗi.
- **Trạng thái:** **Đã cài ở dữ liệu T4 (luật 12 trượt tay, luật 13 đè nhiều phím)**, mức chữ trên màn hình, mỗi luật có bộ kiểm tra riêng. Chưa ở `Corruptor`, chưa ở mức phím. **Nguồn:** Bạn.
- **Cách áp dụng dự kiến:** hai thao tác mới ở mức phím: (a) *trượt*: chèn 1 đến 3 phím kề **liên tiếp** quanh phím định gõ; (b) *đè nhiều phím*: chèn 1 đến 2 phím kề cùng lúc, thứ tự trước hay sau phím đúng là ngẫu nhiên. Cả hai chạy qua bộ ghép Telex để ra chữ trên màn hình.
- **Bằng chứng:** VSEC không có thời điểm gõ nên không đo được. Chỉ biết 16% lỗi là thừa phím (phần lớn là phím thanh), không phân biệt được trượt hay đè.

### B3. Đảo thứ tự khi gõ nhanh
- **Nội dung (nguyên ý bạn):** gõ chậm thì nhập lần lượt các phím của `thường` là `t` → `h` → `u` → `o` → `n` → `g` → `w` → `s`; nhưng gõ nhanh thì chữ `o` có thể bị gõ **trước** chữ `u`.
- **Lưu ý khi ghi lại:** với `thường` (dấu huyền) phím thanh Telex là `f`, không phải `s`, và `w` thường đi ngay sau `o`/`u`. Mình ghi theo ví dụ của bạn để minh họa thứ tự; bạn xác nhận lại phím thanh khi cài.
- **Trạng thái:** **Đã cài ở dữ liệu T4 (luật 14, đảo hai chữ liền nhau)**, mức chữ trên màn hình, có bộ kiểm tra riêng. Ở mức chữ **không** thấy được phím thanh và việc Telex ghép lại, nên "o trước u" của `thường` chưa phản ánh đúng. Chưa ở `Corruptor`, chưa ở mức phím. **Nguồn:** Bạn.
- **Cách áp dụng dự kiến:** thao tác *đảo hai phím liền nhau* ở mức phím, xác suất tăng khi gõ nhanh và khi hai phím do hai tay khác nhau gõ. Phải chạy qua bộ ghép Telex: nhiều lần đảo **không để lại lỗi** vì Telex ghép lại cho ra cùng một chữ (`thuowng` và `thuonwg` đều ra `thương`), và có lần đảo đổi cả chữ (phím thanh gõ sớm).
- **Bằng chứng:** trong VSEC chỉ có **15 hoán vị (0,2%)**, chủ yếu cặp nguyên âm (`oa` 4, `ia` 3) và `an` (4). VSEC là văn bản đã viết xong, không phải lúc gõ nhanh, nên **con số này không phản ánh bạn**.

### B4. Điều kiện kỹ thuật chung cho B1 đến B3
Cả ba cần **mô phỏng ở mức phím rồi chạy qua bộ ghép Telex thật** (`ac-sim` và `ac-telex`), không làm được bằng sửa chữ trên văn bản. LLM vẫn chỉ chấm điểm các câu kết quả theo quy tắc A5.

### B5. Thiếu phím Space: dính 2, 3 hoặc 4 từ liền nhau (bạn yêu cầu 2026-10-08)
- **Nội dung (ý bạn):** từ `quan hệ` thành `quanheej`, `đi chơi` thành `đichơi`...; tổng quát cho **2, 3 hoặc 4 từ dính liền**, không riêng hai từ.
- **Trạng thái:**
  - **Đã cài ở `ac-sim`** (gõ chuỗi phím qua động cơ thật): đoạn 2 đến `join_max` từ (mặc định 4, nâng được bằng `--join-max`); từ trong đoạn có thể là **tiếng Việt, tiếng Anh, tên riêng hoặc số** (đoạn có từ không phải âm tiết tiếng Việt được tính riêng: `space-missing-mixed`); **nhiều đoạn trong một câu** (`--join-more`, `--join-runs`); **lỗi phím ngay trong đoạn** (`--join-slip`). Hai đoạn không chạm nhau (giữa chúng vẫn còn một phím cách). Tỉ lệ mặc định là giả định: 15% số câu có đoạn dính, 30% có thêm một đoạn nữa, 25% đoạn có lỗi phím riêng.
  - **Chưa có** ở bộ gây lỗi bằng luật (`Corruptor`), ở lỗi do LLM chọn (T2), ở prompt LLM theo luật (T4, không biểu diễn được vì đổi số từ của câu), và ở dòng huấn luyện của thầy và trò: nhãn của một đoạn dính là **nhiều từ**, mà đầu ra của thầy và trò chỉ nhận một âm tiết hay "giữ nguyên".
  - Ở động cơ, đoạn dính do **bộ cắt âm tiết** (`split_run`, n-gram) xử lý; nó chỉ chạy trên đoạn gồm âm tiết tiếng Việt.
- **Bằng chứng** (2026-10-08, `ac-sim`, 3.000 câu mỗi kho, `--join 0.6 --join-more 0.4`, tỉ lệ sửa đúng): đoạn **toàn tiếng Việt** 2 từ: 49% (tin tức), 44% (hội thoại), 24% (mạng xã hội); 3 từ: 35%, 27%, 11%; 4 từ: 24%, 19%, 3%. Đoạn **có từ khác tiếng Việt**: **0% ở mọi độ dài** (bỏ sót 92 đến 100%).
- **Yêu cầu tổng quát (bạn, 2026-10-08):** cần khả năng tổng quát cho **mọi số từ dính liền**, không chỉ định trước số từ. Nguyên tắc rút ra: **số từ của đoạn không bao giờ là đầu vào của bài toán**; nó chỉ là cận trên của dữ liệu sinh ra (`--join-max`).
  - **Bộ cắt của động cơ đã không nhận số từ:** `split_run` tìm mọi cách cắt chuỗi phím thành các âm tiết tiếng Việt (beam 6, âm tiết 2 đến 9 phím) và chọn cách nổi bật nhất. Đo ở `ac-sim` với `--join-max 8`, **không có lỗi phím trong đoạn**, tỉ lệ sửa đúng cho đoạn toàn tiếng Việt, theo 2, 3, 4, 5, 6, 7, 8 từ: tin tức 69, 48, 34, 28, 13, 7, 11%; hội thoại 61, 40, 26, 16, 9, 0, 3%; mạng xã hội 28, 15, 6, 3, 2, 2, 2%. Tức hoạt động với mọi độ dài nhưng **giảm dần theo độ dài**: luật "tất cả hoặc không gì" (mọi ranh giới phải đủ chắc) làm xác suất thành công là tích các xác suất từng ranh giới, và càng dài càng nhiều cách cắt cạnh tranh.
  - **Lỗi phím nằm trong đoạn làm mất khoảng 20 điểm** (đoạn 2 từ: 49% có lỗi phím trong đoạn so với 69% không có).
  - **Cách tổng quát cho mô hình học:** gán nhãn ranh giới (ở mỗi vị trí giữa hai phím dự đoán "có phím cách ở đây không"), không phụ thuộc số từ và không "tất cả hoặc không gì". Khi sinh dữ liệu, độ dài đoạn phải lấy từ **một phân phối đuôi dài** (hai từ phổ biến nhất, giảm dần, vẫn có đoạn rất dài, đến cả câu), không phải vài độ dài cố định hay chọn đều.
  - **Tình trạng mã (xong 2026-10-08):** độ dài đoạn lấy từ luật đuôi dài: 2 từ, và mỗi từ thêm với xác suất `join_continue` (mặc định 0,55, cờ `--join-continue`) đến khi gặp `join_max` (mặc định 12, trước là 5) hoặc hết từ. Chạy thử 1.500 câu, `--join 0.8`: số đoạn giảm dần theo độ dài (tin tức 684, 288, 148, 70, 28, 8, 8, 3 đoạn cho 2 đến 9 từ), có đoạn đến 11 từ. Tỉ lệ sửa đúng giảm theo độ dài như đã đo ở trên. 17 test của `ac-sim` đạt.
- **Cần làm tiếp (đề xuất):** (a) một đầu ra "cắt từ" cho trò (hoặc bộ cắt học được) để trò cũng học được dính từ, và mở rộng bộ cắt cho đoạn lẫn tiếng Anh và số; (b) dữ liệu tách nhãn nhiều từ cho huấn luyện; (c) hiệu chuẩn tỉ lệ và độ dài đoạn bằng nhật ký và dữ liệu gõ thật của bạn (hiện đều là giả định).

---

## C. Việc cần làm để có dữ liệu cho phần B (đề xuất, chưa làm)
1. **Trang ghi phím** cho bạn gõ chép các câu mẫu công khai, ghi phím vật lý với thời điểm nhấn và nhả: đo trực tiếp nhầm phím, đè nhiều phím, đảo thứ tự khi gõ nhanh. Dữ liệu ở lại máy bạn.
2. Dùng nhật ký của bạn (`journal_edits`, `journal_hard`, `journal_right`) khi đã tích lũy.
3. Nâng bộ mô phỏng phím trong `ac-sim` theo B1 đến B3, với tiên nghiệm hình học và các trọng số bạn đặt.

---

## D. Câu hỏi mở (cần bạn quyết)
- Giá trị `min-margin` 0,5 nat có quá lỏng không? Ca có margin nhỏ có thể là câu thay cũng đúng (ví dụ `đã` thay `đó`), tức nhãn "gốc đúng hơn" gây tranh cãi.
- Có nên bỏ các ứng viên rác (`tuyn`, `nh`, `nhqà`) và chỉ giữ ứng viên là âm tiết hợp lệ hoặc đã thấy thật không? Hiện khoảng một phần tư lượt chấm đổ vào chúng.
- Với thói quen vùng miền (`t`/`c`, `n`/`l`, `hỏi`/`ngã`), ta dạy thầy sửa hay để nguyên? Đây là lỗi thật nhưng không phải trượt phím, và mỗi người một khác.

---

## F. Sửa nhầm thực tế từ nhật ký (guard; bạn yêu cầu 2026-10-08) — **F2 và F3 đã cài bằng luật cứng; F1 chưa**

**Đã cài (2026-10-08, `smart.rs`, `smart/split.rs`, tắt bằng `set_guards(false)` / cờ `--no-guards`):** (a) từ bắt đầu bằng phím `j`, `f`, `z` không bị sửa (`json`); (b) từ viết hoa toàn bộ chỉ đổi sang từ có điểm ≥ 9 (`LLM` giữ nguyên, `TEH`→`THE` vẫn được); sửa muộn bỏ qua hẳn từ viết hoa toàn bộ; (c) từ đứng trước là tiếng Anh và trong ứng viên một hay hai lỗi có từ tiếng Anh thì không đổi sang từ Việt (`vibe codin`); (d) cách cắt từ không nhận mảnh một chữ (`ti ế`, `Giơ ơ`). Đo: giả lập gõ, số từ đúng bị đổi nhầm /1000 giảm 9 đến 37% (tin tức 1,76→1,10) và sửa đúng mất 0,6 đến 1,8 điểm; Viwiki (107 tài liệu, sửa ngay + muộn) độ chính xác 27,5→31,4%, đổi nhầm /1000 1,29→1,07, sửa đúng giữ 15,1%; nhật ký thật: 8 lần sửa nhầm bị chặn, không mất lần sửa đúng nào trong 205 (lưu ý: các luật được thiết kế từ chính các ca này nên nhật ký không phải số đo độc lập). Thử bộ học (gradient boosting trên bằng chứng n-gram, huấn luyện trên giả lập gõ) không hơn ngưỡng đặt tay: luật cứng đảm nhận F2, F3.

Nguồn: `journal.tsv` (205 lần sửa; 10 lần bạn sửa lại ngay sau khi app sửa, cộng một số lần sửa sai rõ ràng). Tái hiện bằng `ac-bench --revise` bản hiện tại: cả 14 trường hợp ra đúng kết quả sai, đều ở bước **sửa ngay** (sửa muộn không sửa nhầm). `roff`/`boff` → `off` là lần sửa đúng (bạn xác nhận), không tính. Mỗi nhóm là một **lớp lỗi**, không vá từng từ; ví dụ chỉ để minh hoạ, danh sách mở.

### F1. Nhiều cách đọc ngang nhau, sửa ngay theo tần suất
- Ví dụ: `troonf`→`tròn` (trồng 7,99, trông 7,96, tròn 7,91), `dddoi`→`đi` (bạn muốn `đôi`), `phoiots`→`phối` (`phốt`), `chaas`→`chất` (`chấp`; thiếu phụ âm cuối nên có sáu cách đọc), `thuonge`→`thương` (`thường` không có trong danh sách ứng viên).
- Hướng chặn: khi từ đứng đầu chỉ hơn từ thứ hai ít, hoặc khi chỗ thiếu là phụ âm cuối, **hoãn sang sửa muộn** (chờ từ kế tiếp). Hiện tượng thiếu ứng viên (`thường`) là việc riêng ở bộ sinh ứng viên (tone sai kết hợp thiếu móc).
- Dữ liệu cho mô hình học: mẫu có nhiều đáp án gần nhau, nhãn là KEEP hoặc "hoãn", không ép chọn từ phổ biến nhất.

### F2. Từ tiếng Anh, từ kỹ thuật, từ viết tắt
- Ví dụ: `json`→`son` (`j` đầu từ bị coi là phím dấu thừa), `LLM`→`LL` (chữ hoa toàn bộ bị cắt chữ), `codin`→`con` sau `vibe` (ứng viên `coding` 3,4 thấp hơn `con` 6,1 dù từ trước là tiếng Anh), `ddc`→`dc` (giống teencode, mà teencode chỉ bật khi người dùng chọn, và `dc` không phải từ), `casse`→`cá` (6 lần trong môi trường Code). Bạn xác nhận: đây là hệ quả của việc sửa nhầm từ tiếng Anh **`case`**; có **hai kiểu gõ** cùng bị đưa về `cá`: `case` và `casse` (`casse` là cách gõ thoát dấu của Telex để ra `case`). `ac-bench --revise` bản hiện tại trả "không sửa" cho cả hai khi chỉ có ngữ cảnh ngắn, nên lỗi chỉ xảy ra ở môi trường Code với ngữ cảnh thật; cần tái hiện bằng đúng ngữ cảnh đó trước khi chặn.
- Hướng chặn: không sửa từ viết hoa toàn bộ; không sửa khi từ đứng trước là tiếng Anh và vẫn còn ứng viên tiếng Anh; không đổi sang chuỗi không phải từ điển.
- Dữ liệu: câu có thuật ngữ kỹ thuật và từ viết tắt tiếng Anh xen trong câu Việt (T3 miền kỹ thuật) với nhãn KEEP.

### F3. Bộ cắt từ cắt một từ thành hai
- Ví dụ: `Tiees`→`Ti ế`, `tiees`→`ti ế`, `Giowow`→`Giơ ơ` (môi trường Code); các cách đọc một từ điểm cao hơn (`tiếp` 9,1, `tiến` 8,3, `tiết` 8,0) vẫn thua.
- Hướng chặn: không nhận cách cắt có mảnh chỉ gồm một nguyên âm; không cắt khi đã có cách đọc một từ hợp lệ.
- Dữ liệu: từ đơn gõ có lỗi nhưng **không** dính từ phải có nhãn KEEP ở đầu ra "cắt từ" (xem B5), để mô hình học phân biệt dính từ thật với một từ gõ sai.

### F4. Cách kiểm
- Mỗi lớp có một bộ ví dụ cố định trong test của `ac-core`, nhưng chỉ để chống hồi quy; **số liệu đánh giá** lấy từ `ac-sim` (nhóm F2, F3: câu sạch tiếng Anh/kỹ thuật, tỉ lệ "app đổi từ đã đúng") và từ nhật ký thật (số lần sửa lại sau khi sửa). Mục tiêu: giảm số lần sửa nhầm mà không giảm số lần sửa đúng quá một mức bạn chốt.
- Chưa có số đo tác động; sẽ đo trước khi sửa động cơ.

## Nhật ký thay đổi
- 2026-10-08 (F, guard): ghi ba lớp sửa nhầm thực tế từ nhật ký (F1 ngang điểm, F2 tiếng Anh/kỹ thuật/viết tắt, F3 cắt từ nhầm). Chưa cài, chưa đo tác động.
- 2026-10-05: phiên bản 1 (một vị trí mỗi câu, ứng viên ngẫu nhiên từ từ vựng). Chạy thử trên Kaggle: 99% số câu được giữ, độ lệch trung vị 11 nat, quá dễ.
- 2026-10-06: phiên bản 2 (A1 đến A7): ứng viên từ lỗi thật và thao tác đo thật, 4 vị trí mỗi câu, ưu tiên ca khó. Phần B ghi lại theo yêu cầu của bạn, chưa cài.
- 2026-10-06 (phiên bản 2.1, sau kết quả tầng 1 trong RESEARCH.md mục 5, dồn sức vào dữ liệu): `pick_errors` đòi ứng viên là âm tiết hợp lệ thua gốc ít nhất 3 nat (`valid_margin`, mặc định 3,0; từ không hợp lệ vẫn 0,5) và cân các loại lỗi theo tỉ lệ đo (`KIND_WEIGHTS`) chia cho tỉ lệ của loại đó trong nhóm ứng viên của câu. Chưa chạy lại trên Kaggle (GPU còn lỗi); mới có test (21 qua). Mục D câu 1 (`min-margin`) được xử lý một phần bằng `valid_margin`; câu 3 (thói quen vùng miền) đã quyết (bạn, 2026-10-06): **dạy luôn**, không để nguyên; các cặp này có trong lỗi thật của VSEC nên đã vào danh sách nhầm lẫn thật (trọng số `SEEN_PRIOR`), không cần mã mới. Mục D câu 2 đã quyết (bạn, 2026-10-06): **không bỏ ứng viên rác** (không phải từ và chưa thấy trong lỗi thật, như `tuyn`, `nh`); vẫn dạy nhưng trọng số chọn `junk_weight` = 0,3 so với ca chính.

- 2026-10-07: thêm lỗi **phím giữ** vào `Corruptor` (`TRAIN_BOOST` `double` ×2, `RUN_SHARE` 35% là đoạn lặp 2 đến 4 chữ) sau khi `nguuu` không được xử lý; đọc nhật ký thật của bạn: sửa nhầm từ tiếng Anh gõ bằng cách bấm đôi phím dấu Telex (`tesst` thành `tết`) đã sửa ở `smart.rs`, chưa có trong bộ gây lỗi.
- 2026-10-08: **luật B1, B2, B3 vào dữ liệu T4** (`t4_llm_errors.py`, 15 luật, mỗi luật một lời nhắc và một bộ kiểm tra riêng, 19 test). Mức chữ trên màn hình, chưa mức phím (B4). Chưa chạy trên Kaggle. Xem `AUGMENT_PROMPT.md`.

- 2026-10-08 (B5, dính từ): tổng quát phần thiếu phím Space ở `ac-sim`: đoạn 2 đến 4 từ (mặc định), từ bất kỳ (Việt, Anh, tên, số), nhiều đoạn mỗi câu, lỗi phím trong đoạn; thêm kiểu `space-missing-mixed`. Ghi rõ ở mục B5 những chỗ còn thiếu (dòng huấn luyện của thầy và trò, T4) và kết quả đo (đoạn lẫn tiếng Anh/số: động cơ sửa 0%).
- 2026-10-08 (B5, yêu cầu tổng quát): ghi nhận yêu cầu "cần khả năng tổng quát" cho số từ dính liền (số từ không bao giờ là đầu vào; chỉ là cận trên của dữ liệu), kết quả đo đến 8 từ, hướng gán nhãn ranh giới và quy luật độ dài đuôi dài. Mã `join_continue` mới nối nửa chừng, chờ bạn xác nhận.
- 2026-10-08 (B4, mức phím): `ac-sim` (bộ mô phỏng gõ chuỗi phím Telex chạy qua động cơ thật) dùng **bố cục bàn phím thật của laptop bạn** (Dell Vostro 3405, ANSI, đo trên ảnh bạn gửi: `tools/kaggle/keyboard.py` và `layouts/dell_vostro_3405.json`, dùng chung cho Python và Rust, có test đối chiếu) cho lỗi phím kề, kèm thói quen riêng của phím `t` (`r y g` và nặng hơn `e u f h`), và có ba kiểu lỗi mới ở mức phím: **trượt tay** (2 đến 3 phím liền, `Slip::Slide`), **đè nhiều phím** (2 phím sát một phím, `Slip::Multi`), **giữ phím lặp 3 đến 4 lần** (`Slip::Held`). Các trọng số của ba kiểu mới là giả định (3, 3, 4 trên tổng khoảng 110), chưa hiệu chuẩn. Chạy thử 3.000 câu mạng xã hội: động cơ hiện bỏ sót 81% lỗi trượt tay, 53% đè nhiều phím và 55% giữ phím lặp (đây là chỗ cần dữ liệu huấn luyện). Phím Space, dấu câu, Shift, Caps Lock đã có trong bảng bố cục nhưng chưa có bộ sinh lỗi riêng ngoài `CapsHeld` và `SpaceMissing` có sẵn.

---

## E. Kết quả chạy thử phần A (2026-10-06)

**Điều kiện:** kernel Kaggle chỉ dùng CPU, mô hình `Qwen/Qwen2.5-0.5B` (không phải bản 3B dự định: GPU của Kaggle lỗi ngay khi khởi động, nguyên nhân chưa rõ, nghi hết hạn mức), 226 câu, đúng các tham số ở A1 đến A5. Vì dùng mô hình nhỏ hơn nên **margin không so được trực tiếp** với lần chạy phiên bản 1 (3B).

| Chỉ số | Kết quả |
|---|---|
| Lỗi giữ lại | 452 (đúng 2 mỗi câu) |
| Cặp (gốc, sai) khác nhau | 388 trên 452 |
| Cặp đã thấy trong lỗi thật (`seen`) | 307 trên 452 (68%) |
| Margin trung vị | 8,9 nat |
| Margin dưới 2 / dưới 4 / dưới 8 nat | 4,6% / 15,2% / 40,3% |

**Loại lỗi so với tần suất thật (A3):**

| Loại | Giữ lại | Thật |
|---|---|---|
| `tone` | 24,6% | 24% |
| `tone_drop` | 20,8% | 27% |
| `omit` | 15,9% | 15% |
| `tone_add` | **13,1%** | 6% |
| `mark_drop` | 10,4% | 10% |
| `tone_place` | **0,2%** | 2,7% |
| `neighbour`, `extra`, `double`, `insert`, `substitute` | 11% cộng lại | khoảng 15% |

**Nhận xét khi đọc 18 mẫu khó (margin dưới 3 nat):** phần lớn là lỗi người thật có thể mắc (`và ← vài`, `chị ← chỉ`, `sản ← sàn`, `khong ← không`, `án ← ăn`, `soai ← sai`). Khoảng **3 đến 4 trên 18 là mơ hồ, câu thay đọc cũng đúng**: `nó ← nói` ("nó như vậy"), `đây ← đấy`, `trông ← trong`. Nếu dạy thầy rằng gốc đúng hơn ở những ca này, thầy sẽ học cách **"sửa" chữ vốn đúng**: đó là loại lỗi tệ nhất của cả hệ thống.

**Vấn đề rút ra (chưa sửa, chờ bạn quyết):**
1. **Vẫn khá dễ:** chỉ khoảng 15% dưới 4 nat. Hạ nhiệt độ không cứu được vì phần lớn ứng viên là chuỗi không phải từ, LLM chấm rất thấp.
2. **Chọn theo độ khó làm lệch cơ cấu lỗi:** `tone_add` gấp đôi thật, `tone_place` gần như mất (vì `cuả` là chuỗi không phải từ nên luôn dễ). Cần chọn lại theo phân tầng loại lỗi.
3. **Nguy cơ nhãn mơ hồ ở nhóm khó nhất.** Đề xuất quy tắc mới (chưa cài): nếu chữ sai là một **từ hợp lệ phổ biến**, đòi `margin` cao hơn nhiều (ví dụ ít nhất 3 nat) mới giữ; nếu là chuỗi không phải từ thì giữ ngưỡng 0,5.

