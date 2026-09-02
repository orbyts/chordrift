const $ = (selector) => document.querySelector(selector);
const $$ = (selector) => [...document.querySelectorAll(selector)];
const uuid = () => crypto.randomUUID();
const contractVersion = __CHORDRIFT_CONTRACT_VERSION__;
const state = { session: null, compatibility: null, connections: [], provider: null, source: 'provider_observation', activeOperation: null, maintenanceSession: null, destinationPlaylists: [], playlistTracks: [], excludedTracks: [] };

function queryEnvelope(query) { return { contract_version: contractVersion, request_id: uuid(), query }; }

async function contractRequest(path, body) {
  const response = await fetch(path, { method: 'POST', credentials: 'same-origin', headers: { 'content-type': 'application/json', 'x-chordrift-browser': '1' }, body: JSON.stringify(body) });
  const text = await response.text();
  let value = null;
  try { value = text ? JSON.parse(text) : null; } catch (_) { value = { raw: text }; }
  if (!response.ok) {
    const error = new Error(value?.code ? value.code.replaceAll('_', ' ') : `Request failed (${response.status})`);
    error.response = value;
    throw error;
  }
  return value;
}

function formatTime(value) {
  if (!value) return 'not observed yet';
  const date = new Date(value); const delta = Date.now() - date.getTime();
  if (delta >= 0 && delta < 60_000) return 'just now';
  if (delta >= 0 && delta < 3_600_000) return `${Math.floor(delta / 60_000)} min ago`;
  if (delta >= 0 && delta < 86_400_000) return `${Math.floor(delta / 3_600_000)} hr ago`;
  return date.toLocaleString();
}

function node(tag, className, text) {
  const element = document.createElement(tag);
  if (className) element.className = className;
  if (text != null) element.textContent = text;
  return element;
}

function sessionDisplayName(session) {
  const value = typeof session?.display_name === 'string' ? session.display_name.trim() : '';
  return value ? value : 'Account';
}

function fallbackAvatarText(label) {
  const clean = String(label || '').trim().replace(/\s+/g, ' ');
  if (!clean) return 'A';
  const words = clean.split(' ');
  const lead = words[0]?.[0] || '';
  const tail = words[1]?.[0] || '';
  const initials = `${lead}${tail}`.toLocaleUpperCase();
  return initials || 'A';
}

function setAccountMenuOpen(open) {
  const button = $('#account-button');
  const menu = $('#account-menu');
  if (!button || !menu) return;
  const expanded = Boolean(open);
  button.setAttribute('aria-expanded', String(expanded));
  menu.hidden = !expanded;
}

function renderAccountIdentity(session) {
  const accountName = sessionDisplayName(session);
  const displayName = typeof session?.display_name === 'string' ? session.display_name.trim() : '';
  const avatarUrl = session?.avatar_url;
  const image = $('#account-avatar-image');
  const fallback = $('#account-avatar-fallback');
  const button = $('#account-button');
  const menuCopy = $('#account-menu-copy');
  $('#account-shell').hidden = false;
  $('#account-display-name').textContent = accountName;
  button.setAttribute('aria-label', `Account menu for ${accountName}`);
  button.setAttribute('aria-expanded', 'false');
  $('#account-avatar').setAttribute('aria-label', `Signed in account avatar for ${accountName}`);
  $('#account-menu').hidden = true;
  menuCopy.textContent = `Signed in as ${accountName}`;
  if (avatarUrl && displayName) image.alt = `${displayName} avatar`;
  else image.alt = 'Account avatar';
  if (avatarUrl) {
    image.hidden = false;
    image.src = avatarUrl;
    fallback.hidden = true;
  } else {
    image.hidden = true;
    image.removeAttribute('src');
    fallback.hidden = false;
    fallback.textContent = fallbackAvatarText(accountName);
  }
}

async function loadSession() {
  let response;
  try { response = await fetch('/auth/session', { credentials: 'same-origin' }); } catch (_) { showUnavailable(); return; }
  if (!response.ok) { showSignedOut(); return; }
  state.session = await response.json(); showSignedIn();
  await Promise.all([loadCompatibility(), loadProviderConnections()]);
}

