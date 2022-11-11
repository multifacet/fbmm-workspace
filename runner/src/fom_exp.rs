use clap::clap_app;

use libscail::{
    background::{BackgroundContext, BackgroundTask},
    dir, dump_sys_info, get_user_home_dir,
    output::{Parametrize, Timestamp},
    set_kernel_printk_level, time, validator,
    workloads::{
        gen_perf_command_prefix, run_canneal, run_spec17, CannealWorkload, MemcachedWorkloadConfig,
        Spec2017Workload, TasksetCtx, YcsbConfig, YcsbDistribution, YcsbSession, YcsbSystem,
        YcsbWorkload,
    },
    Login,
};

use serde::{Deserialize, Serialize};

use spurs::{cmd, Execute, SshShell};
use spurs_util::escape_for_bash;
use std::time::Instant;

pub const PERIOD: usize = 10; // seconds

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
enum Workload {
    Spec2017Mcf,
    Spec2017Xalancbmk,
    Spec2017Xz,
    Canneal {
        workload: CannealWorkload,
    },
    AllocTest {
        size: usize,
        num_allocs: usize,
    },
    Gups {
        exp: usize,
        num_updates: usize,
    },
    Memcached {
        size: usize,
        op_count: usize,
        read_prop: f32,
        update_prop: f32,
    },
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
enum FomFS {
    Ext4,
    FOMTierFS,
}

#[derive(Debug, Clone, Serialize, Deserialize, Parametrize)]
struct Config {
    #[name]
    exp: String,

    #[name]
    workload: Workload,

