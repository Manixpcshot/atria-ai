/* ==========================================================================
   Atria Dawn — tiny, safe Markdown renderer (no innerHTML from user/model)
   ========================================================================== */
window.md = (() => {
  'use strict';

  function esc(s) {
    return String(s).replace(/[&<>"']/g, (c) => (
      { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]
    ));
  }

  function inline(s) {
    let h = esc(s);
    h = h.replace(/`([^`]+)`/g, '<code class="ic">$1</code>');
    h = h.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
    h = h.replace(/(^|[^*\w])\*([^*\n]+)\*/g, '$1<em>$2</em>');
    h = h.replace(/~~([^~]+)~~/g, '<del>$1</del>');
    h = h.replace(
      /\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g,
      '<a href="$2" class="lnk" data-href="$2">$1</a>'
    );
    return h;
  }

  function renderBlocks(t) {
    const lines = String(t).split('\n');
    let html = '';
    let list = null;
    let para = [];

    const flushPara = () => {
      if (para.length) {
        html += '<p>' + inline(para.join('\n')).replace(/\n/g, '<br/>') + '</p>';
        para = [];
      }
    };
    const flushList = () => {
      if (list) { html += '</' + list + '>'; list = null; }
    };

    for (const raw of lines) {
      const line = raw.trimEnd();
      if (!line.trim()) { flushPara(); flushList(); continue; }

      let m;
      if ((m = line.match(/^(#{1,4})\s+(.*)$/))) {
        flushPara(); flushList();
        const lv = m[1].length;
        html += `<h${lv}>${inline(m[2])}</h${lv}>`;
        continue;
      }
      if (/^\s*(---|___|\*\*\*)\s*$/.test(line)) {
        flushPara(); flushList(); html += '<hr/>'; continue;
      }
      if ((m = line.match(/^\s*[-*•]\s+(.*)$/))) {
        flushPara();
        if (list !== 'ul') { flushList(); html += '<ul>'; list = 'ul'; }
        html += `<li>${inline(m[1])}</li>`;
        continue;
      }
      if ((m = line.match(/^\s*\d+[.)]\s+(.*)$/))) {
        flushPara();
        if (list !== 'ol') { flushList(); html += '<ol>'; list = 'ol'; }
        html += `<li>${inline(m[1])}</li>`;
        continue;
      }
      if ((m = line.match(/^\s*>\s?(.*)$/))) {
        flushPara(); flushList();
        html += `<blockquote>${inline(m[1])}</blockquote>`;
        continue;
      }
      flushList();
      para.push(line);
    }
    flushPara();
    flushList();
    return html;
  }

  function render(src) {
    src = String(src || '');
    const re = /```([\w+#.-]*)[ \t]*\n?([\s\S]*?)(?:\n?```|$)/g;
    let out = '';
    let last = 0;
    let m;
    while ((m = re.exec(src))) {
      if (m.index > last) out += renderBlocks(src.slice(last, m.index));
      const lang = m[1] || 'code';
      const code = m[2].replace(/\n$/, '');
      out +=
        '<div class="code-block">' +
        '<div class="code-head">' +
        `<span class="code-lang" dir="ltr">${esc(lang)}</span>` +
        `<button type="button" class="code-copy" data-code="${esc(code)}">کپی</button>` +
        '</div>' +
        `<pre dir="ltr"><code>${esc(code)}</code></pre>` +
        '</div>';
      last = m.index + m[0].length;
    }
    if (last < src.length) out += renderBlocks(src.slice(last));
    return out;
  }

  return { render };
})();
