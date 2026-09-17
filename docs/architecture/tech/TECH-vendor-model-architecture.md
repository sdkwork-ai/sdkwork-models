> Owner: SDKWork maintainers

Generated: 2026-09-17

> 本文件由 `tools/generate-vendor-model-architecture-doc.mjs` 从模型目录生成，请勿手改。
> 口径：模型表仅收录 `shelfState = listed` 的模型；`Context` = `contextTokens / 1000` 向下取整；
> `Pricing` 取 official 侧 `llm_input_token` / `llm_output_token` 单价（缺则 N/A），分时/分档变体不展开；
> `Modalities` 取 `model.capabilities` 原序；`Client APIs` 取该 vendor 各 region 中最强的 `supportStatus`。

## Vendor Summary

| # | Vendor Code | Display Name | Open Source | Protocols | Regions | Client APIs |
|---|-------------|--------------|-------------|-----------|---------|-------------|
| 1 | alibaba | Alibaba Cloud | No | anthropic_messages, openai_compatible, openai_responses, vendor_native | cn, global | CC:partial / CX:convert / GC:convert |
| 2 | anthropic | Anthropic | No | anthropic_messages | global | CC:supported / CX:unsupported / GC:unsupported |
| 3 | baidu | Baidu AI Cloud | No | openai_compatible, vendor_native | cn | CC:unsupported / CX:unsupported / GC:unsupported |
| 4 | black_forest_labs | Black Forest Labs | No | vendor_native | global | CC:unsupported / CX:unsupported / GC:unsupported |
| 5 | bytedance | ByteDance | No | openai_compatible, openai_responses, vendor_native | cn, global | CC:unsupported / CX:unsupported / GC:unsupported |
| 6 | deepseek | DeepSeek | No | anthropic_messages, openai_compatible, openai_responses | cn, global | CC:convert / CX:convert / GC:convert |
| 7 | elevenlabs | ElevenLabs | No | vendor_native | global | CC:unsupported / CX:unsupported / GC:unsupported |
| 8 | google | Google | No | google_gemini, openai_compatible, vendor_native | global | CC:unsupported / CX:unsupported / GC:supported |
| 9 | kuaishou | Kuaishou | No | vendor_native | cn, global | CC:unsupported / CX:unsupported / GC:unsupported |
| 10 | luma_ai | Luma AI | No | vendor_native | global | CC:unsupported / CX:unsupported / GC:unsupported |
| 11 | meituan | Meituan | Yes | anthropic_messages, openai_compatible | cn | CC:unsupported / CX:unsupported / GC:unsupported |
| 12 | minimax | MiniMax | No | openai_compatible, vendor_native | cn, global | CC:unsupported / CX:unsupported / GC:unsupported |
| 13 | moonshot | Moonshot Kimi | No | anthropic_messages, openai_compatible | cn, global | CC:unsupported / CX:unsupported / GC:unsupported |
| 14 | mureka | Mureka | No | vendor_native | global | CC:unsupported / CX:unsupported / GC:unsupported |
| 15 | openai | OpenAI | No | openai_compatible, openai_responses | global | CC:unsupported / CX:supported / GC:unsupported |
| 16 | pixverse | PixVerse | No | vendor_native | cn, global | CC:unsupported / CX:unsupported / GC:unsupported |
| 17 | runway | Runway | No | vendor_native | global | CC:unsupported / CX:unsupported / GC:unsupported |
| 18 | stability_ai | Stability AI | No | vendor_native | global | CC:unsupported / CX:unsupported / GC:unsupported |
| 19 | stepfun | StepFun | No | anthropic_messages, openai_compatible, openai_responses | cn | CC:unsupported / CX:unsupported / GC:unsupported |
| 20 | suno | Suno | No | vendor_native | global | CC:unsupported / CX:unsupported / GC:unsupported |
| 21 | tencent | Tencent Cloud | No | anthropic_messages, openai_compatible | cn | CC:unsupported / CX:unsupported / GC:unsupported |
| 22 | vidu | Vidu | No | vendor_native | cn, global | CC:unsupported / CX:unsupported / GC:unsupported |
| 23 | xai | xAI | No | openai_compatible, openai_responses | global | CC:unsupported / CX:unsupported / GC:unsupported |
| 24 | xiaomi | Xiaomi MiMo | Yes | anthropic_messages, openai_compatible | cn, global | CC:convert / CX:convert / GC:convert |
| 25 | zhipu | Zhipu AI | No | anthropic_messages, openai_compatible, vendor_native | cn | CC:unsupported / CX:unsupported / GC:unsupported |

