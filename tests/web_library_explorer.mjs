import assert from 'node:assert/strict';

await import('../web/library-explorer.js');

const explorer = globalThis.ChordriftLibraryExplorer;
const tracks = [
  { position: 2, title: 'Beta', album: 'Second', play_count: 4, last_played_at: '2026-01-01T00:00:00Z' },
  { position: 1, title: 'Alpha', album: 'First', play_count: 9, last_played_at: '2025-01-01T00:00:00Z' },
  { position: 3, title: 'Gamma', album: null, play_count: 0, last_played_at: null }
];

assert.deepEqual(explorer.sortPlaylistTracks(tracks, 'custom_order').map((track) => track.title), ['Alpha', 'Beta', 'Gamma']);
assert.deepEqual(explorer.sortPlaylistTracks(tracks, 'most_played').map((track) => track.title), ['Alpha', 'Beta', 'Gamma']);
assert.deepEqual(explorer.sortPlaylistTracks(tracks, 'recently_heard').map((track) => track.title), ['Beta', 'Alpha', 'Gamma']);
assert.deepEqual(explorer.sortPlaylistTracks(tracks, 'album').map((track) => track.title), ['Gamma', 'Alpha', 'Beta']);

const exclusions = tracks.map((track, index) => ({
  ...track,
  excluded_at: `2026-01-0${index + 1}T00:00:00Z`,
  previous_playlist: index === 0 ? 'Archive' : null
}));
assert.deepEqual(explorer.sortExcludedTracks(exclusions, 'recently_excluded').map((track) => track.title), ['Gamma', 'Alpha', 'Beta']);
assert.equal(explorer.excludedGroup(exclusions[0], 'album'), 'Second');
assert.equal(explorer.excludedGroup(exclusions[2], 'album'), 'Unknown album');
assert.equal(explorer.excludedGroup(exclusions[0], 'previous_playlist'), 'Archive');
assert.equal(explorer.excludedGroup(exclusions[1], 'previous_playlist'), 'No surviving source playlist');
assert.equal(explorer.lastHeardBucket(null), 'Never heard in retained history');
assert.equal(explorer.lastHeardBucket('2026-08-20T00:00:00Z', new Date('2026-08-31T00:00:00Z').getTime()), 'Heard in the last 30 days');

console.log('web library explorer harness passed');
