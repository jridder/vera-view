//! Reading and laying out Visio Open XML drawings (.vsdx) for display.
//!
//! - [`package`]: the zip/OPC container
//! - [`model`]: pages, masters, style sheets and shapes as stored in the XML
//! - [`scene`]: resolves inheritance and geometry into drawable items

pub mod formula;
pub mod geometry;
pub mod model;
pub mod package;
pub mod scene;
