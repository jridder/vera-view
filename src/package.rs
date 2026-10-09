//! Reading the Open Packaging Convention (zip) container used by .vsdx files.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::Path;

use anyhow::{Context, Result, bail};

/// A relationship from one package part to another.
#[derive(Debug, Clone)]
pub struct Rel {
    /// Absolute part name of the target (no leading slash, lowercase).
    pub target: String,
    pub kind: String,
}

/// The package's zip archive. Parts are decompressed on demand, since page XML
/// can be very large and is only needed while parsing. Part names are matched
/// case-insensitively, as OPC requires.
pub struct Package {
    archive: RefCell<zip::ZipArchive<Cursor<Vec<u8>>>>,
    /// Lowercase part name (no leading slash) -> index in the archive.
    index: HashMap<String, usize>,
}

impl Package {
    pub fn open(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path).with_context(|| format!("Could not read {}", path.display()))?;
        Self::from_bytes(bytes)
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        // Compound File Binary signature: legacy .vsd/.vss/.vst files.
        if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]) {
            bail!(
                "This is a legacy binary Visio file (.vsd). Only the Open XML formats \
                 (.vsdx, .vsdm, .vstx, .vstm) are supported. Save it as .vsdx in Visio to view it here."
            );
        }
        let archive = zip::ZipArchive::new(Cursor::new(bytes))
            .context("The file is not a valid Visio Open XML package (it is not a zip archive)")?;
        let index = (0..archive.len())
            .filter_map(|i| {
                let name = archive.name_for_index(i)?;
                (!name.ends_with('/')).then(|| (name.trim_start_matches('/').to_ascii_lowercase(), i))
            })
            .collect();
        Ok(Self { archive: RefCell::new(archive), index })
    }

    /// Decompressed contents of a part, or `None` if it is missing or unreadable.
    pub fn get(&self, part: &str) -> Option<Vec<u8>> {
        let i = *self.index.get(&part.trim_start_matches('/').to_ascii_lowercase())?;
        let mut archive = self.archive.borrow_mut();
        let mut file = archive.by_index(i).ok()?;
        let mut buf = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut buf).ok()?;
        Some(buf)
    }

    pub fn get_str(&self, part: &str) -> Option<String> {
        let mut bytes = self.get(part)?;
        if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            bytes.drain(..3);
        }
        Some(String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()))
    }

    /// Relationships of `part`, keyed by relationship id.
    pub fn rels(&self, part: &str) -> HashMap<String, Rel> {
        let mut out = HashMap::new();
        let Some(xml) = self.get_str(&rels_part_name(part)) else {
            return out;
        };
        let Ok(doc) = roxmltree::Document::parse(&xml) else {
            return out;
        };
        for node in doc.descendants().filter(|n| n.has_tag_name_local("Relationship")) {
            if node.attribute("TargetMode").is_some_and(|m| m.eq_ignore_ascii_case("external")) {
                continue;
            }
            let (Some(id), Some(target)) = (node.attribute("Id"), node.attribute("Target")) else {
                continue;
            };
            out.insert(
                id.to_string(),
                Rel {
                    target: resolve(part, target),
                    kind: node.attribute("Type").unwrap_or_default().to_string(),
                },
            );
        }
        out
    }

    /// First relationship of `part` whose type ends with `/suffix`.
    pub fn rel_by_type(&self, part: &str, suffix: &str) -> Option<String> {
        let mut rels: Vec<_> = self.rels(part).into_iter().collect();
        rels.sort_by(|a, b| a.0.cmp(&b.0));
        rels.into_iter()
            .find(|(_, r)| r.kind.rsplit('/').next() == Some(suffix))
            .map(|(_, r)| r.target)
    }
}

trait LocalName {
    fn has_tag_name_local(&self, name: &str) -> bool;
}

impl LocalName for roxmltree::Node<'_, '_> {
    fn has_tag_name_local(&self, name: &str) -> bool {
        self.is_element() && self.tag_name().name() == name
    }
}

/// `visio/pages/page1.xml` -> `visio/pages/_rels/page1.xml.rels`
fn rels_part_name(part: &str) -> String {
    let part = part.trim_start_matches('/');
    match part.rsplit_once('/') {
        Some((dir, file)) => format!("{dir}/_rels/{file}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

/// Resolve a relationship target relative to the part that owns the relationship.
pub fn resolve(base_part: &str, target: &str) -> String {
    let target = target.replace('\\', "/");
    let joined = if let Some(abs) = target.strip_prefix('/') {
        abs.to_string()
    } else {
        let base = base_part.trim_start_matches('/');
        match base.rsplit_once('/') {
            Some((dir, _)) => format!("{dir}/{target}"),
            None => target,
        }
    };
    let mut segments: Vec<&str> = Vec::new();
    for seg in joined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            s => segments.push(s),
        }
    }
    segments.join("/").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_targets() {
        assert_eq!(resolve("visio/pages/page1.xml", "../media/image1.png"), "visio/media/image1.png");
        assert_eq!(resolve("visio/document.xml", "pages/pages.xml"), "visio/pages/pages.xml");
        assert_eq!(resolve("", "/visio/document.xml"), "visio/document.xml");
        assert_eq!(rels_part_name("visio/pages/page1.xml"), "visio/pages/_rels/page1.xml.rels");
        assert_eq!(rels_part_name(""), "_rels/.rels");
    }
}
