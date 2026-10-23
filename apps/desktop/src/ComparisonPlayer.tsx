import { useEffect, useId, useRef, useState } from "react";
import { AudioLines, Download, Pause, Play, Square, Volume2 } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import "./player.css";

export type ComparisonClip = {
  inputAudio: string;
  outputAudio?: string;
  beforeWaveform: number[];
  afterWaveform?: number[];
  durationSec: number;
};
type Track = "before" | "after";
const clock = (seconds: number) => {
  const value = Math.max(0, Math.floor(seconds));
  return `${Math.floor(value / 60)}:${String(value % 60).padStart(2, "0")}`;
};

export default function ComparisonPlayer({
  clip,
}: {
  clip: ComparisonClip | null;
}) {
  const before = useRef<HTMLAudioElement>(null);
  const after = useRef<HTMLAudioElement>(null);
  const position = useRef(0);
  const request = useRef(0);
  const active = useRef<Track | null>(null);
  const [playing, setPlaying] = useState<Track | null>(null);
  const [time, setTime] = useState(0);
  const [measuredDuration, setMeasuredDuration] = useState(0);
  const [volumes, setVolumes] = useState({ before: 1, after: 1 });
  const [error, setError] = useState("");
  const [saving, setSaving] = useState<Track | null>(null);
  const [saveNotice, setSaveNotice] = useState("");
  const savePending = useRef(false);
  const id = useId().replace(/:/g, "");
  const duration =
    clip && Number.isFinite(clip.durationSec) && clip.durationSec > 0
      ? clip.durationSec
      : measuredDuration;
  const durationRef = useRef(duration);
  durationRef.current = duration;
  const audio = (track: Track) =>
    track === "before" ? before.current : after.current;

  async function saveTrack(track: Track) {
    const data = track === "before" ? clip?.inputAudio : clip?.outputAudio;
    if (!data || savePending.current) return;
    savePending.current = true;
    setSaving(track); setError(""); setSaveNotice("");
    try {
      const path = await invoke<string | null>("save_wav", { audio: data, track });
      if (path) setSaveNotice(`WAV saved: ${path}`);
    } catch (error) {
      setError(`Could not save WAV: ${String(error)}`);
    } finally {
      savePending.current = false; setSaving(null);
    }
  }

  // Seeking an unloaded WAV can throw. Keep the position and apply it when metadata arrives.
  function align(element: HTMLAudioElement | null, seconds: number) {
    if (!element || element.readyState === 0) return;
    const limit = Number.isFinite(element.duration)
      ? element.duration
      : durationRef.current;
    try {
      element.currentTime = Math.max(0, Math.min(seconds, limit || seconds));
    } catch {
      /* Metadata handler retries. */
    }
  }
  function seek(seconds: number) {
    const next = Math.max(0, Math.min(seconds, durationRef.current));
    position.current = next;
    setTime(next);
    align(before.current, next);
    align(after.current, next);
  }
  function stop() {
    ++request.current;
    active.current = null;
    before.current?.pause();
    after.current?.pause();
    setPlaying(null);
    seek(0);
  }
  async function toggle(track: Track) {
    const element = audio(track);
    if (!element) return;
    const token = ++request.current;
    const previous = active.current;
    if (previous) {
      const current = audio(previous);
      if (current && current.readyState >= 1) position.current = current.currentTime;
    }
    active.current = null;
    before.current?.pause();
    after.current?.pause();
    setPlaying(null);
    if (previous === track) {
      seek(position.current);
      return;
    }
    setError("");
    seek(
      duration > 0 && position.current >= duration - 0.02
        ? 0
        : position.current,
    );
    active.current = track;
    setPlaying(track);
    try {
      await element.play();
      if (request.current !== token && active.current !== track)
        element.pause();
    } catch {
      if (request.current !== token) return;
      active.current = null;
      setPlaying(null);
      setError(
        "Playback could not start. Try again, or record / load another clip.",
      );
    }
  }
  function metadata(element: HTMLAudioElement) {
    if (Number.isFinite(element.duration) && element.duration > 0)
      setMeasuredDuration(element.duration);
    align(element, position.current);
  }
  function failed(track: Track) {
    if (active.current === track) {
      ++request.current;
      active.current = null;
      setPlaying(null);
    }
    setError(
      `${track === "before" ? "Original" : "Processed"} audio could not be loaded. Record or load the clip again.`,
    );
  }

  useEffect(() => {
    ++request.current;
    active.current = null;
    before.current?.pause();
    after.current?.pause();
    position.current = 0;
    setTime(0);
    setPlaying(null);
    setMeasuredDuration(0);
    setError("");
    return () => {
      ++request.current;
      active.current = null;
      before.current?.pause();
      after.current?.pause();
    };
  }, [clip?.inputAudio, clip?.outputAudio]);

  useEffect(() => {
    if (before.current) before.current.volume = volumes.before;
    if (after.current) after.current.volume = volumes.after;
  }, [volumes, clip?.inputAudio, clip?.outputAudio]);

  useEffect(() => {
    if (!playing) return;
    let frame = 0;
    const tick = () => {
      const element = playing === "before" ? before.current : after.current;
      if (element && element.readyState >= 2 && !element.seeking && active.current === playing) {
        position.current = element.currentTime;
        setTime(element.currentTime);
        // The silent track follows too; switching tracks preserves the exact shared position.
        const other = playing === "before" ? after.current : before.current;
        if (other && Math.abs(other.currentTime - element.currentTime) > 0.08)
          align(other, element.currentTime);
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [playing]);

  const progress = duration > 0 ? Math.min(1, time / duration) : 0;
  const maxPeak = Math.max(
    0.001,
    ...(clip?.beforeWaveform ?? []).filter(Number.isFinite).map(Math.abs),
    ...(clip?.afterWaveform ?? []).filter(Number.isFinite).map(Math.abs),
  );
  function waveform(track: Track, points: number[]) {
    const clipId = `${id}-${track}`;
    return (
      <div className="comparison-waveform">
        <svg
          viewBox="0 0 1000 100"
          preserveAspectRatio="none"
          aria-hidden="true"
        >
          <defs>
            <clipPath id={clipId}>
              <rect width={progress * 1000} height="100" />
            </clipPath>
          </defs>
          <path d="M0 50 H1000" className="comparison-baseline" />
          {[false, true].map((played) => (
            <g
              key={String(played)}
              clipPath={played ? `url(#${clipId})` : undefined}
              className={
                played ? "comparison-peaks-played" : "comparison-peaks"
              }
            >
              {points.map((peak, index) => {
                const height = Number.isFinite(peak)
                  ? Math.max(1, (Math.abs(peak) / maxPeak) * 84)
                  : 1;
                const x = ((index + 0.5) * 1000) / points.length;
                return (
                  <line
                    key={index}
                    x1={x}
                    x2={x}
                    y1={50 - height / 2}
                    y2={50 + height / 2}
                    strokeWidth={Math.min(
                      6,
                      Math.max(1, (1000 / points.length) * 0.58),
                    )}
                    strokeLinecap="round"
                  />
                );
              })}
            </g>
          ))}
        </svg>
        <input
          className="comparison-scrubber"
          type="range"
          min="0"
          max={duration || 1}
          step="0.01"
          value={time}
          disabled={!duration}
          onChange={(event) => seek(Number(event.target.value))}
          aria-label={`Seek ${track === "before" ? "original" : "processed"} audio (linked timeline)`}
          aria-valuetext={`${clock(time)} of ${clock(duration)}`}
        />
        {!points.length && (
          <span className="comparison-no-peaks">Audio preview</span>
        )}
      </div>
    );
  }

  return (
    <section
      className="comparison-player"
      aria-label="Compare original and processed audio"
    >
      <header className="comparison-heading">
        <div>
          <h3>Listen & compare</h3>
          <p>One timeline. Switch tracks to hear the difference.</p>
        </div>
        <button
          className="comparison-stop"
          type="button"
          onClick={stop}
          disabled={!clip?.inputAudio}
          aria-label="Stop both tracks and rewind"
        >
          <Square size={14} /> Stop
        </button>
      </header>
      {!clip?.inputAudio ? (
        <div className="comparison-empty">
          <AudioLines size={28} />
          <p>Record your microphone or load a WAV to hear it here.</p>
        </div>
      ) : (
        <>
          <audio
            ref={before}
            src={clip.inputAudio}
            preload="metadata"
            hidden
            controls={false}
            onLoadedMetadata={(event) => metadata(event.currentTarget)}
            onEnded={() => {
              if (active.current === "before") {
                active.current = null;
                setPlaying(null);
                seek(duration);
              }
            }}
            onError={() => failed("before")}
          />
          {clip.outputAudio && (
            <audio
              ref={after}
              src={clip.outputAudio}
              preload="metadata"
              hidden
              controls={false}
              onLoadedMetadata={(event) => metadata(event.currentTarget)}
              onEnded={() => {
                if (active.current === "after") {
                  active.current = null;
                  setPlaying(null);
                  seek(duration);
                }
              }}
              onError={() => failed("after")}
            />
          )}
          {(["before", "after"] as const).map((track) => {
            const available = track === "before" || Boolean(clip.outputAudio);
            const name = track === "before" ? "Original" : "Processed";
            return (
              <div
                key={track}
                className={`comparison-track comparison-${track}${available ? "" : " comparison-unavailable"}`}
              >
                <div className="comparison-track-heading">
                  <span className="comparison-track-name">{name}</span>
                  <span className="comparison-track-status">
                    {playing === track
                      ? "Playing"
                      : available
                        ? "Ready to listen"
                        : "Process the recording to compare"}
                  </span>
                </div>
                <div className="comparison-track-body">
                  <button
                    className="comparison-play"
                    type="button"
                    disabled={!available}
                    onClick={() => void toggle(track)}
                    aria-label={`${playing === track ? "Pause" : "Play"} ${name.toLowerCase()}`}
                    aria-pressed={playing === track}
                  >
                    {playing === track ? (
                      <Pause size={21} fill="currentColor" />
                    ) : (
                      <Play size={21} fill="currentColor" />
                    )}
                  </button>
                  {available ? (
                    waveform(
                      track,
                      track === "before"
                        ? clip.beforeWaveform
                        : (clip.afterWaveform ?? []),
                    )
                  ) : (
                    <div className="comparison-output-empty">
                      Your processed audio will appear here.
                    </div>
                  )}
                </div>
                <div className="comparison-track-footer">
                  <div className="comparison-file-actions">
                  <span className="comparison-time">
                    {clock(time)} <span>/ {clock(duration)}</span>
                  </span>
                  {available && <button type="button" className="comparison-download" disabled={saving !== null} onClick={() => void saveTrack(track)} aria-label={`Save ${name.toLowerCase()} as WAV`}><Download size={13} aria-hidden="true" />{saving === track ? "Saving…" : "Save WAV"}</button>}
                  </div>
                  <label className="comparison-volume">
                    <Volume2 size={15} aria-hidden="true" />
                    <span className="comparison-volume-label">
                      {name} volume
                    </span>
                    <input
                      type="range"
                      min="0"
                      max="1"
                      step="0.01"
                      value={volumes[track]}
                      disabled={!available}
                      aria-label={`${name} volume`}
                      onChange={(event) =>
                        setVolumes((value) => ({
                          ...value,
                          [track]: Number(event.target.value),
                        }))
                      }
                    />
                    <output>{Math.round(volumes[track] * 100)}%</output>
                  </label>
                </div>
              </div>
            );
          })}
        </>
      )}
      {saveNotice && <p className="comparison-save-status" role="status">{saveNotice}</p>}
      {error && (
        <p className="comparison-error" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
