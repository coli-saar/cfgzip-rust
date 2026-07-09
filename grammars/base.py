
from dataclasses import dataclass


@dataclass
class GrammarSpec:
    name: str
    xgrammar_str: str
    llguidance_str: str
    preprocessing_str: str
