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
- Only synthetic inputs are stored, and the result files are meant to be committed. Microphone recordings stay in the git-ignored `recordings/`.

## Commands

```bash
mise run bench asr run -m qwen3-asr-1.7b            # one model
mise run bench asr run --refresh                    # rerun every case
mise run bench asr report --markdown                # Markdown tables, for docs
mise run bench asr compare recordings/<file>.wav    # transcribe any file with several models (not stored)
mise run bench asr download r2t2                    # fetch weights without running
mise run bench polish list | run | report           # same workflow for polish models
mise run bench record --seconds 5 --note "test"     # record into recordings/
mise run bench:notebook                             # JupyterLab on notebooks/
```

## Adding a model

1. ASR: write an adapter in `src/dito_bench/asr/` that implements `AsrModel` (see `asr/base.py`), or reuse one. Add an `AsrSpec` to `ASR_MODELS` in `asr/registry.py` with the weights source and the runtime packages.
2. Polish: add a `PolishSpec` to `POLISH_MODELS` in `polish/registry.py`. A GGUF or MLX model needs no new adapter.
3. Run `mise run bench asr run -m <key>`. Existing models are not rerun.

Add a runtime's Python package to the matching extra in `pyproject.toml` (`sherpa`, `mlx`, `llama`), then run `mise run bench:sync`.

## Test data

- `data/clips.json`: clips and their reference text. Clips with `hotwords` also run as a second case with those hotwords for models that support them.
- `data/polish_cases.json`: dirty dictation text for the polish models. Optional `dictionary` terms are passed as the reference vocabulary.
- The prompts in `src/dito_bench/polish/prompts.py` mirror `crates/dito-core/src/prompts.rs`. Keep them identical; a change makes stored polish results stale.
- Clip texts, prompts and polish inputs are Japanese, Chinese and English on purpose. They are test data, not documentation.

## Development

```bash
mise run bench:test      # unit tests
mise run bench:lint      # ruff check, ruff format --check, ty check
mise run bench:format    # ruff autofix and format
```

Layout:

```
src/dito_bench/   package: cli, store, sync, clips, metrics, asr/, polish/
data/             clip definitions and polish cases (committed)
results/          stored benchmark results (committed)
notebooks/        interactive playgrounds
tests/            unit tests for the store, metrics, prompts and weights identity
audio/            synthesized clips (git-ignored)
models/           downloaded weights (git-ignored)
recordings/       microphone recordings (git-ignored)
```
