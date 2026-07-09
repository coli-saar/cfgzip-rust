"""Paper grammar fixtures for the cfgzip experiments.

`cpp`, `xml`, and `bython` are ready-made :class:`GrammarSpec` instances. `json`
schemas are dynamic (one grammar per schema in the dataset), so `json_schema` is
exposed as a *module* — call ``json_schema.json_schema_to_grammar_spec(schema)``
to build a spec at runtime.
"""

from grammars import json_schema
from grammars.cpp import grammar_spec as cpp
from grammars.xml_cfg import grammar_spec as xml
from grammars.bython import grammar_spec as bython

__all__ = ["cpp", "xml", "bython", "json_schema"]
