use std::path::Path;
use std::process::{Child, Command};
use std::sync::Mutex;

#[derive(Default)]
pub struct Player(Mutex<Option<Child>>);

impl Player {
    pub fn play(&self, path: &Path) {
        let mut current = self.0.lock().expect("player lock");
        if let Some(child) = current.take() {
            end(child);
        }
        *current = Command::new("afplay").arg(path).spawn().ok();
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
