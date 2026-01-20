//! Camera device enumeration.

use nokhwa::query;
use nokhwa::utils::ApiBackend;

use super::types::{CameraError, CameraInfo};

/// List all available camera devices on the system.
///
/// Returns a vector of `CameraInfo` structs, or an error if querying fails.
/// If no cameras are found, returns an empty vector (not an error).
pub fn list_devices() -> Result<Vec<CameraInfo>, CameraError> {
    let devices = query(ApiBackend::Auto).map_err(|e| CameraError::QueryFailed(e.to_string()))?;

    Ok(devices
        .into_iter()
        .map(|d| CameraInfo {
            index: d.index().as_index().unwrap_or(0),
            name: d.human_name(),
            description: d.description().to_string(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_devices_does_not_error() {
        // Should not error even if no cameras are present
        // (returns empty list instead)
        let result = list_devices();
        assert!(result.is_ok());
    }
}
