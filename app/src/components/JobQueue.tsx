import { useEffect, useState } from 'react'
import { EMPTY_QUEUE, queueRows, type QueueRow } from '../state/queue'
import { useJobsStore } from '../state/jobs'

interface JobQueueProps {
  names: Record<string, string>
}

/** Renders the active job queue. Empty when nothing has been generated. */
export function JobQueue({ names }: JobQueueProps) {
  const jobs = useJobsStore((state) => state.jobs)
  const cancel = useJobsStore((state) => state.cancel)
  const [now, setNow] = useState(() => Date.now())

  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(id)
  }, [])

  const rows = queueRows(jobs, now, names)

  return (
    <section className="panel job-panel">
      <h2 className="job-panel-title">Queue</h2>
      {rows.length === 0 ? (
        <p className="job-empty">{EMPTY_QUEUE}</p>
      ) : (
        <ul className="job-queue">
          {rows.map((row) => (
            <JobRow key={row.id} row={row} onCancel={cancel} />
          ))}
        </ul>
      )}
    </section>
  )
}

function JobRow({ row, onCancel }: { row: QueueRow; onCancel: (id: string) => void }) {
  return (
    <li className={`job-item job-item-${row.status}`}>
      <span className="job-status">{row.label}</span>
      <span className="job-model">{row.model}</span>
      <span className="job-elapsed">{row.elapsed}</span>
      {row.error !== null ? <span className="job-error">{row.error}</span> : null}
      {row.canCancel ? (
        <button
          type="button"
          className="job-cancel"
          onClick={() => void onCancel(row.id)}
        >
          Cancel
        </button>
      ) : null}
    </li>
  )
}
