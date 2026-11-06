// Forward scene shading with a CPU-binned world-space light grid.
struct Globals {
    view_proj: mat4x4<f32>,
    sun_vp: mat4x4<f32>,
    flash_vp: mat4x4<f32>,
    cam_pos: vec4<f32>,   // w = time
    sun_dir: vec4<f32>,
    sun_color: vec4<f32>,
    amb_sky: vec4<f32>,
    amb_ground: vec4<f32>,
    fog: vec4<f32>,       // rgb, w = strength
    flash_pos: vec4<f32>, // w = range
    flash_dir: vec4<f32>, // w = cos outer
    flash_color: vec4<f32>, // w = cos inner
    grid: vec4<f32>,      // origin x, origin z, cell size, dimension
    map: vec4<f32>,       // half extent, ground half extent, -, -
};

struct Light { pos_radius: vec4<f32>, color: vec4<f32> };
const GRID_STRIDE: u32 = 17u;

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var<storage, read> lights: array<Light>;
@group(0) @binding(2) var<storage, read> grid: array<u32>;
@group(0) @binding(3) var sun_map: texture_depth_2d;
@group(0) @binding(4) var flash_map: texture_depth_2d;
@group(0) @binding(5) var shadow_sampler: sampler_comparison;
@group(0) @binding(6) var ground_tex: texture_2d<f32>;
@group(0) @binding(7) var decal_tex: texture_2d<f32>;
@group(0) @binding(8) var detail_tex: texture_2d<f32>;
@group(0) @binding(9) var linear_clamp: sampler;
@group(0) @binding(10) var linear_wrap: sampler;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) m0: vec4<f32>,
    @location(4) m1: vec4<f32>,
    @location(5) m2: vec4<f32>,
    @location(6) tint: vec4<f32>,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) emissive: f32,
};

@vertex
fn vs_main(v: VIn) -> VOut {
    let p = vec4<f32>(v.pos, 1.0);
    let w = vec3<f32>(dot(v.m0, p), dot(v.m1, p), dot(v.m2, p));
    // Inverse-transpose approximation for non-uniform scale.
    let c0 = vec3<f32>(v.m0.x, v.m1.x, v.m2.x);
    let c1 = vec3<f32>(v.m0.y, v.m1.y, v.m2.y);
    let c2 = vec3<f32>(v.m0.z, v.m1.z, v.m2.z);
    let n = c0 * v.normal.x / max(dot(c0, c0), 1e-6)
          + c1 * v.normal.y / max(dot(c1, c1), 1e-6)
          + c2 * v.normal.z / max(dot(c2, c2), 1e-6);
    var o: VOut;
    o.clip = g.view_proj * vec4<f32>(w, 1.0);
    o.world = w;
    o.normal = n;
    // Mesh colours and tints are authored in display space; square to approximate linear.
    let authored = v.color.rgb * v.tint.rgb;
    o.color = authored * authored;
    // Vertex alpha is an emissive mask; instance alpha is its strength (plus a base glow when > 1).
    o.emissive = v.color.a * v.tint.a;
    return o;
}

fn sun_shadow(p: vec3<f32>, n: vec3<f32>) -> f32 {
    let lp = g.sun_vp * vec4<f32>(p + n * 0.05, 1.0);
    let uv = vec2<f32>(lp.x * 0.5 + 0.5, 0.5 - lp.y * 0.5);
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || lp.z > 1.0) { return 1.0; }
    let t = 1.0 / 2048.0;
    var s = textureSampleCompareLevel(sun_map, shadow_sampler, uv + vec2<f32>(-t, -t) * 0.7, lp.z);
    s += textureSampleCompareLevel(sun_map, shadow_sampler, uv + vec2<f32>(t, -t) * 0.7, lp.z);
    s += textureSampleCompareLevel(sun_map, shadow_sampler, uv + vec2<f32>(-t, t) * 0.7, lp.z);
    s += textureSampleCompareLevel(sun_map, shadow_sampler, uv + vec2<f32>(t, t) * 0.7, lp.z);
    return s * 0.25;
}

