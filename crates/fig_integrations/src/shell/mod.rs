use std::borrow::Cow;
use std::io::{
    ErrorKind,
    Write,
};
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use cfg_if::cfg_if;
use clap::ValueEnum;
use fig_os_shim::Env;
use fig_util::{
    CLI_BINARY_NAME,
    PRODUCT_NAME,
    PTY_BINARY_NAME,
    Shell,
    directories,
};
use regex::Regex;
use serde::{
    Deserialize,
    Serialize,
};

use crate::error::{
    ErrorExt,
    Result,
};
use crate::{
    Error,
    FileIntegration,
    Integration,
    backup_file,
};


#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum When {
    Pre,
    Post,
}

impl When {
    pub fn all() -> [When; 2] {
        [Self::Pre, Self::Post]
    }
}

impl std::fmt::Display for When {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            When::Pre => write!(f, "pre"),
            When::Post => write!(f, "post"),
        }
    }
}

fn integration_file_name(dotfile_name: &str, when: &When, shell: &Shell) -> String {
    format!(
        "{}.{when}.{shell}",
        Regex::new(r"^\.").unwrap().replace_all(dotfile_name, ""),
    )
}

/// Locate an executable shell even when a GUI launch omits Homebrew from PATH.
pub fn shell_executable(shell: Shell, env: &Env) -> Option<PathBuf> {
    let mut directories: Vec<PathBuf> = env.get_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    #[cfg(target_os = "macos")]
    directories.extend(["/opt/homebrew/bin", "/usr/local/bin", "/bin", "/usr/bin"].map(PathBuf::from));
    directories.into_iter().map(|directory| directory.join(shell.as_str())).find(|path| {
        if !path.is_file() {
            return false;
        }
        #[cfg(unix)]
        {
            nix::unistd::access(path.as_path(), nix::unistd::AccessFlags::X_OK).is_ok()
        }
        #[cfg(not(unix))]
        {
            true
        }
    })
}

pub trait ShellExt {
    fn get_shell_integrations(&self, env: &Env) -> Result<Vec<Box<dyn ShellIntegration>>>;
    /// Script integrations are installed into the Zevu data directory.
    fn get_script_integrations(&self) -> Result<Vec<ShellScriptShellIntegration>>;
    fn get_fig_integration_source(&self, when: &When) -> String;
}

impl ShellExt for Shell {
    fn get_script_integrations(&self) -> Result<Vec<ShellScriptShellIntegration>> {
        let mut integrations = vec![];

        for file in match self {
            Shell::Bash => [".bashrc", ".bash_profile", ".bash_login", ".profile"].iter(),
            Shell::Zsh => [".zshrc", ".zprofile"].iter(),
            Shell::Fish | Shell::Nu => [].iter(),
        } {
            for when in &When::all() {
                let path = directories::fig_data_dir()?
                    .join("shell")
                    .join(integration_file_name(file, when, self));

                integrations.push(ShellScriptShellIntegration {
                    shell: *self,
                    when: *when,
                    path,
                });
            }
        }

        Ok(integrations)
    }

    fn get_shell_integrations(&self, env: &Env) -> Result<Vec<Box<dyn ShellIntegration>>> {
        let config_dir = self.get_config_directory(env)?;

        let integrations: Vec<Box<dyn ShellIntegration>> = match self {
            Shell::Bash => {
                let mut configs = vec![".bashrc"];
                let other_configs = [".profile", ".bash_login", ".bash_profile"];

                configs.extend(other_configs.into_iter().filter(|f| config_dir.join(f).exists()));

                // Include .profile if none of [.profile, .bash_login, .bash_profile] exist.
                if configs.len() == 1 {
                    configs.push(other_configs[0]);
                }

                configs
                    .into_iter()
                    .map(|filename| {
                        Box::new(DotfileShellIntegration {
                            pre: true,
                            post: true,
                            shell: *self,
                            dotfile_directory: config_dir.clone(),
                            dotfile_name: filename,
                        }) as Box<dyn ShellIntegration>
                    })
                    .collect()
            },
            Shell::Zsh => vec![".zshrc", ".zprofile"]
                .into_iter()
                .map(|filename| {
                    Box::new(DotfileShellIntegration {
                        pre: true,
                        post: true,
                        shell: *self,
                        dotfile_directory: config_dir.clone(),
                        dotfile_name: filename,
                    }) as Box<dyn ShellIntegration>
                })
                .collect(),
            Shell::Fish => {
                let fish_config_dir = config_dir.join("conf.d");
                vec![
                    Box::new(ShellScriptShellIntegration {
                        when: When::Pre,
                        shell: *self,
                        path: fish_config_dir.join("00_zevu_pre.fish"),
                    }),
                    Box::new(ShellScriptShellIntegration {
                        when: When::Post,
                        shell: *self,
                        path: fish_config_dir.join("99_zevu_post.fish"),
                    }),
                ]
            },
            Shell::Nu => vec![],
        };

        Ok(integrations)
    }