    perf_stat: bool,
    perf_counters: Vec<String>,
    disable_thp: bool,
    disable_aslr: bool,
    mm_fault_tracker: bool,
    flame_graph: bool,
    smaps_periodic: bool,
    fom: Option<FomFS>,
    dram_size: usize,
    pmem_size: usize,
    hugetlb: Option<usize>,
    pte_fault_size: usize,

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
    clap_app! { fom_exp =>
        (about: "Run file only memory experiments. Requires `sudo`.")
        (@setting ArgRequiredElseHelp)
        (@setting DisableVersion)
        (@arg HOSTNAME: +required +takes_value
         "The domain name of the remote")
        (@arg USERNAME: +required +takes_value
         "The username on the remote")
        (@subcommand alloctest =>
            (about: "Run the `alloctest` workload.")
            (@arg SIZE: +required +takes_value {validator::is::<usize>}
             "The number of GBs of memory to use")
            (@arg NUM_ALLOCS: +takes_value {validator::is::<usize>}
             "The number of calls to mmap to do")
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
        )
        (@subcommand gups =>
            (about: "Run the GUPS workload used to eval HeMem")
            (@arg EXP: +required +takes_value {validator::is::<usize>}
             "The log of the size of the workload.")
            (@arg NUM_UPDATES: +takes_value {validator::is::<usize>}
             "The number of updates to do. Default is 2^exp / 8")
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
        (@arg PERF_STAT: --perf_stat
         "Attach perf stat to the workload.")
        (@arg PERF_COUNTER: --perf_counter +takes_value ... number_of_values(1)
         requires[PERF_STAT]
         "Which counters to record with perf stat.")
        (@arg DISABLE_THP: --disable_thp
         "Disable THP completely.")
        (@arg DISABLE_ASLR: --disable_aslr
         "Disable ASLR.")
        (@arg MM_FAULT_TRACKER: --mm_fault_tracker
         "Record page fault statistics with mm_fault_tracker.")
        (@arg FLAME_GRAPH: --flame_graph
         "Generate a flame graph of the workload.")
        (@arg SMAPS_PERIODIC: --smaps_periodic
         "Collect /proc/[PID]/smaps data periodically for the workload process")
        (@arg FOM: --fom +takes_value
         requires[DRAM_SIZE] conflicts_with[HUGETLB]
         "Run the workload with file only memory with the specified FS (either ext4 or FOMTierFS).")
        (@arg DRAM_SIZE: --dram_size +takes_value {validator::is::<usize>}
         "(Optional) If passed, reserved the specifies amount of memory in GB as DRAM.")
        (@arg PMEM_SIZE: --pmem_size +takes_value {validator::is::<usize>}
         "(Optional) If passed, reserved the specified amount of memory in GB as PMEM.")
        (@arg HUGETLB: --hugetlb +takes_value {validator::is::<usize>}
         conflicts_with[FOM]
         "Run certain workloads with libhugetlbfs. Specify the number of huge pages to reserve in GB")
        (@arg PTE_FAULT_SIZE: --pte_fault_size +takes_value {validator::is::<usize>}
         "The number of pages to allocate on a DAX pte fault.")
        (@arg THP_TEMPORAL_ZERO: --thp_temporal_zero
         conflicts_with[FOM] conflicts_with[DISABLE_THP]
         "Tell the kernel to use the standard erms zeroing for huge pages.")
        (@arg NO_FPM_FIX: --no_fpm_fix
         "Tell the kernel to ignore the optimization to the follow_page_mask function for FOM.")
        (@arg NO_PMEM_WRITE_ZEROES: --no_pmem_write_zeroes
         "Tell the kernels not to zero FOM pages by copying the zero page.")
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
            Workload::AllocTest { size, num_allocs }
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

        ("spec17", Some(sub_m)) => match sub_m.value_of("WHICH").unwrap() {
            "mcf" => Workload::Spec2017Mcf,
            "xalancbmk" => Workload::Spec2017Xalancbmk,
            "xz" => Workload::Spec2017Xz,
            _ => panic!("Unknown spec workload"),
        },

        ("gups", Some(sub_m)) => {
            let exp = sub_m.value_of("EXP").unwrap().parse::<usize>().unwrap();
            let num_updates = if let Some(updates_str) = sub_m.value_of("NUM_UPDATES") {
                updates_str.parse::<usize>().unwrap()
            } else {
                (2 << exp) / 8
            };
            Workload::Gups { exp, num_updates }
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

        _ => unreachable!(),
    };

    let perf_stat = sub_m.is_present("PERF_STAT");
    let disable_thp = sub_m.is_present("DISABLE_THP");
    let disable_aslr = sub_m.is_present("DISABLE_ASLR");
    let mm_fault_tracker = sub_m.is_present("MM_FAULT_TRACKER");
    let flame_graph = sub_m.is_present("FLAME_GRAPH");
    let smaps_periodic = sub_m.is_present("SMAPS_PERIODIC");
    let fom = sub_m.value_of("FOM").map(|fs| {
        if fs == "ext4" {
            FomFS::Ext4
        } else if fs == "FOMTierFS" {
            FomFS::FOMTierFS
        } else {
            panic!("Invalid FOM file system: {fs}");
        }
    });
    let dram_size = sub_m
        .value_of("DRAM_SIZE")
        .unwrap_or("0")
        .parse::<usize>()
        .unwrap();
    let pmem_size = sub_m
        .value_of("PMEM_SIZE")
        .unwrap_or("0")
        .parse::<usize>()
        .unwrap();
    let hugetlb = sub_m
        .value_of("HUGETLB")
        .map(|huge_size| huge_size.parse::<usize>().unwrap());
    let pte_fault_size = sub_m
        .value_of("PTE_FAULT_SIZE")
        .unwrap_or("1")
        .parse::<usize>()
        .unwrap();
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
        perf_counters,
        disable_thp,
        disable_aslr,
        mm_fault_tracker,
        flame_graph,
        smaps_periodic,
        fom,
        dram_size,
        pmem_size,
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

    let cores = libscail::get_num_cores(&ushell)?;
    let mut tctx = TasksetCtx::new(cores);

    // Setup the output file name
    let results_dir = dir!(&user_home, crate::RESULTS_PATH);

    let (_output_file, params_file, time_file, _sim_file) = cfg.gen_standard_names();
    let perf_stat_file = dir!(&results_dir, cfg.gen_file_name("perf_stat"));
    let perf_record_file = "/tmp/perf.data";
    let mm_fault_file = dir!(&results_dir, cfg.gen_file_name("mm_fault"));
    let flame_graph_file = dir!(&results_dir, cfg.gen_file_name("flamegraph.svg"));
    let smaps_file = dir!(&results_dir, cfg.gen_file_name("smaps"));
    let gups_file = dir!(&results_dir, cfg.gen_file_name("gups"));
    let alloc_test_file = dir!(&results_dir, cfg.gen_file_name("alloctest"));
    let ycsb_file = dir!(&results_dir, cfg.gen_file_name("ycsb"));
    let runtime_file = dir!(&results_dir, cfg.gen_file_name("runtime"));

    let bmks_dir = dir!(&user_home, crate::RESEARCH_WORKSPACE_PATH, crate::BMKS_PATH);
    let gups_dir = dir!(&bmks_dir, "gups/");
    let ycsb_dir = dir!(&bmks_dir, "YCSB");
    let memcached_dir = dir!(&bmks_dir, "memcached/");
    let scripts_dir = dir!(
        &user_home,
        crate::RESEARCH_WORKSPACE_PATH,
        crate::SCRIPTS_PATH
    );
    let spec_dir = dir!(&bmks_dir, crate::SPEC2017_PATH);
    let parsec_dir = dir!(&user_home, crate::PARSEC_PATH);

    // Setup the pmem settings in the grub config before rebooting
    // First, clear the memmap option from the boot options
    ushell.run(cmd!("cat /etc/default/grub"))?;
    ushell.run(cmd!(
        r#"sed 's/ memmap=[0-9]*[KMG]![0-9]*[KMG]//g' \
        /etc/default/grub | sudo tee /etc/default/grub"#
    ))?;
    // Then, if we are doing a pmem experiment, add it in
    if let Some(fs) = &cfg.fom {
        match fs {
            FomFS::Ext4 => {
                ushell.run(cmd!(
                    r#"sed 's/GRUB_CMDLINE_LINUX="\(.*\)"/GRUB_CMDLINE_LINUX="\1 memmap={}G!4G"/' \
                    /etc/default/grub | sudo tee /etc/default/grub"#,
                    cfg.dram_size
                ))?;
            }
            FomFS::FOMTierFS => {
                ushell.run(cmd!(
                    r#"sed 's/GRUB_CMDLINE_LINUX="\(.*\)"/GRUB_CMDLINE_LINUX="\1 memmap={}G!4G memmap={}G!{}G"/' \
                    /etc/default/grub | sudo tee /etc/default/grub"#,
                    cfg.dram_size, cfg.pmem_size, 4 + cfg.dram_size
                ))?;
            }
        }
    }
    // Finally, update the grub config
    ushell.run(cmd!("sudo update-grub2"))?;

    let ushell = connect_and_setup_host(login)?;

    let use_hugetlb = if let Some(hugetlb_size_gb) = &cfg.hugetlb {
        // There are 512 huge pages per GB
        let num_pages = hugetlb_size_gb * 1024 / 2;
        ushell.run(cmd!("sudo hugeadm --pool-pages-min 2MB:{}", num_pages))?;
        // Print out the huge page reservations for the log
        ushell.run(cmd!("hugeadm --pool-list"))?;

        true
    } else {
        false
    };

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
        Workload::Spec2017Xz => "xz_s",
        Workload::Gups { .. } => "gups",
        Workload::Memcached { .. } => "memcached",
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

    // Figure out which cores we will use for the workload
    let pin_cores = match &cfg.workload {
        Workload::Spec2017Mcf | Workload::Spec2017Xz | Workload::Spec2017Xalancbmk => {
            vec![tctx.next(), tctx.next(), tctx.next(), tctx.next()]
        }
        _ => vec![tctx.next()],
    };

    if cfg.perf_stat {
        cmd_prefix.push_str(&gen_perf_command_prefix(
            perf_stat_file,
            &cfg.perf_counters,
            "",
        ));
    }

    if cfg.flame_graph {
        let pin_cores_str = pin_cores
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        cmd_prefix.push_str(&format!(
            "sudo perf record -a -C {} -g -F 99 -o {} ",
            pin_cores_str, &perf_record_file
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

    if let Some(fs) = &cfg.fom {
        cmd_prefix.push_str(&format!("sudo {}/fom_wrapper ", bmks_dir));

        // Set up the remote for FOM
        ushell.run(cmd!("mkdir -p ./daxtmp/"))?;
        ushell.run(cmd!("sudo chown -R $USER daxtmp/"))?;

        match fs {
            FomFS::Ext4 => {
                ushell.run(cmd!("sudo mkfs.ext4 /dev/pmem0"))?;
                ushell.run(cmd!("sudo tune2fs -O ^has_journal /dev/pmem0"))?;
                if !cfg.ext4_metadata {
                    ushell.run(cmd!("sudo tune2fs -O ^metadata_csum /dev/pmem0"))?;
                }
                ushell.run(cmd!("sudo mount -o dax /dev/pmem0 daxtmp/"))?;
            }
            FomFS::FOMTierFS => {
                ushell.run(cmd!(
                    "sudo insmod {}/FOMTierFS/fomtierfs.ko",
                    crate::KERNEL_PATH
                ))?;
                ushell.run(cmd!(
                    "sudo mount -t FOMTierFS -o slowmem=/dev/pmem1 -o basepage={} /dev/pmem0 daxtmp/",
                    cfg.disable_thp
                ))?;
            }
        }

        ushell.run(cmd!(
            "echo \"{}/daxtmp/\" | sudo tee /sys/kernel/mm/fom/file_dir",
            &user_home
        ))?;
        ushell.run(cmd!("echo 1 | sudo tee /sys/kernel/mm/fom/state"))?;
    }

    ushell.run(cmd!(
        "echo {} | sudo tee /sys/kernel/mm/fom/pte_fault_size",
        cfg.pte_fault_size
    ))?;

    // Handle disabling optimizations if requested
    if cfg.thp_temporal_zero {
        ushell.run(cmd!(
            "echo 0 | sudo tee /sys/kernel/mm/fom/nt_huge_page_zero"
        ))?;
    }
    if cfg.no_fpm_fix {
        ushell.run(cmd!(
            "echo 0 | sudo tee /sys/kernel/mm/fom/follow_page_mask_fix"
        ))?;
    }
    if cfg.no_pmem_write_zeroes {
        ushell.run(cmd!(
            "echo 0 | sudo tee /sys/kernel/mm/fom/pmem_write_zeroes"
        ))?;
    }
    if cfg.track_pfn_insert {
        ushell.run(cmd!(
            "echo 1 | sudo tee /sys/kernel/mm/fom/track_pfn_insert"
        ))?;
    }
    if cfg.mark_inode_dirty {
        ushell.run(cmd!(
            "echo 1 | sudo tee /sys/kernel/mm/fom/mark_inode_dirty"
        ))?;
    }
    if cfg.no_prealloc {
        ushell.run(cmd!(
            "echo 0 | sudo tee /sys/kernel/mm/fom/prealloc_map_populate"
        ))?;
    }

    let ycsb = if let Workload::Memcached {
        size,
        op_count,
        read_prop,
        update_prop,
    } = cfg.workload
    {
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
            client_pin_core: 0,
        };
        let ycsb_cfg = YcsbConfig {
            workload: YcsbWorkload::Custom {
                record_count: op_count,
                op_count,
                distribution: YcsbDistribution::Uniform,
                read_prop,
                update_prop,
                insert_prop: 1.0 - read_prop - update_prop,
            },
            system: YcsbSystem::Memcached(memcached_cfg),
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
        Workload::AllocTest { size, num_allocs } => {
            time!(timers, "Workload", {
                run_alloc_test(
                    &ushell,
                    &bmks_dir,
                    size,
                    num_allocs,
                    Some(&cmd_prefix),
                    &alloc_test_file,
                    &runtime_file,
                    pin_cores[0],
                    use_hugetlb,
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

        w @ Workload::Spec2017Mcf | w @ Workload::Spec2017Xz | w @ Workload::Spec2017Xalancbmk => {
            let wkload = match w {
                Workload::Spec2017Mcf => Spec2017Workload::Mcf,
                Workload::Spec2017Xz => Spec2017Workload::Xz { size: 0 },
                Workload::Spec2017Xalancbmk => Spec2017Workload::Xalancbmk,
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

        Workload::Gups { exp, num_updates } => {
            time!(timers, "Workload", {
                run_gups(
                    &ushell,
                    &gups_dir,
                    exp,
                    num_updates,
                    Some(&cmd_prefix),
                    &gups_file,
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
    }

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

    // Clean up the mm_fault_tracker if it was started
    if let Some(handle) = mm_fault_tracker_handle {
        ushell.run(cmd!("sudo killall -SIGINT mm_fault_tracker.py"))?;
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
    cmd_prefix: Option<&str>,
    alloc_test_file: &str,
    runtime_file: &str,
    pin_core: usize,
    use_hugetlb: bool,
) -> Result<(), failure::Error> {
    // alloc_test uses MAP_HUGETLB is it has a third arg
    let hugetlb_arg = if use_hugetlb { "hugetlb" } else { "" };

    let start = Instant::now();
    ushell.run(
        cmd!(
            "sudo taskset -c {} {} ./alloc_test {} {} {} | sudo tee {}",
            pin_core,
            cmd_prefix.unwrap_or(""),
            size,
            num_allocs,
            hugetlb_arg,
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
    exp: usize,
    num_updates: usize,
    cmd_prefix: Option<&str>,
    gups_file: &str,
    runtime_file: &str,
    pin_core: usize,
) -> Result<(), failure::Error> {
    let start = Instant::now();
    ushell.run(
        cmd!(
            "sudo taskset -c {} {} ./gups 1 {} {} 8 | tee {}",
            pin_core,
            cmd_prefix.unwrap_or(""),
            num_updates,
            exp,
            gups_file,
        )
        .cwd(gups_dir),
    )?;
    let duration = Instant::now() - start;

    ushell.run(cmd!("echo {} > {}", duration.as_millis(), runtime_file))?;
    Ok(())
}
