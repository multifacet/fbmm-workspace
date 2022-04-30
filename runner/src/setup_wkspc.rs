/// Configure a freshly acquired cloudlab machine and install
/// all necessary software
use clap::clap_app;

use libscail::{
    clone_git_repo, dir, downloads, downloads::download_and_extract, get_user_home_dir,
    install_spec_2017, with_shell, GitRepo, Login,
};

use spurs::{cmd, Execute, SshShell};

pub fn cli_options() -> clap::App<'static, 'static> {
    clap_app! { setup_wkspc =>
        (about: "Setup a new _ubuntu_ machine. Requires `sudo`.")
        (@setting ArgRequiredElseHelp)
        (@setting DisableVersion)
        (@arg HOSTNAME: +required +takes_value
         "The domain name and ssh port of the remote (e.g. c240g2-031321.wisc.cloudlab.us:22)")
        (@arg USERNAME: +required +takes_value
         "The username of the remote (e.g. bijan)")

        (@arg HOST_DEP: --host_dep
         "(Optional) If passed, install host depenendencies")

        (@arg RESIZE_ROOT: --resize_root
         "(Optional) resize the root partition to take up the whole device, \
          destroying any other partitions on the device. This is useful on cloudlab, \
          where the root partition is 16GB by default.")

        (@arg SWAP_DEVS: +takes_value --swap ...
         "(Optional) specify which devices to use as swap devices. The devices must \
         all be _unmounted_. By default all unpartitioned, unmounted devices are used \
         (e.g. --swap sda sdb sdc).")

        (@arg UNSTABLE_DEVICE_NAMES: --unstable_device_names
         "(Optional) specifies that device names may change across a reboot \
          (e.g. /dev/sda might be /dev/sdb after a reboot). In this case, the device \
          names used in other arguments will be converted to stable names based on device ids.")

        (@arg CLONE_WKSPC: --clone_wkspc
         "(Optional) If passed, clone the workspace on the remote (or update if already cloned). \
         If the method uses HTTPS to access a private repository, the --secret option must also \
         be passed giving the GitHub personal access token or password.")

        (@arg GIT_USER: --git_user +takes_value requires[CLONE_WKSPC]
          "(Optional) The git username to clone with.")

        (@arg WKSPC_BRANCH: --wkspc_branch +takes_value requires[CLONE_WKSPC]
         "(Optional) If passed, clone the specified branch name. If not pased, master is used. \
         requires --clone_wkspc.")

        (@arg SECRET: +takes_value --secret
         "(Optional) If we should clone the workspace, this is the Github personal access \
          token or password for cloning the repo.")

        (@arg HOST_BMKS: --host_bmks
         "(Optional) If passed, build host benchmarks. This also makes them available to the guest.")
        (@arg SPEC_2017: --spec_2017 +takes_value
         "(Optional) If passed, setup and build SPEC 2017 on the remote machine (on the host only). \
          Because SPEC 2017 is not free, you need to pass runner a path to the SPEC 2017 ISO on the \
          driver machine. The ISO will be copied to the remote machine, mounted, and installed there.")
        (@arg JEMALLOC: --jemalloc
         "(Optional) set jemalloc as the system allocator.")
    }
}

struct SetupConfig<'a, A>
where
    A: std::net::ToSocketAddrs + std::fmt::Display + std::fmt::Debug + Clone,
{
    /// Login credentials for the host.
    login: Login<'a, 'a, A>,

    /// Install host dependencies, rename poweorff.
    host_dep: bool,

    /// Resize the root partition to take up the whole device.
    resize_root: bool,
    /// Set the devices to be used
    swap_devices: Option<Vec<&'a str>>,
    /// Device names are unstable and should be converted to UUIDs.
    unstable_names: bool,

    /// Should we clone/update the workspace?
    clone_wkspc: bool,
    /// Git username to clone with
    git_user: Option<&'a str>,
    /// What branch of the workspace should we use?
    wkspc_branch: Option<&'a str>,
    /// The PAT or password to clone/update the workspace with, if needed.
    secret: Option<&'a str>,

    /// Should we build host benchmarks?
    host_bmks: bool,
    /// Should we install SPEC 2017? If so, what is the ISO path?
    spec_2017: Option<&'a str>,

    /// Set jemalloc as the default system allocator.
    jemalloc: bool,
}

