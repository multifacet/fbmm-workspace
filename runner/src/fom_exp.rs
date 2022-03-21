use clap::clap_app;

use libscail::{
    dir, dump_sys_info, set_kernel_printk_level,
    get_user_home_dir,
    output::{Parametrize, Timestamp},
    time, Login,
    workloads::{
        run_canneal, run_spec17, CannealWorkload, Spec2017Workload,
        gen_perf_command_prefix, TasksetCtx,
    },
    validator,
};

use serde::{Deserialize, Serialize};

use spurs::{cmd, Execute, SshShell};
use spurs_util::escape_for_bash;
use std::time::Instant;

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
    },
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
    fom: bool,
    pte_fault_size: usize,

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
        (@arg FOM: --fom
         "Run the workload with file only memory.")
        (@arg PTE_FAULT_SIZE: --pte_fault_size +takes_value {validator::is::<usize>}
         "The number of pages to allocate on a DAX pte fault.")
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
            Workload::AllocTest { size }
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
            match sub_m.value_of("WHICH").unwrap() {
                "mcf" => Workload::Spec2017Mcf,
                "xalancbmk" => Workload::Spec2017Xalancbmk,
                "xz" => Workload::Spec2017Xz,
                _ => panic!("Unknown spec workload"),
            }
        }

        _ => unreachable!(),
    };

    let perf_stat = sub_m.is_present("PERF_STAT");
    let disable_thp = sub_m.is_present("DISABLE_THP");
    let disable_aslr = sub_m.is_present("DISABLE_ASLR");
    let mm_fault_tracker = sub_m.is_present("MM_FAULT_TRACKER");
    let flame_graph = sub_m.is_present("FLAME_GRAPH");
    let fom = sub_m.is_present("FOM");
    let pte_fault_size = sub_m.value_of("PTE_FAULT_SIZE").unwrap_or("1").parse::<usize>().unwrap();
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
        fom,
        pte_fault_size,

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
    let ushell = connect_and_setup_host(login)?;
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
    let runtime_file = dir!(&results_dir, cfg.gen_file_name("runtime"));

    let bmks_dir = dir!(&user_home, crate::RESEARCH_WORKSPACE_PATH, crate::BMKS_PATH);
    let scripts_dir = dir!(&user_home, crate::RESEARCH_WORKSPACE_PATH, crate::SCRIPTS_PATH);
    let spec_dir = dir!(&bmks_dir, crate::SPEC2017_PATH);
    let parsec_dir = dir!(&user_home, crate::PARSEC_PATH);

    ushell.run(cmd!(
        "echo {} > {}",
        escape_for_bash(&serde_json::to_string(&cfg)?),
        dir!(&results_dir, params_file)
    ))?;

    let mut cmd_prefix = String::new();
    let proc_name = match &cfg.workload {
        Workload::AllocTest { size: _ } => "alloc_test",
        Workload::Canneal { workload: _ }=> "canneal",
        Workload::Spec2017Mcf => "mcf_s",
        Workload::Spec2017Xalancbmk => "xalancbmk_s",
        Workload::Spec2017Xz => "xz_s",
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
        1000
    )?;

    if cfg.disable_aslr {
        libscail::disable_aslr(&ushell)?;
    } else {
        libscail::enable_aslr(&ushell)?;
    }

    // Figure out which cores we will use for the workload
    let pin_cores = match &cfg.workload {
        Workload::Spec2017Mcf
        | Workload::Spec2017Xz
        | Workload::Spec2017Xalancbmk => {
            vec![tctx.next(), tctx.next(), tctx.next(), tctx.next()]
        }
        _ => vec![tctx.next()]
    };

    if cfg.perf_stat {
        cmd_prefix.push_str(
            &gen_perf_command_prefix(
                perf_stat_file, &cfg.perf_counters, ""
            )
        );
    }

    if cfg.flame_graph {
        let pin_cores_str = pin_cores
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        cmd_prefix.push_str(
            &format!("sudo perf record -a -C {} -g -F 99 -o {} ", pin_cores_str, &perf_record_file)
        );
    }

    if cfg.fom {
        cmd_prefix.push_str(&format!("{}/fom_wrapper ", bmks_dir));

        // Set up the remote for FOM
        ushell.run(cmd!("sudo mkfs.ext4 /dev/pmem0"))?;
        ushell.run(cmd!("sudo tune2fs -O ^has_journal /dev/pmem0"))?;
        ushell.run(cmd!("mkdir -p ./daxtmp/"))?;
        ushell.run(cmd!("sudo mount -o dax /dev/pmem0 daxtmp/"))?;
        ushell.run(cmd!("sudo chown -R $USER daxtmp/"))?;
        ushell.run(cmd!("echo \"{}/daxtmp/\" | sudo tee /sys/kernel/mm/fom/file_dir", &user_home))?;
        ushell.run(cmd!("echo 1 | sudo tee /sys/kernel/mm/fom/state"))?;
    }

    ushell.run(cmd!("echo {} | sudo tee /sys/kernel/mm/fom/pte_fault_size", cfg.pte_fault_size))?;

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
        Workload::AllocTest { size } => {
            time!(timers, "Workload", {
                run_alloc_test(
                    &ushell,
                    &bmks_dir,
                    size,
                    Some(&cmd_prefix),
                    &runtime_file,
                    pin_cores[0],
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
        | w @ Workload::Spec2017Xz
        | w @ Workload::Spec2017Xalancbmk => {
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
    }

    // Generate the flamegraph if needed
    if cfg.flame_graph {
        ushell.run(cmd!(
            "sudo perf script -i {} | ./FlameGraph/stackcollapse-perf.pl > /tmp/flamegraph",
            &perf_record_file,
        ))?;
        ushell.run(cmd!("./FlameGraph/flamegraph.pl /tmp/flamegraph > {}", flame_graph_file))?;
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

//    ushell.run(cmd!("sudo cpupower frequency-set -g performance",))?;
    set_kernel_printk_level(&ushell, 5)?;

    Ok(ushell)
}

fn run_alloc_test(
    ushell: &SshShell,
    bmks_dir: &str,
    size: usize,
    cmd_prefix: Option<&str>,
    runtime_file: &str,
    pin_core: usize
) -> Result<(), failure::Error> {

    let start = Instant::now();
    ushell.run(cmd!(
        "sudo taskset -c {} {} ./alloc_test {}",
        pin_core,
        cmd_prefix.unwrap_or(""),
        size
    ).cwd(bmks_dir))?;
    let duration = Instant::now() - start;

    ushell.run(cmd!("echo {} > {}", duration.as_millis(), runtime_file))?;
    Ok(())
}
