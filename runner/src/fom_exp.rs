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
    perf_record: bool,
    perf_counters: Vec<String>,
    disable_thp: bool,
    disable_aslr: bool,
    mm_fault_tracker: bool,
    flame_graph: bool,
    fom: bool,

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
        (@arg PERF_RECORD: --perf_record
         "Measure the workload using perf record.")
        (@arg PERF_COUNTER: --perf_counter +takes_value ... number_of_values(1)
         requires[PERF_STAT]
         "Which counters to record with perf stat.")
        (@arg DISABLE_THP: --disable_thp
         "Disable THP completely.")
        (@arg DISABLE_ASLR: --disable_aslr
         "Disable ASLR.")
        (@arg MM_FAULT_TRACKER: --mm_fault_tracker
         "Record page fault statistics with mm_fault_tracker.")
        (@arg FLAME_GRAPH: --flame_graph requires[PERF_RECORD]
         "Generate a flame graph of the workload.")
        (@arg FOM: --fom
         "Run the workload with file only memory.")
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
    let perf_record = sub_m.is_present("PERF_RECORD");
    let disable_thp = sub_m.is_present("DISABLE_THP");
    let disable_aslr = sub_m.is_present("DISABLE_ASLR");
    let mm_fault_tracker = sub_m.is_present("MM_FAULT_TRACKER");
    let flame_graph = sub_m.is_present("FLAME_GRAPH");
    let fom = sub_m.is_present("FOM");
    let perf_counters: Vec<String> = sub_m
        .values_of("PERF_COUNTER")
        .map(|counters| counters.map(Into::into).collect()).unwrap();

    let ushell = SshShell::with_any_key(login.username, login.host)?;
    let remote_research_settings = libscail::get_remote_research_settings(&ushell)?;

    let cfg = Config {
        exp: "fom_exp".into(),
        workload,
        perf_stat,
        perf_record,
        perf_counters,
        disable_thp,
        disable_aslr,
        mm_fault_tracker,
        flame_graph,
        fom,

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
    let tctx = TasksetCtx::new(cores);

    // Setup the output file name
    let results_dir = dir!(&user_home, crate::RESULTS_PATH);

    let (output_file, params_file, time_file, sim_file) = cfg.gen_standard_names();
    let output_file = dir!(&results_dir, output_file);
    let perf_stat_file = dir!(&results_dir, cfg.gen_file_name("perf_stat"));
    let perf_record_file = dir!(&results_dir, cfg.gen_file_name("perf_record"));
    let mm_fault_file = dir!(&results_dir, cfg.gen_file_name("mm_fault"));
    let flame_graph_file = dir!(&results_dir, cfg.gen_file_name("flamegraph.svg"));
    let runtime_file = dir!(&results_dir, cfg.gen_file_name("runtime"));

    let bmks_dir = dir!(&user_home, crate::RESEARCH_WORKSPACE_PATH, crate::BMKS_PATH);
    let spec_dir = dir!(&user_home, crate::RESEARCH_WORKSPACE_PATH, crate::SPEC2017_PATH);
    let parsec_dir = dir!(&user_home, crate::PARSEC_PATH);

    ushell.run(cmd!(
        "echo {} > {}",
        escape_for_bash(&serde_json::to_string(&cfg)?),
        dir!(&results_dir, params_file)
    ))?;

    let mut cmd_prefix = String::new();
    let proc_name = match &cfg.workload {
        Workload::AllocTest { size: _ } => "alloctest",
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
    );

    if cfg.disable_aslr {
        libscail::disable_aslr(&ushell)?;
    } else {
        libscail::enable_aslr(&ushell)?;
    }

    if cfg.perf_stat {
        cmd_prefix.push_str(
            &gen_perf_command_prefix(
                perf_stat_file, &cfg.perf_counters, ""
            )
        );
    }

    if cfg.fom {
        cmd_prefix.push_str(&format!("{}/fom_wrapper ", bmks_dir));        
    }

    let perf_file = if cfg.perf_record {
        Some(perf_record_file.as_str())
    } else {
        None
    };

    match cfg.workload {
        Workload::AllocTest { size } => (),

        Workload::Canneal { workload } => {
            time!(timers, "Workload", {
                run_canneal(
                    &ushell,
                    &parsec_dir,
                    workload,
                    Some(&cmd_prefix),
                    perf_file,
                    None,
                    &runtime_file,
                    tctx.next(),
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
                    ushell,
                    &spec_dir,
                    wkload,
                    None,
                    Some(&cmd_prefix),
                    perf_file,
                    &runtime_file,
                    [tctx.next(), tctx.next(), tctx.next(), tctx.next()],
                )?;
            });
        }
    }

    Ok(())
}

fn connect_and_setup_host<A>(login: &Login<A>) -> Result<SshShell, failure::Error>
where
    A: std::net::ToSocketAddrs + std::fmt::Display + std::fmt::Debug + Clone,
{
    let mut ushell = SshShell::with_any_key(login.username, &login.host)?; 
    spurs_util::reboot(&mut ushell, /* dry_run */ false)?;

    // Keep trying to connect until we succeed
    let ushell = {
        let mut shell;
        loop {
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

    ushell.run(cmd!("sudo cpupower frequency-set -g performance",))?;
    set_kernel_printk_level(&ushell, 5)?;

    Ok(ushell)
}
