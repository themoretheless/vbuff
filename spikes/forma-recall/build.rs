use std::{env, path::PathBuf, process::Command};

// Generates the typed `Recall` form from `ui/Recall.ui` with Forma's Node-based
// generator. `FORMA_ROOT` overrides the default sibling checkout.
fn main() {
    let project = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let forma = env::var_os("FORMA_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| project.join("../../../forma"));
    for path in [
        project.join("ui"),
        project.join("components"),
        forma.join("src"),
        forma.join("scripts/generate-form.mjs"),
    ] {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    println!("cargo:rerun-if-env-changed=FORMA_NODE");
    println!("cargo:rerun-if-env-changed=FORMA_ROOT");
    let status = Command::new(env::var_os("FORMA_NODE").unwrap_or_else(|| "node".into()))
        .arg(forma.join("scripts/generate-form.mjs"))
        .arg("--project")
        .arg(&project)
        .args(["--entry", "ui/Recall.ui", "--output"])
        .arg(PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("recall.rs"))
        .status()
        .expect("Node.js is required to generate the form");
    assert!(status.success(), "Forma form generation failed");
}
