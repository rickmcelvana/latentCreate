import type { ReactNode } from 'react'
import { groupsOf, seedError, type Control, type ControlValue } from '../state/params'
import { MAX_SAFE_SEED, type ParamPanelStore } from '../state/paramPanel'

export function ParamPanel({
  store,
  lyricAccessory,
}: {
  store: ParamPanelStore
  /** Rendered beside the **lyrics** field's label, if the model has one. Audio
   *  passes the approved-version offer here; Cover Art passes nothing, and an
   *  image model has no lyrics control to hang it on either way. */
  lyricAccessory?: ReactNode
}) {
  const model = store((s) => s.model)
  const values = store((s) => s.values)
  const showAdvanced = store((s) => s.showAdvanced)
  const error = store((s) => s.error)
  const setValue = store((s) => s.setValue)
  const rerollSeed = store((s) => s.rerollSeed)
  const toggleAdvanced = store((s) => s.toggleAdvanced)
  const refreshChoices = store((s) => s.refreshChoices)

  return (
    <section className="panel param-panel">
      <h2 className="param-panel-title">Settings</h2>

      {error !== null ? (
        <p className="param-panel-empty">{error}</p>
      ) : model === null ? null : (
        <>
          {model.basic.map((control) => (
            <ParamField
              key={control.name}
              control={control}
              value={values[control.name]}
              onChange={(value) => setValue(control.name, value)}
              onReroll={control.kind === 'seed' ? rerollSeed : undefined}
              onRetryOptions={() => void refreshChoices()}
              accessory={control.kind === 'lyrics' ? lyricAccessory : undefined}
            />
          ))}

          {model.advanced.length > 0 ? (
            <>
              <button
                type="button"
                className="param-advanced-toggle"
                aria-expanded={showAdvanced}
                onClick={toggleAdvanced}
              >
                Advanced settings ({model.advanced.length})
              </button>

              {showAdvanced ? (
                <div className="param-advanced">
                  {model.advanced
                    .filter((c) => c.group === null)
                    .map((control) => (
                      <ParamField
                        key={control.name}
                        control={control}
                        value={values[control.name]}
                        onChange={(value) => setValue(control.name, value)}
                        onReroll={control.kind === 'seed' ? rerollSeed : undefined}
                        onRetryOptions={() => void refreshChoices()}
                        accessory={control.kind === 'lyrics' ? lyricAccessory : undefined}
                      />
                    ))}

                  {groupsOf(model.advanced).map((group) => (
                    <fieldset key={group} className="param-group">
                      <legend className="param-group-title">{group}</legend>
                      {model.advanced
                        .filter((c) => c.group === group)
                        .map((control) => (
                          <ParamField
                            key={control.name}
                            control={control}
                            value={values[control.name]}
                            onChange={(value) => setValue(control.name, value)}
                            onReroll={control.kind === 'seed' ? rerollSeed : undefined}
                            onRetryOptions={() => void refreshChoices()}
                            accessory={control.kind === 'lyrics' ? lyricAccessory : undefined}
                          />
                        ))}
                    </fieldset>
                  ))}
                </div>
              ) : null}
            </>
          ) : null}

          {model.omitted.length > 0 ? (
            <div className="param-omitted">
              <h3 className="param-omitted-title">Not offered by this model</h3>
              <ul>
                {model.omitted.map((omission) => (
                  <li key={omission.name} className="param-omitted-item">
                    {omission.reason !== null
                      ? `${omission.name} — ${omission.reason}`
                      : `${omission.name} — the profile records that this model does not accept it.`}
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
        </>
      )}
    </section>
  )
}

interface ParamFieldProps {
  control: Control
  value: ControlValue | undefined
  onChange: (value: ControlValue) => void
  onReroll?: () => void
  onRetryOptions?: () => void
  /** Extra content on the label row, e.g. the approved-lyric offer. */
  accessory?: ReactNode
}

function ParamField({
  control,
  value,
  onChange,
  onReroll,
  onRetryOptions,
  accessory,
}: ParamFieldProps) {
  const inputId = control.name

  return (
    <div className="param-field">
      <div className="param-field-head">
        <label className="param-field-label" htmlFor={inputId}>
          {control.label}
        </label>
        {accessory}
      </div>

      {control.kind === 'text' || control.kind === 'lyrics' ? (
        <textarea
          id={inputId}
          rows={control.kind === 'lyrics' ? 10 : 2}
          value={value ?? ''}
          onChange={(e) => onChange(e.target.value)}
        />
      ) : null}

      {control.kind === 'int' || control.kind === 'float' ? (
        <>
          <input
            id={inputId}
            type="number"
            min={control.range.min}
            max={control.range.max}
            step={control.kind === 'int' ? 1 : control.range.step ?? undefined}
            value={value ?? ''}
            onChange={(e) => onChange(e.target.value)}
          />
          <span className="param-field-hint">
            {control.range.min}-{control.range.max}
          </span>
        </>
      ) : null}

      {control.kind === 'enum' ? (
        <>
          <select
            id={inputId}
            value={value ?? ''}
            onChange={(e) => onChange(e.target.value)}
            disabled={control.choices.length === 0}
          >
            {control.choices.length === 0 ? (
              <option value="">Not loaded</option>
            ) : (
              control.choices.map((choice) => (
                <option key={choice} value={choice}>
                  {choice}
                </option>
              ))
            )}
          </select>
          {control.optionsNote !== null ? (
            <span className="param-field-hint">
              {control.optionsNote}
              <button type="button" className="param-options-retry" onClick={onRetryOptions}>
                Retry
              </button>
            </span>
          ) : null}
        </>
      ) : null}

      {control.kind === 'seed' ? (
        <>
          <div className="param-seed">
            <input
              id={inputId}
              type="text"
              inputMode="numeric"
              value={value ?? ''}
              onChange={(e) => onChange(e.target.value)}
            />
            <button type="button" className="param-seed-reroll" onClick={onReroll}>
              Reroll
            </button>
          </div>
          {seedError(String(value ?? '')) !== null ? (
            <p className="param-field-error">{seedError(String(value ?? ''))}</p>
          ) : (
            <span className="param-field-hint">
              Any whole number up to {MAX_SAFE_SEED}.
            </span>
          )}
        </>
      ) : null}
    </div>
  )
}
