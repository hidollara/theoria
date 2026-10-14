//! Cargo package discovery for XML imports. Each loading operation owns one
//! metadata snapshot; no global state is shared between macro expansions.
use crate::library::enclosing_manifest;
use organon_core::{Error, loader::Sources};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Default)]
pub struct CargoSources {
    metadata: BTreeMap<PathBuf, Value>,
    tracked: BTreeSet<PathBuf>,
}
impl CargoSources {
    /// Locate the dependency key used by generated Rust, including Cargo renames.
    pub fn dependency_rust_path(&mut self, manifest: &Path, name: &str) -> Result<String, Error> {
        let (manifest, data) = self.metadata(manifest)?;
        let owner = package(data, &manifest)?;
        if owner["name"].as_str() == Some(name) {
            return Ok("crate".into());
        }
        let dependency = owner["dependencies"]
            .as_array()
            .and_then(|dependencies| {
                dependencies
                    .iter()
                    .find(|d| d["name"].as_str() == Some(name))
            })
            .ok_or_else(|| Error(format!("missing {name} dependency")))?;
        Ok(format!(
            "::{}",
            dependency["rename"]
                .as_str()
                .unwrap_or(name)
                .replace('-', "_")
        ))
    }

    pub fn library_name(&mut self, manifest: &Path) -> Result<Option<String>, Error> {
        let (manifest, data) = self.metadata(manifest)?;
        let owner = package(data, &manifest)?;
        Ok(owner["targets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| {
                t["kind"]
                    .as_array()
                    .is_some_and(|k| k.iter().any(|k| k == "lib"))
            })
            .and_then(|t| t["name"].as_str())
            .map(str::to_owned))
    }
    pub fn local_dependencies(&mut self, manifest: &Path) -> Result<Vec<(String, PathBuf)>, Error> {
        let (manifest, data) = self.metadata(manifest)?;
        let owner = package(data, &manifest)?;
        Ok(owner["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|d| {
                Some((d["rename"].as_str().or_else(|| d["name"].as_str()))?.to_owned())
                    .zip(d["path"].as_str().map(PathBuf::from))
            })
            .collect())
    }
    pub fn tracked_paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.tracked.iter()
    }
    fn metadata(&mut self, manifest: &Path) -> Result<(PathBuf, &Value), Error> {
        let manifest = fs::canonicalize(manifest).map_err(|e| Error(e.to_string()))?;
        if let Some(key) = self.metadata.iter().find_map(|(key, data)| {
            data["packages"]
                .as_array()
                .is_some_and(|ps| {
                    ps.iter().any(|p| {
                        p["manifest_path"]
                            .as_str()
                            .is_some_and(|p| same_path(p, &manifest))
                    })
                })
                .then(|| key.clone())
        }) {
            return Ok((manifest, &self.metadata[&key]));
        }
        if !self.metadata.contains_key(&manifest) {
            let output = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
                .args([
                    "metadata",
                    "--offline",
                    "--no-deps",
                    "--format-version",
                    "1",
                    "--manifest-path",
                ])
                .arg(cargo_path(&manifest))
                .output()
                .map_err(|e| Error(format!("cannot read Cargo dependency graph: {e}")))?;
            if !output.status.success() {
                return Err(Error(format!(
                    "Cargo metadata failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
            let data: Value = serde_json::from_slice(&output.stdout)
                .map_err(|e| Error(format!("invalid Cargo metadata: {e}")))?;
            for p in data["packages"]
                .as_array()
                .ok_or_else(|| Error("Cargo metadata has no packages".into()))?
            {
                if let Some(path) = p["manifest_path"].as_str() {
                    self.tracked.insert(PathBuf::from(path));
                }
            }
            if let Some(workspace) = data["workspace_root"].as_str() {
                for name in ["Cargo.toml", "Cargo.lock"] {
                    let path = Path::new(workspace).join(name);
                    if path.is_file() {
                        self.tracked.insert(path);
                    }
                }
            }
            self.metadata.insert(manifest.clone(), data);
        }
        Ok((manifest.clone(), &self.metadata[&manifest]))
    }
}
fn same_path(a: &str, b: &Path) -> bool {
    fs::canonicalize(a).is_ok_and(|p| p == b)
}
impl Sources for CargoSources {
    fn dependency(&mut self, declaring_file: &Path, name: &str) -> Result<PathBuf, Error> {
        let (manifest, data) = self.metadata(&enclosing_manifest(declaring_file)?)?;
        let owner = package(data, &manifest)?;
        // A package can use its own Organon entry without depending on itself.
        if owner["name"].as_str() == Some(name) {
            let entry = manifest.parent().unwrap().join("organon.xml");
            if !entry.is_file() {
                return Err(Error(format!(
                    "package {name:?} does not provide {}",
                    entry.display()
                )));
            }
            return Ok(entry);
        }
        // Match the dependency key, not the published package name or lib target.
        let dependency = owner["dependencies"]
            .as_array()
            .and_then(|ds| {
                ds.iter()
                    .find(|d| d["rename"].as_str().or_else(|| d["name"].as_str()) == Some(name))
            })
            .ok_or_else(|| {
                Error(format!(
                    "{}: no direct dependency named {name:?}",
                    manifest.display()
                ))
            })?;
        if let Some(path) = dependency["path"].as_str() {
            let entry = Path::new(path).join("organon.xml");
            if !entry.is_file() {
                return Err(Error(format!(
                    "dependency {name:?} does not provide {}",
                    entry.display()
                )));
            }
            return Ok(entry);
        }
        let owner_id = owner["id"].clone();
        let dependency = dependency.clone();
        let target = std::env::var("CARGO_BUILD_TARGET")
            .ok()
            .or_else(|| {
                let result =
                    Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
                        .arg("-vV")
                        .output()
                        .ok()?;
                String::from_utf8(result.stdout)
                    .ok()?
                    .lines()
                    .find_map(|l| l.strip_prefix("host: ").map(str::to_owned))
            })
            .ok_or_else(|| {
                Error("cannot determine the Cargo target for package resolution".into())
            })?;
        let output = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args([
                "metadata",
                "--offline",
                "--locked",
                "--format-version",
                "1",
                "--filter-platform",
                &target,
                "--manifest-path",
            ])
            .arg(cargo_path(&manifest))
            .output()
            .map_err(|e| Error(e.to_string()))?;
        if !output.status.success() {
            return Err(Error(format!(
                "Cargo dependency resolution failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        let resolved: Value =
            serde_json::from_slice(&output.stdout).map_err(|e| Error(e.to_string()))?;
        let data = &resolved;
        let packages = data["packages"]
            .as_array()
            .ok_or_else(|| Error("missing resolved packages".into()))?;
        for package in packages {
            if let Some(path) = package["manifest_path"].as_str() {
                self.tracked.insert(PathBuf::from(path));
            }
        }
        let key = dependency["rename"]
            .as_str()
            .or_else(|| dependency["name"].as_str())
            .unwrap()
            .replace('-', "_");
        let node = data["resolve"]["nodes"]
            .as_array()
            .and_then(|ns| ns.iter().find(|n| n["id"] == owner_id))
            .ok_or_else(|| Error("missing Cargo resolve node".into()))?;
        let edge = node["deps"]
            .as_array()
            .and_then(|ds| ds.iter().find(|d| d["name"].as_str() == Some(key.as_str())))
            .ok_or_else(|| {
                Error(format!(
                    "dependency {name:?} is not enabled in the resolved Cargo graph"
                ))
            })?;
        let target = packages
            .iter()
            .find(|p| p["id"] == edge["pkg"])
            .ok_or_else(|| Error("missing dependency package".into()))?;
        let path = Path::new(
            target["manifest_path"]
                .as_str()
                .ok_or_else(|| Error("missing dependency manifest".into()))?,
        )
        .parent()
        .unwrap()
        .join("organon.xml");
        if !path.is_file() {
            return Err(Error(format!(
                "dependency {name:?} does not provide {}",
                path.display()
            )));
        }
        Ok(path)
    }
}

// Cargo workspace matching on Windows does not consistently accept verbatim
// paths, although canonical filesystem identities use them internally.
fn cargo_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{unc}"));
        }
        if let Some(plain) = text.strip_prefix(r"\\?\") {
            return PathBuf::from(plain);
        }
    }
    path.to_path_buf()
}

fn package<'a>(data: &'a Value, manifest: &Path) -> Result<&'a Value, Error> {
    data["packages"]
        .as_array()
        .and_then(|packages| {
            packages.iter().find(|p| {
                p["manifest_path"]
                    .as_str()
                    .is_some_and(|p| same_path(p, manifest))
            })
        })
        .ok_or_else(|| {
            Error(format!(
                "{}: owning package is absent from Cargo metadata",
                manifest.display()
            ))
        })
}
