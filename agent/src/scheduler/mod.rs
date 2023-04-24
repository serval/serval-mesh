#![allow(dead_code)] // temporary, during initial development

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// https://www.notion.so/srvl/Job-Scheduler-spec-6f8860f3e6874341aba0b286373d5f67?pvs=4
///
/// The scheduler, formerly known as the job queue, is in charge of creating scheduled jobs and
/// shepherding them through their lifecycle. At a high level, a scheduled job consists of:
/// - a pointer to a Manifest in the storage system (via an ssri::Integrity hash)
/// - a pointer to an input payload in the storage system
/// - various configuration values indicating how and where the job should be executed
///
/// The scheduler is intended to be intelligent about where it runs jobs, sending them to the most
/// appropriate runner at any given moment. Jobs can be configured to run more than once, or even
/// across the entire fleet of runners, and jobs can have requirements that act as constraints when
/// selecting runners.
///
use ssri::Integrity;
use thiserror::Error;
use uuid::Uuid;

/// The maximum amount of time that a runner should be given to run the job without checking back in
/// with the scheduler; after this amount of time passes, the current run will be considered failed
/// and the job will either be retried or abandoned, depending on how many prior run attempts have
/// been made (see MAX_JOB_ATTEMPTS).
const MAX_JOB_DURATION: Duration = Duration::from_secs(60);

/// The maximum number of times to try running a job that has previously timed out.
const MAX_JOB_ATTEMPTS: u8 = 3;

#[derive(Error, Debug, PartialEq)]
pub enum ServalSchedulerError {
    #[error("The attempted operation is not valid for the current state of the job")]
    InvalidOperationForJobState,

    #[error("The given job could not be located")]
    JobNotFound,
}

/// Represents the state of a job that the scheduler is currently taking care of; transient
/// information that is only relevant to a job during a particular part of its lifetime should live
/// within one of these enum values.
#[derive(Debug, PartialEq)]
enum ScheduledJobState {
    /// The job is waiting to be assigned to a runner.
    Unassigned,
    /// The job has been assigned to a runner and has a deadline, at which point it will be marked
    /// as failed and either move back to Unassignd or marked as Failed.
    InProgress { runner: Uuid, deadline: SystemTime },
    ///  The job was completed successfully.
    Completed {
        runner: Uuid,
        completion_time: SystemTime,
        output: Option<Integrity>,
    },
    /// The job failed to complete; either it timed out MAX_JOB_ATTEMPTS times, or it was explicitly
    /// marked as failed by a runner.
    Failed {
        runner: Uuid,
        failure_time: SystemTime,
        output: Option<Integrity>,
    },
}

/// The priority with which a given job should be scheduled.
#[derive(Debug, Default, PartialEq)]
enum ScheduledJobPriority {
    /// This job should take priority over all jobs of lower priority.
    Emergency = 0,
    /// This job should take priority over all non-emergency jobs.
    HighPriority = 1,
    /// This job should be scheduled with a normal amount of priority.
    #[default]
    Normal = 2,
    /// This job should only be scheduled if there is nothing more important to do.
    LowPriority = 3,
}

/// Represents a constraint on the runner that can run a job.
#[derive(Debug, PartialEq)]
enum ScheduledJobRequirement {
    /// Requires that the runner has the given extension available.
    Extension(String),
    /// Requires that the runner has `/proc` (e.g. is Linux)
    Proc,
}

/// Represents the kind of invocation this job would like to receive.
#[derive(Debug, PartialEq)]
enum ScheduledJobKind {
    /// Run this job on a single runner.
    OneOff,
    /// Run this job on N runners. If the job has been run on fewer than N runners at deadline, its
    /// result will be returned early. This still counts as a successful job execution.
    /// Execution will occur serially on one runner a time.
    Multiple {
        runs: usize,
        deadline: SystemTime,
        runners: Vec<Uuid>,
    },
    /// Run this job on every runner in the mesh. If the job has been run on fewer than N runners
    /// at deadline, its result will be returned early. This still counts as a successful job
    /// execution.
    /// Execution will occur serially on one runner a time.
    Census {
        deadline: SystemTime,
        runners: Vec<Uuid>,
    },
}

