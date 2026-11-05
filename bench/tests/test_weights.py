from pathlib import Path

from dito_bench.weights import Weights


def test_local_identity_is_none_until_files_exist(tmp_path: Path) -> None:
    weights = Weights("example/model", tmp_path / "w", ("*.gguf",))
    assert weights.identity() is None


def test_local_identity_follows_included_files_only(tmp_path: Path) -> None:
    directory = tmp_path / "w"
    directory.mkdir()
    (directory / "model-Q4.gguf").write_bytes(b"1234")
    weights = Weights("example/model", directory, ("*Q4*",))
    first = weights.identity()
    assert first is not None

    (directory / "other-Q8.gguf").write_bytes(b"ignored")
    assert weights.identity() == first

    (directory / "model-Q4.gguf").write_bytes(b"12345")
    assert weights.identity() != first
