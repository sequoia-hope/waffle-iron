# Official examples

Documents the Examples panel (toolbar → Examples, next to Assay) offers. Each
entry in `manifest.json` names a document and, when the document was
generated, the generator that produced it. Opening an example loads a COPY
under a fresh document id, so saving it never touches the shipped file.

| Example | Document | Generator | Parts | Bodies |
|---|---|---|---|---|
| Gravel bike v2 | `gravel-bike-v2.waffle.gz` | `gravel-bike-v2.py` | 11 | ~200 |
| Eiffel Tower | `eiffel-tower.waffle.gz` | `eiffel-tower.py` | 10 | 1,964 |

## Why `.gz`

A `.waffle` is pretty-printed JSON and about two thirds of it is indentation:
the tower is 3.5 MB of text, 177 KB gzipped. These documents are build
artifacts — the reviewable source is the generator beside each one — and
nothing compresses them in transit, because `.waffle` has no registered media
type, so they are stored and shipped compressed
(`docs/notes/eiffel/FEATURE_NOTES.md` §6). The writer still emits
pretty-printed JSON for everything that lands in git as source, which is what
keeps the assay corpus and the fixtures diffable.

`fetchExampleDocument` inflates by sniffing the gzip magic, not the file
extension, so a plain `.waffle` entry still works and a host that labels the
file `Content-Encoding: gzip` (Vite's dev server does; a static host generally
does not) is not inflated twice. The generators write gzip when the output
path ends in `.gz`, with no embedded filename and `mtime 0`, so rebuilding the
same document produces the same bytes.

## Regenerating an example

```
python3 app/static/examples/gravel-bike-v2.py --build app/static/examples/gravel-bike-v2.waffle.gz
python3 app/static/examples/eiffel-tower.py   --build app/static/examples/eiffel-tower.waffle.gz
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