/// Represents a job that the scheduler is currently taking care of. As a job moves through the
/// system, its state value should change. Any transient information that is only relevant for part
/// of a job's lifecycle should live within the ScheduledJobState enum rather than this struct.
#[derive(Debug, PartialEq)]
struct ScheduledJob {
    id: Uuid,
    // Points to the manifest in the storage system
    manifest: Integrity,
    // Points to the input payload in the storage system, if there is any
    input: Option<Integrity>,
    //  The current state of this job; see ScheduledJobState for more information
    state: ScheduledJobState,
    // How many attempts have been made to run this job already
    attempts: u8,
    // When this job was created
    created_at: SystemTime,
    // What priority level this job has
    priority: ScheduledJobPriority,
    // List of requirements for any runner node that wants to run this job
    requirements: Vec<ScheduledJobRequirement>,
    // How to run this job (e.g. one-off, run-on-multiple, run-everywhere); see ScheduledJobKind
    kind: ScheduledJobKind,
    // A record of every runner that every touched this job
    runners: Vec<Uuid>,
}

/// The JobScheduler is responsible for creating jobs, assigning them to runners, and shepherding
/// them through their lifecycle.
struct JobScheduler {
    active_jobs: Vec<ScheduledJob>,
    finished_jobs: Vec<ScheduledJob>,
    available_runners: HashMap<Uuid, Vec<ScheduledJobRequirement>>,
}

impl JobScheduler {
    pub fn new() -> Self {
        JobScheduler {
            active_jobs: vec![],
            finished_jobs: vec![],
            available_runners: HashMap::new(),
        }
    }

    pub fn job(&self, job_id: &Uuid) -> Option<&ScheduledJob> {
        self.active_jobs
            .iter()
            .find(|job| job.id == *job_id)
            .or_else(|| self.finished_jobs.iter().find(|job| job.id == *job_id))
    }

    fn job_mut(&mut self, job_id: &Uuid) -> Option<&mut ScheduledJob> {
        self.active_jobs
            .iter_mut()
            .find(|job| job.id == *job_id)
            .or_else(|| self.finished_jobs.iter_mut().find(|job| job.id == *job_id))
    }

    pub fn extend_job_deadline(&mut self, job_id: &Uuid) -> Result<(), ServalSchedulerError> {
        let Some(job) = self.job_mut(job_id) else {
            return Err(ServalSchedulerError::JobNotFound);
        };

        match job.state {
            ScheduledJobState::InProgress { runner, .. } => {
                // oops: this job has already expired and we haven't updated its state yet. I
                // guess we can let it slide, since it clearly hasn't been assigned to anyone
                // else yet.
                job.state = ScheduledJobState::InProgress {
                    deadline: SystemTime::now() + MAX_JOB_DURATION,
                    runner,
                };
                Ok(())
            }
            _ => Err(ServalSchedulerError::InvalidOperationForJobState),
        }
    }

    pub fn enqueue_job(
        &mut self,
        manifest: Integrity,
        input: Option<Integrity>,
        requirements: Vec<ScheduledJobRequirement>,
        // todo: implement and expose `kind` and `priority`
    ) -> Result<Uuid, ServalSchedulerError> {
        let id = Uuid::new_v4();
        self.active_jobs.push(ScheduledJob {
            id,
            manifest,
            input,
            state: ScheduledJobState::Unassigned,
            attempts: 0,
            created_at: SystemTime::now(),
            requirements,
            runners: vec![],
            kind: ScheduledJobKind::OneOff,
            priority: ScheduledJobPriority::Normal,
        });

        self.tick();

        Ok(id)
    }

    pub fn mark_job_completed(
        &mut self,
        job_id: &Uuid,
        output: Option<Integrity>,
    ) -> Result<(), ServalSchedulerError> {
        let Some(mut job) = self.job_mut(job_id) else {
            return Err(ServalSchedulerError::JobNotFound);
        };

        match job.state {
            ScheduledJobState::InProgress { runner, .. } => {
                job.state = ScheduledJobState::Completed {
                    runner,
                    completion_time: SystemTime::now(),
                    output,
                }
            }
            _ => return Err(ServalSchedulerError::InvalidOperationForJobState),
        }

        Ok(())
    }

    pub fn mark_job_failed(
        &mut self,
        job_id: &Uuid,
        output: Option<Integrity>,
    ) -> Result<(), ServalSchedulerError> {
        let Some(mut job) = self.job_mut(job_id) else {
            return Err(ServalSchedulerError::JobNotFound);
        };

        match job.state {
            ScheduledJobState::InProgress { runner, .. } => {
                job.state = ScheduledJobState::Failed {
                    runner,
                    failure_time: SystemTime::now(),
                    output,
                };
            }
            _ => return Err(ServalSchedulerError::InvalidOperationForJobState),
        }

        Ok(())
    }

