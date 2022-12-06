use anyhow::anyhow;
use chrono::{DateTime, Duration, Utc};
use once_cell::sync::OnceCell;
use std::sync::Mutex;
use uuid::Uuid;

// TODO: something better than a type alias, per https://lexi-lambda.github.io/blog/2019/11/05/parse-don-t-validate/
type StorageAddress = String;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobStatus {
    Pending,
    Active,
    Completed,
    Failed,
}

#[derive(Clone, Debug)]
pub struct Job {
    id: Uuid,
    status: JobStatus,
    binary_addr: StorageAddress,
    input_addr: Option<StorageAddress>,
    output_addr: Option<StorageAddress>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    run_attempts: usize,
    runner_id: Option<Uuid>,
}

const ABANDONED_AGE_SECS: i64 = 300;
const MAX_ATTEMPTS: usize = 3;

type JobQueue = Vec<Job>;
static JOB_QUEUE: OnceCell<Mutex<JobQueue>> = OnceCell::new();

fn get_job_queue() -> &'static Mutex<JobQueue> {
    JOB_QUEUE.get_or_init(|| Mutex::new(vec![]))
}

fn detect_abandoned_jobs() {
    let mut queue = get_job_queue().lock().unwrap();
    let now = Utc::now();
    let is_abandoned = |job: &&mut Job| {
        let time_since_update = (now - job.updated_at).num_seconds();
        job.status == JobStatus::Active && time_since_update > ABANDONED_AGE_SECS
    };

    for mut job in queue.iter_mut().filter(is_abandoned) {
        log::info!("Oh hey I found an abandoned job {job:?}");
        job.status = if job.run_attempts < MAX_ATTEMPTS {
            JobStatus::Pending
        } else {
            JobStatus::Failed
        };
        job.runner_id = None;
        job.updated_at = Utc::now();
    }
}

pub fn claim_job(runner_id: Uuid) -> Option<Job> {
    let mut queue = get_job_queue().lock().unwrap();

    let Some(unclaimed_job_idx) = queue
        .iter()
        .position(|job| job.status == JobStatus::Pending) else {
            return None
        };

    // Take the job out of the queue, update it, put a copy of the updated version back into the
    // queue, and return the (modified) original instance to the caller.
    // This is surely the worst possible way to do this, but I am running out of ideas. :sob:
    let Some(mut job) = queue.get_mut(unclaimed_job_idx) else {
        return None
    };

    job.run_attempts += 1;
    job.runner_id = Some(runner_id);
    job.status = JobStatus::Active;
    job.updated_at = Utc::now();

    Some(job.clone())
}

pub fn enqueue_job(
    binary_addr: StorageAddress,
    input_addr: Option<StorageAddress>,
) -> anyhow::Result<Uuid> {
    let now = Utc::now();
    let id = Uuid::new_v4();
    let job = Job {
        id,
        status: JobStatus::Pending,
        binary_addr,
        input_addr,
        output_addr: None,
        created_at: now,
        updated_at: now,
        completed_at: None,
        run_attempts: 0,
        runner_id: None,
    };
    let mut queue = get_job_queue().lock().unwrap();
    queue.push(job);

    Ok(id)
}

/*
TODO
- Defaults
*/

pub fn tickle_job(job_id: &Uuid) -> anyhow::Result<()> {
    with_job(job_id, &mut |job| {
        if job.status != JobStatus::Active {
            return Err(anyhow!("Only active jobs may be tickled"));
        }

        job.updated_at = Utc::now();
        Ok(())
    })
}

// pub fn get_job(job_id: Uuid) -> anyhow::Result<&'static Job> {
//     let mutex = get_job_queue().lock().unwrap();
//     let queue: &JobQueue = &mutex;
//     let job = queue.iter().find(|job| job.id == job_id);
//     let job = job.ok_or(anyhow!("No such job"))?;
//     Err(anyhow!(""))
// }

fn with_job(
    job_id: &Uuid,
    callback: &mut dyn FnMut(&mut Job) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let mut queue = get_job_queue().lock().unwrap();
    let job = queue.iter_mut().find(|job| job.id == *job_id);
    match job {
        Some(job) => callback(job),
        None => Err(anyhow!("No such job")),
    }
}

pub fn complete_job(job_id: &Uuid, output_addr: &Option<StorageAddress>) -> anyhow::Result<()> {
    with_job(job_id, &mut |job| {
        if job.status != JobStatus::Active {
            return Err(anyhow!("Only active jobs may be completed"));
        }

        job.status = JobStatus::Completed;
        job.output_addr = output_addr.to_owned();
        job.completed_at = Some(Utc::now());

        Ok(())
    })
}

