"""Model weights: where they come from and a cheap identity used to detect changes.

The identity never hashes weight contents. It is the Hugging Face snapshot revision for
models loaded from the Hugging Face cache, and the file names and sizes for models that
live in a local directory.
"""

import hashlib
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True, slots=True)
class Weights:
    """A Hugging Face repository, optionally mirrored into `local_dir`.

    `include` limits both the download and the identity to matching files, for repositories
    that hold many quantizations.
    """

    repo: str
    local_dir: Path | None = None
    include: tuple[str, ...] = ()

    @property
    def ref(self) -> str:
        """What a runtime should load: the local directory if there is one, else the repo id."""
        return str(self.local_dir) if self.local_dir else self.repo

    def fetch(self) -> Path:
        """Download the weights (a no-op when already present) and return their location."""
        from huggingface_hub import snapshot_download

        return Path(
            snapshot_download(
                self.repo,
                local_dir=self.local_dir,
                allow_patterns=list(self.include) or None,
            )
        )

    def identity(self) -> str | None:
        """Identity of the weights on disk, or None when they are not downloaded yet."""
        if self.local_dir is not None:
            return self._local_identity(self.local_dir)
        return self._cache_identity()

    def _cache_identity(self) -> str | None:
        from huggingface_hub import snapshot_download
        from huggingface_hub.errors import LocalEntryNotFoundError

        try:
            snapshot = Path(snapshot_download(self.repo, local_files_only=True))
        except LocalEntryNotFoundError:
            return None
        return f"hf:{self.repo}@{snapshot.name}"

    def _local_identity(self, directory: Path) -> str | None:
        patterns = self.include or ("*",)
        files = sorted(
            {
                path
                for pattern in patterns
                for path in directory.rglob(pattern)
                if path.is_file() and ".cache" not in path.relative_to(directory).parts
            }
        )
        if not files:
            return None
        listing = "\n".join(f"{p.relative_to(directory)}:{p.stat().st_size}" for p in files)
        return f"files:{hashlib.sha256(listing.encode()).hexdigest()[:16]}"
