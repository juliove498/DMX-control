//! Pure pattern evaluator — `(step, slot_index, total_slots) -> SlotState`.
//!
//! No clock, no state, no mutability. The runtime resolves `current_step`
//! from the wall clock and asks the pattern what each slot should do, which
//! makes the whole engine trivially testable without faking time.

use super::{Pattern, SlotState};

/// Evaluate a pattern for a given (step, slot, total).
pub fn evaluate(pattern: &Pattern, step: u64, slot: usize, total: usize) -> SlotState {
    if total == 0 {
        return SlotState::Off;
    }
    match pattern {
        Pattern::AllTogether => all_together(step),
        Pattern::Alternate => alternate(step, slot),
        Pattern::Chase => chase(step, slot, total),
        Pattern::ChaseReverse => chase_reverse(step, slot, total),
        Pattern::PingPong => ping_pong(step, slot, total),
        Pattern::Random => random(step, slot, total),
        Pattern::Wave => wave(step, slot, total, false),
        Pattern::WaveReverse => wave(step, slot, total, true),
        Pattern::Build => build(step, slot, total, false),
        Pattern::BuildReverse => build(step, slot, total, true),
        Pattern::CenterOut => center_out(step, slot, total),
        Pattern::Symmetric => symmetric(step, slot, total),
        Pattern::OutsideIn => outside_in(step, slot, total),
        Pattern::InvertedChase => inverted_chase(step, slot, total),
        Pattern::GroupsOfTwo => groups_of_n(step, slot, total, 2),
        Pattern::GroupsOfThree => groups_of_n(step, slot, total, 3),
        Pattern::HalfSwap => half_swap(step, slot, total),
        Pattern::Edges => edges(step, slot, total),
        Pattern::PulseOut => pulse_radial(step, slot, total, false),
        Pattern::PulseIn => pulse_radial(step, slot, total, true),
        Pattern::Accordion => accordion(step, slot, total),
        Pattern::Bowtie => bowtie(step, slot, total),
        Pattern::DualChase => dual_chase(step, slot, total),
        Pattern::SymmetricBounce => symmetric_bounce(step, slot, total),
    }
}

/// Every slot blinks in unison: on at even steps, off at odd steps.
fn all_together(step: u64) -> SlotState {
    if step.is_multiple_of(2) {
        SlotState::On
    } else {
        SlotState::Off
    }
}

/// Even-indexed slots vs odd-indexed slots, swapping each step.
/// Step 0: 0,2,4… on. Step 1: 1,3,5… on.
fn alternate(step: u64, slot: usize) -> SlotState {
    if (slot as u64) % 2 == step % 2 {
        SlotState::On
    } else {
        SlotState::Off
    }
}

/// One slot lit at a time, marching forward through the slot list.
fn chase(step: u64, slot: usize, total: usize) -> SlotState {
    if (step % total as u64) == slot as u64 {
        SlotState::On
    } else {
        SlotState::Off
    }
}

/// Same as Chase but stepping right-to-left.
fn chase_reverse(step: u64, slot: usize, total: usize) -> SlotState {
    let pos = (total as u64 - 1 - (step % total as u64)) as usize;
    if slot == pos {
        SlotState::On
    } else {
        SlotState::Off
    }
}

/// Forward chase to the end, then reverse to the start, then forward again.
/// For 4 slots the sequence of "on" indices is: 0, 1, 2, 3, 2, 1, 0, 1, …
fn ping_pong(step: u64, slot: usize, total: usize) -> SlotState {
    if total == 1 {
        // No "back and forth" possible — degenerate to a blink.
        return all_together(step);
    }
    let cycle = (2 * total as u64) - 2;
    let pos_in_cycle = step % cycle;
    let pos = if pos_in_cycle < total as u64 {
        pos_in_cycle as usize
    } else {
        // Mirror back from the far end. e.g. for total=4, cycle=6:
        // pos_in_cycle 4 → 6-4 = 2; pos_in_cycle 5 → 6-5 = 1.
        (cycle - pos_in_cycle) as usize
    };
    if slot == pos {
        SlotState::On
    } else {
        SlotState::Off
    }
}

/// One slot lit per step, picked by a deterministic hash of `step`. Does not
/// guarantee "no consecutive repeat" — that property held by the doc would
/// require either step-by-step recurrence (O(step)) or a true permutation
/// generator. For `total >= 4` the chance of a repeat is ≤ 25 %; close
/// enough for an ambient layer. Will revisit in Sub-fase G if it bothers.
fn random(step: u64, slot: usize, total: usize) -> SlotState {
    let chosen = random_slot(step, total);
    if slot == chosen {
        SlotState::On
    } else {
        SlotState::Off
    }
}

