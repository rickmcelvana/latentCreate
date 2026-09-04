import type { ProfileRow } from '../state/profiles'

export function ProfilePickerRow({
  row,
  selected,
  group,
  onSelect,
}: {
  row: ProfileRow
  selected: boolean
  /** The radio group's `name`. Two pickers exist; only one is mounted at a
   *  time, but a shared name would silently make them one group if that ever
   *  changed. */
  group: string
  onSelect: () => void
}) {
  return (
    <li className={`profile-row ${selected ? 'profile-row-selected' : ''}`}>
      <label className="profile-row-pick">
        <input
          type="radio"
          name={group}
          checked={selected}
          onChange={onSelect}
        />
        <span className="profile-row-name">{row.displayName}</span>
      </label>

      <div className="profile-row-meta">
        <span className={`status-pill status-pill-${row.readiness.tone}`}>
          {row.readiness.label}
        </span>
        <span className="profile-row-origin">{row.origin}</span>
        {row.vramClaim !== null ? (
          <span className="profile-row-vram">{row.vramClaim}</span>
        ) : null}
      </div>

      <p className="profile-row-license">
        <span className="profile-row-license-name">{row.license}</span>
        {row.licenseNotes !== null ? ` -- ${row.licenseNotes}` : null}
      </p>
    </li>
  )
}