pub fn fail_job(job_id: &Uuid, output_addr: &Option<StorageAddress>) -> anyhow::Result<()> {
    with_job(job_id, &mut |job| {
        if job.status != JobStatus::Active {
            return Err(anyhow!("Only active jobs may be failed"));
        }

        job.status = JobStatus::Failed;
        job.output_addr = output_addr.to_owned();
        job.completed_at = Some(Utc::now());

        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_queue_len() -> usize {
        get_job_queue().lock().unwrap().len()
    }

    #[test]
    fn test_everything() {
        let runner_id = Uuid::parse_str("26DB349E-E0E9-48DA-9B00-0FF9F2ED2FAA").unwrap();

        // queue is empty, nothing to claim
        assert!(get_queue_len() == 0);
        assert!(claim_job(runner_id).is_none());

        // Enqueue some jobs
        println!("len a {}", get_queue_len());
        let job1_id = enqueue_job(
            String::from("c16c8ad5430916385abee7fbcf0940c458d33024"),
            Some(String::from("eacf14915b010acd192b1096228ee5feeb4d9eb0")),
        )
        .unwrap();
        assert!(with_job(&job1_id, &mut |job| {
            assert!(job.status == JobStatus::Pending);
            assert!(job.binary_addr == String::from("c16c8ad5430916385abee7fbcf0940c458d33024"));
            assert!(
                job.input_addr == Some(String::from("eacf14915b010acd192b1096228ee5feeb4d9eb0"))
            );

            Ok(())
        })
        .is_ok());

        let job2_id = enqueue_job(
            String::from("c16c8ad5430916385abee7fbcf0940c458d33024"),
            Some(String::from("eacf14915b010acd192b1096228ee5feeb4d9eb0")),
        )
        .unwrap();

        // Make sure you can't complete or fail a job that is pending
        assert!(complete_job(&job1_id, &None).is_err());
        assert!(complete_job(&job2_id, &None).is_err());
        assert!(fail_job(&job1_id, &None).is_err());
        assert!(fail_job(&job2_id, &None).is_err());

        // Make sure they get de-queued in the expected order
        let job1 = claim_job(runner_id);
        let job2 = claim_job(runner_id);
        assert!(job1.is_some());
        assert!(job2.is_some());
        let job1 = job1.unwrap();
        let job2 = job2.unwrap();
        assert!(job1.id.eq(&job1_id));
        assert!(job2.id.eq(&job2_id));
        assert!(job1.runner_id == Some(runner_id));
        assert!(job2.runner_id == Some(runner_id));

        // both of the jobs in the queue have already been claimed
        assert!(claim_job(runner_id).is_none());

        // now, make one of them look abandoned
        with_job(&job1.id, &mut |job| {
            println!("Making job look old {job:?}");
            job.updated_at = Utc::now() - Duration::seconds(ABANDONED_AGE_SECS + 1);
            Ok(())
        })
        .unwrap();
        assert!(job1.status == JobStatus::Active);
        detect_abandoned_jobs();
        with_job(&job1.id, &mut |job| {
            assert!(job.status == JobStatus::Pending);
            assert!(job.runner_id == None);
            Ok(())
        })
        .unwrap();

        // test reclaiming a previously abandoned job
        let reclaimed_job1 = claim_job(runner_id).unwrap();
        assert!(reclaimed_job1.id == job1.id);

        // test a job that has been abandoned too many times
        with_job(&job1.id, &mut |job| {
            job.updated_at = Utc::now() - Duration::seconds(ABANDONED_AGE_SECS + 1);
            job.run_attempts = MAX_ATTEMPTS;
            Ok(())
        })
        .unwrap();
        detect_abandoned_jobs();
        with_job(&job1.id, &mut |job| {
            assert!(job.status == JobStatus::Failed);
            Ok(())
        })
        .unwrap();
        assert!(claim_job(runner_id).is_none());

        // we should not be able to complete that abandoned job, because it's now marked as failed
        assert!(complete_job(&job1.id, &None,).is_err());

        // test tickling a job
        with_job(&job2.id, &mut |job| {
            job.updated_at = Utc::now() - Duration::seconds(1);
            Ok(())
        })
        .unwrap();
        let time_before = job2.updated_at;
        assert!(tickle_job(&job2.id).is_ok());
        with_job(&job2.id, &mut |job| {
            assert!(job.updated_at > time_before);
            Ok(())
        })
        .unwrap();

        // test completing a job successfully
        assert!(complete_job(
            &job2.id,
            &Some(String::from("84990611d561094669b8096597917f917e8042bf")),
        )
        .is_ok());
        with_job(&job2_id, &mut |job| {
            assert!(job.status == JobStatus::Completed);
            assert!(
                job.output_addr == Some(String::from("84990611d561094669b8096597917f917e8042bf"))
            );
            Ok(())
        })
        .unwrap();
        // (but you should only be able to complete it once)
        assert!(complete_job(
            &job2.id,
            &Some(String::from("84990611d561094669b8096597917f917e8042bf")),
        )
        .is_err());

        // test explicitly failing a job
        let job3_id = enqueue_job(
            String::from("f680f2e4be898a7adda36d524d9c5e4e6a70f375"),
            Some(String::from("f9fa607695fb3145920d3d5f5ce231e58345f42f")),
        )
        .unwrap();
        assert!(claim_job(runner_id).is_some());
        assert!(fail_job(
            &job3_id,
            &Some(String::from("9e9952ff277a803f0d9ae831d776303dfafca818"))
        )
        .is_ok());
        with_job(&job3_id, &mut |job| {
            assert!(job.status == JobStatus::Failed);
            assert!(job.runner_id == Some(runner_id));
            assert!(
                job.output_addr == Some(String::from("9e9952ff277a803f0d9ae831d776303dfafca818"))
            );
            Ok(())
        })
        .unwrap();
    }
}
