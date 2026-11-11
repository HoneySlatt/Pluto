//! A series of samples reduced to the figures of one run.
//!
//! Over the run, from its first sample to its last:
//! - wake-ups a second: each thread's switches in its last sample less those in its first, or less
//!   nothing for a thread that first appears after the series' first sample, summed, divided by
//!   the seconds between the first and last samples;
//! - processor: the cgroup's `usage_usec` difference, in seconds and as a share of one core; per
//!   share, the processes' `cpu_ticks` the same way as the switches, at 100 ticks a second;
//! - unattributed processor: the cgroup's time less the two shares', never below 0. The cgroup
//!   counts every process that ever ran in it; the shares count only the processes seen in the
//!   samples. What is missing is the time of processes born and gone between two samples, and a
//!   process's last interval before it went. A figure far from 0 means the interval was too long
//!   for the subject;
//! - memory: at the last sample that read memory, each share's sum, and the peak of each share's
//!   headline figure over every sample that read memory. A DRM client held by several processes
//!   or descriptors counts once, for the share of the lowest pid that holds it;
//! - processes without memory: those of that last sample whose memory the kernel refused, so that
//!   a figure that leaves them out is known to.
//!
//! A counter is keyed by the id and the start time together: the kernel reuses ids, and a new
//! process with an old pid is another process.
//!
//! A process is the agents' if its executable's name or its command's name (`comm`) is one of the
//! agents' names, or if one of its ancestors in the sample is. A name is compared without a
//! leading `.` and a trailing `-wrapped` (Nix's wrappers). An agent run by an interpreter or a
//! multicall binary has the interpreter's or the binary's executable, so its `comm` tells. The
//! kernel cuts `comm` to 15 characters: one of exactly that length also matches if it is the
//! start of the agent's name or of `.{agent}-wrapped`.

use crate::proc::DrmClient;
use crate::sample::{ProcessSample, Sample};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// The kernel's USER_HZ: the unit of the `cpu_ticks` of a process.
const TICKS_PER_SECOND: f64 = 100.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct MemoryFigures {
    /// private + swap + gpu: the quality bar's figure.
    pub headline: u64,
    pub private: u64,
    pub swap: u64,
    /// VRAM plus GTT of the share's DRM clients.
    pub gpu: u64,
    pub pss: u64,
    pub rss: u64,
    /// How long the GPU's graphics engine has run for the share's DRM clients since they began,
    /// in nanoseconds: above 0, the share renders on the GPU, not in software.
    pub gpu_engine_ns: u64,
}

/// A thread's wake-ups, with those of every other thread of the same name in processes of the
/// same name (Chromium's processes repeat their threads' names).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BusyThread {
    pub process: String,
    pub thread: String,
    pub agent: bool,
    pub wakeups_per_second: f64,
}

