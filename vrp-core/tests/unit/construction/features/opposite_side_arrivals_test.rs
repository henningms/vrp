use super::*;
use crate::construction::heuristics::{ActivityContext, MoveContext};
use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
use crate::helpers::models::problem::{FleetBuilder, test_driver, test_ovrp_vehicle};
use crate::helpers::models::solution::{ActivityBuilder, RouteBuilder, RouteContextBuilder};
use crate::models::solution::Activity;

const SIZE: usize = 8;

fn cell(from: Location, to: Location) -> usize {
    from * SIZE + to
}

fn profile(index: usize) -> Profile {
    Profile { index, scale: 1. }
}

fn cells(profile: usize, marked: &[(Location, Location)]) -> Arc<OppositeSideCells> {
    let indices = marked.iter().map(|&(from, to)| cell(from, to)).collect();
    Arc::new(OppositeSideCells::new(SIZE, vec![(profile, indices)]).unwrap())
}

fn create_objective(marked: &[(Location, Location)]) -> Arc<dyn FeatureObjective> {
    create_opposite_side_arrivals_feature("opposite_side_arrivals", cells(0, marked)).unwrap().objective.unwrap()
}

fn at(location: Location) -> Activity {
    ActivityBuilder::with_location(location).build()
}

/// A route of the default vehicle, which starts and ends at location 0, through `locations`.
fn route_with(locations: &[Location]) -> RouteContext {
    let mut builder = RouteBuilder::with_default_vehicle();
    for &location in locations {
        builder.add_activity(at(location));
    }
    RouteContextBuilder::default().with_route(builder.build()).build()
}

/// A route of a vehicle with the given profile and no end, through `locations`.
fn open_route_with(profile_index: usize, locations: &[Location]) -> RouteContext {
    let vehicle = Vehicle { profile: profile(profile_index), ..test_ovrp_vehicle("v1") };
    let fleet = FleetBuilder::default().add_driver(test_driver()).add_vehicle(vehicle).build();
    let mut builder = RouteBuilder::default();
    builder.with_vehicle(&fleet, "v1");
    for &location in locations {
        builder.add_activity(at(location));
    }
    RouteContextBuilder::default().with_route(builder.build()).build()
}

fn fitness_of(objective: &dyn FeatureObjective, routes: Vec<RouteContext>) -> Cost {
    objective.fitness(&TestInsertionContextBuilder::default().with_routes(routes).build())
}

/// Estimates inserting an activity at `location` after the tour activity at `leg_index`.
fn estimate(objective: &dyn FeatureObjective, route_ctx: &RouteContext, location: Location, leg_index: usize) -> Cost {
    let solution_ctx = TestInsertionContextBuilder::default().build().solution;
    let tour = &route_ctx.route().tour;
    let target = at(location);
    let activity_ctx = ActivityContext {
        index: leg_index,
        prev: tour.get(leg_index).unwrap(),
        target: &target,
        next: tour.get(leg_index + 1),
    };

    objective.estimate(&MoveContext::activity(&solution_ctx, route_ctx, &activity_ctx))
}

#[test]
fn fitness_counts_marked_legs() {
    let objective = create_objective(&[(0, 1), (2, 3)]);

    assert_eq!(fitness_of(objective.as_ref(), vec![route_with(&[1, 2, 3])]), 2.);
    assert_eq!(fitness_of(objective.as_ref(), vec![route_with(&[3, 2, 1])]), 0.);
}

#[test]
fn fitness_counts_one_arrival_for_several_activities_at_a_location() {
    let objective = create_objective(&[(0, 1)]);

    assert_eq!(fitness_of(objective.as_ref(), vec![route_with(&[1, 1, 1])]), 1.);
}

#[test]
fn fitness_never_counts_staying_at_a_location() {
    let objective = create_objective(&[(1, 1)]);

    assert_eq!(fitness_of(objective.as_ref(), vec![route_with(&[1, 1])]), 0.);
}

