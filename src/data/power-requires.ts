/**
 * Power `requires` expression evaluation, and the powerset-pairing rule derived from it.
 *
 * The game gates individual powers with `BuyRequires` (exported as `requires`). Some of
 * those gates name a POWERSET rather than a power, which is how the game states rules like
 * "Shield Defense cannot be paired with a weapon set": each side's level-1 powers exclude
 * the other side by name, so taking the pair leaves you unable to buy a first power at all.
 * There is no set-level field carrying that rule — Shield Defense's own `buy_requires` is
 * empty on all three forks. See SETGATE-1 in the rebuild's DATA-GAP-REGISTER.
 *
 * Lives here rather than in the picker because two surfaces need the same answer: the power
 * picker (which powers can I buy) and the powerset dropdown (which sets can I pair). A second
 * copy of this evaluator would be free to drift from the first, and the pair rule is derived
 * from the same expressions the picker filters on — one implementation keeps them honest.
 */

import type { Power, Powerset } from '@/types';

export interface RequiresContext {
  /** Display names of all selected powers (primary + secondary) */
  selectedPowerDisplayNames: Set<string>;
  /** Internal names of all selected powers (e.g., "Dark_Regeneration") */
  selectedPowerInternalNames: Set<string>;
  /**
   * The build's powersets keyed by {@link setKey} (e.g. `quills`, `shield_defense`).
   * A `requires` expression names a set the way the game's data does, which for a renamed
   * set is NOT its `id` slug — `Scrapper_Melee.Quills` is the set that ships as "Spines".
   * Build with {@link heldPowersetNames} / {@link setKeyFromId}.
   */
  selectedPowersetKeys: Set<string>;
}

/**
 * Evaluate a single atom (non-compound) requires expression.
 * Returns true if the condition is met.
 */
function evaluateAtom(atom: string, ctx: RequiresContext): boolean {
  const trimmed = atom.trim();

  // Access level checks → always true for planner. Two surface forms in data:
  //   infix : "char>accesslevel >= 0"
  //   RPN   : "accesslevel char> 0 >="
  // The planner doesn't model accesslevel, so any expression that mentions
  // the `accesslevel` token is treated as satisfied. (Was previously
  // matching only the infix substring `char>accesslevel`, which caused
  // Tanker Radiation Melee's Devastating Blow and several Wind Control
  // powers to be silently filtered out of the picker — their bin-extracted
  // requires is the RPN form.)
  if (/\baccesslevel\b/.test(trimmed)) return true;

  // No dots → simple display name prerequisite (e.g., "Dark Nova")
  if (!trimmed.includes('.')) {
    return ctx.selectedPowerDisplayNames.has(trimmed);
  }

  const parts = trimmed.split('.');

  // 3 segments: AT_Category.Powerset_Name.Power_Internal_Name
  // e.g., "Tanker_Defense.Dark_Armor.Obscure_Sustenance"
  if (parts.length === 3) {
    return ctx.selectedPowerInternalNames.has(parts[2]);
  }

  // 2 segments: AT_Category.Powerset_Name
  // e.g., "Tanker_Defense.Shield_Defense" or "Scrapper_Melee.Quills"
  //
  // Matched against the set's INTERNAL name. This used to derive an id slug from the token
  // (`Quills` → `quills` → compared to `spines`) which silently missed for every renamed
  // set — Shield Defense's exclusion of Scrapper Spines and Stalker Ninja Blade evaluated
  // false because the gate names `Quills` and `Ninja_Sword`.
  if (parts.length === 2) {
    return ctx.selectedPowersetKeys.has(setKey(parts[1]));
  }

  return false;
}

/**
 * Canonical form of a powerset name for matching: lowercase, `-` folded to `_`.
 *
 * Both sides of the comparison go through this, so the token's spelling (`Shield_Defense`),
 * a powerset's internal name (`shield_defense`) and a pool's id slug (`force-of-will`) all
 * land on one key.
 */
export function setKey(name: string): string {
  return name.toLowerCase().replace(/-/g, '_');
}

