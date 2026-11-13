| model | path | accuracy | unparsed | output tokens | p50 ms | p95 ms |
|---|---|---|---|---|---|---|
| qwen3.5-2b | readout, whole prompt | 0.495 | 0 | 0.0 | 87 | 90 |
| qwen3.5-2b | readout, resident prefix | 0.495 | 0 | 0.0 | 30 | 32 |
| qwen3.5-2b | generate a letter, whole prompt | 0.495 | 0 | 1.0 | 95 | 103 |
| qwen3.5-2b | generate JSON, whole prompt | 0.502 | 0 | 6.5 | 138 | 160 |
| qwen3.5-4b | readout, whole prompt | 0.827 | 0 | 0.0 | 209 | 214 |
| qwen3.5-4b | readout, resident prefix | 0.827 | 0 | 0.0 | 66 | 69 |
| qwen3.5-4b | generate a letter, whole prompt | 0.827 | 0 | 1.0 | 223 | 228 |
| qwen3.5-4b | generate JSON, whole prompt | 0.822 | 0 | 6.3 | 303 | 359 |
| qwen3.5-9b | readout, whole prompt | 0.892 | 0 | 0.0 | 359 | 364 |
| qwen3.5-9b | readout, resident prefix | 0.892 | 0 | 0.0 | 108 | 113 |
| qwen3.5-9b | generate a letter, whole prompt | 0.892 | 0 | 1.0 | 381 | 387 |
| qwen3.5-9b | generate JSON, whole prompt | 0.893 | 0 | 6.3 | 500 | 602 |
