use pluto_measure::proc::{
    CpuStat, DrmClient, Memory, context_switches, cpu_stat, drm_client, id_list, parent_pid,
    smaps_rollup,
};

const ROLLUP: &str =
    "00010000-7ffdcdd02000 ---p 00000000 00:00 0                              [rollup]
Rss:              285048 kB
Pss:              161775 kB
Pss_Dirty:        108956 kB
Shared_Clean:     140024 kB
Private_Clean:     34860 kB
Private_Dirty:    107988 kB
Swap:                 12 kB
SwapPss:               6 kB
Future_Field:          1 kB
";

#[test]
fn smaps_rollup_in_bytes() {
    assert_eq!(
        smaps_rollup(ROLLUP),
        Memory {
            rss: 285048 * 1024,
            pss: 161775 * 1024,
            private: (34860 + 107988) * 1024,
            swap: 12 * 1024
        }
    );
    assert_eq!(smaps_rollup(""), Memory::default());
}

#[test]
fn switches_from_status() {
    let status =
        "Name:\tbrave\nvoluntary_ctxt_switches:\t1365121\nnonvoluntary_ctxt_switches:\t58037\n";
    assert_eq!(context_switches(status), Some(1365121 + 58037));
    assert_eq!(context_switches("voluntary_ctxt_switches:\t3\n"), None);
}

#[test]
fn parent_pid_after_the_last_parenthesis() {
    assert_eq!(parent_pid("1234 (zed) S 1 1234 1234 0 -1"), Some(1));
    assert_eq!(parent_pid("77 (we) ird (name)) R 42 77 77 0"), Some(42));
    assert_eq!(parent_pid("garbage"), None);
}

const FDINFO: &str = "pos:\t0
flags:\t0100002
drm-driver:\tamdgpu
drm-client-id:\t1487
drm-pdev:\t0000:0d:00.0
drm-resident-gtt:\t2 MiB
drm-memory-vram:\t12 KiB
drm-memory-gtt: \t2048 KiB
drm-engine-gfx:\t123456 ns
";

#[test]
fn a_drm_client() {
    assert_eq!(
        drm_client(FDINFO),
        Some(DrmClient {
            device: "0000:0d:00.0".into(),
            client_id: 1487,
            vram: 12 * 1024,
            gtt: 2048 * 1024,
            gfx_ns: 123456
        })
    );
    let resident_only =
        "drm-client-id:\t9\ndrm-pdev:\tp\ndrm-resident-vram:\t3 MiB\ndrm-resident-gtt:\t1 GiB\n";
    assert_eq!(
        drm_client(resident_only),
        Some(DrmClient {
            device: "p".into(),
            client_id: 9,
            vram: 3 << 20,
            gtt: 1 << 30,
            gfx_ns: 0
        })
    );
    let bytes = "drm-client-id:\t9\ndrm-pdev:\tp\ndrm-memory-vram:\t500\n";
    assert_eq!(drm_client(bytes).unwrap().vram, 500);
    assert_eq!(drm_client("pos:\t0\nflags:\t02\n"), None);
}

#[test]
fn a_cgroups_cpu() {
    assert_eq!(
        cpu_stat("usage_usec 1225\nuser_usec 1127\nsystem_usec 97\nnr_periods 0\n"),
        CpuStat {
            usage_usec: 1225,
            user_usec: 1127,
            system_usec: 97
        }
    );
}

#[test]
fn id_lists() {
    assert_eq!(id_list("12\n13\n\nx\n14\n"), vec![12, 13, 14]);
    assert_eq!(id_list(""), Vec::<u32>::new());
}
