use std::sync::Arc;

use bollard::Docker;
use tokio::sync::{Semaphore, watch};

#[derive(Clone)]
pub(super) struct App {
    pub(super) docker: Docker,
    pub(super) image: String,
    pub(super) origins: Vec<String>,
    pub(super) slots: Arc<Semaphore>,
    pub(super) shutdown: watch::Receiver<bool>,
}
