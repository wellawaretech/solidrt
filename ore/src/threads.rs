//! Thread placement: the priority tier and the cores the OS gives the
//! calling thread. ore names what the OS gives, not who asks: a thread that
//! owns a present deadline raises itself to `Tier::Deadline`, a
//! frame-building thread to `Tier::Interactive`, and a worker whose bursts
//! are lag the user sees (a sort worker) takes `place_burst_worker`. Every
//! call acts on the calling thread only and is best-effort: what the OS
//! refuses is logged at debug and the thread runs as it was.

/// The priority tiers a thread can raise itself to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
  /// A thread that owns a present deadline (a raster thread).
  Deadline,
  /// A thread whose work feeds the deadline (a frame-building thread), one
  /// step below `Deadline`.
  Interactive,
}

/// The nice value of a `Tier::Deadline` thread on Android and Linux: what
/// Android's HWUI render thread runs at.
#[cfg(any(target_os = "android", target_os = "linux"))]
const DEADLINE_NICE: i32 = -8;

/// The nice value of a `Tier::Interactive` thread on Android and Linux:
/// what Android's HWUI UI tier runs at, one step below `DEADLINE_NICE`.
#[cfg(any(target_os = "android", target_os = "linux"))]
const INTERACTIVE_NICE: i32 = -4;

/// Raise the calling thread to the platform's frame/display priority so
/// background processes cannot preempt it mid-frame; `Tier::Deadline` for
/// the thread that owns the present deadline (a raster thread),
/// `Tier::Interactive` for the frame-building tier (a UI thread). This is
/// what OS render threads themselves run at (Android HWUI -4/-8, macOS
/// user-interactive QoS, Windows above-normal). Measured on the 2017
/// MediaTek TV: system services (cast_shell V8 GC) preempted
/// default-priority frame threads ~once a second, each a visible dropped
/// frame; the boost cut drops ~10x
/// (okf/backlog/android-surface-swap-latency.md). Best-effort everywhere:
/// Android sanctions it, desktop Linux typically denies negative nice to
/// unprivileged processes (EPERM, logged at debug and harmless), macOS and
/// Windows always accept their calls.
pub fn raise(tier: Tier) {
  #[cfg(any(target_os = "android", target_os = "linux"))]
  thread_nice(match tier {
    Tier::Deadline => DEADLINE_NICE,
    Tier::Interactive => INTERACTIVE_NICE,
  });
  #[cfg(target_os = "macos")]
  {
    let class = match tier {
      Tier::Deadline => libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE,
      Tier::Interactive => libc::qos_class_t::QOS_CLASS_USER_INITIATED,
    };
    // SAFETY: sets the QoS class of the calling thread only; the class is
    // a valid constant and 0 is the default relative priority.
    if unsafe { libc::pthread_set_qos_class_self_np(class, 0) } != 0 {
      log::debug!("[ore] pthread_set_qos_class_self_np failed");
    }
  }
  #[cfg(target_os = "windows")]
  {
    use windows_sys::Win32::System::Threading::{
      GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_ABOVE_NORMAL, THREAD_PRIORITY_HIGHEST,
    };
    let priority = match tier {
      Tier::Deadline => THREAD_PRIORITY_HIGHEST,
      Tier::Interactive => THREAD_PRIORITY_ABOVE_NORMAL,
    };
    // SAFETY: GetCurrentThread returns the calling thread's pseudo-handle,
    // always valid and never closed; the priority is a valid constant.
    if unsafe { SetThreadPriority(GetCurrentThread(), priority) } == 0 {
      log::debug!("[ore] SetThreadPriority({priority}) failed");
    }
  }
  #[cfg(not(any(target_os = "android", target_os = "linux", target_os = "macos", target_os = "windows")))]
  let _ = tier;
}

/// The nice value of a burst worker on Android and Linux: between the
/// `Tier::Interactive` -4 and the default 0, so the frame tier keeps its
/// precedence over the worker and the worker has its own over the
/// process's background threads. A thread inherits its spawner's nice, so
/// for a worker a frame-building thread starts this is a step down, not a
/// boost. SDL's own `SDL_THREAD_PRIORITY_HIGH` is -10 there, above the
/// `Tier::Deadline` thread, which is why the worker does not use it.
const BURST_WORKER_NICE: i32 = -2;

