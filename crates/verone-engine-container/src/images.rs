//! The Android images this engine can run.
//!
//! Images are identified by digest, so a phone always runs exactly the image that
//! was tested, whatever a tag points to later.

use verone_core::{Error, Result};

/// One Android image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Image {
    /// The Android version users ask for, such as `"14"`.
    pub android: &'static str,
    /// The image repository.
    pub repository: &'static str,
    /// The tag the digest was taken from. For display only.
    pub tag: &'static str,
    /// The content digest the image is pulled and run by.
    pub digest: &'static str,
}

impl Image {
    /// The reference used to pull and run the image.
    pub fn reference(&self) -> String {
        format!("{}@{}", self.repository, self.digest)
    }

    /// The name shown to people.
    pub fn display_name(&self) -> String {
        format!("{}:{}", self.repository, self.tag)
    }
}

/// Every image this version of Verone supports. x86-64 only in Phase 1.
pub const IMAGES: &[Image] = &[Image {
    android: "14",
    repository: "redroid/redroid",
    tag: "14.0.0-latest",
    digest: "sha256:a3e654caad74faed03cd5ead057a989e06f507778cda678e5d3b9772e0092f1c",
}];

/// A small image used to run file operations on root-owned phone data.
pub const HELPER_IMAGE: &str =
    "alpine@sha256:f59abe656b692d901e184be3445e084786602108599d2a51d229c0f5a48c71bf";

/// The image for an Android version.
pub fn image_for(android: &str) -> Result<&'static Image> {
    IMAGES
        .iter()
        .find(|image| image.android == android)
        .ok_or_else(|| {
            Error::not_found(format!("there is no image for Android {android}")).with_hint(format!(
                "Supported versions: {}.",
                supported_versions().join(", ")
            ))
        })
}

/// The Android versions that have an image.
pub fn supported_versions() -> Vec<&'static str> {
    IMAGES.iter().map(|image| image.android).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use verone_core::ErrorKind;

    #[test]
    fn android_14_resolves_to_the_tested_digest() {
        let image = image_for("14").unwrap();
        assert_eq!(
            image.reference(),
            "redroid/redroid@sha256:a3e654caad74faed03cd5ead057a989e06f507778cda678e5d3b9772e0092f1c"
        );
        assert_eq!(image.display_name(), "redroid/redroid:14.0.0-latest");
    }

    #[test]
    fn an_unsupported_version_is_not_found_and_lists_what_exists() {
        let error = image_for("9").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotFound);
        assert_eq!(error.exit_code(), 4);
        assert!(error.message().contains("Android 9"));
        assert!(error.hint().unwrap().contains("14"));
    }

    #[test]
    fn every_image_is_pinned_by_a_full_digest() {
        for image in IMAGES {
            assert!(image.digest.starts_with("sha256:"), "{image:?}");
            assert_eq!(image.digest.len(), "sha256:".len() + 64, "{image:?}");
        }
        assert!(HELPER_IMAGE.contains("@sha256:"));
        assert_eq!(supported_versions(), ["14"]);
    }
}
