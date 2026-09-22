//! File-backed provenance. A confidence score is a claim, never a trust grant.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Quality {
    pub basis: String,
    pub confidence: Option<f64>,
    pub sources: Vec<String>,
    pub last_verified: Option<String>,
    pub superseded_by: Option<String>,
    pub conflicts_with: Vec<String>,
    pub reviewed: bool,
    pub reviewed_content_hash: Option<String>,
}

impl Default for Quality {
    fn default() -> Self {
        Self {
            basis: "unverified".into(),
            confidence: None,
            sources: vec![],
            last_verified: None,
            superseded_by: None,
            conflicts_with: vec![],
            reviewed: false,
            reviewed_content_hash: None,
        }
    }
}

impl Quality {
    pub fn validate(&self, id: &str) -> Result<(), String> {
        if !["unverified", "observed", "inferred"].contains(&self.basis.as_str()) {
            return Err("Basis must be unverified, observed or inferred".into());
        }
        if self
            .confidence
            .is_some_and(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
        {
            return Err("Confidence must be between 0 and 1".into());
        }
        if self.sources.len() > 32
            || self
                .sources
                .iter()
                .any(|s| s.trim().is_empty() || s.len() > 2048)
        {
            return Err("Provide at most 32 non-empty sources, up to 2048 bytes each".into());
        }
        if self.basis == "observed" && self.sources.is_empty() {
            return Err("Observed memories need a source".into());
        }
        if let Some(date) = &self.last_verified {
            chrono::DateTime::parse_from_rfc3339(date)
                .map_err(|_| "Last verified must be an RFC3339 timestamp")?;
        }
        if self.superseded_by.as_deref() == Some(id) || self.conflicts_with.iter().any(|s| s == id)
        {
            return Err("A memory cannot supersede or conflict with itself".into());
        }
        if self.conflicts_with.len() > 32 {
            return Err("At most 32 conflict references are allowed".into());
        }
        Ok(())
    }
}

/// Invalid external frontmatter is kept in the file, but never indexed as reviewed.
pub fn from_value(value: Option<&serde_json::Value>, id: &str) -> Quality {
    value
        .cloned()
        .and_then(|v| serde_json::from_value::<Quality>(v).ok())
        .filter(|q| q.validate(id).is_ok())
        .unwrap_or_default()
}

pub fn for_body(value: Option<&serde_json::Value>, id: &str, body: &str) -> Quality {
    let mut q = from_value(value, id);
    if q.reviewed && q.reviewed_content_hash.as_deref() != Some(&body_hash(body)) {
        q.reviewed = false;
        q.last_verified = None;
        q.reviewed_content_hash = None;
    }
    q
}

pub fn body_hash(body: &str) -> String {
    format!(
        "{:016x}",
        crate::vault::fnv1a64(body.trim_start_matches(['\r', '\n']).trim_end())
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provenance_is_not_implicitly_trusted() {
        assert!(!Quality::default().reviewed);
        let mut q = Quality {
            basis: "observed".into(),
            ..Quality::default()
        };
        assert!(q.validate("a").is_err());
        q.sources.push("test output from run 42".into());
        assert!(q.validate("a").is_ok());
        q.confidence = Some(1.1);
        assert!(q.validate("a").is_err());
        assert_eq!(
            from_value(
                Some(&serde_json::json!({"reviewed":true,"basis":"invented"})),
                "a"
            ),
            Quality::default()
        );
    }
    #[test]
    fn external_body_edits_invalidate_review() {
        let q = serde_json::json!({"basis":"inferred","reviewed":true,"reviewedContentHash":format!("{:016x}",crate::vault::fnv1a64("old body"))});
        assert!(for_body(Some(&q), "a", "old body").reviewed);
        assert!(!for_body(Some(&q), "a", "changed body").reviewed);
    }
}
