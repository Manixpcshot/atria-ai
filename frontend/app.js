/* ==========================================================================
   Atria Dawn — UI logic (vanilla JS + Tauri IPC)
   ========================================================================== */
'use strict';

const $ = (s, el = document) => el.querySelector(s);
const T = window.__TAURI__ || {};

/* ============================ config ============================ */
const DEFAULT_SYSTEM = `You are Atria (آتریا) — a capable, warm AI agent living in the "Atria Dawn" desktop app powered by a Rust engine.

Personality & style:
- Practical, proactive and friendly. Reply in the SAME language the user writes (Persian by default).
- Format answers with Markdown. Prefer concise but complete answers.

You are an AGENT:
- When a tool can help (exact math, current date/time, saving or recalling notes), CALL the tool instead of guessing or claiming you cannot.
- Use "remember" whenever the user asks you to keep something, and "recall" before answering questions about saved notes.
- After tool results, always give the user a clear final answer in their language.`;

const DEFAULTS = {
  api_key: '',
  base_url: 'https://api.atria-asi.ai',
  model: 'Atria-Dawn-Preview',
  max_tokens: 4096,
  temperature: 0.7,
  tools_enabled: true,
  stream: true,
  show_thinking: true,
  system: DEFAULT_SYSTEM,
};

function loadCfg() {
  try { return { ...DEFAULTS, ...JSON.parse(localStorage.getItem('atria.cfg') || '{}') }; }
  catch { return { ...DEFAULTS }; }
}
let cfg = loadCfg();
function persistCfg() {
  try { localStorage.setItem('atria.cfg', JSON.stringify(cfg)); } catch { /* ignore */ }
}

/* ============================ conversations ============================ */
function loadConvs() {
  try {
    const v = JSON.parse(localStorage.getItem('atria.convs') || '[]');
    return Array.isArray(v) ? v : [];
  } catch { return []; }
}
let convs = loadConvs();
let curId = localStorage.getItem('atria.cur') || null;

function persistConvs() {
  try { localStorage.setItem('atria.convs', JSON.stringify(convs)); }
  catch { toast('حافظه پر شد — گفتگوها ذخیره نشدند', 'warn'); }
}

function currentConv(create = true) {
  let c = convs.find((x) => x.id === curId);
  if (!c && create) {
    c = {
      id: 'c' + Date.now().toString(36) + Math.random().toString(36).slice(2, 6),
      title: 'گفتگوی جدید',
      ts: Date.now(),
      messages: [],
    };
    convs.unshift(c);
    curId = c.id;
    localStorage.setItem('atria.cur', curId);
  }
  return c;
}

function makeTitle(conv) {
  for (const m of conv.messages) {
    if (m.role === 'user') {
      for (const b of m.content || []) {
        if (b.type === 'text' && b.text.trim()) return b.text.trim().slice(0, 36);
      }
    }
  }
  return 'گفتگوی جدید';
}