## Model Architecture by Region

### Alibaba Cloud (alibaba)

**Region: CN** (CNY)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| qwen3.7-flash | N/A | N/A | CNY 0.20 / 0.80 |
| qwen3.7-max | 1000K | chat | CNY 12.00 / 36.00 |
| qwen3.7-plus | N/A | N/A | CNY 2.00 / 8.00 |
| qwen3.7-text-embedding | N/A | N/A | N/A |
| qwen3.7-text-embedding-flash | N/A | N/A | N/A |
| qwen3.8-flash | 1000K | chat | CNY 0.80 / 2.70 |
| qwen3.8-max | 1000K | chat | CNY 12.00 / 36.00 |
| text-embedding-v4 | N/A | embedding | N/A |
| wan2.6-i2v | N/A | video | N/A |
| wan2.6-r2v | N/A | video | N/A |
| wan2.6-t2i | N/A | image | N/A |
| wan2.6-t2v | N/A | video | N/A |

**Region: GLOBAL** (USD)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| qwen3.7-flash | N/A | N/A | USD 0.03 / 0.13 |
| qwen3.7-max | 1000K | chat | USD 2.50 / 7.50 |
| qwen3.7-plus | N/A | N/A | USD 0.40 / 1.60 |
| qwen3.8-flash | 1000K | chat | USD 0.15 / 0.47 |
| qwen3.8-max | 1000K | chat | USD 2.00 / 6.00 |

### Anthropic (anthropic)

**Region: GLOBAL** (USD)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| claude-fable-5 | 1000K | chat | USD 10.00 / 50.00 |
| claude-fable-5-1 | 1000K | chat | USD 10.00 / 50.00 |
| claude-haiku-4-5 | 200K | chat | USD 1.00 / 5.00 |
| claude-mythos-5 | 1000K | chat | USD 10.00 / 50.00 |
| claude-mythos-5-1 | 1000K | chat | USD 10.00 / 50.00 |
| claude-opus-4-5 | N/A | chat | USD 5.00 / 25.00 |
| claude-opus-4-6 | 1000K | chat | USD 5.00 / 25.00 |
| claude-opus-4-7 | 1000K | chat | USD 5.00 / 25.00 |
| claude-opus-4-8 | 1000K | chat | USD 5.00 / 25.00 |
| claude-opus-5 | 1000K | chat, reasoning | USD 5.00 / 25.00 |
| claude-sonnet-4-5 | N/A | chat | USD 3.00 / 15.00 |
| claude-sonnet-4-6 | 1000K | chat | USD 3.00 / 15.00 |
| claude-sonnet-5 | 1000K | chat | USD 2.00 / 10.00 |

### Baidu AI Cloud (baidu)

**Region: CN** (CNY)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| ernie-4.5-turbo-128k | 128K | chat | CNY 0.80 / 3.20 |
| ernie-5.0 | N/A | N/A | CNY 6.00 / 24.00 |
| ernie-5.0-thinking-preview | 128K | chat | CNY 6.00 / 24.00 |
| ernie-5.1 | 128K | chat | CNY 4.00 / 18.00 |
| ernie-x1.1 | 64K | chat, reasoning | CNY 1.00 / 4.00 |

### Black Forest Labs (black_forest_labs)

**Region: GLOBAL** (USD)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| flux-2-flex | N/A | image | N/A |
| flux-2-klein-4b | N/A | image | N/A |
| flux-2-klein-9b | N/A | image | N/A |
| flux-2-max | N/A | image | N/A |
| flux-2-pro | N/A | image | N/A |
| flux-3 | N/A | image, video, audio | N/A |
| flux-kontext-max | N/A | N/A | N/A |
| flux-kontext-pro | N/A | N/A | N/A |
| flux-pro-1.0-fill | N/A | N/A | N/A |
| flux-pro-1.1 | N/A | N/A | N/A |
| flux-pro-1.1-ultra | N/A | N/A | N/A |

