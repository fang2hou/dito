train 1679 (dropped {'same_as_test': 0, 'near_test': 1, 'duplicate': 4}), test 600

### headline

| model | stage | accuracy | macro F1 | ECE | auto share | auto accuracy | auto wrong | zh | ja | en |
|---|---|---|---|---|---|---|---|---|---|---|
| qwen3.5-2b | direct readout, first prompt | 49.5 | 0.406 | 0.077 | 8.0 | 87.5 | 6 | 54.5 | 42.5 | 51.5 |
| qwen3.5-2b | direct readout, built prompt | 70.7 | 0.694 | 0.030 | 22.7 | 94.9 | 7 | 76.0 | 67.0 | 69.0 |
| qwen3.5-2b | head, first prompt | 90.0 | 0.892 | 0.019 | 73.7 | 97.5 | 11 | 93.5 | 90.5 | 86.0 |
| qwen3.5-2b | head, built prompt | 93.2 | 0.928 | 0.017 | 81.2 | 98.8 | 6 | 95.5 | 94.0 | 90.0 |
| qwen3.5-4b | direct readout, first prompt | 82.7 | 0.807 | 0.028 | 50.3 | 98.0 | 6 | 86.0 | 77.0 | 85.0 |
| qwen3.5-4b | direct readout, built prompt | 83.7 | 0.836 | 0.034 | 53.2 | 97.2 | 9 | 87.5 | 78.5 | 85.0 |
| qwen3.5-4b | head, first prompt | 93.7 | 0.938 | 0.019 | 84.5 | 98.8 | 6 | 98.0 | 90.5 | 92.5 |
| qwen3.5-4b | head, built prompt | 95.5 | 0.954 | 0.016 | 86.5 | 98.5 | 8 | 98.5 | 93.0 | 95.0 |
| qwen3.5-9b | direct readout, first prompt | 89.2 | 0.875 | 0.061 | 53.2 | 98.4 | 5 | 93.0 | 87.5 | 87.0 |
| qwen3.5-9b | direct readout, built prompt | 92.5 | 0.919 | 0.024 | 72.8 | 98.6 | 6 | 95.5 | 92.5 | 89.5 |
| qwen3.5-9b | head, first prompt | 95.7 | 0.954 | 0.016 | 90.5 | 98.9 | 6 | 98.0 | 93.0 | 96.0 |
| qwen3.5-9b | head, built prompt | 96.7 | 0.966 | 0.012 | 90.3 | 99.3 | 4 | 98.0 | 95.0 | 97.0 |

### per intent (gold intent, test accuracy)

| model | stage | polish (84) | rewrite (144) | translate (60) | summarize (60) | reply (96) | ask (60) | cancel (48) | none (48) |
|---|---|---|---|---|---|---|---|---|---|
| qwen3.5-2b | direct readout, first prompt | 76.2 | 81.2 | 61.7 | 58.3 | 22.9 | 10.0 | 4.2 | 29.2 |
| qwen3.5-2b | direct readout, built prompt | 61.9 | 77.8 | 66.7 | 65.0 | 78.1 | 75.0 | 75.0 | 52.1 |
| qwen3.5-2b | head, first prompt | 94.0 | 95.8 | 91.7 | 90.0 | 82.3 | 91.7 | 81.2 | 85.4 |
| qwen3.5-2b | head, built prompt | 94.0 | 95.1 | 93.3 | 93.3 | 90.6 | 98.3 | 89.6 | 87.5 |
| qwen3.5-4b | direct readout, first prompt | 92.9 | 92.4 | 93.3 | 83.3 | 78.1 | 55.0 | 77.1 | 70.8 |
| qwen3.5-4b | direct readout, built prompt | 71.4 | 97.9 | 68.3 | 76.7 | 84.4 | 86.7 | 87.5 | 81.2 |
| qwen3.5-4b | head, first prompt | 92.9 | 93.8 | 95.0 | 93.3 | 94.8 | 95.0 | 89.6 | 93.8 |
| qwen3.5-4b | head, built prompt | 96.4 | 94.4 | 93.3 | 96.7 | 95.8 | 98.3 | 93.8 | 95.8 |
| qwen3.5-9b | direct readout, first prompt | 92.9 | 92.4 | 96.7 | 91.7 | 89.6 | 83.3 | 89.6 | 66.7 |
| qwen3.5-9b | direct readout, built prompt | 85.7 | 97.2 | 93.3 | 91.7 | 88.5 | 100.0 | 93.8 | 87.5 |
| qwen3.5-9b | head, first prompt | 94.0 | 97.2 | 96.7 | 95.0 | 95.8 | 96.7 | 93.8 | 93.8 |
| qwen3.5-9b | head, built prompt | 96.4 | 97.9 | 96.7 | 95.0 | 93.8 | 100.0 | 100.0 | 93.8 |