fn flash_light(p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    if (g.flash_pos.w <= 0.0) { return vec3<f32>(0.0); }
    let to = g.flash_pos.xyz - p;
    let d2 = dot(to, to);
    let d = sqrt(d2);
    if (d > g.flash_pos.w) { return vec3<f32>(0.0); }
    let l = to / d;
    let spot = dot(-l, g.flash_dir.xyz);
    let cone = clamp((spot - g.flash_dir.w) / (g.flash_color.w - g.flash_dir.w), 0.0, 1.0);
    if (cone <= 0.0) { return vec3<f32>(0.0); }
    let nl = max(dot(n, l), 0.0) * 0.85 + 0.15;
    let lp = g.flash_vp * vec4<f32>(p + n * 0.06, 1.0);
    let z = lp.z / lp.w;
    let uv = vec2<f32>(lp.x / lp.w * 0.5 + 0.5, 0.5 - lp.y / lp.w * 0.5);
    let s = textureSampleCompareLevel(flash_map, shadow_sampler, uv, z);
    let a = 1.0 - d / g.flash_pos.w;
    return g.flash_color.rgb * nl * cone * cone * a * a * s;
}

fn grid_cell(p: vec3<f32>) -> i32 {
    let cx = i32(floor((p.x - g.grid.x) / g.grid.z));
    let cz = i32(floor((p.z - g.grid.y) / g.grid.z));
    let dim = i32(g.grid.w);
    if (cx < 0 || cz < 0 || cx >= dim || cz >= dim) { return -1; }
    return cz * dim + cx;
}

fn shade(p: vec3<f32>, n: vec3<f32>, albedo: vec3<f32>, spec_amount: f32, gloss: f32) -> vec3<f32> {
    let v = normalize(g.cam_pos.xyz - p);
    let hemi = n.y * 0.5 + 0.5;
    var diff = mix(g.amb_ground.rgb, g.amb_sky.rgb, hemi);
    var spec = vec3<f32>(0.0);

    let ndl = max(dot(n, g.sun_dir.xyz), 0.0);
    if (ndl > 0.0) {
        let s = sun_shadow(p, n);
        diff += g.sun_color.rgb * ndl * s;
        let h = normalize(g.sun_dir.xyz + v);
        spec += g.sun_color.rgb * pow(max(dot(n, h), 0.0), gloss) * s * ndl;
    }

    let fl = flash_light(p, n);
    diff += fl;

    let cell = grid_cell(p);
    if (cell >= 0) {
        let base = u32(cell) * GRID_STRIDE;
        let count = grid[base];
        for (var i = 0u; i < count; i++) {
            let li = lights[grid[base + 1u + i]];
            let to = li.pos_radius.xyz - p;
            let d2 = dot(to, to);
            let r2 = li.pos_radius.w * li.pos_radius.w;
            if (d2 < r2) {
                let inv = 1.0 / sqrt(max(d2, 1e-4));
                let l = to * inv;
                let k = 1.0 - d2 / r2;
                let att = k * k;
                diff += li.color.rgb * att * (max(dot(n, l), 0.0) * 0.9 + 0.1);
                let h = normalize(l + v);
                spec += li.color.rgb * att * pow(max(dot(n, h), 0.0), gloss);
            }
        }
    }

    let rim = pow(1.0 - max(dot(n, v), 0.0), 3.0);
    return albedo * diff + spec * spec_amount + (g.amb_sky.rgb * 1.6 + fl * 0.3) * rim * 0.35 * albedo;
}

fn apply_fog(c: vec3<f32>, p: vec3<f32>) -> vec3<f32> {
    let e = max(abs(p.x), abs(p.z));
    let edge = clamp((e - g.map.x * 0.82) / (g.map.x * 0.3), 0.0, 1.0);
    return mix(c, g.fog.rgb, edge * edge);
}

@fragment
fn fs_main(i: VOut) -> @location(0) vec4<f32> {
    let n = normalize(i.normal);
    var c = shade(i.world, n, i.color, 0.25, 32.0);
    c += i.color * i.emissive;
    return vec4<f32>(apply_fog(c, i.world), 1.0);
}

// Ghost preview for base building: additive and pulsing.
@fragment
fn fs_ghost(i: VOut) -> @location(0) vec4<f32> {
    let n = normalize(i.normal);
    let v = normalize(g.cam_pos.xyz - i.world);
    let fres = pow(1.0 - max(dot(n, v), 0.0), 1.5);
    let pulse = 0.65 + 0.35 * sin(g.cam_pos.w * 6.0);
    return vec4<f32>(i.color * (0.2 + fres) * pulse, 0.0);
}

