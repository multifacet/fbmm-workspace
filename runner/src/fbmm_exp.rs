use clap::clap_app;

use libscail::{
    background::{BackgroundContext, BackgroundTask},
    dir, dump_sys_info, get_user_home_dir,
    output::{Parametrize, Timestamp},
    set_kernel_printk_level, time, validator,
    workloads::{
        gen_perf_command_prefix, run_canneal, run_spec17, CannealWorkload, MemcachedWorkloadConfig,
        Spec2017Workload, TasksetCtxBuilder, TasksetCtxInterleaving, YcsbConfig, YcsbDistribution,
        YcsbSession, YcsbSystem, YcsbWorkload,
    },
    Login,
};

use serde::{Deserialize, Serialize};

use spurs::{cmd, Execute, SshShell};
use spurs_util::escape_for_bash;
use std::time::Instant;

pub const PERIOD: usize = 10; // seconds

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
enum PagewalkCoherenceMode {
    Speculation,
    Coherence,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
enum Workload {
    Spec2017Mcf,
    Spec2017Xalancbmk,
    Spec2017Xz {
        size: usize,
    },
    Spec2017CactuBSSN,
    Canneal {
        workload: CannealWorkload,
    },
    AllocTest {
        size: usize,
        num_allocs: usize,
        threads: usize,
        populate: bool,
    },
    Gups {
        threads: usize,
        exp: usize,
        hot_exp: Option<usize>,
        move_hot: bool,
        num_updates: usize,
    },
    PagewalkCoherence {
        mode: PagewalkCoherenceMode,
    },
    Memcached {
        size: usize,
        op_count: usize,
        read_prop: f32,
        update_prop: f32,
    },
    Graph500 {
        size: usize,
    },
    Stream {
        threads: usize,
    },
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
struct MemRegion {
    size: usize,
    start: usize,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
enum MMFS {
    Ext4,
    BasicMMFS {
        num_pages: usize,
    },
    TieredMMFS,
    ContigMMFS,
    BandwidthMMFS,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
struct NodeWeight {
    nid: u32,
    weight: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Parametrize)]
struct Config {
    #[name]
    exp: String,

    #[name]
    workload: Workload,

    perf_stat: bool,
    perf_periodic: bool,
    perf_counters: Vec<String>,
    disable_thp: bool,
    disable_aslr: bool,
    mm_fault_tracker: bool,
    mmap_tracker: bool,
    flame_graph: bool,
    smaps_periodic: bool,
    tmmfs_stats_periodic: bool,
    tmmfs_active_list_periodic: bool,
    lock_stat: bool,
    fbmm: Option<MMFS>,
    tpp: bool,
    dram_region: Option<MemRegion>,
    pmem_region: Option<MemRegion>,
    node_weights: Vec<NodeWeight>,
    numactl: bool,
    badger_trap: bool,
    migrate_task_int: Option<usize>,
    numa_scan_size: Option<usize>,
    numa_scan_delay: Option<usize>,
    numa_scan_period_min: Option<usize>,
    hugetlb: Option<usize>,
    pte_fault_size: Option<usize>,

    thp_temporal_zero: bool,
    no_fpm_fix: bool,
    no_pmem_write_zeroes: bool,
    track_pfn_insert: bool,
    mark_inode_dirty: bool,
    ext4_metadata: bool,
    no_prealloc: bool,

    username: String,
    host: String,

    remote_research_settings: std::collections::BTreeMap<String, String>,