### ByteDance (bytedance)

**Region: CN** (CNY)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| doubao-seed-2-0-code-preview-260215 | 256K | chat | CNY 3.20 / 16.00 |
| doubao-seed-2-0-lite-260215 | 256K | chat, reasoning | CNY 0.60 / 3.60 |
| doubao-seed-2-0-lite-260428 | 262K | chat, reasoning | CNY 0.60 / 3.60 |
| doubao-seed-2-0-mini-260215 | 128K | chat | CNY 0.20 / 2.00 |
| doubao-seed-2-0-mini-260428 | 262K | chat, reasoning | CNY 0.20 / 2.00 |
| doubao-seed-2-0-pro-260215 | 256K | chat | CNY 3.20 / 16.00 |
| doubao-seed-2-1-pro-260628 | 256K | chat, reasoning | CNY 6.00 / 30.00 |
| doubao-seed-2-1-pro-260915 | 1048K | chat, reasoning | CNY 6.00 / 30.00 |
| doubao-seed-2-1-turbo-260628 | 256K | chat, reasoning | CNY 3.00 / 15.00 |
| doubao-seed-evolving | 1048K | chat, reasoning | CNY 6.00 / 30.00 |
| doubao-seedance-2-0-260128 | N/A | video | N/A |
| doubao-seedance-2-0-fast-260128 | N/A | video | N/A |
| doubao-seedance-2-0-mini-260615 | N/A | video | N/A |
| doubao-seedance-2-5-260628 | N/A | video | N/A |
| doubao-seedream-4-0-250828 | N/A | image | N/A |
| doubao-seedream-4-5-251128 | N/A | image | N/A |
| doubao-seedream-5-0-lite-260128 | N/A | image | N/A |
| doubao-seedream-5-0-pro-260628 | N/A | image | N/A |
| seed-music-gensong-v4 | N/A | music | N/A |
| seed-tts-2.0-expressive | N/A | audio | N/A |
| seed-tts-2.0-standard | N/A | audio | N/A |

**Region: GLOBAL** (USD)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| dola-seed-2-1-turbo-260628 | 262K | chat, reasoning | USD 0.50 / 2.50 |
| dreamina-seedance-2-0-260128 | N/A | video | N/A |
| dreamina-seedance-2-0-fast-260128 | N/A | video | N/A |
| dreamina-seedance-2-0-mini-260615 | N/A | video | N/A |
| dreamina-seedance-2-5-260628 | N/A | video | N/A |
| seed-2-0-code-preview-260328 | 262K | chat, reasoning | USD 0.50 / 3.00 |
| seed-2-0-lite-260228 | 262K | chat, reasoning | USD 0.25 / 2.00 |
| seed-2-0-lite-260428 | 262K | chat, reasoning | USD 0.25 / 2.00 |
| seed-2-0-mini-260215 | 262K | chat, reasoning | USD 0.10 / 0.40 |
| seed-2-0-mini-260428 | 262K | chat, reasoning | USD 0.10 / 0.40 |
| seed-2-0-pro-260328 | 262K | chat, reasoning | USD 0.50 / 3.00 |

### DeepSeek (deepseek)

**Region: CN** (CNY)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| deepseek-flash | 1048K | chat | CNY 1.00 / 4.00 |
| deepseek-v4-pro | 1048K | chat | CNY 3.00 / 6.00 |

**Region: GLOBAL** (USD)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| deepseek-flash | 1048K | chat | USD 0.15 / 0.60 |
| deepseek-v4-pro | 1048K | chat | USD 0.43 / 0.87 |

### ElevenLabs (elevenlabs)

