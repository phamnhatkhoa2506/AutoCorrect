"""T3: clean Vietnamese text of the writer's own domain, written by an open LLM (RESEARCH.md, section 5).

The public corpora are news. What this writer types is informal and technical: chat about machine learning, code and
tools, questions to an assistant, notes, everyday talk, with English terms in the middle. The guard that stops wrong
fixes must be taught on that kind of text, so an LLM writes it.

The LLM only writes plain sentences; it decides nothing about what is a mistake. Mistakes are made afterwards by
`t1_data.Corruptor`, and the engine is run on the clean text to find the fixes it should not have made.
Nothing of the writer's journal is used here: only general topics, registers and common technical terms.

    python t3_domain_text.py --out sentences.jsonl --model Qwen/Qwen2.5-3B-Instruct --calls 300 --minutes 30
    python t3_domain_text.py --filter sentences.jsonl --vocab vocab.txt --out in_domain.txt     (no GPU)

Writes JSON lines {"text", "topic", "register", "seeds"}; `--filter` keeps the sentences a Vietnamese writer could
have typed (letters of Vietnamese and English, a sensible length, mostly known words) and drops duplicates.
"""

import argparse
import json
import random
import re
import time
import unicodedata

TOPICS = [
    "huấn luyện mô hình học máy", "chuẩn bị và làm sạch dữ liệu", "chạy mô hình trên GPU miễn phí", "tinh chỉnh một mô hình ngôn ngữ lớn",
    "đánh giá mô hình và đọc kết quả", "viết và sửa lỗi chương trình", "đọc mã và review mã của đồng nghiệp", "dùng git, nhánh và commit",
    "dùng dòng lệnh và shell", "thiết kế API và định dạng JSON", "viết test cho phần mềm", "cài đặt và cấu hình một ứng dụng trên máy tính",
    "phím tắt và cách gõ tiếng Việt trên máy tính", "hỏi một trợ lý AI cách làm việc gì đó", "nhờ trợ lý AI giải thích một đoạn mã",
    "ghi chú công việc cuối ngày", "kế hoạch làm việc tuần này", "họp nhóm và phân công việc", "viết email ngắn cho đồng nghiệp",
    "trò chuyện với bạn bè về cuối tuần", "bàn về món ăn và quán cà phê", "kể chuyện đi du lịch", "chuyện học hành và thi cử",
    "chơi game và bàn về game", "xem phim và nhận xét phim", "nghe nhạc", "chuyện gia đình", "mua sắm trên mạng", "thời tiết và giao thông",
    "tập thể dục và sức khỏe", "tin tức công nghệ", "điện thoại, laptop và linh kiện máy tính", "tốc độ mạng và lỗi kết nối",
    "tài chính cá nhân và tiết kiệm", "tìm việc và phỏng vấn", "học một ngôn ngữ mới", "bình luận trên mạng xã hội", "báo lỗi và mô tả sự cố",
    "so sánh hai công cụ hay hai cách làm", "giải thích một khái niệm cho người mới", "phàn nàn về một chuyện vặt", "hẹn gặp và hẹn giờ",
    "viết lời cảm ơn hoặc xin lỗi", "đặt câu hỏi cho cả nhóm", "nhờ bạn giúp một việc nhỏ", "nhận xét về một bài báo hay một video",
]

REGISTERS = [
    "tin nhắn chat thân mật, câu ngắn, giọng tự nhiên", "câu hỏi gửi cho một trợ lý AI", "ghi chú ngắn cho bản thân",
    "lời hướng dẫn từng bước cho đồng nghiệp", "bình luận trên mạng xã hội", "mô tả một lỗi gặp phải", "đoạn văn ngắn trong email công việc",
    "câu cảm thán và nhận xét ngắn", "lời nhờ vả lịch sự", "câu trả lời giải thích cho người hỏi",
]

# Common technical terms, written as English writers keep them inside Vietnamese text.
TERMS = [
    "json", "query", "reference", "test", "case", "tool", "model", "dataset", "GPU", "CLI", "prompt", "token", "commit", "branch",
    "API", "shell", "script", "bug", "build", "deploy", "log", "config", "cache", "thread", "server", "client", "database",
    "framework", "library", "package", "version", "update", "release", "benchmark", "baseline", "accuracy", "precision", "recall",
    "loss", "batch", "epoch", "embedding", "transformer", "LLM", "fine-tune", "training", "inference", "notebook", "kernel",
    "checkpoint", "pipeline", "feature", "label", "filter", "rule", "rollback", "docker", "python", "rust", "github", "email",
    "app", "file", "folder", "link", "online", "offline", "feedback", "deadline", "meeting", "team", "project", "task", "review",
    "setting", "shortcut", "keyboard", "layout", "plugin", "extension", "browser", "tab", "window", "terminal", "error", "debug",
]

