use crate::format::coord_index::CoordIndex;
use crate::format::problem::JobSkills as ApiJobSkills;
use crate::format::problem::JobPreferences as ApiJobPreferences;
use crate::format::problem::*;
use crate::format::{JobIndex, Location};
use crate::parse_time;
use crate::utils::VariableJobPermutation;
use std::collections::HashMap;
use std::sync::Arc;
use vrp_core::{
    construction::features::{
        BreakPolicy, JobCompatibilityDimension, JobDemandDimension, JobGroupDimension,
        JobMaxRideDurationDimension, JobPreferences as FeatureJobPreferences, JobPreferencesDimension,
        JobRequestedTimesDimension, JobSkills as FeatureJobSkills, JobSkillsDimension,
        LifoGroupDimension, LifoGroupId, LifoTagDimension,
    },
    models::common::*,
    models::problem::{
        Actor, Fleet, Job, JobIdDimension, Jobs, Multi, Place, Single, TransportCost, VehicleIdDimension,
    },
    models::{Lock, LockDetail, LockOrder, LockPosition},
};

// TODO configure sample size
const MULTI_JOB_SAMPLE_SIZE: usize = 3;

type PlaceData = (Option<Location>, Duration, Vec<TimeSpan>, Option<String>, Option<Timestamp>);
type ApiJob = crate::format::problem::Job;

pub(super) fn read_jobs_with_extra_locks(
    api_problem: &ApiProblem,
    props: &ProblemProperties,
    coord_index: &CoordIndex,
    fleet: &Fleet,
    transport: &(dyn TransportCost + Sync + Send),
    job_index: &mut JobIndex,
    environment: &Environment,
) -> (Jobs, Vec<Arc<Lock>>) {
    let random = &environment.random;
    let logger = &environment.logger;

    let (mut jobs, locks) = read_required_jobs(api_problem, props, coord_index, job_index, random);
    let conditional_jobs = read_conditional_jobs(api_problem, coord_index, job_index);

    jobs.extend(conditional_jobs);

    (Jobs::new(fleet, jobs, transport, logger).unwrap(), locks)
}

pub(super) fn read_locks(api_problem: &ApiProblem, job_index: &JobIndex) -> Vec<Arc<Lock>> {
    if api_problem.plan.relations.as_ref().is_none_or(|r| r.is_empty()) {
        return vec![];
    }

    let relations: HashMap<_, Vec<_>> =
        api_problem.plan.relations.as_ref().unwrap().iter().fold(HashMap::new(), |mut acc, r| {
            let shift_index = r.shift_index.unwrap_or_default();
            acc.entry((r.vehicle_id.clone(), shift_index)).or_default().push(r.clone());

            acc
        });

    relations.into_iter().fold(vec![], |mut acc, ((vehicle_id, shift_index), rels)| {
        let condition = create_condition(vehicle_id.clone(), shift_index);
        let details = rels.iter().fold(vec![], |mut acc, rel| {
            let order = match rel.type_field {
                RelationType::Any => LockOrder::Any,
                RelationType::Sequence => LockOrder::Sequence,
                RelationType::Strict => LockOrder::Strict,
            };

            let position = match (rel.jobs.first().map(|s| s.as_str()), rel.jobs.last().map(|s| s.as_str())) {
                (Some("departure"), Some("arrival")) => LockPosition::Fixed,
                (Some("departure"), _) => LockPosition::Departure,
                (_, Some("arrival")) => LockPosition::Arrival,
                _ => LockPosition::Any,
            };

            let (_, jobs) = rel
                .jobs
                .iter()
                .filter(|job| job.as_str() != "departure" && job.as_str() != "arrival")
                .fold((HashMap::<String, _>::default(), vec![]), |(mut indexer, mut jobs), job| {
                    let job_id = match job.as_str() {
                        "break" | "reload" | "recharge" => {
                            let entry = indexer.entry(job.clone()).or_insert(1_usize);
                            let job_index = *entry;
                            *entry += 1;
                            format!("{vehicle_id}_{job}_{shift_index}_{job_index}")
                        }
                        _ => job.clone(),
                    };
                    let job =
                        job_index.get(&job_id).cloned().unwrap_or_else(|| panic!("cannot find job with id: '{job_id}"));

                    jobs.push(job);

                    (indexer, jobs)
                });

            acc.push(LockDetail::new(order, position, jobs));

            acc
        });

        acc.push(Arc::new(Lock::new(condition, details, false)));

        acc
    })
}

