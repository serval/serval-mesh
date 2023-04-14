#![allow(dead_code)]
use std::fs;
use std::path::PathBuf;

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

// TODO: something better than a type alias, per https://lexi-lambda.github.io/blog/2019/11/05/parse-don-t-validate/
type StorageAddress = String;

/// A representation of current job status.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum JobStatus {
    /// This job is pending.
    #[default]
    Pending,
    /// This job is currently being executed.
    Active,
    /// This job is complete.
    Completed,
    /// This job has failed all attempts at execution.
    Failed,
}

/// Job metadata, including run history.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Job {
    id: Uuid,
    status: JobStatus,
    binary_addr: StorageAddress,
    input_addr: Option<StorageAddress>,
    output_addr: Option<StorageAddress>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    completed_at: Option<OffsetDateTime>,
    run_attempts: usize,
    runner_id: Option<Uuid>,
}

const ABANDONED_AGE_SECS: i64 = 300;
const MAX_ATTEMPTS: usize = 3;

/// A temporary in-memory job queue implementation.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct JobQueue {
    persist_filename: Option<PathBuf>,
    queue: Vec<Job>,
}

impl JobQueue {
    /// Create a new job queue. If you provide a path to a writeable file, use that as storage.
    /// Otherwise, the queue is in-memory only.
    pub fn new(persist_filename: Option<PathBuf>) -> JobQueue {
        // If we were given a persist_filename, then go read that file and use its contents as the
        // initial value of our queue.
        let queue: Vec<Job> = persist_filename
            .clone()
            .and_then(|filename| {
                let Ok(json_str) = fs::read_to_string(filename) else {
                    return None;
                };
                let Ok(queue_contents) = serde_json::from_str(&json_str) else {
                    return None;
                };
                queue_contents
            })
            .unwrap_or_default();

        JobQueue {
            persist_filename,
            queue,
        }
    }

    /// Claim a job from the queue, marking it as active.
    pub fn claim_job(&mut self, &runner_id: &Uuid) -> Option<Job> {
        let claim_result = {
            let Some(unclaimed_job_idx) = self.queue
            .iter()
            .position(|job| job.status == JobStatus::Pending) else {
                return None
            };

            // Take the job out of the queue, update it, put a copy of the updated version back into the
            // queue, and return the (modified) original instance to the caller.
            // This is surely the worst possible way to do this, but I am running out of ideas. :sob:
            let Some(mut job) = self.queue.get_mut(unclaimed_job_idx) else {
                return None
            };

            job.run_attempts += 1;
            job.runner_id = Some(runner_id.to_owned());
            job.status = JobStatus::Active;
            job.updated_at = OffsetDateTime::now_utc();

            Some(job.clone())
        };

        if claim_result.is_some() {
            self.maybe_persist();
        }

        claim_result
    }

    /// Move a job to the completed state.
    pub fn complete_job(
        &mut self,
        job_id: &Uuid,
        output_addr: &Option<StorageAddress>,
    ) -> anyhow::Result<()> {
        self.with_job(job_id, &mut |job| {
            if job.status != JobStatus::Active {
                return Err(anyhow!("Only active jobs may be completed"));
            }

            job.status = JobStatus::Completed;
            job.output_addr = output_addr.to_owned();
            job.completed_at = Some(OffsetDateTime::now_utc());

            Ok(())
        })
    }

    /// Sweep for abandoned jobs.
    pub fn detect_abandoned_jobs(&mut self) {
        let now = OffsetDateTime::now_utc();
        let is_abandoned = |job: &&mut Job| {
            let time_since_update = (now - job.updated_at).whole_seconds();
            job.status == JobStatus::Active && time_since_update > ABANDONED_AGE_SECS
        };

        let mut needs_persist = false;
        for mut job in self.queue.iter_mut().filter(is_abandoned) {
            needs_persist = true;

            job.status = if job.run_attempts < MAX_ATTEMPTS {
                JobStatus::Pending
            } else {
                JobStatus::Failed
            };
            job.runner_id = None;
            job.updated_at = OffsetDateTime::now_utc();
        }

        if needs_persist {
            self.maybe_persist();
        }
    }

