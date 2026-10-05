cargo build --release  && ./target/release/autocorrect.exe --debug

3. Hoàn thiện sản phẩm.
- Kiểu gõ VNI (hiện chỉ có Telex).
- Cửa sổ cài đặt: sửa danh sách app theo nhóm (terminal/IDE/tắt hẳn), không phải sửa mã nguồn.
- Đóng gói thành một bản cài (installer), kèm biểu tượng.
- Chống sự cố: nếu Windows gỡ hook bàn phím vì app bị treo, app tự phát hiện và cài lại.

4. Nâng chất lượng sửa lỗi (phần bạn đã nói sẽ bàn riêng):
- nhớ ngôn ngữ của cả cụm từ gần nhất, không chỉ một từ trước (rẻ, đo được ngay trên benchmark);
- thử trigram;
- chỉnh chi phí các kiểu gõ nhầm bằng nhật ký thật của bạn (cần bạn bật nhật ký vài ngày);
- sửa chữ đã gõ xong ở câu trước khi thấy từ kế tiếp (rủi ro cao nhất, cần bạn đồng ý).