**Region: GLOBAL** (USD)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| eleven_flash_v2 | N/A | N/A | N/A |
| eleven_flash_v2_5 | N/A | audio | N/A |
| eleven_multilingual_v2 | N/A | audio | N/A |
| eleven_text_to_sound_v2 | N/A | sfx | N/A |
| eleven_v3 | N/A | audio | N/A |
| eleven_v3_conversational | N/A | audio | N/A |
| music_v1 | N/A | N/A | N/A |
| music_v2 | N/A | music | N/A |
| music_v2_5 | N/A | N/A | N/A |
| scribe_v2 | N/A | audio | N/A |
| scribe_v2_medical | N/A | N/A | N/A |
| scribe_v2_realtime | N/A | audio | N/A |

### Google (google)

**Region: GLOBAL** (USD)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| gemini-2.5-flash-preview-tts | 8K | audio | N/A |
| gemini-2.5-pro-preview-tts | 8K | audio | N/A |
| gemini-3-flash-preview | 1048K | chat | USD 0.50 / 3.00 |
| gemini-3-pro-image | 32K | image | N/A |
| gemini-3.1-flash-image | 32K | image | N/A |
| gemini-3.1-flash-lite | 1048K | chat | USD 0.25 / 1.50 |
| gemini-3.1-flash-lite-image | N/A | N/A | USD 0.25 / 1.50 |
| gemini-3.1-flash-live-preview | 131K | chat, audio | USD 0.75 / 4.50 |
| gemini-3.1-flash-tts-preview | 32K | audio | N/A |
| gemini-3.1-pro-preview | 1048K | chat | USD 2.00 / 12.00 |
| gemini-3.5-flash | 1048K | chat | N/A |
| gemini-3.5-flash-lite | 1048K | chat | USD 0.30 / 2.50 |
| gemini-3.5-live-translate-preview | N/A | N/A | N/A |
| gemini-3.5-transcribe | N/A | N/A | N/A |
| gemini-3.5-transcribe-live | N/A | N/A | N/A |
| gemini-3.6-flash | 1048K | chat | USD 0.75 / 3.75 |
| gemini-3.7-flash | 1048K | chat | USD 0.75 / 3.75 |
| gemini-3.8-flash | 1048K | chat | USD 0.75 / 3.75 |
| gemini-3.8-live | N/A | N/A | USD 0.75 / 4.50 |
| gemini-3.8-live-extended-thinking | N/A | N/A | USD 0.75 / 4.50 |
| gemini-embedding-2 | 8K | embedding | N/A |
| gemini-omni-1.1-flash | N/A | video | USD 1.50 / 9.00 |
| gemini-omni-flash-preview | N/A | N/A | USD 1.50 / 9.00 |
| gemini-robotics-er-2-preview | N/A | N/A | USD 1.00 / 5.00 |
| gemini-robotics-er-2-streaming-preview | N/A | N/A | USD 1.00 / 5.00 |
| lyria-3-clip-preview | N/A | N/A | N/A |
| lyria-3-pro-preview | N/A | N/A | N/A |
| lyria-3.5 | N/A | N/A | N/A |
| veo-3.1-fast-generate-preview | N/A | video | N/A |
| veo-3.1-generate-preview | N/A | video | N/A |
| veo-3.1-lite-generate-preview | N/A | video | N/A |

### Kuaishou (kuaishou)

**Region: CN** (CNY)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| kling-3.0-turbo | N/A | video | N/A |
| kling-ai-avatar-v2 | N/A | video | N/A |
| kling-image-o1 | N/A | image | N/A |
| kling-sound-t2a | N/A | sfx | N/A |
| kling-sound-v2a | N/A | sfx | N/A |
| kling-v2-1 | N/A | image | N/A |
| kling-v2-5-turbo | N/A | video | N/A |
| kling-v2-6 | N/A | video | N/A |
| kling-v3 | N/A | video | N/A |
| kling-v3-omni | N/A | video | N/A |
| kling-video-o1 | N/A | video | N/A |

**Region: GLOBAL** (USD)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| kling-3.0-turbo | N/A | video | N/A |
| kling-ai-avatar-v2 | N/A | video | N/A |
| kling-image-o1 | N/A | image | N/A |
| kling-sound-t2a | N/A | sfx | N/A |
| kling-sound-v2a | N/A | sfx | N/A |
| kling-v2-1 | N/A | image | N/A |
| kling-v2-5-turbo | N/A | video | N/A |
| kling-v2-6 | N/A | video | N/A |
| kling-v3 | N/A | video | N/A |
| kling-v3-omni | N/A | video | N/A |
| kling-video-o1 | N/A | video | N/A |

