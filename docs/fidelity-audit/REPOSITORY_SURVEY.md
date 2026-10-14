# Repository survey ledger

2026-10-14. 74 paths checked; 68 live HEAD pins, six unresolved/obsolete paths excluded. Sixteen repositories have targeted code/header inspection across 21 sources. Zero whole codebases reviewed end-to-end. Root license labels are triage, not legal clearance or a claim about every file. Code/header detail and exact verified source URLs follow the table. No third-party implementation was imported into Kimo.

## Pinned repositories

| Repository | Commit | Root terms observed | Candidate to investigate | Coverage |
|---|---|---|---|---|
| [olive-editor/olive](https://github.com/olive-editor/olive) | `7e0e94abf6610026aebb9ddce8564c39522fac6e` | [GPL text observed](https://raw.githubusercontent.com/olive-editor/olive/7e0e94abf6610026aebb9ddce8564c39522fac6e/LICENSE) | Node graph scheduling and timeline/render separation | Metadata/license/README triage only |
| [mltframework/shotcut](https://github.com/mltframework/shotcut) | `ca026553afeb1c17da366db58674a9f6adfa9f01` | [GPL text observed](https://raw.githubusercontent.com/mltframework/shotcut/ca026553afeb1c17da366db58674a9f6adfa9f01/COPYING) | Filter graph and proxy/media workflow | Metadata/license/README triage only |
| [mltframework/mlt](https://github.com/mltframework/mlt) | `0cdaa1ddeb9235c0655e24853cd211794dfe95b8` | [LGPL text observed](https://raw.githubusercontent.com/mltframework/mlt/0cdaa1ddeb9235c0655e24853cd211794dfe95b8/COPYING) | Timebase, frame producer/consumer architecture | Metadata/license/README triage only |
| [NatronGitHub/Natron](https://github.com/NatronGitHub/Natron) | `3763d805d7d277d10af10025ae41af677682b3e6` | [GPL text observed](https://raw.githubusercontent.com/NatronGitHub/Natron/3763d805d7d277d10af10025ae41af677682b3e6/LICENSE.txt) | Typed image graph, tile/ROI scheduling | Metadata/license/README triage only |
| [blender/blender](https://github.com/blender/blender) | `c8338c9c19a699efb4c0e6e12ab8fa0746e5a6ed` | [GPL text observed](https://raw.githubusercontent.com/blender/blender/c8338c9c19a699efb4c0e6e12ab8fa0746e5a6ed/COPYING) | Geometry modifiers and scene/shading separation | Metadata/license/README triage only |
| [AcademySoftwareFoundation/OpenImageIO](https://github.com/AcademySoftwareFoundation/OpenImageIO) | `d418ea4d6ae7edd3bf1cd3879f79af58fe1d138d` | [Apache-2.0 text observed (check additional terms)](https://raw.githubusercontent.com/AcademySoftwareFoundation/OpenImageIO/d418ea4d6ae7edd3bf1cd3879f79af58fe1d138d/LICENSE.md) | Image cache, tiled I/O and color metadata | Metadata/license/README triage only |
| [AcademySoftwareFoundation/OpenColorIO](https://github.com/AcademySoftwareFoundation/OpenColorIO) | `22dd9c4faa0d1b7cfb3d64d1a6e9cb454acc6cfd` | [BSD-family text observed (clause review per file)](https://raw.githubusercontent.com/AcademySoftwareFoundation/OpenColorIO/22dd9c4faa0d1b7cfb3d64d1a6e9cb454acc6cfd/LICENSE) | Config-based color processor boundaries | Metadata/license/README triage only |
| [AcademySoftwareFoundation/Imath](https://github.com/AcademySoftwareFoundation/Imath) | `5b39e4699b028b3bdd5b9712be2fdc24ab0efcb1` | [BSD-family text observed (clause review per file)](https://raw.githubusercontent.com/AcademySoftwareFoundation/Imath/5b39e4699b028b3bdd5b9712be2fdc24ab0efcb1/LICENSE.md) | Half precision and robust geometry math | Metadata/license/README triage only |
| [AcademySoftwareFoundation/openexr](https://github.com/AcademySoftwareFoundation/openexr) | `43e49312e621619cb1a7150475f2a799a384c8df` | [BSD-family text observed (clause review per file)](https://raw.githubusercontent.com/AcademySoftwareFoundation/openexr/43e49312e621619cb1a7150475f2a799a384c8df/LICENSE.md) | HDR interchange and channel semantics | Metadata/license/README triage only |
| [AcademySoftwareFoundation/OpenShadingLanguage](https://github.com/AcademySoftwareFoundation/OpenShadingLanguage) | `0dc86fc3e30d5417492397646abc1b98490a10b2` | [BSD-family text observed (clause review per file)](https://raw.githubusercontent.com/AcademySoftwareFoundation/OpenShadingLanguage/0dc86fc3e30d5417492397646abc1b98490a10b2/LICENSE.md) | Typed shading execution and derivatives | Targeted code/header spans |
| [AcademySoftwareFoundation/MaterialX](https://github.com/AcademySoftwareFoundation/MaterialX) | `5ed154ef9ab5f45c6cf17d158830f8a637593857` | [Apache-2.0 text observed (check additional terms)](https://raw.githubusercontent.com/AcademySoftwareFoundation/MaterialX/5ed154ef9ab5f45c6cf17d158830f8a637593857/LICENSE) | Portable material graph and unit semantics | Targeted code/header spans |
| [google/filament](https://github.com/google/filament) | `cfb8e32833013d8531e64f2bfccfe8ce815797ff` | [Apache-2.0 text observed (check additional terms)](https://raw.githubusercontent.com/google/filament/cfb8e32833013d8531e64f2bfccfe8ce815797ff/LICENSE) | BRDF consistency and material parameterization | Targeted code/header spans |
| [mmp/pbrt-v4](https://github.com/mmp/pbrt-v4) | `b4ce9687e6c695f5582997c61b0c66cf064bdb4a` | [Apache-2.0 text observed (check additional terms)](https://raw.githubusercontent.com/mmp/pbrt-v4/b4ce9687e6c695f5582997c61b0c66cf064bdb4a/LICENSE.txt) | Sampling, ray geometry and physical camera references | Targeted code/header spans |
| [mitsuba-renderer/mitsuba3](https://github.com/mitsuba-renderer/mitsuba3) | `aa01500f57770a4ef0b067693cae08b3e8a2ac73` | [BSD-family text observed (clause review per file)](https://raw.githubusercontent.com/mitsuba-renderer/mitsuba3/aa01500f57770a4ef0b067693cae08b3e8a2ac73/LICENSE) | Integrator/material separation and sampling | Metadata/license/README triage only |
| [LuxCoreRender/LuxCore](https://github.com/LuxCoreRender/LuxCore) | `67df357416fff0ab2bb8cf21198f0bbcb77df665` | [Apache-2.0 text observed (check additional terms)](https://raw.githubusercontent.com/LuxCoreRender/LuxCore/67df357416fff0ab2bb8cf21198f0bbcb77df665/COPYING.txt) | Light transport and camera models | Metadata/license/README triage only |
| [appleseedhq/appleseed](https://github.com/appleseedhq/appleseed) | `c4ba82893febd711befb194f905e955d3ee143a3` | [MIT text observed](https://raw.githubusercontent.com/appleseedhq/appleseed/c4ba82893febd711befb194f905e955d3ee143a3/LICENSE.txt) | Tile render scheduling and material architecture | Metadata/license/README triage only |
| [libigl/libigl](https://github.com/libigl/libigl) | `7100764c2a2833284a8bdfa9b948d2b3bf124624` | no root license found in bounded paths; no copying authorized | Mesh processing and deformation boundaries | Metadata/license/README triage only |
| [CGAL/cgal](https://github.com/CGAL/cgal) | `6ff023d68ae9cd7585afa09dca8fc905d60ec0e9` | [custom/mixed/other; manual review required](https://raw.githubusercontent.com/CGAL/cgal/6ff023d68ae9cd7585afa09dca8fc905d60ec0e9/LICENSE.md) | Robust predicates and mesh operations | Metadata/license/README triage only |
| [g-truc/glm](https://github.com/g-truc/glm) | `6f14f4792a0cde5d0cf2c910506724d61cb95834` | no root license found in bounded paths; no copying authorized | Camera transforms and quaternion operations | Metadata/license/README triage only |
| [dimforge/nalgebra](https://github.com/dimforge/nalgebra) | `b2466e6c4070b06240d929895473c9141fe24720` | [Apache-2.0 text observed (check additional terms)](https://raw.githubusercontent.com/dimforge/nalgebra/b2466e6c4070b06240d929895473c9141fe24720/LICENSE) | Native Rust vector/matrix structures | Metadata/license/README triage only |
| [dimforge/parry](https://github.com/dimforge/parry) | `b3c3584ce9be9bf6edb821fd7548b817087131ff` | [Apache-2.0 text observed (check additional terms)](https://raw.githubusercontent.com/dimforge/parry/b3c3584ce9be9bf6edb821fd7548b817087131ff/LICENSE) | Convex collision queries and swept tests | Targeted code/header spans |
| [dimforge/rapier](https://github.com/dimforge/rapier) | `475caa3ff4646fd9a796b67d14116256f4962bc7` | [Apache-2.0 text observed (check additional terms)](https://raw.githubusercontent.com/dimforge/rapier/475caa3ff4646fd9a796b67d14116256f4962bc7/LICENSE) | Rigid-body architecture and deterministic stepping | Metadata/license/README triage only |
| [jrouwe/JoltPhysics](https://github.com/jrouwe/JoltPhysics) | `a4b24c3fadd7eade0435f27e9d6a07564273eb5f` | [MIT text observed](https://raw.githubusercontent.com/jrouwe/JoltPhysics/a4b24c3fadd7eade0435f27e9d6a07564273eb5f/LICENSE) | Broadphase and contact solver design | Metadata/license/README triage only |
| [bulletphysics/bullet3](https://github.com/bulletphysics/bullet3) | `d3afe76132fc76b0f9b2359cbaef47a3766e7fbb` | [zlib-style text observed; outside explicit MIT/Apache/BSD copy grant](https://raw.githubusercontent.com/bulletphysics/bullet3/d3afe76132fc76b0f9b2359cbaef47a3766e7fbb/LICENSE.txt) | Collision detection and constraints | Metadata/license/README triage only |
| [erincatto/box2d](https://github.com/erincatto/box2d) | `ac7c751eaeddbabdc1c4d41ae4f3a25d78627790` | [MIT text observed](https://raw.githubusercontent.com/erincatto/box2d/ac7c751eaeddbabdc1c4d41ae4f3a25d78627790/LICENSE) | 2D continuous collision/contact pipeline | Targeted code/header spans |
| [InteractiveComputerGraphics/PositionBasedDynamics](https://github.com/InteractiveComputerGraphics/PositionBasedDynamics) | `10a70bc146a97873dc3c8fef372f5217e010542e` | [MIT text observed](https://raw.githubusercontent.com/InteractiveComputerGraphics/PositionBasedDynamics/10a70bc146a97873dc3c8fef372f5217e010542e/LICENSE) | Position constraints and cloth formulation | Targeted code/header spans |
| [InteractiveComputerGraphics/SPlisHSPlasH](https://github.com/InteractiveComputerGraphics/SPlisHSPlasH) | `c493b75c6c6a41bd8e9dff611d82741c80c3756d` | [MIT text observed](https://raw.githubusercontent.com/InteractiveComputerGraphics/SPlisHSPlasH/c493b75c6c6a41bd8e9dff611d82741c80c3756d/LICENSE) | Particle-neighbor search and fluid constraints | Metadata/license/README triage only |
| [doyubkim/fluid-engine-dev](https://github.com/doyubkim/fluid-engine-dev) | `94c300ff5ad8a2f588e5e27e8e9746a424b29863` | [MIT text observed](https://raw.githubusercontent.com/doyubkim/fluid-engine-dev/94c300ff5ad8a2f588e5e27e8e9746a424b29863/LICENSE.md) | Advection, pressure projection and boundaries | Metadata/license/README triage only |
| [AcademySoftwareFoundation/openvdb](https://github.com/AcademySoftwareFoundation/openvdb) | `84dca76d5759b0ca8d4d30aa473437630a357c4f` | [Apache-2.0 text observed (check additional terms)](https://raw.githubusercontent.com/AcademySoftwareFoundation/openvdb/84dca76d5759b0ca8d4d30aa473437630a357c4f/LICENSE) | Sparse volumetric grid architecture | Metadata/license/README triage only |
| [manimCommunity/manim](https://github.com/manimCommunity/manim) | `622520d34190062f32d68af066c80f1c937fc8bf` | [MIT text observed](https://raw.githubusercontent.com/manimCommunity/manim/622520d34190062f32d68af066c80f1c937fc8bf/LICENSE) | Scene graph and animation composition | Targeted code/header spans |
| [3b1b/manim](https://github.com/3b1b/manim) | `fafa083a4fb274bba9cabde0b6e2f50ba6da0622` | [MIT text observed](https://raw.githubusercontent.com/3b1b/manim/fafa083a4fb274bba9cabde0b6e2f50ba6da0622/LICENSE.md) | Camera and scene orchestration | Metadata/license/README triage only |
| [motion-canvas/motion-canvas](https://github.com/motion-canvas/motion-canvas) | `7b91435c301d530351dcf5ebb91dd139c002e405` | [MIT text observed](https://raw.githubusercontent.com/motion-canvas/motion-canvas/7b91435c301d530351dcf5ebb91dd139c002e405/LICENSE) | Signal/tween composition and timeline semantics | Targeted code/header spans |
| [remotion-dev/remotion](https://github.com/remotion-dev/remotion) | `66953aa534f3aeed0197a95eaf6392a887c6060b` | [Remotion custom commercial restrictions](https://raw.githubusercontent.com/remotion-dev/remotion/66953aa534f3aeed0197a95eaf6392a887c6060b/LICENSE.md) | Frame-addressable composition and rendering | Metadata/license/README triage only |
| [NatronGitHub/openfx](https://github.com/NatronGitHub/openfx) | `0759758d0fb57932b5c4698fff9ef6b4a3132309` | [BSD-family text observed (clause review per file)](https://raw.githubusercontent.com/NatronGitHub/openfx/0759758d0fb57932b5c4698fff9ef6b4a3132309/LICENSE) | Image plugin ABI and ownership boundaries | Metadata/license/README triage only |
| [AcademySoftwareFoundation/openfx](https://github.com/AcademySoftwareFoundation/openfx) | `adb790bf0043714cae6091d58791e8cdabdd9e12` | [BSD-family text observed (clause review per file)](https://raw.githubusercontent.com/AcademySoftwareFoundation/openfx/adb790bf0043714cae6091d58791e8cdabdd9e12/LICENSE.md) | Image effect contracts and ROI | Metadata/license/README triage only |
| [gl-transitions/gl-transitions](https://github.com/gl-transitions/gl-transitions) | `b2dfdcfbec079af4a05306d32b9366aa495c5155` | [MIT text observed](https://raw.githubusercontent.com/gl-transitions/gl-transitions/b2dfdcfbec079af4a05306d32b9366aa495c5155/LICENSE) | Parametric transition shaders and endpoint contracts | Targeted code/header spans |
| [patriciogonzalezvivo/lygia](https://github.com/patriciogonzalezvivo/lygia) | `ce08fe351478b6c14297d103a8ce98d8e787d4d2` | [Prosperity-3.0 noncommercial/trial; restrictive](https://raw.githubusercontent.com/patriciogonzalezvivo/lygia/ce08fe351478b6c14297d103a8ce98d8e787d4d2/LICENSE.md) | Composed shader functions and noise families | Metadata/license/README triage only |
| [ashima/webgl-noise](https://github.com/ashima/webgl-noise) | `6abed1e77ed1e18b181627c35f688eb30c9fe75e` | [MIT text observed](https://raw.githubusercontent.com/ashima/webgl-noise/6abed1e77ed1e18b181627c35f688eb30c9fe75e/LICENSE) | Simplex/gradient noise and derivatives | Targeted code/header spans |
| [stegu/webgl-noise](https://github.com/stegu/webgl-noise) | `22434e04d7753f7e949e8d724ab3da2864c17a0f` | [MIT text observed](https://raw.githubusercontent.com/stegu/webgl-noise/22434e04d7753f7e949e8d724ab3da2864c17a0f/LICENSE) | Periodic noise and derivative variants | Metadata/license/README triage only |
| [Auburn/FastNoiseLite](https://github.com/Auburn/FastNoiseLite) | `785f37a9ad76e283586a379675085f2063ae03f7` | [MIT text observed](https://raw.githubusercontent.com/Auburn/FastNoiseLite/785f37a9ad76e283586a379675085f2063ae03f7/LICENSE) | Coherent noise, cellular and domain warp | Targeted code/header spans |
| [Auburn/FastNoise2](https://github.com/Auburn/FastNoise2) | `c1738b2cc834f92d32f4392991c8c634dec2677d` | [MIT text observed](https://raw.githubusercontent.com/Auburn/FastNoise2/c1738b2cc834f92d32f4392991c8c634dec2677d/LICENSE) | SIMD noise graph and field composition | Metadata/license/README triage only |
| [Razaekel/noise-rs](https://github.com/Razaekel/noise-rs) | `77de33887620c2a7e7263b1d30ef46afbfe705b6` | [MIT text observed](https://raw.githubusercontent.com/Razaekel/noise-rs/77de33887620c2a7e7263b1d30ef46afbfe705b6/LICENSE-MIT) | Rust noise generator composition | Targeted code/header spans |
| [SRombauts/SimplexNoise](https://github.com/SRombauts/SimplexNoise) | `175add2d2a9ba99a43320b66bb87bb21b879b0f5` | [MIT text observed](https://raw.githubusercontent.com/SRombauts/SimplexNoise/175add2d2a9ba99a43320b66bb87bb21b879b0f5/LICENSE.txt) | Compact simplex noise API | Targeted code/header spans |
| [keijiro/NoiseShader](https://github.com/keijiro/NoiseShader) | `550100d4a74de1ba90eb1b8e90f25f9dbeec28d2` | [MIT text observed](https://raw.githubusercontent.com/keijiro/NoiseShader/550100d4a74de1ba90eb1b8e90f25f9dbeec28d2/LICENSE) | Derivative-aware gradient/simplex noise | Metadata/license/README triage only |
| [libretro/glsl-shaders](https://github.com/libretro/glsl-shaders) | `148ce728155232a6b63be772c1810bd84728f359` | no root license found in bounded paths; no copying authorized | Shader collection licensing and pass chains | Metadata/license/README triage only |
| [crosire/reshade-shaders](https://github.com/crosire/reshade-shaders) | `fd0022170615ce0d8162d219bff07232fa6dd84f` | no root license found in bounded paths; no copying authorized | Post-processing shader pass architecture | Metadata/license/README triage only |
| [pmndrs/postprocessing](https://github.com/pmndrs/postprocessing) | `9cf03cff26615636a564d8bfddf765edb8917441` | [zlib-style text observed; outside explicit MIT/Apache/BSD copy grant](https://raw.githubusercontent.com/pmndrs/postprocessing/9cf03cff26615636a564d8bfddf765edb8917441/LICENSE.md) | Effect pass composition and depth inputs | Metadata/license/README triage only |
| [mrdoob/three.js](https://github.com/mrdoob/three.js) | `7d0d76bd6cd69a95c01e2913eed5b86ff758ca91` | [MIT text observed](https://raw.githubusercontent.com/mrdoob/three.js/7d0d76bd6cd69a95c01e2913eed5b86ff758ca91/LICENSE) | Camera/material/channel contracts | Targeted code/header spans |
| [BabylonJS/Babylon.js](https://github.com/BabylonJS/Babylon.js) | `abdd416bcde754c13c88c2bf8c7b491ffa247dff` | [Apache-2.0 text observed (check additional terms)](https://raw.githubusercontent.com/BabylonJS/Babylon.js/abdd416bcde754c13c88c2bf8c7b491ffa247dff/license.md) | Material nodes and scene graph semantics | Metadata/license/README triage only |
| [pixijs/pixijs](https://github.com/pixijs/pixijs) | `65552017d23255c1442a7d4ac7f0d49e3225fb1d` | [MIT text observed](https://raw.githubusercontent.com/pixijs/pixijs/65552017d23255c1442a7d4ac7f0d49e3225fb1d/LICENSE) | 2D filters, masks and alpha composition | Metadata/license/README triage only |
| [servo/webrender](https://github.com/servo/webrender) | `e1c924ebad9ffdfe8c8c606aba77eb3f888c396a` | [MPL text observed](https://raw.githubusercontent.com/servo/webrender/e1c924ebad9ffdfe8c8c606aba77eb3f888c396a/LICENSE) | Tiled render tasks and picture cache | Metadata/license/README triage only |
| [linebender/vello](https://github.com/linebender/vello) | `db90a73ec7309dfa2bd45886b353162a9ea95744` | [MIT text observed](https://raw.githubusercontent.com/linebender/vello/db90a73ec7309dfa2bd45886b353162a9ea95744/LICENSE-MIT) | Vector scene encoding and antialiasing | Metadata/license/README triage only |
| [linebender/tiny-skia](https://github.com/linebender/tiny-skia) | `5d4754777746eef0828be166896eaf482c49f8f2` | [BSD-family text observed (clause review per file)](https://raw.githubusercontent.com/linebender/tiny-skia/5d4754777746eef0828be166896eaf482c49f8f2/LICENSE) | CPU vector raster and coverage | Targeted code/header spans |
| [gfx-rs/wgpu](https://github.com/gfx-rs/wgpu) | `60036cdc0ad979c96ec8277df41ec35b098502f6` | no root license found in bounded paths; no copying authorized | Explicit resource/pipeline abstraction, deferred GPU only | Metadata/license/README triage only |
| [gpuweb/gpuweb](https://github.com/gpuweb/gpuweb) | `25a5dc4537074c9b3844dd95891f5cfd193f4407` | [W3C document + BSD software split; specific file review required](https://raw.githubusercontent.com/gpuweb/gpuweb/25a5dc4537074c9b3844dd95891f5cfd193f4407/LICENSE.md) | Portable graphics contract, not a runtime dependency | Metadata/license/README triage only |
| [FFmpeg/FFmpeg](https://github.com/FFmpeg/FFmpeg) | `045dea2a34ef93f854f3f4a30ddf8f77b35b34ce` | [LGPL text observed](https://raw.githubusercontent.com/FFmpeg/FFmpeg/045dea2a34ef93f854f3f4a30ddf8f77b35b34ce/LICENSE.md) | Filter graph/frame ownership and timebase | Metadata/license/README triage only |
| [videolan/vlc](https://github.com/videolan/vlc) | `3127258a2dac31be87f4ead6492d3862a8a90a31` | [GPL text observed](https://raw.githubusercontent.com/videolan/vlc/3127258a2dac31be87f4ead6492d3862a8a90a31/COPYING) | Media lifecycle and playback architecture | Metadata/license/README triage only |
| [GNOME/gegl](https://github.com/GNOME/gegl) | `5a15f6d2b5e5b432d288d0d800600d547eda58bb` | [GPL text observed](https://raw.githubusercontent.com/GNOME/gegl/5a15f6d2b5e5b432d288d0d800600d547eda58bb/COPYING) | Region-of-interest processing and buffers | Metadata/license/README triage only |
| [opencv/opencv](https://github.com/opencv/opencv) | `b63e0d394b135acb8d881d2e340be97a25d85ab7` | [Apache-2.0 text observed (check additional terms)](https://raw.githubusercontent.com/opencv/opencv/b63e0d394b135acb8d881d2e340be97a25d85ab7/LICENSE) | Image warp, interpolation and filtering | Metadata/license/README triage only |
| [halide/Halide](https://github.com/halide/Halide) | `4e272cbfcc35a107ca11b8839a06ebae2b36221f` | [MIT text observed](https://raw.githubusercontent.com/halide/Halide/4e272cbfcc35a107ca11b8839a06ebae2b36221f/LICENSE.txt) | Algorithm/schedule separation and image pipelines | Metadata/license/README triage only |
| [AcademySoftwareFoundation/rez](https://github.com/AcademySoftwareFoundation/rez) | `f622b817ef635223ca904a4e745d23e709738936` | [Apache-2.0 text observed (check additional terms)](https://raw.githubusercontent.com/AcademySoftwareFoundation/rez/f622b817ef635223ca904a4e745d23e709738936/LICENSE) | Dependency environment provenance | Metadata/license/README triage only |
| [kfrlib/kfr](https://github.com/kfrlib/kfr) | `7c1e55c84b464493695f8af37fa215f53c7798ba` | [GPL text observed](https://raw.githubusercontent.com/kfrlib/kfr/7c1e55c84b464493695f8af37fa215f53c7798ba/LICENSE.txt) | DSP/vector primitives and license boundary | Metadata/license/README triage only |
| [PixarAnimationStudios/OpenSubdiv](https://github.com/PixarAnimationStudios/OpenSubdiv) | `5a0a2bc65643e46979e64674720e398f2b9560c7` | [Tomorrow Open Source Technology License 1.0; not unmodified Apache-2.0](https://raw.githubusercontent.com/PixarAnimationStudios/OpenSubdiv/5a0a2bc65643e46979e64674720e398f2b9560c7/LICENSE.txt) | Subdivision patches and adaptive evaluation | Metadata/license/README triage only |
| [dyne/frei0r](https://github.com/dyne/frei0r) | `618dde79716fab4e428a05e6476325b9e6f5b063` | [GPL text observed](https://raw.githubusercontent.com/dyne/frei0r/618dde79716fab4e428a05e6476325b9e6f5b063/COPYING) | Lightweight image effect plugin ABI | Metadata/license/README triage only |
| [metaory/shadertoy2webgl](https://github.com/metaory/shadertoy2webgl) | `e3c0da7cfa4f3a5241a2a200463cca73393ea032` | [MIT text observed](https://raw.githubusercontent.com/metaory/shadertoy2webgl/e3c0da7cfa4f3a5241a2a200463cca73393ea032/LICENSE) | Uniform/coordinate portability; individual shader rights separate | Metadata/license/README triage only |
| [jwinarske/shadertoy-cxx](https://github.com/jwinarske/shadertoy-cxx) | `0c73ea735429345071a66e82e55cc3c56e58620e` | [MIT text observed](https://raw.githubusercontent.com/jwinarske/shadertoy-cxx/0c73ea735429345071a66e82e55cc3c56e58620e/LICENSE) | C++ shader host; not blanket shader content rights | Metadata/license/README triage only |
| [reindernijhoff/shadertoy-package](https://github.com/reindernijhoff/shadertoy-package) | `53c45964259dc00c1d8f4d9ff1577afb9b9f5128` | [MIT text observed](https://raw.githubusercontent.com/reindernijhoff/shadertoy-package/53c45964259dc00c1d8f4d9ff1577afb9b9f5128/LICENSE) | Shader packaging and per-shader license boundary | Metadata/license/README triage only |
| [felixrieseberg/shadertoy-in-threejs](https://github.com/felixrieseberg/shadertoy-in-threejs) | `4ef049f7fe89ee1abca719c4f9cfe5bb2bf61a83` | no root license found in bounded paths; no copying authorized | Shader host adaptation and source provenance | Metadata/license/README triage only |

## Excluded unresolved paths

- https://github.com/opensubdiv/OpenSubdiv: HEAD could not be resolved at collection time. Not proof the project does not exist.
- https://github.com/AcademySoftwareFoundation/nanovdb: HEAD could not be resolved at collection time. Not proof the project does not exist.
- https://github.com/patriciogonzalezvivo/glsl-noise: HEAD could not be resolved at collection time. Not proof the project does not exist.
- https://github.com/frei0r/frei0r: HEAD could not be resolved at collection time. Not proof the project does not exist.
- https://github.com/Movit/movit: HEAD could not be resolved at collection time. Not proof the project does not exist.
- https://github.com/gegl/gegl: HEAD could not be resolved at collection time. Not proof the project does not exist.

OpenSubdiv was resolved at PixarAnimationStudios/OpenSubdiv; frei0r at dyne/frei0r; GEGL at GNOME/gegl. NanoVDB is not counted as a separate verified repo; OpenVDB README advertises Nano in that tree. Ashima/stegu/NoiseShader and OpenFX forks are related implementations, not independent algorithm inventions.

## Specific inspected sources

### Auburn/FastNoiseLite / Rust/src/lib.rs

- Pin: `785f37a9ad76e283586a379675085f2063ae03f7`
- Observed and byte-verified source URL: https://raw.githubusercontent.com/Auburn/FastNoiseLite/785f37a9ad76e283586a379675085f2063ae03f7/Rust/src/lib.rs
- Inspected lines: 1-25, 650-660, 1026-1070, 2094-2126; SHA256 `fe6528b871e1256e6fcc408e938db12f9035a6bba282253d9c9de8915f87178c`
- Finding: Quintic lattice Perlin and bounded octave FBM, with domain warp entry points. Existing Rust source is a candidate adapter, not a new evaluator dependency by default.

### ashima/webgl-noise / src/noise3D.glsl

- Pin: `6abed1e77ed1e18b181627c35f688eb30c9fe75e`
- Observed and byte-verified source URL: https://raw.githubusercontent.com/ashima/webgl-noise/6abed1e77ed1e18b181627c35f688eb30c9fe75e/src/noise3D.glsl
- Inspected lines: 1-116; SHA256 `bf18982b8edd25218b5096cba6f7838a0508416be51acd1fd88d0ba8acb5169e`
- Finding: Textureless simplex gradients, permutation polynomial and compact-support contributions; GLSL-to-Rust port requires explicit float/reference policy.

### ashima/webgl-noise / src/psrdnoise2D.glsl

- Pin: `6abed1e77ed1e18b181627c35f688eb30c9fe75e`
- Observed and byte-verified source URL: https://raw.githubusercontent.com/ashima/webgl-noise/6abed1e77ed1e18b181627c35f688eb30c9fe75e/src/psrdnoise2D.glsl
- Inspected lines: 1-75; SHA256 `1565234e052c520742483da77cee6b40a1c1fdf94587d3cf933d1dfd6e31df9f`
- Finding: Periodic rotating-gradient derivative contract; header warns about rare lattice precision artifacts, so do not adopt without continuity tests.

### SRombauts/SimplexNoise / src/SimplexNoise.cpp

- Pin: `175add2d2a9ba99a43320b66bb87bb21b879b0f5`
- Observed and byte-verified source URL: https://raw.githubusercontent.com/SRombauts/SimplexNoise/175add2d2a9ba99a43320b66bb87bb21b879b0f5/src/SimplexNoise.cpp
- Inspected lines: 1-28; SHA256 `71ea9417a0960ade39fdfee9e8d25104d2bf4690286eab1f609c8a40f15f29e0`
- Finding: File provenance explicitly credits Gustavson/Eastman and MIT; only header inspected, implementation not reviewed.

### gl-transitions/gl-transitions / transitions/CrossZoom.glsl

- Pin: `b2dfdcfbec079af4a05306d32b9366aa495c5155`
- Observed and byte-verified source URL: https://raw.githubusercontent.com/gl-transitions/gl-transitions/b2dfdcfbec079af4a05306d32b9366aa495c5155/transitions/CrossZoom.glsl
- Inspected lines: 1-68; SHA256 `a30adba2a78deac0ac114a30ea285e972fdd7d488ed0cde90e15bc0eef333e84`
- Finding: MIT file, weighted radial zoom samples and progress-controlled dissolve. Keep rectalogic/gre/glfx attribution chain; no blanket shader-bundle license.

### gl-transitions/gl-transitions / transitions/Directional.glsl

- Pin: `b2dfdcfbec079af4a05306d32b9366aa495c5155`
- Observed and byte-verified source URL: https://raw.githubusercontent.com/gl-transitions/gl-transitions/b2dfdcfbec079af4a05306d32b9366aa495c5155/transitions/Directional.glsl
- Inspected lines: 1-25; SHA256 `fd217e157ec516a0770d4cf4e343b5d92b147b88ea3075842f16adcc5c3e7341`
- Finding: MIT file; raw coordinate push/wrap, independently parameterize direction and progress.

### motion-canvas/motion-canvas / packages/core/src/tweening/tween.ts

- Pin: `7b91435c301d530351dcf5ebb91dd139c002e405`
- Observed and byte-verified source URL: https://raw.githubusercontent.com/motion-canvas/motion-canvas/7b91435c301d530351dcf5ebb91dd139c002e405/packages/core/src/tweening/tween.ts
- Inspected lines: 1-40; SHA256 `a97974e00a462c1b13996ec325b719a0e60ff3860ccc22fca7a83073b6912f58`
- Finding: Logical end time differs from discrete frame advancement; preserve exact end-time accounting and callbacks.

### motion-canvas/motion-canvas / packages/core/src/tweening/tween.test.ts

- Pin: `7b91435c301d530351dcf5ebb91dd139c002e405`
- Observed and byte-verified source URL: https://raw.githubusercontent.com/motion-canvas/motion-canvas/7b91435c301d530351dcf5ebb91dd139c002e405/packages/core/src/tweening/tween.test.ts
- Inspected lines: 1-80; SHA256 `f35b01f5a58ffb83b0cca755925672f08ba7a719d04d2e182e36752e5789ec1e`
- Finding: Duration invariance at 24/60 fps and accumulated time offsets; adapt tests, not TS runtime.

### motion-canvas/motion-canvas / packages/core/src/tweening/spring.ts

- Pin: `7b91435c301d530351dcf5ebb91dd139c002e405`
- Observed and byte-verified source URL: https://raw.githubusercontent.com/motion-canvas/motion-canvas/7b91435c301d530351dcf5ebb91dd139c002e405/packages/core/src/tweening/spring.ts
- Inspected lines: 1-85; SHA256 `529743b4f317731feffcc43b6fb40cc83254870ae0173ce9cb62a3120b5beac6`
- Finding: Mass/stiffness/damping validation and initial-velocity API; Kimo already has analytic springs, do not replace with an integrator.

### google/filament / shaders/src/surface_brdf.fs

- Pin: `cfb8e32833013d8531e64f2bfccfe8ce815797ff`
- Observed and byte-verified source URL: https://raw.githubusercontent.com/google/filament/cfb8e32833013d8531e64f2bfccfe8ce815797ff/shaders/src/surface_brdf.fs
- Inspected lines: 1-85, 120-170; SHA256 `c0d76d244ace64a35beb331784f5358a5863de351643fcb702eb66e7cb84b9f2`
- Finding: GGX/Smith/Schlick variants and anisotropy; port small attributed scalar math only, not Filament renderer or mobile precision macros.

### dimforge/parry / src/query/shape_cast/shape_cast_ball_ball.rs

- Pin: `b3c3584ce9be9bf6edb821fd7548b817087131ff`
- Observed and byte-verified source URL: https://raw.githubusercontent.com/dimforge/parry/b3c3584ce9be9bf6edb821fd7548b817087131ff/src/query/shape_cast/shape_cast_ball_ball.rs
- Inspected lines: 1-100; SHA256 `17d0def3152276ae39b6a17603ec11893aef7b932d4984cffef7f08ae2ca52dc`
- Finding: Translational sphere sweep via relative ray; maximum time, penetration status and witness/normal outputs are valuable raw contracts.

### erincatto/box2d / src/distance.c

- Pin: `ac7c751eaeddbabdc1c4d41ae4f3a25d78627790`
- Observed and byte-verified source URL: https://raw.githubusercontent.com/erincatto/box2d/ac7c751eaeddbabdc1c4d41ae4f3a25d78627790/src/distance.c
- Inspected lines: 614-687, 1148-1190; SHA256 `2f70c6e12ac54c884177b3ec8078536d38b984847d931a5bf49d52d6459dc097`
- Finding: Conservative shape advancement and bounded TOI iteration; larger dependencies and remaining solve path not reviewed.

### InteractiveComputerGraphics/PositionBasedDynamics / PositionBasedDynamics/PositionBasedDynamics.cpp

- Pin: `10a70bc146a97873dc3c8fef372f5217e010542e`
- Observed and byte-verified source URL: https://raw.githubusercontent.com/InteractiveComputerGraphics/PositionBasedDynamics/10a70bc146a97873dc3c8fef372f5217e010542e/PositionBasedDynamics/PositionBasedDynamics.cpp
- Inspected lines: 1-85; SHA256 `9eab9c633258bb58bab99f9ed979da0ac9eb1499d86829b7637d28650eb84a52`
- Finding: Inverse-mass distance projection and dihedral gradients; constraints should be explicit primitives, with zero-length/degenerate guards.

### mrdoob/three.js / src/cameras/PerspectiveCamera.js

- Pin: `7d0d76bd6cd69a95c01e2913eed5b86ff758ca91`
- Observed and byte-verified source URL: https://raw.githubusercontent.com/mrdoob/three.js/7d0d76bd6cd69a95c01e2913eed5b86ff758ca91/src/cameras/PerspectiveCamera.js
- Inspected lines: 150-220; SHA256 `1cc44938885f785649a978ecb6f57c444561381ddab24de19fc59b400009b111`
- Finding: Film gauge/focal length/FOV conversion independent of eye position; port math/API contract to native code, no JS runtime.

### linebender/tiny-skia / src/blend_mode.rs

- Pin: `5d4754777746eef0828be166896eaf482c49f8f2`
- Observed and byte-verified source URL: https://raw.githubusercontent.com/linebender/tiny-skia/5d4754777746eef0828be166896eaf482c49f8f2/src/blend_mode.rs
- Inspected lines: 1-85; SHA256 `841dcc8eb96fe11fc2316fa0a4ff47c29fd837990ce8c1d8be5fcf08f6c86d19`
- Finding: Explicit Porter-Duff vs artistic blend taxonomy; BSD 3-clause root observed.

### linebender/tiny-skia / src/pipeline/highp.rs

- Pin: `5d4754777746eef0828be166896eaf482c49f8f2`
- Observed and byte-verified source URL: https://raw.githubusercontent.com/linebender/tiny-skia/5d4754777746eef0828be166896eaf482c49f8f2/src/pipeline/highp.rs
- Inspected lines: 669-695; SHA256 `81388733345dbae3d29cbf758831809847bb1aa30a13f600d0672409bce38416`
- Finding: Premultiplied source-over multiply-add; implementation reference only for own signed-HDR numeric oracle.

### mmp/pbrt-v4 / src/pbrt/cameras.cpp

- Pin: `b4ce9687e6c695f5582997c61b0c66cf064bdb4a`
- Observed and byte-verified source URL: https://raw.githubusercontent.com/mmp/pbrt-v4/b4ce9687e6c695f5582997c61b0c66cf064bdb4a/src/pbrt/cameras.cpp
- Inspected lines: 1-28, 285-340; SHA256 `ad642074b4dc4f897dd3af0a975728d59660933f475f685419c36a30e7907f3a`
- Finding: Apache camera file; thin lens aperture disk sampling and focal plane rays; preserve CPU budget and avoid adopting full path tracer.

### Razaekel/noise-rs / src/core/worley.rs

- Pin: `77de33887620c2a7e7263b1d30ef46afbfe705b6`
- Observed and byte-verified source URL: https://raw.githubusercontent.com/Razaekel/noise-rs/77de33887620c2a7e7263b1d30ef46afbfe705b6/src/core/worley.rs
- Inspected lines: 1-100; SHA256 `bc07a0f0eb245ef991c3767032dd5051f7633cf5ea3f9939060edfdb878896c8`
- Finding: Cellular metrics, hash-derived cell features and neighbor pruning. Current excerpt is partial; F1/F2 brute-force oracle needed before reuse.

### AcademySoftwareFoundation/MaterialX / libraries/stdlib/genglsl/lib/mx_noise.glsl

- Pin: `5ed154ef9ab5f45c6cf17d158830f8a637593857`
- Observed and byte-verified source URL: https://raw.githubusercontent.com/AcademySoftwareFoundation/MaterialX/5ed154ef9ab5f45c6cf17d158830f8a637593857/libraries/stdlib/genglsl/lib/mx_noise.glsl
- Inspected lines: 1-55; SHA256 `5dfc4ec072be4a43c54e96bffa6f0b6aa8f347b81f20cd4126aead98af43fb92`
- Finding: Important file-level BSD-3-Clause lineage from OSL despite Apache root; retain original Sony notice. Algorithm body not yet reviewed.

### AcademySoftwareFoundation/OpenShadingLanguage / src/liboslexec/opnoise.cpp

- Pin: `0dc86fc3e30d5417492397646abc1b98490a10b2`
- Observed and byte-verified source URL: https://raw.githubusercontent.com/AcademySoftwareFoundation/OpenShadingLanguage/0dc86fc3e30d5417492397646abc1b98490a10b2/src/liboslexec/opnoise.cpp
- Inspected lines: 1-75; SHA256 `9f8abe2bc843cc83e689ab62fdd92b500db1e97d5e7acccd8eed803c8e1ec763`
- Finding: BSD-3-Clause header and derivative/type dispatch boundary; no LLVM runtime adoption proposed.

### manimCommunity/manim / manim/animation/animation.py

- Pin: `622520d34190062f32d68af066c80f1c937fc8bf`
- Observed and byte-verified source URL: https://raw.githubusercontent.com/ManimCommunity/manim/622520d34190062f32d68af066c80f1c937fc8bf/manim/animation/animation.py
- Inspected lines: 1-70; SHA256 `be310092d2bd6283faf4b3f2e36f661024964e558176312a5a0dfe2c974eca5f`
- Finding: Fixed runtime, lag ratio and rate function contracts; native sequencing ideas only, no Python dependency.

## Limits and important decisions

- LYGIA root returns Prosperity-3.0 noncommercial/trial terms at this pin. Remotion root has custom entity/use restrictions. They are not MIT copying candidates.
- MaterialX noise source carries its own OSL/Sony BSD notice despite Apache root. gl-transitions explicitly allows per-file licenses; Directional/CrossZoom are MIT, other transition files include BSD labels.
- OpenSubdiv uses Tomorrow Open Source Technology License 1.0. GPUWeb root distinguishes W3C documents and BSD software. Neither is classified as blanket Apache/BSD copying clearance.
- Some archives use non-root license trees. A bounded root-license miss means unresolved, not unlicensed. No-license/unresolved paths remain no-copy until provenance is recovered.
- Permissive root terms do not grant rights to example assets, shaders imported from third parties or dependencies. No copying should happen on these triage labels alone.
- Named algorithms in metadata-only rows are investigation candidates, not verified claims that the exact implementation/path was read.
