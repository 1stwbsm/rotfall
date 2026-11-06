//! ROTFALL: a top-down wave-survival zombie shooter.
// On Windows, double-clicking the game should not open a console window behind it.
#![cfg_attr(windows, windows_subsystem = "windows")]
// The browser build leaves out the command-line tools and the server.
#![cfg_attr(target_arch = "wasm32", allow(dead_code, unused))]
mod audio;
mod game;
mod ragdoll;
mod render;
mod storage;
mod util;
mod wire;

use audio::{Audio, Track};
use game::app::{Action, App, RawInput};
use game::net::ServerConfig;
use render::{FrameData, Gpu, Renderer};
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

// The ground is painted on the CPU when a map starts, single-threaded in a browser.
#[cfg(not(target_arch = "wasm32"))]
const GROUND_TEX: u32 = 2048;
#[cfg(target_arch = "wasm32")]
const GROUND_TEX: u32 = 1024;

struct Options {
    capture: Option<String>,
    times: Vec<f32>,
    wave: u32,
    scrap: u32,
    size: (u32, u32),
    bench: f32,
    scale: f32,
    render_music: Option<(String, f32, f32, String)>,
    screen: String,
    seed: u64,
    no_audio: bool,
    vsync: bool,
    server: bool,
    cfg: ServerConfig,
    load: Option<std::path::PathBuf>,
    connect: Option<String>,
    host_capture: bool,
    name: String,
    log: bool,
    autoplay: bool,
    force_touch: bool,
    export_ost: Option<String>,
}

fn start_seed() -> u64 {
    #[cfg(target_arch = "wasm32")]
    return js_sys::Date::now() as u64;
    #[cfg(not(target_arch = "wasm32"))]
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(1)
}

fn parse_args() -> Options {
    let mut o = Options {
        capture: None,
        times: vec![10.0],
        wave: 1,
        scrap: 4000,
        size: (1600, 900),
        bench: 0.0,
        scale: 1.0,
        render_music: None,
        screen: "play".into(),
        seed: start_seed(),
        no_audio: false,
        vsync: true,
        server: false,
        cfg: ServerConfig::default(),
        load: None,
        connect: None,
        host_capture: false,
        name: "SURVIVOR".into(),
        log: std::env::var_os("ROTFALL_LOG").is_some(),
        autoplay: false,
        force_touch: false,
        export_ost: None,
    };
    #[cfg(not(target_arch = "wasm32"))]
    let a: Vec<String> = std::env::args().skip(1).collect();
    // A page has no command line, so its URL takes the same options: ?no-audio&scale=0.7
    #[cfg(target_arch = "wasm32")]
    let a: Vec<String> = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .unwrap_or_default()
        .trim_start_matches('?')
        .split('&')
        .filter(|p| !p.is_empty())
        .flat_map(|p| {
            let mut kv = p.splitn(2, '=');
            let k = format!("--{}", kv.next().unwrap_or(""));
            std::iter::once(k).chain(kv.next().map(str::to_string))
        })
        .collect();
    let mut i = 0;
    let next = |i: &mut usize| -> String {
        *i += 1;
        a.get(*i).cloned().unwrap_or_default()
    };
    while i < a.len() {
        match a[i].as_str() {
            "--log" => o.log = true,
            "--autoplay" => o.autoplay = true,
            "--touch" => o.force_touch = true,
            "--export-ost" => o.export_ost = Some(next(&mut i)),
            "--capture" => o.capture = Some(next(&mut i)),
            "--times" => o.times = next(&mut i).split(',').filter_map(|t| t.parse().ok()).collect(),
            "--wave" => o.wave = next(&mut i).parse().unwrap_or(1),
            "--scrap" => o.scrap = next(&mut i).parse().unwrap_or(4000),
            "--size" => {
                let v = next(&mut i);
                if let Some((w, h)) = v.split_once('x') {
                    o.size = (w.parse().unwrap_or(1600), h.parse().unwrap_or(900));
                }
            }
            "--bench" => o.bench = next(&mut i).parse().unwrap_or(20.0),
            "--scale" => o.scale = next(&mut i).parse::<f32>().unwrap_or(1.0).clamp(0.3, 2.0),
            "--screen" => o.screen = next(&mut i),
            "--seed" => o.seed = next(&mut i).parse().unwrap_or(1),
            "--no-audio" => o.no_audio = true,
            "--no-vsync" => o.vsync = false,
            "--server" => o.server = true,
            "--port" => o.cfg.port = next(&mut i).parse().unwrap_or(game::net::DEFAULT_PORT),
            "--name" => {
                let v = next(&mut i);
                o.name = v.clone();
                o.cfg.name = v;
            }
            "--max-players" => o.cfg.max_players = next(&mut i).parse().unwrap_or(4),
            "--password" => o.cfg.password = next(&mut i),
            "--permadeath" => o.cfg.permadeath = true,
            "--load" => o.load = Some(next(&mut i).into()),
            "--connect" => o.connect = Some(next(&mut i)),
            "--host" => o.host_capture = true,
            "--render-music" => {
                let t = next(&mut i);
                let secs = next(&mut i).parse().unwrap_or(30.0);
                let inten = next(&mut i).parse().unwrap_or(1.0);
                let out = next(&mut i);
                o.render_music = Some((t, secs, inten, out));
            }
            "--help" | "-h" => {
                println!("rotfall [--seed N] [--scale 0.3-2.0] [--no-audio] [--no-vsync]");
                println!("Dedicated server: rotfall --server [--port N] [--name TEXT] [--max-players N] [--password TEXT] [--permadeath] [--load FILE]");
                println!("Tools: --capture PREFIX --times 5,20 --wave N --scrap N --size WxH --screen play|title|build|armory|help|menu_new|menu_multi|menu_host|menu_join|menu_records|menu_options|menu_manual|pause");
                println!("       --capture with --host [--port N] or --connect ADDR [--name X] plays a real network session");
                println!("       --bench SECONDS --wave N    (GPU and simulation timing on the current machine)");
                println!("       --render-music menu|build|a|b|c|boss|over SECONDS INTENSITY OUT.wav");
                std::process::exit(0);
            }
            other => eprintln!("ignoring unknown option {other}"),
        }
        i += 1;
    }
    o
}

