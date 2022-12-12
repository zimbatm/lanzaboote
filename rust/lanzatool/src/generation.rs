use std::cmp::Ordering;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use bootspec::generation::Generation as BootspecGeneration;
use bootspec::BootJson;
use bootspec::SpecialisationName;
use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};

use crate::utils;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecureBootExtension {
    #[serde(rename = "osRelease")]
    pub os_release: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ExtendedBootJson {
    pub bootspec: BootJson,
    pub extensions: SecureBootExtension,
}

/// A generation is the actual derivation to which a generation link points.
///
/// This derivation contains almost all information necessary to be installed
/// onto the EFI System Partition. The only information missing is the version
/// number which it retrieved from the generation_link.
#[derive(Debug)]
pub struct Generation {
    /// Profile symlink index
    version: u64,
    /// Top-level specialisation name
    specialisation_name: Option<SpecialisationName>,
    /// Top-level extended boot specification
    pub spec: ExtendedBootJson,
}

impl Generation {
    pub fn from_link(link: &GenerationLink) -> Result<Self> {
        let bootspec_path = link.path.join("boot.json");
        let generation: BootspecGeneration = serde_json::from_slice(
            &fs::read(bootspec_path).context("Failed to read bootspec file")?,
        )
        .context("Failed to parse bootspec json")?;

        let bootspec: BootJson = generation
            .try_into()
            .map_err(|err: &'static str| anyhow!(err))?;

        let extensions = Self::extract_extensions(&bootspec)?;

        Ok(Self {
            version: link.version,
            specialisation_name: None,
            spec: ExtendedBootJson {
                bootspec,
                extensions,
            },
        })
    }

    fn extract_extensions(bootspec: &BootJson) -> Result<SecureBootExtension> {
        Ok(Deserialize::deserialize(
            bootspec.extensions.get("lanzaboote")
            .context("Failed to extract Lanzaboote-specific extension from Bootspec, missing lanzaboote field in `extensions`")?
            .clone()
            .into_deserializer()
        )?)
    }

    pub fn specialise(&self, name: &SpecialisationName, bootspec: &BootJson) -> Result<Self> {
        Ok(Self {
            version: self.version,
            specialisation_name: Some(name.clone()),
            spec: ExtendedBootJson {
                bootspec: bootspec.clone(),
                extensions: Self::extract_extensions(bootspec)?,
            },
        })
    }

    pub fn is_specialized(&self) -> Option<SpecialisationName> {
        self.specialisation_name.clone()
    }
}

impl fmt::Display for Generation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.version)
    }
}

/// A generation link points to a generation (the actual toplevel derivation).
///
/// It can be built from the symlink in /nix/var/nix/profiles/ alone because the name of the symlink
/// enocdes the version number.
#[derive(Debug)]
pub struct GenerationLink {
    pub version: u64,
    pub path: PathBuf,
}

impl GenerationLink {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            version: parse_version(&path).context("Failed to parse version")?,
            path: PathBuf::from(path.as_ref()),
        })
    }
}

// We implement PartialEq, Eq, PartialOrd, and Ord so we can sort the generation links by version.
// This is necessary so we can honor the configuration limit by only installing the configured
// number of newest generations.
impl PartialEq for GenerationLink {
    fn eq(&self, other: &Self) -> bool {
        self.version == other.version
    }
}

impl Eq for GenerationLink {}

impl PartialOrd for GenerationLink {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.version.partial_cmp(&other.version)
    }
}

impl Ord for GenerationLink {
    fn cmp(&self, other: &Self) -> Ordering {
        self.version.cmp(&other.version)
    }
}

fn parse_version(path: impl AsRef<Path>) -> Result<u64> {
    let file_name = path.as_ref().file_name().with_context(|| {
        format!(
            "Failed to extract file name from generation link path: {:?}",
            path.as_ref()
        )
    })?;

    let file_name_str = utils::path_to_string(file_name);

    let generation_version_str = file_name_str
        .split('-')
        .nth(1)
        .with_context(|| format!("Failed to extract version from link: {}", file_name_str))?;

    let generation_version = generation_version_str.parse().with_context(|| {
        format!(
            "Failed to parse generation version: {}",
            generation_version_str
        )
    })?;

    Ok(generation_version)
}
