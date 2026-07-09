
import re
import json
import xgrammar as xgr
from grammars.base import GrammarSpec
from grammars.utils import ebnf_to_nf, remove_lparen


# xgrammar emits `\0` in character classes (e.g. `[^\0-\x1f...]`); interegular's
# regex parser rejects `\0` (treats it as the start of an incomplete octal escape)
# but accepts `\x00`. Normalize at the boundary so the xgrammar_str is parseable
# by both xgrammar and cfgzip's `regex_to_gnf_cfg` (which uses interegular).
def _interegular_sanitize(s: str) -> str:
    return re.sub(r'\\0(?![0-7])', r'\\x00', s)


_preproc_str = r"""
root ::= T1 members_and_embrace | T2 elements_or_embrace
value_non_str ::= T32 members_and_embrace | T2 elements_or_embrace | T3 fraction exponent | T4 fraction exponent | T5 fraction exponent | T6 fraction exponent | T7 | T8 | T9
members_and_embrace ::= T10 characters_and_colon members_suffix | T12
members_suffix ::= value_non_str member_suffix_suffix | T10 characters_and_embrace | T10 characters_and_comma T13 characters_and_colon members_suffix
member_suffix_suffix ::= T33 | T14 characters_and_colon members_suffix
elements_or_embrace ::= T1 members_and_embrace elements_rest T15 | T2 elements_or_embrace elements_rest T15 | T10 characters_item elements_rest T15 | T3 fraction exponent elements_rest T15 | T4 fraction exponent elements_rest T15 | T16 fraction exponent elements_rest T15 | T6 fraction exponent elements_rest T15 | T7 elements_rest T15 | T8 elements_rest T15 | T9 elements_rest T15 | T17
elements ::= T1 members_and_embrace elements_rest | T2 elements_or_embrace elements_rest | T10 characters_item elements_rest | T3 fraction exponent elements_rest | T4 fraction exponent elements_rest | T5 fraction exponent elements_rest | T6 fraction exponent elements_rest | T7 elements_rest | T8 elements_rest | T9 elements_rest
elements_rest ::= "" | T18 elements
characters_and_colon ::= T19 | T20 characters_and_colon | T21 escape characters_and_colon
characters_and_comma ::= T22 | T20 characters_and_comma | T21 escape characters_and_comma
characters_and_embrace ::= T23 | T20 characters_and_embrace | T21 escape characters_and_embrace
characters_item ::= T10 | T20 characters_item | T21 escape characters_item
escape ::= T24 | T25
fraction ::= "" | T26
exponent ::= "" | T27 sign T28 | T29 sign T28
sign ::= "" | T30 | T31
TERMINALS:
T1 ::= \{[ \n\t]*
T2 ::= \[[ \n\t]*
T3 ::= 0
T4 ::= [1-9][0-9]*
T5 ::= \-[0-9]
T6 ::= \-[1-9][0-9]*
T7 ::= true
T8 ::= false
T9 ::= null
T10 ::= "
T12 ::= \}
T13 ::= [ \n\t]*"
T14 ::= [ \n\t]*,[ \n\t]*"
T15 ::= [ \n\t]*\]
T16 ::= \-0
T17 ::= \]
T18 ::= [ \n\t]*,[ \n\t]*
T19 ::= "[ \n\t]*:
T20 ::= [^"\\\0-\x1f]
T21 ::= \\\\
T22 ::= "[ \n\t]*,
T23 ::= "[ \n\t]*\}
T24 ::= ["\\/bfnrt]
T25 ::= u[A-Fa-f0-9][A-Fa-f0-9][A-Fa-f0-9][A-Fa-f0-9]
T26 ::= \.[0-9][0-9]*
T27 ::= e
T28 ::= [0-9][0-9]*
T29 ::= E
T30 ::= \+
T31 ::= \-
T32 ::= [ \n\t]*\{[ \n\t]*
T33 ::= [ \n\t]*\}
"""


def json_schema_to_grammar_spec(*schema_string: str) -> GrammarSpec:
    tokenizer_info = xgr.TokenizerInfo([''], stop_token_ids=0)
    compiler = xgr.GrammarCompiler(tokenizer_info, max_threads=8)

    if len(schema_string) == 0:
        name = 'json'
        grammar = compiler.compile_builtin_json_grammar().grammar
        preproc_str = _preproc_str
    elif len(schema_string) == 1:
        name = f'json_schema_{json.loads(schema_string[0]).get("title", "UNK")}'
        grammar = compiler.compile_json_schema(schema_string[0]).grammar
        preproc_str = None
    else:
        name = 'json_schema_UNK'
        preproc_str = None
        compiled_grammar0 = compiler.compile_json_schema(schema_string[0])
        compiled_grammars = [compiler.compile_json_schema(schema).grammar for schema in schema_string[1:]]
        grammar = compiled_grammar0.grammar.union(*compiled_grammars)

    xgrammar_str = _interegular_sanitize(remove_lparen(str(grammar)))
    preproc_str = ebnf_to_nf(xgrammar_str) if preproc_str is None else preproc_str

    return GrammarSpec(
        name=name,
        xgrammar_str=xgrammar_str,
        preprocessing_str=preproc_str,
        llguidance_str='NONE'  # TODO
    )