CJK = re.compile(r"[　-鿿가-힯Ѐ-ӿ؀-ۿ฀-๿]")
LEAD = re.compile(r"^\s*(?:[-*•]+|\d+[.)]|\(\d+\))\s*")
WORD = re.compile(r"[^\W\d_]+", re.UNICODE)


# Casual messages as people type them in a chat: short, first person, little punctuation. Written for this file, not
# taken from anyone's text. Spelling stays standard (the corrector learns to leave this alone, not to change it).
CASUAL_EXAMPLES = [
    "mình đang train cái model này mà loss cứ đứng im hoài",
    "bạn ơi cho mình hỏi cái này chút",
    "ủa sao nó báo lỗi vậy nhỉ",
    "tối nay rảnh không đi ăn gì đó",
    "để mình thử lại rồi báo bạn nha",
    "hôm qua họp xong mệt quá trời luôn",
    "cái file này mở không được hả bạn",
    "mai mình gửi cho bạn sau nhé",
]


def make_prompt(rng, sentences=10, style="formal"):
    """(topic, register, seeds, chat messages) for one call. `style` is "formal" (sentences of any register) or
    "casual" (short chat messages)."""
    topic, register = rng.choice(TOPICS), rng.choice(REGISTERS)
    seeds = rng.sample(TERMS, rng.choice([0, 0, 1, 2, 3, 4]))
    if style == "casual":
        register = "tin nhắn chat thân mật"
        seed_line = f" Nếu hợp thì có thể dùng các từ tiếng Anh {', '.join(seeds)}, còn không thì bỏ qua." if seeds else ""
        examples = "\n".join(rng.sample(CASUAL_EXAMPLES, 4))
        user = (
            f"Viết {sentences} tin nhắn chat tiếng Việt khác nhau của người trẻ nhắn cho bạn bè hoặc đồng nghiệp, chủ đề: {topic}.{seed_line} "
            "Mỗi tin ngắn, 4 đến 16 từ, nói tự nhiên như đang gõ nhanh: xưng mình, tui, tớ hay bạn, hay dùng nha, nhé, á, hả, nhỉ, luôn, "
            "ít dấu câu, thường không viết hoa đầu câu, nhưng vẫn đúng chính tả và đủ dấu tiếng Việt. "
            f"Ví dụ về giọng văn (đừng chép lại):\n{examples}\n"
            "Mỗi tin một dòng, không đánh số, không giải thích thêm."
        )
        messages = [
            {"role": "system", "content": "Bạn là người Việt đang nhắn tin chat hằng ngày, viết ngắn và tự nhiên."},
            {"role": "user", "content": user},
        ]
        return topic, register, seeds, messages
    seed_line = f" Có thể chen vài thuật ngữ tiếng Anh như: {', '.join(seeds)} (giữ nguyên chữ tiếng Anh)." if seeds else ""
    user = (
        f"Viết {sentences} câu tiếng Việt khác nhau, chủ đề: {topic}. Kiểu viết: {register}.{seed_line} "
        "Mỗi câu 8 đến 25 từ, đầy đủ dấu, đúng chính tả, nghe như người thật viết, mở đầu mỗi câu một cách khác nhau. "
        "Mỗi câu một dòng, không đánh số, không giải thích thêm."
    )
    messages = [
        {"role": "system", "content": "Bạn là một người Việt viết văn bản tự nhiên hằng ngày, chính tả chuẩn."},
        {"role": "user", "content": user},
    ]
    return topic, register, seeds, messages


def parse_lines(text):
    """Sentences from the LLM's reply: one per line, list marks removed."""
    out = []
    for line in text.split("\n"):
        line = LEAD.sub("", line).strip().strip('"').strip()
        if line:
            out.append(unicodedata.normalize("NFC", line))
    return out


LABEL_LINE = re.compile(r"(?i)^(câu|tin|ví dụ|dòng|message)\s*\d*\s*[:.)]?\s*$|^\s*(câu|tin nhắn)\s+\d+\b")
AI_SELF = re.compile(r"(?i)được lập trình (để|bởi)|là một (trí tuệ nhân tạo|mô hình ai|ai\b)|với tư cách là một (ai|trợ lý)|tôi (không có khả năng|là trợ lý)")


