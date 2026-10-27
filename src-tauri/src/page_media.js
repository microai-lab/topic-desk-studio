// Unprivileged per-frame media monitor. Rust owns polling; pages receive no IPC.
(() => {
  if (globalThis.__topicDeskMedia) return;
  let muted = false, stopped = false;
  const originals = new WeakMap(), children = new Map(), contexts = new Set(), gates = new WeakMap();
  const frames = () => Array.from(document.querySelectorAll('iframe,frame')).map(frame => frame.contentWindow).filter(Boolean);
  const media = () => Array.from(document.querySelectorAll('audio,video'));
  const apply = element => {
    // A closing player must not restart while native cookie capture is pending.
    if (stopped && !element.paused) { try { element.pause(); } catch (_) {} }
    if (muted) {
      if (!originals.has(element)) originals.set(element, element.muted);
      if (!element.muted) element.muted = true;
    } else if (originals.has(element)) {
      element.muted = originals.get(element);
      originals.delete(element);
    }
  };
  const send = (target, message) => { try { target.postMessage(message, '*'); } catch (_) {} };
  const setMuted = value => {
    muted = stopped || value === true;
    for (const element of media()) apply(element);
    for (const context of contexts) gates.get(context).gain.value = muted ? 0 : 1;
    for (const child of frames()) send(child, { topicDeskMedia: 'mute', muted });
  };
  const shutdown = () => {
    stopped = true;
    setMuted(true);
    for (const child of frames()) send(child, { topicDeskMedia: 'shutdown' });
  };
  const sample = (probe = true) => {
    if (stopped) return { url: location.href, playing: false };
    const live = frames();
    for (const child of children.keys()) if (!live.includes(child)) children.delete(child);
    for (const child of probe ? live : []) send(child, { topicDeskMedia: 'sample', muted });
    let playing = media().some(element => !element.paused && !element.ended && element.readyState >= 2);
    for (const context of contexts) {
      if (context.state === 'closed') contexts.delete(context);
      else if (context.state === 'running') playing = true;
    }
    return { url: location.href, playing: playing || Array.from(children.values()).some(Boolean) };
  };
  const report = () => {
    if (parent !== globalThis) send(parent, { topicDeskMedia: 'state', playing: sample(false).playing });
  };
  addEventListener('message', event => {
    const message = event.data;
    if (!message || typeof message !== 'object') return;
    if (event.source === parent && parent !== globalThis && message.topicDeskMedia === 'shutdown') {
      shutdown(); report();
    } else if (event.source === parent && parent !== globalThis && ['mute', 'sample'].includes(message.topicDeskMedia)) {
      setMuted(message.muted === true);
      if (message.topicDeskMedia === 'sample') { sample(); report(); }
    } else if (message.topicDeskMedia === 'state' && typeof message.playing === 'boolean' && frames().includes(event.source)) {
      children.set(event.source, message.playing);
      report();
    }
  });
  for (const name of ['play', 'playing', 'pause', 'ended', 'emptied', 'volumechange']) {
    document.addEventListener(name, event => {
      if (event.target instanceof HTMLMediaElement) { apply(event.target); report(); }
    }, true);
  }
  // Gate Web Audio at the output, keeping its clock and video playback running.
  const prototype = globalThis.AudioNode?.prototype;
  if (prototype) {
    const connect = prototype.connect, disconnect = prototype.disconnect;
    prototype.connect = function(destination, ...args) {
      // Offline rendering exports audio data and must never inherit tab mute.
      const realtime = globalThis.AudioContext || globalThis.webkitAudioContext;
      if (realtime && this.context instanceof realtime && destination === this.context.destination) {
        let gate = gates.get(this.context);
        if (!gate) {
          gate = this.context.createGain();
          gate.gain.value = muted ? 0 : 1;
          connect.call(gate, this.context.destination);
          gates.set(this.context, gate); contexts.add(this.context);
          this.context.addEventListener('statechange', report);
        }
        connect.call(this, gate, ...args);
        report();
        return destination;
      }
      return connect.call(this, destination, ...args);
    };
    prototype.disconnect = function(destination, ...args) {
      if (arguments.length && destination === this.context.destination && gates.has(this.context)) {
        return disconnect.call(this, gates.get(this.context), ...args);
      }
      return arguments.length ? disconnect.call(this, destination, ...args) : disconnect.call(this);
    };
  }
  // New media respects tab mute before a later native polling cycle.
  new MutationObserver(() => { for (const element of media()) apply(element); report(); })
    .observe(document, { childList: true, subtree: true });
  globalThis.__topicDeskMedia = { setMuted, sample, shutdown };
})();
