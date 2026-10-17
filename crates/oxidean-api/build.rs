fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protos = [
        "proto/runner.proto",
        "proto/runner/v1/messages.proto",
        "proto/runner/v1/services.proto",
        "proto/ping/v1/messages.proto",
        "proto/ping/v1/services.proto",
    ];
    for p in protos {
        println!("cargo:rerun-if-changed={p}");
    }
    // Prefer PATH / PROTOC; fall back to user-local protoc cache (no sudo required).
    if std::env::var_os("PROTOC").is_none() {
        let home = std::env::var("HOME").unwrap_or_default();
        for ver in ["29.3", "28.3", "27.1"] {
            let candidate = format!("{home}/.cache/protoc-{ver}/bin/protoc");
            if std::path::Path::new(&candidate).is_file() {
                std::env::set_var("PROTOC", candidate);
                break;
            }
        }
    }
    prost_build::compile_protos(&protos, &["proto/"])?;
    Ok(())
}
