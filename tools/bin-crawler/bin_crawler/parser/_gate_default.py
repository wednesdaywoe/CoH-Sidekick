"""Is an effect group's `Requires` satisfied in the default situation?

A gated group is either part of the power's unconditional base or a branch that
only applies under some condition. Deciding which needs an answer to "is this
gate true when nothing special is going on?", and that needs the expression's
STRUCTURE — its root operator and how each leaf sits under it.

The converter used to answer it with `stripped.endsWith('!')`: read the last
token of the `&&`-flattened expression as if it were the top-level operator.
That holds for a conjunction of negations ("no combo level active", "not in a
raid" — genuinely the default state) and is backwards when the `!` negates only
ONE conjunct. Rebirth Guardian's Dispersion Bubble carries `isPVPMap? entref
target> entref source> eq ! &&` — "on a PvP map AND target is not self" — where
the root operator is `&&`, and its whole PvP-only defence set landed in base.
Two facets were patched by hand; 3,230 groups across 223 expressions were
exposed (DATA-GAP-REGISTER COND-1).

So parse the expression and evaluate it, three-valued, under an explicit
DEFAULT SITUATION — what is true of a character standing in ordinary PvE
content with nothing switched on:

    caster    the BUILD — one of the dataset's own player archetypes, unmezzed,
              at full HP and endurance, every meter and combo counter at zero,
              no mode active, owning no temp power, token or entitlement, no
              event fired recently, not flying, movement and recharge unbuffed
    target    a generic living critter at full HP, unmezzed, and NOT the caster,
              not a friend, in no named enemy group, wearing no named costume
              and answering to no named villain — the things a specific target
              would have, a generic one does not
    world     an ordinary PvE map: not a PvP map, no zone event, no mission
              script state, no hard-mode challenge, not on a task force
    the hit   landed. `@ForceHit` reads true, because a planner shows what a
              power does when it connects
    unknowns  the caster's LEVEL, the target's RANK, and every to-hit or `rand`
              ROLL — the export cannot see a build's level or a specific target,
              and guessing one is how a rank fork's two halves end up in the
              same slot

A tree that evaluates TRUE is the power's base case. FALSE (a state that is off
by default) and INDETERMINATE are both conditional: the converter's job is to
keep them out of base, not to pick a winner.

Two things this deliberately does NOT do:

  * It does not decide display policy — with one named exception, kept because
    it was the converter's settled verdict and not COND-1's to re-open:
    `_leech_target_state`. Everything else here is a game reading.
  * It does not pick ONE archetype for the caster. `arch source> Class_Scrapper
    eq` on a pool power is base for a Scrapper and not for anyone else, and one
    boolean cannot say that. So the tree is evaluated once per player archetype
    the dataset defines, and a group whose verdict differs between them reports
    INDETERMINATE_ARCHETYPE carrying the archetypes it IS base for
    ([`GateVerdict.archetypes`]) — a resolution the consumer applies to the
    build it is showing, not a flavour of unknown (DATA-GAP-REGISTER AT-FORK-1).

Not a duplicate of the engine's evaluator. `crates/coh_math/src/expr.rs` walks
the same RPN to answer "given THIS target, is the gate true?", which needs a
target. This runs at export time with no target at all, which is why it asks
what holds by default rather than what is true.
"""

from typing import NamedTuple

from ._requires import RequiresParseError, parse

SATISFIED = "SATISFIED"
UNSATISFIED = "UNSATISFIED"
INDETERMINATE = "INDETERMINATE"
INDETERMINATE_ARCHETYPE = "INDETERMINATE_ARCHETYPE"
UNPARSED = "UNPARSED"
UNCLASSIFIED = "UNCLASSIFIED"

VERDICTS = frozenset({SATISFIED, UNSATISFIED, INDETERMINATE,
                      INDETERMINATE_ARCHETYPE, UNPARSED, UNCLASSIFIED})

TRUE, FALSE = True, False
# `None` is the third value, in the Kleene tables below.

