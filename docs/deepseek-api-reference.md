# DeepSeek API Reference

Source: https://api-docs.deepseek.com

## Models

| Model | Usage | Input (cache miss) | Input (cache hit) | Output |
|-------|-------|--------------------|--------------------|--------|
| deepseek-v4-flash | Default | $0.14/1M | $0.0028/1M | $0.28/1M |
| deepseek-v4-pro | Complex reasoning | Higher | Higher | Higher |

## Base URL

```
https://api.deepseek.com
```

Beta features (FIM, prefix, strict tool calls):
```
https://api.deepseek.com/beta
```

## Authentication

```
Authorization: Bearer {api_key}
```

## Features

### 1. Chat Completion (Standard)

```json
POST /chat/completions
{
  "model": "deepseek-v4-flash",
  "messages": [
    {"role": "system", "content": "You are a helpful assistant."},
    {"role": "user", "content": "Hello!"}
  ],
  "max_tokens": 300,
  "stream": false
}
```

### 2. JSON Output

Set `response_format: {"type": "json_object"}` and include "json" in system or user prompt.

```json
{
  "model": "deepseek-v4-flash",
  "messages": [...],
  "response_format": {"type": "json_object"},
  "max_tokens": 300
}
```

Note: API may occasionally return empty content. Modify prompt to mitigate.

### 3. Thinking Mode

Extended reasoning before answering. No `temperature`/`top_p` in thinking mode.

```json
{
  "model": "deepseek-v4-pro",
  "messages": [...],
  "thinking": {"type": "enabled"},
  "reasoning_effort": "high"
}
```

Response includes `reasoning_content` field alongside `content`.

Effort levels: `high` (default), `max` (complex agents). `low`/`medium` map to `high`.

### 4. Tool Calls

```json
{
  "model": "deepseek-v4-flash",
  "messages": [...],
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "get_weather",
        "description": "Get weather of a location",
        "parameters": {
          "type": "object",
          "properties": {
            "location": {"type": "string"}
          },
          "required": ["location"]
        }
      }
    }
  ]
}
```

Response `tool_calls`: `[{id, function: {name, arguments}, type: "function"}]`

Follow up with `role: "tool"` message containing `tool_call_id` and result.

Strict mode (beta): add `"strict": true` to function definition, use `/beta` base URL.

### 5. Chat Prefix Completion (Beta)

Force output by providing assistant prefix. Requires `/beta` base URL.

```json
{
  "messages": [
    {"role": "user", "content": "Write quicksort"},
    {"role": "assistant", "content": "```python\n", "prefix": true}
  ],
  "stop": ["```"]
}
```

### 6. FIM Completion (Beta)

Fill-in-the-middle. Requires `/beta` base URL. Max 4K tokens.

```json
POST /completions
{
  "model": "deepseek-v4-pro",
  "prompt": "def fib(a):",
  "suffix": "    return fib(a-1) + fib(a-2)",
  "max_tokens": 128
}
```

## Error Codes

| Code | Description | Action |
|------|-------------|--------|
| 400 | Invalid format | Fix request body |
| 401 | Auth fails | Check API key |
| 402 | Insufficient balance | Top up account |
| 422 | Invalid parameters | Fix parameters |
| 429 | Rate limit | Retry with backoff |
| 500 | Server error | Retry with backoff |
| 503 | Server overloaded | Retry with backoff |

## Context Caching (Automatic)

DeepSeek caches prompt prefixes automatically. No special headers needed.

**Important**: Le cache fonctionne sur le préfixe complet des messages, PAS juste le system prompt. Si le user message change à chaque appel (notre cas), le cache ne hit pas.

Pour maximiser le cache :
- Garder les premiers messages identiques entre les appels
- Mettre les éléments variables à la fin du tableau de messages

- Premier appel: cache miss → $0.14/1M input tokens
- Appels suivants avec même préfixe: cache hit → $0.0028/1M input tokens (98% de réduction)

## Response Usage

```json
{
  "usage": {
    "prompt_tokens": 307,
    "completion_tokens": 256,
    "prompt_cache_hit_tokens": 0,
    "prompt_cache_miss_tokens": 307,
    "prompt_tokens_details": {"cached_tokens": 0},
    "completion_tokens_details": {"reasoning_tokens": 162}
  }
}
```

**Notes importantes** :
- `completion_tokens` INCLUT les `reasoning_tokens` (pas besoin d'additionner)
- DeepSeek V4 Flash raisonne par défaut même sans `thinking: enabled`
- `reasoning_tokens` typiques: 100-250 tokens par réponse
- `max_tokens` recommandé: **500** (300 est trop juste avec le reasoning)

Cost = (cache_miss * 0.14 + cache_hit * 0.0028 + completion * 0.28) / 1_000_000

## Exemple réel (mesuré)

```
AVEC thinking (par défaut):
  prompt_tokens: 307, completion_tokens: 256 (reasoning: 162)
  Coût: $0.000115

SANS thinking (désactivé):
  prompt_tokens: 300, completion_tokens: 91
  Coût: $0.000067
  → -65% de tokens, -43% de coût
```

**Recommandation** : Pour les décisions JSON simples, désactiver le thinking mode (`"thinking": {"type": "disabled"}`). Le raisonnement interne gaspille des tokens sans améliorer la qualité de la décision.

Coût typique par appel (sans thinking): ~$0.00007 (~0.007 centime)
