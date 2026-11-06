//! The renderer: a small custom engine on top of wgpu (used purely as a GPU API).
//!
//! Frame graph:
//!   decals (world-space ground layer, persistent) ->
//!   sun shadow map -> flashlight shadow map ->
//!   HDR forward pass (ground, instanced meshes, ghosts, particles; MSAA) ->
//!   bloom pyramid -> tonemapped composite -> UI.
pub mod font;
pub mod mesh;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use mesh::{MESH_COUNT, Vertex};
use std::mem::size_of;
use wgpu::util::DeviceExt;

pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SHADOW_SIZE: u32 = 2048;
const FLASH_SHADOW_SIZE: u32 = 1024;
// The persistent gore layer is the largest texture in the game; browsers get a lighter one.
#[cfg(not(target_arch = "wasm32"))]
const DECAL_SIZE: u32 = 4096;
#[cfg(target_arch = "wasm32")]
const DECAL_SIZE: u32 = 2048;
const BLOOM_LEVELS: usize = 5;
pub const MAX_INSTANCES: usize = 60_000;
pub const MAX_PARTICLES: usize = 16_000;
const MAX_DECALS: usize = 1024;
pub const MAX_LIGHTS: usize = 256;
const MAX_UI_VERTS: usize = 120_000;
const GRID_DIM: usize = 32;
const GRID_CELL: f32 = 3.5;
const GRID_PER_CELL: usize = 16;
const GRID_STRIDE: usize = GRID_PER_CELL + 1;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct Instance {
    pub m0: [f32; 4],
    pub m1: [f32; 4],
    pub m2: [f32; 4],
    /// rgb multiplies the vertex colour; a scales the emissive mask.
    pub tint: [f32; 4],
}

impl Instance {
    #[inline]
    pub fn new(m: &Mat4, tint: [f32; 3], glow: f32) -> Self {
        let c = m.to_cols_array_2d();
        Instance {
            m0: [c[0][0], c[1][0], c[2][0], c[3][0]],
            m1: [c[0][1], c[1][1], c[2][1], c[3][1]],
            m2: [c[0][2], c[1][2], c[2][2], c[3][2]],
            tint: [tint[0], tint[1], tint[2], glow],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Particle {
    pub pos: [f32; 3],
    pub size: f32,
    pub stretch: [f32; 3],
    /// 0 glow, 1 smoke, 2 droplet.
    pub shape: f32,
    /// Linear HDR colour with premultiplied-style alpha: a=0 additive, a=1 over.
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Decal {
    pub center: [f32; 2],
    pub size: f32,
    pub rot: f32,
    pub color: [f32; 4],
    /// seed, kind (0 blood, 1 scorch, 2 acid), map half extent, unused.
    pub params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct PointLight {
    pub pos: [f32; 3],
    pub radius: f32,
    pub color: [f32; 3],
    pub _pad: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct UiVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[derive(Clone, Copy)]
pub struct Flashlight {
    pub pos: Vec3,
    pub dir: Vec3,
    pub color: Vec3,
    pub range: f32,
    pub cos_outer: f32,
    pub cos_inner: f32,
}

/// Asks for the title screen in place of the 3D scene.
#[derive(Clone, Copy)]
pub struct MenuView {
    pub time: f32,
    pub mouse: [f32; 2],
    /// x, y, width, height in pixels. Zero width hides the logo.
    pub logo: [f32; 4],
    /// How strongly to shade the side the menu sits on, 0..1.
    pub dim: f32,
}

/// Everything the game hands to the renderer for one frame.
pub struct FrameData {
    pub instances: Vec<Vec<Instance>>,
    pub ghosts: Vec<(usize, Instance)>,
    pub particles: Vec<Particle>,
    pub decals: Vec<Decal>,
    pub clear_decals: bool,
    pub lights: Vec<PointLight>,
    pub ui: Vec<UiVertex>,
    pub view_proj: Mat4,
    pub cam_pos: Vec3,
    pub focus: Vec3,
    pub sun_dir: Vec3,
    pub sun_color: Vec3,
    pub amb_sky: Vec3,
    pub amb_ground: Vec3,
    pub fog: Vec3,
    pub flashlight: Option<Flashlight>,
    pub time: f32,
    pub exposure: f32,
    pub bloom: f32,
    pub vignette: f32,
    pub damage: f32,
    pub chroma: f32,
    pub desaturate: f32,
    pub menu: Option<MenuView>,
}

impl FrameData {
    pub fn new() -> Self {
        FrameData {
            instances: (0..MESH_COUNT).map(|_| Vec::with_capacity(64)).collect(),
            ghosts: Vec::new(),
            particles: Vec::with_capacity(MAX_PARTICLES),
            decals: Vec::with_capacity(256),
            clear_decals: false,
            lights: Vec::with_capacity(MAX_LIGHTS),
            ui: Vec::with_capacity(16384),
            view_proj: Mat4::IDENTITY,
            cam_pos: Vec3::ONE,
            focus: Vec3::ZERO,
            sun_dir: Vec3::Y,
            sun_color: Vec3::ONE,
            amb_sky: Vec3::splat(0.1),
            amb_ground: Vec3::splat(0.05),
            fog: Vec3::ZERO,
            flashlight: None,
            time: 0.0,
            exposure: 1.0,
            bloom: 0.1,
            vignette: 1.0,
            damage: 0.0,
            chroma: 0.0,
            desaturate: 0.0,
            menu: None,
        }
    }

    pub fn begin(&mut self) {
        for v in &mut self.instances {
            v.clear();
        }
        self.ghosts.clear();
        self.particles.clear();
        self.lights.clear();
        self.ui.clear();
        self.flashlight = None;
        self.menu = None;
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    sun_vp: [[f32; 4]; 4],
    flash_vp: [[f32; 4]; 4],
    cam_pos: [f32; 4],
    sun_dir: [f32; 4],
    sun_color: [f32; 4],
    amb_sky: [f32; 4],
    amb_ground: [f32; 4],
    fog: [f32; 4],
    flash_pos: [f32; 4],
    flash_dir: [f32; 4],
    flash_color: [f32; 4],
    grid: [f32; 4],
    map: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PostUniform {
    a: [f32; 4],
    b: [f32; 4],
}

/// Which GPU and API are in use, for crash reports.
pub static GPU_DESCRIPTION: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter: wgpu::Adapter,
}

impl Gpu {
    pub async fn new(instance: &wgpu::Instance, surface: Option<&wgpu::Surface<'_>>) -> Result<Gpu, String> {
        // Choose deliberately: real hardware over software renderers, then the best-tested
        // graphics API, then the fastest GPU that API can see. The API comes before the GPU type
        // because one physical GPU can report different types through different APIs.
        // WGPU_BACKEND, if set, has already narrowed what the instance can see.
        let score = |a: &wgpu::Adapter| {
            let info = a.get_info();
            let device = match info.device_type {
                wgpu::DeviceType::DiscreteGpu => 3,
                wgpu::DeviceType::IntegratedGpu => 2,
                wgpu::DeviceType::Cpu => 0,
                _ => 1,
            };
            let api = if info.backend == wgpu::Backend::Vulkan { 2 } else { 1 };
            (device > 0, api, device)
        };
        // Browsers do not list adapters: they offer one when asked.
        #[cfg(target_arch = "wasm32")]
        let adapter = {
            let _ = score;
            instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    force_fallback_adapter: false,
                    compatible_surface: surface,
                    apply_limit_buckets: false,
                })
                .await
                .map_err(|e| format!("no suitable GPU adapter was found: {e}"))?
        };
        #[cfg(not(target_arch = "wasm32"))]
        let adapter = instance
            .enumerate_adapters(wgpu::Backends::all())
            .await
            .into_iter()
            .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
            .max_by_key(score)
            .ok_or_else(|| "no suitable GPU adapter was found".to_string())?;
        let description = format!("{} ({:?})", adapter.get_info().name, adapter.get_info().backend);
        eprintln!("GPU: {description}");
        let _ = GPU_DESCRIPTION.set(description);
        let wanted = wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
        let features = adapter.features() & wanted;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("device"),
                required_features: features,
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|e| format!("request_device failed: {e}"))?;
        Ok(Gpu { device, queue, adapter })
    }
}

struct SizedTargets {
    hdr: wgpu::TextureView,
    msaa: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
    bloom: Vec<wgpu::TextureView>,
    down_binds: Vec<wgpu::BindGroup>,
    up_binds: Vec<wgpu::BindGroup>,
    composite_bind: wgpu::BindGroup,
    size: (u32, u32),
}

pub struct Renderer {
    pub gpu: Gpu,
    manual_gamma: bool,
    samples: u32,
    render_scale: f32,
    map_half: f32,

    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    ranges: Vec<(u32, u32)>,
    instance_buf: wgpu::Buffer,
    ghost_buf: wgpu::Buffer,
    particle_buf: wgpu::Buffer,
    decal_buf: wgpu::Buffer,
    ui_buf: wgpu::Buffer,
    globals_buf: wgpu::Buffer,
    light_buf: wgpu::Buffer,
    grid_buf: wgpu::Buffer,
    sun_buf: wgpu::Buffer,
    flash_buf: wgpu::Buffer,
    post_buf: wgpu::Buffer,
    ui_uniform: wgpu::Buffer,

    scene_bind: wgpu::BindGroup,
    sun_bind: wgpu::BindGroup,
    flash_bind: wgpu::BindGroup,
    decal_bind: wgpu::BindGroup,
    ui_bind: wgpu::BindGroup,

    scene_pipe: wgpu::RenderPipeline,
    ghost_pipe: wgpu::RenderPipeline,
    ground_pipe: wgpu::RenderPipeline,
    particle_pipe: wgpu::RenderPipeline,
    shadow_pipe: wgpu::RenderPipeline,
    decal_pipe: wgpu::RenderPipeline,
    decal_fade_pipe: wgpu::RenderPipeline,
    prefilter_pipe: wgpu::RenderPipeline,
    down_pipe: wgpu::RenderPipeline,
    up_pipe: wgpu::RenderPipeline,
    composite_pipe: wgpu::RenderPipeline,
    ui_pipe: wgpu::RenderPipeline,

    post_layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
    clamp_sampler: wgpu::Sampler,
    sun_view: wgpu::TextureView,
    flash_view: wgpu::TextureView,
    decal_view: wgpu::TextureView,

    targets: SizedTargets,
    ranges_used: Vec<(u32, u32, u32)>,
    grid_scratch: Vec<u32>,
    instance_scratch: Vec<Instance>,
    fade_timer: f32,
    last_time: f32,
    decal_needs_clear: bool,
    ground_tex: wgpu::Texture,
    menu_art: MenuArt,
    pub stats: FrameStats,
}

struct MenuArt {
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
    bg_pipe: wgpu::RenderPipeline,
    logo_pipe: wgpu::RenderPipeline,
    aspect: f32,
}

#[derive(Default, Clone, Copy)]
pub struct FrameStats {
    pub instances: usize,
    pub lights: usize,
}

fn tex_entry(binding: u32, vis: wgpu::ShaderStages, sample: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: vis,
        ty: wgpu::BindingType::Texture {
            sample_type: sample,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32, vis: wgpu::ShaderStages, ty: wgpu::SamplerBindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: vis, ty: wgpu::BindingType::Sampler(ty), count: None }
}

fn uniform_entry(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: vis,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    }
}

fn storage_entry(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: vis,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    }
}

fn vertex_layouts() -> [wgpu::VertexBufferLayout<'static>; 2] {
    const V: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Unorm8x4];
    const I: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4];
    [
        wgpu::VertexBufferLayout { array_stride: size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &V },
        wgpu::VertexBufferLayout { array_stride: size_of::<Instance>() as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &I },
    ]
}

