# Prompt cho LLM sinh câu gõ sai: mỗi luật một lời nhắc riêng, viết cho mọi từ và mọi phím

File này để **đọc và phản biện luật**, chưa chạy gì trên Kaggle. Mã: `t4_llm_errors.py` (33 test) và bố cục bàn phím `keyboard.py` (10 test, xuất JSON ở `layouts/us_ansi_laptop.json` cho bộ mô phỏng Rust). Phần "Luật 1 đến 15" và prompt mẫu dưới đây
được **in thẳng từ mã** (`python t4_llm_errors.py --markdown` và `--show N`), nên khớp với thực tế. Sửa luật thì sửa danh sách `RULES` trong mã rồi in lại.
Liên quan: `AUGMENT_RULES.md` (luật của bộ gây lỗi bằng luật `Corruptor`, đang dùng để huấn luyện).

---

## 1. Cách làm, và vì sao viết như vậy

1. Lấy một **câu đúng** (câu thật cùng giọng, hoặc câu LLM viết lại).
2. Với mỗi câu, chọn ngẫu nhiên 2 luật (theo tỉ lệ ở bảng) và gửi **mỗi luật một yêu cầu riêng** cho LLM: chỉ một kiểu lỗi, có hướng dẫn từng bước. LLM không phải nhớ 15 luật cùng lúc.
3. **Chương trình kiểm tra từng phiên bản bằng bộ kiểm tra của đúng luật đã hỏi** (mục 4): lỗi phải đúng kiểu đó, không phải kiểu khác.
4. Đáp án luôn là câu đúng ban đầu.

**Viết cho mọi trường hợp, không chỉ những ví dụ đã nêu** (sửa theo góp ý của bạn 2026-10-08):
- **Bàn phím thật của bạn, mọi phím.** `keyboard.py` là bảng bố cục vật lý của laptop Dell Vostro 3405 (bố cục US, ANSI): mỗi phím có hàng, vị trí và bề rộng thật, gồm cả dấu câu, Shift, Caps Lock, Tab, Space, Enter, Backspace. Hai phím là sát nhau khi hai mặt phím chạm nhau trong một hàng, hoặc chồng lên nhau ít nhất một phần tư phím ở hàng trên hay dưới; phím rộng như Space chạm đúng các phím nó phủ lên (c v b n m , .). Mọi chữ cái và chữ số đều có phím kề (`a`: q w s z cùng Caps Lock và Shift; `l`: k o p và `;`). Trong một từ, lỗi mức chữ chỉ đặt được chữ cái và chữ số; Space, dấu câu, Shift, Caps Lock cần xử lý ở mức ký tự (xem mục 2).
- **Bảng phím sát nhau được tính sẵn và đưa vào prompt cho đúng các chữ của câu đó**, nên LLM chọn từ bảng thay vì phải nhớ hình học bàn phím (chỗ LLM nhỏ yếu).
- **Thói quen riêng của người gõ là một bảng ưu tiên, không phải luật.** Hiện chỉ có phím `t` (hay nhầm `r`, `y`, `g`, nặng hơn `e`, `u`, `f`, `h`), như một mục ví dụ; thêm phím khác là thêm một dòng trong `USER_PARTNERS`, hoặc đo bằng trang ghi phím.
- **Ví dụ không viết tay.** Mỗi lần gọi, mã chọn 3 câu khác nhau từ kho 30 câu đa dạng (đời thường, công việc, kỹ thuật, tên riêng, số) và tự sinh ví dụ bằng đúng bộ sinh của luật rồi kiểm tra; LLM thấy nhiều chữ, nhiều kiểu từ thay vì vài từ cố định.
- **Một bộ sinh dùng chung:** các bộ sinh này cũng tạo lỗi được bằng chương trình, không cần LLM (augment bằng luật cho các luật cấp phím).

**Nguyên tắc đã đổi so với `AUGMENT_RULES.md`:** trước đây "LLM không viết gì, chỉ chấm điểm". Giờ LLM được *đề xuất* lỗi, nhưng **không được quyết đúng sai**: đáp án là câu gốc và mọi lỗi phải qua bộ kiểm tra của luật.

---

## 2. Bảng đối chiếu: mọi luật bạn từng nêu

"Bộ gây lỗi bằng luật" là phần đang dùng thật để huấn luyện (`Corruptor` trong `t1_data.py`). "Dữ liệu T4" là dữ liệu mới theo từng luật (file này).

