import type { SVGProps } from 'react'
import type { ViewId } from '../state/nav'

interface NavIconProps {
  readonly view: ViewId
}

/**
 * Shared attributes for every rail glyph. Typed as `SVGProps` so the boolean
 * DOM attributes stay booleans -- React renders `aria-hidden="true"` and
 * `focusable="false"` from these, whereas the string literals `'true'`/`'false'`
 * do not typecheck against `Booleanish`.
 *
 * Module-level so it is built once rather than on every render.
 */
const SHARED_SVG_PROPS: SVGProps<SVGSVGElement> = {
  viewBox: '0 0 16 16',
  width: 16,
  height: 16,
  fill: 'none',
  stroke: 'currentColor',
  strokeWidth: 1.5,
  strokeLinecap: 'round',
  'aria-hidden': true,
  focusable: false,
}

/** Glyph for one nav destination. Inherits colour via `currentColor`. */
export function NavIcon({ view }: NavIconProps) {
  switch (view) {
    case 'setup':
      return (
        <svg {...SHARED_SVG_PROPS}>
          <line x1={2} y1={4} x2={14} y2={4} />
          <line x1={2} y1={8} x2={14} y2={8} />
          <line x1={2} y1={12} x2={14} y2={12} />
          <circle cx={11} cy={4} r={1.6} fill="currentColor" stroke="none" />
          <circle cx={5} cy={8} r={1.6} fill="currentColor" stroke="none" />
          <circle cx={9} cy={12} r={1.6} fill="currentColor" stroke="none" />
        </svg>
      )
    case 'lyrics':
      return (
        <svg {...SHARED_SVG_PROPS}>
          <line x1={3} y1={3} x2={13} y2={3} />
          <line x1={3} y1={6.5} x2={13} y2={6.5} />
          <line x1={3} y1={10} x2={13} y2={10} />
          <line x1={3} y1={13.5} x2={9} y2={13.5} />
        </svg>
      )
    case 'audio':
      return (
        <svg {...SHARED_SVG_PROPS}>
          <line x1={2} y1={5.5} x2={2} y2={10.5} />
          <line x1={5} y1={3} x2={5} y2={13} />
          <line x1={8} y1={1.5} x2={8} y2={14.5} />
          <line x1={11} y1={4} x2={11} y2={12} />
          <line x1={14} y1={6} x2={14} y2={10} />
        </svg>
      )
    case 'library':
      return (
        <svg {...SHARED_SVG_PROPS}>
          <rect x={2} y={2} width={12} height={3} rx={1} />
          <rect x={2} y={6.5} width={12} height={3} rx={1} />
          <rect x={2} y={11} width={12} height={3} rx={1} />
        </svg>
      )
    case 'art':
      return (
        <svg {...SHARED_SVG_PROPS}>
          <rect x={2} y={2} width={12} height={12} rx={1.5} />
          <circle cx={5.75} cy={5.75} r={1.25} fill="currentColor" stroke="none" />
          <polyline points="2.5,12 6.5,8.5 9,10.5 11.5,8 13.5,10" />
        </svg>
      )
  }
}