pub fn run(sub_m: &clap::ArgMatches<'_>) -> Result<(), failure::Error> {
    let login = Login {
        username: sub_m.value_of("USERNAME").unwrap(),
        hostname: sub_m.value_of("HOSTNAME").unwrap(),
        host: sub_m.value_of("HOSTNAME").unwrap(),
    };

    let host_dep = sub_m.is_present("HOST_DEP");

    let resize_root = sub_m.is_present("RESIZE_ROOT");
    let swap_devices = sub_m.values_of("SWAP_DEVS").map(|i| i.collect());
    let unstable_names = sub_m.is_present("UNSTABLE_DEVICE_NAMES");

    let clone_wkspc = sub_m.is_present("CLONE_WKSPC");
    let git_user = sub_m.value_of("GIT_USER");
    let wkspc_branch = sub_m.value_of("WKSPC_BRANCH");
    let secret = sub_m.value_of("SECRET");

    let host_bmks = sub_m.is_present("HOST_BMKS");
    let spec_2017 = sub_m.value_of("SPEC_2017");

    let jemalloc = sub_m.is_present("JEMALLOC");

    let cfg = SetupConfig {
        login,
        host_dep,
        resize_root,
        swap_devices,
        unstable_names,
        clone_wkspc,
        git_user,
        wkspc_branch,
        secret,
        host_bmks,
        spec_2017,
        jemalloc,
    };

    run_inner(cfg)?;

    Ok(())
}