/// Two adjacent slots lit at once, marching forward (or backward) — like a
/// short comet. For total=1 degenerates to a steady single lit slot.
fn wave(step: u64, slot: usize, total: usize, reverse: bool) -> SlotState {
    if total == 1 {
        return SlotState::On;
    }
    let head = if reverse {
        ((total as u64 - 1) - (step % total as u64)) as usize
    } else {
        (step % total as u64) as usize
    };
    let tail = (head + 1) % total;
    if slot == head || slot == tail {
        SlotState::On
    } else {
        SlotState::Off
    }
}

/// Cumulative on/off chase. Cycle length = total + 1: `total` build-up
/// steps plus one all-off "reset" step that gives the eye a clear restart.
/// `reverse=true` flips the polarity (start full, peel off).
fn build(step: u64, slot: usize, total: usize, reverse: bool) -> SlotState {
    let cycle = (total as u64) + 1;
    let pos = (step % cycle) as usize;
    if pos == total {
        return SlotState::Off;
    }
    let on = if reverse { slot >= pos } else { slot <= pos };
    if on {
        SlotState::On
    } else {
        SlotState::Off
    }
}

/// Lit area expands outward from the centre. Cycle = ceil(total/2) + 1
/// (one "all off" reset). Works for both even and odd totals: even totals
/// have two centre slots, odd totals have a single centre slot.
fn center_out(step: u64, slot: usize, total: usize) -> SlotState {
    if total <= 1 {
        return all_together(step);
    }
    let half = total.div_ceil(2);
    let cycle = (half as u64) + 1;
    let pos = step % cycle;
    if pos == half as u64 {
        return SlotState::Off;
    }
    let radius = pos as f32;
    let centre = (total as f32 - 1.0) / 2.0;
    let dist = (slot as f32 - centre).abs();
    // +0.5 so even totals (centre at .5) light their two middle slots at
    // radius 0, matching the "expanding ring" feel.
    if dist <= radius + 0.5 {
        SlotState::On
    } else {
        SlotState::Off
    }
}

/// Two slots marching simultaneously from each end of the strip toward the
/// centre. For total=6: step 0 lights slots 0 and 5, step 1 lights 1 and 4,
/// step 2 lights 2 and 3, then cycle.
fn symmetric(step: u64, slot: usize, total: usize) -> SlotState {
    if total == 1 {
        return all_together(step);
    }
    let half = total.div_ceil(2);
    let pos = (step % half as u64) as usize;
    if slot == pos || slot == total - 1 - pos {
        SlotState::On
    } else {
        SlotState::Off
    }
}

/// Mirror of `center_out`: lit area shrinks from the edges inward,
/// then resets. step 0 lights every slot, each subsequent step
/// drops the outermost lit ring, until only the centre is lit; one
/// final all-off "reset" closes the cycle for visual punctuation.
fn outside_in(step: u64, slot: usize, total: usize) -> SlotState {
    if total <= 1 {
        return all_together(step);
    }
    let half = total.div_ceil(2);
    let cycle = (half as u64) + 1;
    let pos = step % cycle;
    if pos == half as u64 {
        return SlotState::Off;
    }
    let centre = (total as f32 - 1.0) / 2.0;
    let dist = (slot as f32 - centre).abs();
    // Light if the slot is within the *current* lit-radius envelope.
    // pos=0 → max radius (everything lit); pos grows → envelope
    // shrinks → only the centre slots survive.
    // The +0.5 fudge mirrors CenterOut: even totals (centre at .5
    // between two slots) still resolve cleanly.
    let lit_radius = (half as f32) - 0.5 - pos as f32;
    if dist <= lit_radius {
        SlotState::On
    } else {
        SlotState::Off
    }
}

/// "Shadow chase": all slots lit except one, which marches forward.
/// Reads as a dark spot moving across a fully-lit strip — useful
/// when you want presence on every fixture but a moving accent.
fn inverted_chase(step: u64, slot: usize, total: usize) -> SlotState {
    if total == 0 {
        return SlotState::Off;
    }
    let dark = (step % total as u64) as usize;
    if slot == dark {
        SlotState::Off
    } else {
        SlotState::On
    }
}

/// Slots grouped into blocks of `n` adjacent units that march
/// together. e.g. `n=2` over 8 slots: step 0 lights 0,1 → step 1
/// lights 2,3 → … → step 3 lights 6,7 → cycle. Last group may be
/// short for non-divisible totals; we still cycle through all
/// groups.
fn groups_of_n(step: u64, slot: usize, total: usize, n: usize) -> SlotState {
    if total == 0 || n == 0 {
        return SlotState::Off;
    }
    let groups = total.div_ceil(n);
    let active_group = (step % groups as u64) as usize;
    let slot_group = slot / n;
    if slot_group == active_group {
        SlotState::On
    } else {
        SlotState::Off
    }
}

