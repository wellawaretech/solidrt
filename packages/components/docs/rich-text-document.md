# Document model

The value model behind `RichTextEditor`: a `Document` is `{ text, runs, blocks }` - plain text plus attributed runs (inline formatting) and per-paragraph blocks. `plainDocument(text)` builds one from a string; `createDocumentBuffer(doc)` wraps one in the editing API (`format`, `formatBlock`, `insertAtom`, `attributes`, selection and edits) that `editorRef` hands out. `ATOM` is the inline-atom placeholder character (U+FFFC) for embedded objects.

The shapes (`Document`, `DocumentRun`, `Attributes`, `AttributePatch`, `DocumentBuffer`) are exported so an app can build, inspect, persist, and transform documents outside the editor.
