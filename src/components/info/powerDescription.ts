/**
 * Splits a power description into the runs InfoPanel renders — SECURITY_AUDIT.md F77.
 *
 * The description is dataset text, and it arrives with the game's own markup in
 * it (`<br>` mostly). It used to be flattened with a regex and handed to
 * `dangerouslySetInnerHTML`, which is bypassable: `/<[^>]+>/g` only strips a tag
 * that CLOSES, so `<img src=x onerror=... ` with no `>` survives the strip and
 * then finds its `>` in the NOTE span the same expression appends. The sink was
 * cleared by an earlier pass on the grounds that the source is dataset text, and
 * nobody had audited the 140 scripts that produce the dataset text (F78).
 *
 * Returning runs instead of a string moves the decision out of the browser's
 * HTML parser: React escapes every `text` here, so the worst a hostile
 * description can do is render as its own source.
 */

export interface DescriptionRun {
  /** True for a `NOTE: ...` sentence, which renders highlighted on its own line. */
  note: boolean;
  text: string;
}

/** Matches a NOTE sentence up to its first full stop, or to the end of the text. */
const NOTE = /NOTE:\s*(.*?)(?:\.|$)/g;

export function descriptionRuns(description: string): DescriptionRun[] {
  // `<br>` was a space here before the runs existed; the game writes it as a
  // sentence separator inside one paragraph, not as a line break.
  const flat = description.replace(/<br\s*\/?>/gi, ' ').replace(/<[^>]+>/g, '');

  const runs: DescriptionRun[] = [];
  let cut = 0;
  for (const match of flat.matchAll(NOTE)) {
    const at = match.index ?? 0;
    if (at > cut) runs.push({ note: false, text: flat.slice(cut, at) });
    // The trailing stop is appended, not captured, so a NOTE ending at the end
    // of the description still reads as a sentence.
    runs.push({ note: true, text: `NOTE: ${match[1]}.` });
    cut = at + match[0].length;
  }
  if (cut < flat.length) runs.push({ note: false, text: flat.slice(cut) });
  return runs;
}
