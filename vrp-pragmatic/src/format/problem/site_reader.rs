//! Infers job sites for the site mixing and site visits objectives.
//!
//! A site is a place which pickup-delivery jobs connect with many different places, e.g. a school:
//! the delivery of morning trips and the pickup of afternoon trips, each from another home. Ends
//! within [`SITE_RADIUS`] metres are merged, so the entrances of one school, or schools next to each
//! other, are one site. A job's site is its end connected with the most different places, when that
//! is at least [`SITE_MIN_PLACES`] and no other end of the job is connected with as many. Counting
//! places rather than jobs keeps a dense housing area, where several children board and get off,
//! from outweighing a small school. Jobs with an explicit `site` keep it.

#[cfg(test)]
#[path = "../../../tests/unit/format/problem/site_reader_test.rs"]
mod site_reader_test;

use super::*;
use crate::utils::get_haversine_distance;
use std::collections::{HashMap, HashSet};

/// Ends closer than this many metres are one site.
const SITE_RADIUS: Float = 150.;
/// The minimum number of different places an end is connected with to make it a site.
const SITE_MIN_PLACES: usize = 2;
/// Prefix of inferred site keys, so they never collide with explicit ones.
const INFERRED_SITE_PREFIX: &str = "inferred-site:";

/// Returns the problem with an inferred `site` for every pickup-delivery job without one, when a site
/// objective is used.
pub(crate) fn with_inferred_sites(mut api_problem: ApiProblem) -> ApiProblem {
    if !has_site_objective(api_problem.objectives.as_deref().unwrap_or_default()) {
        return api_problem;
    }

    let sites = infer_sites(&api_problem);
    api_problem
        .plan
        .jobs
        .iter_mut()
        .filter(|job| job.site.is_none())
        .for_each(|job| job.site = sites.get(&job.id).cloned());

    api_problem
}

/// Infers sites of pickup-delivery jobs: job id to site.
pub(crate) fn infer_sites(api_problem: &ApiProblem) -> HashMap<String, String> {
    // distinct ends of each pickup-delivery job: the first place of every pickup and delivery task
    let job_ends = api_problem
        .plan
        .jobs
        .iter()
        .filter_map(|job| {
            let tasks = job.pickups.as_ref()?.iter().chain(job.deliveries.as_ref()?.iter());
            let mut ends = tasks.filter_map(|task| task.places.first()).filter_map(|place| Point::new(&place.location));
            let ends = ends.try_fold(Vec::<Point>::new(), |mut acc, point| {
                if !acc.iter().any(|other| other.key == point.key) {
                    acc.push(point);
                }
                Some(acc)
            })?;

            Some((job.id.as_str(), ends))
        })
        .filter(|(_, ends)| !ends.is_empty())
        .collect::<Vec<_>>();

    // points ordered by how many jobs share them, then by first appearance, so clustering is deterministic
    let mut points: Vec<(Point, usize)> = vec![];
    let mut point_idx: HashMap<PointKey, usize> = HashMap::default();
    job_ends.iter().flat_map(|(_, ends)| ends.iter()).for_each(|point| match point_idx.get(&point.key) {
        Some(&idx) => points[idx].1 += 1,
        None => {
            point_idx.insert(point.key, points.len());
            points.push((point.clone(), 1));
        }
    });
    points.sort_by(|(_, a), (_, b)| b.cmp(a));

    // greedy clustering: a point joins the first cluster whose centre is within the radius
    let mut centres: Vec<Point> = vec![];
    let cluster_of = points
        .iter()
        .map(|(point, _)| {
            let cluster = centres.iter().position(|centre| centre.is_near(point)).unwrap_or_else(|| {
                centres.push(point.clone());
                centres.len() - 1
            });
            (point.key, cluster)
        })
        .collect::<HashMap<_, _>>();

    let job_clusters = job_ends
        .iter()
        .map(|(id, ends)| (*id, ends.iter().map(|point| cluster_of[&point.key]).collect::<HashSet<_>>()))
        .collect::<Vec<_>>();

    // places each cluster is connected with by jobs
    let mut connected = vec![HashSet::<usize>::new(); centres.len()];
    job_clusters.iter().for_each(|(_, clusters)| {
        clusters.iter().for_each(|&a| connected[a].extend(clusters.iter().filter(|&&b| b != a)));
    });

    job_clusters
        .into_iter()
        .filter_map(|(id, clusters)| {
            let mut scores =
                clusters.into_iter().map(|cluster| (connected[cluster].len(), cluster)).collect::<Vec<_>>();
            scores.sort_by(|a, b| b.cmp(a));

            let (best, cluster) = *scores.first()?;
            let is_unique = scores.get(1).is_none_or(|(next, _)| *next < best);

            (best >= SITE_MIN_PLACES && is_unique)
                .then(|| (id.to_string(), format!("{INFERRED_SITE_PREFIX}{}", centres[cluster].label())))
        })
        .collect()
}

fn has_site_objective(objectives: &[Objective]) -> bool {
    objectives.iter().any(|objective| match objective {
        Objective::MinimizeSiteMixing | Objective::MinimizeSiteVisits => true,
        Objective::MultiObjective { objectives, .. } => has_site_objective(objectives),
        _ => false,
    })
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum PointKey {
    Coordinate(u64, u64),
    Index(usize),
}

/// A job end which can be compared with others.
#[derive(Clone)]
struct Point {
    key: PointKey,
    location: Location,
}

impl Point {
    fn new(location: &Location) -> Option<Self> {
        let key = match location {
            Location::Coordinate { lat, lng } => PointKey::Coordinate(lat.to_bits(), lng.to_bits()),
            Location::Reference { index } => PointKey::Index(*index),
            Location::Custom { .. } => return None,
        };

        Some(Self { key, location: location.clone() })
    }

    /// Coordinates are near within the radius; matrix indices only when they are the same.
    fn is_near(&self, other: &Point) -> bool {
        match (self.key, other.key) {
            (PointKey::Coordinate(..), PointKey::Coordinate(..)) => {
                get_haversine_distance(&self.location, &other.location) <= SITE_RADIUS
            }
            (a, b) => a == b,
        }
    }

    fn label(&self) -> String {
        match &self.location {
            Location::Coordinate { lat, lng } => format!("{lat:.5},{lng:.5}"),
            Location::Reference { index } => format!("index:{index}"),
            Location::Custom { .. } => String::default(),
        }
    }
}
