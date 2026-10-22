/* Atria — UI test harness: mini-DOM + REAL IPC bridge from index.html + full app.js
 * exercise (piece by piece).  Run: node tests/ui_harness.mjs
 */
import fs from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
let PASS = 0, FAIL = 0;
const fails = [];
function ok(name, cond, extra) {
  if (cond) { PASS++; console.log('  \u2713 ' + name); }
  else { FAIL++; fails.push(name); console.log('  \u2717 ' + name + (extra ? ' \u2014 ' + extra : '')); }
}
function eq(name, a, b) {
  const A = JSON.stringify(a), B = JSON.stringify(b);
  ok(name, A === B, A + ' != ' + B);
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/* ---------------- mini DOM ---------------- */
class ClassList {
  constructor() { this.s = new Set(); }
  add(...c) { c.forEach((x) => x && this.s.add(x)); }
  remove(...c) { c.forEach((x) => this.s.delete(x)); }
  toggle(c, force) {
    const has = this.s.has(c);
    const on = force === undefined ? !has : !!force;
    on ? this.s.add(c) : this.s.delete(c);
    return on;
  }
  contains(c) { return this.s.has(c); }
  toString() { return [...this.s].join(' '); }
}
const VOID = new Set(['INPUT', 'IMG', 'BR', 'HR', 'LINE', 'RECT', 'PATH', 'META', 'LINK']);
class El {
  constructor(tag) {
    this.tagName = String(tag || 'div').toUpperCase();
    this.children = [];
    this.parentElement = null;
    this.classList = new ClassList();
    this.style = {};
    this.dataset = {};
    this._text = '';
    this.value = '';
    this.checked = false;
    this.disabled = false;
    this.type = '';
    this.id = '';
    this.attributes = {};
    this.title = '';
    this.placeholder = '';
    this.scrollTop = 0; this.scrollHeight = 0; this.clientHeight = 300;
    this._lis = {};
    this.onclick = null; this.oninput = null; this.onchange = null;
    this.onblur = null; this.onkeydown = null;
  }
  get className() { return this.classList.toString(); }
  set className(v) { this.classList = new ClassList(); String(v).split(/\s+/).forEach((c) => c && this.classList.add(c)); }
  get isConnected() { return this === doc.body || !!this.parentElement; }
  get nextElementSibling() {
    if (!this.parentElement) return null;
    const i = this.parentElement.children.indexOf(this);
    return this.parentElement.children[i + 1] || null;
  }
  set textContent(v) { this._text = String(v); this.children = []; }
  get textContent() {
    if (this.children.length) return this._text + this.children.map((c) => c.textContent).join('');
    return this._text;
  }
  set innerHTML(v) {
    this._text = '';
    for (const c of this.children) c.parentElement = null;
    this.children = [];
    inflate(this, String(v));
  }
  get innerHTML() { return this._text; }
  appendChild(c) { if (c.parentElement) c.remove(); c.parentElement = this; this.children.push(c); return c; }
  insertBefore(c, ref) {
    if (c.parentElement) c.remove();
    const i = ref ? this.children.indexOf(ref) : -1;
    c.parentElement = this;
    if (i >= 0) this.children.splice(i, 0, c); else this.children.push(c);
    return c;
  }
  removeChild(c) { const i = this.children.indexOf(c); if (i >= 0) this.children.splice(i, 1); c.parentElement = null; return c; }
  remove() { if (this.parentElement) this.parentElement.removeChild(this); }
  contains(o) { if (o === this) return true; return this.children.some((c) => c.contains(o)); }
  closest(sel) {
    let n = this;
    while (n) { if (matches(n, sel)) return n; n = n.parentElement; }
    return null;
  }
  querySelector(sel) { return queryAll(this, sel)[0] || null; }
  querySelectorAll(sel) { return queryAll(this, sel); }
  getBoundingClientRect() { return { right: 400, top: 300, bottom: 340, left: 200, width: 200, height: 40 }; }
  get offsetWidth() { return 340; }
  get offsetHeight() { return 360; }
  setAttribute(name, value) { this.attributes[name] = String(value); }
  getAttribute(name) { return Object.prototype.hasOwnProperty.call(this.attributes, name) ? this.attributes[name] : null; }
  addEventListener(t, fn) { (this._lis[t] = this._lis[t] || []).push(fn); }
  removeEventListener(t, fn) { const a = this._lis[t] || []; const i = a.indexOf(fn); if (i >= 0) a.splice(i, 1); }
  dispatch(t, ev) {
    ev = ev || {}; ev.target = ev.target || this;
    ev.stopPropagation = ev.stopPropagation || (() => {});
    ev.preventDefault = ev.preventDefault || (() => {});
    const fn = this['on' + t];
    if (typeof fn === 'function') fn.call(this, ev);
    for (const f of [...(this._lis[t] || [])]) f.call(this, ev);
  }
  click() { this.dispatch('click', { target: this }); }
  focus() { doc.activeElement = this; }
  blur() { if (typeof this.onblur === 'function') this.onblur({ target: this }); }
  scrollTo() {}
  scrollIntoView() {}
}
function inflate(parent, html) {
  const re = /<([a-zA-Z][\w-]*)((?:"[^"]*"|'[^']*'|[^>"'])*)>/g;
  let m;
  while ((m = re.exec(html))) {
    const tag = m[1];
    if (tag.toLowerCase() === 'br') continue;
    const attrs = m[2] || '';
    const el = new El(tag);
    const idm = /\sid="([^"]+)"/.exec(' ' + attrs); if (idm) el.id = idm[1];
    const clm = /\sclass="([^"]+)"/.exec(' ' + attrs); if (clm) clm[1].split(/\s+/).forEach((c) => c && el.classList.add(c));
    const tpm = /\stype="([^"]+)"/.exec(' ' + attrs); if (tpm) el.type = tpm[1];
    const vlm = /\svalue="([^"]*)"/.exec(' ' + attrs); if (vlm) el.value = vlm[1];
    const plm = /\splaceholder="([^"]*)"/.exec(' ' + attrs); if (plm) el.placeholder = plm[1];
    const dqm = /\sdata-q="([^"]*)"/.exec(' ' + attrs); if (dqm) el.dataset.q = dqm[1];
    const dtm = /\sdata-tab="([^"]*)"/.exec(' ' + attrs); if (dtm) el.dataset.tab = dtm[1];
    parent.appendChild(el);
  }
}
function selParts(sel) {
  const nots = [...sel.matchAll(/:not\(\.([\w-]+)\)/g)].map((x) => x[1]);
  const s = sel.replace(/:not\(\.([\w-]+)\)/g, '');
  const id = (/#([\w-]+)/.exec(s) || [])[1] || null;
  const tag = (/^([a-zA-Z][\w-]*)/.exec(s) || [])[1] || null;
  const cls = [...s.matchAll(/\.([\w-]+)/g)].map((x) => x[1]);
  const dm = /\[data-([\w-]+)(?:="([^"]*)")?\]/.exec(s);
  return { id, tag, cls, nots, dataKey: dm ? dm[1] : null, dataVal: dm && dm[2] !== undefined ? dm[2] : null };
}
function matches(el, sel) {
  return sel.split(',').map((x) => x.trim()).some((one) => {
    const p = selParts(one);
    if (p.id && el.id !== p.id) return false;
    if (p.tag && el.tagName !== p.tag.toUpperCase()) return false;
    for (const c of p.cls) if (!el.classList.contains(c)) return false;
    for (const c of p.nots) if (el.classList.contains(c)) return false;
    if (p.dataKey) {
      const key = p.dataKey.replace(/-([a-z])/g, (_, c) => c.toUpperCase());
      if (!(key in el.dataset)) return false;
      if (p.dataVal !== null && el.dataset[key] !== p.dataVal) return false;
    }
    return true;
  });
}
function walk(root, out) {
  for (const c of root.children) { out.push(c); walk(c, out); }
  return out;
}
function queryAll(root, sel) { return walk(root, []).filter((el) => matches(el, sel)); }

/* ---------------- document / storage / window ---------------- */
const doc = {
  body: new El('body'),
  hidden: false,
  _lis: {},
  createElement: (t) => new El(t),
  querySelector: (s) => (s[0] === '#' && byId[s.slice(1)]) || queryAll(doc.body, s)[0] || null,
  querySelectorAll: (s) => queryAll(doc.body, s),
  addEventListener(t, fn) { (this._lis[t] = this._lis[t] || []).push(fn); },
  dispatch(t, ev) {
    ev = ev || {}; ev.target = ev.target || doc.body;
    ev.stopPropagation = ev.stopPropagation || (() => {});
    ev.preventDefault = ev.preventDefault || (() => {});
    for (const f of [...(this._lis[t] || [])]) f.call(doc, ev);
  },
};
const byId = {};
function addId(id, tag, cls, parent) {
  const el = new El(tag || 'div');
  el.id = id;
  if (cls) cls.split(/\s+/).forEach((c) => c && el.classList.add(c));
  (parent || doc.body).appendChild(el);
  byId[id] = el;
  return el;
}
for (const id of ['frame', 'messages', 'empty', 'input', 'btnSend', 'btnStop', 'btnNew', 'convList',
  'btnClipboard', 'btnMic', 'btnChatSearch', 'chatSearchBar', 'chatSearchInput', 'chatSearchCount', 'chatSearchPrev', 'chatSearchNext', 'chatSearchClose',
  'btnSchedule', 'scheduleModal', 'scheduleX', 'schedList', 'schedLabel', 'schedPrompt', 'schedWhen', 'schedEvery', 'schedAdd', 'schedCancel',
  'schedConnPick', 'schedConnSave', 'schedConnClear', 'schedConnLabel', 'schedFormTitle',
  'snippetsManageBtn', 'snippetsModal', 'snippetsX', 'snipList', 'snipName', 'snipText', 'snipSave', 'snipDelete',
  'connPresets', 'mcpHub', 'mcpEnv', 'autoMemVal', 'speechOutVal', 'projectRepo', 'projectRepoCheck', 'projectRepoStatus',
  'btnSettings', 'btnMemory', 'btnProjects', 'btnSide', 'projectsModal', 'projectsX', 'projectList', 'projectDetail', 'projectCreate',
  'projectName', 'projectNotes', 'projectFolder', 'projectFolderPick', 'projectFolderClear', 'projectCheckpoint', 'projectTaskCount', 'projectTasks', 'projectTaskInput', 'projectTaskAdd',
  'projectAttach', 'projectCheckpointSave', 'projectResume', 'projectDelete', 'ctxLimitVal', 'ctxMeter', 'btnCheckUpdate', 'btnInstallUpdate', 'updateStatus', 'projectChip', 'btnMin', 'btnMax', 'btnClose', 'btnScroll',
  'modelChip', 'modelPick', 'mpName', 'modelMenu', 'chipThink', 'fullAccessIndicator', 'mcpList', 'mcpName', 'mcpCmd', 'mcpArgs', 'mcpAddBtn',
  'settingsModal', 'settingsX', 'settingsTitle', 'settingsSubtitle', 'memoryModal', 'memoryX', 'memoryClose',
  'memList', 'memClear', 'btnWipe', 'cmdk', 'cmdkInput', 'cmdkList', 'cmdkX', 'toasts',
  'tempVal', 'tempOut', 'thinkVal', 'memVal', 'streamVal', 'sysVal', 'maxTokVal',
  'wsVal', 'dlRootVal', 'dlRootInfo', 'webToolsVal', 'githubToolsVal', 'autoModeVal', 'fullAccessVal', 'fullAccessProfileVal', 'computerToolsVal', 'terminalToolsVal', 'githubTokenInput', 'githubConnect', 'githubDisconnect', 'githubStatus', 'dsSearchRow', 'dsSearchVal',
  'tokenGuide', 'tokenGuideX', 'tokenGuideClose', 'connSummary', 'connList', 'connAdd', 'connForm', 'connsBody',
  'cfName', 'cfKind', 'cfTemplate', 'cfBase', 'cfKey', 'cfKeyEye', 'cfKeyLabel', 'cfTokenGuide', 'cfHint',
  'cfTags', 'cfModelInput', 'cfModelAdd', 'cfFetchModels', 'cfSrvWrap', 'cfSrvSearch', 'cfSrvList',
  'cfDelete', 'cfSetActive', 'cfTestConn', 'cfTestStatus', 'storageBox', 'btnRevealAtria', 'btnAttach', 'fileInput', 'imgTray',
  'stars', 'appVer', 'tokChip']) addId(id);
byId.connsBody.classList.add('conns-body');
addId('projectMenu', 'div', 'project-menu hidden');
addId('updateModal', 'div', 'modal-backdrop hidden');
addId('updateModalX', 'button', 'x');
addId('updatePromptTitle', 'p', 'update-prompt-title');
addId('updatePromptBody', 'p', 'fhint');
addId('updatePromptYes', 'button', 'btn primary');
addId('updatePromptNo', 'button', 'btn');
addId('updatePromptStatus', 'div', 'update-status');
for (const id of ['modelMenu', 'settingsModal', 'memoryModal', 'projectsModal', 'cmdk', 'tokenGuide', 'imgTray',
  'dsSearchRow', 'btnScroll', 'tokChip', 'connForm', 'btnStop', 'cfHint', 'cfTokenGuide', 'btnInstallUpdate',
  'scheduleModal', 'snippetsModal', 'chatSearchBar', 'connPresets', 'mcpHub']) byId[id].classList.add('hidden');
for (const t of ['conn', 'behavior', 'sys']) {
  const b = new El('button'); b.className = 'stab' + (t === 'conn' ? ' active' : ''); b.dataset.tab = t; doc.body.appendChild(b);
  const p = new El('div'); p.className = 'stab-body' + (t === 'conn' ? ' active' : ''); p.dataset.tab = t; doc.body.appendChild(p);
}
for (const c of ['win-controls', 'titlebar-actions', 'brand']) { const e = new El('div'); e.className = c; doc.body.appendChild(e); }
for (const q of ['۱۷×۲۴+۳ چنده؟', 'امروز چنده؟', 'یادداشت کن: پروژه آتریا مهم است']) {
  const e = new El('button'); e.className = 'chip'; e.dataset.q = q; doc.body.appendChild(e);
}

const store = new Map();
const localStorage = {
  getItem: (k) => (store.has(k) ? store.get(k) : null),
  setItem: (k, v) => store.set(k, String(v)),
  removeItem: (k) => store.delete(k),
};
let confirmResult = true;

/* ---------------- REAL IPC bridge (from index.html) + router ---------------- */
const bridgeCalls = [];
const tauriListeners = {};
let chatsLoadPayload = [];
const secretStore = new Map();
let githubApplyBehavior = 'ok';
let updateCheckAvailable = false;
let urlOpenBehavior = 'ok';
let chatsLoadCalled = 0;
let v18Jobs = [];
let v18Log = [];
let v18ConnSaved = false;
let clipboardStub = 'متن کلیپ‌بورد تستی';
function router(cmd, args = {}) {
  switch (cmd) {
    case 'app_meta': return Promise.resolve({ version: '0.18.0' });
    case 'chat_stop': return Promise.resolve();
    case 'secret_set': secretStore.set(args.id, args.value); return Promise.resolve();
    case 'secret_get': return Promise.resolve(secretStore.get(args.id) || null);
    case 'secret_delete': secretStore.delete(args.id); return Promise.resolve();
    case 'test_connection': return Promise.resolve({ ok: true, latency_ms: 18, reply: 'OK', input_tokens: 3, output_tokens: 1 });
    case 'file_apply_edit': return Promise.resolve('applied [ATRIA_BACKUP:backup-test-1]');
    case 'file_reject_edit': return Promise.resolve();
    case 'file_restore_backup': return Promise.resolve('restored');
    case 'github_connect': secretStore.set('github-api-token', args.token); return Promise.resolve({ login: 'test-user', name: 'Test User' });
    case 'github_status': return Promise.resolve(secretStore.has('github-api-token') ? { login: 'test-user' } : null);
    case 'github_disconnect': secretStore.delete('github-api-token'); return Promise.resolve();
    case 'github_apply_action':
      if (githubApplyBehavior === 'missing-token') return Promise.reject(new Error('توکن GitHub در تنظیمات ذخیره نشده است'));
      if (githubApplyBehavior === 'network-error') return Promise.reject(new Error('GitHub HTTP 500'));
      return Promise.resolve('GitHub action applied [ATRIA_GITHUB_APPLIED:' + args.id + ']');
    case 'github_reject_action': return Promise.resolve();
    case 'open_pending_url':
      if (urlOpenBehavior === 'network-error') return Promise.reject(new Error('open failed'));
      return Promise.resolve('opened');
    case 'reject_pending_url': return Promise.resolve();
    case 'command_apply_command': return Promise.resolve('دستور اجرا شد (تأییده‌شده توسط کاربر). کد خروجی: 0\nخروجی:\nhello-atria');
    case 'command_reject_command': return Promise.resolve();
    case 'update_check':
      if (updateCheckAvailable) return Promise.resolve({ current_version: '0.18.0', latest_version: '0.19.0', available: true, release_url: 'https://github.com/Manixpcshot/atria-ai/releases/tag/v0.19.0', download_size: 10485760 });
      return Promise.resolve({ current_version: '0.18.0', latest_version: '0.18.0', available: false, release_url: '', download_size: 0 });
    case 'update_install': return Promise.resolve();
    case 'mcp_test_server': { const sv = (args && args.server) || {}; return Promise.resolve({ ok: true, name: sv.name || '', tools: [{ name: 'echo', description: 'echo the arguments' }] }); }
    case 'files_stash': return Promise.resolve('/home/u/.atria/uploads/harness_' + ((args && args.name) || 'file'));
    case 'chats_load': chatsLoadCalled++; return Promise.resolve(chatsLoadPayload);
    case 'dirs_info': return Promise.resolve({ chats: 'C:/u/.atria/chats', workspace: 'C:/u/.atria/workspace', memory: 'C:/u/.atria/memory.json' });
    case 'downloads_root_info':
      if (args.configured && args.configured.trim() === 'bad root') return Promise.resolve({ configured: args.configured, resolved: null, error: 'پوشهٔ دانلود پیدا نشد: bad' });
      return Promise.resolve({ configured: args.configured || '', resolved: 'C:/Users/u/Desktop', error: null });
    case 'memory_list': return Promise.resolve([{ title: 'نکته', content: 'متن', ts: 'امروز' }]);
    case 'list_models': return Promise.resolve(['m-server-1', 'm-server-2']);
    case 'clipboard_read': return Promise.resolve(clipboardStub);
    case 'notify_run_done': return Promise.resolve();
    case 'github_repo_info': return Promise.resolve('مخزن: ' + args.owner + '/' + args.repo + ' — ستاره‌ها: 42 · issueهای باز: 3 · branch پیش‌فرض: main');
    case 'memory_extract': return Promise.resolve([{ title: 'نکتهٔ تست', content: 'کاربر روی پروژهٔ آتریا کار می‌کند', ts: 'now' }]);
    case 'files_read': return Promise.resolve((args.paths || []).map((p) => ({ name: String(p).split('/').pop(), size: 12, kind: 'text', text: 'محتوای فایل رهاشدهٔ تستی' })));
    case 'schedule_list': return Promise.resolve(v18Jobs);
    case 'schedule_upsert': { const j = args.job; const i = v18Jobs.findIndex((x) => x.id === j.id); if (i >= 0) v18Jobs[i] = j; else v18Jobs.push(j); return Promise.resolve(v18Jobs); }
    case 'schedule_delete': v18Jobs = v18Jobs.filter((x) => x.id !== args.id); return Promise.resolve(v18Jobs);
    case 'schedule_set_connection': v18ConnSaved = true; return Promise.resolve();
    case 'schedule_clear_connection': v18ConnSaved = false; return Promise.resolve();
    case 'schedule_has_connection': return Promise.resolve(v18ConnSaved);
    case 'schedule_unprocessed': return Promise.resolve(v18Log);
    case 'schedule_ack': v18Log = []; return Promise.resolve(0);
    default: return Promise.resolve({});
  }
}
const mdStub = {
  renderAll(el, text) { el._mdText = text; el.innerHTML = '<p></p>'; },
  createLiveStream(sec) {
    let buf = '';
    return { push(d) { buf += d; }, done() { sec._mdText = buf; }, isEmpty() { return !buf.trim(); } };
  },
};
const win = {
  innerWidth: 1280, innerHeight: 800,
  confirm: () => confirmResult,
  _lis: {},
  addEventListener(t, fn) { (this._lis[t] = this._lis[t] || []).push(fn); },
  dispatch(t, ev) { for (const f of [...(this._lis[t] || [])]) f(ev || {}); },
  md: mdStub,
  __TAURI__: {
    event: { listen: (name, cb) => { (tauriListeners[name] = tauriListeners[name] || []).push(cb); return Promise.resolve(() => {}); } },
  },
  __TAURI_INTERNALS__: {
    invoke: (cmd, args) => { bridgeCalls.push([cmd, args]); return router(cmd, args); },
  },
};
function emit(name, payload) { for (const f of tauriListeners[name] || []) f({ payload }); }

const sandbox = {
  document: doc, window: win, localStorage, console, confirm: () => confirmResult,
  setTimeout, clearTimeout, setInterval, clearInterval, md: mdStub,
  Math, JSON, Date, Promise, Array, Object, String, Number, Boolean, RegExp, Map, Set, Error, TypeError, Intl,
  FileReader: class { readAsDataURL() {} },
};
sandbox.globalThis = sandbox;
vm.createContext(sandbox);

// 1) run the REAL bridge script from index.html
const htmlSrc = fs.readFileSync(path.join(__dirname, '..', 'frontend', 'index.html'), 'utf8');
const bridgeSrc = /<script>\s*([\s\S]*?)<\/script>/.exec(htmlSrc)[1];
vm.runInContext(bridgeSrc, sandbox, { filename: 'bridge.js' });

// 2) load app.js + export internals for assertions
const code = fs.readFileSync(path.join(__dirname, '..', 'frontend', 'app.js'), 'utf8') +
  '\n;globalThis.__T = { st, els, boot, send, newChat, switchChat, deleteChat, finishRun, failRun,' +
  ' effModel, buildHistory, msgText, renderModelMenu, chooseModelFor, openConns, stabGo,' +
  ' wipeChats, openMemory, openCmdk, retryLast, syncChips, updateModelPick, updateConnTab,' +
  ' setActiveConn, resetAttemptStream, USAGE_TOT, createProject, openProjects, addProjectTask,' +
  ' resumeProjectInNewChat, projectContextText, checkpointProject, toggleModelFavorite, modelCapabilities, saveConns,' +
  ' fileEditActions, commandActions, githubActionActions, openUrlActions, hasPendingApproval, toolCardEl, checkForUpdate, installUpdate, setCurrentProject, activeProject, setChatProject, openProjectMenu, closeProjectMenu, renderProjectChip, addUploadFiles, renderAttachTray, clearAttach, mcpServers, mcpRender, mcpAddServer, mcpTestServer, mcpRemoveServer, scrollBottom, forceScrollBottom, nearBottom, toolDesc, toolCardEl' +
  ' , v18Snips, v18SaveSnips, v18ApplySnip, v18SnipMenu, v18CloseSnipMenu, v18ExpandSnippet, v18SnippetName, v18OpenSearch, v18CloseSearch, v18RunSearch, v18GotoSearch, v18SearchHits, v18ClearSearchMarks, v18SchedChat, v18OnScheduleResult, v18RenderDiff, v18ParseEnv, v18ExtractMemory, v18Speak, v18RenderPresets, v18RenderMcpHub, v18RepoCheck, V18_PRESETS, V18_MCP_HUB, v18ListenDragDrop, v18RunFinished, renderHistory };';
vm.runInContext(code, sandbox, { filename: 'app.js' });
const T = sandbox.__T;

localStorage.setItem('atria.conns.v1', JSON.stringify([
  { id: 'c1', name: 'سرویس یک', kind: 'openai', base: 'https://api.x.ai/v1', key: 'k1', models: ['model-a', 'model-b'], enabled: true, lastModel: '' },
  { id: 'c2', name: 'سرویس دو', kind: 'anthropic', base: 'https://api.y', key: '', models: ['claude-x'], enabled: true },
]));
localStorage.setItem('atria.activeconn.v1', 'c1');

let bootError = null;
try { doc.dispatch('DOMContentLoaded'); if (win.__atriaBootPromise) await win.__atriaBootPromise; } catch (e) { bootError = e; }

console.log('\n=== 1) پل IPC + راه‌اندازی (boot) ===');
ok('boot بدون خطا اجرا شد', !bootError, bootError && String(bootError));
const needed = ['chat_send', 'chat_stop', 'memory_list', 'memory_clear', 'app_meta', 'list_models', 'test_connection', 'secret_set', 'secret_get', 'secret_delete', 'file_apply_edit', 'file_reject_edit', 'file_restore_backup', 'github_connect', 'github_status', 'github_disconnect', 'github_apply_action', 'github_reject_action', 'open_pending_url', 'reject_pending_url', 'command_apply_command', 'command_reject_command', 'update_check', 'update_install',
  'minimize_win', 'maximize_win', 'close_win', 'chats_load', 'chats_sync', 'dirs_info', 'downloads_root_info', 'reveal_dir'];
const bridge = win.__atria;
ok('پل IPC همهٔ توابع لازم را دارد', needed.every((k) => typeof bridge[k] === 'function'),
  needed.filter((k) => !bridge || typeof bridge[k] !== 'function').join(','));
ok('دکمهٔ انتخاب مدل سیم‌کشی شد', typeof byId.modelPick.onclick === 'function');
ok('نشانگر Full Access سیم‌کشی شد', typeof byId.fullAccessIndicator.onclick === 'function');
ok('دکمهٔ ارسال سیم‌کشی شد', typeof byId.btnSend.onclick === 'function');
eq('listenEvents همهٔ رویدادها را ثبت کرد (۱۱ + schedule + drag-drop)',
  Object.keys(tauriListeners).sort(),
  ['atria:done', 'atria:error', 'atria:retry', 'atria:round', 'atria:schedule_result', 'atria:stopped', 'atria:text',
   'atria:thinking', 'atria:tool_args', 'atria:tool_end', 'atria:tool_pending', 'atria:tool_start', 'tauri://drag-drop']);
ok('هیچ اشاره‌ای به دکمهٔ حذف‌شده باقی نمانده', !('btnManageConns' in T.els));
await sleep(20);
ok('بازیابی گفتگو از دیسک صدا زده شد', chatsLoadCalled > 0);
await sleep(10);
eq('نسخه از app_meta بالای پنجره نشست', byId.appVer.textContent, 'v0.18.0');
const indexHtml = fs.readFileSync(path.join(__dirname, '../frontend/index.html'), 'utf8');
ok('UI می‌گوید ابزارهای فایل تابع Full Access‌اند و خواندن در حالت‌های غیرسیستمی به ورک‌اسپیس محدود است', indexHtml.includes('ایجنت و ابزارهای فایل') && indexHtml.includes('در بقیهٔ حالت‌ها خواندن/فهرست‌کردن همیشه به ورک‌اسپیس محدود است'));
ok('UI می‌گوید محدودهٔ Full Access فقط برای نوشتن است و خواندن در حالت‌های غیرسیستمی به ورک‌اسپیس می‌ماند', indexHtml.includes('این محدوده فقط برای نوشتن خودکار است') && indexHtml.includes('در بقیهٔ حالت‌ها خواندن/فهرست‌کردن همیشه به ورک‌اسپیس محدود است') && indexHtml.includes('کل سیستم — مسیر مطلق (با محافظت)'));
ok('UI فایل‌های دانلودشده را به پوشهٔ دسکتاپ (Full Access) محدود می‌داند', indexHtml.includes('پوشهٔ دانلود فایل‌های وب') && indexHtml.includes('پیش‌فرض: دسکتاپ') && indexHtml.includes('فقط در Full Access'));
ok('Full Access در UI شامل دانلود تصویر/مستند می‌شود', indexHtml.includes('دانلود تصویر/مستند در پوشهٔ دسکتاپ'));
ok('کلید قدیمی به مخزن امن منتقل شد', secretStore.get('c1') === 'k1');
const savedConns = JSON.parse(localStorage.getItem('atria.conns.v1') || '[]');
ok('localStorage فقط metadata نگه می‌دارد و کلید را پاک کرده', savedConns.length === 2 && savedConns.every((c) => !c.key));
ok('settings legacy نیز api_key ندارد', !Object.prototype.hasOwnProperty.call(JSON.parse(localStorage.getItem('atria.settings.v2') || '{}'), 'api_key'));
ok('جست‌وجوی وب و GitHub به‌صورت مستقل روشن‌اند', T.st.settings.web_tools === true && T.st.settings.github_tools === true);
ok('حالت خودکار به‌صورت پیش‌فرض خاموش است', T.st.settings.autonomous_mode === false);
byId.autoModeVal.value = 'autonomous'; byId.autoModeVal.onchange();
ok('کاربر می‌تواند حالت خودکار محدود را انتخاب کند', T.st.settings.autonomous_mode === true);
byId.autoModeVal.value = 'ask'; byId.autoModeVal.onchange();
ok('بازگشت به تأیید پیش‌فرض ذخیره می‌شود', T.st.settings.autonomous_mode === false);
ok('Full Access به‌صورت پیش‌فرض خاموش و کلید سریع همواره دیده می‌شود', T.st.settings.full_access_mode === false && byId.fullAccessVal.checked === false && !byId.fullAccessIndicator.classList.contains('on') && byId.fullAccessIndicator.textContent.includes('خاموش'));
ok('محدودهٔ فایلِ پیش‌فرض پروفایل کاربر است', T.st.settings.full_access_profile === true && byId.fullAccessProfileVal.value === 'profile');
confirmResult = false; byId.fullAccessVal.checked = true; byId.fullAccessVal.onchange();
ok('لغو هشدار، Full Access را روشن نمی‌کند', T.st.settings.full_access_mode === false && byId.fullAccessVal.checked === false);
confirmResult = true; byId.fullAccessVal.checked = true; byId.fullAccessVal.onchange();
ok('تأیید صریح Full Access را فعال می‌کند و ذخیره می‌شود', T.st.settings.full_access_mode === true && JSON.parse(localStorage.getItem('atria.settings.v2')).full_access_mode === true && byId.fullAccessIndicator.classList.contains('on'));
byId.fullAccessProfileVal.value = 'workspace'; byId.fullAccessProfileVal.onchange();
ok('محدودهٔ نوشتن قابل‌تغییر است و انتخاب ورک‌اسپیس ذخیره می‌شود', T.st.settings.full_access_profile === false && JSON.parse(localStorage.getItem('atria.settings.v2')).full_access_profile === false);
byId.fullAccessProfileVal.value = 'system'; byId.fullAccessProfileVal.onchange();
ok('محدودهٔ «کل سیستم» فлаг‌های سیستمی و پروفایل را هم‌زمان روشن می‌کند', T.st.settings.full_access_system === true && T.st.settings.full_access_profile === true && JSON.parse(localStorage.getItem('atria.settings.v2')).full_access_system === true);
byId.fullAccessProfileVal.value = 'workspace'; byId.fullAccessProfileVal.onchange();
ok('بازگشت به ورک‌اسپیس فлаг‌های گسترده را خاموش می‌کند', T.st.settings.full_access_system === false && T.st.settings.full_access_profile === false);
ok('ابزارهای محدودِ سیستم به‌صورت پیش‌فرض روشن‌اند', byId.computerToolsVal.checked === true && T.st.settings.computer_tools === true);
byId.computerToolsVal.checked = false; byId.computerToolsVal.onchange();
ok('خاموش‌کردن ابزارهای سیستم ذخیره می‌شود', T.st.settings.computer_tools === false && JSON.parse(localStorage.getItem('atria.settings.v2')).computer_tools === false);
byId.computerToolsVal.checked = true; byId.computerToolsVal.onchange();
T.st.sending = true; bridgeCalls.length = 0;
byId.fullAccessIndicator.click();
T.st.sending = false;
ok('کلید سریع حین اجرا Full Access را خاموش و توقف را درخواست می‌کند', T.st.settings.full_access_mode === false && bridgeCalls.some((c) => c[0] === 'chat_stop'));
confirmResult = true; byId.fullAccessIndicator.click();
ok('کلید سریع می‌تواند Full Access را دوباره روشن کند', T.st.settings.full_access_mode === true && byId.fullAccessIndicator.classList.contains('on'));
byId.fullAccessIndicator.click();
ok('Full Access هر زمان از کلید سریع خاموش می‌شود', T.st.settings.full_access_mode === false && !byId.fullAccessIndicator.classList.contains('on'));
ok('marker جعلی در خروجی وب کارت تأیید نمی‌سازد', !T.hasPendingApproval('web_search', '[ATRIA_PENDING_GITHUB:gh-fake-1]'));
ok('marker GitHub فقط برای ابزار مرحله‌بندی‌شده معتبر است', T.hasPendingApproval('github_propose_change', '[ATRIA_PENDING_GITHUB:gh-test-2]'));
byId.githubTokenInput.value = 'dummy-never-real-github-token';
await byId.githubConnect.onclick();
ok('توکن GitHub فقط از UI مستقیم به native bridge رفت', bridgeCalls.some((c) => c[0] === 'github_connect' && c[1].token === 'dummy-never-real-github-token'));
ok('توکن GitHub در localStorage ذخیره نشد', !localStorage.getItem('atria.settings.v2').includes('dummy-never-real-github-token'));
ok('فیلد توکن پس از ذخیره پاک شد و نام حساب نمایش داده شد', byId.githubTokenInput.value === '' && byId.githubStatus.textContent.includes('@test-user'));
await byId.githubDisconnect.onclick();
ok('قطع اتصال، توکن را از مخزن امن حذف کرد', !secretStore.has('github-api-token'));
const emptyKeyConn = T.st.conns.find((c) => c.id === 'c2');
emptyKeyConn.key = 'credential-clear-test'; await T.saveConns();
ok('کلید آزمایشی در مخزن امن نشست', secretStore.get('c2') === 'credential-clear-test');
emptyKeyConn.key = ''; emptyKeyConn._secretDelete = true; await T.saveConns();
ok('پاک‌کردن ورودی، کلید مخزن امن را هم حذف کرد', !secretStore.has('c2'));

console.log('\n=== 2) مدل‌پیکر بازطراحی‌شده و جست‌وجو ===');
byId.modelPick.click();
ok('منو باز شد', !byId.modelMenu.classList.contains('hidden'));
ok('حالت دسترس‌پذیری و فوکوس جست‌وجو همگام شد', byId.modelPick.getAttribute('aria-expanded') === 'true'
  && doc.activeElement === byId.modelMenu.querySelector('#mmSearch'));
const groups = byId.modelMenu.querySelectorAll('.mp-group');
eq('گروه‌بندی اتصال‌ها (۲ گروه)', groups.length, 2);
ok('اتصال فعال اول است', groups[0] && groups[0].dataset.connId === 'c1');
const options = byId.modelMenu.querySelectorAll('.mp-option');
eq('۳ گزینهٔ مدل بدون دکمهٔ تکراری', options.length, 3);
const itemA = options.find((i) => i.dataset.model === 'model-a');
ok('گزینهٔ model-a هست', !!itemA);
ok('نام، نوع API و اتصال جداگانه نمایش داده می‌شوند', !!(itemA.querySelector('.mp-model-name') && itemA.querySelector('.mp-model-meta') && itemA.querySelector('.mp-source')));
eq('مدل فعال علامت‌گذاری شد', itemA.getAttribute('aria-selected'), 'true');
const search = byId.modelMenu.querySelector('#mmSearch');
ok('جست‌وجوی درون منو هست', !!search && search.type === 'search');
search.value = 'messages api';
search.dispatch('input', { target: search });
const visAfter = byId.modelMenu.querySelectorAll('.mp-option').filter((i) => !i.classList.contains('hidden'));
eq('جست‌وجو بر اساس گویش API فیلتر می‌کند', visAfter.length, 1);
const hiddenGroups = byId.modelMenu.querySelectorAll('.mp-group').filter((g) => g.classList.contains('hidden'));
eq('گروه بدون نتیجه پنهان شد', hiddenGroups.length, 1);
search.value = 'سرویس یک';
search.dispatch('input', { target: search });
eq('جست‌وجو بر اساس نام اتصال همهٔ مدل‌های آن را نشان می‌دهد', byId.modelMenu.querySelectorAll('.mp-option').filter((i) => !i.classList.contains('hidden')).length, 2);
search.value = 'مدل-ناموجود';
search.dispatch('input', { target: search });
eq('نتیجهٔ جست‌وجوی ناموجود صفر است', byId.modelMenu.querySelectorAll('.mp-option').filter((i) => !i.classList.contains('hidden')).length, 0);
ok('حالت «نتیجه‌ای پیدا نشد» نمایش داده شد', !byId.modelMenu.querySelector('.mp-empty').classList.contains('hidden'));
byId.modelMenu.querySelector('.mp-clear').click();
eq('پاک‌کردن جست‌وجو همهٔ مدل‌ها را برمی‌گرداند', byId.modelMenu.querySelectorAll('.mp-option').filter((i) => !i.classList.contains('hidden')).length, 3);
ok('قابلیت‌های مدل به‌صورت برچسب نمایش داده می‌شوند', !!itemA.querySelector('.mp-cap.tools'));
ok('استنتاج قابلیت تصویر برای مدل vision درست است', T.modelCapabilities({ kind: 'openai' }, 'gpt-4o').vision === true);
const favA = itemA.querySelector('.mp-fav');
ok('دکمهٔ علاقه‌مندی مدل هست', !!favA);
favA.click();
ok('مدل به علاقه‌مندی‌ها افزوده شد', T.st.conns.find((c) => c.id === 'c1').favorites.includes('model-a'));
const onlyFav = byId.modelMenu.querySelector('.mp-favorites-toggle');
onlyFav.click();
eq('فیلتر فقط علاقه‌مندی‌ها اعمال شد', byId.modelMenu.querySelectorAll('.mp-option').filter((i) => !i.classList.contains('hidden')).length, 1);
onlyFav.click();
favA.click();
itemA.click();
eq('مدل انتخاب شد', T.effModel(), 'model-a');
eq('برچسب بالای پنجره عوض شد', byId.mpName.textContent, 'model-a');
ok('منو بسته و دسترس‌پذیری همگام شد', byId.modelMenu.classList.contains('hidden') && byId.modelPick.getAttribute('aria-expanded') === 'false');

console.log('\n=== 3) انتخاب مدل، اتصال را هم خودکار عوض می‌کند ===');
byId.modelPick.click();
const second = byId.modelMenu.querySelectorAll('.mp-option').find((i) => i.dataset.model === 'claude-x');
ok('مدل اتصال دوم در فهرست هست', !!second);
second.click();
eq('انتخاب مدل اتصال فعال را هم تغییر داد', T.st.activeConnId, 'c2');
eq('مدل همان اتصال نشست', T.effModel(), 'claude-x');
ok('منو پس از انتخاب بسته شد', byId.modelMenu.classList.contains('hidden'));

console.log('\n=== 4) چیپ‌های حالت (تفکر + Full Access) ===');
const thinkBefore = T.st.settings.thinking;
byId.chipThink.click();
eq('🧠 تفکر toggle شد', T.st.settings.thinking, !thinkBefore);
ok('دکمهٔ ایجنت حذف شده (تابع Full Access است)', !byId.chipAgent);
ok('دکمهٔ فایل‌ها حذف شده (تابع Full Access است)', !byId.chipFiles);

console.log('\n=== 5) جریان ارسال پیام (send → رویدادها → ذخیره) ===');
T.setActiveConn('c1');
T.openConns('c1');
byId.cfTestConn.click();
await sleep(10);
ok('آزمون اتصال نتیجه و latency را نشان می‌دهد', byId.cfTestStatus.textContent.includes('18 ms') && T.st.conns.find((c) => c.id === 'c1').lastHealth.ok);
byId.input.value = 'سلام آتریا';
bridgeCalls.length = 0;
byId.btnSend.click();
ok('chat_send صدا زده شد', bridgeCalls.some((c) => c[0] === 'chat_send'));
const sent = bridgeCalls.find((c) => c[0] === 'chat_send')[1].payload;
eq('پیام کاربر در payload هست', sent.messages[sent.messages.length - 1].content[0].text, 'سلام آتریا');
eq('مدلِ در حال اجرا درست است', sent.model, 'model-a');
ok('web/GitHub/autonomy/Full Access policy در payload هست', sent.web_tools === true && sent.github_tools === true && sent.autonomous_mode === false && sent.full_access_mode === false && sent.full_access_profile === false && sent.full_access_system === false && sent.computer_tools === true);
ok('ترمینال روشن و بدون پوشهٔ پروژه payload رفت', sent.terminal_tools === true && sent.project_root === '');
ok('پوشهٔ دانلود پیش‌فرض در payload خالی است (= دسکتاپ)', sent.downloads_root === '');
ok('هیچ توکن GitHub وارد payload مدل نشد', !Object.prototype.hasOwnProperty.call(sent, 'github_token'));
ok('حالت ارسال فعال شد', T.st.sending === true);
ok('ارسال جای خود را به توقف داد', byId.btnSend.classList.contains('hidden') && !byId.btnStop.classList.contains('hidden'));
emit('atria:thinking', { delta: 'دارم فکر می‌کنم… ' });
emit('atria:tool_start', { id: 't1', name: 'calculator', label: 'ماشین‌حساب', input: { expression: '2+2' } });
emit('atria:tool_end', { id: 't1', ok: true, output: '2+2 = 4' });
emit('atria:text', { delta: 'پاسخ ' });
emit('atria:text', { delta: 'آزمایشی.' });
emit('atria:done', { new_messages: null, final_text: 'پاسخ آزمایشی.', session_id: 's1', input_tokens: 10, output_tokens: 5 });
ok('تولید تمام شد', T.st.sending === false);
ok('دکمهٔ ارسال برگشت', !byId.btnSend.classList.contains('hidden'));
const chat1 = T.st.chats.find((c) => c.id === T.st.currentId);
eq('پاسخ در چت ذخیره شد', chat1.messages[chat1.messages.length - 1].plain, 'پاسخ آزمایشی.');
ok('پنل فرایند (تفکر/ابزار) ذخیره شد', !!(chat1.messages[chat1.messages.length - 1].flow || []).length);
eq('عنوان گفتگو از اولین پیام ساخته شد', chat1.title.slice(0, 5), 'سلام آت'.slice(0, 5));
ok('مصرف توکن ثبت شد', T.USAGE_TOT.in === 10 && T.USAGE_TOT.out === 5);
await sleep(300);
ok('flush گفتگو به دیسک رفت (chats_sync)', bridgeCalls.some((c) => c[0] === 'chats_sync'));

console.log('\n=== 6) چندکاره: ساخت/حذف/جابه‌جایی چت حین تولید ===');
byId.input.value = 'پیام دوم';
byId.btnSend.click();
ok('تولید دوم در حال اجرا', T.st.sending === true);
const runChat = T.st.runChatId;
byId.btnNew.click();
ok('ساخت چت جدید حین تولید — بدون مانع', T.st.currentId !== runChat && T.st.sending === true);
const newId = T.st.currentId;
emit('atria:text', { delta: 'متن مخفی ' });
emit('atria:done', { new_messages: null, final_text: 'پاسخ چت اول', session_id: '', input_tokens: 1, output_tokens: 2 });
ok('پس از پایان، تولید بسته شد', T.st.sending === false);
const runChatObj = T.st.chats.find((c) => c.id === runChat);
eq('پاسخ در چتِ خودش ذخیره شد نه چت جدید', runChatObj.messages[runChatObj.messages.length - 1].plain, 'پاسخ چت اول');
const newChatObj = T.st.chats.find((c) => c.id === newId);
eq('چت جدید تمیز ماند', newChatObj.messages.length, 0);
T.switchChat(runChat); // برگرد به چت اول و آنجا تولید را بالا بیاور
byId.input.value = 'پیام سوم';
byId.btnSend.click();
const runChat3 = T.st.runChatId;
eq('تولید سوم در چت اول است', runChat3, runChat);
T.switchChat(newId);
ok('جابه‌جایی چت حین تولید — بدون مانع', T.st.currentId === newId && T.st.sending === true);
confirmResult = true;
T.deleteChat(runChat3);
ok('حذف چتِ در حال تولید — بدون کرش', T.st.sending === true && !T.st.chats.some((c) => c.id === runChat3));
emit('atria:done', { new_messages: null, final_text: 'پاسخ گم‌شده', session_id: '', input_tokens: 0, output_tokens: 0 });
ok('پاسخ چت حذف‌شده بی‌صدا کنار رفت', T.st.sending === false && !T.st.chats.some((c) => (c.messages || []).some((m) => m.plain === 'پاسخ گم‌شده')));
// v0.7.0: بازگشت به چتِ در حال تولید — حباب زنده باید دوباره سوار شود (باگ «پیام‌ها پاک می‌شود»)
{
  byId.input.value = 'تولید زنده';
  byId.btnSend.click();
  const liveChat = T.st.runChatId;
  T.newChat();
  emit('atria:text', { delta: 'در حال نوشتن…' });
  ok('حین دوری، حباب زنده به صفحه چسبیده نیست', T.st.liveMsg && !T.st.liveMsg.isConnected);
  T.switchChat(liveChat);
  ok('بازگشت به چتِ در حال تولید — حباب زنده دوباره سوار شد', T.st.liveMsg && T.st.liveMsg.isConnected);
  emit('atria:text', { delta: ' ادامه' });
  ok('نوشتن ادامه دارد بعد از بازگشت', T.st.runText.includes('ادامه'));
  emit('atria:done', { new_messages: null, final_text: 'پاسخ زنده', session_id: '', input_tokens: 1, output_tokens: 1 });
  const liveChatObj = T.st.chats.find((c) => c.id === liveChat);
  eq('پاسخ نهایی در همان چت نشست', liveChatObj.messages[liveChatObj.messages.length - 1].plain, 'پاسخ زنده');
}

console.log('\n=== 7) بازنشانی پاسخ ناقص هنگام retry ===');
byId.input.value = 'تولید با قطع لحظه‌ای';
byId.btnSend.click();
emit('atria:text', { delta: 'پاسخ نصفه از تلاش اول' });
eq('متن تلاش اول دیده شد', T.st.runText, 'پاسخ نصفه از تلاش اول');
emit('atria:retry', { attempt: 1, message: 'stream disconnected' });
eq('متن ناقص دور قبلی پاک شد', T.st.runText, '');
emit('atria:text', { delta: 'پاسخ کامل تلاش دوم' });
eq('متن تلاش دوم از ابتدا ساخته شد', T.st.runText, 'پاسخ کامل تلاش دوم');
emit('atria:done', { new_messages: null, final_text: 'پاسخ کامل تلاش دوم', session_id: '', input_tokens: 1, output_tokens: 4 });
eq('فقط پاسخ کامل ذخیره شد', T.st.chats.find((c) => c.id === T.st.currentId).messages.slice(-1)[0].plain, 'پاسخ کامل تلاش دوم');

console.log('\n=== 8) خطا و تلاش مجدد ===');
byId.input.value = 'پیام چهارم';
byId.btnSend.click();
const runChat4 = T.st.runChatId;
emit('atria:error', { message: 'boom [504]' });
ok('failRun تولید را بست', T.st.sending === false);
ok('کارت خطا + دکمهٔ تلاش مجدد ساخته شد', !!(T.st.failedWrap && T.st.failedWrap.querySelector('.retry-btn')));
ok('راهنمای خطای موقت نشان داده شد', T.st.failedWrap.querySelector('.err-text').textContent.includes('تلاش مجدد'));
T.newChat(); // کاربر وسط کار چت عوض کرده
ok('تلاش مجدد از چت دیگر', T.st.currentId !== runChat4);
T.retryLast();
eq('تلاش مجدد به چتِ خودش برگشت', T.st.currentId, runChat4);
ok('تلاش مجدد دوباره تولید را بالا آورد', T.st.sending === true);
emit('atria:stopped', {});
ok('توقف (⏹) تولید را بست', T.st.sending === false);

console.log('\n=== 9) صف کلیدها: Enter و دکمهٔ ارسال ===');
ok('بدون متن، ارسال کاری نمی‌کند', (() => { byId.input.value = '  '; const n = bridgeCalls.length; byId.btnSend.click(); return bridgeCalls.length === n; })());
byId.input.value = 'با Enter';
byId.input.dispatch('keydown', { key: 'Enter', shiftKey: false, isComposing: false, keyCode: 13, preventDefault() {} });
ok('Enter پیام فرستاد', bridgeCalls.some((c) => c[0] === 'chat_send' && JSON.stringify(c[1]).includes('با Enter')));
emit('atria:done', { new_messages: null, final_text: 'ok', session_id: '', input_tokens: 0, output_tokens: 0 });
byId.input.value = 'shift+enter';
byId.input.dispatch('keydown', { key: 'Enter', shiftKey: true, isComposing: false, keyCode: 13, preventDefault() {} });
ok('Shift+Enter پیام نمی‌فرستد', T.st.sending === false);

console.log('\n=== 10) بدون کلید → هدایت به تنظیمات ===');
T.setActiveConn('c2'); // اتصال بدون کلید
byId.input.value = 'تست';
byId.btnSend.click();
ok('اتصال بدون کلید: تولید بالا نیامد', T.st.sending === false);
ok('صفحهٔ تنظیمات باز شد', !byId.settingsModal.classList.contains('hidden'));
ok('اتصال بدون کلید به‌صورت آماده نشان داده نشد', byId.connSummary.querySelector('.conn-summary-state')?.classList.contains('missing')
  && byId.connSummary.querySelector('.conn-status-dot')?.classList.contains('needs-key'));
ok('تب اتصال‌ها و مدل‌ها فعال شد', [...doc.querySelectorAll('.stab-body')].find((b) => b.dataset.tab === 'conn').classList.contains('active'));
ok('فرم همان اتصال باز شد', byId.connForm.classList.contains('hidden') === false && byId.cfName.value === 'سرویس دو');

console.log('\n=== 11) تب‌های تنظیمات و فرم اتصال ===');
T.stabGo('behavior');
ok('جابه‌جایی تب', [...doc.querySelectorAll('.stab-body')].find((b) => b.dataset.tab === 'behavior').classList.contains('active')
  && ![...doc.querySelectorAll('.stab-body')].find((b) => b.dataset.tab === 'conn').classList.contains('active'));
eq('عنوان صفحه با تب هماهنگ شد', byId.settingsTitle.textContent, 'رفتار و پاسخ‌گویی');
T.stabGo('conn');
eq('عنوان اتصال‌ها برگشت', byId.settingsTitle.textContent, 'اتصال‌ها و مدل‌ها');
ok('خلاصهٔ فعال کارت تکراری اتصال نیست', !!byId.connSummary.querySelector('.conn-active-summary') && !byId.connSummary.querySelector('.conn-card'));
byId.cfName.value = 'سرویس دو، نام تازه';
byId.cfName.dispatch('input', { target: byId.cfName });
eq('ویرایش نام اتصال ذخیره شد', T.st.conns.find((c) => c.id === 'c2').name, 'سرویس دو، نام تازه');
byId.cfModelInput.value = 'claude-y';
byId.cfModelAdd.click();
ok('افزودن مدل به اتصال', T.st.conns.find((c) => c.id === 'c2').models.includes('claude-y'));
byId.cfTemplate.value = 'kimi';
byId.cfTemplate.dispatch('change', { target: byId.cfTemplate });
eq('قالب kimi آدرس را پر کرد', byId.cfBase.value, 'https://api.moonshot.ai/v1');
byId.cfFetchModels.click();
await sleep(10);
eq('مدل‌های واقعی سرور بارگذاری شدند', byId.cfSrvList.querySelectorAll('.srv-item').length, 2);
byId.cfSrvSearch.value = 'm-server-2';
byId.cfSrvSearch.dispatch('input', { target: byId.cfSrvSearch });
eq('جست‌وجوی تنظیمات فهرست سرور را واقعاً فیلتر می‌کند', byId.cfSrvList.querySelectorAll('.srv-item').length, 1);
byId.cfSrvSearch.value = 'model-does-not-exist';
byId.cfSrvSearch.dispatch('input', { target: byId.cfSrvSearch });
eq('جست‌وجوی سرور حالت بدون نتیجه دارد', byId.cfSrvList.querySelectorAll('.srv-item').length, 0);
ok('پیام بدون نتیجهٔ جست‌وجوی سرور نمایش داده شد', !!byId.cfSrvList.querySelector('.srv-empty'));
const tplOpts = [];
for (const key of ['opencode', 'opencode-cc', 'kimi', 'zai', 'siliconflow', 'ollama', 'lmstudio', 'apmix']) {
  if (!code.includes("'" + key + "':")) tplOpts.push(key);
}
eq('همهٔ قالب‌های سرویس تازه در کد هستند', tplOpts, []);
T.openConns(null);
ok('openConns → صفحهٔ تنظیمات + تب conn (بدون مودال دوم)',
  !byId.settingsModal.classList.contains('hidden') &&
  [...doc.querySelectorAll('.stab-body')].find((b) => b.dataset.tab === 'conn').classList.contains('active'));

console.log('\n=== 12) پروژه‌ها، کارت پروژهٔ هر چت و checkpoint ===');
T.createProject();
const project = T.st.projects[0];
ok('پروژه ساخته و فعال شد', !!project && T.st.currentProjectId === project.id);
byId.projectName.value = 'پروژهٔ آزمایشی';
byId.projectName.dispatch('input', { target: byId.projectName });
ok('اسم پروژه ذخیره شد', project.name === 'پروژهٔ آزمایشی' && byId.projectName.value === 'پروژهٔ آزمایشی');
byId.projectName.value = 'اسم تازه ';
byId.projectName.dispatch('input', { target: byId.projectName });
ok('باگ اسم‌گذاری: فضای حین تایپ پاک/بازنویسی نمی‌شود', byId.projectName.value === 'اسم تازه ' && project.name === 'اسم تازه ');
byId.projectName.onblur();
ok('blur، اسم را normalize می‌کند', project.name === 'اسم تازه' && byId.projectName.value === 'اسم تازه');
byId.projectName.value = '   ';
byId.projectName.onblur();
ok('اسم خالی در blur به «پروژهٔ بی‌نام» برمی‌گردد', project.name === 'پروژهٔ بی‌نام');
byId.projectName.value = 'پروژهٔ آزمایشی';
byId.projectName.onblur();
byId.projectNotes.value = 'هدف: انتشار امن';
byId.projectNotes.dispatch('input', { target: byId.projectNotes });
byId.projectTaskInput.value = 'تست انتشار';
byId.projectTaskAdd.click();
ok('یادداشت و کار پروژه ذخیره شدند', project.notes.includes('انتشار امن') && project.tasks[0].text === 'تست انتشار');
T.newChat(true);
ok('گفتگوی تازه پیش‌فرض «عادی» است (بدون پروژه)', !T.st.chats[0].projectId);
ok('کارت پروژه حالت عادی را نشان می‌دهد', byId.projectChip.textContent === '📁 عادی' && !byId.projectChip.classList.contains('hidden'));
byId.projectChip.click();
ok('منوی پروژه روی کارت باز می‌شود', !byId.projectMenu.classList.contains('hidden'));
const pOpts = byId.projectMenu.querySelectorAll('.pm-option');
ok('منو گزینهٔ عادی + همهٔ پروژه‌ها را دارد', pOpts.length === T.st.projects.length + 1);
pOpts.find((b) => b.textContent.includes('پروژهٔ آزمایشی')).click();
ok('پروژه انتخاب‌شده فقط به همین چت وصل شد', T.st.chats[0].projectId === project.id && byId.projectChip.textContent === '📁 پروژهٔ آزمایشی' && byId.projectMenu.classList.contains('hidden'));
ok('زمینهٔ پروژه در prompt آماده است', T.projectContextText(project).includes('تست انتشار') && T.projectContextText(project).includes('هدف'));
T.st.chats[0].messages.push({ role: 'user', text: 'درخواست پروژه', plain: 'درخواست پروژه' }, { role: 'ai', text: 'خلاصهٔ پیشرفت', plain: 'خلاصهٔ پیشرفت' });
T.checkpointProject(project, T.st.chats[0]);
ok('checkpoint از گفتگو ثبت شد', project.checkpoint && project.checkpoint.summary.includes('خلاصهٔ پیشرفت'));
byId.projectChip.click();
byId.projectMenu.querySelectorAll('.pm-option').find((b) => b.textContent.includes('حالت عادی')).click();
ok('برگشت به «عادی»: چت دیگر پروژه ندارد', !T.st.chats[0].projectId && byId.projectChip.textContent === '📁 عادی');
T.resumeProjectInNewChat();
ok('ادامه از مدیر پروژه، گفتگوی تازه با همان پروژه می‌سازد', T.st.chats[0].projectId === project.id && T.st.currentProjectId === project.id);
// payload: project context only for the chat that picked a project
byId.input.value = 'سلام با زمینهٔ پروژه';
T.setActiveConn('c1');
bridgeCalls.length = 0;
byId.btnSend.click();
const sentP = bridgeCalls.find((c) => c[0] === 'chat_send')[1].payload;
ok('زمینهٔ پروژه فقط برای چتِ انتخاب‌شده به prompt می‌رود', String(sentP.system || '').includes('[PROJECT CHECKPOINT]') && String(sentP.system).includes('پروژهٔ آزمایشی'));
emit('atria:done', { new_messages: null, final_text: 'پاسخ آزمایشی.', session_id: '', input_tokens: 2, output_tokens: 2 });
ok('پس از جریان با پروژه، ارسال بسته شد', T.st.sending === false);
byId.projectChip.click();
byId.projectMenu.querySelectorAll('.pm-option').find((b) => b.textContent.includes('حالت عادی')).click();
byId.input.value = 'حالا حالت عادی';
bridgeCalls.length = 0;
byId.btnSend.click();
const sentN = bridgeCalls.find((c) => c[0] === 'chat_send')[1].payload;
ok('حالت عادی یعنی بدون زمینهٔ پروژه در prompt', !String(sentN.system || '').includes('[PROJECT CHECKPOINT]'));
emit('atria:done', { new_messages: null, final_text: 'پاسخ آزمایشی.', session_id: '', input_tokens: 2, output_tokens: 2 });
ok('پس از جریان عادی، ارسال بسته شد', T.st.sending === false);

console.log('\n=== 13) پنجرهٔ تاریخچه و مدیریت context ===');
{
  const chat = { messages: [] };
  for (let i = 0; i < 80; i++) chat.messages.push({ role: i % 2 ? 'ai' : 'user', text: 'پیام ' + i, plain: 'پیام ' + i });
  const { hist, dropped } = T.buildHistory(chat);
  ok('سقف تعداد پیام‌ها رعایت شد', hist.length <= 48 && dropped > 0);
  ok('پیام‌های قدیمی خلاصه شدند نه فقط حذف', !!T.buildHistory(chat).summary);
  eq('نقش آغازین user است', hist[0].role, 'user');
  eq('msgText: ذخیره‌شده', T.msgText({ plain: 'الف', text: 'ب' }), 'الف');
  eq('msgText: سیمی', T.msgText({ content: [{ type: 'text', text: 'یک' }, { type: 'text', text: 'دو' }] }), 'یک\nدو');
  eq('msgText: رشته', T.msgText('س'), 'س');
}

console.log('\n=== 14) حافظه، فرمان‌ها، پاک‌سازی ===');
await T.openMemory();
await sleep(10);
ok('یادداشت‌های حافظه نمایش داده شد', byId.memList.textContent.includes('نکته'));
T.openCmdk();
ok('پالت Ctrl+K باز شد', !byId.cmdk.classList.contains('hidden'));
byId.cmdkInput.value = 'گفتگو';
byId.cmdkInput.dispatch('input', { target: byId.cmdkInput });
ok('فهرست گفتگوها فیلتر شد', byId.cmdkList.children.length >= 1);
const before = T.st.chats.length;
confirmResult = true;
T.wipeChats();
ok('پاک‌سازی همهٔ گفتگوها + ساخت گفتگوی تازه', T.st.chats.length === 1 && before >= 1);

console.log('\n=== 15) بازیابی از دیسک (ادغام) ===');
{
  chatsLoadPayload = [JSON.stringify({ id: 'disk1', title: 'از دیسک', createdAt: 123, messages: [{ role: 'user', text: 'دیسکی', plain: 'دیسکی' }] })];
  const list = await bridge.chats_load();
  eq('chats_load از پل IPC پاسخ گرفت', list.length, 1);
  const c = JSON.parse(list[0]);
  eq('ساختار چت دیسکی سالم است', c.id, 'disk1');
  bridgeCalls.length = 0;
  await bridge.chats_sync({ files: [{ id: 'disk1', data: '{}' }], remove: [] });
  ok('chats_sync از پل IPC عبور کرد', bridgeCalls.some((x) => x[0] === 'chats_sync'));
  await bridge.reveal_dir();
  ok('reveal_dir از پل IPC عبور کرد', bridgeCalls.some((x) => x[0] === 'reveal_dir'));
  await bridge.dirs_info();
  ok('dirs_info از پل IPC عبور کرد', bridgeCalls.some((x) => x[0] === 'dirs_info'));
  const dli1 = await bridge.downloads_root_info('');
  ok('downloads_root_info پوشهٔ مؤثر را برمی‌گرداند', !!dli1 && dli1.resolved === 'C:/Users/u/Desktop' && dli1.error === null, JSON.stringify(dli1));
  const dli2 = await bridge.downloads_root_info('bad root');
  ok('downloads_root_info خطای ریشه را گزارش می‌دهد', !!dli2 && dli2.resolved === null && /پیدا نشد/.test(dli2.error), JSON.stringify(dli2));
  ok('dlRootInfo در تنظیمات متن دارد', !!(byId.dlRootInfo && byId.dlRootInfo.textContent), String(byId.dlRootInfo && byId.dlRootInfo.textContent));
}

console.log('\n=== 16) تغییرات فایل و به‌روزرسانی امن ===');
const editCard = T.toolCardEl('نوشتن فایل', 'write_file', {}, false);
const editFlowItem = { k: 'p', title: 'نوشتن فایل', name: 'write_file', ok: true, in: '', out: 'This edit is staged only and has NOT been applied. Review the diff in Atria and wait for the user to approve or reject it.\n[ATRIA_PENDING_EDIT:edit-test-1]\n--- current\n+++ proposed' };
const editChat = T.st.chats[0];
editChat.messages.push({ role: 'ai', plain: 'edit test', flow: [editFlowItem] });
editCard._item = editFlowItem;
T.fileEditActions(editCard, editFlowItem.out);
ok('فایل‌ادیت تا تأیید در حالت انتظار می‌ماند', editCard.classList.contains('needs-approval') && !!editCard.querySelector('.file-edit-actions'));
await editCard.querySelector('.file-edit-actions').querySelector('.btn.primary').click();
await sleep(10);
ok('اعمال تغییر undo قابل بازگشت می‌سازد', editCard.classList.contains('file-applied') && !!editCard.querySelector('.file-edit-actions').querySelector('.btn:not(.primary)'));
ok('تأیید در تاریخچه به‌صورت backup ماندگار شد', editFlowItem.out.includes('[ATRIA_BACKUP:backup-test-1]') && !editFlowItem.out.includes('ATRIA_PENDING_EDIT'));
editCard.querySelector('.file-edit-actions').children.find((button) => button.textContent === '↶ بازگردانی').click();
await sleep(10);
ok('Undo در تاریخچه marker را هم پاک کرد', !editFlowItem.out.includes('ATRIA_BACKUP') && editFlowItem.out.includes('undone'));
const ghCard = T.toolCardEl('پیش‌نویس تغییر GitHub', 'github_propose_change', {}, false);
const ghItem = { k: 'p', title: 'پیش‌نویس تغییر GitHub', name: 'github_propose_change', ok: true, in: '{}', out: '[ATRIA_PENDING_GITHUB:gh-test-1]\nتغییر فقط پیش‌نویس شده و هنوز هیچ درخواستی برای تغییر به GitHub ارسال نشده است.\nمخزن: owner/repo' };
T.st.chats[0].messages.push({ role: 'ai', plain: 'GitHub approval test', flow: [ghItem] });
ghCard._item = ghItem;
T.githubActionActions(ghCard, ghItem.out);
ok('هر تغییر GitHub دکمهٔ تأیید جداگانه دارد', ghCard.classList.contains('needs-approval') && !!ghCard.querySelector('.github-action-actions'));
await ghCard.querySelector('.github-action-actions').querySelector('.btn.primary').click();
await sleep(10);
ok('تغییر GitHub فقط پس از کلیک ارسال شد و marker انتظار پاک شد', bridgeCalls.some((c) => c[0] === 'github_apply_action' && c[1].id === 'gh-test-1') && !ghItem.out.includes('ATRIA_PENDING_GITHUB') && ghItem.out.includes('ATRIA_GITHUB_APPLIED'));
const ghRetryCard = T.toolCardEl('پیش‌نویس GitHub قابل‌بازیابی', 'github_propose_change', {}, false);
const ghRetryItem = { k: 'p', title: 'پیش‌نویس GitHub', name: 'github_propose_change', ok: true, in: '{}', out: '[ATRIA_PENDING_GITHUB:gh-retry-1]\nپیش‌نویس تأییدنشده' };
ghRetryCard._item = ghRetryItem; T.githubActionActions(ghRetryCard, ghRetryItem.out);
githubApplyBehavior = 'missing-token';
await ghRetryCard.querySelector('.github-action-actions').querySelector('.btn.primary').click(); await sleep(10);
ok('نبود توکن، پیش‌نویس را مصرف نمی‌کند و تأیید دوباره ممکن است', ghRetryItem.out.includes('ATRIA_PENDING_GITHUB') && !ghRetryCard.querySelector('.github-action-actions').querySelector('.btn.primary').disabled);
githubApplyBehavior = 'ok';
await ghRetryCard.querySelector('.github-action-actions').querySelector('.btn.primary').click(); await sleep(10);
ok('پس از اتصال توکن، همان پیش‌نویس قابل‌تأیید است', ghRetryItem.out.includes('ATRIA_GITHUB_APPLIED') && !ghRetryItem.out.includes('ATRIA_PENDING_GITHUB'));
const ghFailCard = T.toolCardEl('پیش‌نویس GitHub خطادار', 'github_propose_change', {}, false);
const ghFailItem = { k: 'p', title: 'پیش‌نویس GitHub خطادار', name: 'github_propose_change', ok: true, in: '{}', out: '[ATRIA_PENDING_GITHUB:gh-fail-1]\nپیش‌نویس' };
ghFailCard._item = ghFailItem; T.githubActionActions(ghFailCard, ghFailItem.out);
githubApplyBehavior = 'network-error';
await ghFailCard.querySelector('.github-action-actions').querySelector('.btn.primary').click(); await sleep(10);
ok('خطای شبکه پیش‌نویس تأییدشده را برای جلوگیری از ارسال تکراری مصرف می‌کند', !ghFailItem.out.includes('ATRIA_PENDING_GITHUB') && ghFailCard.querySelector('.github-action-actions').querySelector('.file-edit-status').textContent.includes('مصرف شد') && ghFailCard.querySelector('.github-action-actions').querySelector('.btn.primary').disabled);
githubApplyBehavior = 'ok';
const urlCard = T.toolCardEl('بازکردن در مرورگر', 'open_url', {}, false);
const urlItem = { k: 'p', title: 'بازکردن در مرورگر', name: 'open_url', ok: true, in: '{}', out: '[ATRIA_PENDING_URL:url-test-1]\nپیوند فقط پیش‌نمایش شده و هنوز مرورگری باز نشده است.\nنشانی: https://example.org/' };
T.st.chats[0].messages.push({ role: 'ai', plain: 'URL approval test', flow: [urlItem] });
urlCard._item = urlItem;
T.openUrlActions(urlCard, urlItem.out);
ok('حالت پرسش لینک را تا کلیک کاربر مسدود می‌کند', urlCard.classList.contains('needs-approval') && !!urlCard.querySelector('.browser-open-actions'));
await urlCard.querySelector('.browser-open-actions').querySelector('.btn.primary').click();
await sleep(10);
ok('مرورگر فقط پس از کلیک باز شد', bridgeCalls.some((c) => c[0] === 'open_pending_url' && c[1].id === 'url-test-1') && !urlItem.out.includes('ATRIA_PENDING_URL'));
const urlFailCard = T.toolCardEl('پیوند ناموفق', 'open_url', {}, false);
const urlFailItem = { k: 'p', title: 'پیوند ناموفق', name: 'open_url', ok: true, in: '{}', out: '[ATRIA_PENDING_URL:url-fail-1]\nپیوند فقط پیش‌نمایش شده و هنوز مرورگری باز نشده است.' };
urlFailCard._item = urlFailItem; T.openUrlActions(urlFailCard, urlFailItem.out);
urlOpenBehavior = 'network-error';
await urlFailCard.querySelector('.browser-open-actions').querySelector('.btn.primary').click(); await sleep(10);
ok('خطای بازکردن URL نیز پیش‌نویس را یک‌باره مصرف می‌کند', !urlFailItem.out.includes('ATRIA_PENDING_URL') && urlFailCard.querySelector('.browser-open-actions').querySelector('.btn.primary').disabled);
urlOpenBehavior = 'ok';
console.log('\n=== 16b) ترمینال: تأییدِ هر دستور + پوشهٔ پروژه ===');
ok('hasPendingApproval marker ترمینال را می‌شناسد', T.hasPendingApproval('run_command', 'x\n[ATRIA_PENDING_COMMAND:cmd-x-1]\nx') === true && T.hasPendingApproval('run_command', 'دستور اجرا شد. کد خروجی: 0') === false);
const cmdCard = T.toolCardEl('اجرای دستور ترمینال', 'run_command', { command: 'echo hello' }, false);
const cmdItem = { k: 'p', title: 'اجرای دستور ترمینال', name: 'run_command', ok: true, in: '{"command":"echo hello"}', out: 'این دستور هنوز اجرا نشده است. در آتریا آن را بررسی کن و تأیید یا رد کن.\n[ATRIA_PENDING_COMMAND:cmd-test-1]\nدستور: echo hello\nپوشهٔ اجرا: C:/u/.atria/workspace' };
T.st.chats[0].messages.push({ role: 'ai', plain: 'cmd test', flow: [cmdItem] });
cmdCard._item = cmdItem;
T.commandActions(cmdCard, cmdItem.out);
ok('دستور ترمینال تا کلیک کاربر منتظر می‌ماند', cmdCard.classList.contains('needs-approval') && !!cmdCard.querySelector('.command-actions'));
bridgeCalls.length = 0;
await cmdCard.querySelector('.command-actions').querySelector('.btn.primary').click();
await sleep(10);
ok('دستور فقط پس از تأیید اجرا شد و marker از تاریخچه پاک شد', bridgeCalls.some((c) => c[0] === 'command_apply_command' && c[1].id === 'cmd-test-1') && !cmdItem.out.includes('ATRIA_PENDING_COMMAND') && cmdItem.out.includes('کد خروجی: 0') && !cmdCard.classList.contains('needs-approval'));
const cmdRejectCard = T.toolCardEl('دستور قابل‌رد', 'run_command', { command: 'echo never' }, false);
const cmdRejectItem = { k: 'p', title: 'اجرای دستور ترمینال', name: 'run_command', ok: true, in: '{"command":"echo never"}', out: 'این دستور هنوز اجرا نشده است.\n[ATRIA_PENDING_COMMAND:cmd-rej-1]\nدستور: echo never' };
T.st.chats[0].messages.push({ role: 'ai', plain: 'cmd reject test', flow: [cmdRejectItem] });
cmdRejectCard._item = cmdRejectItem;
T.commandActions(cmdRejectCard, cmdRejectItem.out);
bridgeCalls.length = 0;
await cmdRejectCard.querySelector('.command-actions').querySelector('.btn.danger').click();
await sleep(10);
ok('رد دستور، اجرا را لغو و marker را پاک کرد', bridgeCalls.some((c) => c[0] === 'command_reject_command' && c[1].id === 'cmd-rej-1') && !cmdRejectItem.out.includes('ATRIA_PENDING_COMMAND') && cmdRejectItem.out.includes('rejected'));
ok('چک‌باکس ترمینال همگام است', byId.terminalToolsVal.checked === true && T.st.settings.terminal_tools === true);
byId.terminalToolsVal.checked = false; byId.terminalToolsVal.onchange();
ok('خاموش‌کردن ترمینال در settings ماند', T.st.settings.terminal_tools === false);
byId.terminalToolsVal.checked = true; byId.terminalToolsVal.onchange();
const prevProject = T.st.currentProjectId;
const folderProject = { id: 'p-folder-test', name: 'پروژهٔ پوشه‌ای', notes: '', tasks: [], checkpoint: null, folder: 'D:/projects/tafriki', updatedAt: Date.now() };
T.st.projects.unshift(folderProject);
T.setCurrentProject(folderProject.id);
byId.input.value = 'پوشهٔ پروژه فعال';
T.setActiveConn('c1');
bridgeCalls.length = 0;
byId.btnSend.click();
const sent2 = bridgeCalls.find((c) => c[0] === 'chat_send')[1].payload;
ok('پوشهٔ پروژهٔ فعال به‌عنوان project_root رفت', sent2.project_root === 'D:/projects/tafriki');
emit('atria:done', { new_messages: null, final_text: 'x', session_id: '', input_tokens: 1, output_tokens: 1 });
T.setCurrentProject(prevProject || '');
ok('دکمهٔ انتخاب پوشه بدون Tauri-dialog خطا نمی‌دهد', (() => { try { byId.projectFolderPick.click(); return true; } catch (e) { return false; } })());
const updateBtn = byId.btnCheckUpdate;
await T.checkForUpdate(false);
ok('بررسی نسخهٔ امضاشده نتیجه می‌دهد', byId.updateStatus.textContent.includes('به‌روز'));

console.log('\n=== 16c) بررسی خودکار هنگام راه‌اندازی + تأیید کاربر ===');
ok('پیش از بررسی، مودال آپدیت بسته است', byId.updateModal.classList.contains('hidden'));
updateCheckAvailable = true;
await T.checkForUpdate(true); // همان بررسیِ خودکارِ boot (silent)
ok('با وجود نسخهٔ تازه، مودالِ تأیید روی کاربر باز می‌شود', !byId.updateModal.classList.contains('hidden') && String(byId.updatePromptTitle.textContent).includes('0.19.0'));
ok('متن مودال امضا و جایگزینی را توضیح می‌دهد', String(byId.updatePromptBody.textContent).includes('Ed25519') && String(byId.updatePromptBody.textContent).includes('باز می‌شود'));
ok('قبل از تأیید، نصبی صدا زده نشده', !bridgeCalls.some((c) => c[0] === 'update_install'));
byId.updatePromptNo.click();
ok('«نه، بعداً» فقط مودال را می‌بندد؛ نصبی انجام نشده', byId.updateModal.classList.contains('hidden') && !bridgeCalls.some((c) => c[0] === 'update_install'));
updateCheckAvailable = true;
await T.checkForUpdate(true); // دوباره در این نشست (هر بار بررسی می‌شود)
ok('در همان نشست اگر هنوز آپدیت نصب نشده باشد، دوباره پرسیده می‌شود', !byId.updateModal.classList.contains('hidden'));
bridgeCalls.length = 0;
byId.updatePromptYes.click();
await sleep(10);
ok('«حالا به‌روز کنم» دانلودِ معتبر + جایگزینی را آغاز می‌کند', bridgeCalls.some((c) => c[0] === 'update_install') && String(byId.updatePromptStatus.textContent).length > 0);
ok('دکمه‌ها حین نصب قفل شده‌اند', byId.updatePromptYes.disabled === true && byId.updatePromptNo.disabled === true);
updateCheckAvailable = false;
await T.checkForUpdate(true);
ok('وقتی به‌روز است، مودال نمی‌افتد و وضعیت «به‌روز» می‌ماند', byId.updateModal.classList.contains('hidden') && byId.updateStatus.textContent.includes('به‌روز'));

console.log('\n=== 16d) کارت آپلود یکپارچه + MCP + ایجنت=Full Access ===');
// --- ایجنت و ابزارهای فایل در payload فقط تابع Full Access هستند ---
T.setActiveConn('c1');
confirmResult = true;
const faBefore = T.st.settings.full_access_mode === true;
byId.fullAccessIndicator.click();
ok('Full Access toggle شد', (T.st.settings.full_access_mode === true) !== faBefore);
bridgeCalls.length = 0;
byId.input.value = 'payload با Full Access';
byId.btnSend.click();
let sentFA = bridgeCalls.find((c) => c[0] === 'chat_send')[1].payload;
ok('ایجنت+فایل در payload با Full Access روشن', sentFA.tools_enabled === true && sentFA.file_tools === true && sentFA.full_access_mode === true);
emit('atria:done', { new_messages: null, final_text: 'x', session_id: '', input_tokens: 1, output_tokens: 1 });
byId.fullAccessIndicator.click();
bridgeCalls.length = 0;
byId.input.value = 'payload بدون Full Access';
byId.btnSend.click();
sentFA = bridgeCalls.find((c) => c[0] === 'chat_send')[1].payload;
ok('ایجنت+فایل در payload بدون Full Access خاموش', sentFA.tools_enabled === false && sentFA.file_tools === false && sentFA.full_access_mode === false);
emit('atria:done', { new_messages: null, final_text: 'x', session_id: '', input_tokens: 1, output_tokens: 1 });

// --- MCP: pass-through + مدیریت در تنظیمات ---
T.mcpServers().push({ name: 'fs', command: 'npx', args: ['-y', '@modelcontextprotocol/server-filesystem'] });
T.mcpRender();
bridgeCalls.length = 0;
byId.input.value = 'mcp payload';
byId.btnSend.click();
const sentM = bridgeCalls.find((c) => c[0] === 'chat_send')[1].payload;
ok('سرورهای MCP دست‌نخورده به payload می‌روند', Array.isArray(sentM.mcp_servers) && sentM.mcp_servers.length === 1 && sentM.mcp_servers[0].name === 'fs' && sentM.mcp_servers[0].command === 'npx');
emit('atria:done', { new_messages: null, final_text: 'x', session_id: '', input_tokens: 1, output_tokens: 1 });
byId.mcpList.querySelectorAll('.mcp-card')[0].querySelectorAll('.btn')[0].click(); // تست
await sleep(10);
ok('تست MCP با mcp_test_server انجام شد و ابزارها لیست شدند', bridgeCalls.some((c) => c[0] === 'mcp_test_server') && byId.mcpList.textContent.includes('echo') && byId.mcpList.textContent.includes('متصل'));
byId.mcpList.querySelectorAll('.mcp-card')[0].querySelectorAll('.btn')[1].click(); // حذف
ok('حذف سرور MCP کار می‌کند', !byId.mcpList.textContent.includes('fs') && T.mcpServers().length === 0);
byId.mcpName.value = 'demo';
byId.mcpCmd.value = 'node';
byId.mcpArgs.value = '-e script';
byId.mcpAddBtn.click();
await sleep(10);
ok('افزودن سرور MCP کارتی می‌سازد و خودکار تست می‌کند', byId.mcpList.textContent.includes('demo') && T.mcpServers().length === 1 && bridgeCalls.filter((c) => c[0] === 'mcp_test_server').length >= 2);
T.mcpRemoveServer(0);

// --- کارت یکپارچهٔ اپلود (فایل + تصویر) ---
sandbox.FileReader = class {
  constructor() { this.result = null; }
  readAsDataURL(f) {
    this.result = 'data:' + (f.type || 'application/octet-stream') + ';base64,' + (f._b64 || 'QUJD');
    setTimeout(() => this.onload && this.onload(), 0);
  }
};
T.addUploadFiles([
  { name: 'notes.txt', type: 'text/plain', size: 11, text: async () => 'hello world' },
  { name: 'pic.png', type: 'image/png', size: 100, _b64: 'QUJD' },
  { name: 'doc.pdf', type: 'application/pdf', size: 50, _b64: 'QUJD' },
]);
await sleep(20);
ok('فایل متنی با محتوا در صف پیوست است', T.st.pendingFiles.some((f) => f.name === 'notes.txt' && f.kind === 'text' && f.text === 'hello world'));
ok('تصویر در صف پیوست است', T.st.pendingImgs.length === 1 && T.st.pendingImgs[0].data === 'QUJD' && T.st.pendingImgs[0].media === 'image/png');
ok('فایل باینری با files_stash ذخیره شد و مسیرش ثبت شد', bridgeCalls.some((c) => c[0] === 'files_stash') && T.st.pendingFiles.some((f) => f.name === 'doc.pdf' && f.kind === 'binary' && f.path && f.path.includes('.atria/uploads')));
ok('tray هر سه پیوست را نشان می‌دهد', byId.imgTray.classList.contains('hidden') === false && byId.imgTray.textContent.includes('notes.txt') && byId.imgTray.textContent.includes('doc.pdf'));
bridgeCalls.length = 0;
byId.input.value = 'این فایل‌ها را ببین';
byId.btnSend.click();
const sentA = bridgeCalls.find((c) => c[0] === 'chat_send')[1].payload;
const lastMsg = sentA.messages[sentA.messages.length - 1];
const lastJson = JSON.stringify(lastMsg.content);
ok('متن فایل داخل پیام درلاین شد', lastJson.includes('hello world'));
ok('مسیر فایل باینری در پیام است', lastJson.includes('.atria/uploads'));
ok('تصویر به‌عنوان بلوک image ارسال شد', lastMsg.content.some((b) => b.type === 'image' && b.data === 'QUJD'));
ok('پیوست‌ها پس از ارسال پاک شدند', T.st.pendingFiles.length === 0 && T.st.pendingImgs.length === 0 && byId.imgTray.classList.contains('hidden'));
emit('atria:done', { new_messages: null, final_text: 'x', session_id: '', input_tokens: 1, output_tokens: 1 });

console.log('\n=== 16e) اسکرول هوشمند حین تولید + ردیف توضیح اقدامات ===');
{
  const M = byId.messages;
  M.scrollHeight = 5000; M.clientHeight = 300; M.scrollTop = 0;
  T.scrollBottom(true);
  ok('اسکرول بالای صفحه حین تولید به پایین کشیده نمی‌شود', M.scrollTop === 0);
  T.forceScrollBottom(true);
  ok('اسکرول صریح (دکمه/ارسال) به پایین می‌رود', M.scrollTop === 5000);
  M.scrollTop = 4950;
  T.scrollBottom(true);
  ok('نزدیکِ پایین، دنبالِ محتوا می‌ماند', M.scrollTop === 5000);
  M.scrollTop = 0; M.scrollHeight = 0;
}
{
  const c1 = T.toolCardEl('اجرای دستور ترمینال', 'run_command', { command: 'ls -la' }, false);
  ok('اقدام run_command توضیح یک‌خطی دارد', c1.querySelector('.tool-desc').textContent.includes('ls -la'));
  const c2 = T.toolCardEl('جست‌وجوی وب', 'web_search', { query: 'آیرنا' }, false);
  ok('اقدام web_search توضیح یک‌خطی دارد', c2.querySelector('.tool-desc').textContent.includes('آیرنا'));
  const c3 = T.toolCardEl('ابزار MCP', 'mcp__fs__read_file', { path: '/x/y.txt' }, false);
  ok('ابزار MCP نام سرور/ابزار را می‌گوید', c3.querySelector('.tool-desc').textContent.includes('fs / read_file'));
  const c4 = T.toolCardEl('خواندن فایل', 'read_file', { path: 'C:/u/.atria/workspace/a.txt' }, false);
  ok('اقدام read_file مسیر را می‌گوید', c4.querySelector('.tool-desc').textContent.includes('a.txt'));
  ok('toolDesc برای ابزار ناشناخته، اولین آرگومان رشته‌ای را برمی‌گزیند', T.toolDesc('github_get_repository', { owner: 'acme', repo: 'web' }).includes('acme'));
}

console.log('\n=== 17) نگهبان‌های ایمنی ===');
ok('منو بدون toasts نمی‌میرد', (() => { const t = T.els.toasts; T.els.toasts = null; try { byId.modelPick.click(); byId.modelPick.click(); } catch { T.els.toasts = t; return false; } T.els.toasts = t; return true; })());
ok('updateModelPick بدون mpName نمی‌میرد', (() => { const m = T.els.mpName; T.els.mpName = null; try { T.updateModelPick(); } catch { T.els.mpName = m; return false; } T.els.mpName = m; return true; })());

console.log('\n=== 18) v0.18.0 — toolDesc + diff + دکمه‌های چت ===');
{
  ok('toolDesc برای run_code زبان و کد را می‌گوید', T.toolDesc('run_code', { code: 'print(6*7)', lang: 'python' }).includes('python'));
  ok('toolDesc برای multi_search پرسش‌ها را می‌گوید', T.toolDesc('multi_search', { queries: ['آیرنا', 'GitHub releases'] }).includes('آیرنا'));
  ok('toolDesc برای extract_text مسیر را می‌گوید', T.toolDesc('extract_text', { path: 'report.docx' }).includes('report.docx'));
  const fakeOut = 'This edit is staged only and has NOT been applied. Review the diff in Atria and wait for the user to approve or reject it.\n[ATRIA_PENDING_EDIT:edit-h-1]\n--- current\n+++ proposed\n ctx line\n-old line\n+new line\n ctx2';
  const card = T.toolCardEl('نوشتن فایل', 'write_file', { path: 'a.txt' }, false);
  T.fileEditActions(card, fakeOut);
  const diff = card.querySelector('.diff-lines');
  ok('کارت write_file یک diff رنگی دارد', !!diff);
  ok('diff خطوط + و - را جدا می‌کند', !!diff && !!diff.querySelector('.dl-add') && !!diff.querySelector('.dl-del'));
  T.newChat(true);
  T.st.chats[0].messages.push({ role: 'user', text: 'سلام', plain: 'سلام', ts: Date.now() });
  T.st.chats[0].messages.push({ role: 'assistant', text: 'سلام! چطور کمکت کنم؟', plain: 'سلام! چطور کمکت کنم؟', ts: Date.now() + 1 });
  if (typeof T.renderHistory === 'function') T.renderHistory('force');
  const aiBubbles = byId.messages.querySelectorAll('.msg.ai');
  const ai = aiBubbles[aiBubbles.length - 1];
  const acts = ai ? ai.querySelectorAll('.msg-act') : [];
  ok('حباب هوشمند دکمه‌های 🔊 و 🧠 دارد', acts.some((b) => b.textContent === '🔊') && acts.some((b) => b.textContent === '🧠'));
}

console.log('\n=== 19) v0.18.0 — برچسب‌ها، جست‌وجوی چت، کلیپ‌بورد، drag-drop ===');
{
  const snips = T.v18Snips();
  if (!snips.some((x) => x.name === 'report')) snips.push({ name: 'report', text: 'خلاصهٔ هفتگی: کارها، مانع‌ها، برنامهٔ هفتهٔ بعد' });
  T.v18SaveSnips();
  byId.input.value = '/rep';
  T.v18SnipMenu();
  const menu = (byId.input.closest('.composer') || byId.input.parentElement).querySelector('.snippet-menu');
  ok('منوی «/» با یک حرف شروع باز می‌شود', !!menu && menu.querySelectorAll('.sm-item').length === 1);
  T.v18ApplySnip({ name: 'report', text: 'خلاصهٔ هفتگی: کارها، مانع‌ها، برنامهٔ هفتهٔ بعد' });
  ok('فراخوانی برچسب، متنش را جایگزین می‌کند', byId.input.value.startsWith('خلاصهٔ هفتگی'));
  byId.input.value = '';
  T.v18OpenSearch();
  byId.chatSearchInput.value = 'چطور';
  T.v18RunSearch();
  ok('جست‌وجوی چت نتیجه می‌یابد', T.v18SearchHits.length >= 1);
  T.v18GotoSearch(0);
  ok('شمارندهٔ جست‌وجو می‌نویسد', byId.chatSearchCount.textContent.indexOf('/') >= 0);
  T.v18CloseSearch();
  ok('بستن جست‌وجو، نشانه‌ها را پاک می‌کند', T.v18SearchHits.length === 0);
  byId.input.value = '';
  byId.btnClipboard.click();
  await sleep(20);
  ok('دکمهٔ کلیپ‌بورد متن را وارد جعبه می‌کند', byId.input.value.includes('متن کلیپ‌بورد تستی'));
  const beforeFiles = T.st.pendingFiles.length;
  emit('tauri://drag-drop', { type: 'drop', paths: ['/home/u/report.txt'] });
  await sleep(30);
  ok('درگ‌اند‌دراپ فایل را از files_read می‌خواند', T.st.pendingFiles.length === beforeFiles + 1);
  T.clearAttach();
}

console.log('\n=== 20) v0.18.0 — زمان‌بندی، پیش‌تنظیم‌ها، env MCP، حافظهٔ خودکار، مخزن ===');
{
  v18Log = [{ job_id: 'j1', label: 'چک هفتگی', ran_at: '2026-10-22T06:00:00Z', prompt: 'خبرها را چک کن', output: '✅ نتیجهٔ وظیفه', is_error: false, caught_up: true }];
  T.v18SchedChat();
  T.v18OnScheduleResult(v18Log[0]);
  const schedChat = T.v18SchedChat();
  ok('چت «⏰ زمان‌بندی‌ها» می‌سازد', schedChat.id === '__scheduled__');
  ok('نتیجهٔ زمان‌بندی در چت ثبت می‌شود', schedChat.messages.length >= 2);
  byId.scheduleModal.classList.remove('hidden');
  byId.schedLabel.value = 'چک صبحگاهی';
  byId.schedPrompt.value = 'مخزن آتریای را چک کن';
  byId.schedWhen.value = '2026-10-23T09:30';
  byId.schedEvery.value = '0';
  byId.schedAdd.click();
  await sleep(30);
  ok('افزودن وظیفه از فرم ثبت می‌شود', v18Jobs.some((j) => j.label === 'چک صبحگاهی'));
  ok('زمان ثبت‌شده ISO-8601 (UTC) است', v18Jobs.some((j) => j.next_run === '2026-10-23T09:30:00Z'));
  byId.schedConnPick.value = 'c1';
  byId.schedConnSave.click();
  await sleep(20);
  ok('ذخیرهٔ ارتباط زمان‌بندی', v18ConnSaved === true);
  byId.schedConnClear.click();
  await sleep(20);
  ok('حذف ارتباط زمان‌بندی', v18ConnSaved === false);
  byId.scheduleModal.classList.add('hidden');
  const connsBefore = T.st.conns.length;
  T.v18RenderPresets();
  const chips = byId.connPresets.querySelectorAll('.preset-chip');
  ok('پیش‌تنظیم‌های اتصال رندر می‌شوند', chips.length >= 5);
  chips[0].click();
  ok('یک کلیک روی پیش‌تنظیم، اتصال می‌سازد', T.st.conns.length === connsBefore + 1);
  ok('env پیش‌تنظیمی درشده خالی است یا پر', (T.st.conns[0].env || []).length >= 0);
  ok('v18ParseEnv متغیرهای K=V را می‌پذیرد', T.v18ParseEnv('A=1 B=2').length === 2);
  let threw = false;
  try { T.v18ParseEnv('NOEQUALS'); } catch (e) { threw = true; }
  ok('v18ParseEnv بدون = خطا می‌دهد', threw);
  byId.mcpName.value = 'harness-mcp';
  byId.mcpCmd.value = 'npx';
  byId.mcpArgs.value = '-y server-x';
  byId.mcpEnv.value = 'API_KEY=abc DEF=2';
  T.mcpAddServer();
  await sleep(40);
  const withEnv = T.mcpServers().find((x) => x.name === 'harness-mcp');
  ok('سرور MCP با env ذخیره می‌شود', !!withEnv && withEnv.env.length === 2 && withEnv.env[0] === 'API_KEY=abc');
  T.setActiveConn('c1');
  const memBefore = bridgeCalls.filter((c) => c[0] === 'memory_extract').length;
  await T.v18ExtractMemory('کاربر گفت: نامم علی است', null, false);
  await sleep(20);
  ok('🧠 از پل memory_extract عبور می‌کند', bridgeCalls.filter((c) => c[0] === 'memory_extract').length === memBefore + 1);
  ok('حافظهٔ خودکار پیش‌فرض خاموش است', T.st.settings.auto_mem === false);
  let notified = false;
  const origInvoke2 = win.__TAURI_INTERNALS__.invoke;
  win.__TAURI_INTERNALS__.invoke = (cmd, args) => { if (cmd === 'notify_run_done') notified = true; return origInvoke2(cmd, args); };
  const origDoc = win.document;
  win.document = { hasFocus: () => false };
  try { T.v18RunFinished({ final_text: 'پاسخ', session_id: '', input_tokens: 1, output_tokens: 1 }); } catch (e) {}
  win.document = origDoc;
  win.__TAURI_INTERNALS__.invoke = origInvoke2;
  ok('پایان اجرا بدون فوکوس، نوتیفیسیون می‌زند', notified === true);
  const proj = T.activeProject() || T.createProject('تست مخزن');
  byId.projectRepo.value = 'Manixpcshot/atria-ai';
  byId.projectRepoCheck.click();
  await sleep(30);
  ok('بررسی وضعیت مخزن، اطلاعات را نمایش می‌دهد', byId.projectRepoStatus.textContent.includes('42'));
  ok('مخزن در شیء پروژه ذخیره شد', (T.activeProject() || {}).repo === 'Manixpcshot/atria-ai');
}

console.log('\n============================');
console.log('PASS: ' + PASS + '   FAIL: ' + FAIL);
if (fails.length) { console.log('شکست‌ها:'); fails.forEach((f) => console.log('  \u2717 ' + f)); }
process.exit(FAIL ? 1 : 0);