    /// Add a job to the work queue.
    pub fn enqueue_job(
        &mut self,
        binary_addr: StorageAddress,
        input_addr: Option<StorageAddress>,
    ) -> anyhow::Result<Uuid> {
        let now = OffsetDateTime::now_utc();
        let id = Uuid::new_v4();
        let job = Job {
            id,
            created_at: now,
            updated_at: now,
            binary_addr,
            input_addr,
            status: JobStatus::Pending,
            completed_at: None,
            output_addr: None,
            run_attempts: 0,
            runner_id: None,
        };
        self.queue.push(job);

        self.maybe_persist();

        Ok(id)
    }

    /// Move a job to the failed state.
    pub fn fail_job(
        &mut self,
        job_id: &Uuid,
        output_addr: &Option<StorageAddress>,
    ) -> anyhow::Result<()> {
        self.with_job(job_id, &mut |job| {
            if job.status != JobStatus::Active {
                return Err(anyhow!("Only active jobs may be failed"));
            }

            job.status = JobStatus::Failed;
            job.output_addr = output_addr.to_owned();
            job.completed_at = Some(OffsetDateTime::now_utc());

            Ok(())
        })
    }

    /// Fetch job metadata by id.
    pub fn get_job(&self, job_id: &Uuid) -> anyhow::Result<Job> {
        let job = self.queue.iter().find(|job| job.id == *job_id);
        let job = job.ok_or_else(|| anyhow!("No such job"))?;

        Ok(job.clone())
    }

    fn maybe_persist(&self) {
        let Some(filename) = &self.persist_filename else {
            // Persistence is not configured
            return
        };

        log::info!("Persisting to {filename:?}");

        match serde_json::to_string(&self.queue) {
            Ok(data) => {
                if let Err(err) = fs::write(filename, data) {
                    log::warn!("Writing serialized queue to {filename:?} failed: {err:?}")
                }
            }
            Err(err) => log::warn!("Serializing queue to JSON failed: {err:?}"),
        }
    }

    /// Touch an active job to indicate it is still being processed.
    pub fn tickle_job(&mut self, job_id: &Uuid) -> anyhow::Result<()> {
        self.with_job(job_id, &mut |job| {
            if job.status != JobStatus::Active {
                return Err(anyhow!("Only active jobs may be tickled"));
            }

            job.updated_at = OffsetDateTime::now_utc();
            Ok(())
        })
    }

    fn with_job(
        &mut self,
        job_id: &Uuid,
        callback: &mut dyn FnMut(&mut Job) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let job = self.queue.iter_mut().find(|job| job.id == *job_id);
        let job = job.ok_or_else(|| anyhow!("No such job"))?;

        let res = callback(job);
        self.maybe_persist();

        res
    }
}

#[cfg(test)]
mod tests {
    use time::ext::NumericalDuration;

    use super::*;

