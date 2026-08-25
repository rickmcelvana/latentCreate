# LLM wire captures

Raw responses from real endpoints, replayed in `llm-bridge`'s tests so nothing in
CI needs a running model. Captured 2026-08-24; full analysis in
[docs/LLM-SURFACE.md](../../docs/LLM-SURFACE.md).

| File | Endpoint | What it proves |
|---|---|---|
| `ollama-chat-stream.sse` | Ollama 0.17 `/v1/chat/completions`, `gemma4:12b-it-qat`, prompt "Reply with exactly: tulip" | A model this app recommends for lyrics spends 163 characters on `delta.reasoning` and 5 on `delta.content`. Byte-for-byte as it came off the socket. |
| `ollama-tags.json` | Ollama 0.32.15 `/api/tags`, four entries | A local chat model, an embedding-only model, and two cloud models -- including `families: null` and an empty `parameter_size`. |
| `ollama-pull.ndjson` | Ollama 0.32.15 `/api/pull` of `all-minilm` | The full pull lifecycle in NDJSON: manifest, per-layer progress, verify, write, `success`. `completed` is absent on 12 of the 23 frames. |

Recapture with:

```bash
curl -sN http://127.0.0.1:11434/v1/chat/completions -H "Content-Type: application/json" \
  -d '{"model":"gemma4:12b-it-qat","messages":[{"role":"user","content":"Reply with exactly: tulip"}],"stream":true,"max_tokens":200}'
```

```bash
curl -s http://127.0.0.1:11434/api/tags
curl -sN http://127.0.0.1:11434/api/pull -d '{"model":"all-minilm","stream":true}'
```
