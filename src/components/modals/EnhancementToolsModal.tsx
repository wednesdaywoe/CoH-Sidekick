/**
 * EnhancementToolsModal — bulk-edit enhancement levels, attunement, and
 * boosters across the entire build. Replaces the old one-shot "Maximize
 * Enhancements" confirm dialog.
 *
 * Each of the four controls is independently opt-in (checkbox + value):
 * leave a box unchecked and that aspect of the build is untouched. Apply
 * commits everything in a single history checkpoint.
 */

import { useState } from 'react';
import { useBuildStore } from '@/stores';
import { Modal, ModalBody, ModalFooter } from './Modal';
import { Button, Slider, Toggle } from '@/components/ui';
import { enhancementLevelRange } from '@/utils/calculations';
import { getCommonIOLevels } from '@/data/boost-index';

interface EnhancementToolsModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export function EnhancementToolsModal({ isOpen, onClose }: EnhancementToolsModalProps) {
  const maximizeEnhancementLevels = useBuildStore((s) => s.maximizeEnhancementLevels);

  // Each section has an enabled flag and its value. The store action only
  // applies an option when it's defined; we send `undefined` for any
  // section the user didn't enable.
  const [specialEnabled, setSpecialEnabled] = useState(true);
  // Relative level, not absolute. The old control was a 47–53 "level" slider,
  // which is ±3 around 50 wearing absolute clothing — it wrote `slot.level`,
  // a field nothing reads on a special and serialization discards.
  const relativeRange = enhancementLevelRange('special');
  const [relativeLevel, setRelativeLevel] = useState(relativeRange.max);

  // The band is the crafted-record roster's ends, not a typed 10-50: set pieces
  // take every integer inside their own range, generic IOs only the roster's
  // steps, and the store floors them onto it on the way in (BOOST-6).
  const ioCraftLevels = getCommonIOLevels();
  const [ioLevelEnabled, setIoLevelEnabled] = useState(true);
  const [ioLevel, setIoLevel] = useState(ioCraftLevels[ioCraftLevels.length - 1]);

  const [attuneAll, setAttuneAll] = useState(false);

  const [boostEnabled, setBoostEnabled] = useState(true);
  const [boostLevel, setBoostLevel] = useState(5);

  const handleApply = () => {
    maximizeEnhancementLevels({
      relativeLevel: specialEnabled ? relativeLevel : undefined,
      ioLevel: ioLevelEnabled ? ioLevel : undefined,
      attuneAll: attuneAll ? true : undefined,
      boostLevel: boostEnabled ? boostLevel : undefined,
    });
    onClose();
  };

  const nothingSelected = !specialEnabled && !ioLevelEnabled && !attuneAll && !boostEnabled;

  return (
    <Modal isOpen={isOpen} onClose={onClose} title="Enhancement Tools" size="md">
      <ModalBody>
        <div className="px-1 py-1 space-y-4">
          <p className="text-sm text-gray-400">
            Bulk-edit enhancement levels, attunement, and boosters across
            every slotted enhancement in the build.
          </p>

          {/* Relative level — origin + special enhancements */}
          <Section
            checked={specialEnabled}
            onCheckedChange={setSpecialEnabled}
            label="Relative level"
            description="SO/DO/TO, Hamidon, Titan, Hydra, D-Sync, and prestige enhancements. How far the enhancement's level sits from your combat level — below even, it is weaker."
          >
            <Slider
              value={relativeLevel}
              min={relativeRange.min}
              max={relativeRange.max}
              onChange={(e) => setRelativeLevel(Number(e.target.value))}
              disabled={!specialEnabled}
              valueFormatter={(v) => (v === 0 ? 'Even' : v > 0 ? `+${v}` : `${v}`)}
            />
          </Section>

          {/* Non-attuned IO level */}
          <Section
            checked={ioLevelEnabled}
            onCheckedChange={setIoLevelEnabled}
            label="Set non-attuned IO level"
            description="Generic IOs and non-attuned set IOs. Set IOs are clamped to each set's level range, generic IOs to the levels the game crafts them at; attuned IOs are skipped."
          >
            <Slider
              value={ioLevel}
              min={ioCraftLevels[0]}
              max={ioCraftLevels[ioCraftLevels.length - 1]}
              onChange={(e) => setIoLevel(Number(e.target.value))}
              disabled={!ioLevelEnabled}
              valueFormatter={(v) => `Level ${v}`}
            />
          </Section>

          {/* Attune all set IOs */}
          <Section
            checked={attuneAll}
            onCheckedChange={setAttuneAll}
            label="Attune all set IOs"
            description="Flips every set IO to attuned, scaling them with character level. Mirrors the in-game catalyst — any existing boosters on those IOs are cleared, since attuned IOs cannot carry boosters."
          />

          {/* Boost level */}
          <Section
            checked={boostEnabled}
            onCheckedChange={setBoostEnabled}
            label="Booster level"
            description="Booster level (+0 to +5) applied to every non-attuned IO at level 50+ (generic or set). Attuned IOs and sub-50 IOs cannot accept boosters in-game."
          >
            <Slider
              value={boostLevel}
              min={0}
              max={5}
              onChange={(e) => setBoostLevel(Number(e.target.value))}
              disabled={!boostEnabled}
              valueFormatter={(v) => (v === 0 ? 'None' : `+${v}`)}
            />
          </Section>
        </div>
      </ModalBody>
      <ModalFooter>
        <div className="flex items-center justify-end gap-2 w-full">
          <Button variant="ghost" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="primary"
            size="sm"
            onClick={handleApply}
            disabled={nothingSelected}
            title={nothingSelected ? 'Enable at least one option' : undefined}
          >
            Apply to build
          </Button>
        </div>
      </ModalFooter>
    </Modal>
  );
}

interface SectionProps {
  checked: boolean;
  onCheckedChange: (next: boolean) => void;
  label: string;
  description: string;
  children?: React.ReactNode;
}

function Section({ checked, onCheckedChange, label, description, children }: SectionProps) {
  return (
    <div className={`rounded border border-slate-700 bg-slate-800/40 px-3 py-2 transition-opacity ${checked ? '' : 'opacity-60'}`}>
      <div className="flex items-start gap-3">
        <div className="pt-0.5">
          <Toggle
            id={`enh-tool-${label.replace(/\s+/g, '-').toLowerCase()}`}
            name={label}
            checked={checked}
            onChange={(e) => onCheckedChange(e.target.checked)}
          />
        </div>
        <div className="flex-1 min-w-0">
          <div className="text-sm font-medium text-gray-200">{label}</div>
          <div className="text-xs text-gray-500">{description}</div>
        </div>
      </div>
      {children && <div className="mt-2 pl-12">{children}</div>}
    </div>
  );
}
