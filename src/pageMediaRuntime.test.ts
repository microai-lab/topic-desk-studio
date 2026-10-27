/** Deterministic fixtures for unprivileged media observation, frame relay, and output gating. */
import { readFileSync } from 'node:fs'
import vm from 'node:vm'
import { describe, expect, it, vi } from 'vitest'
const script = readFileSync(new URL('../src-tauri/src/page_media.js', import.meta.url), 'utf8')
function fixture(parent?: { postMessage: ReturnType<typeof vi.fn> }) {
  class Media {
    paused = true; ended = false; readyState = 2; muted = false
    pause() { this.paused = true }
  }
  class AudioNode {
    connections: AudioNode[] = []; disconnected: unknown[] = []; gain = { value: 1 }
    constructor(readonly context: AudioContext) {}
    connect(destination: AudioNode) { this.connections.push(destination); return destination }
    disconnect(...args: unknown[]) { this.disconnected = args }
  }
  class AudioContext {
    destination = new AudioNode(this); state = 'running'; listeners = new Map()
    createGain() { return new AudioNode(this) }
    addEventListener(name: string, callback: () => void) { this.listeners.set(name, callback) }
  }
  const media: Media[] = [], frames: { contentWindow: { postMessage: ReturnType<typeof vi.fn> } }[] = []
  const listeners = new Map(), documentListeners = new Map(), mutations: (() => void)[] = []
  const sandbox = vm.createContext({
    document: { querySelectorAll: (selector: string) => selector === 'audio,video' ? media : frames,
      addEventListener: (name: string, callback: unknown) => documentListeners.set(name, callback) },
    location: { href: 'https://example.com/article' },
    HTMLMediaElement: Media, AudioNode, AudioContext,
    MutationObserver: class { constructor(callback: () => void) { mutations.push(callback) } observe() {} },
    addEventListener: (name: string, callback: unknown) => listeners.set(name, callback),
    ...(parent ? { parent } : {}),
  })
  if (!parent) vm.runInContext('parent = globalThis', sandbox)
  vm.runInContext(script, sandbox)
  const runtime = sandbox.__topicDeskMedia as { setMuted(value: boolean): void; shutdown(): void; sample(): { playing: boolean } }
  return { runtime, Media, AudioNode, AudioContext, media, frames, documentListeners, mutations, listeners, sandbox }
}
describe('reader media runtime', () => {
  it('tracks playback and clears paused, ended, or unready media', () => {
    const f = fixture(), video = new f.Media(); f.media.push(video)
    expect(f.runtime.sample().playing).toBe(false)
    video.paused = false
    expect(f.runtime.sample().playing).toBe(true)
    video.ended = true
    expect(f.runtime.sample().playing).toBe(false)
    video.ended = false; video.readyState = 1
    expect(f.runtime.sample().playing).toBe(false)
  })
  it('restores existing website mute settings and enforces mute on new media', () => {
    const f = fixture(), audio = new f.Media(), alreadyMuted = new f.Media()
    alreadyMuted.muted = true; f.media.push(audio, alreadyMuted)
    f.runtime.setMuted(true)
    expect(audio.muted).toBe(true)
    const newAudio = new f.Media(); f.media.push(newAudio); f.mutations[0]!()
    expect(newAudio.muted).toBe(true)
    audio.muted = false; f.documentListeners.get('volumechange')({ target: audio })
    expect(audio.muted).toBe(true)
    f.runtime.setMuted(false)
    expect(audio.muted).toBe(false)
    expect(newAudio.muted).toBe(false)
    expect(alreadyMuted.muted).toBe(true)
  })
  it('gates Web Audio without suspending its clock and preserves connect/disconnect semantics', () => {
    const f = fixture(), context = new f.AudioContext(), source = new f.AudioNode(context)
    expect(source.connect(context.destination)).toBe(context.destination)
    const gate = source.connections[0]!
    expect(gate.connections[0]).toBe(context.destination)
    expect(f.runtime.sample().playing).toBe(true)
    f.runtime.setMuted(true)
    expect(gate.gain.value).toBe(0)
    expect(context.state).toBe('running')
    source.disconnect(context.destination)
    expect(source.disconnected[0]).toBe(gate)
    f.runtime.setMuted(false)
    expect(gate.gain.value).toBe(1)
    context.state = 'closed'
    expect(f.runtime.sample().playing).toBe(false)
  })
  it('leaves offline rendering output unchanged by tab mute', () => {
    const f = fixture()
    const offline = Object.assign(Object.create(null), new f.AudioContext())
    const source = new f.AudioNode(offline)
    f.runtime.setMuted(true)
    source.connect(offline.destination)
    expect(source.connections[0]).toBe(offline.destination)
    expect(f.runtime.sample().playing).toBe(false)
  })
  it('permanently stops closed players, including new media and Web Audio output', () => {
    const f = fixture(), video = new f.Media(), context = new f.AudioContext()
    const source = new f.AudioNode(context), child = { postMessage: vi.fn() }
    video.paused = false; f.media.push(video); f.frames.push({ contentWindow: child })
    source.connect(context.destination)
    f.runtime.shutdown()
    expect(video.paused).toBe(true)
    expect(video.muted).toBe(true)
    expect(source.connections[0]!.gain.value).toBe(0)
    expect(child.postMessage).toHaveBeenCalledWith({ topicDeskMedia: 'shutdown' }, '*')
    // A delayed polling unmute must not revive a tab awaiting native disposal.
    f.runtime.setMuted(false)
    video.paused = false; video.muted = false
    f.documentListeners.get('play')({ target: video })
    expect(video.paused).toBe(true)
    expect(video.muted).toBe(true)
    expect(source.connections[0]!.gain.value).toBe(0)
    const newVideo = new f.Media(); newVideo.paused = false; f.media.push(newVideo)
    f.mutations[0]!()
    expect(newVideo.paused).toBe(true)
    expect(newVideo.muted).toBe(true)
    expect(f.runtime.sample().playing).toBe(false)
  })
  it('accepts terminal shutdown only from its parent and relays it to nested frames', () => {
    const parent = { postMessage: vi.fn() }, f = fixture(parent), video = new f.Media()
    const child = { postMessage: vi.fn() }
    video.paused = false; f.media.push(video); f.frames.push({ contentWindow: child })
    const message = f.listeners.get('message')
    message({ source: {}, data: { topicDeskMedia: 'shutdown' } })
    expect(video.paused).toBe(false)
    message({ source: parent, data: { topicDeskMedia: 'shutdown' } })
    expect(video.paused).toBe(true)
    expect(child.postMessage).toHaveBeenCalledWith({ topicDeskMedia: 'shutdown' }, '*')
    expect(parent.postMessage.mock.calls.at(-1)![0]).toMatchObject({ playing: false })
  })
  it('accepts state only from live child frames and avoids recursive probe loops', () => {
    const f = fixture(), child = { postMessage: vi.fn() }, foreign = {}
    f.frames.push({ contentWindow: child })
    f.runtime.sample(); const probes = child.postMessage.mock.calls.length
    const message = f.listeners.get('message')
    message({ source: foreign, data: { topicDeskMedia: 'state', playing: true } })
    expect(f.runtime.sample().playing).toBe(false)
    const before = child.postMessage.mock.calls.length
    message({ source: child, data: { topicDeskMedia: 'state', playing: true } })
    expect(child.postMessage).toHaveBeenCalledTimes(before)
    expect(f.runtime.sample().playing).toBe(true)
    expect(probes).toBe(1)
    f.frames.length = 0
    expect(f.runtime.sample().playing).toBe(false)
  })
  it('relays mute and playback through nested frames without accepting foreign controls', () => {
    const parent = { postMessage: vi.fn() }, f = fixture(parent), video = new f.Media()
    f.media.push(video); video.paused = false
    const message = f.listeners.get('message')
    message({ source: {}, data: { topicDeskMedia: 'mute', muted: true } })
    expect(video.muted).toBe(false)
    message({ source: parent, data: { topicDeskMedia: 'sample', muted: true } })
    expect(video.muted).toBe(true)
    expect(parent.postMessage.mock.calls.at(-1)![0]).toMatchObject({ playing: true, topicDeskMedia: 'state' })
  })
  it('does not install multiple observers or AudioNode wrappers', () => {
    const f = fixture(), connect = f.AudioNode.prototype.connect
    vm.runInContext(script, f.sandbox)
    expect(f.AudioNode.prototype.connect).toBe(connect)
    expect(f.mutations).toHaveLength(1)
  })
})