/// How many of the busiest threads the figures name.
pub const BUSIEST_THREADS: usize = 12;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Share {
    pub memory: MemoryFigures,
    pub memory_peak: u64,
    pub wakeups_per_second: f64,
    pub cpu_seconds: f64,
    /// Processes in the last sample.
    pub processes: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Figures {
    pub seconds: f64,
    pub samples: usize,
    /// The whole cgroup.
    pub cpu_seconds: f64,
    /// `cpu_seconds / seconds`, so 1.0 is one core busy throughout.
    pub cpu_cores: f64,
    pub wakeups_per_second: f64,
    /// The program itself: every process that is not an agent's.
    pub app: Share,
    /// The agents' processes and their descendants.
    pub agents: Share,
    pub threads_seen: usize,
    /// The threads that woke most, by process and thread name, busiest first: what the wake-ups
    /// are, as `pidstat -w -t` showed them for M1.2.
    pub busiest_threads: Vec<BusyThread>,
    /// Processor time of the cgroup that no process seen accounts for: see the module's text.
    pub unattributed_cpu_seconds: f64,
    /// Processes of the last memory sample whose memory could not be read.
    pub processes_without_memory: usize,
    /// When the cgroup could no longer be read, in milliseconds from the first sample: the
    /// subject ended before the run did. Set by the caller from the series; `None` here.
    pub ended_early_at_ms: Option<u64>,
}

/// The figures of a series; all zero for no sample. For one sample, every figure that needs a
/// span of time (`seconds`, the rates, the processor time) is zero, and the rest is counted.
pub fn summarize(samples: &[Sample], agents: &[String]) -> Figures {
    let (Some(first), Some(last)) = (samples.first(), samples.last()) else {
        return Figures::default();
    };
    let elapsed_ms = last.at_ms.saturating_sub(first.at_ms);

    let mut switches: HashMap<(u32, u32, u64), Counter> = HashMap::new();
    let mut thread_names: HashMap<(u32, u32, u64), (String, String)> = HashMap::new();
    let mut ticks: HashMap<(u32, u64), Counter> = HashMap::new();
    let mut last_memory_sample = None;
    let mut memory = ByShare::<MemoryFigures>::default();
    let mut memory_peak = ByShare::<u64>::default();
    let mut processes = ByShare::<usize>::default();
    for (index, sample) in samples.iter().enumerate() {
        let agent_pids = agent_pids(&sample.processes, agents);
        let starts_series = index == 0;
        for process in &sample.processes {
            let agent = agent_pids.contains(&process.pid);
            observe(
                &mut ticks,
                (process.pid, process.start_time),
                process.cpu_ticks,
                agent,
                starts_series,
            );
            if index + 1 == samples.len() {
                let count = processes.of(agent);
                *count = count.saturating_add(1);
            }
        }
        let process_names: HashMap<u32, &str> = sample
            .processes
            .iter()
            .map(|process| (process.pid, process.name.as_str()))
            .collect();
        for thread in &sample.threads {
            let agent = agent_pids.contains(&thread.pid);
            thread_names
                .entry((thread.pid, thread.tid, thread.start_time))
                .or_insert_with(|| {
                    let process = process_names.get(&thread.pid).copied().unwrap_or_default();
                    (process.to_string(), thread.name.clone())
                });
            observe(
                &mut switches,
                (thread.pid, thread.tid, thread.start_time),
                thread.switches,
                agent,
                starts_series,
            );
        }
        if sample.read_memory {
            let sample_memory = memory_of(sample, &agent_pids);
            memory_peak.app = memory_peak.app.max(sample_memory.app.headline);
            memory_peak.agents = memory_peak.agents.max(sample_memory.agents.headline);
            memory = sample_memory;
            last_memory_sample = Some(sample);
        }
    }
    let switches_by_share = sum_deltas(switches.values());
    let ticks_by_share = sum_deltas(ticks.values());

    let cpu_usec = last.cpu.usage_usec.saturating_sub(first.cpu.usage_usec);
    let app_cpu_seconds = ticks_by_share.app as f64 / TICKS_PER_SECOND;
    let agents_cpu_seconds = ticks_by_share.agents as f64 / TICKS_PER_SECOND;
    let cpu_seconds = cpu_usec as f64 / 1e6;
    Figures {
        seconds: elapsed_ms as f64 / 1000.0,
        samples: samples.len(),
        cpu_seconds,
        cpu_cores: if elapsed_ms == 0 {
            0.0
        } else {
            cpu_usec as f64 / (elapsed_ms as f64 * 1000.0)
        },
        wakeups_per_second: per_second(
            switches_by_share
                .app
                .saturating_add(switches_by_share.agents),
            elapsed_ms,
        ),
        app: Share {
            memory: memory.app,
            memory_peak: memory_peak.app,
            wakeups_per_second: per_second(switches_by_share.app, elapsed_ms),
            cpu_seconds: app_cpu_seconds,
            processes: processes.app,
        },
        agents: Share {
            memory: memory.agents,
            memory_peak: memory_peak.agents,
            wakeups_per_second: per_second(switches_by_share.agents, elapsed_ms),
            cpu_seconds: agents_cpu_seconds,
            processes: processes.agents,
        },
        threads_seen: switches.len(),
        busiest_threads: busiest_threads(&switches, &thread_names, elapsed_ms),
        unattributed_cpu_seconds: (cpu_seconds - app_cpu_seconds - agents_cpu_seconds).max(0.0),
        processes_without_memory: last_memory_sample.map_or(0, |sample| {
            sample
                .processes
                .iter()
                .filter(|process| process.memory.is_none())
                .count()
        }),
        ended_early_at_ms: None,
    }
}

/// The [`BUSIEST_THREADS`] threads that woke most, summed by process name, thread name and share.
fn busiest_threads(
    switches: &HashMap<(u32, u32, u64), Counter>,
    names: &HashMap<(u32, u32, u64), (String, String)>,
    elapsed_ms: u64,
) -> Vec<BusyThread> {
    let mut by_name: HashMap<(&str, &str, bool), u64> = HashMap::new();
    for (key, counter) in switches {
        let (process, thread) = names.get(key).map_or(("", ""), |(process, thread)| {
            (process.as_str(), thread.as_str())
        });
        let sum = by_name.entry((process, thread, counter.agent)).or_default();
        *sum = sum.saturating_add(counter.delta());
    }
    let mut busiest: Vec<BusyThread> = by_name
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .map(|((process, thread, agent), count)| BusyThread {
            process: process.to_string(),
            thread: thread.to_string(),
            agent,
            wakeups_per_second: per_second(count, elapsed_ms),
        })
        .collect();
    // Busiest first; equal rates by name, so that the list is the same from run to run.
    busiest.sort_by(|a, b| {
        b.wakeups_per_second
            .total_cmp(&a.wakeups_per_second)
            .then_with(|| (&a.process, &a.thread).cmp(&(&b.process, &b.thread)))
    });
    busiest.truncate(BUSIEST_THREADS);
    busiest
}

/// A process name as compared with the agents' names: without a leading `.` and a trailing
/// `-wrapped`.
pub fn plain_name(name: &str) -> &str {
    let name = name.strip_prefix('.').unwrap_or(name);
    name.strip_suffix("-wrapped").unwrap_or(name)
}

/// One value for the program and one for the agents.
#[derive(Default)]
struct ByShare<T> {
    app: T,
    agents: T,
}

impl<T> ByShare<T> {
    fn of(&mut self, agent: bool) -> &mut T {
        if agent {
            &mut self.agents
        } else {
            &mut self.app
        }
    }
}

/// A counter of one thread or process, seen over several samples.
struct Counter {
    first: u64,
    last: u64,
    /// Seen in the series' first sample. A counter that appears later began at 0.
    in_first_sample: bool,
    /// Whether it belonged to the agents in the last sample that held it.
    agent: bool,
}

impl Counter {
    fn delta(&self) -> u64 {
        self.last
            .saturating_sub(if self.in_first_sample { self.first } else { 0 })
    }
}

fn observe<K: std::hash::Hash + Eq>(
    counters: &mut HashMap<K, Counter>,
    key: K,
    value: u64,
    agent: bool,
    in_first_sample: bool,
) {
    let counter = counters.entry(key).or_insert(Counter {
        first: value,
        last: value,
        in_first_sample,
        agent,
    });
    counter.last = value;
    counter.agent = agent;
}

fn sum_deltas<'a>(counters: impl Iterator<Item = &'a Counter>) -> ByShare<u64> {
    let mut sums = ByShare::<u64>::default();
    for counter in counters {
        let sum = sums.of(counter.agent);
        *sum = sum.saturating_add(counter.delta());
    }
    sums
}

