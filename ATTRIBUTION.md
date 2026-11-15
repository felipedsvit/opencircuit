# Attribution

Third-party files that are part of the repository, with their origin and
licence. Everything else in `assets/` and in the source tree is original and
under the licences in `LICENSE-MIT` and `LICENSE-APACHE`. Dependencies are
checked by `cargo deny` and are not listed here.

## Liberation Mono Regular 2.1.5

| | |
| --- | --- |
| File | `assets/fonts/liberation-mono/LiberationMono-Regular.ttf` |
| Licence text | `assets/fonts/liberation-mono/LICENSE` (the file of the release) |
| Licence | SIL Open Font License 1.1 (SPDX: `OFL-1.1`) |
| Copyright | Digitized data copyright (c) 2010 Google Corporation; copyright (c) 2012 Red Hat, Inc. Reserved Font Name: Liberation |
| Source | <https://github.com/liberationfonts/liberation-fonts>, release 2.1.5, `liberation-fonts-ttf-2.1.5.tar.gz` linked from the notes of the release |
| SHA-256 | `f2b83c763e8afd21709333370bed4774337fae82267937e2b5aea7e2fbd922c1` |
| Used for | the text of the PDF export (`circuit-export`), compiled into the binary and embedded as a subset in each PDF |

The file is the unmodified file of the release, so it keeps its name. The
subset written into a PDF is part of that document, which the licence leaves
free of its terms; the subset is never offered as a font file. The font is
metric-compatible with Courier New, so texts keep the widths of the Courier
text that the export used before.

Do not rename, edit or subset the file in the repository: the Reserved Font
Name would then have to change. Update it only with a new release, a new
checksum above and the new licence text.
