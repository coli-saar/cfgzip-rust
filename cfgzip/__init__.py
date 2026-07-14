
__version__ = "0.1.0"

from cfgzip.preprocessing import preprocess
from cfgzip.utils import EquivalenceClassData


def __getattr__(name):
    if name in {"MaskTranslator", "BaseProcessor", "XgrammarProcessor"}:
        from cfgzip.generation import MaskTranslator, BaseProcessor, XgrammarProcessor

        exports = {
            "MaskTranslator": MaskTranslator,
            "BaseProcessor": BaseProcessor,
            "XgrammarProcessor": XgrammarProcessor,
        }
        return exports[name]
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
