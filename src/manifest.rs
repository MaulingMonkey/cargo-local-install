use super::*;

use serde::*;

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ffi::*;
use std::fs::read_to_string;
use std::path::*;



pub(super) fn find_cwd_installs() -> Result<Vec<InstallSet>, Error> {
    let mut path = std::env::current_dir().map_err(|err| error!(err, "unable to determine cwd: {}", err))?;
    let mut files = Vec::new();
    loop {
        path.push("Cargo.toml");
        if path.exists() {
            let file = File::from_path(&path)?;

            let mut installs = Vec::new();
            for has_meta in vec![file.toml.workspace, file.toml.package].into_iter().flatten() {
                for (name, data) in has_meta.metadata.local_install.into_iter() {
                    installs.push(match data {
                        InstallData::Version(version) => Install {
                            name: name.into(),
                            flags: vec![
                                InstallFlag::new("--locked", vec![]),
                                InstallFlag::new("--version", vec![version.into()]),
                            ]
                        },
                        InstallData::Table { package, locked, source } => {
                            let name = OsStr::new(package.as_ref().map(|p| p.as_str()).unwrap_or(&name));
                            let mut flags = match source {
                                InstallSource::Local { path }                                   => vec![ InstallFlag::new("--path", vec![path.into()]) ],
                                InstallSource::Git { git }                                      => vec![ InstallFlag::new("--git", vec![git.into()]) ],
                                InstallSource::GitRev { git, rev }                              => vec![ InstallFlag::new("--git", vec![git.into()]), InstallFlag::new("--rev", vec![rev.into()] ) ],
                                InstallSource::GitBranch { git, branch }                        => vec![ InstallFlag::new("--git", vec![git.into()]), InstallFlag::new("--branch", vec![branch.into()] ) ],
                                InstallSource::Registry { version, registry: Some(registry) }   => vec![ InstallFlag::new("--version", vec![fix_version(&version).into()]), InstallFlag::new("--registry", vec![registry.into()]) ],
                                InstallSource::Registry { version, registry: None }             => vec![ InstallFlag::new("--version", vec![fix_version(&version).into()]) ],
                            };
                            if locked.unwrap_or(true) { flags.push(InstallFlag::new("--locked", vec![])); }
                            Install { name: name.into(), flags }
                        },
                    });
                }
            }

            // TODO: add flag to search the entire workspace instead of merely the CWD tree?
            if !installs.is_empty() {
                files.push(InstallSet {
                    bin: file.directory.join("bin"),
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

#[derive(Deserialize)]
struct CargoToml {
    workspace:  Option<HasMetadata>,
    package:    Option<HasMetadata>,
}

#[derive(Deserialize)]
struct HasMetadata {
    #[serde(default)] metadata: Metadata
}

#[derive(Deserialize, Default)]
struct Metadata {
    #[serde(default, rename = "local-install")] local_install: BTreeMap<String, InstallData>,
}

#[derive(Deserialize)]
//#[serde(deny_unknown_fields)] // XXX
#[serde(untagged)]
enum InstallData {
    Table {
        package:                    Option<String>,
        locked:                     Option<bool>,
        // TODO: features, default-features, optional?
        #[serde(flatten)] source:   InstallSource,
    },
    Version(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(untagged)]
enum InstallSource {
    Registry    { version: String, registry: Option<String> },
    Local       { path: PathBuf },
    GitRev      { git: String, rev:    String },
    GitBranch   { git: String, branch: String },
    Git         { git: String },
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