struct Running {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    frame: FrameData,
    app: App,
    raw: RawInput,
    last: Instant,
    smooth_dt: f32,
    fps_acc: f32,
    fps_frames: u32,
    log: bool,
    /// Milliseconds spent since the last log line: simulating, building the frame, submitting it.
    cpu: [f32; 3],
}

struct GameHandler {
    options: Options,
    running: Option<Running>,
    /// The window while its renderer is being set up, which is asynchronous in a browser.
    window: Option<Arc<Window>>,
    #[cfg(target_arch = "wasm32")]
    pending: std::rc::Rc<std::cell::RefCell<Option<Running>>>,
}

/// Builds the GPU, renderer and game for a window. Shared by the desktop and the browser.
async fn start_game(window: Arc<Window>, instance: wgpu::Instance, scale: f32, seed: u64, no_audio: bool, vsync: bool, log: bool, demo: Option<(u32, u32)>, force_touch: bool) -> Result<Running, String> {
    let surface = instance.create_surface(window.clone()).map_err(|e| format!("could not create a drawing surface: {e}"))?;
    let gpu = Gpu::new(&instance, Some(&surface)).await.map_err(|e| {
        format!("Could not start graphics: {e}\n\nThe game needs a GPU driver with Vulkan, or OpenGL 4.3 or newer. Updating your graphics driver may help.")
    })?;
    let size = initial_size(&window);
    let caps = surface.get_capabilities(&gpu.adapter);
    let format = caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(caps.formats[0]);
    let present_mode = if vsync { wgpu::PresentMode::AutoVsync } else { wgpu::PresentMode::AutoNoVsync };
    let mut config = surface.get_default_config(&gpu.adapter, size.0, size.1).ok_or("the drawing surface is not supported")?;
    config.format = format;
    config.present_mode = present_mode;
    surface.configure(&gpu.device, &config);

    let audio = if no_audio { Audio::silent() } else { Audio::start() };
    let mut app = App::new(seed, audio);
    let (screen, touch_capable) = device_info(&window);
    app.detect_device(screen, touch_capable, force_touch);
    // Lets the bot play in a real window, for measuring a full horde.
    if let Some((wave, scrap)) = demo {
        app.start_autoplay(wave, scrap);
    }
    let ground = app.world.map.bake_ground(GROUND_TEX);
    let renderer = Renderer::new(gpu, format, (config.width, config.height), scale, game::map::HALF, (&ground, GROUND_TEX));
    Ok(Running {
        window,
        surface,
        config,
        renderer,
        frame: FrameData::new(),
        app,
        raw: RawInput::default(),
        last: Instant::now(),
        smooth_dt: 1.0 / 60.0,
        fps_acc: 0.0,
        fps_frames: 0,
        log,
        cpu: [0.0; 3],
    })
}