/**
 * The key for a powerset, from the name the game's own data uses for it.
 *
 * `setPath` is the fully-qualified binary name (`Scrapper_Melee.Quills`), and a two-segment
 * `requires` token names the set exactly that way — so its LEAF is the key.
 *
 * Falls back to the `id` slug for records without one — power POOLS, which
 * `convert-pool-powers.cjs` does not stamp. No shipped expression names a pool by a
 * two-segment path (every one names an archetype set), so the fallback is unexercised
 * today; it is here so a fork that starts naming pools resolves rather than silently
 * missing, which is the failure this function exists to have fixed.
 *
 * There was a third source, an unqualified `internalName` off the Powerset. Nothing ever
 * stamped one — all 980 sets across the three forks carry `setPath` and none carries
 * `internalName` — so the branch was unreachable behind the leaf read above it.
 */
export function setKeyFromId(id: string | undefined, setPath?: string): string | undefined {
  const leaf = setPath?.split('.').pop();
  if (leaf) return setKey(leaf);
  const slug = id?.split('/')[1];
  return slug ? setKey(slug) : undefined;
}

/** Build a {@link RequiresContext}'s `selectedPowersetKeys` from the sets a build holds. */
export function heldPowersetNames(sets: (Powerset | undefined)[]): Set<string> {
  return new Set(
    sets
      .map((s) => setKeyFromId(s?.id, s?.setPath))
      .filter((n): n is string => n !== undefined),
  );
}

/** Comparison operators that appear in raw .def RPN — always treated as
 *  always-true gates by the planner since they're typed against runtime
 *  attribs (accesslevel, level, etc.) the planner doesn't model. */
const RPN_COMPARISON_OPS = new Set(['>=', '<=', '>', '<', '==', '!=']);

/**
 * Evaluate a Reverse Polish Notation requires expression. Raw .def files use
 * RPN (operators come after operands), and the converter doesn't translate
 * to infix, so we evaluate RPN directly.
 *
 * Examples:
 *   "X !"                       → !X
 *   "A B && C !"                → A && B && !C
 *   "A B || C || D || !"        → !(A || B || C || D)
 *   "accesslevel char> 0 >="    → true (accesslevel comparison)
 *
 * Operand tokens that aren't power references (numeric literals, attribute
 * modifiers like `char>` that prefix an attrib access) push `true` so the
 * surrounding comparison/logic doesn't underflow the stack.
 */
function evaluateRpnRequires(raw: readonly string[], ctx: RequiresContext): boolean | null {
  // Trailing comma is a terminator some powers carry — strip it off the last token.
  const tokens = raw
    .map((t, i) => (i === raw.length - 1 ? t.replace(/,$/, '') : t))
    .filter(Boolean);
  const stack: boolean[] = [];
  for (const tok of tokens) {
    if (tok === '!') {
      if (stack.length < 1) return null;
      stack.push(!stack.pop()!);
    } else if (tok === '&&') {
      if (stack.length < 2) return null;
      const b = stack.pop()!;
      const a = stack.pop()!;
      stack.push(a && b);
    } else if (tok === '||') {
      if (stack.length < 2) return null;
      const b = stack.pop()!;
      const a = stack.pop()!;
      stack.push(a || b);
    } else if (RPN_COMPARISON_OPS.has(tok)) {
      // Comparisons are runtime-attribute gates (accesslevel, level, etc.)
      // that the planner treats as always satisfied. Consume two operands
      // and push true.
      if (stack.length < 2) return null;
      stack.pop();
      stack.pop();
      stack.push(true);
    } else if (isRpnAttribQualifier(tok)) {
      // Qualifier tokens like `char>` and `target>` are postfix modifiers
      // on the previous attrib token (`accesslevel char>` together means
      // "the char's accesslevel"), not a separate stack push. No-op.
    } else if (isRpnNumericLiteral(tok)) {
      // Numeric literals push true — they're operands for a following
      // comparison op, which the planner short-circuits.
      stack.push(true);
    } else {
      stack.push(evaluateAtom(tok, ctx));
    }
  }
  if (stack.length !== 1) return null; // not a clean RPN expression
  return stack[0];
}

/** Numeric literal token (e.g. `0`, `1.5`, `-3`). */
function isRpnNumericLiteral(tok: string): boolean {
  return /^-?\d+(\.\d+)?$/.test(tok);
}

/** Postfix qualifier tokens like `char>` / `target>` that modify the
 *  previous attrib token rather than pushing their own stack value. */