    pub fn register_runner(&mut self, runner: Uuid, capabilities: Vec<ScheduledJobRequirement>) {
        if self.available_runners.contains_key(&runner) {
            return;
        }

        self.available_runners.insert(runner, capabilities);

        self.tick();
    }

    /// Determines whether the given runner is capable of executing the given job. This should look
    /// at the list of ScheduledJobRequirement values that the job has and make sure that the runner
    /// is compatible with all of them.
    fn could_runner_execute_job(&self, runner: &Uuid, job: &ScheduledJob) -> bool {
        let Some(runner_capabilities) = self.available_runners.get(runner) else {
            // this shouldn't happen, but...
            return false;
        };

        job.requirements
            .iter()
            .all(|req| runner_capabilities.contains(req))
    }

    fn active_jobs(&self) -> Vec<&ScheduledJob> {
        self.active_jobs.iter().collect()
    }

    fn finished_jobs(&self) -> Vec<&ScheduledJob> {
        self.finished_jobs.iter().collect()
    }

    fn tick(&mut self) {
        // 1. handle timed-out jobs
        let now = SystemTime::now();
        let mut jobs_to_fail = vec![];
        for job in self.active_jobs.iter_mut() {
            match job.state {
                ScheduledJobState::InProgress { deadline, runner } if deadline < now => {
                    if job.attempts < MAX_JOB_ATTEMPTS {
                        // Give it another go
                        log::info!(
                            "Job {} took too long; moving it back into the work queue",
                            job.id
                        );
                        job.state = ScheduledJobState::Unassigned;
                    } else {
                        log::info!("Job {} failed too many times; giving up", job.id);
                        job.state = ScheduledJobState::Failed {
                            runner,
                            failure_time: SystemTime::now(),
                            output: None,
                        };
                        jobs_to_fail.push(job.id);
                    }
                }
                _ => {}
            }
        }
        for id in jobs_to_fail.into_iter() {
            let Some(idx) = self.active_jobs.iter().position(|job| job.id == id) else {
                // This should not happen, but computers ¯\_(ツ)_/¯
                continue;
            };
            let job = self.active_jobs.swap_remove(idx);
            self.finished_jobs.push(job);
        }

        // 2. Assign pending jobs to available runners
        if !self.available_runners.is_empty() {
            let mut available_job_ids: Vec<_> = self
                .active_jobs
                .iter()
                .filter(|job| job.state == ScheduledJobState::Unassigned)
                .map(|job| job.id)
                .collect();
            let deadline = SystemTime::now() + MAX_JOB_DURATION;
            let mut available_runners: Vec<_> = self.available_runners.keys().collect();
            let mut runners_to_remove = vec![];

            // todo: pull jobs out in priority order
            for job_id in available_job_ids.drain(..) {
                if available_runners.is_empty() {
                    // No runners to assign work to
                    break;
                }

                for runner_id in available_runners.clone() {
                    let job = self.job(&job_id).expect("Failed to get job");
                    if !self.could_runner_execute_job(runner_id, job) {
                        // This runner doesn't have soemthing that the job requires
                        continue;
                    }
                    if job.runners.contains(runner_id) {
                        // This runner has already had a shot at this job
                        continue;
                    }

                    log::info!("Assigned job {} to runner {}", job.id, runner_id);
                    let idx = available_runners
                        .iter()
                        .position(|r| *r == runner_id)
                        .expect("Failed to find runner");
                    available_runners.swap_remove(idx);
                    runners_to_remove.push(*runner_id);

                    let mut job = self
                        .active_jobs
                        .iter_mut()
                        .find(|j| j.id == job_id)
                        .expect("Failed to get job");

                    job.attempts += 1;
                    job.state = ScheduledJobState::InProgress {
                        runner: runner_id.to_owned(),
                        deadline,
                    };
                    job.runners.push(runner_id.to_owned());
                    break;
                }
            }
            for runner_id in runners_to_remove.into_iter() {
                self.available_runners.remove(&runner_id);
            }
        }

        // 3. Create a timeout to run tick again even if no calls to enqueue or register_runner
        // occur.
        let next_deadline = self
            .active_jobs
            .iter()
            .filter_map(|job| match job.state {
                ScheduledJobState::InProgress { deadline, .. } => Some(deadline),
                _ => None,
            })
            .min();
        log::info!("Should tick again no later than {next_deadline:?}");
        // todo: actually implement this timeout somehow
    }
}

