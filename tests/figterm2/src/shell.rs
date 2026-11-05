use std::io::{Read, Write};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use fig_proto::local::{EditBufferHook, InterceptedKeyHook, PostExecHook, PreExecHook, PromptHook, ShellContext};
use fig_proto::remote::clientbound;
use fig_remote_ipc::RemoteHookHandler;
use fig_remote_ipc::figterm::FigtermState;
use fig_util::RUNTIME_DIR_NAME;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use tempfile::TempDir;
use tokio::net::UnixListener;
use tokio::sync::watch;
use tokio::task::{JoinHandle, JoinSet};
use uuid::Uuid;

const TIMEOUT: Duration = Duration::from_secs(15);
const PROMPT: &str = "ZEVU_PTY_READY> ";

#[derive(Debug, Clone, Default)]
struct HookState {
    buffer: Option<String>,
    cursor: Option<i64>,
    context: Option<ShellContext>,
    prompts: usize,
    pre_execs: usize,
    post_execs: usize,
    exit_code: Option<i32>,
}

#[derive(Debug, Clone)]
struct RemoteHook {
    state: watch::Sender<HookState>,
}

#[async_trait::async_trait]
impl RemoteHookHandler for RemoteHook {
    type Error = anyhow::Error;

    async fn edit_buffer(
        &mut self,
        hook: &EditBufferHook,
        _session_id: Uuid,
        _figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<clientbound::response::Response>> {
        self.state.send_modify(|state| {
            state.buffer = Some(hook.text.clone());
            state.cursor = Some(hook.cursor);
        });
        Ok(None)
    }

    async fn prompt(
        &mut self,
        _hook: &PromptHook,
        _session_id: Uuid,
        _figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<clientbound::response::Response>> {
        self.state.send_modify(|state| state.prompts += 1);
        Ok(None)
    }

    async fn pre_exec(
        &mut self,
        _hook: &PreExecHook,
        _session_id: Uuid,
        _figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<clientbound::response::Response>> {
        self.state.send_modify(|state| state.pre_execs += 1);
        Ok(None)
    }

    async fn post_exec(
        &mut self,
        hook: &PostExecHook,
        _session_id: Uuid,
        _figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<clientbound::response::Response>> {
        self.state.send_modify(|state| {
            state.post_execs += 1;
            state.exit_code = hook.exit_code;
        });
        Ok(None)
    }

    async fn intercepted_key(
        &mut self,
        _hook: InterceptedKeyHook,
        _session_id: Uuid,
    ) -> Result<Option<clientbound::response::Response>> {
        Ok(None)
    }

    async fn shell_context(&mut self, context: &ShellContext, _session_id: Uuid) {
        self.state.send_modify(|state| state.context = Some(context.clone()));
    }
}

/// A real figterm process and shell with no inherited user environment or dotfiles.
pub struct Shell {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    state: watch::Receiver<HookState>,
    output: Arc<Mutex<Vec<u8>>>,
    output_changed: watch::Receiver<()>,
    listener_task: JoinHandle<()>,
    reader_thread: Option<std::thread::JoinHandle<()>>,
    _tempdir: TempDir,
}

impl Shell {
    pub async fn init(shell: &str) -> Result<Self> {
        let executable = match shell {
            "zsh" => PathBuf::from("/bin/zsh"),
            "bash" | "fish" => {
                let prefix = Command::new("brew").arg("--prefix").output().context("Finding Homebrew")?;
                anyhow::ensure!(prefix.status.success(), "brew --prefix failed");
                PathBuf::from(String::from_utf8(prefix.stdout)?.trim()).join("bin").join(shell)
            },
            _ => bail!("Unsupported regression shell: {shell}"),
        };
        anyhow::ensure!(executable.is_file(), "Required shell is missing: {}", executable.display());

        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize()?;
        let cli = root.join("target/debug/q_cli");
        let figterm = root.join("target/debug/figterm");
        anyhow::ensure!(cli.is_file() && figterm.is_file(), "Build debug q_cli and figterm before running PTY regressions");

        // Short paths keep AF_UNIX addresses below macOS's sockaddr_un limit.
        let tempdir = tempfile::Builder::new().prefix("zevu-pty-").tempdir_in("/tmp")?;
        let home = tempdir.path().join("home");
        let config = tempdir.path().join("config");
        let zdotdir = tempdir.path().join("zsh");
        let tmp = tempdir.path().join("tmp");
        let bin = home.join(".local/bin");
        for directory in [&home, &config, &zdotdir, &tmp, &bin] {
            std::fs::create_dir_all(directory)?;
        }
        symlink(cli, bin.join("zevu"))?;
        symlink(&figterm, bin.join("zevu-term"))?;

        // Exercise the production init command, not a hand-written approximation of hooks.
        let profile = match shell {
            "bash" => {
                std::fs::write(home.join(".inputrc"), "set editing-mode emacs\nset enable-bracketed-paste on\n")?;
                home.join(".bashrc")
            },
            "zsh" => {
                std::fs::write(zdotdir.join(".zshenv"), "unsetopt GLOBAL_RCS\n")?;
                zdotdir.join(".zshrc")
            },
            "fish" => {
                std::fs::create_dir_all(config.join("fish"))?;
                config.join("fish/config.fish")
            },
            _ => unreachable!(),
        };
        let source = match shell {
            "bash" => format!("eval \"$(zevu init bash pre)\"\nPS1='{PROMPT}'\nbind 'set editing-mode emacs'\neval \"$(zevu init bash post)\"\n"),
            "zsh" => format!("eval \"$(zevu init zsh pre)\"\nPROMPT='{PROMPT}'\nRPROMPT=''\nbindkey -e\neval \"$(zevu init zsh post)\"\n"),
            "fish" => format!("zevu init fish pre | source\nfunction fish_prompt; printf '{PROMPT}'; end\nfunction fish_right_prompt; end\nset -g fish_greeting\nset -g fish_autosuggestion_enabled 0\nfish_default_key_bindings\nzevu init fish post | source\n"),
            _ => unreachable!(),
        };
        std::fs::write(profile, source)?;

        let runtime_dir = tmp.join(RUNTIME_DIR_NAME);
        std::fs::create_dir_all(&runtime_dir)?;
        std::fs::set_permissions(&runtime_dir, std::fs::Permissions::from_mode(0o700))?;
        // Bind before spawning figterm: its first connection must hit this fixture.
        let listener = UnixListener::bind(runtime_dir.join("remote.sock"))?;
        let (sender, state) = watch::channel(HookState::default());
        let figterm_state = Arc::new(FigtermState::new());
        let listener_task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let Ok((stream, _)) = accepted else { break };
                        connections.spawn(fig_remote_ipc::remote::handle_remote_ipc(
                            stream,
                            figterm_state.clone(),
                            RemoteHook { state: sender.clone() },
                        ));
                    },
                    _ = connections.join_next(), if !connections.is_empty() => {},
                }
            }
        });

