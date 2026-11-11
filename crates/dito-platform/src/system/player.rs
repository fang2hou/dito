use std::path::Path;
use std::process::{Child, Command};
use std::sync::Mutex;

#[derive(Default)]
pub struct Player(Mutex<Option<Child>>);

impl Player {
    pub fn play(&self, path: &Path) {
        let mut current = self.0.lock().expect("player lock");
        if let Some(mut child) = current.take() {
            let _ = child.kill();
        }
        *current = Command::new("afplay").arg(path).spawn().ok();
    }

    pub fn stop(&self) {
        if let Some(mut child) = self.0.lock().expect("player lock").take() {
            let _ = child.kill();
        }
    }
}
