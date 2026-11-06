use std::path::PathBuf;

const AGENT_LABEL: &str = "com.fang2hou.dito";

fn agent_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/LaunchAgents").join(format!("{AGENT_LABEL}.plist")))
}

fn agent_plist(exe: &std::path::Path) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n  <key>Label</key><string>{AGENT_LABEL}</string>\n  <key>ProgramArguments</key><array><string>{}</string></array>\n  <key>RunAtLoad</key><true/>\n</dict>\n</plist>\n",
        exe.display()
    )
}

/// Register or remove a LaunchAgent that starts this executable at login.
pub fn set_launch_at_login(enabled: bool) -> std::io::Result<()> {
    let Some(path) = agent_path() else { return Ok(()) };
    if !enabled {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    let plist = agent_plist(&std::env::current_exe()?);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, plist)
}

pub fn launch_at_login_enabled() -> bool {
    agent_path().is_some_and(|p| p.exists())
}
