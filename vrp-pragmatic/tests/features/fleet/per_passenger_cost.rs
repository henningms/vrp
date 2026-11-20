use crate::format::problem::PragmaticProblem;
use crate::format::problem::*;
use crate::format::solution::*;
use crate::format_time;
use crate::helpers::*;

fn create_vehicle_type(type_id: &str, per_run: Option<f64>, per_passenger: Option<f64>) -> VehicleType {
    VehicleType {
        type_id: type_id.to_string(),
        vehicle_ids: vec![format!("{type_id}_1")],
        costs: VehicleCosts { per_run, per_passenger, ..create_default_vehicle_costs() },
        shifts: vec![create_default_open_vehicle_shift()],
        ..create_default_vehicle_type()
    }
}

fn create_problem(jobs: Vec<Job>, vehicles: Vec<VehicleType>) -> Problem {
    Problem {
        plan: Plan { jobs, ..create_empty_plan() },
        fleet: Fleet { vehicles, ..create_default_fleet() },
        ..create_empty_problem()
    }
}

fn solve(problem: Problem) -> Solution {
    let matrix = create_matrix_from_problem(&problem);
    solve_with_metaheuristic(problem, Some(vec![matrix]))
}

/// The default vehicle has a fixed cost of 10 and costs 1 per distance and time unit.
fn get_base_cost(tour: &Tour) -> f64 {
    10. + tour.statistic.distance as f64 + tour.statistic.duration as f64
}

fn get_tour<'a>(solution: &'a Solution, job_id: &str) -> &'a Tour {
    solution
        .tours
        .iter()
        .find(|tour| tour.stops.iter().flat_map(|stop| stop.activities()).any(|activity| activity.job_id == job_id))
        .expect("job is not assigned")
}

/// Two riders on one line who ride together: one from 1 to 5, the other from 2 to 4.
fn create_riders() -> Vec<Job> {
    vec![
        create_pickup_delivery_job("first", (1., 0.), (5., 0.)),
        create_pickup_delivery_job("second", (2., 0.), (4., 0.)),
    ]
}

#[test]
fn two_passengers_cost_twice_the_per_passenger_amount() {
    let solution = solve(create_problem(create_riders(), vec![create_vehicle_type("vehicle", None, Some(300.))]));

    assert!(solution.unassigned.is_none());
    assert_eq!(solution.tours.len(), 1);
    let tour = &solution.tours[0];
    assert_eq!(tour.statistic.cost, get_base_cost(tour) + 600.);
    assert_eq!(solution.statistic.cost, tour.statistic.cost);
}

#[test]
fn two_passengers_sharing_a_run_cost_the_per_run_amount_once() {
    let solution = solve(create_problem(create_riders(), vec![create_vehicle_type("vehicle", Some(300.), None)]));

    assert!(solution.unassigned.is_none());
    assert_eq!(solution.tours.len(), 1);
    let tour = &solution.tours[0];
    assert_eq!(tour.statistic.distance, 5);
    assert_eq!(tour.statistic.cost, get_base_cost(tour) + 300.);
    assert_eq!(solution.statistic.cost, tour.statistic.cost);
}

#[test]
fn charges_a_pickup_delivery_job_once_for_its_passengers() {
    let job = Job { passengers: Some(3), ..create_pickup_delivery_job("rider", (1., 0.), (5., 0.)) };

    let solution = solve(create_problem(vec![job], vec![create_vehicle_type("vehicle", None, Some(100.))]));

    assert!(solution.unassigned.is_none());
    let tour = &solution.tours[0];
    assert_eq!(tour.statistic.cost, get_base_cost(tour) + 300.);
    assert_eq!(solution.statistic.cost, tour.statistic.cost);
}

#[test]
fn prefers_the_vehicle_type_with_the_lower_per_passenger_amount() {
    let vehicles = vec![
        create_vehicle_type("expensive", None, Some(100.)),
        create_vehicle_type("cheap", None, Some(10.)),
        create_vehicle_type("also_expensive", None, Some(100.)),
    ];

    let solution = solve(create_problem(create_riders(), vehicles));

    assert!(solution.unassigned.is_none());
    assert_eq!(solution.tours.len(), 1);
    let tour = &solution.tours[0];
    assert_eq!(tour.type_id, "cheap");
    assert_eq!(tour.statistic.cost, get_base_cost(tour) + 20.);
}