fn dyn_buffer(device: &wgpu::Device, label: &str, size: usize, usage: wgpu::BufferUsages) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size as u64,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn upload_texture(gpu: &Gpu, label: &str, w: u32, h: u32, format: wgpu::TextureFormat, bpp: u32, levels: &[Vec<u8>]) -> wgpu::TextureView {
    let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (i, data) in levels.iter().enumerate() {
        let (lw, lh) = ((w >> i).max(1), (h >> i).max(1));
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: i as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            data,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(lw * bpp), rows_per_image: Some(lh) },
            wgpu::Extent3d { width: lw, height: lh, depth_or_array_layers: 1 },
        );
    }
    tex.create_view(&Default::default())
}

fn write_ground(queue: &wgpu::Queue, tex: &wgpu::Texture, data: &[u8], size: u32) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        data,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size * 4), rows_per_image: Some(size) },
        wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
    );
}

/// Tileable detail texture: r = fine grain, g = blotches, b/a = fake normal.
fn build_detail(size: u32) -> Vec<Vec<u8>> {
    let hash = |x: i32, y: i32, per: i32, s: u32| -> f32 {
        let (x, y) = (x.rem_euclid(per) as u32, y.rem_euclid(per) as u32);
        let mut h = x.wrapping_mul(0x8DA6B343) ^ y.wrapping_mul(0xD8163841) ^ s.wrapping_mul(0xCB1AB31F);
        h ^= h >> 13;
        h = h.wrapping_mul(0x5BD1E995);
        h ^= h >> 15;
        (h & 0xFFFF) as f32 / 65535.0
    };
    let value = |u: f32, v: f32, per: i32, s: u32| -> f32 {
        let (x, y) = (u * per as f32, v * per as f32);
        let (ix, iy) = (x.floor() as i32, y.floor() as i32);
        let (fx, fy) = (x - ix as f32, y - iy as f32);
        let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
        let a = hash(ix, iy, per, s) + (hash(ix + 1, iy, per, s) - hash(ix, iy, per, s)) * sx;
        let b = hash(ix, iy + 1, per, s) + (hash(ix + 1, iy + 1, per, s) - hash(ix, iy + 1, per, s)) * sx;
        a + (b - a) * sy
    };
    let n = size as usize;
    let mut base = vec![0u8; n * n * 4];
    let height = |u: f32, v: f32| {
        0.5 * value(u, v, 64, 1) + 0.3 * value(u, v, 128, 2) + 0.2 * value(u, v, 256, 3)
    };
    for y in 0..n {
        for x in 0..n {
            let (u, v) = (x as f32 / n as f32, y as f32 / n as f32);
            let e = 1.0 / n as f32;
            let h0 = height(u, v);
            let dx = height(u + e, v) - h0;
            let dy = height(u, v + e) - h0;
            let blotch = 0.5 * value(u, v, 8, 7) + 0.3 * value(u, v, 16, 8) + 0.2 * value(u, v, 32, 9);
            let i = (y * n + x) * 4;
            base[i] = (h0 * 255.0) as u8;
            base[i + 1] = (blotch * 255.0) as u8;
            base[i + 2] = ((0.5 + dx * 7.0).clamp(0.0, 1.0) * 255.0) as u8;
            base[i + 3] = ((0.5 + dy * 7.0).clamp(0.0, 1.0) * 255.0) as u8;
        }
    }
    let mut levels = vec![base];
    let mut s = n;
    while s > 1 {
        let prev = levels.last().unwrap();
        let ns = s / 2;
        let mut next = vec![0u8; ns * ns * 4];
        for y in 0..ns {
            for x in 0..ns {
                for c in 0..4 {
                    let g = |a: usize, b: usize| prev[(b * s + a) * 4 + c] as u32;
                    let v = g(x * 2, y * 2) + g(x * 2 + 1, y * 2) + g(x * 2, y * 2 + 1) + g(x * 2 + 1, y * 2 + 1);
                    next[(y * ns + x) * 4 + c] = (v / 4) as u8;
                }
            }
        }
        levels.push(next);
        s = ns;
    }
    levels
}

