import assert from 'node:assert/strict';

await import('../web/maintenance-decisions.js');

const helper = globalThis.ChordriftMaintenance;
const liked = { surface_id: '33333333-3333-4333-8333-333333333333', name: 'Liked Songs' };
const placement = {
  change_id: '11111111-1111-4111-8111-111111111111',
  kind: 'direct_intake',
  summary: 'Choose a destination for Fixture Track',
  current_surface: null
};
const saved = {
  change_id: '22222222-2222-4222-8222-222222222222',
  kind: 'saved_state',
  summary: 'Choose whether Fixture Track remains in Liked Songs',
  current_surface: liked
};

const destinationSurface = {
  surface_id: '5fdad879-f894-5f70-810b-57ace590e9b0',
  name: 'Neon Affection'
};
const destination = await helper.resolution(placement, JSON.stringify(destinationSurface));
assert.deepEqual(destination, {
  type: 'place',
  parameters: {
    destination: destinationSurface
  }
});
assert.deepEqual(await helper.resolution(saved, 'consume'), {
  type: 'consume_intake',
  parameters: { source: liked }
});
assert.deepEqual(await helper.resolution(saved, 'keep'), { type: 'keep_observed' });

await assert.rejects(helper.resolution(placement, ''), /Choose an answer/);
await assert.rejects(helper.resolution(placement, 'Neon Affection'), /not part of this review/);
await assert.rejects(
  helper.resolution({ ...saved, current_surface: null }, 'consume'),
  /source is missing/
);

console.log('web maintenance decision DTO harness passed');
