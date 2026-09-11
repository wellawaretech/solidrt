// The ease every camera control's self-driven motion runs on: a glide to
// a commanded pose (glideTo, fit) and the damping of an unbracketed input
// delta (a wheel notch) both close the gap to a goal pose by the same
// exponential fraction per frame, so the two controls here and the 2d
// camera read the same. Frame-rate independent: the step is
// 1 - e^(-rate * dt), never a fixed fraction.

// E-foldings per second toward the goal: high enough that a wheel notch
// reads as one push, low enough to look smooth (the 2d camera's rate).
export const GLIDE_EASE = 9
// A motion lands - snaps to its goal and stops - once this fraction of
// its initial gap remains: under a pixel on any pose component a glide
// moves through at ordinary distances.
export const GLIDE_EPSILON = 0.001

/** The fraction of the remaining gap an ease at `rate` closes over dt. */
export let easeStep = (rate: number, dt: number): number => 1 - Math.exp(-rate * dt)
