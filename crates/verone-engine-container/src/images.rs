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
    /// True if Android in this image ignores restart and power-off requests made
    /// inside it, so the power watcher must carry them out (decision D-017).
    pub power_watcher: bool,
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

/// Every image this version of Verone supports. x86-64 only.
///
/// The first entry is the default Android version: `verone doctor` checks the space
/// needed to download it. Android 12 was added for the benchmark's like-for-like set
/// (`docs/benchmark.md`, assumption A-22).
pub const IMAGES: &[Image] = &[
    Image {
        android: "14",
        repository: "redroid/redroid",
        tag: "14.0.0-latest",
        digest: "sha256:a3e654caad74faed03cd5ead057a989e06f507778cda678e5d3b9772e0092f1c",
        power_watcher: false,
    },
    Image {
        android: "12",
        repository: "redroid/redroid",
        tag: "12.0.0-latest",
        digest: "sha256:52332b2d74f337982d5ac281a8020ec297fb1ea05cbdcdaaa9c19a2065ae1adc",
        power_watcher: true,
    },
];

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

/// The Android versions whose phones need the power watcher.
pub fn power_watcher_versions() -> Vec<&'static str> {
    IMAGES
        .iter()
        .filter(|image| image.power_watcher)
        .map(|image| image.android)
        .collect()
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
    fn android_12_resolves_to_the_tested_digest() {
        let image = image_for("12").unwrap();
        assert_eq!(
            image.reference(),
            "redroid/redroid@sha256:52332b2d74f337982d5ac281a8020ec297fb1ea05cbdcdaaa9c19a2065ae1adc"
        );
        assert_eq!(image.display_name(), "redroid/redroid:12.0.0-latest");
    }

    #[test]
    fn only_android_12_needs_the_power_watcher() {
        assert_eq!(power_watcher_versions(), ["12"]);
    }

    #[test]
    fn the_default_android_version_comes_first() {
        assert_eq!(IMAGES[0].android, verone_core::profile::DEFAULT_ANDROID);
    }

    #[test]
    fn an_unsupported_version_is_not_found_and_lists_what_exists() {
        let error = image_for("9").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotFound);
        assert_eq!(error.exit_code(), 4);
        assert!(error.message().contains("Android 9"));
        assert!(error.hint().unwrap().contains("14, 12"));
    }

    #[test]
    fn every_image_is_pinned_by_a_full_digest() {
        for image in IMAGES {
            assert!(image.digest.starts_with("sha256:"), "{image:?}");
            assert_eq!(image.digest.len(), "sha256:".len() + 64, "{image:?}");
        }
        assert!(HELPER_IMAGE.contains("@sha256:"));
        assert_eq!(supported_versions(), ["14", "12"]);
    }
}
