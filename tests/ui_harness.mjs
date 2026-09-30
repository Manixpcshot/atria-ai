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
  'btnSettings', 'btnMemory', 'btnProjects', 'btnSide', 'projectsModal', 'projectsX', 'projectList', 'projectDetail', 'projectCreate',
  'projectName', 'projectNotes', 'projectCheckpoint', 'projectTaskCount', 'projectTasks', 'projectTaskInput', 'projectTaskAdd',
  'projectAttach', 'projectCheckpointSave', 'projectResume', 'projectDelete', 'ctxLimitVal', 'ctxMeter', 'btnCheckUpdate', 'btnInstallUpdate', 'updateStatus', 'projectChip', 'btnMin', 'btnMax', 'btnClose', 'btnScroll',
  'modelChip', 'modelPick', 'mpName', 'modelMenu', 'chipAgent', 'chipThink', 'chipFiles',
  'settingsModal', 'settingsX', 'settingsTitle', 'settingsSubtitle', 'memoryModal', 'memoryX', 'memoryClose',
  'memList', 'memClear', 'btnWipe', 'cmdk', 'cmdkInput', 'cmdkList', 'cmdkX', 'toasts',
  'tempVal', 'tempOut', 'thinkVal', 'memVal', 'streamVal', 'sysVal', 'maxTokVal',
  'toolsVal', 'fileToolsVal', 'wsVal', 'dsSearchRow', 'dsSearchVal',
  'tokenGuide', 'tokenGuideX', 'tokenGuideClose', 'connSummary', 'connList', 'connAdd', 'connForm', 'connsBody',
  'cfName', 'cfKind', 'cfTemplate', 'cfBase', 'cfKey', 'cfKeyEye', 'cfKeyLabel', 'cfTokenGuide', 'cfHint',
  'cfTags', 'cfModelInput', 'cfModelAdd', 'cfFetchModels', 'cfSrvWrap', 'cfSrvSearch', 'cfSrvList',
  'cfDelete', 'cfSetActive', 'cfTestConn', 'cfTestStatus', 'storageBox', 'btnRevealAtria', 'btnAttach', 'fileInput', 'imgTray',
  'stars', 'appVer', 'tokChip']) addId(id);