    fn get_fig_integration_source(&self, when: &When) -> String {
        let script = match (self, when) {
            (Shell::Fish, When::Pre) => include_str!("scripts/pre.fish"),
            (Shell::Fish, When::Post) => include_str!("scripts/post.fish"),
            (Shell::Zsh, When::Pre) => include_str!("scripts/pre.sh"),
            (Shell::Zsh, When::Post) => include_str!("scripts/post.zsh"),
            (Shell::Bash, When::Pre) => {
                concat!(
                    "function __fig_source_bash_preexec() {\n",
                    include_str!("scripts/bash-preexec.sh"),
                    "}\n",
                    "__fig_source_bash_preexec\n",
                    "# shellcheck disable=SC2329\n",
                    "function __bp_adjust_histcontrol() { :; }\n",
                    include_str!("scripts/pre.sh")
                )
            },
            (Shell::Bash, When::Post) => {
                concat!(
                    "function __fig_source_bash_preexec() {\n",
                    include_str!("scripts/bash-preexec.sh"),
                    "}\n",
                    "__fig_source_bash_preexec\n",
                    "# shellcheck disable=SC2329\n",
                    "function __bp_adjust_histcontrol() { :; }\n",
                    include_str!("scripts/post.bash")
                )
            },
            (Shell::Nu, When::Pre) => include_str!("scripts/pre.nu"),
            (Shell::Nu, When::Post) => include_str!("scripts/post.nu"),
        };

        script
            .replace("{{CLI_BINARY_NAME}}", CLI_BINARY_NAME)
            .replace("{{PTY_BINARY_NAME}}", PTY_BINARY_NAME)
    }
}

pub trait ShellIntegration: Send + Sync + Integration + ShellIntegrationClone {
    // The unique name of the integration file
    fn file_name(&self) -> &str;
    fn get_shell(&self) -> Shell;
    fn path(&self) -> PathBuf;
}

impl std::fmt::Display for dyn ShellIntegration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.get_shell(), self.path().display())
    }
}

pub trait ShellIntegrationClone {
    fn clone_box(&self) -> Box<dyn ShellIntegration>;
}

impl<T> ShellIntegrationClone for T
where
    T: 'static + ShellIntegration + Clone,
{
    fn clone_box(&self) -> Box<dyn ShellIntegration> {
        Box::new(self.clone())
    }
}

// We can now implement Clone manually by forwarding to clone_box.
impl Clone for Box<dyn ShellIntegration> {
    fn clone(&self) -> Box<dyn ShellIntegration> {
        self.clone_box()
    }
}

#[derive(Debug, Clone)]
pub struct ShellScriptShellIntegration {
    pub shell: Shell,
    pub when: When,
    pub path: PathBuf,
}

fn get_prefix(s: &str) -> &str {
    match s.find('.') {
        Some(i) => &s[..i],
        None => s,
    }
}

impl ShellScriptShellIntegration {
    fn get_file_integration(&self) -> FileIntegration {
        FileIntegration {
            path: self.path.clone(),
            contents: self.get_contents(),
            #[cfg(unix)]
            mode: None,
        }
    }

    fn get_name(&self) -> Option<&str> {
        self.path.file_name().and_then(|s| s.to_str())
    }