fn per_second(count: u64, elapsed_ms: u64) -> f64 {
    if elapsed_ms == 0 {
        0.0
    } else {
        count as f64 * 1000.0 / elapsed_ms as f64
    }
}

/// The longest `comm` the kernel keeps.
const COMM_LENGTH: usize = 15;

/// Whether a process is the agents': its executable's name or its `comm` is one of theirs, or an
/// ancestor's is.
fn agent_pids(processes: &[ProcessSample], agents: &[String]) -> HashSet<u32> {
    let by_pid: HashMap<u32, &ProcessSample> = processes
        .iter()
        .map(|process| (process.pid, process))
        .collect();
    let is_agent = |process: &ProcessSample| {
        agents
            .iter()
            .filter(|agent| !agent.is_empty())
            .any(|agent| is_named(process, agent))
    };
    processes
        .iter()
        .filter(|process| {
            let mut ancestor = *process;
            // A reused pid could make the parents loop: never climb more steps than there are
            // processes.
            for _ in 0..processes.len() {
                if is_agent(ancestor) {
                    return true;
                }
                match ancestor.parent.and_then(|parent| by_pid.get(&parent)) {
                    Some(parent) => ancestor = parent,
                    None => return false,
                }
            }
            false
        })
        .map(|process| process.pid)
        .collect()
}

/// Whether a process's names say it is `agent`: the plain name of the display name, the
/// executable or the `comm` is the agent's, or the `comm` is the kernel's cut of it.
fn is_named(process: &ProcessSample, agent: &str) -> bool {
    [&process.name, &process.exe_name, &process.comm]
        .into_iter()
        .any(|name| plain_name(name) == agent)
        || is_cut_name(&process.comm, agent)
}

/// A `comm` of exactly the kernel's length may be the start of a longer name.
fn is_cut_name(comm: &str, agent: &str) -> bool {
    comm.len() == COMM_LENGTH
        && (agent.starts_with(comm) || format!(".{agent}-wrapped").starts_with(comm))
}

/// The memory of each share in a sample that read memory.
fn memory_of(sample: &Sample, agent_pids: &HashSet<u32>) -> ByShare<MemoryFigures> {
    let mut figures = ByShare::<MemoryFigures>::default();
    for process in &sample.processes {
        if let Some(memory) = process.memory {
            let share = figures.of(agent_pids.contains(&process.pid));
            share.private = share.private.saturating_add(memory.private);
            share.swap = share.swap.saturating_add(memory.swap);
            share.pss = share.pss.saturating_add(memory.pss);
            share.rss = share.rss.saturating_add(memory.rss);
        }
    }
    for (client, holder) in clients_by_lowest_pid(&sample.processes) {
        let share = figures.of(agent_pids.contains(&holder));
        share.gpu = share
            .gpu
            .saturating_add(client.vram)
            .saturating_add(client.gtt);
        share.gpu_engine_ns = share.gpu_engine_ns.saturating_add(client.gfx_ns);
    }
    for share in [&mut figures.app, &mut figures.agents] {
        share.headline = share
            .private
            .saturating_add(share.swap)
            .saturating_add(share.gpu);
    }
    figures
}

/// Each DRM client of the sample once, with the lowest pid that holds it: the same on every
/// sample, whatever the order of the processes.
fn clients_by_lowest_pid(processes: &[ProcessSample]) -> Vec<(&DrmClient, u32)> {
    let mut clients: HashMap<(&str, u64), (&DrmClient, u32)> = HashMap::new();
    for process in processes {
        for client in &process.drm {
            let holder = clients
                .entry((client.device.as_str(), client.client_id))
                .or_insert((client, process.pid));
            holder.1 = holder.1.min(process.pid);
        }
    }
    clients.into_values().collect()
}
