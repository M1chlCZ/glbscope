//! Inspect binary glTF (GLB) files and check them against asset budgets.
//!
//! The crate parses the GLB container by hand, reads the JSON chunk, and
//! reports geometry, texture, and structure statistics. [`check`] compares a
//! [`Report`] with a [`Budget`] and returns one [`Finding`] per violation.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const GLB_MAGIC: u32 = 0x4654_6C67;
const CHUNK_JSON: u32 = 0x4E4F_534A;
const CHUNK_BIN: u32 = 0x004E_4942;

/// Optional limits applied to one model. Every field is optional.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Budget {
    /// Maximum file size in bytes.
    pub max_bytes: Option<u64>,
    /// Maximum number of triangles.
    pub max_triangles: Option<u64>,
    /// Maximum number of vertices.
    pub max_vertices: Option<u64>,
    /// Maximum number of nodes.
    pub max_nodes: Option<u64>,
    /// Maximum number of materials.
    pub max_materials: Option<u64>,
    /// Maximum number of textures.
    pub max_textures: Option<u64>,
    /// Maximum number of embedded image bytes.
    pub max_texture_bytes: Option<u64>,
    /// When not empty, only these extensions may appear in the file.
    #[serde(default)]
    pub allowed_extensions: Vec<String>,
    /// Extensions that must appear in the file.
    #[serde(default)]
    pub required_extensions: Vec<String>,
}

impl Budget {
    /// Parse a budget document from TOML text.
    pub fn from_toml(text: &str) -> Result<Self> {
        toml::from_str(text).context("invalid budget file")
    }
}

/// One budget violation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    /// Budget rule or extension name that failed.
    pub rule: String,
    /// Measured value.
    pub actual: u64,
    /// Allowed value.
    pub limit: u64,
}

/// Axis aligned bounds collected from the POSITION accessors.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Bounds {
    /// Minimum corner.
    pub min: [f64; 3],
    /// Maximum corner.
    pub max: [f64; 3],
}

