import numpy as np

from dito_bench.intent.data import INTENTS, load_cases
from dito_bench.intent.report.confusion import boundary_rows, confusion_matrix


def test_boundary_rows_count_the_cases_read_as_the_neighbour() -> None:
    cases = [c for c in load_cases() if c.lang == "en"]
    p = np.zeros((len(cases), len(INTENTS)))
    p[np.arange(len(cases)), [INTENTS.index(c.intent) for c in cases]] = 1
    replies = [i for i, c in enumerate(cases) if c.intent == "reply"]
    p[replies[0]] = np.eye(len(INTENTS))[INTENTS.index("rewrite")]
    rows = boundary_rows(cases, p)
    assert rows[0][2] == f"1 of {len(replies)}"
    assert str(rows[1][2]).startswith("0 of")
    assert int(confusion_matrix(cases, p)[INTENTS.index("reply"), INTENTS.index("rewrite")]) == 1