    #[allow(clippy::needless_return)]
    fn get_contents(&self) -> String {
        let Self { shell, when, path } = self;
        let rcfile = match path.file_name().and_then(|x| x.to_str()) {
            Some(name) => format!(" --rcfile {}", get_prefix(name)),
            None => "".into(),
        };
        cfg_if!(
            if #[cfg(target_os = "macos")] {
                return match self.shell {
                    // Check if ~/.local/bin/{CLI_BINARY_NAME} is executable before eval
                    Shell::Bash | Shell::Zsh => format!("[ -x ~/.local/bin/{CLI_BINARY_NAME} ] && eval \"$(~/.local/bin/{CLI_BINARY_NAME} init {shell} {when}{rcfile})\""),
                    Shell::Fish => format!("test -x ~/.local/bin/{CLI_BINARY_NAME}; and eval (~/.local/bin/{CLI_BINARY_NAME} init {shell} {when}{rcfile} | string split0)"),
                    Shell::Nu => "".into(),
                }
            } else {
                let add_to_path_line = match self.shell {
                    Shell::Bash | Shell::Zsh => indoc::indoc! {r#"
                        _Q_LOCAL_BIN="$HOME/.local/bin"
                        [[ ":$PATH:" != *":$_Q_LOCAL_BIN:"* ]] && PATH="${PATH:+"$PATH:"}$_Q_LOCAL_BIN"
                        unset _Q_LOCAL_BIN
                    "#},
                    Shell::Fish => "contains $HOME/.local/bin $PATH; or set -a PATH $HOME/.local/bin",
                    Shell::Nu => "",
                };

                let source_line = match self.shell {
                    Shell::Fish => format!("command -qv {CLI_BINARY_NAME}; and eval ({CLI_BINARY_NAME} init {shell} {when}{rcfile} | string split0)"),
                    Shell::Bash | Shell::Zsh => {
                        // Check that the current shell is bash
                        let bash_pre = if self.shell.is_bash() { "[ -n \"$BASH_VERSION\" ] && " } else { "" };
                        format!("{bash_pre}command -v {CLI_BINARY_NAME} >/dev/null 2>&1 && eval \"$({CLI_BINARY_NAME} init {shell} {when}{rcfile})\"")
                    }
                    Shell::Nu => "".into(),
                };

                return format!("{add_to_path_line}\n{source_line}\n");
            }
        );
    }
}

#[async_trait]
impl Integration for ShellScriptShellIntegration {
    async fn is_installed(&self) -> Result<()> {
        self.get_file_integration().is_installed().await
    }

    async fn install(&self) -> Result<()> {
        self.get_file_integration().install().await
    }

    async fn uninstall(&self) -> Result<()> {
        match self.get_file_integration().is_installed().await {
            Ok(()) => self.get_file_integration().uninstall().await,
            Err(Error::FileDoesNotExist(_)) => Ok(()),
            Err(err) => Err(err),
        }
    }

    fn describe(&self) -> String {
        format!("{} {}", self.shell, self.when)
    }

    async fn migrate(&self) -> Result<()> {
        self.get_file_integration().install().await
    }
}

impl ShellIntegration for ShellScriptShellIntegration {
    fn file_name(&self) -> &str {
        self.get_name().unwrap_or("unknown_script")
    }

    fn get_shell(&self) -> Shell {
        self.shell
    }

    fn path(&self) -> PathBuf {
        self.path.clone()
    }
}

/// zsh and bash integration where we modify a dotfile with pre/post hooks that reference script
/// files.
#[derive(Debug, Clone)]
pub struct DotfileShellIntegration {
    pub shell: Shell,
    pub pre: bool,
    pub post: bool,
    pub dotfile_directory: PathBuf,
    pub dotfile_name: &'static str,
}

impl DotfileShellIntegration {
    fn dotfile_path(&self) -> PathBuf {
        self.dotfile_directory.join(self.dotfile_name)
    }


    fn script_integration(&self, when: When) -> Result<ShellScriptShellIntegration> {
        let integration_file_name = format!(
            "{}.{}.{}",
            Regex::new(r"^\.").unwrap().replace_all(self.dotfile_name, ""),
            when,
            self.shell
        );
        Ok(ShellScriptShellIntegration {
            shell: self.shell,
            when,
            path: directories::fig_data_dir()?.join("shell").join(integration_file_name),
        })
    }

    #[allow(clippy::unused_self)]
    fn description(&self, when: When) -> String {
        match when {
            When::Pre => format!("# {PRODUCT_NAME} pre block. Keep at the top of this file."),
            When::Post => format!("# {PRODUCT_NAME} post block. Keep at the bottom of this file."),
        }
    }


    fn source_text(&self, when: When) -> Result<String> {
        let home = directories::home_dir()?;
        let integration_path = self.script_integration(when)?.path;
        let path = format!("\"${{HOME}}/{}\"", integration_path.strip_prefix(home)?.display());

        match self.shell {
            Shell::Fish => Ok(format!("test -f {path}; and builtin source {path}")),
            _ => Ok(format!("[[ -f {path} ]] && builtin source {path}")),
        }
    }

    fn source_regex(&self, when: When, constrain_position: bool) -> Result<Regex> {
        let block = match when {
            When::Pre => format!("{}\n{}\n", self.description(when), self.source_text(when)?),
            When::Post => format!("\n{}\n{}\n", self.description(when), self.source_text(when)?),
        };
        Ok(Regex::new(&format!(
            "{}{}{}",
            if constrain_position && when == When::Pre { r"\A(?:#![^\n]*\n)?" } else { "" },
            regex::escape(&block),
            if constrain_position && when == When::Post { r"\z" } else { "" },
        ))?)
    }