### Luma AI (luma_ai)

**Region: GLOBAL** (USD)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| ray-3.2 | N/A | video | N/A |

### Meituan (meituan)

**Region: CN** (CNY)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| longcat-2.0 | 1048K | chat, reasoning | CNY 2.00 / 8.00 |

### MiniMax (minimax)

**Region: CN** (CNY)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| hailuo-02 | N/A | video | N/A |
| hailuo-2.3 | N/A | video | N/A |
| hailuo-2.3-fast | N/A | video | N/A |
| image-01 | N/A | image | N/A |
| image-01-live | N/A | image | N/A |
| M2-her | 65K | chat | CNY 2.10 / 8.40 |
| MiniMax-H3 | N/A | video | N/A |
| MiniMax-H3-Context-IR | N/A | chat, video | CNY 5.80 / 23.00 |
| MiniMax-H3-Max | N/A | video | N/A |
| MiniMax-H3-Regeneration | N/A | video | N/A |
| MiniMax-M2 | 204K | chat | CNY 2.10 / 8.40 |
| MiniMax-M2.1 | 204K | chat | CNY 2.10 / 8.40 |
| MiniMax-M2.1-highspeed | 204K | chat | CNY 4.20 / 16.80 |
| MiniMax-M2.5 | 204K | chat | CNY 2.10 / 8.40 |
| MiniMax-M2.5-highspeed | 204K | chat | CNY 4.20 / 16.80 |
| MiniMax-M2.7 | 204K | chat | CNY 2.10 / 8.40 |
| MiniMax-M2.7-highspeed | 204K | chat | CNY 4.20 / 16.80 |
| MiniMax-M3 | 1000K | chat, code | CNY 2.10 / 8.40 |
| speech-02-hd | N/A | audio | N/A |
| speech-02-turbo | N/A | audio | N/A |
| speech-2.6-hd | N/A | audio | N/A |
| speech-2.6-turbo | N/A | audio | N/A |
| speech-2.8-hd | N/A | audio | N/A |
| speech-2.8-turbo | N/A | audio | N/A |

**Region: GLOBAL** (USD)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| hailuo-02 | N/A | video | N/A |
| hailuo-2.3 | N/A | video | N/A |
| hailuo-2.3-fast | N/A | video | N/A |
| image-01 | N/A | image | N/A |
| M2-her | 65K | chat | USD 0.30 / 1.20 |
| MiniMax-H3 | N/A | video | N/A |
| MiniMax-H3-Context-IR | N/A | chat, video | USD 0.90 / 3.60 |
| MiniMax-H3-Max | N/A | video | N/A |
| MiniMax-H3-Regeneration | N/A | video | N/A |
| MiniMax-M2 | 204K | chat | USD 0.30 / 1.20 |
| MiniMax-M2.1 | 204K | chat | USD 0.30 / 1.20 |
| MiniMax-M2.1-highspeed | 204K | chat | USD 0.60 / 2.40 |
| MiniMax-M2.5 | 204K | chat | USD 0.30 / 1.20 |
| MiniMax-M2.5-highspeed | 204K | chat | USD 0.60 / 2.40 |
| MiniMax-M2.7 | 204K | chat | USD 0.30 / 1.20 |
| MiniMax-M2.7-highspeed | 204K | chat | USD 0.60 / 2.40 |
| MiniMax-M3 | 1000K | chat, code | USD 0.30 / 1.20 |
| music-cover | N/A | music | N/A |
| speech-02-hd | N/A | audio | N/A |
| speech-02-turbo | N/A | audio | N/A |
| speech-2.6-hd | N/A | audio | N/A |
| speech-2.6-turbo | N/A | audio | N/A |
| speech-2.8-hd | N/A | audio | N/A |
| speech-2.8-turbo | N/A | audio | N/A |

