//! Every WGSL source the renderer hands to wgpu, parsed and validated
//! with naga — the same front end wgpu uses at `create_shader_module`.
//!
//! This test exists because the wetness seam (PR #43) added
//! `u.surface.x` to props.wgsl and a `surface` field to the Rust
//! `Uniforms` struct, but never added the field to the WGSL `Uniforms`
//! in common.wgsl. Every headless gate passed — no VM here enumerates
//! a GPU adapter, so nothing ever compiled the shader — and the first
//! real adapter (an M1 Mac, 2026-10-31) rejected it at startup:
//! "invalid field accessor `surface`". Parsing needs no GPU, so this
//! class of break is now caught on every machine, including CI.

use std::path::PathBuf;

fn shader_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/shaders")
}

fn read(name: &str) -> String {
    let path = shader_dir().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Mirror of `lib.rs::shader_src`: common.wgsl concatenated before the
/// body, exactly as the renderer assembles it.
fn assembled(body_file: &str) -> String {
    format!("{}\n{}", read("common.wgsl"), read(body_file))
}

fn check(label: &str, source: &str) {
    let module = naga::front::wgsl::parse_str(source).unwrap_or_else(|e| {
        panic!(
            "shader '{label}' fails WGSL parse:\n{}",
            e.emit_to_string(source)
        )
    });
    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    );
    if let Err(e) = validator.validate(&module) {
        panic!("shader '{label}' fails naga validation: {e:?}");
    }
}

#[test]
fn every_renderer_shader_parses_and_validates() {
    // Assembled through shader_src() in lib.rs (common + body).
    for body in [
        "sky.wgsl",
        "terrain.wgsl",
        "water.wgsl",
        "depth.wgsl",
        "operator.wgsl",
        "props.wgsl",
    ] {
        check(body, &assembled(body));
    }
    // Standalone sources handed to wgpu as-is.
    for solo in [
        "fxaa.wgsl",
        "blit.wgsl",
        "resolve.wgsl",
        "bloom.wgsl",
        "cull.wgsl",
    ] {
        check(solo, &read(solo));
    }
}