| Luật của bạn | Bộ gây lỗi bằng luật | Dữ liệu T4 (mỗi luật một prompt + kiểm tra riêng) | Ghi chú |
|---|---|---|---|
| Lỗi dấu thanh, dấu mũ, bỏ chữ... theo tỉ lệ đo trên lỗi thật | **Đã cài** | Luật 1 đến 9 | Tỉ lệ đo trên 8.645 lỗi VSEC |
| Thói quen vùng miền (t/c, n/l, hỏi/ngã): dạy luôn | Một phần (qua danh sách nhầm lẫn thật của VSEC ở T2) | **Luật 15** | Bạn quyết 2026-10-06 |
| **B1. Nhầm phím theo logic bàn phím** (ví dụ phím `t`) | Chưa cài đủ (phím kề hình học thô, chưa có bảng ưu tiên) | **Luật 10: mọi phím chữ và số, hình học thật + bảng ưu tiên (hiện có `t`)** | Mức chữ trên màn hình, chưa mức phím. VSEC không đủ để kiểm chứng riêng `t` |
| **B2. Trượt tay dài, đè nhiều phím liền nhau** | Chưa cài | **Luật 12** (trượt 2 đến 3 phím liền) và **luật 13** (đè 1 đến 2 phím kề cùng lúc) | Cùng bàn phím thật; mức chữ |
| **B3. Gõ nhanh nên đảo thứ tự** (o trước u...) | Chưa cài đúng (đảo chỉ chiếm 0,2%) | **Luật 14** (đảo hai chữ liền nhau, chỗ nào cũng được) | Mức chữ không thấy phím thanh và việc Telex ghép lại |
| Phím giữ, bật nảy (`nguuu`) | **Đã cài** (2026-10-07) | **Luật 11** | Hai con số tỉ lệ là giả định của mình |
| **B5. Thiếu phím Space: dính 2 đến 4 từ** (`quan hệ` thành `quanheej`) | Chưa cài (nhãn là nhiều từ) | **Không biểu diễn được ở T4** (đổi số từ của câu) | Đã cài ở `ac-sim` (đoạn 2 đến 4 từ, mọi loại từ, nhiều đoạn, lỗi phím trong đoạn); xem `AUGMENT_RULES.md` mục B5 |
| Từ tiếng Anh gõ bằng cách bấm đôi phím dấu Telex phải giữ nguyên | Chưa có; đã sửa ở bộ sửa trong app | Gián tiếp: các luật giữ nguyên từ tiếng Anh, trừ luật 3, 6, 8, 10 đến 14 | Cần dữ liệu mức phím |
| Teen-code chỉ là tùy chọn | Đúng: không sinh | Bộ kiểm tra loại | |
| Không dạy quy ước đặt dấu (`thoả`/`thỏa`) | **Đã cài** | Bộ kiểm tra loại | |
| Tập đo chỉ gồm lỗi thật | Giữ nguyên | Giữ nguyên | |

**B4 (mô phỏng ở mức phím qua bộ ghép Telex thật): đã làm ở `ac-sim` (2026-10-08).** Nó dùng bố cục bàn phím thật của bạn và có các kiểu lỗi nhầm phím kề (kèm thói quen của `t`), trượt tay, đè nhiều phím, giữ phím lặp, đảo phím; xem `AUGMENT_RULES.md` (nhật ký thay đổi). Trọng số các kiểu mới là giả định, **chưa hiệu chuẩn**. Luật 10 đến 14 trong file này (LLM) vẫn chỉ ở mức chữ trên màn hình; với các lỗi cơ học, nên dùng `ac-sim` thay cho LLM.

**Các loại phím chưa có bộ sinh lỗi (bố cục `keyboard.py` đã có các phím này; thiếu là bộ sinh lỗi cho chúng; đề xuất, chưa làm, cần bạn quyết):**
- **Shift và Caps Lock:** sai chữ hoa (`bẠn`, `HÔm`, quên viết hoa đầu câu). Cần cách xử lý riêng ở phía sửa vì chữ sửa là chữ thường và `match_case` quyết chữ hoa.
- **Phím cách (Space):** dính hai từ (`trênKaggle`) hoặc tách một từ (`tr ên`). Làm đổi số từ nên không qua bộ kiểm tra hiện tại; phần dính từ đã có bộ cắt âm tiết riêng ở động cơ.
- **Phím dấu câu cạnh chữ** (dấu phẩy, chấm, chấm phẩy, nháy đơn sát `m`, `l`, `p`): làm tách từ nên cũng đổi số từ.
- **Backspace, Delete và phím điều hướng:** thiếu hoặc thừa chữ do xóa nhầm; phần lớn đã nằm trong luật 3 và 6 về kết quả.