/// The icon as pixel data, which is how Windows and X11 receive it.
#[cfg(not(target_arch = "wasm32"))]
fn window_icon() -> Option<winit::window::Icon> {
    let (w, h, rgba) = render::decode_png(include_bytes!("../assets/icon_80x80.png"));
    winit::window::Icon::from_rgba(rgba, w, h).ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn window_attributes() -> winit::window::WindowAttributes {
    let attrs = Window::default_attributes().with_title("ROTFALL").with_inner_size(PhysicalSize::new(1600, 900)).with_window_icon(window_icon());
    // Windows keeps two icons: the one above is the title bar's, this one the taskbar's.
    #[cfg(windows)]
    let attrs = {
        use winit::platform::windows::WindowAttributesExtWindows;
        attrs.with_taskbar_icon(window_icon())
    };
    // Wayland has no window icons at all. The compositor finds one through the app ID.
    #[cfg(target_os = "linux")]
    let attrs = {
        use winit::platform::wayland::WindowAttributesExtWayland;
        use winit::platform::x11::WindowAttributesExtX11;
        let attrs = WindowAttributesExtWayland::with_name(attrs, APP_ID, APP_ID);
        WindowAttributesExtX11::with_name(attrs, APP_ID, APP_ID)
    };
    attrs
}

#[cfg(target_os = "linux")]
const APP_ID: &str = "rotfall";

/// Linux desktops look up a running app's icon through a desktop entry, so one has to exist.
/// `NoDisplay` keeps it out of the application menu: this only teaches the desktop the icon.
#[cfg(target_os = "linux")]
fn install_desktop_entry() {
    let Some(data) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()).map(std::path::PathBuf::from).or_else(|| {
        std::env::var_os("HOME").filter(|v| !v.is_empty()).map(|h| std::path::PathBuf::from(h).join(".local/share"))
    }) else {
        return;
    };
    let Ok(exe) = std::env::current_exe() else { return };
    // Quoted as the desktop entry format asks, since the game may live under a path with spaces.
    let exec = exe.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName=ROTFALL\nComment=Top-down zombie wave shooter\nExec=\"{exec}\"\nIcon={APP_ID}\nStartupWMClass={APP_ID}\nCategories=Game;ActionGame;\nNoDisplay=true\n"
    );
    let write = |path: std::path::PathBuf, bytes: &[u8]| {
        if std::fs::read(&path).is_ok_and(|old| old == bytes) {
            return;
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&path, bytes);
    };
    write(data.join("icons/hicolor/256x256/apps/rotfall.png"), include_bytes!("../assets/icon_256.png"));
    write(data.join(format!("applications/{APP_ID}.desktop")), entry.as_bytes());
}

/// The screen's size in logical pixels. Desktop systems do not say whether they have touch.
#[cfg(not(target_arch = "wasm32"))]
fn device_info(window: &Window) -> ((f32, f32), bool) {
    let screen = window.current_monitor().map_or((1920.0, 1080.0), |m| {
        let s = m.size().to_logical::<f32>(m.scale_factor());
        (s.width, s.height)
    });
    (screen, false)
}

#[cfg(not(target_arch = "wasm32"))]
fn initial_size(window: &Window) -> (u32, u32) {
    let s = window.inner_size();
    (s.width.max(1), s.height.max(1))
}

/// A canvas has no size until layout has run, so ask the browser about the page instead.
#[cfg(target_arch = "wasm32")]
fn initial_size(_window: &Window) -> (u32, u32) {
    let w = web_sys::window().expect("no browser window");
    let dpr = w.device_pixel_ratio();
    let get = |v: Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>| v.ok().and_then(|x| x.as_f64()).unwrap_or(800.0);
    (((get(w.inner_width()) * dpr) as u32).clamp(1, MAX_WEB_SIZE), ((get(w.inner_height()) * dpr) as u32).clamp(1, MAX_WEB_SIZE))
}

/// The screen's size in logical pixels, and whether it reports touch.
#[cfg(target_arch = "wasm32")]
fn device_info(_window: &Window) -> ((f32, f32), bool) {
    let w = web_sys::window().expect("no browser window");
    let screen = w.screen().ok().map_or((1920.0, 1080.0), |s| (s.width().unwrap_or(1920) as f32, s.height().unwrap_or(1080) as f32));
    (screen, w.navigator().max_touch_points() > 0)
}

