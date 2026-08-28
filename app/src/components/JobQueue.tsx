import { useJobsStore, type Job } from '../state/jobs'

/** Renders the active job queue. Empty when nothing has been generated. */
export function JobQueue() {
  const jobs = useJobsStore((state) => state.jobs)
  const cancel = useJobsStore((state) => state.cancel)
  const entries = Object.values(jobs)
  if (entries.length === 0) return null
  return (
    <ul className="job-queue">
      {entries.map((job) => (
        <JobItem key={job.id} job={job} onCancel={cancel} />
      ))}
    </ul>
  )
}

function JobItem({ job, onCancel }: { job: Job; onCancel: (id: string) => void }) {
  const running =
    job.status !== 'completed' && job.status !== 'failed' && job.status !== 'cancelled'
  return (
    <li className={`job-item job-item-${job.status}`}>
      <span className="job-status">{job.status}</span>
      {job.error !== null ? <span className="job-error">{job.error}</span> : null}
      {running ? (
        <button type="button" className="job-cancel" onClick={() => onCancel(job.id)}>
          Cancel
        </button>
      ) : null}
    </li>
  )
}
