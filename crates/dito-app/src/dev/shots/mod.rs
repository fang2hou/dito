mod capture;
mod demo;
mod hud;
#[cfg(feature = "mock")]
mod inline_asr;
mod input;
mod onboarding;
mod pages;
mod runner;
mod startup;
mod verify;

pub use runner::run;
