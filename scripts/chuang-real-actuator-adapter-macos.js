ObjC.import('Foundation');

const auditLabel = 'actuator.operation.live';
const requiredEnv = 'CHUANG_REAL_ACTUATOR_ENABLE';

function run(argv) {
  if (argv.length !== 1) throw new Error('usage: actuator-macos.js ALLOWLIST');
  const allowlist = JSON.parse(readText(argv[0]));
  const requestData = $.NSFileHandle.fileHandleWithStandardInput.readDataToEndOfFile;
  const requestText = ObjC.unwrap($.NSString.alloc.initWithDataEncoding(requestData, $.NSUTF8StringEncoding));
  if (!requestText.trim()) throw new Error('actuator request JSON is empty');
  const request = JSON.parse(requestText);
  const result = handle(allowlist, request);
  return JSON.stringify(result);
}

function readText(path) {
  const text = $.NSString.stringWithContentsOfFileEncodingError(path, $.NSUTF8StringEncoding, null);
  if (!text) throw new Error('actuator allowlist not found: ' + path);
  return ObjC.unwrap(text);
}

function response(observation, appHandle, evidenceRef, message) {
  return {
    observation: observation || null,
    app_handle: appHandle || null,
    evidence_ref: evidenceRef || null,
    message: message || 'ok'
  };
}

function boundary(action, options) {
  options = options || {};
  const readOnly = !!options.readOnly;
  const real = !!options.real;
  const prefix = real
    ? 'allowlisted live actuator operation requested'
    : readOnly
      ? 'allowlisted read-only actuator observation'
      : 'dry-run actuator operation accepted';
  let message = prefix
    + '; allowed=true dry_run=' + ((!real && !readOnly) ? 'true' : 'false')
    + ' action=' + action
    + ' real_execution=' + (real ? 'true' : 'false')
    + ' read_only=' + (readOnly ? 'true' : 'false')
    + ' live_gate_required=' + (readOnly ? 'false' : 'true')
    + ' audit_label=' + auditLabel
    + ' required_env=' + requiredEnv
    + ' platform=macos';
  if (options.evidencePath) message += ' evidence_path=' + options.evidencePath;
  return message;
}

function environment(name) {
  const value = $.NSProcessInfo.processInfo.environment.objectForKey(name);
  return value ? ObjC.unwrap(value) : '';
}

function liveEnabled() {
  return environment(requiredEnv) === '1';
}

function shellQuote(value) {
  return "'" + String(value).replace(/'/g, "'\\''") + "'";
}

function shell(command) {
  const app = Application.currentApplication();
  app.includeStandardAdditions = true;
  return app.doShellScript(command);
}

function frontmostState() {
  const events = Application('System Events');
  const processes = events.applicationProcesses.whose({ frontmost: true })();
  if (!processes.length) return { app: 'unavailable', title: 'unavailable' };
  const process = processes[0];
  let title = 'unavailable';
  try {
    const windows = process.windows();
    if (windows.length) title = windows[0].name();
  } catch (_) {}
  return { app: process.name(), title: title };
}

function saveScreenshot(allowlist) {
  if (!allowlist.screenshot_allowed) throw new Error('screenshot not allowlisted');
  let root = environment('CHUANG_ACTUATOR_EVIDENCE_DIR');
  if (!root) root = ObjC.unwrap($.NSHomeDirectory()) + '/Library/Application Support/chuang-agent/evidence';
  shell('/bin/mkdir -p ' + shellQuote(root));
  const path = root + '/screenshot-' + Date.now() + '-' + $.NSProcessInfo.processInfo.processIdentifier + '.png';
  try {
    shell('/usr/sbin/screencapture -x ' + shellQuote(path));
  } catch (error) {
    throw new Error('macOS screenshot failed; allow Screen Recording permission for Terminal: ' + error);
  }
  if (!$.NSFileManager.defaultManager.fileExistsAtPath(path)) throw new Error('macOS screenshot file was not created');
  const uri = ObjC.unwrap($.NSURL.fileURLWithPath(path).absoluteString);
  return response(null, null, { uri: uri }, boundary('screenshot', { readOnly: true, evidencePath: path }));
}

function findApp(allowlist, name) {
  return (allowlist.apps || []).find(function (entry) { return entry.app_name === name; });
}

function handle(allowlist, request) {
  const action = String(request.action || '');
  if (action === 'observe') {
    const state = frontmostState();
    return response({
      target: request.observe_target,
      summary: 'current_app=' + state.app + ' current_window_title=' + state.title + ' platform=macos',
      evidence_ref: { uri: 'chuang-actuator://observe/macos' }
    }, null, null, boundary('observe', { readOnly: true }));
  }
  if (action === 'screenshot') return saveScreenshot(allowlist);
  if (action === 'open_app') {
    const name = String((request.open_app || {}).app_name || '');
    const app = findApp(allowlist, name);
    if (!app) throw new Error('app not allowlisted: ' + name);
    if (liveEnabled()) {
      shell((app.open_command || []).map(shellQuote).join(' ') + ' >/dev/null 2>&1 &');
      return response(null, { app_name: name, handle_id: 'chuang-actuator://app/' + name }, null, boundary('open_app', { real: true }));
    }
    return response(null, { app_name: name, handle_id: 'chuang-actuator://app/' + name }, null, boundary('open_app'));
  }
  if (action === 'focus') {
    if (!allowlist.focus_allowed) throw new Error('focus not allowlisted');
    const target = String(request.focus_target || '');
    if (liveEnabled() && target) {
      Application(target).activate();
      return response(null, null, null, boundary('focus', { real: true }));
    }
    return response(null, null, null, boundary('focus'));
  }
  if (action === 'click') {
    if (!allowlist.click_allowed) throw new Error('click not allowlisted');
    if (liveEnabled()) {
      const coordinates = (request.click_target || {}).Coordinates;
      if (!coordinates) throw new Error('click target missing Coordinates');
      shell("/usr/bin/osascript -e 'tell application \"System Events\" to click at {" + Number(coordinates.x) + ',' + Number(coordinates.y) + "}'");
      return response(null, null, null, boundary('click', { real: true }));
    }
    return response(null, null, null, boundary('click'));
  }
  if (action === 'input_text') {
    if (!allowlist.input_allowed) throw new Error('input_text not allowlisted');
    if (request.text && request.text.Secret) throw new Error('secret input is not supported by this command adapter');
    const text = String((request.text || {}).Plain || '');
    if (liveEnabled()) {
      Application('System Events').keystroke(text);
      return response(null, null, null, boundary('input_text', { real: true }));
    }
    return response(null, null, null, boundary('input_text'));
  }
  throw new Error('unsupported actuator action: ' + action);
}
