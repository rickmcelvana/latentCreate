import { useEffect } from 'react'
import { JobQueue } from '../components/JobQueue'
import { useJobsStore } from '../state/jobs'

export function AudioStudio() {
  const startListening = useJobsStore((state) => state.startListening)

  useEffect(() => {
    void startListening()
  }, [startListening])

  return (
    <>
      <h1 className="view-title">Audio</h1>
      <p className="view-subtitle">
        Style tags, lyrics, and the settings worth changing.
      </p>
      <JobQueue />
      <div className="panel muted">
        No generations yet. Finish Setup to enable audio.
      </div>
    </>
  )
}
