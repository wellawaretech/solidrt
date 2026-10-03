---
title: Wait for audio before anchoring the first video frame
description: When play follows open closely, the first frame can reach start_audio before the reader has queued any audio; the empty sink makes it anchor the picture on itself, so the picture starts ahead of the sound until the clock correction catches up, and the forge test asserting the audio anchor fails intermittently on CI.
created: 2026-10-02
---

# Wait for audio before anchoring the first video frame

The first frame after `play` (and after a seek or buffering) is meant to wait
until the sink starts consuming and then anchor on the sound's content time,
so picture and sound start together. When the sink holds nothing at that
moment it anchors on itself instead, due at once (`start_audio`,
forge/src/video/worker.rs, the `audio.queued_us() == 0` early return). That
fallback is right for a track that has ended, but it also fires when the
audio has simply not arrived yet:

- `Player::open` returns once the reader has parsed the headers up to the
  first keyframe; the reader thread queues packets after that, one per
  loop iteration. The demuxer holds the audio read before that keyframe
  (`held_audio`, forge/src/video/webm.rs) and releases it when the gate
  opens, and `next_packet` hands the video queue out before the audio
  queue: the keyframe is pushed one iteration before the first audio
  packet, both already in memory.
- The worker feeds the sink once at the top of each loop pass, then handles
  commands, feeds the presenter and presents, all in the same pass.
- When `play` lands right after open and the reader thread is descheduled
  between those two pushes, the top-of-loop feed finds no audio, the
  presenter gets its video packet, and the first frame reaches
  `start_audio` with an empty sink.

Effect: an app that opens and plays at once can show the picture ahead of
the sound by at least the output latency (`AUDIO_OUTPUT_LATENCY_US`, 60 ms)
plus however late the first audio packet was, until the audio-clock
correction moves the anchor. It recovers, but the start is out of sync.

## How it showed

The v0.0.66 release gate failed on `gate / test-forge (linux)`, 150 of 151
passed:

```
tests::worker::the_first_frame_anchors_on_the_sound_when_a_sink_consumes
first frame due 1098832ns after play, expected the 60000000ns latency
```

Due about 1 ms after play is the self-anchored path, not a short wait. Not
reproduced locally in 105 runs (alone, the full suite with
`--test-threads=1`, all cores loaded, pinned to one loaded core): the
window between the reader's first audio packet and the worker's first pass
is too narrow on a fast machine. The mechanism above comes from reading the
code, not from a captured run.

## Done looks like

The first frame anchors on the sound whenever the stream has an audio track
that has not ended, however late its first packet arrives within
`AUDIO_START_TIMEOUT`. Self-anchoring stays for an ended track, a command
arriving during the wait, and the timeout. The test passes on CI because of
the fix, not because the timing happened to work out.

## Approach

- Pass the reader into `start_audio`. While the sink is empty and the track
  is not done, keep feeding it from `reader.next_audio()` inside the existing
  timeout wait, then wait for the position to move as now. The empty-queue
  early return becomes "track done and nothing queued".
- Update the comment above `start_audio` ("With nothing queued ...") and the
  `AUDIO_START_TIMEOUT` doc to say the wait covers the first packet too.
- A deterministic test needs the audio held back until after `play`, for
  example a served source that stalls just past the first video block (the
  `stalling` helper in forge/src/tests/worker.rs) and resumes after `play`.
  Check that the buffering rule does not take over first.
- The same function has a known first-start anchor offset on the TV (the
  "video plane start sync" line in okf/tiny.md); worth doing in the same
  pass.

## Findings

Built 2026-10-03. `start_audio` takes the reader's `next_audio` as a
closure; while the sink is empty it feeds the track inside the timeout
wait, and the early return is now "track ended and nothing queued"
(`AudioTrack::ended`). The stepped-clock, command and timeout exits are
as before.

The same race exists after a seek: the reader publishes the epoch's
resume position before it pushes the epoch's first packet, but the
worker reads it at the top of the pass and feeds the presenter later in
the same pass, so a resume landing in between presents a new-epoch
picture while `resume_pending` is still set. That frame self-anchored
too, and with `start_audio` feeding it would have fed on the seek
target's discard point and had the sink cleared by the next pass's
`audio.seek(resume_us)`. The presenter feed is now skipped while the
resume is pending (one pass at most), so nothing of an epoch is consumed
before its resume is known.

The stalling-source test suggested above cannot show the race: in both
audio fixtures the first audio packet precedes the keyframe in the file
(byte 680 against 994 in video_av.webm) and is held through the gate, so
no byte position separates the two packets the reader hands out. The
test (`the_first_frame_waits_for_an_audio_packet_still_on_its_way`,
forge/src/tests/worker.rs) drives `start_audio` directly with a reader
stand-in that answers Waiting for 50 ms and then hands the fixture's
packets out, and checks the anchor sits on the sound's content time;
with the old early return it anchors after 1.6 us. The integration test
that failed on CI stays as the end-to-end witness.

TV run (2026-10-03, armeabi-v7a Player.dev, avsync.webm on the plane
with autoplay, a scratch probe since deleted): eight fresh opens and two
seek-to-zero restarts logged no sink timeout, no late re-anchor and no
drop; six starts logged no anchor move at all and two logged one move of
-40.3 and -44.0 ms about 0.3 s in, the okf/tiny.md "video plane start
sync" pattern, unchanged by this fix (on the TV the first frame takes
hundreds of ms, so the audio is always queued when the anchor is taken).
On the TV's own speakers flash and beep land together; through the
soundbar the picture led, which is the output-latency setting the sound
path needs (an app setting to come, okf/tiny.md), not the anchor.

The tiny.md "video plane start sync" line is closed by measurement
(2026-10-03, the info line `start_audio` now logs at every start: the
sink position jump at the anchor and the content time anchored). On the
TV the jump is 21.3, 42.7 or 64.0 ms across seven starts: whole multiples
of 1024 frames at 48 kHz, so the device buffer is 21.3 ms and the device
primes with a burst of one to three buffers before the 2 ms poll sees
the position move. Starts therefore differ by up to 43 ms in where the
picture sits against the sound, which the sync corrects only past 40 ms;
the rest is a start-to-start jitter of about 20 ms either side of the
mean the 60 ms latency constant was tuned around, under what the eye and
ear tell apart on avsync.webm. "Anchor half the jump earlier" would not
remove it (the burst size varies, not a fixed buffer's phase). A
threshold-free first correction once the sync has its samples would make
every start land on the steady-state mapping; rejected: it can only act
by shifting the picture's schedule, so every start, seek and buffering
resume would get the one-vsync hitch that today only the starts past 40
ms get (two of eight on the TV), and the 21 ms position quantization
would still leave about 10 ms of start jitter. A test that needs an
exact first-frame time uses the fake sink, which has no burst.

Found on the way: a /reload of an app with a plane player fails every
other time on the TV ("Video plane not created: one already exists"):
the new engine opens its plane about 150 ms after it starts and the old
engine's plane is removed 250 to 700 ms later. Pre-existing, untouched.