byId.connsBody.classList.add('conns-body');
for (const id of ['modelMenu', 'settingsModal', 'memoryModal', 'projectsModal', 'cmdk', 'tokenGuide', 'imgTray',
  'dsSearchRow', 'btnScroll', 'tokChip', 'connForm', 'btnStop', 'cfHint', 'cfTokenGuide', 'btnInstallUpdate']) byId[id].classList.add('hidden');
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
let chatsLoadCalled = 0;
function router(cmd, args = {}) {
  switch (cmd) {
    case 'app_meta': return Promise.resolve({ version: '0.8.0' });
    case 'secret_set': secretStore.set(args.id, args.value); return Promise.resolve();
    case 'secret_get': return Promise.resolve(secretStore.get(args.id) || null);
    case 'secret_delete': secretStore.delete(args.id); return Promise.resolve();
    case 'test_connection': return Promise.resolve({ ok: true, latency_ms: 18, reply: 'OK', input_tokens: 3, output_tokens: 1 });
    case 'file_apply_edit': return Promise.resolve('applied [ATRIA_BACKUP:backup-test-1]');
    case 'file_reject_edit': return Promise.resolve();
    case 'file_restore_backup': return Promise.resolve('restored');
    case 'update_check': return Promise.resolve({ current_version: '0.8.0', latest_version: '0.8.0', available: false, release_url: '', download_size: 0 });
    case 'update_install': return Promise.resolve();
    case 'chats_load': chatsLoadCalled++; return Promise.resolve(chatsLoadPayload);
    case 'dirs_info': return Promise.resolve({ chats: 'C:/u/.atria/chats', workspace: 'C:/u/.atria/workspace', memory: 'C:/u/.atria/memory.json' });
    case 'memory_list': return Promise.resolve([{ title: 'نکته', content: 'متن', ts: 'امروز' }]);
    case 'list_models': return Promise.resolve(['m-server-1', 'm-server-2']);
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
  ' fileEditActions, toolCardEl, checkForUpdate, installUpdate };';
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
const needed = ['chat_send', 'chat_stop', 'memory_list', 'memory_clear', 'app_meta', 'list_models', 'test_connection', 'secret_set', 'secret_get', 'secret_delete', 'file_apply_edit', 'file_reject_edit', 'file_restore_backup', 'update_check', 'update_install',
  'minimize_win', 'maximize_win', 'close_win', 'chats_load', 'chats_sync', 'dirs_info', 'reveal_dir'];
const bridge = win.__atria;
ok('پل IPC همهٔ توابع لازم را دارد', needed.every((k) => typeof bridge[k] === 'function'),
  needed.filter((k) => !bridge || typeof bridge[k] !== 'function').join(','));
ok('دکمهٔ انتخاب مدل سیم‌کشی شد', typeof byId.modelPick.onclick === 'function');
ok('دکمهٔ ایجنت سیم‌کشی شد', typeof byId.chipAgent.onclick === 'function');
ok('دکمهٔ ارسال سیم‌کشی شد', typeof byId.btnSend.onclick === 'function');
eq('listenEvents هر ۱۱ رویداد را ثبت کرد',
  Object.keys(tauriListeners).sort(),
  ['atria:done', 'atria:error', 'atria:retry', 'atria:round', 'atria:stopped', 'atria:text',
   'atria:thinking', 'atria:tool_args', 'atria:tool_end', 'atria:tool_pending', 'atria:tool_start']);
ok('هیچ اشاره‌ای به دکمهٔ حذف‌شده باقی نمانده', !('btnManageConns' in T.els));
await sleep(20);
ok('بازیابی گفتگو از دیسک صدا زده شد', chatsLoadCalled > 0);
await sleep(10);
eq('نسخه از app_meta بالای پنجره نشست', byId.appVer.textContent, 'v0.8.0');
ok('کلید قدیمی به مخزن امن منتقل شد', secretStore.get('c1') === 'k1');
const savedConns = JSON.parse(localStorage.getItem('atria.conns.v1') || '[]');
ok('localStorage فقط metadata نگه می‌دارد و کلید را پاک کرده', savedConns.length === 2 && savedConns.every((c) => !c.key));
ok('settings legacy نیز api_key ندارد', !Object.prototype.hasOwnProperty.call(JSON.parse(localStorage.getItem('atria.settings.v2') || '{}'), 'api_key'));
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

console.log('\n=== 4) چیپ‌های حالت (ایجنت/تفکر/فایل) ===');
const toolsBefore = T.st.settings.tools_enabled;
byId.chipAgent.click();
eq('⚡ ایجنت toggle شد', T.st.settings.tools_enabled, !toolsBefore);
ok('کلاس on همگام شد', byId.chipAgent.classList.contains('on') === T.st.settings.tools_enabled);
eq('چک‌باکس تنظیمات همگام شد', byId.toolsVal.checked, T.st.settings.tools_enabled);
const thinkBefore = T.st.settings.thinking;
byId.chipThink.click();
eq('🧠 تفکر toggle شد', T.st.settings.thinking, !thinkBefore);
byId.chipFiles.click();
ok('📂 فایل‌ها toggle شد', typeof T.st.settings.file_tools === 'boolean');

console.log('\n=== 5) جریان ارسال پیام (send → رویدادها → ذخیره) ===');
byId.chipAgent.click(); // برگرداندن حالت ایجنت
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

console.log('\n=== 12) پروژه‌ها، checkpoint و ادامهٔ گفتگو ===');
T.createProject();
const project = T.st.projects[0];
ok('پروژه ساخته و فعال شد', !!project && T.st.currentProjectId === project.id);
byId.projectName.value = 'پروژهٔ آزمایشی';
byId.projectName.dispatch('input', { target: byId.projectName });
byId.projectNotes.value = 'هدف: انتشار امن';
byId.projectNotes.dispatch('input', { target: byId.projectNotes });
byId.projectTaskInput.value = 'تست انتشار';
byId.projectTaskAdd.click();
ok('یادداشت و کار پروژه ذخیره شدند', project.notes.includes('انتشار امن') && project.tasks[0].text === 'تست انتشار');
T.newChat(true);
ok('گفتگوی تازه به پروژه متصل شد', T.st.chats[0].projectId === project.id);
ok('زمینهٔ پروژه در prompt آماده است', T.projectContextText(project).includes('تست انتشار') && T.projectContextText(project).includes('هدف'));
T.st.chats[0].messages.push({ role: 'user', text: 'درخواست پروژه', plain: 'درخواست پروژه' }, { role: 'ai', text: 'خلاصهٔ پیشرفت', plain: 'خلاصهٔ پیشرفت' });
T.checkpointProject(project, T.st.chats[0]);
ok('checkpoint از گفتگو ثبت شد', project.checkpoint && project.checkpoint.summary.includes('خلاصهٔ پیشرفت'));
T.resumeProjectInNewChat();
ok('ادامه، گفتگوی تازه با همان پروژه می‌سازد', T.st.chats[0].projectId === project.id && T.st.currentProjectId === project.id);

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
const updateBtn = byId.btnCheckUpdate;
await T.checkForUpdate(false);
ok('بررسی نسخهٔ امضاشده نتیجه می‌دهد', byId.updateStatus.textContent.includes('به‌روز'));

console.log('\n=== 17) نگهبان‌های ایمنی ===');
ok('منو بدون toasts نمی‌میرد', (() => { const t = T.els.toasts; T.els.toasts = null; try { byId.modelPick.click(); byId.modelPick.click(); } catch { T.els.toasts = t; return false; } T.els.toasts = t; return true; })());
ok('updateModelPick بدون mpName نمی‌میرد', (() => { const m = T.els.mpName; T.els.mpName = null; try { T.updateModelPick(); } catch { T.els.mpName = m; return false; } T.els.mpName = m; return true; })());

console.log('\n============================');
console.log('PASS: ' + PASS + '   FAIL: ' + FAIL);
if (fails.length) { console.log('شکست‌ها:'); fails.forEach((f) => console.log('  \u2717 ' + f)); }
process.exit(FAIL ? 1 : 0);
