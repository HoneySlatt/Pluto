//! Edge cases of reading one sample from a tree of files: names, vanished threads, descriptors.

use pluto_measure::sample::Reader;
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

const GPU_CLIENT: &str = "drm-client-id:\t1\ndrm-pdev:\tgpu\ndrm-memory-vram:\t4 KiB\n";

/// The 22nd field of `stat` is the start time.
fn stat(pid: u32, name: &str, start_time: u64) -> String {
    format!("{pid} ({name}) S 1 {pid} {pid} 0 -1 0 0 0 0 0 5 5 0 0 20 0 1 0 {start_time} 1 2 3")
}

fn tree(name: &str) -> Tree {
    let root = std::env::temp_dir().join(format!("pluto-test-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let proc = root.join("proc");
    write(&proc.join("100/stat"), &stat(100, "python3", 4242));
    write(&proc.join("100/comm"), ".vibe-wrapped\n");
    std::os::unix::fs::symlink(
        "/nix/store/abc/bin/python3.14 (deleted)",
        proc.join("100/exe"),
    )
    .unwrap();
    write(&proc.join("100/smaps_rollup"), "Rss: 1 kB\n");
    write(&proc.join("100/task/100/stat"), &stat(100, "python3", 4242));
    write(&proc.join("100/task/101/stat"), &stat(100, "worker", 5000));
    for tid in [100, 101] {
        write(
            &proc.join(format!("100/task/{tid}/status")),
            "voluntary_ctxt_switches:\t1\nnonvoluntary_ctxt_switches:\t1\n",
        );
    }
    // Descriptor 3 is a file whose fdinfo happens to look like a GPU client: it must not be
    // read. Descriptor 7 is a render node.
    fs::create_dir_all(proc.join("100/fd")).unwrap();
    std::os::unix::fs::symlink("/tmp/not-a-gpu", proc.join("100/fd/3")).unwrap();
    std::os::unix::fs::symlink("/dev/dri/renderD128", proc.join("100/fd/7")).unwrap();
    write(&proc.join("100/fdinfo/3"), GPU_CLIENT);
    write(
        &proc.join("100/fdinfo/7"),
        "drm-client-id:\t2\ndrm-pdev:\tgpu\ndrm-memory-vram:\t8 KiB\n",
    );
    let cgroup = root.join("cgroup");
    write(&cgroup.join("cgroup.procs"), "100\n");
    write(&cgroup.join("cpu.stat"), "usage_usec 1\n");
    Tree(root)
}

#[test]
fn a_process_keeps_both_names_and_its_start_time() {
    let tree = tree("names");
    let reader = Reader::new(&tree.0.join("proc"), &tree.0.join("cgroup"));
    let sample = reader.sample(0, false).unwrap();
    let process = &sample.processes[0];
    assert_eq!(process.exe_name, "python3.14", "' (deleted)' is stripped");
    assert_eq!(process.comm, ".vibe-wrapped");
    assert_eq!(process.name, "python3.14");
    assert_eq!(process.start_time, 4242);
    let mut starts: Vec<(u32, u64)> = sample
        .threads
        .iter()
        .map(|t| (t.tid, t.start_time))
        .collect();
    starts.sort();
    assert_eq!(starts, vec![(100, 4242), (101, 5000)]);
}

#[test]
fn a_sample_remembers_whether_it_read_memory() {
    let tree = tree("flag");
    let reader = Reader::new(&tree.0.join("proc"), &tree.0.join("cgroup"));
    assert!(reader.sample(0, true).unwrap().read_memory);
    assert!(!reader.sample(0, false).unwrap().read_memory);
}

#[test]
fn only_descriptors_on_a_render_node_are_read() {
    let tree = tree("fdinfo");
    let reader = Reader::new(&tree.0.join("proc"), &tree.0.join("cgroup"));
    let sample = reader.sample(0, true).unwrap();
    let ids: Vec<u64> = sample.processes[0]
        .drm
        .iter()
        .map(|c| c.client_id)
        .collect();
    assert_eq!(ids, vec![2]);
}

#[test]
fn a_thread_gone_between_its_two_reads_is_left_out() {
    let root = std::env::temp_dir().join(format!("pluto-test-gone-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let _tree = Tree(root.clone());
    let proc = root.join("proc");
    write(&proc.join("7/stat"), &stat(7, "app", 10));
    write(&proc.join("7/comm"), "app\n");
    write(&proc.join("7/task/7/stat"), &stat(7, "app", 10));
    write(
        &proc.join("7/task/7/status"),
        "voluntary_ctxt_switches:\t3\nnonvoluntary_ctxt_switches:\t0\n",
    );
    // Thread 8 ended after its `status` was listed: its `stat` is gone.
    write(
        &proc.join("7/task/8/status"),
        "voluntary_ctxt_switches:\t9\nnonvoluntary_ctxt_switches:\t0\n",
    );
    write(&root.join("cgroup/cgroup.procs"), "7\n");
    write(&root.join("cgroup/cpu.stat"), "usage_usec 1\n");
    let sample = Reader::new(&proc, &root.join("cgroup"))
        .sample(0, false)
        .unwrap();
    let tids: Vec<u32> = sample.threads.iter().map(|thread| thread.tid).collect();
    assert_eq!(tids, vec![7]);
}
