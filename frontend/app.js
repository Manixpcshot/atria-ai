/* Atria — UI controller (v0.2 — matches the Dawn CSS contract) */
'use strict';

const $ = (s) => document.querySelector(s);

/* ---------------- providers ---------------- */

const CONN_TEMPLATES = {
  'atria':      { label: 'آتریا — Messages API',     base: 'https://api.atria-asi.ai',                                kind: 'anthropic' },
  'atria-cc':   { label: 'آتریا — Chat Completions', base: 'https://api.atria-asi.ai',                                kind: 'openai' },
  'claude':     { label: 'Anthropic (Claude)',       base: 'https://api.anthropic.com',                               kind: 'anthropic' },
  'openai':     { label: 'OpenAI',                   base: 'https://api.openai.com/v1',                               kind: 'openai' },
  'gemini':     { label: 'Google Gemini',            base: 'https://generativelanguage.googleapis.com/v1beta/openai', kind: 'openai' },
  'groq':       { label: 'Groq',                     base: 'https://api.groq.com/openai/v1',                          kind: 'openai' },
  'xai':        { label: 'xAI (Grok)',               base: 'https://api.x.ai/v1',                                     kind: 'openai' },
  'deepseek':   { label: 'DeepSeek — API رسمی',      base: 'https://api.deepseek.com/v1',                             kind: 'openai' },
  'mistral':    { label: 'Mistral',                  base: 'https://api.mistral.ai/v1',                               kind: 'openai' },
  'openrouter': { label: 'OpenRouter',               base: 'https://openrouter.ai/api/v1',                            kind: 'openai' },
  'omniroute':  { label: 'OmniRoute — گیت‌وی همه‌چیز', base: 'http://localhost:20128/v1',                              kind: 'openai' },
  'deepseek-web': { label: 'DeepSeek — حساب وب (یوزر توکن)', base: 'https://chat.deepseek.com',                       kind: 'deepseek_web' },
  'custom':     { label: 'سفارشی…',                   base: '',                                                        kind: 'openai' },
};

/* ---------------- state ---------------- */

const DEFAULTS = {
  provider: 'atria',
  api_key: '',
  base_url: 'https://api.atria-asi.ai',
  api_kind: 'anthropic',
  model: 'Atria-Dawn-Preview',
  thinking: true,
  mem: true,
  temp: 0.7,
  stream: true,
  system: '',
  max_tokens: 4096,
  tools_enabled: true,
  file_tools: true,
  workspace: '',
  ds_search: false,
};
const LS_SETTINGS = 'atria.settings.v2';
const LS_CHATS = 'atria.chats.v1';
const LS_CURRENT = 'atria.current.v1';
const LS_CONNS = 'atria.conns.v1';
const LS_ACTIVE_CONN = 'atria.activeconn.v1';

const st = {
  settings: loadSettings(),
  chats: loadChats(),
  currentId: localStorage.getItem(LS_CURRENT) || null,
  sending: false,
  runText: '',
  live: null,
  streamEl: null,
  stackEl: null,
  run: null,
  cursorHolder: null,
  conns: [],
  activeConnId: '',
};
let dirtyChats = new Set();
let goneChats = new Set();
let flushTimer = 0;
let diskDirs = null;

function loadSettings() {
  let s;
  try { s = { ...DEFAULTS, ...JSON.parse(localStorage.getItem(LS_SETTINGS) || '{}') }; }
  catch { s = { ...DEFAULTS }; }
  // سقف سرویس 65536 — مقدارهای خراب قدیمی را خودکار درمان کن
  s.max_tokens = Math.min(65536, Math.max(256, Number(s.max_tokens) || 4096));
  return s;
}
function saveSettings() {
  localStorage.setItem(LS_SETTINGS, JSON.stringify(st.settings));
  els.modelChip.textContent = st.settings.model || '—';
  if (els.mpName) els.mpName.textContent = st.settings.model || '—';
}
function loadChats() {
  try { return JSON.parse(localStorage.getItem(LS_CHATS) || '[]'); } catch { return []; }
}
function saveChats(dirtiedId) {
  if (dirtiedId) dirtyChats.add(dirtiedId);
  try { localStorage.setItem(LS_CHATS, JSON.stringify(st.chats)); } catch {}
  scheduleFlush();
}
function scheduleFlush() {
  if (flushTimer) clearTimeout(flushTimer);
  flushTimer = setTimeout(flushChats, 250);
}
async function flushChats() {
  flushTimer = 0;
  if (!window.__atria || !window.__atria.chats_sync) return;
  const files = [];
  for (const c of st.chats) if (dirtyChats.has(c.id)) files.push({ id: c.id, data: JSON.stringify(c) });
  const remove = Array.from(goneChats);
  if (!files.length && !remove.length) return;
  dirtyChats.clear();
  goneChats.clear();
  try { await window.__atria.chats_sync({ files, remove }); } catch {}
}
function removeChatFile(id) { goneChats.add(id); dirtyChats.delete(id); scheduleFlush(); }
function wipeChats() {
  for (const c of st.chats) goneChats.add(c.id);
  dirtyChats.clear();
  st.chats = [];
  try { localStorage.setItem(LS_CHATS, '[]'); } catch {}
  scheduleFlush();
  newChat(true);
}
let saveSetTimer = 0;
function saveSettingsSoon() {
  if (saveSetTimer) clearTimeout(saveSetTimer);
  saveSetTimer = setTimeout(() => { saveSetTimer = 0; saveSettings(); }, 250);
}
let connListTimer = 0, connTabTimer = 0, saveConnsTimer = 0;
function saveConnsSoon() {
  if (saveConnsTimer) clearTimeout(saveConnsTimer);
  saveConnsTimer = setTimeout(() => { saveConnsTimer = 0; saveConns(); }, 250);
}
function updateConnListSoon() {
  if (connListTimer) clearTimeout(connListTimer);
  connListTimer = setTimeout(() => { connListTimer = 0; renderConnList(); }, 200);
}
function updateConnTabSoon() {
  if (connTabTimer) clearTimeout(connTabTimer);
  connTabTimer = setTimeout(() => { connTabTimer = 0; updateConnTab(); }, 200);
}

// restore conversations from ~/.atria/chats (disk is the source of truth)
async function restoreFromDisk() {
  if (!window.__atria || !window.__atria.chats_load) return;
  try {
    diskDirs = await window.__atria.dirs_info();
    if (!st.settings.workspace && diskDirs && diskDirs.workspace) {
      st.settings.workspace = String(diskDirs.workspace);
      if (els.wsVal) els.wsVal.value = st.settings.workspace;
      saveSettings();
    }
    renderStorage();
    const list = await window.__atria.chats_load();
    const disk = [];
    for (const raw of list || []) {
      try { const c = JSON.parse(raw); if (c && c.id && Array.isArray(c.messages)) disk.push(c); } catch {}
    }
    if (disk.length) {
      const byId = new Map();
      for (const c of [...st.chats, ...disk]) {
        const old = byId.get(c.id);
        const n = (c.messages || []).length;
        const nOld = old ? (old.messages || []).length : -1;
        if (n >= nOld) byId.set(c.id, c); // newest content wins
      }
      st.chats = Array.from(byId.values()).sort((a, b) => (b.createdAt || 0) - (a.createdAt || 0));
      if (!currentChat()) {
        st.currentId = st.chats[0] ? st.chats[0].id : null;
        if (st.currentId) localStorage.setItem(LS_CURRENT, st.currentId);
      }
      renderConvList();
      renderHistory();
    } else if (st.chats.length) {
      for (const c of st.chats) dirtyChats.add(c.id); // one-time migration LS -> disk
      flushChats();
    }
  } catch {}
}
function renderStorage() {
  if (!els.storageBox || !diskDirs) return;
  const rows = [
    ['گفتگوها', diskDirs.chats],
    ['ورک‌اسپیس', diskDirs.workspace],
    ['حافظه', diskDirs.memory],
  ];
  els.storageBox.innerHTML = '';
  for (const [k, v] of rows) {
    const r = document.createElement('div');
    r.className = 'st-row';
    r.innerHTML = '<b></b><span></span>';
    r.querySelector('b').textContent = k;
    r.querySelector('span').textContent = String(v || '');
    els.storageBox.appendChild(r);
  }
}
function currentChat() { return st.chats.find((c) => c.id === st.currentId) || null; }

