use super::*;
use crate::helpers::*;

const SCHOOL: (f64, f64) = (59.95, 10.75);
const OTHER_SCHOOL: (f64, f64) = (59.97, 10.80);

/// Homes about a kilometre apart.
fn home(n: usize) -> (f64, f64) {
    (59.90 + 0.01 * n as f64, 10.60)
}

fn north_of(point: (f64, f64), metres: f64) -> (f64, f64) {
    (point.0 + metres / 111_320., point.1)
}

fn trip(id: &str, from: (f64, f64), to: (f64, f64)) -> Job {
    create_pickup_delivery_job(id, from, to)
}

fn problem(jobs: Vec<Job>, objectives: Option<Vec<Objective>>) -> Problem {
    Problem { plan: Plan { jobs, ..create_empty_plan() }, objectives, ..create_empty_problem() }
}

fn site_mixing() -> Option<Vec<Objective>> {
    Some(vec![Objective::MinimizeUnassigned { breaks: None }, Objective::MinimizeSiteMixing, Objective::MinimizeCost])
}

fn site_of(problem: &Problem, id: &str) -> Option<String> {
    infer_sites(problem).get(id).cloned()
}

#[test]
fn infers_school_as_site_of_morning_trips() {
    let p = problem((1..=3).map(|n| trip(&format!("t{n}"), home(n), SCHOOL)).collect(), site_mixing());

    let site = site_of(&p, "t1");
    assert!(site.is_some());
    assert_eq!(site_of(&p, "t2"), site);
    assert_eq!(site_of(&p, "t3"), site);
}

#[test]
fn infers_school_as_site_of_afternoon_trips() {
    let p = problem((1..=3).map(|n| trip(&format!("t{n}"), SCHOOL, home(n))).collect(), site_mixing());

    let site = site_of(&p, "t1");
    assert!(site.is_some());
    assert_eq!(site_of(&p, "t2"), site);
    assert_eq!(site_of(&p, "t3"), site);
}

#[test]
fn merges_ends_within_radius() {
    let near = north_of(SCHOOL, 120.);
    let mut jobs = (1..=3).map(|n| trip(&format!("a{n}"), home(n), SCHOOL)).collect::<Vec<_>>();
    jobs.extend((4..=6).map(|n| trip(&format!("b{n}"), home(n), near)));
    let p = problem(jobs, site_mixing());

    assert!(site_of(&p, "a1").is_some());
    assert_eq!(site_of(&p, "b4"), site_of(&p, "a1"));
}

#[test]
fn keeps_ends_beyond_radius_apart() {
    let far = north_of(SCHOOL, 200.);
    let mut jobs = (1..=3).map(|n| trip(&format!("a{n}"), home(n), SCHOOL)).collect::<Vec<_>>();
    jobs.extend((4..=6).map(|n| trip(&format!("b{n}"), home(n), far)));
    let p = problem(jobs, site_mixing());

    assert!(site_of(&p, "a1").is_some() && site_of(&p, "b4").is_some());
    assert_ne!(site_of(&p, "b4"), site_of(&p, "a1"));
    assert_eq!(site_of(&p, "b5"), site_of(&p, "b4"));
}

#[test]
fn infers_no_site_for_place_connected_to_one_other_place() {
    let p = problem(vec![trip("t1", home(1), SCHOOL), trip("t2", home(1), SCHOOL)], site_mixing());

    assert_eq!(site_of(&p, "t1"), None);
}

#[test]
fn prefers_school_over_dense_housing_area() {
    // four children from one housing area, at two schools, with their afternoon trips home
    let area = home(9);
    let mut jobs = vec![];
    for (child, school) in [("k1", SCHOOL), ("k2", SCHOOL), ("k3", OTHER_SCHOOL), ("k4", OTHER_SCHOOL)] {
        jobs.push(trip(&format!("{child}_am"), area, school));
        jobs.push(trip(&format!("{child}_pm"), school, area));
    }
    // the small school also has riders from two other places
    jobs.push(trip("r1", home(1), SCHOOL));
    jobs.push(trip("r2", home(2), SCHOOL));
    let p = problem(jobs, site_mixing());

    assert!(site_of(&p, "r1").is_some());
    assert_eq!(site_of(&p, "k1_am"), site_of(&p, "r1"));
    assert_eq!(site_of(&p, "k2_pm"), site_of(&p, "r1"));
}