def keep_sentence(text, vocab, min_words=4):
    """Whether a sentence is plain enough to be taught as correct: a sensible length, no foreign scripts, no
    leftover markup or list label, no talk about being an AI, quotes that close, and mostly words a Vietnamese
    writer knows (syllables of the vocabulary, or ASCII words)."""
    words = WORD.findall(text)
    if not min_words <= len(words) <= 45 or CJK.search(text) or any(c in text for c in "<>{}[]|\\_#*"):
        return False
    if LABEL_LINE.search(text) or AI_SELF.search(text) or text.count('"') % 2 or text.count("“") != text.count("”"):
        return False
    known = sum(1 for w in words if w.lower() in vocab or w.isascii())
    return known >= 0.85 * len(words)


def filter_file(src, vocab_path, out_path, min_words=4):
    vocab = {w.strip() for w in open(vocab_path, encoding="utf-8") if w.strip()}
    seen, kept, total = set(), 0, 0
    with open(out_path, "w", encoding="utf-8", newline="\n") as out:
        for line in open(src, encoding="utf-8"):
            total += 1
            text = json.loads(line)["text"]
            key = " ".join(w.lower() for w in WORD.findall(text))
            if key in seen or not keep_sentence(text, vocab, min_words):
                continue
            seen.add(key)
            out.write(text + "\n")
            kept += 1
    print(f"{kept} of {total} sentences kept (duplicates and odd ones dropped)")


def generate(args):
    import torch
    from transformers import AutoModelForCausalLM, AutoTokenizer

    rng = random.Random(args.seed * 100 + args.shard)
    tok = AutoTokenizer.from_pretrained(args.model, padding_side="left")
    kwargs = {"torch_dtype": torch.float16}
    if args.load_4bit:
        from transformers import BitsAndBytesConfig
        kwargs = {"quantization_config": BitsAndBytesConfig(load_in_4bit=True, bnb_4bit_compute_dtype=torch.float16, bnb_4bit_quant_type="nf4")}
    model = AutoModelForCausalLM.from_pretrained(args.model, device_map={"": 0}, **kwargs).eval()
    started, calls, written = time.time(), 0, 0
    print(f"shard {args.shard}/{args.shards}: {args.calls} calls of {args.sentences} sentences, model {args.model}", flush=True)
    with open(args.out, "w", encoding="utf-8") as out:
        while calls < args.calls and (not args.minutes or (time.time() - started) / 60 < args.minutes):
            batch = [make_prompt(rng, args.sentences, args.style) for _ in range(min(args.batch, args.calls - calls))]
            texts = [tok.apply_chat_template(m, tokenize=False, add_generation_prompt=True) for _, _, _, m in batch]
            enc = tok(texts, return_tensors="pt", padding=True).to(model.device)
            with torch.no_grad():
                gen = model.generate(**enc, max_new_tokens=args.max_new_tokens, do_sample=True, temperature=args.temperature,
                                     top_p=0.95, repetition_penalty=1.05, pad_token_id=tok.pad_token_id or tok.eos_token_id)
            replies = tok.batch_decode(gen[:, enc["input_ids"].shape[1]:], skip_special_tokens=True)
            for (topic, register, seeds, _), reply in zip(batch, replies):
                for sentence in parse_lines(reply):
                    out.write(json.dumps({"text": sentence, "topic": topic, "register": register, "seeds": seeds}, ensure_ascii=False) + "\n")
                    written += 1
            calls += len(batch)
            out.flush()
            if (calls // args.batch) % 5 == 0:
                print(f"  {calls} calls, {written} sentences, {written / max(1e-9, time.time() - started):.1f} sentences/s", flush=True)
    print(f"done: {calls} calls, {written} sentences, {(time.time() - started) / 60:.1f} min", flush=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--filter", default=None, help="raw sentences file to filter instead of generating")
    ap.add_argument("--vocab", default=None)
    ap.add_argument("--style", choices=["formal", "casual"], default="formal", help="casual: short chat messages")
    ap.add_argument("--model", default="Qwen/Qwen2.5-3B-Instruct")
    ap.add_argument("--load-4bit", action="store_true", help="for a 7B model on a 16 GB card")
    ap.add_argument("--shard", type=int, default=0)
    ap.add_argument("--shards", type=int, default=1)
    ap.add_argument("--calls", type=int, default=100)
    ap.add_argument("--sentences", type=int, default=10, help="sentences asked per call")
    ap.add_argument("--batch", type=int, default=24)
    ap.add_argument("--max-new-tokens", type=int, default=420)
    ap.add_argument("--temperature", type=float, default=1.0)
    ap.add_argument("--minutes", type=float, default=0)
    ap.add_argument("--seed", type=int, default=7)
    args = ap.parse_args()
    if args.filter:
        filter_file(args.filter, args.vocab, args.out, 3 if args.style == "casual" else 4)
    else:
        generate(args)


if __name__ == "__main__":
    main()
