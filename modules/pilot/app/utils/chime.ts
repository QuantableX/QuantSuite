/**
 * The "needs you" sound (setting: Play a sound): two short, quiet sine tones
 * made with WebAudio — no file, nothing to load. Browsers start an audio
 * context only after a user gesture; a refused one stays silent.
 */
let ctx: AudioContext | null = null

export function chime(): void {
  try {
    ctx ??= new AudioContext()
    if (ctx.state === 'suspended') void ctx.resume()
    const now = ctx.currentTime
    for (const [freq, at] of [
      [880, 0],
      [1174.66, 0.12],
    ] as const) {
      const osc = ctx.createOscillator()
      const gain = ctx.createGain()
      osc.type = 'sine'
      osc.frequency.value = freq
      gain.gain.setValueAtTime(0, now + at)
      gain.gain.linearRampToValueAtTime(0.08, now + at + 0.015)
      gain.gain.exponentialRampToValueAtTime(0.0001, now + at + 0.28)
      osc.connect(gain).connect(ctx.destination)
      osc.start(now + at)
      osc.stop(now + at + 0.3)
    }
  } catch {
    /* no audio: the toast and the badge still say it */
  }
}
