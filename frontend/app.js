/* Atria — UI controller */
'use strict';

const $ = (s) => document.querySelector(s);
const $$ = (s) => document.querySelectorAll(s);

/* ---------------- providers ---------------- */

const PROVIDERS = {
  'atria':       { label: 'آتریا — Messages API',      base: 'https://api.atria-asi.ai',                                        kind: 'anthropic', model: 'Atria-Dawn-Preview' },
  'atria-cc':    { label: 'آتریا — Chat Completions',  base: 'https://api.atria-asi.ai',                                        kind: 'openai',    model: 'Atria-Dawn-Preview' },
  'openai':      { label: 'OpenAI',                    base: 'https://api.openai.com/v1',                                       kind: 'openai',    model: 'gpt-4o-mini' },
  'gemini':      { label: 'Google Gemini',             base: 'https://generativelanguage.googleapis.com/v1beta/openai',         kind: 'openai',    model: 'gemini-2.5-flash' },
  'groq':        { label: 'Groq',                      base: 'https://api.groq.com/openai/v1',                                  kind: 'openai',    model: 'llama-3.3-70b-versatile' },
  'deepseek':    { label: 'DeepSeek',                  base: 'https://api.deepseek.com/v1',                                     kind: 'openai',    model: 'deepseek-chat' },
  'openrouter':  { label: 'OpenRouter',                base: 'https://openrouter.ai/api/v1',                                    kind: 'openai',    model: 'openrouter/auto' },
  'custom':      { label: 'سفارشی…',                    base: '',                                                                kind: 'openai',    model: '' },
};

/* ---------------- state ---------------- */

const DEFAULTS = {
  // اتصال — کلید API دیگر در برنامه تعبیه نشده؛ کاربر آن را وارد می‌کند
  provider: 'atria',
  api_key: '',
  base_url: 'https://api.atria-asi.ai',
  api_kind: 'anthropic',
  model: 'Atria-Dawn-Preview',
  // رفتار
  thinking: true,
  mem: true,
  temp: 0.7,
  stream: true,
  // سیستم
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
  runThink: '',
  timers: { blink: null, type: null },
};

function loadSettings() {
  try {
    const s = JSON.parse(localStorage.getItem(LS_SETTINGS) || '{}');
    // migration from v1: keep values, drop any legacy embedded key if user never set one
    return { ...DEFAULTS, ...s };
  } catch { return { ...DEFAULTS }; }
}
function saveSettings() {
  localStorage.setItem(LS_SETTINGS, JSON.stringify(st.settings));
  els.modelChip.textContent = 'مدل: ' + (st.settings.model || '—');
}
function loadChats() {
  try { return JSON.parse(localStorage.getItem(LS_CHATS) || '[]'); } catch { return []; }
}
function saveChats() { localStorage.setItem(LS_CHATS, JSON.stringify(st.chats)); }

function currentChat() { return st.chats.find((c) => c.id === st.currentId) || null; }

/* ---------------- dom refs ---------------- */

let els = {};
function cacheEls() {
  els = {
    chatList: $('#chatList'), chatTitle: $('#chatTitle'), modelChip: $('#modelChip'),
    chat: $('#chat'), empty: $('#empty'), input: $('#input'), send: $('#send'),
    newChat: $('#newChat'), clearBtn: $('#clearBtn'), settingsBtn: $('#settingsBtn'),
    settingsPanel: $('#settingsPanel'), keyVal: $('#keyVal'), modelVal: $('#modelVal'),
    baseVal: $('#baseVal'), kindVal: $('#kindVal'), provVal: $('#provVal'),
    tempVal: $('#tempVal'), tempOut: $('#tempOut'), thinkVal: $('#thinkVal'),
    memVal: $('#memVal'), streamVal: $('#streamVal'), sysVal: $('#sysVal'),
    maxTokVal: $('#maxTokVal'), toolsVal: $('#toolsVal'), fileToolsVal: $('#fileToolsVal'),
    wsVal: $('#wsVal'), closeSettings: $('#closeSettings'), wipe: $('#wipe'),
    hint: $('#hint'), memoryBtn: $('#memoryBtn'), memoryPanel: $('#memoryPanel'),
    closeMemory: $('#closeMemory'), memList: $('#memList'), memClear: $('#memClear'),
    toast: $('#toast'), cmdk: $('#cmdk'), cmdkInput: $('#cmdkInput'), cmdkList: $('#cmdkList'),
  };
}

