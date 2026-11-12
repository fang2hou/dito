| model | round | accuracy | auto share | auto accuracy | auto wrong | won | lost | McNemar p |
|---|---|---|---|---|---|---|---|---|
| qwen3.5-2b | 1 | 92.8 | 79.3 | 98.5 | 7 |  |  |  |
| qwen3.5-2b | 2 | 93.2 | 81.2 | 98.8 | 6 | 14 | 12 | 0.8450 |
| qwen3.5-4b | 1 | 95.5 | 85.5 | 99.2 | 4 |  |  |  |
| qwen3.5-4b | 2 | 95.5 | 86.5 | 98.5 | 8 | 6 | 6 | 1.0000 |
| qwen3.5-9b | 1 | 96.3 | 92.0 | 99.1 | 5 |  |  |  |
| qwen3.5-9b | 2 | 96.7 | 90.3 | 99.3 | 4 | 9 | 7 | 0.8036 |

| model | intent | cases | round 1 | round 2 | won | lost |
|---|---|---|---|---|---|---|
| qwen3.5-2b | polish | 84 | 95.2 | 94.0 | 1 | 2 |
| qwen3.5-2b | rewrite | 144 | 95.8 | 95.1 | 2 | 3 |
| qwen3.5-2b | translate | 60 | 96.7 | 93.3 | 0 | 2 |
| qwen3.5-2b | summarize | 60 | 88.3 | 93.3 | 3 | 0 |
| qwen3.5-2b | reply | 96 | 85.4 | 90.6 | 7 | 2 |
| qwen3.5-2b | ask | 60 | 100.0 | 98.3 | 0 | 1 |
| qwen3.5-2b | cancel | 48 | 87.5 | 89.6 | 1 | 0 |
| qwen3.5-2b | none | 48 | 91.7 | 87.5 | 0 | 2 |
| qwen3.5-4b | polish | 84 | 96.4 | 96.4 | 0 | 0 |
| qwen3.5-4b | rewrite | 144 | 95.1 | 94.4 | 0 | 1 |
| qwen3.5-4b | translate | 60 | 93.3 | 93.3 | 1 | 1 |
| qwen3.5-4b | summarize | 60 | 96.7 | 96.7 | 1 | 1 |
| qwen3.5-4b | reply | 96 | 95.8 | 95.8 | 1 | 1 |
| qwen3.5-4b | ask | 60 | 96.7 | 98.3 | 2 | 1 |
| qwen3.5-4b | cancel | 48 | 93.8 | 93.8 | 0 | 0 |
| qwen3.5-4b | none | 48 | 95.8 | 95.8 | 1 | 1 |
| qwen3.5-9b | polish | 84 | 96.4 | 96.4 | 1 | 1 |
| qwen3.5-9b | rewrite | 144 | 97.2 | 97.9 | 3 | 2 |
| qwen3.5-9b | translate | 60 | 96.7 | 96.7 | 0 | 0 |
| qwen3.5-9b | summarize | 60 | 95.0 | 95.0 | 1 | 1 |
| qwen3.5-9b | reply | 96 | 91.7 | 93.8 | 3 | 1 |
| qwen3.5-9b | ask | 60 | 100.0 | 100.0 | 0 | 0 |
| qwen3.5-9b | cancel | 48 | 97.9 | 100.0 | 1 | 0 |
| qwen3.5-9b | none | 48 | 97.9 | 93.8 | 0 | 2 |

| model | boundary | misread side | cases | round 1 | round 2 |
|---|---|---|---|---|---|
| qwen3.5-2b | reply / rewrite | reply read as rewrite | 96 | 5 | 2 |
| qwen3.5-2b | reply / rewrite | rewrite read as reply | 144 | 1 | 1 |
| qwen3.5-2b | reply / ask | reply read as ask | 96 | 1 | 2 |
| qwen3.5-2b | reply / ask | ask read as reply | 60 | 0 | 0 |
| qwen3.5-2b | ask / none | ask read as none | 60 | 0 | 0 |
| qwen3.5-2b | ask / none | none read as ask | 48 | 1 | 1 |
| qwen3.5-2b | rewrite (shorten) / summarize | rewrite (shorten) read as summarize | 39 | 1 | 2 |
| qwen3.5-2b | rewrite (shorten) / summarize | summarize read as rewrite | 60 | 0 | 0 |
| qwen3.5-2b | polish / rewrite | polish read as rewrite | 84 | 3 | 4 |
| qwen3.5-2b | polish / rewrite | rewrite read as polish | 144 | 3 | 2 |
| qwen3.5-4b | reply / rewrite | reply read as rewrite | 96 | 1 | 1 |
| qwen3.5-4b | reply / rewrite | rewrite read as reply | 144 | 2 | 2 |
| qwen3.5-4b | reply / ask | reply read as ask | 96 | 1 | 2 |
| qwen3.5-4b | reply / ask | ask read as reply | 60 | 1 | 0 |
| qwen3.5-4b | ask / none | ask read as none | 60 | 1 | 0 |
| qwen3.5-4b | ask / none | none read as ask | 48 | 1 | 1 |
| qwen3.5-4b | rewrite (shorten) / summarize | rewrite (shorten) read as summarize | 39 | 1 | 1 |
| qwen3.5-4b | rewrite (shorten) / summarize | summarize read as rewrite | 60 | 0 | 0 |
| qwen3.5-4b | polish / rewrite | polish read as rewrite | 84 | 1 | 1 |
| qwen3.5-4b | polish / rewrite | rewrite read as polish | 144 | 1 | 2 |
| qwen3.5-9b | reply / rewrite | reply read as rewrite | 96 | 2 | 2 |
| qwen3.5-9b | reply / rewrite | rewrite read as reply | 144 | 2 | 1 |
| qwen3.5-9b | reply / ask | reply read as ask | 96 | 2 | 2 |
| qwen3.5-9b | reply / ask | ask read as reply | 60 | 0 | 0 |
| qwen3.5-9b | ask / none | ask read as none | 60 | 0 | 0 |
| qwen3.5-9b | ask / none | none read as ask | 48 | 0 | 1 |
| qwen3.5-9b | rewrite (shorten) / summarize | rewrite (shorten) read as summarize | 39 | 0 | 0 |
| qwen3.5-9b | rewrite (shorten) / summarize | summarize read as rewrite | 60 | 2 | 0 |
| qwen3.5-9b | polish / rewrite | polish read as rewrite | 84 | 1 | 2 |
| qwen3.5-9b | polish / rewrite | rewrite read as polish | 144 | 1 | 1 |

| model | round 1 out-of-fold wrong (right %) | round 2 out-of-fold wrong (right %) |
|---|---|---|
| qwen3.5-2b | 132 of 1375 (90.4) | 137 of 1679 (91.8) |
| qwen3.5-4b | 85 of 1375 (93.8) | 91 of 1679 (94.6) |
| qwen3.5-9b | 73 of 1375 (94.7) | 69 of 1679 (95.9) |