function isRpnAttribQualifier(tok: string): boolean {
  return tok.endsWith('>') && !tok.includes('.');
}

/**
 * Detect RPN form. The last whitespace-separated token is one of the operator
 * symbols `!`, `&&`, `||` (the operator follows its operands in RPN).
 * Comparison ops (`>=`, `<=`, etc.) also count — they appear at the end of
 * accesslevel-gated expressions like `accesslevel char> 0 >=`.
 */
function looksLikeRpn(tokens: readonly string[]): boolean {
  if (tokens.length < 2) return false;
  const lastTok = tokens[tokens.length - 1].replace(/,$/, '');
  return (
    lastTok === '!' || lastTok === '&&' || lastTok === '||' ||
    RPN_COMPARISON_OPS.has(lastTok)
  );
}

/**
 * Evaluate a power requires expression against the current build.
 *
 * Two forms appear in data:
 * - **RPN** (from raw .def files):
 *     "X !"                  → !X
 *     "A B && C !"           → A && B && !C
 *     "A B || C || D || !"   → !(A || B || C || D)
 * - **Infix** (translated by an earlier converter pass):
 *     "Power Name"           → requires power by display name
 *     "AT.Set.Power"         → requires power by internal name
 *     "AT.Set"               → requires powerset to be selected
 *     "!expr"                → prefix negation
 *     "!(a || b || c)"       → none of the listed items can be present
 *     "a && b"               → both conditions must be true
 *     "char>accesslevel >= 0" → always true
 *
 * Detection: if the trailing token is an operator (`!`, `&&`, `||`), parse as
 * RPN; otherwise fall through to the infix logic.
 */
export function evaluateRequires(requires: string | readonly string[], ctx: RequiresContext): boolean {
  // The wire carries requires as a token array (COND-8); the infix fallbacks below
  // parse a legacy hand-edited grammar whose atoms are space-free dotted paths, so
  // joining for THAT path is the format's own round trip, not a token-boundary guess.
  // The recursive calls this function makes on infix substrings arrive as strings.
  const tokens = Array.isArray(requires)
    ? (requires as readonly string[]).filter(Boolean)
    : (requires as string).trim().split(/\s+/).filter(Boolean);
  const expr = Array.isArray(requires)
    ? tokens.join(' ')
    : (requires as string).trim();

  // RPN form (raw .def expressions). Includes detection for comparison ops
  // (`>=`, `<=`, `>`, `<`, `==`, `!=`) so accesslevel-gated expressions like
  // `accesslevel char> 0 >=` and the compound `accesslevel char> 0 >= AT.Set.Power &&`
  // (Tanker Radiation Melee Devastating Blow, Wind Control Clear Skies) are
  // recognized as RPN and handed to the evaluator that knows how to short-
  // circuit accesslevel.
  if (looksLikeRpn(tokens)) {
    const result = evaluateRpnRequires(tokens, ctx);
    if (result !== null) return result;
    // Fall through to infix on RPN parse failure (defensive)
  }

  // Handle AND: "expr1 && expr2 && ..."
  if (expr.includes('&&')) {
    return expr.split('&&').every(part => evaluateRequires(part.trim(), ctx));
  }

  // Handle negated group: !(a || b || c)
  if (expr.startsWith('!(') && expr.endsWith(')')) {
    const inner = expr.slice(2, -1);
    return inner.split('||').every(part => !evaluateAtom(part.trim(), ctx));
  }

  // Handle parenthesized group: (a || b || c) or (expr)
  if (expr.startsWith('(') && expr.endsWith(')')) {
    const inner = expr.slice(1, -1);
    if (inner.includes('||')) {
      return inner.split('||').some(part => evaluateAtom(part.trim(), ctx));
    }
    return evaluateRequires(inner, ctx);
  }

  // Handle simple negation: !atom
  if (expr.startsWith('!')) {
    return !evaluateAtom(expr.slice(1), ctx);
  }

  // Handle count expression: "A + B + C > N" (need more than N of the listed powers)
  if (expr.includes('>') && expr.includes('+')) {
    const [sumPart, thresholdPart] = expr.split('>').map(s => s.trim());
    const threshold = parseInt(thresholdPart, 10);
    if (!isNaN(threshold)) {
      const atoms = sumPart.split('+').map(s => s.trim());
      const count = atoms.filter(a => evaluateAtom(a, ctx)).length;
      return count > threshold;
    }
  }

  // Simple atom
  return evaluateAtom(expr, ctx);
}

