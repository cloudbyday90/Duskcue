use std::io;

use landlock::RulesetStatus;
use tokio::process::Command;

use super::SandboxConfig;
pub(crate) use super::descriptors::BootstrapReadiness;
use super::descriptors::{self, PreparedDescriptors};
use super::{filesystem, filter, managed_elf};

pub(crate) struct PreparedLaunch {
    descriptors: PreparedDescriptors,
    filesystem: Option<landlock::RulesetCreated>,
    executable: &'static str,
}

impl PreparedLaunch {
    pub(crate) fn new(config: &SandboxConfig<'_>) -> io::Result<Self> {
        managed_elf::validate()?;
        Self::prepare(config, managed_elf::EXECUTABLE)
    }

    #[cfg(test)]
    pub(super) fn probe(config: &SandboxConfig<'_>) -> io::Result<Self> {
        managed_elf::validate_probe()?;
        Self::prepare(config, "/usr/local/libexec/duskcue-ffmpeg-probe")
    }

    #[cfg(test)]
    pub(super) fn corrupt_policy(&mut self, offset: usize) -> io::Result<()> {
        self.descriptors = PreparedDescriptors::corrupt(&filter::build_ffmpeg_filter()?, offset)?;
        Ok(())
    }

    fn prepare(config: &SandboxConfig<'_>, executable: &'static str) -> io::Result<Self> {
        let filter = filter::build_ffmpeg_filter()?;
        Ok(Self {
            descriptors: PreparedDescriptors::new(&filter)?,
            filesystem: Some(filesystem::prepare(config)?),
            executable,
        })
    }

    pub(crate) fn command(&mut self) -> Command {
        let mut command = Command::new(self.executable);
        for variable in [
            "LD_PRELOAD",
            "LD_LIBRARY_PATH",
            "LD_AUDIT",
            "LD_DEBUG",
            "LD_DEBUG_OUTPUT",
            "LD_PROFILE",
            "LD_ASSUME_KERNEL",
            "GLIBC_TUNABLES",
        ] {
            command.env_remove(variable);
        }
        let policy_fd = self.descriptors.policy_fd();
        let acknowledgment_fd = self.descriptors.acknowledgment_fd();
        command.env("DUSKCUE_FFMPEG_POLICY_FD", policy_fd.to_string());
        command.env("DUSKCUE_FFMPEG_ACK_FD", acknowledgment_fd.to_string());
        let mut filesystem = self.filesystem.take();
        unsafe {
            command.pre_exec(move || {
                let ruleset = filesystem
                    .take()
                    .ok_or_else(|| io::Error::from_raw_os_error(libc::EINVAL))?;
                let status = ruleset
                    .restrict_self()
                    .map_err(|_| io::Error::from_raw_os_error(libc::EPERM))?;
                if !status.no_new_privs {
                    return Err(io::Error::from_raw_os_error(libc::EPERM));
                }
                let enforcement = match status.ruleset {
                    RulesetStatus::NotEnforced => 0,
                    RulesetStatus::PartiallyEnforced => 1,
                    RulesetStatus::FullyEnforced => 2,
                };
                descriptors::inherit(policy_fd)?;
                descriptors::inherit(acknowledgment_fd)?;
                descriptors::write_filesystem_status(acknowledgment_fd, enforcement)
            });
        }
        command
    }

    pub(crate) fn readiness(self) -> BootstrapReadiness {
        self.descriptors.readiness()
    }
}
