"""Đọc kết quả `cargo check --message-format=json` và in lỗi/cảnh báo thành annotation của GitHub Actions.

GitHub giữ tối đa 10 lỗi và 10 cảnh báo mỗi bước, nên in lỗi trước. Mỗi annotation chứa toàn bộ thông báo của
trình biên dịch (đổi xuống dòng thành %0A theo cú pháp workflow command).
"""

import json
import sys


def escape(text: str) -> str:
    return text.replace("%", "%25").replace("\r", "").replace("\n", "%0A")


def main(path: str) -> None:
    # Console của máy Windows trên GitHub không phải UTF-8: lỗi có trích dòng code tiếng Việt sẽ làm print() hỏng.
    sys.stdout.reconfigure(encoding="utf-8")
    messages = []
    with open(path, encoding="utf-8", errors="replace") as f:
        for line in f:
            try:
                item = json.loads(line)
            except json.JSONDecodeError:
                continue
            if item.get("reason") != "compiler-message":
                continue
            msg = item["message"]
            if msg.get("level") not in ("error", "warning"):
                continue
            span = next((s for s in msg.get("spans", []) if s.get("is_primary")), None)
            messages.append((msg["level"], span, msg.get("rendered") or msg.get("message", "")))

    messages.sort(key=lambda m: m[0] != "error")
    for level, span, rendered in messages:
        where = ""
        if span:
            where = f" file=src-tauri/{span['file_name']},line={span['line_start']},col={span['column_start']}"
        print(f"::{level}{where}::{escape(rendered)}")
    print(f"{sum(m[0] == 'error' for m in messages)} error(s), {sum(m[0] == 'warning' for m in messages)} warning(s)")


if __name__ == "__main__":
    main(sys.argv[1])