/// WebGPU only promises textures this large.
#[cfg(target_arch = "wasm32")]
const MAX_WEB_SIZE: u32 = 8192;

impl ApplicationHandler for GameHandler {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.running.is_some() || self.window.is_some() {
            return;
        }
        let o = &self.options;
        #[cfg(not(target_arch = "wasm32"))]
        let attrs = window_attributes();
        #[cfg(target_arch = "wasm32")]
        let attrs = {
            use winit::platform::web::WindowAttributesExtWebSys;
            Window::default_attributes().with_title("ROTFALL").with_append(true)
        };
        let window = Arc::new(el.create_window(attrs).expect("window creation failed"));
        window.set_cursor_visible(false);
        // A browser paces frames itself; polling would just spin its main thread between them.
        #[cfg(not(target_arch = "wasm32"))]
        el.set_control_flow(ControlFlow::Poll);
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(Box::new(el.owned_display_handle())).with_env());
        let (scale, seed, no_audio, vsync, log) = (o.scale, o.seed, o.no_audio, o.vsync, o.log);
        let demo = o.autoplay.then_some((o.wave, o.scrap));
        let force_touch = o.force_touch;
        #[cfg(not(target_arch = "wasm32"))]
        match pollster::block_on(start_game(window, instance, scale, seed, no_audio, vsync, log, demo, force_touch)) {
            Ok(r) => self.running = Some(r),
            Err(e) => fatal(&e),
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.window = Some(window.clone());
            let pending = self.pending.clone();
            // Hold big screens to about 1080p: browsers have less headroom than native code.
            let tall = initial_size(&window).1 as f32;
            // Phones have tiny screens with huge pixel counts and far less GPU to drive them.
            let (_, touch_capable) = device_info(&window);
            let cap = if touch_capable { 720.0 } else { 1080.0 };
            let scale = (cap / tall).min(1.0) * scale;
            wasm_bindgen_futures::spawn_local(async move {
                let waker = window.clone();
                match start_game(window, instance, scale, seed, no_audio, vsync, log, demo, force_touch).await {
                    Ok(r) => {
                        web_ready();
                        *pending.borrow_mut() = Some(r);
                        waker.request_redraw();
                    }
                    Err(e) => fatal(&e),
                }
            });
        }
    }

    /// A browser tab closing or going away: the last chance to keep the run.
    fn suspended(&mut self, _el: &ActiveEventLoop) {
        if let Some(r) = self.running.as_mut() {
            r.app.shutdown();
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(r) = self.running.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => {
                r.app.shutdown();
                el.exit();
            }
            WindowEvent::Resized(s) => {
                if s.width > 0 && s.height > 0 {
                    #[cfg(target_arch = "wasm32")]
                    let s = PhysicalSize::new(s.width.min(MAX_WEB_SIZE), s.height.min(MAX_WEB_SIZE));
                    r.config.width = s.width;
                    r.config.height = s.height;
                    r.surface.configure(&r.renderer.gpu.device, &r.config);
                    r.renderer.resize((s.width, s.height));
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    if let Some(t) = &event.text {
                        r.raw.typed.extend(t.chars());
                    }
                }
                if let PhysicalKey::Code(code) = event.physical_key {
                    // Both Enter keys mean the same thing everywhere in the game.
                    let code = if code == KeyCode::NumpadEnter { KeyCode::Enter } else { code };
                    match event.state {
                        ElementState::Pressed => {
                            if !event.repeat {
                                r.raw.pressed.push(code);
                            }
                            r.raw.down.insert(code);
                        }
                        ElementState::Released => {
                            r.raw.down.remove(&code);
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                r.raw.mouse = (position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == ElementState::Pressed;
                match button {
                    MouseButton::Left => {
                        r.raw.lmb = pressed;
                        if pressed {
                            r.raw.lmb_pressed = true;
                        }
                    }
                    MouseButton::Right => {
                        r.raw.rmb = pressed;
                        if pressed {
                            r.raw.rmb_pressed = true;
                        }
                    }
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let d = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 30.0) as f32,
                };
                if d != 0.0 {
                    r.raw.wheel += d.signum() as i32;
                }
            }
            WindowEvent::Touch(t) => {
                use game::touch::{TouchEv, TouchPhase};
                let phase = match t.phase {
                    winit::event::TouchPhase::Started => TouchPhase::Start,
                    winit::event::TouchPhase::Moved => TouchPhase::Move,
                    _ => TouchPhase::End,
                };
                r.raw.touches.push(TouchEv { id: t.id, phase, x: t.location.x as f32, y: t.location.y as f32 });
            }
            WindowEvent::Focused(false) => {
                r.raw.down.clear();
                r.raw.lmb = false;
                r.raw.rmb = false;
            }
            WindowEvent::RedrawRequested => r.tick(el),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        #[cfg(target_arch = "wasm32")]
        if self.running.is_none() {
            self.running = self.pending.borrow_mut().take();
        }
        if let Some(r) = &self.running {
            r.window.request_redraw();
        }
    }
}

impl Running {
    fn tick(&mut self, el: &ActiveEventLoop) {
        let now = Instant::now();
        let real_dt = (now - self.last).as_secs_f32();
        self.last = now;
        self.fps_acc += real_dt;
        self.fps_frames += 1;
        if self.fps_acc >= 0.5 {
            self.app.fps = self.fps_frames as f32 / self.fps_acc;
            self.app.frame_ms = self.fps_acc * 1000.0 / self.fps_frames as f32;
            if self.log {
                let n = self.fps_frames as f32;
                let line = format!(
                    "fps {:.0} | sim {:.1} scene {:.1} submit {:.1} ms | wave {} zombies {} instances {} size {:?}",
                    self.app.fps, self.cpu[0] / n, self.cpu[1] / n, self.cpu[2] / n,
                    self.app.world.wave, self.app.world.zombies.len(), self.renderer.stats.instances, (self.config.width, self.config.height)
                );
                #[cfg(target_arch = "wasm32")]
                web_sys::console::warn_1(&line.into());
                #[cfg(not(target_arch = "wasm32"))]
                eprintln!("{line}");
                self.cpu = [0.0; 3];
            }
            self.fps_acc = 0.0;
            self.fps_frames = 0;
        }
        // A lightly filtered timestep hides scheduler jitter; long stalls are clamped.
        self.smooth_dt += (real_dt.min(0.05) - self.smooth_dt) * 0.25;
        let size = (self.config.width, self.config.height);
        self.app.begin_frame(&mut self.raw, size);
        let t0 = Instant::now();
        self.app.update(self.smooth_dt, &self.raw, size);
        let t1 = Instant::now();
        self.cpu[0] += (t1 - t0).as_secs_f32() * 1000.0;
        if std::mem::take(&mut self.app.ground_dirty) {
            let ground = self.app.world.map.bake_ground(GROUND_TEX);
            self.renderer.set_ground(&ground, GROUND_TEX);
        }
        self.app.draw(&mut self.frame, &self.raw, size);
        let t2 = Instant::now();
        self.cpu[1] += (t2 - t1).as_secs_f32() * 1000.0;
        self.raw.end_frame();
        if let Some(full) = self.app.want_fullscreen.take() {
            self.window.set_fullscreen(full.then_some(winit::window::Fullscreen::Borderless(None)));
        }
        if self.app.quit {
            self.app.shutdown();
            el.exit();
            return;
        }
        let tex = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.renderer.gpu.device, &self.config);
                return;
            }
            _ => return,
        };
        let view = tex.texture.create_view(&Default::default());
        self.renderer.render(&mut self.frame, &view, size);
        self.cpu[2] += t2.elapsed().as_secs_f32() * 1000.0;
        self.window.pre_present_notify();
        self.renderer.gpu.queue.present(tex);
    }
}

