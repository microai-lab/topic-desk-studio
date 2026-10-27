// Read publisher-provided card links as untrusted data; never rebuild a bare note URL.
(() => {
  const rows = [], seen = new Map();
  for (const link of document.querySelectorAll('a[href*="/explore/"],a[href*="/discovery/item/"]')) {
    let url;
    try { url = new URL(link.href, location.href); } catch (_) { continue; }
    const match = url.pathname.match(/^\/(?:explore|discovery\/item)\/([a-f0-9]{24})\/?$/i);
    if (!match || url.protocol !== 'https:' || !['xiaohongshu.com', 'www.xiaohongshu.com'].includes(url.hostname)) continue;
    const id = match[1].toLowerCase();
    const old = seen.get(id);
    if (old !== undefined) {
      if (!new URL(rows[old].url).searchParams.has('xsec_token') && url.searchParams.has('xsec_token')) rows[old].url = url.href;
      continue;
    }
    const card = link.closest('section, article, .note-item, [class*="note-item"]') || link.parentElement;
    const titleNode = card?.querySelector('.title span, .title, [class*="title"] span, [class*="title"]');
    const title = (titleNode?.textContent || link.getAttribute('aria-label') || '').trim();
    if (!title || rows.length >= 50) continue;
    seen.set(id, rows.length);
    rows.push({ id, title, url: url.href });
  }
  return rows;
})()
