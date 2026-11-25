//! Command-line mode selection.
//!
//! Two modes, and the difference is deliberate:
//!
//! - `--demo` runs the synthetic frontend. It touches nothing on disk.
//! - `--profile <DIR>` runs the real player against exactly that directory.
//!   There is no default and no discovery: the installed Muzon's profile is
//!   never picked up implicitly, because opening someone's real library and
//!   session without being asked is not something a launcher should decide.

use std::fmt;
use std::path::PathBuf;

/// What the binary was asked to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// The synthetic frontend, no backend.
    Demo,
    /// The real player, with all data confined to this directory.
    Real(PathBuf),
}

/// How the arguments could not be understood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// Nothing was asked for.
    NoMode,
    /// `--demo` and `--profile` together say two different things.
    BothModes,
    DuplicateProfile,
    /// `--profile` without a directory after it.
    MissingProfile,
    /// A flag this build does not know.
    Unknown(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoMode => write!(f, "choose a mode: --demo or --profile <DIR>"),
            Self::DuplicateProfile => write!(f, "--profile may only be specified once"),
            Self::BothModes => write!(f, "--demo and --profile cannot be used together"),
            Self::MissingProfile => write!(f, "--profile needs a directory"),
            Self::Unknown(argument) => write!(f, "unknown argument: {argument}"),
        }
    }
}

impl std::error::Error for CliError {}

/// Shown for `--help` and after a usage error.
pub const USAGE: &str = "\
Muzon — native player

Usage:
  muzon-native --demo
      Synthetic library, simulated playback; nothing is read or written.

  muzon-native --profile <DIR>
      Real player. The database, session, playback settings and cover cache
      all live inside <DIR>; nothing is read from an installed Muzon.

Options:
  -h, --help    Show this text.

Examples:
  muzon-native --profile /tmp/muzon-profile
  muzon-native --profile /tmp/muzon-native";

/// Parses the arguments after the program name.
///
/// `--help` and `-h` are handled by the caller so it can exit with success;
/// every other surprise is an error.
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Mode, CliError> {
    let mut demo = false;
    let mut profile: Option<PathBuf> = None;
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--demo" => demo = true,
            "--profile" => {
                let value = args.next().ok_or(CliError::MissingProfile)?;
                if value.is_empty() || value.starts_with('-') {
                    return Err(CliError::MissingProfile);
                }
                if profile.is_some() {
                    return Err(CliError::DuplicateProfile);
                }
                profile = Some(PathBuf::from(value));
            }
            other => {
                if let Some(value) = other.strip_prefix("--profile=") {
                    if value.is_empty() {
                        return Err(CliError::MissingProfile);
                    }
                    if profile.is_some() {
                        return Err(CliError::DuplicateProfile);
                    }
                    profile = Some(PathBuf::from(value));
                } else {
                    return Err(CliError::Unknown(argument));
                }
            }
        }
    }
    match (demo, profile) {
        (true, Some(_)) => Err(CliError::BothModes),
        (true, None) => Ok(Mode::Demo),
        (false, Some(profile)) => Ok(Mode::Real(profile)),
        (false, None) => Err(CliError::NoMode),
    }
}

/// Whether the arguments ask for help.
#[must_use]
pub fn wants_help(args: &[String]) -> bool {
    args.iter()
        .any(|argument| argument == "--help" || argument == "-h")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn demo_and_profile_are_the_two_modes() {
        assert_eq!(parse(args(&["--demo"])), Ok(Mode::Demo));
        assert_eq!(
            parse(args(&["--profile", "/tmp/p"])),
            Ok(Mode::Real(PathBuf::from("/tmp/p")))
        );
        assert_eq!(
            parse(args(&["--profile=/tmp/p"])),
            Ok(Mode::Real(PathBuf::from("/tmp/p")))
        );
        assert_eq!(
            parse(args(&["--demo", "--profile", "/tmp/p"])),
            Err(CliError::BothModes)
        );
    }

    #[test]
    fn there_is_no_default_mode_and_no_default_profile() {
        assert_eq!(parse(args(&[])), Err(CliError::NoMode));
        assert_eq!(parse(args(&["--profile"])), Err(CliError::MissingProfile));
        assert_eq!(parse(args(&["--profile="])), Err(CliError::MissingProfile));
        assert_eq!(
            parse(args(&["--library", "/tmp"])),
            Err(CliError::Unknown("--library".into()))
        );
    }

    #[test]
    fn profile_requires_a_real_argument_and_cannot_be_repeated() {
        assert_eq!(
            parse(args(&["--profile", "--demo"])),
            Err(CliError::MissingProfile)
        );
        assert_eq!(
            parse(args(&["--profile", ""])),
            Err(CliError::MissingProfile)
        );
        assert_eq!(
            parse(args(&["--profile=a", "--profile=b"])),
            Err(CliError::DuplicateProfile)
        );
    }

    #[test]
    fn help_is_recognised_without_being_a_mode() {
        assert!(wants_help(&args(&["--help"])));
        assert!(wants_help(&args(&["-h"])));
        assert!(!wants_help(&args(&["--demo"])));
    }
}
