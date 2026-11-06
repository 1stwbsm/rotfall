// Paints blood, scorch marks and acid permanently into the world-space ground layer.
struct DIn {
    @builtin(vertex_index) vi: u32,
    @location(0) center_size_rot: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) params: vec4<f32>, // seed, kind, map half extent, -
};
struct DOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) seed: f32,
    @location(3) kind: f32,
};
@group(0) @binding(0) var noise_tex: texture_2d<f32>;
@group(0) @binding(1) var wrap: sampler;

@vertex
fn vs(i: DIn) -> DOut {
    var q = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0));
    let c = q[i.vi];
    let cs = cos(i.center_size_rot.w);
    let sn = sin(i.center_size_rot.w);
    let r = vec2<f32>(c.x * cs - c.y * sn, c.x * sn + c.y * cs) * i.center_size_rot.z;
    let w = (i.center_size_rot.xy + r) / i.params.z;
    var o: DOut;
    o.clip = vec4<f32>(w.x, -w.y, 0.0, 1.0);
    o.uv = c;
    o.color = i.color;
    o.seed = i.params.x;
    o.kind = i.params.y;
    return o;
}

@fragment
fn fs(i: DOut) -> @location(0) vec4<f32> {
    let r = length(i.uv);
    let n = textureSampleLevel(noise_tex, wrap, i.uv * 0.21 + vec2<f32>(i.seed, i.seed * 1.7), 0.0).g;
    let n2 = textureSampleLevel(noise_tex, wrap, i.uv * 0.67 + vec2<f32>(i.seed * 0.3, i.seed), 0.0).r;
    let m = n * 0.65 + n2 * 0.35;
    var a: f32;
    if (i.kind < 0.5) {
        // Blood: blobby edge with satellite drops.
        a = smoothstep(0.02, 0.10, (1.0 - r) - (m - 0.32) * 1.05);
    } else if (i.kind < 1.5) {
        // Scorch: soft and streaky.
        a = clamp((1.0 - r) * 1.7 - m * 0.55, 0.0, 1.0);
    } else {
        // Acid: thick pool with a defined rim.
        a = smoothstep(0.0, 0.2, (1.0 - r) - (m - 0.4) * 0.7);
    }
    return vec4<f32>(i.color.rgb, a * i.color.a);
}

// A very faint pass that slowly dries the ground so old gore does not saturate forever.
@vertex
fn vs_fade(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let x = f32((vi << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(vi & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}
@fragment
fn fs_fade() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}