### per tag (test accuracy)

| model | stage | asr_error (156) | self_correction (79) | retraction (32) | negation (46) | filler (231) | code_mix (47) | polite (37) | multi_request (74) | question (112) |
|---|---|---|---|---|---|---|---|---|---|---|
| qwen3.5-2b | direct readout, first prompt | 46.8 | 32.9 | 0.0 | 65.2 | 49.8 | 72.3 | 56.8 | 39.2 | 29.5 |
| qwen3.5-2b | direct readout, built prompt | 67.9 | 40.5 | 62.5 | 80.4 | 68.4 | 91.5 | 62.2 | 55.4 | 72.3 |
| qwen3.5-2b | head, first prompt | 86.5 | 78.5 | 78.1 | 93.5 | 90.5 | 95.7 | 91.9 | 89.2 | 88.4 |
| qwen3.5-2b | head, built prompt | 89.1 | 86.1 | 87.5 | 97.8 | 93.9 | 97.9 | 94.6 | 87.8 | 95.5 |
| qwen3.5-4b | direct readout, first prompt | 75.6 | 68.4 | 78.1 | 87.0 | 81.4 | 89.4 | 91.9 | 75.7 | 73.2 |
| qwen3.5-4b | direct readout, built prompt | 73.7 | 70.9 | 90.6 | 82.6 | 85.3 | 93.6 | 89.2 | 77.0 | 88.4 |
| qwen3.5-4b | head, first prompt | 91.0 | 83.5 | 87.5 | 95.7 | 93.9 | 95.7 | 97.3 | 91.9 | 94.6 |
| qwen3.5-4b | head, built prompt | 91.7 | 94.9 | 100.0 | 100.0 | 96.5 | 97.9 | 91.9 | 95.9 | 94.6 |
| qwen3.5-9b | direct readout, first prompt | 81.4 | 84.8 | 96.9 | 100.0 | 94.4 | 95.7 | 89.2 | 91.9 | 89.3 |
| qwen3.5-9b | direct readout, built prompt | 82.7 | 89.9 | 100.0 | 100.0 | 95.2 | 97.9 | 89.2 | 93.2 | 95.5 |
| qwen3.5-9b | head, first prompt | 93.6 | 92.4 | 96.9 | 97.8 | 97.0 | 100.0 | 94.6 | 97.3 | 95.5 |
| qwen3.5-9b | head, built prompt | 93.6 | 92.4 | 100.0 | 97.8 | 97.0 | 100.0 | 97.3 | 95.9 | 98.2 |

### where neighbouring intents are confused (selected head, built prompt)

#### qwen3.5-2b rules_kw_fewshot_echo, boundary pairs

| boundary | one side | misread | other side | misread |
|---|---|---|---|---|
| reply / rewrite | reply read as rewrite | 2 of 96 | rewrite read as reply | 1 of 144 |
| reply / ask | reply read as ask | 2 of 96 | ask read as reply | 0 of 60 |
| ask / none | ask read as none | 0 of 60 | none read as ask | 1 of 48 |
| rewrite (shorten) / summarize | rewrite (shorten) read as summarize | 2 of 39 | summarize read as rewrite | 0 of 60 |
| polish / rewrite | polish read as rewrite | 4 of 84 | rewrite read as polish | 2 of 144 |

