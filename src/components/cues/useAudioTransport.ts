import { useCallback, useEffect, useRef, useState } from "react";

// The editor's own transport: an HTMLAudioElement the webview decodes and
// plays through the laptop's output, so authoring never needs VirtualDJ.
// Time is read every animation frame into a ref (the canvas draws from
// it) and mirrored into React state at a gentle 10 Hz for the readouts.
// The element is torn down on unmount so the decoded track doesn't
// linger in memory once the editor closes (see the feature flag).

export interface LoopRegion {
  a: number | null;
  b: number | null;
  on: boolean;
}

export interface TransportTick {
  time: number;
  prev: number;
  /** The jump between prev and time was a seek/loop, not playback. */
  discontinuous: boolean;
}

export interface Transport {
  ready: boolean;
  playing: boolean;
  duration: number;
  rate: number;
  loop: LoopRegion;
  error: string | null;
  /** Current time, updated every frame (read, never write). */
  timeRef: React.MutableRefObject<number>;
  /** Current time at ~10 Hz for React renders. */
  displayTime: number;
  play: () => void;
  pause: () => void;
  toggle: () => void;
  seek: (secs: number) => void;
  nudge: (delta: number) => void;
  setRate: (rate: number) => void;
  setLoop: (loop: LoopRegion) => void;
}

const DISPLAY_HZ = 10;

export function useAudioTransport(
  src: string | null,
  onTick?: (tick: TransportTick) => void,
): Transport {
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const timeRef = useRef(0);
  const prevRef = useRef(0);
  const seekedRef = useRef(false);
  const loopRef = useRef<LoopRegion>({ a: null, b: null, on: false });
  const onTickRef = useRef(onTick);
  onTickRef.current = onTick;

  const [ready, setReady] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [duration, setDuration] = useState(0);
  const [rate, setRateState] = useState(1);
  const [loop, setLoopState] = useState<LoopRegion>({ a: null, b: null, on: false });
  const [error, setError] = useState<string | null>(null);
  const [displayTime, setDisplayTime] = useState(0);

  // One element for the hook's lifetime.
  useEffect(() => {
    const audio = new Audio();
    audio.preload = "auto";
    audioRef.current = audio;
    const onMeta = () => {
      setDuration(Number.isFinite(audio.duration) ? audio.duration : 0);
      setReady(true);
    };
    const onPlay = () => setPlaying(true);
    const onPause = () => setPlaying(false);
    const onEnded = () => setPlaying(false);
    const onError = () => {
      setError(audio.error?.message ?? "audio error");
      setReady(false);
    };
    audio.addEventListener("loadedmetadata", onMeta);
    audio.addEventListener("play", onPlay);
    audio.addEventListener("pause", onPause);
    audio.addEventListener("ended", onEnded);
    audio.addEventListener("error", onError);
    return () => {
      audio.removeEventListener("loadedmetadata", onMeta);
      audio.removeEventListener("play", onPlay);
      audio.removeEventListener("pause", onPause);
      audio.removeEventListener("ended", onEnded);
      audio.removeEventListener("error", onError);
      audio.pause();
      audio.removeAttribute("src");
      audio.load();
      audioRef.current = null;
    };
  }, []);

  // Source swaps.
  useEffect(() => {
    const audio = audioRef.current;
    if (!audio) return;
    setReady(false);
    setError(null);
    setDuration(0);
    timeRef.current = 0;
    prevRef.current = 0;
    setDisplayTime(0);
    audio.pause();
    if (src) {
      audio.src = src;
      audio.load();
    } else {
      audio.removeAttribute("src");
      audio.load();
    }
  }, [src]);

  // Frame loop: read time, enforce the A/B loop, tick the listener.
  useEffect(() => {
    let raf = 0;
    let lastDisplay = 0;
    const frame = (now: number) => {
      const audio = audioRef.current;
      if (audio) {
        let t = audio.currentTime;
        const lp = loopRef.current;
        if (lp.on && lp.a != null && lp.b != null && lp.b > lp.a && t >= lp.b && !audio.paused) {
          audio.currentTime = lp.a;
          t = lp.a;
          seekedRef.current = true;
        }
        const prev = prevRef.current;
        const discontinuous = seekedRef.current || t < prev || t - prev > 1.5;
        seekedRef.current = false;
        timeRef.current = t;
        if (t !== prev) {
          onTickRef.current?.({ time: t, prev, discontinuous });
        }
        prevRef.current = t;
        if (now - lastDisplay > 1000 / DISPLAY_HZ) {
          lastDisplay = now;
          setDisplayTime(t);
        }
      }
      raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(raf);
  }, []);

  const play = useCallback(() => {
    audioRef.current?.play().catch((e) => setError(e instanceof Error ? e.message : String(e)));
  }, []);
  const pause = useCallback(() => {
    audioRef.current?.pause();
  }, []);
  const toggle = useCallback(() => {
    const audio = audioRef.current;
    if (!audio) return;
    if (audio.paused) play();
    else audio.pause();
  }, [play]);
  const seek = useCallback((secs: number) => {
    const audio = audioRef.current;
    if (!audio) return;
    const max = Number.isFinite(audio.duration) ? audio.duration : Number.POSITIVE_INFINITY;
    const t = Math.max(0, Math.min(max, secs));
    audio.currentTime = t;
    timeRef.current = t;
    prevRef.current = t;
    seekedRef.current = true;
    setDisplayTime(t);
  }, []);
  const nudge = useCallback(
    (delta: number) => {
      seek(timeRef.current + delta);
    },
    [seek],
  );
  const setRate = useCallback((r: number) => {
    const audio = audioRef.current;
    if (audio) audio.playbackRate = r;
    setRateState(r);
  }, []);
  const setLoop = useCallback((lp: LoopRegion) => {
    loopRef.current = lp;
    setLoopState(lp);
  }, []);

  return {
    ready,
    playing,
    duration,
    rate,
    loop,
    error,
    timeRef,
    displayTime,
    play,
    pause,
    toggle,
    seek,
    nudge,
    setRate,
    setLoop,
  };
}