/// Left half lit at even steps, right half lit at odd steps.
/// For odd totals the centre slot belongs to the left half.
fn half_swap(step: u64, slot: usize, total: usize) -> SlotState {
    if total == 0 {
        return SlotState::Off;
    }
    let half = total.div_ceil(2);
    let in_left = slot < half;
    let left_on = step.is_multiple_of(2);
    if (in_left && left_on) || (!in_left && !left_on) {
        SlotState::On
    } else {
        SlotState::Off
    }
}

/// Only the first and last slots blink in unison; the inner slots
/// stay off. Degenerates to `all_together` when total == 1 (single
/// slot is both edges) or 2.
fn edges(step: u64, slot: usize, total: usize) -> SlotState {
    if total == 0 {
        return SlotState::Off;
    }
    if total <= 2 {
        return all_together(step);
    }
    let on_step = step.is_multiple_of(2);
    let is_edge = slot == 0 || slot == total - 1;
    if is_edge && on_step {
        SlotState::On
    } else {
        SlotState::Off
    }
}

/// Single expanding (or contracting) ring: only slots at the current
/// radius from centre are lit. Different from CenterOut/OutsideIn
/// which are cumulative — this one reads as a single travelling
/// pulse, like a sonar ping. `inward=true` reverses the direction.
fn pulse_radial(step: u64, slot: usize, total: usize, inward: bool) -> SlotState {
    if total <= 1 {
        return all_together(step);
    }
    let half = total.div_ceil(2);
    let cycle = (half as u64) + 1;
    let pos = (step % cycle) as usize;
    if pos == half {
        return SlotState::Off;
    }
    let radius = if inward { half - 1 - pos } else { pos };
    let centre = (total as f32 - 1.0) / 2.0;
    let dist = (slot as f32 - centre).abs();
    // Even totals have centre at .5 between two slots; the radius
    // ring there sits at dist 0.5 / 1.5 / 2.5 — match those with a
    // 0.5 fudge band, same trick CenterOut uses.
    let lo = (radius as f32) - 0.5;
    let hi = (radius as f32) + 0.5;
    if dist >= lo && dist <= hi {
        SlotState::On
    } else {
        SlotState::Off
    }
}

/// Continuous breathing — single radial ring rides centre→edges→
/// centre over a 2·half − 1 cycle, no reset frame. Reads as a
/// sustained "in/out" pulse perfect for ambient pads.
fn accordion(step: u64, slot: usize, total: usize) -> SlotState {
    if total <= 1 {
        return all_together(step);
    }
    let half = total.div_ceil(2);
    // Cycle = 2·half − 1 produces positions 0,1,…,half−1,half−2,…,1
    // before wrapping. Even totals: 5 slots → cycle 5 (0,1,2,1,0,…).
    let cycle = (2 * half as u64).saturating_sub(1).max(1);
    let pos_in_cycle = step % cycle;
    let radius_idx = if pos_in_cycle < half as u64 {
        pos_in_cycle as usize
    } else {
        // Mirror back from the far end of the cycle. For cycle C =
        // 2H − 1 the reflection around H−1 lands at C − 1 − pos.
        // pos H → H−2, pos H+1 → H−3, …, pos C−1 → 0.
        (cycle - 1 - pos_in_cycle) as usize
    };
    let centre = (total as f32 - 1.0) / 2.0;
    let dist = (slot as f32 - centre).abs();
    let lo = (radius_idx as f32) - 0.5;
    let hi = (radius_idx as f32) + 0.5;
    if dist >= lo && dist <= hi {
        SlotState::On
    } else {
        SlotState::Off
    }
}

/// Cumulative symmetric build from the edges inward. Both ends grow
/// toward the centre simultaneously; the final frame in the cycle is
/// "all off" so the eye registers a clean restart.
fn bowtie(step: u64, slot: usize, total: usize) -> SlotState {
    if total <= 1 {
        return all_together(step);
    }
    let half = total.div_ceil(2);
    let cycle = (half as u64) + 1;
    let pos = (step % cycle) as usize;
    if pos == half {
        return SlotState::Off;
    }
    // Slot is lit if it sits within `pos` of EITHER edge — i.e. the
    // distance to the *nearest* edge is <= pos.
    let dist_to_edge = slot.min(total - 1 - slot);
    if dist_to_edge <= pos {
        SlotState::On
    } else {
        SlotState::Off
    }
}

