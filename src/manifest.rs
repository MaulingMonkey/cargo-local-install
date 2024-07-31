use super::*;

use cargo_toml::{Dependency, DepsSet};
use serde::*;

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
                for (name, dependency) in has_meta.metadata.local_install.into_iter() {

                        match dependency {
                            Dependency::Simple(version) => {
                                installs.push(Install {
                                    name,
                                    flags: vec![ InstallFlag::new("--version", vec![fix_version(&version)]) ],
                                });
                            }
                            Dependency::Detailed(detail) => {
                                let mut install = Install {
                                    name: detail.package.unwrap_or(name),
                                    flags: Vec::new(),
                                };

                                if let Some(version) = detail.version {
                                    install.flags.push(InstallFlag::new("--version", vec![fix_version(&version).to_string()]));
                                }
                                if let Some(registry) = detail.registry {
                                    install.flags.push(InstallFlag::new("--registry", vec![registry]));
                                }

                                if let Some(path) = detail.path {
                                    install.flags.push(InstallFlag::new("--path", vec![dir.join(path).to_string_lossy().into()]));
                                }

                                if let Some(git) = detail.git {
                                    install.flags.push(InstallFlag::new("--git", vec![git]));
                                }
                                if let Some(rev) = detail.rev {
                                    install.flags.push(InstallFlag::new("--rev", vec![rev]));
                                }
                                if let Some(tag) = detail.tag {
                                    install.flags.push(InstallFlag::new("--tag", vec![tag]));
                                }
                                if let Some(branch) = detail.branch {
                                    install.flags.push(InstallFlag::new("--branch", vec![branch]));
                                }


                                if !detail.default_features {
                                     install.flags.push(InstallFlag::new("--no-default-features", vec![]));
                                }
                                if !detail.features.is_empty() {
                                    install.flags.push(InstallFlag::new("--features", detail.features));
                                }

                                installs.push(install);
                            }

                            _ => {
                                error!(None, "unsupported dependency type: {:?}", dependency);
                                continue;
                            },
                        }
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

#[derive(Default, Deserialize)]
struct CargoToml {
    workspace:  Option<HasMetadata>,
    package:    Option<HasMetadata>,
}

#[derive(Default, Deserialize)]
struct HasMetadata {
    metadata: Metadata
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Metadata {
    local_install: DepsSet,
}

impl File {
    fn from_path(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let text = read_to_string(path).map_err(|err| error!(err, "unable to read {}: {}", path.display(), err))?;

        Ok(File {
            toml: toml::from_str::<CargoToml>(&text).map_err(|err| error!(None, "unable to parse {}: {}", path.display(), err))?,
            //file: path.into(),
            directory: {
                let mut d = path.to_path_buf();
                if !d.pop() { return Err(error!(None, "unable to determine containing directory for Cargo.toml"))? }
                d
            },
        })
    }
}



fn fix_version(v: &str) -> String {
    let first = v.chars().next().unwrap_or('\0');
    if first.is_ascii_digit() {
        format!("^{}", v)
    } else {
        v.to_string()
    }
}
