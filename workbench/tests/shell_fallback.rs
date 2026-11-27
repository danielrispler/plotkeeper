//! #701: with `SHELL` unset, programs are found through `/bin/sh`, not `/bin/zsh`, which a Linux machine may lack.

use std::os::unix::fs::PermissionsExt;
use studio_workbench::gh_path;

#[test]
fn with_shell_unset_gh_is_found_through_bin_sh() {
    let home = tempfile::tempdir().unwrap();
    let bin = home.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let gh = bin.join("gh");
    std::fs::write(&gh, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
    // Only a login `sh` reads `~/.profile`; a login zsh reads `~/.zprofile` and never sees this folder.
    std::fs::write(
        home.path().join(".profile"),
        format!("PATH=\"{}:$PATH\"\n", bin.display()),
    )
    .unwrap();
    // SAFETY: nextest runs each test in its own process, so no other thread reads the environment.
    unsafe {
        std::env::remove_var("SHELL");
        std::env::set_var("HOME", home.path());
    }
    assert_eq!(gh_path(), gh.to_str().unwrap());
}
