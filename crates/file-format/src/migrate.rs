use feature_engine::types::FeatureTree;

use crate::errors::LoadError;

/// Apply format migrations from `from_version` to `to_version`.
///
/// Migrations are applied sequentially: v1→v2, v2→v3, etc.
/// Currently version 1 is the only version, so no migrations exist yet.
pub fn migrate(
    tree: FeatureTree,
    from_version: u32,
    to_version: u32,
) -> Result<FeatureTree, LoadError> {
    // Currently only version 1 exists, so any migration request is an error.
    // As the format evolves, add match arms: 1 => migrate_v1_to_v2(tree)?
    if from_version != to_version {
        return Err(LoadError::MigrationFailed {
            from: from_version,
            to: to_version,
            reason: format!(
                "no migration path from v{} to v{}",
                from_version, to_version
            ),
        });
    }
    Ok(tree)
}
