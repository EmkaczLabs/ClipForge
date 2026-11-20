//! Local clip library and SQLite persistence crate.

pub mod migrations;
pub mod scanner;
pub mod store;

pub use clipforge_common::{ClipFilter, ClipId, ClipMetadata, ClipStore, GameInfo};
pub use scanner::DirectoryScanner;
pub use store::{
    ClipListOptions, ClipOrigin, ClipSort, PermanentDeleteResult, RestoreResult, SqliteClipStore,
    TRASH_DIR_NAME, TrashInfo, TrashMoveResult,
};