# A fourth value that is not a truth value at all: a clause that does not bear
# on the base case and drops out of whatever operator it meets. Only a
# `@CustomFX` comparison produces it — the engine stores that variable per
# AttribMod to pick a VISUAL (`combateval_StoreCustomFXToken`), and the forks it
# guards carry the same numbers on every branch: Rebirth Cinders splits its Held
# 8.0/6.0 `Ranged_Immobilize` across `@customFX BrightFieryBinds eq` and its
# negation, identical either way. Every player has some customization, so
# exactly one branch always holds; reading the clause as absent is therefore the
# faithful answer, not a display convenience.
_VACUOUS = "vacuous"


class GateVocabError(Exception):
    """A requires clause this classifier has no reading for."""


class _Situation(NamedTuple):
    """What one evaluation pass is allowed to assume about the two entities.

    `player_classes` is the dataset's own player-archetype catalogue, folded to
    lower case. It is the discriminator that tells `arch target> Class_Controller`
    (is a TEAMMATE a Controller) from `arch target> Class_Minion_Grunt` (what
    RANK is the critter) — two different questions the game spells with the same
    attribute, so nothing but the catalogue can separate them. `None` means the
    caller supplied none, and a clause that needs one raises rather than guess.

    `source_class` is the archetype bound to the caster for this pass, or `None`
    while it is still unbound. [`evaluate`] runs the tree once per archetype to
    turn an archetype fork into an answer instead of a shrug.
    """

    player_classes: frozenset | None
    source_class: str | None


# ---------------------------------------------------------------- attributes

# Every mez, knock and movement-suppression magnitude reads 0 on a character
# nothing has been done to.
_ZERO_MAGNITUDE = (
    "kheld", "ksleep", "kstunned", "kstun", "kimmobilized", "kimmobilize",
    "kterrorized", "kterrorize", "kconfused", "kconfuse", "kuntouchable",
    "kfly", "kknockback", "kknockup", "kknockdown", "krepel", "kplacate",
    "kintangible", "kteleport", "konlyaffectsself", "kafraid", "ktaunt",
)

# The value each attribute reads in the default situation, by stem (an aspect
# prefix is stripped first — see `_attr_value`). `None` means the export cannot
# know it.
ATTR_DEFAULT = {
    **{stem: 0.0 for stem in _ZERO_MAGNITUDE},
    # Meters and counters that start empty. `kstealth` is the Domination meter
    # on the forks and actual stealth everywhere else; both start at 0.
    "kmeter": 0.0,
    "kstealth": 0.0,
    "krage": 0.0,          # Fury
    "kabsorb": 0.0,
    "kendurance": 100.0,
    "kendurance%": 100.0,
    "khitpoints%": 100.0,
    # Alive and at full health. cur == max keeps every ratio test honest:
    # `Cur.kHitPoints target> Max.kHitPoints target> 0.9 * >` reads TRUE.
    "khitpoints": 1000.0,
    "ktohit": 0.75,        # the game's base to-hit
    # Movement and recharge on a character carrying no buff.
    "krunspeed": 1.0, "kflyspeed": 1.0, "kspeedjumping": 1.0,
    "kjumpheight": 1.0, "krechargetime": 1.0, "kspeedrunning": 1.0,
    # The build's level is an input the export does not have.
    "combatlevel": None,
    "level": None,
}
_ASPECTS = ("cur.", "mod.", "max.", "str.", "abs.", "res.")

# Read off an entity but boolean, not a magnitude.
_ENTITY_FLAGS = {
    "ispet": FALSE,          # the default target is not a pet
    "intaskforce": FALSE,    # ordinary content, not a task force
}

# `<attr> <selector> <name> eq` where the attribute names WHO the entity is.
# None of these can be answered without a specific entity, with one exception:
# the planner's target is a critter, which is what `enttype` asks.
_IDENTITY_ATTRS = {
    "enttype", "arch", "rank", "group", "costume", "entref", "type", "origin",
    "alignment", "allyid", "praetorianprogress", "villaingroup",
}
# The subset naming ONE specific identity, which a generic target does not have.
_NAMED_IDENTITY_ATTRS = {"group", "costume", "type", "origin", "villaingroup"}

