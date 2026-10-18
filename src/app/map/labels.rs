//! Greedy label placement: each dot, in order, claims the first spot that
//! fits, so earlier dots win rather than the layout being solved globally.

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

fn overlaps(a: &Slot, b: &Slot) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

fn within(slot: &Slot, frame: (f32, f32)) -> bool {
    slot.x >= 0.0 && slot.y >= 0.0 && slot.right() <= frame.0 && slot.bottom() <= frame.1
}

/// Tries right, left, below, then above each dot, and gives up on a label
/// (`None`) rather than let it leave `frame` or overlap another label or any
/// dot, whose disc counts as its bounding square.
pub(super) fn place(
    dots: &[(f32, f32)],
    sizes: &[(f32, f32)],
    radius: f32,
    gap: f32,
    frame: (f32, f32),
) -> Vec<Option<Slot>> {
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
    let mut out = Vec::with_capacity(dots.len());

    for (&(cx, cy), &(w, h)) in dots.iter().zip(sizes) {
        let candidates = [
            Slot {
                x: cx + radius + gap,
                y: cy - h / 2.0,
                w,
                h,
            },
            Slot {
                x: cx - radius - gap - w,
                y: cy - h / 2.0,
                w,
                h,
            },
            Slot {
                x: cx - w / 2.0,
                y: cy + radius + gap,
                w,
                h,
            },
            Slot {
                x: cx - w / 2.0,
                y: cy - radius - gap - h,
                w,
                h,
            },
        ];

        let slot = candidates.into_iter().find(|candidate| {
            within(candidate, frame)
                && !placed.iter().any(|p| overlaps(candidate, p))
                && !discs.iter().any(|d| overlaps(candidate, d))
        });

        if let Some(slot) = slot {
            placed.push(slot);
        }
        out.push(slot);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lone_dot_goes_right() {
        let placed = place(&[(50.0, 50.0)], &[(20.0, 10.0)], 5.0, 2.0, (200.0, 200.0));
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
        let placed = place(&[(190.0, 50.0)], &[(20.0, 10.0)], 5.0, 2.0, (200.0, 100.0));
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
        let placed = place(&dots, &sizes, 5.0, 2.0, (110.0, 200.0));
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
        let placed = place(&[(5.0, 5.0)], &[(2.0, 2.0)], 4.0, 1.0, (10.0, 10.0));
        assert_eq!(placed, [None]);
    }

    #[test]
    fn a_label_never_overlaps_another_dots_disc() {
        let dots = [(30.0, 30.0), (45.0, 30.0), (30.0, 60.0), (60.0, 45.0)];
        let sizes = [(20.0, 10.0); 4];
        let radius = 5.0;
        let placed = place(&dots, &sizes, radius, 2.0, (200.0, 200.0));

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
        assert_eq!(place(&dots, &sizes, 2.0, 1.0, (100.0, 100.0)).len(), 3);
    }

    #[test]
    fn empty_input_yields_empty_output() {
        assert_eq!(place(&[], &[], 5.0, 2.0, (100.0, 100.0)), Vec::new());
    }
}
