import { describe, it, expect } from 'vitest';
import {
  announcementAutoOpens,
  deriveGroupState,
  roadmapProgress,
  ROADMAP_GROUPS,
  type RoadmapGroup,
} from '@/data/core/roadmap';

const group = (states: RoadmapGroup['items'][number]['state'][]): RoadmapGroup => ({
  id: 'x',
  title: 'X',
  items: states.map((state, i) => ({ label: `item ${i}`, state })),
});

describe('deriveGroupState', () => {
  it('is "done" when every item is done', () => {
    expect(deriveGroupState(group(['done', 'done']))).toBe('done');
  });
  it('is "planned" when every item is planned', () => {
    expect(deriveGroupState(group(['planned', 'planned']))).toBe('planned');
  });
  it('is "in-progress" when any item is in-progress', () => {
    expect(deriveGroupState(group(['done', 'in-progress', 'planned']))).toBe('in-progress');
  });
  it('is "in-progress" for a done+planned mix with no explicit in-progress', () => {
    expect(deriveGroupState(group(['done', 'planned']))).toBe('in-progress');
  });
});

describe('roadmapProgress', () => {
  it('counts done items and total items in a fixture', () => {
    const groups = [group(['done', 'planned']), group(['done', 'done', 'in-progress'])];
    expect(roadmapProgress(groups)).toEqual({ done: 3, total: 5 });
  });
  it('defaults to ROADMAP_GROUPS and reports positive progress', () => {
    const { done, total } = roadmapProgress();
    expect(total).toBeGreaterThan(done);
    expect(done).toBeGreaterThan(0);
  });
});

describe('ROADMAP_GROUPS invariants', () => {
  it('every group has a unique id and at least one item', () => {
    const ids = ROADMAP_GROUPS.map((g) => g.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const g of ROADMAP_GROUPS) expect(g.items.length).toBeGreaterThan(0);
  });
});

describe('announcementAutoOpens', () => {
  const ids = ['a', 'b'];

  it('opens for a new roadmap version even after "Don\'t show these again"', () => {
    // Every featurette dismissed, and the PREVIOUS roadmap version seen.
    expect(announcementAutoOpens(['a', 'b', 'roadmap-v1'], ids, 2)).toBe(true);
  });

  it('stays shut once the current version has been seen', () => {
    expect(announcementAutoOpens(['a', 'b', 'roadmap-v2'], ids, 2)).toBe(false);
  });

  it('still opens for an undismissed featurette', () => {
    expect(announcementAutoOpens(['a', 'roadmap-v2'], ids, 2)).toBe(true);
  });
});