_ENTITY_SELECTORS = {"source>", "target>", "source.owner>", "target.owner>",
                     "source.creator>", "target.creator>"}

# Seconds since an event that has not happened. Large enough that every
# `EventTimeSince> N >` in the corpus reads TRUE and every `<= N` reads FALSE.
_NEVER = 1.0e9


def _attr_value(name):
    stem = name.lower()
    for aspect in _ASPECTS:
        if stem.startswith(aspect):
            stem = stem[len(aspect):]
            # A strength reads 1.0 unbuffed whatever the attribute is, so it
            # never needs its own row.
            if aspect == "str.":
                return 1.0
            break
    if stem not in ATTR_DEFAULT:
        raise GateVocabError(f"no default for attribute {name!r}")
    return ATTR_DEFAULT[stem]


# ---------------------------------------------------------------- predicates

# Predicates that are simply OFF in the default situation. Grouped by what kind
# of thing is off, because that is the classification the register asked for.
_ABSENT = {
    # STATE — the caster or target has not switched this on
    "source.mode?", "target.mode?", "mode?",
    "source.ownpower?", "target.ownpower?", "ownpower?",
    "source.toggleactive?", "target.toggleactive?", "toggleactive?",
    "source.tokenowned?", "target.tokenowned?", "tokenowned?",
    "architect.tokenowned?",
    "source.onstoryarc?", "target.onstoryarc?", "onstoryarc?",
    # IDENTITY of the generic target — it carries no content tag, and the
    # relationship default is "a foe". `target.isFriend?` is a caster/target
    # RELATIONSHIP (`character_TargetIsFriend`), not a target property; an ally
    # power's own gate therefore reads FALSE here, which preserves the verdict
    # the shortcut reached and leaves the ally case to its own register entry.
    "source.hastag?", "target.hastag?", "hastag?",
    "target.isfriend?",
    # ENVIRONMENT — an ordinary PvE map with no event or challenge running
    "ispvpmap?", "ismissionmap?", "isarchitectmap?", "istutorialmap?",
    "isheroonlymap?", "isvillainonlymap?", "isheroandvillainmap?",
    "ishazardmap?", "istrialmap?", "isbetashard?",
    "target.challenge?", "challenge?",
    "source.involume?", "source.involume>", "target.involume?",
    "target.involume>", "involume>",
    "ontaskforce?", "onflashback?", "onarchitect?",
    "hasanyactivemission?", "missionstarted?", "missionobjective?",
    "taskowner?",
    # ENTITLEMENT — a badge, token or store product the export cannot check.
    # Absent is the reading that keeps entitlement content out of base.
    "owned?", "owns?", "badgeowned?", "source.owned?", "target.owned?",
    "productowned?", "source.productowned?", "target.productowned?",
    "productavailable?",
    "loyaltyowned?", "source.loyaltyowned?", "target.loyaltyowned?",
    "loyaltytierowned?", "source.loyaltytierowned?", "target.loyaltytierowned?",
    "loyaltylevelowned?", "source.loyaltylevelowned?",
    "target.loyaltylevelowned?",
    "isvip?", "source.isvip?", "target.isvip?",
    "isreactivationactive?",
    "costumekey?", "hassouvenir?", "hasclue?", "hascontact?",
    "canactivate?", "canbuypowerwithoutoverflow?", "cangiverespec?",
    "hasfreespec?", "powerset?", "enabledpower?",
    # The account server is not consulted; treating it as unavailable keeps the
    # content it gates conditional rather than folding a store check into base.
    "isaccountserveravailable?", "source.isaccountserveravailable?",
    "target.isaccountserveravailable?",
    "isaccountinventoryloaded?", "source.isaccountinventoryloaded?",
    "target.isaccountinventoryloaded?",
}

# `<name> target.VillainName?` asks whether the target IS one named critter.
# The planner's target is a generic one, so it is none of them — the same
# reading `group`/`costume` get below, and the one that keeps Oil Slick's
# "don't hit my own patch" exclusion (`Pets_OilSlickTarget target.VillainName>
# !`) in base.
_ABSENT_NAMED_IDENTITY = {"target.villainname>", "source.villainname>"}

