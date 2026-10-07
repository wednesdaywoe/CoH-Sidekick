"""Structural reading of a `Requires` RPN expression's entity-type constraint.

The forks encode the PvE/PvP split two ways. One is a flag on the Effect, which
Parse7 carries in the binary. The other is a clause in the group's `Requires`
expression testing the target's entity type — `enttype target> player eq` or
`... critter eq` — and that one has to be *read*, because a clause can be
negated, or sit in a disjunction with a branch that has nothing to do with
combat scope.

Every consumer used to answer this with a substring test for
`target> player eq`. That is right for the common bare case and backwards for
the two shapes below, both of which occur in the shipped data (MAPGATE-1):

    arch target> Class_Minion_Grunt eq … enttype target> player eq || !
        "target is not a minion and not a player" — the Scrapper/Stalker
        critical-hit branch against lieutenants and bosses. The authored def
        pairs it with a `CritSmall` sibling that has the same clause *without*
        the `player eq ||` and without the `!`. Read as PvP-only, the higher
        crit rate disappears from PvE entirely.

    Raid target.HasTag? enttype target> player eq || kRage source> 70 < &&
        "target is raid-tagged OR a player, and Fury is under 70". The
        `player eq` is one branch of an `||`, so it neither implies nor
        forbids PvP.

So parse the expression and ask a satisfiability question instead: treating the
target's entity type as one variable and every other boolean leaf as free, can
this group ever apply against a critter, and can it ever apply against a
player? That answers exactly what the consumer needs to know, and it answers it
from the expression's structure rather than from the presence of a token.

Not a duplicate of the engine's evaluator. `crates/coh_math/src/expr.rs` walks
the same RPN but answers a different question: given THIS target, is the gate
true? That needs a target, so it can only run once a fight is being projected.
This runs at export time with no target at all, which is why it asks what is
satisfiable rather than what is true. The two agree where they overlap, and the
engine's own test
(`the_crit_rank_gates_partition_the_target_classes`) is the third
independent witness that the first shape above applies against a boss.
"""

