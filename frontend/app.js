/* Atria — UI controller (v0.2 — matches the Dawn CSS contract) */
'use strict';

const $ = (s) => document.querySelector(s);

/* ---------------- providers ---------------- */

const PROVIDERS = {
  'atria':      { label: 'آتریا — Messages API',     base: 'https://api.atria-asi.ai',                                kind: 'anthropic', model: 'Atria-Dawn-Preview' },
  'atria-cc':   { label: 'آتریا — Chat Completions', base: 'https://api.atria-asi.ai',                                kind: 'openai',    model: 'Atria-Dawn-Preview' },
  'openai':     { label: 'OpenAI',                   base: 'https://api.openai.com/v1',                               kind: 'openai',    model: 'gpt-4o-mini' },
  'gemini':     { label: 'Google Gemini',            base: 'https://generativelanguage.googleapis.com/v1beta/openai', kind: 'openai',    model: 'gemini-2.5-flash' },
  'groq':       { label: 'Groq',                     base: 'https://api.groq.com/openai/v1',                          kind: 'openai',    model: 'llama-3.3-70b-versatile' },
  'deepseek':   { label: 'DeepSeek',                 base: 'https://api.deepseek.com/v1',                             kind: 'openai',    model: 'deepseek-chat' },
  'openrouter': { label: 'OpenRouter',               base: 'https://openrouter.ai/api/v1',                            kind: 'openai',    model: 'openrouter/auto' },
  'custom':     { label: 'سفارشی…',                   base: '',                                                        kind: 'openai',    model: '' },
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
};
const LS_SETTINGS = 'atria.settings.v2';
const LS_CHATS = 'atria.chats.v1';
const LS_CURRENT = 'atria.current.v1';

const st = {
  settings: loadSettings(),
  chats: loadChats(),
  currentId: localStorage.getItem(LS_CURRENT) || null,
  sending: false,
  runText: '',
  live: null,
  streamEl: null,
  stackEl: null,
  thinkEl: null,
  cursorHolder: null,
};