#[test]
fn infers_schools_of_siblings_sharing_a_home() {
    let jobs = vec![
        trip("a1", home(1), SCHOOL),
        trip("a2", home(2), SCHOOL),
        trip("b1", home(3), OTHER_SCHOOL),
        trip("b2", home(4), OTHER_SCHOOL),
        trip("sibling_a", home(9), SCHOOL),
        trip("sibling_b", home(9), OTHER_SCHOOL),
    ];
    let p = problem(jobs, site_mixing());

    assert_eq!(site_of(&p, "sibling_a"), site_of(&p, "a1"));
    assert_eq!(site_of(&p, "sibling_b"), site_of(&p, "b1"));
    assert_ne!(site_of(&p, "sibling_a"), site_of(&p, "sibling_b"));
}

#[test]
fn infers_no_site_when_both_ends_are_equally_shared() {
    let stop = home(9);
    let jobs = vec![
        trip("both", stop, SCHOOL),
        trip("stop1", stop, home(1)),
        trip("stop2", stop, home(2)),
        trip("school1", home(3), SCHOOL),
        trip("school2", home(4), SCHOOL),
    ];
    let p = problem(jobs, site_mixing());

    assert_eq!(site_of(&p, "both"), None);
    assert!(site_of(&p, "school1").is_some());
}

#[test]
fn infers_sites_for_index_locations() {
    let with_indices = |id: &str, from: usize, to: usize| {
        let mut job = trip(id, (0., 0.), (0., 0.));
        job.pickups.as_mut().unwrap()[0].places[0].location = Location::new_reference(from);
        job.deliveries.as_mut().unwrap()[0].places[0].location = Location::new_reference(to);
        job
    };
    let p = problem((1..=3).map(|n| with_indices(&format!("t{n}"), n, 0)).collect(), site_mixing());

    assert!(site_of(&p, "t1").is_some());
    assert_eq!(site_of(&p, "t3"), site_of(&p, "t1"));
}

#[test]
fn keeps_explicit_site_and_fills_the_others() {
    let mut jobs = (1..=3).map(|n| trip(&format!("t{n}"), home(n), SCHOOL)).collect::<Vec<_>>();
    jobs[0].site = Some("given".to_string());

    let p = with_inferred_sites(problem(jobs, site_mixing()));

    assert_eq!(p.plan.jobs[0].site.as_deref(), Some("given"));
    assert!(p.plan.jobs[1].site.is_some());
    assert_ne!(p.plan.jobs[1].site.as_deref(), Some("given"));
}

#[test]
fn infers_sites_for_site_objective_in_multi_objective() {
    let objectives = Some(vec![
        Objective::MinimizeUnassigned { breaks: None },
        Objective::MultiObjective {
            strategy: MultiStrategy::WeightedSum { weights: vec![1., 500.] },
            objectives: vec![Objective::MinimizeCost, Objective::MinimizeSiteVisits],
        },
    ]);
    let jobs = (1..=3).map(|n| trip(&format!("t{n}"), home(n), SCHOOL)).collect();

    let p = with_inferred_sites(problem(jobs, objectives));

    assert!(p.plan.jobs.iter().all(|job| job.site.is_some()));
}

#[test]
fn infers_nothing_without_site_objective() {
    let jobs = (1..=3).map(|n| trip(&format!("t{n}"), home(n), SCHOOL)).collect();

    let p = with_inferred_sites(problem(jobs, None));

    assert!(p.plan.jobs.iter().all(|job| job.site.is_none()));
}