#### qwen3.5-2b rules_kw_fewshot_echo, gold (rows) by predicted (columns)


| gold | polish | rewrite | translate | summarize | reply | ask | cancel | none |
|---|---|---|---|---|---|---|---|---|
| polish | 79 | 4 | 0 | 0 | 1 | 0 | 0 | 0 |
| rewrite | 2 | 137 | 0 | 3 | 1 | 0 | 1 | 0 |
| translate | 0 | 1 | 56 | 2 | 0 | 0 | 0 | 1 |
| summarize | 0 | 0 | 2 | 56 | 0 | 0 | 0 | 2 |
| reply | 1 | 2 | 3 | 0 | 87 | 2 | 1 | 0 |
| ask | 0 | 0 | 0 | 0 | 0 | 59 | 1 | 0 |
| cancel | 0 | 0 | 1 | 1 | 3 | 0 | 43 | 0 |
| none | 2 | 0 | 1 | 0 | 2 | 1 | 0 | 42 |

#### qwen3.5-4b rules_kw_fewshot_echo, boundary pairs

| boundary | one side | misread | other side | misread |
|---|---|---|---|---|
| reply / rewrite | reply read as rewrite | 1 of 96 | rewrite read as reply | 2 of 144 |
| reply / ask | reply read as ask | 2 of 96 | ask read as reply | 0 of 60 |
| ask / none | ask read as none | 0 of 60 | none read as ask | 1 of 48 |
| rewrite (shorten) / summarize | rewrite (shorten) read as summarize | 1 of 39 | summarize read as rewrite | 0 of 60 |
| polish / rewrite | polish read as rewrite | 1 of 84 | rewrite read as polish | 2 of 144 |

#### qwen3.5-4b rules_kw_fewshot_echo, gold (rows) by predicted (columns)


| gold | polish | rewrite | translate | summarize | reply | ask | cancel | none |
|---|---|---|---|---|---|---|---|---|
| polish | 81 | 1 | 1 | 0 | 1 | 0 | 0 | 0 |
| rewrite | 2 | 136 | 0 | 2 | 2 | 1 | 0 | 1 |
| translate | 0 | 1 | 56 | 1 | 1 | 0 | 0 | 1 |
| summarize | 0 | 0 | 0 | 58 | 0 | 1 | 0 | 1 |
| reply | 0 | 1 | 1 | 0 | 92 | 2 | 0 | 0 |
| ask | 0 | 1 | 0 | 0 | 0 | 59 | 0 | 0 |
| cancel | 0 | 1 | 0 | 0 | 0 | 0 | 45 | 2 |
| none | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 46 |

#### qwen3.5-9b rules_kw_fewshot_echo, boundary pairs

| boundary | one side | misread | other side | misread |
|---|---|---|---|---|
| reply / rewrite | reply read as rewrite | 2 of 96 | rewrite read as reply | 1 of 144 |
| reply / ask | reply read as ask | 2 of 96 | ask read as reply | 0 of 60 |
| ask / none | ask read as none | 0 of 60 | none read as ask | 1 of 48 |
| rewrite (shorten) / summarize | rewrite (shorten) read as summarize | 0 of 39 | summarize read as rewrite | 0 of 60 |
| polish / rewrite | polish read as rewrite | 2 of 84 | rewrite read as polish | 1 of 144 |

#### qwen3.5-9b rules_kw_fewshot_echo, gold (rows) by predicted (columns)


