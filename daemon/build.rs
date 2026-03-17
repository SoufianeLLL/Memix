use std::path::Path;

fn main() {
    let model_dir = Path::new("models/all-MiniLM-L6-v2");
    let onnx_path = model_dir.join("onnx/model.onnx");

    if !onnx_path.exists() {
        println!("cargo:warning=Bundled embedding model files not found in daemon/models/all-MiniLM-L6-v2");
        println!("cargo:warning=Run: bash scripts/download_model.sh");
    }

    println!("cargo:rerun-if-changed=models/");
}
