//! Turns a page of the document model into a flat list of drawable items in
//! paper coordinates (inches, origin at the bottom-left of the page, y up).
//!
//! This is where ShapeSheet semantics live: inheritance from masters and style
//! sheets, shape transforms, geometry rows, text blocks and line ends.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use eframe::egui::Color32;
use lyon_tessellation::math::point;
use lyon_tessellation::path::Path as LyonPath;
use lyon_tessellation::{BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, VertexBuffers};

use crate::formula;
use crate::geometry::{self, Affine, P, dist};
use crate::model::{Cell, Document, Master, Page, Row, Shape};

pub struct Scene {
    /// Paper size in inches.
    pub width: f32,
    pub height: f32,
    pub items: Vec<Item>,
    pub shape_count: usize,
    pub warnings: BTreeSet<String>,
}

pub struct Item {
    /// Bounding box in paper coordinates: [min_x, min_y, max_x, max_y].
    pub bbox: [f32; 4],
    pub kind: ItemKind,
}

pub enum ItemKind {
    /// A tessellated fill (handles concave outlines and holes).
    Fill { verts: Vec<[f32; 2]>, indices: Vec<u32>, color: Color32 },
    Stroke { pts: Vec<[f32; 2]>, closed: bool, width: f32, color: Color32, pattern: u32 },
    /// Small filled convex polygon, used for arrowheads.
    Polygon { pts: Vec<[f32; 2]>, color: Color32 },
    Text(TextItem),
    /// Corners are top-left, top-right, bottom-right, bottom-left of the image.
    Image { part: String, quad: [[f32; 2]; 4] },
    Placeholder { quad: [[f32; 2]; 4], label: String },
}

pub struct TextItem {
    pub center: [f32; 2],
    pub width: f32,
    pub height: f32,
    /// left, right, top, bottom (inches)
    pub margins: [f32; 4],
    /// Counter-clockwise rotation in paper space (radians).
    pub angle: f32,
    /// 0 = left, 1 = centre, 2 = right (and justified variants treated as left)
    pub halign: u8,
    /// 0 = top, 1 = middle, 2 = bottom
    pub valign: u8,
    pub runs: Vec<StyledRun>,
    /// All runs' text joined, for searching.
    pub plain: String,
}

pub struct StyledRun {
    pub text: String,
    /// Font size in inches.
    pub size: f32,
    pub color: Color32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
}

/// Visio's built-in colour palette (used when a colour cell holds an index).
const PALETTE: [u32; 24] = [
    0x000000, 0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00, 0xFF00FF, 0x00FFFF, 0x800000, 0x008000,
    0x000080, 0x808000, 0x800080, 0x008080, 0xC0C0C0, 0xE6E6E6, 0xCDCDCD, 0xB3B3B3, 0x9A9A9A, 0x808080,
    0x666666, 0x4D4D4D, 0x333333, 0x1A1A1A,
];

const DEFAULT_FONT_SIZE: f64 = 12.0 / 72.0;
const DEFAULT_MARGIN: f64 = 4.0 / 72.0;
const DEFAULT_LINE_WEIGHT: f64 = 0.75 / 72.0;

pub fn build(doc: &Document, page_index: usize) -> Scene {
    let page = &doc.pages[page_index];
    let (w, h, s) = page_metrics(page);
    let mut builder = Builder {
        doc,
        items: Vec::new(),
        shape_count: 0,
        warnings: BTreeSet::new(),
        tessellator: FillTessellator::new(),
    };

    // Background pages are drawn first, innermost background at the bottom.
    let mut chain: Vec<&Page> = Vec::new();
    let mut seen = HashSet::from([page.id]);
    let mut next = page.back_page;
    while let Some(id) = next {
        match doc.page_by_id(id) {
            Some(bg) if seen.insert(bg.id) => {
                chain.push(bg);
                next = bg.back_page;
            }
            _ => break,
        }
    }
    for bg in chain.iter().rev() {
        builder.page(bg);
    }
    builder.page(page);

    Scene {
        width: (w * s) as f32,
        height: (h * s) as f32,
        items: builder.items,
        shape_count: builder.shape_count,
        warnings: builder.warnings,
    }
}

/// The master shape `local` inherits from, and the master its sub-shapes resolve
/// `MasterShape` references against. `Err` carries the id of a missing master.
fn resolve_master<'a>(
    doc: &'a Document,
    local: &Shape,
    parent_master: Option<&'a Master>,
) -> Result<(Option<&'a Shape>, Option<&'a Master>), u32> {
    Ok(match local.master {
        Some(id) => {
            let m = doc.masters.get(&id).ok_or(id)?;
            (local.master_shape.and_then(|ms| m.find(ms)).or_else(|| m.top()), Some(m))
        }
        None => match (local.master_shape, parent_master) {
            (Some(ms), Some(m)) => (m.find(ms), Some(m)),
            _ => (None, parent_master),
        },
    })
}