        let pty_pair = native_pty_system().openpty(PtySize {
            cols: 80,
            rows: 24,
            ..Default::default()
        })?;
        let mut cmd = CommandBuilder::new(figterm);
        cmd.args(["--", executable.to_str().context("Non-UTF8 shell path")?, "-i"]);
        cmd.env_clear();
        cmd.cwd(&home);
        cmd.env("HOME", &home);
        cmd.env("XDG_CONFIG_HOME", &config);
        cmd.env("XDG_DATA_HOME", tempdir.path().join("data"));
        cmd.env("XDG_CACHE_HOME", tempdir.path().join("cache"));
        cmd.env("XDG_STATE_HOME", tempdir.path().join("state"));
        cmd.env("ZDOTDIR", &zdotdir);
        cmd.env("TMPDIR", &tmp);
        cmd.env("XDG_RUNTIME_DIR", &tmp);
        cmd.env("PATH", format!("{}:{}:/usr/bin:/bin:/usr/sbin:/sbin", bin.display(), executable.parent().unwrap().display()));
        cmd.env("SHELL", &executable);
        cmd.env("USER", "zevu-pty-test");
        cmd.env("TERM", "xterm-256color");
        // GUI terminal launchers may supply an existing but empty TTY variable.
        cmd.env("TTY", "");
        cmd.env("LANG", "en_US.UTF-8");
        cmd.env("LC_ALL", "en_US.UTF-8");
        let child = pty_pair.slave.spawn_command(cmd)?;
        // No parent-side slave handle may keep the drain thread alive after exit.
        drop(pty_pair.slave);
        let writer = pty_pair.master.take_writer()?;
        let mut reader = pty_pair.master.try_clone_reader()?;
        let output = Arc::new(Mutex::new(Vec::new()));
        let (output_sender, output_changed) = watch::channel(());
        let reader_thread = std::thread::spawn({
            let output = output.clone();
            move || {
                let mut bytes = [0; 4096];
                while let Ok(count) = reader.read(&mut bytes) {
                    if count == 0 { break; }
                    output.lock().unwrap().extend_from_slice(&bytes[..count]);
                    output_sender.send_replace(());
                }
            }
        });
        let mut instance = Self {
            master: pty_pair.master,
            writer,
            child,
            state,
            output,
            output_changed,
            listener_task,
            reader_thread: Some(reader_thread),
            _tempdir: tempdir,
        };
        instance.wait_output(PROMPT).await?;
        // Startup hooks may precede the socket handshake and be drained by figterm.
        // Keep requesting a fresh prompt until one is observed over the live transport.
        let prompts = instance.state.borrow().prompts;
        let ready = tokio::time::timeout(TIMEOUT, async {
            let mut retry = tokio::time::interval(Duration::from_millis(100));
            loop {
                {
                    let state = instance.state.borrow();
                    if state.prompts > prompts && state.context.is_some() {
                        return Ok::<_, anyhow::Error>(());
                    }
                }
                tokio::select! {
                    changed = instance.state.changed() => changed.context("Shell hook stream closed")?,
                    _ = retry.tick() => instance.write("\r")?,
                }
            }
        }).await;
        match ready {
            Ok(result) => result?,
            Err(_) => bail!("Timed out waiting for initial shell hooks:\n{}", instance.transcript()),
        }
        let context = instance.state.borrow().context.clone().context("Missing shell context")?;
        anyhow::ensure!(
            context.shell_path.as_deref().map(std::path::Path::new)
                .and_then(|path| path.canonicalize().ok()) == Some(executable.canonicalize()?),
            "Unexpected shell executable: {context:?}",
        );
        anyhow::ensure!(
            context.current_working_directory.as_deref().map(std::path::Path::new)
                .and_then(|path| path.canonicalize().ok()) == Some(home.canonicalize()?),
            "Shell did not start in its isolated HOME: {context:?}",
        );
        Ok(instance)
    }

    fn transcript(&self) -> String {
        String::from_utf8_lossy(&self.output.lock().unwrap()).into_owned()
    }

    async fn wait_hooks(&mut self, predicate: impl Fn(&HookState) -> bool) -> Result<()> {
        let matched = matches!(
            tokio::time::timeout(TIMEOUT, self.state.wait_for(predicate)).await,
            Ok(Ok(_))
        );
        if !matched {
            bail!("Timed out waiting for shell hook; state={:?}\nPTY output:\n{}", *self.state.borrow(), self.transcript());
        }
        Ok(())
    }

    pub fn write(&mut self, text: &str) -> Result<()> {
        self.writer.write_all(text.as_bytes())?;
        self.writer.flush()?;
        Ok(())
    }

    pub async fn typed(&mut self, text: &str) -> Result<()> {
        for character in text.chars() {
            self.write(character.encode_utf8(&mut [0; 4]))?;
            tokio::task::yield_now().await;
        }
        Ok(())
    }

    /// Buffer text and byte cursor must come from the same edit-buffer hook.
    pub async fn expect_edit_buffer(&mut self, expected: &str, cursor: usize) -> Result<()> {
        self.wait_hooks(|state| {
            state.buffer.as_deref() == Some(expected) && state.cursor == Some(cursor as i64)
        }).await
    }

    pub async fn wait_output(&mut self, expected: &str) -> Result<()> {
        let result = tokio::time::timeout(TIMEOUT, async {
            loop {
                if self.transcript().contains(expected) {
                    return Ok::<_, anyhow::Error>(());
                }
                self.output_changed.changed().await.context("PTY output stream closed")?;
            }
        }).await;
        match result {
            Ok(result) => result,
            Err(_) => bail!("Timed out waiting for output {expected:?}; PTY output:\n{}", self.transcript()),
        }
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.master.resize(PtySize { cols, rows, ..Default::default() })?;
        Ok(())
    }

    pub async fn reset(&mut self) -> Result<()> {
        self.write("\x03")?;
        self.expect_edit_buffer("", 0).await?;
        self.resize(80, 24)?;
        Ok(())
    }

    pub async fn execute(&mut self) -> Result<()> {
        let prompts = self.state.borrow().prompts;
        let pre_execs = self.state.borrow().pre_execs;
        let post_execs = self.state.borrow().post_execs;
        self.write("\r")?;
        self.wait_hooks(|state| {
            state.prompts > prompts && state.pre_execs > pre_execs && state.post_execs > post_execs
        }).await?;
        anyhow::ensure!(
            self.state.borrow().exit_code == Some(0),
            "Shell command failed; state={:?}\nPTY output:\n{}",
            *self.state.borrow(), self.transcript(),
        );
        Ok(())
    }
}

impl Drop for Shell {
    fn drop(&mut self) {
        // Let the shell exit so that its PTY and the continuous drain thread close too.
        let _ = self.writer.write_all(b"\x03exit\r");
        let _ = self.writer.flush();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) { break; }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.listener_task.abort();
        // Closing figterm and the parent-side slave lets the continuous drain reach EOF.
        if let Some(reader) = self.reader_thread.take() {
            let _ = reader.join();
        }
    }
}
