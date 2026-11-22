use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

fn configure(root: &std::path::Path, shell: &str) -> Result<()> {
    let status = Command::new("/bin/sh")
        .args(["-c", include_str!("../path.sh")])
        .env("HOME", root)
        .env("SHELL", shell)
        .env_remove("ZDOTDIR")
        .env_remove("XDG_CONFIG_HOME")
        .status()?;
    assert!(status.success());
    Ok(())
}

#[test]
fn zsh_path_setup_is_idempotent_and_preserves_symlinks_and_existing_content() -> Result<()> {
    let root = tempfile::tempdir()?;
    let original = "export MY_SETTING=kept\n";
    std::fs::write(root.path().join("zshenv-config"), original)?;
    std::fs::set_permissions(
        root.path().join("zshenv-config"),
        std::fs::Permissions::from_mode(0o640),
    )?;
    symlink("zshenv-config", root.path().join(".zshenv"))?;
    configure(root.path(), "/bin/zsh")?;
    configure(root.path(), "/bin/zsh")?;
    assert!(root.path().join(".zshenv").is_symlink());
    assert_eq!(
        std::fs::metadata(root.path().join("zshenv-config"))?
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
    let text = std::fs::read_to_string(root.path().join(".zshenv"))?;
    assert_eq!(text.matches("case ").count(), 1);
    assert!(text.ends_with(original));
    let output = Command::new("/bin/sh")
        .args([
            "-c",
            ". \"$HOME/.zshenv\"; . \"$HOME/.zshenv\"; printf '%s' \"$PATH\"",
        ])
        .env("HOME", root.path())
        .env("PATH", "/usr/bin:/bin")
        .output()?;
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("{}/.local/bin:/usr/bin:/bin", root.path().display())
    );
    Ok(())
}

#[test]
fn bash_path_setup_precedes_noninteractive_early_returns_and_preserves_login_choice() -> Result<()>
{
    let root = tempfile::tempdir()?;
    std::fs::write(root.path().join(".bashrc"), "return\n")?;
    std::fs::write(
        root.path().join(".bash_login"),
        "export LOGIN_SETTING=kept\n",
    )?;
    configure(root.path(), "/bin/bash")?;
    assert!(!root.path().join(".bash_profile").exists());
    assert!(!root.path().join(".profile").exists());
    let output = Command::new("/bin/bash")
        .args([
            "--noprofile",
            "--norc",
            "-c",
            ". \"$HOME/.bashrc\"; printf '%s' \"$PATH\"",
        ])
        .env("HOME", root.path())
        .env("PATH", "/usr/bin:/bin")
        .output()?;
    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .starts_with(&format!("{}/.local/bin:", root.path().display()))
    );
    Ok(())
}

#[test]
fn custom_zsh_and_fish_config_directories_are_respected() -> Result<()> {
    let root = tempfile::tempdir()?;
    let zsh = root.path().join("custom zsh");
    std::fs::create_dir(&zsh)?;
    let status = Command::new("/bin/sh")
        .args(["-c", include_str!("../path.sh")])
        .env("HOME", root.path())
        .env("SHELL", "/bin/zsh")
        .env("ZDOTDIR", &zsh)
        .status()?;
    assert!(status.success());
    assert!(zsh.join(".zshenv").exists());
    assert!(!root.path().join(".zshenv").exists());
    configure(root.path(), "/usr/bin/fish")?;
    configure(root.path(), "/usr/bin/fish")?;
    assert_eq!(
        std::fs::read_to_string(root.path().join(".config/fish/conf.d/sheprd-path.fish"))?,
        "fish_add_path --global \"$HOME/.local/bin\"\n"
    );
    Ok(())
}

#[test]
fn a_failed_path_update_does_not_truncate_the_existing_config() -> Result<()> {
    let root = tempfile::tempdir()?;
    let original = "export MY_SETTING=kept\n";
    std::fs::write(root.path().join(".zshenv"), original)?;
    let bin = root.path().join("bin");
    std::fs::create_dir(&bin)?;
    crate::test_executable::write(bin.join("mv"), "#!/bin/sh\nexit 1\n", 0o755)?;
    let status = Command::new("/bin/sh")
        .args(["-c", include_str!("../path.sh")])
        .env("HOME", root.path())
        .env("SHELL", "/bin/zsh")
        .env_remove("ZDOTDIR")
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .status()?;
    assert!(!status.success());
    assert_eq!(
        std::fs::read_to_string(root.path().join(".zshenv"))?,
        original
    );
    assert!(
        !std::fs::read_dir(root.path())?
            .filter_map(std::result::Result::ok)
            .any(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with(".sheprd-path."))
    );
    Ok(())
}