---

## 3. Từng luật: lời nhắc, ví dụ và kiểm tra

Mỗi luật có phần "Cách làm" gửi cho LLM, số từ đổi mỗi phiên bản, và hai ví dụ do bộ sinh tạo (mỗi lần gọi là các câu và từ khác).

### Luật 1. thiếu dấu thanh
- Tỉ lệ gần đúng: 27%. Nguồn: đo trên lỗi thật (VSEC).
- **Cách làm đưa cho LLM:** Chọn một từ có dấu thanh (sắc, huyền, hỏi, ngã, nặng) và viết lại từ đó KHÔNG CÒN dấu thanh, giữ nguyên các chữ và dấu mũ, móc khác.
- Số từ đổi mỗi phiên bản: 1 đến 2. Từ tiếng Anh: giữ nguyên. Bảng phím sát nhau trong prompt: không.
- Hai ví dụ do bộ sinh của luật tạo (mỗi lần gọi là các câu và từ khác nhau):
  - `dạo này giá xăng tăng nên mình đi xe buýt nhiều hơn` → `dạo này gia xăng tăng nên mình đi xe buyt nhiều hơn` (giá → gia, buýt → buyt)
  - `tuần sau lớp mình đi dã ngoại ở Đà Lạt` → `tuần sau lớp mình đi dã ngoai ở Đà Lạt` (ngoại → ngoai)

### Luật 2. sai dấu thanh sang dấu khác
- Tỉ lệ gần đúng: 24%. Nguồn: đo trên lỗi thật (VSEC).
- **Cách làm đưa cho LLM:** Chọn một từ có dấu thanh và đổi dấu thanh đó thành MỘT DẤU THANH KHÁC (sắc, huyền, hỏi, ngã, nặng), giữ nguyên chữ cái.
- Số từ đổi mỗi phiên bản: 1 đến 2. Từ tiếng Anh: giữ nguyên. Bảng phím sát nhau trong prompt: không.
- Hai ví dụ do bộ sinh của luật tạo (mỗi lần gọi là các câu và từ khác nhau):
  - `tối nay mình rảnh, đi ăn gì đó nhé` → `tối nay mình rảnh, đi ăn gì đõ nhé` (đó → đõ)
  - `mình thử query json bằng script mới mà vẫn chưa ra kết quả` → `mình thử query json bằng script mời mà vần chưa ra kết quả` (mới → mời, vẫn → vần)

### Luật 3. thiếu một chữ cái
- Tỉ lệ gần đúng: 15%. Nguồn: đo trên lỗi thật (VSEC).
- **Cách làm đưa cho LLM:** Chọn một từ và BỎ ĐI đúng một chữ cái của nó (chữ nào cũng được, đầu, giữa hay cuối từ), các chữ còn lại giữ nguyên thứ tự và dấu.
- Số từ đổi mỗi phiên bản: 1 đến 2. Từ tiếng Anh: được phép đổi. Bảng phím sát nhau trong prompt: không.
- Hai ví dụ do bộ sinh của luật tạo (mỗi lần gọi là các câu và từ khác nhau):
  - `dạo này giá xăng tăng nên mình đi xe buýt nhiều hơn` → `dạo này giá xăng tăng nê mình đi xe buýt nhiều ơn` (nên → nê, hơn → ơn)
  - `cô giáo dặn cả lớp ôn kỹ phần lịch sử trước kỳ thi` → `cô giáo dặ cả lớp ôn kỹ hần lịch sử trước kỳ thi` (dặn → dặ, phần → hần)

### Luật 4. thiếu dấu mũ, móc hay nét của đ
- Tỉ lệ gần đúng: 10%. Nguồn: đo trên lỗi thật (VSEC).
- **Cách làm đưa cho LLM:** Chọn một từ có chữ đ, â, ê, ô, ă, ơ hoặc ư (từ chưa có dấu thanh) và viết lại KHÔNG CÒN dấu mũ, móc, nét ngang hay dấu trăng của chữ đó (đ thành d, ê thành e, ư thành u...).
- Số từ đổi mỗi phiên bản: 1 đến 2. Từ tiếng Anh: giữ nguyên. Bảng phím sát nhau trong prompt: không.
- Hai ví dụ do bộ sinh của luật tạo (mỗi lần gọi là các câu và từ khác nhau):
  - `mình vừa cài lại Windows, máy chạy nhanh hơn nhiều` → `mình vùa cài lại Windows, máy chạy nhanh hơn nhiều` (vừa → vùa)
  - `quán phở đầu ngõ ngon và giá rất hợp lý` → `quán phở dầu ngõ ngon và giá rất họp lý` (đầu → dầu, hợp → họp)

