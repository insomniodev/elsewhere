//! Greedy label placement: each dot, in order, claims the first spot that
//! fits, so earlier dots win rather than the layout being solved globally.

use std::f32::consts::FRAC_1_SQRT_2;

/// In the same pixel space as the dots.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Slot {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Slot {
    fn right(&self) -> f32 {
        self.x + self.w
    }

    fn bottom(&self) -> f32 {
        self.y + self.h
    }
}

/// Where a label went.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Placed {
    pub slot: Slot,
    /// For a label set away from its dot: where on its edge the line from
    /// the dot's centre ends.
    pub leader: Option<(f32, f32)>,
}

fn overlaps(a: &Slot, b: &Slot) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

fn within(slot: &Slot, frame: &Slot) -> bool {
    slot.x >= frame.x
        && slot.y >= frame.y
        && slot.right() <= frame.right()
        && slot.bottom() <= frame.bottom()
}

/// `slot` slid the least distance to lie inside `frame`, or `None` when it is
/// too big to.
fn clamp(slot: &Slot, frame: &Slot) -> Option<Slot> {
    if slot.w > frame.w || slot.h > frame.h {
        return None;
    }
    Some(Slot {
        x: slot.x.clamp(frame.x, frame.right() - slot.w),
        y: slot.y.clamp(frame.y, frame.bottom() - slot.h),
        ..*slot
    })
}

/// Whether the line between the two points passes through `slot`. Running
/// along an edge or touching a corner does not count.
fn crosses(from: (f32, f32), to: (f32, f32), slot: &Slot) -> bool {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    // The stretch of the line, as shares of its length, inside every edge
    // tried so far.
    let (mut enter, mut leave) = (0.0_f32, 1.0_f32);
    for (step, room) in [
        (-dx, from.0 - slot.x),
        (dx, slot.right() - from.0),
        (-dy, from.1 - slot.y),
        (dy, slot.bottom() - from.1),
    ] {
        if step == 0.0 {
            if room <= 0.0 {
                return false;
            }
        } else if step < 0.0 {
            enter = enter.max(room / step);
        } else {
            leave = leave.min(room / step);
        }
    }
    enter < leave
}

