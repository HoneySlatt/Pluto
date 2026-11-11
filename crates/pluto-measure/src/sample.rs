//! One sample of a cgroup: its processes, their threads, memory and GPU clients, and the cgroup's
//! processor time.
//!
//! The processes are those of `cgroup.procs` in the cgroup and in every cgroup below it; the
//! threads, those of each process's `task/` directory. A process or thread that ends while it is
//! being read is left out of the sample, never an error; a file that cannot be read for another
//! reason (the kernel refuses another user's `smaps_rollup`) leaves its field empty. Reading
//! memory (`smaps_rollup` and the `fdinfo` of the GPU descriptors) costs the kernel more than the
//! rest, so a sample takes it only when asked.
//!
//! The roots are parameters so that tests read a tree of their own: `/proc` and the cgroup's
//! directory under `/sys/fs/cgroup` in real use.

use crate::proc::{self, CpuStat, DrmClient, Memory};
use std::collections::{HashSet, VecDeque};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Linux's `ESRCH`: reading the files of a process that has just ended can fail with it, as well
/// as with "not found".
const ESRCH: i32 = 3;

/// What reading one file of `/proc` gave. A process that ends is not an error; a file the kernel
/// refuses to this user is not one either.
enum Read {
    Text(String),
    /// The file, and so most likely its process or thread, is gone.
    Gone,
    /// Any other failure, such as a permission error.
    Refused,
}

fn read_text(path: &Path) -> Read {
    match fs::read_to_string(path) {
        Ok(text) => Read::Text(text),
        Err(error) if is_gone(&error) => Read::Gone,
        Err(_) => Read::Refused,
    }
}