### Luật 5. thêm dấu thanh vào chữ không có thanh
- Tỉ lệ gần đúng: 6%. Nguồn: đo trên lỗi thật (VSEC).
- **Cách làm đưa cho LLM:** Chọn một từ KHÔNG có dấu thanh và gắn thêm một dấu thanh (sắc, huyền, hỏi, ngã hoặc nặng) vào nguyên âm của nó.
- Số từ đổi mỗi phiên bản: 1 đến 2. Từ tiếng Anh: giữ nguyên. Bảng phím sát nhau trong prompt: không.
- Hai ví dụ do bộ sinh của luật tạo (mỗi lần gọi là các câu và từ khác nhau):
  - `đây là file của mình` → `đây là fíle của mình` (file → fíle)
  - `dạo này giá xăng tăng nên mình đi xe buýt nhiều hơn` → `dạo này giá xăng tăng nên mình đi xé buýt nhiều hờn` (xe → xé, hơn → hờn)

### Luật 6. thừa một chữ lạ hoặc chữ ở phím kề
- Tỉ lệ gần đúng: 5%. Nguồn: đo trên lỗi thật (VSEC).
- **Cách làm đưa cho LLM:** Chọn một từ và CHÈN THÊM đúng một chữ cái vào nó (ở đầu, giữa hay cuối từ). Chữ thêm không phải là chữ lặp lại chữ bên cạnh. Có thể là một phím nằm sát bên (xem bảng phím sát nhau) hoặc một chữ bất kỳ.
- Số từ đổi mỗi phiên bản: 1 đến 2. Từ tiếng Anh: giữ nguyên. Bảng phím sát nhau trong prompt: có.
- Hai ví dụ do bộ sinh của luật tạo (mỗi lần gọi là các câu và từ khác nhau):
  - `tuần sau lớp mình đi dã ngoại ở Đà Lạt` → `tuần sau lớp mình bđi dã ngoại ở Đà Lạt` (đi → bđi)
  - `tiền điện tháng này tăng gần hai trăm nghìn` → `tiềqn điện tháng này tăng gầnj hai trăm nghìn` (tiền → tiềqn, gần → gầnj)

### Luật 7. thêm dấu mũ hoặc móc sai
- Tỉ lệ gần đúng: 3%. Nguồn: đo trên lỗi thật (VSEC).
- **Cách làm đưa cho LLM:** Chọn một từ có nguyên âm a, o, e, u hoặc d chưa có dấu mũ, móc hay nét và gắn thêm một dấu như vậy (a thành â hay ă, o thành ô hay ơ, e thành ê, u thành ư, d thành đ).
- Số từ đổi mỗi phiên bản: 1 đến 2. Từ tiếng Anh: giữ nguyên. Bảng phím sát nhau trong prompt: không.
- Hai ví dụ do bộ sinh của luật tạo (mỗi lần gọi là các câu và từ khác nhau):
  - `cô giáo dặn cả lớp ôn kỹ phần lịch sử trước kỳ thi` → `cô giáo đặn cả lớp ôn kỹ phần lịch sử trước kỳ thi` (dặn → đặn)
  - `mỗi lần mình thử đều lỗi` → `mỗi lần mình thử đềư lỗi` (đều → đềư)

### Luật 8. thay chữ bằng chữ ở xa trên bàn phím
- Tỉ lệ gần đúng: 3%. Nguồn: đo trên lỗi thật (VSEC).
- **Cách làm đưa cho LLM:** Chọn một từ và thay đúng một chữ cái bằng một chữ cái KHÁC Ở XA nó trên bàn phím: một phím KHÔNG nằm trong bảng phím sát nhau của chữ đó.
- Số từ đổi mỗi phiên bản: 1 đến 2. Từ tiếng Anh: giữ nguyên. Bảng phím sát nhau trong prompt: có.
- Hai ví dụ do bộ sinh của luật tạo (mỗi lần gọi là các câu và từ khác nhau):
  - `cô giáo dặn cả lớp ôn kỹ phần lịch sử trước kỳ thi` → `cô giáo dặc cả lớp ôn kỹ phần lịch sử trước kỳ thc` (dặn → dặc, thi → thc)
  - `mỗi lần mình thử đều lỗi` → `mỗi bần mình thử đều lẫi` (lần → bần, lỗi → lẫi)