function loadSettings() {
  try { return { ...DEFAULTS, ...JSON.parse(localStorage.getItem(LS_SETTINGS) || '{}') }; }
  catch { return { ...DEFAULTS }; }
}
function saveSettings() {
  localStorage.setItem(LS_SETTINGS, JSON.stringify(st.settings));
  els.modelChip.textContent = st.settings.model || '—';
}
function loadChats() {
  try { return JSON.parse(localStorage.getItem(LS_CHATS) || '[]'); } catch { return []; }
}
function saveChats() { localStorage.setItem(LS_CHATS, JSON.stringify(st.chats)); }
function currentChat() { return st.chats.find((c) => c.id === st.currentId) || null; }

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
    settingsModal: $('#settingsModal'), settingsX: $('#settingsX'), settingsClose: $('#settingsClose'),
    memoryModal: $('#memoryModal'), memoryX: $('#memoryX'), memoryClose: $('#memoryClose'),
    memList: $('#memList'), memClear: $('#memClear'), btnWipe: $('#btnWipe'),
    cmdk: $('#cmdk'), cmdkInput: $('#cmdkInput'), cmdkList: $('#cmdkList'), cmdkX: $('#cmdkX'),
    toasts: $('#toasts'),
    provVal: $('#provVal'), keyVal: $('#keyVal'), keyEye: $('#keyEye'),
    modelVal: $('#modelVal'), baseVal: $('#baseVal'), kindVal: $('#kindVal'),
    tempVal: $('#tempVal'), tempOut: $('#tempOut'),
    thinkVal: $('#thinkVal'), memVal: $('#memVal'), streamVal: $('#streamVal'),
    sysVal: $('#sysVal'), maxTokVal: $('#maxTokVal'),
    toolsVal: $('#toolsVal'), fileToolsVal: $('#fileToolsVal'), wsVal: $('#wsVal'),
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
  els.provVal.innerHTML = Object.entries(PROVIDERS)
    .map(([id, p]) => `<option value="${id}">${p.label}</option>`).join('');
  els.provVal.value = st.settings.provider || 'atria';
  els.keyVal.value = st.settings.api_key || '';
  els.modelVal.value = st.settings.model || '';
  els.baseVal.value = st.settings.base_url || '';
  els.kindVal.value = st.settings.api_kind || 'anthropic';

  els.provVal.onchange = () => {
    const p = PROVIDERS[els.provVal.value];
    st.settings.provider = els.provVal.value;
    if (p) {
      if (els.provVal.value !== 'custom' || !els.baseVal.value) els.baseVal.value = p.base;
      if (p.model && els.provVal.value !== 'custom') els.modelVal.value = p.model;
      els.kindVal.value = p.kind;
      st.settings.api_kind = p.kind;
    }
    st.settings.base_url = els.baseVal.value.trim();
    st.settings.model = els.modelVal.value.trim();
    saveSettings();
  };
  els.keyVal.oninput = () => { st.settings.api_key = els.keyVal.value.trim(); saveSettings(); };
  els.modelVal.oninput = () => { st.settings.model = els.modelVal.value.trim(); saveSettings(); };
  els.baseVal.oninput = () => { st.settings.base_url = els.baseVal.value.trim(); saveSettings(); };
  els.kindVal.onchange = () => { st.settings.api_kind = els.kindVal.value; saveSettings(); };
  els.keyEye.onclick = () => {
    const show = els.keyVal.type === 'password';
    els.keyVal.type = show ? 'text' : 'password';
    els.keyEye.textContent = show ? '🙈' : '👁';
  };

  els.tempVal.value = st.settings.temp;
  els.tempOut.textContent = Number(st.settings.temp).toFixed(1);
  els.thinkVal.checked = !!st.settings.thinking;
  els.memVal.checked = !!st.settings.mem;
  els.streamVal.checked = !!st.settings.stream;
  els.tempVal.oninput = () => {
    st.settings.temp = Number(els.tempVal.value);
    els.tempOut.textContent = st.settings.temp.toFixed(1);
    saveSettings();
  };
  els.thinkVal.onchange = () => { st.settings.thinking = els.thinkVal.checked; saveSettings(); };
  els.memVal.onchange = () => { st.settings.mem = els.memVal.checked; saveSettings(); };
  els.streamVal.onchange = () => { st.settings.stream = els.streamVal.checked; saveSettings(); };

  els.sysVal.value = st.settings.system || '';
  els.maxTokVal.value = st.settings.max_tokens || 4096;
  els.toolsVal.checked = !!st.settings.tools_enabled;
  els.fileToolsVal.checked = !!st.settings.file_tools;
  els.wsVal.value = st.settings.workspace || '';
  els.sysVal.oninput = () => { st.settings.system = els.sysVal.value; saveSettings(); };
  els.maxTokVal.onchange = () => {
    st.settings.max_tokens = Math.max(256, Number(els.maxTokVal.value) || 4096);
    els.maxTokVal.value = st.settings.max_tokens;
    saveSettings();
  };
  els.toolsVal.onchange = () => { st.settings.tools_enabled = els.toolsVal.checked; saveSettings(); };
  els.fileToolsVal.onchange = () => { st.settings.file_tools = els.fileToolsVal.checked; saveSettings(); };
  els.wsVal.oninput = () => { st.settings.workspace = els.wsVal.value.trim(); saveSettings(); };

  document.querySelectorAll('.stab').forEach((b) => b.addEventListener('click', () => {
    document.querySelectorAll('.stab').forEach((x) => x.classList.toggle('active', x === b));
    document.querySelectorAll('.stab-body').forEach((x) =>
      x.classList.toggle('active', x.dataset.tab === b.dataset.tab));
  }));
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
  const chat = { id: 'c' + Date.now(), title: 'گفتگوی جدید', createdAt: Date.now(), messages: [] };
  st.chats.unshift(chat);
  st.currentId = chat.id;
  saveChats();
  localStorage.setItem(LS_CURRENT, chat.id);
  renderConvList();
  renderHistory();
  if (!silent) els.input.focus();
}