fn read_required_jobs(
    api_problem: &ApiProblem,
    props: &ProblemProperties,
    coord_index: &CoordIndex,
    job_index: &mut JobIndex,
    random: &Arc<dyn Random>,
) -> (Vec<Job>, Vec<Arc<Lock>>) {
    let mut jobs = vec![];
    let has_multi_dimens = props.has_multi_dimen_capacity;

    let get_single_from_task = |task: &JobTask, activity_type: &str, is_static_demand: bool| {
        let absent = (empty(), empty());
        let capacity = task.demand.clone().map_or_else(empty, MultiDimLoad::new);
        let demand = if is_static_demand { (capacity, empty()) } else { (empty(), capacity) };

        let demand = match activity_type {
            "pickup" => Demand { pickup: demand, delivery: absent },
            "delivery" => Demand { pickup: absent, delivery: demand },
            "replacement" => Demand { pickup: demand, delivery: demand },
            "service" => Demand { pickup: absent, delivery: absent },
            _ => panic!("invalid activity type."),
        };

        let places = task
            .places
            .iter()
            .map(|p| {
                let requested_time = p.requested_time.as_ref().map(|t| parse_time(t));
                (Some(p.location.clone()), p.duration, parse_times(&p.times), p.tag.clone(), requested_time)
            })
            .collect();

        get_single_with_dimens(places, demand, &task.order, activity_type, has_multi_dimens, coord_index)
    };

    api_problem.plan.jobs.iter().for_each(|job| {
        let pickups = job.pickups.as_ref().map_or(0, |p| p.len());
        let deliveries = job.deliveries.as_ref().map_or(0, |p| p.len());
        let is_static_demand = pickups == 0 || deliveries == 0;

        let singles =
            job.pickups
                .iter()
                .flat_map(|tasks| tasks.iter().map(|task| get_single_from_task(task, "pickup", is_static_demand)))
                .chain(job.deliveries.iter().flat_map(|tasks| {
                    tasks.iter().map(|task| get_single_from_task(task, "delivery", is_static_demand))
                }))
                .chain(
                    job.replacements
                        .iter()
                        .flat_map(|tasks| tasks.iter().map(|task| get_single_from_task(task, "replacement", true))),
                )
                .chain(
                    job.services
                        .iter()
                        .flat_map(|tasks| tasks.iter().map(|task| get_single_from_task(task, "service", false))),
                )
                .collect::<Vec<_>>();

        assert!(!singles.is_empty());

        let problem_job = if singles.len() > 1 {
            let deliveries_start_index = job.pickups.as_ref().map_or(0, |p| p.len());
            get_multi_job(job, singles, deliveries_start_index, random)
        } else {
            get_single_job(job, singles.into_iter().next().unwrap())
        };

        job_index.insert(job.id.clone(), problem_job.clone());
        jobs.push(problem_job);
    });

    (jobs, vec![])
}