impl Renderer {
    /// `ground` is an sRGB RGBA image covering the whole playfield; alpha is specular amount.
    pub fn new(
        gpu: Gpu,
        target_format: wgpu::TextureFormat,
        size: (u32, u32),
        render_scale: f32,
        map_half: f32,
        ground: (&[u8], u32),
    ) -> Renderer {
        let device = gpu.device.clone();
        let device = &device;
        let manual_gamma = !target_format.is_srgb();
        // Anti-aliasing needs every attachment to support it, and drivers differ: some offer 4x
        // on colour but not depth. Without the feature, only the WebGPU guarantees apply.
        use wgpu::TextureFormatFeatureFlags as F;
        let hdr = gpu.adapter.get_texture_format_features(HDR_FORMAT).flags;
        let depth = gpu.adapter.get_texture_format_features(DEPTH_FORMAT).flags;
        // WebGPU guarantees 4x for these formats, and has no feature to ask for.
        let samples = if (cfg!(target_arch = "wasm32") || gpu.device.features().contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES))
            && hdr.contains(F::MULTISAMPLE_X4 | F::MULTISAMPLE_RESOLVE)
            && depth.contains(F::MULTISAMPLE_X4)
        {
            4
        } else {
            1
        };
        if samples == 1 {
            eprintln!("note: 4x anti-aliasing is not available on this GPU; running without it");
        }