/* ---------------- connections (multiple API endpoints per provider) ---------------- */

function normConn(c) {
  if (!c || typeof c !== 'object') return null;
  return {
    id: c.id || ('conn_' + Date.now() + '_' + Math.random().toString(36).slice(2, 7)),
    name: String(c.name || 'اتصال'),
    kind: c.kind === 'deepseek_web' ? 'deepseek_web' : c.kind === 'openai' ? 'openai' : 'anthropic',
    base: String(c.base || ''),
    key: String(c.key || ''),
    models: Array.isArray(c.models) ? c.models.map(String).filter(Boolean) : [],
  };
}
function loadConns() {
  try {
    const a = JSON.parse(localStorage.getItem(LS_CONNS) || '[]');
    if (Array.isArray(a)) return a.map(normConn).filter(Boolean);
  } catch {}
  return [];
}
function saveConns() { localStorage.setItem(LS_CONNS, JSON.stringify(st.conns)); }

// one-time migration: old single API settings become the first connection
function migrateConns() {
  if (localStorage.getItem(LS_CONNS)) return;
  const s = st.settings;
  const list = [];
  if (s.api_key || s.base_url || s.model) {
    const c = normConn({
      name: 'اتصال من',
      kind: s.api_kind,
      base: s.base_url,
      key: s.api_key,
      models: s.model ? [s.model] : [],
    });
    if (c) list.push(c);
  }
  st.conns = list;
  saveConns();
}

function activeConn() {
  return st.conns.find((c) => c.id === st.activeConnId) || st.conns[0] || null;
}
function setActiveConn(id) {
  st.activeConnId = id || '';
  localStorage.setItem(LS_ACTIVE_CONN, st.activeConnId);
  const c = activeConn();
  if (c && c.models.length && !c.models.includes(st.settings.model)) {
    st.settings.model = c.models[0];
    saveSettings();
  }
  updateModelPick();
  updateConnTab();
}
function kindLabel(kind) {
  return kind === 'deepseek_web' ? 'DeepSeek وب' : kind === 'openai' ? 'Chat Completions' : 'Messages API';
}

/* ------- connection manager modal ------- */

let connEditId = null;
let connSrv = [];
let menuShowConns = false;

function editConn() { return st.conns.find((c) => c.id === connEditId) || null; }

function openConns(id) {
  connEditId = id || (activeConn() ? activeConn().id : null);
  if (!connEditId && st.conns.length) connEditId = st.conns[0].id;
  connSrv = [];
  renderConnsModal();
  openModal(els.connsModal);
}

function renderConnsModal() {
  renderConnList();
  loadConnForm();
}

function renderConnList() {
  els.connList.innerHTML = '';
  if (!st.conns.length) {
    els.connList.innerHTML = '<div class="mem-empty">هنوز اتصالی نیست — «اتصال جدید» بزن</div>';
    return;
  }
  for (const c of st.conns) {
    const it = document.createElement('button');
    it.className = 'conn-card' + (c.id === connEditId ? ' active' : '');
    it.innerHTML = '<div class="cc-name"></div><div class="cc-meta"></div>';
    it.querySelector('.cc-name').textContent = c.name + (c.id === st.activeConnId ? ' ⭐' : '');
    it.querySelector('.cc-meta').textContent =
      kindLabel(c.kind) + ' · ' + c.models.length + ' مدل' + (c.key ? '' : ' · بدون کلید');
    it.onclick = () => { connEditId = c.id; connSrv = []; renderConnsModal(); };
    els.connList.appendChild(it);
  }
}

function loadConnForm() {
  const c = editConn();
  const has = !!c;
  els.connForm.classList.toggle('hidden', !has);
  if (!c) return;
  els.cfName.value = c.name;
  els.cfKind.value = c.kind;
  els.cfBase.value = c.base;
  els.cfKey.value = c.key;
  els.cfSetActive.textContent = c.id === st.activeConnId ? '⭐ اتصال فعال ✓' : '⭐ اتصال فعال';
  renderModelTags();
  renderSrvList();
  updateConnFormHints();
}

function commitConnForm() {
  const c = editConn();
  if (!c) return;
  c.name = els.cfName.value.trim() || 'اتصال';
  c.kind = els.cfKind.value;
  c.base = els.cfBase.value.trim();
  c.key = els.cfKey.value.trim();
  saveConnsSoon();
  updateConnListSoon();
  updateConnTabSoon();
  updateModelPick();
}

function updateConnFormHints() {
  const c = editConn();
  if (!c) return;
  const isDs = c.kind === 'deepseek_web';
  els.cfKeyLabel.textContent = isDs ? 'یوزر توکن دیپ‌سیک (از localStorage سایت)' : 'کلید API';
  els.cfKey.placeholder = isDs ? 'مقدار userToken — یا کل JSON آیتم userToken' : 'کلید همین اتصال';
  els.cfTokenGuide.classList.toggle('hidden', !isDs);
  const hint = els.cfHint;
  if (isDs) {
    hint.classList.remove('hidden');
    hint.textContent = '⚠ غیررسمی: با یوزر توکن حساب وب دیپ‌سیک کار می‌کند (نیازی به کلید API نیست) — با احتیاط و مسئولیت خودت.';
  } else if (c.base.includes('20128') || /omniroute/i.test(c.name)) {
    hint.classList.remove('hidden');
    hint.textContent = 'OmniRoute را اجرا کن (npm i -g omniroute) و کلید را از Dashboard → Endpoint → Registered Keys بساز.';
  } else {
    hint.classList.add('hidden');
    hint.textContent = '';
  }
}

function renderModelTags() {
  const c = editConn();
  els.cfTags.innerHTML = '';
  if (!c) return;
  if (!c.models.length) {
    els.cfTags.innerHTML = '<div class="mem-empty">هنوز مدلی برای این اتصال نگذاشته‌ای</div>';
    return;
  }
  for (const m of c.models) {
    const t = document.createElement('span');
    t.className = 'model-tag';
    t.innerHTML = '<b></b><button class="mt-x" title="حذف">✕</button>';
    t.querySelector('b').textContent = m;
    t.querySelector('.mt-x').onclick = () => {
      c.models = c.models.filter((x) => x !== m);
      if (st.settings.model === m) { st.settings.model = c.models[0] || ''; saveSettings(); }
      saveConns();
      renderModelTags();
      renderSrvList();
      updateConnTab();
      updateModelPick();
    };
    els.cfTags.appendChild(t);
  }
}

function addConnModel(name) {
  const c = editConn();
  const m = (name || '').trim();
  if (!c || !m) return;
  if (!c.models.includes(m)) c.models.push(m);
  saveConns();
  renderModelTags();
  renderSrvList();
  updateConnTab();
  updateModelPick();
}

async function fetchConnModels() {
  const c = editConn();
  if (!c) return;
  els.cfFetchModels.textContent = '…';
  try {
    connSrv = await window.__atria.list_models({
      base: els.cfBase.value.trim(),
      key: els.cfKey.value.trim(),
    });
    renderSrvList();
    toast(connSrv.length + ' مدل از سرور آمد — روی هرکدام کلیک کن تا اضافه شود', 'ok');
  } catch (e) {
    toast('دریافت فهرست مدل‌ها: ' + e, 'err');
  } finally {
    els.cfFetchModels.textContent = '🔎 دریافت از سرور';
  }
}

function renderSrvList() {
  const q = (els.cfSrvSearch.value || '').trim().toLowerCase();
  els.cfSrvWrap.classList.toggle('hidden', !connSrv.length);
  els.cfSrvList.innerHTML = '';
  const c = editConn();
  let shown = 0;
  for (const id of connSrv) {
    if (q && !id.toLowerCase().includes(q)) continue;
    shown++;
    if (shown > 400) break;
    const it = document.createElement('button');
    const picked = c && c.models.includes(id);
    it.className = 'srv-item' + (picked ? ' picked' : '');
    it.textContent = (picked ? '✓ ' : '＋ ') + id;
    it.onclick = () => {
      if (!c) return;
      if (c.models.includes(id)) c.models = c.models.filter((x) => x !== id);
      else c.models.push(id);
      saveConns();
      renderModelTags();
      renderSrvList();
      updateConnTab();
      updateModelPick();
    };
    els.cfSrvList.appendChild(it);
  }
}