/// Whether any shape on the page (or its background pages) has text accepted by
/// `matches`. Much cheaper than building the page's scene.
pub fn page_has_text(doc: &Document, page_index: usize, matches: &dyn Fn(&str) -> bool) -> bool {
    fn walk(doc: &Document, shapes: &[Shape], parent_master: Option<&Master>, matches: &dyn Fn(&str) -> bool) -> bool {
        shapes.iter().any(|s| {
            if s.del || s.kind.eq_ignore_ascii_case("Guide") {
                return false;
            }
            let (master, master_doc) = resolve_master(doc, s, parent_master).unwrap_or((None, None));
            let text = s.text.as_ref().or_else(|| master.and_then(|m| m.text.as_ref()));
            if text.is_some_and(|runs| matches(&runs.iter().map(|r| r.text.as_str()).collect::<String>())) {
                return true;
            }
            let children = s.children.as_deref().or_else(|| master.and_then(|m| m.children.as_deref()));
            children.is_some_and(|c| walk(doc, c, master_doc, matches))
        })
    }
    let mut seen = HashSet::new();
    let mut next = doc.pages.get(page_index);
    while let Some(page) = next.filter(|p| seen.insert(p.id)) {
        if walk(doc, &page.shapes, None, matches) {
            return true;
        }
        next = page.back_page.and_then(|id| doc.page_by_id(id));
    }
    false
}

/// Page width, height (drawing units) and the drawing-to-paper scale factor.
pub fn page_metrics(page: &Page) -> (f64, f64, f64) {
    let get = |name: &str, default: f64| {
        page.sheet.cells.get(name).and_then(Cell::num).filter(|v| v.is_finite() && *v > 0.0).unwrap_or(default)
    };
    let w = get("PageWidth", 8.5);
    let h = get("PageHeight", 11.0);
    let scale = get("PageScale", 1.0) / get("DrawingScale", 1.0);
    (w, h, if scale.is_finite() && scale > 0.0 { scale } else { 1.0 })
}

struct Builder<'a> {
    doc: &'a Document,
    items: Vec<Item>,
    shape_count: usize,
    warnings: BTreeSet<String>,
    tessellator: FillTessellator,
}

#[derive(Clone, Copy, PartialEq)]
enum Axis {
    X,
    Y,
    /// A length not tied to one axis (e.g. an arc's bow): scaled by the mean ratio.
    Mean,
    None,
}

#[derive(Clone, Copy)]
enum StyleKind {
    Line,
    Fill,
    Text,
}

/// A shape together with the master shape it inherits from.
struct Resolved<'a> {
    doc: &'a Document,
    local: &'a Shape,
    master: Option<&'a Shape>,
    width: f64,
    height: f64,
    /// instance size / master size, for scaling inherited values we cannot recompute.
    ratio: [f64; 2],
}

/// A geometry row seen through inheritance: local cells override master cells.
#[derive(Clone, Copy)]
struct RowView<'a> {
    local: Option<&'a Row>,
    master: Option<&'a Row>,
}

impl<'a> RowView<'a> {
    fn kind(&self) -> Option<&'a str> {
        self.local.and_then(|r| r.t.as_deref()).or_else(|| self.master.and_then(|r| r.t.as_deref()))
    }

    fn get(&self, name: &str) -> Option<(&'a Cell, bool)> {
        self.local
            .and_then(|r| r.cells.get(name))
            .map(|c| (c, false))
            .or_else(|| self.master.and_then(|r| r.cells.get(name)).map(|c| (c, true)))
    }
}

impl<'a> Resolved<'a> {
    fn new(doc: &'a Document, local: &'a Shape, master: Option<&'a Shape>) -> Self {
        let cell = |s: &Shape, n: &str| s.cells.get(n).and_then(Cell::num);
        let width = cell(local, "Width").or_else(|| master.and_then(|m| cell(m, "Width"))).unwrap_or(0.0);
        let height = cell(local, "Height").or_else(|| master.and_then(|m| cell(m, "Height"))).unwrap_or(0.0);
        let ratio_of = |inst: f64, name: &str| match master.and_then(|m| cell(m, name)) {
            Some(m) if m.abs() > 1e-9 => inst / m,
            _ => 1.0,
        };
        let ratio = [ratio_of(width, "Width"), ratio_of(height, "Height")];
        Self { doc, local, master, width, height, ratio }
    }

    fn cell(&self, name: &str) -> Option<(&'a Cell, bool)> {
        self.local
            .cells
            .get(name)
            .map(|c| (c, false))
            .or_else(|| self.master.and_then(|m| m.cells.get(name)).map(|c| (c, true)))
    }