/// Tries right, left, below, then above each dot, then beside it hanging down
/// or up from the dot's own edge, then the same eight slid inside `frame`,
/// which a narrow window needs at its edges. Where a crowd leaves none of
/// those, the label is set off from its dot by each of `reaches` in turn,
/// nearest first, to the same four sides and then the four corners, and
/// tied back to the dot by a leader.
///
/// Gives up on a label (`None`) rather than let it leave `frame`, overlap
/// another label or any dot, whose disc counts as its bounding square, or
/// lie over a leader; rather than run its own leader through a label or a
/// dot; and for a dot outside `frame`, whose label would float free of it.
pub(super) fn place(
    dots: &[(f32, f32)],
    sizes: &[(f32, f32)],
    radius: f32,
    gap: f32,
    frame: Slot,
    reaches: &[f32],
) -> Vec<Option<Placed>> {
    assert_eq!(dots.len(), sizes.len());

    let discs: Vec<Slot> = dots
        .iter()
        .map(|&(x, y)| Slot {
            x: x - radius,
            y: y - radius,
            w: radius * 2.0,
            h: radius * 2.0,
        })
        .collect();

    let mut placed: Vec<Slot> = Vec::new();
    let mut leaders: Vec<((f32, f32), (f32, f32))> = Vec::new();
    let mut out = Vec::with_capacity(dots.len());

    for (dot, (&(cx, cy), &(w, h))) in dots.iter().zip(sizes).enumerate() {
        let clear = radius + gap;
        let beside = [
            (cx + clear, cy - h / 2.0),
            (cx - clear - w, cy - h / 2.0),
            (cx - w / 2.0, cy + clear),
            (cx - w / 2.0, cy - clear - h),
            (cx + clear, cy - radius),
            (cx + clear, cy + radius - h),
            (cx - clear - w, cy - radius),
            (cx - clear - w, cy + radius - h),
        ]
        .map(|(x, y)| Slot { x, y, w, h });
        let apart = |reach: f32| {
            let side = clear + reach;
            let corner = clear + reach * FRAC_1_SQRT_2;
            [
                (cx + side, cy - h / 2.0),
                (cx - side - w, cy - h / 2.0),
                (cx - w / 2.0, cy + side),
                (cx - w / 2.0, cy - side - h),
                (cx + corner, cy + corner),
                (cx + corner, cy - corner - h),
                (cx - corner - w, cy + corner),
                (cx - corner - w, cy - corner - h),
            ]
            .map(|(x, y)| Slot { x, y, w, h })
        };

        let free = |candidate: &Slot| {
            within(candidate, &frame)
                && !placed.iter().any(|p| overlaps(candidate, p))
                && !discs.iter().any(|d| overlaps(candidate, d))
                && !leaders
                    .iter()
                    .any(|&(from, to)| crosses(from, to, candidate))
        };
        // The nearest point of the label, where its leader would end, when
        // the way there is clear.
        let led = |candidate: &Slot| {
            let end = (
                cx.clamp(candidate.x, candidate.right()),
                cy.clamp(candidate.y, candidate.bottom()),
            );
            let blocked = placed.iter().any(|p| crosses((cx, cy), end, p))
                || discs
                    .iter()
                    .enumerate()
                    .any(|(other, disc)| other != dot && crosses((cx, cy), end, disc));
            (!blocked).then_some(end)
        };
        let shown =
            (frame.x..=frame.right()).contains(&cx) && (frame.y..=frame.bottom()).contains(&cy);
        let found = shown
            .then(|| {
                beside
                    .iter()
                    .copied()
                    .find(&free)
                    .or_else(|| {
                        beside
                            .iter()
                            .filter_map(|candidate| clamp(candidate, &frame))
                            .find(&free)
                    })
                    .map(|slot| Placed { slot, leader: None })
                    .or_else(|| {
                        reaches.iter().find_map(|&reach| {
                            apart(reach).into_iter().filter(&free).find_map(|slot| {
                                led(&slot).map(|end| Placed {
                                    slot,
                                    leader: Some(end),
                                })
                            })
                        })
                    })
            })
            .flatten();

        if let Some(found) = found {
            placed.push(found.slot);
            if let Some(end) = found.leader {
                leaders.push(((cx, cy), end));
            }
        }
        out.push(found);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Without leaders, as most of these need.
    fn place(
        dots: &[(f32, f32)],
        sizes: &[(f32, f32)],
        radius: f32,
        gap: f32,
        frame: Slot,
    ) -> Vec<Option<Slot>> {
        super::place(dots, sizes, radius, gap, frame, &[])
            .into_iter()
            .map(|placed| placed.map(|placed| placed.slot))
            .collect()
    }

    #[test]
    fn a_label_blocked_on_all_four_sides_hangs_down_beside_its_dot() {
        // The other dots stand in the way right, left and below; the frame
        // cuts off above.
        let dots = [(50.0, 50.0), (67.0, 36.0), (33.0, 36.0), (50.0, 90.0)];
        let sizes = [(20.0, 40.0), (1.0, 1.0), (1.0, 1.0), (1.0, 1.0)];
        let shown = Slot {
            x: 0.0,
            y: 10.0,
            w: 200.0,
            h: 200.0,
        };
        let placed = place(&dots, &sizes, 5.0, 2.0, shown);
        assert_eq!(
            placed[0],
            Some(Slot {
                x: 57.0,
                y: 45.0,
                w: 20.0,
                h: 40.0
            })
        );
    }

    type Three = [(f32, f32); 3];

    /// A dot hemmed in by two others and by the frame's left edge, so that
    /// no spot beside it is free.
    fn hemmed() -> (Three, Three, Slot) {
        (
            [(100.0, 100.0), (114.0, 91.0), (114.0, 109.0)],
            [(20.0, 10.0); 3],
            Slot {
                x: 90.0,
                y: 80.0,
                w: 100.0,
                h: 40.0,
            },
        )
    }

    #[test]
    fn a_crowded_label_is_hidden_without_a_reach_to_try() {
        let (dots, sizes, shown) = hemmed();
        assert_eq!(place(&dots, &sizes, 5.0, 2.0, shown)[0], None);
    }

    #[test]
    fn a_crowded_label_is_set_apart_on_a_leader() {
        let (dots, sizes, shown) = hemmed();
        let placed = super::place(&dots, &sizes, 5.0, 2.0, shown, &[12.0]);
        assert_eq!(
            placed[0],
            Some(Placed {
                slot: Slot {
                    x: 119.0,
                    y: 95.0,
                    w: 20.0,
                    h: 10.0
                },
                leader: Some((119.0, 100.0)),
            })
        );
    }

    #[test]
    fn a_leader_never_runs_through_another_dot() {
        // The only free spot lies straight past the second dot.
        let dots = [(100.0, 100.0), (112.0, 100.0)];
        let sizes = [(20.0, 10.0); 2];
        let shown = Slot {
            x: 90.0,
            y: 93.0,
            w: 100.0,
            h: 14.0,
        };
        let placed = super::place(&dots, &sizes, 5.0, 2.0, shown, &[12.0]);
        assert_eq!(placed[0], None);
    }

    #[test]
    fn a_later_label_keeps_off_an_earlier_leader() {
        let (dots, sizes, shown) = hemmed();
        let placed = super::place(&dots, &sizes, 5.0, 2.0, shown, &[12.0]);
        let from = dots[0];
        let to = placed[0].and_then(|placed| placed.leader).unwrap();
        for later in placed[1..].iter().flatten() {
            assert!(!crosses(from, to, &later.slot));
        }
    }

    #[test]
    fn a_line_crosses_what_it_passes_through_and_not_what_it_touches() {
        let slot = Slot {
            x: 10.0,
            y: 10.0,
            w: 10.0,
            h: 10.0,
        };
        assert!(crosses((0.0, 15.0), (30.0, 15.0), &slot));
        assert!(crosses((15.0, 15.0), (30.0, 30.0), &slot));
        assert!(!crosses((0.0, 10.0), (30.0, 10.0), &slot));
        assert!(!crosses((0.0, 15.0), (10.0, 15.0), &slot));
        assert!(!crosses((0.0, 0.0), (5.0, 30.0), &slot));
    }

    fn frame(w: f32, h: f32) -> Slot {
        Slot {
            x: 0.0,
            y: 0.0,
            w,
            h,
        }
    }

    #[test]
    fn a_lone_dot_goes_right() {
        let placed = place(
            &[(50.0, 50.0)],
            &[(20.0, 10.0)],
            5.0,
            2.0,
            frame(200.0, 200.0),
        );
        assert_eq!(
            placed,
            [Some(Slot {
                x: 57.0,
                y: 45.0,
                w: 20.0,
                h: 10.0
            })]
        );
    }

    #[test]
    fn a_dot_near_the_right_edge_goes_left() {
        let placed = place(
            &[(190.0, 50.0)],
            &[(20.0, 10.0)],
            5.0,
            2.0,
            frame(200.0, 100.0),
        );
        assert_eq!(
            placed,
            [Some(Slot {
                x: 163.0,
                y: 45.0,
                w: 20.0,
                h: 10.0
            })]
        );
    }

    #[test]
    fn a_second_dot_falls_through_to_below() {
        let dots = [(50.0, 50.0), (90.0, 50.0)];
        let sizes = [(20.0, 10.0), (20.0, 10.0)];
        let placed = place(&dots, &sizes, 5.0, 2.0, frame(110.0, 200.0));
        assert_eq!(
            placed,
            [
                Some(Slot {
                    x: 57.0,
                    y: 45.0,
                    w: 20.0,
                    h: 10.0
                }),
                Some(Slot {
                    x: 80.0,
                    y: 57.0,
                    w: 20.0,
                    h: 10.0
                }),
            ]
        );
    }

    #[test]
    fn a_dot_boxed_in_on_all_sides_yields_none() {
        let placed = place(&[(5.0, 5.0)], &[(2.0, 2.0)], 4.0, 1.0, frame(10.0, 10.0));
        assert_eq!(placed, [None]);
    }

    #[test]
    fn a_label_never_overlaps_another_dots_disc() {
        let dots = [(30.0, 30.0), (45.0, 30.0), (30.0, 60.0), (60.0, 45.0)];
        let sizes = [(20.0, 10.0); 4];
        let radius = 5.0;
        let placed = place(&dots, &sizes, radius, 2.0, frame(200.0, 200.0));

        let discs: Vec<Slot> = dots
            .iter()
            .map(|&(x, y)| Slot {
                x: x - radius,
                y: y - radius,
                w: radius * 2.0,
                h: radius * 2.0,
            })
            .collect();

        for slot in placed.into_iter().flatten() {
            for disc in &discs {
                assert!(!overlaps(&slot, disc));
            }
        }
    }

    #[test]
    fn output_length_equals_input_length() {
        let dots = [(10.0, 10.0), (20.0, 20.0), (30.0, 30.0)];
        let sizes = [(5.0, 5.0); 3];
        assert_eq!(place(&dots, &sizes, 2.0, 1.0, frame(100.0, 100.0)).len(), 3);
    }

    #[test]
    fn a_label_slides_inside_a_frame_too_narrow_beside_its_dot() {
        // No side has room as it stands; below does once slid off the edge.
        let placed = place(
            &[(20.0, 15.0)],
            &[(60.0, 10.0)],
            5.0,
            2.0,
            frame(70.0, 30.0),
        );
        assert_eq!(
            placed,
            [Some(Slot {
                x: 0.0,
                y: 20.0,
                w: 60.0,
                h: 10.0
            })]
        );
    }

    #[test]
    fn keeps_to_the_frame_rather_than_the_map() {
        let shown = Slot {
            x: 100.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
        };
        let placed = place(&[(180.0, 50.0)], &[(30.0, 10.0)], 5.0, 2.0, shown);
        assert_eq!(placed[0].map(|slot| slot.x), Some(143.0));
    }

    #[test]
    fn a_dot_outside_the_frame_is_left_unlabelled() {
        let shown = Slot {
            x: 100.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
        };
        assert_eq!(
            place(&[(95.0, 50.0)], &[(30.0, 10.0)], 5.0, 2.0, shown),
            [None]
        );
    }

    #[test]
    fn empty_input_yields_empty_output() {
        assert_eq!(place(&[], &[], 5.0, 2.0, frame(100.0, 100.0)), Vec::new());
    }
}
