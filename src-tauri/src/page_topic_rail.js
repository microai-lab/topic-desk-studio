/* A page-local shadow overlay avoids native WebView gaps; it has no Tauri privileges. */
function (data) {
  const old = document.getElementById('__topic_desk_rail');
  if (!data.visible || !data.topics.length) { old?.remove(); return null; }
  const signature = JSON.stringify(data);
  if (old?.dataset.signature === signature) return null;
  old?.remove();
  const host = document.createElement('div');
  host.id = '__topic_desk_rail';
  host.setAttribute('translate', 'no');
  host.dataset.signature = signature;
  // Force only our host's geometry; never alter page margins or document layout.
  host.style.cssText = 'all:initial!important;position:fixed!important;left:0!important;top:0!important;width:32px!important;height:100vh!important;z-index:2147483647!important;pointer-events:none!important;display:block!important';
  const root = host.attachShadow({ mode: 'closed' });
  const dark = data.theme === 'dark' || (data.theme === 'system' && matchMedia('(prefers-color-scheme: dark)').matches);
  const style = document.createElement('style');
  style.textContent = `
    :host{color-scheme:${dark ? 'dark' : 'light'}}
    *{box-sizing:border-box}button{font-family:system-ui,sans-serif}
    nav{position:absolute;inset:0;display:flex;align-items:center;pointer-events:none}
    .markers{width:32px;max-height:calc(100% - 32px);overflow-y:auto;scrollbar-width:none;pointer-events:auto}
    .markers::-webkit-scrollbar{display:none}
    .marker{display:flex;align-items:center;justify-content:center;width:32px;height:16px;padding:0;border:0;background:transparent;cursor:pointer}
    /* Page colors can differ from app theme: light fills plus dark outlines work on either surface. */
    .marker i{width:10px;height:2px;border-radius:2px;background:#d6d6e3;box-shadow:0 0 0 1px #424250,0 1px 3px #0005;opacity:1;transition:width .14s,background-color .14s}
    .marker[aria-current] i{background:#a69eff;width:18px;height:3px;box-shadow:0 0 0 1px #424250,0 0 5px #a69eff88;opacity:1}
    .marker:hover i,.marker:focus-visible i{width:18px;background:#b9b3ff;opacity:1}
    button:focus-visible{outline:2px solid #6960ff;outline-offset:-2px}
    .preview{position:fixed;left:32px;width:min(340px,calc(100vw - 48px));padding-left:10px;pointer-events:auto}
    .card{display:grid;gap:9px;width:100%;padding:16px;text-align:left;border:1px solid ${dark ? '#41414d' : '#e2e2ec'};border-radius:14px;background:${dark ? '#24242c' : '#fff'};color:${dark ? '#f1f1f5' : '#202027'};box-shadow:0 8px 28px #0002;cursor:pointer;animation:appear .14s ease}
    small{font-size:11px;color:${dark ? '#aaaabb' : '#848496'}}
    strong{font-size:13px;line-height:1.6;overflow-wrap:anywhere;display:-webkit-box;-webkit-box-orient:vertical;-webkit-line-clamp:3;overflow:hidden}
    .hint{font-size:11px;color:#7870ff}
    @keyframes appear{from{opacity:0;transform:translateX(-4px)}to{opacity:1;transform:none}}
    @media(prefers-reduced-motion:reduce){.card{animation:none}.marker i{transition:none}}
  `;
  const nav = document.createElement('nav');
  nav.setAttribute('aria-label', data.locale === 'zh' ? '收起的话题列表' : 'Collapsed topics');
  const markers = document.createElement('div'); markers.className = 'markers';
  let preview = null;
  const dismiss = () => { preview?.remove(); preview = null; };
  const open = topic => {
    dismiss();
    const request = new URL('topic-desk-topic://open');
    request.searchParams.set('topic', JSON.stringify(topic));
    window.open(request.href, '_blank');
  };
  const reveal = (topic, marker) => {
    dismiss();
    preview = document.createElement('div'); preview.className = 'preview';
    preview.style.top = Math.max(12, Math.min(marker.getBoundingClientRect().top - 12, innerHeight - 180)) + 'px';
    const card = document.createElement('button'); card.type = 'button'; card.className = 'card';
    const source = document.createElement('small'); source.textContent = topic.platformName;
    const title = document.createElement('strong'); title.textContent = topic.title;
    const hint = document.createElement('span'); hint.className = 'hint'; hint.textContent = data.locale === 'zh' ? '在新页签中阅读 ↗' : 'Read in a new tab ↗';
    card.append(source, title, hint); card.onclick = () => open(topic);
    preview.append(card); nav.append(preview);
  };
  for (const topic of data.topics) {
    const marker = document.createElement('button'); marker.type = 'button'; marker.className = 'marker';
    marker.setAttribute('aria-label', topic.title);
    if (topic.id === data.active) marker.setAttribute('aria-current', 'page');
    marker.append(document.createElement('i'));
    marker.onpointerenter = () => reveal(topic, marker);
    marker.onfocus = () => reveal(topic, marker);
    marker.onclick = () => open(topic);
    markers.append(marker);
  }
  markers.onscroll = dismiss;
  nav.onpointerleave = dismiss;
  nav.onkeydown = event => { if (event.key === 'Escape') { event.stopPropagation(); dismiss(); } };
  nav.onfocusout = event => { if (!nav.contains(event.relatedTarget)) dismiss(); };
  nav.append(markers); root.append(style, nav); document.documentElement.append(host);
  return true;
}
