use super::*;

use serde::*;

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::{self, Formatter};
use std::ffi::*;
use std::fs::read_to_string;
use std::path::*;



pub(super) fn find_cwd_installs(maybe_dst_bin: Option<PathBuf>) -> Result<Vec<InstallSet>, Error> {
    let mut path = std::env::current_dir().map_err(|err| error!(err, "unable to determine cwd: {}", err))?;
    let mut files = Vec::new();
    loop {
        path.push("Cargo.toml");
        if path.exists() {
            let file = File::from_path(&path)?;
            let dir = path.parent().unwrap();

            let mut installs = Vec::new();
            for has_meta in vec![file.toml.workspace, file.toml.package].into_iter().flatten() {
                for (name, InstallData { package, locked, source, default_features }) in has_meta.metadata.local_install.into_iter() {
                    installs.push({
                        let name = OsStr::new(package.as_ref().map(|p| p.as_str()).unwrap_or(&name));
                        let mut flags = match source {
                            InstallSource::Local { path }                                   => vec![ InstallFlag::new("--path", vec![dir.join(path).into()]) ],
                            InstallSource::Git { git }                                      => vec![ InstallFlag::new("--git", vec![git.into()]) ],
                            InstallSource::GitRev { git, rev }                              => vec![ InstallFlag::new("--git", vec![git.into()]), InstallFlag::new("--rev", vec![rev.into()] ) ],
                            InstallSource::GitBranch { git, branch }                        => vec![ InstallFlag::new("--git", vec![git.into()]), InstallFlag::new("--branch", vec![branch.into()] ) ],
                            InstallSource::Registry { version, registry: Some(registry) }   => vec![ InstallFlag::new("--version", vec![fix_version(&version).into()]), InstallFlag::new("--registry", vec![registry.into()]) ],
                            InstallSource::Registry { version, registry: None }             => vec![ InstallFlag::new("--version", vec![fix_version(&version).into()]) ],
                        };
                        if locked { flags.push(InstallFlag::new("--locked", vec![])); }
                        if !default_features { flags.push(InstallFlag::new("--no-default-features", vec![])); }
                        Install { name: name.into(), flags }
                    });
                }
            }

            // TODO: add flag to search the entire workspace instead of merely the CWD tree?
            if !installs.is_empty() {

                let file_dst_bin;
                if let Some(dst_bin) = maybe_dst_bin {
                    file_dst_bin = dst_bin;
                } else {
                    file_dst_bin = file.directory.join("bin");
                }

                files.push(InstallSet {
                    bin: file_dst_bin,
                    src: Some(path.clone()),
                    installs,
                });
            }
            break;
        }
        if !path.pop() || !path.pop() { break }
    }
    Ok(files)
}



struct File {
    directory:  PathBuf,
    //file:     PathBuf,
    toml:       CargoToml,
}

#[derive(Default)]
struct CargoToml {
    workspace:  Option<HasMetadata>,
    package:    Option<HasMetadata>,
}

#[derive(Default)]
struct HasMetadata {
    metadata: Metadata
}

#[derive(Default)]
struct Metadata {
    local_install: BTreeMap<String, InstallData>,
}

struct InstallData {
    package:    Option<String>,
    locked:     bool,
    // TODO: features, optional?
    default_features: bool,
    source:     InstallSource,
}

enum InstallSource {
    Registry    { version: String, registry: Option<String> },
    Local       { path: PathBuf },
    GitRev      { git: String, rev:    String },
    GitBranch   { git: String, branch: String },
    Git         { git: String },
}




impl<'de> Deserialize<'de> for CargoToml {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct CargoTomlVisitor;
        impl<'de> de::Visitor<'de> for CargoTomlVisitor {
            type Value = CargoToml;
            fn expecting(&self, formatter: &mut Formatter) -> fmt::Result { formatter.write_str("a workspace or package table") }
            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut r = Self::Value::default();
                while let Some(key) = map.next_key()? {
                    match key {
                        "package" => {
                            if r.package.is_some() { return Err(de::Error::duplicate_field("package")) }
                            r.package = map.next_value()?;
                        },
                        "workspace" => {
                            if r.workspace.is_some() { return Err(de::Error::duplicate_field("workspace")) }
                            r.workspace = map.next_value()?;
                        },
                        _other => {
                            let _ : de::IgnoredAny = map.next_value()?;
                        },
                    }
                }
                Ok(r)
            }
        }
        d.deserialize_any(CargoTomlVisitor)
    }
}

