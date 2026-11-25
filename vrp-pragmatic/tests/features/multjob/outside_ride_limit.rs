use crate::format::problem::*;
use crate::helpers::*;

/// A companion boards at (1, 0), the rider at (5, 0), and both are set down at (7, 0). The rider
/// rides 2; from the companion's pickup the ride is 7.
fn create_companion_problem(companion_outside_ride_limit: bool) -> Problem {
    let mut job = create_multi_job(
        "rider",
        vec![((1., 0.), 1., vec![1]), ((5., 0.), 1., vec![1])],
        vec![((7., 0.), 1., vec![2])],
    );
    job.max_ride_duration = Some(5.);
    job.fixed_order = Some(true);
    if companion_outside_ride_limit {
        job.pickups.as_mut().unwrap()[0].outside_ride_limit = Some(true);
    }

    Problem {
        plan: Plan { jobs: vec![job], ..create_empty_plan() },
        fleet: create_default_fleet(),
        ..create_empty_problem()
    }
}

parameterized_test! {can_leave_a_task_out_of_the_ride_limit, (companion_outside_ride_limit, is_assigned), {
    can_leave_a_task_out_of_the_ride_limit_impl(companion_outside_ride_limit, is_assigned);
}}

can_leave_a_task_out_of_the_ride_limit! {
    case01_whole_job_measured: (false, false),
    case02_companion_pickup_outside: (true, true),
}

fn can_leave_a_task_out_of_the_ride_limit_impl(companion_outside_ride_limit: bool, is_assigned: bool) {
    let problem = create_companion_problem(companion_outside_ride_limit);
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert_eq!(solution.unassigned.is_none(), is_assigned);
}