    fn remove_from_text(&self, text: String, when: When) -> Result<String> {
        Ok(self.source_regex(when, false)?.replace_all(&text, "").into_owned())
    }

    fn matches_text(&self, text: &str, when: When) -> Result<()> {
        let dotfile = self.dotfile_path();
        remove_legacy_zevu_block(text)?;
        if !self.source_regex(when, false)?.is_match(text) {
            let message = format!("{} does not source {} integration", dotfile.display(), when);
            return Err(Error::NotInstalled(message.into()));
        }
        if !self.source_regex(when, true)?.is_match(text) {
            let position = match when {
                When::Pre => "first",
                When::Post => "last",
            };
            let message = format!(
                "{} does not source {} integration {}",
                dotfile.display(),
                when,
                position
            );
            return Err(Error::ImproperInstallation(message.into()));
        }
        Ok(())
    }

    fn profile_contents(&self) -> Result<String> {
        match std::fs::read_to_string(self.dotfile_path()).with_path(self.path()) {
            Ok(contents) => {
                remove_legacy_zevu_block(&contents)?;
                Ok(contents)
            },
            Err(Error::Io(err)) if err.kind() == ErrorKind::NotFound => Ok(String::new()),
            Err(err) => Err(err),
        }
    }

    fn without_owned_blocks(&self, original: &str) -> Result<String> {
        let mut contents = remove_legacy_zevu_block(original)?.into_owned();
        if self.pre {
            contents = self.remove_from_text(contents, When::Pre)?;
        }
        if self.post {
            contents = self.remove_from_text(contents, When::Post)?;
        }
        Ok(contents)
    }

    fn installed_contents(&self, original: &str) -> Result<String> {
        let mut contents = self.without_owned_blocks(original)?;
        if self.pre {
            let (shebang, rest) = split_shebang(&contents);
            contents = format!("{shebang}{}\n{}\n{rest}", self.description(When::Pre), self.source_text(When::Pre)?);
        }
        if self.post {
            contents = format!("{contents}\n{}\n{}\n", self.description(When::Post), self.source_text(When::Post)?);
        }
        Ok(contents)
    }

    #[cfg(test)]
    fn install_profile(&self, backup_dir: &Path) -> Result<()> {
        let original = self.profile_contents()?;
        let contents = self.installed_contents(&original)?;
        self.write_profile(&original, &contents, backup_dir)
    }

    fn uninstall_profile(&self, backup_dir: &Path) -> Result<()> {
        let original = self.profile_contents()?;
        let contents = self.without_owned_blocks(&original)?;
        self.write_profile(&original, &contents, backup_dir)
    }

    fn prepare_profile(&self, original: &str, contents: &str) -> Result<Option<(tempfile::NamedTempFile, PathBuf)>> {
        if original == contents {
            return Ok(None);
        }
        let target = writable_target(&self.dotfile_path())?;
        let parent = target.parent().ok_or_else(|| Error::Custom("Profile has no parent directory".into()))?;
        std::fs::create_dir_all(parent).with_path(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent).with_path(parent)?;
        if let Ok(metadata) = std::fs::metadata(&target) {
            file.as_file().set_permissions(metadata.permissions()).with_path(&target)?;
        }
        file.write_all(contents.as_bytes()).with_path(&target)?;
        file.as_file().sync_all().with_path(&target)?;
        Ok(Some((file, target)))
    }

    fn write_profile(&self, original: &str, contents: &str, backup_dir: &Path) -> Result<()> {
        if let Some((file, target)) = self.prepare_profile(original, contents)? {
            backup_file(self.dotfile_path(), Some(backup_dir))?;
            file.persist(&target).map_err(|err| err.error).with_path(&target)?;
        }
        Ok(())
    }
}

#[async_trait]
impl Integration for DotfileShellIntegration {
    fn describe(&self) -> String {
        format!(
            "{}{}{} into {}",
            self.shell,
            if self.pre { " pre" } else { "" },
            if self.post { " post" } else { "" },
            self.dotfile_name,
        )
    }

