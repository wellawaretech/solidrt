# @solidrt/3d - positional sound

Positional sound attached to scene nodes. Read ../AGENTS.md first: it
carries the model this file builds on.

## Positional sound

A sound that lives in the world - an engine per kart, a waterfall, a
shot passing by - is `attachSound(node, sound, { distanceModel?,
refDistance?, maxDistance?, rolloff?, doppler? })` over a core Sound
(`createSound`, `createPcmSound`, `createSoundStream` from
@solidrt/core/audio): Three's PositionalAudio under a mesh, Unity's
AudioSource on a GameObject, Godot's AudioStreamPlayer3D, in the Web
Audio PannerNode vocabulary. Nothing to call per frame: the spatial
core writes each voice's pan, distance gain, doppler rate and occlusion
low-pass after every flush that moved the node, the camera or an
occluder, and nothing while the scene stands still (the hand-rolled
pattern, reading positions and writing setPan/setGain/setRate per voice
per frame, keeps working for a sound you do not attach). The sound's
own `setGain` and `setRate` keep meaning "how loud / how fast is this
thing": the placement composes under them; `setPan` is the core's while
attached. The laws: pan is the LATERAL angle to the emitter (the
interaural cue; a source overhead or dead ahead is center), blended to
center inside `refDistance` so a source you stand inside surrounds you;
gain is Web Audio's inverse (default), linear or exponential law over
the distance clamped to [refDistance, maxDistance] (defaults 1 and
10000, rolloff 1); the doppler ratio comes from velocities the core
derives from the positions (smoothed over 80 ms; a jump faster than
sound is a teleport, not a chirp; the ratio is clamped to an octave
either way), times `doppler` (1 physical, 0 off). Use MONO clips: a
stereo clip is balanced, not placed. The listener is the scene's
(`scene.listener()`, made on first use): it hears from the camera, and
`scene.setListener(node)` makes a node hear instead (a third-person
rig's body, a first-person head; its local -z forward, +x right; null
returns to the camera); `speedOfSound` on createScene (default 343,
metres) is what the doppler is measured against and the one knob that
fixes the world's unit. Occluders with audio (Culling in large-worlds.md) crossing the
straight path dim and dull the sound (the gain and a low-pass, over 150
ms so a wall never clicks); a hidden occluder blocks neither sight nor
sound. The binding follows the node's scene membership like spring
bones: it places the voices when the node enters a scene and, when the
node LEAVES, stops them and releases the placement (a looping engine
must not outlive its kart); a sound played while its node is outside
any scene plays unplaced until the node enters. `attachSound` returns
the detach. The cost model is the spatial core's: `/stats` carries
`audioSpatialWrites`, which climbs by the moving emitters per frame and
by nothing in a still scene. Not here, additive when wanted: cones, HRTF,
reverb zones, air absorption by distance, virtual voices past
maxDistance, an app-supplied velocity. `okf/plans/spatial-audio-emitters.md`
holds the design.