function bindConns() {
  // template select options
  els.cfTemplate.innerHTML = '<option value="">— سفارشی —</option>' +
    Object.entries(CONN_TEMPLATES)
      .map(([id, t]) => `<option value="${id}">${t.label}</option>`).join('');
  els.connAdd.onclick = () => {
    const c = normConn({ name: 'اتصال ' + (st.conns.length + 1), kind: 'openai', base: '', key: '', models: [] });
    st.conns.push(c);
    saveConns();
    connEditId = c.id;
    connSrv = [];
    renderConnsModal();
    updateConnTab();
  };
  els.cfName.oninput = commitConnForm;
  els.cfName.onkeydown = (e) => { if (!(e.isComposing || e.keyCode === 229) && e.key === 'Enter') { e.preventDefault(); els.cfBase.focus(); } };
  els.cfBase.onkeydown = (e) => { if (!(e.isComposing || e.keyCode === 229) && e.key === 'Enter') { e.preventDefault(); els.cfKey.focus(); } };
  els.cfKey.onkeydown = (e) => { if (!(e.isComposing || e.keyCode === 229) && e.key === 'Enter') { e.preventDefault(); els.cfKey.blur(); } };
  els.cfName.onblur = els.cfBase.onblur = els.cfKey.onblur = () => { commitConnForm(); saveConns(); };
  els.cfKind.onchange = () => { commitConnForm(); updateConnFormHints(); };
  els.cfTemplate.onchange = () => {
    const t = CONN_TEMPLATES[els.cfTemplate.value];
    const c = editConn();
    if (!c || !t) return;
    c.kind = t.kind;
    c.base = t.base;
    if (/^اتصال/.test(c.name)) c.name = t.label;
    saveConns();
    loadConnForm();
    renderConnList();
    updateConnTab();
  };
  els.cfBase.oninput = commitConnForm;
  els.cfKey.oninput = commitConnForm;
  els.cfKeyEye.onclick = () => {
    const show = els.cfKey.type === 'password';
    els.cfKey.type = show ? 'text' : 'password';
    els.cfKeyEye.textContent = show ? '🙈' : '👁';
  };
  els.cfTokenGuide.onclick = () => openModal(els.tokenGuide);
  els.cfModelAdd.onclick = () => { addConnModel(els.cfModelInput.value); els.cfModelInput.value = ''; };
  els.cfModelInput.onkeydown = (e) => {
    if (e.isComposing || e.keyCode === 229) return;
    if (e.key === 'Enter') {
      e.preventDefault();
      addConnModel(els.cfModelInput.value);
      els.cfModelInput.value = '';
    }
  };
  els.cfFetchModels.onclick = fetchConnModels;
  els.cfSrvSearch.oninput = renderSrvList;
  els.cfSetActive.onclick = () => {
    const c = editConn();
    if (!c) return;
    setActiveConn(c.id);
    renderConnsModal();
    toast('اتصال فعال: ' + c.name, 'ok');
  };
  els.cfDelete.onclick = () => {
    const c = editConn();
    if (!c) return;
    if (!confirm('اتصال «' + c.name + '» حذف شود؟')) return;
    st.conns = st.conns.filter((x) => x.id !== c.id);
    saveConns();
    if (st.activeConnId === c.id) {
      st.activeConnId = st.conns[0] ? st.conns[0].id : '';
      localStorage.setItem(LS_ACTIVE_CONN, st.activeConnId);
    }
    connEditId = st.conns[0] ? st.conns[0].id : null;
    connSrv = [];
    renderConnsModal();
    updateConnTab();
    updateModelPick();
  };
  els.connsX.onclick = () => closeModal(els.connsModal);
  els.connsClose.onclick = () => closeModal(els.connsModal);
  els.connsModal.onclick = (e) => { if (e.target === els.connsModal) closeModal(els.connsModal); };
  els.btnManageConns.onclick = () => openConns(null);
}

function updateConnTab() {
  const c = activeConn();
  if (els.connSummary) {
    els.connSummary.innerHTML = '';
    if (!c) {
      els.connSummary.innerHTML = '<div class="mem-empty">اتصالی نیست — از «مدیریت کلیدها و اتصال‌ها» بساز</div>';
    } else {
      const d = document.createElement('div');
      d.className = 'conn-card active';
      d.innerHTML = '<div class="cc-name"></div><div class="cc-meta"></div>';
      d.querySelector('.cc-name').textContent = '⭐ ' + c.name;
      d.querySelector('.cc-meta').textContent =
        kindLabel(c.kind) + ' · ' + (c.base || 'بدون آدرس') + ' · ' + c.models.length + ' مدل · ' + (c.key ? 'کلید دارد' : 'بدون کلید');
      d.onclick = () => openConns(c.id);
      els.connSummary.appendChild(d);
    }
  }
  if (els.dsSearchRow) {
    els.dsSearchRow.classList.toggle('hidden', !(c && c.kind === 'deepseek_web'));
  }
}

/* ---------------- dom ---------------- */

let els = {};
function cacheEls() {
  els = {
    frame: $('#frame'), messages: $('#messages'), empty: $('#empty'),
    input: $('#input'), btnSend: $('#btnSend'), btnStop: $('#btnStop'),
    btnNew: $('#btnNew'), convList: $('#convList'),
    btnSettings: $('#btnSettings'), btnMemory: $('#btnMemory'), btnSide: $('#btnSide'),
    btnMin: $('#btnMin'), btnMax: $('#btnMax'), btnClose: $('#btnClose'),
    btnScroll: $('#btnScroll'),
    modelChip: $('#modelChip'),
    modelPick: $('#modelPick'), mpName: $('#mpName'), modelMenu: $('#modelMenu'),
    chipAgent: $('#chipAgent'), chipThink: $('#chipThink'), chipFiles: $('#chipFiles'),
    settingsModal: $('#settingsModal'), settingsX: $('#settingsX'), settingsClose: $('#settingsClose'),
    memoryModal: $('#memoryModal'), memoryX: $('#memoryX'), memoryClose: $('#memoryClose'),
    memList: $('#memList'), memClear: $('#memClear'), btnWipe: $('#btnWipe'),
    cmdk: $('#cmdk'), cmdkInput: $('#cmdkInput'), cmdkList: $('#cmdkList'), cmdkX: $('#cmdkX'),
    toasts: $('#toasts'),
    tempVal: $('#tempVal'), tempOut: $('#tempOut'),
    thinkVal: $('#thinkVal'), memVal: $('#memVal'), streamVal: $('#streamVal'),
    sysVal: $('#sysVal'), maxTokVal: $('#maxTokVal'),
    toolsVal: $('#toolsVal'), fileToolsVal: $('#fileToolsVal'), wsVal: $('#wsVal'),
    dsSearchRow: $('#dsSearchRow'), dsSearchVal: $('#dsSearchVal'),
    tokenGuide: $('#tokenGuide'), tokenGuideX: $('#tokenGuideX'), tokenGuideClose: $('#tokenGuideClose'),
    connSummary: $('#connSummary'), btnManageConns: $('#btnManageConns'),
    connsModal: $('#connsModal'), connsX: $('#connsX'), connsClose: $('#connsClose'),
    connList: $('#connList'), connAdd: $('#connAdd'), connForm: $('#connForm'),
    cfName: $('#cfName'), cfKind: $('#cfKind'), cfTemplate: $('#cfTemplate'),
    cfBase: $('#cfBase'), cfKey: $('#cfKey'), cfKeyEye: $('#cfKeyEye'),
    cfKeyLabel: $('#cfKeyLabel'), cfTokenGuide: $('#cfTokenGuide'), cfHint: $('#cfHint'),
    cfTags: $('#cfTags'), cfModelInput: $('#cfModelInput'), cfModelAdd: $('#cfModelAdd'),
    cfFetchModels: $('#cfFetchModels'), cfSrvWrap: $('#cfSrvWrap'),
    cfSrvSearch: $('#cfSrvSearch'), cfSrvList: $('#cfSrvList'),
    cfDelete: $('#cfDelete'), cfSetActive: $('#cfSetActive'),
    storageBox: $('#storageBox'), btnRevealAtria: $('#btnRevealAtria'),
  };
}

