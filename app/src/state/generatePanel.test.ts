import { beforeEach, describe, expect, it, vi } from 'vitest'
import aceProfile from '../../../profiles/ace-step-1.5-turbo.json'
import type { GenerationSpec, Submission } from '../bridge/generate'
import type { ProfileInputs } from '../bridge/profiles'
import { useGenerateStore } from './generatePanel'
import { applyJobEvent, useJobsStore } from './jobs'
import { useLoraPanelStore } from './loraPanel'
import { useLyricsStore } from './lyrics'
import { defaults, panelModel } from './params'
import { useParamPanelStore } from './paramPanel'

const model = panelModel(aceProfile.inputs as unknown as ProfileInputs)

let sent: GenerationSpec | null = null
let reply: Submission | Error = {
  prompt_id: 'prompt-abc',
  workflow_path: 'C:/jobs/1/workflow.json',
  unchecked_slots: [],
  lora_nodes: [],
  output_format: 'flac',
}
let callCount = 0
let failAfter: number | null = null
let inFlight = 0
let maxInFlight = 0

vi.mock('../bridge/comfy', () => ({ isTauri: () => true }))
vi.mock('../bridge/generate', () => ({
  generateAudio: async (spec: GenerationSpec) => {
    sent = spec
    callCount++
    inFlight++
    maxInFlight = Math.max(maxInFlight, inFlight)
    if (failAfter !== null && callCount > failAfter) {
      inFlight--
      return Promise.reject(new Error('mid-batch failure'))
    }
    if (reply instanceof Error) {
      inFlight--
      return Promise.reject(reply)
    }
    await Promise.resolve()
    inFlight--
    const promptId = callCount === 1 ? reply.prompt_id : `prompt-${callCount}`
    return { ...reply, prompt_id: promptId }
  },
}))

beforeEach(() => {
  sent = null
  reply = {
    prompt_id: 'prompt-abc',
    workflow_path: 'C:/jobs/1/workflow.json',
    unchecked_slots: [],
    lora_nodes: [],
    output_format: 'flac',
  }
  callCount = 0
  failAfter = null
  inFlight = 0
  maxInFlight = 0
  useGenerateStore.setState({
    busy: false,
    error: null,
    last: null,
    lastProfileId: null,
    count: 1,
    queued: 0,
  })
  useJobsStore.setState({ jobs: {} })
  useLoraPanelStore.setState({ stack: [] })
  useLyricsStore.setState({ doc: null })
  useParamPanelStore.setState({
    profileId: 'ace-step-1.5-turbo',
    model,
    values: { ...defaults(model), seed: 4242 },
    error: null,
  })
})

