use slvs::constraint::{
    Angle, ArcLineTangent, AtMidpoint, Diameter, EqPtLnDistances, EqualAngle, EqualLengthLines,
    EqualRadius, Horizontal, LengthRatio, Parallel, Perpendicular, PointsCoincident,
    PtLineDistance, PtOnCircle, PtOnLine, PtPtDistance, SymmetricHoriz, SymmetricLine,
    SymmetricVert, Vertical, WhereDragged,
};

use crate::entity_mapping::SketchToSlvs;
use crate::types::{EntityKind, SketchConstraint};

impl SketchToSlvs {
    /// Add all sketch constraints to the slvs system.
    pub fn add_constraints(&mut self, constraints: &[SketchConstraint]) {
        for constraint in constraints {
            self.add_constraint(constraint);
        }
    }

    fn add_constraint(&mut self, constraint: &SketchConstraint) {
        match constraint {
            SketchConstraint::Coincident { point_a, point_b } => {
                let pa = self.point_handles[point_a];
                let pb = self.point_handles[point_b];
                self.system
                    .constrain(PointsCoincident::new(
                        self.group,
                        pa,
                        pb,
                        Some(self.workplane),
                    ))
                    .expect("failed to add coincident constraint");
            }

            SketchConstraint::Horizontal { entity } => {
                let line = self.line_handles[entity];
                self.system
                    .constrain(Horizontal::from_line(self.group, self.workplane, line))
                    .expect("failed to add horizontal constraint");
            }

            SketchConstraint::Vertical { entity } => {
                let line = self.line_handles[entity];
                self.system
                    .constrain(Vertical::from_line(self.group, self.workplane, line))
                    .expect("failed to add vertical constraint");
            }

            SketchConstraint::Parallel { line_a, line_b } => {
                let la = self.line_handles[line_a];
                let lb = self.line_handles[line_b];
                self.system
                    .constrain(Parallel::new(self.group, la, lb, Some(self.workplane)))
                    .expect("failed to add parallel constraint");
            }

            SketchConstraint::Perpendicular { line_a, line_b } => {
                let la = self.line_handles[line_a];
                let lb = self.line_handles[line_b];
                self.system
                    .constrain(Perpendicular::new(self.group, la, lb, Some(self.workplane)))
                    .expect("failed to add perpendicular constraint");
            }

            SketchConstraint::Tangent { line, curve } => {
                let curve_kind = self.entity_types[curve];
                let line_handle = self.line_handles[line];
                match curve_kind {
                    EntityKind::Arc => {
                        let arc = self.arc_handles[curve];
                        self.system
                            .constrain(ArcLineTangent::new(
                                self.group,
                                self.workplane,
                                arc,
                                line_handle,
                                false,
                            ))
                            .expect("failed to add arc-line tangent constraint");
                    }
                    EntityKind::Circle => {
                        // Circle-line tangent: use CurveCurveTangent is not appropriate.
                        // slvs doesn't have a direct circle-line tangent. Use PtOnCircle
                        // combined with perpendicular, or use the distance approach.
                        // For now, we'll handle this by noting it's unsupported for circles
                        // without arc endpoints. Tangent is primarily used with arcs.
                        panic!("Tangent constraint between line and circle is not directly supported; use arc endpoints instead");
                    }
                    _ => panic!(
                        "Tangent curve must be an arc or circle, got {:?}",
                        curve_kind
                    ),
                }
            }

            SketchConstraint::Equal { entity_a, entity_b } => {
                let kind_a = self.entity_types[entity_a];
                let kind_b = self.entity_types[entity_b];
                match (kind_a, kind_b) {
                    (EntityKind::Line, EntityKind::Line) => {
                        let la = self.line_handles[entity_a];
                        let lb = self.line_handles[entity_b];
                        self.system
                            .constrain(EqualLengthLines::new(
                                self.group,
                                la,
                                lb,
                                Some(self.workplane),
                            ))
                            .expect("failed to add equal length constraint");
                    }
                    (EntityKind::Circle, EntityKind::Circle) => {
                        let ca = self.circle_handles[entity_a];
                        let cb = self.circle_handles[entity_b];
                        self.system
                            .constrain(EqualRadius::new(self.group, ca, cb))
                            .expect("failed to add equal radius constraint");
                    }
                    (EntityKind::Arc, EntityKind::Arc) => {
                        let aa = self.arc_handles[entity_a];
                        let ab = self.arc_handles[entity_b];
                        self.system
                            .constrain(EqualRadius::new(self.group, aa, ab))
                            .expect("failed to add equal radius constraint");
                    }
                    (EntityKind::Circle, EntityKind::Arc) => {
                        let ca = self.circle_handles[entity_a];
                        let ab = self.arc_handles[entity_b];
                        self.system
                            .constrain(EqualRadius::new(self.group, ca, ab))
                            .expect("failed to add equal radius constraint");
                    }
                    (EntityKind::Arc, EntityKind::Circle) => {
                        let aa = self.arc_handles[entity_a];
                        let cb = self.circle_handles[entity_b];
                        self.system
                            .constrain(EqualRadius::new(self.group, aa, cb))
                            .expect("failed to add equal radius constraint");
                    }
                    _ => panic!(
                        "Equal constraint not supported between {:?} and {:?}",
                        kind_a, kind_b
                    ),
                }
            }

            SketchConstraint::Symmetric {
                entity_a,
                entity_b,
                symmetry_line,
            } => {
                let pa = self.point_handles[entity_a];
                let pb = self.point_handles[entity_b];
                let line = self.line_handles[symmetry_line];
                self.system
                    .constrain(SymmetricLine::new(self.group, self.workplane, pa, pb, line))
                    .expect("failed to add symmetric constraint");
            }

            SketchConstraint::SymmetricH { point_a, point_b } => {
                let pa = self.point_handles[point_a];
                let pb = self.point_handles[point_b];
                self.system
                    .constrain(SymmetricHoriz::new(self.group, self.workplane, pa, pb))
                    .expect("failed to add symmetric horizontal constraint");
            }

            SketchConstraint::SymmetricV { point_a, point_b } => {
                let pa = self.point_handles[point_a];
                let pb = self.point_handles[point_b];
                self.system
                    .constrain(SymmetricVert::new(self.group, self.workplane, pa, pb))
                    .expect("failed to add symmetric vertical constraint");
            }

            SketchConstraint::Midpoint { point, line } => {
                let pt = self.point_handles[point];
                let ln = self.line_handles[line];
                self.system
                    .constrain(AtMidpoint::new(self.group, pt, ln, Some(self.workplane)))
                    .expect("failed to add midpoint constraint");
            }

            SketchConstraint::Distance {
                entity_a,
                entity_b,
                value,
            } => {
                let kind_a = self.entity_types[entity_a];
                let kind_b = self.entity_types[entity_b];
                match (kind_a, kind_b) {
                    (EntityKind::Point, EntityKind::Point) => {
                        let pa = self.point_handles[entity_a];
                        let pb = self.point_handles[entity_b];
                        self.system
                            .constrain(PtPtDistance::new(
                                self.group,
                                pa,
                                pb,
                                *value,
                                Some(self.workplane),
                            ))
                            .expect("failed to add pt-pt distance constraint");
                    }
                    (EntityKind::Point, EntityKind::Line) => {
                        let pt = self.point_handles[entity_a];
                        let ln = self.line_handles[entity_b];
                        self.system
                            .constrain(PtLineDistance::new(
                                self.group,
                                pt,
                                ln,
                                *value,
                                Some(self.workplane),
                            ))
                            .expect("failed to add pt-line distance constraint");
                    }
                    (EntityKind::Line, EntityKind::Point) => {
                        // Swap: treat as point-to-line distance
                        let pt = self.point_handles[entity_b];
                        let ln = self.line_handles[entity_a];
                        self.system
                            .constrain(PtLineDistance::new(
                                self.group,
                                pt,
                                ln,
                                *value,
                                Some(self.workplane),
                            ))
                            .expect("failed to add pt-line distance constraint");
                    }
                    _ => panic!(
                        "Distance constraint not supported between {:?} and {:?}",
                        kind_a, kind_b
                    ),
                }
            }

            SketchConstraint::Angle {
                line_a,
                line_b,
                value_degrees,
            } => {
                let la = self.line_handles[line_a];
                let lb = self.line_handles[line_b];
                self.system
                    .constrain(Angle::new(
                        self.group,
                        la,
                        lb,
                        *value_degrees,
                        Some(self.workplane),
                        false,
                    ))
                    .expect("failed to add angle constraint");
            }

            SketchConstraint::Radius { entity, value } => {
                let kind = self.entity_types[entity];
                let diameter = *value * 2.0;
                match kind {
                    EntityKind::Circle => {
                        let c = self.circle_handles[entity];
                        self.system
                            .constrain(Diameter::new(self.group, c, diameter))
                            .expect("failed to add radius constraint");
                    }
                    EntityKind::Arc => {
                        let a = self.arc_handles[entity];
                        self.system
                            .constrain(Diameter::new(self.group, a, diameter))
                            .expect("failed to add radius constraint");
                    }
                    _ => panic!("Radius constraint requires circle or arc, got {:?}", kind),
                }
            }

            SketchConstraint::Diameter { entity, value } => {
                let kind = self.entity_types[entity];
                match kind {
                    EntityKind::Circle => {
                        let c = self.circle_handles[entity];
                        self.system
                            .constrain(Diameter::new(self.group, c, *value))
                            .expect("failed to add diameter constraint");
                    }
                    EntityKind::Arc => {
                        let a = self.arc_handles[entity];
                        self.system
                            .constrain(Diameter::new(self.group, a, *value))
                            .expect("failed to add diameter constraint");
                    }
                    _ => panic!("Diameter constraint requires circle or arc, got {:?}", kind),
                }
            }

            SketchConstraint::OnEntity { point, entity } => {
                let pt = self.point_handles[point];
                let kind = self.entity_types[entity];
                match kind {
                    EntityKind::Line => {
                        let ln = self.line_handles[entity];
                        self.system
                            .constrain(PtOnLine::new(self.group, pt, ln, Some(self.workplane)))
                            .expect("failed to add point-on-line constraint");
                    }
                    EntityKind::Circle => {
                        let c = self.circle_handles[entity];
                        self.system
                            .constrain(PtOnCircle::new(self.group, pt, c))
                            .expect("failed to add point-on-circle constraint");
                    }
                    EntityKind::Arc => {
                        let a = self.arc_handles[entity];
                        self.system
                            .constrain(PtOnCircle::new(self.group, pt, a))
                            .expect("failed to add point-on-arc constraint");
                    }
                    _ => panic!(
                        "OnEntity target must be line, circle, or arc, got {:?}",
                        kind
                    ),
                }
            }

            SketchConstraint::Dragged { point } => {
                let pt = self.point_handles[point];
                self.system
                    .constrain(WhereDragged::new(self.group, pt, Some(self.workplane)))
                    .expect("failed to add dragged constraint");
            }

            SketchConstraint::EqualAngle {
                line_a,
                line_b,
                line_c,
                line_d,
            } => {
                let la = self.line_handles[line_a];
                let lb = self.line_handles[line_b];
                let lc = self.line_handles[line_c];
                let ld = self.line_handles[line_d];
                self.system
                    .constrain(EqualAngle::new(
                        self.group,
                        la,
                        lb,
                        lc,
                        ld,
                        Some(self.workplane),
                        false,
                    ))
                    .expect("failed to add equal angle constraint");
            }

            SketchConstraint::Ratio {
                entity_a,
                entity_b,
                value,
            } => {
                let la = self.line_handles[entity_a];
                let lb = self.line_handles[entity_b];
                self.system
                    .constrain(LengthRatio::new(
                        self.group,
                        la,
                        lb,
                        *value,
                        Some(self.workplane),
                    ))
                    .expect("failed to add length ratio constraint");
            }

            SketchConstraint::EqualPointToLine {
                point_a,
                point_b,
                line,
            } => {
                let pa = self.point_handles[point_a];
                let pb = self.point_handles[point_b];
                let ln = self.line_handles[line];
                // EqPtLnDistances: dist(point_a, line_a) == dist(point_b, line_b)
                // Our interface has one line, so use it for both
                self.system
                    .constrain(EqPtLnDistances::new(
                        self.group,
                        ln,
                        pa,
                        ln,
                        pb,
                        Some(self.workplane),
                    ))
                    .expect("failed to add equal point-to-line distance constraint");
            }

            SketchConstraint::SameOrientation { .. } => {
                // SameOrientation operates on Normal entities, which are not
                // directly exposed in the sketch entity model. This constraint
                // is reserved for 3D normal alignment. Skip in 2D sketch context.
            }
        }
    }
}
