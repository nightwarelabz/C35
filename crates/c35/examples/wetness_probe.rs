//! Wetness probe: the renderer's weather seam, exercised
//! at the API level — the default is the shipped full-wet
//! look (1.0), a host value lands as given, and out-of-range
//! values clamp to the 0..=1 the shader math expects.
//!
//! The visual half of the seam is one host line per frame
//! (`renderer.set_wetness(weather.wetness())` next to the
//! fog bind); this probe is the compile-and-readback proof
//! that the setter exists and moves the number the frame's
//! uniforms carry.
//!
//! Run: cargo run -q -p c35 --example wetness_probe

fn main() {
    let instance = c35::create_instance();
    let mut renderer = c35::pollster::block_on(c35::Renderer::new(
        &instance,
        c35::wgpu::TextureFormat::Rgba8UnormSrgb,
        320,
        180,
        1.0,
    ));

    println!("default wetness: {:.2}", renderer.wetness());
    renderer.set_wetness(0.35);
    println!("after set_wetness(0.35): {:.2}", renderer.wetness());
    renderer.set_wetness(2.5);
    println!(
        "after set_wetness(2.5): {:.2} (clamped)",
        renderer.wetness()
    );
    renderer.set_wetness(-1.0);
    println!(
        "after set_wetness(-1.0): {:.2} (clamped)",
        renderer.wetness()
    );
    println!("Reads: the seam defaults to the shipped look, takes the");
    println!("host's number as given, and refuses numbers off the scale.");
}