### Moonshot Kimi (moonshot)

**Region: CN** (CNY)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| kimi-k2.6 | 262K | chat | CNY 6.50 / 27.00 |
| kimi-k2.7-code | 262K | chat, code, reasoning | CNY 6.50 / 27.00 |
| kimi-k2.7-code-01hw | 262K | chat, code, reasoning | CNY 6.50 / 27.00 |
| kimi-k2.7-code-highspeed | 262K | chat, code, reasoning | CNY 13.00 / 54.00 |
| kimi-k3 | 1048K | chat | CNY 20.00 / 100.00 |

**Region: GLOBAL** (USD)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| kimi-k2.6 | 262K | chat | USD 0.95 / 4.00 |
| kimi-k2.7-code | 262K | chat, code, reasoning | USD 0.95 / 4.00 |
| kimi-k2.7-code-01hw | 262K | chat, code, reasoning | USD 0.95 / 4.00 |
| kimi-k2.7-code-highspeed | 262K | chat, code, reasoning | USD 1.90 / 8.00 |
| kimi-k3 | 1048K | chat | USD 3.00 / 15.00 |

### Mureka (mureka)

**Region: GLOBAL** (USD)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| mureka-o2 | N/A | music | N/A |
| mureka-v7.6 | N/A | music | N/A |
| mureka-v8 | N/A | music | N/A |
| mureka-v9 | N/A | music | N/A |
| mureka-v9.5 | N/A | music | N/A |

### OpenAI (openai)

**Region: GLOBAL** (USD)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| gpt-4o-mini-transcribe | 128K | audio | USD 1.25 / 5.00 |
| gpt-4o-mini-tts | N/A | audio | N/A |
| gpt-4o-transcribe | N/A | audio | USD 2.50 / 10.00 |
| gpt-4o-transcribe-diarize | N/A | audio | USD 2.50 / 10.00 |
| gpt-5.3-codex | N/A | chat | USD 1.75 / 14.00 |
| gpt-5.5-cyber | N/A | chat | USD 12.50 / 75.00 |
| gpt-5.6-cyber | N/A | chat | USD 12.50 / 75.00 |
| gpt-5.6-luna | 1050K | chat | USD 0.20 / 1.20 |
| gpt-5.6-sol | 1050K | chat | USD 4.00 / 20.00 |
| gpt-5.6-terra | 1050K | chat | USD 2.00 / 12.00 |
| gpt-6-astra | 1050K | chat | USD 10.00 / 50.00 |
| gpt-audio-1.5 | N/A | audio, chat | USD 2.50 / 10.00 |
| gpt-image-2 | N/A | image | N/A |
| gpt-image-2.5-flare | N/A | image | N/A |
| gpt-image-2.5-sunburst | N/A | image | N/A |
| gpt-live-1 | N/A | audio, chat | N/A |
| gpt-live-transcribe | N/A | audio | N/A |
| gpt-realtime-1.5 | N/A | chat, audio | USD 4.00 / 16.00 |
| gpt-realtime-2 | 128K | chat, audio | USD 4.00 / 24.00 |
| gpt-realtime-2.1 | 128K | chat, audio | USD 4.00 / 24.00 |
| gpt-realtime-2.1-mini | 128K | chat, audio | USD 0.60 / 2.40 |
| gpt-realtime-mini | 128K | audio | USD 0.60 / 2.40 |
| gpt-realtime-translate | 128K | audio | USD 4.00 / 16.00 |
| gpt-realtime-whisper | 128K | audio | USD 3.00 / 12.00 |
| gpt-transcribe | N/A | audio | N/A |
| sora-2 | N/A | video | N/A |
| sora-2-pro | N/A | video | N/A |
| text-embedding-3-large | N/A | embedding | N/A |
| text-embedding-3-small | N/A | embedding | N/A |
| tts-1-hd | N/A | audio | N/A |
| whisper-1 | N/A | audio | N/A |

### PixVerse (pixverse)

**Region: CN** (CNY)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| pixverse-c1-t2v | N/A | video | N/A |
| pixverse-v6-t2v | N/A | video | N/A |

