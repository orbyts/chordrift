const $ = (selector) => document.querySelector(selector);
const $$ = (selector) => [...document.querySelectorAll(selector)];
const uuid = () => crypto.randomUUID();
const contractVersion = { major: 1, minor: 3 };
const state = { session: null, compatibility: null, connections: [], provider: null, source: 'provider_observation', activeOperation: null, maintenanceSession: null };

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

async function loadSession() {
  let response;
  try { response = await fetch('/auth/session', { credentials: 'same-origin' }); } catch (_) { showUnavailable(); return; }
  if (!response.ok) { showSignedOut(); return; }
  state.session = await response.json(); showSignedIn();
  await Promise.all([loadCompatibility(), loadProviderConnections()]);
}

function showSignedOut() {
  $('#login').hidden = false; $('#logout-form').hidden = true; $('#provider-context').hidden = true;
  $('#signed-out').hidden = false; $('#signed-in').hidden = true;
}
function showSignedIn() {
  $('#login').hidden = true; $('#logout-form').hidden = false; $('#signed-out').hidden = true; $('#signed-in').hidden = false;
  $('#session-dot').classList.add('online'); $('#session-label').textContent = 'Signed in'; $('#session-detail').textContent = 'Existing Chordrift library';
}
function showUnavailable() {
  $('#signed-out').hidden = false; $('#signed-out h1').textContent = 'Chordrift is temporarily unavailable.';
  $('#signed-out p:not(.eyebrow)').textContent = 'Your provider library was not changed.';
}

