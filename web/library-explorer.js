(function (root) {
  const text = (value) => String(value || '').toLocaleLowerCase();
  const time = (value) => value ? new Date(value).getTime() : Number.NEGATIVE_INFINITY;
  const compareText = (left, right) => text(left).localeCompare(text(right));
  const stable = (tracks, compare) => tracks.map((track, index) => ({ track, index }))
    .sort((left, right) => compare(left.track, right.track) || left.index - right.index)
    .map(({ track }) => track);

  function sortPlaylistTracks(tracks, mode) {
    const compare = {
      custom_order: (a, b) => a.position - b.position,
      most_played: (a, b) => b.play_count - a.play_count || compareText(a.title, b.title),
      recently_heard: (a, b) => time(b.last_played_at) - time(a.last_played_at) || compareText(a.title, b.title),
      album: (a, b) => compareText(a.album, b.album) || compareText(a.title, b.title),
      title: (a, b) => compareText(a.title, b.title)
    }[mode] || ((a, b) => a.position - b.position);
    return stable(tracks, compare);
  }

  function sortExcludedTracks(tracks, mode) {
    const compare = {
      recently_excluded: (a, b) => time(b.excluded_at) - time(a.excluded_at),
      most_played: (a, b) => b.play_count - a.play_count || compareText(a.title, b.title),
      recently_heard: (a, b) => time(b.last_played_at) - time(a.last_played_at) || compareText(a.title, b.title),
      album: (a, b) => compareText(a.album, b.album) || compareText(a.title, b.title),
      previous_playlist: (a, b) => compareText(a.previous_playlist, b.previous_playlist) || compareText(a.title, b.title),
      title: (a, b) => compareText(a.title, b.title)
    }[mode] || ((a, b) => time(b.excluded_at) - time(a.excluded_at));
    return stable(tracks, compare);
  }

  function lastHeardBucket(lastPlayedAt, now = Date.now()) {
    if (!lastPlayedAt) return 'Never heard in retained history';
    const ageDays = Math.max(0, now - new Date(lastPlayedAt).getTime()) / 86_400_000;
    if (ageDays <= 30) return 'Heard in the last 30 days';
    if (ageDays <= 180) return 'Heard 1–6 months ago';
    if (ageDays <= 365) return 'Heard 6–12 months ago';
    return 'Not heard for over a year';
  }

  function excludedGroup(track, mode, now = Date.now()) {
    if (mode === 'album') return track.album || 'Unknown album';
    if (mode === 'previous_playlist') return track.previous_playlist || 'No surviving source playlist';
    if (mode === 'last_heard') return lastHeardBucket(track.last_played_at, now);
    return '';
  }

  root.ChordriftLibraryExplorer = {
    sortPlaylistTracks,
    sortExcludedTracks,
    excludedGroup,
    lastHeardBucket
  };
})(globalThis);
