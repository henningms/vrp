use super::*;
use crate::format::problem::*;
use crate::helpers::*;

#[test]
fn can_use_index_with_coordinate_an_unknown_location_types() {
    let unknown_location = Location::Custom { r#type: CustomLocationType::Unknown };
    let problem = Problem {
        plan: Plan {
            jobs: vec![
                create_delivery_job("job1", (1., 0.)),
                create_delivery_job("job2", (2., 0.)),
                Job {
                    deliveries: Some(vec![JobTask {
                        places: vec![JobPlace {
                            location: unknown_location.clone(),
                            duration: 0.,
                            times: None,
                            tag: None,
                            requested_time: None,
                        }],
                        demand: None,
                        named_demand: None,
                        order: None,
                    }]),
                    ..create_job("job3")
                },
            ],
            ..create_empty_plan()
        },
        fleet: create_default_fleet(),
        ..create_empty_problem()
    };

    let index = CoordIndex::new(&problem);

    assert!(index.has_coordinates());
    assert!(index.has_custom());
    assert!(!index.has_indices());
    assert_eq!(index.max_matrix_index(), 2);
    assert_eq!(index.custom_locations_len(), 1);
    // Location::Coordinate type
    assert_eq!(index.get_by_loc(&(1., 0.).to_loc()), Some(0));
    assert_eq!(index.get_by_loc(&(2., 0.).to_loc()), Some(1));
    assert_eq!(index.get_by_loc(&(0., 0.).to_loc()), Some(2));
    assert_eq!(index.get_by_idx(0), Some((1., 0.).to_loc()));
    assert_eq!(index.get_by_idx(1), Some((2., 0.).to_loc()));
    assert_eq!(index.get_by_idx(2), Some((0., 0.).to_loc()));
    assert!(!index.is_special_index(0));
    assert!(!index.is_special_index(1));
    assert!(!index.is_special_index(2));
    // Location::Custom
    assert_eq!(index.get_by_loc(&unknown_location), Some(9));
    assert_eq!(index.get_by_idx(9), Some(unknown_location));
    assert!(index.is_special_index(9));
    // out of range
    assert_eq!(index.get_by_loc(&(3., 0.).to_loc()), None);
    assert_eq!(index.get_by_idx(3), None);
    assert_eq!(index.get_by_idx(8), None);
    assert_eq!(index.get_by_idx(10), None);
    assert!(!index.is_special_index(3));
}

#[test]
fn new_with_extra_locations_registers_candidate_coordinates() {
    // Problem: one delivery job at (1,0); default fleet depot at (0,0).
    let problem = Problem {
        plan: Plan { jobs: vec![create_delivery_job("job1", (1., 0.))], ..create_empty_plan() },
        fleet: create_default_fleet(),
        ..create_empty_problem()
    };

    let problem_coord_count = CoordIndex::new(&problem).max_matrix_index() + 1;
    let candidate_loc = (5., 0.).to_loc();

    // Without registration the candidate coordinate is unknown — this is what
    // silently produced bogus infeasible verdicts for new booking coordinates.
    assert_eq!(CoordIndex::new(&problem).get_by_loc(&candidate_loc), None);

    // Registered as an extra location, it resolves to the next free index,
    // appended AFTER all problem coordinates, and grows the matrix dimension.
    let index = CoordIndex::new_with_extra_locations(&problem, std::slice::from_ref(&candidate_loc));
    assert_eq!(index.get_by_loc(&candidate_loc), Some(problem_coord_count));
    assert_eq!(index.max_matrix_index(), problem_coord_count);

    // A coordinate already present in the problem keeps its original index
    // (dedup), so passing it as an extra does not create a phantom column.
    let existing = (1., 0.).to_loc();
    let plain_existing = CoordIndex::new(&problem).get_by_loc(&existing);
    let dedup = CoordIndex::new_with_extra_locations(&problem, std::slice::from_ref(&existing));
    assert_eq!(dedup.get_by_loc(&existing), plain_existing);
    assert_eq!(dedup.max_matrix_index() + 1, problem_coord_count);
}

#[test]
fn ferry_quays_are_indexed_after_fleet_and_before_extra_locations() {
    // Problem: one job at (1,0); a fleet whose start (0,0), end (10,0) and reload (20,0) are all
    // distinct, so the fleet loop contributes three separate indices - pinning the ferry walk to
    // AFTER the whole fleet loop, not merely after `shift.start`. A crossing with two quays not
    // seen elsewhere; one extra (candidate) location.
    let problem = Problem {
        plan: Plan { jobs: vec![create_delivery_job("job1", (1., 0.))], ..create_empty_plan() },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                shifts: vec![VehicleShift {
                    reloads: Some(vec![VehicleReload { location: (20., 0.).to_loc(), ..create_default_reload() }]),
                    ..create_default_vehicle_shift_with_locations((0., 0.), (10., 0.))
                }],
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        ferry_crossings: Some(vec![create_ferry_crossing("crossing1", (30., 0.), (31., 0.))]),
        ..create_empty_problem()
    };
    let extra = (40., 0.).to_loc();

    let index = CoordIndex::new_with_extra_locations(&problem, std::slice::from_ref(&extra));

    // This exact ordering is mirrored by a separate Rust service building the routing matrix;
    // a divergence here would silently permute every travel time.
    assert_eq!(index.get_by_loc(&(1., 0.).to_loc()), Some(0), "plan job comes first");
    assert_eq!(index.get_by_loc(&(0., 0.).to_loc()), Some(1), "fleet start comes after plan");
    assert_eq!(index.get_by_loc(&(10., 0.).to_loc()), Some(2), "fleet end comes after fleet start");
    assert_eq!(index.get_by_loc(&(20., 0.).to_loc()), Some(3), "fleet reload comes after fleet end");
    assert_eq!(index.get_by_loc(&(30., 0.).to_loc()), Some(4), "quay A comes after the whole fleet loop");
    assert_eq!(index.get_by_loc(&(31., 0.).to_loc()), Some(5), "quay B comes after quay A");
    assert_eq!(index.get_by_loc(&extra), Some(6), "extra locations come after ferry quays");
}

#[test]
fn ferry_quay_coinciding_with_existing_location_adds_no_new_index_entries() {
    // Both quays coincide with locations already in the problem (job and fleet depot).
    let problem = Problem {
        plan: Plan { jobs: vec![create_delivery_job("job1", (1., 0.))], ..create_empty_plan() },
        fleet: create_default_fleet(),
        ferry_crossings: Some(vec![create_ferry_crossing("crossing1", (1., 0.), (0., 0.))]),
        ..create_empty_problem()
    };

    let index = CoordIndex::new(&problem);

    assert_eq!(index.get_by_loc(&(1., 0.).to_loc()), Some(0));
    assert_eq!(index.get_by_loc(&(0., 0.).to_loc()), Some(1));
    assert_eq!(index.max_matrix_index(), 1, "no extra index entries were created for the coinciding quays");
}