/// Two heads marching forward in lockstep, half a strip apart. For
/// even totals this is a perfect mirror; odd totals get an off-by-one
/// pair that wraps cleanly through the modulo.
fn dual_chase(step: u64, slot: usize, total: usize) -> SlotState {
    if total <= 1 {
        return all_together(step);
    }
    let half = total / 2;
    let head_a = (step % total as u64) as usize;
    let head_b = (head_a + half) % total;
    if slot == head_a || slot == head_b {
        SlotState::On
    } else {
        SlotState::Off
    }
}

/// Symmetric pair walks edges→centre→edges→centre… Same shape as
/// Symmetric but with PingPong-style direction reversal at the centre
/// crease. Cycle = 2·half − 2; centre slot (odd totals) lights alone
/// at the crease, pair at all other positions.
fn symmetric_bounce(step: u64, slot: usize, total: usize) -> SlotState {
    if total <= 1 {
        return all_together(step);
    }
    let half = total.div_ceil(2);
    if half <= 1 {
        // total == 2 → no real bounce; degenerate to alternating
        // each end being lit, which still reads symmetrically.
        return if slot as u64 == step % 2 {
            SlotState::On
        } else {
            SlotState::Off
        };
    }
    let cycle = 2 * half as u64 - 2;
    let pos_in_cycle = step % cycle;
    let pos = if pos_in_cycle < half as u64 {
        pos_in_cycle as usize
    } else {
        (cycle - pos_in_cycle) as usize
    };
    if slot == pos || slot == total - 1 - pos {
        SlotState::On
    } else {
        SlotState::Off
    }
}