function switchChat(id) {
  if (st.sending) return toast('ابتدا تولید را متوقف کن', 'warn');
  st.currentId = id;
  localStorage.setItem(LS_CURRENT, id);
  renderConvList();
  renderHistory();
  closeModal(els.cmdk);
}

function deleteChat(id) {
  st.chats = st.chats.filter((c) => c.id !== id);
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

function bubbleEl(role, text) {
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
    els.messages.appendChild(bubbleEl(m.role === 'user' ? 'user' : 'ai', m.plain || m.text || '…'));
  }
  scrollBottom(true);
}

/* ------- live pieces: thinking box / tool cards / cursor ------- */

function ensureThink() {
  if (st.thinkEl && st.thinkEl.isConnected) return st.thinkEl;
  const box = document.createElement('div');
  box.className = 'think-box has';
  box.innerHTML =
    '<button class="think-head"><span class="think-orb"></span><span class="chev">▾</span>' +
    '<span class="think-title">در حال فکر کردن…</span>' +
    '<span class="dots"><i></i><i></i><i></i></span></button>' +
    '<div class="think-body"></div>';
  box.querySelector('.think-head').onclick = () => box.classList.toggle('open');
  st.stackEl.appendChild(box);
  st.thinkEl = box;
  return box;
}

function finishThink() {
  if (st.thinkEl) {
    st.thinkEl.classList.remove('has');
    st.thinkEl.querySelector('.think-title').textContent = 'تفکر مدل';
    st.thinkEl = null;
  }
}