function showSignedOut() {
  $('#login').hidden = false; $('#logout-form').hidden = true; $('#provider-context').hidden = true;
  $('#account-shell').hidden = true; setAccountMenuOpen(false);
  $('#signed-out').hidden = false; $('#signed-in').hidden = true;
}
function showSignedIn() {
  $('#login').hidden = true; renderAccountIdentity(state.session); $('#account-shell').hidden = false; $('#signed-out').hidden = true; $('#signed-in').hidden = false;
  $('#session-dot').classList.add('online'); $('#session-label').textContent = 'Signed in'; $('#session-detail').textContent = 'Existing Chordrift library';
}
function showUnavailable() {
  $('#login').hidden = false; $('#logout-form').hidden = true; $('#account-shell').hidden = true; setAccountMenuOpen(false); $('#provider-context').hidden = true;
  $('#signed-out').hidden = false; $('#signed-out h1').textContent = 'Chordrift is temporarily unavailable.';
  $('#signed-out p:not(.eyebrow)').textContent = 'Your provider library was not changed.';
}

async function loadCompatibility() {
  state.compatibility = await contractRequest('/v1/compatibility', {
    contract_versions: { minimum: contractVersion, maximum: contractVersion }, schema_versions: { minimum: 48, maximum: 52 },
    requested_features: ['service.authenticated-transport.v1', 'service.product-identity.v1', 'service.provider-credential-vault.v1', 'service.durable-operations.v1', 'service.remote-cli.v1', 'maintenance.task-session.v1']
  });
  const observationAvailable = state.compatibility.features['service.durable-operations.v1'] === 'available';
  const maintenanceAvailable = state.compatibility.features['maintenance.task-session.v1'] === 'available';
  $('#start-maintenance').disabled = !observationAvailable || !state.provider?.credential_ready;
  $('#maintenance-availability').textContent = maintenanceAvailable
    ? 'Observation and ordinary maintenance are ready. Provider effects still require an exact review.'
    : observationAvailable
      ? 'Read-only provider observation is ready. Maintenance interpretation is the next production boundary.'
      : 'Hosted provider work is being connected. Library inspection is available now.';
}

async function loadProviderConnections() {
  const response = await contractRequest('/v1/queries', queryEnvelope({ type: 'provider_connections' }));
  state.connections = response.view.value.connections;
  const select = $('#provider-select'); select.replaceChildren();
  for (const connection of state.connections) {
    const provider = connection.provider === 'spotify' ? 'Spotify' : connection.provider;
    const option = node('option', '', `${provider} · ${connection.display_name || 'Account'}`);
    option.value = connection.provider_connection_id; select.append(option);
  }
  state.provider = state.connections[0] || null; $('#provider-context').hidden = false; renderProviderState();
  if (state.provider) await Promise.all([loadPlaylists(), loadComparison(), loadExclusions(), loadActivity()]);
}

function renderProviderState() {
  const select = $('#provider-select');
  select.hidden = !state.provider;
  $('#spotify-add').hidden = !state.provider;
  if (!state.provider) {
    $('#provider-dot').classList.remove('online');
    $('#provider-state').textContent = 'No music provider connected';
    $('#spotify-connect').hidden = false;
    $('#spotify-connect').textContent = 'Connect Spotify';
    $('#spotify-connect').href = '/providers/spotify/connect';
    $('#spotify-disconnect').hidden = true;
    $('#start-maintenance').disabled = true;
    return;
  }
  const connected = state.provider.credential_ready;
  $('#provider-dot').classList.toggle('online', connected);
  $('#provider-state').textContent = `${connected ? 'Authorized' : 'Reconnect required'} · last verified ${formatTime(state.provider.observed_at)}`;
  $('#spotify-connect').hidden = connected;
  $('#spotify-connect').textContent = 'Reconnect Spotify';
  $('#spotify-connect').href = `/providers/spotify/connect?provider_connection_id=${encodeURIComponent(state.provider.provider_connection_id)}`;
  $('#spotify-disconnect').hidden = !connected;
  $('#start-maintenance').disabled = !connected || state.compatibility?.features['service.durable-operations.v1'] !== 'available' || Boolean(state.activeOperation);
}

