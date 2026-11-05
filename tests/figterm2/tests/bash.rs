use figterm2::Shell;

async fn shell(name: &str) {
    let mut shell = Shell::init(name).await.unwrap();

    // Startup must resolve an empty inherited TTY to the real controlling terminal.
    let tty_check = if name == "fish" {
        "test \"$TTY\" = (command tty)"
    } else {
        "test \"$TTY\" = \"$(tty)\""
    };
    shell.typed(tty_check).await.unwrap();
    shell.expect_edit_buffer(tty_check, tty_check.len()).await.unwrap();
    shell.execute().await.unwrap();

    let basic = "echo hello world";
    shell.typed(basic).await.unwrap();
    shell.expect_edit_buffer(basic, basic.len()).await.unwrap();
    shell.typed("\x08").await.unwrap();
    shell.expect_edit_buffer("echo hello worl", basic.len() - 1).await.unwrap();
    shell.reset().await.unwrap();

    // A multibyte, double-width character is one shell editing character, not bytes.
    let unicode = "printf 'ZEVU_UNICODE=%s\\n' café界";
    shell.typed(unicode).await.unwrap();
    shell.expect_edit_buffer(unicode, unicode.len()).await.unwrap();
    shell.typed("\x08").await.unwrap();
    let unicode_prefix = "printf 'ZEVU_UNICODE=%s\\n' café";
    shell.expect_edit_buffer(unicode_prefix, unicode_prefix.len()).await.unwrap();
    shell.typed("界").await.unwrap();
    shell.expect_edit_buffer(unicode, unicode.len()).await.unwrap();
    shell.execute().await.unwrap();
    shell.wait_output("ZEVU_UNICODE=café界\r\n").await.unwrap();

    // Edit a token in the middle without losing the suffix or executing the command.
    let original = "printf 'ZEVU_MID=%s\\n' MID_OLD_END";
    let edited = "printf 'ZEVU_MID=%s\\n' MID_NEW_END";
    let before_suffix = "printf 'ZEVU_MID=%s\\n' MID_OLD";
    let edited_prefix = "printf 'ZEVU_MID=%s\\n' MID_NEW";
    shell.typed(original).await.unwrap();
    shell.expect_edit_buffer(original, original.len()).await.unwrap();
    shell.typed("\x01").await.unwrap();
    shell.expect_edit_buffer(original, 0).await.unwrap();
    shell.typed(&"\x06".repeat(before_suffix.len())).await.unwrap();
    shell.expect_edit_buffer(original, before_suffix.len()).await.unwrap();
    shell.typed("\x08\x08\x08NEW").await.unwrap();
    shell.expect_edit_buffer(edited, edited_prefix.len()).await.unwrap();
    shell.execute().await.unwrap();
    shell.wait_output("ZEVU_MID=MID_NEW_END\r\n").await.unwrap();

    // Reflow a wrapped edit buffer and prove SIGWINCH reached the inner shell's PTY.
    let long = format!("echo {}", "1234567890".repeat(9));
    shell.typed(&long).await.unwrap();
    shell.expect_edit_buffer(&long, long.len()).await.unwrap();
    shell.resize(40, 30).unwrap();
    shell.typed("tail").await.unwrap();
    let resized = format!("{long}tail");
    shell.expect_edit_buffer(&resized, resized.len()).await.unwrap();
    shell.execute().await.unwrap();
    let size = "stty size";
    shell.typed(size).await.unwrap();
    shell.expect_edit_buffer(size, size.len()).await.unwrap();
    shell.execute().await.unwrap();
    shell.wait_output("30 40\r\n").await.unwrap();
}

#[tokio::test]
async fn bash() {
    shell("bash").await;
}

#[tokio::test]
async fn zsh() {
    shell("zsh").await;
}

#[tokio::test]
async fn fish() {
    shell("fish").await;
}