fn read_conditional_jobs(api_problem: &ApiProblem, coord_index: &CoordIndex, job_index: &mut JobIndex) -> Vec<Job> {
    let mut jobs = vec![];

    api_problem.fleet.vehicles.iter().for_each(|vehicle| {
        for (shift_index, shift) in vehicle.shifts.iter().enumerate() {
            if let Some(breaks) = &shift.breaks {
                read_optional_breaks(coord_index, job_index, &mut jobs, vehicle, shift_index, breaks);
            }

            if let Some(reloads) = &shift.reloads {
                read_reloads(coord_index, job_index, &mut jobs, vehicle, shift_index, reloads);
            }

            if let Some(recharges) = &shift.recharges {
                read_recharges(coord_index, job_index, &mut jobs, vehicle, shift_index, recharges);
            }
        }
    });

    jobs
}

fn read_optional_breaks(
    coord_index: &CoordIndex,
    job_index: &mut JobIndex,
    jobs: &mut Vec<Job>,
    vehicle: &VehicleType,
    shift_index: usize,
    breaks: &[VehicleBreak],
) {
    (1..)
        .zip(breaks.iter().filter_map(|vehicle_break| match vehicle_break {
            VehicleBreak::Optional { time, places, policy } => Some((time, places, policy)),
            VehicleBreak::Required { .. } => None,
        }))
        .flat_map(|(break_idx, (break_time, break_places, policy))| {
            vehicle
                .vehicle_ids
                .iter()
                .map(|vehicle_id| {
                    let times = match &break_time {
                        VehicleOptionalBreakTime::TimeWindow(time) if time.len() != 2 => {
                            panic!("break with invalid time window specified: must have start and end!")
                        }
                        VehicleOptionalBreakTime::TimeOffset(offsets) if offsets.len() != 2 => {
                            panic!("break with invalid offset specified: must have start and end!")
                        }
                        VehicleOptionalBreakTime::TimeWindow(time) => vec![TimeSpan::Window(parse_time_window(time))],
                        VehicleOptionalBreakTime::TimeOffset(offset) => {
                            vec![TimeSpan::Offset(TimeOffset::new(*offset.first().unwrap(), *offset.last().unwrap()))]
                        }
                    };

                    let job_id = format!("{vehicle_id}_break_{shift_index}_{break_idx}");
                    let places = break_places
                        .iter()
                        .map(|place| (place.location.clone(), place.duration, times.clone(), place.tag.clone(), None))
                        .collect();

                    let mut job =
                        get_conditional_job(coord_index, vehicle_id.clone(), &job_id, "break", shift_index, places);

                    if let Some(policy) = policy {
                        let policy = match policy {
                            VehicleOptionalBreakPolicy::SkipIfNoIntersection => BreakPolicy::SkipIfNoIntersection,
                            VehicleOptionalBreakPolicy::SkipIfArrivalBeforeEnd => BreakPolicy::SkipIfArrivalBeforeEnd,
                        };

                        job.dimens.set_break_policy(policy);
                    }

                    (job_id, job)
                })
                .collect::<Vec<_>>()
        })
        .for_each(|(job_id, single)| add_conditional_job(job_index, jobs, job_id, single));
}

fn read_reloads(
    coord_index: &CoordIndex,
    job_index: &mut JobIndex,
    jobs: &mut Vec<Job>,
    vehicle: &VehicleType,
    shift_index: usize,
    reloads: &[VehicleReload],
) {
    read_specific_job_places(
        "reload",
        coord_index,
        job_index,
        jobs,
        vehicle,
        shift_index,
        reloads.iter().map(|reload| JobPlace {
            location: reload.location.clone(),
            duration: reload.duration,
            times: reload.times.clone(),
            tag: reload.tag.clone(),
            requested_time: None,
        }),
    )
}

fn read_recharges(
    coord_index: &CoordIndex,
    job_index: &mut JobIndex,
    jobs: &mut Vec<Job>,
    vehicle: &VehicleType,
    shift_index: usize,
    recharges: &VehicleRecharges,
) {
    read_specific_job_places(
        "recharge",
        coord_index,
        job_index,
        jobs,
        vehicle,
        shift_index,
        recharges.stations.iter().cloned(),
    )
}

