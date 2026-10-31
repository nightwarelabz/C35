use c35::scene::height_at;
fn main() {
    // ASCII map: rows z from -260..260 (top = -260 far/north), cols x -260..260.
    // Char encodes height: ' ' deep water, '.' shore, ':' low, 'o' mid, 'O' high, '#' peak
    for j in 0..27 {
        let z = -260.0 + j as f32 * 20.0;
        let mut line = String::new();
        for i in 0..53 {
            let x = -260.0 + i as f32 * 10.0;
            let h = height_at(x, z);
            line.push(match h {
                h if h < -3.0 => ' ',
                h if h < 0.5 => '.',
                h if h < 6.0 => ':',
                h if h < 16.0 => 'o',
                h if h < 30.0 => 'O',
                _ => '#',
            });
        }
        println!("z={:>5.0} {}", z, line);
    }
    // Ray from a candidate eye (SE, over the bay) toward the massif:
    for (eye, tgt) in [
        ((60.0f32, 76.0f32), (-36.0f32, -58.0f32)),
        ((86.0, 96.0), (-36.0, -58.0)),
        ((110.0, 60.0), (-40.0, -50.0)),
    ] {
        let mut maxh = f32::MIN;
        for s in 0..=40 {
            let t = s as f32 / 40.0;
            let x = eye.0 + (tgt.0 - eye.0) * t;
            let z = eye.1 + (tgt.1 - eye.1) * t;
            maxh = maxh.max(height_at(x, z));
        }
        println!(
            "eye {:?} terrain h at eye = {:.1}, max h along ray = {:.1}",
            eye,
            height_at(eye.0, eye.1),
            maxh
        );
    }
}