function commandEnvelope(command) {
  return { contract_version: contractVersion, request_id: uuid(), idempotency_key: uuid(), command };
}

async function startMaintenance() {
  if (!state.provider || state.activeOperation) return;
  const panel = $('#maintenance-session'); panel.hidden = false;
  panel.replaceChildren(node('strong', '', 'Maintenance queued'), node('p', 'availability', 'Waiting for the hosted worker. Provider state will be read; Spotify will not be changed.'));
  $('#start-maintenance').disabled = true;
  try {
    const sessionId = uuid();
    const receipt = await contractRequest('/v1/commands', commandEnvelope({ type: 'start_maintenance', parameters: { session_id: sessionId, provider_connection_id: state.provider.provider_connection_id } }));
    state.activeOperation = receipt;
    const operation = await followOperation(receipt.operation_id);
    if (operation.state.state === 'completed') await loadMaintenanceSession(operation.state.details?.result_id || sessionId);
  } catch (error) {
    panel.replaceChildren(node('strong', '', 'Observation could not start'), node('p', 'warning', error.message));
  } finally {
    state.activeOperation = null; renderProviderState();
  }
}

async function disconnectSpotify(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const button = form.querySelector('button');
  button.disabled = true;
  try {
    const receipt = await contractRequest('/v1/commands', commandEnvelope({
      type: 'disconnect_provider',
      parameters: { provider_connection_id: state.provider.provider_connection_id }
    }));
    state.activeOperation = receipt;
    await followOperation(receipt.operation_id);
    state.activeOperation = null;
    state.maintenanceSession = null;
    $('#maintenance-session').hidden = true;
    await loadProviderConnections();
  } catch (error) {
    const panel = $('#maintenance-session');
    panel.hidden = false;
    panel.replaceChildren(node('strong', '', 'Spotify could not disconnect'), node('p', 'warning', error.message));
  } finally {
    button.disabled = false;
  }
}