# Counters that read zero: nothing owned, no event fired, no script advanced.
_ZERO_COUNTERS = {
    "source.ownpowernum?", "target.ownpowernum?", "ownpowernum?",
    "source.eventcount>", "target.eventcount>", "eventcount>",
    "zoneevent>", "scriptmessage>", "arcvar>", "auth>",
    "source.tokenval>", "target.tokenval>", "tokenval>",
    "source.tokentime>", "target.tokentime>", "tokentime>",
    "badgecount", "stat>", "boostsslotted>", "emptyslots>",
    "certspurchased>", "voucherspurchased>", "vouchersunclaimed>",
    "loyaltynodesbought>", "loyaltypointsearned>",
    "source.loyaltypointsearned>", "target.loyaltypointsearned>",
    "power.attacktypecount>",
}
_ELAPSED_SINCE = {"source.eventtimesince>", "target.eventtimesince>",
                  "eventtimesince>", "source.eventtime>", "target.eventtime>",
                  "eventtime>"}
# Readings that depend on something the export has no view of at all.
_INDETERMINATE_READS = {
    "distance", "power.base>", "power.boosted>", "system>",
    "source.mapteamarea>", "target.mapteamarea>", "mapteamarea>",
    "source.teamsize>", "target.teamsize>", "teamsize>",
    "mapname>",
}


def _number(node, sit):
    """Numeric value of a node in the default situation, or None if unknown."""
    op, kids = node.op, node.kids
    low = op.lower()
    if not kids:
        try:
            return float(op)
        except ValueError:
            pass
        if low == "now":
            return _NEVER
        if low in ("rand", "distance"):
            return None
        if low == "@forcehit":
            # "skip the to-hit check" — the flag is what makes an auto-hit patch
            # land, and it pairs with the roll as `@ToHitRoll @ToHit < @ForceHit
            # ||`, i.e. "the effect landed". A planner shows per-hit numbers, so
            # the default situation is a landed hit.
            return 1.0
        if low.startswith("@"):
            return None            # @ToHitRoll / @ToHit / @ChanceMods: the roll
        if low in _INDETERMINATE_READS or low in _ZERO_COUNTERS:
            return None if low in _INDETERMINATE_READS else 0.0
        # A bare name in numeric position. The engine pushed it as a string
        # (`eval_Validate` treats an unregistered token as an operand) and the
        # comparison coerces it; `target.TickDamage 0 >` reads a value no export
        # can quantify. Unknown, not an error — the fail-loud that matters is on
        # unrecognised OPERATORS, which `parse` and `_bool` still raise for.
        return None
    if low in ("+", "-", "*", "/", "%", "pow"):
        a, b = (_number(k, sit) for k in kids)
        if a is None or b is None:
            return None
        if low == "+":
            return a + b
        if low == "-":
            return a - b
        if low == "*":
            return a * b
        if low == "pow":
            return a ** b
        return a / b if b else None
    if low == "negate":
        a = _number(kids[0], sit)
        return None if a is None else -a
    if low == "minmax":
        lo, hi, val = (_number(k, sit) for k in kids)
        if lo is None or hi is None or val is None:
            return None
        return max(lo, min(hi, val))
    if low in _ENTITY_SELECTORS:
        name = kids[0].op.lower()
        if name in _ENTITY_FLAGS:
            return 1.0 if _ENTITY_FLAGS[name] else 0.0
        if name in _IDENTITY_ATTRS:
            return None
        return _attr_value(kids[0].op)
    if low in _ZERO_COUNTERS:
        return 0.0
    if low in _ELAPSED_SINCE:
        return _NEVER
    if low in _INDETERMINATE_READS:
        return None
    if low in _ABSENT:
        # A mode read compared numerically — `kRendingSliceCooldown Source.Mode?
        # 0 ==` asks whether the mode is off, which it is.
        return 0.0
    if low in ("!", "&&", "||", "eq", "==", "!=", "<", "<=", ">", ">="):
        # The engine pushes an int for every boolean, so one can be summed:
        # `EndActivateClick target.EventCount> 0 > EndActivateInsp
        # target.EventCount> 0 > +` counts how many of two events fired.
        val = _bool(node, sit)
        return None if val is None else (1.0 if val else 0.0)
    raise GateVocabError(f"no numeric reading for {op!r}")


