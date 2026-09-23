fn hard_light_component(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        return cb * (2.0 * cs);
    }
    return cb + (2.0 * cs - 1.0) - cb * (2.0 * cs - 1.0);
}

fn color_dodge_component(cb: f32, cs: f32) -> f32 {
    if cb <= 0.0 { return 0.0; }
    if cs >= 1.0 { return 1.0; }
    return min(1.0, cb / (1.0 - cs));
}

fn color_burn_component(cb: f32, cs: f32) -> f32 {
    if cb >= 1.0 { return 1.0; }
    if cs <= 0.0 { return 0.0; }
    return 1.0 - min(1.0, (1.0 - cb) / cs);
}

fn soft_light_component(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        return cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb);
    }
    var d = sqrt(cb);
    if cb <= 0.25 {
        d = ((16.0 * cb - 12.0) * cb + 4.0) * cb;
    }
    return cb + (2.0 * cs - 1.0) * (d - cb);
}

fn lum(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.3, 0.59, 0.11));
}

fn sat(c: vec3<f32>) -> f32 {
    return max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b));
}

fn clip_color(c: vec3<f32>) -> vec3<f32> {
    let l = lum(c);
    let n = min(c.r, min(c.g, c.b));
    let x = max(c.r, max(c.g, c.b));
    var result = c;
    if n < 0.0 {
        result = vec3<f32>(l) + (result - vec3<f32>(l)) * l / (l - n);
    }
    if x > 1.0 {
        result = vec3<f32>(l) + (result - vec3<f32>(l)) * (1.0 - l) / (x - l);
    }
    return result;
}

fn set_lum(c: vec3<f32>, l: f32) -> vec3<f32> {
    return clip_color(c + vec3<f32>(l - lum(c)));
}

fn set_sat(c: vec3<f32>, s: f32) -> vec3<f32> {
    let cmin = min(c.r, min(c.g, c.b));
    let cmax = max(c.r, max(c.g, c.b));
    if cmax <= cmin {
        return vec3<f32>(0.0);
    }
    return (c - vec3<f32>(cmin)) * (s / (cmax - cmin));
}
