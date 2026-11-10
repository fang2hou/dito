mod capture;
mod demo;
mod dictionary;
mod hud;
#[cfg(feature = "mock")]
mod inline_asr;
mod input;
mod onboarding;
mod pages;
mod runner;
mod tile_row;
mod verify;

pub use runner::run;