### Luật 9. đặt dấu thanh sai nguyên âm
- Tỉ lệ gần đúng: 3%. Nguồn: đo trên lỗi thật (VSEC).
- **Cách làm đưa cho LLM:** Chọn một từ có từ hai nguyên âm trở lên và có dấu thanh, rồi chuyển dấu thanh sang NGUYÊN ÂM KHÁC của cùng từ.
- Số từ đổi mỗi phiên bản: 1 đến 2. Từ tiếng Anh: giữ nguyên. Bảng phím sát nhau trong prompt: không.
- Hai ví dụ do bộ sinh của luật tạo (mỗi lần gọi là các câu và từ khác nhau):
  - `chị Lan mới chuyển sang làm việc ở Hà Nội` → `chị Lan mơí chuyển sang làm việc ở Hà Nội` (mới → mơí)
  - `dạo này giá xăng tăng nên mình đi xe buýt nhiều hơn` → `daọ này gía xăng tăng nên mình đi xe buýt nhiều hơn` (dạo → daọ, giá → gía)

### Luật 10. nhầm sang một phím sát bên trên bàn phím
- Tỉ lệ gần đúng: 3%. Nguồn: Bạn (chưa đo).
- **Cách làm đưa cho LLM:** Chọn một từ và thay đúng MỘT phím (chữ cái bất kỳ trong từ, hoặc cả chữ số) bằng MỘT PHÍM NẰM SÁT BÊN nó trên bàn phím, lấy từ bảng phím sát nhau bên dưới. Bảng đã gồm cả những phím người gõ hay nhầm theo thói quen riêng. Mọi chữ cái trong từ đều có thể bị nhầm, không riêng chữ nào. Giữ nguyên các chữ khác.
- Số từ đổi mỗi phiên bản: 1 đến 2. Từ tiếng Anh: được phép đổi. Bảng phím sát nhau trong prompt: có.
- Hai ví dụ do bộ sinh của luật tạo (mỗi lần gọi là các câu và từ khác nhau):
  - `mình nghĩ là nên thử lại` → `mình nghĩ là nêj thử lại` (nên → nêj)
  - `hôm qua mưa to nên trận bóng bị hoãn` → `hôm qua mưa to hên trận bóng bị hoãn` (nên → hên)

### Luật 11. giữ phím hay bật nảy: lặp một chữ 2 đến 4 lần
- Tỉ lệ gần đúng: 4%. Nguồn: Mình, từ nhật ký của bạn (chưa đo).
- **Cách làm đưa cho LLM:** Chọn một từ và LẶP MỘT CHỮ CÁI bất kỳ của nó thêm 1 đến 3 lần liên tiếp (tổng cộng chữ đó xuất hiện 2 đến 4 lần liền nhau); chỉ một chỗ lặp, các chữ khác giữ nguyên.
- Số từ đổi mỗi phiên bản: 1 đến 2. Từ tiếng Anh: được phép đổi. Bảng phím sát nhau trong prompt: không.
- Hai ví dụ do bộ sinh của luật tạo (mỗi lần gọi là các câu và từ khác nhau):
  - `tiền điện tháng này tăng gần hai trăm nghìn` → `tiền điện tháng nààày tăng gầnn hai trăm nghìn` (này → nààày, gần → gầnn)
  - `em muốn đặt một phòng đôi cho hai đêm` → `em muốn đặt một phòng đđôi cho hai đêm` (đôi → đđôi)

### Luật 12. trượt tay: chèn 2 đến 3 phím sát nhau liên tiếp
- Tỉ lệ gần đúng: 2%. Nguồn: Bạn (chưa đo).
- **Cách làm đưa cho LLM:** Tay bị trượt khi gõ một chữ của từ (đầu, giữa hay cuối): chèn thêm 2 hoặc 3 phím LIỀN NHAU, mỗi phím nằm sát chữ bên cạnh chỗ chèn hoặc sát phím vừa chèn, lấy từ bảng phím sát nhau.
- Số từ đổi mỗi phiên bản: 1 đến 2. Từ tiếng Anh: giữ nguyên. Bảng phím sát nhau trong prompt: có.
- Hai ví dụ do bộ sinh của luật tạo (mỗi lần gọi là các câu và từ khác nhau):
  - `nhớ mang theo giấy tờ khi đi làm thủ tục nhé` → `nhớ mang ehgtheo giấyu6 tờ khi đi làm thủ tục nhé` (theo → ehgtheo, giấy → giấyu6)
  - `mình vừa cài lại Windows, máy chạy nhanh hơn nhiều` → `mình vừa cài lại Windows, má6y6y chạy nhanh hơn nhiều` (máy → má6y6y)