    async fn install(&self) -> Result<()> {
        let original = self.profile_contents()?;
        if self.is_installed().await.is_ok() {
            return Ok(());
        }
        let contents = self.installed_contents(&original)?;
        // Check every destination before staging the profile or changing any file.
        writable_target(&self.dotfile_path())?;
        let scripts = [When::Pre, When::Post]
            .into_iter()
            .filter(|when| match when { When::Pre => self.pre, When::Post => self.post })
            .map(|when| self.script_integration(when))
            .collect::<Result<Vec<_>>>()?;
        for script in &scripts {
            writable_target(&script.path)?;
        }
        let prepared = self.prepare_profile(&original, &contents)?;
        let backup_dir = directories::utc_backup_dir()?;
        if prepared.is_some() {
            backup_file(self.dotfile_path(), Some(&backup_dir))?;
        }
        for script in scripts {
            script.install().await?;
        }
        if let Some((file, target)) = prepared {
            file.persist(&target).map_err(|err| err.error).with_path(&target)?;
        }
        Ok(())
    }

    async fn uninstall(&self) -> Result<()> {
        self.profile_contents()?;
        let mut scripts = Vec::new();
        for when in [When::Pre, When::Post] {
            if !match when { When::Pre => self.pre, When::Post => self.post } {
                continue;
            }
            let script = self.script_integration(when)?;
            match script.is_installed().await {
                Ok(()) => {
                    writable_target(&script.path)?;
                    scripts.push(script);
                },
                Err(Error::FileDoesNotExist(_)) => {},
                Err(err) => return Err(err),
            }
        }
        self.uninstall_profile(&directories::utc_backup_dir()?)?;
        for script in scripts {
            script.uninstall().await?;
        }

        Ok(())
    }

    async fn is_installed(&self) -> Result<()> {
        let dotfile = self.dotfile_path();

        let contents = std::fs::read_to_string(&dotfile).with_path(&dotfile)?;
        if !matches!(remove_legacy_zevu_block(&contents)?, Cow::Borrowed(_)) {
            return Err(Error::LegacyInstallation("Legacy Zevu hook is still installed".into()));
        }
        // Only Zevu's explicit opt-out applies; other applications' markers are untouched.
        if Regex::new(r"(?mi)^\s*#\s*zevu ignore\s?.*$")?.is_match(&contents) {
            return Ok(());
        }

        if self.pre {
            self.matches_text(&contents, When::Pre)?;
            self.script_integration(When::Pre)?.is_installed().await?;
        }

        if self.post {
            self.matches_text(&contents, When::Post)?;
            self.script_integration(When::Post)?.is_installed().await?;
        }

        Ok(())
    }

    async fn migrate(&self) -> Result<()> {
        self.install().await
    }
}

impl ShellIntegration for DotfileShellIntegration {
    fn get_shell(&self) -> Shell {
        self.shell
    }

    fn path(&self) -> PathBuf {
        self.dotfile_path()
    }

    fn file_name(&self) -> &str {
        self.dotfile_name
    }
}

/// Resolve only the final file's symlinks, keeping profile links and their targets intact.
/// Opening without truncation checks file permissions before any replacement is staged.
fn writable_target(path: &Path) -> Result<PathBuf> {
    let mut target = path.to_path_buf();
    for _ in 0..40 {
        match std::fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let link = std::fs::read_link(&target).with_path(&target)?;
                target = if link.is_absolute() { link } else { target.parent().unwrap().join(link) };
            },
            Ok(_) => {
                std::fs::OpenOptions::new().write(true).open(&target).with_path(&target)?;
                #[cfg(unix)]
                if let Some(parent) = target.parent() {
                    nix::unistd::access(parent, nix::unistd::AccessFlags::W_OK | nix::unistd::AccessFlags::X_OK)
                        .map_err(std::io::Error::from).with_path(parent)?;
                }
                return Ok(target);
            },
            Err(err) if err.kind() == ErrorKind::NotFound => {
                let mut ancestor = target.parent();
                while let Some(parent) = ancestor {
                    if parent.exists() {
                        #[cfg(unix)]
                        nix::unistd::access(parent, nix::unistd::AccessFlags::W_OK | nix::unistd::AccessFlags::X_OK)
                            .map_err(std::io::Error::from).with_path(parent)?;
                        return Ok(target);
                    }
                    ancestor = parent.parent();
                }
                return Err(Error::Custom("Integration path has no existing parent".into()));
            },
            Err(err) => return Err(err).with_path(&target),
        }
    }
    Err(Error::Custom("Too many profile symlinks".into()))
}

