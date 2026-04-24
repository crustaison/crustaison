#!/usr/bin/env python3
"""Embedding subprocess for Crusty — reads text from stdin, prints JSON embedding to stdout."""
import sys, json
from llama_cpp import Llama

MODEL_PATH = "/home/sean/.cache/nexa.ai/nexa_sdk/models/Qwen/Qwen3-Embedding-0.6B-GGUF/Qwen3-Embedding-0.6B-f16.gguf"

_llm = None
def get_llm():
    global _llm
    if _llm is None:
        _llm = Llama(model_path=MODEL_PATH, embedding=True, n_ctx=512, verbose=False)
    return _llm

text = sys.stdin.read().strip()
try:
    llm = get_llm()
    result = llm.create_embedding(text)
    vec = result['data'][0]['embedding']
    json.dump({"embedding": vec}, sys.stdout)
except Exception as e:
    json.dump({"error": str(e)}, sys.stdout)
    sys.exit(1)
