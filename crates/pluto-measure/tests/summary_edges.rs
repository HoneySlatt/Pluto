//! Edge cases of the summary: which processes are the agents', whether a sample read memory, the
//! figures that say what is missing, and counters of reused ids.

use pluto_measure::proc::{CpuStat, DrmClient, Memory};
use pluto_measure::sample::{ProcessSample, Sample, ThreadSample};
use pluto_measure::summary::summarize;

fn process(pid: u32, parent: u32, exe_name: &str, comm: &str) -> ProcessSample {
    ProcessSample {
        pid,
        parent: Some(parent),
        name: if exe_name.is_empty() { comm } else { exe_name }.into(),
        exe_name: exe_name.into(),
        comm: comm.into(),
        start_time: 1,
        cpu_ticks: 0,
        memory: Some(Memory {
            rss: 10,
            pss: 10,
            private: 10,
            swap: 0,
        }),
        drm: Vec::new(),
    }
}

fn sample(at_ms: u64, read_memory: bool, processes: Vec<ProcessSample>) -> Sample {
    Sample {
        at_ms,
        read_memory,
        processes,
        threads: Vec::new(),
        cpu: CpuStat::default(),
    }
}

fn agent_count(exe_name: &str, comm: &str, agent: &str) -> usize {
    let figures = summarize(
        &[sample(0, true, vec![process(10, 1, exe_name, comm)])],
        &[agent.to_string()],
    );
    figures.agents.processes
}

#[test]
fn an_interpreter_runs_the_agent() {
    assert_eq!(agent_count("python3.14", ".vibe-wrapped", "vibe"), 1);
}

#[test]
fn a_multicall_binary_is_named_by_its_comm() {
    assert_eq!(agent_count("coreutils", "sleep", "sleep"), 1);
}

#[test]
fn the_kernels_cut_comm_still_matches() {
    assert_eq!(".opencode-wrapp".len(), 15);
    assert_eq!(agent_count("node", ".opencode-wrapp", "opencode"), 1);
    // A prefix of the agent's own name, when the name is longer than the cut.
    assert_eq!(
        agent_count("node", "a-very-long-nam", "a-very-long-name-here"),
        1
    );
}

#[test]
fn a_cut_comm_is_not_a_loose_prefix() {
    // Shorter than the kernel's cut: the whole name was there, so a prefix is another program.
    assert_eq!(agent_count("node", ".opencode-wrap", "opencode"), 0);
    // Exactly the cut, but not a prefix of anything the agent can be called.
    assert_eq!(agent_count("node", "xopencode-wrapp", "opencode"), 0);
    assert_eq!(agent_count("node", "", ""), 0, "an empty name is nobody's");
}

#[test]
fn a_sample_that_read_memory_with_no_process_ends_at_zero() {
    let figures = summarize(
        &[
            sample(0, true, vec![process(10, 1, "zed", "zed")]),
            sample(1000, true, Vec::new()),
        ],
        &[],
    );
    assert_eq!(figures.app.memory.headline, 0);
    assert_eq!(figures.app.memory_peak, 10, "the peak stays");
}

#[test]
fn a_sample_that_did_not_read_memory_is_not_a_memory_sample() {
    let mut without = process(10, 1, "zed", "zed");
    without.memory = None;
    let figures = summarize(
        &[
            sample(0, true, vec![process(10, 1, "zed", "zed")]),
            sample(1000, false, vec![without]),
        ],
        &[],
    );
    assert_eq!(figures.app.memory.headline, 10);
    assert_eq!(figures.processes_without_memory, 0);
}

#[test]
fn processes_whose_memory_could_not_be_read_are_counted() {
    let mut refused = process(11, 1, "other-users", "other-users");
    refused.memory = None;
    let figures = summarize(
        &[
            sample(0, true, vec![process(10, 1, "zed", "zed")]),
            sample(
                1000,
                true,
                vec![process(10, 1, "zed", "zed"), refused.clone(), {
                    let mut second = refused;
                    second.pid = 12;
                    second
                }],
            ),
        ],
        &[],
    );
    assert_eq!(figures.processes_without_memory, 2);
    assert_eq!(figures.app.processes, 3);
}

