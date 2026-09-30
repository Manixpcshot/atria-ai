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
  'cerebras':   { label: 'Cerebras — سریع‌ترین استنتاج', base: 'https://api.cerebras.ai/v1',                                kind: 'openai' },
  'sambanova':  { label: 'SambaNova',                   base: 'https://api.sambanova.ai/v1',                               kind: 'openai' },
  'together':   { label: 'Together AI',                 base: 'https://api.together.xyz/v1',                               kind: 'openai' },
  'fireworks':  { label: 'Fireworks AI',                base: 'https://api.fireworks.ai/inference/v1',                      kind: 'openai' },
  'hyperbolic': { label: 'Hyperbolic',                  base: 'https://api.hyperbolic.xyz/v1',                             kind: 'openai' },
  'nvidia':     { label: 'NVIDIA NIM',                  base: 'https://integrate.api.nvidia.com/v1',                       kind: 'openai' },
  'github':     { label: 'GitHub Models (رایگان)',      base: 'https://models.github.ai/inference',                         kind: 'openai' },
  'perplexity': { label: 'Perplexity (Sonar)',          base: 'https://api.perplexity.ai/chat/completions',                 kind: 'openai' },
  'apmix':      { label: 'ApMix — Messages API',      base: 'https://api.apmix.ai',                                       kind: 'anthropic' },
  'apmix-cc':   { label: 'ApMix — Chat Completions',  base: 'https://api.apmix.ai/v1',                                    kind: 'openai' },
  'opencode':   { label: 'OpenCode Zen — مدل‌های کدنویسی', base: 'https://opencode.ai/zen/v1',                      kind: 'openai' },
  'opencode-cc':{ label: 'OpenCode Zen — Claude',          base: 'https://opencode.ai/zen',                         kind: 'anthropic' },
  'kimi':       { label: 'Kimi (Moonshot)',                base: 'https://api.moonshot.ai/v1',                      kind: 'openai' },
  'zai':        { label: 'Z.AI (GLM)',                     base: 'https://api.z.ai/api/paas/v4',                    kind: 'openai' },
  'siliconflow':{ label: 'SiliconFlow — متن‌باز',          base: 'https://api.siliconflow.com/v1',                  kind: 'openai' },
  'ollama':     { label: 'Ollama (محلی)',                  base: 'http://localhost:11434/v1',                       kind: 'openai' },
  'lmstudio':   { label: 'LM Studio (محلی)',               base: 'http://localhost:1234/v1',                        kind: 'openai' },
  'custom':     { label: 'سفارشی…',                   base: '',                                                        kind: 'openai' },
};

/* ---------------- state ---------------- */

// شخصیت پیش‌فرض: هویت «آتریا» + رفتارِ یک مدل واقعی (بدون سلام‌های تکراری و بدون بی‌هویتی)
const PERSONA = [
  'من «آتریا» هستم — دستیار هوشمند آتریا (Atria)، ساخته‌شده روی مدل Atria-Dawn.',
  'اگر نام یا هویتم را پرسیدند، دقیقاً همین را بگو: «آتریا»؛ نگو که بی‌نام یا صرفاً یک دستیار معمولی‌ام، و نام مدل‌های دیگر (مثل ChatGPT یا Claude یا DeepSeek) را برای خودم قبول نکن.',
  'مثل مدل‌های واقعی رفتار کن: جواب را مستقیم و طبیعی بده. لازم نیست در هر پیام سلام و احوالپرسی یا تعارف تکراری کنی — فقط اگر کاربر در آغاز گفتگو سلام کرد، کوتاه جواب سلام بده و برو سر اصل مطلب.',
  'خلاصه، دقیق و کاربردی باش؛ به‌جای عذرخواهی‌های مکرر راه‌حل بده و چیزی را که نمی‌دانی ادعا نکن.',
].join('\n');
const LS_PERSONA = 'atria.persona.v1';

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
  system: PERSONA,
  max_tokens: 8192,
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
  pendingImgs: [],
};
let dirtyChats = new Set();
let goneChats = new Set();
let flushTimer = 0;
let diskDirs = null;
let lastSession = '';