fn random_slot(step: u64, total: usize) -> usize {
    if total <= 1 {
        return 0;
    }
    // Knuth's integer hash: multiply by a large prime, xor-shift the high
    // bits in to disturb low-order patterns. Cheap and well-distributed
    // for our usage (one call per step).
    let h = step
        .wrapping_mul(2654435761u64)
        .wrapping_add(0x9E37_79B9_7F4A_7C15);
    let h = h ^ (h >> 33);
    (h as usize) % total
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(pattern: &Pattern, step: u64, total: usize) -> Vec<SlotState> {
        (0..total)
            .map(|s| evaluate(pattern, step, s, total))
            .collect()
    }

    fn count_on(states: &[SlotState]) -> usize {
        states.iter().filter(|s| matches!(s, SlotState::On)).count()
    }

    fn on_index(states: &[SlotState]) -> Option<usize> {
        states.iter().position(|s| matches!(s, SlotState::On))
    }

    // ----- AllTogether ----------------------------------------------------

    #[test]
    fn all_together_blinks_in_unison() {
        for step in 0..10 {
            let states = snapshot(&Pattern::AllTogether, step, 4);
            // Either every slot is on, or every slot is off.
            let n = count_on(&states);
            assert!(n == 0 || n == 4, "step {step}: count {n}");
        }
    }

    #[test]
    fn empty_total_returns_off() {
        assert_eq!(evaluate(&Pattern::AllTogether, 0, 0, 0), SlotState::Off);
        assert_eq!(evaluate(&Pattern::Chase, 0, 0, 0), SlotState::Off);
        assert_eq!(evaluate(&Pattern::PingPong, 0, 0, 0), SlotState::Off);
        assert_eq!(evaluate(&Pattern::Random, 0, 0, 0), SlotState::Off);
    }

    // ----- Alternate ------------------------------------------------------

    #[test]
    fn alternate_splits_evens_and_odds() {
        // Step 0: even slots on, odd off.
        let states = snapshot(&Pattern::Alternate, 0, 4);
        assert_eq!(states[0], SlotState::On);
        assert_eq!(states[1], SlotState::Off);
        assert_eq!(states[2], SlotState::On);
        assert_eq!(states[3], SlotState::Off);
        // Step 1: swap.
        let states = snapshot(&Pattern::Alternate, 1, 4);
        assert_eq!(states[0], SlotState::Off);
        assert_eq!(states[1], SlotState::On);
        assert_eq!(states[2], SlotState::Off);
        assert_eq!(states[3], SlotState::On);
    }

    #[test]
    fn alternate_each_step_lights_exactly_half_for_even_total() {
        for step in 0..8 {
            let states = snapshot(&Pattern::Alternate, step, 6);
            assert_eq!(count_on(&states), 3, "step {step}");
        }
    }

    // ----- Chase ----------------------------------------------------------

    #[test]
    fn chase_lights_one_slot_per_step_in_order() {
        let total = 4;
        for step in 0..16 {
            let states = snapshot(&Pattern::Chase, step, total);
            assert_eq!(count_on(&states), 1, "step {step}");
            assert_eq!(on_index(&states), Some((step as usize) % total));
        }
    }

    #[test]
    fn chase_reverse_lights_one_slot_per_step_backwards() {
        let total = 4;
        for step in 0..16 {
            let states = snapshot(&Pattern::ChaseReverse, step, total);
            assert_eq!(count_on(&states), 1, "step {step}");
            let expected = total - 1 - ((step as usize) % total);
            assert_eq!(on_index(&states), Some(expected));
        }
    }

    // ----- PingPong -------------------------------------------------------

    #[test]
    fn ping_pong_traces_forward_then_backward_for_4_slots() {
        // Expected on-index per step for total=4: 0,1,2,3,2,1, 0,1,2,3,2,1,…
        let expected: &[usize] = &[0, 1, 2, 3, 2, 1, 0, 1, 2, 3, 2, 1];
        for (step, &want) in expected.iter().enumerate() {
            let states = snapshot(&Pattern::PingPong, step as u64, 4);
            assert_eq!(count_on(&states), 1, "step {step}");
            assert_eq!(on_index(&states), Some(want), "step {step}");
        }
    }

    #[test]
    fn ping_pong_total_3_traces_short_cycle() {
        // total=3: cycle length 4 (= 2*3 - 2). Expected: 0, 1, 2, 1, 0, 1, 2, 1, …
        let expected: &[usize] = &[0, 1, 2, 1, 0, 1, 2, 1];
        for (step, &want) in expected.iter().enumerate() {
            let states = snapshot(&Pattern::PingPong, step as u64, 3);
            assert_eq!(on_index(&states), Some(want), "step {step}");
        }
    }

    #[test]
    fn ping_pong_total_1_blinks_like_all_together() {
        for step in 0..6 {
            let s = evaluate(&Pattern::PingPong, step, 0, 1);
            let expected = if step % 2 == 0 {
                SlotState::On
            } else {
                SlotState::Off
            };
            assert_eq!(s, expected, "step {step}");
        }
    }

    // ----- Random ---------------------------------------------------------

    #[test]
    fn random_lights_exactly_one_slot_per_step() {
        let total = 8;
        for step in 0..200 {
            let states = snapshot(&Pattern::Random, step, total);
            assert_eq!(count_on(&states), 1, "step {step}");
        }
    }

    #[test]
    fn random_total_1_always_lights_slot_0() {
        for step in 0..50 {
            assert_eq!(evaluate(&Pattern::Random, step, 0, 1), SlotState::On);
        }
    }

    // ----- Wave -----------------------------------------------------------

    #[test]
    fn wave_lights_two_adjacent_slots() {
        let total = 5;
        for step in 0..15 {
            let states = snapshot(&Pattern::Wave, step, total);
            assert_eq!(count_on(&states), 2, "step {step}");
            // The two on indices differ by 1 mod total.
            let lit: Vec<usize> = states
                .iter()
                .enumerate()
                .filter(|(_, s)| matches!(s, SlotState::On))
                .map(|(i, _)| i)
                .collect();
            let diff = (lit[1] + total - lit[0]) % total;
            assert!(
                diff == 1 || diff == total - 1,
                "step {step}: lit slots {lit:?} not adjacent"
            );
        }
    }

    #[test]
    fn wave_total_1_keeps_single_slot_on() {
        for step in 0..6 {
            assert_eq!(evaluate(&Pattern::Wave, step, 0, 1), SlotState::On);
        }
    }

    #[test]
    fn wave_reverse_walks_backward() {
        // total=4: forward head sequence is 0,1,2,3; reverse head 3,2,1,0.
        let total = 4;
        let heads_reverse: &[usize] = &[3, 2, 1, 0, 3, 2, 1, 0];
        for (step, &head) in heads_reverse.iter().enumerate() {
            let states = snapshot(&Pattern::WaveReverse, step as u64, total);
            // Head and head+1 (mod total) are both on.
            assert_eq!(states[head], SlotState::On, "step {step}");
            assert_eq!(states[(head + 1) % total], SlotState::On, "step {step}");
        }
    }

    // ----- Build / BuildReverse -------------------------------------------

    #[test]
    fn build_accumulates_then_resets() {
        let total = 3;
        // Cycle = 4 (= total + 1)
        // step 0: slots 0 on              → 0,_,_
        // step 1: slots 0,1 on            → 0,1,_
        // step 2: slots 0,1,2 on          → 0,1,2
        // step 3: all off                 → reset
        let expected: &[&[bool]] = &[
            &[true, false, false],
            &[true, true, false],
            &[true, true, true],
            &[false, false, false],
            &[true, false, false], // cycle restart
        ];
        for (step, want) in expected.iter().enumerate() {
            let states = snapshot(&Pattern::Build, step as u64, total);
            for (slot, &on_want) in want.iter().enumerate() {
                let actual = matches!(states[slot], SlotState::On);
                assert_eq!(actual, on_want, "step {step} slot {slot}");
            }
        }
    }

    #[test]
    fn build_reverse_peels_off_then_resets() {
        let total = 3;
        let expected: &[&[bool]] = &[
            &[true, true, true],
            &[false, true, true],
            &[false, false, true],
            &[false, false, false],
            &[true, true, true], // cycle restart
        ];
        for (step, want) in expected.iter().enumerate() {
            let states = snapshot(&Pattern::BuildReverse, step as u64, total);
            for (slot, &on_want) in want.iter().enumerate() {
                let actual = matches!(states[slot], SlotState::On);
                assert_eq!(actual, on_want, "step {step} slot {slot}");
            }
        }
    }

    // ----- CenterOut ------------------------------------------------------

    #[test]
    fn center_out_total_5_grows_then_resets() {
        // Centre is slot 2. half = 3 (ceil). Cycle = 4.
        // step 0 (radius 0): only slot 2
        // step 1 (radius 1): slots 1,2,3
        // step 2 (radius 2): slots 0,1,2,3,4 (all)
        // step 3 (reset):    all off
        let total = 5;
        let expected: &[&[bool]] = &[
            &[false, false, true, false, false],
            &[false, true, true, true, false],
            &[true, true, true, true, true],
            &[false, false, false, false, false],
        ];
        for (step, want) in expected.iter().enumerate() {
            let states = snapshot(&Pattern::CenterOut, step as u64, total);
            for (slot, &on_want) in want.iter().enumerate() {
                let actual = matches!(states[slot], SlotState::On);
                assert_eq!(actual, on_want, "step {step} slot {slot}");
            }
        }
    }

    #[test]
    fn center_out_total_4_lights_two_middle_slots_first() {
        // Centre slots are 1 and 2. half = 2. Cycle = 3.
        // step 0: slots 1,2
        // step 1: slots 0,1,2,3 (all)
        // step 2: reset
        let total = 4;
        let expected: &[&[bool]] = &[
            &[false, true, true, false],
            &[true, true, true, true],
            &[false, false, false, false],
        ];
        for (step, want) in expected.iter().enumerate() {
            let states = snapshot(&Pattern::CenterOut, step as u64, total);
            for (slot, &on_want) in want.iter().enumerate() {
                let actual = matches!(states[slot], SlotState::On);
                assert_eq!(actual, on_want, "step {step} slot {slot}");
            }
        }
    }

    // ----- Symmetric ------------------------------------------------------

    #[test]
    fn symmetric_lights_both_ends_converging() {
        // total=6: half=3.
        // step 0 → slots 0 and 5
        // step 1 → slots 1 and 4
        // step 2 → slots 2 and 3
        // step 3 → cycle restart (=step 0)
        let total = 6;
        let expected: &[&[bool]] = &[
            &[true, false, false, false, false, true],
            &[false, true, false, false, true, false],
            &[false, false, true, true, false, false],
            &[true, false, false, false, false, true],
        ];
        for (step, want) in expected.iter().enumerate() {
            let states = snapshot(&Pattern::Symmetric, step as u64, total);
            for (slot, &on_want) in want.iter().enumerate() {
                let actual = matches!(states[slot], SlotState::On);
                assert_eq!(actual, on_want, "step {step} slot {slot}");
            }
        }
    }

    #[test]
    fn symmetric_odd_total_lights_centre_alone_at_end() {
        // total=5: half=3. step 2 → slots 2 and 5-1-2=2 (just centre alone).
        let total = 5;
        let states = snapshot(&Pattern::Symmetric, 2, total);
        assert_eq!(count_on(&states), 1);
        assert_eq!(on_index(&states), Some(2));
    }

    // ----- Random ---------------------------------------------------------

    // ----- OutsideIn ------------------------------------------------------

    #[test]
    fn outside_in_total_5_shrinks_to_centre_then_resets() {
        // Mirror of CenterOut. half=3, cycle=4.
        // step 0: all lit
        // step 1: edges off, slots 1..=3 lit
        // step 2: only centre slot 2 lit
        // step 3: reset (all off)
        let total = 5;
        let expected: &[&[bool]] = &[
            &[true, true, true, true, true],
            &[false, true, true, true, false],
            &[false, false, true, false, false],
            &[false, false, false, false, false],
        ];
        for (step, want) in expected.iter().enumerate() {
            let states = snapshot(&Pattern::OutsideIn, step as u64, total);
            for (slot, &on_want) in want.iter().enumerate() {
                let actual = matches!(states[slot], SlotState::On);
                assert_eq!(actual, on_want, "step {step} slot {slot}");
            }
        }
    }

    // ----- InvertedChase --------------------------------------------------

    #[test]
    fn inverted_chase_dark_slot_marches_forward() {
        let total = 4;
        for step in 0..16 {
            let states = snapshot(&Pattern::InvertedChase, step, total);
            // Exactly one slot off, the rest on.
            assert_eq!(count_on(&states), total - 1, "step {step}");
            let dark_idx = (step as usize) % total;
            assert_eq!(states[dark_idx], SlotState::Off, "step {step}");
        }
    }

    // ----- GroupsOfN ------------------------------------------------------

    #[test]
    fn groups_of_two_lights_pairs() {
        // total=8, n=2 → 4 groups. step 0 lights 0,1; step 1 lights 2,3; etc.
        let total = 8;
        let expected: &[&[bool]] = &[
            &[true, true, false, false, false, false, false, false],
            &[false, false, true, true, false, false, false, false],
            &[false, false, false, false, true, true, false, false],
            &[false, false, false, false, false, false, true, true],
            &[true, true, false, false, false, false, false, false], // cycle
        ];
        for (step, want) in expected.iter().enumerate() {
            let states = snapshot(&Pattern::GroupsOfTwo, step as u64, total);
            for (slot, &on_want) in want.iter().enumerate() {
                let actual = matches!(states[slot], SlotState::On);
                assert_eq!(actual, on_want, "step {step} slot {slot}");
            }
        }
    }

    #[test]
    fn groups_of_three_lights_triplets() {
        // total=6, n=3 → 2 groups. step 0: 0,1,2 → step 1: 3,4,5 → cycle.
        let total = 6;
        let states = snapshot(&Pattern::GroupsOfThree, 0, total);
        assert_eq!(count_on(&states), 3);
        for slot_state in &states[0..3] {
            assert_eq!(*slot_state, SlotState::On);
        }
        let states = snapshot(&Pattern::GroupsOfThree, 1, total);
        for slot_state in &states[3..6] {
            assert_eq!(*slot_state, SlotState::On);
        }
    }

    // ----- HalfSwap -------------------------------------------------------

    #[test]
    fn half_swap_alternates_halves() {
        // total=6 → halves of size 3.
        let total = 6;
        let states = snapshot(&Pattern::HalfSwap, 0, total);
        assert_eq!(states[0], SlotState::On);
        assert_eq!(states[1], SlotState::On);
        assert_eq!(states[2], SlotState::On);
        assert_eq!(states[3], SlotState::Off);
        assert_eq!(states[4], SlotState::Off);
        assert_eq!(states[5], SlotState::Off);
        let states = snapshot(&Pattern::HalfSwap, 1, total);
        assert_eq!(states[0], SlotState::Off);
        assert_eq!(states[3], SlotState::On);
    }

    // ----- Edges ----------------------------------------------------------

    #[test]
    fn edges_blinks_only_first_and_last() {
        let total = 5;
        let states = snapshot(&Pattern::Edges, 0, total);
        assert_eq!(states[0], SlotState::On);
        assert_eq!(states[total - 1], SlotState::On);
        assert_eq!(count_on(&states), 2);
        let states = snapshot(&Pattern::Edges, 1, total);
        assert_eq!(count_on(&states), 0);
    }

    #[test]
    fn edges_total_2_or_less_acts_as_all_together() {
        for total in 1..=2 {
            for step in 0..4 {
                let states = snapshot(&Pattern::Edges, step, total);
                let n = count_on(&states);
                assert!(n == 0 || n == total, "total {total} step {step}: {n}");
            }
        }
    }

    // ----- PulseOut / PulseIn ---------------------------------------------

    #[test]
    fn pulse_out_total_5_single_ring_walks_outward() {
        // total=5, half=3, cycle=4. Centre slot 2.
        // step 0: radius 0 → slot 2
        // step 1: radius 1 → slots 1, 3
        // step 2: radius 2 → slots 0, 4
        // step 3: reset
        let total = 5;
        let expected: &[&[bool]] = &[
            &[false, false, true, false, false],
            &[false, true, false, true, false],
            &[true, false, false, false, true],
            &[false, false, false, false, false],
        ];
        for (step, want) in expected.iter().enumerate() {
            let states = snapshot(&Pattern::PulseOut, step as u64, total);
            for (slot, &on_want) in want.iter().enumerate() {
                let actual = matches!(states[slot], SlotState::On);
                assert_eq!(actual, on_want, "step {step} slot {slot}");
            }
        }
    }

    #[test]
    fn pulse_in_walks_edges_to_centre() {
        let total = 5;
        let expected: &[&[bool]] = &[
            &[true, false, false, false, true],
            &[false, true, false, true, false],
            &[false, false, true, false, false],
            &[false, false, false, false, false],
        ];
        for (step, want) in expected.iter().enumerate() {
            let states = snapshot(&Pattern::PulseIn, step as u64, total);
            for (slot, &on_want) in want.iter().enumerate() {
                let actual = matches!(states[slot], SlotState::On);
                assert_eq!(actual, on_want, "step {step} slot {slot}");
            }
        }
    }

    #[test]
    fn pulse_out_even_total_lights_centre_pair() {
        // total=4 → centre at 1.5. Step 0 should light slots 1 and 2.
        let states = snapshot(&Pattern::PulseOut, 0, 4);
        assert_eq!(states[0], SlotState::Off);
        assert_eq!(states[1], SlotState::On);
        assert_eq!(states[2], SlotState::On);
        assert_eq!(states[3], SlotState::Off);
    }

    // ----- Accordion ------------------------------------------------------

    #[test]
    fn accordion_breathes_without_reset_frame() {
        // total=5, half=3, cycle = 5. Sequence: centre / r1 / r2 / r1 / centre.
        let total = 5;
        let expected: &[&[bool]] = &[
            &[false, false, true, false, false],
            &[false, true, false, true, false],
            &[true, false, false, false, true],
            &[false, true, false, true, false],
            &[false, false, true, false, false],
            &[false, false, true, false, false], // cycle wraps cleanly
        ];
        for (step, want) in expected.iter().enumerate() {
            let states = snapshot(&Pattern::Accordion, step as u64, total);
            for (slot, &on_want) in want.iter().enumerate() {
                let actual = matches!(states[slot], SlotState::On);
                assert_eq!(actual, on_want, "step {step} slot {slot}");
            }
        }
    }

    // ----- Bowtie ---------------------------------------------------------

    #[test]
    fn bowtie_grows_from_both_edges() {
        // total=5, half=3, cycle=4.
        // step 0: only edges (slots 0, 4)
        // step 1: edges + their neighbours (0, 1, 3, 4)
        // step 2: everything (centre included)
        // step 3: reset
        let total = 5;
        let expected: &[&[bool]] = &[
            &[true, false, false, false, true],
            &[true, true, false, true, true],
            &[true, true, true, true, true],
            &[false, false, false, false, false],
        ];
        for (step, want) in expected.iter().enumerate() {
            let states = snapshot(&Pattern::Bowtie, step as u64, total);
            for (slot, &on_want) in want.iter().enumerate() {
                let actual = matches!(states[slot], SlotState::On);
                assert_eq!(actual, on_want, "step {step} slot {slot}");
            }
        }
    }

    // ----- DualChase ------------------------------------------------------

    #[test]
    fn dual_chase_lights_pairs_half_apart() {
        // total=8, half=4. step 0: slots 0,4; step 1: 1,5; step 2: 2,6;
        // step 3: 3,7; step 4: cycles back to 0,4 (since (4+4)%8=0).
        let total = 8;
        for step in 0..16 {
            let states = snapshot(&Pattern::DualChase, step, total);
            assert_eq!(count_on(&states), 2, "step {step}");
            let head = (step as usize) % total;
            let mate = (head + total / 2) % total;
            assert_eq!(states[head], SlotState::On, "step {step}");
            assert_eq!(states[mate], SlotState::On, "step {step}");
        }
    }

    // ----- SymmetricBounce ------------------------------------------------

    #[test]
    fn symmetric_bounce_pair_pingpongs_through_strip() {
        // total=6, half=3, cycle=4. Pos sequence: 0,1,2,1,0,1,2,1,…
        // Pairs: (0,5), (1,4), (2,3), (1,4), (0,5)…
        let total = 6;
        let expected_pos: &[usize] = &[0, 1, 2, 1, 0, 1, 2, 1];
        for (step, &pos) in expected_pos.iter().enumerate() {
            let states = snapshot(&Pattern::SymmetricBounce, step as u64, total);
            assert_eq!(states[pos], SlotState::On, "step {step}");
            assert_eq!(states[total - 1 - pos], SlotState::On, "step {step}");
            assert_eq!(count_on(&states), 2, "step {step}");
        }
    }

    // ----- Random ---------------------------------------------------------

    #[test]
    fn random_distribution_is_roughly_uniform() {
        // Coarse sanity check: over many steps, every slot should get hit.
        // We aren't claiming uniformity strict enough for e.g. χ² — just
        // that no slot is starved.
        let total = 6;
        let mut counts = vec![0usize; total];
        for step in 0..6_000 {
            let chosen = on_index(&snapshot(&Pattern::Random, step, total)).unwrap();
            counts[chosen] += 1;
        }
        for (i, &c) in counts.iter().enumerate() {
            // Expected ~1000. Tight enough to catch a bug, loose enough not
            // to flake.
            assert!(c > 600, "slot {i} only hit {c} times");
            assert!(c < 1500, "slot {i} hit {c} times — distribution skewed");
        }
    }
}
