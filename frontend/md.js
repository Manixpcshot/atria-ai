/* Atria — safe markdown streaming renderer (v0.2: incremental).
 *
 * - No HTML from the model is ever injected (escapes first).
 * - Supports: headings, bold/italic/strike, inline + fenced code, links,
 *   bullet/numbered lists, blockquotes, hr, and bare URLs.
 * - Streaming: `createLiveStream(el)` grows a message **incrementally** —
 *   finished blocks are frozen, code lines are appended one-by-one with a
 *   line-in animation and never re-rendered (no full-block refresh).
 */
(function (global) {
  'use strict';

  function esc(s) {
    return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;')
      .replace(/>/g, '&gt;').replace(/"/g, '&quot;');
  }

  /* ---------------- inline ---------------- */

  function renderInline(text) {
    let s = esc(text);

    // inline code — protect contents from further transforms
    const codes = [];
    s = s.replace(/`([^`\n]+)`/g, function (_, c) {
      codes.push(c);
      return '\u0000CODE' + (codes.length - 1) + '\u0000';
    });

    // bold / italic / strike
    s = s.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
    s = s.replace(/(^|[\s(])\*([^*\n]+)\*/g, '$1<em>$2</em>');
    s = s.replace(/(^|[\s(])_([^_\n]+)_/g, '$1<em>$2</em>');
    s = s.replace(/~~([^~]+)~~/g, '<del>$1</del>');

    // links [text](url) — only http(s)
    s = s.replace(/\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g,
      '<a href="$2" target="_blank" rel="noreferrer">$1</a>');

    // bare URLs
    s = s.replace(/(^|[\s(])((?:https?:\/\/)[^\s<)]+)/g,
      '$1<a href="$2" target="_blank" rel="noreferrer">$2</a>');

    // restore inline code
    s = s.replace(/\u0000CODE(\d+)\u0000/g, function (_, i) {
      return '<code>' + codes[+i] + '</code>';
    });
    return s;
  }

  /* ---------------- syntax highlighting (per line) ---------------- */

  const KW = [
    'fn','let','mut','const','struct','enum','impl','trait','pub','use','mod','match',
    'if','else','while','for','loop','return','break','continue','in','as','where',
    'async','await','move|','dyn','ref','type','static','crate','self','super','true','false',
    'def','class','import','from','export','function','var','new','this','try','catch',
    'finally','throw','switch','case','default','do','elif','except','lambda','yield',
    'None','True','False','null','undefined','void','int','float','str','bool','string',
    'long','double','char','unsigned','signed','struct','public','private','protected',
  ];
  const KW_RE = new RegExp('\\b(' + KW.join('|').replace(/\|\\|/g, '|') + ')\\b', 'g');

  function highlightLine(src) {
    // tokenize: comments | strings | numbers | keywords | idents
    let out = '';
    const re = /(\/\/.*$|#.*$)|("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')|(\b\d+(?:\.\d+)?\b)|([A-Za-z_][A-Za-z0-9_]*)/gm;
    let last = 0, m;
    while ((m = re.exec(src))) {
      out += esc(src.slice(last, m.index));
      if (m[1]) out += '<span class="c-com">' + esc(m[1]) + '</span>';
      else if (m[2]) out += '<span class="c-str">' + esc(m[2]) + '</span>';
      else if (m[3]) out += '<span class="c-num">' + esc(m[3]) + '</span>';
      else if (m[4]) {
        KW_RE.lastIndex = 0;
        out += KW_RE.test(m[4])
          ? '<span class="c-kw">' + esc(m[4]) + '</span>'
          : /^[A-Z]/.test(m[4])
            ? '<span class="c-typ">' + esc(m[4]) + '</span>'
            : esc(m[4]);
      }
      last = re.lastIndex;
    }
    out += esc(src.slice(last));
    return out;
  }

  /* ---------------- block parsing ---------------- */

  // Split markdown into [{t:'prose'|'code', text, lang?, closed?}].
  // Handles unclosed fences mid-stream.
  function parseBlocks(text) {
    const out = [];
    let pos = 0;
    const re = /(^|\n)[ \t]*(`{3,}|~{3,})[ \t]*([^\n`]*)\n?/g;
    let m;
    while ((m = re.exec(text))) {
      const fence = m[2];
      const lang = (m[3] || '').trim().toLowerCase();
      const contentStart = re.lastIndex;
      const close = new RegExp('\\n[ \\t]*' + fence[0] + '{' + fence.length + ',}[ \\t]*(\\n|$)');
      const rest = text.slice(contentStart);
      const cm = rest.match(close);
      const code = cm ? rest.slice(0, cm.index) : rest;
      const end = cm ? contentStart + cm.index + cm[0].length : text.length;
      const prose = text.slice(pos, m.index + (m[1] ? 1 : 0));
      if (prose.trim()) out.push({ t: 'prose', text: prose });
      out.push({ t: 'code', text: code, lang, closed: !!cm });
      pos = end;
      re.lastIndex = end;
    }
    const tail = text.slice(pos);
    if (tail.trim()) out.push({ t: 'prose', text: tail });
    return out;
  }

  function extractCodeBlocks(text) {
    return parseBlocks(text).filter(function (b) { return b.t === 'code'; });
  }

  /* ---------------- block rendering (one-shot) ---------------- */

  function codeShell(lang) {
    const el = document.createElement('div');
    el.className = 'code';
    const head = document.createElement('div');
    head.className = 'code-head';
    head.innerHTML = '<span class="code-dots"><i></i><i></i><i></i></span>' +
      '<span class="code-lang">' + esc(lang || 'code') + '</span>';
    const copy = document.createElement('button');
    copy.className = 'copy-btn';
    copy.textContent = 'کپی';
    copy.addEventListener('click', function () {
      const code = Array.prototype.map.call(
        el.querySelectorAll('.code-line .lc'),
        function (n) { return n.textContent; }
      ).join('\n');
      navigator.clipboard.writeText(code).then(function () {
        copy.textContent = 'کپی شد ✓';
        setTimeout(function () { copy.textContent = 'کپی'; }, 1200);
      });
    });
    head.appendChild(copy);
    el.appendChild(head);
    const lines = document.createElement('div');
    lines.className = 'code-lines';
    el.appendChild(lines);
    return el;
  }

  function lineEl(src, num, animate) {
    const line = document.createElement('div');
    line.className = 'code-line' + (animate ? ' anim' : '');
    const ln = document.createElement('span');
    ln.className = 'ln';
    ln.textContent = String(num);
    const lc = document.createElement('span');
    lc.className = 'lc';
    lc.innerHTML = highlightLine(src);
    line.appendChild(ln);
    line.appendChild(lc);
    return line;
  }

  function buildCode(block, animate) {
    const el = codeShell(block.lang);
    const lines = el.querySelector('.code-lines');
    let list = block.text.split('\n');
    if (block.closed && list[list.length - 1] === '') list = list.slice(0, -1);
    if (!list.length) list = [''];
    list.forEach(function (src, i) {
      lines.appendChild(lineEl(src, i + 1, animate));
    });
    return el;
  }

  function renderPara(text, animate) {
    const el = document.createElement('div');
    el.className = 'para';
    const chunks = text.split(/\n{2,}/);
    for (const chunk of chunks) {
      const t = chunk.trim();
      if (!t) continue;
      if (/^---+$/.test(t)) { el.appendChild(Object.assign(document.createElement('hr'), { className: 'md-hr' })); continue; }
      if (/^#{1,6}\s/.test(t)) {
        const level = Math.min(6, (t.match(/^#+/) || ['#'])[0].length);
        const h = document.createElement('h' + level);
        h.className = 'md-h' + level + (animate ? ' thinking-anim' : '');
        h.innerHTML = renderInline(t.replace(/^#+\s*/, ''));
        el.appendChild(h);
        continue;
      }
      if (/^>\s?/.test(t)) {
        const q = document.createElement('blockquote');
        q.className = 'md-quote' + (animate ? ' thinking-anim' : '');
        q.innerHTML = renderInline(t.replace(/^>\s?/gm, ''));
        el.appendChild(q);
        continue;
      }
      const lines = t.split('\n');
      const isList = lines.every(function (l) { return !l.trim() || /^\s*([-*+]|\d+[.)])\s+/.test(l); });
      if (isList && lines.some(function (l) { return /^\s*([-*+]|\d+[.)])\s+/.test(l); })) {
        const ordered = /^\s*\d+[.)]\s+/.test(lines.find(function (l) { return l.trim(); }) || '');
        const ul = document.createElement(ordered ? 'ol' : 'ul');
        ul.className = 'md-list' + (animate ? ' thinking-anim' : '');
        lines.forEach(function (l) {
          const m = l.match(/^\s*(?:[-*+]|\d+[.)])\s+(.*)$/);
          if (m) {
            const li = document.createElement('li');
            li.innerHTML = renderInline(m[1]);
            ul.appendChild(li);
          }
        });
        el.appendChild(ul);
        continue;
      }
      const p = document.createElement('p');
      if (animate) p.className = 'thinking-anim';
      p.innerHTML = renderInline(t).replace(/\n/g, '<br>');
      el.appendChild(p);
    }
    return el;
  }

  function renderBlock(block, animate) {
    return block.t === 'code' ? buildCode(block, animate) : renderPara(block.text, animate);
  }

  /* ---------------- public API ---------------- */

  // Full render (history / final).
  function renderAll(el, text) {
    el.innerHTML = '';
    el.classList.remove('streaming');
    const blocks = parseBlocks(text);
    if (!blocks.length) return;
    blocks.forEach(function (b) { el.appendChild(renderBlock(b, true)); });
  }

  // Incremental live renderer: push deltas as they arrive.
  // Completed blocks are frozen; code lines are appended one-by-one —
  // existing lines are never touched (no per-line refresh).
  function createLiveStream(el) {
    let text = '';
    const parts = []; // [{block, el, nLines, dirty}]

    function ensure(block, i) {
      if (!parts[i]) {
        const bel = renderBlock(block, true);
        el.appendChild(bel);
        parts[i] = {
          el: bel, t: block.t,
          nLines: block.t === 'code' ? block.el === undefined ? countLines(block) : 0 : 0,
          raw: block.text,
        };
        if (block.t === 'code') {
          // lines already rendered by buildCode; remember how many
          parts[i].nLines = parts[i].el.querySelectorAll('.code-line').length;
        }
        return parts[i];
      }
      return parts[i];
    }

    function countLines(block) {
      let list = block.text.split('\n');
      if (block.closed && list[list.length - 1] === '') list = list.slice(0, -1);
      return Math.max(1, list.length);
    }

    function push(delta) {
      text += delta;
      const blocks = parseBlocks(text);
      while (parts.length > blocks.length) {
        const p = parts.pop();
        if (p && p.el.parentNode) p.el.parentNode.removeChild(p.el);
      }
      blocks.forEach(function (block, i) {
        const part = ensure(block, i);
        if (part.raw === block.text) return;
        part.raw = block.text;
        if (block.t === 'prose') {
          // paragraphs: cheap re-render inside this block only
          const fresh = renderPara(block.text, false);
          fresh.className = part.el.className;
          part.el.innerHTML = fresh.innerHTML;
        } else {
          // code: append ONLY the new lines; update the trailing partial line
          const linesBox = part.el.querySelector('.code-lines');
          let list = block.text.split('\n');
          const hasPartial = !block.closed;
          if (!hasPartial && list[list.length - 1] === '') list = list.slice(0, -1);
          if (!list.length) list = [''];
          // update existing last line if it is the partial one
          const existing = linesBox.querySelectorAll('.code-line');
          if (hasPartial && existing.length && existing.length === list.length) {
            const lastEl = existing[existing.length - 1];
            lastEl.querySelector('.lc').innerHTML = highlightLine(list[list.length - 1]);
          }
          for (let k = part.nLines; k < list.length; k++) {
            const isLast = hasPartial && k === list.length - 1;
            const lel = lineEl(list[k], k + 1, true);
            if (isLast) lel.classList.add('partial');
            linesBox.appendChild(lel);
          }
          // keep partial marker on the final line while streaming
          const all = linesBox.querySelectorAll('.code-line');
          for (let k = 0; k < all.length; k++) all[k].classList.toggle('partial', hasPartial && k === all.length - 1);
          part.nLines = list.length;
        }
      });
    }

    function done() {
      // settle any trailing partial line
      const pendings = el.querySelectorAll('.code-line.partial');
      for (const p of pendings) p.classList.remove('partial');
    }

    return { push: push, done: done, isEmpty: function () { return !text.trim(); } };
  }

  function render(el, text) { el.innerHTML = renderInline(text); }

  global.md = {
    render: render,
    renderAll: renderAll,
    renderInline: renderInline,
    createLiveStream: createLiveStream,
    parseBlocks: parseBlocks,
    extractCodeBlocks: extractCodeBlocks,
  };
})(window);