**Region: GLOBAL** (USD)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| pixverse-v6 | N/A | video | N/A |

### Runway (runway)

**Region: GLOBAL** (USD)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| act_two | N/A | video | N/A |
| aleph2 | N/A | video | N/A |
| gemini_2.5_flash | N/A | image | N/A |
| gemini_image3_pro | N/A | image | N/A |
| gemini_omni_flash | N/A | video | N/A |
| gen4_image | N/A | image | N/A |
| gen4_image_turbo | N/A | image | N/A |
| gen4_turbo | N/A | video | N/A |
| gen4.5 | N/A | video | N/A |
| gpt_image_2 | N/A | image | N/A |
| gpt_image_2_5_flare | N/A | image | N/A |
| gpt_image_2_5_sunburst | N/A | image | N/A |
| grok_imagine_1_5 | N/A | video | N/A |
| grok_imagine_image_2 | N/A | image | N/A |
| gwm1_avatars | N/A | video, audio | N/A |
| h3_max | N/A | video | N/A |
| hailuo3 | N/A | video | N/A |
| happyhorse_1_0 | N/A | video | N/A |
| magnific_precision_upscaler_v2 | N/A | image | N/A |
| muse_image | N/A | image | N/A |
| ruby | N/A | video | N/A |
| seedance2 | N/A | video | N/A |
| seedance2_5 | N/A | video | N/A |
| seedance2_fast | N/A | video | N/A |
| seedance2_mini | N/A | video | N/A |
| seedream5_lite | N/A | image | N/A |
| seedream5_pro | N/A | image | N/A |
| veo3.1 | N/A | video | N/A |
| veo3.1_fast | N/A | video | N/A |
| wan3 | N/A | video | N/A |

### Stability AI (stability_ai)

**Region: GLOBAL** (USD)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| stable-audio-2.5 | N/A | music | N/A |
| stable-audio-2.5-sfx | N/A | sfx | N/A |
| stable-audio-3.0 | N/A | music | N/A |
| stable-diffusion-3-5-flash | N/A | N/A | N/A |
| stable-diffusion-3-5-large | N/A | N/A | N/A |
| stable-diffusion-3-5-large-turbo | N/A | N/A | N/A |
| stable-diffusion-3-5-medium | N/A | N/A | N/A |
| stable-image-core | N/A | image | N/A |
| stable-image-ultra | N/A | image | N/A |

### StepFun (stepfun)

**Region: CN** (CNY)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| step-3.5-flash | 256K | chat, reasoning | CNY 0.70 / 2.10 |
| step-3.5-flash-2603 | 256K | chat, reasoning | CNY 0.70 / 2.10 |
| step-3.7-flash | 256K | chat, reasoning | CNY 1.35 / 8.10 |

### Suno (suno)

**Region: GLOBAL** (USD)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|

### Tencent Cloud (tencent)

**Region: CN** (CNY)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| hy-mt2-lite | N/A | chat | CNY 0.30 / 1.20 |
| hy-mt2-plus | N/A | chat | CNY 0.50 / 2.00 |
| hy-mt2-pro | N/A | chat | CNY 0.50 / 2.00 |
| hy-role | N/A | chat | CNY 2.40 / 9.60 |
| hy-role-latest | N/A | chat | CNY 2.40 / 9.60 |
| hy3 | 256K | chat, reasoning | CNY 1.00 / 4.00 |
| hy4-preview | 1000K | chat | CNY 6.00 / 18.00 |

### Vidu (vidu)

**Region: CN** (CNY)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| audio1.0-text2audio | N/A | sfx | N/A |
| audio1.0-timing2audio | N/A | sfx | N/A |
| viduq3 | N/A | video, audio | N/A |
| viduq3-mix | N/A | video | N/A |
| viduq3-pro | N/A | video | N/A |
| viduq3-pro-fast | N/A | video | N/A |
| viduq3-turbo | N/A | video | N/A |

**Region: GLOBAL** (USD)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| audio1.0-text2audio | N/A | sfx | N/A |
| audio1.0-timing2audio | N/A | sfx | N/A |
| viduq3 | N/A | video, audio | N/A |
| viduq3-mix | N/A | video | N/A |
| viduq3-pro | N/A | video | N/A |
| viduq3-pro-fast | N/A | video | N/A |
| viduq3-turbo | N/A | video | N/A |

