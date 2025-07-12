# VSRG-Renderer

A Renderer/Playback Engine for VSRG Maps (Currently only Quaver)

Mainly made to showcase/display Quaver SV maps

## Usage

Run the renderer with the path to a song directory containing a `.qua` file:

```bash
cargo run -- path/to/song --fullscreen --rate 1.2 --volume 0.05 --mirror
```

Flags:

- `--fullscreen` start in fullscreen
- `--rate` playback rate (default `1.0`)
- `--volume` initial audio volume (default `0.03`)
- `--mirror` mirror notes horizontally
- `--no-sv` ignore scroll velocity changes
- `--no-ssf` ignore scroll speed factor changes