| gold | polish | rewrite | translate | summarize | reply | ask | cancel | none |
|---|---|---|---|---|---|---|---|---|
| polish | 81 | 2 | 1 | 0 | 0 | 0 | 0 | 0 |
| rewrite | 1 | 141 | 0 | 1 | 1 | 0 | 0 | 0 |
| translate | 0 | 1 | 58 | 0 | 0 | 0 | 0 | 1 |
| summarize | 2 | 0 | 0 | 57 | 0 | 0 | 0 | 1 |
| reply | 0 | 2 | 2 | 0 | 90 | 2 | 0 | 0 |
| ask | 0 | 0 | 0 | 0 | 0 | 60 | 0 | 0 |
| cancel | 0 | 0 | 0 | 0 | 0 | 0 | 48 | 0 |
| none | 0 | 0 | 0 | 0 | 0 | 1 | 2 | 45 |

### what was selected

| model | prompt | selected by out-of-fold loss | won | lost | McNemar p | intent head parts | direct weight |
|---|---|---|---|---|---|---|---|
| qwen3.5-2b | base | head on last@16 + span@14 | 248 | 5 | 0.0000 | [[16, 'last'], [14, 'span']] |  |
| qwen3.5-2b | rules_kw_fewshot_echo | head on last@14 + span@15 | 142 | 7 | 0.0000 | [[14, 'last'], [15, 'span']] |  |
| qwen3.5-4b | base | head on last@20 + span@18 | 74 | 8 | 0.0000 | [[20, 'last'], [18, 'span']] |  |
| qwen3.5-4b | rules_kw_fewshot_echo | head on last@19 + span@17 | 76 | 5 | 0.0000 | [[19, 'last'], [17, 'span']] |  |
| qwen3.5-9b | base | head on last + span, layer 21 | 41 | 2 | 0.0000 | [[21, 'last'], [21, 'span']] |  |
| qwen3.5-9b | rules_kw_fewshot_echo | head on last@20 + span@18 | 31 | 6 | 0.0000 | [[20, 'last'], [18, 'span']] |  |

### every candidate


qwen3.5-2b base

| method | out-of-fold loss | accuracy | auto share | auto accuracy | won | lost | p |
|---|---|---|---|---|---|---|---|
| direct |  | 49.5 | 8.0 | 87.5 |  |  |  |
| direct + temperature |  | 49.5 | 1.5 | 88.9 | 0 | 0 | 1.000 |
| direct + batch calibration |  | 53.0 | 9.3 | 87.5 | 52 | 31 | 0.028 |
| logit head (8 inputs) |  | 61.7 | 15.7 | 90.4 | 112 | 39 | 0.000 |
| hidden head, last, layer 16 | 0.422 | 87.0 | 66.8 | 96.5 | 235 | 10 | 0.000 |
| hidden head, span, layer 14 | 0.376 | 90.3 | 69.0 | 98.8 | 254 | 9 | 0.000 |
| hidden head, last, final layer 23 | 0.503 | 84.7 | 59.3 | 95.2 | 225 | 14 | 0.000 |
| head on last + span, layer 14 | 0.361 | 90.2 | 73.8 | 97.5 | 249 | 5 | 0.000 |
| head on last@16 + span@14 | 0.336 | 90.0 | 73.7 | 97.5 | 248 | 5 | 0.000 |
| head on layer 14 + final layer | 0.344 | 90.0 | 71.3 | 97.4 | 249 | 6 | 0.000 |
| head + direct (weight 0.9) | 0.372 | 90.3 | 67.2 | 99.0 | 253 | 8 | 0.000 |
| SELECTED by out-of-fold loss: head on last@16 + span@14 | 0.336 | 90.0 | 73.7 | 97.5 | 248 | 5 | 0.000 |

qwen3.5-2b rules_kw_fewshot_echo

