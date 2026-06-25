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
    let index = CoordIndex::new_with_extra_locations(&problem, &[candidate_loc.clone()]);
    assert_eq!(index.get_by_loc(&candidate_loc), Some(problem_coord_count));
    assert_eq!(index.max_matrix_index(), problem_coord_count);

    // A coordinate already present in the problem keeps its original index
    // (dedup), so passing it as an extra does not create a phantom column.
    let existing = (1., 0.).to_loc();
    let plain_existing = CoordIndex::new(&problem).get_by_loc(&existing);
    let dedup = CoordIndex::new_with_extra_locations(&problem, &[existing.clone()]);
    assert_eq!(dedup.get_by_loc(&existing), plain_existing);
    assert_eq!(dedup.max_matrix_index() + 1, problem_coord_count);
}