/// The utilization floor a burst worker asks the scheduler for, on the
/// kernel's 0..1024 capacity scale; 1024 is the largest core at its top
/// clock. A worker that runs tens of milliseconds a few times a second has
/// a low average utilization, and an energy-aware scheduler answers that
/// average with a little core at a low clock; the floor replaces the
/// average with what the burst needs.
const BURST_WORKER_UTIL_MIN: u32 = 1024;

/// The placement `place_burst_worker` gave a thread, kept so the thread
/// can re-assert it: the nice value and the utilization floor stick, but a
/// hard affinity does not survive a cpuset move (Android rewrites a
/// thread's allowed cores when the app changes between its top-app,
/// foreground and background groups, and the Pixel 7 returns from the
/// background with the pin gone), so a pinned worker calls `reassert`
/// before each job. While the app sits in a group without the pinned cores
/// the re-pin fails and the thread runs where the group allows; the next
/// job in the top-app group restores it.
pub struct Placement {
  /// The cores the thread was pinned to; empty when it was not.
  cores: Vec<usize>,
}

impl Placement {
  /// Re-apply the pin, when there is one.
  pub fn reassert(&self) {
    if !self.cores.is_empty() {
      pin_thread_to(&self.cores);
    }
  }
}

/// Give the calling thread the placement of a burst worker: a thread whose
/// duration is lag the user sees but never a frame's deadline (a sort
/// worker: the splat order lags the camera by one sort). Below the frame
/// tiers (`raise`) and above the default, on a performance core: on Linux
/// a nice value plus a utilization floor through `sched_setattr`, and
/// where the floor is refused a hard affinity to the cores above the
/// smallest capacity. Android refuses the floor by policy (its kernel
/// makes any clamp change depend on CAP_SYS_NICE, which no app has), so
/// there the pin is the placement; measured on the Pixel 7 it is what
/// moves the sort, nice alone moves nothing. macOS places by QoS and a
/// thread at the default QoS already runs on the performance cores;
/// Windows has no placement hint; both are no-ops. Best-effort everywhere,
/// like `raise`: desktop Linux denies the nice, and the floor only costs
/// clock on a uniform CPU.
pub fn place_burst_worker() -> Placement {
  thread_nice(BURST_WORKER_NICE);
  if thread_util_min(BURST_WORKER_UTIL_MIN) {
    return Placement { cores: Vec::new() };
  }
  let cores = performance_cores();
  if !cores.is_empty() && pin_thread_to(&cores) {
    log::debug!("[ore] burst worker pinned to cpus {cores:?}");
    Placement { cores }
  } else {
    Placement { cores: Vec::new() }
  }
}

/// Set the calling thread's nice value (Android and Linux; a no-op
/// elsewhere). Whether the kernel accepted it: desktop Linux typically
/// denies a negative value to an unprivileged process (EPERM, logged at
/// debug and harmless).
fn thread_nice(nice: i32) -> bool {
  #[cfg(any(target_os = "android", target_os = "linux"))]
  {
    // SAFETY: gettid has no preconditions and names the calling thread.
    let tid = unsafe { libc::gettid() };
    // SAFETY: setpriority on PRIO_PROCESS with a thread id changes that
    // one thread's nice value; it reads and writes no memory of ours.
    if unsafe { libc::setpriority(libc::PRIO_PROCESS, tid as libc::id_t, nice) } != 0 {
      log::debug!("[ore] setpriority({nice}) not permitted for thread {tid}");
      return false;
    }
    true
  }
  #[cfg(not(any(target_os = "android", target_os = "linux")))]
  {
    let _ = nice;
    false
  }
}