# Reads that return a NAME rather than a magnitude. `_NO_NAME` is a value that
# equals nothing: the default map is not any map the data names, and the default
# team area is not any named area, so an `eq` against either is FALSE.
_NO_NAME = object()
_NAME_READS = {
    "mapname>": _NO_NAME,
    "source.mapteamarea>": _NO_NAME,
    "target.mapteamarea>": _NO_NAME,
    "mapteamarea>": _NO_NAME,
    # A costume choice the export cannot see. Reached only when `_is_customfx`
    # did not already drop the clause as vacuous — see `_VACUOUS`.
    "@customfx": None,
    # The executing power's own category / powerset / numbers.
    "power.base>": None,
    "power.boosted>": None,
}


def _name(node):
    """String value of a node: the name itself, `_NO_NAME`, or None if unknown."""
    low = node.op.lower()
    if low in _NAME_READS:
        return _NAME_READS[low]
    if not node.kids:
        if low.startswith("@"):
            return None                     # an eval variable, not a literal
        return node.op                      # a plain name operand
    # Everything else is a read off some entity. `_identity_eq` has already had
    # its turn, so whatever reaches here is a name the export cannot pin.
    return None


def _string_equal(node):
    """The engine's `eq` / string-mode `==`: case-insensitive name comparison."""
    a, b = (_name(k) for k in node.kids)
    if a is None or b is None:
        return None
    if a is _NO_NAME or b is _NO_NAME:
        return FALSE
    return a.lower() == b.lower()


def _leech_target_state(node):
    """PLANNER POLICY — the one place a display decision enters this module.

    A leech power splits on whether the foe it hit is still alive: DNA Siphon
    heals per LIVING target (`Cur.kHitPoints target> 0 >`) and gains
    +Regen/+Recovery per DEFEATED one (`… 0 ==`). The gated effect IS what the
    power advertises — its shortHelp reads "Self +HP, +End, +Special" — so the
    converter has always folded both halves into the base display, and this
    keeps that verdict rather than re-deciding it here. Structurally the alive
    half is satisfied and the defeated half is not.

    Deliberately anchored to the exact two shapes the converter's own strip
    matched: a threshold test (`kHitPoints% target> 15 <`, Scourge) is a real
    conditional and must not be swept in. Returns None when not that shape.
    """
    if node.op.lower() not in (">", "=="):
        return None
    lhs, rhs = node.kids
    if lhs.op.lower() != "target>" or not lhs.kids:
        return None
    if lhs.kids[0].op.lower() != "cur.khitpoints":
        return None
    try:
        if float(rhs.op) != 0.0:
            return None
    except ValueError:
        return None
    return TRUE


