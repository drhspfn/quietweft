# Quietweft interaction contract

Source: current public-release brief and shared `rust/audio` StartOptions/LiveMetrics API. There are no billing, accounts, remote permissions, deletion, or retained recordings in this interface.

| Capability | Canonical owner | Source of truth | Allowed variants | Verification |
|---|---|---|---|---|
| Select/Listbox | Native HTML select | App.tsx / DESIGN.md | Operating-system-owned popup | Native keyboard, opened popup |
| Form | App.tsx device labels and disabled states | StartOptions API | Microphone / output | Missing devices, failed start |
| Scrollbar | styles.css global baseline | DESIGN.md | Narrow document | Native window resize |
| Toast | No transient toast; inline feedback owner | App.tsx | Status / error | aria-live status and alert |

Microphone never starts on application launch. Explicit Start microphone enables capture and output; Stop microphone tears down the session. Closing the process releases streams. Devices and destination are locked while live. Refresh preserves a still-present selection, otherwise chooses the default available device. No device enumeration or refresh may start capture.

Send to a call shows virtual output devices; Listen in headphones shows ordinary output devices. Headphones mode is live monitoring, with a feedback warning. Empty virtual output gives installation/refresh guidance and an alternative headphone route. No driver installer is launched by this app.

Noise suppression is binary. Off passes the original signal through the running route; it does not stop the microphone. Successful IPC commits switch state; failure keeps prior state and gives inline text. Only one mutation is pending at a time. Start/stop errors preserve device selections. Audio device disconnect is displayed by runtime status, and stopped meters clear.

English interface and device-provided names. Native selects keep standard keyboard/popup behavior. Buttons have accessible names and visible focus. Status is polite; actionable errors are alerts. Disabled controls are noninteractive, busy buttons retain dimensions, and document scrolling remains available at small heights and zoom.
