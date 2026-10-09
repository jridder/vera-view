//! Draws the Vera View icon and writes `assets/vera-view.ico` (all standard
//! Windows sizes) and `assets/icon-256.png` (used for the window icon).
//! Run with `cargo run --example make_icon`.

use std::io::Cursor;

/// Icon artwork, defined on a 256 × 256 canvas. Returns straight RGBA in 0..=1.
fn sample(x: f32, y: f32) -> [f32; 4] {
    if !rounded_rect(x, y, [8.0, 8.0, 248.0, 248.0], 52.0) {
        return [0.0; 4];
    }
    let t = y / 256.0;
    let mut c = mix(rgb(0x3182CE), rgb(0x1E4E8C), t);
    // Elbow connector from the top box down and across to the bottom box.
    let connector = ((x - 92.0).abs() <= 8.0 && (110.0..=180.0).contains(&y))
        || ((y - 172.0).abs() <= 8.0 && (84.0..=120.0).contains(&x));
    if connector {
        c = rgb(0xE2E8F0);
    }
    if rounded_rect(x, y, [40.0, 48.0, 144.0, 116.0], 14.0) {
        c = rgb(0xFFFFFF);
    }
    if rounded_rect(x, y, [112.0, 140.0, 216.0, 208.0], 14.0) {
        c = rgb(0xF6AD55);
    }
    [c[0], c[1], c[2], 1.0]
}

fn rounded_rect(x: f32, y: f32, [x0, y0, x1, y1]: [f32; 4], r: f32) -> bool {
    if x < x0 || x > x1 || y < y0 || y > y1 {
        return false;
    }
    let cx = x.clamp(x0 + r, x1 - r);
    let cy = y.clamp(y0 + r, y1 - r);
    (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}

fn rgb(n: u32) -> [f32; 3] {
    [(n >> 16 & 0xFF) as f32 / 255.0, (n >> 8 & 0xFF) as f32 / 255.0, (n & 0xFF) as f32 / 255.0]
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Render at `size` pixels with 8 × 8 supersampling for smooth edges.
fn render(size: u32) -> image::RgbaImage {
    const SS: u32 = 8;
    let scale = 256.0 / size as f32;
    image::RgbaImage::from_fn(size, size, |px, py| {
        let mut acc = [0.0f32; 4];
        for sy in 0..SS {
            for sx in 0..SS {
                let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) * scale;
                let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) * scale;
                let [r, g, b, a] = sample(x, y);
                // Accumulate premultiplied colour.
                acc[0] += r * a;
                acc[1] += g * a;
                acc[2] += b * a;
                acc[3] += a;
            }
        }
        let n = (SS * SS) as f32;
        let a = acc[3] / n;
        let un = |c: f32| if a > 0.0 { (c / n / a * 255.0).round() as u8 } else { 0 };
        image::Rgba([un(acc[0]), un(acc[1]), un(acc[2]), (a * 255.0).round() as u8])
    })
}

fn png_bytes(img: &image::RgbaImage) -> Vec<u8> {
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).expect("PNG encoding");
    out
}

fn main() -> std::io::Result<()> {
    std::fs::create_dir_all("assets")?;
    let sizes = [16u32, 20, 24, 32, 40, 48, 64, 128, 256];
    let images: Vec<Vec<u8>> = sizes.iter().map(|&s| png_bytes(&render(s))).collect();

    // ICO container with PNG-compressed entries (supported since Windows Vista).
    let mut ico = Vec::new();
    ico.extend_from_slice(&[0, 0, 1, 0]);
    ico.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&size, data) in sizes.iter().zip(&images) {
        let dim = if size >= 256 { 0 } else { size as u8 };
        ico.extend_from_slice(&[dim, dim, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes()); // colour planes
        ico.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        ico.extend_from_slice(&(data.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for data in &images {
        ico.extend_from_slice(data);
    }
    std::fs::write("assets/vera-view.ico", ico)?;
    std::fs::write("assets/icon-256.png", &images[sizes.len() - 1])?;

    // Linux desktops (hicolor theme).
    let png_512 = png_bytes(&render(512));
    std::fs::write("assets/icon-512.png", &png_512)?;

    // macOS .icns: PNG-compressed entries, each tagged with its size and pixel density.
    let entries: [(&[u8; 4], u32); 10] = [
        (b"icp4", 16),
        (b"icp5", 32),
        (b"icp6", 64),
        (b"ic07", 128),
        (b"ic08", 256),
        (b"ic09", 512),
        (b"ic10", 1024), // 512 @2x
        (b"ic11", 32),   // 16 @2x
        (b"ic12", 64),   // 32 @2x
        (b"ic13", 256),  // 128 @2x
    ];
    let mut body = Vec::new();
    for (kind, size) in entries {
        let data = png_bytes(&render(size));
        body.extend_from_slice(kind);
        body.extend_from_slice(&(data.len() as u32 + 8).to_be_bytes());
        body.extend_from_slice(&data);
    }
    let mut icns = b"icns".to_vec();
    icns.extend_from_slice(&(body.len() as u32 + 8).to_be_bytes());
    icns.extend_from_slice(&body);
    std::fs::write("assets/vera-view.icns", icns)?;

    println!("Wrote assets/vera-view.ico, vera-view.icns, icon-256.png and icon-512.png");
    Ok(())
}