fn is_gone(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::NotFound || error.raw_os_error() == Some(ESRCH)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadSample {
    pub pid: u32,
    pub tid: u32,
    /// `starttime` of the thread's `stat`, 0 if unreadable. The kernel reuses ids: the id and the
    /// start time together name one thread.
    pub start_time: u64,
    /// Context switches since the thread began, voluntary and not.
    pub switches: u64,
    /// The thread's name, from its `stat`; empty if unreadable.
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSample {
    pub pid: u32,
    pub parent: Option<u32>,
    /// To show: `exe_name`, else `comm`.
    pub name: String,
    /// The executable's file name (`/proc/<pid>/exe`, " (deleted)" stripped); empty if the link
    /// cannot be read. For a program run by an interpreter or a multicall binary it is not the
    /// program's name: see `comm`.
    pub exe_name: String,
    /// The command's name (`/proc/<pid>/comm`), cut by the kernel to 15 characters. It is the
    /// program's name where `exe_name` is the interpreter's or the multicall binary's.
    pub comm: String,
    /// `starttime` of `stat`, 0 if unreadable: with `pid`, it names one process for good.
    pub start_time: u64,
    /// `utime` plus `stime` from `/proc/<pid>/stat`, in the kernel's USER_HZ ticks: 100 a second.
    pub cpu_ticks: u64,
    /// Present when the sample read memory and the kernel allowed this process's.
    pub memory: Option<Memory>,
    /// The process's DRM clients, read with its memory; empty otherwise.
    pub drm: Vec<DrmClient>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample {
    /// When it was taken, in milliseconds from the first sample of the series.
    pub at_ms: u64,
    /// Whether it was asked to read memory. A process may still have `memory: None` in it: the
    /// kernel refused the read.
    pub read_memory: bool,
    pub processes: Vec<ProcessSample>,
    pub threads: Vec<ThreadSample>,
    /// The cgroup's own `cpu.stat`, which counts processes that ended too.
    pub cpu: CpuStat,
}

pub struct Reader {
    proc_root: PathBuf,
    cgroup: PathBuf,
}

impl Reader {
    pub fn new(proc_root: &Path, cgroup: &Path) -> Reader {
        Reader {
            proc_root: proc_root.to_path_buf(),
            cgroup: cgroup.to_path_buf(),
        }
    }

    /// Takes one sample. An error only if the cgroup itself cannot be read.
    pub fn sample(&self, at_ms: u64, with_memory: bool) -> io::Result<Sample> {
        let cpu = proc::cpu_stat(&fs::read_to_string(self.cgroup.join("cpu.stat"))?);
        let mut processes = Vec::new();
        let mut threads = Vec::new();
        for pid in self.cgroup_pids()? {
            if let Some((process, process_threads)) = self.read_process(pid, with_memory) {
                processes.push(process);
                threads.extend(process_threads);
            }
        }
        Ok(Sample {
            at_ms,
            read_memory: with_memory,
            processes,
            threads,
            cpu,
        })
    }

    /// The pids of the cgroup and of every cgroup below it, each once, in the order found. Only
    /// the cgroup's own list must be readable: a sub-cgroup removed meanwhile is just gone.
    fn cgroup_pids(&self) -> io::Result<Vec<u32>> {
        let mut pids = proc::id_list(&fs::read_to_string(self.cgroup.join("cgroup.procs"))?);
        let mut to_visit: VecDeque<PathBuf> = sub_directories(&self.cgroup).into();
        while let Some(directory) = to_visit.pop_front() {
            if let Ok(text) = fs::read_to_string(directory.join("cgroup.procs")) {
                pids.extend(proc::id_list(&text));
            }
            to_visit.extend(sub_directories(&directory));
        }
        let mut seen = HashSet::new();
        pids.retain(|pid| seen.insert(*pid));
        Ok(pids)
    }

    /// `None` when the process ended while it was read.
    fn read_process(
        &self,
        pid: u32,
        with_memory: bool,
    ) -> Option<(ProcessSample, Vec<ThreadSample>)> {
        let dir = self.proc_root.join(pid.to_string());
        let (parent, cpu_ticks, start_time) = match read_text(&dir.join("stat")) {
            Read::Text(text) => (
                proc::parent_pid(&text),
                proc::cpu_ticks(&text).unwrap_or(0),
                proc::start_time(&text).unwrap_or(0),
            ),
            Read::Gone => return None,
            Read::Refused => (None, 0, 0),
        };
        let (exe_name, comm) = read_names(&dir)?;
        let name = if exe_name.is_empty() {
            &comm
        } else {
            &exe_name
        }
        .clone();
        let threads = read_threads(&dir, pid)?;
        let (memory, drm) = if with_memory {
            let memory = match read_text(&dir.join("smaps_rollup")) {
                Read::Text(text) => Some(proc::smaps_rollup(&text)),
                Read::Gone => return None,
                Read::Refused => None,
            };
            (memory, read_drm_clients(&dir))
        } else {
            (None, Vec::new())
        };
        Some((
            ProcessSample {
                pid,
                parent,
                name,
                exe_name,
                comm,
                start_time,
                cpu_ticks,
                memory,
                drm,
            },
            threads,
        ))
    }
}

/// The executable's file name and `comm`, each empty if it cannot be read. `None` when the
/// process ended. The kernel appends " (deleted)" to the link of an executable replaced while it
/// runs; the name is the same program's.
fn read_names(dir: &Path) -> Option<(String, String)> {
    let exe_name = fs::read_link(dir.join("exe"))
        .ok()
        .and_then(|exe| exe.file_name().map(|n| n.to_string_lossy().into_owned()))
        .map(|file_name| {
            file_name
                .strip_suffix(" (deleted)")
                .unwrap_or(&file_name)
                .to_string()
        })
        .unwrap_or_default();
    let comm = match read_text(&dir.join("comm")) {
        // Only the line's end goes: a name may end in a space, and its length matters (the cut).
        Read::Text(comm) => comm.strip_suffix('\n').unwrap_or(&comm).to_string(),
        Read::Gone => return None,
        Read::Refused => String::new(),
    };
    Some((exe_name, comm))
}

/// `None` when the process ended. A thread that ended, or whose status cannot be read, is left
/// out; one whose `stat` cannot be read has the start time 0.
fn read_threads(dir: &Path, pid: u32) -> Option<Vec<ThreadSample>> {
    let task_dir = dir.join("task");
    let tids = match numbered_entries(&task_dir) {
        Ok(tids) => tids,
        Err(error) if is_gone(&error) => return None,
        Err(_) => return Some(Vec::new()),
    };
    let threads = tids
        .into_iter()
        .filter_map(|tid| {
            let thread_dir = task_dir.join(tid.to_string());
            // `stat` first: a thread that ends between the two reads is then left out, rather
            // than kept under a start time of 0, which would count its switches a second time.
            let (start_time, name) = match read_text(&thread_dir.join("stat")) {
                Read::Text(text) => (
                    proc::start_time(&text).unwrap_or(0),
                    proc::command_name(&text).unwrap_or_default().to_string(),
                ),
                Read::Gone => return None,
                Read::Refused => (0, String::new()),
            };
            let Read::Text(status) = read_text(&thread_dir.join("status")) else {
                return None;
            };
            Some(ThreadSample {
                pid,
                tid,
                start_time,
                switches: proc::context_switches(&status)?,
                name,
            })
        })
        .collect();
    Some(threads)
}

/// The DRM clients behind the process's descriptors, one per descriptor. Only the descriptors
/// that point into `/dev/dri/` are read: the `fdinfo` of other files is costly to the kernel and
/// holds no client. A descriptor whose link cannot be read is read anyway, since the parser
/// refuses what is not a client.
fn read_drm_clients(dir: &Path) -> Vec<DrmClient> {
    let fd_dir = dir.join("fd");
    let fdinfo_dir = dir.join("fdinfo");
    let Ok(fds) = numbered_entries(&fdinfo_dir) else {
        return Vec::new();
    };
    fds.into_iter()
        .filter(|fd| {
            fs::read_link(fd_dir.join(fd.to_string()))
                .map(|target| target.starts_with("/dev/dri"))
                .unwrap_or(true)
        })
        .filter_map(|fd| match read_text(&fdinfo_dir.join(fd.to_string())) {
            Read::Text(text) => proc::drm_client(&text),
            Read::Gone | Read::Refused => None,
        })
        .collect()
}

/// The entries of a directory named by a number, in increasing order.
fn numbered_entries(dir: &Path) -> io::Result<Vec<u32>> {
    let mut numbers: Vec<u32> = fs::read_dir(dir)?
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
        .collect();
    numbers.sort_unstable();
    Ok(numbers)
}

/// The directories directly inside `dir`, sorted so that a sample lists processes in the same
/// order every time.
fn sub_directories(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut directories: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .collect();
    directories.sort();
    directories
}
