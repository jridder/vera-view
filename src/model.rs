//! The Visio document model: pages, masters, style sheets and shapes, parsed
//! from the XML parts of a .vsdx package. Values are kept as raw ShapeSheet
//! cells; interpretation happens in `scene`.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use anyhow::{Context, Result, bail};
use roxmltree::Node;

use crate::package::{Package, Rel};

const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// A ShapeSheet cell: `V` is the cached value, `F` the formula (if any).
#[derive(Debug, Clone, Default)]
pub struct Cell {
    pub v: Option<String>,
    pub f: Option<String>,
}

impl Cell {
    pub fn num(&self) -> Option<f64> {
        self.v.as_deref().and_then(|v| v.trim().parse().ok())
    }
}

#[derive(Debug, Clone, Default)]
pub struct Row {
    /// Row type, e.g. `MoveTo`, `LineTo` (geometry rows only).
    pub t: Option<String>,
    pub del: bool,
    pub cells: HashMap<String, Cell>,
}

#[derive(Debug, Clone, Default)]
pub struct Section {
    pub del: bool,
    pub cells: HashMap<String, Cell>,
    pub rows: BTreeMap<u32, Row>,
}

/// A run of text that uses one Character row (`cp`) and one Paragraph row (`pp`).
#[derive(Debug, Clone)]
pub struct TextRun {
    pub cp: u32,
    pub pp: u32,
    pub text: String,
}

/// A shape, master shape or style sheet (they share the same XML structure).
#[derive(Debug, Clone, Default)]
pub struct Shape {
    pub id: u32,
    pub kind: String,
    pub del: bool,
    pub master: Option<u32>,
    pub master_shape: Option<u32>,
    pub line_style: Option<u32>,
    pub fill_style: Option<u32>,
    pub text_style: Option<u32>,
    pub cells: HashMap<String, Cell>,
    pub geometry: BTreeMap<u32, Section>,
    pub character: BTreeMap<u32, Row>,
    pub paragraph: BTreeMap<u32, Row>,
    pub text: Option<Vec<TextRun>>,
    /// Package part name of an embedded image (foreign shapes).
    pub image: Option<String>,
    /// `None` when the shape has no `<Shapes>` element of its own.
    pub children: Option<Vec<Shape>>,
}

pub struct Master {
    pub shapes: Vec<Shape>,
}

impl Master {
    /// The shape an instance inherits from when it does not name a `MasterShape`.
    pub fn top(&self) -> Option<&Shape> {
        match self.shapes.as_slice() {
            [only] => Some(only),
            _ => None,
        }
    }

    pub fn find(&self, id: u32) -> Option<&Shape> {
        fn walk(shapes: &[Shape], id: u32) -> Option<&Shape> {
            for s in shapes {
                if s.id == id {
                    return Some(s);
                }
                if let Some(found) = s.children.as_deref().and_then(|c| walk(c, id)) {
                    return Some(found);
                }
            }
            None
        }
        walk(&self.shapes, id)
    }
}

pub struct Page {
    pub id: u32,
    pub name: String,
    pub background: bool,
    pub back_page: Option<u32>,
    /// The PageSheet (page size, scale ...).
    pub sheet: Shape,
    pub shapes: Vec<Shape>,
}

pub struct Document {
    pub pages: Vec<Page>,
    pub masters: HashMap<u32, Master>,
    pub styles: HashMap<u32, Shape>,
    pub package: Package,
}

impl Document {
    pub fn open(path: &Path) -> Result<Self> {
        Self::from_package(Package::open(path)?)
    }

    pub fn from_package(package: Package) -> Result<Self> {
        let doc_part = package
            .rel_by_type("", "document")
            .unwrap_or_else(|| "visio/document.xml".to_string());
        let Some(doc_xml) = package.get_str(&doc_part) else {
            bail!("The package has no Visio document part. Is this really a Visio file?");
        };
        let styles = parse_styles(&doc_xml).context("Could not parse document.xml")?;

        let masters_part = package
            .rel_by_type(&doc_part, "masters")
            .unwrap_or_else(|| "visio/masters/masters.xml".to_string());
        let masters = parse_masters(&package, &masters_part)?;

        let pages_part = package
            .rel_by_type(&doc_part, "pages")
            .unwrap_or_else(|| "visio/pages/pages.xml".to_string());
        let pages = parse_pages(&package, &pages_part)?;
        if pages.is_empty() {
            bail!("The document contains no pages.");
        }

        Ok(Self { pages, masters, styles, package })
    }