/* ---------------- helpers ---------------- */

function invokeCmd(name, args) {
  try {
    return window.__atria.invoke(name, args || {}).catch((e) => { toast(String(e), 'err'); });
  } catch (e) {
    toast('پل ارتباطی IPC آماده نیست: ' + e, 'err');
    return Promise.reject(e);
  }
}

function toast(msg, kind) {
  const t = document.createElement('div');
  t.className = 'toast' + (kind ? ' ' + kind : '');
  t.textContent = msg;
  els.toasts.appendChild(t);
  setTimeout(() => { t.classList.add('out'); setTimeout(() => t.remove(), 350); }, 2800);
}

function stars() {
  const box = $('#stars');
  for (let i = 0; i < 46; i++) {
    const s = document.createElement('i');
    const size = Math.random() * 2.2 + 0.8;
    s.style.cssText = `width:${size}px;height:${size}px;left:${Math.random() * 100}%;top:${Math.random() * 100}%;animation-delay:${(Math.random() * 4).toFixed(2)}s`;
    box.appendChild(s);
  }
}

function autosize() {
  els.input.style.height = 'auto';
  els.input.style.height = Math.min(180, els.input.scrollHeight) + 'px';
  els.btnSend.disabled = !els.input.value.trim();
}

function scrollBottom(instant) {
  if (instant) els.messages.scrollTop = els.messages.scrollHeight;
  else els.messages.scrollTo({ top: els.messages.scrollHeight, behavior: 'smooth' });
}

/* ---------------- settings ---------------- */

function bindSettings() {
  els.tempVal.value = st.settings.temp;
  els.tempOut.textContent = Number(st.settings.temp).toFixed(1);
  els.thinkVal.checked = !!st.settings.thinking;
  els.memVal.checked = !!st.settings.mem;
  els.streamVal.checked = !!st.settings.stream;
  els.tempVal.oninput = () => {
    st.settings.temp = Number(els.tempVal.value);
    els.tempOut.textContent = st.settings.temp.toFixed(1);
    saveSettingsSoon();
  };
  els.thinkVal.onchange = () => { st.settings.thinking = els.thinkVal.checked; saveSettings(); syncChips(); };
  els.memVal.onchange = () => { st.settings.mem = els.memVal.checked; saveSettings(); };
  els.streamVal.onchange = () => { st.settings.stream = els.streamVal.checked; saveSettings(); };

  els.sysVal.value = st.settings.system || '';
  els.maxTokVal.value = st.settings.max_tokens || 4096;
  els.toolsVal.checked = !!st.settings.tools_enabled;
  els.fileToolsVal.checked = !!st.settings.file_tools;
  els.wsVal.value = st.settings.workspace || '';
  // فیلدهای متنی: ذخیره با تأخیر (تایپ بدون لگ) + نهایی‌سازی در blur
  els.sysVal.oninput = () => { st.settings.system = els.sysVal.value; saveSettingsSoon(); };
  els.sysVal.onblur = saveSettings;
  els.maxTokVal.onchange = () => {
    let v = Math.round(Number(els.maxTokVal.value));
    if (!Number.isFinite(v)) v = 4096;
    v = Math.min(65536, Math.max(256, v));
    st.settings.max_tokens = v;
    els.maxTokVal.value = v;
    saveSettings();
  };
  els.toolsVal.onchange = () => { st.settings.tools_enabled = els.toolsVal.checked; saveSettings(); syncChips(); };
  els.fileToolsVal.onchange = () => { st.settings.file_tools = els.fileToolsVal.checked; saveSettings(); syncChips(); };
  els.dsSearchVal.checked = !!st.settings.ds_search;
  els.dsSearchVal.onchange = () => { st.settings.ds_search = els.dsSearchVal.checked; saveSettings(); };
  // ورک‌اسپیس: حین تایپ trim نشود (مکان‌نما نپرد) — فقط در blur
  els.wsVal.oninput = () => { st.settings.workspace = els.wsVal.value; saveSettingsSoon(); };
  els.wsVal.onblur = () => {
    els.wsVal.value = els.wsVal.value.trim();
    st.settings.workspace = els.wsVal.value;
    saveSettings();
  };
  if (els.btnRevealAtria) els.btnRevealAtria.onclick = () => {
    if (window.__atria && window.__atria.reveal_dir) window.__atria.reveal_dir().catch(() => {});
  };
}

function openModal(m) { m.classList.remove('hidden'); }
function closeModal(m) { m.classList.add('hidden'); }

/* ---------------- conversations ---------------- */

function renderConvList() {
  els.convList.innerHTML = '';
  if (!st.chats.length) {
    const e = document.createElement('div');
    e.className = 'mem-empty';
    e.textContent = 'هنوز گفتگویی نیست';
    els.convList.appendChild(e);
    return;
  }
  for (const c of st.chats) {
    const item = document.createElement('div');
    item.className = 'conv' + (c.id === st.currentId ? ' active' : '');
    const t = document.createElement('div');
    t.className = 'conv-title';
    t.textContent = c.title;
    const d = document.createElement('div');
    d.className = 'conv-date';
    d.textContent = new Date(c.createdAt).toLocaleDateString('fa-IR');
    const del = document.createElement('button');
    del.className = 'conv-del';
    del.title = 'حذف';
    del.textContent = '✕';
    del.onclick = (e) => { e.stopPropagation(); deleteChat(c.id); };
    item.appendChild(t); item.appendChild(d); item.appendChild(del);
    item.onclick = () => switchChat(c.id);
    els.convList.appendChild(item);
  }
}

function newChat(silent) {
  if (st.sending) return toast('ابتدا تولید را متوقف کن', 'warn');
  const chat = { id: 'c' + Date.now() + Math.random().toString(36).slice(2, 6), title: 'گفتگوی جدید', createdAt: Date.now(), messages: [] };
  st.chats.unshift(chat);
  st.currentId = chat.id;
  saveChats(chat.id);
  localStorage.setItem(LS_CURRENT, chat.id);
  renderConvList();
  renderHistory();
  scrollBottom(true);
  if (!silent) els.input.focus();
}

function switchChat(id) {
  if (st.sending) return toast('ابتدا تولید را متوقف کن', 'warn');
  st.currentId = id;
  localStorage.setItem(LS_CURRENT, id);
  renderConvList();
  renderHistory();
  scrollBottom(true);
  closeModal(els.cmdk);
}

function deleteChat(id) {
  if (st.sending) return toast('ابتدا تولید را متوقف کن', 'warn');
  const c = st.chats.find((x) => x.id === id);
  if (!confirm('گفتگوی «' + (c ? c.title : 'این گفتگو') + '» حذف شود؟')) return;
  st.chats = st.chats.filter((x) => x.id !== id);
  removeChatFile(id);
  saveChats();
  if (st.currentId === id) {
    st.currentId = st.chats[0] ? st.chats[0].id : null;
    if (!st.currentId) newChat(true);
    else localStorage.setItem(LS_CURRENT, st.currentId);
  }
  renderConvList();
  renderHistory();
}

/* ---------------- rendering ---------------- */

function avatarEl(role) {
  const a = document.createElement('div');
  a.className = 'avatar ' + (role === 'user' ? 'you' : 'atri');
  if (role === 'user') a.textContent = 'شما';
  else { const img = document.createElement('img'); img.src = 'logo.svg'; img.alt = ''; a.appendChild(img); }
  return a;
}

