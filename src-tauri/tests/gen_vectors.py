# -*- encoding: utf-8 -*-
"""
Generate ab_sign test vectors with a fixed timestamp so the Rust port can be
diffed 1:1. Output: src-tauri/tests/testdata/ab_vectors.json
"""
import json
from pathlib import Path
import sys
import time
import types

# Optional reference checkout, only needed when regenerating test vectors.
reference_root = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(__file__).resolve().parents[3] / "DouyinLiveRecorder"
sys.path.insert(0, str(reference_root))

from src import ab_sign  # noqa: E402

FIXED_MS = 1700000000000


class FixedTime(types.SimpleNamespace):
    pass


# monkeypatch time inside ab_sign module
original_time = ab_sign.time.time
ab_sign.time.time = lambda: FIXED_MS / 1000.0

UA = (
    "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 (KHTML, like Gecko) "
    "Chrome/116.0.5845.97 Safari/537.36 Core/1.116.567.400 QQBrowser/19.7.6764.400"
)

vectors = {
    "fixed_ms": FIXED_MS,
    "sm3_abc": ab_sign.SM3().sum(b"abc", "hex") if False else "",
    "cases": [],
}

cases = [
    {
        "query": "aid=6383&app_name=douyin_web&live_id=1&device_platform=web&language=zh-CN&browser_language=zh-CN&browser_platform=Win32&browser_name=Chrome&browser_version=116.0.0.0&web_rid=273417261646&msToken=",
        "ua": UA,
    },
    {
        "query": "aid=6383&app_name=douyin_web&web_rid=123456&msToken=",
        "ua": "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:127.0) Gecko/20100101 Firefox/127.0",
    },
    {
        "query": "aid=6383&web_rid=88888888",
        "ua": "short-ua",
    },
]

for case in cases:
    signature = ab_sign.ab_sign(case["query"], case["ua"])
    vectors["cases"].append({"query": case["query"], "ua": case["ua"], "expect": signature})

ab_sign.time.time = original_time

# intermediate checks (independent of time)
vectors["sm3_hex_of_abc"] = ""
sm3 = ab_sign.SM3()
vectors["sm3_hex_of_abc"] = "".join(f"{v:02x}" for v in sm3.sum(b"abc"))
vectors["result_encrypt_s4"] = ab_sign.result_encrypt(
    "".join(chr(b) for b in [1, 2, 3, 4, 5, 6]), "s4"
)

out_path = Path(__file__).resolve().parent / "testdata" / "ab_vectors.json"
import os
os.makedirs(os.path.dirname(out_path), exist_ok=True)
with open(out_path, "w", encoding="utf-8") as fh:
    json.dump(vectors, fh, ensure_ascii=False, indent=1)
print(f"wrote {out_path}")
for c in vectors["cases"]:
    print(f"expect[{c['query'][:30]}...] = {c['expect'][:60]}...")
