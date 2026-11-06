struct Ui { screen: vec4<f32> }; // width, height, manual gamma flag, -
@group(0) @binding(0) var<uniform> u: Ui;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct VOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) color: vec4<f32> };

@vertex
fn vs(@location(0) p: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>) -> VOut {
    var o: VOut;
    o.pos = vec4<f32>(p.x / u.screen.x * 2.0 - 1.0, 1.0 - p.y / u.screen.y * 2.0, 0.0, 1.0);
    o.uv = uv;
    o.color = color;
    return o;
}

@fragment
fn fs(i: VOut) -> @location(0) vec4<f32> {
    let a = textureSample(atlas, samp, i.uv).r * i.color.a;
    var rgb = i.color.rgb;
    // UI colours are authored in sRGB; convert when the target encodes for us.
    if (u.screen.z < 0.5) { rgb = pow(rgb, vec3<f32>(2.2)); }
    return vec4<f32>(rgb, a);
}
