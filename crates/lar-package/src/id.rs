use crate::Error;

/// Validate a reverse-DNS package id (e.g. `org.example.editor`).
pub fn validate_package_id(id: &str) -> Result<(), Error> {
    if id.is_empty() {
        return Err(Error::InvalidPackageId {
            id: id.to_string(),
            reason: "must not be empty".into(),
        });
    }
    let labels: Vec<&str> = id.split('.').collect();
    if labels.len() < 2 {
        return Err(Error::InvalidPackageId {
            id: id.to_string(),
            reason: "must contain at least two dot-separated labels".into(),
        });
    }
    for label in labels {
        if label.is_empty() {
            return Err(Error::InvalidPackageId {
                id: id.to_string(),
                reason: "labels must not be empty".into(),
            });
        }
        let mut chars = label.chars();
        let Some(first) = chars.next() else {
            return Err(Error::InvalidPackageId {
                id: id.to_string(),
                reason: "labels must not be empty".into(),
            });
        };
        if !first.is_ascii_lowercase() {
            return Err(Error::InvalidPackageId {
                id: id.to_string(),
                reason: "each label must start with a lowercase letter".into(),
            });
        }
        if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return Err(Error::InvalidPackageId {
                id: id.to_string(),
                reason: "labels may only contain lowercase letters, digits, and hyphens".into(),
            });
        }
        if label.ends_with('-') {
            return Err(Error::InvalidPackageId {
                id: id.to_string(),
                reason: "labels must not end with a hyphen".into(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_reverse_dns() {
        validate_package_id("org.example.editor").unwrap();
        validate_package_id("org.qt.qtbase").unwrap();
    }

    #[test]
    fn rejects_bad_ids() {
        assert!(validate_package_id("editor").is_err());
        assert!(validate_package_id("Org.Example").is_err());
        assert!(validate_package_id(".org.example").is_err());
        assert!(validate_package_id("org..example").is_err());
    }
}