/// Ask the scheduler for a utilization floor for the calling thread
/// (Linux 5.3 and later with utilization clamping): `util` on the 0..1024
/// capacity scale, the policy and the nice value left as they are. Whether
/// the kernel took it: a kernel without the clamp refuses (ENOSYS,
/// EOPNOTSUPP or EINVAL) and Android refuses an unprivileged caller
/// (EPERM), both logged at debug.
fn thread_util_min(util: u32) -> bool {
  #[cfg(any(target_os = "android", target_os = "linux"))]
  {
    // `struct sched_attr` with the clamp fields (SCHED_ATTR_SIZE_VER1); the
    // libc crate declares the older, shorter version and no wrapper.
    #[repr(C)]
    struct SchedAttr {
      size: u32,
      sched_policy: u32,
      sched_flags: u64,
      sched_nice: i32,
      sched_priority: u32,
      sched_runtime: u64,
      sched_deadline: u64,
      sched_period: u64,
      sched_util_min: u32,
      sched_util_max: u32,
    }
    const SCHED_ATTR_SIZE_VER1: u32 = 56;
    const SCHED_FLAG_KEEP_POLICY: u64 = 0x08;
    const SCHED_FLAG_KEEP_PARAMS: u64 = 0x10;
    const SCHED_FLAG_UTIL_CLAMP_MIN: u64 = 0x20;
    let attr = SchedAttr {
      size: SCHED_ATTR_SIZE_VER1,
      sched_policy: 0,
      sched_flags: SCHED_FLAG_KEEP_POLICY | SCHED_FLAG_KEEP_PARAMS | SCHED_FLAG_UTIL_CLAMP_MIN,
      sched_nice: 0,
      sched_priority: 0,
      sched_runtime: 0,
      sched_deadline: 0,
      sched_period: 0,
      sched_util_min: util,
      sched_util_max: 0,
    };
    let calling_thread: libc::pid_t = 0;
    let flags: libc::c_uint = 0;
    // SAFETY: the kernel reads `attr.size` bytes from a live, fully
    // initialized struct laid out as SCHED_ATTR_SIZE_VER1, and changes only
    // the calling thread (pid 0).
    if unsafe { libc::syscall(libc::SYS_sched_setattr, calling_thread, &attr as *const SchedAttr, flags) } != 0 {
      log::debug!("[ore] sched_setattr(util_min={util}) refused: {}", std::io::Error::last_os_error());
      return false;
    }
    true
  }
  #[cfg(not(any(target_os = "android", target_os = "linux")))]
  {
    let _ = util;
    false
  }
}

/// The cores above the smallest capacity, from the kernel's per-cpu
/// `cpu_capacity` (Android and Linux): on the Pixel 7 the two mid and two
/// big cores beside four little ones. Empty on a uniform CPU, where the
/// files say one capacity, or without the files.
fn performance_cores() -> Vec<usize> {
  #[cfg(any(target_os = "android", target_os = "linux"))]
  {
    let max_cpus = std::mem::size_of::<libc::cpu_set_t>() * 8;
    let mut capacities: Vec<(usize, u32)> = Vec::new();
    for cpu in 0..max_cpus {
      let Ok(text) = std::fs::read_to_string(format!("/sys/devices/system/cpu/cpu{cpu}/cpu_capacity")) else { break };
      let Ok(capacity) = text.trim().parse::<u32>() else { break };
      capacities.push((cpu, capacity));
    }
    let Some(least) = capacities.iter().map(|&(_, capacity)| capacity).min() else { return Vec::new() };
    capacities.iter().filter(|&&(_, capacity)| capacity > least).map(|&(cpu, _)| cpu).collect()
  }
  #[cfg(not(any(target_os = "android", target_os = "linux")))]
  {
    Vec::new()
  }
}

/// Pin the calling thread to `cores` (Android and Linux; false elsewhere).
/// Whether the kernel took it: it refuses a set with no core the thread's
/// cpuset allows (EINVAL, logged at debug).
fn pin_thread_to(cores: &[usize]) -> bool {
  #[cfg(any(target_os = "android", target_os = "linux"))]
  {
    // SAFETY: cpu_set_t is a plain bit array; all zeroes is the empty set.
    let mut set: libc::cpu_set_t = unsafe { std::mem::zeroed() };
    // SAFETY: CPU_ZERO and CPU_SET write only bits of the live `set`; each
    // core comes from `performance_cores`, below the set's capacity.
    unsafe {
      libc::CPU_ZERO(&mut set);
      for &cpu in cores {
        libc::CPU_SET(cpu, &mut set);
      }
    }
    // SAFETY: the kernel reads size_of::<cpu_set_t>() bytes from the live
    // `set` and changes only the calling thread (pid 0).
    if unsafe { libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set) } != 0 {
      log::debug!("[ore] sched_setaffinity({cores:?}) refused: {}", std::io::Error::last_os_error());
      return false;
    }
    true
  }
  #[cfg(not(any(target_os = "android", target_os = "linux")))]
  {
    let _ = cores;
    false
  }
}
