/**
 * LevelSpinner — compact `[-]  [editable number]  [+]` control with
 * vertical-drag support on the value cell.
 *
 * Drag mechanics: pointerdown on the number cell starts a drag. While
 * dragging, vertical movement adjusts the value (~one step per
 * `pxPerStep` pixels of drag, drag up to increase). The pointer is
 * captured so the drag tracks even if it leaves the element. Single
 * clicks (no movement) focus the input for typing — drag and click are
 * mutually exclusive based on a small movement threshold.
 *
 * Typing: free-edit while focused; the value commits and clamps to
 * [min, max] on Enter or blur. Esc reverts to the previous value.
 *
 * `allowedValues` narrows all three gestures to a roster: the buttons and the
 * drag walk it entry by entry, and a typed value snaps to the nearest one. That
 * is for a band whose steps are data rather than arithmetic — generic IOs are
 * crafted at the nine levels the export names, not at every integer between them.
 */

import { useEffect, useRef, useState } from 'react';

interface LevelSpinnerProps {
  value: number;
  min: number;
  max: number;
  onChange: (next: number) => void;
  /** Step for +/- buttons (and drag-per-`pxPerStep`-pixels). */
  step?: number;
  /** The only values the control may land on. Its ends replace `min`/`max` and
   *  one entry replaces `step`; a typed or dragged value snaps to the nearest. */
  allowedValues?: number[];
  /** Pixels of vertical drag that advance the value by one step. */
  pxPerStep?: number;
  /** Render `+` in front of positive values (used for boost level). */
  showPlus?: boolean;
  /** Tailwind class for the value text colour. */
  valueColorClass?: string;
  /** Tooltip on each cell (− / value / +). */
  decreaseTitle?: string;
  increaseTitle?: string;
  valueTitle?: string;
  /** Disable all controls. */
  disabled?: boolean;
  /** Width of the value cell. Use `"w-6"`, `"w-8"`, etc. */
  widthClass?: string;
}

const DRAG_MOVEMENT_THRESHOLD_PX = 3;

function clamp(n: number, min: number, max: number): number {
  if (Number.isNaN(n)) return min;
  return Math.min(max, Math.max(min, n));
}

/** The roster entry closest to `n`; ties go to the lower one. */
function nearest(values: number[], n: number): number {
  if (Number.isNaN(n)) return values[0];
  return values.reduce((best, v) => (Math.abs(v - n) < Math.abs(best - n) ? v : best), values[0]);
}

export function LevelSpinner({
  value,
  min,
  max,
  onChange,
  step = 1,
  allowedValues,
  pxPerStep = 6,
  showPlus = false,
  valueColorClass = 'text-link',
  decreaseTitle,
  increaseTitle,
  valueTitle,
  disabled = false,
  widthClass = 'w-6',
}: LevelSpinnerProps) {
  const [editing, setEditing] = useState(false);
  const [draftText, setDraftText] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);

  // Drag state. Tracked in a ref so the move/up listeners read fresh
  // values without re-binding on every render.
  const dragRef = useRef<{
    pointerId: number;
    startY: number;
    startValue: number;
    moved: boolean;
  } | null>(null);

  // With a roster the band is its ends, and one move is one entry; without, the
  // props say both.
  const lo = allowedValues ? allowedValues[0] : min;
  const hi = allowedValues ? allowedValues[allowedValues.length - 1] : max;
  const snap = (n: number) => (allowedValues ? nearest(allowedValues, n) : clamp(n, lo, hi));
  const moved = (from: number, steps: number) => {
    if (!allowedValues) return clamp(from + steps * step, lo, hi);
    const i = allowedValues.indexOf(snap(from));
    return allowedValues[clamp(i + steps, 0, allowedValues.length - 1)];
  };

  const commit = (raw: string) => {
    const n = Math.round(parseFloat(raw));
    if (Number.isFinite(n)) {
      const snapped = snap(n);
      if (snapped !== value) onChange(snapped);
    }
    setEditing(false);
  };

  const handlePointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    // Only initiate drag on primary button; let inputs handle text caret
    // placement themselves once focused.
    if (disabled || editing || e.button !== 0) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragRef.current = {
      pointerId: e.pointerId,
      startY: e.clientY,
      startValue: value,
      moved: false,
    };
  };

  const handlePointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    const d = dragRef.current;
    if (!d || d.pointerId !== e.pointerId) return;
    const deltaY = d.startY - e.clientY; // up = increase
    if (!d.moved && Math.abs(deltaY) < DRAG_MOVEMENT_THRESHOLD_PX) return;
    d.moved = true;
    const steps = Math.round(deltaY / pxPerStep);
    const next = moved(d.startValue, steps);
    if (next !== value) onChange(next);
  };

  const handlePointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    const d = dragRef.current;
    if (!d || d.pointerId !== e.pointerId) return;
    (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    const wasClick = !d.moved;
    dragRef.current = null;
    // A drag that never moved past threshold counts as a click → enter
    // edit mode so the user can type a new value.
    if (wasClick && !disabled) {
      setDraftText(String(value));
      setEditing(true);
    }
  };

  useEffect(() => {
    if (editing) inputRef.current?.select();
  }, [editing]);

  const dec = () => onChange(moved(value, -1));
  const inc = () => onChange(moved(value, 1));

  const displayValue = showPlus ? `+${value}` : String(value);

  return (
    <div className="flex items-center gap-1.5" aria-disabled={disabled}>
      <button
        type="button"
        onClick={dec}
        title={decreaseTitle}
        disabled={disabled || value <= lo}
        className="w-5 h-5 rounded text-xs bg-gray-700 hover:bg-gray-600 text-gray-300 flex items-center justify-center disabled:opacity-30 disabled:cursor-not-allowed"
      >−</button>
      {editing ? (
        <input
          ref={inputRef}
          type="number"
          inputMode="numeric"
          min={lo}
          max={hi}
          step={allowedValues ? undefined : step}
          value={draftText}
          onChange={(e) => setDraftText(e.target.value)}
          onBlur={(e) => commit(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') { e.preventDefault(); commit((e.target as HTMLInputElement).value); }
            else if (e.key === 'Escape') { e.preventDefault(); setEditing(false); }
          }}
          // Match the static cell's footprint so the row doesn't reflow.
          // Suppress the native number-input spinner buttons — in this
          // narrow cell they overlap and hide the digits.
          className={`text-sm font-mono ${widthClass} text-center bg-transparent border border-[var(--color-primary)]/60 rounded outline-none ${valueColorClass} px-0.5 [appearance:textfield] [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none`}
        />
      ) : (
        <div
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={handlePointerUp}
          onPointerCancel={handlePointerUp}
          title={valueTitle ?? 'Drag up/down to change, click to type'}
          className={`text-sm font-mono ${widthClass} text-center select-none cursor-ns-resize ${valueColorClass} ${disabled ? 'opacity-60 cursor-not-allowed' : ''}`}
          // Hint the browser to treat vertical drag as our gesture, not a
          // touch scroll. Without this the panel scrolls when dragging
          // the value on touch devices.
          style={{ touchAction: 'none' }}
        >
          {displayValue}
        </div>
      )}
      <button
        type="button"
        onClick={inc}
        title={increaseTitle}
        disabled={disabled || value >= hi}
        className="w-5 h-5 rounded text-xs bg-gray-700 hover:bg-gray-600 text-gray-300 flex items-center justify-center disabled:opacity-30 disabled:cursor-not-allowed"
      >+</button>
    </div>
  );
}