#[test]
fn puts_the_job_with_more_passengers_where_the_total_is_lowest() {
    // every vehicle serves one job only and both jobs ride the same way, so only the fees tell the two
    // assignments apart
    let jobs = vec![
        Job { passengers: Some(4), ..create_pickup_delivery_job("group", (1., 0.), (5., 0.)) },
        create_pickup_delivery_job("single", (1., 0.), (5., 0.)),
    ];
    let one_job_only = || Some(VehicleLimits { max_distance: None, max_duration: None, tour_size: Some(2) });
    let vehicles = vec![
        VehicleType { limits: one_job_only(), ..create_vehicle_type("expensive", None, Some(100.)) },
        VehicleType { limits: one_job_only(), ..create_vehicle_type("cheap", None, Some(10.)) },
    ];

    let solution = solve(create_problem(jobs, vehicles));

    assert!(solution.unassigned.is_none());
    assert_eq!(solution.tours.len(), 2);
    let (group, single) = (get_tour(&solution, "group"), get_tour(&solution, "single"));
    assert_eq!(group.type_id, "cheap");
    assert_eq!(single.type_id, "expensive");
    assert_eq!(group.statistic.cost, get_base_cost(group) + 40.);
    assert_eq!(single.statistic.cost, get_base_cost(single) + 100.);
    assert_eq!(solution.statistic.cost, group.statistic.cost + single.statistic.cost);
}

#[test]
fn unassigned_job_is_charged_nothing() {
    let too_big = Job {
        passengers: Some(5),
        deliveries: Some(vec![JobTask { demand: Some(vec![100]), ..create_task((3., 0.), None) }]),
        ..create_job("too_big")
    };
    let jobs = vec![create_delivery_job("parcel", (1., 0.)), too_big];

    let solution = solve(create_problem(jobs, vec![create_vehicle_type("vehicle", None, Some(100.))]));

    assert_eq!(solution.unassigned.as_ref().map(|unassigned| unassigned.len()), Some(1));
    assert_eq!(solution.tours.len(), 1);
    let tour = &solution.tours[0];
    assert_eq!(tour.statistic.cost, get_base_cost(tour) + 100.);
    assert_eq!(solution.statistic.cost, tour.statistic.cost);
}