/// Remove only the complete, former Zevu hook. Validate all markers before mutation.
fn remove_legacy_zevu_block(contents: &str) -> Result<Cow<'_, str>> {
    const START: &str = "# >>> Zevu autocomplete >>>";
    const END: &str = "# <<< Zevu autocomplete <<<";
    let mut start = None;
    let mut ranges = Vec::new();
    let mut offset = 0;
    for line in contents.split_inclusive('\n') {
        let marker = line.trim_end_matches(['\r', '\n']);
        match marker {
            START if start.is_none() => start = Some(offset),
            END if start.is_some() => ranges.push((start.take().unwrap(), offset + line.len())),
            START | END => return Err(Error::Custom("Malformed or nested legacy Zevu hook markers".into())),
            _ => {},
        }
        offset += line.len();
    }
    if start.is_some() {
        return Err(Error::Custom("Unclosed legacy Zevu hook marker".into()));
    }
    if ranges.is_empty() {
        return Ok(Cow::Borrowed(contents));
    }
    let mut result = String::with_capacity(contents.len());
    let mut cursor = 0;
    for (start, end) in ranges {
        result.push_str(&contents[cursor..start]);
        cursor = end;
    }
    result.push_str(&contents[cursor..]);
    Ok(Cow::Owned(result))
}

/// Splits the line containing the shebang (if any) with the rest of the string.
/// If the shebang exists, the newline is included. Otherwise, an empty slice is returned.
fn split_shebang(contents: &str) -> (&str, &str) {
    if contents.starts_with("#!") {
        match contents.find('\n') {
            Some(i) => (&contents[..i + 1], &contents[i + 1..]),
            None => ("", contents),
        }
    } else {
        ("", contents)
    }
}

#[cfg(test)]
mod test {
    use std::io::Write;
    use std::process::{
        Command,
        Stdio,
    };

    use fig_util::build::SKIP_SHELLCHECK_TESTS;

    use super::*;