        let lib = mesh::build_library();
        let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("verts"),
            contents: bytemuck::cast_slice(&lib.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("indices"),
            contents: bytemuck::cast_slice(&lib.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        use wgpu::BufferUsages as U;
        let instance_buf = dyn_buffer(device, "instances", MAX_INSTANCES * size_of::<Instance>(), U::VERTEX);
        let ghost_buf = dyn_buffer(device, "ghosts", 256 * size_of::<Instance>(), U::VERTEX);
        let particle_buf = dyn_buffer(device, "particles", MAX_PARTICLES * size_of::<Particle>(), U::VERTEX);
        let decal_buf = dyn_buffer(device, "decals", MAX_DECALS * size_of::<Decal>(), U::VERTEX);
        let ui_buf = dyn_buffer(device, "ui", MAX_UI_VERTS * size_of::<UiVertex>(), U::VERTEX);
        let globals_buf = dyn_buffer(device, "globals", size_of::<Globals>(), U::UNIFORM);
        let light_buf = dyn_buffer(device, "lights", MAX_LIGHTS * size_of::<PointLight>(), U::STORAGE);
        let grid_buf = dyn_buffer(device, "light grid", GRID_DIM * GRID_DIM * GRID_STRIDE * 4, U::STORAGE);
        let sun_buf = dyn_buffer(device, "sun vp", 64, U::UNIFORM);
        let flash_buf = dyn_buffer(device, "flash vp", 64, U::UNIFORM);
        let post_buf = dyn_buffer(device, "post", size_of::<PostUniform>(), U::UNIFORM);
        let ui_uniform = dyn_buffer(device, "ui uniform", 16, U::UNIFORM);

        // Depth textures for shadow mapping.
        let shadow_tex = |label: &str, s: u32| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d { width: s, height: s, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: DEPTH_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let sun_view = shadow_tex("sun shadow", SHADOW_SIZE);
        let flash_view = shadow_tex("flash shadow", FLASH_SHADOW_SIZE);

        let decal_view = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("decal layer"),
                size: wgpu::Extent3d { width: DECAL_SIZE, height: DECAL_SIZE, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());

        let ground_tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ground"),
            size: wgpu::Extent3d { width: ground.1, height: ground.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        write_ground(&gpu.queue, &ground_tex, ground.0, ground.1);
        let ground_view = ground_tex.create_view(&Default::default());
        let detail_levels = build_detail(512);
        let detail_view = upload_texture(&gpu, "detail", 512, 512, wgpu::TextureFormat::Rgba8Unorm, 4, &detail_levels);
        let atlas_view = upload_texture(&gpu, "font", font::ATLAS_W, font::ATLAS_H, wgpu::TextureFormat::R8Unorm, 1, std::slice::from_ref(&font::build_atlas()));

        let linear = |addr: wgpu::AddressMode| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: addr,
                address_mode_v: addr,
                address_mode_w: addr,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            })
        };
        let clamp_sampler = linear(wgpu::AddressMode::ClampToEdge);
        let wrap_sampler = linear(wgpu::AddressMode::Repeat);
        let nearest = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            compare: Some(wgpu::CompareFunction::LessEqual),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        use wgpu::ShaderStages as St;
        use wgpu::TextureSampleType as T;
        let float = T::Float { filterable: true };
        let scene_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene"),
            entries: &[
                uniform_entry(0, St::VERTEX_FRAGMENT),
                storage_entry(1, St::FRAGMENT),
                storage_entry(2, St::FRAGMENT),
                tex_entry(3, St::FRAGMENT, T::Depth),
                tex_entry(4, St::FRAGMENT, T::Depth),
                sampler_entry(5, St::FRAGMENT, wgpu::SamplerBindingType::Comparison),
                tex_entry(6, St::FRAGMENT, float),
                tex_entry(7, St::FRAGMENT, float),
                tex_entry(8, St::FRAGMENT, float),
                sampler_entry(9, St::FRAGMENT, wgpu::SamplerBindingType::Filtering),
                sampler_entry(10, St::FRAGMENT, wgpu::SamplerBindingType::Filtering),
            ],
        });
        let scene_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene"),
            layout: &scene_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: light_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: grid_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&sun_view) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&flash_view) },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::Sampler(&shadow_sampler) },
                wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(&ground_view) },
                wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(&decal_view) },
                wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::TextureView(&detail_view) },
                wgpu::BindGroupEntry { binding: 9, resource: wgpu::BindingResource::Sampler(&clamp_sampler) },
                wgpu::BindGroupEntry { binding: 10, resource: wgpu::BindingResource::Sampler(&wrap_sampler) },
            ],
        });

        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow"),
            entries: &[uniform_entry(0, St::VERTEX)],
        });
        let single_uniform_bind = |b: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &shadow_layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: b.as_entire_binding() }],
            })
        };
        let sun_bind = single_uniform_bind(&sun_buf);
        let flash_bind = single_uniform_bind(&flash_buf);

        let decal_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("decal"),
            entries: &[tex_entry(0, St::FRAGMENT, float), sampler_entry(1, St::FRAGMENT, wgpu::SamplerBindingType::Filtering)],
        });
        let decal_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &decal_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&detail_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&wrap_sampler) },
            ],
        });

        let post_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post"),
            entries: &[tex_entry(0, St::FRAGMENT, float), sampler_entry(1, St::FRAGMENT, wgpu::SamplerBindingType::Filtering)],
        });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composite"),
            entries: &[
                tex_entry(0, St::FRAGMENT, float),
                sampler_entry(1, St::FRAGMENT, wgpu::SamplerBindingType::Filtering),
                tex_entry(2, St::FRAGMENT, float),
                uniform_entry(3, St::FRAGMENT),
            ],
        });
        let ui_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui"),
            entries: &[
                uniform_entry(0, St::VERTEX_FRAGMENT),
                tex_entry(1, St::FRAGMENT, float),
                sampler_entry(2, St::FRAGMENT, wgpu::SamplerBindingType::Filtering),
            ],
        });
        let ui_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &ui_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: ui_uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&atlas_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&nearest) },
            ],
        });

        // ---------- pipelines ----------
        let scene_sh = device.create_shader_module(wgpu::include_wgsl!("scene.wgsl"));
        let shadow_sh = device.create_shader_module(wgpu::include_wgsl!("shadow.wgsl"));
        let decal_sh = device.create_shader_module(wgpu::include_wgsl!("decal.wgsl"));
        let post_sh = device.create_shader_module(wgpu::include_wgsl!("post.wgsl"));
        let ui_sh = device.create_shader_module(wgpu::include_wgsl!("ui.wgsl"));

        let layout = |bgs: &[&wgpu::BindGroupLayout]| {
            let list: Vec<Option<&wgpu::BindGroupLayout>> = bgs.iter().map(|b| Some(*b)).collect();
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &list,
                immediate_size: 0,
            })
        };
        let scene_pl = layout(&[&scene_layout]);
        let depth_state = |write: bool, cmp: wgpu::CompareFunction| wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(write),
            depth_compare: Some(cmp),
            stencil: Default::default(),
            bias: Default::default(),
        };
        let ms = wgpu::MultisampleState { count: samples, mask: !0, alpha_to_coverage_enabled: false };
        let hdr_target = |blend: Option<wgpu::BlendState>| {
            [Some(wgpu::ColorTargetState { format: HDR_FORMAT, blend, write_mask: wgpu::ColorWrites::ALL })]
        };
        let premult = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let culled = wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Back), front_face: wgpu::FrontFace::Ccw, ..Default::default() };
        let vl = vertex_layouts().map(Some);
        let scene_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scene"),
            layout: Some(&scene_pl),
            vertex: wgpu::VertexState { module: &scene_sh, entry_point: Some("vs_main"), compilation_options: Default::default(), buffers: &vl },
            primitive: culled,
            depth_stencil: Some(depth_state(true, wgpu::CompareFunction::Less)),
            multisample: ms,
            fragment: Some(wgpu::FragmentState { module: &scene_sh, entry_point: Some("fs_main"), compilation_options: Default::default(), targets: &hdr_target(None) }),
            multiview_mask: None,
            cache: None,
        });
        let ghost_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ghost"),
            layout: Some(&scene_pl),
            vertex: wgpu::VertexState { module: &scene_sh, entry_point: Some("vs_main"), compilation_options: Default::default(), buffers: &vl },
            primitive: culled,
            depth_stencil: Some(depth_state(false, wgpu::CompareFunction::Less)),
            multisample: ms,
            fragment: Some(wgpu::FragmentState { module: &scene_sh, entry_point: Some("fs_ghost"), compilation_options: Default::default(), targets: &hdr_target(Some(premult)) }),
            multiview_mask: None,
            cache: None,
        });
        let ground_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ground"),
            layout: Some(&scene_pl),
            vertex: wgpu::VertexState { module: &scene_sh, entry_point: Some("vs_ground"), compilation_options: Default::default(), buffers: &[] },
            primitive: Default::default(),
            depth_stencil: Some(depth_state(true, wgpu::CompareFunction::Less)),
            multisample: ms,
            fragment: Some(wgpu::FragmentState { module: &scene_sh, entry_point: Some("fs_ground"), compilation_options: Default::default(), targets: &hdr_target(None) }),
            multiview_mask: None,
            cache: None,
        });
        const P_ATTR: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4];
        let particle_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("particles"),
            layout: Some(&scene_pl),
            vertex: wgpu::VertexState {
                module: &scene_sh,
                entry_point: Some("vs_particle"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout { array_stride: size_of::<Particle>() as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &P_ATTR })],
            },
            primitive: Default::default(),
            depth_stencil: Some(depth_state(false, wgpu::CompareFunction::Less)),
            multisample: ms,
            fragment: Some(wgpu::FragmentState { module: &scene_sh, entry_point: Some("fs_particle"), compilation_options: Default::default(), targets: &hdr_target(Some(premult)) }),
            multiview_mask: None,
            cache: None,
        });
        let shadow_pl = layout(&[&shadow_layout]);
        let shadow_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow"),
            layout: Some(&shadow_pl),
            vertex: wgpu::VertexState { module: &shadow_sh, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &vl },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: wgpu::DepthBiasState { constant: 2, slope_scale: 2.0, clamp: 0.0 },
            }),
            multisample: Default::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });
        let decal_pl = layout(&[&decal_layout]);
        const D_ATTR: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4];
        let alpha_blend = wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
            alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
        };
        let decal_target = [Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba8Unorm, blend: Some(alpha_blend), write_mask: wgpu::ColorWrites::ALL })];
        let decal_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("decals"),
            layout: Some(&decal_pl),
            vertex: wgpu::VertexState {
                module: &decal_sh,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout { array_stride: size_of::<Decal>() as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &D_ATTR })],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &decal_sh, entry_point: Some("fs"), compilation_options: Default::default(), targets: &decal_target }),
            multiview_mask: None,
            cache: None,
        });
        // The fade removes a tiny fraction of the alpha channel only.
        let fade_blend = wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
            alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Constant, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::ReverseSubtract },
        };
        let empty_pl = layout(&[]);
        let decal_fade_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("decal fade"),
            layout: Some(&empty_pl),
            vertex: wgpu::VertexState { module: &decal_sh, entry_point: Some("vs_fade"), compilation_options: Default::default(), buffers: &[] },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &decal_sh,
                entry_point: Some("fs_fade"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba8Unorm, blend: Some(fade_blend), write_mask: wgpu::ColorWrites::ALPHA })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let post_pl = layout(&[&post_layout]);
        let fullscreen = |label: &str, entry: &str, pl: &wgpu::PipelineLayout, format: wgpu::TextureFormat, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(pl),
                vertex: wgpu::VertexState { module: &post_sh, entry_point: Some("vs_full"), compilation_options: Default::default(), buffers: &[] },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &post_sh,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let prefilter_pipe = fullscreen("prefilter", "fs_prefilter", &post_pl, HDR_FORMAT, None);
        let down_pipe = fullscreen("down", "fs_down", &post_pl, HDR_FORMAT, None);
        let additive = wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
            alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
        };
        let up_pipe = fullscreen("up", "fs_up", &post_pl, HDR_FORMAT, Some(additive));
        let comp_pl = layout(&[&composite_layout]);
        let composite_pipe = fullscreen("composite", "fs_composite", &comp_pl, target_format, None);

        let ui_pl = layout(&[&ui_layout]);
        const UI_ATTR: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];
        let ui_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui"),
            layout: Some(&ui_pl),
            vertex: wgpu::VertexState {
                module: &ui_sh,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout { array_stride: size_of::<UiVertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &UI_ATTR })],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &ui_sh,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: target_format, blend: Some(alpha_blend), write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let menu_art = build_menu_art(&gpu, target_format, &detail_view, &clamp_sampler, &wrap_sampler, alpha_blend);

        let targets = Self::make_targets(device, size, render_scale, samples, &post_layout, &composite_layout, &clamp_sampler, &post_buf);

        Renderer {
            manual_gamma,
            samples,
            render_scale,
            map_half,
            vbuf,
            ibuf,
            ranges: lib.ranges,
            instance_buf,
            ghost_buf,
            particle_buf,
            decal_buf,
            ui_buf,
            globals_buf,
            light_buf,
            grid_buf,
            sun_buf,
            flash_buf,
            post_buf,
            ui_uniform,
            scene_bind,
            sun_bind,
            flash_bind,
            decal_bind,
            ui_bind,
            scene_pipe,
            ghost_pipe,
            ground_pipe,
            particle_pipe,
            shadow_pipe,
            decal_pipe,
            decal_fade_pipe,
            prefilter_pipe,
            down_pipe,
            up_pipe,
            composite_pipe,
            ui_pipe,
            post_layout,
            composite_layout,
            clamp_sampler,
            sun_view,
            flash_view,
            decal_view,
            targets,
            ranges_used: Vec::with_capacity(MESH_COUNT),
            grid_scratch: vec![0; GRID_DIM * GRID_DIM * GRID_STRIDE],
            instance_scratch: Vec::with_capacity(MAX_INSTANCES),
            fade_timer: 0.0,
            last_time: 0.0,
            decal_needs_clear: true,
            ground_tex,
            menu_art,
            stats: Default::default(),
            gpu,
        }
    }

    /// Replace the ground painting, for when a new map is generated.
    pub fn set_ground(&mut self, data: &[u8], size: u32) {
        write_ground(&self.gpu.queue, &self.ground_tex, data, size);
    }

    #[allow(clippy::too_many_arguments)]
    fn make_targets(
        device: &wgpu::Device,
        size: (u32, u32),
        scale: f32,
        samples: u32,
        post_layout: &wgpu::BindGroupLayout,
        composite_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        post_buf: &wgpu::Buffer,
    ) -> SizedTargets {
        let w = ((size.0 as f32 * scale) as u32).max(16);
        let h = ((size.1 as f32 * scale) as u32).max(16);
        let color = |label: &str, w: u32, h: u32, sc: u32, extra: wgpu::TextureUsages| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: sc,
                    dimension: wgpu::TextureDimension::D2,
                    format: HDR_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | extra,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let hdr = color("hdr", w, h, 1, wgpu::TextureUsages::TEXTURE_BINDING);
        let msaa = (samples > 1).then(|| color("hdr msaa", w, h, samples, wgpu::TextureUsages::empty()));
        let depth = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let bloom: Vec<_> = (0..BLOOM_LEVELS)
            .map(|i| color("bloom", (w >> (i + 1)).max(2), (h >> (i + 1)).max(2), 1, wgpu::TextureUsages::TEXTURE_BINDING))
            .collect();
        let post_bind = |view: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: post_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                ],
            })
        };
        let mut down_binds = vec![post_bind(&hdr)];
        for i in 0..BLOOM_LEVELS - 1 {
            down_binds.push(post_bind(&bloom[i]));
        }
        let up_binds = (1..BLOOM_LEVELS).map(|i| post_bind(&bloom[i])).collect();
        let composite_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: composite_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&hdr) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&bloom[0]) },
                wgpu::BindGroupEntry { binding: 3, resource: post_buf.as_entire_binding() },
            ],
        });
        SizedTargets { hdr, msaa, depth, bloom, down_binds, up_binds, composite_bind, size }
    }

    pub fn resize(&mut self, size: (u32, u32)) {
        if size.0 == 0 || size.1 == 0 || size == self.targets.size {
            return;
        }
        let device = self.gpu.device.clone();
        self.targets = Self::make_targets(
            &device,
            size,
            self.render_scale,
            self.samples,
            &self.post_layout,
            &self.composite_layout,
            &self.clamp_sampler,
            &self.post_buf,
        );
    }

    fn write_light_grid(&mut self, frame: &FrameData) -> [f32; 4] {
        let half = GRID_DIM as f32 * GRID_CELL * 0.5;
        let ox = frame.focus.x - half;
        let oz = frame.focus.z - half;
        self.grid_scratch.iter_mut().for_each(|c| *c = 0);
        let dim = GRID_DIM as i32;
        for (i, l) in frame.lights.iter().enumerate().take(MAX_LIGHTS) {
            let x0 = (((l.pos[0] - l.radius - ox) / GRID_CELL).floor() as i32).max(0);
            let x1 = (((l.pos[0] + l.radius - ox) / GRID_CELL).floor() as i32).min(dim - 1);
            let z0 = (((l.pos[2] - l.radius - oz) / GRID_CELL).floor() as i32).max(0);
            let z1 = (((l.pos[2] + l.radius - oz) / GRID_CELL).floor() as i32).min(dim - 1);
            for z in z0..=z1 {
                for x in x0..=x1 {
                    let base = (z * dim + x) as usize * GRID_STRIDE;
                    let n = self.grid_scratch[base] as usize;
                    if n < GRID_PER_CELL {
                        self.grid_scratch[base + 1 + n] = i as u32;
                        self.grid_scratch[base] = (n + 1) as u32;
                    }
                }
            }
        }
        self.gpu.queue.write_buffer(&self.grid_buf, 0, bytemuck::cast_slice(&self.grid_scratch));
        [ox, oz, GRID_CELL, GRID_DIM as f32]
    }

    /// Render one frame to `target`.
    pub fn render(&mut self, frame: &mut FrameData, target: &wgpu::TextureView, target_size: (u32, u32)) {
        let queue = self.gpu.queue.clone();
        let device = self.gpu.device.clone();

        if let Some(menu) = frame.menu {
            self.render_menu(frame, &menu, target, target_size);
            return;
        }

        // ---- gather instances
        self.instance_scratch.clear();
        self.ranges_used.clear();
        for (mesh, list) in frame.instances.iter().enumerate() {
            if list.is_empty() {
                continue;
            }
            let start = self.instance_scratch.len();
            let room = MAX_INSTANCES - start;
            if room == 0 {
                break;
            }
            let take = list.len().min(room);
            self.instance_scratch.extend_from_slice(&list[..take]);
            self.ranges_used.push((mesh as u32, start as u32, take as u32));
        }
        queue.write_buffer(&self.instance_buf, 0, bytemuck::cast_slice(&self.instance_scratch));

        let n_part = frame.particles.len().min(MAX_PARTICLES);
        if n_part > 0 {
            queue.write_buffer(&self.particle_buf, 0, bytemuck::cast_slice(&frame.particles[..n_part]));
        }
        let n_lights = frame.lights.len().min(MAX_LIGHTS);
        if n_lights > 0 {
            queue.write_buffer(&self.light_buf, 0, bytemuck::cast_slice(&frame.lights[..n_lights]));
        }
        self.stats = FrameStats { instances: self.instance_scratch.len(), lights: n_lights };
        let grid = self.write_light_grid(frame);

        // ---- matrices
        // Sun shadow frustum follows the focus, snapped to texels to prevent crawling.
        let extent = 48.0f32;
        let sun_eye = frame.focus + frame.sun_dir * 80.0;
        let up = if frame.sun_dir.y.abs() > 0.95 { Vec3::Z } else { Vec3::Y };
        let sun_view = look_at_rh(sun_eye, frame.focus, up);
        let sun_proj = ortho_rh(extent, 1.0, 180.0);
        let mut sun_vp = sun_proj * sun_view;
        let origin = sun_vp.project_point3(Vec3::ZERO);
        let texel = 2.0 / SHADOW_SIZE as f32;
        let snap_x = (origin.x / texel).round() * texel - origin.x;
        let snap_y = (origin.y / texel).round() * texel - origin.y;
        sun_vp = Mat4::from_translation(Vec3::new(snap_x, snap_y, 0.0)) * sun_vp;

        let (flash_vp, flash_pos, flash_dir, flash_col) = if let Some(f) = frame.flashlight {
            let fov = (f.cos_outer.clamp(-1.0, 1.0).acos() * 2.05).clamp(0.2, 2.8);
            let up = if f.dir.y.abs() > 0.95 { Vec3::Z } else { Vec3::Y };
            let v = look_to_rh(f.pos, f.dir, up);
            let p = perspective_rh(fov, 1.0, 0.15, f.range);
            (
                p * v,
                [f.pos.x, f.pos.y, f.pos.z, f.range],
                [f.dir.x, f.dir.y, f.dir.z, f.cos_outer],
                [f.color.x, f.color.y, f.color.z, f.cos_inner],
            )
        } else {
            (Mat4::IDENTITY, [0.0; 4], [0.0, -1.0, 0.0, 0.9], [0.0, 0.0, 0.0, 1.0])
        };
        let g = Globals {
            view_proj: frame.view_proj.to_cols_array_2d(),
            sun_vp: sun_vp.to_cols_array_2d(),
            flash_vp: flash_vp.to_cols_array_2d(),
            cam_pos: [frame.cam_pos.x, frame.cam_pos.y, frame.cam_pos.z, frame.time],
            sun_dir: frame.sun_dir.extend(0.0).to_array(),
            sun_color: frame.sun_color.extend(0.0).to_array(),
            amb_sky: frame.amb_sky.extend(0.0).to_array(),
            amb_ground: frame.amb_ground.extend(0.0).to_array(),
            fog: frame.fog.extend(1.0).to_array(),
            flash_pos,
            flash_dir,
            flash_color: flash_col,
            grid,
            map: [self.map_half, self.map_half * 1.8, 0.0, 0.0],
        };
        queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&g));
        queue.write_buffer(&self.sun_buf, 0, bytemuck::cast_slice(&sun_vp.to_cols_array_2d()));
        queue.write_buffer(&self.flash_buf, 0, bytemuck::cast_slice(&flash_vp.to_cols_array_2d()));
        let post = PostUniform {
            a: [frame.exposure, frame.bloom, frame.time % 100.0, frame.vignette],
            b: [frame.damage, frame.chroma, frame.desaturate, if self.manual_gamma { 1.0 } else { 0.0 }],
        };
        queue.write_buffer(&self.post_buf, 0, bytemuck::bytes_of(&post));
        queue.write_buffer(
            &self.ui_uniform,
            0,
            bytemuck::cast_slice(&[target_size.0 as f32, target_size.1 as f32, if self.manual_gamma { 1.0f32 } else { 0.0 }, 0.0]),
        );

        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });

        // ---- persistent decals
        let n_dec = frame.decals.len().min(MAX_DECALS);
        if n_dec > 0 {
            queue.write_buffer(&self.decal_buf, 0, bytemuck::cast_slice(&frame.decals[..n_dec]));
        }
        let clear = self.decal_needs_clear || frame.clear_decals;
        self.decal_needs_clear = false;
        frame.clear_decals = false;
        let dt = (frame.time - self.last_time).clamp(0.0, 1.0);
        self.last_time = frame.time;
        self.fade_timer += dt;
        let fade_now = self.fade_timer > 1.0;
        if n_dec > 0 || clear || fade_now {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("decals"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.decal_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if clear { wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT) } else { wgpu::LoadOp::Load },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if fade_now {
                self.fade_timer = 0.0;
                pass.set_pipeline(&self.decal_fade_pipe);
                pass.set_blend_constant(wgpu::Color { r: 0.004, g: 0.004, b: 0.004, a: 0.004 });
                pass.draw(0..3, 0..1);
            }
            if n_dec > 0 {
                pass.set_pipeline(&self.decal_pipe);
                pass.set_bind_group(0, &self.decal_bind, &[]);
                pass.set_vertex_buffer(0, self.decal_buf.slice(..));
                pass.draw(0..6, 0..n_dec as u32);
            }
        }
        frame.decals.clear();

        // ---- shadow maps
        let shadow_pass = |enc: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, bind: &wgpu::BindGroup, this: &Renderer| {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&this.shadow_pipe);
            pass.set_bind_group(0, bind, &[]);
            pass.set_vertex_buffer(0, this.vbuf.slice(..));
            pass.set_vertex_buffer(1, this.instance_buf.slice(..));
            pass.set_index_buffer(this.ibuf.slice(..), wgpu::IndexFormat::Uint32);
            for &(mesh, first, count) in &this.ranges_used {
                let (fi, ic) = this.ranges[mesh as usize];
                pass.draw_indexed(fi..fi + ic, 0, first..first + count);
            }
        };
        shadow_pass(&mut enc, &self.sun_view, &self.sun_bind, self);
        // Keep the flashlight map cleared even when off (it is not sampled then).
        if frame.flashlight.is_some() {
            shadow_pass(&mut enc, &self.flash_view, &self.flash_bind, self);
        }

        // ---- main HDR pass
        {
            let (view, resolve) = match &self.targets.msaa {
                Some(m) => (m, Some(&self.targets.hdr)),
                None => (&self.targets.hdr, None),
            };
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: resolve,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: frame.fog.x as f64, g: frame.fog.y as f64, b: frame.fog.z as f64, a: 1.0 }),
                        store: if resolve.is_some() { wgpu::StoreOp::Discard } else { wgpu::StoreOp::Store },
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.targets.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.scene_bind, &[]);
            // Opaque meshes first so early-z rejects ground fragments.
            pass.set_pipeline(&self.scene_pipe);
            pass.set_vertex_buffer(0, self.vbuf.slice(..));
            pass.set_vertex_buffer(1, self.instance_buf.slice(..));
            pass.set_index_buffer(self.ibuf.slice(..), wgpu::IndexFormat::Uint32);
            for &(mesh, first, count) in &self.ranges_used {
                let (fi, ic) = self.ranges[mesh as usize];
                pass.draw_indexed(fi..fi + ic, 0, first..first + count);
            }
            pass.set_pipeline(&self.ground_pipe);
            pass.draw(0..6, 0..1);
            if !frame.ghosts.is_empty() {
                let list: Vec<Instance> = frame.ghosts.iter().take(256).map(|g| g.1).collect();
                queue.write_buffer(&self.ghost_buf, 0, bytemuck::cast_slice(&list));
                pass.set_pipeline(&self.ghost_pipe);
                pass.set_vertex_buffer(1, self.ghost_buf.slice(..));
                for (i, (mesh, _)) in frame.ghosts.iter().take(256).enumerate() {
                    let (fi, ic) = self.ranges[*mesh];
                    pass.draw_indexed(fi..fi + ic, 0, i as u32..i as u32 + 1);
                }
            }
            if n_part > 0 {
                pass.set_pipeline(&self.particle_pipe);
                pass.set_vertex_buffer(0, self.particle_buf.slice(..));
                pass.draw(0..6, 0..n_part as u32);
            }
        }

        // ---- bloom pyramid
        for i in 0..BLOOM_LEVELS {
            let mut pass = color_pass(&mut enc, "bloom down", &self.targets.bloom[i], wgpu::LoadOp::Clear(wgpu::Color::BLACK));
            pass.set_pipeline(if i == 0 { &self.prefilter_pipe } else { &self.down_pipe });
            pass.set_bind_group(0, &self.targets.down_binds[i], &[]);
            pass.draw(0..3, 0..1);
        }
        for i in (0..BLOOM_LEVELS - 1).rev() {
            let mut pass = color_pass(&mut enc, "bloom up", &self.targets.bloom[i], wgpu::LoadOp::Load);
            pass.set_pipeline(&self.up_pipe);
            pass.set_bind_group(0, &self.targets.up_binds[i], &[]);
            pass.draw(0..3, 0..1);
        }

        // ---- composite and UI
        {
            let mut pass = color_pass(&mut enc, "composite", target, wgpu::LoadOp::Clear(wgpu::Color::BLACK));
            pass.set_pipeline(&self.composite_pipe);
            pass.set_bind_group(0, &self.targets.composite_bind, &[]);
            pass.draw(0..3, 0..1);
            let n = frame.ui.len().min(MAX_UI_VERTS);
            if n > 0 {
                queue.write_buffer(&self.ui_buf, 0, bytemuck::cast_slice(&frame.ui[..n]));
                pass.set_pipeline(&self.ui_pipe);
                pass.set_bind_group(0, &self.ui_bind, &[]);
                pass.set_vertex_buffer(0, self.ui_buf.slice(..));
                pass.draw(0..n as u32, 0..1);
            }
        }
        queue.submit([enc.finish()]);
    }

    fn render_menu(&mut self, frame: &FrameData, menu: &MenuView, target: &wgpu::TextureView, size: (u32, u32)) {
        let queue = &self.gpu.queue;
        let art = &self.menu_art;
        let gamma = if self.manual_gamma { 1.0f32 } else { 0.0 };
        let data: [f32; 12] = [
            size.0 as f32, size.1 as f32, art.aspect, menu.time,
            menu.mouse[0], menu.mouse[1], gamma, menu.dim,
            menu.logo[0], menu.logo[1], menu.logo[2], menu.logo[3],
        ];
        queue.write_buffer(&art.uniform, 0, bytemuck::cast_slice(&data));
        queue.write_buffer(&self.ui_uniform, 0, bytemuck::cast_slice(&[size.0 as f32, size.1 as f32, gamma, 0.0]));
        let n = frame.ui.len().min(MAX_UI_VERTS);
        if n > 0 {
            queue.write_buffer(&self.ui_buf, 0, bytemuck::cast_slice(&frame.ui[..n]));
        }
        let mut enc = self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("menu") });
        {
            let mut pass = color_pass(&mut enc, "menu", target, wgpu::LoadOp::Clear(wgpu::Color::BLACK));
            pass.set_bind_group(0, &art.bind, &[]);
            pass.set_pipeline(&art.bg_pipe);
            pass.draw(0..3, 0..1);
            if menu.logo[2] > 0.0 {
                pass.set_pipeline(&art.logo_pipe);
                pass.draw(0..6, 0..1);
            }
            if n > 0 {
                pass.set_pipeline(&self.ui_pipe);
                pass.set_bind_group(0, &self.ui_bind, &[]);
                pass.set_vertex_buffer(0, self.ui_buf.slice(..));
                pass.draw(0..n as u32, 0..1);
            }
        }
        queue.submit([enc.finish()]);
    }

    /// Read back an offscreen RGBA8 target; used by headless capture.
    pub fn read_texture(&self, tex: &wgpu::Texture) -> Vec<u8> {
        let (w, h) = (tex.width(), tex.height());
        let row = (w * 4).div_ceil(256) * 256;
        let buf = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.gpu.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.gpu.queue.submit([enc.finish()]);
        let slice = buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |r| r.expect("readback map failed"));
        let _ = self.gpu.device.poll(wgpu::PollType::wait_indefinitely());
        let data = slice.get_mapped_range().expect("map range");
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h as usize {
            let s = y * row as usize;
            out.extend_from_slice(&data[s..s + w as usize * 4]);
        }
        drop(data);
        buf.unmap();
        out
    }
}

