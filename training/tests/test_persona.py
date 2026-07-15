from __future__ import annotations

from mw_training.persona import render_persona, teacher_prompt_fragment


def test_persona_rendering_is_deterministic_for_seed_and_traits():
    traits = [900, 700, 400, 200, 100]
    needs = [500, 800, 300]

    first = render_persona(traits, needs, seed=42)
    assert first == render_persona(traits, needs, seed=42)
    assert first != render_persona(traits, needs, seed=43)


def test_trait_salience_changes_narrative_frame():
    needs = [500, 500, 500]
    aggressive = render_persona([1000, 0, 0, 0, 0], needs, seed=7)
    acquisitive = render_persona([0, 0, 0, 0, 1000], needs, seed=7)

    assert aggressive != acquisitive
    assert "confront" in aggressive.lower()
    assert "acquisit" in acquisitive.lower()


def test_teacher_fragment_carries_override_instruction():
    fragment = teacher_prompt_fragment([1000, 0, 0, 0, 0], [100, 100, 100], seed=9)

    assert fragment.startswith("Persona sketch: ")
    assert "Let personality override the obvious choice" in fragment
