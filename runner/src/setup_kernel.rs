use clap::clap_app;

use libscail::{
    dir, get_user_home_dir, get_git_hash, GitRepo,
    KernelBaseConfigSource, KernelConfig, KernelPkgType, KernelSrc, Login,
};

use spurs::{cmd, Execute, SshShell};

pub fn cli_options() -> clap::App<'static, 'static> {
    clap_app! { setup_kernel =>
        (about: "Sets up the given _centos_ with the given kernel. Requires `sudo`.")
        (@setting ArgRequiredElseHelp)
        (@setting DisableVersion)
        (@setting TrailingVarArg)
        (@arg HOSTNAME: +required +takes_value
         "The domain name of the remote (e.g. c240g2-031321.wisc.cloudlab.us:22)")
        (@arg USERNAME: +required +takes_value
         "The username on the remote (e.g. markm)")
        (@arg REPO: --repo +required +takes_value
         "The git repo where the kernel is stored.")
        (@arg BRANCH: --branch +takes_value
         "The branch of the repo to clone. Defaults to \"main\"")
        (@arg GIT_USER: --git_user +required +takes_value
         "The username of the GitHub account to use to clone the kernel")
        (@arg SECRET: --secret +takes_value
         "The GitHub access token to use")
        (@arg CONFIGS: +allow_hyphen_values ...
         "Space separated list of Linux kernel configuration options, prefixed by \
         + to enable and - to disable. For example, +CONFIG_ZSWAP or \
         -CONFIG_PAGE_TABLE_ISOLATION"
        )
    }
}

pub fn run(sub_m: &clap::ArgMatches<'_>) -> Result<(), failure::Error> {
    let login = Login {
        username: sub_m.value_of("USERNAME").unwrap(),
        hostname: sub_m.value_of("HOSTNAME").unwrap(),
        host: sub_m.value_of("HOSTNAME").unwrap(),
    };

    let repo = sub_m.value_of("REPO").unwrap();
    let branch = sub_m.value_of("BRANCH").unwrap_or("main");
    let git_user = sub_m.value_of("GIT_USER").unwrap();
    let secret = sub_m.value_of("SECRET");

    let git_repo = if let Some(_secret) = &secret {
        GitRepo::HttpsPrivate {
            username: git_user,
            repo: repo,
        }
    } else {
        GitRepo::HttpsPublic {
            repo: repo,
        }
    };

    let kernel_config: Vec<_> = sub_m
        .values_of("CONFIGS")
        .map(|values| {
            values
                .map(|arg| parse_config_option(arg).unwrap())
                .collect()
        })
        .unwrap_or_else(|| vec![]);

    let ushell = SshShell::with_any_key(&login.username, &login.host)?;

    let user_home = get_user_home_dir(&ushell)?;
    let kernel_path = dir!(&user_home, crate::KERNEL_PATH);

    libscail::clone_git_repo(
        &ushell,
        git_repo,
        Some(&kernel_path),
        Some(&branch),
        secret,
        &[]
    )?;

    // Get the base config
    let config = ushell.run(cmd!("ls -1 /boot/config-* | head -n1").use_bash())?.stdout;
    let config = config.trim();
    let git_hash = get_git_hash(&ushell, &kernel_path)?;
    let kernel_localversion = libscail::gen_local_version(branch, &git_hash);

    libscail::build_kernel(
        &ushell,
        KernelSrc::Git {
            repo_path: kernel_path.clone(),
            commitish: (&branch).to_string(),
        },
        KernelConfig {
            base_config: KernelBaseConfigSource::Path(config.into()),
            extra_options: &kernel_config,
        },
        Some(&kernel_localversion),
        KernelPkgType::Deb,
        None,
        true,
    )?;

    // Find the deb package
    let kernel_deb = ushell
        .run(
            cmd!(
                "ls -Art {} | \
                grep .*\\.deb |\
                grep -v headers | \
                grep -v libc | \
                grep -v dbg | \
                tail -n 1",
                kernel_path
            )
        )?
        .stdout;
    let kernel_headers_deb = ushell
        .run(
            cmd!(
                "ls -Art {} | \
                grep .*\\.deb | \
                grep headers | \
                tail -n 1",
                kernel_path
            )
        )?
        .stdout;
    let kernel_deb = kernel_deb.trim();
    let kernel_headers_deb = kernel_headers_deb.trim();

    ushell.run(cmd!("sudo dpkg -i {} {}", kernel_deb, kernel_headers_deb).cwd(kernel_path))?;
    ushell.run(cmd!("sudo grub-set-default 0"))?;

    Ok(())
}

fn parse_config_option(opt: &str) -> Result<(&str, bool), failure::Error> {
    fn check(s: &str) -> Result<&str, failure::Error> {
        if s.is_empty() {
            Err(failure::format_err!("Empty string is not a valid option"))
        } else {
            for c in s.chars() {
                if !c.is_ascii_alphanumeric() && c != '_' {
                    return Err(failure::format_err!("Invalid config name \"{}\"", s));
                }
            }
            Ok(s)
        }
    }

    if opt.is_empty() {
        Err(failure::format_err!("Empty string is not a valid option"))
    } else {
        match &opt[0..1] {
            "+" => Ok((check(&opt[1..])?, true)),
            "-" => Ok((check(&opt[1..])?, false)),
            _ => Err(failure::format_err!(
                "Kernel config option must be prefixed with + or -"
            )),
        }
    }
}
