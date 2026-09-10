use std::{io, path::PathBuf};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Run,
    Demo,
    Status,
    Detect,
    Login,
    Logout,
    Help,
}
#[derive(Debug, Default)]
pub struct Options {
    pub mode: Mode,
    pub browser: Option<PathBuf>,
}
impl Options {
    pub fn parse(args: impl IntoIterator<Item = String>) -> io::Result<Self> {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            if arg == "--browser-path" {
                options.browser = Some(PathBuf::from(
                    args.next()
                        .filter(|s| !s.starts_with("--"))
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "--browser-path requires a path",
                            )
                        })?,
                ));
                continue;
            }
            let mode = match arg.as_str() {
                "--demo" => Mode::Demo,
                "--status" | "--check-auth" => Mode::Status,
                "--detect-browsers" => Mode::Detect,
                "--login" => Mode::Login,
                "--logout" | "logout" => Mode::Logout,
                "--help" | "-h" => Mode::Help,
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("Unknown option: {arg}"),
                    ));
                }
            };
            if options.mode != Mode::Run && options.mode != mode {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Choose one operation at a time",
                ));
            }
            options.mode = mode;
        }
        Ok(options)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_path_is_independent_of_option_order() {
        for args in [
            ["--status", "--browser-path", "/custom/chrome"],
            ["--browser-path", "/custom/chrome", "--status"],
        ] {
            let opts = Options::parse(args.map(str::to_string)).unwrap();
            assert_eq!(opts.mode, Mode::Status);
            assert_eq!(opts.browser, Some(PathBuf::from("/custom/chrome")));
        }
    }
    #[test]
    fn missing_values_and_conflicting_operations_fail() {
        for args in [
            vec!["--browser-path"],
            vec!["--browser-path", "--logout"],
            vec!["--login", "--logout"],
            vec!["--unknown"],
        ] {
            assert!(Options::parse(args.into_iter().map(str::to_string)).is_err());
        }
    }
}
