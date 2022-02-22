/// Configure a freshly acquired cloudlab machine and install
/// all necessary software

use std::path::PathBuf;
use std::process::Command;

use clap::clap_app;

use libscail::{
    clone_git_repo, dir, downloads,
    downloads::{Download, download, download_and_extract},
    get_user_home_dir, GitRepo,
    rsync_to_remote, with_shell, KernelBaseConfigSource, KernelConfig, KernelPkgType, KernelSrc,
    Login, ServiceAction
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
        install_host_dependencies(&ushell, &cfg)?;
        libscail::install_rust(&ushell)?;
    }

    if cfg.clone_wkspc {
        clone_research_workspace(&ushell, &cfg)?;
    }

    if cfg.jemalloc {
        libscail::install_jemalloc(&ushell)?;
    }

    if cfg.host_bmks {
        build_host_benchmarks(&ushell)?;
    }

    Ok(())
}

fn install_host_dependencies<A>(
    ushell: &SshShell,
    cfg: &SetupConfig<'_, A>,
) -> Result<(), failure::Error>
where
    A: std::net::ToSocketAddrs + std::fmt::Display + std::fmt::Debug + Clone,
{
    // Make sure we're up to date
    ushell.run(cmd!("sudo apt update; sudo apt upgrade -y"))?;

    with_shell! { ushell =>
        spurs_util::ubuntu::apt_install(&[
            "build-essential",
            "libssl-dev",
            "numactl",
            "perf",
            "openjdk-8-jdk",
            "fuse",
            "memcached",
            "redis-server",
            "python3",
            "python3-devel",
            "cmake3",
            "curl",
            "bcc-tools",
            "libbcc-examples",
        ]),
    };

    // Set up maven
    let user_home = &get_user_home_dir(&ushell)?;
    download_and_extract(ushell, downloads::MAVEN, user_home, Some("maven"))?;
    ushell.run(cmd!(
        "echo -e 'export JAVA_HOME=/usr/lib/jvm/java/\n\
         export M2_HOME=~{}/maven/\n\
         export MAVEN_HOME=$M2_HOME\n\
         export PATH=${{M2_HOME}}/bin:${{PATH}}' | \
         sudo tee /etc/profile.d/java.sh",
        cfg.login.username
    ))?;

    Ok(())
}

fn clone_research_workspace<A>(
    ushell: &SshShell,
    cfg: &SetupConfig<'_, A>,
) -> Result<(), failure::Error>
where
    A: std::net::ToSocketAddrs + std::fmt::Display + std::fmt::Debug + Clone,
{
    const SUBMODULES: &[&str] = &["libscail"];
    let user = &cfg.git_user.unwrap_or("");
    let wkspc_repo = GitRepo::HttpsPrivate {
        repo: "github.com/BijanT/fom-research-workspace.git",
        username: user,
    };

    clone_git_repo(ushell, wkspc_repo, Some("research-workspace"), None, cfg.secret, SUBMODULES)?;

    Ok(())
}

fn build_host_benchmarks(
    ushell: &SshShell,
) -> Result<(), failure::Error>
{
    Ok(())
}