#[test]
fn processor_time_nobody_owns_is_reported() {
    let mut first = sample(0, true, vec![process(10, 1, "zed", "zed")]);
    first.cpu.usage_usec = 1_000_000;
    let mut last = sample(10_000, true, vec![process(10, 1, "zed", "zed")]);
    last.processes[0].cpu_ticks = 300;
    last.cpu.usage_usec = 6_000_000;
    let figures = summarize(&[first, last], &[]);
    assert_eq!(figures.cpu_seconds, 5.0);
    assert_eq!(figures.app.cpu_seconds, 3.0);
    assert_eq!(figures.unattributed_cpu_seconds, 2.0);
}

#[test]
fn unattributed_processor_time_is_never_negative() {
    let first = sample(0, true, vec![process(10, 1, "zed", "zed")]);
    let mut last = sample(1000, true, vec![process(10, 1, "zed", "zed")]);
    last.processes[0].cpu_ticks = 500;
    last.cpu.usage_usec = 1_000_000;
    let figures = summarize(&[first, last], &[]);
    assert_eq!(figures.unattributed_cpu_seconds, 0.0);
}

#[test]
fn a_reused_pid_never_merges_two_processes() {
    let mut old = process(10, 1, "zed", "zed");
    old.cpu_ticks = 1000;
    let mut new = process(10, 1, "zed", "zed");
    new.start_time = 2;
    new.cpu_ticks = 10;
    let figures = summarize(
        &[sample(0, true, vec![old]), sample(1000, true, vec![new])],
        &[],
    );
    assert_eq!(figures.app.cpu_seconds, 0.1, "the new one, from 0");
}

#[test]
fn a_reused_thread_id_never_merges_two_threads() {
    let thread = |start_time, switches| ThreadSample {
        pid: 10,
        tid: 11,
        start_time,
        switches,
        name: String::new(),
    };
    let mut first = sample(0, true, vec![process(10, 1, "zed", "zed")]);
    first.threads = vec![thread(1, 500)];
    let mut last = sample(1000, true, vec![process(10, 1, "zed", "zed")]);
    last.threads = vec![thread(2, 7)];
    let figures = summarize(&[first, last], &[]);
    assert_eq!(figures.threads_seen, 2);
    assert_eq!(figures.wakeups_per_second, 7.0);
}

#[test]
fn a_shared_gpu_client_goes_to_its_lowest_pid() {
    let client = DrmClient {
        device: "gpu".into(),
        client_id: 1,
        vram: 1000,
        gtt: 0,
        gfx_ns: 0,
    };
    // The agent is listed first, but the program's pid is the lower: the program has it.
    let mut agent = process(300, 1, "claude", "claude");
    agent.drm = vec![client.clone()];
    let mut app = process(100, 1, "zed", "zed");
    app.drm = vec![client];
    let figures = summarize(
        &[sample(0, true, vec![agent, app])],
        &["claude".to_string()],
    );
    assert_eq!(figures.app.memory.gpu, 1000);
    assert_eq!(figures.agents.memory.gpu, 0);
}

#[test]
fn huge_figures_do_not_overflow() {
    let mut big = process(10, 1, "zed", "zed");
    big.memory = Some(Memory {
        rss: u64::MAX,
        pss: u64::MAX,
        private: u64::MAX,
        swap: u64::MAX,
    });
    big.cpu_ticks = u64::MAX;
    let figures = summarize(
        &[
            sample(0, true, vec![big.clone(), big.clone()]),
            sample(1, true, vec![big.clone(), big]),
        ],
        &[],
    );
    assert_eq!(figures.app.memory.headline, u64::MAX);
}