    fn env_value(&self, name: &str, extra: &[(&str, f64)]) -> Option<f64> {
        if name.eq_ignore_ascii_case("Width") {
            return Some(self.width);
        }
        if name.eq_ignore_ascii_case("Height") {
            return Some(self.height);
        }
        extra.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| *v)
    }

    /// Numeric value of a cell. Values inherited from a master were computed
    /// for the master's size, so formulas are re-evaluated with this shape's
    /// size, and failing that the cached value is scaled.
    fn value(&self, cell: &Cell, from_master: bool, axis: Axis, extra: &[(&str, f64)]) -> Option<f64> {
        let env = |n: &str| self.env_value(n, extra);
        if !from_master {
            return cell.num().or_else(|| cell.f.as_deref().and_then(|f| formula::eval(f, &env)));
        }
        match cell.f.as_deref() {
            Some(f) => formula::eval(f, &env).or_else(|| {
                cell.num().map(|v| match axis {
                    Axis::X => v * self.ratio[0],
                    Axis::Y => v * self.ratio[1],
                    Axis::Mean => v * (self.ratio[0] * self.ratio[1]).abs().sqrt(),
                    Axis::None => v,
                })
            }),
            None => cell.num(),
        }
    }

    fn num(&self, name: &str, axis: Axis, extra: &[(&str, f64)]) -> Option<f64> {
        let (cell, from_master) = self.cell(name)?;
        self.value(cell, from_master, axis, extra)
    }

    fn style_id(&self, kind: StyleKind) -> Option<u32> {
        let pick = |s: &Shape| match kind {
            StyleKind::Line => s.line_style,
            StyleKind::Fill => s.fill_style,
            StyleKind::Text => s.text_style,
        };
        pick(self.local).or_else(|| self.master.and_then(pick))
    }

    /// The style sheets this shape inherits from, nearest first.
    fn styles(&self, kind: StyleKind) -> impl Iterator<Item = &'a Shape> + use<'a> {
        let styles = &self.doc.styles;
        let parent = move |s: &Shape| match kind {
            StyleKind::Line => s.line_style,
            StyleKind::Fill => s.fill_style,
            StyleKind::Text => s.text_style,
        };
        let first = self.style_id(kind).and_then(|id| styles.get(&id));
        // The depth limit guards against cyclic style references.
        std::iter::successors(first, move |s| parent(s).filter(|&id| id != s.id).and_then(|id| styles.get(&id))).take(16)
    }

    fn styled(&self, name: &str, kind: StyleKind) -> Option<&'a Cell> {
        self.cell(name).map(|(c, _)| c).or_else(|| self.styles(kind).find_map(|s| s.cells.get(name)))
    }

    fn styled_num(&self, name: &str, kind: StyleKind) -> Option<f64> {
        self.styled(name, kind).and_then(|c| c.num().or_else(|| c.f.as_deref().and_then(|f| formula::eval(f, &|_| None))))
    }

    fn styled_color(&self, name: &str, kind: StyleKind) -> Option<Color32> {
        self.styled(name, kind).and_then(parse_color)
    }

    /// A cell of a Character (`para == false`) or Paragraph row.
    fn text_row_cell(&self, para: bool, ix: u32, name: &str) -> Option<&'a Cell> {
        let rows = |s: &'a Shape| if para { &s.paragraph } else { &s.character };
        let from = |s: &'a Shape, ix: u32| rows(s).get(&ix).and_then(|r| r.cells.get(name));
        from(self.local, ix)
            .or_else(|| self.master.and_then(|m| from(m, ix)))
            .or_else(|| from(self.local, 0))
            .or_else(|| self.master.and_then(|m| from(m, 0)))
            .or_else(|| self.styles(StyleKind::Text).find_map(|s| from(s, 0)))
    }
}

fn parse_color(cell: &Cell) -> Option<Color32> {
    let v = cell.v.as_deref().unwrap_or_default().trim();
    if let Some(hex) = v.strip_prefix('#')
        && hex.len() == 6 {
            let n = u32::from_str_radix(hex, 16).ok()?;
            return Some(rgb(n));
        }
    if let Ok(index) = v.parse::<usize>() {
        return PALETTE.get(index).map(|&n| rgb(n));
    }
    let f = cell.f.as_deref().unwrap_or(v);
    if let Some(args) = formula::call_args(f, "RGB", &|_| None)
        && let [r, g, b] = args[..] {
            return Some(Color32::from_rgb(r as u8, g as u8, b as u8));
        }
    None
}

fn rgb(n: u32) -> Color32 {
    Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8)
}

