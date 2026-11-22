#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{pane:?} is not a pane id")]
    Pane { pane: String },
}
