use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::RwLock;

use super::file::ShowFileV1;
use super::fixture::FixtureDefinition;
use std::collections::HashMap;

#[derive(Default)]
pub struct ShowStateInner {
    pub show: ShowFileV1,
    pub library: HashMap<String, FixtureDefinition>,
    pub path: Option<PathBuf>,
    pub dirty: bool,
}

#[derive(Clone, Default)]
pub struct ShowState(Arc<RwLock<ShowStateInner>>);

impl ShowState {
    pub fn new(initial: ShowStateInner) -> Self {
        Self(Arc::new(RwLock::new(initial)))
    }

    pub fn read(&self) -> parking_lot::RwLockReadGuard<'_, ShowStateInner> {
        self.0.read()
    }

    pub fn write(&self) -> parking_lot::RwLockWriteGuard<'_, ShowStateInner> {
        self.0.write()
    }
}