| method | out-of-fold loss | accuracy | auto share | auto accuracy | won | lost | p |
|---|---|---|---|---|---|---|---|
| direct |  | 70.7 | 22.7 | 94.9 |  |  |  |
| direct + temperature |  | 70.7 | 20.0 | 97.5 | 0 | 0 | 1.000 |
| direct + batch calibration |  | 70.2 | 16.7 | 90.0 | 31 | 34 | 0.804 |
| logit head (8 inputs) |  | 75.5 | 33.8 | 94.6 | 59 | 30 | 0.003 |
| hidden head, last, layer 14 | 0.299 | 91.5 | 76.0 | 97.6 | 141 | 16 | 0.000 |
| hidden head, span, layer 15 | 0.309 | 92.7 | 77.0 | 99.8 | 146 | 14 | 0.000 |
| hidden head, last, final layer 23 | 0.384 | 86.5 | 70.3 | 95.3 | 108 | 13 | 0.000 |
| head on last + span, layer 14 | 0.260 | 93.0 | 80.7 | 98.8 | 143 | 9 | 0.000 |
| head on last@14 + span@15 | 0.254 | 93.2 | 81.2 | 98.8 | 142 | 7 | 0.000 |
| head on layer 14 + final layer | 0.294 | 89.3 | 76.5 | 96.5 | 122 | 10 | 0.000 |
| head + direct (weight 0.9) | 0.294 | 91.2 | 73.3 | 98.0 | 138 | 15 | 0.000 |
| SELECTED by out-of-fold loss: head on last@14 + span@15 | 0.254 | 93.2 | 81.2 | 98.8 | 142 | 7 | 0.000 |

qwen3.5-4b base

| method | out-of-fold loss | accuracy | auto share | auto accuracy | won | lost | p |
|---|---|---|---|---|---|---|---|
| direct |  | 82.7 | 50.3 | 98.0 |  |  |  |
| direct + temperature |  | 82.7 | 44.3 | 98.5 | 0 | 0 | 1.000 |
| direct + batch calibration |  | 83.7 | 50.5 | 96.7 | 33 | 27 | 0.519 |
| logit head (8 inputs) |  | 85.8 | 51.8 | 96.5 | 36 | 17 | 0.013 |
| hidden head, last, layer 20 | 0.255 | 91.8 | 82.8 | 97.8 | 67 | 12 | 0.000 |
| hidden head, span, layer 18 | 0.288 | 93.0 | 79.5 | 97.7 | 83 | 21 | 0.000 |
| hidden head, last, final layer 31 | 0.296 | 92.5 | 81.8 | 97.8 | 66 | 7 | 0.000 |
| head on last + span, layer 20 | 0.243 | 93.8 | 84.7 | 98.6 | 74 | 7 | 0.000 |
| head on last@20 + span@18 | 0.240 | 93.7 | 84.5 | 98.8 | 74 | 8 | 0.000 |
| head on layer 20 + final layer | 0.245 | 92.8 | 83.3 | 98.2 | 68 | 7 | 0.000 |
| head + direct (weight 1.0) | 0.255 | 91.8 | 82.8 | 97.8 | 67 | 12 | 0.000 |
| SELECTED by out-of-fold loss: head on last@20 + span@18 | 0.240 | 93.7 | 84.5 | 98.8 | 74 | 8 | 0.000 |

qwen3.5-4b rules_kw_fewshot_echo

| method | out-of-fold loss | accuracy | auto share | auto accuracy | won | lost | p |
|---|---|---|---|---|---|---|---|
| direct |  | 83.7 | 53.2 | 97.2 |  |  |  |
| direct + temperature |  | 83.7 | 57.0 | 96.8 | 0 | 0 | 1.000 |
| direct + batch calibration |  | 86.3 | 57.7 | 96.0 | 38 | 22 | 0.052 |
| logit head (8 inputs) |  | 89.8 | 69.8 | 96.7 | 42 | 5 | 0.000 |
| hidden head, last, layer 19 | 0.206 | 93.2 | 82.7 | 98.6 | 70 | 13 | 0.000 |
| hidden head, span, layer 17 | 0.223 | 95.3 | 84.7 | 99.0 | 79 | 9 | 0.000 |
| hidden head, last, final layer 31 | 0.235 | 93.0 | 85.5 | 97.7 | 62 | 6 | 0.000 |
| head on last + span, layer 19 | 0.183 | 94.8 | 85.8 | 98.6 | 73 | 6 | 0.000 |
| head on last@19 + span@17 | 0.180 | 95.5 | 86.5 | 98.5 | 76 | 5 | 0.000 |
| head on layer 19 + final layer | 0.210 | 93.5 | 86.7 | 96.5 | 62 | 3 | 0.000 |
| head + direct (weight 0.9) | 0.202 | 93.7 | 82.5 | 98.4 | 69 | 9 | 0.000 |
| SELECTED by out-of-fold loss: head on last@19 + span@17 | 0.180 | 95.5 | 86.5 | 98.5 | 76 | 5 | 0.000 |