### Luật 13. một ngón đè cùng lúc 2 đến 3 phím
- Tỉ lệ gần đúng: 2%. Nguồn: Bạn (chưa đo).
- **Cách làm đưa cho LLM:** Một ngón đè trúng cùng lúc phím đúng và 1 đến 2 phím sát bên: chèn thêm 1 hoặc 2 chữ vào ngay TRƯỚC hoặc SAU một chữ bất kỳ của từ, là phím nằm sát chữ đó theo bảng phím sát nhau.
- Số từ đổi mỗi phiên bản: 1 đến 2. Từ tiếng Anh: giữ nguyên. Bảng phím sát nhau trong prompt: có.
- Hai ví dụ do bộ sinh của luật tạo (mỗi lần gọi là các câu và từ khác nhau):
  - `tiền điện tháng này tăng gần hai trăm nghìn` → `tijền điện tháng này tăng gầjqn hai trăm nghìn` (tiền → tijền, gần → gầjqn)
  - `tối nay mình rảnh, đi ăn gì đó nhé` → `tối nay mình dcrảnh, đi ăn gì đó nhé` (rảnh → dcrảnh)

### Luật 14. đảo hai chữ liền nhau khi gõ nhanh
- Tỉ lệ gần đúng: 2%. Nguồn: Bạn (VSEC chỉ có 0,2%).
- **Cách làm đưa cho LLM:** Gõ nhanh nên HAI CHỮ CÁI LIỀN NHAU (ở chỗ nào trong từ cũng được) bị gõ ngược thứ tự: chọn một từ và đảo vị trí hai chữ cái đứng cạnh nhau. Các chữ khác giữ nguyên.
- Số từ đổi mỗi phiên bản: 1 đến 2. Từ tiếng Anh: được phép đổi. Bảng phím sát nhau trong prompt: không.
- Hai ví dụ do bộ sinh của luật tạo (mỗi lần gọi là các câu và từ khác nhau):
  - `dạo này giá xăng tăng nên mình đi xe buýt nhiều hơn` → `dạo này giá xăng tăgn nên mình iđ xe buýt nhiều hơn` (tăng → tăgn, đi → iđ)
  - `báo cáo tháng này cần nộp trước thứ sáu` → `báo coá tháng này cần nộp trước thứ sáu` (cáo → coá)

### Luật 15. nhầm theo thói quen vùng miền: t/c cuối vần, n/l đầu từ, hỏi/ngã
- Tỉ lệ gần đúng: 3%. Nguồn: VSEC (đã lẫn trong các luật 1 đến 9).
- **Cách làm đưa cho LLM:** Viết nhầm theo thói quen phát âm vùng miền, chỉ MỘT trong ba kiểu: (a) đổi t và c ở cuối vần; (b) đổi n và l ở đầu từ; (c) đổi dấu hỏi và dấu ngã cho nhau. Chọn từ nào áp dụng được cũng được.
- Số từ đổi mỗi phiên bản: 1 đến 2. Từ tiếng Anh: giữ nguyên. Bảng phím sát nhau trong prompt: không.
- Hai ví dụ do bộ sinh của luật tạo (mỗi lần gọi là các câu và từ khác nhau):
  - `mình thử query json bằng script mới mà vẫn chưa ra kết quả` → `mình thử query json bằng scripc mới mà vẩn chưa ra kết quả` (script → scripc, vẫn → vẩn)
  - `tôi muốn hỏi về thủ tục đăng ký tạm trú` → `tôi muốn hõi về thũ tục đăng ký tạm trú` (hỏi → hõi, thủ → thũ)

## Prompt mẫu đầy đủ (luật 10, nhầm sang phím sát bên)

Đây là một yêu cầu thật gửi cho LLM; các luật khác có cùng khung, đổi "Kiểu lỗi", "Cách làm", và chỉ các luật liên quan đến phím mới có bảng phím sát nhau. Chú ý bảng phím tính sẵn cho các chữ của câu cuối cùng.