/// Renders the running game to PNG files with no window at all.
fn run_capture(o: &Options) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle().with_env());
    let gpu = pollster::block_on(Gpu::new(&instance, None)).expect("no GPU");
    // Captures keep their data apart from the player's saves.
    let dir = std::env::temp_dir().join(format!("rotfall-capture-{}", std::process::id()));
    let mut app = App::with_dir(o.seed, Audio::silent(), dir.clone());
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("capture"),
        size: wgpu::Extent3d { width: o.size.0, height: o.size.1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&Default::default());
    let ground = app.world.map.bake_ground(GROUND_TEX);
    let mut renderer = Renderer::new(gpu, format, o.size, o.scale, game::map::HALF, (&ground, GROUND_TEX));
    let mut frame = FrameData::new();
    let mut raw = RawInput { mouse: (o.size.0 as f32 * 0.5, o.size.1 as f32 * 0.5), ..Default::default() };

    use game::app::Page;
    let menu_page = match o.screen.as_str() {
        "title" => Some(Page::Main),
        "menu_new" => Some(Page::NewGame),
        "menu_multi" => Some(Page::Multiplayer),
        "menu_host" => Some(Page::Host),
        "menu_join" => Some(Page::Join),
        "menu_records" => Some(Page::Records),
        "menu_options" => Some(Page::Options),
        "menu_manual" => Some(Page::Manual),
        "menu_extras" => Some(Page::Extras),
        "menu_gallery" => Some(Page::Gallery),
        "menu_ost" => Some(Page::Soundtrack),
        _ => None,
    };
    let mut scripted_client = false;
    if let Some(p) = menu_page {
        app.page = p;
    } else if o.host_capture {
        app.settings.port = o.cfg.port.to_string();
        app.settings.name = "HOST-BOT".into();
        app.action = Some(Action::StartHost);
        app.update(0.016, &raw, o.size);
        app.autoplay = true;
        app.panel_open = o.screen == "panel";
        app.world.scrap = o.scrap;
        app.world.wave = o.wave.saturating_sub(1);
        app.world.invincible = true;
        app.world.hold_timer = false;
        app.world.phase = game::Phase::Build { timer: 3.0 };
    } else if let Some(addr) = &o.connect {
        app.settings.address = addr.clone();
        app.settings.name = o.name.clone();
        app.action = Some(Action::Join);
        app.update(0.016, &raw, o.size);
        let start = Instant::now();
        while app.screen == game::app::Screen::Title {
            app.update(0.016, &raw, o.size);
            std::thread::sleep(std::time::Duration::from_millis(5));
            if start.elapsed().as_secs() > 20 {
                eprintln!("could not join: {:?}", app.menu_error);
                std::process::exit(1);
            }
        }
        println!("joined as player {}", app.world.local);
        app.panel_open = false;
        scripted_client = true;
    } else {
        app.start_autoplay(o.wave, o.scrap);
    }
    if std::mem::take(&mut app.ground_dirty) {
        let ground = app.world.map.bake_ground(GROUND_TEX);
        renderer.set_ground(&ground, GROUND_TEX);
    }
    match o.screen.as_str() {
        "build" => {
            app.build_sel = Some(6);
            app.autoplay = true;
        }
        "armory" => app.armory_open = true,
        "touch" => {
            app.autoplay = false;
            app.touch.enabled = true;
        }
        "touch_prompt" => {
            app.detect_device((390.0, 844.0), true, false);
        }
        "dead_rot" | "dead_zombie" | "dead_blast" => {
            use game::player::DeathCause as C;
            app.autoplay = false;
            let cause = match o.screen.as_str() {
                "dead_rot" => C::Regular,
                "dead_zombie" => C::Zombie,
                _ => C::Explosion,
            };
            app.world.invincible = false;
            let local = app.world.local;
            app.world.hurt_player(local, 1000.0, glam::Vec3::X, cause);
        }
        "knocked" => {
            app.autoplay = false;
            app.world.invincible = false;
            let at = app.world.me().pos + glam::Vec3::X;
            app.world.explosions.push(game::combat::Explosion { pos: at, radius: 7.0, damage: 40.0, from_enemy: false, force: 22.0, spares_players: false, barrel: false });
        }
        "help" => app.help_open = true,
        "pause" => {
            app.autoplay = false;
            app.screen = game::app::Screen::Paused;
        }
        "over_perm" => {
            app.action = Some(Action::NewGame { permadeath: true });
            app.update(0.016, &raw, o.size);
            app.autoplay = false;
            app.world.wave = 11;
            app.world.stats.wave_reached = 11;
            app.world.stats.kills = 1873;
            let local = app.world.local;
            app.world.hurt_player(local, 1000.0, glam::Vec3::ZERO, game::player::DeathCause::Explosion);
        }
        _ => {}
    }
    let dt = 1.0 / 60.0;
    let last = o.times.iter().copied().fold(0.0, f32::max);
    let mut next = 0;
    let mut times = o.times.clone();
    times.sort_by(f32::total_cmp);
    let (mut sim_total, mut sim_max) = (0.0f32, 0.0f32);
    let mut frames = 0;
    let mut t = 0.0;
    let mut gpu_since = 0;
    let realtime = o.host_capture || scripted_client;
    let began = Instant::now();
    while t <= last + dt {
        let start = Instant::now();
        if scripted_client {
            // Walk in circles around the base, shooting outward: real keys through the real path.
            use winit::keyboard::KeyCode as K;
            raw.down.clear();
            let k = ((t * 0.5) as u32) % 4;
            raw.down.insert([K::KeyW, K::KeyA, K::KeyS, K::KeyD][k as usize]);
            raw.lmb = true;
            let a = t * 0.9;
            raw.mouse = (o.size.0 as f32 * (0.5 + 0.3 * a.cos()), o.size.1 as f32 * (0.5 + 0.3 * a.sin()));
        }
        app.update(dt, &raw, o.size);
        let ms = start.elapsed().as_secs_f32() * 1000.0;
        sim_total += ms;
        sim_max = sim_max.max(ms);
        frames += 1;
        t += dt;
        gpu_since += 1;
        let snapshot = next < times.len() && t >= times[next];
        if std::mem::take(&mut app.ground_dirty) {
            let ground = app.world.map.bake_ground(GROUND_TEX);
            renderer.set_ground(&ground, GROUND_TEX);
        }
        if gpu_since >= 3 || snapshot {
            gpu_since = 0;
            app.draw(&mut frame, &raw, o.size);
            renderer.render(&mut frame, &view, o.size);
        }
        // A network session has another process on the far end, so it must run at real speed.
        if realtime {
            let due = began + std::time::Duration::from_secs_f32(t);
            let now = Instant::now();
            if due > now {
                std::thread::sleep(due - now);
            }
        }
        if snapshot {
            let px = renderer.read_texture(&tex);
            let name = format!("{}_{:03}.png", o.capture.as_deref().unwrap_or("capture"), times[next] as u32);
            util::write_png(&name, o.size.0, o.size.1, &px).expect("write png");
            let w = &app.world;
            println!(
                "{name}: t={t:.1} wave={} zombies={} ragdolls={} particles={} instances={} lights={} kills={} structures={} scrap={}",
                w.wave, w.zombies.len(), w.ragdolls.bodies.len(), w.fx.p.len(), renderer.stats.instances, renderer.stats.lights, w.stats.kills, w.structures.len(), w.scrap
            );
            next += 1;
        }
        raw.end_frame();
    }
    println!("simulation: avg {:.2} ms, worst {:.2} ms per step", sim_total / frames as f32, sim_max);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Measures real frame throughput, GPU included, on this machine.
