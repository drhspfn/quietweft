---
version: alpha
colors:
  background: "#101C29"
  panel: "#17293B"
  primary: "#7DD4F6"
  text: "#E9F2F7"
  muted: "#9BAFC2"
  source: "#E4B679"
typography:
  body:
    fontFamily: '"Segoe UI Variable", "Segoe UI", system-ui, sans-serif'
  display:
    fontFamily: 'Bahnschrift, "Segoe UI", sans-serif'
rounded:
  panel: "12px"
  control: "7px"
spacing:
  panel: "14px"
  page: "24px"
components:
  route:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.text}"
  primaryAction:
    backgroundColor: "{colors.primary}"
  sourceMeter:
    backgroundColor: "{colors.source}"
  help:
    textColor: "{colors.muted}"
---
# Quietweft design

## Overview
Quietweft is a small local microphone utility, designed as a single audio route rather than a research dashboard. The generated Q mark carries the identity; the surrounding interface stays quiet. Audience: desktop callers and streamers. Product register, English only. No Japan-market workflow is implied.

## Colors
Midnight is the canvas, blue panel is the route, sky is the processed signal and active state. Amber identifies the source meter. Text states always accompany active color. Runtime ownership: `apps/desktop/src/styles.css` `:root` implements these values as CSS custom properties consumed by shared controls and Level. The mark is an asset, never a replacement for state text.

## Typography
Segoe UI Variable is the body family with platform fallbacks. Bahnschrift gives the wordmark a compact instrument-like character. Device names stay in native selects with platform-owned popup geometry. No remote fonts.

## Layout
One native window, 520 by 650 logical pixels, minimum 480 by 560. One document scrollbar owns overflow. Header, audio route, suppression switch, two actual meters, microphone action. No settings or secondary page. A narrow presentation reduces page padding without hiding controls.

## Elevation & Depth
Flat panels and one-pixel borders; no glow, gradients, or artificial waveform decoration.

## Shapes
Panels use 12px corners, controls 7px. The generated Q mark is the single expressive element.

## Components
Native selects own device selection. Destination buttons select call routing or headphones. The switch applies suppression without changing devices. Level meters use actual runtime amplitudes and are empty when stopped. The primary action keeps stable size while pending. `UX-CONTRACT.md` owns behavior.

## Do's and Don'ts
Use concise action names and truthful live state. Reserve meter animation for real signal. Do not fabricate quality metrics, claim hardware latency from algorithm delay, auto-start microphones, or add recording/file tests. Honor reduced motion and forced colors.