    pub fn page_by_id(&self, id: u32) -> Option<&Page> {
        self.pages.iter().find(|p| p.id == id)
    }
}

fn parse_styles(xml: &str) -> Result<HashMap<u32, Shape>> {
    let doc = roxmltree::Document::parse(xml)?;
    let no_rels = HashMap::new();
    Ok(doc
        .descendants()
        .filter(|n| is(n, "StyleSheet"))
        .map(|n| {
            let s = parse_shape(n, &no_rels);
            (s.id, s)
        })
        .collect())
}

fn parse_masters(package: &Package, part: &str) -> Result<HashMap<u32, Master>> {
    let mut masters = HashMap::new();
    let Some(xml) = package.get_str(part) else {
        return Ok(masters);
    };
    let doc = roxmltree::Document::parse(&xml).with_context(|| format!("Could not parse {part}"))?;
    let rels = package.rels(part);
    for m in doc.root_element().children().filter(|n| is(n, "Master")) {
        let Some(id) = attr_u32(m, "ID") else { continue };
        let Some(target) = rel_id(m).and_then(|r| rels.get(&r)) else { continue };
        let Some(content) = package.get_str(&target.target) else { continue };
        let content_doc = roxmltree::Document::parse(&content)
            .with_context(|| format!("Could not parse {}", target.target))?;
        let content_rels = package.rels(&target.target);
        let shapes = top_shapes(content_doc.root_element(), &content_rels);
        masters.insert(id, Master { shapes });
    }
    Ok(masters)
}

fn parse_pages(package: &Package, part: &str) -> Result<Vec<Page>> {
    let Some(xml) = package.get_str(part) else {
        bail!("The document has no pages part ({part}).");
    };
    let doc = roxmltree::Document::parse(&xml).with_context(|| format!("Could not parse {part}"))?;
    let rels = package.rels(part);
    let no_rels = HashMap::new();
    let mut pages = Vec::new();
    for (index, p) in doc.root_element().children().filter(|n| is(n, "Page")).enumerate() {
        let id = attr_u32(p, "ID").unwrap_or(index as u32);
        let name = p
            .attribute("Name")
            .or(p.attribute("NameU"))
            .map(str::to_string)
            .unwrap_or_else(|| format!("Page-{}", index + 1));
        let sheet = p
            .children()
            .find(|n| is(n, "PageSheet"))
            .map(|n| parse_shape(n, &no_rels))
            .unwrap_or_default();
        let mut shapes = Vec::new();
        if let Some(target) = rel_id(p).and_then(|r| rels.get(&r))
            && let Some(content) = package.get_str(&target.target) {
                let content_doc = roxmltree::Document::parse(&content)
                    .with_context(|| format!("Could not parse {}", target.target))?;
                shapes = top_shapes(content_doc.root_element(), &package.rels(&target.target));
            }
        pages.push(Page {
            id,
            name,
            background: p.attribute("Background").is_some_and(|v| v == "1" || v == "true"),
            back_page: attr_u32(p, "BackPage"),
            sheet,
            shapes,
        });
    }
    Ok(pages)
}

/// Shapes directly under the `<Shapes>` element of a `PageContents`/`MasterContents` root.
fn top_shapes(root: Node, rels: &HashMap<String, Rel>) -> Vec<Shape> {
    root.children()
        .filter(|n| is(n, "Shapes"))
        .flat_map(|n| n.children().filter(|c| is(c, "Shape")))
        .map(|n| parse_shape(n, rels))
        .collect()
}

