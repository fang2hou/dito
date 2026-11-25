use std::path::Path;
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const REAP_POLL: Duration = Duration::from_millis(250);

#[derive(Default)]
pub struct Player(Arc<Mutex<Option<Child>>>);

impl Player {
    pub fn play(&self, path: &Path) {
        let mut current = self.0.lock().expect("player lock");
        if let Some(child) = current.take() {
            end(child);
        }
        let Ok(child) = Command::new("afplay").arg(path).spawn() else { return };
        let pid = child.id();
        *current = Some(child);
        let slot = Arc::downgrade(&self.0);
        // Reaps the process as soon as it finishes playing; a killed or replaced one is reaped by `end`.
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(REAP_POLL);
                let Some(slot) = slot.upgrade() else { return };
                let mut current = slot.lock().expect("player lock");
                let Some(child) = current.as_mut().filter(|child| child.id() == pid) else { return };
                if !matches!(child.try_wait(), Ok(None)) {
                    current.take();
                    return;
                }
            }
        });
    }

    pub fn stop(&self) {
        if let Some(child) = self.0.lock().expect("player lock").take() {
            end(child);
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Waiting reaps the process: one killed or finished but never waited for stays a zombie until Dito quits.
fn end(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finished_playback_is_reaped_without_another_call() {
        let player = Player::default();
        player.play(Path::new("/nonexistent.wav"));
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while player.0.lock().unwrap().is_some() {
            assert!(std::time::Instant::now() < deadline, "afplay was not reaped");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