/// Decodes an embedded PNG into RGBA8.
fn decode_png(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = dec.read_info().expect("embedded PNG is invalid");
    let mut buf = vec![0; reader.output_buffer_size().expect("embedded PNG is too large")];
    let info = reader.next_frame(&mut buf).expect("embedded PNG is invalid");
    let (w, h) = (info.width, info.height);
    let px = match info.color_type {
        png::ColorType::Rgba => buf[..(w * h * 4) as usize].to_vec(),
        png::ColorType::Rgb => buf[..(w * h * 3) as usize].chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        other => panic!("unexpected PNG format {other:?}"),
    };
    (w, h, px)
}

/// Box-filtered mip chain, so that the art stays clean in small windows.
fn mip_chain(w: u32, h: u32, base: Vec<u8>) -> Vec<Vec<u8>> {
    let mut levels = vec![base];
    let (mut lw, mut lh) = (w, h);
    while lw > 1 && lh > 1 && levels.len() < 5 {
        let prev = levels.last().unwrap();
        let (nw, nh) = (lw / 2, lh / 2);
        let mut next = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh as usize {
            for x in 0..nw as usize {
                for c in 0..4 {
                    let g = |a: usize, b: usize| prev[(b * lw as usize + a) * 4 + c] as u32;
                    let v = g(x * 2, y * 2) + g(x * 2 + 1, y * 2) + g(x * 2, y * 2 + 1) + g(x * 2 + 1, y * 2 + 1);
                    next[(y * nw as usize + x) * 4 + c] = (v / 4) as u8;
                }
            }
        }
        levels.push(next);
        lw = nw;
        lh = nh;
    }
    levels
}

