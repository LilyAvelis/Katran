use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=assets/branding/katran.ico");
    println!("cargo:rerun-if-changed=assets/branding/katran.rc");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows")
        || env::var_os("CARGO_FEATURE_UI").is_none()
    {
        return;
    }
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let numeric = format!("{},0", version.replace('.', ","));
    let icon = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("assets/branding/katran.ico");
    let script = fs::read_to_string("assets/branding/katran.rc")
        .unwrap()
        .replace("@ICON@", &icon.to_string_lossy().replace('\\', "/"))
        .replace("@VERSION@", &version)
        .replace("@NUMERIC_VERSION@", &numeric);
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("katran.rc");
    fs::write(&output, script).unwrap();
    // A Windows UI build must include its identity, rather than silently shipping
    // the stock icon when the resource compiler is absent.
    embed_resource::compile_for(output, ["katran-ui"], embed_resource::NONE)
        .manifest_required()
        .expect("Failed to embed Katran Windows icon/version resources");
}