    #[test]
    fn test_everything() {
        let mut job_queue = JobQueue::new(None);
        let runner_id = Uuid::parse_str("26DB349E-E0E9-48DA-9B00-0FF9F2ED2FAA").unwrap();

        // queue is empty, nothing to claim
        assert!(job_queue.queue.is_empty());
        assert!(job_queue.claim_job(&runner_id).is_none());

        // Enqueue some jobs
        println!("len a {}", job_queue.queue.len());
        let job1_id = job_queue
            .enqueue_job(
                String::from("c16c8ad5430916385abee7fbcf0940c458d33024"),
                Some(String::from("eacf14915b010acd192b1096228ee5feeb4d9eb0")),
            )
            .unwrap();
        assert!(job_queue
            .with_job(&job1_id, &mut |job| {
                assert!(job.status == JobStatus::Pending);
                assert!(job.binary_addr == *"c16c8ad5430916385abee7fbcf0940c458d33024");
                assert!(
                    job.input_addr
                        == Some(String::from("eacf14915b010acd192b1096228ee5feeb4d9eb0"))
                );

                Ok(())
            })
            .is_ok());

        let job2_id = job_queue
            .enqueue_job(
                String::from("c16c8ad5430916385abee7fbcf0940c458d33024"),
                Some(String::from("eacf14915b010acd192b1096228ee5feeb4d9eb0")),
            )
            .unwrap();

        // Make sure you can't complete or fail a job that is pending
        assert!(job_queue.complete_job(&job1_id, &None).is_err());
        assert!(job_queue.complete_job(&job2_id, &None).is_err());
        assert!(job_queue.fail_job(&job1_id, &None).is_err());
        assert!(job_queue.fail_job(&job2_id, &None).is_err());

        // Make sure they get de-queued in the expected order
        let job1 = job_queue.claim_job(&runner_id);
        let job2 = job_queue.claim_job(&runner_id);
        assert!(job1.is_some());
        assert!(job2.is_some());
        let job1 = job1.unwrap();
        let job2 = job2.unwrap();
        assert!(job1.id.eq(&job1_id));
        assert!(job2.id.eq(&job2_id));
        assert!(job1.runner_id == Some(runner_id));
        assert!(job2.runner_id == Some(runner_id));
        assert!(job1.created_at <= job2.created_at);

        // both of the jobs in the queue have already been claimed
        assert!(job_queue.claim_job(&runner_id).is_none());

        // now, make one of them look abandoned
        job_queue
            .with_job(&job1.id, &mut |job| {
                println!("Making job look old {job:?}");
                job.updated_at = OffsetDateTime::now_utc() - (ABANDONED_AGE_SECS + 1).seconds();
                Ok(())
            })
            .unwrap();
        assert!(job1.status == JobStatus::Active);
        job_queue.detect_abandoned_jobs();
        job_queue
            .with_job(&job1.id, &mut |job| {
                assert!(job.status == JobStatus::Pending);
                assert!(job.runner_id.is_none());
                Ok(())
            })
            .unwrap();

        // test reclaiming a previously abandoned job
        let reclaimed_job1 = job_queue.claim_job(&runner_id).unwrap();
        assert!(reclaimed_job1.id == job1.id);

        // test a job that has been abandoned too many times
        job_queue
            .with_job(&job1.id, &mut |job| {
                job.updated_at = OffsetDateTime::now_utc() - (ABANDONED_AGE_SECS + 1).seconds();
                job.run_attempts = MAX_ATTEMPTS;
                Ok(())
            })
            .unwrap();
        job_queue.detect_abandoned_jobs();
        job_queue
            .with_job(&job1.id, &mut |job| {
                assert!(job.status == JobStatus::Failed);
                Ok(())
            })
            .unwrap();
        assert!(job_queue.claim_job(&runner_id).is_none());

        // we should not be able to complete that abandoned job, because it's now marked as failed
        assert!(job_queue.complete_job(&job1.id, &None,).is_err());

        // test tickling a job
        job_queue
            .with_job(&job2.id, &mut |job| {
                job.updated_at = OffsetDateTime::now_utc() - 1.seconds();
                Ok(())
            })
            .unwrap();
        let time_before = job2.updated_at;
        assert!(job_queue.tickle_job(&job2.id).is_ok());
        job_queue
            .with_job(&job2.id, &mut |job| {
                assert!(job.updated_at > time_before);
                Ok(())
            })
            .unwrap();

        // test completing a job successfully
        assert!(job_queue
            .complete_job(
                &job2.id,
                &Some(String::from("84990611d561094669b8096597917f917e8042bf")),
            )
            .is_ok());
        job_queue
            .with_job(&job2_id, &mut |job| {
                assert!(job.status == JobStatus::Completed);
                assert!(
                    job.output_addr
                        == Some(String::from("84990611d561094669b8096597917f917e8042bf"))
                );
                Ok(())
            })
            .unwrap();
        // (but you should only be able to complete it once)
        assert!(job_queue
            .complete_job(
                &job2.id,
                &Some(String::from("84990611d561094669b8096597917f917e8042bf")),
            )
            .is_err());

        // test explicitly failing a job
        let job3_id = job_queue
            .enqueue_job(
                String::from("f680f2e4be898a7adda36d524d9c5e4e6a70f375"),
                Some(String::from("f9fa607695fb3145920d3d5f5ce231e58345f42f")),
            )
            .unwrap();
        assert!(job_queue.claim_job(&runner_id).is_some());
        assert!(job_queue
            .fail_job(
                &job3_id,
                &Some(String::from("9e9952ff277a803f0d9ae831d776303dfafca818"))
            )
            .is_ok());
        job_queue
            .with_job(&job3_id, &mut |job| {
                assert!(job.status == JobStatus::Failed);
                assert!(job.runner_id == Some(runner_id));
                assert!(
                    job.output_addr
                        == Some(String::from("9e9952ff277a803f0d9ae831d776303dfafca818"))
                );
                Ok(())
            })
            .unwrap();
    }
}
