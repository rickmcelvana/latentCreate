import { beforeEach, describe, expect, it, vi } from 'vitest'
import imageProfile from '../../../testdata/profiles/flux2-klein-9b-image.json'
import type { ProfileInputs } from '../bridge/profiles'
import { useArtPanelStore } from './paramPanel'
import { useGenerateArtStore } from './artGenerate'

const imageInputs = imageProfile.inputs as unknown as ProfileInputs

let mockIsTauri = true
let mockGenerateImage = vi.fn()
let mockRegister = vi.fn()

vi.mock('../bridge/comfy', () => ({ isTauri: () => mockIsTauri }))
vi.mock('../bridge/profiles', () => ({
  getProfileInputs: () => Promise.resolve(imageInputs),
  getEnumChoices: () => Promise.resolve({}),
}))
vi.mock('../bridge/generate', () => ({
  generateImage: (spec: unknown) => mockGenerateImage(spec),
}))
vi.mock('./jobs', () => ({
  useJobsStore: {
    getState: () => ({ register: (id: string, profileId: string) => mockRegister(id, profileId) }),
  },
}))

beforeEach(() => {
  mockIsTauri = true
  mockGenerateImage = vi.fn()
  mockRegister = vi.fn()
  useArtPanelStore.setState({
    profileId: null,
    model: null,
    values: {},
    showAdvanced: false,
    seedPinned: false,
    error: null,
    busy: false,
  })
  useGenerateArtStore.setState({
    busy: false,
    error: null,
    last: null,
    lastProfileId: null,
    count: 1,
    queued: 0,
    title: null,
  })
})

function seedFrom(spec: unknown): number {
  return (spec as { inputs: Record<string, { type: string; value: number }> }).inputs.seed.value
}