    fn run_shellcheck(source: String) {
        if SKIP_SHELLCHECK_TESTS {
            return;
        }

        let shell_arg = "--shell=bash";
        let mut child = Command::new("shellcheck")
            .args([shell_arg, "--color=always", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        let mut stdin = child.stdin.take().unwrap();
        std::thread::spawn(move || {
            stdin.write_all(source.as_bytes()).unwrap();
        });

        let output = child.wait_with_output().unwrap();
        if !output.status.success() {
            let stdout = String::from_utf8(output.stdout).unwrap();
            let stderr = String::from_utf8(output.stderr).unwrap();

            if !stdout.is_empty() {
                println!("{stdout}");
            }

            if !stderr.is_empty() {
                eprintln!("{stderr}");
            }

            if stdout.contains("error") {
                panic!();
            }
        }
    }

    fn check_script(shell: Shell, when: When) {
        run_shellcheck(shell.get_fig_integration_source(&when));
    }

    #[test]
    fn shellcheck_bash_pre() {
        check_script(Shell::Bash, When::Pre);
    }

    #[test]
    fn shellcheck_bash_post() {
        check_script(Shell::Bash, When::Post);
    }


    fn profile(root: &Path, shell: Shell, name: &'static str) -> DotfileShellIntegration {
        DotfileShellIntegration {
            shell,
            pre: true,
            post: true,
            dotfile_directory: root.to_path_buf(),
            dotfile_name: name,
        }
    }

    #[test]
    fn profile_install_reinstall_uninstall_preserves_user_bytes_and_foreign_hooks() {
        let user = "#!/bin/zsh\n# Fig pre block. Keep at the top of this file.\r\n\
                    [[ -f ~/.fig/shell/zshrc.pre.zsh ]] && source ~/.fig/shell/zshrc.pre.zsh\r\n\
                    # Amazon Q post block. Keep at the bottom of this file.\n\
                    export USER_VALUE='résumé'\r\n\n# foreign footer without newline";
        for (shell, name) in [(Shell::Zsh, ".zshrc"), (Shell::Zsh, ".zprofile"), (Shell::Bash, ".bashrc"),
                              (Shell::Bash, ".bash_profile"), (Shell::Bash, ".bash_login"), (Shell::Bash, ".profile")] {
            let root = tempfile::tempdir().unwrap();
            let integration = profile(root.path(), shell, name);
            let backups = root.path().join("backups");
            std::fs::write(integration.path(), user.as_bytes()).unwrap();
            integration.install_profile(&backups).unwrap();
            let installed = std::fs::read(integration.path()).unwrap();
            integration.install_profile(&backups).unwrap();
            assert_eq!(std::fs::read(integration.path()).unwrap(), installed);
            let text = String::from_utf8(installed).unwrap();
            assert_eq!(text.matches("# Zevu pre block.").count(), 1);
            assert_eq!(text.matches("# Zevu post block.").count(), 1);
            integration.uninstall_profile(&backups).unwrap();
            integration.uninstall_profile(&backups).unwrap();
            assert_eq!(std::fs::read(integration.path()).unwrap(), user.as_bytes());
            assert_eq!(std::fs::read(backups.join(name)).unwrap(), user.as_bytes());
        }
    }

    #[test]
    fn legacy_zevu_hook_removal_backs_up_and_preserves_outside_bytes() {
        let root = tempfile::tempdir().unwrap();
        let integration = profile(root.path(), Shell::Zsh, ".zshrc");
        let backups = root.path().join("backups");
        let before = "export BEFORE='α'\r\n\n";
        let after = "\r\n# Amazon Q hook remains\nexport AFTER=1";
        let original = format!("{before}# >>> Zevu autocomplete >>>\r\nold helper\n# <<< Zevu autocomplete <<<\r\n{after}");
        std::fs::write(integration.path(), original.as_bytes()).unwrap();
        integration.install_profile(&backups).unwrap();
        let installed = std::fs::read(integration.path()).unwrap();
        integration.install_profile(&backups).unwrap();
        assert_eq!(std::fs::read(integration.path()).unwrap(), installed);
        assert!(!String::from_utf8(installed).unwrap().contains("old helper"));
        integration.uninstall_profile(&backups).unwrap();
        assert_eq!(std::fs::read(integration.path()).unwrap(), format!("{before}{after}").as_bytes());
        assert_eq!(std::fs::read(backups.join(".zshrc")).unwrap(), original.as_bytes());
    }

    #[tokio::test]
    async fn malformed_legacy_markers_never_mutate_profile_or_create_backup() {
        for contents in [
            "# >>> Zevu autocomplete >>>\nmissing end",
            "before\n# <<< Zevu autocomplete <<<\nafter",
            "# >>> Zevu autocomplete >>>\n# >>> Zevu autocomplete >>>\n# <<< Zevu autocomplete <<<\n",
            "# >>> Zevu autocomplete >>>\n# <<< Zevu autocomplete <<<\n# <<< Zevu autocomplete <<<",
        ] {
            let root = tempfile::tempdir().unwrap();
            let integration = profile(root.path(), Shell::Zsh, ".zshrc");
            let backups = root.path().join("backups");
            std::fs::write(integration.path(), contents).unwrap();
            assert!(integration.install().await.is_err());
            assert!(integration.uninstall().await.is_err());
            assert!(integration.install_profile(&backups).is_err());
            assert!(integration.uninstall_profile(&backups).is_err());
            assert_eq!(std::fs::read(integration.path()).unwrap(), contents.as_bytes());
            assert!(!backups.exists());
        }
    }

    #[test]
    fn custom_zdotdir_and_missing_directories_stay_isolated() {
        let root = tempfile::tempdir().unwrap();
        let zdotdir = root.path().join("custom/zsh");
        let env = Env::from_slice(&[("ZDOTDIR", zdotdir.to_str().unwrap())]);
        let integrations = Shell::Zsh.get_shell_integrations(&env).unwrap();
        assert_eq!(integrations.len(), 2);
        for integration in integrations {
            assert!(integration.path().starts_with(&zdotdir));
        }
        let integration = profile(&zdotdir, Shell::Zsh, ".zshrc");
        integration.install_profile(&root.path().join("backups")).unwrap();
        assert!(integration.path().is_file());
        integration.uninstall_profile(&root.path().join("backups")).unwrap();
        assert!(std::fs::read(integration.path()).unwrap().is_empty());
        assert!(!root.path().join(".zshrc").exists());
    }

    #[cfg(unix)]
    #[test]
    fn profile_links_keep_relative_absolute_and_dangling_targets() {
        use std::os::unix::fs::symlink;
        for relative in [true, false] {
            for existing in [true, false] {
                let root = tempfile::tempdir().unwrap();
                let target = root.path().join("managed/config/zshrc");
                let link = if relative { PathBuf::from("managed/config/zshrc") } else { target.clone() };
                let integration = profile(root.path(), Shell::Zsh, ".zshrc");
                let user = if existing { "export KEEP=1\r\n" } else { "" };
                if existing {
                    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
                    std::fs::write(&target, user).unwrap();
                }
                symlink(&link, integration.path()).unwrap();
                let backups = root.path().join("backups");
                integration.install_profile(&backups).unwrap();
                integration.install_profile(&backups).unwrap();
                assert_eq!(std::fs::read_link(integration.path()).unwrap(), link);
                integration.uninstall_profile(&backups).unwrap();
                assert_eq!(std::fs::read_link(integration.path()).unwrap(), link);
                assert_eq!(std::fs::read(&target).unwrap(), user.as_bytes());
            }
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn permission_denied_leaves_profile_and_backup_untouched() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let integration = profile(root.path(), Shell::Zsh, ".zshrc");
        let user = "# >>> Zevu autocomplete >>>\nold\n# <<< Zevu autocomplete <<<\nkeep\n";
        std::fs::write(integration.path(), user).unwrap();
        std::fs::set_permissions(integration.path(), std::fs::Permissions::from_mode(0o400)).unwrap();
        let backups = root.path().join("backups");
        assert!(integration.install().await.is_err());
        assert!(integration.uninstall().await.is_err());
        assert!(integration.install_profile(&backups).is_err());
        assert!(integration.uninstall_profile(&backups).is_err());
        assert_eq!(std::fs::read(integration.path()).unwrap(), user.as_bytes());
        assert!(!backups.exists());
    }

    #[tokio::test]
    async fn fish_scripts_round_trip_without_touching_foreign_conf() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("xdg");
        let env = Env::from_slice(&[("XDG_CONFIG_HOME", config.to_str().unwrap())]);
        let integrations = Shell::Fish.get_shell_integrations(&env).unwrap();
        let conf = config.join("fish/conf.d");
        std::fs::create_dir_all(&conf).unwrap();
        for foreign in ["00_fig_pre.fish", "00_q_pre.fish", "user.fish"] {
            std::fs::write(conf.join(foreign), format!("# keep {foreign}\r\n")).unwrap();
        }
        for integration in &integrations {
            integration.install().await.unwrap();
            let installed = std::fs::read(integration.path()).unwrap();
            integration.install().await.unwrap();
            assert_eq!(std::fs::read(integration.path()).unwrap(), installed);
            integration.uninstall().await.unwrap();
            integration.uninstall().await.unwrap();
            assert!(!integration.path().exists());
        }
        for foreign in ["00_fig_pre.fish", "00_q_pre.fish", "user.fish"] {
            assert_eq!(std::fs::read(conf.join(foreign)).unwrap(), format!("# keep {foreign}\r\n").as_bytes());
        }
        let owned_path = conf.join("00_zevu_pre.fish");
        std::fs::write(&owned_path, "# user-owned conflict\n").unwrap();
        assert!(integrations[0].uninstall().await.is_err());
        assert_eq!(std::fs::read_to_string(owned_path).unwrap(), "# user-owned conflict\n");
    }

    #[cfg(unix)]
    #[test]
    fn executable_shell_discovery_rejects_nonexecutable_files() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let shell = root.path().join("nu");
        std::fs::write(&shell, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o600)).unwrap();
        let env = Env::from_slice(&[("PATH", root.path().to_str().unwrap())]);
        assert_ne!(shell_executable(Shell::Nu, &env), Some(shell.clone()));
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(shell_executable(Shell::Nu, &env), Some(shell));
        assert!(Shell::all().contains(&Shell::Nu));
    }