/// The problem of `can_handle_limited_capacity` whose solution costs 88.
fn create_limited_capacity_problem(passengers: Option<i32>, per_passenger: Option<f64>) -> Problem {
    let multi_job = |id: &str, pickups: [f64; 2], delivery: f64| Job {
        passengers,
        ..create_multi_job(
            id,
            vec![((pickups[0], 0.), 1., vec![1]), ((pickups[1], 0.), 1., vec![1])],
            vec![((delivery, 0.), 1., vec![2])],
        )
    };
    let vehicle = create_vehicle_with_capacity("my_vehicle", vec![2]);

    Problem {
        plan: Plan {
            jobs: vec![multi_job("multi_1", [1., 2.], 10.), multi_job("multi_2", [3., 4.], 11.)],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![VehicleType { costs: VehicleCosts { per_passenger, ..vehicle.costs.clone() }, ..vehicle }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    }
}

#[test]
fn without_per_passenger_cost_the_solution_is_unchanged() {
    let solve = |passengers, per_passenger| solve(create_limited_capacity_problem(passengers, per_passenger));
    let expected = Statistic {
        cost: 88.,
        distance: 36,
        duration: 42,
        times: Timing { driving: 36, serving: 6, ..Timing::default() },
    };

    assert_eq!(solve(None, None).statistic, expected);
    assert_eq!(solve(Some(2), None).statistic, expected);
    assert_eq!(solve(Some(2), Some(0.)).statistic, expected);
    assert_eq!(solve(Some(2), Some(100.)).statistic, Statistic { cost: 88. + 400., ..expected });
}

#[test]
fn per_passenger_cost_and_passengers_are_read_and_written() {
    let costs: VehicleCosts =
        serde_json::from_str(r#"{"distance": 1, "time": 1, "perPassenger": 50}"#).expect("cannot read costs");
    assert_eq!(costs.per_passenger, Some(50.));
    assert!(serde_json::to_string(&costs).unwrap().contains(r#""perPassenger":50.0"#));

    let costs: VehicleCosts = serde_json::from_str(r#"{"distance": 1, "time": 1}"#).expect("cannot read costs");
    assert_eq!(costs.per_passenger, None);
    assert!(!serde_json::to_string(&costs).unwrap().contains("perPassenger"));

    let job: Job = serde_json::from_str(
        r#"{"id": "job", "passengers": 3, "services": [{"places": [{"location": {"index": 0}, "duration": 1}]}]}"#,
    )
    .expect("cannot read job");
    assert_eq!(job.passengers, Some(3));
    assert!(serde_json::to_string(&job).unwrap().contains(r#""passengers":3"#));
}

#[test]
fn rejects_invalid_passengers_and_per_passenger_cost_when_reading() {
    let read_error_codes = |passengers: Option<i32>, per_passenger: Option<f64>| {
        let job = Job { passengers, ..create_pickup_delivery_job("rider", (1., 0.), (5., 0.)) };
        let problem = create_problem(vec![job], vec![create_vehicle_type("vehicle", None, per_passenger)]);
        let matrix = create_matrix_from_problem(&problem);

        match (problem, vec![matrix]).read_pragmatic() {
            Ok(_) => vec![],
            Err(errors) => errors.errors.into_iter().map(|error| error.code).collect::<Vec<_>>(),
        }
    };

    assert_eq!(read_error_codes(Some(1), Some(100.)), Vec::<String>::new());
    assert_eq!(read_error_codes(Some(0), Some(100.)), vec!["E9108".to_string()]);
    assert_eq!(read_error_codes(Some(-1), Some(100.)), vec!["E9108".to_string()]);
    assert_eq!(read_error_codes(Some(1), Some(-100.)), vec!["E9308".to_string()]);
}

fn get_activity_ids(tour: &Tour) -> Vec<Vec<String>> {
    tour.stops.iter().map(|stop| stop.activities().iter().map(|activity| activity.job_id.clone()).collect()).collect()
}

#[test]
fn charges_the_passengers_of_every_clustered_job() {
    let solve_clustered = |per_passenger| {
        let jobs = vec![
            Job { passengers: Some(1), ..create_delivery_job("first", (1., 0.)) },
            Job { passengers: Some(2), ..create_delivery_job("second", (2., 0.)) },
        ];
        let problem = create_problem(jobs, vec![create_vehicle_type("vehicle", None, per_passenger)]);
        let clustering = Clustering::Vicinity {
            profile: VehicleProfile { matrix: "car".to_string(), scale: None },
            threshold: VicinityThresholdPolicy {
                duration: 3.,
                distance: 3.,
                min_shared_time: None,
                smallest_time_window: None,
                max_jobs_per_cluster: None,
            },
            visiting: VicinityVisitPolicy::Continue,
            serving: VicinityServingPolicy::Original { parking: 0. },
            filtering: None,
        };

        solve(Problem { plan: Plan { clustering: Some(clustering), ..problem.plan }, ..problem })
    };

    let (with_fee, without_fee) = (solve_clustered(Some(100.)), solve_clustered(None));

    for solution in [&with_fee, &without_fee] {
        assert!(solution.unassigned.is_none());
        assert_eq!(solution.tours.len(), 1);
        let clustered_stop = get_activity_ids(&solution.tours[0]).into_iter().find(|ids| ids.len() == 2);
        assert!(
            clustered_stop.is_some_and(|ids| ids.contains(&"first".to_string()) && ids.contains(&"second".to_string()))
        );
    }
    assert_eq!(with_fee.tours[0].statistic.cost - without_fee.tours[0].statistic.cost, 300.);
    assert_eq!(with_fee.statistic.cost, with_fee.tours[0].statistic.cost);
}

#[test]
fn charges_every_job_of_a_multi_trip_tour_once() {
    let jobs = vec![
        Job { passengers: Some(2), ..create_delivery_job("first", (1., 0.)) },
        Job { passengers: Some(3), ..create_delivery_job("second", (2., 0.)) },
    ];
    let vehicle = VehicleType {
        shifts: vec![VehicleShift {
            end: Some(ShiftEnd { earliest: None, latest: format_time(100.), location: (0., 0.).to_loc() }),
            reloads: Some(vec![VehicleReload { location: (0., 0.).to_loc(), duration: 2., ..create_default_reload() }]),
            ..create_default_open_vehicle_shift()
        }],
        capacity: Some(vec![1]),
        ..create_vehicle_type("vehicle", None, Some(100.))
    };

    let solution = solve(create_problem(jobs, vec![vehicle]));

    assert!(solution.unassigned.is_none());
    assert_eq!(solution.tours.len(), 1);
    let tour = &solution.tours[0];
    let activity_ids = get_activity_ids(tour).concat();
    assert_eq!(activity_ids.iter().filter(|id| *id == "reload").count(), 1);
    assert_eq!(tour.statistic.cost, get_base_cost(tour) + 500.);
    assert_eq!(solution.statistic.cost, tour.statistic.cost);
}
