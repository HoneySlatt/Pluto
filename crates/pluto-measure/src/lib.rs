//! A program measured from outside ([ADR-0026]).
//!
//! The rivals' bench runs each subject in a cgroup of its own. `pluto-measure` samples every
//! process and thread in that cgroup, at a fixed interval, without touching the subject, and sums
//! them alike for every program, whether it is one process (Zed, Pluto's client) or fifteen
//! (an Electron application and its agents):
//!
//! - memory: private (clean and dirty) plus swap plus GPU memory, the bar's figure; proportional
//!   (PSS) and resident (RSS) beside it;
//! - processor time: the cgroup's own `cpu.stat`;
//! - wake-ups: every thread's context switches, voluntary and not, as M1.2 counted them with
//!   `pidstat -w`, summed over the threads seen, including those that start after the first sample;
//! - which processes are the agents': those whose executable's name or command's name is given as
//!   an agent's, and all their descendants.
//!
//! - [`proc`]: the kernel's files parsed, from their text.
//! - [`sample`]: one sample of a cgroup, read from `/proc` and `/sys/fs/cgroup`.
//! - [`series`]: samples taken over time, and a subject that ends before the end.
//! - [`summary`]: a series of samples reduced to the figures.
//!
//! [ADR-0026]: ../../../docs/adr/0026-the-rivals-measured-how.md

pub mod proc;
pub mod sample;
pub mod series;
pub mod summary;