fn with_transparency(color: Color32, transparency: f64) -> Color32 {
    let alpha = ((1.0 - transparency.clamp(0.0, 1.0)) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

struct SubPath {
    pts: Vec<P>,
    closed: bool,
}

struct GeometrySection {
    paths: Vec<SubPath>,
    no_fill: bool,
    no_line: bool,
}

impl Builder<'_> {
    fn page(&mut self, page: &Page) {
        let (_, _, s) = page_metrics(page);
        let t = Affine::scale(s, s);
        for shape in &page.shapes {
            self.shape(shape, None, &t);
        }
    }

    fn shape(&mut self, local: &Shape, parent_master: Option<&Master>, parent: &Affine) {
        if local.del || local.kind.eq_ignore_ascii_case("Guide") {
            return;
        }
        self.shape_count += 1;
        let (master, master_doc) = resolve_master(self.doc, local, parent_master).unwrap_or_else(|id| {
            self.warnings.insert(format!("Missing master {id}"));
            (None, None)
        });
        let r = Resolved::new(self.doc, local, master);
        let (w, h) = (r.width, r.height);

        let pin_x = r.num("PinX", Axis::None, &[]).unwrap_or(0.0);
        let pin_y = r.num("PinY", Axis::None, &[]).unwrap_or(0.0);
        let loc_x = r.num("LocPinX", Axis::X, &[]).unwrap_or(w * 0.5);
        let loc_y = r.num("LocPinY", Axis::Y, &[]).unwrap_or(h * 0.5);
        let angle = r.num("Angle", Axis::None, &[]).unwrap_or(0.0);
        let flip_x = r.num("FlipX", Axis::None, &[]).unwrap_or(0.0) != 0.0;
        let flip_y = r.num("FlipY", Axis::None, &[]).unwrap_or(0.0) != 0.0;
        let local_t = Affine::translate(pin_x, pin_y)
            .then_after(&Affine::rotate(angle))
            .then_after(&Affine::scale(if flip_x { -1.0 } else { 1.0 }, if flip_y { -1.0 } else { 1.0 }))
            .then_after(&Affine::translate(-loc_x, -loc_y));
        let t = parent.then_after(&local_t);

        let image = local.image.as_ref().or_else(|| master.and_then(|m| m.image.as_ref()));
        if let Some(part) = image {
            self.image(&r, part, &t);
        }

        let sections = self.geometry(&r);
        self.draw_geometry(&r, &sections, &t);

        if let Some(children) = &local.children {
            for child in children {
                self.shape(child, master_doc, &t);
            }
        } else if let Some(children) = master.and_then(|m| m.children.as_ref()) {
            // Sub-shapes only stored in the master: stretch them to this instance.
            let ct = t.then_after(&Affine::scale(r.ratio[0], r.ratio[1]));
            for child in children {
                self.shape(child, master_doc, &ct);
            }
        }

        self.text(&r, &t);
    }

    fn geometry(&mut self, r: &Resolved) -> Vec<GeometrySection> {
        let (w, h) = (r.width, r.height);
        let empty = BTreeMap::new();
        let local_geo = &r.local.geometry;
        let master_geo = r.master.map(|m| &m.geometry).unwrap_or(&empty);
        let ixs: BTreeSet<u32> = local_geo.keys().chain(master_geo.keys()).copied().collect();

        let mut out = Vec::new();
        for ix in ixs {
            let ls = local_geo.get(&ix);
            let ms = master_geo.get(&ix);
            if ls.is_some_and(|s| s.del) || (ls.is_none() && ms.is_some_and(|s| s.del)) {
                continue;
            }
            let flag = |name: &str| {
                ls.and_then(|s| s.cells.get(name))
                    .or_else(|| ms.and_then(|s| s.cells.get(name)))
                    .and_then(Cell::num)
                    .is_some_and(|v| v != 0.0)
            };
            if flag("NoShow") {
                continue;
            }
            let row_ixs: BTreeSet<u32> = ls
                .map(|s| s.rows.keys())
                .into_iter()
                .flatten()
                .chain(ms.map(|s| s.rows.keys()).into_iter().flatten())
                .copied()
                .collect();

            let mut paths = Vec::new();
            let mut cur: Vec<P> = Vec::new();
            let mut pos: P = [0.0, 0.0];
            let flush = |cur: &mut Vec<P>, paths: &mut Vec<SubPath>| {
                if cur.len() >= 2 {
                    let size = w.abs().max(h.abs()).max(1e-6);
                    let closed = dist(cur[0], *cur.last().unwrap()) < size * 1e-6;
                    paths.push(SubPath { pts: std::mem::take(cur), closed });
                }
                cur.clear();
            };

            for rix in row_ixs {
                let row = RowView {
                    local: ls.and_then(|s| s.rows.get(&rix)),
                    master: ms.and_then(|s| s.rows.get(&rix)),
                };
                if row.local.is_some_and(|r| r.del) {
                    continue;
                }
                let Some(kind) = row.kind() else { continue };
                let val = |name: &str, axis: Axis| {
                    row.get(name).and_then(|(c, m)| r.value(c, m, axis, &[])).unwrap_or(0.0)
                };
                let xy = || [val("X", Axis::X), val("Y", Axis::Y)];
                let rel = |xn: &str, yn: &str| [val(xn, Axis::None) * w, val(yn, Axis::None) * h];
                if !matches!(kind, "MoveTo" | "RelMoveTo" | "Ellipse" | "InfiniteLine") && cur.is_empty() {
                    cur.push(pos);
                }
                match kind {
                    "MoveTo" | "RelMoveTo" => {
                        flush(&mut cur, &mut paths);
                        pos = if kind == "MoveTo" { xy() } else { rel("X", "Y") };
                        cur.push(pos);
                    }
                    "LineTo" | "SplineStart" | "SplineKnot" => {
                        pos = xy();
                        cur.push(pos);
                    }
                    "RelLineTo" => {
                        pos = rel("X", "Y");
                        cur.push(pos);
                    }
                    "ArcTo" => {
                        let end = xy();
                        geometry::arc_to(pos, end, val("A", Axis::Mean), &mut cur);
                        pos = end;
                    }
                    "EllipticalArcTo" | "RelEllipticalArcTo" => {
                        let (end, ctrl) = if kind == "EllipticalArcTo" {
                            (xy(), [val("A", Axis::X), val("B", Axis::Y)])
                        } else {
                            (rel("X", "Y"), rel("A", "B"))
                        };
                        let ratio = row.get("D").map_or(1.0, |_| val("D", Axis::None));
                        geometry::elliptical_arc_to(pos, ctrl, end, val("C", Axis::None), ratio, &mut cur);
                        pos = end;
                    }
                    "RelCubBezTo" => {
                        let end = rel("X", "Y");
                        geometry::cubic_to(pos, rel("A", "B"), rel("C", "D"), end, &mut cur);
                        pos = end;
                    }
                    "RelQuadBezTo" => {
                        let end = rel("X", "Y");
                        geometry::quad_to(pos, rel("A", "B"), end, &mut cur);
                        pos = end;
                    }
                    "PolylineTo" => {
                        let end = xy();
                        if let Some((args, from_master)) = self.formula_args(r, &row, "A", "POLYLINE")
                            && args.len() >= 2 {
                                let pts = args[2..].as_chunks::<2>().0.iter().map(|c| [c[0], c[1]]);
                                for p in pts {
                                    cur.push(fit_point(p, args[0], args[1], w, h, from_master, r.ratio));
                                }
                            }
                        cur.push(end);
                        pos = end;
                    }
                    "NURBSTo" => {
                        let end = xy();
                        self.nurbs(r, &row, pos, end, &mut cur);
                        pos = end;
                    }
                    "Ellipse" => {
                        flush(&mut cur, &mut paths);
                        let center = xy();
                        let a = [val("A", Axis::X), val("B", Axis::Y)];
                        let b = [val("C", Axis::X), val("D", Axis::Y)];
                        paths.push(SubPath { pts: geometry::ellipse(center, a, b), closed: true });
                    }
                    "InfiniteLine" => {}
                    other => {
                        self.warnings.insert(format!("Unsupported geometry row '{other}'"));
                        pos = xy();
                        cur.push(pos);
                    }
                }
            }
            flush(&mut cur, &mut paths);
            out.push(GeometrySection { paths, no_fill: flag("NoFill"), no_line: flag("NoLine") });
        }
        out
    }

    /// Arguments of a formula-valued cell such as `POLYLINE(...)` or `NURBS(...)`.
    fn formula_args(&self, r: &Resolved, row: &RowView, cell: &str, func: &str) -> Option<(Vec<f64>, bool)> {
        let (c, from_master) = row.get(cell)?;
        let src = c.f.as_deref().or(c.v.as_deref())?;
        let env = |n: &str| r.env_value(n, &[]);
        formula::call_args(src, func, &env).map(|a| (a, from_master))
    }

    /// `NURBSTo`: X,Y = last control point, A = second-to-last knot, B = last weight,
    /// C = first knot, D = first weight, E = NURBS(knotLast, degree, xType, yType, {x, y, knot, weight}...).
    fn nurbs(&mut self, r: &Resolved, row: &RowView, start: P, end: P, out: &mut Vec<P>) {
        let Some((args, from_master)) = self.formula_args(r, row, "E", "NURBS") else {
            out.push(end);
            return;
        };
        if args.len() < 4 {
            out.push(end);
            return;
        }
        let val = |name: &str| row.get(name).and_then(|(c, m)| r.value(c, m, Axis::None, &[])).unwrap_or(0.0);
        let (knot_last, degree, x_type, y_type) = (args[0], args[1].round().max(1.0) as usize, args[2], args[3]);
        let mut ctrl = vec![start];
        let mut weights = vec![val("D")];
        let mut knots = vec![val("C")];
        for q in args[4..].as_chunks::<4>().0 {
            ctrl.push(fit_point([q[0], q[1]], x_type, y_type, r.width, r.height, from_master, r.ratio));
            knots.push(q[2]);
            weights.push(q[3]);
        }
        ctrl.push(end);
        weights.push(val("B"));
        knots.push(val("A"));
        knots.push(knot_last);

        // Visio stores one knot per control point; build a clamped knot vector so the
        // curve runs from the current point to the end point.
        let n = ctrl.len();
        let degree = degree.min(n - 1);
        let (k0, k1) = (knots[0], *knots.last().unwrap());
        let (k0, k1) = if k1 > k0 { (k0, k1) } else { (0.0, 1.0) };
        let needed = n - degree - 1;
        let mut interior: Vec<f64> = knots[1..knots.len() - 1]
            .iter()
            .copied()
            .filter(|k| *k > k0 + 1e-9 && *k < k1 - 1e-9)
            .collect();
        if interior.len() != needed {
            interior = (1..=needed).map(|i| k0 + (k1 - k0) * i as f64 / (needed + 1) as f64).collect();
        }
        let mut full = vec![k0; degree + 1];
        full.extend(interior);
        full.extend(std::iter::repeat_n(k1, degree + 1));
        geometry::nurbs(&ctrl, &weights, &full, degree, out);
    }

    fn draw_geometry(&mut self, r: &Resolved, sections: &[GeometrySection], t: &Affine) {
        if sections.iter().all(|s| s.paths.is_empty()) {
            return;
        }
        let to_paper = |pts: &[P]| -> Vec<[f32; 2]> {
            pts.iter().map(|&p| t.apply(p)).map(|p| [p[0] as f32, p[1] as f32]).collect()
        };

        // Fill: all closed sub-paths together, even-odd, as Visio does for combined shapes.
        let pattern = r.styled_num("FillPattern", StyleKind::Fill).unwrap_or(1.0);
        if pattern != 0.0 {
            let closed: Vec<Vec<[f32; 2]>> = sections
                .iter()
                .filter(|s| !s.no_fill)
                .flat_map(|s| s.paths.iter().filter(|p| p.closed))
                .map(|p| to_paper(&p.pts))
                .collect();
            if !closed.is_empty() {
                let color = r.styled_color("FillForegnd", StyleKind::Fill).unwrap_or(Color32::WHITE);
                let trans = r.styled_num("FillForegndTrans", StyleKind::Fill).unwrap_or(0.0);
                self.fill(&closed, with_transparency(color, trans));
            }
        }

        let line_pattern = r.styled_num("LinePattern", StyleKind::Line).unwrap_or(1.0) as u32;
        if line_pattern == 0 {
            return;
        }
        let color = r.styled_color("LineColor", StyleKind::Line).unwrap_or(Color32::BLACK);
        let color = with_transparency(color, r.styled_num("LineColorTrans", StyleKind::Line).unwrap_or(0.0));
        let width = r.styled_num("LineWeight", StyleKind::Line).unwrap_or(DEFAULT_LINE_WEIGHT).max(0.0) as f32;
        let mut open_paths: Vec<Vec<[f32; 2]>> = Vec::new();
        for section in sections.iter().filter(|s| !s.no_line) {
            for path in &section.paths {
                let pts = to_paper(&path.pts);
                if !path.closed {
                    open_paths.push(pts.clone());
                }
                let pad = width;
                self.push(bbox_of(&pts, pad), ItemKind::Stroke { pts, closed: path.closed, width, color, pattern: line_pattern });
            }
        }

        // Line ends apply to the start of the first and the end of the last open path.
        let begin = r.styled_num("BeginArrow", StyleKind::Line).unwrap_or(0.0) as u32;
        let end = r.styled_num("EndArrow", StyleKind::Line).unwrap_or(0.0) as u32;
        if let Some(first) = open_paths.first().filter(|_| begin != 0) {
            let size = r.styled_num("BeginArrowSize", StyleKind::Line).unwrap_or(2.0);
            self.arrow(first, begin, size, width, color);
        }
        if let Some(last) = open_paths.last().filter(|_| end != 0) {
            let size = r.styled_num("EndArrowSize", StyleKind::Line).unwrap_or(2.0);
            let rev: Vec<[f32; 2]> = last.iter().rev().copied().collect();
            self.arrow(&rev, end, size, width, color);
        }
    }

    fn fill(&mut self, paths: &[Vec<[f32; 2]>], color: Color32) {
        let mut builder = LyonPath::builder();
        for pts in paths {
            if pts.len() < 3 {
                continue;
            }
            builder.begin(point(pts[0][0], pts[0][1]));
            for p in &pts[1..] {
                builder.line_to(point(p[0], p[1]));
            }
            builder.end(true);
        }
        let path = builder.build();
        let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
        let options = FillOptions::default().with_fill_rule(FillRule::EvenOdd).with_tolerance(0.0005);
        let result = self.tessellator.tessellate_path(
            &path,
            &options,
            &mut BuffersBuilder::new(&mut buffers, |v: FillVertex| v.position().to_array()),
        );
        if result.is_err() || buffers.indices.is_empty() {
            return;
        }
        let bbox = bbox_of(&buffers.vertices, 0.0);
        self.push(bbox, ItemKind::Fill { verts: buffers.vertices, indices: buffers.indices, color });
    }

    /// Arrowhead at `pts[0]`, pointing away from the rest of the path.
    fn arrow(&mut self, pts: &[[f32; 2]], kind: u32, size: f64, weight: f32, color: Color32) {
        let Some(&tip) = pts.first() else { return };
        let length = (0.06 + 0.025 * size.clamp(0.0, 6.0)) as f32 + weight * 2.0;
        // Use a point about one arrow length back, so curved paths aim sensibly.
        let back = pts
            .iter()
            .skip(1)
            .find(|p| (p[0] - tip[0]).hypot(p[1] - tip[1]) >= length)
            .or(pts.last())
            .copied()
            .unwrap_or(tip);
        let (dx, dy) = (tip[0] - back[0], tip[1] - back[1]);
        let len = dx.hypot(dy);
        if len < 1e-6 {
            return;
        }
        let (ux, uy) = (dx / len, dy / len);
        let half = length * 0.35;
        let base = [tip[0] - ux * length, tip[1] - uy * length];
        let left = [base[0] - uy * half, base[1] + ux * half];
        let right = [base[0] + uy * half, base[1] - ux * half];
        let pts = vec![left, tip, right];
        let bbox = bbox_of(&pts, weight);
        if kind == 1 {
            // Open arrowhead: two strokes.
            self.push(bbox, ItemKind::Stroke { pts, closed: false, width: weight, color, pattern: 1 });
        } else {
            self.push(bbox, ItemKind::Polygon { pts, color });
        }
    }

    fn image(&mut self, r: &Resolved, part: &str, t: &Affine) {
        let ox = r.num("ImgOffsetX", Axis::X, &[]).unwrap_or(0.0);
        let oy = r.num("ImgOffsetY", Axis::Y, &[]).unwrap_or(0.0);
        let iw = r.num("ImgWidth", Axis::X, &[]).unwrap_or(r.width);
        let ih = r.num("ImgHeight", Axis::Y, &[]).unwrap_or(r.height);
        let corner = |x: f64, y: f64| {
            let p = t.apply([x, y]);
            [p[0] as f32, p[1] as f32]
        };
        let quad = [corner(ox, oy + ih), corner(ox + iw, oy + ih), corner(ox + iw, oy), corner(ox, oy)];
        let bbox = bbox_of(&quad, 0.0);
        let ext = part.rsplit('.').next().unwrap_or_default().to_ascii_lowercase();
        if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "bmp" | "gif") {
            self.push(bbox, ItemKind::Image { part: part.to_string(), quad });
        } else {
            self.warnings.insert(format!("Embedded .{ext} images are not supported"));
            self.push(bbox, ItemKind::Placeholder { quad, label: ext.to_ascii_uppercase() });
        }
    }

    fn text(&mut self, r: &Resolved, t: &Affine) {
        let Some(runs) = r.local.text.as_ref().or_else(|| r.master.and_then(|m| m.text.as_ref())) else {
            return;
        };
        if r.styled_num("HideText", StyleKind::Text).unwrap_or(0.0) != 0.0 {
            return;
        }
        let mut styled: Vec<StyledRun> = runs
            .iter()
            .map(|run| {
                let cell = |name: &str| r.text_row_cell(false, run.cp, name);
                let num = |name: &str| cell(name).and_then(Cell::num);
                let style = num("Style").unwrap_or(0.0) as u32;
                let color = cell("Color").and_then(parse_color).unwrap_or(Color32::BLACK);
                StyledRun {
                    text: run.text.replace("\r\n", "\n").replace(['\r', '\u{2028}', '\u{2029}'], "\n"),
                    size: num("Size").filter(|s| *s > 0.0).unwrap_or(DEFAULT_FONT_SIZE) as f32,
                    color: with_transparency(color, num("ColorTrans").unwrap_or(0.0)),
                    bold: style & 1 != 0,
                    italic: style & 2 != 0,
                    underline: style & 4 != 0,
                    strike: num("Strikethru").unwrap_or(0.0) != 0.0,
                }
            })
            .collect();
        // Visio usually terminates text with a paragraph break.
        while let Some(last) = styled.last_mut() {
            let trimmed = last.text.trim_end_matches('\n').len();
            last.text.truncate(trimmed);
            if last.text.is_empty() {
                styled.pop();
            } else {
                break;
            }
        }
        if styled.iter().all(|s| s.text.trim().is_empty()) {
            return;
        }

        let (w, h) = (r.width, r.height);
        let tw = r.num("TxtWidth", Axis::X, &[]).unwrap_or(w);
        let th = r.num("TxtHeight", Axis::Y, &[]).unwrap_or(h);
        let extra = [("TxtWidth", tw), ("TxtHeight", th)];
        let tpx = r.num("TxtPinX", Axis::X, &extra).unwrap_or(w * 0.5);
        let tpy = r.num("TxtPinY", Axis::Y, &extra).unwrap_or(h * 0.5);
        let tlx = r.num("TxtLocPinX", Axis::X, &extra).unwrap_or(tw * 0.5);
        let tly = r.num("TxtLocPinY", Axis::Y, &extra).unwrap_or(th * 0.5);
        let tangle = r.num("TxtAngle", Axis::None, &[]).unwrap_or(0.0);

        let text_t = t
            .then_after(&Affine::translate(tpx, tpy))
            .then_after(&Affine::rotate(tangle))
            .then_after(&Affine::translate(-tlx, -tly));
        let center = text_t.apply([tw * 0.5, th * 0.5]);
        let x_axis = text_t.apply_vec([1.0, 0.0]);
        let y_axis = text_t.apply_vec([0.0, 1.0]);
        let width = (tw * x_axis[0].hypot(x_axis[1])).abs() as f32;
        let height = (th * y_axis[0].hypot(y_axis[1])).abs() as f32;
        // Orientation follows the box's "up" direction, so horizontally flipped
        // shapes keep readable text, as in Visio.
        let angle = geometry::angle_of([y_axis[1], -y_axis[0]]) as f32;

        let margin = |name: &str| r.styled_num(name, StyleKind::Text).unwrap_or(DEFAULT_MARGIN).max(0.0) as f32;
        let margins = [margin("LeftMargin"), margin("RightMargin"), margin("TopMargin"), margin("BottomMargin")];
        let first_pp = runs.first().map_or(0, |r| r.pp);
        let halign = r.text_row_cell(true, first_pp, "HorzAlign").and_then(Cell::num).unwrap_or(1.0) as u8;
        let valign = r.styled_num("VerticalAlign", StyleKind::Text).unwrap_or(1.0) as u8;

        let max_size = styled.iter().map(|s| s.size).fold(0.0, f32::max);
        let c = [center[0] as f32, center[1] as f32];
        let reach = width.hypot(height) * 0.5 + max_size * 4.0;
        let bbox = [c[0] - reach, c[1] - reach, c[0] + reach, c[1] + reach];
        self.push(
            bbox,
            ItemKind::Text(TextItem {
                center: c,
                width,
                height,
                margins,
                angle,
                // 3 and 4 are justified variants; lay those out left-aligned.
                halign: if halign <= 2 { halign } else { 0 },
                valign: valign.min(2),
                plain: styled.iter().map(|r| r.text.as_str()).collect(),
                runs: styled,
            }),
        );
    }

    fn push(&mut self, bbox: [f32; 4], kind: ItemKind) {
        self.items.push(Item { bbox, kind });
    }
}

/// A point from a POLYLINE/NURBS formula: type 0 is relative to the shape size.
fn fit_point(p: P, x_type: f64, y_type: f64, w: f64, h: f64, from_master: bool, ratio: [f64; 2]) -> P {
    let x = if x_type == 0.0 { p[0] * w } else if from_master { p[0] * ratio[0] } else { p[0] };
    let y = if y_type == 0.0 { p[1] * h } else if from_master { p[1] * ratio[1] } else { p[1] };
    [x, y]
}

fn bbox_of(pts: &[[f32; 2]], pad: f32) -> [f32; 4] {
    let mut b = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
    for p in pts {
        b[0] = b[0].min(p[0]);
        b[1] = b[1].min(p[1]);
        b[2] = b[2].max(p[0]);
        b[3] = b[3].max(p[1]);
    }
    [b[0] - pad, b[1] - pad, b[2] + pad, b[3] + pad]
}