# Arity of every operator, ported from the game's own evaluator registration —
# the only table that can settle this, because a `Requires` is run by that
# evaluator and by nothing else. Three sources, all in the leaked server tree:
#
#   libs/UtilitiesLib/src/utils/eval.c   `s_FuncTable` — the built-in operators
#                                        every context gets from `eval_Create`
#   Common/entity/character_combat_eval.c
#                                        `combateval_Init` — the context that
#                                        evaluates an EFFECT GROUP's requires
#   Common/entity/character_eval.c       `chareval_AddDefaultFuncs` — the
#                                        context that evaluates a POWER's own
#                                        `requires` (availability), a different
#                                        table reached through `chareval_requires`
#
# The union is safe: no name is registered in both contexts with a different
# arity. Two entries this replaced were wrong, and neither could surface while
# `entity_scope` short-circuited on expressions without `enttype`:
# `source.MapTeamArea>` was 1 and is 0 (the engine registers
# `source.mapTeamArea>` with no arguments), which is why four Praetorian
# expressions failed to reduce.
#
# Operand tokens (numbers, class names, power paths, `@`-variables) are absent
# and take arity 0 — see `parse` for what the engine does with a token it does
# not recognise.
ARITY = {
    # --- eval.c s_FuncTable: built-in operators -------------------------------
    '&&': 2, '||': 2, '!': 1,
    '>': 2, '<': 2, '>=': 2, '<=': 2, '==': 2,
    'eq': 2,
    '+': 2, '*': 2, '/': 2, '%': 2, '-': 2, 'pow': 2,
    'negate': 1, 'rand': 0, 'minmax': 3, 'date>': 1,
    'var>': 1, 'auto>': 1, 'dup': 1, 'drop': 1,
    # --- character_combat_eval.c combateval_Init: an effect group's requires --
    'target.VillainName>': 1,
    'target>': 1, 'source>': 1,
    'target.EventTime>': 1, 'source.EventTime>': 1,
    'target.EventCount>': 1, 'source.EventCount>': 1,
    'target.EventTimeSince>': 1, 'source.EventTimeSince>': 1,
    'target.TeamSize>': 1, 'source.TeamSize>': 1,
    'now': 0, 'distance': 0,
    'target.HasTag?': 1, 'source.HasTag?': 1,
    'target.Owned?': 1, 'source.Owned?': 1,
    'target.TokenOwned?': 1, 'source.TokenOwned?': 1,
    'target.TokenVal>': 1, 'source.TokenVal>': 1,
    'target.TokenTime>': 1, 'source.TokenTime>': 1,
    'target.mode?': 1, 'source.mode?': 1,
    'target.ToggleActive?': 1, 'source.ToggleActive?': 1,
    'mapname>': 0,
    'source.mapTeamArea>': 0, 'target.mapTeamArea>': 0,
    'auth>': 1, 'system>': 1,
    'target.OnStoryArc?': 1, 'source.OnStoryArc?': 1,
    'isPvPMap?': 0, 'isMissionMap?': 0, 'isArchitectMap?': 0,
    'target.ownPower?': 1, 'source.ownPower?': 1,
    'target.ownPowerNum?': 1, 'source.ownPowerNum?': 1,
    'target.inVolume?': 1, 'target.inVolume>': 1,
    'source.inVolume?': 1, 'source.inVolume>': 1,
    'target.owner>': 1, 'source.owner>': 1,
    'target.creator>': 1, 'source.creator>': 1,
    'target.isFriend?': 0,
    'target.LoyaltyOwned?': 1, 'source.LoyaltyOwned?': 1,
    'target.LoyaltyTierOwned?': 1, 'source.LoyaltyTierOwned?': 1,
    'target.LoyaltyLevelOwned?': 1, 'source.LoyaltyLevelOwned?': 1,
    'target.ProductOwned?': 1, 'source.ProductOwned?': 1,
    'ProductAvailable?': 1,
    'target.LoyaltyPointsEarned>': 0, 'source.LoyaltyPointsEarned>': 0,
    'target.isVIP?': 0, 'source.isVIP?': 0,
    'target.isAccountServerAvailable?': 0, 'source.isAccountServerAvailable?': 0,
    'target.isAccountInventoryLoaded?': 0, 'source.isAccountInventoryLoaded?': 0,
    'power.base>': 1, 'power.boosted>': 1, 'power.attacktypecount>': 0,
    'ZoneEvent>': 1,
    # --- character_eval.c chareval_AddDefaultFuncs: a power's own requires ----
    'char>': 1, 'ent>': 1, 'owner>': 1,
    'EventTime>': 1, 'EventCount>': 1, 'stat>': 1,
    'HasTag?': 1, 'HasSouvenir?': 1, 'HasClue?': 1, 'TokenOwned?': 1,
    'CostumeKey?': 1, 'OnTaskForce?': 0, 'OnFlashback?': 0, 'OnArchitect?': 0,
    'TokenVal>': 1, 'TokenTime>': 1, 'TeamSize>': 1, 'badgecount': 0,
    'owns?': 1, 'owned?': 1, 'BadgeOwned?': 1,
    'mode?': 1, 'powerset?': 1, 'enabledpower?': 1, 'ToggleActive?': 1,
    'CanActivate?': 1, 'ownPower?': 1, 'ownPowerNum?': 1,
    'EmptySlots>': 1, 'CanGiveRespec?': 0, 'HasFreespec?': 0,
    'BoostsSlotted>': 1, 'hasContact?': 1, 'inVolume>': 1,
    'MissionObjective?': 1, 'TaskOwner?': 0, 'MissionStarted?': 0,
    'OnStoryArc?': 1, 'HasAnyActiveMission?': 0,
    'isHeroOnlyMap?': 0, 'isVillainOnlyMap?': 0, 'isHeroAndVillainMap?': 0,
    'isHazardMap?': 0, 'isTrialMap?': 0, 'mapTeamArea>': 0,
    'architect.TokenOwned?': 1, 'isBetaShard?': 0, 'IsReactivationActive?': 0,
    'CertsPurchased>': 1, 'VouchersPurchased>': 1, 'VouchersUnclaimed>': 1,
    'LoyaltyOwned?': 1, 'LoyaltyTierOwned?': 1, 'LoyaltyLevelOwned?': 1,
    'LoyaltyNodesBought>': 0, 'LoyaltyPointsEarned>': 0,
    'CanBuyPowerWithoutOverflow?': 1, 'ProductOwned?': 1,
    'isVIP?': 0, 'isAccountServerAvailable?': 0, 'isAccountInventoryLoaded?': 0,
    # --- fork additions, arity by stack balance -------------------------------
    # Not in the leaked tree's tables, but shipped in the data and reducing to
    # exactly one value only at this arity.
    'arcvar>': 1,           # mission-arc variable
    'ScriptMessage>': 1,    # zone script message (HC's sibling of ZoneEvent>)
    'Challenge?': 1,        # HC hard mode; `target.Challenge?` is its scoped twin
    'target.Challenge?': 1,
    'isTutorialMap?': 0,    # one more map predicate, alongside isPvPMap? et al
}
# The data is inconsistently cased; match on a folded key.
_ARITY = {k.lower(): v for k, v in ARITY.items()}

# Tokens shaped like a selector or predicate that the engine does NOT register,
# and so pushes as a plain operand. Listing them keeps `parse`'s alarm for an
# unregistered `>`/`?` token — a real vocabulary gap — while letting the
# handful the corpus actually ships read the way the engine reads them.
UNREGISTERED_OPERANDS = {
    # `target.VillainName>` is registered; the `source.` spelling is not, so
    # `Clockwork_Paladin_New source.VillainName> eq` compares two literals.
    'source.villainname>',
}

# The selector that names the target itself. `target.owner>` is deliberately not
# here: an `enttype target.owner> …` test is about a pet's master, not about who
# is being hit, so it says nothing about combat scope. Same for every `source>`
# form — those describe the caster.
_TARGET_SELECTORS = {'target>'}