function loadSettings() {
  let s;
  try { s = { ...DEFAULTS, ...JSON.parse(localStorage.getItem(LS_SETTINGS) || '{}') }; }
  catch { s = { ...DEFAULTS }; }
  // سقف سرویس 65536 — مقدارهای خراب قدیمی را خودکار درمان کن
  s.max_tokens = Math.min(65536, Math.max(256, Number(s.max_tokens) || 8192));
  return s;
}
function saveSettings() {
  localStorage.setItem(LS_SETTINGS, JSON.stringify(st.settings));
  const label = effModel() || '—';
  if (els.modelChip) els.modelChip.textContent = label;
  if (els.mpName) els.mpName.textContent = label;
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
  try {
    await window.__atria.chats_sync({ files, remove });
    for (const f of files) dirtyChats.delete(f.id);
    for (const id of remove) goneChats.delete(id);
  } catch {
    setTimeout(() => { if (!flushTimer) scheduleFlush(); }, 2500);
  }
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
    lastModel: String(c.lastModel || ''),
    enabled: c.enabled !== false,
    models: Array.isArray(c.models) ? [...new Set(c.models.map((m) => String(m || '').trim()).filter(Boolean))] : [],
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

function normEndpoint(value) {
  return String(value || '').trim().replace(/\/+$/, '').toLowerCase();
}

function isPhantomDefaultConn(c) {
  const lastModel = String(c && c.lastModel || '').trim();
  return !!c && c.name === 'اتصال من' && !String(c.key || '').trim() &&
    normEndpoint(c.base) === normEndpoint(DEFAULTS.base_url) &&
    (!lastModel || lastModel === DEFAULTS.model) && Array.isArray(c.models) &&
    c.models.length === 1 && c.models[0] === DEFAULTS.model;
}

function isGeneratedConnName(name) {
  const value = String(name || '').trim();
  return !value || value === 'اتصال' || /^اتصال(?: \d+| جدید(?: \d+)?)?$/.test(value);
}

function isEmptyAutoConn(c) {
  return !!c && isGeneratedConnName(c.name) && !String(c.base || '').trim() &&
    !String(c.key || '').trim() && !String(c.lastModel || '').trim() &&
    Array.isArray(c.models) && c.models.length === 0;
}

// Migrate only data explicitly configured by the user. DEFAULTS always contains a
// base URL and model, so treating either as proof of setup used to invent a fake connection.
function migrateConns() {
  const saved = localStorage.getItem(LS_CONNS);
  let hasSavedList = false;
  if (saved !== null) {
    try { hasSavedList = Array.isArray(JSON.parse(saved)); } catch {}
  }
  if (hasSavedList) {
    const clean = st.conns.filter((c) => !isPhantomDefaultConn(c) && !isEmptyAutoConn(c));
    if (clean.length !== st.conns.length || JSON.stringify(clean) !== JSON.stringify(st.conns)) {
      st.conns = clean;
      saveConns();
    }
    return;
  }

  let legacy = {};
  try {
    const value = JSON.parse(localStorage.getItem(LS_SETTINGS) || '{}');
    if (value && typeof value === 'object' && !Array.isArray(value)) legacy = value;
  } catch {}
  const legacyKey = String(legacy.api_key ?? st.settings.api_key ?? '').trim();
  const legacyBase = String(legacy.base_url ?? st.settings.base_url ?? '').trim();
  const legacyModel = String(legacy.model ?? st.settings.model ?? '').trim();
  const changedBase = !!legacyBase && normEndpoint(legacyBase) !== normEndpoint(DEFAULTS.base_url);
  const changedModel = !!legacyModel && legacyModel !== DEFAULTS.model;
  const explicitlyConfigured = !!legacyKey || changedBase || changedModel;

  const list = [];
  if (explicitlyConfigured) {
    const c = normConn({
      name: 'اتصال من',
      kind: legacy.api_kind || st.settings.api_kind,
      base: legacyBase,
      key: legacyKey,
      models: legacyModel && (changedModel || legacyKey) ? [legacyModel] : [],
    });
    if (c) list.push(c);
  }
  st.conns = list;
  saveConns();
}

// یک‌بار: اگر system خالی است، شخصیت آتریا را بگذار (بعداً پاک‌کردنِ عمدیِ کاربر می‌ماند)
function migratePersona() {
  if (localStorage.getItem(LS_PERSONA)) return;
  if (!st.settings.system || !st.settings.system.trim()) {
    st.settings.system = PERSONA;
    saveSettings();
  }
  localStorage.setItem(LS_PERSONA, '1');
}

function activeConn() {
  return st.conns.find((c) => c.id === st.activeConnId) || st.conns[0] || null;
}
function setActiveConn(id) {
  const c = st.conns.find((x) => x.id === id) || st.conns[0] || null;
  st.activeConnId = c ? c.id : '';
  localStorage.setItem(LS_ACTIVE_CONN, st.activeConnId);
  const model = c
    ? ((c.lastModel && c.models.includes(c.lastModel)) ? c.lastModel : (c.models[0] || ''))
    : '';
  if (model !== st.settings.model) {
    st.settings.model = model;
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
let connSrvRequest = 0;

function editConn() { return st.conns.find((c) => c.id === connEditId) || null; }

const SETTINGS_TAB_COPY = {
  conn: ['اتصال‌ها و مدل‌ها', 'ارائه‌دهنده‌ها، آدرس API، کلیدها و مدل‌های قابل انتخاب را مدیریت کن.'],
  behavior: ['رفتار و پاسخ‌گویی', 'طول پاسخ، تفکر، حافظه و حالت جریان زنده را تنظیم کن.'],
  sys: ['سیستم و ابزارها', 'شخصیت دستیار، دسترسی فایل و محل ذخیره‌سازی را پیکربندی کن.'],
};

function stabGo(tab) {
  const selected = SETTINGS_TAB_COPY[tab] ? tab : 'conn';
  document.querySelectorAll('.stab').forEach((x) => {
    const active = x.dataset.tab === selected;
    x.classList.toggle('active', active);
    if (x.setAttribute) x.setAttribute('aria-selected', active ? 'true' : 'false');
  });
  document.querySelectorAll('.stab-body').forEach((x) => x.classList.toggle('active', x.dataset.tab === selected));
  if (els.settingsTitle) els.settingsTitle.textContent = SETTINGS_TAB_COPY[selected][0];
  if (els.settingsSubtitle) els.settingsSubtitle.textContent = SETTINGS_TAB_COPY[selected][1];
}

function openConns(id) {
  const requested = id ? st.conns.find((c) => c.id === id) : null;
  const current = requested || activeConn();
  connEditId = current ? current.id : null;
  connSrvRequest++;
  connSrv = [];
  if (els.cfSrvSearch) els.cfSrvSearch.value = '';
  if (els.cfFetchModels) {
    els.cfFetchModels.disabled = false;
    els.cfFetchModels.textContent = '🔎 دریافت از سرور';
  }
  renderConnsModal();
  openModal(els.settingsModal);
  stabGo('conn');
}

function renderConnsModal() {
  renderConnList();
  loadConnForm();
}

function renderConnList() {
  els.connList.innerHTML = '';
  const list = Array.isArray(st.conns) ? st.conns : [];
  if (!list.length) {
    els.connList.innerHTML = '<div class="mem-empty">هنوز اتصالی نیست — «＋ اتصال جدید» بزن</div>';
    return;
  }
  for (const c of list) {
    try {
      const it = document.createElement('button');
      it.className = 'conn-card' + (c.id === connEditId ? ' active' : '');
      it.innerHTML = '<div class="cc-name"></div><div class="cc-meta"></div>';
      it.querySelector('.cc-name').textContent = (c.name || 'اتصال بی‌نام') + (c.id === st.activeConnId ? ' ⭐' : '');
      const meta = it.querySelector('.cc-meta');
      meta.innerHTML = '<span class="cc-chip"></span><span class="cc-chip ltr"></span><span class="cc-chip"></span>';
      const chips = meta.querySelectorAll('.cc-chip');
      const mods = Array.isArray(c.models) ? c.models.length : 0;
      chips[0].textContent = kindLabel(c.kind);
      chips[1].textContent = c.base || 'بدون آدرس';
      chips[2].textContent = mods + ' مدل · ' + (c.key ? 'کلید دارد' : 'بدون کلید');
      it.onclick = () => { connEditId = c.id; connSrvRequest++; connSrv = []; if (els.cfSrvSearch) els.cfSrvSearch.value = ''; renderConnsModal(); };
      els.connList.appendChild(it);
    } catch (e) { console.error('renderConnList item:', e); }
  }
}

function matchingTemplate(conn) {
  if (!conn) return '';
  const base = normEndpoint(conn.base);
  const entry = Object.entries(CONN_TEMPLATES).find(([, t]) =>
    t.kind === conn.kind && base && normEndpoint(t.base) === base);
  return entry ? entry[0] : '';
}

function loadConnForm() {
  const c = editConn();
  els.connForm.classList.toggle('hidden', !c);
  const connsBody = document.querySelector('.conns-body');
  if (connsBody) connsBody.classList.toggle('has-form', !!c);
  els.cfKey.type = 'password';
  els.cfKeyEye.textContent = '👁';
  els.cfModelInput.value = '';
  els.cfSrvSearch.value = '';
  connSrv = [];
  els.cfSrvList.innerHTML = '';
  els.cfSrvWrap.classList.add('hidden');
  els.cfFetchModels.disabled = false;
  els.cfFetchModels.textContent = '🔎 دریافت از سرور';
  els.cfTemplate.value = '';
  els.cfTags.innerHTML = '';

  if (!c) {
    els.cfName.value = '';
    els.cfKind.value = 'openai';
    els.cfBase.value = '';
    els.cfKey.value = '';
    els.cfSetActive.textContent = '⭐ اتصال فعال';
    els.cfKeyLabel.textContent = 'کلید API';
    els.cfKey.placeholder = 'کلید همین اتصال';
    els.cfTokenGuide.classList.add('hidden');
    els.cfHint.classList.add('hidden');
    return;
  }

  els.cfName.value = c.name;
  els.cfKind.value = c.kind;
  els.cfBase.value = c.base;
  els.cfKey.value = c.key;
  els.cfTemplate.value = matchingTemplate(c);
  els.cfSetActive.textContent = c.id === st.activeConnId ? '⭐ اتصال فعال ✓' : '⭐ اتصال فعال';
  renderModelTags();
  renderSrvList();
  updateConnFormHints();
}

function commitConnForm(finalize = false) {
  const c = editConn();
  if (!c) return;
  c.name = els.cfName.value.trim() || (finalize ? 'اتصال جدید' : '');
  if (finalize && !els.cfName.value.trim()) els.cfName.value = c.name;
  c.kind = els.cfKind.value;
  c.base = els.cfBase.value.trim();
  c.key = els.cfKey.value.trim();
  if (els.cfTemplate.value && els.cfTemplate.value !== matchingTemplate(c)) els.cfTemplate.value = '';
  saveConns();
  renderConnList();
  updateConnTab();
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
    t.dir = 'ltr';
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
  if (!c || els.cfFetchModels.disabled) return;
  const requestId = ++connSrvRequest;
  const connId = c.id;
  const base = els.cfBase.value.trim();
  const key = els.cfKey.value.trim();
  els.cfFetchModels.disabled = true;
  els.cfFetchModels.textContent = 'در حال دریافت…';
  connSrv = [];
  renderSrvList();
  try {
    const result = await window.__atria.list_models({ base, key });
    if (requestId !== connSrvRequest || connEditId !== connId) return;
    connSrv = Array.isArray(result)
      ? [...new Set(result.map((x) => String(x || '').trim()).filter(Boolean))]
      : [];
    renderSrvList();
    toast(connSrv.length + ' مدل از سرور آمد — روی هرکدام کلیک کن تا اضافه شود', 'ok');
  } catch (e) {
    if (requestId === connSrvRequest && connEditId === connId) toast('دریافت فهرست مدل‌ها: ' + e, 'err');
  } finally {
    if (requestId === connSrvRequest && connEditId === connId) {
      els.cfFetchModels.disabled = false;
      els.cfFetchModels.textContent = '🔎 دریافت از سرور';
    }
  }
}

function resetConnServerModels() {
  connSrvRequest++;
  connSrv = [];
  renderSrvList();
  els.cfFetchModels.disabled = false;
  els.cfFetchModels.textContent = '🔎 دریافت از سرور';
}

function renderSrvList() {
  const q = (els.cfSrvSearch.value || '').trim().toLocaleLowerCase();
  els.cfSrvWrap.classList.toggle('hidden', !connSrv.length);
  els.cfSrvList.innerHTML = '';
  const c = editConn();
  let shown = 0;
  for (const id of connSrv) {
    if (q && !id.toLocaleLowerCase().includes(q)) continue;
    shown++;
    if (shown > 400) break;
    const it = document.createElement('button');
    const picked = c && c.models.includes(id);
    it.className = 'srv-item' + (picked ? ' picked' : '');
    it.dir = 'ltr';
    it.title = id;
    const tick = document.createElement('span');
    tick.className = 'srv-tick';
    tick.textContent = picked ? '✓' : '+';
    const nm = document.createElement('bdi');
    nm.className = 'srv-name';
    nm.textContent = id;
    it.appendChild(tick);
    it.appendChild(nm);
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
  if (connSrv.length && q && shown === 0) {
    const empty = document.createElement('div');
    empty.className = 'mem-empty srv-empty';
    empty.textContent = 'مدلی با این عبارت پیدا نشد';
    els.cfSrvList.appendChild(empty);
  }
}

function bindConns() {
  // template select options
  els.cfTemplate.innerHTML = '<option value="">— سفارشی —</option>' +
    Object.entries(CONN_TEMPLATES)
      .map(([id, t]) => `<option value="${id}">${t.label}</option>`).join('');
  els.connAdd.onclick = () => {
    const unfinished = st.conns.find((c) =>
      isGeneratedConnName(c.name) && !c.base.trim() && !c.key.trim() && !c.models.length);
    if (unfinished) {
      connEditId = unfinished.id;
      renderConnsModal();
      els.cfName.focus();
      return;
    }
    let suffix = 1;
    let name = 'اتصال جدید';
    while (st.conns.some((c) => c.name === name)) name = 'اتصال جدید ' + (++suffix);
    const c = normConn({ name, kind: 'openai', base: '', key: '', models: [] });
    st.conns.unshift(c);
    saveConns();
    setActiveConn(c.id);
    connEditId = c.id;
    connSrvRequest++;
    connSrv = [];
    renderConnsModal();
    els.cfName.focus();
  };
  els.cfName.oninput = () => commitConnForm();
  els.cfName.onkeydown = (e) => { if (!(e.isComposing || e.keyCode === 229) && e.key === 'Enter') { e.preventDefault(); els.cfBase.focus(); } };
  els.cfBase.onkeydown = (e) => { if (!(e.isComposing || e.keyCode === 229) && e.key === 'Enter') { e.preventDefault(); els.cfKey.focus(); } };
  els.cfKey.onkeydown = (e) => { if (!(e.isComposing || e.keyCode === 229) && e.key === 'Enter') { e.preventDefault(); els.cfKey.blur(); } };
  els.cfName.onblur = els.cfBase.onblur = els.cfKey.onblur = () => commitConnForm(true);
  els.cfKind.onchange = () => { commitConnForm(); updateConnFormHints(); };
  els.cfTemplate.onchange = () => {
    const t = CONN_TEMPLATES[els.cfTemplate.value];
    const c = editConn();
    if (!c || !t) return;
    c.kind = t.kind;
    c.base = t.base;
    if (isGeneratedConnName(c.name)) c.name = t.label;
    connSrvRequest++;
    saveConns();
    loadConnForm();
    renderConnList();
    updateConnTab();
  };
  els.cfBase.oninput = () => { resetConnServerModels(); commitConnForm(); };
  els.cfKey.oninput = () => { resetConnServerModels(); commitConnForm(); };
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
    const wasActive = st.activeConnId === c.id;
    st.conns = st.conns.filter((x) => x.id !== c.id);
    saveConns();
    if (wasActive) setActiveConn(st.conns[0] ? st.conns[0].id : '');
    connEditId = st.conns[0] ? st.conns[0].id : null;
    connSrvRequest++;
    connSrv = [];
    renderConnsModal();
    updateConnTab();
    updateModelPick();
  };
  // v0.6.8: دکمهٔ مدیریت قدیمی حذف شده بود ولی سیم‌کشی‌اش جامانده بود (شیء نال = قاتل boot)
}

function updateConnTab() {
  const c = activeConn();
  if (els.connSummary) {
    els.connSummary.innerHTML = '';
    if (!c) {
      const empty = document.createElement('div');
      empty.className = 'conn-empty-summary';
      empty.textContent = 'هنوز اتصالی ساخته نشده است. از فهرست پایین «اتصال جدید» را بزن.';
      els.connSummary.appendChild(empty);
    } else {
      const summary = document.createElement('div');
      summary.className = 'conn-active-summary';
      const hasCredentials = !!String(c.key || '').trim() || c.kind === 'deepseek_web';
      const status = document.createElement('span');
      status.className = 'conn-status-dot' + (hasCredentials ? '' : ' needs-key');
      status.setAttribute?.('aria-hidden', 'true');
      const name = document.createElement('bdi');
      name.className = 'conn-summary-name';
      name.dir = 'auto';
      name.textContent = c.name || 'اتصال بی‌نام';
      const model = document.createElement('bdi');
      model.className = 'conn-summary-model';
      model.dir = 'ltr';
      model.textContent = effModel() || 'مدلی انتخاب نشده';
      const kind = document.createElement('span');
      kind.className = 'conn-summary-kind';
      kind.textContent = kindLabel(c.kind);
      const state = document.createElement('span');
      state.className = 'conn-summary-state' + (hasCredentials ? '' : ' missing');
      state.textContent = hasCredentials ? 'آمادهٔ استفاده' : 'کلید API وارد نشده';
      summary.appendChild(status);
      summary.appendChild(name);
      summary.appendChild(model);
      summary.appendChild(kind);
      summary.appendChild(state);
      els.connSummary.appendChild(summary);
    }
  }
  if (els.dsSearchRow) {
    els.dsSearchRow.classList.toggle('hidden', !(c && c.kind === 'deepseek_web'));
  }
  if (els.btnAttach) {
    els.btnAttach.classList.toggle('hidden', !!(c && c.kind === 'deepseek_web'));
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
    settingsModal: $('#settingsModal'), settingsX: $('#settingsX'),
    settingsTitle: $('#settingsTitle'), settingsSubtitle: $('#settingsSubtitle'),
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
    connSummary: $('#connSummary'),
    connList: $('#connList'), connAdd: $('#connAdd'), connForm: $('#connForm'),
    cfName: $('#cfName'), cfKind: $('#cfKind'), cfTemplate: $('#cfTemplate'),
    cfBase: $('#cfBase'), cfKey: $('#cfKey'), cfKeyEye: $('#cfKeyEye'),
    cfKeyLabel: $('#cfKeyLabel'), cfTokenGuide: $('#cfTokenGuide'), cfHint: $('#cfHint'),
    cfTags: $('#cfTags'), cfModelInput: $('#cfModelInput'), cfModelAdd: $('#cfModelAdd'),
    cfFetchModels: $('#cfFetchModels'), cfSrvWrap: $('#cfSrvWrap'),
    cfSrvSearch: $('#cfSrvSearch'), cfSrvList: $('#cfSrvList'),
    cfDelete: $('#cfDelete'), cfSetActive: $('#cfSetActive'),
    storageBox: $('#storageBox'), btnRevealAtria: $('#btnRevealAtria'),
    btnAttach: $('#btnAttach'), fileInput: $('#fileInput'), imgTray: $('#imgTray'),
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
  if (!els.toasts) return;
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
  els.btnSend.disabled = !els.input.value.trim() && !st.pendingImgs.length;
}

function scrollBottom(instant) {
  if (instant) els.messages.scrollTop = els.messages.scrollHeight;
  else els.messages.scrollTo({ top: els.messages.scrollHeight, behavior: 'smooth' });
}

/* ---------------- settings ---------------- */

function bindSettings() {
  // یک مسیر واحد برای همگام‌سازی تب، محتوای صفحه و عنوان.
  document.querySelectorAll('.stab').forEach((b) => {
    b.addEventListener('click', () => stabGo(b.dataset.tab));
  });
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
  els.maxTokVal.value = st.settings.max_tokens || 8192;
  els.toolsVal.checked = !!st.settings.tools_enabled;
  els.fileToolsVal.checked = !!st.settings.file_tools;
  els.wsVal.value = st.settings.workspace || '';
  // فیلدهای متنی: ذخیره با تأخیر (تایپ بدون لگ) + نهایی‌سازی در blur
  els.sysVal.oninput = () => { st.settings.system = els.sysVal.value; saveSettingsSoon(); };
  els.sysVal.onblur = saveSettings;
  els.maxTokVal.onchange = () => {
    let v = Math.round(Number(els.maxTokVal.value));
    if (!Number.isFinite(v)) v = 8192;
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
    d.className = 'conv-sub';
    const lastMsg = c.messages[c.messages.length - 1];
    const prev = lastMsg ? String(lastMsg.plain || lastMsg.text || '').replace(/\s+/g, ' ').trim().slice(0, 64) : '';
    const dstr = new Date(c.createdAt).toLocaleDateString('fa-IR');
    d.textContent = prev ? prev + ' · ' + dstr : dstr;
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
  st.currentId = id;
  localStorage.setItem(LS_CURRENT, id);
  renderConvList();
  renderHistory();
  scrollBottom(true);
  closeModal(els.cmdk);
}

function deleteChat(id) {
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

function bubbleEl(role, text, flow, images) {
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
  if (images && images.length) {
    const tray = document.createElement('div');
    tray.className = 'msg-imgs';
    for (const im of images) {
      const el = document.createElement('img');
      el.className = 'msg-img';
      el.alt = im.name || '';
      el.src = 'data:' + im.media + ';base64,' + im.data;
      tray.appendChild(el);
    }
    b.insertBefore(tray, md);
  }
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
      bubbleEl(m.role === 'user' ? 'user' : 'ai', m.plain || m.text || '…', m.flow, m.images));
  }
  scrollBottom(true);
  ensureLiveVisible(); // v0.7.0: اگر تولید در همین چت در جریان است، حباب زنده برگردد
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

/* ---------------- image attachments (vision models) ---------------- */

const MAX_IMGS = 4;
const MAX_IMG_BYTES = 4500 * 1024;

function addImageFiles(files) {
  const conn = activeConn();
  if (conn && conn.kind === 'deepseek_web') {
    return toast('در حساب وب دیپ‌سیک فعلاً پیوست تصویر پشتیبانی نمی‌شود', 'warn');
  }
  for (const f of Array.from(files || [])) {
    if (st.pendingImgs.length >= MAX_IMGS) { toast('حداکثر ' + MAX_IMGS + ' تصویر', 'warn'); break; }
    if (!/^image\//.test(f.type)) continue;
    if (f.size > MAX_IMG_BYTES) { toast('حجم «' + f.name + '» زیاد است (حداکثر ۴٫۵ مگابایت)', 'warn'); continue; }
    const r = new FileReader();
    r.onload = () => {
      const dataUrl = String(r.result || '');
      const m = /^data:([^;,]+);base64,(.*)$/.exec(dataUrl);
      if (!m) return;
      st.pendingImgs.push({ data: m[2], media: m[1], name: f.name });
      renderImgTray();
    };
    r.readAsDataURL(f);
  }
}

function renderImgTray() {
  if (!els.imgTray) return;
  els.imgTray.innerHTML = '';
  els.imgTray.classList.toggle('hidden', !st.pendingImgs.length);
  st.pendingImgs.forEach((im, i) => {
    const chip = document.createElement('div');
    chip.className = 'img-chip';
    chip.innerHTML = '<img alt=""><button class="img-x" title="حذف">✕</button>';
    chip.querySelector('img').src = 'data:' + im.media + ';base64,' + im.data;
    chip.querySelector('.img-x').onclick = () => {
      st.pendingImgs.splice(i, 1);
      renderImgTray();
    };
    els.imgTray.appendChild(chip);
  });
  autosize();
}

function clearImgs() {
  st.pendingImgs = [];
  renderImgTray();
}

/* ---------------- effective model + history window ---------------- */

// مدل مؤثرِ اتصال فعال: انتخاب فعلی اگر مجاز باشد، وگرنه آخرین مدل همین اتصال، وگرنه اولی
function effModel() {
  const conn = activeConn();
  if (!conn) return '';
  if (st.settings.model && conn.models.includes(st.settings.model)) return st.settings.model;
  if (conn.lastModel && conn.models.includes(conn.lastModel)) return conn.lastModel;
  return conn.models[0] || '';
}

// پنجرهٔ تاریخچه: چت‌های بلند را سبک می‌کند تا صف/مهلت گیت‌وی‌های محلی (OmniRoute) نفس بکشد
const MAX_CTX_CHARS = 120000;
const MAX_CTX_MSGS = 48;
function buildHistory(chat) {
  const all = chat.messages.map((m) => {
    const content = [];
    for (const im of m.images || []) content.push({ type: 'image', data: im.data, media: im.media });
    const t = msgText(m);
    if (t || !content.length) content.push({ type: 'text', text: t });
    return { role: m.role === 'user' ? 'user' : 'assistant', content };
  });
  let chars = 0;
  const kept = [];
  for (let i = all.length - 1; i >= 0; i--) {
    const len = all[i].content.reduce((a, b) => a + ((b && b.text) || '').length, 0);
    if (kept.length && (kept.length >= MAX_CTX_MSGS || chars + len > MAX_CTX_CHARS)) break;
    kept.unshift(all[i]);
    chars += len;
  }
  // نقش آغازین باید user باشد
  while (kept.length > 1 && kept[0].role !== 'user') kept.shift();
  return { hist: kept, dropped: all.length - kept.length };
}

// خطاهای موقت (شلوغی صف/مهلت گیت‌وی) که ارزش راهنمای «تلاش دوباره» را دارند
const TRANSIENT_RE = /(maxwaitms|rate.?limit|ratelimit|queue|expiration|timeout|timed out|overloaded|temporarily|try again|too many requests|\[50[234]\]|\[429\])/i;

/* ---------------- send flow ---------------- */

function send() {
  const text = els.input.value.trim();
  if (!text && !st.pendingImgs.length) return;
  if (st.sending) return toast('تولید فعلی هنوز تمام نشده — ⏹ بزن یا صبر کن', 'warn');
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
  if (conn.kind === 'openai' && !conn.base) {
    openConns(conn.id);
    return toast('آدرس پایه (Base URL) اتصال «' + conn.name + '» خالی است', 'warn');
  }
  if (conn.kind !== 'deepseek_web' && !effModel()) {
    openConns(conn.id);
    return toast('برای اتصال «' + conn.name + '» هنوز مدلی تعیین نکرده‌ای', 'warn');
  }

  const sendImgs = st.pendingImgs.slice();
  chat.messages.push({ role: 'user', text, plain: text, ts: Date.now(), images: sendImgs.length ? sendImgs : undefined });
  if (chat.title === 'گفتگوی جدید') {
    const t1 = (text || '[تصویر]').replace(/\s+/g, ' ').trim();
    chat.title = t1.slice(0, 42) + (t1.length > 42 ? '…' : '');
    renderConvList();
  }
  saveChats(chat.id);
  clearImgs();
  els.empty.classList.add('hidden', 'off');
  els.messages.appendChild(bubbleEl('user', text, null, sendImgs));
  els.input.value = '';
  autosize();
  scrollBottom();

  startTurn();
}

function startTurn() {
  st.sending = true;
  st.runChatId = st.currentId; // چتِ همین تولید — حتی اگر کاربر وسط کار چت را عوض کند
  st.runText = '';
  st.streamEl = null;
  st.stackEl = null;
  st.liveMsg = null;
  st.run = newRun();
  st.live = null;
  liveMdEl(); // create the live ai message shell
  startBlink();
  els.btnSend.classList.add('hidden');
  els.btnStop.classList.remove('hidden');

  const chat = currentChat();
  const { hist: history, dropped } = buildHistory(chat);
  if (dropped > 0) {
    toast('به‌خاطر طول گفتگو، ' + dropped + ' پیام قدیمی‌تر برای مدل فرستاده نشد (متن‌ها در برنامه می‌مانند)', 'warn');
  }
  if (!window.__atria || !window.__atria.chat_send) {
    return failRun('پل ارتباطی IPC آماده نیست — برنامه را دوباره باز کن');
  }
  const conn = activeConn();
  window.__atria.chat_send({
    payload: {
      api_key: conn ? conn.key : '',
      base_url: (conn && conn.base) || (conn && conn.kind === 'anthropic' ? 'https://api.anthropic.com' : conn && conn.kind === 'deepseek_web' ? 'https://chat.deepseek.com' : ''),
      model: effModel() || 'Atria-Dawn-Preview',
      max_tokens: Math.min(65536, Math.max(256, Number(st.settings.max_tokens) || 8192)),
      temperature: Number(st.settings.temp),
      system: st.settings.system || '',
      tools_enabled: !!st.settings.tools_enabled,
      stream: st.settings.stream !== false,
      kind: conn ? conn.kind : 'anthropic',
      file_tools: !!st.settings.file_tools,
      workspace: st.settings.workspace || '',
      thinking: !!st.settings.thinking,
      web_search: !!st.settings.ds_search,
      session_id: (chat && chat.dsSession) || '',
      messages: history,
    },
  }).catch((e) => failRun(e && e.message ? e.message : String(e)));
}

function retryLast() {
  if (st.sending) return;
  if (st.runChatId && st.currentId !== st.runChatId && st.chats.some((x) => x.id === st.runChatId)) {
    switchChat(st.runChatId); // تلاش مجدد در همان چت
  }
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
  if (run.panel && (run.panel.isConnected || (st.liveMsg && st.liveMsg.contains(run.panel)))) return run.panel;
  if (!st.stackEl) liveMdEl(); // v0.6.8: اگر هنوز حباب ساخته نشده، اول بساز
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
  if (!stack) { run.panel = box; run.flow = box.querySelector('.flow'); return box; }
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
  ensureLiveVisible();
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
  ensureLiveVisible();
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
    st.liveMsg = wrap; // برای سوار دوباره بعد از جابه‌جایی چت
    scrollBottom();
  }
  return st.mdEl;
}

st.liveEl = function () { return liveMdEl(); };

// اگر وسط تولید به چت دیگری رفتیم و برگشتیم، حباب زنده را دوباره سوار صفحه کن
function ensureLiveVisible() {
  if (!st.sending || !st.liveMsg) return;
  const chat = currentChat();
  if (!chat || chat.id !== st.runChatId) return;
  if (!st.liveMsg.isConnected) {
    els.messages.appendChild(st.liveMsg);
    scrollBottom(true);
  }
}

function ensureLive() {
  ensureLiveVisible();
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

function resetAttemptStream() {
  closeLive();
  st.runText = '';
  if (st.mdEl) st.mdEl.innerHTML = '';
  if (st.run) {
    st.run.thinkStep = null;
    st.run.thinkTn = null;
    st.run.thinkLen = 0;
    st.run.thinkKind = '';
  }
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
    let shown = 'خطا: ' + msg;
    if (TRANSIENT_RE.test(String(msg))) {
      shown += '\n\n💡 این خطا معمولاً موقتی است (شلوغی صف/مهلت گیت‌وی) — «تلاش مجدد» بزن. اگر باز هم تکرار شد، گفتگوی جدید باز کن تا تاریخچه سبک شود.';
    }
    e.querySelector('.err-text').textContent = shown;
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
  const chat = st.chats.find((x) => x.id === st.runChatId) || null;
  const plain = finalText && finalText.trim() ? finalText : st.runText || '(پاسخ خالی)';
  if (!chat) { toast('گفتگوی این تولید حذف شده بود؛ پاسخ ذخیره نشد', 'warn'); return; }
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
    chat.messages.push({ role: 'ai', text: plain, plain, ts: Date.now() });
    pushed.push(chat.messages[chat.messages.length - 1]);
  }
  if (!pushed.length) {
    chat.messages.push({ role: 'ai', text: plain, plain, ts: Date.now() });
    pushed.push(chat.messages[chat.messages.length - 1]);
  }
  for (let i = pushed.length - 1; i >= 0; i--) {
    if (pushed[i].role === 'ai') { if (flow) pushed[i].flow = flow; break; }
  }
  if (lastSession) chat.dsSession = lastSession;
  saveChats(chat.id);
  if (st.currentId === chat.id) {
    renderHistory(); // نمایش نهایی = متن کامل ذخیره‌شده (نه متن ناقص استریم زنده)
    scrollBottom();
  } else {
    toast('پاسخ در گفتگوی «' + chat.title + '» ذخیره شد', 'ok');
  }
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
      addChip('↻ مرحلهٔ مدل ' + e.payload.round);
    }
  });
  t.listen('atria:tool_pending', (e) => onToolPending(e.payload.id, e.payload.name));
  t.listen('atria:tool_args', (e) => onToolArgs(e.payload.id, e.payload.n || 0));
  t.listen('atria:tool_start', (e) =>
    onToolStart(e.payload.id, e.payload.name, e.payload.label, e.payload.input));
  t.listen('atria:tool_end', (e) => onToolEnd(e.payload.id, e.payload.ok, e.payload.output));
  t.listen('atria:retry', (e) => {
    resetAttemptStream();
    ensurePanel();
    addChip('↻ تلاش دوباره (' + e.payload.attempt + ' از ۳) — پاسخ ناتمام دور ریخته شد');
  });
  t.listen('atria:done', (e) => {
    lastSession = e.payload.session_id || '';
    onUsage(e.payload.input_tokens || 0, e.payload.output_tokens || 0);
    finishRun(e.payload.new_messages, e.payload.final_text);
  });
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
  const label = effModel() || '—';
  const conn = activeConn();
  if (els.mpName) els.mpName.textContent = label;
  if (els.modelChip) els.modelChip.textContent = label;
  if (els.modelPick) {
    els.modelPick.title = conn ? `${label} · ${conn.name}` : 'انتخاب مدل';
    els.modelPick.setAttribute?.('aria-label', conn
      ? `مدل انتخاب‌شده: ${label}؛ اتصال: ${conn.name}`
      : 'انتخاب مدل');
  }
}

function closeModelMenu() {
  if (els.modelMenu) els.modelMenu.classList.add('hidden');
  if (els.modelPick) els.modelPick.setAttribute?.('aria-expanded', 'false');
}

function renderModelMenu() {
  const menu = els.modelMenu;
  if (!menu) return;
  menu.innerHTML = '';
  menu.classList.add('model-menu');
  menu.setAttribute?.('role', 'dialog');
  menu.setAttribute?.('aria-label', 'انتخاب مدل و اتصال');
  menu.setAttribute?.('aria-modal', 'false');
  if (els.modelPick) {
    els.modelPick.setAttribute?.('aria-haspopup', 'dialog');
    els.modelPick.setAttribute?.('aria-controls', 'modelMenu');
    els.modelPick.setAttribute?.('aria-expanded', 'true');
  }

  const active = activeConn();
  const selectedModel = effModel();
  const head = document.createElement('header');
  head.className = 'mp-head';
  const headCopy = document.createElement('div');
  headCopy.className = 'mp-head-copy';
  const title = document.createElement('strong');
  title.className = 'mp-title';
  title.textContent = 'انتخاب مدل';
  const hint = document.createElement('span');
  hint.className = 'mp-head-hint';
  hint.textContent = 'مدل را انتخاب کن؛ اتصال هم خودکار عوض می‌شود.';
  headCopy.appendChild(title);
  headCopy.appendChild(hint);
  head.appendChild(headCopy);
  menu.appendChild(head);

  const current = document.createElement('div');
  current.className = 'mp-current';
  const currentMark = document.createElement('span');
  currentMark.className = 'mp-current-mark';
  currentMark.textContent = '●';
  currentMark.setAttribute?.('aria-hidden', 'true');
  const currentCopy = document.createElement('div');
  currentCopy.className = 'mp-current-copy';
  const currentLabel = document.createElement('span');
  currentLabel.className = 'mp-current-label';
  currentLabel.textContent = 'در حال استفاده';
  const currentName = document.createElement('bdi');
  currentName.className = 'mp-current-model';
  currentName.dir = 'ltr';
  currentName.textContent = selectedModel || 'مدلی انتخاب نشده';
  const currentConn = document.createElement('bdi');
  currentConn.className = 'mp-current-conn';
  currentConn.dir = 'auto';
  currentConn.textContent = active ? `${active.name} · ${kindLabel(active.kind)}` : 'اتصالی انتخاب نشده';
  currentCopy.appendChild(currentLabel);
  currentCopy.appendChild(currentName);
  currentCopy.appendChild(currentConn);
  current.appendChild(currentMark);
  current.appendChild(currentCopy);
  menu.appendChild(current);

  const searchWrap = document.createElement('div');
  searchWrap.className = 'mp-search-wrap';
  const searchIcon = document.createElement('span');
  searchIcon.className = 'mp-search-icon';
  searchIcon.textContent = '⌕';
  searchIcon.setAttribute?.('aria-hidden', 'true');
  const searchInput = document.createElement('input');
  searchInput.className = 'mp-search';
  searchInput.id = 'mmSearch';
  searchInput.type = 'search';
  searchInput.dir = 'auto';
  searchInput.autocomplete = 'off';
  searchInput.spellcheck = false;
  searchInput.placeholder = 'جست‌وجوی مدل، اتصال یا API…';
  searchInput.setAttribute?.('aria-label', 'جست‌وجوی مدل، اتصال یا API');
  const clearSearch = document.createElement('button');
  clearSearch.className = 'mp-clear hidden';
  clearSearch.type = 'button';
  clearSearch.title = 'پاک‌کردن جست‌وجو';
  clearSearch.textContent = '×';
  clearSearch.setAttribute?.('aria-label', 'پاک‌کردن جست‌وجو');
  searchWrap.appendChild(searchIcon);
  searchWrap.appendChild(searchInput);
  searchWrap.appendChild(clearSearch);
  menu.appendChild(searchWrap);

  const results = document.createElement('div');
  results.className = 'mp-results';
  results.setAttribute?.('role', 'listbox');
  results.setAttribute?.('aria-label', 'مدل‌های موجود');
  menu.appendChild(results);

  const allConnections = st.conns
    .filter((conn) => conn && conn.enabled !== false)
    .slice()
    .sort((a, b) => (a.id === (active && active.id) ? -1 : b.id === (active && active.id) ? 1 : 0));
  const groups = [];
  let modelCount = 0;

  for (const conn of allConnections) {
    const models = Array.isArray(conn.models) ? [...new Set(conn.models.map((m) => String(m || '').trim()).filter(Boolean))] : [];
    const group = document.createElement('section');
    group.className = 'mp-group';
    group.dataset.connId = conn.id;
    const groupHead = document.createElement('div');
    groupHead.className = 'mp-group-head';
    const groupName = document.createElement('bdi');
    groupName.className = 'mp-group-name';
    groupName.dir = 'auto';
    groupName.textContent = conn.name || 'اتصال بی‌نام';
    const groupMeta = document.createElement('span');
    groupMeta.className = 'mp-group-meta';
    groupMeta.textContent = `${kindLabel(conn.kind)} · ${models.length} مدل`;
    const groupState = document.createElement('span');
    groupState.className = 'mp-group-state' + (active && conn.id === active.id ? ' active' : '');
    groupState.textContent = active && conn.id === active.id ? 'فعال' : '';
    groupHead.appendChild(groupName);
    groupHead.appendChild(groupMeta);
    groupHead.appendChild(groupState);
    group.appendChild(groupHead);

    const rows = [];
    for (const model of models) {
      const option = document.createElement('button');
      option.className = 'mp-option';
      option.type = 'button';
      option.dataset.connectionId = conn.id;
      option.dataset.model = model;
      option.setAttribute?.('role', 'option');
      const isCurrent = !!(active && conn.id === active.id && model === selectedModel);
      option.setAttribute?.('aria-selected', isCurrent ? 'true' : 'false');
      if (isCurrent) option.classList.add('active');

      const identity = document.createElement('span');
      identity.className = 'mp-option-identity';
      const modelName = document.createElement('bdi');
      modelName.className = 'mp-model-name';
      modelName.dir = 'ltr';
      modelName.textContent = model;
      const modelMeta = document.createElement('span');
      modelMeta.className = 'mp-model-meta';
      modelMeta.textContent = kindLabel(conn.kind);
      identity.appendChild(modelName);
      identity.appendChild(modelMeta);
      const source = document.createElement('bdi');
      source.className = 'mp-source';
      source.dir = 'auto';
      source.textContent = conn.name || 'اتصال بی‌نام';
      const check = document.createElement('span');
      check.className = 'mp-check';
      check.textContent = isCurrent ? '✓' : '';
      check.setAttribute?.('aria-hidden', 'true');
      option.appendChild(identity);
      option.appendChild(source);
      option.appendChild(check);
      option.onclick = () => chooseModelFor(conn.id, model);
      option.onkeydown = (event) => {
        if (event.key === 'Escape') { event.preventDefault(); closeModelMenu(); els.modelPick?.focus(); return; }
        if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
        event.preventDefault();
        const visible = Array.from(menu.querySelectorAll('.mp-option')).filter((item) => !item.classList.contains('hidden'));
        const index = visible.indexOf(option);
        const next = visible[(index + (event.key === 'ArrowDown' ? 1 : visible.length - 1)) % visible.length];
        if (next) next.focus();
      };
      group.appendChild(option);
      rows.push(option);
      modelCount++;
    }

    let noModels = null;
    if (!models.length) {
      noModels = document.createElement('div');
      noModels.className = 'mp-no-models';
      noModels.textContent = conn.key || conn.kind === 'deepseek_web'
        ? 'برای این اتصال هنوز مدلی اضافه نشده است.'
        : 'مدل و کلید این اتصال را در تنظیمات تکمیل کن.';
      group.appendChild(noModels);
    }
    results.appendChild(group);
    groups.push({ root: group, conn, rows, noModels });
  }

  const empty = document.createElement('div');
  empty.className = 'mp-empty hidden';
  const emptyIcon = document.createElement('span');
  emptyIcon.className = 'mp-empty-icon';
  emptyIcon.textContent = '⌕';
  const emptyTitle = document.createElement('strong');
  emptyTitle.className = 'mp-empty-title';
  const emptyCopy = document.createElement('span');
  emptyCopy.className = 'mp-empty-copy';
  empty.appendChild(emptyIcon);
  empty.appendChild(emptyTitle);
  empty.appendChild(emptyCopy);
  menu.appendChild(empty);

  const footer = document.createElement('footer');
  footer.className = 'mp-footer';
  const footerNote = document.createElement('span');
  footerNote.className = 'mp-footer-note';
  footerNote.textContent = 'جست‌وجو بین نام مدل، اتصال و نوع API';
  const manage = document.createElement('button');
  manage.className = 'mp-manage';
  manage.type = 'button';
  manage.textContent = '⚙ مدیریت اتصال‌ها';
  manage.onclick = (event) => {
    event.stopPropagation();
    closeModelMenu();
    openConns(null);
  };
  footer.appendChild(footerNote);
  footer.appendChild(manage);
  menu.appendChild(footer);

  function filterModels(rawQuery) {
    const query = String(rawQuery || '').normalize('NFKC').trim().toLocaleLowerCase();
    let visibleCount = 0;
    for (const entry of groups) {
      const connectionText = `${entry.conn.name || ''} ${kindLabel(entry.conn.kind)} ${entry.conn.base || ''}`
        .normalize('NFKC').toLocaleLowerCase();
      const connectionMatches = !!query && connectionText.includes(query);
      let groupVisible = false;
      for (const option of entry.rows) {
        const model = String(option.dataset.model || '').normalize('NFKC').toLocaleLowerCase();
        const matches = !query || connectionMatches || model.includes(query);
        option.classList.toggle('hidden', !matches);
        if (matches) { visibleCount++; groupVisible = true; }
      }
      if (entry.noModels && (!query || connectionMatches)) groupVisible = true;
      entry.root.classList.toggle('hidden', !groupVisible);
    }
    clearSearch.classList.toggle('hidden', !searchInput.value);
    const noMatch = !!query && visibleCount === 0;
    const noModels = modelCount === 0;
    empty.classList.toggle('hidden', !noMatch && !noModels);
    emptyTitle.textContent = noMatch ? 'نتیجه‌ای پیدا نشد' : 'هنوز مدلی برای انتخاب نیست';
    emptyCopy.textContent = noMatch
      ? 'عبارت جست‌وجو را کوتاه‌تر کن یا آن را پاک کن.'
      : 'از تنظیمات، یک اتصال بساز و مدل‌های آن را اضافه کن.';
    results.classList.toggle('hidden', noModels && !allConnections.length);
  }

  searchInput.oninput = () => filterModels(searchInput.value);
  clearSearch.onclick = () => {
    searchInput.value = '';
    filterModels('');
    searchInput.focus();
  };
  searchInput.onkeydown = (event) => {
    if (event.key === 'Escape') { event.preventDefault(); closeModelMenu(); els.modelPick?.focus(); return; }
    const visible = Array.from(menu.querySelectorAll('.mp-option')).filter((item) => !item.classList.contains('hidden'));
    if (event.key === 'ArrowDown' && visible.length) { event.preventDefault(); visible[0].focus(); }
    if (event.key === 'Enter' && visible.length) { event.preventDefault(); visible[0].click(); }
  };
  filterModels('');
}

function chooseModelFor(connId, model) {
  st.activeConnId = connId || '';
  localStorage.setItem(LS_ACTIVE_CONN, st.activeConnId);
  st.settings.model = model;
  const cc = st.conns.find((x) => x.id === connId);
  if (cc) { cc.lastModel = model; saveConns(); }
  saveSettings();
  updateModelPick();
  closeModelMenu();
  toast('مدل: ' + model + (cc ? ' — ' + cc.name : ''), 'ok');
}

function chooseModel(model) {
  const act = activeConn();
  if (act && act.models.includes(model)) return chooseModelFor(act.id, model);
  for (const cc of st.conns) if (cc.models.includes(model)) return chooseModelFor(cc.id, model);
  chooseModelFor(act ? act.id : '', model);
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
  migratePersona();
  const savedActiveId = localStorage.getItem(LS_ACTIVE_CONN) || '';
  st.activeConnId = st.conns.some((c) => c.id === savedActiveId)
    ? savedActiveId
    : (st.conns[0] ? st.conns[0].id : '');
  localStorage.setItem(LS_ACTIVE_CONN, st.activeConnId);
  stars();
  bindSettings();
  saveSettings();
  renderConvList();
  renderHistory();

  els.btnNew.onclick = () => newChat();
  els.btnSettings.onclick = () => openConns(null);
  els.btnMemory.onclick = openMemory;
  els.btnSide.onclick = () => els.frame.classList.toggle('side-hidden');
  els.btnMin.onclick = () => invokeCmd('minimize_win');
  els.btnMax.onclick = () => invokeCmd('maximize_win');
  els.btnClose.onclick = () => invokeCmd('close_win');
  // جلوگیری از بلعیده‌شدن کلیک توسط ناحیهٔ drag تایتل‌بار
  document.querySelectorAll('.win-controls, .titlebar-actions, .brand').forEach((el) =>
    el.addEventListener('mousedown', (e) => e.stopPropagation()));

  els.settingsX.onclick = () => closeModal(els.settingsModal);
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
  // صفحهٔ کامل تنظیمات — کلیک پس‌زمینه نمی‌بندد (فقط ✕ / بستن / Esc)
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
  if (els.btnAttach) {
    els.btnAttach.onclick = () => els.fileInput.click();
    els.fileInput.onchange = () => {
      addImageFiles(els.fileInput.files);
      els.fileInput.value = '';
    };
  }
  els.btnStop.onclick = stop;

  // نوار مدل/حالت (مثل کلاد) — v0.6.8: هیچ خطایی نباید بقیهٔ سیم‌کشی را بکُشد
  try { bindConns(); } catch (e) { console.error('bindConns:', e); }
  try { updateModelPick(); } catch (e) { console.error('updateModelPick:', e); }
  try { syncChips(); } catch (e) { console.error('syncChips:', e); }
  try { updateConnTab(); } catch (e) { console.error('updateConnTab:', e); }
  restoreFromDisk();
  window.addEventListener('beforeunload', () => {
    try { saveConns(); saveSettings(); } catch {}
    if (flushTimer) { clearTimeout(flushTimer); flushChats(); }
  });
  document.addEventListener('visibilitychange', () => { if (document.hidden && (dirtyChats.size || goneChats.size)) flushChats(); });
  els.modelPick.onclick = (e) => {
    e.stopPropagation();
    if (els.modelMenu.classList.contains('hidden')) openModelMenuAt(els.modelPick);
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
      if (els.modelMenu && !els.modelMenu.classList.contains('hidden')) {
        closeModelMenu();
        return;
      }
      if (els.tokenGuide && !els.tokenGuide.classList.contains('hidden')) {
        closeModal(els.tokenGuide);
        return;
      }
      if (els.settingsModal && !els.settingsModal.classList.contains('hidden')) {
        closeModal(els.settingsModal);
        return;
      }
      closeModal(els.cmdk);
      closeModal(els.memoryModal);
    }
  });

  // Connection credentials are stored in atria.conns.v1, not in the legacy settings object.
  const startupConn = activeConn();
  if (!startupConn || !String(startupConn.key || '').trim()) {
    openConns(startupConn ? startupConn.id : null);
    setTimeout(() => toast(
      startupConn ? 'کلید/توکن اتصال فعال را وارد کن' : 'برای شروع، یک اتصال جدید بساز و کلید/API را وارد کن',
      'warn'), 400);
  }
}