qwen3.5-9b base

| method | out-of-fold loss | accuracy | auto share | auto accuracy | won | lost | p |
|---|---|---|---|---|---|---|---|
| direct |  | 89.2 | 53.2 | 98.4 |  |  |  |
| direct + temperature |  | 89.2 | 62.3 | 98.1 | 0 | 0 | 1.000 |
| direct + batch calibration |  | 88.8 | 61.5 | 98.1 | 10 | 12 | 0.832 |
| logit head (8 inputs) |  | 88.5 | 70.5 | 98.3 | 7 | 11 | 0.481 |
| hidden head, last, layer 21 | 0.194 | 94.7 | 89.0 | 99.1 | 39 | 6 | 0.000 |
| hidden head, span, layer 17 | 0.267 | 94.8 | 82.0 | 98.8 | 53 | 19 | 0.000 |
| hidden head, last, final layer 31 | 0.222 | 95.0 | 91.5 | 98.4 | 40 | 5 | 0.000 |
| head on last + span, layer 21 | 0.183 | 95.7 | 90.5 | 98.9 | 41 | 2 | 0.000 |
| head on last@21 + span@17 | 0.184 | 95.8 | 89.8 | 99.1 | 44 | 4 | 0.000 |
| head on layer 21 + final layer | 0.201 | 95.3 | 90.8 | 98.9 | 40 | 3 | 0.000 |
| head + direct (weight 1.0) | 0.194 | 94.7 | 89.0 | 99.1 | 39 | 6 | 0.000 |
| SELECTED by out-of-fold loss: head on last + span, layer 21 | 0.183 | 95.7 | 90.5 | 98.9 | 41 | 2 | 0.000 |

qwen3.5-9b rules_kw_fewshot_echo

| method | out-of-fold loss | accuracy | auto share | auto accuracy | won | lost | p |
|---|---|---|---|---|---|---|---|
| direct |  | 92.5 | 72.8 | 98.6 |  |  |  |
| direct + temperature |  | 92.5 | 73.7 | 98.6 | 0 | 0 | 1.000 |
| direct + batch calibration |  | 91.8 | 72.3 | 98.2 | 3 | 7 | 0.344 |
| logit head (8 inputs) |  | 92.5 | 79.8 | 99.2 | 14 | 14 | 1.000 |
| hidden head, last, layer 20 | 0.155 | 95.3 | 88.8 | 98.9 | 26 | 9 | 0.006 |
| hidden head, span, layer 18 | 0.141 | 96.3 | 89.5 | 99.4 | 31 | 8 | 0.000 |
| hidden head, last, final layer 31 | 0.172 | 96.2 | 91.5 | 98.5 | 26 | 4 | 0.000 |
| head on last + span, layer 18 | 0.129 | 96.5 | 89.3 | 99.4 | 35 | 11 | 0.001 |
| head on last@20 + span@18 | 0.122 | 96.7 | 90.3 | 99.3 | 31 | 6 | 0.000 |
| head on layer 18 + final layer | 0.151 | 96.5 | 91.7 | 99.1 | 25 | 1 | 0.000 |
| head + direct (weight 0.7) | 0.124 | 96.5 | 90.8 | 99.4 | 29 | 5 | 0.000 |
| SELECTED by out-of-fold loss: head on last@20 + span@18 | 0.122 | 96.7 | 90.3 | 99.3 | 31 | 6 | 0.000 |

### prompt effect on the direct readout (test), exact McNemar against the first prompt

