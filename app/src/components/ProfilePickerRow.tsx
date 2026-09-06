import type { ReactNode } from 'react'
import type { ProfileRow } from '../state/profiles'

/**
 * One model in a picker list: pick it, and read what taking it on means.
 *
 * The single row style for every model list in the app. Setup passes
 * `children` to hang its install controls under the licence; the studios pass
 * none. Two components would have been two styles a week later.
 */
export function ProfilePickerRow({
  row,
  selected,
  group,
  onSelect,
  children,
}: {
  row: ProfileRow
  selected: boolean
  /** The radio group's `name`. Several pickers exist; a shared name would
   *  silently make two lists one group. */
  group: string
  onSelect: () => void
  /** Extra controls for this row -- Setup's file list, progress and Install. */
  children?: ReactNode
}) {
  return (
    <li className={`picker-row ${selected ? 'picker-row-selected' : ''}`}>
      <label className="picker-row-pick">
        <input
          type="radio"
          name={group}
          checked={selected}
          onChange={onSelect}
        />
        <span className="picker-row-name">{row.displayName}</span>
      </label>

      <div className="picker-row-meta">
        <span className={`status-pill status-pill-${row.readiness.tone}`}>
          {row.readiness.label}
        </span>
        <span className="picker-row-tag">{row.origin}</span>
        {row.vramClaim !== null ? (
          <span className="picker-row-tag">{row.vramClaim}</span>
        ) : null}
      </div>

      <p className="picker-row-license">
        <span className="picker-row-license-name">{row.license}</span>
        {row.licenseNotes !== null ? ` -- ${row.licenseNotes}` : null}
      </p>

      {children !== undefined ? <div className="picker-row-extra">{children}</div> : null}
    </li>
  )
}
