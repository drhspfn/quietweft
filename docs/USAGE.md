# Using Quietweft

Download the desktop ZIP from [Releases](https://github.com/drhspfn/quietweft/releases/latest), extract the entire folder and open **Quietweft.exe**. Keep its model and runtime files together. Windows requires Microsoft Edge WebView2 Runtime; current Windows installations commonly already have it. The application is not code-signed, so Windows may display an unknown-publisher prompt.

1. Choose your microphone.
2. Choose **Listen in headphones** and your headphones to check the result. Avoid speakers because they can feed audio back into the microphone.
3. Press **Start microphone**. Switch noise removal on or off to compare. Press **Stop** when finished.

Audio is processed locally. The app does not upload recordings or automatically start the microphone. Version 1.4.4 uses the pinned model shipped in the archive. It has one suppression switch rather than class-specific controls.

## Calls and streaming

To send processed audio into Discord, a meeting application or a recorder, install a virtual audio endpoint separately, for example [VB-CABLE](https://vb-audio.com/Cable/). Follow its own installation and license instructions; no audio driver is included or installed by Quietweft.

In Quietweft choose **Send to a call**, then the virtual cable's playback/input endpoint. In your calling application select the cable's recording/output endpoint as its microphone. For VB-CABLE these are typically **CABLE Input** in Quietweft and **CABLE Output** in the calling application. Keep the calling application's speaker output on your headphones. Avoid stacking several suppression systems while comparing results.

Headphone monitoring and WAV processing work without a virtual cable. Feeding processed audio directly into another application's microphone requires a virtual endpoint or integration with that application.

## Troubleshooting

- **No microphone:** check Windows microphone privacy access and refresh the device list.
- **No call output:** install a virtual endpoint separately, or select headphone monitoring.
- **No model:** extract the complete ZIP. Do not move the executable out of its folder.
- **Dropouts:** close competing audio workloads and try another output device. Device buffering affects total latency.
- **Some sounds remain or speech changes:** results depend on the recording. There is no quality guarantee or claimed comparative quality score for this release.

For file processing, binary streams and integration, see [CLI](CLI.md) and [Embedding](EMBEDDING.md).