fn upload_art(gpu: &Gpu, label: &str, bytes: &[u8]) -> (wgpu::TextureView, f32) {
    let (w, h, px) = decode_png(bytes);
    // upload_texture halves both dimensions per level, as mip_chain does.
    let levels = mip_chain(w, h, px);
    (upload_texture(gpu, label, w, h, wgpu::TextureFormat::Rgba8UnormSrgb, 4, &levels), w as f32 / h as f32)
}

fn build_menu_art(
    gpu: &Gpu,
    format: wgpu::TextureFormat,
    detail: &wgpu::TextureView,
    clamp: &wgpu::Sampler,
    wrap: &wgpu::Sampler,
    blend: wgpu::BlendState,
) -> MenuArt {
    use wgpu::ShaderStages as St;
    let device = &gpu.device;
    let (bg, aspect) = upload_art(gpu, "menu background", include_bytes!("../../assets/menu_bg.png"));
    let (logo, _) = upload_art(gpu, "menu logo", include_bytes!("../../assets/menu_logo.png"));
    let uniform = dyn_buffer(device, "menu uniform", 48, wgpu::BufferUsages::UNIFORM);
    let float = wgpu::TextureSampleType::Float { filterable: true };
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("menu"),
        entries: &[
            uniform_entry(0, St::VERTEX_FRAGMENT),
            tex_entry(1, St::FRAGMENT, float),
            tex_entry(2, St::FRAGMENT, float),
            tex_entry(3, St::FRAGMENT, float),
            sampler_entry(4, St::FRAGMENT, wgpu::SamplerBindingType::Filtering),
            sampler_entry(5, St::FRAGMENT, wgpu::SamplerBindingType::Filtering),
        ],
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("menu"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&bg) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&logo) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(detail) },
            wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(clamp) },
            wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::Sampler(wrap) },
        ],
    });
    let sh = device.create_shader_module(wgpu::include_wgsl!("menu.wgsl"));
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
    let pipe = |vs: &str, fs: &str, blend: Option<wgpu::BlendState>| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("menu"),
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &sh, entry_point: Some(vs), compilation_options: Default::default(), buffers: &[] },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &sh,
                entry_point: Some(fs),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let bg_pipe = pipe("vs_bg", "fs_bg", None);
    let logo_pipe = pipe("vs_logo", "fs_logo", Some(blend));
    MenuArt { uniform, bind, bg_pipe, logo_pipe, aspect }
}