fn read_specific_job_places(
    job_type: &str,
    coord_index: &CoordIndex,
    job_index: &mut JobIndex,
    jobs: &mut Vec<Job>,
    vehicle: &VehicleType,
    shift_index: usize,
    get_places: impl Iterator<Item = JobPlace>,
) {
    (1..)
        .zip(get_places)
        .flat_map(|(place_idx, place)| {
            vehicle
                .vehicle_ids
                .iter()
                .map(|vehicle_id| {
                    let job_id = format!("{vehicle_id}_{job_type}_{shift_index}_{place_idx}");
                    let times = parse_times(&place.times);

                    let requested_time = place.requested_time.as_ref().map(|t| parse_time(t));
                    let job = get_conditional_job(
                        coord_index,
                        vehicle_id.clone(),
                        &job_id,
                        job_type,
                        shift_index,
                        vec![(Some(place.location.clone()), place.duration, times, place.tag.clone(), requested_time)],
                    );

                    (job_id, job)
                })
                .collect::<Vec<_>>()
        })
        .for_each(|(job_id, single)| add_conditional_job(job_index, jobs, job_id, single));
}

fn get_conditional_job(
    coord_index: &CoordIndex,
    vehicle_id: String,
    job_id: &str,
    job_type: &str,
    shift_index: usize,
    places: Vec<PlaceData>,
) -> Single {
    let mut single = get_single(places, coord_index);
    single
        .dimens
        .set_job_id(job_id.to_string())
        .set_job_type(job_type.to_string())
        .set_shift_index(shift_index)
        .set_vehicle_id(vehicle_id);

    single
}

fn add_conditional_job(job_index: &mut JobIndex, jobs: &mut Vec<Job>, job_id: String, single: Single) {
    let job = Job::Single(Arc::new(single));
    job_index.insert(job_id, job.clone());
    jobs.push(job);
}

fn get_single(places: Vec<PlaceData>, coord_index: &CoordIndex) -> Single {
    let tags = places
        .iter()
        .map(|(_, _, _, tag, _)| tag)
        .enumerate()
        .filter_map(|(idx, tag)| tag.as_ref().map(|tag| (idx, tag.clone())))
        .collect::<Vec<_>>();

    // Collect requested times for each place index
    let requested_times: HashMap<usize, Timestamp> = places
        .iter()
        .enumerate()
        .filter_map(|(idx, (_, _, _, _, requested_time))| {
            requested_time.map(|t| (idx, t))
        })
        .collect();

    let places = places
        .into_iter()
        .map(|(location, duration, times, _, _)| Place {
            location: location.as_ref().and_then(|l| coord_index.get_by_loc(l)),
            duration,
            times,
        })
        .collect();

    let mut dimens = Dimensions::default();

    dimens.set_place_tags(tags);

    if !requested_times.is_empty() {
        dimens.set_job_requested_times(requested_times);
    }

    Single { places, dimens }
}

fn get_single_with_dimens(
    places: Vec<PlaceData>,
    demand: Demand<MultiDimLoad>,
    order: &Option<i32>,
    activity_type: &str,
    has_multi_dimens: bool,
    coord_index: &CoordIndex,
) -> Single {
    let mut single = get_single(places, coord_index);
    let dimens = &mut single.dimens;

    if has_multi_dimens {
        dimens.set_job_demand(demand)
    } else {
        dimens.set_job_demand(Demand {
            pickup: (SingleDimLoad::new(demand.pickup.0.load[0]), SingleDimLoad::new(demand.pickup.1.load[0])),
            delivery: (SingleDimLoad::new(demand.delivery.0.load[0]), SingleDimLoad::new(demand.delivery.1.load[0])),
        })
    }
    .set_job_type(activity_type.to_string());

    if let Some(order) = order {
        dimens.set_job_order(*order);
    }

    single
}