impl<'de> Deserialize<'de> for HasMetadata {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct HasMetadataVisitor;
        impl<'de> de::Visitor<'de> for HasMetadataVisitor {
            type Value = HasMetadata;
            fn expecting(&self, formatter: &mut Formatter) -> fmt::Result { formatter.write_str("a workspace or package table") }
            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut r = Self::Value::default();
                let mut one = false;
                while let Some(key) = map.next_key()? {
                    match key {
                        "metadata" => if one {
                            return Err(de::Error::duplicate_field("metadata"));
                        } else {
                            one = true;
                            r.metadata = map.next_value()?;
                        },
                        _other => {
                            let _ : de::IgnoredAny = map.next_value()?;
                        },
                    }
                }
                Ok(r)
            }
        }
        d.deserialize_any(HasMetadataVisitor)
    }
}

impl<'de> Deserialize<'de> for Metadata {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct MetadataVisitor;
        impl<'de> de::Visitor<'de> for MetadataVisitor {
            type Value = Metadata;
            fn expecting(&self, formatter: &mut Formatter) -> fmt::Result { formatter.write_str("a metadata table") }
            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut r = Metadata::default();
                let mut one = false;
                while let Some(key) = map.next_key()? {
                    match key {
                        "local-install" => if one {
                            return Err(de::Error::duplicate_field("local-install"));
                        } else {
                            one = true;
                            r.local_install = map.next_value()?;
                        },
                        _other => {
                            let _ : de::IgnoredAny = map.next_value()?;
                        },
                    }
                }
                Ok(r)
            }
        }
        d.deserialize_any(MetadataVisitor)
    }
}

impl<'de> Deserialize<'de> for InstallData {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct InstallDataVisitor;
        impl<'de> de::Visitor<'de> for InstallDataVisitor {
            type Value = InstallData;
            fn expecting(&self, formatter: &mut Formatter) -> fmt::Result { formatter.write_str("a version string or installation dependency table") }
            fn visit_str   <E>(self, value: &str  ) -> Result<Self::Value, E> { Ok(InstallData { package: None, locked: true, default_features: true, source: InstallSource::Registry { version: value.into(), registry: None } }) }
            fn visit_string<E>(self, value: String) -> Result<Self::Value, E> { Ok(InstallData { package: None, locked: true, default_features: true, source: InstallSource::Registry { version: value,        registry: None } }) }
            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut package     : Option<String> = None;
                let mut locked      : Option<bool  > = None;
                let mut default_features      : Option<bool  > = None;