function toolCardEl(label, name, input) {
  const card = document.createElement('div');
  card.className = 'tool-card';
  card.innerHTML =
    '<div class="tool-head"><span class="tool-ic">⚙️</span><span class="tool-label"></span>' +
    '<span class="tool-chip"></span><span class="tool-state">در حال اجرا…</span></div>' +
    '<div class="tool-io"><div class="tool-in"></div><div class="tool-out hidden"></div></div>';
  card.querySelector('.tool-label').textContent = label || name || 'ابزار';
  card.querySelector('.tool-chip').textContent = name || '';
  card.querySelector('.tool-in').textContent = JSON.stringify(input ?? {}, null, 2);
  return card;
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

/* ---------------- send flow ---------------- */

function send() {
  const text = els.input.value.trim();
  if (!text || st.sending) return;
  const chat = currentChat();
  if (!chat) { newChat(true); return send(); }

  if (!st.settings.api_key) {
    openModal(els.settingsModal);
    return toast('ابتدا کلید API خودت را در تنظیمات وارد کن', 'warn');
  }

  chat.messages.push({ role: 'user', text, plain: text, ts: Date.now() });
  if (chat.title === 'گفتگوی جدید') {
    chat.title = text.slice(0, 42) + (text.length > 42 ? '…' : '');
    renderConvList();
  }
  els.empty.classList.add('hidden', 'off');
  els.messages.appendChild(bubbleEl('user', text));
  els.input.value = '';
  autosize();
  scrollBottom();

  st.sending = true;
  st.runText = '';
  st.streamEl = null;
  st.stackEl = null;
  st.thinkEl = null;
  st.live = null;
  liveMdEl(); // create the live ai message shell
  startBlink();
  els.btnSend.classList.add('hidden');
  els.btnStop.classList.remove('hidden');

  const history = chat.messages.map((m) => ({ role: m.role, content: m.text }));
  if (!window.__atria || !window.__atria.chat_send) {
    return failRun('پل ارتباطی IPC آماده نیست — برنامه را دوباره باز کن');
  }
  window.__atria.chat_send({
    payload: {
      api_key: st.settings.api_key,
      base_url: st.settings.base_url || 'https://api.atria-asi.ai',
      model: st.settings.model || 'Atria-Dawn-Preview',
      max_tokens: Number(st.settings.max_tokens) || 4096,
      temperature: Number(st.settings.temp),
      system: st.settings.system || '',
      tools_enabled: !!st.settings.tools_enabled,
      stream: st.settings.stream !== false,
      kind: st.settings.api_kind || 'anthropic',
      file_tools: !!st.settings.file_tools,
      workspace: st.settings.workspace || '',
      messages: history,
    },
  }).catch((e) => failRun(e && e.message ? e.message : String(e)));
}

function stop() { window.__atria.chat_stop(); }

/* ------- live event handlers ------- */

function onThinking(delta) {
  if (!st.settings.thinking) return;
  closeLive();
  const box = ensureThink();
  box.querySelector('.think-body').textContent += delta;
  scrollBottom(true);
}

function onText(delta) {
  st.runText += delta;
  if (st.cursorHolder) stopBlink();
  ensureLive().push(delta);
  scrollBottom(true);
}

function onToolStart(label, name, input) {
  closeLive();
  stopBlink();
  st.stackEl.appendChild(toolCardEl(label, name, input));
  scrollBottom(true);
}

function onToolEnd(card, ok, output) {
  closeLive();
  if (!card) return;
  card.classList.add(ok ? 'done' : 'failed');
  card.querySelector('.tool-state').textContent = ok ? '✓ انجام شد' : '✕ خطا';
  const out = card.querySelector('.tool-out');
  out.classList.remove('hidden');
  out.textContent = (output || '').slice(0, 4000);
  scrollBottom(true);
}

function failRun(msg) {
  stopBlink();
  closeLive();
  finishThink();
  st.sending = false;
  els.btnSend.classList.remove('hidden');
  els.btnStop.classList.add('hidden');
  if (st.streamEl) {
    st.streamEl.classList.remove('streaming');
    const e = document.createElement('div');
    e.className = 'tool-out';
    e.style.borderColor = 'rgba(248,113,113,.35)';
    e.textContent = 'خطا: ' + msg;
    st.stackEl.appendChild(e);
    scrollBottom();
  }
  toast('خطا: ' + msg, 'err');
}

function finishRun(newMessages, finalText) {
  stopBlink();
  closeLive();
  finishThink();
  st.sending = false;
  els.btnSend.classList.remove('hidden');
  els.btnStop.classList.add('hidden');
  if (st.streamEl) st.streamEl.classList.remove('streaming');
  const chat = currentChat();
  const plain = finalText && finalText.trim() ? finalText : st.runText || '(پاسخ خالی)';
  if (newMessages && newMessages.length) {
    for (const m of newMessages) {
      chat.messages.push({
        role: m.role === 'user' ? 'user' : 'ai',
        text: m.text || m.plain || '',
        plain: m.plain || m.text || '',
        ts: Date.now(),
      });
    }
  } else {
    chat.messages.push({ role: 'ai', text: st.runText || plain, plain, ts: Date.now() });
  }
  saveChats();
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
      finishThink();
      const r = document.createElement('div');
      r.className = 'round-chip';
      r.textContent = '↻ مرحلهٔ ابزار ' + e.payload.round;
      st.stackEl.appendChild(r);
      scrollBottom(true);
    }
  });
  let lastCard = null;
  t.listen('atria:tool_start', (e) => {
    onToolStart(e.payload.label || e.payload.name, e.payload.name, e.payload.input);
    lastCard = st.stackEl.lastElementChild;
  });
  t.listen('atria:tool_end', (e) => onToolEnd(lastCard, e.payload.ok, e.payload.output));
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

function boot() {
  cacheEls();
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
  els.btnWipe.onclick = () => {
    if (confirm('همهٔ گفتگوها حذف شوند؟')) {
      st.chats = [];
      saveChats();
      newChat(true);
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
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(); }
  });
  els.btnSend.onclick = send;
  els.btnStop.onclick = stop;
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