| model | first prompt | built prompt | won | lost | p |
|---|---|---|---|---|---|
| qwen3.5-2b | 49.5 | 70.7 | 186 | 59 | 0.0000 |
| qwen3.5-4b | 82.7 | 83.7 | 57 | 51 | 0.6306 |
| qwen3.5-9b | 89.2 | 92.5 | 34 | 14 | 0.0055 |

### slot heads

| model | prompt | task | classes | train | test | layer | pool | majority | accuracy |
|---|---|---|---|---|---|---|---|---|---|
| qwen3.5-2b | base | rewrite.op | 7 | 340 | 144 | 14 | span | 19.4 | 80.6 |
| qwen3.5-2b | base | translate.target | 6 | 186 | 60 | 23 | last | 25.0 | 81.7 |
| qwen3.5-2b | rules_kw_fewshot_echo | rewrite.op | 7 | 340 | 144 | 15 | span | 19.4 | 80.6 |
| qwen3.5-2b | rules_kw_fewshot_echo | translate.target | 6 | 186 | 60 | 23 | last | 25.0 | 88.3 |
| qwen3.5-4b | base | rewrite.op | 7 | 340 | 144 | 18 | span | 19.4 | 81.2 |
| qwen3.5-4b | base | translate.target | 6 | 186 | 60 | 31 | last | 25.0 | 78.3 |
| qwen3.5-4b | rules_kw_fewshot_echo | rewrite.op | 7 | 340 | 144 | 19 | span | 19.4 | 81.9 |
| qwen3.5-4b | rules_kw_fewshot_echo | translate.target | 6 | 186 | 60 | 31 | last | 25.0 | 78.3 |
| qwen3.5-9b | base | rewrite.op | 7 | 340 | 144 | 21 | last | 19.4 | 77.8 |
| qwen3.5-9b | base | translate.target | 6 | 186 | 60 | 31 | last | 25.0 | 76.7 |
| qwen3.5-9b | rules_kw_fewshot_echo | rewrite.op | 7 | 340 | 144 | 31 | last | 19.4 | 73.6 |
| qwen3.5-9b | rules_kw_fewshot_echo | translate.target | 6 | 186 | 60 | 31 | last | 25.0 | 86.7 |

### learning curve (first prompt)

| training cases | qwen3.5-2b | qwen3.5-4b | qwen3.5-9b |
|---|---|---|---|
| 50 | 47.0 | 74.5 | 85.7 |
| 100 | 69.8 | 85.0 | 90.3 |
| 200 | 76.2 | 87.8 | 92.2 |
| 400 | 81.7 | 89.2 | 93.2 |
| 800 | 87.0 | 92.0 | 95.2 |
| 1679 | 90.2 | 93.8 | 95.7 |

### best layer by pooling (first prompt, test accuracy)

| model | layers | best answer-position layer | accuracy | best instruction-mean layer | accuracy | final layer, answer position |
|---|---|---|---|---|---|---|
| qwen3.5-2b | 24 | 14 | 87.5 | 9 | 90.5 | 84.7 |
| qwen3.5-4b | 32 | 23 | 93.8 | 17 | 94.0 | 92.5 |
| qwen3.5-9b | 32 | 26 | 96.2 | 15 | 94.8 | 95.0 |

### production path (`intent_decide`, the 600 test cases, shipped heads)

| model | prompt tokens | p50 ms | p95 ms | max prob. difference | same top answer | accuracy | auto share | auto accuracy | auto wrong |
|---|---|---|---|---|---|---|---|---|---|
| qwen3.5-2b | 1383 | 36 | 41 | 0.0042 | 100.0 | 93.2 | 81.2 | 98.8 | 6 |
| qwen3.5-4b | 1383 | 75 | 88 | 0.0000 | 100.0 | 95.5 | 86.5 | 98.5 | 8 |
| qwen3.5-9b | 1383 | 120 | 135 | 0.0000 | 100.0 | 96.7 | 90.3 | 99.3 | 4 |