```
Bạn là công cụ tạo dữ liệu kiểm thử gõ sai tiếng Việt. Chỉ trả về các dòng JSON được yêu cầu.
Nhiệm vụ: từ một câu ĐÚNG, viết 3 phiên bản khác nhau như thể người gõ bị lỗi. Mỗi phiên bản có từ 1 đến 2 từ bị gõ sai THEO ĐÚNG MỘT KIỂU LỖI dưới đây, và KHÔNG làm gì khác.

KIỂU LỖI: nhầm sang một phím sát bên trên bàn phím.
CÁCH LÀM: Chọn một từ và thay đúng MỘT phím (chữ cái bất kỳ trong từ, hoặc cả chữ số) bằng MỘT PHÍM NẰM SÁT BÊN nó trên bàn phím, lấy từ bảng phím sát nhau bên dưới. Bảng đã gồm cả những phím người gõ hay nhầm theo thói quen riêng. Mọi chữ cái trong từ đều có thể bị nhầm, không riêng chữ nào. Giữ nguyên các chữ khác.

BỐ CỤC BÀN PHÍM (QWERTY): hàng số 1234567890; hàng trên qwertyuiop; hàng giữa asdfghjkl; hàng dưới zxcvbnm; mỗi hàng lệch nhẹ so với hàng trên nó. Hai phím là SÁT NHAU khi cùng hàng và liền nhau, hoặc ở hàng ngay trên hay ngay dưới và lệch nhau chưa tới một phím.
BẢNG PHÍM SÁT NHAU của các chữ trong câu này (đã tính sẵn, gồm cả thói quen riêng của người gõ; chỉ dùng phím trong bảng khi luật cần phím sát bên):
a: q s w z
c: d f v x
e: 3 4 d r s w
h: b g j n u y
i: 8 9 j k o u
j: h i k m n u
m: j k n
n: b h j m
o: 0 9 i k l p
q: 1 2 a w
r: 4 5 d e f t
s: a d e w x z
t: 5 6 e f g h r u y
u: 7 8 h i j y
v: b c f g
y: 6 7 g h t u

Quy tắc chung:
- Áp dụng cho BẤT KỲ từ nào trong câu mà kiểu lỗi làm được, không chỉ những từ giống ví dụ; chọn từ khác nhau giữa các phiên bản.
- Chỉ đổi những từ ghi trong edits; mọi từ khác, dấu câu và khoảng trắng giữ NGUYÊN. Số từ của câu không đổi.
- Vị trí i bắt đầu từ 0 và đếm cả dấu câu như một mục riêng (dấu phẩy là một mục).
- Từ tiếng Anh và tên riêng được phép đổi theo kiểu lỗi này.
- Không dùng viết tắt hay chữ lóng (ko, dc, k...). Không đổi cả từ thành từ khác nghĩa.
- Mỗi phiên bản một dòng JSON, không giải thích: {"typed": "câu sau khi gõ sai", "edits": [{"i": vị trí, "from": "từ đúng", "to": "từ gõ sai"}]}

Ví dụ cho kiểu lỗi này (các câu và từ khác nhau mỗi lần; đừng chép lại):
Câu đúng: mỗi lần mình thử đều lỗi
{"typed": "mỗo kần mình thử đều lỗi", "edits": [{"i": 0, "from": "mỗi", "to": "mỗo"}, {"i": 1, "from": "lần", "to": "kần"}]}
{"typed": "kỗi lần mình thử đều lỗi", "edits": [{"i": 0, "from": "mỗi", "to": "kỗi"}]}

Câu đúng: mình nghĩ là nên thử lại
{"typed": "mình nghĩ pà nên hhử lại", "edits": [{"i": 2, "from": "là", "to": "pà"}, {"i": 4, "from": "thử", "to": "hhử"}]}
{"typed": "mình nghĩ oà hên thử lại", "edits": [{"i": 2, "from": "là", "to": "oà"}, {"i": 3, "from": "nên", "to": "hên"}]}

Câu đúng: cô giáo dặn cả lớp ôn kỹ phần lịch sử trước kỳ thi
{"typed": "cô giáo cặn cả lớp ôn kỹ phần lịch sử trước kỳ thi", "edits": [{"i": 2, "from": "dặn", "to": "cặn"}]}
{"typed": "cô giáo dặn cả lớp ôm kỹ phần lịch sử trước kỳ thi", "edits": [{"i": 5, "from": "ôn", "to": "ôm"}]}

Câu đúng: Hôm nay mình thử query json trên server của team

```

---

## 4. Bộ kiểm tra

**Chung cho mọi luật** (một phiên bản chỉ được giữ khi cả hai đều đúng):
- Câu gõ sai có cùng số mục (từ và dấu câu) với câu đúng; các từ khác nhau giữa hai câu đúng bằng danh sách `edits`, và số từ đổi nằm trong giới hạn của luật.
- Mỗi từ đổi gồm chữ cái và chữ số (không có dấu câu), khớp `from` và `to` với câu, không phải chữ lóng/teen-code, không trùng một từ khác trong câu, không phải quy ước đặt dấu.

**Riêng từng luật:**