describe('useGenerateStore.submit', () => {
  it('test_submit_sends_the_assembled_spec', async () => {
    await useGenerateStore.getState().submit()

    expect(sent?.profile_id).toBe('ace-step-1.5-turbo')
    expect(sent?.inputs.seed).toEqual({ type: 'seed', value: 4242 })
    expect(useGenerateStore.getState().last?.prompt_id).toBe('prompt-abc')
    expect(useGenerateStore.getState().lastProfileId).toBe('ace-step-1.5-turbo')
    expect(useGenerateStore.getState().error).toBeNull()
  })

  /**
   * Protects: the queue hears about the job this store submitted.
   *
   * `generate_audio` starts the pump itself, so nothing goes through
   * `jobs.run` on this path -- and `applyJobEvent` ignores events for ids the
   * store does not know, which it must. Without `register` the two are correct
   * separately and deaf together: the generation runs to completion on the GPU
   * and every event is discarded, leaving an empty queue and no error anywhere.
   */
  it('test_the_submitted_job_is_registered_with_the_queue', async () => {
    await useGenerateStore.getState().submit()

    expect(useJobsStore.getState().jobs['prompt-abc']).toBeDefined()
    expect(useJobsStore.getState().jobs['prompt-abc'].status).toBe('queued')
  })

  /**
   * Protects: the events actually land afterwards.
   *
   * "The id is in the map" and "the pump's events now reach the queue" are
   * different claims, and only the second one is the point of registering. This
   * drives a real event through the same reducer the listener uses.
   */
  it('test_pump_events_reach_the_registered_job', async () => {
    await useGenerateStore.getState().submit()

    const running = applyJobEvent(useJobsStore.getState().jobs, {
      kind: 'progress',
      payload: { id: 'prompt-abc', status: 'running', outputs: [] },
    })
    expect(running['prompt-abc'].status).toBe('running')

    const done = applyJobEvent(running, {
      kind: 'done',
      payload: { id: 'prompt-abc', outputs: ['out.flac'] },
    })
    expect(done['prompt-abc'].status).toBe('completed')
    expect(done['prompt-abc'].outputs).toEqual(['out.flac'])
  })

  /** Protects: an unregistered id is still ignored -- `register` is the fix, not a loosened reducer. */
  it('test_an_unregistered_job_is_still_ignored', () => {
    const after = applyJobEvent(
      {},
      { kind: 'progress', payload: { id: 'someone-elses', status: 'running', outputs: [] } },
    )

    expect(after).toEqual({})
  })

  /**
   * Protects: an unusable seed is not submitted.
   *
   * The button is disabled on the same rule, but a button is a view; this is
   * the layer that decides what reaches Rust.
   */
  it('test_a_blocked_panel_submits_nothing', async () => {
    useParamPanelStore.setState({
      values: { ...defaults(model), seed: '18446744073709551615' },
    })

    await useGenerateStore.getState().submit()

    expect(sent).toBeNull()
    expect(useJobsStore.getState().jobs).toEqual({})
  })

  /**
   * Protects: the command's error reaches the user whole.
   *
   * Verbatim and on its own -- the param panel once shipped comfy-cli's raw
   * transport error spliced into the middle of a sentence, and a person reading
   * the screen is what found it.
   */
  it('test_a_failed_submit_keeps_the_error_verbatim', async () => {
    reply = new Error('cannot reach http://127.0.0.1:8188/prompt')

    await useGenerateStore.getState().submit()
    const state = useGenerateStore.getState()

    expect(state.error).toContain('cannot reach http://127.0.0.1:8188/prompt')
    expect(state.last).toBeNull()
    expect(state.busy).toBe(false)
    expect(useJobsStore.getState().jobs).toEqual({})
  })

  /**
   * Protects: two clicks do not queue two jobs.
   *
   * They would run with the same seed and, because ACE-Step is not reproducible
   * run-to-run (MCP-SURFACE 17.3), would not even be the same track.
   */
  it('test_a_submit_in_flight_blocks_a_second', async () => {
    useGenerateStore.setState({ busy: true })

    await useGenerateStore.getState().submit()

    expect(sent).toBeNull()
  })

  /** Protects: registering the same id twice does not reset a running job. */
  it('test_registering_twice_keeps_the_existing_job', () => {
    useJobsStore.getState().register('prompt-abc', 'ace-step-1.5-turbo')
    useJobsStore.setState((s) => ({
      jobs: { ...s.jobs, 'prompt-abc': { ...s.jobs['prompt-abc'], status: 'running' } },
    }))

    useJobsStore.getState().register('prompt-abc', 'minimax-music-3')

    expect(useJobsStore.getState().jobs['prompt-abc'].status).toBe('running')
    expect(useJobsStore.getState().jobs['prompt-abc'].profileId).toBe('ace-step-1.5-turbo')
  })

  /**
   * Protects: the job the queue registers knows which model produced it.
   *
   * The value has to survive the hop from the param panel through `submit` to
   * the store -- a test that only asserts the id is in the map passes while
   * `profileId` arrives `undefined`, and the row then renders blank with
   * nothing failing anywhere. Same shape as the `register` call itself, which
   * three tested seams once met without.
   */
  it('test_a_submitted_job_carries_the_profile_it_was_generated_for', async () => {
    await useGenerateStore.getState().submit()

    const job = useJobsStore.getState().jobs['prompt-abc']
    expect(job).toBeDefined()
    expect(job.profileId).toBe('ace-step-1.5-turbo')
    expect(job.submittedAt).toBeGreaterThan(0)
  })

  /** Protects: a batch queues every spec and registers every job. */
  it('test_a_batch_submits_every_spec_and_registers_every_job', async () => {
    useGenerateStore.setState({ count: 4 })

    await useGenerateStore.getState().submit()

    expect(callCount).toBe(4)
    const jobs = useJobsStore.getState().jobs
    expect(Object.keys(jobs)).toHaveLength(4)
    expect(jobs['prompt-abc']).toBeDefined()
    expect(jobs['prompt-2']).toBeDefined()
    expect(jobs['prompt-3']).toBeDefined()
    expect(jobs['prompt-4']).toBeDefined()
  })

  /**
   * Protects: a partial batch keeps what was queued and does not wipe the note.
   *
   * `last` must survive a failure: two jobs are on the GPU and the screen should
   * still say so.
   */
  it('test_a_failure_midway_keeps_what_was_queued', async () => {
    useGenerateStore.setState({ count: 4 })
    failAfter = 2

    await useGenerateStore.getState().submit()

    const state = useGenerateStore.getState()
    expect(state.queued).toBe(2)
    expect(state.error).toContain('mid-batch failure')
    expect(state.last).not.toBeNull()
    expect(Object.keys(useJobsStore.getState().jobs)).toHaveLength(2)
  })

  /**
   * Protects: a retry that queues nothing reports zero, not the last success.
   *
   * `last` is deliberately kept through a failure (a partial batch is really
   * running), so `queued` is the only thing that can tell the bar this click
   * added nothing -- see `notesFor`'s zero guard.
   */
  it('test_a_click_that_queues_nothing_reports_zero', async () => {
    await useGenerateStore.getState().submit()
    expect(useGenerateStore.getState().queued).toBe(1)

    failAfter = 0
    await useGenerateStore.getState().submit()

    const state = useGenerateStore.getState()
    expect(state.queued).toBe(0)
    expect(state.error).toContain('mid-batch failure')
    expect(Object.keys(useJobsStore.getState().jobs)).toHaveLength(1)
  })

  /** Protects: jobs are submitted one at a time, never in parallel. */
  it('test_a_batch_submits_one_at_a_time', async () => {
    useGenerateStore.setState({ count: 4 })

    await useGenerateStore.getState().submit()

    expect(maxInFlight).toBe(1)
  })
})

describe('useGenerateStore.useApprovedLyric', () => {
  /** Protects: the offer fills the profile's lyrics control with the approved text. */
  it('test_the_approved_lyric_fills_the_lyrics_field', () => {
    useLyricsStore.setState({
      doc: {
        id: 'lyric-01',
        title: null,
        approved: 1,
        versions: [
          {
            number: 1,
            text: 'the approved words',
            created_at: '2026-08-28T00:00:00Z',
            source: { kind: 'human' },
          },
        ],
      },
    })

    useGenerateStore.getState().useApprovedLyric()

    expect(useParamPanelStore.getState().values.lyrics).toBe('the approved words')
  })

  /** Protects: nothing approved changes nothing. */
  it('test_no_approved_lyric_leaves_the_field_alone', () => {
    useParamPanelStore.setState({ values: { ...defaults(model), lyrics: 'mine' } })

    useGenerateStore.getState().useApprovedLyric()

    expect(useParamPanelStore.getState().values.lyrics).toBe('mine')
  })
})