def _identity_eq(node, sit):
    """`<attr> <selector> <name> eq` → value, or `(None, False)` if not that shape."""
    if node.op.lower() not in ("eq", "=="):
        return None, False
    lhs, rhs = node.kids
    lop = lhs.op.lower()
    if lop not in _ENTITY_SELECTORS or not lhs.kids:
        return None, False
    attr = lhs.kids[0].op.lower()
    if attr not in _IDENTITY_ATTRS:
        return None, False
    if attr == "entref":
        # `entref target> entref source> eq` — "the target IS the caster". The
        # planner's default recipient is somebody else; an effect that really
        # does skip the caster carries that as `not_on_caster`, a discriminator
        # of its own, so this clause is not what decides the base case.
        return FALSE, True
    if attr == "arch":
        # `arch` is one attribute asking three different questions, and only the
        # CLASS IT NAMES tells them apart — the selector does not, because both
        # sides name player archetypes and critter classes alike.
        named = rhs.op.lower()
        if lop == "target>":
            if sit.player_classes is None:
                raise GateVocabError(
                    f"`arch target> {rhs.op}` needs the dataset's player-class "
                    "catalogue to say whether it names an archetype or a rank")
            if named in sit.player_classes:
                # A named PLAYER archetype on the target. Cosmic Balance
                # (targets_affected: Teammate, radius 300) asks whether a
                # CONTROLLER IS ON THE TEAM — its own description says so. The
                # default situation has no teammate and a target that is a
                # generic critter, and a critter is not a Controller either, so
                # both readings of "the target" answer FALSE.
                return FALSE, True
            # A critter class: what it really asks is the target's RANK, which
            # falls through to the rule below.
        elif lop == "source>":
            # The CASTER's archetype — the build's own, which `evaluate` binds
            # one archetype at a time. Unbound it is simply unknown.
            return (None if sit.source_class is None
                    else sit.source_class == named), True
        else:
            # `source.owner>` / `*.creator>`: the archetype of whoever SUMMONED
            # the caster. Binding the build here would assume the caster is the
            # build, which is exactly what these selectors exist to deny — the
            # gate sits on a pet's power and asks about its master.
            return None, True
    if attr == "enttype" and lop == "target>":
        name = rhs.op.lower()
        # The planner's default target is a critter. An unseen third entity type
        # reads unknown rather than collapsing onto either answer.
        return (TRUE if name == "critter" else FALSE if name == "player" else None), True
    if attr in _NAMED_IDENTITY_ATTRS:
        # A NAMED identity — an enemy group, a costume, a villain type. The
        # planner's target is a generic critter, so it is in no named group and
        # wears no named costume, which keeps a "not a pet" exclusion (Toy Bat's
        # `group target> MastermindPets eq ! …`) in base.
        return FALSE, True
    # Rank, alignment, allegiance, story progress — and `arch target>` naming a
    # critter class, which is the same question under the other spelling:
    # properties every entity has, whose value the export cannot see. Rank in
    # particular must stay unknown; reading a rank fork's negated half as the
    # default is what put Thunderous Blast's rank-gated -0.3 endurance drain
    # over its ungated -0.55 (MAPGATE-1).
    return None, True


def _is_customfx(node):
    """`@CustomFX <name> eq` — a visual selector, in either casing."""
    if node.op.lower() not in ("eq", "=="):
        return False
    return any(k.op.lower() == "@customfx" and not k.kids for k in node.kids)


def _bool(node, sit):
    """Three-valued value of a boolean node in the default situation."""
    op, kids = node.op, node.kids
    low = op.lower()
    if low == "!":
        val = _bool(kids[0], sit)
        if val is _VACUOUS:
            return _VACUOUS
        return val if val is None else (not val)
    if low in ("&&", "||"):
        a, b = (_bool(k, sit) for k in kids)
        # A vacuous operand drops out, leaving the other one to decide.
        if a is _VACUOUS:
            return b
        if b is _VACUOUS:
            return a
        short, long_ = (FALSE, TRUE) if low == "&&" else (TRUE, FALSE)
        if a is short or b is short:
            return short
        if a is None or b is None:
            return None
        return long_
    if _is_customfx(node):
        return _VACUOUS
    policy = _leech_target_state(node)
    if policy is not None:
        return policy
    val, matched = _identity_eq(node, sit)
    if matched:
        return val
    if low in ("eq", "==", "!=", "<", "<=", ">", ">="):
        if low in ("eq", "=="):
            # `eq` is StringEqual, and `==` delegates to it whenever both sides
            # are names (`eval.c` NumericEqual: "The user probably meant to use
            # string equality"). So read the pair as numbers only when both
            # sides really are numbers.
            try:
                a, b = (_number(k, sit) for k in kids)
            except GateVocabError:
                a = b = None
            if a is not None and b is not None:
                return a == b
            return _string_equal(node)
        a, b = (_number(k, sit) for k in kids)
        if a is None or b is None:
            return None
        if low == "!=":
            return a != b
        if low == "<":
            return a < b
        if low == "<=":
            return a <= b
        if low == ">":
            return a > b
        return a >= b
    if low in _ABSENT or low in _ABSENT_NAMED_IDENTITY:
        return FALSE
    if low in _ENTITY_SELECTORS and kids:
        name = kids[0].op.lower()
        if name in _ENTITY_FLAGS:
            return _ENTITY_FLAGS[name]
    if not kids:
        if low == "@forcehit":
            return TRUE
        if low.startswith("@"):
            return None                     # a to-hit or chance roll
        try:
            # The bare `1` / `0` sentinels some powers carry as a no-op gate.
            return float(op) != 0.0
        except ValueError:
            pass
    # A numeric reading used where a boolean was expected — the sum above, or a
    # bare counter used as its own gate.
    try:
        val = _number(node, sit)
    except GateVocabError:
        raise GateVocabError(f"no boolean reading for {op!r}") from None
    return None if val is None else val != 0.0