async function loadCompatibility() {
  state.compatibility = await contractRequest('/v1/compatibility', {
    contract_versions: { minimum: contractVersion, maximum: contractVersion }, schema_versions: { minimum: 48, maximum: 51 },
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
  state.provider = state.connections[0] || null; $('#provider-context').hidden = !state.provider; renderProviderState();
  if (state.provider) await Promise.all([loadPlaylists(), loadExclusions(), loadActivity()]);
}

function renderProviderState() {
  if (!state.provider) return;
  const connected = state.provider.credential_ready;
  $('#provider-dot').classList.toggle('online', connected);
  $('#provider-state').textContent = `${connected ? 'Connected' : 'Read-only record'} · observed ${formatTime(state.provider.observed_at)}`;
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

async function followOperation(operationId) {
  const panel = $('#maintenance-session');
  for (;;) {
    const response = await contractRequest('/v1/queries', queryEnvelope({ type: 'operation', parameters: { operation_id: operationId } }));
    const operation = response.view.value; const current = operation.state;
    const progress = current.details?.progress;
    panel.replaceChildren(
      node('strong', '', current.state.replaceAll('_', ' ')),
      node('p', 'availability', progress ? `${progress.phase.replaceAll('_', ' ')} · ${progress.completed}${progress.total == null ? '' : ` / ${progress.total}`}` : 'Spotify remains unchanged during observation.')
    );
    if (['completed', 'failed', 'cancelled'].includes(current.state)) {
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
  renderMaintenanceSession();
}

function renderMaintenanceSession() {
  const view = state.maintenanceSession; const panel = $('#maintenance-session');
  if (!view) return;
  panel.hidden = false; panel.replaceChildren();
  panel.append(node('strong', '', view.state.replaceAll('_', ' ')), node('p', 'availability', `Revision ${view.revision} · ${view.observed_changes.length} observed change${view.observed_changes.length === 1 ? '' : 's'} · no provider write authorized`));
  const list = node('div', 'card-list');
  for (const change of view.observed_changes) {
    const card = node('div', 'record-card');
    card.append(node('strong', '', change.summary), node('span', '', change.resolution ? `Recorded · ${change.kind.replaceAll('_', ' ')}` : `Decision needed · ${change.kind.replaceAll('_', ' ')}`));
    list.append(card);
  }
  if (!view.observed_changes.length) list.append(node('p', 'empty', 'Provider and Chordrift intent are already aligned.'));
  panel.append(list);
  if (view.allowed_actions.includes('resolve')) {
    const resolve = node('button', '', 'Keep the observed provider state'); resolve.type = 'button';
    resolve.addEventListener('click', resolveObservedChanges, { once: true }); panel.append(resolve);
  }
  if (view.allowed_actions.includes('refresh')) {
    const refresh = node('button', '', 'Check provider again'); refresh.type = 'button';
    refresh.addEventListener('click', refreshMaintenance, { once: true }); panel.append(refresh);
  }
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

async function resolveObservedChanges() {
  const view = state.maintenanceSession;
  const decisions = view.observed_changes.filter((change) => !change.resolution).map((change) => ({ change_id: change.change_id, resolution: { type: 'keep_observed' } }));
  await runMaintenanceCommand({ type: 'resolve_maintenance', parameters: { session_id: view.session_id, expected_revision: view.revision, decisions } });
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
    table.replaceChildren();
    for (const track of response.view.value.tracks) {
      const row = node('tr', 'track-row'); row.tabIndex = 0; row.append(node('td', 'position', track.position));
      const identity = node('td', 'track-identity'); identity.append(node('strong', '', track.title), node('span', '', track.artists));
      row.append(identity, node('td', 'album', track.album || '—'));
      row.addEventListener('click', () => loadTrack(track.provider_track_id));
      row.addEventListener('keydown', (event) => { if (event.key === 'Enter') loadTrack(track.provider_track_id); });
      table.append(row);
    }
  } catch (error) { showTableError(table, error.message); }
}

function showTableError(table, message) {
  table.replaceChildren(); const row = node('tr'); const cell = node('td', 'empty', message); cell.colSpan = 3; row.append(cell); table.append(row);
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
    const tracks = response.view.value.tracks; $('#exclusion-count').textContent = tracks.length.toLocaleString();
    const list = $('#exclusion-list'); list.replaceChildren();
    for (const track of tracks) {
      const card = node('button', 'record-card'); card.type = 'button';
      const text = node('div'); text.append(node('strong', '', track.title), node('span', '', track.artists));
      const meta = node('div', 'record-meta'); meta.append(node('span', '', track.previous_playlist || 'No prior playlist'), node('span', '', formatTime(track.excluded_at)));
      card.append(text, meta); card.addEventListener('click', () => loadTrack(track.provider_track_id)); list.append(card);
    }
    if (!tracks.length) list.append(node('p', 'empty', 'The exclusion archive is empty.'));
  } catch (error) { $('#exclusion-list').replaceChildren(node('p', 'warning', error.message)); }
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
  if (name === 'compatibility') return { path: '/v1/compatibility', body: { contract_versions: { minimum: contractVersion, maximum: contractVersion }, schema_versions: { minimum: 48, maximum: 51 }, requested_features: ['service.authenticated-transport.v1', 'service.product-identity.v1', 'service.provider-credential-vault.v1', 'service.durable-operations.v1', 'service.remote-cli.v1', 'maintenance.task-session.v1'] } };
  if (name === 'provider_connections') return { path: '/v1/queries', body: queryEnvelope({ type: 'provider_connections' }) };
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
  $$('.tab').forEach((tab) => tab.classList.toggle('active', tab === button));
  $$('.view').forEach((view) => view.classList.toggle('active', view.id === `view-${button.dataset.view}`));
}));
$$('[data-source]').forEach((button) => button.addEventListener('click', () => {
  state.source = button.dataset.source; $$('[data-source]').forEach((item) => item.classList.toggle('active', item === button));
  $('#playlist-title').textContent = 'Choose a playlist'; $('#playlist-count').textContent = ''; showTableError($('#playlist-tracks'), 'Select a playlist to inspect its recorded order.'); loadPlaylists();
}));
$('#provider-select').addEventListener('change', async (event) => {
  state.provider = state.connections.find((connection) => connection.provider_connection_id === event.target.value); renderProviderState(); await Promise.all([loadPlaylists(), loadExclusions()]);
});
$('#preset').addEventListener('change', selectPreset); $('#send').addEventListener('click', sendDeveloperRequest);
$('#start-maintenance').addEventListener('click', startMaintenance);
$('.dialog-close').addEventListener('click', () => $('#track-dialog').close());
$('#track-dialog').addEventListener('click', (event) => { if (event.target === $('#track-dialog')) $('#track-dialog').close(); });
selectPreset(); loadSession();
