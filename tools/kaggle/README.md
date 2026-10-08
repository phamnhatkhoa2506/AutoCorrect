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

---

# Thầy T1: huấn luyện trên Kaggle (RESEARCH.md, mục 5)

Các file: `t1_data.py` (dữ liệu và đo, Python thuần), `test_t1_data.py` (kiểm tra cục bộ), `prepare_corpus.py`
(dựng bộ dữ liệu), `t1_teacher_train.py` (notebook huấn luyện).

## 1. Kiểm tra cục bộ (nhẹ, không cần GPU)

    python -m unittest tools/kaggle/test_t1_data.py

## 2. Dựng bộ dữ liệu (máy bạn, vài phút, không tải gì lên)

    python tools/kaggle/prepare_corpus.py

Ghi `tools/kaggle/upload/autocorrect-train/` (khoảng 1,4 triệu câu công khai, VSEC chia 90/10, Viwiki để thử,
từ vựng, `t1_data.py`). Không có dữ liệu của bạn trong đó.

## 3. Tải lên (riêng tư) và chạy

    kaggle datasets create -p tools/kaggle/upload/autocorrect-train

Tạo notebook mới, bật **GPU** (T4) và **Internet** (để tải mô hình nền từ Hugging Face), gắn dataset
`autocorrect-train`, dán `t1_teacher_train.py` (mỗi khối `# %%` là một ô) và chạy lần lượt. Chỉnh ô Settings nếu cần:
`MODEL_NAME` (mặc định `FacebookAI/xlm-roberta-base`, mình chưa kiểm tra nó có trên Kaggle hiện nay), `STEPS`,
`MAX_MINUTES`. Hạn mức GPU mỗi tuần của Kaggle: bạn kiểm tra trên tài khoản.

Kết quả trong `/kaggle/working/t1/`: `teacher.pt` (checkpoint tốt nhất theo VSEC dev), `train_log.json`,
`viwiki_curves.json` (đường đánh đổi trên Viwiki), `soft_labels.jsonl` (cho trò). Tải về `viwiki_curves.json` và
so với `cargo run -p ac-bench --release -- --viwiki` ở cùng mức đổi nhầm trên 1000 từ.

Chưa chạy lần nào trên GPU: lần đầu có thể phải sửa vài chỗ nhỏ (tên mô hình, tham số tokenizer, bộ nhớ).

## 4. Tùy chọn: lỗi khó do LLM mở chọn (T2, hai GPU T4)

**Mọi quy tắc chọn lỗi (đã cài, bạn yêu cầu thêm, câu hỏi mở) nằm ở [`AUGMENT_RULES.md`](AUGMENT_RULES.md)**: danh sách mở, thêm quy tắc mới ở đó.

`t2_lm_negatives.py` (một tiến trình) và `t2_augment_kaggle.py` (notebook chạy hai tiến trình, mỗi GPU một nửa số
câu). Với mỗi câu sạch, ta liệt kê các âm tiết dễ nhầm với một âm tiết trong câu, một mô hình ngôn ngữ 2B đến 3B
chấm câu đã thay, và chỉ giữ lỗi mà câu vẫn đọc được nhưng kém câu gốc: đáp án luôn là câu gốc, LLM không bao giờ
quyết định cái gì đúng.

Notebook mới: bật **GPU T4 x2** và Internet, gắn dataset `autocorrect-train`, dán `t2_augment_kaggle.py`. Ô "Probe" chạy 300
câu trên một GPU và in tốc độ cùng 25 mẫu: **đọc các mẫu trước khi chạy cả đợt**; nếu trông như nhiễu thì đổi `MODEL`
hoặc tăng `MIN_MARGIN`. Kết quả `llm_negatives.jsonl` tải về, đặt cạnh `corpus.txt` trong dataset
(`kaggle datasets version`), thầy T1 tự dùng nếu có (`LLM_FRACTION`, mặc định 10%). So hai lần chạy T1 (có và
không có) trên đường cong Viwiki để biết nó có giúp không.

Đã kiểm tra trên CPU với mô hình ngẫu nhiên nhỏ: chấm theo lô cho cùng điểm với chấm từng câu, chạy từ đầu đến cuối
ra bản ghi hợp lệ, luồng huấn luyện đọc được bản ghi. Chưa chạy với mô hình thật; tốc độ và chất lượng lỗi chưa biết.