/* ---------------- settings ---------------- */

function bindSettings() {
  // اتصال
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
      if (p.model && (!els.modelVal.value || els.provVal.value !== 'custom')) els.modelVal.value = p.model;
      els.kindVal.value = p.kind;
      st.settings.api_kind = p.kind;
    }
    st.settings.base_url = els.baseVal.value;
    st.settings.model = els.modelVal.value;
    saveSettings();
  };
  els.keyVal.oninput = () => { st.settings.api_key = els.keyVal.value.trim(); saveSettings(); };
  els.modelVal.oninput = () => { st.settings.model = els.modelVal.value.trim(); saveSettings(); };
  els.baseVal.oninput = () => { st.settings.base_url = els.baseVal.value.trim(); saveSettings(); };
  els.kindVal.onchange = () => { st.settings.api_kind = els.kindVal.value; saveSettings(); };

  // رفتار
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

  // سیستم
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

  // تب‌های تنظیمات
  $$('.stab').forEach((b) => b.addEventListener('click', () => {
    $$('.stab').forEach((x) => x.classList.toggle('active', x === b));
    $$('.stab-body').forEach((x) => x.classList.toggle('active', x.dataset.tab === b.dataset.tab));
  }));
}

function openSettings() {
  els.settingsPanel.classList.add('open');
  els.hint.classList.add('open');
}
function closeSettings() {
  els.settingsPanel.classList.remove('open');
  els.hint.classList.remove('open');
}

/* ---------------- chat list ---------------- */

function renderChatList() {
  els.chatList.innerHTML = '';
  if (!st.chats.length) {
    els.chatList.innerHTML = '<div style="color:var(--text-dim);font-size:12px;padding:10px 12px">هنوز گفتگویی نیست</div>';
    return;
  }
  for (const c of st.chats) {
    const item = document.createElement('div');
    item.className = 'chat-item' + (c.id === st.currentId ? ' active' : '');
    const t = document.createElement('span');
    t.className = 'title';
    t.textContent = c.title;
    const del = document.createElement('button');
    del.className = 'del';
    del.title = 'حذف';
    del.textContent = '✕';
    del.onclick = (e) => { e.stopPropagation(); deleteChat(c.id); };
    item.appendChild(t);
    item.appendChild(del);
    item.onclick = () => switchChat(c.id);
    els.chatList.appendChild(item);
  }
}

function newChat(silent) {
  const chat = {
    id: 'c' + Date.now(),
    title: 'گفتگوی جدید',
    createdAt: Date.now(),
    messages: [],
  };
  st.chats.unshift(chat);
  st.currentId = chat.id;
  saveChats();
  localStorage.setItem(LS_CURRENT, chat.id);
  renderChatList();
  renderHistory();
  if (!silent) els.input.focus();
}

function switchChat(id) {
  if (st.sending) return toast('ابتدا تولید را متوقف کن');
  st.currentId = id;
  localStorage.setItem(LS_CURRENT, id);
  renderChatList();
  renderHistory();
  closeCmdk();
}

function deleteChat(id) {
  st.chats = st.chats.filter((c) => c.id !== id);
  saveChats();
  if (st.currentId === id) {
    st.currentId = st.chats[0] ? st.chats[0].id : null;
    if (!st.currentId) newChat(true);
    else localStorage.setItem(LS_CURRENT, st.currentId);
  }
  renderChatList();
  renderHistory();
}

/* ---------------- rendering ---------------- */

function bubbleEl(role, text, animate) {
  const wrap = document.createElement('div');
  wrap.className = 'msg ' + role + (animate ? ' pop-in' : '');
  const b = document.createElement('div');
  b.className = 'bubble';
  md.renderAll(b, text);
  wrap.appendChild(b);
  return wrap;
}

