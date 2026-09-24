# Official examples

Documents the Examples panel (toolbar → Examples, next to Assay) offers. Each
entry in `manifest.json` names a `.waffle` document and, when the document was
generated, the generator that produced it. Opening an example loads a COPY
under a fresh document id, so saving it never touches the shipped file.

| Example | Document | Generator | Parts | Bodies |
|---|---|---|---|---|
| Gravel bike v2 | `gravel-bike-v2.waffle` | `gravel-bike-v2.py` | 11 | ~200 |
| Eiffel Tower | `eiffel-tower.waffle` | `eiffel-tower.py` | 10 | 1,964 |

## Regenerating an example

```
python3 app/static/examples/gravel-bike-v2.py --build app/static/examples/gravel-bike-v2.waffle
python3 app/static/examples/eiffel-tower.py   --build app/static/examples/eiffel-tower.waffle
```

The generator drives `target/release/waffle-host` (`cargo build -p waffle-host
--release`) over its stdio frames with the same agent-tool calls an MCP agent
would make, then copies the autosaved document out. `python3
gravel-bike-v2.py recipe.json` emits those calls instead, for an agent to
replay over the link.

The tower takes ~85 s to build, which is longer than its 1,052 calls suggest:
per-call cost grows with how many features the tab already holds, so a big tab
costs O(N²) to author (measured in `docs/notes/eiffel/FEATURE_NOTES.md` §0).

## Adding an example

In development (`npm run dev`), the Examples panel has a "Save current as
example" button: it writes the open document to this directory and adds a
manifest entry (`POST /api/examples`). Commit both. For a production build the
panel reads `manifest.json` from this directory as static files, exactly as the
Assay browser reads `assay/`.
