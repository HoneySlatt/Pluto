//! The kernel's files parsed from their text. Every parser takes the file's whole text and
//! ignores lines it does not know, so that new kernel fields break nothing. Sizes are in bytes.

/// `/proc/<pid>/smaps_rollup`: its `Rss`, `Pss`, `Private_Clean` plus `Private_Dirty`, and `Swap`
/// lines, each in kB (1024 bytes). A missing line counts as 0.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Memory {
    pub rss: u64,
    pub pss: u64,
    pub private: u64,
    pub swap: u64,
}

pub fn smaps_rollup(text: &str) -> Memory {
    let mut memory = Memory::default();
    let mut private_clean = 0u64;
    let mut private_dirty = 0u64;
    for (key, value) in key_values(text) {
        let Some(bytes) = first_number(value).map(|kb| kb.saturating_mul(1024)) else {
            continue;
        };
        match key {
            "Rss" => memory.rss = bytes,
            "Pss" => memory.pss = bytes,
            "Private_Clean" => private_clean = bytes,
            "Private_Dirty" => private_dirty = bytes,
            "Swap" => memory.swap = bytes,
            _ => {}
        }
    }
    memory.private = private_clean.saturating_add(private_dirty);
    memory
}

/// `/proc/<pid>/task/<tid>/status`: `voluntary_ctxt_switches` plus `nonvoluntary_ctxt_switches`;
/// `None` if either is missing.
pub fn context_switches(text: &str) -> Option<u64> {
    let mut voluntary = None;
    let mut involuntary = None;
    for (key, value) in key_values(text) {
        match key {
            "voluntary_ctxt_switches" => voluntary = first_number(value),
            "nonvoluntary_ctxt_switches" => involuntary = first_number(value),
            _ => {}
        }
    }
    Some(voluntary?.saturating_add(involuntary?))
}

/// `/proc/<pid>/stat`: the parent's pid, the fourth field. The second field, the command's name in
/// parentheses, may itself hold spaces and parentheses: fields are counted after its last `)`.
/// `/proc/<pid>/stat` or a thread's: the command's name, between the first `(` and the last `)`
/// (the name itself may hold parentheses).
pub fn command_name(text: &str) -> Option<&str> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    text.get(open + 1..close)
}

pub fn parent_pid(text: &str) -> Option<u32> {
    fields_after_name(text)?.nth(1)?.parse().ok()
}

/// `/proc/<pid>/stat`: `utime` plus `stime`, the 14th and 15th fields, in clock ticks.
pub(crate) fn cpu_ticks(text: &str) -> Option<u64> {
    // `state` is the field after the name, so `utime` is the 12th from there (index 11).
    let mut fields = fields_after_name(text)?.skip(11);
    let user: u64 = fields.next()?.parse().ok()?;
    let system: u64 = fields.next()?.parse().ok()?;
    Some(user.saturating_add(system))
}

/// `/proc/<pid>/stat` or `/proc/<pid>/task/<tid>/stat`: `starttime`, the 22nd field, in clock ticks
/// after boot. With the id, it names one process or thread for good: the kernel reuses ids.
pub(crate) fn start_time(text: &str) -> Option<u64> {
    // `state` is the field after the name, so `starttime` is the 20th from there (index 19).
    fields_after_name(text)?.nth(19)?.parse().ok()
}

/// One DRM client, from `/proc/<pid>/fdinfo/<fd>` of a GPU file descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrmClient {
    /// `drm-pdev` and `drm-client-id`: the same client seen through several descriptors, or
    /// several processes, is counted once.
    pub device: String,
    pub client_id: u64,
    /// `drm-memory-vram`, else `drm-resident-vram`; units `KiB`, `MiB`, `GiB`, or none (bytes).
    pub vram: u64,
    /// `drm-memory-gtt`, else `drm-resident-gtt`, the same way.
    pub gtt: u64,
    /// `drm-engine-gfx`, in nanoseconds of the GPU's graphics engine; 0 if absent.
    pub gfx_ns: u64,
}

/// `None` for a descriptor that is not a DRM client (no `drm-client-id`).
pub fn drm_client(text: &str) -> Option<DrmClient> {
    let mut client_id = None;
    let mut device = String::new();
    let mut memory_vram = None;
    let mut resident_vram = None;
    let mut memory_gtt = None;
    let mut resident_gtt = None;
    let mut gfx_ns = 0;
    for (key, value) in key_values(text) {
        match key {
            "drm-client-id" => client_id = first_number(value),
            "drm-pdev" => device = value.to_string(),
            "drm-memory-vram" => memory_vram = size_in_bytes(value),
            "drm-resident-vram" => resident_vram = size_in_bytes(value),
            "drm-memory-gtt" => memory_gtt = size_in_bytes(value),
            "drm-resident-gtt" => resident_gtt = size_in_bytes(value),
            "drm-engine-gfx" => gfx_ns = first_number(value).unwrap_or(0),
            _ => {}
        }
    }
    Some(DrmClient {
        device,
        client_id: client_id?,
        vram: memory_vram.or(resident_vram).unwrap_or(0),
        gtt: memory_gtt.or(resident_gtt).unwrap_or(0),
        gfx_ns,
    })
}

/// A cgroup's `cpu.stat`: `usage_usec`, `user_usec`, `system_usec`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuStat {
    pub usage_usec: u64,
    pub user_usec: u64,
    pub system_usec: u64,
}

pub fn cpu_stat(text: &str) -> CpuStat {
    let mut stat = CpuStat::default();
    for line in text.lines() {
        let mut words = line.split_whitespace();
        let (Some(key), Some(value)) = (words.next(), words.next()) else {
            continue;
        };
        let Ok(value) = value.parse() else { continue };
        match key {
            "usage_usec" => stat.usage_usec = value,
            "user_usec" => stat.user_usec = value,
            "system_usec" => stat.system_usec = value,
            _ => {}
        }
    }
    stat
}

/// A list of numbers one a line, as `cgroup.procs` and `cgroup.threads`; lines that are not
/// numbers are skipped.
pub fn id_list(text: &str) -> Vec<u32> {
    text.lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect()
}

/// The lines of the form `Key: value`, split at the first colon and trimmed.
fn key_values(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim(), value.trim()))
}

fn first_number(value: &str) -> Option<u64> {
    value.split_whitespace().next()?.parse().ok()
}

/// A size as the kernel writes it in `fdinfo`: a number, then `KiB`, `MiB`, `GiB`, or nothing for
/// bytes. `None` for another unit, which this reader cannot scale.
fn size_in_bytes(value: &str) -> Option<u64> {
    let mut words = value.split_whitespace();
    let number: u64 = words.next()?.parse().ok()?;
    let unit_shift = match words.next() {
        None => 0,
        Some("KiB") => 10,
        Some("MiB") => 20,
        Some("GiB") => 30,
        Some(_) => return None,
    };
    Some(number.saturating_mul(1 << unit_shift))
}

/// The fields of a `stat` line after the command's name, starting with the state. The name is
/// in parentheses and may hold any character, so the last `)` ends it.
fn fields_after_name(text: &str) -> Option<std::str::SplitWhitespace<'_>> {
    let (_, after_name) = text.rsplit_once(')')?;
    Some(after_name.split_whitespace())
}