function toolChip(label, ok, output) {
  const chip = document.createElement('div');
  chip.className = 'tool-chip' + (ok === false ? ' err' : '');
  chip.innerHTML = '<span class="tool-dot"></span><b>' + md.renderInline(label) + '</b>';
  chip.title = (output || '').slice(0, 400);
  chip.onclick = () => toast((output || '').slice(0, 220) || '…');
  return chip;
}

function thinkingBubble(text) {
  const d = document.createElement('div');
  d.className = 'think-bubble';
  d.textContent = text;
  return d;
}

function renderHistory() {
  const chat = currentChat();
  els.chat.innerHTML = '';
  if (!chat || !chat.messages.length) {
    els.empty.style.display = 'flex';
    els.chatTitle.textContent = chat ? chat.title : '';
    return;
  }
  els.empty.style.display = 'none';
  els.chatTitle.textContent = chat.title;
  for (const m of chat.messages) {
    els.chat.appendChild(bubbleEl(m.role === 'user' ? 'user' : 'assistant', m.plain || '…', false));
  }
  scrollBottom(true);
}

function scrollBottom(instant) {
  if (instant) els.chat.scrollTop = els.chat.scrollHeight;
  else els.chat.scrollTo({ top: els.chat.scrollHeight, behavior: 'smooth' });
}

/* ---------------- send flow ---------------- */

function send() {
  const text = els.input.value.trim();
  if (!text || st.sending) return;
  const chat = currentChat();
  if (!chat) { newChat(true); return send(); }

  if (!st.settings.api_key) {
    openSettings();
    return toast('ابتدا کلید API خودت را در تنظیمات وارد کن');
  }

  // user bubble
  chat.messages.push({ role: 'user', text, plain: text, ts: Date.now() });
  if (chat.title === 'گفتگوی جدید') {
    chat.title = text.slice(0, 42) + (text.length > 42 ? '…' : '');
    els.chatTitle.textContent = chat.title;
    renderChatList();
  }
  els.empty.style.display = 'none';
  els.chat.appendChild(bubbleEl('user', text, true));
  els.input.value = '';
  autosize();
  scrollBottom();

  // assistant placeholder + live renderer
  const wrap = document.createElement('div');
  wrap.className = 'msg assistant pop-in';
  const bub = document.createElement('div');
  bub.className = 'bubble streaming';
  wrap.appendChild(bub);
  els.chat.appendChild(wrap);
  scrollBottom();

  st.sending = true;
  st.runText = '';
  st.runThink = '';
  st.live = null;         // current live answer section (md.createLiveStream)
  st.streamEl = bub;      // whole bubble (thinking + tools + answers)
  startBlink(bub);

  const history = chat.messages.map((m) => ({ role: m.role, content: m.text }));

  window.__atria.chat_send({
    payload: {
      api_key: st.settings.api_key,
      base_url: st.settings.base_url || 'https://api.atria-asi.ai',
      model: st.settings.model || 'Atria-Dawn-Preview',
      max_tokens: Number(st.settings.max_tokens) || 4096,
      temperature: Number(st.settings.temp),
      system: st.settings.mem ? st.settings.system : (st.settings.system || ''),
      tools_enabled: !!st.settings.tools_enabled,
      stream: st.settings.stream !== false,
      kind: st.settings.api_kind || 'anthropic',
      file_tools: !!st.settings.file_tools,
      workspace: st.settings.workspace || '',
      messages: history,
    },
  }).catch((e) => {
    failRun(e && e.message ? e.message : String(e));
  });
}

function stop() { window.__atria.chat_stop(); }

function startBlink(bub) {
  const c = document.createElement('span');
  c.className = 'cursor';
  st.cursorEl = c;
  const holder = document.createElement('div');
  holder.className = 'para';
  holder.appendChild(c);
  bub.appendChild(holder);
  st.cursorHolder = holder;
}
function stopBlink() {
  if (st.cursorHolder && st.cursorHolder.parentNode) st.cursorHolder.parentNode.removeChild(st.cursorHolder);
  st.cursorHolder = null;
}