#[cfg(test)]
mod test {
    use std::time::{Duration, SystemTime};

    use ssri::Integrity;
    use uuid::Uuid;

    use super::JobScheduler;
    use crate::scheduler::{ScheduledJobState, ServalSchedulerError, MAX_JOB_DURATION};

    fn simulate_timeout(scheduler: &mut JobScheduler, job_id: &Uuid) {
        let job = scheduler.job_mut(job_id).expect("Failed to get job");
        match job.state {
            ScheduledJobState::InProgress { runner, .. } => {
                job.state = ScheduledJobState::InProgress {
                    deadline: SystemTime::now() - (MAX_JOB_DURATION + Duration::from_secs(1)),
                    runner,
                }
            }
            _ => panic!(),
        }
        scheduler.tick();
    }

    #[test]
    fn test() {
        let mut scheduler = JobScheduler::new();
        let job1 = scheduler
            .enqueue_job(
                Integrity::from(b"manifest1"),
                Some(Integrity::from(b"input1")),
                vec![],
            )
            .unwrap();
        let job2 = scheduler
            .enqueue_job(Integrity::from(b"manifest2"), None, vec![])
            .unwrap();

        assert_eq!(2, scheduler.active_jobs().len());
        assert_eq!(0, scheduler.finished_jobs().len());
        assert!(scheduler.job(&job1).is_some());
        assert!(scheduler.job(&job2).is_some());

        // Tick with no runners: nothing should happen
        scheduler.tick();
        assert_eq!(2, scheduler.active_jobs().len());
        assert_eq!(0, scheduler.finished_jobs().len());

        // Register a runner
        let runner1 = Uuid::new_v4();
        scheduler.register_runner(runner1, vec![]);

        // job1 should've been assigned to the runner
        assert!(matches!(
            scheduler.job(&job1).unwrap().state,
            ScheduledJobState::InProgress {
                runner,
                ..
            } if runner == runner1
        ));
        assert!(matches!(
            scheduler.job(&job2).unwrap().state,
            ScheduledJobState::Unassigned
        ));

        // Let's mark the job as complete
        scheduler
            .mark_job_completed(&job1, Some(Integrity::from(b"output2")))
            .unwrap();
        // trying to change its state a second time should not work
        assert_eq!(
            scheduler.mark_job_completed(&job1, Some(Integrity::from(b"output2"))),
            Err(ServalSchedulerError::InvalidOperationForJobState)
        );
        assert_eq!(
            scheduler.mark_job_failed(&job1, Some(Integrity::from(b"output2"))),
            Err(ServalSchedulerError::InvalidOperationForJobState)
        );

        // Register another runner and claim the other job, then simulate a timeout. Do this twice
        // to eat up our first two attempts:
        for attempt_num in 0..2 {
            assert_eq!(attempt_num, scheduler.job(&job2).unwrap().attempts);
            scheduler.register_runner(Uuid::new_v4(), vec![]);
            assert_eq!(attempt_num + 1, scheduler.job(&job2).unwrap().attempts);
            assert!(matches!(
                scheduler.job(&job2).unwrap().state,
                ScheduledJobState::InProgress { .. }
            ));
            simulate_timeout(&mut scheduler, &job2);
            assert_eq!(
                ScheduledJobState::Unassigned,
                scheduler.job(&job2).unwrap().state
            );
        }
        // Now, use our final attempt and make sure we end up in the failed state
        assert_eq!(2, scheduler.job(&job2).unwrap().attempts);
        let final_runner = Uuid::new_v4();
        scheduler.register_runner(final_runner, vec![]);
        assert_eq!(3, scheduler.job(&job2).unwrap().attempts);
        assert!(matches!(
            scheduler.job(&job2).unwrap().state,
            ScheduledJobState::InProgress { .. }
        ));
        simulate_timeout(&mut scheduler, &job2);
        match scheduler.job(&job2).unwrap().state {
            ScheduledJobState::Failed { runner, .. } => {
                assert_eq!(runner, final_runner);
            }
            _ => panic!(),
        };
    }
}
