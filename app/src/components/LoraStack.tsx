import {
  ADD_PLACEHOLDER,
  EMPTY_STACK,
  addable,
  catalogNote,
  excludedNote,
  fullNote,
  pickerGroups,
  stackRows,
  supersededCount,
} from '../state/loras'
import { useLoraPanelStore } from '../state/loraPanel'

export function LoraStack() {
  const panel = useLoraPanelStore((s) => s.panel)
  const stack = useLoraPanelStore((s) => s.stack)
  const showSuperseded = useLoraPanelStore((s) => s.showSuperseded)
  const addPath = useLoraPanelStore((s) => s.addPath)
  const removeRow = useLoraPanelStore((s) => s.removeRow)
  const toggleRow = useLoraPanelStore((s) => s.toggleRow)
  const setStrength = useLoraPanelStore((s) => s.setStrength)
  const moveRow = useLoraPanelStore((s) => s.moveRow)
  const toggleSuperseded = useLoraPanelStore((s) => s.toggleSuperseded)
  const refresh = useLoraPanelStore((s) => s.refresh)

  if (panel === null) return null

  const note = catalogNote(panel.catalog)
  const offers = pickerGroups(panel.catalog, stack, showSuperseded)
  const rows = stackRows(stack, panel.catalog)
  const checkpoints = supersededCount(panel.catalog)
  const excluded = excludedNote(panel.catalog)

  return (
    <section className="panel lora-stack">
      <h2 className="lora-stack-title">LoRAs</h2>

      {note !== null ? (
        <p className="lora-stack-note">
          {note}
          <button
            type="button"
            className="lora-stack-retry"
            onClick={() => void refresh()}
          >
            Retry
          </button>
        </p>
      ) : null}

      {stack.length === 0 ? (
        <p className="lora-stack-empty">{EMPTY_STACK}</p>
      ) : (
        <ul className="lora-rows">
          {rows.map((view) => (
            <li
              key={view.row.path}
              className={`lora-row ${view.missing ? 'lora-row-missing' : ''} ${
                view.row.enabled ? '' : 'lora-row-bypassed'
              }`}
            >
              <span className="lora-row-label">{view.row.label}</span>

              <input
                type="range"
                className="lora-row-strength"
                min={panel.strength.min}
                max={panel.strength.max}
                step={panel.strength.step ?? 0.01}
                value={view.row.strength}
                onChange={(e) => setStrength(view.index, Number(e.target.value))}
                disabled={!view.row.enabled}
                aria-label={`${view.row.label} strength`}
              />
              <span className="lora-row-value">
                {view.row.strength.toFixed(2)}
              </span>

              <button
                type="button"
                className="lora-row-move"
                disabled={!view.canMoveUp}
                onClick={() => moveRow(view.index, view.index - 1)}
                aria-label="Move up"
              >
                ↑
              </button>
              <button
                type="button"
                className="lora-row-move"
                disabled={!view.canMoveDown}
                onClick={() => moveRow(view.index, view.index + 1)}
                aria-label="Move down"
              >
                ↓
              </button>

              <label className="lora-row-bypass">
                <input
                  type="checkbox"
                  checked={view.row.enabled}
                  onChange={() => toggleRow(view.index)}
                />
                On
              </label>

              <button
                type="button"
                className="lora-row-remove"
                onClick={() => removeRow(view.index)}
                aria-label={`Remove ${view.row.label}`}
              >
                Remove
              </button>

              {view.note !== null ? (
                <p className="lora-row-note">{view.note}</p>
              ) : null}
            </li>
          ))}
        </ul>
      )}

      <div className="lora-stack-add">
        <select
          className="lora-picker"
          value=""
          onChange={(e) => addPath(e.target.value)}
          disabled={!addable(panel, stack) || offers.length === 0}
        >
          <option value="">
            {addable(panel, stack) ? ADD_PLACEHOLDER : fullNote(panel)}
          </option>
          {offers.map((group) => (
            <optgroup key={group.label} label={group.label}>
              {group.entries.map((entry) => (
                <option key={entry.path} value={entry.path}>
                  {entry.display}
                </option>
              ))}
            </optgroup>
          ))}
        </select>
        <span className="lora-stack-count">
          {stack.length} of {panel.max_stack}
        </span>
      </div>

      {checkpoints > 0 ? (
        <button
          type="button"
          className="lora-checkpoints-toggle"
          aria-pressed={showSuperseded}
          onClick={toggleSuperseded}
        >
          Training checkpoints ({checkpoints})
        </button>
      ) : null}

      {excluded !== null ? (
        <p className="lora-stack-excluded">{excluded}</p>
      ) : null}
    </section>
  )
}
