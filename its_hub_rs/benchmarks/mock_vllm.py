"""Mock vLLM server for benchmarking. Returns canned responses with ~5ms latency."""
import asyncio
import time
import uuid

import uvicorn
from fastapi import FastAPI

app = FastAPI()


@app.post("/v1/chat/completions")
async def chat_completions(request: dict):
    """Return a canned response with configurable delay."""
    await asyncio.sleep(0.005)  # 5ms simulated inference latency
    return {
        "id": f"chatcmpl-{uuid.uuid4()}",
        "object": "chat.completion",
        "created": int(time.time()),
        "model": request.get("model", "mock-model"),
        "choices": [
            {
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "The answer is \\boxed{42}",
                },
                "finish_reason": "stop",
            }
        ],
        "usage": {
            "prompt_tokens": 50,
            "completion_tokens": 20,
            "total_tokens": 70,
        },
    }


@app.get("/v1/models")
async def models():
    return {
        "data": [{"id": "mock-model", "object": "model", "owned_by": "mock"}]
    }


if __name__ == "__main__":
    uvicorn.run(app, host="127.0.0.1", port=9999, log_level="warning")