// ============================================
// POWERSET PAIRING
// ============================================

/**
 * The context-free half of the picker's pick filter — the checks that need only the power.
 *
 * The picker also drops form sub-powers and archetype inherents, both of which need state this
 * function doesn't have. Omitting them can only ever make a set look MORE buyable than it is,
 * so a pairing check built on this fails open (no block) rather than blocking a legal pair.
 *
 * Exported because `AvailablePowers` used to carry a hand-copied twin of these three checks,
 * and the copies drifted: SHOWFLAGS-2 had to be fixed in both places. One predicate, so the
 * gate that grades it (`hidden-mechanic-picks.test.ts`) grades what the picker actually runs.
 */
export function isBuyablePick(p: Power): boolean {
  // -1 is the auto-grant sentinel; HC's bin stores it unsigned, so it also arrives as 0xFFFFFFFF.
  if (p.available < 0 || p.available >= 0x80000000) return false;
  if (p.powerType === 'Global Enhancement') return false;
  // The grant itself, at any level. `AutoIssue` is the game handing the power over, and the
  // level term on it says WHEN, never whether it is bought — Clear Skies arrives once Vacuum
  // and Vortex are trained and costs nothing (ROSTER-2). The Rust planner has read it
  // this way all along (`pick_gate`, powers.rs); this side was still inferring the same fact
  // from a hand-written list of granted powers, which reached neither Clear Skies nor
  // Thunderspy's Fetid Presence. AutoIssue forces Free (`powers_load.c:950`) and the corpus
  // agrees on all four forks, so this arm subsumes the Free half of the hidden check below.
  if (p.autoIssue) return false;
  // Hidden from the Manage screen is not the same as not for sale — see SHOWFLAGS-2 and
  // the picker's own copy of this filter in AvailablePowers.tsx. `free` is the axis, and it
  // still has work to do above: Seismic Shockwaves is handed over Free without AutoIssue.
  if ((p.mechanicType === 'hiddenPassive' || p.mechanicType === 'hiddenAuto') && p.free) return false;
  return true;
}

/** The level-1 powers of `set` that are buyable while `held` are the build's powersets. */
function level1Picks(set: Powerset, held: Set<string>): Power[] {
  const ctx: RequiresContext = {
    selectedPowerDisplayNames: new Set(),
    selectedPowerInternalNames: new Set(),
    selectedPowersetKeys: held,
  };
  return set.powers.filter(
    (p) => p.available === 0 && isBuyablePick(p) && (!p.requires || evaluateRequires(p.requires, ctx)),
  );
}

/**
 * The powersets that would be left unable to buy a first power if `a` and `b` were paired.
 *
 * Empty means the pairing is legal. A non-empty result names the starved side(s). These
 * exclusions are USUALLY reciprocal — Shield Defense's level-1 powers exclude Titan Weapons
 * and Titan Weapons' exclude Shield Defense — but not always: Thunderspy leaves Stone Armor's
 * Stone Skin ungated while Claws still excludes Stone Armor, so only Claws starves and the
 * pairing is unbuildable all the same. Both directions are checked and either one starving is
 * enough, so a fork that gates only one side still gets the right verdict.
 *
 * A set that has no level-1 picks even unpaired is skipped rather than reported: it is starved
 * by something other than this pairing, and blaming the pairing would be wrong.
 */
export function pairingStarvation(a: Powerset | undefined, b: Powerset | undefined): Powerset[] {
  if (!a?.id || !b?.id || a.id === b.id) return [];
  const paired = heldPowersetNames([a, b]);
  const starved: Powerset[] = [];
  for (const set of [a, b]) {
    if (level1Picks(set, new Set()).length === 0) continue;
    if (level1Picks(set, paired).length === 0) starved.push(set);
  }
  return starved;
}

/** Whether `a` and `b` can be taken together. */
export function isPairable(a: Powerset | undefined, b: Powerset | undefined): boolean {
  return pairingStarvation(a, b).length === 0;
}
