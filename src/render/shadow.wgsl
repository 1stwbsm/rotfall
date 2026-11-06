struct S { vp: mat4x4<f32> };
@group(0) @binding(0) var<uniform> s: S;
@vertex
fn vs(@location(0) pos: vec3<f32>, @location(3) m0: vec4<f32>, @location(4) m1: vec4<f32>, @location(5) m2: vec4<f32>) -> @builtin(position) vec4<f32> {
    let p = vec4<f32>(pos, 1.0);
    return s.vp * vec4<f32>(dot(m0, p), dot(m1, p), dot(m2, p), 1.0);
}