fn run_inner<A>(cfg: SetupConfig<'_, A>) -> Result<(), failure::Error>
where
    A: std::net::ToSocketAddrs + std::fmt::Display + std::fmt::Debug + Clone,
{
    // Connect to the remote
    let ushell = SshShell::with_any_key(cfg.login.username, &cfg.login.host)?;

    if cfg.host_dep {
        install_host_dependencies(&ushell)?;
        libscail::install_rust(&ushell)?;
    }

    set_up_host_devices(&ushell, &cfg)?;

    if cfg.clone_wkspc {
        clone_research_workspace(&ushell, &cfg)?;
    }

    if cfg.jemalloc {
        libscail::install_jemalloc(&ushell)?;
    }

    if cfg.host_bmks {
        build_host_benchmarks(&ushell)?;
    }

    if let Some(iso_path) = cfg.spec_2017 {
        let spec_path = dir!(
            crate::RESEARCH_WORKSPACE_PATH,
            crate::BMKS_PATH,
            crate::SPEC2017_PATH
        );
        let config = "spec-linux-x86.cfg";
        install_spec_2017(&ushell, &cfg.login, iso_path, &config, &spec_path)?;
    }

    ushell.run(cmd!("echo DONE"))?;

    Ok(())
}

fn install_host_dependencies(
    ushell: &SshShell,
) -> Result<(), failure::Error>
{
    // Make sure we're up to date
    ushell.run(cmd!("sudo apt update; sudo apt upgrade -y"))?;

    with_shell! { ushell =>
        spurs_util::ubuntu::apt_install(&[
            "build-essential",
            "libssl-dev",
            "libelf-dev",
            "libdw-dev",
            "libncurses-dev",
            "dwarves",
            "libpci-dev",
            "numactl",
            "linux-tools-common",
            "openjdk-8-jdk",
            "fuse",
            "memcached",
            "libmemcached-tools",
            "redis-server",
            "python3",
            "cmake",
            "gfortran",
            "curl",
            "bpfcc-tools",
            "libhugetlbfs-bin",
            "maven",
        ]),
    };

    // Clone FlameGraph
    let flamegraph_repo = GitRepo::HttpsPublic {
        repo: "github.com/brendangregg/FlameGraph.git",
    };
    clone_git_repo(ushell, flamegraph_repo, None, None, None, &[])?;

    Ok(())
}

fn clone_research_workspace<A>(
    ushell: &SshShell,
    cfg: &SetupConfig<'_, A>,
) -> Result<(), failure::Error>
where
    A: std::net::ToSocketAddrs + std::fmt::Display + std::fmt::Debug + Clone,
{
    const SUBMODULES: &[&str] = &["libscail", "bmks/YCSB", "bmks/memcached"];
    let user = &cfg.git_user.unwrap_or("");
    let branch = cfg.wkspc_branch.unwrap_or("main");
    let wkspc_repo = GitRepo::HttpsPrivate {
        repo: "github.com/BijanT/fom-research-workspace.git",
        username: user,
    };

    clone_git_repo(
        ushell,
        wkspc_repo,
        Some("research-workspace"),
        Some(branch),
        cfg.secret,
        SUBMODULES,
    )?;

    Ok(())
}

fn build_host_benchmarks(ushell: &SshShell) -> Result<(), failure::Error> {
    let user_home = get_user_home_dir(ushell)?;
    let num_cores = libscail::get_num_cores(ushell)?;

    ushell.run(cmd!("mkdir -p {}", crate::RESULTS_PATH))?;

    // Build microbenchmarks
    let bmks_dir = dir!(crate::RESEARCH_WORKSPACE_PATH, crate::BMKS_PATH);
    ushell.run(cmd!("make").cwd(bmks_dir))?;

    // Download PARSEC and build canneal
    download_and_extract(ushell, downloads::PARSEC, &user_home, None)?;
    ushell.run(cmd!("./parsecmgmt -a build -p canneal").cwd("parsec-3.0/bin/"))?;

    // memcached
    with_shell! { ushell in &dir!(crate::RESEARCH_WORKSPACE_PATH, crate::BMKS_PATH, "memcached") =>
        cmd!("./autogen.sh"),
        cmd!("./configure"),
        cmd!("make -j {}", num_cores),
    }

    // Build YCSB
    let ycsb_dir = dir!(crate::RESEARCH_WORKSPACE_PATH, crate::BMKS_PATH, "YCSB");
    ushell.run(cmd!("mvn clean package").cwd(ycsb_dir))?;

    Ok(())
}

fn set_up_host_devices<A>(ushell: &SshShell, cfg: &SetupConfig<'_, A>) -> Result<(), failure::Error>
where
    A: std::net::ToSocketAddrs + std::fmt::Display + std::fmt::Debug + Clone,
{
    // Remove any existing swap partitions from /etc/fstab because we plan to do all of our own
    // mounting and unmounting. Moreover, if fstab contains a swap partition that we destroy during
    // setup, systemd will sit around trying to find it and adding minutes to every reboot.
    ushell.run(cmd!(
        r#"sudo sed -i 's/^.*swap.*$/#& # COMMENTED OUT BY setup_wkspc/' /etc/fstab"#
    ))?;

    if cfg.resize_root {
        libscail::resize_root_partition(ushell)?;
    }

    if let Some(swap_devs) = &cfg.swap_devices {
        if swap_devs.is_empty() {
            let unpartitioned =
                spurs_util::get_unpartitioned_devs(ushell, /* dry_run */ false)?;
            for dev in unpartitioned.iter() {
                ushell.run(cmd!("sudo mkswap /dev/{}", dev))?;
            }
        } else {
            let mut swap_devices = Vec::new();
            for dev in swap_devs.iter() {
                let dev = if cfg.unstable_names {
                    let dev_id = libscail::get_device_id(ushell, dev)?;
                    dir!("disk/by-id/", dev_id)
                } else {
                    (*dev).to_owned()
                };

                ushell.run(cmd!("sudo mkswap /dev/{}", dev))?;

                swap_devices.push(dev);
            }

            libscail::set_remote_research_setting(&ushell, "swap-devices", &swap_devices)?;
        }
    }

    Ok(())
}
