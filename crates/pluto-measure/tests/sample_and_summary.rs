use pluto_measure::proc::{CpuStat, DrmClient, Memory};
use pluto_measure::sample::{ProcessSample, Reader, Sample, ThreadSample};
use pluto_measure::summary::{plain_name, summarize};
use std::fs;
use std::path::{Path, PathBuf};

struct Tree(PathBuf);

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// A fake /proc with a parent process (two threads, a GPU client) and a child, and a cgroup with
/// the parent at the top and the child in a sub-cgroup.
fn tree(name: &str) -> Tree {
    let root = std::env::temp_dir().join(format!("pluto-measure-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let proc = root.join("proc");
    write(
        &proc.join("100/stat"),
        "100 (app) S 1 100 100 0 -1 4194560 0 0 0 0 250 50 0 0",
    );
    write(&proc.join("100/comm"), "app\n");
    std::os::unix::fs::symlink("/nix/store/abc-zed/bin/zed", proc.join("100/exe")).unwrap();
    write(
        &proc.join("100/smaps_rollup"),
        "Rss: 100 kB\nPss: 80 kB\nPrivate_Clean: 10 kB\nPrivate_Dirty: 50 kB\nSwap: 5 kB\n",
    );
    write(&proc.join("100/fdinfo/3"), "pos:\t0\n");
    write(
        &proc.join("100/fdinfo/7"),
        "drm-client-id:\t1\ndrm-pdev:\tgpu\ndrm-memory-vram:\t4 KiB\ndrm-memory-gtt:\t2 KiB\n",
    );
    write(
        &proc.join("100/fdinfo/8"),
        "drm-client-id:\t1\ndrm-pdev:\tgpu\ndrm-memory-vram:\t4 KiB\ndrm-memory-gtt:\t2 KiB\n",
    );
    write(
        &proc.join("100/task/100/status"),
        "voluntary_ctxt_switches:\t10\nnonvoluntary_ctxt_switches:\t1\n",
    );
    write(
        &proc.join("100/task/101/status"),
        "voluntary_ctxt_switches:\t5\nnonvoluntary_ctxt_switches:\t0\n",
    );
    write(
        &proc.join("200/stat"),
        "200 (.claude-wrapped) S 100 200 200 0 -1 0 0 0 0 0 7 3 0 0",
    );
    write(&proc.join("200/comm"), ".claude-wrapped\n");
    write(
        &proc.join("200/smaps_rollup"),
        "Rss: 40 kB\nPss: 30 kB\nPrivate_Dirty: 20 kB\n",
    );
    write(
        &proc.join("200/task/200/status"),
        "voluntary_ctxt_switches:\t2\nnonvoluntary_ctxt_switches:\t2\n",
    );
    // Every thread has its `stat`, as the kernel gives it: its start time is field 22, 0 here.
    for (pid, tid) in [(100, 100), (100, 101), (200, 200)] {
        write(
            &proc.join(format!("{pid}/task/{tid}/stat")),
            &format!("{tid} (t) S 1 1 1 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 0 0 0"),
        );
    }
    let cgroup = root.join("cgroup");
    write(&cgroup.join("cgroup.procs"), "100\n");
    write(
        &cgroup.join("cpu.stat"),
        "usage_usec 5000000\nuser_usec 4000000\nsystem_usec 1000000\n",
    );
    write(&cgroup.join("agents/cgroup.procs"), "200\n300\n");
    Tree(root)
}

#[test]
fn a_sample_reads_every_process_below_the_cgroup() {
    let tree = tree("read");
    let reader = Reader::new(&tree.0.join("proc"), &tree.0.join("cgroup"));
    let sample = reader.sample(0, true).unwrap();
    assert_eq!(
        sample.cpu,
        CpuStat {
            usage_usec: 5_000_000,
            user_usec: 4_000_000,
            system_usec: 1_000_000
        }
    );
    let mut pids: Vec<u32> = sample.processes.iter().map(|p| p.pid).collect();
    pids.sort();
    assert_eq!(
        pids,
        vec![100, 200],
        "300 ended before it was read: left out"
    );
    let app = sample.processes.iter().find(|p| p.pid == 100).unwrap();
    assert_eq!(app.name, "zed");
    assert_eq!(app.parent, Some(1));
    assert_eq!(app.cpu_ticks, 300);
    assert_eq!(
        app.memory,
        Some(Memory {
            rss: 100 * 1024,
            pss: 80 * 1024,
            private: 60 * 1024,
            swap: 5 * 1024
        })
    );
    assert_eq!(
        app.drm.len(),
        2,
        "one per descriptor; the summary counts the client once"
    );
    let agent = sample.processes.iter().find(|p| p.pid == 200).unwrap();
    assert_eq!(
        agent.name, ".claude-wrapped",
        "comm, since exe cannot be read"
    );
    assert_eq!(agent.parent, Some(100));
    let mut threads = sample.threads.clone();
    threads.sort_by_key(|t| t.tid);
    assert_eq!(
        threads,
        vec![
            ThreadSample {
                pid: 100,
                tid: 100,
                start_time: 0,
                switches: 11,
                name: "t".into(),
            },
            ThreadSample {
                pid: 100,
                tid: 101,
                start_time: 0,
                switches: 5,
                name: "t".into(),
            },
            ThreadSample {
                pid: 200,
                tid: 200,
                start_time: 0,
                switches: 4,
                name: "t".into(),
            },
        ]
    );
    let light = reader.sample(1000, false).unwrap();
    assert!(
        light
            .processes
            .iter()
            .all(|p| p.memory.is_none() && p.drm.is_empty())
    );
}

#[test]
fn a_missing_cgroup_is_an_error() {
    let tree = tree("missing");
    assert!(
        Reader::new(&tree.0.join("proc"), &tree.0.join("nowhere"))
            .sample(0, false)
            .is_err()
    );
}

fn process(
    pid: u32,
    parent: u32,
    name: &str,
    ticks: u64,
    private: u64,
    drm: Vec<DrmClient>,
) -> ProcessSample {
    ProcessSample {
        pid,
        parent: Some(parent),
        name: name.into(),
        exe_name: String::new(),
        comm: String::new(),
        start_time: 0,
        cpu_ticks: ticks,
        memory: Some(Memory {
            rss: private * 2,
            pss: private + 1,
            private,
            swap: 1,
        }),
        drm,
    }
}

fn gpu(id: u64) -> DrmClient {
    DrmClient {
        device: "gpu".into(),
        client_id: id,
        vram: 1000,
        gtt: 24,
        gfx_ns: 0,
    }
}

#[test]
fn figures_over_a_series() {
    let first = Sample {
        at_ms: 0,
        read_memory: true,
        processes: vec![
            process(100, 1, "electron", 100, 5000, vec![gpu(1)]),
            process(101, 100, "electron", 10, 3000, vec![gpu(1)]),
            process(200, 100, ".claude-wrapped", 50, 7000, vec![]),
        ],
        threads: vec![
            ThreadSample {
                pid: 100,
                tid: 100,
                start_time: 0,
                switches: 1000,

                name: String::new(),
            },
            ThreadSample {
                pid: 200,
                tid: 200,
                start_time: 0,
                switches: 50,

                name: String::new(),
            },
        ],
        cpu: CpuStat {
            usage_usec: 1_000_000,
            user_usec: 0,
            system_usec: 0,
        },
    };
    let middle = Sample {
        at_ms: 5000,
        read_memory: false,
        processes: vec![],
        threads: vec![],
        cpu: CpuStat::default(),
    };
    let mut last = first.clone();
    last.at_ms = 10_000;
    last.cpu.usage_usec = 3_000_000;
    last.processes[0] = process(100, 1, "electron", 300, 9000, vec![gpu(1)]);
    last.processes[2] = process(200, 100, ".claude-wrapped", 150, 8000, vec![]);
    last.processes
        .push(process(201, 200, "bash", 20, 500, vec![]));
    last.threads = vec![
        ThreadSample {
            pid: 100,
            tid: 100,
            start_time: 0,
            switches: 1100,

            name: String::new(),
        },
        ThreadSample {
            pid: 200,
            tid: 200,
            start_time: 0,
            switches: 80,

            name: String::new(),
        },
        ThreadSample {
            pid: 201,
            tid: 201,
            start_time: 0,
            switches: 20,

            name: String::new(),
        },
    ];
    let mut middle = middle;
    middle.processes = vec![];
    let figures = summarize(&[first, middle, last], &["claude".to_string()]);
    assert_eq!(figures.seconds, 10.0);
    assert_eq!(figures.samples, 3);
    assert_eq!(figures.cpu_seconds, 2.0);
    assert_eq!(figures.cpu_cores, 0.2);
    assert_eq!(figures.wakeups_per_second, (100.0 + 30.0 + 20.0) / 10.0);
    assert_eq!(figures.threads_seen, 3);
    assert_eq!(figures.app.processes, 2);
    assert_eq!(figures.agents.processes, 2, "bash descends from the agent");
    assert_eq!(figures.app.wakeups_per_second, 10.0);
    assert_eq!(figures.agents.wakeups_per_second, 5.0);
    assert_eq!(figures.app.cpu_seconds, 2.0, "200 ticks");
    assert_eq!(
        figures.agents.cpu_seconds, 1.2,
        "100 + 20 ticks, bash from 0"
    );
    assert_eq!(figures.app.memory.private, 9000 + 3000);
    assert_eq!(figures.app.memory.swap, 2);
    assert_eq!(
        figures.app.memory.gpu, 1024,
        "the client shared by two processes, once"
    );
    assert_eq!(figures.app.memory.headline, 12000 + 2 + 1024);
    assert_eq!(figures.app.memory.rss, 24000);
    assert_eq!(figures.app.memory.pss, 12002);
    assert_eq!(figures.app.memory_peak, 12000 + 2 + 1024);
    assert_eq!(figures.agents.memory.headline, 8500 + 2);
}

#[test]
fn the_peak_is_the_largest_sample() {
    let mut big = Sample {
        at_ms: 0,
        read_memory: true,
        processes: vec![process(1, 0, "zed", 0, 9000, vec![])],
        threads: vec![],
        cpu: CpuStat::default(),
    };
    let mut small = big.clone();
    small.at_ms = 2000;
    small.processes[0] = process(1, 0, "zed", 0, 100, vec![]);
    let figures = summarize(&[big.clone(), small], &[]);
    assert_eq!(figures.app.memory.headline, 101);
    assert_eq!(figures.app.memory_peak, 9001);
    big.at_ms = 0;
    let one = summarize(&[big], &[]);
    assert_eq!(one.seconds, 0.0);
    assert_eq!(one.wakeups_per_second, 0.0);
    assert_eq!(one.app.memory.headline, 9001);
    assert_eq!(summarize(&[], &[]), Default::default());
}

#[test]
fn plain_names() {
    assert_eq!(plain_name(".claude-wrapped"), "claude");
    assert_eq!(plain_name("codex"), "codex");
    assert_eq!(plain_name(".opencode"), "opencode");
    assert_eq!(plain_name("x-wrapped-wrapped"), "x-wrapped");
}

#[test]
fn the_busiest_threads_are_named_and_summed_by_name() {
    let thread = |pid, tid, switches: u64, name: &str| ThreadSample {
        pid,
        tid,
        start_time: 0,
        switches,
        name: name.into(),
    };
    let processes = vec![
        process(1, 0, "electron", 0, 10, vec![]),
        process(2, 1, "electron", 0, 10, vec![]),
        process(3, 1, ".claude-wrapped", 0, 10, vec![]),
    ];
    let mut first = Sample {
        at_ms: 0,
        processes: processes.clone(),
        threads: vec![
            thread(1, 1, 0, "Compositor"),
            thread(2, 20, 0, "Compositor"),
            thread(3, 30, 0, "claude"),
        ],
        cpu: CpuStat::default(),
        read_memory: true,
    };
    let mut last = first.clone();
    last.at_ms = 10_000;
    last.threads = vec![
        thread(1, 1, 100, "Compositor"),
        thread(2, 20, 50, "Compositor"),
        thread(3, 30, 200, "claude"),
    ];
    first.threads.push(thread(1, 2, 0, "idle"));
    let figures = summarize(&[first, last], &["claude".to_string()]);
    let names: Vec<(&str, &str, bool, f64)> = figures
        .busiest_threads
        .iter()
        .map(|t| {
            (
                t.process.as_str(),
                t.thread.as_str(),
                t.agent,
                t.wakeups_per_second,
            )
        })
        .collect();
    assert_eq!(
        names,
        vec![
            (".claude-wrapped", "claude", true, 20.0),
            ("electron", "Compositor", false, 15.0),
        ],
        "summed across the processes of one name; a thread that never woke is left out"
    );
}

#[test]
fn the_gpu_engines_time_shows_rendering_on_the_gpu() {
    let mut client = DrmClient {
        device: "gpu".into(),
        client_id: 1,
        vram: 0,
        gtt: 0,
        gfx_ns: 5_000,
    };
    let held = |client: DrmClient| Sample {
        at_ms: 0,
        processes: vec![process(1, 0, "zed", 0, 10, vec![client.clone(), client])],
        threads: vec![],
        cpu: CpuStat::default(),
        read_memory: true,
    };
    assert_eq!(
        summarize(&[held(client.clone())], &[])
            .app
            .memory
            .gpu_engine_ns,
        5_000
    );
    client.gfx_ns = 0;
    assert_eq!(summarize(&[held(client)], &[]).app.memory.gpu_engine_ns, 0);
}