function ensureLive() {
  if (!st.live) {
    if (st.cursorHolder) stopBlink();
    const sec = document.createElement('div');
    sec.className = 'stream-answer';
    st.streamEl.appendChild(sec);
    st.live = md.createLiveStream(sec);
  }
  return st.live;
}
function closeLive() {
  if (st.live) { st.live.done(); st.live = null; }
}

function onThinking(delta) {
  if (!st.settings.thinking) return;
  closeLive();
  st.runThink += delta;
  // فقط append — هیچ رندر مجددی رخ نمی‌دهد
  let tb = st.streamEl.querySelector('.think-bubble:last-of-type');
  const last = st.streamEl.lastElementChild;
  if (!tb || (last && !last.classList.contains('think-bubble'))) {
    tb = thinkingBubble('');
    st.streamEl.appendChild(tb);
  }
  tb.textContent += delta;
  scrollBottom(true);
}

function onText(delta) {
  if (st.cursorHolder) stopBlink();
  st.runText += delta;
  ensureLive().push(delta);
  scrollBottom(true);
}

function onToolStart(label) {
  closeLive();
  if (st.cursorHolder) stopBlink();
  st.streamEl.appendChild(toolChip(label + ' …', true, 'در حال اجرا…'));
  scrollBottom(true);
}

function onToolEnd(label, ok, output) {
  closeLive();
  st.streamEl.appendChild(toolChip(label, ok, output));
  scrollBottom(true);
}

function failRun(msg) {
  stopBlink();
  closeLive();
  st.sending = false;
  const e = document.createElement('div');
  e.className = 'stream-error';
  e.textContent = 'خطا: ' + msg;
  st.streamEl.appendChild(e);
  st.streamEl.classList.remove('streaming');
  scrollBottom();
}

function finishRun(newMessages, finalText) {
  stopBlink();
  closeLive();
  st.sending = false;
  st.streamEl.classList.remove('streaming');
  const chat = currentChat();
  const plain = finalText && finalText.trim() ? finalText : st.runText || '(پاسخ خالی)';
  if (newMessages && newMessages.length) {
    for (const m of newMessages) chat.messages.push({ role: m.role === 'user' ? 'user' : 'assistant', text: m.text || m.plain || '', plain: m.plain || m.text || '', ts: Date.now() });
  } else {
    chat.messages.push({ role: 'assistant', text: st.runText || plain, plain, ts: Date.now() });
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
      const r = document.createElement('div');
      r.className = 'round-chip';
      r.textContent = '↻ مرحلهٔ ابزار ' + e.payload.round;
      st.streamEl.appendChild(r);
      scrollBottom(true);
    }
  });
  t.listen('atria:tool_start', (e) => onToolStart(e.payload.label || e.payload.name));
  t.listen('atria:tool_end', (e) => onToolEnd(e.payload.label || e.payload.name, e.payload.ok, e.payload.output));
  t.listen('atria:done', (e) => finishRun(e.payload.new_messages, e.payload.final_text));
  t.listen('atria:error', (e) => failRun(e.payload.message || 'unknown'));
  t.listen('atria:stopped', () => {
    failRun('متوقف شد');
    toast('تولید متوقف شد');
  });
}

/* ---------------- memory modal ---------------- */

async function openMemory() {
  els.memoryPanel.classList.add('open');
  els.memList.innerHTML = '<div class="spinner"></div>';
  try {
    const notes = await window.__atria.memory_list();
    els.memList.innerHTML = '';
    if (!notes || !notes.length) {
      els.memList.innerHTML = '<div style="color:var(--text-dim);font-size:12px;padding:8px 2px">حافظه خالی است</div>';
      return;
    }
    for (const n of notes) {
      const el = document.createElement('div');
      el.className = 'note';
      el.innerHTML = '<div class="note-title"></div><div class="note-content"></div><div class="note-ts"></div>';
      el.querySelector('.note-title').textContent = n.title;
      el.querySelector('.note-content').textContent = n.content;
      el.querySelector('.note-ts').textContent = n.ts;
      els.memList.appendChild(el);
    }
  } catch (e) {
    els.memList.innerHTML = '<div class="stream-error">خطا در بارگذاری حافظه</div>';
  }
}