fn fill_dimens(job: &ApiJob, dimens: &mut Dimensions) {
    dimens.set_job_id(job.id.clone());

    if let Some(value) = job.value {
        dimens.set_job_value(value);
    }

    if let Some(group) = job.group.clone() {
        dimens.set_job_group(group);
    }

    if let Some(compat) = job.compatibility.clone() {
        dimens.set_job_compatibility(compat);
    }

    if let Some(skills) = get_skills(&job.skills) {
        dimens.set_job_skills(skills);
    }

    if let Some(preferences) = get_preferences(&job.preferences) {
        dimens.set_job_preferences(preferences);
    }
}

fn get_single_job(job: &ApiJob, single: Single) -> Job {
    let mut single = single;
    fill_dimens(job, &mut single.dimens);

    Job::Single(Arc::new(single))
}

fn get_multi_job(job: &ApiJob, mut singles: Vec<Single>, deliveries_start_index: usize, random: &Arc<dyn Random>) -> Job {
    let mut dimens: Dimensions = Default::default();
    fill_dimens(job, &mut dimens);

    // If this job has a max ride duration, store it in the Multi job dimensions
    if let Some(max_ride_duration) = job.max_ride_duration {
        dimens.set_job_max_ride_duration(max_ride_duration);
    }

    // If this job has a LIFO tag, set it on all singles and derive group ID from job ID
    if let Some(lifo_tag) = &job.lifo_tag {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // Derive LIFO group ID from job ID
        let mut hasher = DefaultHasher::new();
        job.id.hash(&mut hasher);
        let lifo_id = LifoGroupId(hasher.finish() as usize);

        for single in &mut singles {
            single.dimens.set_lifo_tag(lifo_tag.clone());
            single.dimens.set_lifo_group(lifo_id);
        }
    }

    let singles = singles.into_iter().map(Arc::new).collect::<Vec<_>>();

    // Use fixed order permutation if:
    // 1. fixedOrder is explicitly set to true, OR
    // 2. It's a simple 2-job case (1 pickup + 1 delivery) - existing behavior
    let use_fixed_order = job.fixed_order.unwrap_or(false) || (singles.len() == 2 && deliveries_start_index == 1);

    let multi = if use_fixed_order {
        Multi::new_shared(singles, dimens)
    } else {
        let jobs_len = singles.len();
        Multi::new_shared_with_permutator(
            singles,
            dimens,
            Box::new(VariableJobPermutation::new(
                jobs_len,
                deliveries_start_index,
                MULTI_JOB_SAMPLE_SIZE,
                random.clone(),
            )),
        )
    };

    Job::Multi(multi)
}

fn create_condition(vehicle_id: String, shift_index: usize) -> Arc<dyn Fn(&Actor) -> bool + Sync + Send> {
    Arc::new(move |actor: &Actor| {
        *actor.vehicle.dimens.get_vehicle_id().unwrap() == vehicle_id
            && actor.vehicle.dimens.get_shift_index().copied().unwrap() == shift_index
    })
}

fn get_skills(skills: &Option<ApiJobSkills>) -> Option<FeatureJobSkills> {
    skills
        .as_ref()
        .map(|skills| FeatureJobSkills::new(skills.all_of.clone(), skills.one_of.clone(), skills.none_of.clone()))
}

fn get_preferences(preferences: &Option<ApiJobPreferences>) -> Option<FeatureJobPreferences> {
    preferences.as_ref().map(|prefs| {
        FeatureJobPreferences::new(prefs.preferred.clone(), prefs.acceptable.clone(), prefs.avoid.clone(), prefs.weight)
    })
}

fn empty() -> MultiDimLoad {
    MultiDimLoad::default()
}

fn parse_times(times: &Option<Vec<Vec<String>>>) -> Vec<TimeSpan> {
    times.as_ref().map_or(vec![TimeSpan::Window(TimeWindow::max())], |tws| {
        tws.iter().map(|tw| TimeSpan::Window(parse_time_window(tw))).collect()
    })
}
