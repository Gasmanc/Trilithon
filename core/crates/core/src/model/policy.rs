//! Policy attachment and preset version types.

use serde::{Deserialize, Serialize};

use crate::model::identifiers::PresetId;

/// A snapshot of a policy preset at a specific version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct PresetVersion {
    /// Identifier of the preset.
    pub preset_id: PresetId,
    /// Version number of the preset body.
    pub version: u32,
    /// Canonical JSON of the preset body at this version.
    pub body_json: String,
}

/// Attachment of a policy preset to a route.
///
/// Phase 18 supplies the resolved-attachment shape. For Phase 4 the
/// attachment is the `(preset_id, version)` pair held alongside the route.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct PolicyAttachment {
    /// Identifier of the attached preset.
    pub preset_id: PresetId,
    /// Version of the preset at the time of attachment.
    pub preset_version: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_version_serde_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let pv = PresetVersion {
            preset_id: PresetId("01PRESET00000000000000000A".to_owned()),
            version: 3,
            body_json: r#"{"rate_limit":100}"#.to_owned(),
        };
        let json = serde_json::to_string(&pv)?;
        let rt: PresetVersion = serde_json::from_str(&json)?;
        assert_eq!(pv, rt);
        Ok(())
    }
}