class GateVerdict(NamedTuple):
    """A group's base-case verdict, plus the archetypes it forks over.

    `archetypes` is empty unless `verdict` is `INDETERMINATE_ARCHETYPE`, where it
    names — in the dataset's own `Class_*` spelling — every player archetype the
    gate is SATISFIED for. That set is the resolution: the group is base for a
    build of one of those archetypes and conditional for every other, which is
    the thing one boolean could not say.
    """

    verdict: str
    archetypes: tuple[str, ...] = ()


def _verdict_of(tree, sit) -> str:
    """One evaluation pass's verdict, or UNCLASSIFIED if a clause has no reading."""
    try:
        value = _bool(tree, sit)
    except GateVocabError:
        return UNCLASSIFIED
    if value is None:
        return INDETERMINATE
    if value is _VACUOUS:
        # The whole gate was visual — nothing left that bears on the base case.
        return SATISFIED
    return SATISFIED if value else UNSATISFIED


def _names_caster_archetype(node) -> bool:
    """Whether any clause reads the CASTER's archetype (`arch source>`)."""
    if node.op.lower() == "source>" and node.kids:
        if node.kids[0].op.lower() == "arch":
            return True
    return any(_names_caster_archetype(k) for k in node.kids)


def evaluate(tokens: list[str], *, player_classes=()) -> GateVerdict:
    """Whether a group's `Requires` holds in the default situation.

    `UNPARSED` when the RPN does not reduce, `UNCLASSIFIED` when it reduces but
    names a clause with no reading — both explicit, never folded into a verdict,
    so a vocabulary gap surfaces instead of silently becoming a base effect.
    Both are at zero corpus-wide and gated there.

    `player_classes` is the dataset's own player archetypes, as `parse_classes`
    names them. Every `arch` clause needs it: on the target to tell an archetype
    test from a rank test, on the caster to enumerate the fork. Passing none
    leaves both unanswerable, so a gate that asks reports UNCLASSIFIED — an
    honest "this caller supplied no catalogue", not a guess that reads as data.
    """
    if not tokens:
        return GateVerdict(SATISFIED)
    try:
        tree = parse(tokens)
    except RequiresParseError:
        return GateVerdict(UNPARSED)

    catalogue = frozenset(name.lower() for name in player_classes) or None
    unbound = _Situation(catalogue, source_class=None)
    if catalogue is None or not _names_caster_archetype(tree):
        return GateVerdict(_verdict_of(tree, unbound))

    # The caster's archetype is the one unknown the export CAN retire: run the
    # tree once per archetype the dataset defines. Agreement means the fork was
    # cosmetic (a gate naming a critter class holds for no build at all); a split
    # is the real thing, and the SATISFIED side of it is the answer.
    per_archetype = {
        name: _verdict_of(tree, _Situation(catalogue, name.lower()))
        for name in player_classes
    }
    distinct = set(per_archetype.values())
    if len(distinct) == 1:
        return GateVerdict(distinct.pop())
    return GateVerdict(
        INDETERMINATE_ARCHETYPE,
        tuple(name for name, v in per_archetype.items() if v == SATISFIED),
    )


def default_verdict(tokens: list[str], *, player_classes=()) -> str:
    """[`evaluate`]'s verdict alone, for callers that do not resolve the fork."""
    return evaluate(tokens, player_classes=player_classes).verdict
