// Isolated reader runtime: only visible text is queued; no native privileges or credentials.
(token => {
  globalThis.__topicDeskPageTranslation?.stop?.();
  const blocked = 'script,style,noscript,textarea,input,select,option,code,pre,svg,canvas,[aria-hidden="true"],[translate="no"],[contenteditable="true"],.topic-desk-translation';
  const split = text => {
    if (text.length <= 900) return text ? [text] : [];
    const chars = Array.from(text), parts = [];
    for (let i = 0; i < chars.length;) {
      let end = Math.min(i + 900, chars.length);
      if (end < chars.length) {
        let boundary = -1;
        for (let j = end - 1; j >= i + 450; j--) {
          if (/[.!?。！？\n]/u.test(chars[j])) { boundary = j + 1; break; }
        }
        if (boundary < 0) for (let j = end - 1; j >= i + 450; j--) {
          if (/\s/u.test(chars[j])) { boundary = j + 1; break; }
        }
        if (boundary > i) end = boundary;
      }
      parts.push(chars.slice(i, end).join('')); i = end;
    }
    return parts;
  };
  const visible = element => {
    if (!element.isConnected) return false;
    const r = element.getBoundingClientRect();
    if (!(r.width > 0 && r.height > 0 && r.bottom > 0 && r.top < innerHeight && r.right > 0 && r.left < innerWidth)) return false;
    const style = getComputedStyle(element);
    return style.visibility !== 'hidden' && style.display !== 'none' && style.opacity !== '0';
  };
  const seen = new WeakMap(), ownedSlots = new WeakMap(), slots = [], retries = [];
  const rendered = new Set(), slotIds = new WeakMap(), errors = new Set();
  // Keep completed translations for this page session, including content that
  // a virtualized site removes and recreates while the reader scrolls.
  const completed = new Map();
  const release = id => {
    const slot = slots[id];
    if (!slot) return;
    const ids = slotIds.get(slot);
    ids?.delete(id);
    if (!ids?.size && !slot.isConnected) {
      slotIds.delete(slot);
      rendered.delete(slot);
    }
    delete slots[id]; delete state.elements[id]; delete state.texts[id];
  };
  const removeOutput = element => {
    for (const slot of ownedSlots.get(element) || []) {
      for (const id of slotIds.get(slot) || []) { errors.delete(id); release(id); }
      rendered.delete(slot); slot.remove();
    }
    ownedSlots.delete(element);
    seen.delete(element);
  };
  // Index text ownership once. Idle drains do no tree walking, style resolution,
  // layout reads or string normalization. Only changed subtrees are re-indexed.
  const groups = new Map(), nodeOwners = new WeakMap(), pending = new Set();
  const dirtyRoots = new Set([document.body]), removedRoots = new Set();
  let viewportDirty = true;
  const translationNode = node => (node?.nodeType === 1 ? node : node?.parentElement)?.closest('.topic-desk-translation');
  const markViewport = () => { viewportDirty = true; };
  const intersections = new IntersectionObserver(entries => {
    for (const entry of entries) {
      if (entry.isIntersecting) pending.add(entry.target);
      else pending.delete(entry.target);
    }
  });
  const mutations = new MutationObserver(records => invalidate(records));
  function invalidate(records) {
    for (const record of records) {
      if (translationNode(record.target)) continue;
      if (record.type === 'childList') {
        for (const node of record.removedNodes) if (!translationNode(node)) removedRoots.add(node);
        for (const node of record.addedNodes) if (!translationNode(node)) dirtyRoots.add(node);
      } else dirtyRoots.add(record.target);
    }
  }
  function walk(root, visit) {
    if (root.nodeType === 3) { visit(root); return; }
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) visit(node);
  }
  function forget(node) {
    const owner = nodeOwners.get(node);
    if (!owner) return;
    const group = groups.get(owner);
    group?.delete(node);
    nodeOwners.delete(node);
    pending.add(owner);
    if (!group?.size) { groups.delete(owner); intersections.unobserve(owner); removeOutput(owner); }
  }
  function indexChanges() {
    const owners = new WeakMap();
    function ownerOf(element) {
      if (!element) return null;
      if (owners.has(element)) return owners.get(element);
      const owner = element.closest(blocked) ? null
        : element.parentElement && getComputedStyle(element).display === 'inline'
          ? ownerOf(element.parentElement) : element;
      owners.set(element, owner);
      return owner;
    }
    for (const root of removedRoots) walk(root, forget);
    removedRoots.clear();
    // Coalesce nested mutation roots so framework updates cannot scan a subtree
    // multiple times in the same dispatch.
    for (const root of dirtyRoots) {
      let ancestor = root.parentNode, covered = false;
      while (ancestor) { if (dirtyRoots.has(ancestor)) { covered = true; break; } ancestor = ancestor.parentNode; }
      if (covered) continue;
      walk(root, node => {
        forget(node);
        if (!node.isConnected) return;
        const owner = ownerOf(node.parentElement);
        if (!owner) return;
        if (!groups.has(owner)) { groups.set(owner, new Set()); intersections.observe(owner); }
        groups.get(owner).add(node);
        nodeOwners.set(node, owner);
        pending.add(owner);
      });
    }
    dirtyRoots.clear();
  }
  const style = document.createElement('style');
  style.textContent = '.topic-desk-translation{display:block!important;color:#6570d9!important;font:inherit!important;white-space:pre-wrap!important;margin:.35em 0!important}.topic-desk-translation[data-loading]::after{content:"";display:inline-block;width:.7em;height:.7em;border:2px solid #dfe9ed;border-top-color:#78868d;border-radius:50%;animation:td-translate-spin .7s linear infinite}@keyframes td-translate-spin{to{transform:rotate(360deg)}}';
  document.head?.appendChild(style);
  // Errors must not inherit a heading's size or expose long provider messages
  // as article content. Full diagnostics remain available on hover.
  style.textContent += '.topic-desk-translation[data-error]{display:inline-block!important;font:12px/1.5 sans-serif!important;font-weight:400!important;color:#a65c43!important;margin:4px 0!important;letter-spacing:normal!important}';
  const state = {
    token, split, visible, texts: [], elements: [], slots,
    // Pending requests are ranked against the current viewport, not the viewport
    // at collection time. Disconnected and offscreen segments are not dispatched.
    prioritize(ids) {
      const valid = ids.filter(id => slots[id]?.isConnected && state.elements[id]?.isConnected);
      const ready = valid.filter(id => visible(state.elements[id]))
        .sort((a, b) => state.elements[a].getBoundingClientRect().top - state.elements[b].getBoundingClientRect().top);
      const validIds = new Set(valid);
      for (const id of ids) if (!validIds.has(id)) release(id);
      return { valid, ready };
    },
    retry(index) {
      const slot = slots[index];
      if (!state.token || !slot?.isConnected || !slot.hasAttribute('data-error')) return;
      slot.removeAttribute('data-error'); slot.removeAttribute('title'); slot.removeAttribute('aria-label');
      slot.removeAttribute('role'); slot.removeAttribute('tabindex');
      slot.textContent = ''; slot.setAttribute('data-loading', '');
      retries.push({ slot, element: state.elements[index], text: state.texts[index] });
      errors.delete(index);
      release(index);
    },
    stop() {
      intersections.disconnect(); mutations.disconnect();
      document.removeEventListener('scroll', markViewport, true);
      globalThis.removeEventListener('resize', markViewport);
      document.removeEventListener('visibilitychange', markViewport);
      rendered.forEach(n => n.remove()); style.remove(); state.token = null;
      groups.clear(); pending.clear(); dirtyRoots.clear(); removedRoots.clear();
      rendered.clear(); errors.clear(); retries.length = 0;
      completed.clear();
      slots.length = 0; state.elements.length = 0; state.texts.length = 0;
      if (globalThis.__topicDeskPageTranslation === state) globalThis.__topicDeskPageTranslation = null;
    },
    collect() {
      const texts = [];
      if (!state.token) return { texts, detectedLanguage: '' };
      if (document.hidden) return { texts, detectedLanguage: document.documentElement.lang || '', failed: errors.size };
      for (const retry of retries.splice(0)) {
        if (!retry.slot.isConnected) continue;
        const id = slots.length;
        slots.push(retry.slot); state.elements.push(retry.element); state.texts.push(retry.text);
        slotIds.get(retry.slot)?.add(id);
        retry.slot.onclick = () => state.retry(id);
        texts.push(retry.text);
      }
      invalidate(mutations.takeRecords());
      indexChanges();
      // IntersectionObserver handles regular scrolling; body-owned bare text
      // still needs a range check since the body never leaves the viewport.
      if (viewportDirty && groups.has(document.body)) pending.add(document.body);
      viewportDirty = false;
      const ready = [];
      for (const element of pending) {
        if (texts.length >= 24) break;
        pending.delete(element);
        const nodes = groups.get(element);
        if (!nodes) { seen.delete(element); ready.push([element, null, []]); continue; }
        if (!visible(element)) continue;
        let raw = '';
        const ordered = Array.from(nodes).sort((a, b) => a.compareDocumentPosition(b) & 2 ? 1 : -1);
        for (const node of ordered) {
          if (element === document.body) {
            const range = document.createRange();
            range.selectNodeContents(node);
            if (!Array.from(range.getClientRects()).some(r => r.bottom > 0 && r.top < innerHeight && r.right > 0 && r.left < innerWidth)) continue;
          }
          raw += node.textContent;
        }
        const text = raw.replace(/\s+/g, ' ').trim();
        if (seen.get(element) === text) continue;
        // Limit each native dispatch, not total page coverage. Remaining visible
        // blocks are picked up by the next pass without truncating their text.
        const parts = split(text);
        for (const part of parts) if (!completed.has(part)) texts.push(part);
        ready.push([element, text, parts]);
      }
      // Finish all geometry/style reads before adding spinners. Interleaved DOM
      // writes and reads otherwise force synchronous relayout for each block.
      for (const [element, text, parts] of ready) {
        removeOutput(element);
        if (text !== null) seen.set(element, text);
        const currentSlots = [];
        ownedSlots.set(element, currentSlots);
        for (const part of parts) {
          const slot = document.createElement('span');
          slot.className = 'topic-desk-translation';
          if (completed.has(part)) slot.textContent = completed.get(part);
          else slot.setAttribute('data-loading', '');
          element.appendChild(slot);
          currentSlots.push(slot);
          rendered.add(slot);
          if (!completed.has(part)) {
            slots.push(slot); state.elements.push(element); state.texts.push(part);
            slotIds.set(slot, new Set([slots.length - 1]));
          }
        }
      }
      return { texts, detectedLanguage: document.documentElement.lang || '', failed: errors.size };
    },
    apply(start, values, language, error) {
      let count = 0;
      values.forEach((value, offset) => {
        const slot = slots[start + offset];
        if (!slot?.isConnected) return;
        slot.removeAttribute('data-loading');
        slot.lang = language;
        slot.onclick = null; slot.onkeydown = null;
        const normalize = text => text.normalize('NFC').replace(/\s+/g, ' ').trim();
        if (!error) completed.set(state.texts[start + offset], value);
        if (!error && normalize(value) === normalize(state.texts[start + offset])) {
          rendered.delete(slot); slot.remove(); release(start + offset);
          return;
        }
        slot.textContent = error ? '⚠ 翻译未完成 · 点击重试' : value;
        if (error) {
          slot.setAttribute('data-error', '');
          slot.setAttribute('title', error);
          slot.setAttribute('aria-label', error);
          slot.setAttribute('role', 'button');
          slot.setAttribute('tabindex', '0');
          slot.onclick = () => state.retry(start + offset);
          slot.onkeydown = event => {
            if (event.key === 'Enter' || event.key === ' ') {
              event.preventDefault(); state.retry(start + offset);
            }
          };
          errors.add(start + offset);
        } else {
          errors.delete(start + offset);
          release(start + offset);
        }
        count++;
      });
      return count;
    },
    applyEntries(entries, language, error) {
      let count = 0;
      for (const [id, value] of entries) count += state.apply(id, [value], language, error);
      return count;
    },
    // Streaming text is provisional: keep the loading state and cache only the
    // final validated answer delivered through applyEntries.
    applyPartial(entries, language) {
      for (const [id, value] of entries) {
        const slot = slots[id];
        if (!slot?.isConnected || !slot.hasAttribute('data-loading')) continue;
        slot.lang = language;
        slot.textContent = value;
      }
    }
  };
  mutations.observe(document.body, { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: ['class', 'style', 'hidden', 'aria-hidden', 'translate', 'contenteditable'] });
  document.addEventListener('scroll', markViewport, { passive: true, capture: true });
  globalThis.addEventListener('resize', markViewport, { passive: true });
  document.addEventListener('visibilitychange', markViewport);
  globalThis.__topicDeskPageTranslation = state;
  return state.collect();
})
