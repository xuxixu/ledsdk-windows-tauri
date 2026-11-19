# Third-party notices

This repository redistributes the following third-party binaries under `sdk/` and in the built
`dist/` directory. This file is an inventory, not a replacement for the upstream license texts.

| Component | Files | License / action |
| --- | --- | --- |
| LedSDK | `sdk/x64/LedSDK.dll`, `sdk/x64/LedSDK.lib`, `sdk/linux-x64/libLedSDK.so*`, `sdk/ledsdk.h` | Vendor SDK; use is governed by the vendor agreement. |
| Qt 5 | `sdk/x64/Qt5*.dll`, `sdk/x64/platforms/qwindows.dll` | LGPLv3/GPLv3 or commercial Qt license; confirm the applicable option and include notices/source-offer requirements. |
| FFmpeg | `sdk/x64/av*.dll`, `sdk/x64/sw*.dll`, `sdk/x64/postproc-55.dll`, `ffmpeg.exe`, `ffprobe.exe` | LGPL/GPL depending on the build configuration; confirm the build flags and include the corresponding license. |
| Microsoft Visual C++ Redistributable | `sdk/x64/vcredist_x64.exe` | Microsoft redistributable terms; keep the original package and notices. |
| Rust crates | `Cargo.lock` | Each crate's license is identified by its package metadata; run `cargo metadata`/`cargo-deny` before release. |

Before public distribution, replace this inventory with the complete upstream license texts and
source-offer materials required by the applicable licenses. Do not ship the `dist/` directory
without completing that review.
