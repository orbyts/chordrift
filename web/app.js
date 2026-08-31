const requestBox = document.querySelector('#request');
const responseBox = document.querySelector('#response');
const preset = document.querySelector('#preset');
const send = document.querySelector('#send');
const statusLabel = document.querySelector('#session-label');
const statusDetail = document.querySelector('#session-detail');
const statusDot = document.querySelector('#session-dot');
const httpStatus = document.querySelector('#http-status');
const requestKind = document.querySelector('#request-kind');

const uuid = () => crypto.randomUUID();
const contractVersion = { major: 1, minor: 3 };

function requestFor(name) {
  if (name === 'compatibility') {
    return {
      path: '/v1/compatibility',
      body: {
        contract_versions: { minimum: contractVersion, maximum: contractVersion },
        schema_versions: { minimum: 48, maximum: 50 },
        requested_features: [
          'service.authenticated-transport.v1',
          'service.product-identity.v1',
          'service.provider-credential-vault.v1',
          'service.durable-operations.v1',
          'service.remote-cli.v1'
        ]
      }
    };
  }
  if (name === 'operation_history') {
    return {
      path: '/v1/queries',
      body: {
        contract_version: contractVersion,
        request_id: uuid(),
        query: { type: 'operation_history', parameters: { account_id: 'ACCOUNT_ID_FROM_SESSION' } }
      }
    };
  }
  return {
    path: name === 'custom_command' ? '/v1/commands' : '/v1/queries',
    body: {
      contract_version: contractVersion,
      request_id: uuid(),
      ...(name === 'custom_command'
        ? { idempotency_key: uuid(), command: { type: 'observe_provider', parameters: { provider_connection_id: 'UUID' } } }
        : { query: { type: 'diagnostics', parameters: { operation_id: null } } })
    }
  };
}

function selectPreset() {
  const request = requestFor(preset.value);
  requestBox.value = JSON.stringify(request.body, null, 2);
  requestBox.dataset.path = request.path;
  requestKind.textContent = `POST ${request.path}`;
}

async function sessionStatus() {
  try {
    const response = await fetch('/auth/session', { credentials: 'same-origin' });
    const body = await response.json();
    statusDot.className = response.ok ? 'online' : '';
    statusLabel.textContent = response.ok ? 'Signed in to Chordrift' : 'Not signed in';
    statusDetail.textContent = body.account_id ? `Account ${body.account_id}` : '';
    if (body.account_id && preset.value === 'operation_history') {
      selectPreset();
      requestBox.value = requestBox.value.replace('ACCOUNT_ID_FROM_SESSION', body.account_id);
    }
  } catch (_) {
    statusLabel.textContent = 'Service unavailable';
    statusDetail.textContent = '';
  }
}

async function sendRequest() {
  let body;
  try {
    body = JSON.parse(requestBox.value);
  } catch (error) {
    responseBox.textContent = `Invalid JSON: ${error.message}`;
    return;
  }
  send.disabled = true;
  httpStatus.textContent = 'Sending…';
  try {
    const response = await fetch(requestBox.dataset.path, {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'content-type': 'application/json', 'x-chordrift-browser': '1' },
      body: JSON.stringify(body)
    });
    const text = await response.text();
    httpStatus.textContent = `${response.status} ${response.statusText}`;
    try { responseBox.textContent = JSON.stringify(JSON.parse(text), null, 2); }
    catch (_) { responseBox.textContent = text || '(empty response)'; }
  } catch (error) {
    httpStatus.textContent = 'Network error';
    responseBox.textContent = error.message;
  } finally {
    send.disabled = false;
  }
}

preset.addEventListener('change', () => { selectPreset(); sessionStatus(); });
send.addEventListener('click', sendRequest);
selectPreset();
sessionStatus();
