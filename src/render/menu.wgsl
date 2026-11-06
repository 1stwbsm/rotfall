// The title screen: the pre-rendered Blender image, kept alive with subtle motion.
struct Menu {
    a: vec4<f32>,    // screen width, height, image aspect, time
    b: vec4<f32>,    // mouse x, y in -1..1, manual gamma flag, darkening of the menu side
    logo: vec4<f32>, // x, y, width, height in pixels; width 0 hides it
};
@group(0) @binding(0) var<uniform> m: Menu;
@group(0) @binding(1) var bg: texture_2d<f32>;
@group(0) @binding(2) var logo_tex: texture_2d<f32>;
@group(0) @binding(3) var detail: texture_2d<f32>;
@group(0) @binding(4) var clamp_s: sampler;
@group(0) @binding(5) var wrap_s: sampler;

struct V { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };

@vertex
fn vs_bg(@builtin(vertex_index) vi: u32) -> V {
    let u = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    var o: V;
    o.pos = vec4<f32>(u.x * 2.0 - 1.0, 1.0 - u.y * 2.0, 0.0, 1.0);
    o.uv = u;
    return o;
}

fn encode(c: vec3<f32>) -> vec3<f32> {
    if (m.b.z > 0.5) { return pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2)); }
    return c;
}

@fragment
fn fs_bg(i: V) -> @location(0) vec4<f32> {
    let t = m.a.w;
    // Cover the window whatever its shape, with a slow push-in and a little mouse parallax.
    let sa = m.a.x / m.a.y;
    var fit = vec2<f32>(1.0, 1.0);
    if (sa > m.a.z) { fit.y = m.a.z / sa; } else { fit.x = sa / m.a.z; }
    let zoom = 1.06 + 0.012 * sin(t * 0.07);
    let uv = (i.uv - 0.5) * fit / zoom + 0.5 + m.b.xy * vec2<f32>(0.012, 0.008);
    var c = textureSample(bg, clamp_s, uv).rgb;

    // The lights live: the reactor breathes and the fires flicker. Both key off hue.
    let blue = smoothstep(0.0, 0.25, c.b - c.r);
    let warm = smoothstep(0.0, 0.25, c.r - c.b);
    let flicker = sin(t * 11.0) * 0.4 + sin(t * 17.3 + 1.0) * 0.35 + sin(t * 6.1 + 2.0) * 0.5;
    c *= 1.0 + blue * 0.11 * sin(t * 1.3) + warm * 0.13 * flicker;

    // Ground fog drifting across the street.
    let n1 = textureSample(detail, wrap_s, uv * vec2<f32>(0.55, 0.3) + vec2<f32>(t * 0.006, 0.0)).g;
    let n2 = textureSample(detail, wrap_s, uv * vec2<f32>(1.0, 0.55) - vec2<f32>(t * 0.011, t * 0.002)).g;
    let band = smoothstep(0.42, 0.68, uv.y) * smoothstep(1.0, 0.72, uv.y);
    let fog = clamp((n1 * 0.6 + n2 * 0.4 - 0.25) * 1.6, 0.0, 1.0) * band;
    // Scales with what is already lit, so that fog reads as glow and never lifts the blacks.
    let lit = c.r * 0.3 + c.g * 0.5 + c.b * 0.5;
    c += vec3<f32>(0.0022, 0.0055, 0.0075) * fog + vec3<f32>(0.12, 0.2, 0.26) * fog * min(lit, 0.2);

    // Shade the side the menu sits on, and the corners.
    let side = smoothstep(0.42, 0.0, i.uv.x) * m.b.w;
    c *= 1.0 - 0.55 * side;
    let d = i.uv - 0.5;
    c *= 1.0 - clamp(dot(d, d) * 0.9, 0.0, 0.8);
    // Dither in display space: dark gradients in 8 bits would otherwise show contours.
    let h = fract(sin(dot(i.pos.xy + vec2<f32>(t * 13.0, 0.0), vec2<f32>(12.9898, 78.233))) * 43758.5453) - 0.5;
    let g = sqrt(max(c, vec3<f32>(0.0))) + h * 1.8 / 255.0;
    c = g * g;
    return vec4<f32>(encode(c), 1.0);
}

@vertex
fn vs_logo(@builtin(vertex_index) vi: u32) -> V {
    var q = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0));
    let u = q[vi];
    let p = m.logo.xy + u * m.logo.zw;
    var o: V;
    o.pos = vec4<f32>(p.x / m.a.x * 2.0 - 1.0, 1.0 - p.y / m.a.y * 2.0, 0.0, 1.0);
    o.uv = u;
    return o;
}

@fragment
fn fs_logo(i: V) -> @location(0) vec4<f32> {
    let c = textureSample(logo_tex, clamp_s, i.uv);
    return vec4<f32>(encode(c.rgb * 1.5), c.a);
}