fn parse_shape(node: Node, rels: &HashMap<String, Rel>) -> Shape {
    let mut shape = Shape {
        id: attr_u32(node, "ID").unwrap_or(0),
        kind: node.attribute("Type").unwrap_or("Shape").to_string(),
        del: is_del(node),
        master: attr_u32(node, "Master"),
        master_shape: attr_u32(node, "MasterShape"),
        line_style: attr_u32(node, "LineStyle"),
        fill_style: attr_u32(node, "FillStyle"),
        text_style: attr_u32(node, "TextStyle"),
        ..Default::default()
    };
    for child in node.children().filter(Node::is_element) {
        match child.tag_name().name() {
            "Cell" => {
                if let Some((name, cell)) = parse_cell(child) {
                    shape.cells.insert(name, cell);
                }
            }
            "Section" => match child.attribute("N") {
                Some("Geometry") => {
                    let ix = attr_u32(child, "IX").unwrap_or(shape.geometry.len() as u32);
                    shape.geometry.insert(ix, parse_section(child));
                }
                Some("Character") => shape.character = parse_section(child).rows,
                Some("Paragraph") => shape.paragraph = parse_section(child).rows,
                _ => {}
            },
            "Text" => shape.text = Some(parse_text(child)),
            "ForeignData" => {
                shape.image = child
                    .descendants()
                    .find(|n| is(n, "Rel"))
                    .and_then(|r| rel_id_attr(r))
                    .and_then(|id| rels.get(&id))
                    .map(|r| r.target.clone());
            }
            "Shapes" => {
                shape.children = Some(
                    child
                        .children()
                        .filter(|n| is(n, "Shape"))
                        .map(|n| parse_shape(n, rels))
                        .collect(),
                );
            }
            _ => {}
        }
    }
    shape
}

fn parse_cell(node: Node) -> Option<(String, Cell)> {
    let name = node.attribute("N")?.to_string();
    Some((
        name,
        Cell {
            v: node.attribute("V").map(str::to_string),
            f: node.attribute("F").map(str::to_string),
        },
    ))
}

fn parse_section(node: Node) -> Section {
    let mut section = Section { del: is_del(node), ..Default::default() };
    for child in node.children().filter(Node::is_element) {
        match child.tag_name().name() {
            "Cell" => {
                if let Some((name, cell)) = parse_cell(child) {
                    section.cells.insert(name, cell);
                }
            }
            "Row" => {
                let ix = attr_u32(child, "IX").unwrap_or(section.rows.len() as u32);
                let mut row = Row {
                    t: child.attribute("T").map(str::to_string),
                    del: is_del(child),
                    cells: HashMap::new(),
                };
                for cell in child.children().filter(|n| is(n, "Cell")) {
                    if let Some((name, c)) = parse_cell(cell) {
                        row.cells.insert(name, c);
                    }
                }
                section.rows.insert(ix, row);
            }
            _ => {}
        }
    }
    section
}

/// Flatten the mixed content of a `<Text>` element into runs. `<cp>` and `<pp>`
/// markers switch the character/paragraph formatting for the text that follows.
fn parse_text(node: Node) -> Vec<TextRun> {
    let mut runs: Vec<TextRun> = Vec::new();
    let (mut cp, mut pp) = (0, 0);
    let mut push = |cp: u32, pp: u32, text: &str| match runs.last_mut() {
        Some(last) if last.cp == cp && last.pp == pp => last.text.push_str(text),
        _ => runs.push(TextRun { cp, pp, text: text.to_string() }),
    };
    for child in node.children() {
        if child.is_text() {
            push(cp, pp, child.text().unwrap_or_default());
            continue;
        }
        match child.tag_name().name() {
            "cp" => cp = attr_u32(child, "IX").unwrap_or(0),
            "pp" => pp = attr_u32(child, "IX").unwrap_or(0),
            // Fields hold their last displayed value as text content.
            "fld" => {
                let text: String = child.descendants().filter_map(|n| n.text()).collect();
                push(cp, pp, &text);
            }
            _ => {}
        }
    }
    runs
}

fn is(node: &Node, name: &str) -> bool {
    node.is_element() && node.tag_name().name() == name
}

fn is_del(node: Node) -> bool {
    node.attribute("Del").is_some_and(|v| v == "1" || v == "true")
}

fn attr_u32(node: Node, name: &str) -> Option<u32> {
    node.attribute(name).and_then(|v| v.trim().parse().ok())
}

/// The `r:id` of the first `<Rel>` child of `node`.
fn rel_id(node: Node) -> Option<String> {
    node.children().find(|n| is(n, "Rel")).and_then(rel_id_attr)
}

fn rel_id_attr(node: Node) -> Option<String> {
    node.attribute((REL_NS, "id"))
        .or_else(|| {
            node.attributes()
                .find(|a| a.name() == "id" && a.namespace().is_some())
                .map(|a| a.value())
        })
        .map(str::to_string)
}
