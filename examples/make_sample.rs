//! Writes `samples/sample.vsdx`, a small drawing that exercises the features
//! the viewer supports. Run with `cargo run --example make_sample`.

use std::io::Write;

use zip::write::SimpleFileOptions;

const NS: &str = r#"xmlns="http://schemas.microsoft.com/office/visio/2012/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xml:space="preserve""#;
const REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const REL_BASE: &str = "http://schemas.microsoft.com/visio/2010/relationships";

fn c(n: &str, v: impl std::fmt::Display) -> String {
    format!(r#"<Cell N="{n}" V="{v}"/>"#)
}

fn cf(n: &str, v: impl std::fmt::Display, f: &str) -> String {
    format!(r#"<Cell N="{n}" V="{v}" F="{f}"/>"#)
}

fn row(t: &str, ix: u32, cells: &[String]) -> String {
    format!(r#"<Row T="{t}" IX="{ix}">{}</Row>"#, cells.concat())
}

fn geom(ix: u32, extra: &[String], rows: &[String]) -> String {
    format!(r#"<Section N="Geometry" IX="{ix}">{}{}</Section>"#, extra.concat(), rows.concat())
}

fn polygon(ix: u32, pts: &[(f64, f64)]) -> String {
    let mut rows = vec![row("MoveTo", 1, &[c("X", pts[0].0), c("Y", pts[0].1)])];
    for (i, p) in pts.iter().skip(1).chain(std::iter::once(&pts[0])).enumerate() {
        rows.push(row("LineTo", i as u32 + 2, &[c("X", p.0), c("Y", p.1)]));
    }
    geom(ix, &[], &rows)
}

fn rect_geom(w: f64, h: f64) -> String {
    polygon(0, &[(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)])
}

/// A 2-D shape. `body` holds extra cells and sections.
fn shape(id: u32, attrs: &str, pin: (f64, f64), size: (f64, f64), body: &str, text: &str) -> String {
    let text = if text.is_empty() { String::new() } else { format!("<Text>{text}</Text>") };
    format!(
        r#"<Shape ID="{id}" Type="Shape" LineStyle="3" FillStyle="3" TextStyle="3" {attrs}>{}{}{}{}{}{}{body}{text}</Shape>"#,
        c("PinX", pin.0),
        c("PinY", pin.1),
        c("Width", size.0),
        c("Height", size.1),
        c("LocPinX", size.0 / 2.0),
        c("LocPinY", size.1 / 2.0),
    )
}

fn fill(color: &str) -> String {
    c("FillForegnd", color)
}

fn char_row(cells: &[String]) -> String {
    format!(r#"<Section N="Character"><Row IX="0">{}</Row></Section>"#, cells.concat())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all("samples")?;
    let file = std::fs::File::create("samples/sample.vsdx")?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default();
    let mut put = |name: &str, data: &[u8]| -> Result<(), Box<dyn std::error::Error>> {
        zip.start_file(name, opts)?;
        zip.write_all(data)?;
        Ok(())
    };

    put(
        "[Content_Types].xml",
        br#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Default Extension="png" ContentType="image/png"/>
<Override PartName="/visio/document.xml" ContentType="application/vnd.ms-visio.drawing.main+xml"/>
<Override PartName="/visio/pages/pages.xml" ContentType="application/vnd.ms-visio.pages+xml"/>
<Override PartName="/visio/pages/page1.xml" ContentType="application/vnd.ms-visio.page+xml"/>
<Override PartName="/visio/pages/page2.xml" ContentType="application/vnd.ms-visio.page+xml"/>
<Override PartName="/visio/pages/page3.xml" ContentType="application/vnd.ms-visio.page+xml"/>
<Override PartName="/visio/masters/masters.xml" ContentType="application/vnd.ms-visio.masters+xml"/>
<Override PartName="/visio/masters/master1.xml" ContentType="application/vnd.ms-visio.master+xml"/>
</Types>"#,
    )?;
    put(
        "_rels/.rels",
        format!(r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{REL_NS}"><Relationship Id="rId1" Type="{REL_BASE}/document" Target="visio/document.xml"/></Relationships>"#).as_bytes(),
    )?;

    // Style sheets: 0 = "No Style" with the defaults, 3 = "Normal" deriving from it.
    let defaults = [
        c("LineWeight", 0.01041666666666667),
        c("LineColor", "#000000"),
        c("LinePattern", 1),
        c("FillForegnd", "#ffffff"),
        c("FillPattern", 1),
        c("VerticalAlign", 1),
        c("LeftMargin", 0.05555555555555555),
        c("RightMargin", 0.05555555555555555),
        c("TopMargin", 0.05555555555555555),
        c("BottomMargin", 0.05555555555555555),
    ]
    .concat();
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><VisioDocument {NS}><StyleSheets>
<StyleSheet ID="0" NameU="No Style">{defaults}{}<Section N="Paragraph"><Row IX="0">{}</Row></Section></StyleSheet>
<StyleSheet ID="3" NameU="Normal" LineStyle="0" FillStyle="0" TextStyle="0">{}</StyleSheet>
</StyleSheets></VisioDocument>"#,
        char_row(&[c("Size", 0.1666666666666667), c("Color", "#000000"), c("Style", 0)]),
        c("HorzAlign", 1),
        c("LineColor", "#3b3b3b"),
    );
    put("visio/document.xml", document.as_bytes())?;
    put(
        "visio/_rels/document.xml.rels",
        format!(r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{REL_NS}"><Relationship Id="rId1" Type="{REL_BASE}/pages" Target="pages/pages.xml"/><Relationship Id="rId2" Type="{REL_BASE}/masters" Target="masters/masters.xml"/></Relationships>"#).as_bytes(),
    )?;

    // Master 2: a "Process" box whose geometry is defined with Width/Height formulas.
    let master_geom = geom(
        0,
        &[],
        &[
            row("MoveTo", 1, &[cf("X", 0.1, "Width*0.1"), c("Y", 0)]),
            row("LineTo", 2, &[cf("X", 0.9, "Width*0.9"), c("Y", 0)]),
            row("ArcTo", 3, &[cf("X", 1, "Width"), cf("Y", 0.1, "Height*0.1333"), cf("A", 0.03, "0.03 in")]),
            row("LineTo", 4, &[cf("X", 1, "Width"), cf("Y", 0.65, "Height*0.8667")]),
            row("ArcTo", 5, &[cf("X", 0.9, "Width*0.9"), cf("Y", 0.75, "Height"), cf("A", 0.03, "0.03 in")]),
            row("LineTo", 6, &[cf("X", 0.1, "Width*0.1"), cf("Y", 0.75, "Height")]),
            row("ArcTo", 7, &[c("X", 0), cf("Y", 0.65, "Height*0.8667"), cf("A", 0.03, "0.03 in")]),
            row("LineTo", 8, &[c("X", 0), cf("Y", 0.1, "Height*0.1333")]),
            row("ArcTo", 9, &[cf("X", 0.1, "Width*0.1"), c("Y", 0), cf("A", 0.03, "0.03 in")]),
        ],
    );
    let master = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><MasterContents {NS}><Shapes><Shape ID="5" Type="Shape" LineStyle="3" FillStyle="3" TextStyle="3">{}{}{}{}{}{}{}{}{master_geom}</Shape></Shapes></MasterContents>"#,
        c("Width", 1),
        c("Height", 0.75),
        cf("LocPinX", 0.5, "Width*0.5"),
        cf("LocPinY", 0.375, "Height*0.5"),
        fill("#5b9bd5"),
        c("LineColor", "#2e5d8a"),
        c("LineWeight", 0.0208),
        char_row(&[c("Color", "#ffffff"), c("Style", 1)]),
    );
    put("visio/masters/master1.xml", master.as_bytes())?;
    put(
        "visio/masters/masters.xml",
        format!(r#"<?xml version="1.0" encoding="UTF-8"?><Masters {NS}><Master ID="2" NameU="Process" Name="Process"><Rel r:id="rId1"/></Master></Masters>"#).as_bytes(),
    )?;
    put(
        "visio/masters/_rels/masters.xml.rels",
        format!(r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{REL_NS}"><Relationship Id="rId1" Type="{REL_BASE}/master" Target="master1.xml"/></Relationships>"#).as_bytes(),
    )?;

    // ---- Page 1: feature tour ----
    let mut shapes = String::new();
    shapes += &shape(1, "", (1.5, 6.6), (2.0, 1.0), &(fill("#f4b183") + &rect_geom(2.0, 1.0) + &char_row(&[c("Style", 1)])), "Rectangle");
    let ellipse = geom(0, &[], &[row("Ellipse", 1, &[c("X", 1), c("Y", 0.6), c("A", 2), c("B", 0.6), c("C", 1), c("D", 1.2)])]);
    shapes += &shape(2, "", (4.5, 6.6), (2.0, 1.2), &(fill("#a9d18e") + &ellipse), "Ellipse");
    shapes += &shape(
        3,
        "",
        (7.4, 6.6),
        (1.5, 1.3),
        &(fill("#ffd966") + &c("FillForegndTrans", 0.3) + &c("Angle", 0.5235987755982988) + &polygon(0, &[(0.0, 0.0), (1.5, 0.0), (0.75, 1.3)])),
        "Rotated 30°",
    );
    // Master instance at 2.5 x 1 (master is 1 x 0.75): geometry must be recomputed.
    shapes += &format!(
        r#"<Shape ID="4" Type="Shape" Master="2">{}{}{}{}<Text>From master</Text></Shape>"#,
        c("PinX", 9.6),
        c("PinY", 6.6),
        c("Width", 2.5),
        c("Height", 1.0)
    );
    // Concave star.
    let star: Vec<(f64, f64)> = (0..10)
        .map(|i| {
            let a = std::f64::consts::FRAC_PI_2 + i as f64 * std::f64::consts::PI / 5.0;
            let r = if i % 2 == 0 { 1.0 } else { 0.42 };
            (1.0 + r * a.cos(), 1.0 + r * a.sin())
        })
        .collect();
    shapes += &shape(5, "", (1.5, 3.9), (2.0, 2.0), &(fill("#ff6b6b") + &polygon(0, &star)), "Star");
    // Donut: two geometry sections, the inner one becomes a hole (even-odd).
    let ring = |ix: u32, r: f64| geom(ix, &[], &[row("Ellipse", 1, &[c("X", 1), c("Y", 1), c("A", 1.0 + r), c("B", 1), c("C", 1), c("D", 1.0 + r)])]);
    shapes += &shape(6, "", (4.5, 3.9), (2.0, 2.0), &(fill("#8064a2") + &ring(0, 1.0) + &ring(1, 0.5)), "");
    // Open arcs: ArcTo and EllipticalArcTo, thick line, no fill.
    let arcs = geom(
        0,
        &[c("NoFill", 1)],
        &[
            row("MoveTo", 1, &[c("X", 0), c("Y", 0.2)]),
            row("ArcTo", 2, &[c("X", 1), c("Y", 0.2), c("A", -0.5)]),
            row("EllipticalArcTo", 3, &[c("X", 2), c("Y", 0.2), c("A", 1.5), c("B", 1.6), c("C", 0), c("D", 0.5)]),
        ],
    );
    shapes += &shape(7, "", (7.4, 3.7), (2.0, 1.6), &(c("LineWeight", 0.04) + &c("LineColor", "#c00000") + &arcs), "");
    // NURBS and polyline (relative coordinates).
    let curves = geom(
        0,
        &[c("NoFill", 1)],
        &[
            row("MoveTo", 1, &[c("X", 0), c("Y", 0)]),
            row(
                "NURBSTo",
                2,
                &[c("X", 2), c("Y", 0), c("A", 1), c("B", 1), c("C", 0), c("D", 1), c("E", "NURBS(1, 3, 0, 0, 0.1, 1, 0, 1, 0.9, 1, 0, 1)")],
            ),
            row("MoveTo", 3, &[c("X", 0), c("Y", 0.3)]),
            row("PolylineTo", 4, &[c("X", 2), c("Y", 0.3), cf("A", "", "POLYLINE(0, 0, 0.25, 0.6, 0.5, 0.15, 0.75, 0.6)")]),
        ],
    );
    shapes += &shape(8, "", (10.0, 3.6), (2.0, 1.0), &(c("LineWeight", 0.025) + &c("LineColor", "#2e75b6") + &curves), "");
    // Dashed 1-D connector with an arrowhead, rectangle -> ellipse.
    shapes += &format!(
        r#"<Shape ID="9" Type="Shape" LineStyle="3">{}{}{}{}{}{}{}{}{}{}</Shape>"#,
        c("PinX", 3.0),
        c("PinY", 6.6),
        c("Width", 0.95),
        c("Height", 0),
        c("LocPinX", 0.475),
        c("LocPinY", 0),
        c("EndArrow", 4),
        c("LinePattern", 2),
        c("LineWeight", 0.014),
        geom(0, &[c("NoFill", 1)], &[row("MoveTo", 1, &[c("X", 0), c("Y", 0)]), row("LineTo", 2, &[c("X", 0.95), c("Y", 0)])]),
    );
    // A rotated group with two children.
    let child_a = shape(11, "", (0.7, 0.7), (1.2, 1.0), &(fill("#9dc3e6") + &rect_geom(1.2, 1.0)), "A");
    let child_b = shape(
        12,
        "",
        (2.3, 0.7),
        (1.0, 1.0),
        &(fill("#c5e0b4") + &geom(0, &[], &[row("Ellipse", 1, &[c("X", 0.5), c("Y", 0.5), c("A", 1), c("B", 0.5), c("C", 0.5), c("D", 1)])])),
        "B",
    );
    shapes += &format!(
        r#"<Shape ID="10" Type="Group">{}{}{}{}{}{}{}<Shapes>{child_a}{child_b}</Shapes></Shape>"#,
        c("PinX", 6.0),
        c("PinY", 1.3),
        c("Width", 3.0),
        c("Height", 1.4),
        c("LocPinX", 1.5),
        c("LocPinY", 0.7),
        c("Angle", -0.17453292519943295),
    );
    // Left/top aligned text with mixed formatting and wrapping.
    let text_body = [
        c("LinePattern", 0),
        c("FillPattern", 0),
        c("VerticalAlign", 0),
        rect_geom(3.2, 1.4),
        r##"<Section N="Character"><Row IX="0"><Cell N="Size" V="0.1389"/></Row><Row IX="1"><Cell N="Size" V="0.1389"/><Cell N="Color" V="#c00000"/><Cell N="Style" V="2"/></Row><Row IX="2"><Cell N="Size" V="0.2083"/><Cell N="Style" V="5"/></Row></Section>"##.to_string(),
        r#"<Section N="Paragraph"><Row IX="0"><Cell N="HorzAlign" V="0"/></Row></Section>"#.to_string(),
    ]
    .concat();
    shapes += &shape(
        13,
        "",
        (1.9, 1.3),
        (3.2, 1.4),
        &text_body,
        r#"<cp IX="2"/>Text blocks
<cp IX="0"/>Left aligned, top anchored, and long enough to wrap onto more lines. <cp IX="1"/>Italic red run.
"#,
    );
    // Embedded bitmap.
    shapes += &format!(
        r#"<Shape ID="14" Type="Foreign" LineStyle="3">{}{}{}{}{}{}{}<ForeignData ForeignType="Bitmap"><Rel r:id="rId1"/></ForeignData></Shape>"#,
        c("PinX", 9.8),
        c("PinY", 1.3),
        c("Width", 1.4),
        c("Height", 1.4),
        c("LocPinX", 0.7),
        c("LocPinY", 0.7),
        geom(0, &[c("NoFill", 1)], &[row("MoveTo", 1, &[c("X", 0), c("Y", 0)]), row("LineTo", 2, &[c("X", 1.4), c("Y", 0)]), row("LineTo", 3, &[c("X", 1.4), c("Y", 1.4)]), row("LineTo", 4, &[c("X", 0), c("Y", 1.4)]), row("LineTo", 5, &[c("X", 0), c("Y", 0)])]),
    );
    let page1 = format!(r#"<?xml version="1.0" encoding="UTF-8"?><PageContents {NS}><Shapes>{shapes}</Shapes></PageContents>"#);
    put("visio/pages/page1.xml", page1.as_bytes())?;
    put(
        "visio/pages/_rels/page1.xml.rels",
        format!(r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{REL_NS}"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/></Relationships>"#).as_bytes(),
    )?;

    let mut png = Vec::new();
    let img = image::RgbaImage::from_fn(64, 64, |x, y| {
        let checker = ((x / 8) + (y / 8)) % 2 == 0;
        if checker { image::Rgba([(x * 4) as u8, 120, (y * 4) as u8, 255]) } else { image::Rgba([255, 255, 255, 255]) }
    });
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;
    put("visio/media/image1.png", &png)?;

    // ---- Page 2 (foreground) uses page 3 as its background ----
    let page2 = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><PageContents {NS}><Shapes>{}</Shapes></PageContents>"#,
        shape(1, "", (5.5, 4.25), (4.0, 1.5), &(fill("#ddebf7") + &rect_geom(4.0, 1.5) + &char_row(&[c("Size", 0.25)])), "Foreground page")
    );
    put("visio/pages/page2.xml", page2.as_bytes())?;
    let mut bg = shape(1, "", (5.5, 4.25), (10.5, 8.0), &(c("FillPattern", 0) + &c("LineWeight", 0.03) + &rect_geom(10.5, 8.0)), "");
    bg += &shape(2, "", (5.5, 7.9), (6.0, 0.5), &(c("LinePattern", 0) + &c("FillPattern", 0) + &rect_geom(6.0, 0.5) + &char_row(&[c("Size", 0.2), c("Color", "#7f7f7f")])), "Background page title");
    let page3 = format!(r#"<?xml version="1.0" encoding="UTF-8"?><PageContents {NS}><Shapes>{bg}</Shapes></PageContents>"#);
    put("visio/pages/page3.xml", page3.as_bytes())?;

    let sheet = [c("PageWidth", 11), c("PageHeight", 8.5)].concat();
    let pages = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Pages {NS}>
<Page ID="0" NameU="Feature tour" Name="Feature tour"><PageSheet>{sheet}</PageSheet><Rel r:id="rId1"/></Page>
<Page ID="4" NameU="Foreground" Name="Foreground" BackPage="5"><PageSheet>{sheet}</PageSheet><Rel r:id="rId2"/></Page>
<Page ID="5" NameU="Background" Name="Background" Background="1"><PageSheet>{sheet}</PageSheet><Rel r:id="rId3"/></Page>
</Pages>"#
    );
    put("visio/pages/pages.xml", pages.as_bytes())?;
    put(
        "visio/pages/_rels/pages.xml.rels",
        format!(r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{REL_NS}"><Relationship Id="rId1" Type="{REL_BASE}/page" Target="page1.xml"/><Relationship Id="rId2" Type="{REL_BASE}/page" Target="page2.xml"/><Relationship Id="rId3" Type="{REL_BASE}/page" Target="page3.xml"/></Relationships>"#).as_bytes(),
    )?;

    zip.finish()?;
    println!("Wrote samples/sample.vsdx");
    Ok(())
}
