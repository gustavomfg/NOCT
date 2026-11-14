//! Chooses the user-data directory after the product was renamed. A legacy directory is moved
//! to the new name, unless the new directory already holds the data. Port of
//! `electron/persistence/ProductUserData.ts`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::recovery::{inspect_database_file, is_recoverable_corruption};
use crate::runtime::DATABASE_FILE_NAME;

/// What is known about a database file before the directory is chosen.
#[derive(Debug, PartialEq, Eq)]
enum DatabaseState {
    Missing,
    Valid { schema_version: u32 },
    Invalid,
    Unknown,
}

fn database_state(path: &Path) -> DatabaseState {
    if !path.exists() {
        return DatabaseState::Missing;
    }
    match inspect_database_file(path) {
        Ok(schema_version) => DatabaseState::Valid { schema_version },
        Err(error) if is_recoverable_corruption(&error) => DatabaseState::Invalid,
        Err(_) => DatabaseState::Unknown,
    }
}

/// Returns the directory the application uses for its data. The new directory is used unless
/// the legacy one holds data that the new one does not: a legacy database where the new
/// directory has none, has a damaged one, or has an empty one left by an interrupted startup.
/// When the new directory is absent, the legacy one is renamed to it. If the rename fails, the
/// legacy directory keeps serving the data.
pub fn migrate_product_user_data(
    app_data: &Path,
    current_name: &str,
    legacy_name: &str,
) -> PathBuf {
    let current = app_data.join(current_name);
    let legacy = app_data.join(legacy_name);

    if current.exists() {
        let current_state = database_state(&current.join(DATABASE_FILE_NAME));
        let legacy_state = database_state(&legacy.join(DATABASE_FILE_NAME));
        let legacy_is_usable = matches!(
            &legacy_state,
            DatabaseState::Valid { schema_version } if *schema_version > 0
        );
        let current_is_empty = matches!(&current_state, DatabaseState::Valid { schema_version: 0 });
        let use_legacy = (current_state == DatabaseState::Missing
            && legacy_state != DatabaseState::Missing)
            || (current_state == DatabaseState::Invalid && legacy_is_usable)
            || (current_is_empty && legacy_is_usable);
        return if use_legacy { legacy } else { current };
    }

    if !legacy.exists() {
        return current;
    }
    match fs::rename(&legacy, &current) {
        Ok(()) => current,
        Err(_) => legacy,
    }
}