fn run_bench(o: &Options) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle().with_env());
    let gpu = pollster::block_on(Gpu::new(&instance, None)).expect("no GPU");
    let mut app = App::new(o.seed, Audio::silent());
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bench"),
        size: wgpu::Extent3d { width: o.size.0, height: o.size.1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = tex.create_view(&Default::default());
    let ground = app.world.map.bake_ground(GROUND_TEX);
    let mut renderer = Renderer::new(gpu, format, o.size, o.scale, game::map::HALF, (&ground, GROUND_TEX));
    let mut frame = FrameData::new();
    let raw = RawInput::default();
    app.start_autoplay(o.wave, o.scrap);
    let ground = app.world.map.bake_ground(GROUND_TEX);
    renderer.set_ground(&ground, GROUND_TEX);
    let dt = 1.0 / 60.0;
    let (mut sim, mut build, mut gpu_t) = (0.0f32, 0.0f32, 0.0f32);
    let (mut n, mut worst_frame) = (0, 0.0f32);
    let (mut max_z, mut max_i) = (0, 0);
    let warm = 8.0;
    let mut t = 0.0;
    while t < o.bench + warm {
        let a = Instant::now();
        app.update(dt, &raw, o.size);
        let b = Instant::now();
        app.draw(&mut frame, &raw, o.size);
        let c = Instant::now();
        renderer.render(&mut frame, &view, o.size);
        let _ = renderer.gpu.device.poll(wgpu::PollType::wait_indefinitely());
        let d = Instant::now();
        t += dt;
        if t > warm {
            sim += (b - a).as_secs_f32();
            build += (c - b).as_secs_f32();
            gpu_t += (d - c).as_secs_f32();
            worst_frame = worst_frame.max((d - a).as_secs_f32());
            n += 1;
            max_z = max_z.max(app.world.zombies.len());
            max_i = max_i.max(renderer.stats.instances);
        }
    }
    let k = 1000.0 / n as f32;
    println!(
        "{}x{} scale {}: sim {:.2} ms | scene+ui {:.2} ms | gpu {:.2} ms | avg fps {:.0} | worst frame {:.1} ms | peak zombies {} | peak instances {}",
        o.size.0, o.size.1, o.scale, sim * k, build * k, gpu_t * k, 1000.0 / ((sim + build + gpu_t) * k), worst_frame * 1000.0, max_z, max_i
    );
}

