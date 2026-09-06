/**
 * The quick-swap dropdown: one label, one `<select>`, one optional note.
 *
 * Used by every screen that *changes* what is already configured -- the two
 * studios' models, the Lyrics model, the Library's project. Deliberately dumb:
 * it renders `options` and reports a choice, and every rule about which options
 * exist and what the note says lives in `state/`, where a node-only test suite
 * can reach it.
 *
 * The full row -- licence, VRAM claim, readiness, install -- is Setup's job.
 * This control exists to swap fast while generating, so it carries names only.
 */
export function QuickSwap({
  label,
  value,
  options,
  onChange,
  note,
  disabled,
  emptyLabel,
}: {
  label: string
  /** The chosen id, or null when nothing is chosen. */
  value: string | null
  options: { id: string; name: string }[]
  onChange: (id: string) => void
  /** A sentence under the control, or null. */
  note?: string | null
  disabled?: boolean
  /** The placeholder shown when nothing is chosen yet. */
  emptyLabel?: string
}) {
  const empty = options.length === 0
  return (
    <div className="quick-swap">
      <div className="quick-swap-row">
        <label className="quick-swap-label" htmlFor={`quick-swap-${label}`}>
          {label}
        </label>
        <select
          id={`quick-swap-${label}`}
          className="quick-swap-select"
          // A `<select>` cannot hold null. The empty string is the placeholder
          // option's value, so "nothing chosen" is a real, selectable state
          // rather than the first model silently standing in for one.
          value={value ?? ''}
          disabled={disabled === true || empty}
          onChange={(event) => {
            if (event.target.value !== '') onChange(event.target.value)
          }}
        >
          {value === null || empty ? (
            <option value="">{empty ? (emptyLabel ?? 'Nothing to choose') : 'Choose…'}</option>
          ) : null}
          {options.map((option) => (
            <option key={option.id} value={option.id}>
              {option.name}
            </option>
          ))}
        </select>
      </div>
      {note !== null && note !== undefined ? <p className="quick-swap-note">{note}</p> : null}
    </div>
  )
}
