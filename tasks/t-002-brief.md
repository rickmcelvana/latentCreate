# T-002: Nav rail + five placeholder views
**Depends:** T-001 (done) | **Dir:** `app/`

**Files to create:**
`app/src/state/nav.ts`, `app/src/state/nav.test.ts`,
`app/src/components/NavRail.tsx`, `app/src/components/NavIcon.tsx`,
`app/src/views/Setup.tsx`, `app/src/views/LyricsStudio.tsx`,
`app/src/views/AudioStudio.tsx`, `app/src/views/Library.tsx`,
`app/src/views/CoverArt.tsx`

**Files to modify:** `app/src/App.tsx`, `app/src/theme.css`

> **Starting point.** T-001 left `App.tsx` as a one-panel placeholder that calls
> `bridge/shell.ts` to show the Rust shell version. `theme.css` already defines the
> palette and `.app-shell`, `.nav-rail`, `.nav-brand`, `.nav-brand-accent`,
> `.content-pane`, `.view-title`, `.view-subtitle`, `.panel`, `.muted`,
> `.status-pill`, `.status-pill-ok`, `.status-pill-warn`. **Extend these; do not
> restyle or re-derive the palette.** All colours and spacing come from the existing
> custom properties (`var(--accent)`, `var(--gap-md)`, ...) — no literal hex colours
> and no hard-coded pixel spacing anywhere in this diff.

## Goal
A left nav rail switches between five placeholder views. The active view lives in a
Zustand store — not in component state, not in a router. Each view renders a titled
empty state. The result is the app's permanent frame; every later phase fills these
views in.

## Spec

### `state/nav.ts`
```ts
export type ViewId = 'setup' | 'lyrics' | 'audio' | 'library' | 'art'

export interface NavItem {
  readonly id: ViewId
  readonly label: string
}

/** Rail order is product order: configure, write, generate, keep, decorate. */
export const NAV_ITEMS: readonly NavItem[] = [
  { id: 'setup',   label: 'Setup' },
  { id: 'lyrics',  label: 'Lyrics' },
  { id: 'audio',   label: 'Audio' },
  { id: 'library', label: 'Library' },
  { id: 'art',     label: 'Cover Art' },
]
```
Plus a Zustand store exported as `useNavStore`: state `activeView: ViewId`
(initial `'setup'`) and action `setView(id: ViewId): void`. No persistence, no
middleware.

### `components/NavIcon.tsx`
One component taking `{ view: ViewId }`, returning a 16x16 inline SVG.
**No icon library — adding a dependency is out of scope.** Use these exact
primitives so nothing has to be invented. Shared attributes on the `<svg>`:
`viewBox="0 0 16 16"`, `width={16}`, `height={16}`, `fill="none"`,
`stroke="currentColor"`, `strokeWidth={1.5}`, `strokeLinecap="round"`,
`aria-hidden="true"`, `focusable="false"`.

- `setup` — three sliders: lines `(2,4)-(14,4)`, `(2,8)-(14,8)`, `(2,12)-(14,12)`;
  plus dots with `fill="currentColor"` `stroke="none"` `r={1.6}` at `(11,4)`,
  `(5,8)`, `(9,12)`.
- `lyrics` — four text lines: `(3,3)-(13,3)`, `(3,6.5)-(13,6.5)`,
  `(3,10)-(13,10)`, `(3,13.5)-(9,13.5)`.
- `audio` — five meter bars as vertical lines at x = 2, 5, 8, 11, 14, each centred
  on y=8 with half-heights 2.5, 5, 6.5, 4, 2 respectively (so the x=8 bar runs
  from y=1.5 to y=14.5).
- `library` — three stacked rounded rects, each `x={2} width={12} height={3}
  rx={1}`, at y = 2, 6.5, 11.
- `art` — `rect x={2} y={2} width={12} height={12} rx={1.5}`,
  `circle cx={5.75} cy={5.75} r={1.25}`,
  `polyline points="2.5,12 6.5,8.5 9,10.5 11.5,8 13.5,10"`.

### `components/NavRail.tsx`
Renders, inside the existing `.nav-rail` element:
1. `.nav-brand` — keep T-001's markup exactly:
   `latent<span className="nav-brand-accent">Create</span>`.
2. One `<button type="button">` per `NAV_ITEMS` entry, containing `<NavIcon>` and
   the label. Each button: `className="nav-item"` plus `nav-item-active` when it is
   the active view, `aria-current="page"` when active, and `onClick` calling
   `setView(item.id)`. Real buttons, not divs.
3. A footer (`.nav-rail-footer`) holding the version line moved out of `App.tsx`.

