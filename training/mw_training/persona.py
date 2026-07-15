"""Narrative persona sketches for Spark teacher prompts.

The SOUL feature vector stays numeric, but the teacher benefits from a short
human-readable sketch.  This module deliberately uses only a local, seeded
``random.Random`` instance: rendering a character must not perturb simulation
or training randomness.
"""

from __future__ import annotations

import math
import random
from collections.abc import Mapping, Sequence
from numbers import Real
from typing import TypeAlias

# This is the narrative-facing order from the teacher-prompt contract.  Rust's
# persisted persona array currently stores greed before caution; callers that
# pass a trajectory record should provide a mapping or reorder that array.
TRAIT_NAMES = ("aggression", "sociability", "industriousness", "caution", "greed")
NEED_NAMES = ("hunger", "energy", "social")
PERSONALITY_OVERRIDE = (
    "Let personality override the obvious choice: choose what this character "
    "would do, even when another action looks safest or most efficient."
)

Vector: TypeAlias = Mapping[str, Real] | Sequence[Real]

_TRAIT_WORDS: dict[str, tuple[tuple[str, ...], tuple[str, ...], tuple[str, ...]]] = {
    "aggression": (
        (
            "confrontational",
            "a confrontation-seeking challenger",
            "ready for confrontation",
        ),
        ("even-tempered", "a de-escalator", "slow to pick a fight"),
        ("bold", "a forceful presence", "quick to meet resistance head-on"),
    ),
    "sociability": (
        ("gregarious", "a warm companion", "drawn toward company"),
        ("reserved", "a solitary observer", "more comfortable at the edge of a crowd"),
        ("welcoming", "a natural host", "quick to build a connection"),
    ),
    "industriousness": (
        ("industrious", "a steady worker", "proud of useful effort"),
        ("unhurried", "a reluctant laborer", "protective of their free time"),
        ("diligent", "a reliable craftsperson", "inclined to finish what they start"),
    ),
    "caution": (
        ("cautious", "a guarded planner", "careful before taking a risk"),
        ("venturesome", "a risk-taker", "willing to gamble on a hunch"),
        ("watchful", "a measured scout", "always checking what could go wrong"),
    ),
    "greed": (
        (
            "acquisitive",
            "an acquisitive keeper of gains",
            "acquisitive and alert to every chance to claim more",
        ),
        ("open-handed", "a generous neighbor", "willing to share what they have"),
        ("possessive", "a careful owner", "reluctant to let a useful thing go"),
    ),
}

_NEED_WORDS = {
    "hunger": ("a full belly", "food and immediate comfort"),
    "energy": ("rest and stamina", "having enough strength to keep going"),
    "social": ("companionship", "being seen and included by others"),
}


def _values(vector: Vector, names: tuple[str, ...], label: str) -> tuple[float, ...]:
    if isinstance(vector, Mapping):
        try:
            raw = [vector[name] for name in names]
        except KeyError as exc:
            raise ValueError(f"{label} is missing {exc.args[0]!r}") from exc
    else:
        raw = list(vector)
    if len(raw) != len(names):
        raise ValueError(f"{label} must contain {len(names)} values, got {len(raw)}")

    result: list[float] = []
    for value in raw:
        if (
            not isinstance(value, Real)
            or isinstance(value, bool)
            or not math.isfinite(value)
        ):
            raise ValueError(f"{label} values must be finite numbers")
        # Trajectory exports use fixed-point [0, 1000], while prompt callers
        # commonly use normalized [0, 1].  Accept both without changing output.
        normalized = float(value) / 1000.0 if abs(float(value)) > 1.0 else float(value)
        result.append(min(1.0, max(0.0, normalized)))
    return tuple(result)


def _choice(seed: int, axis: int, options: tuple[str, ...]) -> str:
    # Avoid Python's process-randomized hash() so the seed means the same thing
    # across worker processes and runs.
    mixed = (int(seed) & ((1 << 64) - 1)) ^ (0x9E3779B97F4A7C15 * (axis + 1))
    return random.Random(mixed & ((1 << 64) - 1)).choice(options)


def render_persona(traits: Vector, need_weights: Vector, seed: int) -> str:
    """Render a deterministic, short in-character persona sketch.

    ``traits`` are ordered aggression, sociability, industriousness, caution,
    greed (or supplied by those names as a mapping).  Values may be normalized
    floats or the fixed-point integers emitted by the Rust simulation.
    """
    trait_values = _values(traits, TRAIT_NAMES, "traits")
    needs = _values(need_weights, NEED_NAMES, "need_weights")

    # Always describe the strongest axis, then add one other salient axis. This
    # keeps the sketch short while guaranteeing that a dominant trait is legible.
    ranked = sorted(range(len(trait_values)), key=lambda i: (-trait_values[i], i))
    salient = ranked[:2]
    if trait_values[salient[0]] < 0.35:
        salient = [salient[0]]

    clauses: list[str] = []
    for axis in salient:
        value = trait_values[axis]
        name = TRAIT_NAMES[axis]
        if value >= 0.68:
            bucket = 0
        elif value <= 0.32:
            bucket = 1
        else:
            bucket = 2
        words = _choice(seed, axis, _TRAIT_WORDS[name][bucket])
        clauses.append(words)

    if not clauses:
        clauses.append(
            _choice(
                seed,
                99,
                ("quietly observant", "hard to read", "still finding their footing"),
            )
        )

    strongest_need = max(range(len(needs)), key=lambda i: (needs[i], -i))
    need_name = NEED_NAMES[strongest_need]
    need = _NEED_WORDS[need_name]
    need_phrase = _choice(seed, 200 + strongest_need, need)
    if len(clauses) == 1:
        trait_sentence = f"They are {clauses[0]}, with a mind fixed on {need_phrase}."
    else:
        trait_sentence = (
            f"They are {clauses[0]} and {clauses[1]}, "
            f"with a mind fixed on {need_phrase}."
        )

    opening = _choice(
        seed,
        300,
        (
            "A vivid presence",
            "An unmistakable character",
            "A person with a distinct edge",
        ),
    )
    return f"{opening}: {trait_sentence}"


def teacher_prompt_fragment(traits: Vector, need_weights: Vector, seed: int) -> str:
    """Build the persona fragment inserted into a Spark teacher prompt."""
    sketch = render_persona(traits, need_weights, seed)
    return f"Persona sketch: {sketch}\n{PERSONALITY_OVERRIDE}"


# Explicit aliases make the intent discoverable to callers that call the output
# a sketch rather than a rendered persona, without maintaining another codepath.
persona_sketch = render_persona
render_teacher_prompt = teacher_prompt_fragment


__all__ = [
    "NEED_NAMES",
    "PERSONALITY_OVERRIDE",
    "TRAIT_NAMES",
    "persona_sketch",
    "render_persona",
    "render_teacher_prompt",
    "teacher_prompt_fragment",
]
