# Thí nghiệm M1 trên Kaggle: thầy LLM chấm điểm ứng viên

Mục tiêu: xem một LLM chọn đúng ứng viên hơn n-gram hiện tại ở những nhóm khó hay không (cổng quyết định
trong trang thiết kế pipeline). **Không dùng dữ liệu của bạn**: chỉ câu từ kho công khai.

## 1. Xuất dữ liệu (máy bạn, nhẹ)

    cargo run -p ac-bench --release -- --export export.jsonl --sentences 1500

`--sentences N` là số câu mỗi tập; 1500 cho khoảng 70 nghìn dòng, vài chục MB. Notebook tự lấy mẫu
tối đa 3000 dòng cho mỗi nhóm (tập, loại lỗi), nên không cần xuất quá nhiều.

## 2. Chạy thầy (Kaggle hoặc máy GPU khác)

1. Tạo Kaggle Dataset **riêng tư** tên `autocorrect-export`, tải `export.jsonl` lên.
2. Tạo Notebook mới, bật **GPU** (T4 hoặc P100 là đủ cho mô hình 1 đến 3 tỉ tham số), bật Internet
   để tải mô hình từ Hugging Face, gắn dataset vừa tạo.
3. Dán `m1_teacher_scoring.py` vào notebook, mỗi khối `# %%` là một ô.
4. Sửa ô Settings nếu cần: `MODEL_NAME` (xem lưu ý dưới), `LOAD_4BIT`, `MAX_PER_GROUP`.
5. Chạy hết. Cuối cùng tải `teacher_scores.jsonl` về.

## 3. Đọc kết quả (máy bạn, nhẹ, không cần GPU)

    python tools/kaggle/analyze_m1.py export.jsonl teacher_scores.jsonl

In hai bảng:
- Độ chính xác chọn ứng viên đúng theo (tập, loại lỗi) của ba cách: n-gram của app, thầy chỉ nhìn
  ngữ cảnh trái, thầy nhìn cả ngữ cảnh phải; kèm khoảng tin cậy 95% của chênh lệch thầy-trái trừ n-gram.
- Chế độ an toàn: nếu chỉ sửa khi xác suất của ứng viên tốt nhất ≥ tau thì được bao nhiêu sửa đúng/sai
  và bao nhiêu từ đúng bị đổi nhầm (trên 1000 từ). Đây là phép so công bằng nhất với mục tiêu "sửa nhầm thấp".

## Cách đọc (đừng bỏ qua)

- So **thầy-trái** với **n-gram** mới công bằng (cùng thông tin). **Thầy-đầy-đủ** cho biết trần nếu có
  sửa muộn.
- Độ chính xác chỉ tính trên mẫu mà đáp án nằm trong ứng viên (cột `cover` cho biết tỉ lệ). Nhóm đáp án
  nằm ngoài ứng viên không cải thiện được chỉ bằng cách đổi cách chấm.
- Điểm n-gram của app có kèm thiên lệch ngôn ngữ; điểm của thầy là log xác suất thô nên thiên về cụm ngắn
  và phổ biến. Đây là so sánh thô, chưa phải so sánh hoàn hảo.
- Với mỗi nhóm, chỉ tin chênh lệch khi khoảng tin cậy không chứa 0 và mẫu đủ lớn.
- Hãy xem riêng tập `vd` (hội thoại) và `vs` (giọng thân mật): thầy có thể giỏi văn viết chuẩn nhưng
  kém ở giọng lóng.

## Về mô hình thầy

Mặc định `Qwen/Qwen2.5-1.5B` (mô hình nền, không phải bản chat). Mình chưa kiểm tra mô hình đó có sẵn
và xử lý tiếng Việt tốt đến đâu trên Kaggle hiện nay: hãy thử vài mô hình (bản 1.5B, 3B, và một mô hình
tiếng Việt như Vistral hoặc PhoGPT nếu có, kèm `LOAD_4BIT = True` cho bản 7B) và so kết quả giữa chúng.
Mô hình dạng chat/instruct không phù hợp cho cách chấm xác suất này.
