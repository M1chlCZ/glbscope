use glbscope::{Budget, check, inspect};

fn glb(json: &str, bin: &[u8]) -> Vec<u8> {
    let mut json_bytes = json.as_bytes().to_vec();
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }
    let mut bin_bytes = bin.to_vec();
    while bin_bytes.len() % 4 != 0 {
        bin_bytes.push(0);
    }
    let total = 12
        + 8
        + json_bytes.len()
        + if bin_bytes.is_empty() {
            0
        } else {
            8 + bin_bytes.len()
        };
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&0x4654_6C67u32.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
    out.extend_from_slice(&json_bytes);
    if !bin_bytes.is_empty() {
        out.extend_from_slice(&(bin_bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&0x004E_4942u32.to_le_bytes());
        out.extend_from_slice(&bin_bytes);
    }
    out
}

const TRIANGLE: &str = r#"{
  "asset": {"version": "2.0", "generator": "fixture"},
  "scenes": [{"nodes": [0]}],
  "nodes": [{"children": [1]}, {"mesh": 0}],
  "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "indices": 1}]}],
  "accessors": [
    {"count": 3, "min": [0.0, 0.0, 0.0], "max": [1.0, 2.0, 3.0]},
    {"count": 3}
  ],
  "materials": [{"name": "one"}],
  "textures": [{"source": 0}],
  "images": [{"mimeType": "image/png", "bufferView": 0}],
  "bufferViews": [{"byteLength": 48}],
  "buffers": [{"byteLength": 48}],
  "extensionsUsed": ["KHR_materials_unlit"]
}"#;

#[test]
fn reports_triangle_statistics() {
    let report = inspect(&glb(TRIANGLE, &[0u8; 48])).expect("inspect");
    assert_eq!(report.file_bytes as usize, glb(TRIANGLE, &[0u8; 48]).len());
    assert_eq!(report.triangles, 1);
    assert_eq!(report.vertices, 3);
    assert_eq!(report.nodes, 2);
    assert_eq!(report.max_depth, 2);
    assert_eq!(report.primitives, 1);
    assert_eq!(report.materials, 1);
    assert_eq!(report.textures, 1);
    assert_eq!(report.images, 1);
    assert_eq!(report.image_bytes, 48);
    assert_eq!(report.extensions_used, vec!["KHR_materials_unlit"]);
    let bounds = report.bounds.expect("bounds");
    assert_eq!(bounds.min, [0.0, 0.0, 0.0]);
    assert_eq!(bounds.max, [1.0, 2.0, 3.0]);
}

#[test]
fn rejects_malformed_containers() {
    let valid = glb(TRIANGLE, &[]);

    let mut bad_magic = valid.clone();
    bad_magic[0] = 0;
    assert!(inspect(&bad_magic).is_err());

    let mut bad_version = valid.clone();
    bad_version[4] = 3;
    assert!(inspect(&bad_version).is_err());

    let mut bad_length = valid.clone();
    bad_length[8] = bad_length[8].wrapping_add(4);
    assert!(inspect(&bad_length).is_err());

    assert!(inspect(&valid[..valid.len() - 2]).is_err());
    assert!(inspect(&valid[..10]).is_err());
    assert!(inspect(b"not a glb file").is_err());
}

#[test]
fn rejects_missing_and_duplicate_json_chunks() {
    let empty = glb("{}", &[]);
    assert!(inspect(&empty).is_ok());

    let mut duplicate = empty.clone();
    duplicate.extend_from_slice(&(4u32).to_le_bytes());
    duplicate.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
    duplicate.extend_from_slice(b"    ");
    let length = duplicate.len() as u32;
    duplicate[8..12].copy_from_slice(&length.to_le_bytes());
    assert!(inspect(&duplicate).is_err());
}

#[test]
fn rejects_invalid_json() {
    assert!(inspect(&glb("not json", &[])).is_err());
    assert!(inspect(&glb("[1, 2, 3]", &[])).is_err());
}

#[test]
fn applies_budget_rules() {
    let report = inspect(&glb(TRIANGLE, &[0u8; 48])).expect("inspect");
    let budget = Budget {
        max_bytes: Some(1),
        max_triangles: Some(1),
        max_vertices: Some(3),
        max_nodes: Some(1),
        max_materials: Some(0),
        max_textures: Some(0),
        max_texture_bytes: Some(10),
        allowed_extensions: vec!["KHR_draco_mesh_compression".to_string()],
        required_extensions: vec!["KHR_materials_unlit".to_string()],
    };
    let findings = check(&report, &budget);
    let rules: Vec<&str> = findings
        .iter()
        .map(|finding| finding.rule.as_str())
        .collect();
    assert!(rules.contains(&"max_bytes"));
    assert!(rules.contains(&"max_nodes"));
    assert!(rules.contains(&"max_materials"));
    assert!(rules.contains(&"max_textures"));
    assert!(rules.contains(&"max_texture_bytes"));
    assert!(rules.contains(&"allowed_extensions:KHR_materials_unlit"));
    assert!(!rules.contains(&"max_triangles"));
    assert!(!rules.contains(&"max_vertices"));
    assert!(
        !rules
            .iter()
            .any(|rule| rule.starts_with("required_extensions"))
    );

    let empty = check(&report, &Budget::default());
    assert!(empty.is_empty());
}