#[test]
fn fitness_sums_over_routes() {
    let objective = create_objective(&[(0, 1), (0, 2), (2, 3)]);

    assert_eq!(fitness_of(objective.as_ref(), vec![route_with(&[1]), route_with(&[2, 3])]), 3.);
}

#[test]
fn fitness_reads_the_cells_of_the_vehicle_profile() {
    let objective = create_opposite_side_arrivals_feature("opposite_side_arrivals", cells(1, &[(0, 1)]))
        .unwrap()
        .objective
        .unwrap();

    assert_eq!(fitness_of(objective.as_ref(), vec![open_route_with(1, &[1])]), 1.);
    assert_eq!(fitness_of(objective.as_ref(), vec![open_route_with(0, &[1])]), 0.);
}

#[test]
fn estimate_charges_a_marked_leg_into_the_target() {
    let objective = create_objective(&[(1, 2)]);

    assert_eq!(estimate(objective.as_ref(), &route_with(&[1]), 2, 1), 1.);
}

#[test]
fn estimate_charges_a_marked_leg_the_target_creates_into_the_next_activity() {
    let objective = create_objective(&[(2, 3)]);

    assert_eq!(estimate(objective.as_ref(), &route_with(&[1, 3]), 2, 1), 1.);
}

#[test]
fn estimate_credits_a_marked_leg_the_target_breaks() {
    let objective = create_objective(&[(1, 3)]);

    assert_eq!(estimate(objective.as_ref(), &route_with(&[1, 3]), 2, 1), -1.);
}

#[test]
fn estimate_at_the_end_of_an_open_tour_counts_only_the_leg_into_the_target() {
    let objective = create_opposite_side_arrivals_feature("opposite_side_arrivals", cells(0, &[(1, 2), (2, 0)]))
        .unwrap()
        .objective
        .unwrap();
    let route_ctx = open_route_with(0, &[1]);

    assert!(route_ctx.route().tour.get(2).is_none());
    assert_eq!(estimate(objective.as_ref(), &route_ctx, 2, 1), 1.);
}

#[test]
fn estimates_add_up_to_the_fitness_of_the_built_tour() {
    let marked = [(0, 2), (2, 1), (1, 3), (3, 0), (0, 1)];
    let objective = create_objective(&marked);
    let tour = [2, 1, 3];

    let mut total = Cost::default();
    for (index, &location) in tour.iter().enumerate() {
        total += estimate(objective.as_ref(), &route_with(&tour[..index]), location, index);
    }

    assert_eq!(total, fitness_of(objective.as_ref(), vec![route_with(&tour)]));
    assert_eq!(total, 4.);
}

#[test]
fn cells_are_kept_per_profile() {
    let cells = OppositeSideCells::new(SIZE, vec![(1, vec![cell(2, 3)])]).unwrap();

    assert!(cells.is_opposite(&profile(1), 2, 3));
    assert!(!cells.is_opposite(&profile(0), 2, 3));
    assert!(!cells.is_opposite(&profile(2), 2, 3));
    assert!(!cells.is_opposite(&profile(1), 3, 2));
}

#[test]
fn locations_outside_the_matrix_are_never_opposite() {
    let cells = OppositeSideCells::new(SIZE, vec![(0, vec![cell(7, 6)])]).unwrap();

    assert!(cells.is_opposite(&profile(0), 7, 6));
    assert!(!cells.is_opposite(&profile(0), SIZE, 0));
    assert!(!cells.is_opposite(&profile(0), 0, SIZE));
    assert!(!OppositeSideCells::new(0, vec![]).unwrap().is_opposite(&profile(0), 0, 1));
}

#[test]
fn cells_reject_an_index_outside_the_matrix() {
    assert!(OppositeSideCells::new(SIZE, vec![(0, vec![SIZE * SIZE])]).is_err());
}
