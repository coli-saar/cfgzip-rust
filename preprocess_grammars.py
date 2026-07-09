"""Offline preprocessing: compute token equivalence classes for paper grammars.

Runs cfgzip's :func:`preprocess` over a (model tokenizer, grammar) pair and
saves the resulting :class:`EquivalenceClassData` to disk, ready for
``estimate_latency.py`` (or any :class:`XgrammarProcessor`) to load.

Ported from tests/preproc.py (research repo): imports fixed for the standalone
``grammars`` package; the hardcoded ``parse_args(...)`` override and
``/home/mj/Downloads`` paths removed (real CLI now); re-ported to the current
API (``preprocess(...).save(path)`` — the ``filepath=`` arg was removed in P3).

Gated tokenizers (Llama) read the HuggingFace access token from ``HF_TOKEN``
in the environment (or ``--hf-token``).

Example:
    python preprocess_grammars.py --model gpt --task xml --output /path/to/gpt_xml
    HF_TOKEN=hf_... python preprocess_grammars.py --model llama --task json --output /path/to/llama_json
"""

import os

import time
import argparse
from tqdm import tqdm
from datasets import load_dataset
from transformers import AutoTokenizer

from grammars import json_schema, xml, cpp, bython
from cfgzip import preprocess


MODEL_CONFIGS = {
    "gpt": {
        "model_id": "openai/gpt-oss-20b",
        "ignore_range": (199998, 200018),
        "n_logits": 201088,
    },
    "llama": {
        "model_id": "meta-llama/Llama-3.2-3B-Instruct",
        "ignore_range": (128000, 128255),
        "n_logits": None,
    },
    "qwen": {
        "model_id": "Qwen/Qwen3-4B-Instruct-2507",
        "ignore_range": (151643, 151668),
        "n_logits": 151936,
    },
}
GRAMMAR_MODULES = {"cpp": cpp, "bython": bython, "xml": xml}
TASKS_WITH_SKIP_TOKENS = {"cpp", "bython"}


def parse_args(argv=None):
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--model", "-m", required=True, choices=list(MODEL_CONFIGS))
    p.add_argument("--task", "-t", required=True, choices=["json", "cpp", "bython", "xml"])
    p.add_argument("--output", "-o", required=True,
                   help="output dir (for json, used as <output>/schema_<i>)")
    p.add_argument("--hf-token", default=None, help="HuggingFace token (falls back to HF_TOKEN env var)")
    p.add_argument("--num-workers", type=int, default=16)
    return p.parse_args(argv)


def main(args):
    config = MODEL_CONFIGS[args.model]
    token = args.hf_token or os.environ.get("HF_TOKEN")
    tokenizer = AutoTokenizer.from_pretrained(config["model_id"], **({"token": token} if token else {}))
    tokenizer.clean_up_tokenization_spaces = False

    ignore_set = set(range(*config["ignore_range"]))

    def ignore_tokens(_, tok_id):
        return tok_id in ignore_set

    def skip_tokens(tok, _):
        for k in range(len(tok) - 3):
            tok_k3 = tok[k:(k + 3)]
            for byte_id in (42, 43, 45):
                if tok_k3 == (byte_id,) * 3:
                    return True
        return False

    preprocess_kwargs = {"ignore_tokens": ignore_tokens, "num_workers": args.num_workers}
    if config["n_logits"] is not None:
        preprocess_kwargs["n_logits"] = config["n_logits"]
    if args.task in TASKS_WITH_SKIP_TOKENS:
        preprocess_kwargs["skip_compute_tokens"] = skip_tokens

    t0 = time.perf_counter()
    if args.task == "json":
        # \x00-byte tokens get their own singleton class for json: xgrammar's runtime rejects any
        # token whose bytes contain \x00 as a special control token, but cfgzip's grammar accepts
        # \x00 via classes like [^/]. Without skipping, the null-byte token becomes the shortest
        # member of a large class and is chosen as its representative — which xgrammar then refuses
        # to consume at runtime, rejecting every member of the class.
        def skip_null_byte_tokens(tok, _):
            return 0 in tok

        preprocess_kwargs["skip_compute_tokens"] = skip_null_byte_tokens
        ds = load_dataset("NousResearch/json-mode-eval", trust_remote_code=True)["train"]
        for i, inst in enumerate(tqdm(ds)):
            if i in (39, 58):  # malformed schemas in the dataset
                continue
            grammar_spec = json_schema.json_schema_to_grammar_spec(inst["schema"])
            eq_data = preprocess(grammar_spec.xgrammar_str, tokenizer, **preprocess_kwargs)
            eq_data.save(f"{args.output}/schema_{i}")
    else:
        eq_data = preprocess(
            GRAMMAR_MODULES[args.task].xgrammar_str, tokenizer, use_tqdm=True, **preprocess_kwargs,
        )
        eq_data.save(args.output)

    print(f"\n\nTOTAL TIME: {time.perf_counter() - t0}")


if __name__ == "__main__":
    main(parse_args())
