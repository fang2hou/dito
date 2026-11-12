"""Intent routing for voice commands: a direct option-logit readout plus small trained heads.

Shared: `data` (cases and the near-test filter), `prompts` (router prompt variants), `stats`,
`models` and `locations`. By role:

- `probe`: runs the model and keeps the run cache (local, git-ignored).
- `train`: fits heads and writes the `.heads` files the app bundles, and checks the app path.
- `experiments`: studies that ship nothing (prompt sweep, option order, efficiency, rounds,
  learning curve, errors).
- `report`: the tables and charts, read from the fit reports.

`pipeline` runs all of it in order.
"""
