// Bloom pyramid and final composite.
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct FsOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };

@vertex
fn vs_full(@builtin(vertex_index) vi: u32) -> FsOut {
    let u = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    var o: FsOut;
    o.pos = vec4<f32>(u.x * 2.0 - 1.0, 1.0 - u.y * 2.0, 0.0, 1.0);
    o.uv = u;
    return o;
}

// First downsample: extracts bright areas with a soft knee.
@fragment
fn fs_prefilter(i: FsOut) -> @location(0) vec4<f32> {
    let t = 1.0 / vec2<f32>(textureDimensions(src));
    var c = textureSample(src, samp, i.uv + t * vec2<f32>(-1.0, -1.0));
    c += textureSample(src, samp, i.uv + t * vec2<f32>(1.0, -1.0));
    c += textureSample(src, samp, i.uv + t * vec2<f32>(-1.0, 1.0));
    c += textureSample(src, samp, i.uv + t * vec2<f32>(1.0, 1.0));
    let col = min(c.rgb * 0.25, vec3<f32>(30.0));
    let l = max(col.r, max(col.g, col.b));
    let k = clamp((l - 0.7) / 0.6, 0.0, 1.0);
    return vec4<f32>(col * k * k, 1.0);
}

@fragment
fn fs_down(i: FsOut) -> @location(0) vec4<f32> {
    let t = 1.0 / vec2<f32>(textureDimensions(src));
    var c = textureSample(src, samp, i.uv) * 4.0;
    c += textureSample(src, samp, i.uv + t * vec2<f32>(-1.0, -1.0));
    c += textureSample(src, samp, i.uv + t * vec2<f32>(1.0, -1.0));
    c += textureSample(src, samp, i.uv + t * vec2<f32>(-1.0, 1.0));
    c += textureSample(src, samp, i.uv + t * vec2<f32>(1.0, 1.0));
    return vec4<f32>(c.rgb / 8.0, 1.0);
}

@fragment
fn fs_up(i: FsOut) -> @location(0) vec4<f32> {
    let t = 1.0 / vec2<f32>(textureDimensions(src));
    var c = textureSample(src, samp, i.uv + t * vec2<f32>(-1.0, 0.0));
    c += textureSample(src, samp, i.uv + t * vec2<f32>(1.0, 0.0));
    c += textureSample(src, samp, i.uv + t * vec2<f32>(0.0, -1.0));
    c += textureSample(src, samp, i.uv + t * vec2<f32>(0.0, 1.0));
    c += textureSample(src, samp, i.uv + t * vec2<f32>(-0.7, -0.7));
    c += textureSample(src, samp, i.uv + t * vec2<f32>(0.7, -0.7));
    c += textureSample(src, samp, i.uv + t * vec2<f32>(-0.7, 0.7));
    c += textureSample(src, samp, i.uv + t * vec2<f32>(0.7, 0.7));
    return vec4<f32>(c.rgb / 8.0, 1.0);
}

struct Post {
    a: vec4<f32>, // exposure, bloom strength, time, vignette
    b: vec4<f32>, // damage flash, chroma, desaturate, manual gamma flag
};
@group(0) @binding(2) var bloom: texture_2d<f32>;
@group(0) @binding(3) var<uniform> post: Post;

fn aces(x: vec3<f32>) -> vec3<f32> {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_composite(i: FsOut) -> @location(0) vec4<f32> {
    let d = i.uv - 0.5;
    let r2 = dot(d, d);
    let ca = post.b.y * r2;
    var hdr = vec3<f32>(
        textureSample(src, samp, i.uv + d * ca).r,
        textureSample(src, samp, i.uv).g,
        textureSample(src, samp, i.uv - d * ca).b);
    let b = textureSample(bloom, samp, i.uv).rgb;
    hdr += b * post.a.y;
    var c = aces(hdr * post.a.x);
    let luma = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    c = mix(c, vec3<f32>(luma), post.b.z);
    // Lightly teal shadows, warm highlights.
    c = mix(c * vec3<f32>(0.93, 1.0, 1.06), c * vec3<f32>(1.04, 1.0, 0.95), clamp(luma * 1.4, 0.0, 1.0));
    c *= 1.0 - clamp(r2 * post.a.w, 0.0, 0.9);
    // Red pressure at the screen edges when hurt.
    let edge = smoothstep(0.08, 0.5, r2);
    c = mix(c, vec3<f32>(0.5, 0.01, 0.01), edge * post.b.x);
    // Film grain.
    let seed = dot(i.pos.xy, vec2<f32>(12.9898, 78.233)) + post.a.z * 37.0;
    let grain = fract(sin(seed) * 43758.5453) - 0.5;
    c += grain * 0.012;
    c = max(c, vec3<f32>(0.0));
    if (post.b.w > 0.5) { c = pow(c, vec3<f32>(1.0 / 2.2)); }
    return vec4<f32>(c, 1.0);
}
