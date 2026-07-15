# CFGzip

**Lossless token vocabulary compression for fast CFG-constrained decoding.**

CFGzip precomputes token equivalence classes for a fixed context-free grammar
and tokenizer. At generation time, a grammar engine such as XGrammar2 can run on
the much smaller class vocabulary, while CFGzip expands the resulting mask back
to the model's full token vocabulary without changing which byte strings are
accepted.

The offline preprocessor is now the Rust CLI, `cfgzip-preprocess`. The older
Python preprocessor is deprecated and will be removed from the recommended
workflow.

## How It Works

Within a given grammar, many tokens are interchangeable: whenever one token is
valid, the others in its equivalence class are valid too. CFGzip computes these
classes once for a `(grammar, tokenizer)` pair and writes three artifacts:

- `tc.pt`: token id to class id
- `inv.pt`: token ids that are invalid in every grammar context
- `cr.pkl`: byte representatives for the compressed class vocabulary

At inference time, the grammar engine masks only the representative vocabulary
instead of all 100k-200k model tokens. `MaskTranslator` then losslessly expands
that class-level mask back to the full vocabulary.

See [our paper](https://arxiv.org/abs/2605.29986) for the compression algorithm
and correctness proof.

## When To Use It

CFGzip is best for **static, reused grammars**: programming languages, fixed
schemas, structured DSLs, XML-like formats, and other large grammars that serve
many requests. The precompute step is offline work, so it usually pays off when
the same grammar/tokenizer pair is reused across many generations.

CFGzip is usually not a good fit for highly dynamic per-request schemas, where
the precompute cost would be paid for every request.

## Install

Install the Python runtime package for loading artifacts and using the
generation-time processors:

```bash
pip install "cfgzip[xgrammar]"
```

Build the Rust preprocessor from this repository:

```bash
cargo build --release
```

Use the release binary for real preprocessing. The repository configures
aggressive release settings such as `target-cpu=native`, fat LTO, and one
codegen unit, so release builds take longer but are the intended timing target.

## Quickstart

CFGzip has two phases:

1. Run the Rust CLI once to produce equivalence-class artifacts.
2. Load those artifacts from Python during generation.

### 1. Write A Grammar File

Create `/tmp/arithmetic.gbnf`:

```gbnf
root   ::= expr
expr   ::= term (("+" | "-") term)*
term   ::= factor (("*" | "/") factor)*
factor ::= [0-9]+ | "(" expr ")"
```

### 2. Preprocess With Rust

```bash
target/release/cfgzip-preprocess \
  --model-id gpt2 \
  --grammar-file /tmp/arithmetic.gbnf \
  --output cfgzip_data/arithmetic \
  --num-threads 8
```

The output directory must not already contain files. Progress bars are shown for
token preprocessing, class bucketing, and output writing. Pass `--no-progress`
for quiet output.

For gated HuggingFace models, set `HF_TOKEN` or pass `--hf-token`:

```bash
export HF_TOKEN=hf_...
```

Useful options:

- `--model-id`: HuggingFace model id whose `tokenizer.json` should be used.
- `--grammar-file`: path to a GBNF grammar file.
- `--output`: empty or non-existent directory where artifacts will be written.
- `--start-symbol`: grammar start rule, default `root`.
- `--n-logits`: override the output logit vocabulary size.
- `--ignore-range START..END`: exclude token ids, useful for special tokens.
- `--skip-null-bytes`: put tokens containing `\x00` in singleton classes.
- `--skip-repeat-bytes`: put tokens containing a repeated decoded-byte run in
  singleton classes instead of processing them through the grammar. This is a
  conservative preprocessing/runtime tradeoff for low-entropy tokenizer tokens
  such as long runs of spaces.
- `--skip-repeat-min-run`: minimum repeated-byte run length for
  `--skip-repeat-bytes`, default `4`.
- `--num-threads`: Rayon worker count for token traversal.
- `--cache-dir`: HuggingFace asset cache directory.

### 3. Use The Artifacts During Generation

`XgrammarProcessor` is a standard `transformers` `LogitsProcessor`:

```python
from transformers import AutoModelForCausalLM, AutoTokenizer, LogitsProcessorList
from cfgzip import XgrammarProcessor

grammar = open("/tmp/arithmetic.gbnf").read()
tokenizer = AutoTokenizer.from_pretrained("gpt2")
model = AutoModelForCausalLM.from_pretrained("gpt2")

processor = XgrammarProcessor.auto_pipeline(
    "cfgzip_data/arithmetic",
    tokenizer,
    grammar,
    device=model.device,
)

inputs = tokenizer("Calculator: ", return_tensors="pt").to(model.device)
out = model.generate(
    **inputs,
    max_new_tokens=16,
    logits_processor=LogitsProcessorList([processor]),
)
print(tokenizer.decode(out[0]))
```

For the common case of one grammar reused across many batches, compile once and
rebuild only the lightweight per-batch processor:

```python
from cfgzip import XgrammarProcessor

mask_translator, compiled_grammar = XgrammarProcessor.load_and_compile(
    "cfgzip_data/arithmetic",
    tokenizer,
    grammar,
    device=model.device,
)

for batch in batches:
    processor = XgrammarProcessor.from_compiled(mask_translator, compiled_grammar, tokenizer)
    output = model.generate(
        **batch,
        logits_processor=LogitsProcessorList([processor]),
    )
```

`auto_pipeline` is just `load_and_compile` plus `from_compiled`; use the split
form whenever the same grammar is used more than once.

## Paper C++ Benchmark

To run the same C++ grammar used by the paper scripts, write it to a file:

```bash
python - <<'PY'
from grammars import cpp
with open("/tmp/cfgzip_cpp.gbnf", "w") as f:
    f.write(cpp.xgrammar_str)
PY
```

Then time the Rust preprocessor:

```bash
rm -rf /tmp/cfgzip_llama_cpp_rust

/usr/bin/time -p target/release/cfgzip-preprocess \
  --model-id meta-llama/Llama-3.2-3B-Instruct \
  --grammar-file /tmp/cfgzip_cpp.gbnf \
  --output /tmp/cfgzip_llama_cpp_rust \
  --ignore-range 128000..128255 \
  --skip-repeat-bytes \
  --num-threads 16
```

## Runtime And Class Counts

The following measurements use the
`meta-llama/Llama-3.2-3B-Instruct` tokenizer. Its logit vocabulary has
**128,256 token ids**. The reported class count is the number of compressed
CFGzip token classes produced by the Rust preprocessor.

Experiment setup:

- Hardware: MacBook Pro, Apple M4 Pro, 14 cores (10 performance, 4 efficiency),
  24 GB memory.
- OS: macOS 15.6.1.
- Build: `cargo build --release`.
- Threads: measured with both `--num-threads 16` and `--num-threads 1`.
- Repeated-token policy: `--skip-repeat-bytes --skip-repeat-min-run 4`.
- Timing: `/usr/bin/time -p`; the table reports `real` wall-clock seconds.
- C++ run also uses `--ignore-range 128000..128255`, matching the paper
  benchmark command above.

| Grammar  | 16 threads | Single CPU core | Classes |
|---|---:|---:|---:|
| C++  | 1.42s | 2.53s | 3,830 |
| bython  | 0.87s | 1.59s | 2,498 |
| XML  | 0.69s | 1.06s | 2,262 |

The `--skip-repeat-bytes` option keeps complicated repeated-byte tokens as
singleton classes. This slightly increases the compressed class vocabulary, but
it avoids expensive exact preprocessing for rare tokenizer tokens such as long
space runs.

## Grammar Format

Grammars are written in **GBNF** (GGML BNF), the grammar notation used by
[llama.cpp](https://github.com/ggml-org/llama.cpp/blob/master/grammars/README.md)
and XGrammar2.

CFGzip currently parses core GBNF. Some advanced constructs, such as `{m,n}`
repetition counts, are not supported yet.

The start rule should be named `root`, or pass `--start-symbol`. The start
symbol must not appear in any rule body; keep recursion on an inner
non-terminal, as in `root ::= expr`.

The same grammar string should be supplied to both the Rust preprocessor and
the generation-time grammar engine.

## Deprecated Python Preprocessor

The Python `cfgzip.preprocessing.preprocess` API remains available for
compatibility, but it is no longer the recommended offline path. New workflows
should call `cfgzip-preprocess` and load the generated artifacts with
`EquivalenceClassData.load()` or `XgrammarProcessor`.

The Rust preprocessor may produce fewer classes than the Python implementation
when `regex-automata` merges terminal-regex states that are language-equivalent;
the artifact format and accepted byte strings remain compatible with the Python
runtime.

## Scope And Limitations

- **Engine backend:** v0.1.0 supports XGrammar2 only. `BaseProcessor` defines
  the extension point for adding engines such as llguidance or
  transformers-cfg.
- **Grammar notation:** v0.1.0 supports core GBNF. Support for additional
  grammar formats is planned alongside additional decoding engines.
- **Offline cost:** preprocessing can still take minutes for large grammars and
  tokenizers. This is expected; the artifacts are intended to be reused.

## Public API

| Name | Description |
|---|---|
| `cfgzip-preprocess` | Rust CLI that computes equivalence-class artifacts for a `(grammar, tokenizer)` pair. |
| `EquivalenceClassData` | Loads and stores precomputed artifact data; `.load`, `.save`, `.to`. |
| `XgrammarProcessor` | XGrammar wrapper and `LogitsProcessor`; `.auto_pipeline`, `.load_and_compile`, `.from_compiled`. |
| `MaskTranslator` | Expands a class-level mask back to the full token vocabulary in-place. |
| `BaseProcessor` | Abstract base and extension point for new grammar engines. |
| `preprocess` | Deprecated Python offline preprocessor retained for compatibility. |

## On AI Usage

The main algorithm and functions were written by hand. We used Claude Code to
port our research repository to a pip-installable module, including writing
tests, docstrings, and portions of this README.

## Citation

If you use CFGzip in your research, please cite:

```bibtex
@article{sullivan2026accelerating,
  title   = {Accelerating Constrained Decoding with Token Space Compression},
  author  = {Sullivan, Michael and Koller, Alexander},
  journal = {arXiv preprint arXiv:2605.29986},
  year    = {2026},
  url     = {https://arxiv.org/abs/2605.29986}
}
```