/* ---------------- cmd-k ---------------- */

function openCmdk() {
  els.cmdk.classList.add('open');
  els.cmdkInput.value = '';
  renderCmdkList('');
  els.cmdkInput.focus();
}
function closeCmdk() { els.cmdk.classList.remove('open'); }
function renderCmdkList(q) {
  const list = st.chats.filter((c) => !q || c.title.includes(q));
  els.cmdkList.innerHTML = '';
  if (!list.length) {
    els.cmdkList.innerHTML = '<div style="color:var(--text-dim);font-size:12px;padding:8px 10px">چیزی پیدا نشد</div>';
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

/* ---------------- misc ui ---------------- */

let toastTimer = null;
function toast(msg) {
  els.toast.textContent = msg;
  els.toast.classList.add('show');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => els.toast.classList.remove('show'), 2600);
}

function autosize() {
  els.input.style.height = 'auto';
  els.input.style.height = Math.min(220, els.input.scrollHeight) + 'px';
  els.send.disabled = !els.input.value.trim() || st.sending;
  els.send.classList.toggle('loading', st.sending);
  els.send.textContent = st.sending ? '■' : '➤';
}

/* ---------------- boot ---------------- */

function boot() {
  cacheEls();
  bindSettings();
  saveSettings(); // refresh model chip

  els.newChat.onclick = () => { if (!st.sending) { newChat(); } };
  els.clearBtn.onclick = () => {
    const chat = currentChat();
    if (chat && chat.messages.length) { chat.messages = []; saveChats(); renderHistory(); toast('گفتگو پاک شد'); }
  };
  els.settingsBtn.onclick = (e) => { e.stopPropagation(); els.settingsPanel.classList.toggle('open'); els.hint.classList.toggle('open'); };
  els.closeSettings.onclick = closeSettings;
  els.wipe.onclick = () => {
    if (confirm('همهٔ گفتگوها حذف شوند؟')) {
      st.chats = [];
      saveChats();
      newChat(true);
      toast('همه گفتگوها حذف شدند');
    }
  };
  els.memoryBtn.onclick = openMemory;
  els.closeMemory.onclick = () => els.memoryPanel.classList.remove('open');
  els.memClear.onclick = async () => {
    await window.__atria.memory_clear();
    openMemory();
    toast('حافظه پاک شد');
  };
  document.addEventListener('click', (e) => {
    if (els.settingsPanel.classList.contains('open') && !els.settingsPanel.contains(e.target) && e.target !== els.settingsBtn) {
      closeSettings();
    }
  });

  els.input.addEventListener('input', autosize);
  els.input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(); }
  });
  els.send.onclick = () => (st.sending ? stop() : send());
  autosize();

  listenEvents();

  window.addEventListener('keydown', (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      els.cmdk.classList.contains('open') ? closeCmdk() : openCmdk();
    }
    if (e.key === 'Escape') { closeCmdk(); els.memoryPanel.classList.remove('open'); closeSettings(); }
  });
  els.cmdk.addEventListener('click', (e) => { if (e.target === els.cmdk) closeCmdk(); });
  els.cmdkInput.addEventListener('input', () => renderCmdkList(els.cmdkInput.value.trim()));
  els.memoryPanel.addEventListener('click', (e) => { if (e.target === els.memoryPanel) els.memoryPanel.classList.remove('open'); });

  if (!st.currentId || !currentChat()) newChat(true);
  else { renderChatList(); renderHistory(); }

  // اولین اجرا: کلیدی تعبیه نشده — تنظیمات را باز کن
  if (!st.settings.api_key) {
    openSettings();
    setTimeout(() => toast('برای شروع، کلید API خودت را وارد کن'), 400);
  }
}

document.addEventListener('DOMContentLoaded', boot);