### xAI (xai)

**Region: GLOBAL** (USD)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| grok-4.20-0309-non-reasoning | 1000K | chat | USD 1.25 / 2.50 |
| grok-4.20-0309-reasoning | 1000K | chat, reasoning | USD 1.25 / 2.50 |
| grok-4.20-multi-agent-0309 | 1000K | chat, reasoning | USD 1.25 / 2.50 |
| grok-4.3 | 1000K | chat | USD 1.25 / 2.50 |
| grok-4.5 | 500K | chat, reasoning | USD 2.00 / 6.00 |
| grok-4.6 | 500K | chat, reasoning | USD 2.00 / 6.00 |
| grok-build-0.1 | 256K | chat | USD 1.00 / 2.00 |
| grok-imagine-image | N/A | image | N/A |
| grok-imagine-image-2.0 | N/A | image | N/A |
| grok-imagine-image-quality | N/A | image | N/A |
| grok-imagine-video | N/A | video | N/A |
| grok-imagine-video-1.5 | N/A | video | N/A |

### Xiaomi MiMo (xiaomi)

**Region: CN** (CNY)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| mimo-v2.5 | 1048K | chat, image, audio, video | CNY 1.00 / 2.00 |
| mimo-v2.5-asr | 8K | audio | N/A |
| mimo-v2.5-pro | 1048K | chat | CNY 3.00 / 6.00 |

**Region: GLOBAL** (USD)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| mimo-v2.5 | 1048K | chat, image, audio, video | USD 0.14 / 0.28 |
| mimo-v2.5-asr | 8K | audio | N/A |
| mimo-v2.5-pro | 1048K | chat | USD 0.43 / 0.87 |

### Zhipu AI (zhipu)

**Region: CN** (CNY)

| Model ID | Context | Modalities | Pricing (Input/Output) |
|----------|---------|------------|------------------------|
| cogvideox-3 | N/A | video | N/A |
| cogview-4-250304 | N/A | image | N/A |
| embedding-2 | 8K | embedding | N/A |
| embedding-3 | N/A | embedding | N/A |
| glm-5 | 200K | chat, reasoning | CNY 4.00 / 18.00 |
| glm-5-turbo | 200K | chat, reasoning | CNY 5.00 / 22.00 |
| glm-5.1 | 200K | chat | CNY 6.00 / 24.00 |
| glm-5.2 | 1000K | chat, reasoning | CNY 8.00 / 28.00 |
| glm-5.3 | 1000K | chat | CNY 8.00 / 28.00 |
| glm-5.3-flash | 1000K | chat | CNY 0.80 / 2.80 |
| glm-5v-turbo | 200K | chat, reasoning | CNY 5.00 / 22.00 |
| glm-image | N/A | image | N/A |
| glm-ocr | 32K | chat | CNY 0.20 / 0.20 |

## Statistics Summary

### Vendor Count by Region

| Region | Vendors | Models | Pricing Files |
|--------|---------|--------|---------------|
| CN | 14 | 140 | 134 |
| GLOBAL | 20 | 285 | 272 |

### Client API Support

| API | Supported | Partial | Convert | Unsupported |
|-----|-----------|---------|---------|-------------|
| claude_code | 1 | 1 | 2 | 21 |
| codex | 1 | 0 | 3 | 21 |
| gemini_cli | 1 | 0 | 3 | 21 |

### Protocol Support

| Protocol | Vendors |
|----------|---------|
| vendor_native | 16 |
| openai_compatible | 14 |
| anthropic_messages | 9 |
| openai_responses | 6 |
| google_gemini | 1 |

### Capability Support

| Capability | Vendors |
|------------|---------|
| chat | 15 |
| video | 14 |
| image | 11 |
| reasoning | 10 |
| audio | 7 |
| music | 7 |
| embedding | 4 |
| sfx | 4 |
| code | 2 |
| tool | 2 |
| streaming | 1 |