describe('useGenerateArtStore', () => {
  it('test_submit_queues_one_job_per_variation_and_registers_every_prompt_id', async () => {
    let callCount = 0
    mockGenerateImage = vi.fn(async () => ({
      prompt_id: `prompt-${++callCount}`,
      workflow_path: '/tmp/wf.json',
      unchecked_slots: [],
      lora_nodes: [],
      output_format: 'png',
    }))

    await useArtPanelStore.getState().load('flux2-klein-9b')
    useGenerateArtStore.getState().setCount(4)
    await useGenerateArtStore.getState().submit()

    expect(mockGenerateImage).toHaveBeenCalledTimes(4)
    expect(mockRegister).toHaveBeenCalledTimes(4)
    expect(mockRegister).toHaveBeenNthCalledWith(1, 'prompt-1', 'flux2-klein-9b')
    expect(mockRegister).toHaveBeenNthCalledWith(2, 'prompt-2', 'flux2-klein-9b')
    expect(mockRegister).toHaveBeenNthCalledWith(3, 'prompt-3', 'flux2-klein-9b')
    expect(mockRegister).toHaveBeenNthCalledWith(4, 'prompt-4', 'flux2-klein-9b')
  })

  it('test_specs_carry_no_loras_and_no_lyric_ref', async () => {
    mockGenerateImage = vi.fn(async () => ({
      prompt_id: 'prompt-1',
      workflow_path: '/tmp/wf.json',
      unchecked_slots: [],
      lora_nodes: [],
      output_format: 'png',
    }))

    await useArtPanelStore.getState().load('flux2-klein-9b')
    await useGenerateArtStore.getState().submit()

    const spec = mockGenerateImage.mock.calls[0][0]
    expect(spec.loras).toEqual([])
    expect(spec.lyrics).toBeNull()
  })

  it('test_each_variation_gets_a_different_seed', async () => {
    mockGenerateImage = vi.fn(async () => ({
      prompt_id: 'prompt-x',
      workflow_path: '/tmp/wf.json',
      unchecked_slots: [],
      lora_nodes: [],
      output_format: 'png',
    }))

    await useArtPanelStore.getState().load('flux2-klein-9b')
    useGenerateArtStore.getState().setCount(4)
    await useGenerateArtStore.getState().submit()

    const seeds = mockGenerateImage.mock.calls.map((call) => seedFrom(call[0]))
    expect(new Set(seeds).size).toBe(4)
  })

  it('test_unpinned_seed_is_rerolled_on_submit_pinned_is_kept', async () => {
    // No `crypto` stub. `global.crypto` is getter-only here, and nothing below
    // needs a known seed: every assertion is about whether the value *changed*
    // and whether the spec carries the one on screen, which the real generator
    // answers. A stub would only add a global to restore.
    {
      mockGenerateImage = vi.fn(async () => ({
        prompt_id: 'prompt-x',
        workflow_path: '/tmp/wf.json',
        unchecked_slots: [],
        lora_nodes: [],
        output_format: 'png',
      }))

      await useArtPanelStore.getState().load('flux2-klein-9b')
      const beforeSeed = useArtPanelStore.getState().values.seed
      expect(useArtPanelStore.getState().seedPinned).toBe(false)

      await useGenerateArtStore.getState().submit()

      const afterSeed = useArtPanelStore.getState().values.seed
      expect(afterSeed).not.toBe(beforeSeed)
      expect(useArtPanelStore.getState().seedPinned).toBe(false)
      expect(seedFrom(mockGenerateImage.mock.calls[0][0])).toBe(afterSeed)

      useArtPanelStore.getState().setValue('seed', afterSeed)
      expect(useArtPanelStore.getState().seedPinned).toBe(true)
      await useGenerateArtStore.getState().submit()
      expect(seedFrom(mockGenerateImage.mock.calls[1][0])).toBe(afterSeed)
    }
  })

  it('test_title_reaches_the_spec_and_empty_becomes_null', async () => {
    mockGenerateImage = vi.fn(async () => ({
      prompt_id: 'prompt-1',
      workflow_path: '/tmp/wf.json',
      unchecked_slots: [],
      lora_nodes: [],
      output_format: 'png',
    }))

    await useArtPanelStore.getState().load('flux2-klein-9b')
    useGenerateArtStore.getState().setTitle('My Art')
    await useGenerateArtStore.getState().submit()
    expect(mockGenerateImage.mock.calls[0][0].title).toBe('My Art')

    useGenerateArtStore.getState().setTitle('   ')
    await useGenerateArtStore.getState().submit()
    expect(mockGenerateImage.mock.calls[1][0].title).toBeNull()
  })

  it('test_failing_generate_image_leaves_error_verbatim_busy_false_and_does_not_clear_last', async () => {
    mockGenerateImage = vi.fn(async () => ({
      prompt_id: 'prompt-1',
      workflow_path: '/tmp/wf.json',
      unchecked_slots: [],
      lora_nodes: [],
      output_format: 'png',
    }))

    await useArtPanelStore.getState().load('flux2-klein-9b')
    await useGenerateArtStore.getState().submit()
    const firstLast = useGenerateArtStore.getState().last
    expect(firstLast).not.toBeNull()

    mockGenerateImage = vi.fn(async () => {
      throw new Error('ComfyUI refused: not an image profile')
    })
    await useGenerateArtStore.getState().submit()

    expect(useGenerateArtStore.getState().error).toBe(
      'Error: ComfyUI refused: not an image profile',
    )
    expect(useGenerateArtStore.getState().busy).toBe(false)
    expect(useGenerateArtStore.getState().last).toEqual(firstLast)
  })

  it('test_second_click_while_busy_queues_nothing', async () => {
    let resolveFirst: (() => void) | undefined
    mockGenerateImage = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveFirst = resolve
        }),
    )

    await useArtPanelStore.getState().load('flux2-klein-9b')
    const first = useGenerateArtStore.getState().submit()
    await Promise.resolve()
    const second = useGenerateArtStore.getState().submit()

    expect(mockGenerateImage).toHaveBeenCalledTimes(1)
    expect(useGenerateArtStore.getState().busy).toBe(true)

    resolveFirst!()
    await first
    await second

    expect(useGenerateArtStore.getState().busy).toBe(false)
  })

  it('test_submits_are_sequential_not_concurrent', async () => {
    let concurrent = 0
    let maxConcurrent = 0
    let callCount = 0
    mockGenerateImage = vi.fn(async () => {
      concurrent++
      maxConcurrent = Math.max(maxConcurrent, concurrent)
      await new Promise((r) => setTimeout(r, 5))
      concurrent--
      return {
        prompt_id: `prompt-${++callCount}`,
        workflow_path: '/tmp/wf.json',
        unchecked_slots: [],
        lora_nodes: [],
        output_format: 'png',
      }
    })

    await useArtPanelStore.getState().load('flux2-klein-9b')
    useGenerateArtStore.getState().setCount(4)
    await useGenerateArtStore.getState().submit()

    expect(maxConcurrent).toBe(1)
    expect(mockGenerateImage).toHaveBeenCalledTimes(4)
  })
})