/// Windows-only: windowed programs have nowhere to print, so errors need their own way out.
#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(hwnd: *mut c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AllocConsole() -> i32;
    }

    pub fn dialog(text: &str) {
        let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
        unsafe {
            MessageBoxW(std::ptr::null_mut(), wide(text).as_ptr(), wide("ROTFALL").as_ptr(), 0x10);
        }
    }

    /// The command-line modes (dedicated server, benchmarks) do want a console.
    pub fn console() {
        unsafe {
            AllocConsole();
        }
    }
}

/// Hides the page's loading message once the game is on screen.
#[cfg(target_arch = "wasm32")]
fn web_ready() {
    use wasm_bindgen::JsCast;
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
    if let Some(e) = doc.get_element_by_id("status") {
        e.remove();
    }
    // Key presses only reach a focused canvas, and players should not have to click first.
    if let Some(c) = doc.query_selector("canvas").ok().flatten().and_then(|c| c.dyn_into::<web_sys::HtmlElement>().ok()) {
        let _ = c.focus();
    }
}

/// Puts a message on the page, for when there is no game to show.
#[cfg(target_arch = "wasm32")]
fn web_message(text: &str) {
    web_sys::console::error_1(&text.into());
    if let Some(e) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id("status")) {
        e.set_text_content(Some(text));
    }
}