function historyPanelEl(flow) {
  const box = document.createElement('div');
  box.className = 'think-box run-panel';
  const head = document.createElement('button');
  head.className = 'think-head';
  head.innerHTML = '<span class="think-orb"><i></i></span>' +
    '<span class="think-title">فرایند تفکر</span>' +
    '<span class="think-now"></span><span class="chev">▾</span>';
  head.querySelector('.think-now').textContent = (flow.length || 0) + ' گام';
  head.onclick = () => box.classList.toggle('open');
  const body = document.createElement('div');
  body.className = 'think-body';
  const flowEl = document.createElement('div');
  flowEl.className = 'flow';
  for (const it of flow) {
    if (it.k === 't') {
      const el = document.createElement('div');
      el.className = 'step';
      el.innerHTML = '<div class="step-title"></div><div class="step-text"></div>';
      el.querySelector('.step-title').textContent = it.title || 'اندیشیدن…';
      el.querySelector('.step-text').textContent = it.text || '';
      flowEl.appendChild(el);
    } else if (it.k === 'p') {
      const card = toolCardEl(it.title, it.name, null, false);
      card.classList.remove('pending');
      card.classList.add(it.ok ? 'done' : 'failed');
      card.querySelector('.tool-state').textContent =
        it.ok ? '✓ انجام شد' : (it.ok === false ? '✕ خطا' : '');
      const inEl = card.querySelector('.tool-in');
      if (it.in) { inEl.classList.remove('hidden'); inEl.textContent = it.in; }
      if (it.out) {
        const out = card.querySelector('.tool-out');
        out.classList.remove('hidden');
        out.textContent = it.out;
      }
      flowEl.appendChild(card);
    } else {
      const r = document.createElement('div');
      r.className = 'round-chip';
      r.textContent = it.text || '↻ مرحلهٔ ابزار';
      flowEl.appendChild(r);
    }
  }
  body.appendChild(flowEl);
  box.appendChild(head);
  box.appendChild(body);
  return box;
}

function bubbleEl(role, text, flow) {
  const wrap = document.createElement('div');
  wrap.className = 'msg ' + (role === 'user' ? 'user' : 'ai');
  wrap.appendChild(avatarEl(role));
  const stack = document.createElement('div');
  stack.className = 'stack';
  const b = document.createElement('div');
  b.className = 'bubble';
  const md = document.createElement('div');
  md.className = 'md';
  b.appendChild(md);
  if (flow && flow.length && role !== 'user') stack.appendChild(historyPanelEl(flow));
  stack.appendChild(b);
  wrap.appendChild(stack);
  // render markdown into md container
  (window.md.renderAll)(md, text || '…');
  return wrap;
}

function renderHistory() {
  const chat = currentChat();
  els.messages.innerHTML = '';
  els.empty.classList.toggle('hidden', !!(chat && chat.messages.length));
  els.empty.classList.toggle('off', !!(chat && chat.messages.length));
  if (!chat) return;
  for (const m of chat.messages) {
    els.messages.appendChild(
      bubbleEl(m.role === 'user' ? 'user' : 'ai', m.plain || m.text || '…', m.flow));
  }
  scrollBottom(true);
}

/* ---------------- wire helpers ---------------- */

// Extract visible text from either a stored message ({plain|text}) or a
// wire message from Rust ({content:[{type:'text',...}]}).
function msgText(m) {
  if (!m) return '';
  if (typeof m === 'string') return m;
  if (typeof m.plain === 'string' && m.plain) return m.plain;
  if (typeof m.text === 'string' && m.text) return m.text;
  if (Array.isArray(m.content)) {
    return m.content
      .filter((b) => b && b.type === 'text')
      .map((b) => b.text || '')
      .join('\n');
  }
  return '';
}

/* ---------------- send flow ---------------- */

function send() {
  const text = els.input.value.trim();
  if (!text || st.sending) return;
  const chat = currentChat();
  if (!chat) { newChat(true); return send(); }

  const conn = activeConn();
  if (!conn) {
    openConns(null);
    return toast('ابتدا یک اتصال با کلید بساز', 'warn');
  }
  if (!conn.key) {
    openConns(conn.id);
    return toast('کلید/توکن اتصال «' + conn.name + '» را وارد کن', 'warn');
  }

  chat.messages.push({ role: 'user', text, plain: text, ts: Date.now() });
  if (chat.title === 'گفتگوی جدید') {
    const t1 = text.replace(/\s+/g, ' ').trim();
    chat.title = t1.slice(0, 42) + (t1.length > 42 ? '…' : '');
    renderConvList();
  }
  saveChats(chat.id);
  els.empty.classList.add('hidden', 'off');
  els.messages.appendChild(bubbleEl('user', text));
  els.input.value = '';
  autosize();
  scrollBottom();

  startTurn();
}

function startTurn() {
  st.sending = true;
  st.runText = '';
  st.streamEl = null;
  st.stackEl = null;
  st.run = newRun();
  st.live = null;
  liveMdEl(); // create the live ai message shell
  startBlink();
  els.btnSend.classList.add('hidden');
  els.btnStop.classList.remove('hidden');

  const chat = currentChat();
  const history = chat.messages.map((m) => ({
    role: m.role === 'user' ? 'user' : 'assistant',
    content: [{ type: 'text', text: msgText(m) }],
  }));
  if (!window.__atria || !window.__atria.chat_send) {
    return failRun('پل ارتباطی IPC آماده نیست — برنامه را دوباره باز کن');
  }
  const conn = activeConn();
  window.__atria.chat_send({
    payload: {
      api_key: conn ? conn.key : '',
      base_url: conn ? conn.base : '',
      model: st.settings.model || (conn && conn.models[0]) || 'Atria-Dawn-Preview',
      max_tokens: Math.min(65536, Math.max(256, Number(st.settings.max_tokens) || 4096)),
      temperature: Number(st.settings.temp),
      system: st.settings.system || '',
      tools_enabled: !!st.settings.tools_enabled,
      stream: st.settings.stream !== false,
      kind: conn ? conn.kind : 'anthropic',
      file_tools: !!st.settings.file_tools,
      workspace: st.settings.workspace || '',
      thinking: !!st.settings.thinking,
      web_search: !!st.settings.ds_search,
      messages: history,
    },
  }).catch((e) => failRun(e && e.message ? e.message : String(e)));
}

function retryLast() {
  if (st.sending) return;
  if (st.failedWrap && st.failedWrap.parentNode) st.failedWrap.parentNode.removeChild(st.failedWrap);
  st.failedWrap = null;
  if (st.streamEl) {
    const msg = st.streamEl.closest('.msg');
    if (msg && msg.parentNode) msg.parentNode.removeChild(msg);
  }
  startTurn();
}

function stop() { window.__atria.chat_stop(); }

/* ------- live pieces: run panel (DeepSeek-style think + tools timeline) ------- */

const FA_STEP_START = /(خب|حالا|اکنون|بیایید|بریم|برویم|ابتدا|اولین|سپس|در نهایت|می‌خواهم|میخوام|باید|لازم|الان|هم‌اکنون|now|next|let'?s|first|then|i'?ll|i will|time to|going to|okay|right|alright)/i;
const FA_TOOL = {
  calculator: 'ماشین‌حساب', current_time: 'ساعت و تاریخ', remember: 'ذخیره در حافظه',
  recall: 'جست‌وجوی حافظه', list_files: 'فهرست فایل‌ها', read_file: 'خواندن فایل',
  write_file: 'نوشتن فایل', edit_file: 'ویرایش فایل', bash: 'اجرای دستور',
};
function faTool(name) { return FA_TOOL[name] || 'اجرای ابزار'; }
function fmtN(n) { return n > 999 ? (n / 1000).toFixed(1) + 'k' : String(n); }

function fmtInput(input) {
  let s;
  try { s = JSON.stringify(input ?? {}, null, 2); } catch { s = String(input); }
  if (s.length > 1200) s = s.slice(0, 1200) + '\n… (' + fmtN(s.length) + ' کاراکتر)';
  return s;
}