function relDate(ts) {
  const d = Date.now() - ts;
  const m = Math.floor(d / 60000);
  if (m < 1) return 'همین حالا';
  if (m < 60) return `${m} دقیقه پیش`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} ساعت پیش`;
  if (h < 48) return 'دیروز';
  return new Date(ts).toLocaleDateString('fa-IR');
}

/* ============================ toasts ============================ */
function toast(msg, kind = '') {
  const el = document.createElement('div');
  el.className = `toast ${kind}`;
  el.textContent = msg;
  $('#toasts').appendChild(el);
  setTimeout(() => {
    el.classList.add('out');
    setTimeout(() => el.remove(), 350);
  }, 3400);
}

/* ============================ window controls ============================ */
function initWindow() {
  try {
    const w = T.window?.getCurrentWindow ? T.window.getCurrentWindow() : null;
    if (!w) return;
    $('#btnMin')?.addEventListener('click', () => w.minimize());
    $('#btnMax')?.addEventListener('click', () => w.toggleMaximize());
    $('#btnClose')?.addEventListener('click', () => w.close());
  } catch { /* running in a plain browser */ }
}

/* ============================ stars ============================ */
function initStars() {
  const box = $('#stars');
  for (let i = 0; i < 46; i++) {
    const s = document.createElement('i');
    const size = (Math.random() * 1.8 + 0.7).toFixed(1);
    s.style.cssText =
      `left:${(Math.random() * 100).toFixed(2)}%;top:${(Math.random() * 100).toFixed(2)}%;` +
      `width:${size}px;height:${size}px;animation-delay:${(Math.random() * 4).toFixed(2)}s;` +
      `animation-duration:${(3 + Math.random() * 4).toFixed(2)}s`;
    box.appendChild(s);
  }
}

/* ============================ rendering: sidebar ============================ */
function renderSidebar() {
  const list = $('#convList');
  list.innerHTML = '';
  for (const c of convs) {
    const el = document.createElement('div');
    el.className = 'conv' + (c.id === curId ? ' active' : '');
    el.innerHTML =
      `<span class="conv-title"></span>` +
      `<span class="conv-date"></span>` +
      `<button class="conv-del" title="حذف گفتگو">✕</button>`;
    el.querySelector('.conv-title').textContent = c.title;
    el.querySelector('.conv-date').textContent = relDate(c.ts);
    el.addEventListener('click', () => selectConv(c.id));
    el.querySelector('.conv-del').addEventListener('click', (ev) => {
      ev.stopPropagation();
      deleteConv(c.id);
    });
    list.appendChild(el);
  }
}

function selectConv(id) {
  if (running) { toast('صبر کنید پاسخ فعلی تمام شود', 'warn'); return; }
  curId = id;
  localStorage.setItem('atria.cur', id);
  renderAll();
}

function deleteConv(id) {
  convs = convs.filter((c) => c.id !== id);
  if (curId === id) {
    curId = convs[0]?.id || null;
    if (curId) localStorage.setItem('atria.cur', curId);
    else localStorage.removeItem('atria.cur');
  }
  persistConvs();
  renderAll();
  toast('گفتگو حذف شد', 'ok');
}

function newConv() {
  if (running) { toast('صبر کنید پاسخ فعلی تمام شود', 'warn'); return; }
  curId = null;
  localStorage.removeItem('atria.cur');
  renderAll();
  $('#input').focus();
}

/* ============================ rendering: messages ============================ */

/** Build one AI turn display (think box + tool cards + text). */
function aiTurnEl() {
  const wrap = document.createElement('div');
  wrap.className = 'msg ai';
  wrap.innerHTML =
    '<div class="avatar atri"><img src="assets/logo.png" alt="آتریا"/></div>' +
    '<div class="stack">' +
    '  <div class="think-box hidden"><button type="button" class="think-head">' +
    '    <span class="think-ic">🧠</span><span class="think-title">در حال تفکر…</span><span class="chev">⌄</span>' +
    '  </button><div class="think-body"></div></div>' +
    '  <div class="tools"></div>' +
    '  <div class="bubble md hidden"><div class="md-text"></div></div>' +
    '  <div class="status-line hidden"><span class="dots"><i></i><i></i><i></i></span><span class="status-text"></span></div>' +
    '  <div class="msg-meta hidden"></div>' +
    '</div>';
  const thinkBox = wrap.querySelector('.think-box');
  thinkBox.querySelector('.think-head').addEventListener('click', () => thinkBox.classList.toggle('open'));
  return wrap;
}

function toolCardEl({ id, name, label, input }) {
  const el = document.createElement('div');
  el.className = 'tool-card';
  el.dataset.id = id;
  const inputStr = typeof input === 'string' ? input : JSON.stringify(input ?? {});
  el.innerHTML =
    '<div class="tool-head">' +
    '  <span class="tool-ic">⚙️</span><span class="tool-label"></span>' +
    '  <span class="tool-chip" dir="ltr"></span><span class="tool-state">در حال اجرا…</span>' +
    '</div>' +
    '<div class="tool-io"><div class="tool-in"></div><div class="tool-out hidden"></div></div>';
  el.querySelector('.tool-label').textContent = label || name || 'ابزار';
  el.querySelector('.tool-chip').textContent = name || '';
  el.querySelector('.tool-in').textContent = inputStr;
  return el;
}

function toolCardDone(card, ok, output) {
  card.classList.add(ok ? 'done' : 'failed');
  card.querySelector('.tool-state').textContent = ok ? '✓ موفق' : '✕ خطا';
  const out = card.querySelector('.tool-out');
  out.textContent = output || '';
  out.classList.remove('hidden');
}

function renderHistory(messages) {
  const box = $('#messages');
  box.innerHTML = '';
  const toolsPending = new Map(); // tool_use_id -> card element

  const flushUserText = (blocks) => {
    const text = blocks.filter((b) => b.type === 'text').map((b) => b.text).join('\n');
    if (!text.trim()) return;
    const wrap = document.createElement('div');
    wrap.className = 'msg user';
    wrap.innerHTML = '<div class="bubble md"><div class="md-text"></div></div>' +
      '<div class="avatar you">شما</div>';
    wrap.querySelector('.md-text').innerHTML = window.md.render(text);
    box.appendChild(wrap);
  };

  const isToolResults = (m) =>
    m.role === 'user' && (m.content || []).length > 0 &&
    m.content.every((b) => b.type === 'tool_result');

  let i = 0;
  while (i < messages.length) {
    const m = messages[i];

    if (m.role === 'user' && isToolResults(m)) {
      for (const b of m.content) {
        const card = toolsPending.get(b.tool_use_id);
        if (card) toolCardDone(card, !b.is_error, b.content || '');
      }
      i++;
      continue;
    }

    if (m.role === 'user') {
      flushUserText(m.content || []);
      i++;
      continue;
    }

    // ---- assistant turn (may span several messages around tool results) ----
    const wrap = aiTurnEl();
    const thinkBox = wrap.querySelector('.think-box');
    const thinkBody = wrap.querySelector('.think-body');
    const toolsBox = wrap.querySelector('.tools');
    const bubble = wrap.querySelector('.bubble');
    const mdText = wrap.querySelector('.md-text');
    const meta = wrap.querySelector('.msg-meta');
    let textParts = [];
    let thinkParts = [];
    let hasTools = false;

    for (;;) {
      for (const b of m.content || []) {
        if (b.type === 'thinking') {
          thinkParts.push(b.thinking);
        } else if (b.type === 'tool_use') {
          const card = toolCardEl({ id: b.id, name: b.name, label: toolLabel(b.name), input: b.input });
          toolsBox.appendChild(card);
          toolsPending.set(b.id, card);
          hasTools = true;
        } else if (b.type === 'text') {
          textParts.push(b.text);
        }
      }
      i++;
      // attach following tool results (and continue with next assistant msg)
      if (i < messages.length && isToolResults(messages[i])) {
        for (const b of messages[i].content) {
          const card = toolsPending.get(b.tool_use_id);
          if (card) toolCardDone(card, !b.is_error, b.content || '');
        }
        i++;
        if (i < messages.length && messages[i].role === 'assistant') {
          m = messages[i];
          continue;
        }
      }
      break;
    }

    const thinking = thinkParts.join('\n\n');
    if (thinking && cfg.show_thinking) {
      thinkBox.classList.remove('hidden');
      thinkBody.textContent = thinking;
      thinkBox.classList.add('has', 'open');
    }
    const text = textParts.join('\n\n').trim();
    if (text) {
      bubble.classList.remove('hidden');
      mdText.innerHTML = window.md.render(text);
      addMetaActions(meta, text, i >= messages.length);
      meta.classList.remove('hidden');
    }
    box.appendChild(wrap);
    void hasTools;
  }
  scrollBottom(true);
}

function addMetaActions(meta, text, withRegen = false) {
  const copy = document.createElement('button');
  copy.className = 'meta-btn';
  copy.textContent = '📋 کپی';
  copy.addEventListener('click', () => copyText(text, copy));
  meta.appendChild(copy);
  if (withRegen) {
    const regen = document.createElement('button');
    regen.className = 'meta-btn';
    regen.textContent = '🔄 بازتولید';
    regen.addEventListener('click', regenLast);
    meta.appendChild(regen);
  }
}

function toolLabel(name) {
  return { calculator: 'ماشین‌حساب', current_time: 'ساعت و تاریخ', remember: 'ذخیره در حافظه', recall: 'جست‌وجوی حافظه' }[name] || 'ابزار';
}

function copyText(text, btn) {
  navigator.clipboard.writeText(text).then(
    () => {
      if (btn) {
        const old = btn.textContent;
        btn.textContent = '✓ کپی شد';
        setTimeout(() => (btn.textContent = old), 1200);
      }
      toast('کپی شد', 'ok');
    },
    () => toast('کپی ممکن نشد', 'err')
  );
}

function renderAll() {
  renderSidebar();
  const conv = currentConv(false);
  const msgs = conv?.messages || [];
  renderHistory(msgs);
  $('#empty').classList.toggle('hidden', msgs.length > 0 || running);
  $('#modelBadge').textContent = cfg.model;
}

/* ============================ scroll helpers ============================ */
function nearBottom() {
  const box = $('#messages');
  return box.scrollHeight - box.scrollTop - box.clientHeight < 220;
}
function scrollBottom(force = false) {
  const box = $('#messages');
  if (force || nearBottom()) box.scrollTop = box.scrollHeight;
}

/* ============================ live generation ============================ */
let running = false;
let live = null;
let queued = false;

function setStatus(text) {
  if (!live) return;
  live.statusText.textContent = text;
  live.status.classList.remove('hidden');
}

function scheduleLiveRender() {
  if (queued) return;
  queued = true;
  requestAnimationFrame(() => {
    queued = false;
    if (!live) return;
    if (live.thinking) {
      if (cfg.show_thinking) {
        live.thinkBox.classList.remove('hidden');
        live.thinkBox.classList.add('has');
        live.thinkBody.textContent = live.thinking;
      }
    }
    if (live.text) {
      live.bubble.classList.remove('hidden');
      live.bubble.classList.add('streaming');
      live.mdText.innerHTML = window.md.render(live.text) + '<span class="cursor"></span>';
    }
    scrollBottom();
  });
}

function startLive() {
  const wrap = aiTurnEl();
  $('#empty').classList.add('hidden');
  $('#messages').appendChild(wrap);
  live = {
    wrap,
    thinkBox: wrap.querySelector('.think-box'),
    thinkBody: wrap.querySelector('.think-body'),
    toolsBox: wrap.querySelector('.tools'),
    bubble: wrap.querySelector('.bubble'),
    mdText: wrap.querySelector('.md-text'),
    status: wrap.querySelector('.status-line'),
    statusText: wrap.querySelector('.status-text'),
    meta: wrap.querySelector('.msg-meta'),
    thinking: '',
    text: '',
  };
  live.thinkTitle = wrap.querySelector('.think-title');
  setStatus('در حال اتصال به آتریا…');
  scrollBottom(true);
}

function finishLive() {
  if (!live) return;
  live.bubble.classList.remove('streaming');
  const cursor = live.mdText.querySelector('.cursor');
  if (cursor) cursor.remove();
  if (live.text) {
    live.mdText.innerHTML = window.md.render(live.text);
    addMetaActions(live.meta, live.text, true);
  }
  live.status.classList.add('hidden');
  live = null;
}

function setRunning(v) {
  running = v;
  $('#sendBtn').classList.toggle('hidden', v);
  $('#stopBtn').classList.toggle('hidden', !v);
  $('#input').disabled = false;
}

/* ============================ sending ============================ */
async function send(text) {
  text = (text || '').trim();
  if (!text || running) return;
  if (!cfg.api_key) {
    toast('کلید API را در تنظیمات وارد کنید', 'warn');
    openModal('settingsModal');
    return;
  }

  const conv = currentConv();
  conv.messages.push({ role: 'user', content: [{ type: 'text', text }] });
  conv.ts = Date.now();
  if (conv.title === 'گفتگوی جدید') conv.title = makeTitle(conv);
  persistConvs();
  renderSidebar();

  // draw the user bubble + AI shell
  $('#empty').classList.add('hidden');
  renderHistory(conv.messages); // includes the just-pushed user msg
  startLive();
  setRunning(true);

  try {
    await T.core.invoke('chat_send', {
      payload: {
        api_key: cfg.api_key,
        base_url: cfg.base_url,
        model: cfg.model,
        max_tokens: cfg.max_tokens,
        temperature: cfg.temperature,
        system: cfg.system,
        tools_enabled: cfg.tools_enabled,
        stream: cfg.stream,
        messages: conv.messages,
      },
    });
  } catch (e) {
    finishLive();
    setRunning(false);
    toast('خطا در شروع گفتگو: ' + e, 'err');
    renderAll();
  }
}

function sendPartial(text) {
  const conv = currentConv(false);
  if (live && live.text.trim()) {
    conv.messages.push({ role: 'assistant', content: [{ type: 'text', text: live.text }] });
    persistConvs();
  }
  finishLive();
  setRunning(false);
  void text;
  renderAll();
}

function regenLast() {
  if (running) return;
  const conv = currentConv(false);
  if (!conv || !conv.messages.length) return;
  // find last user text message and truncate back to it
  let idx = -1;
  for (let i = conv.messages.length - 1; i >= 0; i--) {
    const m = conv.messages[i];
    if (m.role === 'user' && (m.content || []).some((b) => b.type === 'text')) { idx = i; break; }
  }
  if (idx < 0) return;
  const text = conv.messages[idx].content.find((b) => b.type === 'text').text;
  conv.messages = conv.messages.slice(0, idx);
  persistConvs();
  renderAll();
  send(text);
}

/* ============================ Tauri events ============================ */
function listenEvents() {
  const on = (name, fn) => T.event?.listen(name, fn).catch?.(() => {});
  on('atria:thinking', (e) => {
    if (!live) return;
    live.thinking += e.payload.delta;
    live.thinkTitle.textContent = 'در حال تفکر…';
    scheduleLiveRender();
  });
  on('atria:text', (e) => {
    if (!live) return;
    live.text += e.payload.delta;
    setStatus('در حال نوشتن پاسخ…');
    scheduleLiveRender();
  });
  on('atria:round', (e) => {
    if (!live) return;
    if (e.payload.round > 1) setStatus(`مرحله ${e.payload.round} — ادامهٔ تحلیل…`);
  });
  on('atria:tool_start', (e) => {
    if (!live) return;
    const p = e.payload;
    const card = toolCardEl(p);
    live.toolsBox.appendChild(card);
    live.toolsBox.dataset.pending = p.id;
    setStatus(`در حال اجرای ابزار: ${p.label}…`);
    card.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  });
  on('atria:tool_end', (e) => {
    if (!live) return;
    const card = live.toolsBox.querySelector(`[data-id="${e.payload.id}"]`);
    if (card) toolCardDone(card, e.payload.ok, e.payload.output);
    setStatus('در حال ادامهٔ تحلیل…');
  });
  on('atria:done', (e) => {
    const p = e.payload;
    const conv = currentConv();
    for (const m of p.new_messages || []) conv.messages.push(m);
    conv.ts = Date.now();
    persistConvs();
    finishLive();
    setRunning(false);
    renderAll();
    if (p.rounds > 1 || p.output_tokens) {
      toast(`آتریا پاسخ داد ✨ (${p.rounds} مرحله · ${p.output_tokens} توکن)`, 'ok');
    }
  });
  on('atria:stopped', () => {
    toast('تولید متوقف شد', 'warn');
    sendPartial();
  });
  on('atria:error', (e) => {
    const msg = e.payload?.message || 'خطای ناشناخته';
    toast('خطا: ' + msg, 'err');
    sendPartial();
  });
}

/* ============================ settings ============================ */
function openModal(id) {
  $('#' + id).classList.remove('hidden');
}
function closeModal(id) {
  $('#' + id).classList.add('hidden');
}

function fillSettings() {
  $('#setApiKey').value = cfg.api_key;
  $('#setBase').value = cfg.base_url;
  $('#setModel').value = cfg.model;
  $('#setMaxTokens').value = cfg.max_tokens;
  $('#setTemp').value = cfg.temperature;
  $('#maxTokensVal').textContent = cfg.max_tokens;
  $('#tempVal').textContent = Number(cfg.temperature).toFixed(2);
  $('#setTools').checked = !!cfg.tools_enabled;
  $('#setStream').checked = !!cfg.stream;
  $('#setThink').checked = !!cfg.show_thinking;
  $('#setSystem').value = cfg.system;
}

function bindSettings() {
  $('#settingsBtn').addEventListener('click', () => { fillSettings(); openModal('settingsModal'); });
  $('#setMaxTokens').addEventListener('input', (e) => ($('#maxTokensVal').textContent = e.target.value));
  $('#setTemp').addEventListener('input', (e) => ($('#tempVal').textContent = Number(e.target.value).toFixed(2)));
  $('#eyeBtn').addEventListener('click', () => {
    const inp = $('#setApiKey');
    inp.type = inp.type === 'password' ? 'text' : 'password';
  });
  $('#saveCfg').addEventListener('click', () => {
    cfg.api_key = $('#setApiKey').value.trim();
    cfg.base_url = $('#setBase').value.trim() || DEFAULTS.base_url;
    cfg.model = $('#setModel').value.trim() || DEFAULTS.model;
    cfg.max_tokens = Number($('#setMaxTokens').value) || DEFAULTS.max_tokens;
    cfg.temperature = Number($('#setTemp').value);
    cfg.tools_enabled = $('#setTools').checked;
    cfg.stream = $('#setStream').checked;
    cfg.show_thinking = $('#setThink').checked;
    cfg.system = $('#setSystem').value;
    persistCfg();
    closeModal('settingsModal');
    renderAll();
    toast('تنظیمات ذخیره شد ✓', 'ok');
  });
  $('#resetCfg').addEventListener('click', () => {
    cfg = { ...DEFAULTS };
    persistCfg();
    fillSettings();
    toast('به پیش‌فرض‌ها بازنشانی شد', 'ok');
  });
  document.querySelectorAll('[data-close]').forEach((b) =>
    b.addEventListener('click', () => closeModal(b.dataset.close))
  );
  document.querySelectorAll('.modal-backdrop').forEach((b) =>
    b.addEventListener('click', (e) => { if (e.target === b) b.classList.add('hidden'); })
  );
}

/* ============================ memory ============================ */
async function openMemory() {
  openModal('memoryModal');
  const list = $('#memList');
  list.innerHTML = '<div class="mem-empty">در حال بارگذاری…</div>';
  try {
    const notes = (await T.core.invoke('memory_list')) || [];
    list.innerHTML = '';
    if (!notes.length) {
      list.innerHTML = '<div class="mem-empty">هنوز چیزی ذخیره نشده — به آتریا بگویید «یادت باشد…»</div>';
      return;
    }
    for (const n of notes) {
      const el = document.createElement('div');
      el.className = 'mem-note';
      el.innerHTML = '<div class="mem-title"></div><div class="mem-content"></div><div class="mem-ts"></div>';
      el.querySelector('.mem-title').textContent = n.title;
      el.querySelector('.mem-content').textContent = n.content;
      el.querySelector('.mem-ts').textContent = n.ts;
      list.appendChild(el);
    }
  } catch (e) {
    list.innerHTML = '<div class="mem-empty">خطا در بارگذاری حافظه</div>';
    void e;
  }
}

/* ============================ init ============================ */
function bindMain() {
  $('#newChatBtn').addEventListener('click', newConv);
  $('#sideToggle').addEventListener('click', () => $('#messages').closest('.frame').classList.toggle('side-hidden'));
  $('#memoryBtn').addEventListener('click', openMemory);
  $('#memClear').addEventListener('click', async () => {
    await T.core.invoke('memory_clear');
    openMemory();
    toast('حافظه پاک شد', 'ok');
  });

  const input = $('#input');
  const composer = $('#composer');
  composer.addEventListener('submit', (e) => {
    e.preventDefault();
    const text = input.value;
    input.value = '';
    autoGrow();
    send(text);
  });
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      composer.requestSubmit();
    }
  });
  function autoGrow() {
    input.style.height = 'auto';
    input.style.height = Math.min(input.scrollHeight, 180) + 'px';
  }
  input.addEventListener('input', autoGrow);

  $('#stopBtn').addEventListener('click', async () => {
    try { await T.core.invoke('chat_stop'); } catch { /* ignore */ }
  });

  $('#scrollBtn').addEventListener('click', () => {
    $('#messages').scrollTop = $('#messages').scrollHeight;
  });
  $('#messages').addEventListener('scroll', () => {
    $('#scrollBtn').classList.toggle('hidden', nearBottom());
  });

  $('#suggestions').addEventListener('click', (e) => {
    const chip = e.target.closest('.chip');
    if (chip) send(chip.textContent.trim());
  });

  document.addEventListener('click', (e) => {
    const copy = e.target.closest('.code-copy');
    if (copy) {
      copyText(copy.dataset.code || '', copy);
      return;
    }
    const lnk = e.target.closest('.lnk');
    if (lnk && lnk.dataset.href) {
      e.preventDefault();
      copyText(lnk.dataset.href);
    }
  });

  document.addEventListener('keydown', (e) => {
    if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === 'o') { e.preventDefault(); newConv(); }
    if (e.key === 'Escape') {
      document.querySelectorAll('.modal-backdrop').forEach((m) => m.classList.add('hidden'));
    }
  });
}

/* ============================ demo mode ============================ */
function maybeSeedDemo() {
  if (!/[?&]demo/.test(location.search)) return;
  if (!convs.some((c) => c.id === 'demo')) {
    convs.unshift({
      id: 'demo',
      title: 'گفتگوی نمونه',
      ts: Date.now(),
      messages: [
        { role: 'user', content: [{ type: 'text', text: '(17*24+3) رو حساب کن و یادت باشه نتیجه ۴۱۱ است.' }] },
        { role: 'assistant', content: [
          { type: 'thinking', thinking: 'The user wants me to calculate 17*24+3 and remember it.\nI should call the calculator tool first, then save it with remember.', signature: 'demo-sig-1' },
          { type: 'tool_use', id: 'demo-t1', name: 'calculator', input: { expression: '17*24+3' } },
        ]},
        { role: 'user', content: [
          { type: 'tool_result', tool_use_id: 'demo-t1', content: '17*24+3 = 411', is_error: false },
        ]},
        { role: 'assistant', content: [
          { type: 'thinking', thinking: 'Now I save the result to memory.', signature: 'demo-sig-2' },
          { type: 'tool_use', id: 'demo-t2', name: 'remember', input: { title: 'محاسبه', content: '17*24+3 = 411' } },
        ]},
        { role: 'user', content: [
          { type: 'tool_result', tool_use_id: 'demo-t2', content: 'Saved note "محاسبه" to memory.', is_error: false },
        ]},
        { role: 'assistant', content: [
          { type: 'thinking', thinking: 'Compose the final Persian answer with Markdown.', signature: 'demo-sig-3' },
          { type: 'text', text: '**۴۱۱** ✨\n\nنحوهٔ محاسبه:\n\n- `17 × 24 = 408`\n- `408 + 3 = 411`\n\nنتیجه در **حافظهٔ آتریا** هم ذخیره شد.\n\n```python\nresult = 17 * 24 + 3  # 411\n```' },
        ]},
      ],
    });
    persistConvs();
  }
  curId = 'demo';
  localStorage.setItem('atria.cur', 'demo');
}

async function init() {
  initStars();
  initWindow();
  bindMain();
  bindSettings();
  listenEvents();
  maybeSeedDemo();
  renderAll();
  try {
    const meta = await T.core.invoke('app_meta');
    if (meta?.version) $('.engine-note').innerHTML = `<span class="dot-live"></span> موتور Rust · v${meta.version}`;
  } catch { /* ignore */ }
  $('#input').focus();
}

init();