fn color_pass<'e>(enc: &'e mut wgpu::CommandEncoder, label: &str, view: &wgpu::TextureView, load: wgpu::LoadOp<wgpu::Color>) -> wgpu::RenderPass<'e> {
    enc.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

// Camera matrices (right-handed, depth range 0..1).
pub fn look_to_rh(eye: Vec3, dir: Vec3, up: Vec3) -> Mat4 {
    let f = dir.normalize();
    let s = f.cross(up).normalize();
    let u = s.cross(f);
    Mat4::from_cols(
        glam::vec4(s.x, u.x, -f.x, 0.0),
        glam::vec4(s.y, u.y, -f.y, 0.0),
        glam::vec4(s.z, u.z, -f.z, 0.0),
        glam::vec4(-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0),
    )
}

pub fn look_at_rh(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
    look_to_rh(eye, target - eye, up)
}

pub fn perspective_rh(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let h = 1.0 / (fov_y * 0.5).tan();
    let r = far / (near - far);
    Mat4::from_cols(
        glam::vec4(h / aspect, 0.0, 0.0, 0.0),
        glam::vec4(0.0, h, 0.0, 0.0),
        glam::vec4(0.0, 0.0, r, -1.0),
        glam::vec4(0.0, 0.0, r * near, 0.0),
    )
}

pub fn ortho_rh(half: f32, near: f32, far: f32) -> Mat4 {
    let r = 1.0 / (near - far);
    Mat4::from_cols(
        glam::vec4(1.0 / half, 0.0, 0.0, 0.0),
        glam::vec4(0.0, 1.0 / half, 0.0, 0.0),
        glam::vec4(0.0, 0.0, r, 0.0),
        glam::vec4(0.0, 0.0, r * near, 1.0),
    )
}