// ---------------------------------------------------------------- ground
struct GOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
};

@vertex
fn vs_ground(@builtin(vertex_index) vi: u32) -> GOut {
    var q = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0));
    let e = g.map.y;
    let w = vec3<f32>(q[vi].x * e, 0.0, q[vi].y * e);
    var o: GOut;
    o.clip = g.view_proj * vec4<f32>(w, 1.0);
    o.world = w;
    return o;
}

@fragment
fn fs_ground(i: GOut) -> @location(0) vec4<f32> {
    let p = i.world;
    let uv = p.xz / (2.0 * g.map.x) + 0.5;
    let base = textureSample(ground_tex, linear_clamp, uv);
    let decal = textureSample(decal_tex, linear_clamp, uv);
    let d1 = textureSample(detail_tex, linear_wrap, p.xz * 0.27);
    let d2 = textureSample(detail_tex, linear_wrap, p.xz * 0.041 + vec2<f32>(0.37, 0.11));
    var albedo = base.rgb * (0.62 + 0.55 * d1.r) * (0.72 + 0.5 * d2.g);
    // Fake micro normal from the detail texture channels.
    var n = normalize(vec3<f32>((d1.b - 0.5) * 0.9, 1.0, (d1.a - 0.5) * 0.9));
    let wet = decal.a;
    albedo = mix(albedo, decal.rgb, wet);
    if (wet > 0.25) { n = normalize(mix(n, vec3<f32>(0.0, 1.0, 0.0), wet)); }
    let spec_amount = base.a * 0.35 + wet * decal.r * 1.0;
    let c = shade(p, n, albedo, spec_amount, 24.0 + wet * 48.0);
    return vec4<f32>(apply_fog(c, p), 1.0);
}

// ---------------------------------------------------------------- particles
struct PIn {
    @builtin(vertex_index) vi: u32,
    @location(0) pos_size: vec4<f32>,
    @location(1) stretch_shape: vec4<f32>,
    @location(2) color: vec4<f32>,
};
struct POut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) shape: f32,
    @location(3) world: vec2<f32>,
};

@vertex
fn vs_particle(i: PIn) -> POut {
    var q = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0));
    let c = q[i.vi];
    let center = i.pos_size.xyz;
    let size = i.pos_size.w;
    let view = normalize(g.cam_pos.xyz - center);
    var right: vec3<f32>;
    var up: vec3<f32>;
    var mid = center;
    let s = i.stretch_shape.xyz;
    let sl = length(s);
    if (sl > 1e-4) {
        // Stretched along motion: the quad extends along s and is `size` wide.
        up = s * 0.5;
        right = normalize(cross(s, view)) * size;
        mid = center - s * 0.5;
    } else {
        right = normalize(cross(vec3<f32>(0.0, 1.0, 0.001), view)) * size;
        up = normalize(cross(view, right)) * size;
    }
    let w = mid + right * c.x + up * c.y;
    var o: POut;
    o.clip = g.view_proj * vec4<f32>(w, 1.0);
    o.uv = c;
    o.color = i.color;
    o.shape = i.stretch_shape.w;
    o.world = w.xz;
    return o;
}

@fragment
fn fs_particle(i: POut) -> @location(0) vec4<f32> {
    let r = length(i.uv);
    if (r >= 1.0) { discard; }
    var cov: f32;
    if (i.shape < 0.5) {
        // Soft glow.
        let k = 1.0 - r;
        cov = k * k;
    } else if (i.shape < 1.5) {
        // Puffy smoke, eroded by noise.
        let n = textureSample(detail_tex, linear_wrap, i.world * 0.09 + i.uv * 0.05).g;
        cov = clamp((1.0 - r) * (0.55 + n * 1.4) - 0.1, 0.0, 1.0);
    } else {
        // Hard droplet.
        cov = smoothstep(1.0, 0.75, r);
    }
    // Premultiplied: alpha 0 is purely additive, alpha 1 is purely over.
    return vec4<f32>(i.color.rgb * cov, i.color.a * cov);
}
