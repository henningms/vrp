use super::*;
use crate::format::problem::{Plan, Problem};
use crate::helpers::*;
use vrp_core::models::problem::FerryDirection;

#[test]
fn resolves_quays_to_matrix_indices_via_coord_index() {
    let crossing = create_ferry_crossing("crossing1", (2., 0.), (3., 0.));
    let problem = Problem {
        plan: Plan { jobs: vec![create_delivery_job("job1", (1., 0.))], ..create_empty_plan() },
        fleet: create_default_fleet(),
        ferry_crossings: Some(vec![crossing.clone()]),
        ..create_empty_problem()
    };
    let coord_index = CoordIndex::new(&problem);

    let ferry_index = create_ferry_index(&[crossing], &coord_index).unwrap();

    assert_eq!(ferry_index.crossings().len(), 1);
    let resolved = &ferry_index.crossings()[0];
    assert_eq!(resolved.id, "crossing1");
    assert_eq!(resolved.quay_a, coord_index.get_by_loc(&(2., 0.).to_loc()).unwrap());
    assert_eq!(resolved.quay_b, coord_index.get_by_loc(&(3., 0.).to_loc()).unwrap());
    assert_eq!(resolved.crossing_sec, 300.);
    assert_eq!(resolved.boarding_buffer_sec, 600.);
}

#[test]
fn sorts_sailings_by_departure_ascending() {
    let mut crossing = create_ferry_crossing("crossing1", (2., 0.), (3., 0.));
    crossing.sailings.a_to_b = vec![
        FerrySailing { dep: 3600., arr: 4500. },
        FerrySailing { dep: 0., arr: 900. },
        FerrySailing { dep: 1800., arr: 2700. },
    ];
    crossing.sailings.b_to_a = vec![
        FerrySailing { dep: 5400., arr: 6300. },
        FerrySailing { dep: 900., arr: 1800. },
        FerrySailing { dep: 2700., arr: 3600. },
    ];
    let problem = Problem {
        fleet: create_default_fleet(),
        ferry_crossings: Some(vec![crossing.clone()]),
        ..create_empty_problem()
    };
    let coord_index = CoordIndex::new(&problem);

    let ferry_index = create_ferry_index(&[crossing], &coord_index).unwrap();
    let resolved = &ferry_index.crossings()[0];

    let a_to_b_deps = resolved.sailings(FerryDirection::AToB).iter().map(|sailing| sailing.dep).collect::<Vec<_>>();
    assert_eq!(a_to_b_deps, vec![0., 1800., 3600.]);

    let b_to_a_deps = resolved.sailings(FerryDirection::BToA).iter().map(|sailing| sailing.dep).collect::<Vec<_>>();
    assert_eq!(b_to_a_deps, vec![900., 2700., 5400.]);
}

#[test]
fn returns_format_error_when_quay_does_not_resolve() {
    let crossing = create_ferry_crossing("crossing1", (2., 0.), (3., 0.));
    // coord_index built from a problem that never saw this crossing: its quays are unknown to it,
    // mirroring the real bug this guards against (coordinate set and index disagreeing).
    let coord_index = CoordIndex::new(&create_empty_problem());

    let error = create_ferry_index(&[crossing], &coord_index).expect_err("expected a format error");

    assert_eq!(error.errors.first().map(|err| err.code.as_str()), Some("E9001"));
}