PVE_ONLY = 'PVE_ONLY'
PVP_ONLY = 'PVP_ONLY'
EITHER = 'EITHER'
NEVER = 'NEVER'
UNPARSED = 'UNPARSED'


class RequiresParseError(Exception):
    """The expression could not be reduced to a single value."""


class _Node:
    __slots__ = ('op', 'kids')

    def __init__(self, op, kids):
        self.op, self.kids = op, kids


def parse(tokens: list[str]) -> _Node:
    """RPN token list -> tree. Raises unless it reduces to exactly one value.

    Takes the list the wire holds, not a joined string. A costume-FX token like
    `Minimal FX` is one operand and re-splitting it made two, which is what left
    40 Homecoming expressions unreducible (DATA-GAP-REGISTER COND-8).
    """
    stack: list[_Node] = []
    for tok in tokens:
        n = _ARITY.get(tok.lower())
        if n is None:
            # The engine pushes any token its function table does not name as a
            # plain operand (`eval.c` `eval_Validate`: `else iSize++`). Do the
            # same — but a token SHAPED like a selector or predicate that is
            # neither registered nor a known-unregistered spelling is a
            # vocabulary gap, not a name, so say so rather than corrupting the
            # tree with a silent literal.
            if ((tok.endswith('>') or tok.endswith('?'))
                    and tok.lower() not in UNREGISTERED_OPERANDS):
                raise RequiresParseError(f'no arity for {tok!r} in {tokens!r}')
            n = 0
        if n:
            if len(stack) < n:
                raise RequiresParseError(f'stack underflow at {tok!r} in {tokens!r}')
            kids = stack[-n:]
            del stack[-n:]
        else:
            kids = []
        stack.append(_Node(tok, kids))
    if len(stack) != 1:
        raise RequiresParseError(
            f'reduced to {len(stack)} values, not 1: {tokens!r}')
    return stack[0]


def _enttype_name(node: _Node):
    """`enttype target> <name> eq` -> `<name>`, else None."""
    if node.op.lower() not in ('eq', '=='):
        return None
    lhs, rhs = node.kids
    if (lhs.op.lower() in _TARGET_SELECTORS
            and lhs.kids and lhs.kids[0].op.lower() == 'enttype'
            and not rhs.kids):
        return rhs.op.lower()
    return None


def _target_enttype_names(node: _Node) -> set:
    """Every entity-type name the expression tests the target against."""
    name = _enttype_name(node)
    out = {name} if name is not None else set()
    for k in node.kids:
        out |= _target_enttype_names(k)
    return out


def _satisfiable(node: _Node, want: bool, ent_type: str) -> bool:
    """Can `node` evaluate to `want` when the target's entity type is `ent_type`?

    Every boolean leaf except a target `enttype` test is free, so this asks
    whether *some* assignment works, not whether one is likely. Leaves are
    treated as independent; where they are not, that only ever widens the
    answer toward EITHER, which is the verdict that keeps content.
    """
    name = _enttype_name(node)
    if name is not None:
        return want == (name == ent_type)
    op = node.op
    if op == '!':
        return _satisfiable(node.kids[0], not want, ent_type)
    if op == '&&':
        a, b = node.kids
        if want:
            return (_satisfiable(a, True, ent_type)
                    and _satisfiable(b, True, ent_type))
        return (_satisfiable(a, False, ent_type)
                or _satisfiable(b, False, ent_type))
    if op == '||':
        a, b = node.kids
        if want:
            return (_satisfiable(a, True, ent_type)
                    or _satisfiable(b, True, ent_type))
        return (_satisfiable(a, False, ent_type)
                and _satisfiable(b, False, ent_type))
    return True


def entity_scope(tokens: list[str]) -> str:
    """The combat scope a group's `Requires` confines it to.

    `UNPARSED` when the expression constrains entity type but the parse failed —
    an explicit unknown, never folded into `EITHER`, so a vocabulary gap
    surfaces instead of silently keeping PvP content.
    """
    if not any('enttype' in tok.lower() for tok in tokens):
        # No entity-type test at all, so nothing here bears on combat scope.
        # Short-circuiting also keeps the arity table off the critical path for
        # the ~1,200 expressions built from selectors it has never had to know.
        return EITHER
    try:
        root = parse(tokens)
    except RequiresParseError:
        return UNPARSED
    # The corpus only ever tests `player` and `critter`, but treat the entity
    # type as an open set so an unseen third name reads as "some non-player
    # type" rather than collapsing both branches to NEVER.
    names = _target_enttype_names(root)
    others = (names - {'player'}) or {'critter'}
    pve = any(_satisfiable(root, True, et) for et in others)
    pvp = _satisfiable(root, True, 'player')
    if pve and not pvp:
        return PVE_ONLY
    if pvp and not pve:
        return PVP_ONLY
    if not pve and not pvp:
        return NEVER
    return EITHER