document.addEventListener('DOMContentLoaded', boot);






/* ============ v0.6.1 — مصرف توکن + منوی مدل + نسخهٔ برنامه ============ */

function openModelMenuAt(el) {
  if (!el) return;
  try {
    if (els.modelMenu && els.modelMenu.parentElement !== document.body) document.body.appendChild(els.modelMenu);
    if (!els.modelMenu) els.modelMenu = $('#modelMenu');
    renderModelMenu();
    const m = els.modelMenu;
    if (!m) return;
    m.style.display = '';
    m.classList.remove('hidden');
    const r = el.getBoundingClientRect();
    m.style.position = 'fixed';
    m.style.zIndex = '120';
    const w = Math.min(m.offsetWidth || 340, window.innerWidth - 16);
    const h = Math.min(m.offsetHeight || 360, window.innerHeight - 16);
    let left = Math.min(Math.max(8, r.right - w), Math.max(8, window.innerWidth - w - 8));
    let top = r.top - h - 8;
    if (top < 8) top = Math.min(r.bottom + 8, Math.max(8, window.innerHeight - h - 8));
    m.style.left = left + 'px';
    m.style.top = top + 'px';
    m.style.bottom = 'auto';
    const search = m.querySelector('#mmSearch');
    if (search) search.focus();
  } catch (err) {
    toast('منوی مدل باز نشد: ' + err, 'err');
  }
}

