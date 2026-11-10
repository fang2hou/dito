mod capture;
mod demo;
mod dictionary;
mod hud;
#[cfg(feature = "mock")]
mod inline_asr;
mod input;
mod onboarding;
mod pages;
mod polish_form;
mod runner;
mod startup;
mod tile_row;
mod verify;

pub use runner::run;
