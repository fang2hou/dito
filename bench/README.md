# dito-bench

Local benchmark harness for the models Dito can run: speech recognition (ASR) and text polishing. It runs on Apple Silicon and compares accuracy and speed on the same inputs.

Results are stored per model, so adding a model or changing one input only runs what is new. Earlier measurements and conclusions are in [FINDINGS.md](FINDINGS.md).

## Quick start

All commands run from the repository root through [mise](https://mise.jdx.dev). `mise install` provides Python, uv and ruff at the pinned versions.

```bash
mise run bench:sync                      # install the Python environment (first time, and after dependency changes)
mise run bench clips                     # synthesize the test clips with macOS `say`
mise run bench asr list                  # known models, whether weights are downloaded, recorded results
mise run bench asr run                   # run missing or stale cases for the default model set
mise run bench asr report                # print stored results; runs no model
```

Anything after `mise run bench` is passed to `dito-bench`. The first run of a model downloads its weights from Hugging Face (about 6.5 GB for the default ASR set).

## How results are stored

```
results/<task>/<chip>/<model>.json      task: asr or polish; chip: for example apple-m4-pro
```

- Each file holds one record per case, plus the weights identity, chip, memory, macOS, Python and package versions. It never contains a host or user name.
- A record is fresh when its fingerprint matches. For ASR the fingerprint covers the clip audio hash and the hotwords. For polish it covers the exact system prompt, the input text and the token limit.
- `run` executes only missing or stale cases. If the weights on disk changed (a new Hugging Face revision, or different file sizes for local models), all records of that model are dropped. `--refresh` reruns everything.
- Error rates are computed at report time from the stored text and the clip definitions, so fixing a reference text needs no rerun.
- Results are kept per chip because speed is hardware specific. `--chip "Apple M4 Pro"` reads another machine's results.
- Only synthetic inputs are stored, and the result files are meant to be committed. The commit hooks accept a result file up to 8 MB; store results, not intermediate files. Microphone recordings stay in the git-ignored `recordings/`.

## Commands

```bash
mise run bench asr run -m qwen3-asr-1.7b            # one model
mise run bench asr run --refresh                    # rerun every case
mise run bench asr report --markdown                # Markdown tables, for docs
mise run bench asr compare recordings/<file>.wav    # transcribe any file with several models (not stored)
mise run bench asr download r2t2                    # fetch weights without running
mise run bench polish list | run | report           # same workflow for polish models
mise run bench intent run -v base -s train test --layers all   # voice command intent routing (see INTENT.md): probe runs into cache/intent/
mise run bench intent fit | export | parity         # fit heads on the runs, write the .heads files into crates/dito-core/src/intent/heads/, check the app path against numpy
mise run bench intent decide -m qwen3.5-4b "text"   # one decision through the app path (intent_decide)
mise run bench intent summary | confusion | oof | efficiency | sweep | order | errors | charts   # tables and charts, no model needed
mise run bench intent all                           # regenerate every intent artifact in results/intent/<chip>/
mise run bench record --seconds 5 --note "test"     # record into recordings/
mise run bench:notebook                             # JupyterLab on notebooks/
```

## Adding a model

1. ASR: write an adapter in `src/dito_bench/asr/` that implements `AsrModel` (see `asr/base.py`), or reuse one. Add an `AsrSpec` to `ASR_MODELS` in `asr/registry.py` with the weights source and the runtime packages.
2. Polish: add a `PolishSpec` to `POLISH_MODELS` in `polish/registry.py`. A GGUF or MLX model needs no new adapter.
3. Run `mise run bench asr run -m <key>`. Existing models are not rerun.

Add a runtime's Python package to the matching extra in `pyproject.toml` (`sherpa`, `mlx`, `llama`), then run `mise run bench:sync`.

Packages come from `https://pypi.flatt.tech/simple/`, which `uv.toml` sets as the default index and `uv.lock` records. This index is required: keep it when adding dependencies or regenerating the lock file.

## Test data

- `data/clips.json`: clips and their reference text. Clips with `hotwords` also run as a second case with those hotwords for models that support them.
- `data/polish_cases.json`: dirty dictation text for the polish models. Optional `wordbook` terms are passed as the reference vocabulary.
- `data/intent/test.json`: the frozen intent test set, 600 spoken command instructions (200 per language) with gold intents and an instruction card. The cards give the rewrite and translate arguments.
- `data/intent/train/*.json`: the intent training set; cases that repeat or nearly repeat a test case are dropped at load time.
- `data/intent/GUIDE.md`: the one guide for both intent sets: intents, rules, fields, counts and quality bar.
- The prompt in `src/dito_bench/polish/prompts.py` mirrors `crates/dito-core/src/prompts.rs`: the scaffold, the demonstration turns and the default rules of each language. Keep them identical; a change makes stored polish results stale.
- Clip texts, prompts and polish inputs are Japanese, Chinese and English on purpose. They are test data, not documentation.

## Development

```bash
mise run bench:test      # unit tests
mise run bench:lint      # ruff check, ruff format --check, ty check
mise run bench:format    # ruff autofix and format
```

Layout:

```
src/dito_bench/   package: cli, store, sync, clips, metrics, asr/, polish/, intent/
data/             clip definitions, polish cases (committed)
data/intent/      intent test.json, train/ and GUIDE.md (committed)
results/          stored benchmark results, intent fit reports and charts included (committed)
notebooks/        interactive playgrounds
tests/            unit tests for the store, metrics, prompts, weights identity and the intent package
audio/            synthesized clips (git-ignored)
models/           downloaded weights (git-ignored)
recordings/       microphone recordings (git-ignored)
cache/            intent probe runs: option logits, timings and hidden states (git-ignored, local only)
```

The intent package is split by role (details in [INTENT.md](INTENT.md)):

```
src/dito_bench/intent/
  data.py         cases, loading, validation, the near-test filter
  prompts.py      router prompt variants
  stats.py        probabilities, metrics, McNemar
  models.py       model keys and where their GGUF files are
  locations.py    chip, cache folder, results folder
  probe/          runner (the intent_probe example) and the run cache
  train/          heads, fit, fit report, export, the .heads format, parity with the app path
  experiments/    prompt sweep, option order, efficiency, rounds, learning curve, errors
  report/         tables, summary, confusion, out-of-fold errors, charts
  pipeline.py     `intent all`
```

Only `train/` produces something the app ships. The rest reads runs and fit reports and writes tables.

### Intent heads

The heads under `crates/dito-core/src/intent/heads/` are committed build inputs: a build of the app never refits, and the bench is not needed to build it. To refit (new training data, a new router prompt, another GGUF file):

```bash
mise run bench intent all      # probes what is missing, fits, exports the heads, checks parity, rewrites results/intent/<chip>/
mise run bench intent fit      # only the fit reports, from the stored probe runs
mise run bench intent export   # only the .heads files, from the stored probe runs
cargo test -p dito-local --lib intent   # the heads match the catalog files and the router prompt
```

Commit the changed `.heads` files and `results/intent/<chip>/` together. `cache/intent/` is local only: it holds several GB of probe runs and is rebuilt from the models in Dito's model folder when missing. A run is valid while its stored requests equal the current ones, which come from the case texts and the prompt, so moving a data file does not invalidate the cache.

## Polish prompt exploration

`notebooks/prompt_escape.py` measures how a prompt construction holds up when the dictated text looks like a request: an instruction to translate, a question, a correction, a sentence with words of another language. It runs the cases in `data/prompt_escape_cases.json` on the 2B, 4B and 9B polish models, rendered and sampled as the app does. See [POLISH_PROMPT.md](POLISH_PROMPT.md) for the findings and the construction Dito ships.

## Haas-ja exploration

`haas-ja` is an opt-in Japanese-only model using a community mixed INT4/INT8 ONNX conversion through the existing sherpa-onnx CPU runtime. Run `mise run bench asr run -m haas-ja`. See [HAAS_JA.md](HAAS_JA.md) for M5 Pro CPU and MLX Metal measurements, quantized formats, limitations and reproduction commands.