// «هر فکر بگوید الآن چه می‌کند» — آخرین جملهٔ آغازگر («خب بریم برای نوشتن فایل»…)
// یا اولین جمله، خلاصه‌شده به‌عنوان عنوان گام.
function pickStepTitle(text) {
  const clean = (text || '').replace(/[#*`>]/g, ' ').replace(/\s+/g, ' ').trim();
  if (!clean) return 'در حال اندیشیدن…';
  const parts = clean.split(/(?<=[.!?؟])\s+/);
  let chosen = '';
  for (const s of parts) if (FA_STEP_START.test(s)) chosen = s;
  if (!chosen) chosen = parts[0] || clean;
  chosen = chosen.trim();
  if (chosen.length > 58) chosen = chosen.slice(0, 58) + '…';
  return chosen;
}

function newRun() {
  return { panel: null, flow: null, cards: new Map(), thinkStep: null, thinkTn: null,
           thinkLen: 0, thinkKind: '', log: [] };
}

function ensurePanel() {
  if (!st.run) st.run = newRun();
  const run = st.run;
  if (run.panel && run.panel.isConnected) return run.panel;
  const box = document.createElement('div');
  box.className = 'think-box run-panel has open';
  box.innerHTML =
    '<button class="think-head"><span class="think-orb"><i></i></span>' +
    '<span class="think-title">در حال اندیشیدن…</span>' +
    '<span class="think-now"></span>' +
    '<span class="think-wave"><i></i><i></i><i></i></span>' +
    '<span class="chev">▾</span></button>' +
    '<div class="think-body"><div class="flow"></div></div>';
  box.querySelector('.think-head').onclick = () => box.classList.toggle('open');
  const stack = st.stackEl;
  const bub = stack.querySelector('.bubble');
  if (bub) stack.insertBefore(box, bub); else stack.appendChild(box);
  run.panel = box;
  run.flow = box.querySelector('.flow');
  return box;
}

function setNow(title) {
  const run = st.run;
  if (!run.panel) return;
  run.panel.querySelector('.think-now').textContent = title ? 'اکنون: ' + title : '';
}

function collapsePanel() {
  const run = st.run;
  if (!run || !run.panel) return;
  run.panel.classList.remove('has', 'open');
  run.panel.querySelector('.think-title').textContent = 'فرایند تفکر';
  run.panel.querySelector('.think-now').textContent =
    run.log.length ? run.log.length + ' گام' : '';
}

function pushLog(item) { if (st.run.log.length < 48) st.run.log.push(item); }

function addChip(text) {
  ensurePanel();
  const r = document.createElement('div');
  r.className = 'round-chip';
  r.textContent = text;
  st.run.flow.appendChild(r);
  pushLog({ k: 'r', text: text });
  scrollBottom(true);
}

function addThinkStep() {
  const run = st.run;
  ensurePanel();
  const el = document.createElement('div');
  el.className = 'step';
  el.innerHTML = '<div class="step-title"></div><div class="step-text"></div>';
  run.flow.appendChild(el);
  const item = { k: 't', title: '', text: '' };
  pushLog(item);
  el._item = item;
  run.thinkStep = el;
  run.thinkTn = null;
  run.thinkLen = 0;
  run.thinkKind = 'think';
  return el;
}

function onThinking(delta) {
  if (!st.settings.thinking) return;
  closeLive();
  const run = st.run || (st.run = newRun());
  let el = run.thinkStep;
  if (!el || run.thinkKind !== 'think' || run.thinkLen > 300) el = addThinkStep();
  const item = el._item;
  if (!run.thinkTn) {
    run.thinkTn = document.createTextNode('');
    el.querySelector('.step-text').appendChild(run.thinkTn);
  }
  run.thinkTn.textContent += delta;
  run.thinkLen += delta.length;
  if (item.text.length < 2200) item.text += delta;
  if (run.thinkLen > 70 || /[.!?؟\n]/.test(delta)) {
    const t = pickStepTitle(item.text);
    if (t && t !== item.title) {
      item.title = t;
      el.querySelector('.step-title').textContent = t;
      setNow(t);
    }
  }
  scrollBottom(true);
}

function onText(delta) {
  st.runText += delta;
  if (st.cursorHolder) stopBlink();
  if (st.run) { st.run.thinkStep = null; st.run.thinkKind = 'text'; }
  ensureLive().push(delta);
  scrollBottom(true);
}

function toolCardEl(label, name, input, pending) {
  const card = document.createElement('div');
  card.className = 'tool-card' + (pending ? ' pending' : '');
  card.innerHTML =
    '<div class="tool-head"><span class="tool-ic">⚙️</span><span class="tool-label"></span>' +
    '<span class="tool-chip"></span><span class="tool-state"></span></div>' +
    '<div class="tool-bar"><i></i></div>' +
    '<div class="tool-io"><div class="tool-in"></div><div class="tool-out hidden"></div></div>';
  card.querySelector('.tool-label').textContent = label || name || 'ابزار';
  card.querySelector('.tool-chip').textContent = name || '';
  card.querySelector('.tool-state').textContent = pending ? 'در حال آماده‌سازی…' : 'در حال اجرا…';
  const inEl = card.querySelector('.tool-in');
  if (input == null) inEl.classList.add('hidden');
  else inEl.textContent = fmtInput(input);
  return card;
}

function toolKey(id, name) {
  const i = id ? String(id) : '';
  return i || (name ? 'call_' + name : '');
}

function findCard(id) {
  const run = st.run;
  if (!run || !run.flow) return null;
  const key = id ? String(id) : '';
  if (key && run.cards.has(key)) return run.cards.get(key);
  const open = run.flow.querySelectorAll('.tool-card:not(.done):not(.failed)');
  return open.length ? open[open.length - 1] : null;
}

function onToolPending(id, name) {
  closeLive();
  stopBlink();
  const run = st.run || (st.run = newRun());
  ensurePanel();
  run.thinkStep = null;
  run.thinkKind = 'tool';
  const label = faTool(name);
  const card = toolCardEl(label, name, null, true);
  run.flow.appendChild(card);
  card._key = toolKey(id, name);
  card._chars = 0;
  if (card._key) run.cards.set(card._key, card);
  const item = { k: 'p', title: label, name: name, ok: null, in: '', out: '' };
  pushLog(item);
  card._item = item;
  setNow(label);
  scrollBottom(true);
}

function onToolArgs(id, n) {
  const card = findCard(id);
  if (!card) return;
  card._chars = (card._chars || 0) + (Number(n) || 0);
  card.querySelector('.tool-state').textContent =
    'در حال نوشتن محتوا… ' + fmtN(card._chars) + ' کاراکتر';
}

function onToolStart(id, name, label, input) {
  closeLive();
  stopBlink();
  const run = st.run || (st.run = newRun());
  ensurePanel();
  run.thinkStep = null;
  run.thinkKind = 'tool';
  let card = findCard(id);
  if (!card) {
    card = toolCardEl(label || faTool(name), name, null, true);
    run.flow.appendChild(card);
    card._key = toolKey(id, name);
    if (card._key) run.cards.set(card._key, card);
    const item = { k: 'p', title: label || faTool(name), name: name, ok: null, in: '', out: '' };
    pushLog(item);
    card._item = item;
  }
  card.classList.remove('pending');
  if (label) card.querySelector('.tool-label').textContent = label;
  if (name) card.querySelector('.tool-chip').textContent = name;
  card.querySelector('.tool-state').textContent = 'در حال اجرا…';
  const inEl = card.querySelector('.tool-in');
  inEl.classList.remove('hidden');
  const shown = fmtInput(input);
  inEl.textContent = shown;
  if (card._item) card._item.in = shown.slice(0, 500);
  setNow(label || faTool(name));
  scrollBottom(true);
}

function onToolEnd(id, ok, output) {
  closeLive();
  const card = findCard(id);
  if (!card) return;
  card.classList.remove('pending');
  card.classList.add(ok ? 'done' : 'failed');
  card.querySelector('.tool-state').textContent = ok ? '✓ انجام شد' : '✕ خطا';
  const out = card.querySelector('.tool-out');
  out.classList.remove('hidden');
  out.textContent = (output || '').slice(0, 4000);
  if (card._item) {
    card._item.ok = ok;
    card._item.out = (output || '').slice(0, 500);
  }
  setNow('بررسی نتیجه');
  scrollBottom(true);
}

function startBlink() {
  stopBlink();
  const holder = document.createElement('div');
  holder.className = 'para';
  const c = document.createElement('span');
  c.className = 'cursor';
  holder.appendChild(c);
  st.liveEl().appendChild(holder);
  st.cursorHolder = holder;
}
function stopBlink() {
  if (st.cursorHolder && st.cursorHolder.parentNode) st.cursorHolder.parentNode.removeChild(st.cursorHolder);
  st.cursorHolder = null;
}

function liveMdEl() {
  if (!st.streamEl) {
    const wrap = document.createElement('div');
    wrap.className = 'msg ai';
    wrap.appendChild(avatarEl('ai'));
    const stack = document.createElement('div');
    stack.className = 'stack';
    const bub = document.createElement('div');
    bub.className = 'bubble streaming';
    const md = document.createElement('div');
    md.className = 'md';
    bub.appendChild(md);
    stack.appendChild(bub);
    wrap.appendChild(stack);
    els.messages.appendChild(wrap);
    st.streamEl = bub;
    st.stackEl = stack;
    st.mdEl = md;
    scrollBottom();
  }
  return st.mdEl;
}

st.liveEl = function () { return liveMdEl(); };

function ensureLive() {
  if (!st.live) {
    stopBlink();
    const sec = document.createElement('div');
    sec.className = 'stream-answer';
    liveMdEl().appendChild(sec);
    st.live = md.createLiveStream(sec);
  }
  return st.live;
}
function closeLive() {
  if (st.live) { st.live.done(); st.live = null; }
}

function failRun(msg) {
  stopBlink();
  closeLive();
  collapsePanel();
  st.sending = false;
  els.btnSend.classList.remove('hidden');
  els.btnStop.classList.add('hidden');
  if (st.streamEl) {
    st.streamEl.classList.remove('streaming');
    const e = document.createElement('div');
    e.className = 'err-card';
    e.innerHTML = '<div class="err-text"></div><button class="btn primary retry-btn">🔄 تلاش مجدد</button>';
    e.querySelector('.err-text').textContent = 'خطا: ' + msg;
    e.querySelector('.retry-btn').onclick = retryLast;
    st.stackEl.appendChild(e);
    st.failedWrap = e;
    scrollBottom();
  }
  toast('خطا: ' + msg, 'err');
}

function finishRun(newMessages, finalText) {
  stopBlink();
  closeLive();
  collapsePanel();
  st.sending = false;
  els.btnSend.classList.remove('hidden');
  els.btnStop.classList.add('hidden');
  if (st.streamEl) st.streamEl.classList.remove('streaming');
  const chat = currentChat();
  const plain = finalText && finalText.trim() ? finalText : st.runText || '(پاسخ خالی)';
  const flow = (st.run && st.run.log.length) ? st.run.log : null;
  const pushed = [];
  if (newMessages && newMessages.length) {
    for (const m of newMessages) {
      const t = msgText(m);
      if (!t.trim()) continue; // skip tool/think-only messages
      chat.messages.push({ role: m.role === 'user' ? 'user' : 'ai', text: t, plain: t, ts: Date.now() });
      pushed.push(chat.messages[chat.messages.length - 1]);
    }
  } else {
    chat.messages.push({ role: 'ai', text: st.runText || plain, plain, ts: Date.now() });
    pushed.push(chat.messages[chat.messages.length - 1]);
  }
  for (let i = pushed.length - 1; i >= 0; i--) {
    if (pushed[i].role === 'ai') { if (flow) pushed[i].flow = flow; break; }
  }
  saveChats(chat.id);
  scrollBottom();
}

/* ---------------- events from Rust ---------------- */

function listenEvents() {
  const t = window.__atria;
  t.listen('atria:thinking', (e) => onThinking(e.payload.delta));
  t.listen('atria:text', (e) => onText(e.payload.delta));
  t.listen('atria:round', (e) => {
    if (e.payload.round > 1) {
      closeLive();
      ensurePanel();
      st.run.thinkStep = null;
      addChip('↻ مرحلهٔ ابزار ' + e.payload.round);
    }
  });
  t.listen('atria:tool_pending', (e) => onToolPending(e.payload.id, e.payload.name));
  t.listen('atria:tool_args', (e) => onToolArgs(e.payload.id, e.payload.n || 0));
  t.listen('atria:tool_start', (e) =>
    onToolStart(e.payload.id, e.payload.name, e.payload.label, e.payload.input));
  t.listen('atria:tool_end', (e) => onToolEnd(e.payload.id, e.payload.ok, e.payload.output));
  t.listen('atria:retry', (e) => {
    closeLive();
    ensurePanel();
    addChip('↻ تلاش مجدد (' + e.payload.attempt + ' از ۳)…');
  });
  t.listen('atria:done', (e) => finishRun(e.payload.new_messages, e.payload.final_text));
  t.listen('atria:error', (e) => failRun(e.payload.message || 'unknown'));
  t.listen('atria:stopped', () => { failRun('متوقف شد'); toast('تولید متوقف شد', 'warn'); });
}

/* ---------------- memory modal ---------------- */

async function openMemory() {
  openModal(els.memoryModal);
  els.memList.innerHTML = '<div class="mem-empty">…</div>';
  try {
    const notes = await window.__atria.memory_list();
    els.memList.innerHTML = '';
    if (!notes || !notes.length) {
      els.memList.innerHTML = '<div class="mem-empty">حافظه خالی است</div>';
      return;
    }
    for (const n of notes) {
      const el = document.createElement('div');
      el.className = 'mem-note';
      el.innerHTML = '<div class="mem-title"></div><div class="mem-content"></div><div class="mem-ts"></div>';
      el.querySelector('.mem-title').textContent = n.title;
      el.querySelector('.mem-content').textContent = n.content;
      el.querySelector('.mem-ts').textContent = n.ts;
      els.memList.appendChild(el);
    }
  } catch {
    els.memList.innerHTML = '<div class="mem-empty">خطا در بارگذاری حافظه</div>';
  }
}

/* ---------------- cmd-k ---------------- */

function openCmdk() {
  openModal(els.cmdk);
  els.cmdkInput.value = '';
  renderCmdkList('');
  els.cmdkInput.focus();
}
function renderCmdkList(q) {
  const list = st.chats.filter((c) => !q || c.title.includes(q));
  els.cmdkList.innerHTML = '';
  if (!list.length) {
    els.cmdkList.innerHTML = '<div class="mem-empty">چیزی پیدا نشد</div>';
    return;
  }
  for (const c of list) {
    const item = document.createElement('div');
    item.className = 'cmdk-item' + (c.id === st.currentId ? ' active' : '');
    item.textContent = c.title;
    item.onclick = () => switchChat(c.id);
    els.cmdkList.appendChild(item);
  }
}

/* ---------------- boot ---------------- */


/* ---------------- model picker + mode chips (Claude-like bar) ---------------- */

function updateModelPick() {
  els.mpName.textContent = st.settings.model || '—';
}

function closeModelMenu() { els.modelMenu.classList.add('hidden'); }

function renderModelMenu() {
  els.modelMenu.innerHTML = '';
  const tools = document.createElement('div');
  tools.className = 'mm-tools';
  tools.innerHTML =
    '<input class="mm-search" id="mmSearch" placeholder="جست‌وجوی مدل…" spellcheck="false">' +
    '<button class="mm-fetch" id="mmManage" title="مدیریت اتصال‌ها و مدل‌ها">⚙︎</button>';
  els.modelMenu.appendChild(tools);

  const c = activeConn();
  const head = document.createElement('div');
  head.className = 'mm-head mm-conn-head';
  if (menuShowConns) {
    head.textContent = 'انتخاب اتصال:';
    els.modelMenu.appendChild(head);
    if (!st.conns.length) {
      const e2 = document.createElement('div');
      e2.className = 'mem-empty';
      e2.textContent = 'اتصالی نیست';
      els.modelMenu.appendChild(e2);
    }
    for (const cc of st.conns) {
      const it = document.createElement('button');
      it.className = 'mm-item' + (c && cc.id === c.id ? ' active' : '');
      it.innerHTML = '<span class="mm-model"></span><span class="mm-src"></span>';
      it.querySelector('.mm-model').textContent = cc.name;
      it.querySelector('.mm-src').textContent =
        kindLabel(cc.kind) + ' · ' + cc.models.length + ' مدل' + (cc.key ? '' : ' · بدون کلید');
      it.onclick = () => {
        setActiveConn(cc.id);
        menuShowConns = false;
        renderModelMenu();
        toast('اتصال: ' + cc.name, 'ok');
      };
      els.modelMenu.appendChild(it);
    }
  } else {
    head.innerHTML = '<span class="mm-conn-name"></span><button class="mm-switch" title="تعویض اتصال">↻ تعویض اتصال</button>';
    head.querySelector('.mm-conn-name').textContent = 'اتصال: ' + (c ? c.name : '—');
    head.querySelector('.mm-switch').onclick = (e) => {
      e.stopPropagation();
      menuShowConns = true;
      renderModelMenu();
    };
    els.modelMenu.appendChild(head);
    const models = c ? c.models : [];
    if (!models.length) {
      const e2 = document.createElement('div');
      e2.className = 'mem-empty';
      e2.textContent = 'برای این اتصال هنوز مدلی تعیین نکرده‌ای';
      els.modelMenu.appendChild(e2);
    }
    for (const m of models) {
      const it = document.createElement('button');
      it.className = 'mm-item' + (st.settings.model === m ? ' active' : '');
      it.innerHTML = '<span class="mm-model"></span><span class="mm-src"></span>';
      it.querySelector('.mm-model').textContent = m;
      it.querySelector('.mm-src').textContent = c ? c.name : '';
      it.onclick = () => chooseModel(m);
      els.modelMenu.appendChild(it);
    }
    const addBtn = document.createElement('button');
    addBtn.className = 'mm-item mm-custom';
    addBtn.textContent = '＋ افزودن/حذف مدل‌های این اتصال…';
    addBtn.onclick = () => { closeModelMenu(); openConns(c ? c.id : null); };
    els.modelMenu.appendChild(addBtn);
  }
  const manage = document.createElement('button');
  manage.className = 'mm-item mm-custom';
  manage.textContent = '⚙︎ مدیریت اتصال‌ها و مدل‌ها…';
  manage.onclick = () => { closeModelMenu(); openConns(null); };
  els.modelMenu.appendChild(manage);

  tools.querySelector('#mmSearch').oninput = (e) => {
    const q = e.target.value.trim().toLowerCase();
    els.modelMenu.querySelectorAll('.mm-item').forEach((it) => {
      const hit = !q || it.textContent.toLowerCase().includes(q);
      it.classList.toggle('hidden', !hit);
    });
  };
  tools.querySelector('#mmManage').onclick = (e) => {
    e.stopPropagation();
    closeModelMenu();
    openConns(null);
  };
}

function chooseModel(model) {
  st.settings.model = model;
  saveSettings();
  updateModelPick();
  closeModelMenu();
  toast('مدل: ' + model, 'ok');
}

function syncChips() {
  els.chipAgent.classList.toggle('on', !!st.settings.tools_enabled);
  els.chipThink.classList.toggle('on', !!st.settings.thinking);
  els.chipFiles.classList.toggle('on', !!st.settings.file_tools);
}

function boot() {
  cacheEls();
  st.conns = loadConns();
  migrateConns();
  st.activeConnId = localStorage.getItem(LS_ACTIVE_CONN) || (st.conns[0] ? st.conns[0].id : '');
  stars();
  bindSettings();
  saveSettings();
  renderConvList();
  renderHistory();

  els.btnNew.onclick = () => { if (!st.sending) newChat(); };
  els.btnSettings.onclick = () => openModal(els.settingsModal);
  els.btnMemory.onclick = openMemory;
  els.btnSide.onclick = () => els.frame.classList.toggle('side-hidden');
  els.btnMin.onclick = () => invokeCmd('minimize_win');
  els.btnMax.onclick = () => invokeCmd('maximize_win');
  els.btnClose.onclick = () => invokeCmd('close_win');
  // جلوگیری از بلعیده‌شدن کلیک توسط ناحیهٔ drag تایتل‌بار
  document.querySelectorAll('.win-controls, .titlebar-actions, .brand').forEach((el) =>
    el.addEventListener('mousedown', (e) => e.stopPropagation()));

  els.settingsX.onclick = () => closeModal(els.settingsModal);
  els.settingsClose.onclick = () => closeModal(els.settingsModal);
  els.memoryX.onclick = () => closeModal(els.memoryModal);
  els.memoryClose.onclick = () => closeModal(els.memoryModal);
  els.cmdkX.onclick = () => closeModal(els.cmdk);
  els.tokenGuideX.onclick = () => closeModal(els.tokenGuide);
  els.tokenGuideClose.onclick = () => closeModal(els.tokenGuide);
  els.tokenGuide.onclick = (e) => { if (e.target === els.tokenGuide) closeModal(els.tokenGuide); };
  els.btnWipe.onclick = () => {
    if (confirm('همهٔ گفتگوها حذف شوند؟')) {
      wipeChats();
      toast('همه گفتگوها حذف شدند', 'ok');
    }
  };
  els.memClear.onclick = async () => {
    await window.__atria.memory_clear();
    openMemory();
    toast('حافظه پاک شد', 'ok');
  };
  els.settingsModal.onclick = (e) => { if (e.target === els.settingsModal) closeModal(els.settingsModal); };
  els.memoryModal.onclick = (e) => { if (e.target === els.memoryModal) closeModal(els.memoryModal); };
  els.cmdk.onclick = (e) => { if (e.target === els.cmdk) closeModal(els.cmdk); };
  els.cmdkInput.oninput = () => renderCmdkList(els.cmdkInput.value.trim());

  // suggestion chips
  document.querySelectorAll('.chip[data-q]').forEach((c) => c.onclick = () => {
    els.input.value = c.dataset.q;
    autosize();
    els.input.focus();
  });

  els.input.addEventListener('input', autosize);
  els.input.addEventListener('keydown', (e) => {
    if (e.isComposing || e.keyCode === 229) return;
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(); }
  });
  els.btnSend.onclick = send;
  els.btnStop.onclick = stop;

  // نوار مدل/حالت (مثل کلاد)
  bindConns();
  updateModelPick();
  syncChips();
  updateConnTab();
  restoreFromDisk();
  window.addEventListener('beforeunload', () => { if (flushTimer) { clearTimeout(flushTimer); flushChats(); } });
  document.addEventListener('visibilitychange', () => { if (document.hidden && (dirtyChats.size || goneChats.size)) flushChats(); });
  els.modelPick.onclick = (e) => {
    e.stopPropagation();
    if (els.modelMenu.classList.contains('hidden')) { renderModelMenu(); els.modelMenu.classList.remove('hidden'); }
    else closeModelMenu();
  };
  document.addEventListener('click', (e) => {
    if (!els.modelMenu.classList.contains('hidden') &&
        !els.modelMenu.contains(e.target) && !els.modelPick.contains(e.target)) closeModelMenu();
  });
  els.chipAgent.onclick = () => {
    st.settings.tools_enabled = !st.settings.tools_enabled;
    els.toolsVal.checked = st.settings.tools_enabled;
    saveSettings(); syncChips();
    toast(st.settings.tools_enabled ? '⚡ حالت ایجنت روشن شد' : '⚡ حالت ایجنت خاموش شد', 'ok');
  };
  els.chipThink.onclick = () => {
    st.settings.thinking = !st.settings.thinking;
    els.thinkVal.checked = st.settings.thinking;
    saveSettings(); syncChips();
  };
  els.chipFiles.onclick = () => {
    st.settings.file_tools = !st.settings.file_tools;
    els.fileToolsVal.checked = st.settings.file_tools;
    saveSettings(); syncChips();
  };
  els.btnScroll.onclick = () => scrollBottom();
  els.messages.addEventListener('scroll', () => {
    const far = els.messages.scrollHeight - els.messages.scrollTop - els.messages.clientHeight > 240;
    els.btnScroll.classList.toggle('hidden', !far);
  });
  autosize();

  try { listenEvents(); } catch (e) {
    console.error(e);
    toast('پل ارتباطی IPC آماده نیست — برنامه را دوباره باز کن', 'err');
  }

  window.addEventListener('keydown', (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      els.cmdk.classList.contains('hidden') ? openCmdk() : closeModal(els.cmdk);
    }
    if (e.key === 'Escape') {
      closeModal(els.cmdk);
      closeModal(els.settingsModal);
      closeModal(els.memoryModal);
    }
  });

  // اولین اجرا: کلیدی تعبیه نشده — تنظیمات را باز کن
  if (!st.settings.api_key) {
    openModal(els.settingsModal);
    setTimeout(() => toast('برای شروع، کلید API خودت را وارد کن', 'warn'), 400);
  }
}

document.addEventListener('DOMContentLoaded', boot);