const USAGE_TOT = (() => {
  try { return JSON.parse(localStorage.getItem('atria.usage.v1')) || { in: 0, out: 0 }; }
  catch (e) { return { in: 0, out: 0 }; }
})();

function onUsage(i, o) {
  i = Number(i) || 0; o = Number(o) || 0;
  let est = false;
  if (!i && !o) {
    i = Math.max(1, Math.ceil((st.runText || '').length * 1.6 / 3));
    o = Math.max(1, Math.ceil((st.runText || '').length / 3));
    est = true;
  }
  USAGE_TOT.in = (USAGE_TOT.in || 0) + i;
  USAGE_TOT.out = (USAGE_TOT.out || 0) + o;
  try { localStorage.setItem('atria.usage.v1', JSON.stringify(USAGE_TOT)); } catch (e) {}
  const chip = $('#tokChip');
  if (chip && (i || o)) {
    chip.classList.remove('hidden');
    chip.textContent = (est ? '~' : '') + '◈' + fmtN(i) + ' ◇' + fmtN(o);
    chip.title = (est ? 'مصرف تخمینی — ' : 'مصرف توکن — ') + 'ورودی ' + i + ' / خروجی ' + o +
      ' — مجموع نشست: ' + USAGE_TOT.in + ' ورودی + ' + USAGE_TOT.out + ' خروجی';
  }
}

function vInit() {
  cx0();
  // نسخهٔ برنامه بالای پنجره
  try {
    if (window.__atria && window.__atria.invoke) {
      window.__atria.invoke('app_meta', {}).then((m) => {
        const el = $('#appVer');
        if (el && m && m.version) el.textContent = 'v' + m.version;
      }).catch(() => {});
    }
  } catch (e) {}
}

function cx0() {
  try {
    const chip = $('#tokChip');
    if (chip && (USAGE_TOT.in || USAGE_TOT.out)) {
      chip.classList.remove('hidden');
      chip.textContent = '◈' + fmtN(USAGE_TOT.in) + ' ◇' + fmtN(USAGE_TOT.out);
      chip.title = 'مجموع مصرف نشست: ' + USAGE_TOT.in + ' ورودی + ' + USAGE_TOT.out + ' خروجی';
    }
  } catch (e) {}
}
document.addEventListener('DOMContentLoaded', vInit);