| Luật | Điều kiện |
|---|---|
| 1 | đúng là thiếu dấu thanh (`tone_drop`) |
| 2 | đúng là đổi sang dấu thanh khác (`tone`) |
| 3 | đúng là bỏ một chữ (`omit`) |
| 4 | đúng là bỏ dấu mũ/móc/nét đ (`mark_drop`) |
| 5 | đúng là thêm dấu thanh (`tone_add`) |
| 6 | thừa đúng một chữ, không lặp chữ bên cạnh (`insert` hoặc `extra`) |
| 7 | đúng là thêm dấu mũ/móc (`mark_add`) |
| 8 | thay một chữ bằng chữ ở xa trên bàn phím (`substitute`) |
| 9 | chuyển dấu thanh sang nguyên âm khác, không phải quy ước (`tone_place`) |
| 10 | đổi đúng một phím (chữ hoặc số) thành phím sát bên theo bố cục thật, cộng bảng ưu tiên của người gõ |
| 11 | lặp thêm một chữ ở đúng một chỗ, tối đa 5 lần liền nhau |
| 12 | chèn 2 đến 3 phím, mỗi phím sát chữ bên cạnh chỗ chèn hoặc sát phím vừa chèn |
| 13 | chèn 1 đến 2 phím, mỗi phím sát chữ ngay trước hoặc sau chỗ chèn |
| 14 | đảo đúng hai chữ liền nhau (`swap`) |
| 15 | `t`/`c` cuối vần, `n`/`l` đầu từ, hoặc hỏi/ngã đổi cho nhau, các chữ khác giữ nguyên |

Các test (33) kiểm tra: bố cục bàn phím (mọi chữ và số có phím kề, quan hệ kề là hai chiều, `t` cạnh `r y 5 6 f g`, bảng riêng của người gõ nằm ngoài hình học); **với từng chữ cái và chữ số** bộ sinh phím kề chỉ ra phím kề; mỗi luật sinh được lỗi qua đúng bộ kiểm tra của nó trên hàng nghìn từ ngẫu nhiên; ví dụ trong prompt đổi theo từng yêu cầu; các bộ kiểm tra phân biệt được kiểu lỗi.

---

## 5. Điểm yếu đã biết (nói thẳng)

- **Chưa chạy trên Kaggle**: chưa biết bao nhiêu phần trăm phiên bản của LLM 3B qua được bộ kiểm tra theo từng luật. Mình sẽ đo trên khoảng 500 câu và báo tỉ lệ giữ lại của từng luật trước khi chạy lớn; luật nào tỉ lệ quá thấp thì đổi sang 7B, hoặc dùng thẳng bộ sinh bằng chương trình (đã có, mục 1).
- **Luật 10 đến 14 là xấp xỉ ở mức chữ.** Lỗi gõ thật xảy ra ở mức phím rồi qua bộ ghép Telex; ở đây không thấy được điều đó. Bộ kiểm tra chỉ đảm bảo lỗi đúng kiểu, **không** đảm bảo tần suất giống lỗi gõ thật của bạn (cần dữ liệu gõ thật, xem `AUGMENT_RULES.md` mục C).
- **Bố cục là bàn phím US ANSI của laptop** (suy ra từ bố cục Windows 00000409; hình dạng phím Enter chưa được bạn xác nhận). Bảng đã gồm dấu câu, Shift, Caps Lock, Tab, Space, nhưng **lỗi mức chữ trong từ chỉ dùng chữ và số**; các phím làm tách từ hoặc đổi chữ hoa chưa có bộ sinh lỗi. Laptop thật có phím nhỏ hơn và cụm phím mũi tên riêng, chưa mô hình hóa.
- **Tỉ lệ giữa các luật** dùng để chọn luật cho từng câu; tỉ lệ của luật 10 đến 14 là giả định của mình, không đo.
- LLM có thể tạo từ gõ sai trùng một từ có thật (nhãn gây tranh cãi): bước chấm điểm bằng LLM (T2) vẫn nên chạy sau để loại ca mơ hồ.

---

## 6. Cần bạn quyết

1. Danh sách 15 luật và cách làm của từng luật đã đúng ý bạn chưa? Luật nào cần sửa, thêm hay bỏ?
2. Các loại phím chưa có luật (Shift/Caps Lock, Space, dấu câu): có muốn làm không, và theo kiểu nào?
3. Bảng ưu tiên thói quen riêng: ngoài `t`, bạn có phím nào khác không? Hoặc dùng trang ghi phím để đo thay vì đoán.
4. Có chạy thử trên khoảng 500 câu rồi xem tỉ lệ giữ lại của từng luật không?
