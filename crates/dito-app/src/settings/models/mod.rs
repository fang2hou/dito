mod action;
mod inputs;
mod installed_row;
mod local_rows;
mod metadata;
mod model_menu;
mod page;
#[cfg(feature = "screenshots")]
mod probe;
mod provider_form;
mod storage;

pub use page::{ModalChanged, ModelsPage};

pub(in crate::settings) use provider_form::test_status;