/// Reports a problem that stops the game, somewhere the player will actually see it.
#[cfg(target_arch = "wasm32")]
fn fatal(text: &str) -> ! {
    web_message(text);
    wasm_bindgen::throw_str(text)
}

/// Reports a problem that stops the game, somewhere the player will actually see it.
#[cfg(not(target_arch = "wasm32"))]
fn fatal(text: &str) -> ! {
    eprintln!("{text}");
    #[cfg(windows)]
    win::dialog(text);
    std::process::exit(1)
}

/// Leaves a record of any crash, and tells the player where it is.
fn install_crash_log() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default(info);
        let path = game::save::data_dir().join("crash.log");
        #[cfg(not(target_arch = "wasm32"))]
        let _ = std::fs::create_dir_all(game::save::data_dir());
        let backtrace = std::backtrace::Backtrace::force_capture();
        #[cfg(not(target_arch = "wasm32"))]
        let _ = std::fs::write(&path, format!(
            "ROTFALL {}\nGPU: {}\n{info}\n\n{backtrace}\n",
            env!("CARGO_PKG_VERSION"),
            render::GPU_DESCRIPTION.get().map_or("not started", |s| s.as_str())
        ));
        #[cfg(windows)]
        win::dialog(&format!("ROTFALL crashed: {info}\n\nDetails were saved to {}", path.display()));
        #[cfg(target_arch = "wasm32")]
        web_message(&format!("ROTFALL crashed: {info}"));
    }));
}

fn main() {
    install_crash_log();
    #[cfg(windows)]
    {
        let args: Vec<String> = std::env::args().collect();
        let cli = ["--server", "--capture", "--bench", "--render-music", "--help", "-h"];
        if args.iter().any(|a| cli.contains(&a.as_str())) {
            win::console();
        }
    }
    let o = parse_args();
    if let Some((t, secs, inten, out)) = &o.render_music {
        let track = match t.as_str() {
            "menu" => Track::Menu,
            "build" => Track::Build,
            "a" => Track::CombatA,
            "b" => Track::CombatB,
            "c" => Track::CombatC,
            "boss" => Track::Boss,
            _ => Track::GameOver,
        };
        audio::render_track_wav(track, *secs, *inten, out).expect("failed to write wav");
        println!("wrote {out}");
        return;
    }
    if let Some(dir) = &o.export_ost {
        audio::export_soundtrack(dir).expect("failed to write the soundtrack");
        return;
    }
    if o.server {
        game::net::run_dedicated(o.cfg.clone(), o.seed, o.load.clone(), game::save::data_dir());
        return;
    }
    if o.capture.is_some() {
        run_capture(&o);
        return;
    }
    if o.bench > 0.0 {
        run_bench(&o);
        return;
    }
    #[cfg(target_os = "linux")]
    install_desktop_entry();
    let el = EventLoop::new().expect("event loop");
    let mut h = GameHandler {
        options: o,
        running: None,
        window: None,
        #[cfg(target_arch = "wasm32")]
        pending: Default::default(),
    };
    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::EventLoopExtWebSys;
        el.spawn_app(h);
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Err(e) = el.run_app(&mut h) {
        eprintln!("event loop error: {e}");
    }
}