async function followOperation(operationId) {
  const panel = $('#maintenance-session');
  for (;;) {
    const response = await contractRequest('/v1/queries', queryEnvelope({ type: 'operation', parameters: { operation_id: operationId } }));
    const operation = response.view.value; const current = operation.state;
    const progress = current.details?.progress;
    const credentialExpired = ['failed', 'recoverable'].includes(current.state)
      && current.details?.error?.code === 'authentication_required';
    const failed = current.state === 'failed';
    const errorCode = current.details?.error?.code?.replaceAll('_', ' ');
    panel.replaceChildren(
      node('strong', '', current.state.replaceAll('_', ' ')),
      node('p', credentialExpired || failed ? 'warning' : 'availability', credentialExpired
        ? 'Spotify access was removed or expired. Reconnect Spotify; your Chordrift library is unchanged.'
        : failed
          ? `Chordrift stopped${errorCode ? ` · ${errorCode}` : ''}. Provider state will be checked again before any retry.`
          : progress ? `${progress.phase.replaceAll('_', ' ')} · ${progress.completed}${progress.total == null ? '' : ` / ${progress.total}`}` : 'Checking the latest provider state.')
    );
    if (['completed', 'failed', 'cancelled', 'recoverable'].includes(current.state)) {
      await Promise.all([loadProviderConnections(), loadActivity()]);
      return operation;
    }
    const cancel = node('button', '', 'Cancel safely'); cancel.type = 'button';
    cancel.addEventListener('click', cancelActiveOperation, { once: true }); panel.append(cancel);
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
}

async function loadMaintenanceSession(sessionId) {
  const response = await contractRequest('/v1/queries', queryEnvelope({ type: 'maintenance_session', parameters: { session_id: sessionId } }));
  state.maintenanceSession = response.view.value;
  if (!state.destinationPlaylists.length) {
    const destinations = await contractRequest('/v1/queries', queryEnvelope({ type: 'library_playlists', parameters: { provider_connection_id: state.provider.provider_connection_id, source: 'chordrift_model' } }));
    state.destinationPlaylists = destinations.view.value.playlists;
  }
  renderMaintenanceSession();
}

function renderMaintenanceSession() {
  const view = state.maintenanceSession; const panel = $('#maintenance-session');
  if (!view) return;
  panel.hidden = false; panel.replaceChildren();
  panel.append(node('strong', '', view.state.replaceAll('_', ' ')), node('p', 'availability', `Revision ${view.revision} · ${view.observed_changes.length} observed change${view.observed_changes.length === 1 ? '' : 's'} · ${view.provider_effects.length} exact provider change${view.provider_effects.length === 1 ? '' : 's'}`));
  const list = node('div', 'card-list');
  for (const change of view.observed_changes) {
    const card = node('div', 'record-card');
    const label = node('div'); label.append(node('strong', '', change.summary), node('span', '', change.resolution ? `Recorded · ${change.kind.replaceAll('_', ' ')}` : `Decision needed · ${change.kind.replaceAll('_', ' ')}`));
    if (!change.resolution && change.recommendation_reason) label.append(node('span', 'recommendation', `Suggested from ${change.recommendation_reason.toLowerCase()}. Review or change it before recording.`));
    card.append(label);
    if (!change.resolution) card.append(decisionControl(change));
    list.append(card);
  }
  if (!view.observed_changes.length) list.append(node('p', 'empty', 'Provider and Chordrift intent are already aligned.'));
  panel.append(list);
  if (view.provider_effects.length) {
    panel.append(node('h3', '', 'Exact provider changes'));
    if (view.provider_effects.some((effect) => effect.kind === 'add_track') && view.observed_changes.some((change) => change.kind === 'saved_state' && change.resolution?.type === 'consume_intake')) {
      panel.append(node('p', 'availability', 'Safe placement: Chordrift will add and verify the destination first. Removing the track from Liked Songs will be offered as a separate exact review afterward.'));
    }
    const effects = node('div', 'card-list');
    for (const effect of view.provider_effects) {
      const card = node('div', 'record-card');
      card.append(node('strong', '', effect.summary), node('span', '', effect.kind.replaceAll('_', ' '))); effects.append(card);
    }
    panel.append(effects);
  }
  if (view.allowed_actions.includes('resolve')) {
    const resolve = node('button', '', 'Record these decisions'); resolve.type = 'button';
    resolve.addEventListener('click', resolveObservedChanges); panel.append(resolve);
  }
  if (view.allowed_actions.includes('refresh')) {
    const refresh = node('button', '', 'Check provider again'); refresh.type = 'button';
    refresh.addEventListener('click', refreshMaintenance, { once: true }); panel.append(refresh);
  }
  if (view.allowed_actions.includes('authorize')) {
    const authorize = node('button', '', 'Apply exactly these changes'); authorize.type = 'button';
    authorize.addEventListener('click', authorizeMaintenance, { once: true }); panel.append(authorize);
  }
}

function decisionControl(change) {
  const select = node('select', 'decision-select'); select.dataset.changeId = change.change_id; select.dataset.kind = change.kind;
  if (['direct_intake', 'reclassification', 'removal'].includes(change.kind)) {
    const recommendedDestinationId = ChordriftMaintenance.recommendedDestinationId(change);
    const prompt = node('option', '', 'Choose destination…'); prompt.value = ''; select.append(prompt);
    for (const playlist of state.destinationPlaylists) {
      const option = node('option', '', playlist.name); option.value = JSON.stringify(playlist.maintenance_surface);
      if (recommendedDestinationId === playlist.maintenance_surface.surface_id) option.selected = true;
      select.append(option);
    }
    if (change.kind === 'removal') {
      const excluded = node('option', '', 'Keep removed · add to Excluded'); excluded.value = 'exclude'; select.append(excluded);
    }
  } else if (change.kind === 'saved_state') {
    const keep = node('option', '', 'Keep in Liked Songs'); keep.value = 'keep'; select.append(keep);
    const consume = node('option', '', 'Remove from Likes after placement'); consume.value = 'consume'; select.append(consume);
  } else {
    const keep = node('option', '', 'Keep observed provider state'); keep.value = 'keep'; select.append(keep);
  }
  return select;
}

async function runMaintenanceCommand(command) {
  const panel = $('#maintenance-session');
  const receipt = await contractRequest('/v1/commands', commandEnvelope(command));
  state.activeOperation = receipt; renderProviderState();
  try {
    const operation = await followOperation(receipt.operation_id);
    if (operation.state.state === 'completed') await loadMaintenanceSession(state.maintenanceSession.session_id);
  } finally { state.activeOperation = null; renderProviderState(); }
}

async function refreshMaintenance() {
  const view = state.maintenanceSession;
  await runMaintenanceCommand({ type: 'refresh_maintenance', parameters: { session_id: view.session_id, expected_revision: view.revision } });
}

async function resolveObservedChanges(event) {
  const view = state.maintenanceSession;
  const button = event?.currentTarget;
  if (button) button.disabled = true;
  const decisions = [];
  try {
    for (const change of view.observed_changes.filter((item) => !item.resolution)) {
      const select = document.querySelector(`[data-change-id="${change.change_id}"]`); const selected = select?.value;
      if (!selected) { select?.focus(); throw new Error(`Choose an answer for ${change.summary}.`); }
      const resolution = await ChordriftMaintenance.resolution(change, selected);
      decisions.push({ change_id: change.change_id, resolution });
    }
    await runMaintenanceCommand({ type: 'resolve_maintenance', parameters: { session_id: view.session_id, expected_revision: view.revision, decisions } });
  } catch (error) {
    $('#maintenance-session').append(node('p', 'warning', `Decisions were not recorded: ${error.message}`));
    if (button?.isConnected) button.disabled = false;
  }
}

async function authorizeMaintenance() {
  const view = state.maintenanceSession;
  if (!view.review_id || !window.confirm('Apply only the exact provider changes shown above?')) return;
  await runMaintenanceCommand({ type: 'authorize_maintenance', parameters: { session_id: view.session_id, expected_revision: view.revision, review_id: view.review_id } });
}

async function cancelActiveOperation() {
  if (!state.activeOperation) return;
  await contractRequest('/v1/commands', commandEnvelope({
    type: 'cancel_operation',
    parameters: {
      operation_id: state.activeOperation.operation_id,
      cancellation_id: state.activeOperation.cancellation_id
    }
  }));
}

async function loadPlaylists() {
  if (!state.provider) return;
  const response = await contractRequest('/v1/queries', queryEnvelope({ type: 'library_playlists', parameters: { provider_connection_id: state.provider.provider_connection_id, source: state.source } }));
  const view = response.view.value;
  $('#library-context').textContent = state.source === 'provider_observation'
    ? `Newest complete ${state.provider.provider} observation · ${formatTime(view.state_at)}. Refreshing the provider is a separate read-only action.`
    : `Chordrift's newest editable or approved managed model · ${formatTime(view.state_at)}.`;
  const list = $('#playlist-list'); list.replaceChildren();
  if (!view.playlists.length) list.append(node('p', 'empty', 'No playlists in this state.'));
  for (const playlist of view.playlists) {
    const button = node('button', 'playlist-row'); button.type = 'button';
    button.append(node('strong', '', playlist.name), node('span', '', `${playlist.track_count.toLocaleString()} tracks`));
    button.addEventListener('click', () => loadPlaylistTracks(playlist, button)); list.append(button);
  }
}

async function loadPlaylistTracks(playlist, button) {
  $$('.playlist-row').forEach((row) => row.classList.remove('active')); button.classList.add('active');
  $('#playlist-title').textContent = playlist.name; $('#playlist-count').textContent = `${playlist.track_count.toLocaleString()} tracks`;
  const table = $('#playlist-tracks'); showTableError(table, 'Loading ordered membership…');
  try {
    const response = await contractRequest('/v1/queries', queryEnvelope({ type: 'library_playlist_tracks', parameters: { provider_connection_id: state.provider.provider_connection_id, playlist_id: playlist.playlist_id, source: state.source } }));
    state.playlistTracks = response.view.value.tracks; renderPlaylistTracks();
  } catch (error) { showTableError(table, error.message); }
}

function renderPlaylistTracks() {
  const table = $('#playlist-tracks'); table.replaceChildren();
  const tracks = ChordriftLibraryExplorer.sortPlaylistTracks(state.playlistTracks, $('#playlist-sort').value);
  for (const track of tracks) {
    const row = node('tr', 'track-row'); row.tabIndex = 0; row.append(node('td', 'position', track.position));
    const identity = node('td', 'track-identity'); identity.append(node('strong', '', track.title), node('span', '', track.artists));
    row.append(identity, node('td', 'album', track.album || '—'), node('td', 'listening-cell', track.play_count.toLocaleString()), node('td', 'listening-cell', formatTime(track.last_played_at)));
    row.addEventListener('click', () => loadTrack(track.provider_track_id));
    row.addEventListener('keydown', (event) => { if (event.key === 'Enter') loadTrack(track.provider_track_id); });
    table.append(row);
  }
}

function showTableError(table, message) {
  table.replaceChildren(); const row = node('tr'); const cell = node('td', 'empty', message); cell.colSpan = 5; row.append(cell); table.append(row);
}

async function loadTrack(providerTrackId) {
  const detail = $('#track-detail'); detail.replaceChildren(node('p', 'empty', 'Loading track history…')); $('#track-dialog').showModal();
  try {
    const response = await contractRequest('/v1/queries', queryEnvelope({ type: 'library_track', parameters: { provider_connection_id: state.provider.provider_connection_id, provider_track_id: providerTrackId } }));
    const track = response.view.value; detail.replaceChildren();
    detail.append(node('p', 'eyebrow', 'Track detail'), node('h2', 'dialog-title', track.title), node('p', 'dialog-artists', track.artists));
    const metrics = node('div', 'metrics');
    for (const [label, value] of [['Meaningful plays', track.play_count], ['Listening events', track.event_count], ['Last heard', formatTime(track.last_played_at)], ['Liked now', track.saved ? 'Yes' : 'No']]) {
      const metric = node('div', 'metric'); metric.append(node('strong', '', value), node('span', '', label)); metrics.append(metric);
    }
    detail.append(metrics, node('h3', '', 'Current placements'));
    const placements = node('div', 'card-list');
    for (const placement of track.placements) {
      const card = node('div', 'compact-card'); card.append(node('strong', '', placement.name), node('span', '', `Position ${placement.position} · ${placement.source.replaceAll('_', ' ')}`)); placements.append(card);
    }
    if (!track.placements.length) placements.append(node('p', 'empty', 'Not currently placed.'));
    detail.append(placements); if (track.exclusion_reason) detail.append(node('p', 'warning', `Excluded: ${track.exclusion_reason}`));
  } catch (error) { detail.replaceChildren(node('p', 'warning', error.message)); }
}

async function loadExclusions() {
  if (!state.provider) return;
  try {
    const response = await contractRequest('/v1/queries', queryEnvelope({ type: 'excluded_tracks', parameters: { provider_connection_id: state.provider.provider_connection_id } }));
    state.excludedTracks = response.view.value.tracks; $('#exclusion-count').textContent = state.excludedTracks.length.toLocaleString(); renderExclusions();
  } catch (error) { $('#exclusion-list').replaceChildren(node('p', 'warning', error.message)); }
}

function renderExclusions() {
  const list = $('#exclusion-list'); list.replaceChildren();
  const tracks = ChordriftLibraryExplorer.sortExcludedTracks(state.excludedTracks, $('#exclusion-sort').value);
  const groupMode = $('#exclusion-group').value; let previousGroup = null;
  for (const track of tracks) {
    const group = ChordriftLibraryExplorer.excludedGroup(track, groupMode);
    if (group && group !== previousGroup) { list.append(node('h2', 'group-heading', group)); previousGroup = group; }
    const card = node('button', 'record-card'); card.type = 'button';
    const text = node('div'); text.append(node('strong', '', track.title), node('span', '', `${track.artists}${track.album ? ` · ${track.album}` : ''}`));
    const meta = node('div', 'record-meta');
    meta.append(node('strong', '', `${track.play_count.toLocaleString()} plays · ${formatTime(track.last_played_at)}`), node('span', '', `${track.previous_playlist || 'No prior playlist'} · excluded ${formatTime(track.excluded_at)}`));
    card.append(text, meta); card.addEventListener('click', () => loadTrack(track.provider_track_id)); list.append(card);
  }
  if (!tracks.length) list.append(node('p', 'empty', 'The exclusion archive is empty.'));
}

async function loadComparison() {
  if (!state.provider) return;
  const panel = $('#library-comparison');
  try {
    const response = await contractRequest('/v1/queries', queryEnvelope({ type: 'library_comparison', parameters: { provider_connection_id: state.provider.provider_connection_id } }));
    const comparison = response.view.value; panel.replaceChildren();
    const summary = node('div', 'comparison-heading');
    summary.append(node('strong', '', `${comparison.aligned_playlists} aligned`), node('span', '', `${comparison.differing_playlists} need explanation`)); panel.append(summary);
    const differing = comparison.playlists.filter((playlist) => playlist.status !== 'aligned');
    if (!differing.length) { panel.append(node('p', 'empty', 'Provider membership, Chordrift membership, and custom order are aligned.')); return; }
    const list = node('div', 'card-list');
    for (const playlist of differing) {
      const card = node('div', 'compact-card');
      const identity = node('div'); identity.append(node('strong', '', playlist.name), node('span', '', playlist.explanation));
      const counts = node('div', 'record-meta'); counts.append(node('strong', '', `${playlist.provider_track_count} provider · ${playlist.chordrift_track_count} Chordrift`), node('span', '', playlist.status.replaceAll('_', ' ')));
      card.append(identity, counts); list.append(card);
    }
    panel.append(list);
  } catch (error) { panel.replaceChildren(node('p', 'warning', `Comparison unavailable: ${error.message}`)); }
}

async function loadActivity() {
  if (!state.session) return;
  try {
    const response = await contractRequest('/v1/queries', queryEnvelope({ type: 'operation_history', parameters: { account_id: state.session.account_id } }));
    const list = $('#operation-list'); list.replaceChildren();
    for (const operation of response.view.value.operations) {
      const card = node('div', 'record-card'); card.append(node('strong', '', operation.state.state.replaceAll('_', ' ')), node('span', '', operation.operation_id)); list.append(card);
    }
    if (!response.view.value.operations.length) list.append(node('p', 'empty', 'No hosted operations yet.'));
  } catch (error) { $('#operation-list').replaceChildren(node('p', 'warning', error.message)); }
}

function requestFor(name) {
  if (name === 'compatibility') return { path: '/v1/compatibility', body: { contract_versions: { minimum: contractVersion, maximum: contractVersion }, schema_versions: { minimum: 48, maximum: 52 }, requested_features: ['service.authenticated-transport.v1', 'service.product-identity.v1', 'service.provider-credential-vault.v1', 'service.durable-operations.v1', 'service.remote-cli.v1', 'maintenance.task-session.v1'] } };
  if (name === 'provider_connections') return { path: '/v1/queries', body: queryEnvelope({ type: 'provider_connections' }) };
  if (name === 'library_comparison') return { path: '/v1/queries', body: queryEnvelope({ type: 'library_comparison', parameters: { provider_connection_id: state.provider?.provider_connection_id || 'UUID' } }) };
  if (name === 'operation_history') return { path: '/v1/queries', body: queryEnvelope({ type: 'operation_history', parameters: { account_id: state.session?.account_id || 'ACCOUNT_ID' } }) };
  return { path: name === 'custom_command' ? '/v1/commands' : '/v1/queries', body: name === 'custom_command'
    ? { contract_version: contractVersion, request_id: uuid(), idempotency_key: uuid(), command: { type: 'observe_provider', parameters: { provider_connection_id: state.provider?.provider_connection_id || 'UUID' } } }
    : queryEnvelope({ type: 'diagnostics', parameters: { operation_id: null } }) };
}
function selectPreset() {
  const request = requestFor($('#preset').value); $('#request').value = JSON.stringify(request.body, null, 2); $('#request').dataset.path = request.path; $('#request-kind').textContent = `POST ${request.path}`;
}
async function sendDeveloperRequest() {
  let body; try { body = JSON.parse($('#request').value); } catch (error) { $('#response').textContent = `Invalid JSON: ${error.message}`; return; }
  $('#send').disabled = true; $('#http-status').textContent = 'Sending…';
  try { const value = await contractRequest($('#request').dataset.path, body); $('#http-status').textContent = '200'; $('#response').textContent = JSON.stringify(value, null, 2); }
  catch (error) { $('#http-status').textContent = 'Error'; $('#response').textContent = JSON.stringify(error.response || { error: error.message }, null, 2); }
  finally { $('#send').disabled = false; }
}

$$('.tab').forEach((button) => button.addEventListener('click', () => {
  $$('.tab').forEach((tab) => {
    const active = tab === button;
    tab.classList.toggle('active', active);
    tab.setAttribute('aria-selected', String(active));
  });
  $$('.view').forEach((view) => view.classList.toggle('active', view.id === `view-${button.dataset.view}`));
}));
const tabs = $$('.tab');
tabs.forEach((button, index) => {
  const viewId = `view-${button.dataset.view}`;
  button.setAttribute('role', 'tab');
  button.setAttribute('id', `tab-${button.dataset.view}`);
  button.setAttribute('aria-controls', viewId);
  button.setAttribute('aria-selected', String(index === 0));
});
$$('.view').forEach((view) => {
  view.setAttribute('role', 'tabpanel');
  const tabForView = $(`.tab[data-view="${view.id.replace('view-', '')}"]`);
  if (tabForView) view.setAttribute('aria-labelledby', tabForView.id);
});
document.querySelector('.section-tabs')?.setAttribute('role', 'tablist');

const accountButton = $('#account-button');
if (accountButton) {
  accountButton.addEventListener('click', () => setAccountMenuOpen($('#account-menu').hidden));
  accountButton.addEventListener('keydown', (event) => {
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      setAccountMenuOpen(true);
      $('#account-menu')?.querySelector('[role="menuitem"]')?.focus();
    }
  });
}
document.addEventListener('click', (event) => {
  const shell = $('#account-shell');
  if (!shell || shell.hidden || $('#account-menu').hidden) return;
  if (!shell.contains(event.target)) setAccountMenuOpen(false);
});
document.addEventListener('keydown', (event) => {
  if (event.key === 'Escape') setAccountMenuOpen(false);
});