Move T-001's `appVersion()` / `isTauri()` logic here: render
`<span className="nav-version muted">v{version}</span>` once the version resolves,
and `<span className="nav-version muted">browser preview</span>` when not running
in Tauri. **Keep the `bridge/shell.ts` import** — it is the only proof the Tauri
boundary works, and dropping it would silently lose that coverage.

### `App.tsx`
Becomes the composition root:
```tsx
<div className="app-shell">
  <NavRail />
  <main className="content-pane">{/* active view */}</main>
</div>
```
Pick the view from `activeView` with a `switch` that is **exhaustive over `ViewId`
with no `default` branch**, so TypeScript fails the build when a sixth view is
added and not wired up.

### The five views
Each file exports a named component matching its filename, takes no props, and
renders:
```tsx
<>
  <h1 className="view-title">{title}</h1>
  <p className="view-subtitle">{subtitle}</p>
  <div className="panel muted">{emptyState}</div>
</>
```
Use this copy verbatim — UX copy is not the executor's call:

| File | title | subtitle | empty state |
|---|---|---|---|
| `Setup.tsx` | Setup | Connect ComfyUI and, optionally, a model for writing lyrics. | Nothing configured yet. Connection steps arrive in Phase 1. |
| `LyricsStudio.tsx` | Lyrics | Describe the song; your local model writes the words. | No lyrics yet. Finish Setup to enable writing. |
| `AudioStudio.tsx` | Audio | Style tags, lyrics, and the settings worth changing. | No generations yet. Finish Setup to enable audio. |
| `Library.tsx` | Library | Everything you have made, with the recipe that made it. | Nothing saved yet. Generated tracks land here. |
| `CoverArt.tsx` | Cover Art | Artwork for singles and albums, from the same ComfyUI. | No artwork yet. Optional — configure an image model in Setup. |

### `theme.css` additions
Add rules for `.nav-item`, `.nav-item-active`, `.nav-rail-footer`, `.nav-version`:

- `.nav-item` — full width, left-aligned, `background: transparent`, no border,
  `color: var(--text-muted)`, `font: inherit`, `cursor: pointer`,
  `border-radius: var(--radius)`, padding from the gap scale, icon+label in a flex
  row with `gap: var(--gap-sm)`, and `transition: … var(--transition)`.
  Include a `border-left: 2px solid transparent` so the active state does not
  shift layout.
- `:hover` — `background: var(--panel-hover)`, `color: var(--text)`.
- `:focus-visible` — a visible outline in `var(--accent)` (keyboard users must be
  able to see where they are).
- `.nav-item-active` — `color: var(--text)`, `border-left-color: var(--accent)`,
  and the icon tinted `var(--accent)` (the SVG uses `currentColor`, so target the
  `svg` inside the active item).
- `.nav-rail-footer` — `margin-top: auto`, a `1px solid var(--border)` top edge,
  padding from the gap scale.
- `.nav-version` — small (12px is fine as a font size), muted.

## Acceptance criteria
- [ ] `npx tsc -b` clean (run from `app/`)
- [ ] `npm run build` succeeds
- [ ] `npm test` green, including new `state/nav.test.ts` with these three tests:
      `test_initial_view_is_setup`, `test_set_view_changes_active_view`,
      `test_nav_items_are_unique_and_ordered`
- [ ] `npx oxlint src` reports 0 warnings and 0 errors
- [ ] Every className introduced in TSX has a matching rule in `theme.css`
- [ ] No literal colour values and no hard-coded spacing — custom properties only
- [ ] No new entries in `package.json`
- [ ] No changes outside the listed files

## Out of scope
Any Tauri `invoke` beyond the existing `appVersion` call. Real view content, forms,
or any ComfyUI/LLM wiring. Routing libraries. Icon or UI-component dependencies.
Persisting the active view across restarts.

## Notes for the executor
- Tests run in vitest's **node** environment. There is no jsdom and none is to be
  added, so `nav.test.ts` covers the store and `NAV_ITEMS` as pure logic. Do not
  write DOM-rendering tests — this matches the sibling repos' deliberate choice.
- `verbatimModuleSyntax` is on: type-only imports must use `import type`.
- `noUnusedLocals` and `noUnusedParameters` are on.
- Zustand v4 is installed; import from `zustand` (`create`).

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --file app/src/App.tsx --file app/src/theme.css --file app/src/state/nav.ts --file app/src/state/nav.test.ts --file app/src/components/NavRail.tsx --file app/src/components/NavIcon.tsx --file app/src/views/Setup.tsx --file app/src/views/LyricsStudio.tsx --file app/src/views/AudioStudio.tsx --file app/src/views/Library.tsx --file app/src/views/CoverArt.tsx
```