                let mut version     : Option<String> = None;
                let mut registry    : Option<String> = None;
                let mut path        : Option<PathBuf> = None;
                let mut git         : Option<String> = None;
                let mut rev         : Option<String> = None;
                let mut branch      : Option<String> = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        "package" => {
                            if package.is_some() { return Err(de::Error::duplicate_field("package")) }
                            package = Some(map.next_value()?);
                        },
                        "locked" => {
                            if locked.is_some() { return Err(de::Error::duplicate_field("locked")) }
                            locked = Some(map.next_value()?);
                        },
                        "default_features" => {
                            if default_features.is_some() { return Err(de::Error::duplicate_field("default_features")) }
                            default_features = Some(map.next_value()?);
                        },
                        "version" => {
                            if version  .is_some() { return Err(de::Error::duplicate_field("version")); }
                            if path     .is_some() { return Err(de::Error::custom("field `version` conflicts with field `path`")); }
                            if git      .is_some() { return Err(de::Error::custom("field `version` conflicts with field `git`")); }
                            if rev      .is_some() { return Err(de::Error::custom("field `version` conflicts with field `rev`")); }
                            if branch   .is_some() { return Err(de::Error::custom("field `version` conflicts with field `branch`")); }
                            version = Some(map.next_value()?);
                        },
                        "registry" => {
                            if registry .is_some() { return Err(de::Error::duplicate_field("registry")); }
                            if path     .is_some() { return Err(de::Error::custom("field `registry` conflicts with field `path`")); }
                            if git      .is_some() { return Err(de::Error::custom("field `registry` conflicts with field `git`")); }
                            if rev      .is_some() { return Err(de::Error::custom("field `registry` conflicts with field `rev`")); }
                            if branch   .is_some() { return Err(de::Error::custom("field `registry` conflicts with field `branch`")); }
                            registry = Some(map.next_value()?);
                        }
                        "path" => {
                            if path     .is_some() { return Err(de::Error::duplicate_field("path")); }
                            if version  .is_some() { return Err(de::Error::custom("field `path` conflicts with field `version`")); }
                            if registry .is_some() { return Err(de::Error::custom("field `path` conflicts with field `registry`")); }
                            if git      .is_some() { return Err(de::Error::custom("field `path` conflicts with field `git`")); }
                            if rev      .is_some() { return Err(de::Error::custom("field `path` conflicts with field `rev`")); }
                            if branch   .is_some() { return Err(de::Error::custom("field `path` conflicts with field `branch`")); }
                            path = Some(map.next_value()?);
                        },
                        "git" => {
                            if git      .is_some() { return Err(de::Error::duplicate_field("git")); }
                            if path     .is_some() { return Err(de::Error::custom("field `git` conflicts with field `path`")); }
                            if version  .is_some() { return Err(de::Error::custom("field `git` conflicts with field `version`")); }
                            if registry .is_some() { return Err(de::Error::custom("field `git` conflicts with field `registry`")); }
                            git = Some(map.next_value()?);
                        },
                        "rev" => {
                            if rev      .is_some() { return Err(de::Error::duplicate_field("rev")); }
                            if path     .is_some() { return Err(de::Error::custom("field `rev` conflicts with field `path`")); }
                            if version  .is_some() { return Err(de::Error::custom("field `rev` conflicts with field `version`")); }
                            if registry .is_some() { return Err(de::Error::custom("field `rev` conflicts with field `registry`")); }
                            if branch   .is_some() { return Err(de::Error::custom("field `rev` conflicts with field `branch`")); }
                            rev = Some(map.next_value()?);
                        },
                        "branch" => {
                            if branch   .is_some() { return Err(de::Error::duplicate_field("branch")); }
                            if path     .is_some() { return Err(de::Error::custom("field `branch` conflicts with field `path`")); }
                            if version  .is_some() { return Err(de::Error::custom("field `branch` conflicts with field `version`")); }
                            if registry .is_some() { return Err(de::Error::custom("field `branch` conflicts with field `registry`")); }
                            if rev      .is_some() { return Err(de::Error::custom("field `branch` conflicts with field `rev`")); }
                            branch = Some(map.next_value()?);
                        },
                        other => return Err(de::Error::unknown_field(other, &["package", "locked", "version", "registry", "path", "git", "rev", "branch"])),
                    }
                }

                let source = if let Some(version) = version {
                    InstallSource::Registry { version, registry }
                } else if let Some(path) = path {
                    InstallSource::Local { path }
                } else if let Some(git) = git {
                    if let Some(branch) = branch {
                        InstallSource::GitBranch { git, branch }
                    } else if let Some(rev) = rev {
                        InstallSource::GitRev { git, rev }
                    } else {
                        InstallSource::Git { git }
                    }
                } else {
                    return Err(de::Error::custom("Expected `version`, `path`, or `git`"));
                };

                Ok(InstallData {
                    package,
                    locked: locked.unwrap_or(true),
                    source,
                    default_features: default_features.unwrap_or(true),
                })
            }
        }
        d.deserialize_any(InstallDataVisitor)
    }
}



impl File {
    fn from_path(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let text = read_to_string(path).map_err(|err| error!(err, "unable to read {}: {}", path.display(), err))?;
        Ok(File {
            toml: toml::from_str(&text).map_err(|err| error!(None, "unable to parse {}: {}", path.display(), err))?,
            //file: path.into(),
            directory: {
                let mut d = path.to_path_buf();
                if !d.pop() { return Err(error!(None, "unable to determine containing directory for Cargo.toml"))? }
                d
            },
        })
    }
}



fn fix_version(v: &str) -> Cow<OsStr> {
    let first = v.chars().next().unwrap_or('\0');
    if ('0'..='9').contains(&first) {
        OsString::from(format!("^{}", v)).into()
    } else {
        OsStr::new(v).into()
    }
}