$$('[data-source]').forEach((button) => button.addEventListener('click', () => {
  state.source = button.dataset.source; $$('[data-source]').forEach((item) => item.classList.toggle('active', item === button));
  $('#playlist-title').textContent = 'Choose a playlist'; $('#playlist-count').textContent = ''; showTableError($('#playlist-tracks'), 'Select a playlist to inspect its recorded order.'); loadPlaylists();
}));
$('#provider-select').addEventListener('change', async (event) => {
  state.provider = state.connections.find((connection) => connection.provider_connection_id === event.target.value); renderProviderState(); await Promise.all([loadPlaylists(), loadComparison(), loadExclusions()]);
});
$('#preset').addEventListener('change', selectPreset); $('#send').addEventListener('click', sendDeveloperRequest);
$('#playlist-sort').addEventListener('change', renderPlaylistTracks);
$('#exclusion-sort').addEventListener('change', renderExclusions);
$('#exclusion-group').addEventListener('change', renderExclusions);
$('#start-maintenance').addEventListener('click', startMaintenance);
$('#spotify-disconnect').addEventListener('submit', disconnectSpotify);
$('.dialog-close').addEventListener('click', () => $('#track-dialog').close());
$('#track-dialog').addEventListener('click', (event) => { if (event.target === $('#track-dialog')) $('#track-dialog').close(); });
selectPreset(); loadSession();
