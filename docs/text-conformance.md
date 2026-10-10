# Text and font specification status

This file records which external specifications the text and font code intends
to implement. It is a conformance ledger, not a design overview.

The implementation lives in `crates/theoria-text`; source paths below are
relative to that crate's `src/` directory.

For CSS and W3C specifications, this ledger records whether the individual
semantics adopted by Theoria conform to their referenced specifications. It
does not claim that Theoria as a whole is a conforming CSS formatting engine.

Last audited: 2026-08-29.

## Status vocabulary

- **Conformant**: the versioned official conformance data passes in this
  repository.
- **Implemented**: the behavior is present and tested, but the specification
  does not provide a complete conformance suite here.
- **Delegated**: a dependency performs the operation; Theoria has not
  independently established full conformance.
- **Partial**: only the explicitly listed subset is implemented.
- **Not implemented**: no conformance claim is made.

Passing a dependency's tests is not by itself recorded as Theoria conformance.
Likewise, using CSS-like names does not imply CSS conformance.

## Version baseline

- [Unicode 17.0.0](https://www.unicode.org/versions/Unicode17.0.0/)
- [UAX #9 revision 51](https://www.unicode.org/reports/tr9/tr9-51.html)
- [UAX #14 revision 54](https://www.unicode.org/reports/tr14/tr14-54.html)
- [UAX #15 revision 57](https://www.unicode.org/reports/tr15/tr15-57.html)
- [UAX #24 revision 39](https://www.unicode.org/reports/tr24/tr24-39.html)
- [UAX #29 revision 47](https://www.unicode.org/reports/tr29/tr29-47.html)
- [UAX #50 revision 33](https://www.unicode.org/reports/tr50/tr50-33.html)
- [UAX #53 revision 11](https://www.unicode.org/reports/tr53/tr53-11.html)
- [UTS #51 revision 29](https://www.unicode.org/reports/tr51/tr51-29.html)
- [OpenType 1.9.1 (May 2024)](https://learn.microsoft.com/en-us/typography/opentype/spec/)
- [CSS Writing Modes Level 4, Candidate Recommendation, 30 July 2019](https://www.w3.org/TR/2019/CR-css-writing-modes-4-20190730/)
- [CSS Text Module Level 4, Working Draft, 14 August 2026](https://www.w3.org/TR/2026/WD-css-text-4-20260814/)
- [CSS Text Decoration Module Level 4, Working Draft, 4 May 2022](https://www.w3.org/TR/2022/WD-css-text-decor-4-20220504/)
- [W3C Compositing and Blending Level 1, Candidate Recommendation Draft, 21 March 2024](https://www.w3.org/TR/2024/CRD-compositing-1-20240321/)

The precise dependency versions used to realize these behaviors remain locked
in `Cargo.lock`. W3C links above are the immutable `This Version` publications,
not their mutable `Latest published version` aliases. Microsoft does not expose
an equivalent dated URL for the current OpenType 1.9.1 publication; the
normative baseline there is the explicitly recorded version and release date,
while its URL is only a navigation aid.

## Unicode algorithms and properties

| Specification | Status | Theoria implementation and evidence | Known limits |
| --- | --- | --- | --- |
| UAX #9, Unicode Bidirectional Algorithm | **Conformant** for the tested Unicode 17 algorithm | `text/unicode.rs::bidi_info` supplies ICU4X Unicode 17 `Bidi_Class` data to `unicode-bidi`. `BidiTest.txt` and `BidiCharacterTest.txt` are executed by `text/unicode/conformance_tests.rs`. Layout resolves bidi for a complete paragraph before extracting visual line runs. | This is plaintext UBA. Theoria does not implement a DOM/CSS `unicode-bidi` box model. |
| UAX #14, default line breaking | **Conformant** for the Unicode 17 default test data | `text/unicode.rs::line_breaks` uses ICU4X's Unicode 17 segmenter. The complete `LineBreakTest.txt` is executed by `text/unicode/conformance_tests.rs`. | `Strict`, `Normal`, `Loose`, locale-sensitive dictionary segmentation, emergency wrapping, and injected hyphenation are tailorings or engine policy and are not covered by the default conformance claim. Complex-context LSTM segmentation is enabled only for an explicit `th`, `lo`, `km`, or `my` locale. |
| UAX #29, extended grapheme clusters | **Conformant** for Unicode 17 default extended clusters | ICU4X `GraphemeClusterSegmenter`; complete `GraphemeBreakTest.txt`. Grapheme boundaries are used for font fallback and layout policies that must not split user-perceived characters. | Legacy grapheme clusters are not exposed. |
| UAX #29, default word boundaries | **Conformant** for Unicode 17 default word boundaries | ICU4X `WordSegmenter`; complete `WordBreakTest.txt`. Used to find words for an injected hyphenator. | Word selection/editor commands are not yet a public API. Locale-specific lexical word analysis is not claimed. |
| UAX #29, sentence boundaries | **Not implemented** | None. | `SentenceBreakTest.txt` is intentionally not included. No current layout operation requires sentence segmentation. |
| UAX #24 and UCD `Script_Extensions` | **Implemented** as itemization input | `text/unicode.rs::script_extensions` and `text/shaping.rs::script_segments` retain multi-script candidates, resolve common/inherited text, and keep paired brackets with their surrounding script. | UAX #24 defines properties rather than one normative script-run algorithm. Theoria's run resolution is engine policy and has focused tests, not an external conformance suite. |
| UAX #50 `Vertical_Orientation` | **Implemented** as the default for mixed vertical text | ICU4X Unicode 17 `Vertical_Orientation` data is mapped in `text/unicode.rs`; `text/layout/vertical/runs.rs` maps U/Tu to upright, R to sideways, and probes for a vertical alternate before falling back for Tr. | UAX #50 is not a complete vertical layout algorithm. There is no official conformance file wired into this repository. |
| UAX #53 Arabic Mark Transient Reordering | **Delegated**; implementation path audited | Arabic shaping and mark reordering are performed by HarfRust's OpenType shaper. HarfRust 0.13.2 uses generated Unicode 17 data and applies the Arabic modified combining classes during shaping normalization. | Theoria has no independent UAX #53 fixture corpus, so it does not claim independently established UAX #53 conformance. |
| UAX #15 normalization | **Not implemented as a public operation** | The original UTF-8 is preserved. HarfRust may normalize transient shaping buffers internally. | Theoria does not normalize stored text and does not claim a normalization API. |
| UTS #51 emoji presentation and sequences | **Partial; no conformance claim** | Extended grapheme clusters stay intact during fallback, and HarfRust receives variation selectors and ZWJ sequences. Color glyph realizations are supported. | Presentation without an explicit variation selector remains an environment policy selected by font order. Theoria does not currently claim the UTS #51 C2 display capability and runs no Unicode emoji conformance data. |

The versioned official Unicode files live under
`crates/theoria-text/tests/data/unicode/17.0.0/`. Their README records provenance.

## OpenType consumption

Theoria consumes OpenType data; it is not an OpenType font validator. Malformed
table handling is largely delegated to Skrifa and HarfRust.

| OpenType area | Status | Theoria implementation | Known limits |
| --- | --- | --- | --- |
| Standalone fonts and collections | **Implemented** | `Font::from_bytes` and `Font::from_collection_face` retain immutable bytes and a selected face index. | Name-based face selection and a system font resolver are outside this layer. |
| `cmap`, GSUB, GPOS and script/language feature application | **Delegated** | HarfRust shapes itemized runs; Theoria supplies direction, ISO 15924 script, language, variation coordinates, context, and OpenType feature tags in `text/shaping/harfrust_backend.rs`. | Theoria does not run the Unicode text-rendering-test corpus or claim complete shaping equivalence with HarfBuzz. Invalid language strings are currently ignored by the shaping and ICU boundaries rather than reported. |
| TrueType `glyf`, CFF and CFF2 outlines | **Implemented through Skrifa** | `font/realization/outline.rs` lowers line, quadratic and cubic contours to generic `Path`; realization then uses ordinary Canvas coverage. | Outlines are unhinted. |
| Font variations | **Implemented** for explicit axis values | `Font::with_variation` uses Skrifa normalization; the same location is passed to shaping, outlines, metrics, BASE and COLR. | Named instances, STAT metadata, and automatic `opsz` selection are not exposed. |
| Horizontal metrics | **Implemented** | Shaping advances and offsets come from HarfRust; font-wide metrics come from Skrifa. | Device-specific hinted metrics are not used. |
| `vhea`, `vmtx`, VVAR and VORG vertical metrics | **Implemented/delegated** | HarfRust supplies vertical glyph advances and origins, including VORG; `font.rs` distinguishes `vhea` 1.0 and 1.1 for column spacing and applies available MVAR deltas. | Synthesized metrics for fonts without vertical data use Theoria fallbacks and are not an OpenType conformance claim. |
| BASE baselines | **Partial** | `font/baseline.rs` reads horizontal/vertical BASE axes, script records, DFLT fallback, variation deltas, `romn`, and ideographic center derived from `ideo`/`idtp`. | Other baseline tags, language-system MinMax data, and feature-specific extents are not exposed. |
| COLR v0/v1 and CPAL | **Implemented** through Skrifa traversal | `font/realization/colr.rs` lowers solid paints, linear/radial/sweep gradients, transforms, glyph/box clips, layers, all declared COLR composite modes, palette colors and foreground color to Canvas semantics. Tests exercise v0, v1, variable paint data, and a v1 font without a clip list. Variation coordinates are supplied. | Coverage is representative rather than a complete external COLR corpus. Palette labels/types and named palette selection are not exposed. The optional `paint_cached_color_glyph` optimization deliberately falls back to Skrifa traversal. |
| CBDT/CBLC, EBDT/EBLC and sbix bitmap glyphs | **Implemented through Skrifa's bitmap abstraction** | `font/realization/bitmap.rs` handles color PNG, premultiplied BGRA, and 1/2/4/8-bit alpha masks, then lowers them to `Image`/`ColorField` or `Coverage`. | Formats not surfaced by Skrifa's `BitmapData` abstraction are not independently decoded. No glyph bitmap cache is present. |
| OpenType SVG glyphs and CPAL variables | **Implemented for static rendering** | `font/realization/svg.rs` selects the requested glyph element, expands gzip documents with a size limit, resolves CPAL variables/currentColor, rasterizes through resvg, and returns an Image-backed Canvas. | Animation is deliberately disabled; OpenType permits static rendering. SVG is rasterized at realization size and capped at 4096 pixels per dimension. It is not retained as vector Canvas operations. |
| TrueType hinting and device tables | **Not implemented** | Outlines use `DrawSettings::unhinted`. | Small-size rendering quality may differ from platform text rasterizers. |
| OpenType JSTF | **Not implemented** | Arabic and CJK justification are Theoria layout policies. Safe tatweel insertion information comes from HarfRust. | Font-provided JSTF priority and lookup data are not consumed. |
| OpenType MATH | **Not implemented** | None. | Mathematical layout is outside the current UI text foundation. |
| AAT-specific shaping behavior | **Delegated/not audited** | HarfRust contains AAT support, but Theoria has no AAT-specific API or tests. | No conformance claim is made for `morx`, `kerx`, or related Apple tables. |

## CSS and W3C-related behavior

The public API uses several familiar CSS terms, but it is not a CSS formatting
engine. The following limits are intentional and must remain explicit.

| Specification | Status | Implemented subset | Important mismatch or omission |
| --- | --- | --- | --- |
| CSS Writing Modes Level 4 | **Partial; no conformance claim** | Horizontal layout, vertical columns in both block directions, mixed/upright/sideways orientation, explicit combined upright text, UAX #50 orientation data, the strong-LTR bidi behavior of explicit `text-orientation: upright`, and central-baseline synthesis for mixed sideways runs without usable vertical `BASE` data. | Sideways writing modes and the complete CSS box/baseline model are absent. |
| CSS Text Level 4 | **Partial; no conformance claim** | Unicode soft wrapping, strictness selection, `overflow-wrap`-like emergency breaks, injected hyphenation, soft hyphens, greedy/balanced line choice, alignment, spacing, tabs, ellipsis, CJK and Arabic justification, and end hanging punctuation. | This is not the CSS inline formatting model. `HangingPunctuation` has only `None`/`End`; justification and hyphenation policies are smaller; white-space processing, word-break values, line-break-anywhere, CSS boxes, ruby and inline replaced elements are absent. |
| CSS Text Decoration Level 4 | **Partial; no conformance claim** | Underline, overline and line-through with source, thickness and offset. | No decoration style variants, skip-ink, propagation model, font-metric `from-font`, emphasis marks, shadows, or CSS cascade. |
| W3C Compositing and Blending Level 1 | **Implemented as Canvas color algebra** | Canvas retains builtin Porter-Duff identities and all Level 1 blend-mode identities; Vulkan lowers supported combinations using premultiplied internal values. COLR composite modes map through these semantics. | Theoria does not implement CSS group/stacking-context behavior. Canvas coverage is an additional semantic stage specific to this engine. |

## Engine policy, not external conformance

These behaviors are intentionally Theoria policy:

- ordered font fallback at extended-grapheme boundaries, selecting the first
  supplied font that shapes the complete cluster without `.notdef` and never
  performing an implicit platform-font search;
- replacement or error behavior for missing glyphs: replacement searches the
  same ordered chain for U+FFFD, then uses the first font's `.notdef` as the
  final visible indication while retaining the original logical range;
- glyph presentation is selected only after shaping, in COLR, SVG, bitmap,
  then outline order; a realization failure does not silently restart shaping
  with another font;
- injected dictionary hyphenation and its prefix/suffix/consecutive-line limits;
- greedy versus balanced line selection;
- paragraph indents, tab stops and layout overflow reporting;
- conversion of positioned glyphs into generic Canvas operations.

They should be tested as Theoria behavior rather than described as compliance
with Unicode or OpenType.

## Compliance gaps versus verification gaps

The currently claimed **Conformant** subset has no accepted known failures.
A regression expectation must describe specification-compliant behavior; a
known mismatch may not be converted into an accepted test result.

The following are verification gaps. They limit the strength of a claim, but
are not substitutes for a known implementation defect:

1. Add targeted UAX #53/AMTRA cases or document a verified HarfRust guarantee
   before claiming independently established Arabic mark-rendering conformance.
2. Add broader vertical-font fixtures and a complete external COLR corpus
   before claiming coverage of every valid table form.
3. Keep installed Windows fonts as optional integration coverage for broader
   real-world scripts, vertical metrics and color presentations.

The following capabilities are outside the present conformance surface rather
than accepted non-conformances: UAX #29 sentence segmentation; a UTS #51 C2
emoji-display claim and its environment presentation policy; the full CSS
inline formatting, ruby and replaced-element models; hinting; JSTF; MATH; font
resolution; glyph caches/atlases; and interactive text editing such as hit
testing, caret navigation and selection state. If one becomes part of a claimed
profile, its required behavior must be implemented before that claim is added.

## Maintenance rule

When changing a Unicode, OpenType, shaping, vertical-layout or color-font path:

1. update the relevant row and its known limits;
2. use an immutable `This Version` publication URL as the conformance target;
   a mutable `Latest` URL, when useful, is only a navigation aid and never the
   normative baseline;
3. do not promote **Delegated** or **Partial** to **Conformant** without an
   external conformance suite or an equally explicit verification argument;
4. update `crates/theoria-text/tests/data/unicode/<version>/` and all five Unicode conformance tests
   together when changing the Unicode version baseline.
