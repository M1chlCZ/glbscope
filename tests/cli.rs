use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn glb(json: &str) -> Vec<u8> {
    let mut json_bytes = json.as_bytes().to_vec();
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }
    let total = 12 + 8 + json_bytes.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&0x4654_6C67u32.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
    out.extend_from_slice(&json_bytes);
    out
}

const TRIANGLE: &str = r#"{
  "asset": {"version": "2.0"},
  "scenes": [{"nodes": [0]}],
  "nodes": [{"mesh": 0}],
  "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "indices": 1}]}],
  "accessors": [{"count": 3}, {"count": 3}]
}"#;

fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("glbscope-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("workspace");
    dir
}

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_glbscope"))
}

#[test]
fn exits_clean_for_valid_input() {
    let dir = workspace("clean");
    let model = dir.join("model.glb");
    fs::write(&model, glb(TRIANGLE)).expect("write model");
    let output = binary().arg(&model).output().expect("run");
    assert_eq!(output.status.code(), Some(0));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn exits_one_on_budget_findings() {
    let dir = workspace("budget");
    let model = dir.join("model.glb");
    let budget = dir.join("budget.toml");
    fs::write(&model, glb(TRIANGLE)).expect("write model");
    fs::write(&budget, "max_triangles = 0\n").expect("write budget");
    let output = binary()
        .args(["--budget", budget.to_str().unwrap()])
        .arg(&model)
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(1));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn exits_two_on_invalid_input() {
    let dir = workspace("invalid");
    let model = dir.join("broken.glb");
    fs::write(&model, b"broken").expect("write model");
    let output = binary().arg(&model).output().expect("run");
    assert_eq!(output.status.code(), Some(2));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn json_output_is_deterministic() {
    let dir = workspace("json");
    let model = dir.join("model.glb");
    fs::write(&model, glb(TRIANGLE)).expect("write model");
    let first = binary().arg("--json").arg(&model).output().expect("run");
    let second = binary().arg("--json").arg(&model).output().expect("run");
    assert_eq!(first.stdout, second.stdout);
    assert!(String::from_utf8_lossy(&first.stdout).contains("\"triangles\": 1"));
    fs::remove_dir_all(&dir).ok();
}