/// Statistics of one GLB file.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    /// Total file size in bytes.
    pub file_bytes: u64,
    /// Size of the JSON chunk in bytes.
    pub json_bytes: u64,
    /// Size of the BIN chunk in bytes.
    pub bin_bytes: u64,
    /// Generator string from the asset block.
    pub generator: Option<String>,
    /// glTF asset version.
    pub asset_version: String,
    /// Extensions declared in `extensionsUsed`.
    pub extensions_used: Vec<String>,
    /// Extensions declared in `extensionsRequired`.
    pub extensions_required: Vec<String>,
    /// Scene count.
    pub scenes: u64,
    /// Node count.
    pub nodes: u64,
    /// Mesh count.
    pub meshes: u64,
    /// Primitive count over all meshes.
    pub primitives: u64,
    /// Accessor count.
    pub accessors: u64,
    /// Material count.
    pub materials: u64,
    /// Texture count.
    pub textures: u64,
    /// Image count.
    pub images: u64,
    /// Animation count.
    pub animations: u64,
    /// Vertex count over all primitives.
    pub vertices: u64,
    /// Triangle count over all primitives.
    pub triangles: u64,
    /// Embedded image bytes.
    pub image_bytes: u64,
    /// Deepest node hierarchy level, zero for an empty scene.
    pub max_depth: u64,
    /// Combined bounds, absent when no POSITION accessor has min and max.
    pub bounds: Option<Bounds>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Gltf {
    #[serde(default)]
    asset: Asset,
    #[serde(default)]
    scenes: Vec<Scene>,
    #[serde(default)]
    nodes: Vec<Node>,
    #[serde(default)]
    meshes: Vec<Mesh>,
    #[serde(default)]
    accessors: Vec<Accessor>,
    #[serde(default)]
    materials: Vec<serde_json::Value>,
    #[serde(default)]
    textures: Vec<serde_json::Value>,
    #[serde(default)]
    images: Vec<Image>,
    #[serde(default)]
    buffer_views: Vec<BufferView>,
    #[serde(default)]
    animations: Vec<serde_json::Value>,
    #[serde(default)]
    extensions_used: Vec<String>,
    #[serde(default)]
    extensions_required: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Asset {
    #[serde(default)]
    version: String,
    #[serde(default)]
    generator: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Scene {
    #[serde(default)]
    nodes: Vec<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct Node {
    #[serde(default)]
    children: Vec<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct Mesh {
    #[serde(default)]
    primitives: Vec<Primitive>,
}

#[derive(Debug, Default, Deserialize)]
struct Primitive {
    #[serde(default)]
    attributes: HashMap<String, usize>,
    #[serde(default)]
    indices: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct Accessor {
    #[serde(default)]
    count: u64,
    #[serde(default)]
    min: Option<Vec<f64>>,
    #[serde(default)]
    max: Option<Vec<f64>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Image {
    #[serde(default)]
    buffer_view: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BufferView {
    #[serde(default)]
    byte_length: u64,
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32> {
    let slice = bytes.get(offset..offset + 4).context("truncated GLB")?;
    let array: [u8; 4] = slice.try_into().context("truncated GLB")?;
    Ok(u32::from_le_bytes(array))
}

/// Parse one GLB document and return its statistics.
pub fn inspect(bytes: &[u8]) -> Result<Report> {
    if bytes.len() < 12 {
        bail!("file is shorter than the GLB header");
    }
    if u32_at(bytes, 0)? != GLB_MAGIC {
        bail!("file does not start with the GLB magic");
    }
    if u32_at(bytes, 4)? != 2 {
        bail!("unsupported GLB version");
    }
    let declared = u32_at(bytes, 8)? as usize;
    if declared != bytes.len() {
        bail!("declared length does not match the file size");
    }

    let mut offset = 12usize;
    let mut json: Option<&[u8]> = None;
    let mut bin_seen = false;
    let mut bin_bytes = 0u64;
    while offset < bytes.len() {
        if bytes.len() - offset < 8 {
            bail!("truncated chunk header");
        }
        let length = u32_at(bytes, offset)? as usize;
        let kind = u32_at(bytes, offset + 4)?;
        let start = offset + 8;
        let end = start.checked_add(length).context("chunk length overflow")?;
        if end > bytes.len() {
            bail!("chunk extends past the end of the file");
        }
        match kind {
            CHUNK_JSON => {
                if json.is_some() {
                    bail!("duplicate JSON chunk");
                }
                json = Some(&bytes[start..end]);
            }
            CHUNK_BIN => {
                if bin_seen {
                    bail!("duplicate BIN chunk");
                }
                bin_seen = true;
                bin_bytes = length as u64;
            }
            other => bail!("unknown chunk type {other:#010x}"),
        }
        offset = end;
    }

    let json = json.context("missing JSON chunk")?;
    let gltf: Gltf = serde_json::from_slice(json).context("invalid glTF JSON")?;

    let mut primitives = 0u64;
    let mut vertices = 0u64;
    let mut triangles = 0u64;
    for mesh in &gltf.meshes {
        for primitive in &mesh.primitives {
            primitives += 1;
            if let Some(index) = primitive.attributes.get("POSITION") {
                if let Some(accessor) = gltf.accessors.get(*index) {
                    vertices += accessor.count;
                    if primitive.indices.is_none() {
                        triangles += accessor.count / 3;
                    }
                }
            }
            if let Some(index) = primitive.indices {
                if let Some(accessor) = gltf.accessors.get(index) {
                    triangles += accessor.count / 3;
                }
            }
        }
    }

    let mut image_bytes = 0u64;
    for image in &gltf.images {
        if let Some(view) = image.buffer_view {
            if let Some(buffer_view) = gltf.buffer_views.get(view) {
                image_bytes += buffer_view.byte_length;
            }
        }
    }

    let mut memo = vec![None; gltf.nodes.len()];
    let mut visiting = vec![false; gltf.nodes.len()];
    let roots: Vec<usize> = if gltf.scenes.is_empty() {
        (0..gltf.nodes.len()).collect()
    } else {
        gltf.scenes
            .iter()
            .flat_map(|scene| scene.nodes.iter().copied())
            .collect()
    };
    let mut max_depth = 0u64;
    for root in roots {
        max_depth = max_depth.max(depth_from(root, &gltf.nodes, &mut visiting, &mut memo));
    }

    let mut bounds: Option<Bounds> = None;
    for mesh in &gltf.meshes {
        for primitive in &mesh.primitives {
            let Some(index) = primitive.attributes.get("POSITION") else {
                continue;
            };
            let Some(accessor) = gltf.accessors.get(*index) else {
                continue;
            };
            let (Some(min), Some(max)) = (accessor.min.as_ref(), accessor.max.as_ref()) else {
                continue;
            };
            if min.len() < 3 || max.len() < 3 {
                continue;
            }
            let min = [min[0], min[1], min[2]];
            let max = [max[0], max[1], max[2]];
            bounds = Some(match bounds {
                None => Bounds { min, max },
                Some(current) => Bounds {
                    min: [
                        current.min[0].min(min[0]),
                        current.min[1].min(min[1]),
                        current.min[2].min(min[2]),
                    ],
                    max: [
                        current.max[0].max(max[0]),
                        current.max[1].max(max[1]),
                        current.max[2].max(max[2]),
                    ],
                },
            });
        }
    }

    Ok(Report {
        file_bytes: bytes.len() as u64,
        json_bytes: json.len() as u64,
        bin_bytes,
        generator: gltf.asset.generator.clone(),
        asset_version: gltf.asset.version.clone(),
        extensions_used: gltf.extensions_used.clone(),
        extensions_required: gltf.extensions_required.clone(),
        scenes: gltf.scenes.len() as u64,
        nodes: gltf.nodes.len() as u64,
        meshes: gltf.meshes.len() as u64,
        primitives,
        accessors: gltf.accessors.len() as u64,
        materials: gltf.materials.len() as u64,
        textures: gltf.textures.len() as u64,
        images: gltf.images.len() as u64,
        animations: gltf.animations.len() as u64,
        vertices,
        triangles,
        image_bytes,
        max_depth,
        bounds,
    })
}

fn depth_from(
    index: usize,
    nodes: &[Node],
    visiting: &mut [bool],
    memo: &mut [Option<u64>],
) -> u64 {
    if index >= nodes.len() || visiting[index] {
        return 0;
    }
    if let Some(depth) = memo[index] {
        return depth;
    }
    visiting[index] = true;
    let mut depth = 1u64;
    for child in &nodes[index].children {
        depth = depth.max(1 + depth_from(*child, nodes, visiting, memo));
    }
    visiting[index] = false;
    memo[index] = Some(depth);
    depth
}

/// Compare a report with a budget and return one finding per violation.
pub fn check(report: &Report, budget: &Budget) -> Vec<Finding> {
    let mut findings = Vec::new();
    push_limit(
        &mut findings,
        "max_bytes",
        report.file_bytes,
        budget.max_bytes,
    );
    push_limit(
        &mut findings,
        "max_triangles",
        report.triangles,
        budget.max_triangles,
    );
    push_limit(
        &mut findings,
        "max_vertices",
        report.vertices,
        budget.max_vertices,
    );
    push_limit(&mut findings, "max_nodes", report.nodes, budget.max_nodes);
    push_limit(
        &mut findings,
        "max_materials",
        report.materials,
        budget.max_materials,
    );
    push_limit(
        &mut findings,
        "max_textures",
        report.textures,
        budget.max_textures,
    );
    push_limit(
        &mut findings,
        "max_texture_bytes",
        report.image_bytes,
        budget.max_texture_bytes,
    );
    if !budget.allowed_extensions.is_empty() {
        for extension in &report.extensions_used {
            if !budget.allowed_extensions.contains(extension) {
                findings.push(Finding {
                    rule: format!("allowed_extensions:{extension}"),
                    actual: 1,
                    limit: 0,
                });
            }
        }
    }
    for extension in &budget.required_extensions {
        if !report.extensions_used.contains(extension) {
            findings.push(Finding {
                rule: format!("required_extensions:{extension}"),
                actual: 0,
                limit: 1,
            });
        }
    }
    findings
}

fn push_limit(findings: &mut Vec<Finding>, rule: &str, actual: u64, limit: Option<u64>) {
    if let Some(limit) = limit {
        if actual > limit {
            findings.push(Finding {
                rule: rule.to_string(),
                actual,
                limit,
            });
        }
    }
}