    #[test]
    fn test_split_shebang() {
        let shebang = "#!/usr/bin/env sh";
        let contents = "echo hello world";
        let with_shebang = format!("{}\n{}", shebang, contents);
        let with_shebang_no_lf = format!("{}{}", shebang, contents);
        let without_shebang = contents;
        assert_eq!(
            (format!("{shebang}\n").as_str(), contents),
            split_shebang(&with_shebang),
            "split with shebang and linefeed"
        );
        assert_eq!(
            ("", format!("{shebang}{contents}").as_str()),
            split_shebang(&with_shebang_no_lf),
            "split with shebang and no linefeed"
        );
        assert_eq!(("", contents), split_shebang(without_shebang), "split with no shebang");
    }

    #[cfg(target_os = "linux")]
    fn all_dotfile_shell_integrations() -> Vec<ShellScriptShellIntegration> {
        Shell::all()
            .iter()
            .flat_map(|shell| shell.get_script_integrations().unwrap())
            .collect()
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn dotfile_shell_integrations_snapshot() {
        for integration in all_dotfile_shell_integrations() {
            let integration_name = format!(
                "{} {}",
                integration.describe(),
                integration.path.file_name().unwrap().to_str().unwrap()
            )
            .replace(' ', "_");
            insta::assert_snapshot!(integration_name, integration.get_contents());
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn dotfile_shell_integrations_shellcheck() {
        for integration in all_dotfile_shell_integrations() {
            run_shellcheck(integration.get_contents());
        }
    }
}
