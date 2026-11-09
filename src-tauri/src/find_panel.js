/* Unprivileged page-local find dialog. Fixed positioning and DOM ranges leave page layout unchanged. */
function showFindPanel(visible, locale) {
  // The article owns keyboard focus after a search. Keep its shortcut listener
  // for the document lifetime, even when the floating dialog has been removed.
  const shortcutKey = '__topicDeskFindShortcut';
  let shortcut = globalThis[shortcutKey];
  if (!shortcut) {
    shortcut = { locale, open: showFindPanel };
    globalThis[shortcutKey] = shortcut;
    document.addEventListener('keydown', event => {
      if (event.isComposing || event.altKey || !(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== 'f') return;
      event.preventDefault();
      event.stopPropagation();
      shortcut.open(true, shortcut.locale);
    }, true);
  }
  shortcut.locale = locale;
  shortcut.open = showFindPanel;
  const key = '__topicDeskFindPanel';
  const previous = document.getElementById(key);
  if (!visible) { previous?.remove(); return true; }
  if (previous) { previous.focus(); return true; }
  const host = document.createElement('div');
  host.id = key;
  host.style.cssText = 'all:initial!important;position:fixed!important;top:0!important;right:16px!important;z-index:2147483647!important;width:min(420px,calc(100vw - 32px))!important;';
  const root = host.attachShadow({ mode: 'closed' });
  const zh = locale === 'zh';
  root.innerHTML = `<style>
    :host{color-scheme:light}*{box-sizing:border-box}form{display:flex;align-items:center;gap:4px;margin:0;padding:10px 12px 10px 16px;min-height:52px;background:#fdfbff;color:#34343b;border:1px solid #e7e5ed;border-top:0;border-radius:0 0 12px 12px;box-shadow:0 4px 12px #0002;font:14px system-ui}
    input{flex:1;min-width:0;width:150px;border:0;outline:0;padding:5px 0;background:transparent;color:inherit;font:inherit}input::placeholder{color:#92909c}button{flex:none;display:grid;place-items:center;width:30px;height:30px;border:0;border-radius:50%;background:transparent;color:#5f6368;cursor:pointer}button:hover:not(:disabled){background:#eae7f0}button:disabled{color:#c4c3ca;cursor:default}button:focus-visible{outline:2px solid #6058ff}output{flex:none;min-width:42px;text-align:right;font-size:13px;color:#5f6368;white-space:nowrap;margin-right:12px}i{flex:none;width:1px;height:28px;background:#dadce8;margin-right:7px}svg{width:18px;height:18px;fill:none;stroke:currentColor;stroke-width:2;stroke-linecap:round;stroke-linejoin:round}
  </style><form role="dialog"><input maxlength="1000"/><output aria-live="polite">0/0</output><i aria-hidden="true"></i><button type="button" data-prev><svg viewBox="0 0 24 24"><path d="m6 15 6-6 6 6"/></svg></button><button data-next><svg viewBox="0 0 24 24"><path d="m6 9 6 6 6-6"/></svg></button><button type="button" data-close><svg viewBox="0 0 24 24"><path d="m6 6 12 12M6 18 18 6"/></svg></button></form>`;
  const input = root.querySelector('input');
  const form = root.querySelector('form');
  const output = root.querySelector('output');
  const prev = root.querySelector('[data-prev]');
  const next = root.querySelector('[data-next]');
  form.setAttribute('aria-label', zh ? '在页面中查找' : 'Find in page');
  input.placeholder = zh ? '在页面中查找' : 'Find in page';
  input.setAttribute('aria-label', input.placeholder);
  for (const [selector, label] of [['[data-prev]', zh ? '上一项' : 'Previous match'], ['[data-next]', zh ? '下一项' : 'Next match'], ['[data-close]', zh ? '关闭查找' : 'Close find']]) {
    const button = root.querySelector(selector); button.title = label; button.setAttribute('aria-label', label);
  }
  let matches = [];
  let current = -1;
  let composing = false;
  let dirty = true;
  const update = () => {
    output.textContent = `${current + 1}/${matches.length}`;
    prev.disabled = next.disabled = matches.length === 0;
  };
  const select = index => {
    current = index;
    const range = matches[current];
    if (range) {
      const selection = window.getSelection();
      selection.removeAllRanges(); selection.addRange(range);
      range.startContainer.parentElement.scrollIntoView({ block: 'center', inline: 'nearest', behavior: 'instant' });
    }
    update();
    if (root.activeElement !== input) input.focus();
  };
  // Visible text only. Preserve inline spans but separate block boundaries so matches cannot bridge paragraphs.
  const rebuild = () => {
    matches = []; current = -1;
    window.getSelection()?.removeAllRanges();
    if (!input.value || !document.body) { update(); return; }
    const nodes = [];
    let text = '', block = null;
    const walker = document.createTreeWalker(document.body, 4);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const parent = node.parentElement;
      if (!parent || host.contains(parent) || parent.closest('script,style,noscript,textarea,select,[hidden],[aria-hidden="true"]') || !parent.getClientRects().length) continue;
      const group = parent.closest('p,div,li,section,article,h1,h2,h3,h4,td,pre,blockquote');
      if (group !== block && text) text += '\n';
      block = group;
      nodes.push({ node, start: text.length, end: text.length + node.textContent.length });
      text += node.textContent;
    }
    const escaped = input.value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    let firstIndex = 0, lastIndex = 0;
    for (const match of text.matchAll(new RegExp(escaped, 'gi'))) {
      const start = match.index, end = start + match[0].length;
      // Match offsets increase monotonically; scan nodes once even on long articles.
      while (firstIndex < nodes.length && nodes[firstIndex].end <= start) firstIndex++;
      lastIndex = Math.max(lastIndex, firstIndex);
      while (lastIndex < nodes.length && nodes[lastIndex].end < end) lastIndex++;
      const first = nodes[firstIndex], last = nodes[lastIndex];
      if (!first || !last) continue;
      const range = document.createRange();
      range.setStart(first.node, start - first.start); range.setEnd(last.node, end - last.start);
      matches.push(range);
    }
    select(matches.length ? 0 : -1);
  };
  const find = backwards => {
    if (composing) return;
    if (dirty) { dirty = false; rebuild(); return; }
    if (matches.length) select((current + (backwards ? -1 : 1) + matches.length) % matches.length);
  };
  form.onsubmit = event => { event.preventDefault(); if (!composing) find(false); };
  prev.onclick = () => find(true);
  const close = () => { window.getSelection()?.removeAllRanges(); host.remove(); };
  root.querySelector('[data-close]').onclick = close;
  // Editing the page selection or moving focus during marked-text composition
  // invalidates WebKit's IME caret anchor. Editing only invalidates the displayed
  // count; document traversal, selection and scrolling wait for explicit Enter.
  const invalidate = () => {
    dirty = true; matches = []; current = -1; update();
  };
  input.oncompositionstart = () => { composing = true; invalidate(); };
  input.oncompositionend = () => { composing = false; invalidate(); };
  input.oninput = invalidate;
  host.onkeydown = event => {
    if (composing || event.isComposing || event.keyCode === 229) return;
    if (event.key === 'Escape') { event.preventDefault(); close(); }
    // Handle Enter directly: the disabled next-match submit button must not
    // prevent starting a new search when no results have been computed yet.
    else if (event.key === 'Enter') { event.preventDefault(); find(event.shiftKey === true); }
  };
  host.focus = () => { input.focus(); input.select(); };
  document.documentElement.append(host);
  update(); input.focus();
  return true;
}