    #[timestamp]
    timestamp: Timestamp,
}

pub fn cli_options() -> clap::App<'static, 'static> {
    clap_app! { fbmm_exp =>
        (about: "Run file based mm experiments. Requires `sudo`.")
        (@setting ArgRequiredElseHelp)
        (@setting DisableVersion)
        (@arg HOSTNAME: +required +takes_value
         "The domain name of the remote")
        (@arg USERNAME: +required +takes_value
         "The username on the remote")
        (@subcommand alloctest =>
            (about: "Run the `alloctest` workload.")
            (@arg SIZE: +required +takes_value {validator::is::<usize>}
             "The number of pages to map in each allocation")
            (@arg NUM_ALLOCS: +takes_value {validator::is::<usize>}
             "The number of calls to mmap to do")
            (@arg THREADS: --threads +takes_value {validator::is::<usize>}
             "The number of threads to run alloctest with")
            (@arg POPULATE: --populate
             "Run alloctest where regions are MMAPed with the MAP_POPULATE flag")
        )
        (@subcommand canneal =>
            (about: "Run the canneal workload.")
            (@group CANNEAL_WORKLOAD =>
                (@arg SMALL: --small
                 "Use the small workload.")
                (@arg MEDIUM: --medium
                 "Use the medium workload.")
                (@arg LARGE: --large
                 "Use the large workload.")
                (@arg NATIVE: --native
                 "Use the native workload.")
            )
        )
        (@subcommand spec17 =>
            (about: "Run a spec workload on cloudlab")
            (@arg WHICH: +required
             "Which spec worklosd to run.")
            (@arg SIZE: --spec_size +takes_value {validator::is::<usize>}
             "The size of the spec workload input.")
        )
        (@subcommand gups =>
            (about: "Run the GUPS workload used to eval HeMem")
            (@arg MOVE_HOT: --move_hot
             requires[HOT_EXP]
             "Move the hotset partway through GUPS's execution.")
            (@arg THREADS: --threads +takes_value {validator::is::<usize>}
             "The number of threads to run GUPS with. Default: 1")
            (@arg EXP: +required +takes_value {validator::is::<usize>}
             "The log of the size of the workload.")
            (@arg HOT_EXP: +takes_value {validator::is::<usize>}
             "The log of the size of the hot region, if there is one")
            (@arg NUM_UPDATES: +takes_value {validator::is::<usize>}
             "The number of updates to do. Default is 2^exp / 8")
        )
        (@subcommand pagewalk_coherence =>
            (about: "Run the ubmk from https://blog.stuffedcow.net/2015/08/pagewalk-coherence/\
             to determine what the pagewalk consistency the CPU has.")
            (@group MODE =>
                (@attributes +required)
                (@arg SPECULATION: --speculation
                 "Run to check for speculation.")
                (@arg COHERENCE: --coherence
                 "Run to check basic coherence.")
            )
        )
        (@subcommand memcached =>
            (about: "Run the memcached workload driven by YCSB")
            (@arg SIZE: +required +takes_value {validator::is::<usize>}
             "The number of GBs for the workload.")
            (@arg OP_COUNT: --op_count +takes_value {validator::is::<usize>}
             "The number of operations to perform during the workload.\
             The default is 1000.")
            (@arg READ_PROP: --read_prop +takes_value {validator::is::<f32>}
             "The proportion of read operations to perform as a value between 0 and 1.\
             The default is 0.5. The proportion on insert operations will be 1 - read_prop - update_prop.")
            (@arg UPDATE_PROP: --update_prop +takes_value {validator::is::<f32>}
             "The proportion of read operations to perform as a value between 0 and 1.\
             The default is 0.5. The proportion on insert operations will be 1 - read_prop - update_prop")
        )
        (@subcommand graph500 =>
            (about: "Run the Graph500 workload")
            (@arg SIZE: +required +takes_value {validator::is::<usize>}
             "2^size nodes will be used for the workload.")
        )
        (@subcommand stream =>
            (about: "Run the STREAM ubmk")
            (@arg THREADS: --threads +takes_value {validator::is::<usize>}
             "The number of threads to run GUPS with. Default: 1")
        )
        (@arg PERF_STAT: --perf_stat
         "Attach perf stat to the workload.")
        (@arg PERF_PERIODIC: --perf_periodic
         requires[PERF_STAT]
         "Record perf stat periodically throughout the execution of the application.")
        (@arg PERF_COUNTER: --perf_counter +takes_value ... number_of_values(1)
         requires[PERF_STAT]
         "Which counters to record with perf stat.")
        (@arg DISABLE_THP: --disable_thp
         "Disable THP completely.")
        (@arg DISABLE_ASLR: --disable_aslr
         "Disable ASLR.")
        (@arg MM_FAULT_TRACKER: --mm_fault_tracker
         "Record page fault statistics with mm_fault_tracker.")
        (@arg MMAP_TRACKER: --mmap_tracker
         "Record page fault statistics with mmap_tracker.")
        (@arg FLAME_GRAPH: --flame_graph
         "Generate a flame graph of the workload.")
        (@arg SMAPS_PERIODIC: --smaps_periodic
         "Collect /proc/[PID]/smaps data periodically for the workload process")
        (@arg TMMFS_STATS_PERIODIC: --tmmfs_stats_periodic
         requires[TIEREDMMFS]
         "Collect /sys/fs/tieredmmfs/stats data periodically.")
        (@arg TMMFS_ACTIVE_LIST_PERIODIC: --tmmfs_active_list_periodic
         requires[TIEREDMMFS]
         "Collect /sys/fs/tieredmmfs/active_list data periodically.")
        (@arg NUMACTL: --numactl
         "If passed, use numactl to make sure the workload only allocates from numa node 0.")
        (@arg BADGER_TRAP: --badger_trap
         "If passed, use badger trap to monitor the TLB misses of the workload.")
        (@arg LOCK_STAT: --lock_stat
         "Collect lock statistics from the workload.")
        (@arg FBMM: --fbmm
         requires[MMFS_TYPE] conflicts_with[TPP] conflicts_with[HUGETLB]
         "Run the workload with file based mm with the specified FS (either ext4 or TieredMMFS).")
        (@arg TPP: --tpp
         requires[DRAM_SIZE] conflicts_with[FBMM] conflicts_with[HUGETLB]
         "Run the workload with TPP.")
        (@group MMFS_TYPE =>
            (@attributes requires[FBMM])
            (@arg EXT4: --ext4
             "Use ext4 as the MM filesystem.")
            (@arg BASICMMFS: --basicmmfs +takes_value {validator::is::<usize>}
             "Use the BasicMMFS as the MM filesystem. Takes the number of pages it should reserve.")
            (@arg TIEREDMMFS: --tieredmmfs
             requires[DRAM_SIZE] requires[PMEM_SIZE]
             "Use TieredMMFS as the MM filesystem.")
            (@arg CONTIGMMFS: --contigmmfs
             "Use the ContgMMFS as the MM filesystem.")
            (@arg BWMMFS: --bwmmfs
             "Use the BandwidthMMFS as the MM filesystem.")
        )
        (@arg DRAM_SIZE: --dram_size +takes_value {validator::is::<usize>}
         "If passed, reserved the specifies amount of memory in GB as DRAM.")
        (@arg DRAM_START: --dram_start +takes_value {validator::is::<usize>}
         "If passed, specifies the starting point of the reserved DRAM in GB. Default is 4GB")
        (@arg PMEM_SIZE: --pmem_size +takes_value {validator::is::<usize>}
         requires[TIEREDMMFS]
         "If passed, reserved the specified amount of memory in GB as PMEM.")
        (@arg PMEM_START: --pmem_start +takes_value {validator::is::<usize>}
         requires[TIEREDMMFS]
         "If passed, specifies the starting point of the reserved PMEM in GB. \
         Default is dram_size + dram_start.")
        (@arg NODE_WEIGHT: --node_weight +takes_value ... number_of_values(1)
         requires[BWMMFS]
         "The node weights to use when using BWMMFS. Taken in the form of \"<nid>:<weight>\". \
         The default node weight is 1.")
        (@arg MIGRATE_TASK_INT: --migrate_task_int +takes_value {validator::is::<usize>}
         "(Optional) If passed, sets the migration task interval (in ms) to the specified value.")
        (@arg NUMA_SCAN_SIZE:  --numa_scan_size +takes_value {validator::is::<usize>}
         "(Optional) If passed, sets the size of the numa balancing scan size in MB.")
        (@arg NUMA_SCAN_DELAY: --numa_scan_delay +takes_value {validator::is::<usize>}
         "(Optional) If passed, sets the time to delay numa balancing scanning in ms.")
        (@arg NUMA_SCAN_PERIOD_MIN: --numa_scan_period_min +takes_value {validator::is::<usize>}
         "(Optional) If passed, sets the minimum period between numa balancing scans in ms.")
        (@arg HUGETLB: --hugetlb +takes_value {validator::is::<usize>}
         conflicts_with[FBMM] conflicts_with[TPP]
         "Run certain workloads with libhugetlbfs. Specify the number of huge pages to reserve in GB")
        (@arg PTE_FAULT_SIZE: --pte_fault_size +takes_value {validator::is::<usize>}
         "The number of pages to allocate on a DAX pte fault.")
        (@arg THP_TEMPORAL_ZERO: --thp_temporal_zero
         conflicts_with[FBMM] conflicts_with[DISABLE_THP]
         "Tell the kernel to use the standard erms zeroing for huge pages.")
        (@arg NO_FPM_FIX: --no_fpm_fix
         "Tell the kernel to ignore the optimization to the follow_page_mask function for FOM.")
        (@arg NO_PMEM_WRITE_ZEROES: --no_pmem_write_zeroes
         "Tell the kernels not to zero FBMM pages by copying the zero page.")
        (@arg TRACK_PFN_INSERT: --track_pfn_insert
         "Tell the kernel to call the expensive track_pfn_insert function.")
        (@arg MARK_INODE_DIRTY: --mark_inode_dirty
         "Tell the kernel to call the expensive mark_inode_dirty function.")
        (@arg EXT4_METADATA: --ext4_metadata
         "Have ext4 keep track of metadata, including checksums.")
        (@arg NO_PREALLOC: --no_prealloc
         "Do not preallocate memory on MAP_POPULATE.")
    }
}

pub fn run(sub_m: &clap::ArgMatches<'_>) -> Result<(), failure::Error> {
    let login = Login {
        username: sub_m.value_of("USERNAME").unwrap(),
        hostname: sub_m.value_of("HOSTNAME").unwrap(),
        host: sub_m.value_of("HOSTNAME").unwrap(),
    };

    let workload = match sub_m.subcommand() {
        ("alloctest", Some(sub_m)) => {
            let size = sub_m.value_of("SIZE").unwrap().parse::<usize>().unwrap();
            let num_allocs = sub_m
                .value_of("NUM_ALLOCS")
                .unwrap_or("1")
                .parse::<usize>()
                .unwrap();
            let threads = sub_m
                .value_of("THREADS")
                .unwrap_or("1")
                .parse::<usize>()
                .unwrap();
            let populate = sub_m.is_present("POPULATE");
            Workload::AllocTest { size, num_allocs, threads, populate }
        }

        ("canneal", Some(sub_m)) => {
            let workload = if sub_m.is_present("SMALL") {
                CannealWorkload::Small
            } else if sub_m.is_present("MEDIUM") {
                CannealWorkload::Medium
            } else if sub_m.is_present("LARGE") {
                CannealWorkload::Large
            } else {
                CannealWorkload::Native
            };

            Workload::Canneal { workload }
        }

        ("spec17", Some(sub_m)) => {
            let size = sub_m
                .value_of("SIZE")
                .unwrap_or("0")
                .parse::<usize>()
                .unwrap();

            match sub_m.value_of("WHICH").unwrap() {
                "mcf" => Workload::Spec2017Mcf,
                "xalancbmk" => Workload::Spec2017Xalancbmk,
                "xz" => Workload::Spec2017Xz { size },
                "cactubssn" => Workload::Spec2017CactuBSSN,
                _ => panic!("Unknown spec workload"),
            }
        }

        ("gups", Some(sub_m)) => {
            let move_hot = sub_m.is_present("MOVE_HOT");
            let threads = sub_m
                .value_of("THREADS")
                .unwrap_or("1")
                .parse::<usize>()
                .unwrap();
            let exp = sub_m.value_of("EXP").unwrap().parse::<usize>().unwrap();
            let hot_exp = sub_m
                .value_of("HOT_EXP")
                .map(|v| v.parse::<usize>().unwrap());
            let num_updates = if let Some(updates_str) = sub_m.value_of("NUM_UPDATES") {
                updates_str.parse::<usize>().unwrap()
            } else {
                (1 << exp) / 8
            };
            Workload::Gups {
                threads,
                exp,
                hot_exp,
                move_hot,
                num_updates,
            }
        }

        ("pagewalk_coherence", Some(sub_m)) => {
            let mode = if sub_m.is_present("SPECULATION") {
                PagewalkCoherenceMode::Speculation
            } else {
                PagewalkCoherenceMode::Coherence
            };

            Workload::PagewalkCoherence { mode }
        }

        ("memcached", Some(sub_m)) => {
            let size = sub_m.value_of("SIZE").unwrap().parse::<usize>().unwrap();
            let op_count = sub_m
                .value_of("OP_COUNT")
                .unwrap_or("1000")
                .parse::<usize>()
                .unwrap();
            let read_prop = sub_m
                .value_of("READ_PROP")
                .unwrap_or("0.5")
                .parse::<f32>()
                .unwrap();
            let update_prop = sub_m
                .value_of("UPDATE_PROP")
                .unwrap_or("0.5")
                .parse::<f32>()
                .unwrap();

            Workload::Memcached {
                size,
                op_count,
                read_prop,
                update_prop,
            }
        }

        ("graph500", Some(sub_m)) => {
            let size = sub_m.value_of("SIZE").unwrap().parse::<usize>().unwrap();

            Workload::Graph500 { size }
        }

        ("stream", Some(sub_m)) => {
            let threads = sub_m
                .value_of("THREADS")
                .unwrap_or("1")
                .parse::<usize>()
                .unwrap();
            Workload::Stream { threads }
        }

        _ => unreachable!(),
    };

    let perf_stat = sub_m.is_present("PERF_STAT");
    let perf_periodic = sub_m.is_present("PERF_PERIODIC");
    let disable_thp = sub_m.is_present("DISABLE_THP");
    let disable_aslr = sub_m.is_present("DISABLE_ASLR");
    let mm_fault_tracker = sub_m.is_present("MM_FAULT_TRACKER");
    let mmap_tracker = sub_m.is_present("MMAP_TRACKER");
    let flame_graph = sub_m.is_present("FLAME_GRAPH");
    let smaps_periodic = sub_m.is_present("SMAPS_PERIODIC");
    let tmmfs_stats_periodic = sub_m.is_present("TMMFS_STATS_PERIODIC");
    let tmmfs_active_list_periodic = sub_m.is_present("TMMFS_ACTIVE_LIST_PERIODIC");
    let numactl = sub_m.is_present("NUMACTL");
    let lock_stat = sub_m.is_present("LOCK_STAT");
    let badger_trap = sub_m.is_present("BADGER_TRAP");
    let fbmm = sub_m.is_present("FBMM").then(|| {
        if sub_m.is_present("EXT4") {
            MMFS::Ext4
        } else if let Some(num_pages_str) = sub_m.value_of("BASICMMFS") {
            let num_pages = num_pages_str.parse::<usize>().unwrap();
            MMFS::BasicMMFS {
                num_pages,
            }
        } else if sub_m.is_present("TIEREDMMFS") {
            MMFS::TieredMMFS
        } else if sub_m.is_present("CONTIGMMFS") {
            MMFS::ContigMMFS
        } else if sub_m.is_present("BWMMFS") {
            MMFS::BandwidthMMFS
        } else {
            panic!("Invalid MM file system. Use either --ext4 or --tieredmmfs");
        }
    });
    let tpp = sub_m.is_present("TPP");
    let dram_region = sub_m.is_present("DRAM_SIZE").then(|| {
        let dram_size = sub_m
            .value_of("DRAM_SIZE")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        // 4GB seems to be where RAM starts in phys mem in most system
        let dram_start = sub_m
            .value_of("DRAM_START")
            .unwrap_or("4")
            .parse::<usize>()
            .unwrap();

        MemRegion {
            size: dram_size,
            start: dram_start,
        }
    });
    let pmem_region = sub_m.is_present("PMEM_SIZE").then(|| {
        let pmem_size = sub_m
            .value_of("PMEM_SIZE")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        let pmem_start = sub_m
            .value_of("PMEM_START")
            .unwrap_or(&(dram_region.unwrap().size + dram_region.unwrap().start).to_string())
            .parse::<usize>()
            .unwrap();

        MemRegion {
            size: pmem_size,
            start: pmem_start,
        }
    });
    let node_weights: Vec<NodeWeight> =
        sub_m
            .values_of("NODE_WEIGHT")
            .map_or(Vec::new(), |counters| {
                counters
                    .map(|s| {
                        // The format of a node weight is <nid>:<weight>
                        let split: Vec<&str> = s.split(":").collect();
                        let nid = split[0].parse::<u32>().unwrap();
                        let weight = split[1].parse::<u32>().unwrap();

                        NodeWeight { nid, weight }
                    })
                    .collect()
            });
    let migrate_task_int = sub_m
        .value_of("MIGRATE_TASK_INT")
        .map(|interval| interval.parse::<usize>().unwrap());
    let numa_scan_size = sub_m
        .value_of("NUMA_SCAN_SIZE")
        .map(|size| size.parse::<usize>().unwrap());
    let numa_scan_delay = sub_m
        .value_of("NUMA_SCAN_DELAY")
        .map(|delay| delay.parse::<usize>().unwrap());
    let numa_scan_period_min = sub_m
        .value_of("NUMA_SCAN_PERIOD_MIN")
        .map(|delay| delay.parse::<usize>().unwrap());
    let hugetlb = sub_m
        .value_of("HUGETLB")
        .map(|huge_size| huge_size.parse::<usize>().unwrap());
    let pte_fault_size = sub_m
        .value_of("PTE_FAULT_SIZE")
        .map(|v| v.parse::<usize>().unwrap());
    let thp_temporal_zero = sub_m.is_present("THP_TEMPORAL_ZERO");
    let no_fpm_fix = sub_m.is_present("NO_FPM_FIX");
    let no_pmem_write_zeroes = sub_m.is_present("NO_PMEM_WRITE_ZEROES");
    let track_pfn_insert = sub_m.is_present("TRACK_PFN_INSERT");
    let mark_inode_dirty = sub_m.is_present("MARK_INODE_DIRTY");
    let no_prealloc = sub_m.is_present("NO_PREALLOC");
    let ext4_metadata = sub_m.is_present("EXT4_METADATA");
    let perf_counters: Vec<String> = sub_m
        .values_of("PERF_COUNTER")
        .map_or(Vec::new(), |counters| counters.map(Into::into).collect());

    let ushell = SshShell::with_any_key(login.username, login.host)?;
    let remote_research_settings = libscail::get_remote_research_settings(&ushell)?;

    let cfg = Config {
        exp: "fom_exp".into(),
        workload,
        perf_stat,
        perf_periodic,
        perf_counters,
        disable_thp,
        disable_aslr,
        mm_fault_tracker,
        mmap_tracker,
        flame_graph,
        smaps_periodic,
        tmmfs_stats_periodic,
        tmmfs_active_list_periodic,
        numactl,
        badger_trap,
        lock_stat,
        fbmm,
        tpp,
        dram_region,
        pmem_region,
        node_weights,
        migrate_task_int,
        numa_scan_size,
        numa_scan_delay,
        numa_scan_period_min,
        hugetlb,
        pte_fault_size,

        thp_temporal_zero,
        no_fpm_fix,
        no_pmem_write_zeroes,
        track_pfn_insert,
        mark_inode_dirty,
        ext4_metadata,
        no_prealloc,

        username: login.username.into(),
        host: login.hostname.into(),

        remote_research_settings,

        timestamp: Timestamp::now(),
    };

    run_inner(&login, &cfg)
}

fn run_inner<A>(login: &Login<A>, cfg: &Config) -> Result<(), failure::Error>
where
    A: std::net::ToSocketAddrs + std::fmt::Display + std::fmt::Debug + Clone,
{
    // Collect timers on VM
    let mut timers = vec![];
    let ushell = SshShell::with_any_key(login.username, &login.host)?;
    let user_home = get_user_home_dir(&ushell)?;

    // Setup the output file name
    let results_dir = dir!(&user_home, crate::RESULTS_PATH);

    let (_output_file, params_file, time_file, _sim_file) = cfg.gen_standard_names();
    let perf_stat_file = dir!(&results_dir, cfg.gen_file_name("perf_stat"));
    let perf_record_file = "/tmp/perf.data";
    let mm_fault_file = dir!(&results_dir, cfg.gen_file_name("mm_fault"));
    let mmap_tracker_file = dir!(&results_dir, cfg.gen_file_name("mmap_tracker"));
    let flame_graph_file = dir!(&results_dir, cfg.gen_file_name("flamegraph.svg"));
    let smaps_file = dir!(&results_dir, cfg.gen_file_name("smaps"));
    let tmmfs_stats_periodic_file = dir!(&results_dir, cfg.gen_file_name("tmmfs_stats_periodic"));
    let tmmfs_active_list_periodic_file =
        dir!(&results_dir, cfg.gen_file_name("tmmfs_active_list"));
    let lock_stat_file = dir!(&results_dir, cfg.gen_file_name("lock_stat"));
    let gups_file = dir!(&results_dir, cfg.gen_file_name("gups"));
    let coherence_file = dir!(&results_dir, cfg.gen_file_name("coherence"));
    let alloc_test_file = dir!(&results_dir, cfg.gen_file_name("alloctest"));
    let ycsb_file = dir!(&results_dir, cfg.gen_file_name("ycsb"));
    let runtime_file = dir!(&results_dir, cfg.gen_file_name("runtime"));
    let tieredmmfs_stats_file = dir!(&results_dir, cfg.gen_file_name("tieredmmfs_stats"));
    let vmstat_file = dir!(&results_dir, cfg.gen_file_name("vmstat"));
    let graph500_file = dir!(&results_dir, cfg.gen_file_name("graph500"));
    let stream_file = dir!(&results_dir, cfg.gen_file_name("stream"));
    let badger_trap_file = dir!(&results_dir, cfg.gen_file_name("badger_trap"));
    let fbmm_stats_file = dir!(&results_dir, cfg.gen_file_name("fbmm_stats"));

    let bmks_dir = dir!(&user_home, crate::RESEARCH_WORKSPACE_PATH, crate::BMKS_PATH);
    let gups_dir = dir!(&bmks_dir, "gups/");
    let coherence_dir = dir!(&bmks_dir, "pagewalk_coherence/");
    let ycsb_dir = dir!(&bmks_dir, "YCSB");
    let memcached_dir = dir!(&bmks_dir, "memcached/");
    let graph500_dir = dir!(&bmks_dir, "graph500/src/");
    let scripts_dir = dir!(
        &user_home,
        crate::RESEARCH_WORKSPACE_PATH,
        crate::SCRIPTS_PATH
    );
    let spec_dir = dir!(&bmks_dir, crate::SPEC2017_PATH);
    let parsec_dir = dir!(&user_home, crate::PARSEC_PATH);

    // Setup the pmem settings in the grub config before rebooting
    // First, clear the memmap and tpp options from the boot options
    ushell.run(cmd!("cat /etc/default/grub"))?;
    ushell.run(cmd!(
        r#"sed 's/ memmap=[0-9]*[KMG]![0-9]*[KMG]//g' \
        /etc/default/grub | sed 's/ do_tpp//g' | sed 's/ maxcpus=[0-9]*//g' | \
        sudo tee /tmp/grub"#
    ))?;
    ushell.run(cmd!("sudo mv /tmp/grub /etc/default/grub"))?;
    // Then, if we are doing an experiment where we reserve RAM, add it in
    if let Some(dram) = &cfg.dram_region {
        if let Some(pmem) = &cfg.pmem_region {
            ushell.run(cmd!(
                r#"sed 's/GRUB_CMDLINE_LINUX="\(.*\)"/GRUB_CMDLINE_LINUX="\1 memmap={}G!{}G memmap={}G!{}G"/' \
                /etc/default/grub | sudo tee /tmp/grub"#,
                dram.size, dram.start, pmem.size, pmem.start
            ))?;
            ushell.run(cmd!("sudo mv /tmp/grub /etc/default/grub"))?;
        } else {
            ushell.run(cmd!(
                r#"sed 's/GRUB_CMDLINE_LINUX="\(.*\)"/GRUB_CMDLINE_LINUX="\1 memmap={}G!{}G"/' \
                /etc/default/grub | sudo tee /tmp/grub"#,
                dram.size,
                dram.start
            ))?;
            ushell.run(cmd!("sudo mv /tmp/grub /etc/default/grub"))?;
        }
    }
    // If we are doing an experiment using tpp, add in the option to setup the tiering
    // If a node has compute, it will be considered toptier, so restrict the CPUs too
    if cfg.tpp {
        ushell.run(cmd!(
            r#"sed 's/GRUB_CMDLINE_LINUX="\(.*\)"/GRUB_CMDLINE_LINUX="\1 do_tpp maxcpus=8"/' \
            /etc/default/grub | sudo tee /tmp/grub"#
        ))?;
        ushell.run(cmd!("sudo mv /tmp/grub /etc/default/grub"))?;
    }

    // Finally, update the grub config
    ushell.run(cmd!("sudo update-grub2"))?;

    let ushell = connect_and_setup_host(login)?;

    if let Some(hugetlb_size_gb) = &cfg.hugetlb {
        // There are 512 huge pages per GB
        let num_pages = hugetlb_size_gb * 1024 / 2;
        ushell.run(cmd!("sudo hugeadm --pool-pages-min 2MB:{}", num_pages))?;
        // Print out the huge page reservations for the log
        ushell.run(cmd!("hugeadm --pool-list"))?;
    }

    ushell.run(cmd!(
        "echo {} > {}",
        escape_for_bash(&serde_json::to_string(&cfg)?),
        dir!(&results_dir, params_file)
    ))?;

    let mut cmd_prefix = String::new();
    let proc_name = match &cfg.workload {
        Workload::AllocTest { .. } => "alloc_test",
        Workload::Canneal { workload: _ } => "canneal",
        Workload::Spec2017Mcf => "mcf_s",
        Workload::Spec2017Xalancbmk => "xalancbmk_s",
        Workload::Spec2017Xz { size: _ } => "xz_s",
        Workload::Spec2017CactuBSSN => "cactuBSSN_s",
        Workload::Gups { .. } => "gups",
        Workload::PagewalkCoherence { .. } => "paging",
        Workload::Memcached { .. } => "memcached",
        Workload::Graph500 { .. } => "graph500_refere",
        Workload::Stream { .. } => "stream",
    };

    let (
        transparent_hugepage_enabled,
        transparent_hugepage_defrag,
        transparent_hugepage_khugepaged_defrag,
    ) = if cfg.disable_thp {
        ("never".into(), "never".into(), 0)
    } else {
        ("always".into(), "always".into(), 1)
    };
    libscail::turn_on_thp(
        &ushell,
        transparent_hugepage_enabled,
        transparent_hugepage_defrag,
        transparent_hugepage_khugepaged_defrag,
        1000,
        1000,
    )?;

    if cfg.disable_aslr {
        libscail::disable_aslr(&ushell)?;
    } else {
        libscail::enable_aslr(&ushell)?;
    }

    let mut tctx = match &cfg.workload {
        Workload::Memcached { .. } | Workload::Gups { .. } |Workload::Stream { .. } => TasksetCtxBuilder::from_lscpu(&ushell)?
            .numa_interleaving(TasksetCtxInterleaving::Sequential)
            .skip_hyperthreads(true)
            .build(),
        Workload::AllocTest { .. } | Workload::Spec2017CactuBSSN => TasksetCtxBuilder::from_lscpu(&ushell)?
            .numa_interleaving(TasksetCtxInterleaving::Sequential)
            .skip_hyperthreads(false)
            .build(),
        _ => {
            let cores = libscail::get_num_cores(&ushell)?;
            TasksetCtxBuilder::simple(cores).build()
        }
    };

    // Figure out which cores we will use for the workload
    let num_pin_cores = match &cfg.workload {
        Workload::Spec2017Mcf | Workload::Spec2017Xz { .. } | Workload::Spec2017Xalancbmk => 4,
        Workload::Spec2017CactuBSSN => 16,
        Workload::Gups { threads, .. } | Workload::AllocTest { threads, .. } | Workload::Stream { threads } => *threads,
        _ => 1,
    };
    let mut pin_cores = Vec::<usize>::new();
    for _ in 0..num_pin_cores {
        if let Ok(new_core) = tctx.next() {
            pin_cores.push(new_core);
        } else {
            return Err(std::fmt::Error.into());
        }
    }

    let pin_cores_str = pin_cores
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");
    if cfg.perf_stat {
        let mut extra_args = format!(" -C {} ", &pin_cores_str);

        if cfg.perf_periodic {
            // Times 1000 because PERIOD is in seconds, and -I takes ms
            extra_args.push_str(format!(" -I {} ", PERIOD * 1000).as_str());
        }

        cmd_prefix.push_str(&gen_perf_command_prefix(
            perf_stat_file,
            &cfg.perf_counters,
            extra_args,
        ));
    }

    if cfg.flame_graph {
        cmd_prefix.push_str(&format!(
            "sudo perf record -a -C {} -g -F 1999 -o {} ",
            &pin_cores_str, &perf_record_file
        ));
    }

    let mut bgctx = BackgroundContext::new(&ushell);
    if cfg.smaps_periodic {
        bgctx.spawn(BackgroundTask {
            name: "smaps",
            period: PERIOD,
            cmd: format!(
                "((sudo cat /proc/`pgrep -x {}  | sort -n \
                    | head -n1`/smaps) || echo none) | tee -a {}",
                &proc_name, &smaps_file
            ),
            ensure_started: smaps_file,
        })?;
    }

    if cfg.tmmfs_stats_periodic {
        bgctx.spawn(BackgroundTask {
            name: "tieredmmfs_stats",
            period: PERIOD,
            cmd: format!(
                "(cat /sys/fs/tieredmmfs/stats || echo wait) | tee -a {}",
                &tmmfs_stats_periodic_file
            ),
            ensure_started: tmmfs_stats_periodic_file,
        })?;
    }

    if cfg.tmmfs_active_list_periodic {
        bgctx.spawn(BackgroundTask {
            name: "tieredmmfs_active_list",
            period: PERIOD * 3, // This is a lot of data, so *3 to limit collection
            cmd: format!(
                "(cat /sys/fs/tieredmmfs/active_list || echo wait) | tee -a {}",
                &tmmfs_active_list_periodic_file
            ),
            ensure_started: tmmfs_active_list_periodic_file,
        })?;
    }

    if cfg.numactl {
        cmd_prefix.push_str("numactl --membind=0 ");
    }

    if cfg.lock_stat {
        // Enable collection of statistic
        ushell.run(cmd!("echo 1 | sudo tee /proc/sys/kernel/lock_stat"))?;
        // Clear the existing stats is there are any
        ushell.run(cmd!("echo 0 | sudo tee /proc/lock_stat"))?;
    }

    if let Some(fs) = &cfg.fbmm {
        cmd_prefix.push_str(&format!(
            "sudo {}/fbmm_wrapper \"{}/daxtmp/\" ",
            bmks_dir, user_home
        ));

        // Set up the remote for FOM
        ushell.run(cmd!("mkdir -p ./daxtmp/"))?;

        match fs {
            MMFS::Ext4 { .. } => {
                ushell.run(cmd!("sudo mkfs.ext4 /dev/pmem0"))?;
                ushell.run(cmd!("sudo tune2fs -O ^has_journal /dev/pmem0"))?;
                if !cfg.ext4_metadata {
                    ushell.run(cmd!("sudo tune2fs -O ^metadata_csum /dev/pmem0"))?;
                }
                ushell.run(cmd!("sudo mount -o dax /dev/pmem0 daxtmp/"))?;
            }
            MMFS::BasicMMFS { num_pages } => {
                ushell.run(cmd!(
                    "sudo insmod {}/BasicMMFS/basicmmfs.ko",
                    crate::KERNEL_PATH
                ))?;
                ushell.run(cmd!(
                    "sudo mount -t BasicMMFS BasicMMFS -o numpages={} daxtmp/",
                    num_pages,
                ))?;
            }
            MMFS::TieredMMFS { .. } => {
                ushell.run(cmd!(
                    "sudo insmod {}/TieredMMFS/tieredmmfs.ko",
                    crate::KERNEL_PATH
                ))?;
                ushell.run(cmd!(
                    "sudo mount -t TieredMMFS -o slowmem=/dev/pmem1 -o basepage={} /dev/pmem0 daxtmp/",
                    cfg.disable_thp
                ))?;

                if let Some(interval) = cfg.migrate_task_int {
                    ushell.run(cmd!(
                        "echo {} | sudo tee /sys/fs/tieredmmfs/migrate_task_int",
                        interval
                    ))?;
                }
            }
            MMFS::ContigMMFS { .. } => {
                ushell.run(cmd!(
                    "sudo insmod {}/ContigMMFS/contigmmfs.ko",
                    crate::KERNEL_PATH
                ))?;

                ushell.run(cmd!("sudo mount -t ContigMMFS ContigMMFS daxtmp/"))?;
            }
            MMFS::BandwidthMMFS { .. } => {
                ushell.run(cmd!(
                    "sudo insmod {}/BandwidthMMFS/bandwidth.ko",
                    crate::KERNEL_PATH
                ))?;

                ushell.run(cmd!("sudo mount -t BandwidthMMFS BandwidthMMFS daxtmp/"))?;

                // Set the appropriate node weights
                for weight in &cfg.node_weights {
                    ushell.run(cmd!(
                        "echo {} | sudo tee /sys/fs/bwmmfs*/node{}/weight",
                        weight.weight,
                        weight.nid
                    ))?;
                }
            }
        }

        ushell.run(cmd!("sudo chown -R $USER daxtmp/"))?;
        ushell.run(cmd!("echo 1 | sudo tee /sys/kernel/mm/fbmm/state"))?;
    }

    if cfg.tpp {
        // Set the NUMA policy to TPP
        ushell.run(cmd!("sudo sysctl kernel.numa_balancing=2"))?;
        // Enable for NUMA demotion
        ushell.run(cmd!(
            "echo 1 | sudo tee /sys/kernel/mm/numa/demotion_enabled"
        ))?;

        if let Some(size) = cfg.numa_scan_size {
            ushell.run(cmd!(
                "echo {} | sudo tee /proc/sys/kernel/numa_balancing_scan_size_MB",
                size
            ))?;
        }
        if let Some(delay) = cfg.numa_scan_delay {
            ushell.run(cmd!(
                "echo {} | sudo tee /proc/sys/kernel/numa_balancing_scan_delay_ms",
                delay
            ))?;
        }
        if let Some(period) = cfg.numa_scan_period_min {
            ushell.run(cmd!(
                "echo {} | sudo tee /proc/sys/kernel/numa_balancing_scan_period_min_ms",
                period
            ))?;
        }
    } else {
        // These options are not in the TPP kernel
        if let Some(fault_size) = &cfg.pte_fault_size {
            ushell.run(cmd!(
                "echo {} | sudo tee /sys/kernel/mm/fbmm/pte_fault_size",
                fault_size
            ))?;
        }

        // Handle disabling optimizations if requested
        if cfg.thp_temporal_zero {
            ushell.run(cmd!(
                "echo 0 | sudo tee /sys/kernel/mm/fbmm/nt_huge_page_zero"
            ))?;
        }
        if cfg.no_fpm_fix {
            ushell.run(cmd!(
                "echo 0 | sudo tee /sys/kernel/mm/fbmm/follow_page_mask_fix"
            ))?;
        }
        if cfg.no_pmem_write_zeroes {
            ushell.run(cmd!(
                "echo 0 | sudo tee /sys/kernel/mm/fbmm/pmem_write_zeroes"
            ))?;
        }
        if cfg.track_pfn_insert {
            ushell.run(cmd!(
                "echo 1 | sudo tee /sys/kernel/mm/fbmm/track_pfn_insert"
            ))?;
        }
        if cfg.mark_inode_dirty {
            ushell.run(cmd!(
                "echo 1 | sudo tee /sys/kernel/mm/fbmm/mark_inode_dirty"
            ))?;
        }
        if cfg.no_prealloc {
            ushell.run(cmd!(
                "echo 0 | sudo tee /sys/kernel/mm/fbmm/prealloc_map_populate"
            ))?;
        }
    }

    // Badger trap will capture stats for anything "after" it in the command,
    // so it should be the last thing in the command prefix to only capture the
    // workload's staticstics
    if cfg.badger_trap {
        cmd_prefix.push_str(&format!("{}/badger-trap command ", bmks_dir));
    }

    // Start the mm_fault_tracker BPF script if requested
    let mmap_tracker_handle = if cfg.mmap_tracker {
        let spawn_handle = ushell.spawn(cmd!(
            "sudo {}/mmap_tracker.py -c {} | tee {}",
            &scripts_dir,
            &proc_name,
            &mmap_tracker_file,
        ))?;
        // Wait some time for the BPF validator to begin
        println!("Waiting for BPF validator to complete...");
        ushell.run(cmd!("sleep 10"))?;

        Some(spawn_handle)
    } else {
        None
    };

    let ycsb = if let Workload::Memcached {
        size,
        op_count,
        read_prop,
        update_prop,
    } = cfg.workload
    {
        // Empirically, this is the amount of bytes a single record takes
        const RECORD_SIZE: usize = 1350;
        // "size" is the size in GB on the cache, so take off a GB to add some wiggle room
        let record_count = ((size - 1) << 30) / RECORD_SIZE;
        let client_pin_core = if let Ok(core) = tctx.next() {
            Some(core)
        } else {
            None
        };
        let memcached_cfg = MemcachedWorkloadConfig {
            user: &login.username,
            memcached: &memcached_dir,
            server_size_mb: size << 10,
            wk_size_gb: size,
            output_file: None,
            pintool: None,
            cmd_prefix: Some(&cmd_prefix),
            mmu_perf: None,
            server_start_cb: |_| Ok(()),
            allow_oom: true,
            hugepages: !cfg.disable_thp,
            server_pin_core: Some(pin_cores[0]),
        };
        let ycsb_cfg = YcsbConfig {
            workload: YcsbWorkload::Custom {
                record_count,
                op_count,
                distribution: YcsbDistribution::Zipfian,
                read_prop,
                update_prop,
                insert_prop: 1.0 - read_prop - update_prop,
            },
            system: YcsbSystem::Memcached(memcached_cfg),
            client_pin_core: client_pin_core,
            ycsb_path: &ycsb_dir,
            ycsb_result_file: Some(&ycsb_file),
        };
        let mut ycsb = YcsbSession::new(ycsb_cfg);

        ycsb.start_and_load(&ushell)?;

        Some(ycsb)
    } else {
        None
    };

    // Start the mm_fault_tracker BPF script if requested
    let mm_fault_tracker_handle = if cfg.mm_fault_tracker {
        let spawn_handle = ushell.spawn(cmd!(
            "sudo {}/mm_fault_tracker.py -c {} | tee {}",
            &scripts_dir,
            &proc_name,
            &mm_fault_file
        ))?;
        // Wait some time for the BPF validator to begin
        println!("Waiting for BPF validator to complete...");
        ushell.run(cmd!("sleep 10"))?;

        Some(spawn_handle)
    } else {
        None
    };

    match cfg.workload {
        Workload::AllocTest { size, num_allocs, threads, populate } => {
            time!(timers, "Workload", {
                run_alloc_test(
                    &ushell,
                    &bmks_dir,
                    size,
                    num_allocs,
                    threads,
                    Some(&cmd_prefix),
                    &alloc_test_file,
                    &runtime_file,
                    &pin_cores_str,
                    populate,
                )?;
            });
        }

        Workload::Canneal { workload } => {
            time!(timers, "Workload", {
                run_canneal(
                    &ushell,
                    &parsec_dir,
                    workload,
                    Some(&cmd_prefix),
                    None,
                    &runtime_file,
                    pin_cores[0],
                )?;
            });
        }

        w @ Workload::Spec2017Mcf
        | w @ Workload::Spec2017Xz { size: _ }
        | w @ Workload::Spec2017Xalancbmk
        | w @ Workload::Spec2017CactuBSSN => {
            let wkload = match w {
                Workload::Spec2017Mcf => Spec2017Workload::Mcf,
                Workload::Spec2017Xz { size } => Spec2017Workload::Xz { size },
                Workload::Spec2017Xalancbmk => Spec2017Workload::Xalancbmk,
                Workload::Spec2017CactuBSSN => Spec2017Workload::CactuBSSN,
                _ => unreachable!(),
            };

            time!(timers, "Workload", {
                run_spec17(
                    &ushell,
                    &spec_dir,
                    wkload,
                    None,
                    Some(&cmd_prefix),
                    &runtime_file,
                    pin_cores,
                )?;
            });
        }

        Workload::Gups {
            threads,
            exp,
            hot_exp,
            move_hot,
            num_updates,
        } => {
            time!(timers, "Workload", {
                run_gups(
                    &ushell,
                    &gups_dir,
                    threads,
                    exp,
                    hot_exp,
                    move_hot,
                    num_updates,
                    Some(&cmd_prefix),
                    &gups_file,
                    &runtime_file,
                    &pin_cores_str,
                )?;
            });
        }

        Workload::PagewalkCoherence { mode } => {
            time!(timers, "Workload", {
                run_pagewalk_coherence(
                    &ushell,
                    &coherence_dir,
                    mode,
                    Some(&cmd_prefix),
                    &coherence_file,
                    &runtime_file,
                    pin_cores[0],
                )?;
            });
        }

        Workload::Memcached { .. } => {
            let mut ycsb = ycsb.unwrap();

            //Run the workload
            time!(timers, "Workload", ycsb.run(&ushell))?;

            // Make sure the server dies.
            ushell.run(cmd!("sudo pkill -INT memcached"))?;
            while let Ok(..) = ushell.run(cmd!(
                "{}/scripts/memcached-tool localhost:11211",
                memcached_dir
            )) {}
            std::thread::sleep(std::time::Duration::from_secs(20));
        }

        Workload::Graph500 { size } => {
            time!(timers, "Workload", {
                run_graph500(
                    &ushell,
                    &graph500_dir,
                    size,
                    Some(&cmd_prefix),
                    &graph500_file,
                    &runtime_file,
                    pin_cores[0],
                )?;
            });
        }

        Workload::Stream { .. } => {
            time!(timers, "Workload", {
                run_stream(
                    &ushell,
                    &bmks_dir,
                    Some(&cmd_prefix),
                    &stream_file,
                    &runtime_file,
                    &pin_cores_str,
                )?;
            })
        }
    }

    // If we are using FBMM, print some stats
    if let Some(fs) = &cfg.fbmm {
        ushell.run(cmd!("cat /sys/kernel/mm/fbmm/stats | tee {}", &fbmm_stats_file))?;

        match fs {
            // If we are using TieredMMFS, print some more stats
            MMFS::TieredMMFS { .. } => {
                ushell.run(cmd!(
                    "cat /sys/fs/tieredmmfs/stats | tee {}",
                    &tieredmmfs_stats_file
                ))?;
            }
            _ => {}
        }
    }

    ushell.run(cmd!("cat /proc/vmstat | tee {}", &vmstat_file))?;

    // Generate the flamegraph if needed
    if cfg.flame_graph {
        ushell.run(cmd!(
            "sudo perf script -i {} | ./FlameGraph/stackcollapse-perf.pl > /tmp/flamegraph",
            &perf_record_file,
        ))?;
        ushell.run(cmd!(
            "./FlameGraph/flamegraph.pl /tmp/flamegraph > {}",
            flame_graph_file
        ))?;
    }

    // Record the lock statistics if needed
    if cfg.lock_stat {
        ushell.run(cmd!(
            "sudo cat /proc/lock_stat | sudo tee {}",
            lock_stat_file
        ))?;
    }

    // Record the badger trap stats if needed
    if cfg.badger_trap {
        ushell.run(cmd!("dmesg | tail -n 10 | sudo tee {}", badger_trap_file))?;
    }

    // Clean up the mm_fault_tracker if it was started
    if let Some(handle) = mm_fault_tracker_handle {
        ushell.run(cmd!("sudo killall -SIGINT mm_fault_tracker.py"))?;
        handle.join().1?;
    }
    if let Some(handle) = mmap_tracker_handle {
        ushell.run(cmd!("sudo killall -SIGINT mmap_tracker.py"))?;
        handle.join().1?;
    }

    ushell.run(cmd!("date"))?;

    ushell.run(cmd!("free -h"))?;

    ushell.run(cmd!(
        "echo {} > {}",
        escape_for_bash(&libscail::timings_str(timers.as_slice())),
        dir!(&results_dir, time_file)
    ))?;

    let glob = cfg.gen_file_name("");
    println!("RESULTS: {}", dir!(&results_dir, glob));
    Ok(())
}

fn connect_and_setup_host<A>(login: &Login<A>) -> Result<SshShell, failure::Error>
where
    A: std::net::ToSocketAddrs + std::fmt::Display + std::fmt::Debug + Clone,
{
    let ushell = SshShell::with_any_key(login.username, &login.host)?;
    //    spurs_util::reboot(&mut ushell, /* dry_run */ false)?;
    let _ = ushell.run(cmd!("sudo reboot"));
    // It sometimes takes a few seconds for the reboot to actually happen,
    // so make sure we wait a bit for it.
    std::thread::sleep(std::time::Duration::from_secs(5));

    // Keep trying to connect until we succeed
    let ushell = {
        let mut shell;
        loop {
            println!("Attempting to reconnect...");
            shell = match SshShell::with_any_key(login.username, &login.host) {
                Ok(shell) => shell,
                Err(_) => {
                    std::thread::sleep(std::time::Duration::from_secs(10));
                    continue;
                }
            };
            match shell.run(cmd!("whoami")) {
                Ok(_) => break,
                Err(_) => {
                    std::thread::sleep(std::time::Duration::from_secs(10));
                    continue;
                }
            }
        }

        shell
    };

    dump_sys_info(&ushell)?;

    ushell.run(cmd!(
        "sudo LD_LIBRARY_PATH=/usr/lib64/ cpupower frequency-set -g performance",
    ))?;
    ushell.run(cmd!("lscpu"))?;
    set_kernel_printk_level(&ushell, 5)?;

    Ok(ushell)
}

fn run_alloc_test(
    ushell: &SshShell,
    bmks_dir: &str,
    size: usize,
    num_allocs: usize,
    threads: usize,
    cmd_prefix: Option<&str>,
    alloc_test_file: &str,
    runtime_file: &str,
    pin_cores_str: &str,
    use_map_populate: bool,
) -> Result<(), failure::Error> {
    // alloc_test uses MAP_POPULATE if it has a fourth arg
    let populate_arg = if use_map_populate { "populate" } else { "" };

    let start = Instant::now();
    ushell.run(
        cmd!(
            "sudo taskset -c {} {} ./alloc_test {} {} {} {} | sudo tee {}",
            pin_cores_str,
            cmd_prefix.unwrap_or(""),
            size,
            num_allocs,
            threads,
            populate_arg,
            alloc_test_file
        )
        .cwd(bmks_dir),
    )?;
    let duration = Instant::now() - start;

    ushell.run(cmd!("echo {} > {}", duration.as_millis(), runtime_file))?;
    Ok(())
}

fn run_gups(
    ushell: &SshShell,
    gups_dir: &str,
    threads: usize,
    exp: usize,
    hot_exp: Option<usize>,
    move_hot: bool,
    num_updates: usize,
    cmd_prefix: Option<&str>,
    gups_file: &str,
    runtime_file: &str,
    pin_cores_str: &str,
) -> Result<(), failure::Error> {
    let start = Instant::now();

    if let Some(hot_exp) = hot_exp {
        ushell.run(
            cmd!(
                "sudo taskset -c {} {} ./gups-hotset-move {} {} {} 8 {} {} | tee {}",
                pin_cores_str,
                cmd_prefix.unwrap_or(""),
                threads,
                num_updates,
                exp,
                hot_exp,
                if move_hot { 1 } else { 0 },
                gups_file,
            )
            .cwd(gups_dir),
        )?;
    } else {
        ushell.run(
            cmd!(
                "sudo taskset -c {} {} ./gups {} {} {} 8 | tee {}",
                pin_cores_str,
                cmd_prefix.unwrap_or(""),
                threads,
                num_updates,
                exp,
                gups_file,
            )
            .cwd(gups_dir),
        )?;
    }
    let duration = Instant::now() - start;

    ushell.run(cmd!("echo {} > {}", duration.as_millis(), runtime_file))?;
    Ok(())
}

fn run_pagewalk_coherence(
    ushell: &SshShell,
    coherence_dir: &str,
    mode: PagewalkCoherenceMode,
    cmd_prefix: Option<&str>,
    coherence_file: &str,
    runtime_file: &str,
    pin_core: usize,
) -> Result<(), failure::Error> {
    // Building this ubmks requires the kernel to be built, so we build it now
    // instead of during setup
    ushell.run(cmd!("make").cwd(coherence_dir))?;
    ushell.run(cmd!("sudo insmod ./pgmod.ko").cwd(coherence_dir))?;

    let start = Instant::now();
    ushell.run(
        cmd!(
            "sudo taskset -c {} {} ./paging --mode {} | tee {}",
            pin_core,
            cmd_prefix.unwrap_or(""),
            match mode {
                PagewalkCoherenceMode::Speculation => 0,
                PagewalkCoherenceMode::Coherence => 1,
            },
            coherence_file,
        )
        .cwd(coherence_dir),
    )?;
    let duration = Instant::now() - start;

    ushell.run(cmd!("echo {} > {}", duration.as_millis(), runtime_file))?;

    Ok(())
}

fn run_graph500(
    ushell: &SshShell,
    graph500_dir: &str,
    size: usize,
    cmd_prefix: Option<&str>,
    graph500_file: &str,
    runtime_file: &str,
    pin_core: usize,
) -> Result<(), failure::Error> {
    let start = Instant::now();

    ushell.run(
        cmd!(
            "sudo taskset -c {} {} ./graph500_reference_bfs_sssp {} | tee {}",
            pin_core,
            cmd_prefix.unwrap_or(""),
            size,
            graph500_file
        )
        .cwd(graph500_dir),
    )?;

    let duration = Instant::now() - start;
    ushell.run(cmd!("echo {} > {}", duration.as_millis(), runtime_file))?;

    Ok(())
}

fn run_stream(
    ushell: &SshShell,
    bmks_dir: &str,
    cmd_prefix: Option<&str>,
    stream_file: &str,
    runtime_file: &str,
    pin_cores_str: &str,
) -> Result<(), failure::Error> {
    let start = Instant::now();

    ushell.run(
        cmd!(
            "sudo taskset -c {} {} ./stream | tee {}",
            pin_cores_str,
            cmd_prefix.unwrap_or(""),
            stream_file
        )
        .cwd(bmks_dir),
    )?;

    let duration = Instant::now() - start;
    ushell.run(cmd!("echo {} > {}", duration.as_millis(), runtime_file))?;

    Ok(())
}
